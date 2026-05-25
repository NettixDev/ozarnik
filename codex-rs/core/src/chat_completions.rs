//! OpenAI-compatible chat-completions transport.
//!
//! Used when `ModelProviderInfo.wire_api == WireApi::Chat`. Targets OnlySQ and any other provider
//! that implements OpenAI's chat-completions SSE protocol.
//!
//! Request shape:
//!     POST {base_url}/chat/completions
//!     Authorization: Bearer {key}
//!     { model, messages: [...], tools: [...], stream: true }
//!
//! Stream shape:
//!     data: {"choices":[{"index":0,"delta":{"content":"..."} ,"finish_reason":null}]}
//!     ...
//!     data: [DONE]

/// Append a formatted diagnostic line to `~/.sqagent/sqagent-debug.log` (falls back to
/// `~/.codex/` if a legacy install exists). Best-effort.
fn dbg_log_impl(msg: &str) {
    let Some(home) = dirs::home_dir() else { return };
    let dir = home.join(".ozarnik");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("OZARNIK-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
    }
}

macro_rules! dbg_log {
    ($($arg:tt)*) => { crate::chat_completions::dbg_log_impl(&format!($($arg)*)) };
}

use codex_api::ResponseEvent;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use codex_tools::ToolSpec;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::client_common::Prompt;
use crate::client_common::ResponseStream;

/// Convert internal `ResponseItem`s into chat-completions `messages` array.
fn convert_input_to_messages(input: &[ResponseItem]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(input.len());
    for item in input {
        match item {
            ResponseItem::Message { role, content, .. } => {
                // Collapse ContentItem list into a single string for the simplest path.
                // Multimodal (images) is not yet supported here.
                let mut text = String::new();
                for c in content {
                    match c {
                        ContentItem::InputText { text: t } => text.push_str(t),
                        ContentItem::OutputText { text: t } => text.push_str(t),
                        ContentItem::InputImage { .. } => {
                            // Drop images for now; chat-completions multimodal payloads
                            // are provider-specific and not all OnlySQ models accept them.
                        }
                    }
                }
                out.push(json!({ "role": role, "content": text }));
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                out.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": { "name": name, "arguments": arguments },
                    }],
                }));
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                let content_str = output.to_string();
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content_str,
                }));
            }
            ResponseItem::CustomToolCallOutput { call_id, output, .. } => {
                // Freeform tool result. Send back via the same chat-completions `role: tool`
                // mechanism — the model only sees a string blob regardless of whether the
                // original tool was Function or Freeform.
                let content_str = output.to_string();
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content_str,
                }));
            }
            ResponseItem::CustomToolCall { call_id, name, input, .. } => {
                // History echo of a previous freeform tool call. Re-serialise as an assistant
                // message with tool_calls so the model sees its own prior call when reading
                // back the transcript.
                out.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": { "name": name, "arguments": input },
                    }],
                }));
            }
            // Reasoning, LocalShellCall, CustomToolCall*, ToolSearch*, WebSearchCall, etc.
            // are Responses-API-specific and have no direct chat-completions analogue.
            // Skip them rather than mis-serialise.
            _ => {}
        }
    }
    out
}

/// Convert internal `ToolSpec`s into chat-completions `tools` array.
/// Returns `(tools_json, freeform_names)` — the set of names indicates which tools were
/// originally Freeform and must be re-emitted as `CustomToolCall` items rather than
/// `FunctionCall` items, otherwise codex's tool registry refuses the payload.
fn convert_tools(tools: &[ToolSpec]) -> (Vec<Value>, std::collections::HashSet<String>) {
    let mut freeform_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in tools {
        match t {
            ToolSpec::Function(f) => {
                dbg_log!("[sqagent] tool spec: Function name={}", f.name);
                out.push(json!({
                    "type": "function",
                    "function": {
                        "name": f.name,
                        "description": f.description,
                        "parameters": f.parameters,
                    },
                }));
            }
            ToolSpec::Freeform(f) => {
                // Codex's `Freeform` tool (e.g. apply_patch) is an OpenAI Responses-API custom
                // tool that uses a grammar-validated free-text input. Chat-completions does not
                // support this directly. Re-encode it as a regular `function` tool whose only
                // argument is a string named `input` carrying the raw freeform body.
                dbg_log!("[sqagent] tool spec: Freeform name={} (re-encoding as function)", f.name);
                freeform_names.insert(f.name.clone());
                let full_description = format!(
                    "{}\n\nThis is a freeform tool. Call it by passing the entire body (including any grammar markers required by the tool) as a JSON string in the single argument named `input`.",
                    f.description
                );
                out.push(json!({
                    "type": "function",
                    "function": {
                        "name": f.name,
                        "description": full_description,
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "Raw freeform body for the tool."
                                }
                            },
                            "required": ["input"],
                            "additionalProperties": false
                        }
                    },
                }));
            }
            other => {
                dbg_log!(
                    "[sqagent] tool spec: DROPPED non-Function/Freeform variant: {:?}",
                    other
                );
            }
        }
    }
    (out, freeform_names)
}

#[derive(Debug, Deserialize)]
struct ChatChunk {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    delta: ChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    content: Option<String>,
    // OnlySQ sends `tool_calls: null` (explicit null) in chunks without tool calls. Serde
    // cannot deserialize `null` into a Vec, so wrap in Option to accept it gracefully.
    #[serde(default)]
    tool_calls: Option<Vec<ChatToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCallDelta {
    // OnlySQ sometimes ships the entire tool_call as a single non-delta object inside one
    // streaming chunk and omits `index` entirely. Treat missing as 0.
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChatFnDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatFnDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: i64,
    #[serde(default)]
    completion_tokens: i64,
    #[serde(default)]
    total_tokens: i64,
}

#[derive(Default)]
struct AccumulatedToolCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

/// Stream a chat-completions request against an OpenAI-compatible endpoint.
///
/// `base_url` should be the provider's base URL (e.g. `https://api.onlysq.ru/ai/openai`).
/// The function appends `/chat/completions`.
pub(crate) async fn stream_chat_completions(
    prompt: &Prompt,
    model_slug: &str,
    base_url: &str,
    api_key: &str,
) -> Result<ResponseStream> {
    let messages = convert_input_to_messages(&prompt.input);
    let (tools, freeform_names) = convert_tools(&prompt.tools);

    let mut body = json!({
        "model": model_slug,
        "messages": messages,
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        if prompt.parallel_tool_calls {
            body["parallel_tool_calls"] = Value::Bool(true);
        }
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| CodexErr::Stream(format!("reqwest build: {e}"), None))?;

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .header("accept", "text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| CodexErr::Stream(format!("chat-completions request failed: {e}"), None))?;
    tracing::info!(
        "chat-completions: POST {url} status={} content-type={:?}",
        resp.status(),
        resp.headers().get("content-type")
    );

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(CodexErr::Stream(
            format!("chat-completions HTTP {status}: {text}"),
            None,
        ));
    }

    // OnlySQ workaround: in streaming mode `tool_calls[].function.arguments` arrives empty
    // (literally `""` or `"{}"`). The real arguments only appear in non-streaming responses.
    // Mirror the streaming request as a non-streaming one in parallel and use it as a fallback
    // when emitting tool calls. See note.txt in repo root.
    let fallback_args: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let fallback_handle = {
        let fb = fallback_args.clone();
        let url_nb = url.clone();
        let key_nb = api_key.to_string();
        let mut body_nb = body.clone();
        body_nb["stream"] = Value::Bool(false);
        let client_nb = client.clone();
        tokio::spawn(async move {
            let r = match client_nb
                .post(&url_nb)
                .bearer_auth(&key_nb)
                .json(&body_nb)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    dbg_log!("[sqagent] fallback non-stream send error: {e}");
                    return;
                }
            };
            let status = r.status();
            if !status.is_success() {
                let body = r.text().await.unwrap_or_default();
                dbg_log!("[sqagent] fallback non-stream HTTP {status}: {body}");
                return;
            }
            let json = match r.json::<Value>().await {
                Ok(j) => j,
                Err(e) => {
                    dbg_log!("[sqagent] fallback non-stream parse error: {e}");
                    return;
                }
            };
            dbg_log!("[sqagent] fallback non-stream OK, parsing tool_calls");
            let mut map = fb.lock().await;
            if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                for choice in choices {
                    let Some(tcs) = choice
                        .get("message")
                        .and_then(|m| m.get("tool_calls"))
                        .and_then(|t| t.as_array())
                    else {
                        continue;
                    };
                    for tc in tcs {
                        let Some(func) = tc.get("function") else {
                            continue;
                        };
                        let name = func
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = func
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}")
                            .to_string();
                        if !name.is_empty() {
                            map.insert(name, args);
                        }
                    }
                }
            }
        })
    };

    let (tx, rx_event) = mpsc::channel::<Result<ResponseEvent>>(64);
    let consumer_dropped = CancellationToken::new();
    let consumer_dropped_child = consumer_dropped.clone();
    let fallback_args_for_task = fallback_args.clone();
    let freeform_names_for_task = freeform_names;

    tokio::spawn(async move {
        // Emit Created immediately.
        let _ = tx.send(Ok(ResponseEvent::Created)).await;

        let mut byte_stream = resp.bytes_stream();
        let mut buf = Vec::<u8>::new();
        let mut assistant_text = String::new();
        let mut tool_calls: BTreeMap<u32, AccumulatedToolCall> = BTreeMap::new();
        let mut response_id: Option<String> = None;
        let mut finish_reason: Option<String> = None;
        let mut token_usage: Option<TokenUsage> = None;
        let mut done = false;
        let mut message_item_added = false;

        'outer: while let Some(next) = byte_stream.next().await {
            if consumer_dropped_child.is_cancelled() {
                break;
            }
            let bytes = match next {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx
                        .send(Err(CodexErr::Stream(format!("sse read error: {e}"), None)))
                        .await;
                    return;
                }
            };
            buf.extend_from_slice(&bytes);

            // Process complete lines from buf.
            loop {
                let Some(pos) = buf.iter().position(|&b| b == b'\n') else {
                    break;
                };
                let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                // Strip trailing \n and optional \r.
                let mut line = match std::str::from_utf8(&line_bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => continue,
                };
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let payload = match trimmed.strip_prefix("data:") {
                    Some(p) => p.trim_start(),
                    None => continue,
                };
                if payload == "[DONE]" {
                    dbg_log!("[sqagent] SSE: [DONE]");
                    done = true;
                    break 'outer;
                }
                let chunk: ChatChunk = match serde_json::from_str(payload) {
                    Ok(c) => {
                        dbg_log!("[sqagent] SSE chunk: {}", payload);
                        c
                    }
                    Err(e) => {
                        dbg_log!("[sqagent] SSE parse error: {} for payload: {}", e, payload);
                        continue;
                    }
                };
                if response_id.is_none() {
                    response_id = chunk.id.clone();
                }
                if let Some(u) = chunk.usage {
                    token_usage = Some(TokenUsage {
                        input_tokens: u.prompt_tokens,
                        cached_input_tokens: 0,
                        output_tokens: u.completion_tokens,
                        reasoning_output_tokens: 0,
                        total_tokens: u.total_tokens,
                    });
                }
                for choice in chunk.choices {
                    if let Some(fr) = choice.finish_reason {
                        finish_reason = Some(fr);
                    }
                    if let Some(text) = choice.delta.content
                        && !text.is_empty()
                    {
                        if !message_item_added {
                            // Codex session loop requires OutputItemAdded before any
                            // OutputTextDelta. Emit an empty assistant Message item now;
                            // OutputItemDone later carries the final accumulated text.
                            let item = ResponseItem::Message {
                                id: response_id.clone(),
                                role: "assistant".to_string(),
                                content: vec![ContentItem::OutputText {
                                    text: String::new(),
                                }],
                                phase: None,
                            };
                            let _ = tx.send(Ok(ResponseEvent::OutputItemAdded(item))).await;
                            message_item_added = true;
                        }
                        assistant_text.push_str(&text);
                        let _ = tx.send(Ok(ResponseEvent::OutputTextDelta(text))).await;
                    }
                    // Accumulate tool-call name + arguments. Do NOT emit ToolCallInputDelta
                    // events here: OnlySQ sends empty arguments in streaming mode, so live
                    // deltas would propagate `{}` to downstream consumers. Real arguments are
                    // resolved from the non-streaming fallback request below.
                    for tc in choice.delta.tool_calls.unwrap_or_default() {
                        let entry = tool_calls.entry(tc.index).or_default();
                        if let Some(id) = tc.id {
                            entry.id = Some(id);
                        }
                        if let Some(f) = tc.function {
                            if let Some(n) = f.name {
                                entry.name.push_str(&n);
                            }
                            if let Some(a) = f.arguments {
                                entry.arguments.push_str(&a);
                            }
                        }
                    }
                }
            }
        }

        // Emit assistant message (if any) and tool calls as completed items.
        if !assistant_text.is_empty() {
            let item = ResponseItem::Message {
                id: response_id.clone(),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: assistant_text,
                }],
                phase: None,
            };
            let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
        }
        // Resolve fallback args (from parallel non-streaming request) for any tool call whose
        // streamed arguments came back empty or `{}` — that is OnlySQ's documented streaming
        // quirk. See note.txt in repo root. Wait up to 30s for the fallback request to finish
        // before giving up; in practice it usually completes before the stream is fully drained.
        let timeout_result = tokio::time::timeout(Duration::from_secs(30), fallback_handle).await;
        dbg_log!(
            "[sqagent] stream ended. fallback_timed_out={} streamed_tool_calls={} finish_reason={:?}",
            timeout_result.is_err(),
            tool_calls.len(),
            finish_reason
        );
        let fb_map = fallback_args_for_task.lock().await;
        dbg_log!(
            "[sqagent] fallback map size={} keys={:?}",
            fb_map.len(),
            fb_map.keys().collect::<Vec<_>>()
        );
        for (_idx, mut acc) in tool_calls {
            let trimmed = acc.arguments.trim();
            let needs_fallback = trimmed.is_empty() || trimmed == "{}";
            dbg_log!(
                "[sqagent] tool_call name={} streamed_args={:?} needs_fallback={}",
                acc.name, acc.arguments, needs_fallback
            );
            if needs_fallback {
                if let Some(real) = fb_map.get(&acc.name) {
                    dbg_log!(
                        "[sqagent] substituting fallback args for {}: {} bytes raw={:?}",
                        acc.name,
                        real.len(),
                        real
                    );
                    acc.arguments = real.clone();
                } else {
                    dbg_log!(
                        "[sqagent] WARN: empty args for {} and no fallback",
                        acc.name
                    );
                }
            }
            // Freeform unwrap (AFTER fallback substitution): if args parse as JSON object
            // with a single `input` string key, unwrap it. Codex's freeform-tool handler
            // (e.g. apply_patch) expects the raw body, not our `{"input": "..."}` shim.
            if let Ok(parsed) = serde_json::from_str::<Value>(&acc.arguments)
                && let Some(obj) = parsed.as_object()
                && obj.len() == 1
                && let Some(inner) = obj.get("input").and_then(|v| v.as_str())
            {
                dbg_log!(
                    "[sqagent] unwrapping freeform input for {} ({} bytes)",
                    acc.name,
                    inner.len()
                );
                acc.arguments = inner.to_string();
            }
            let call_id = acc.id.clone().unwrap_or_default();
            // Freeform tools (e.g. apply_patch) must be emitted as CustomToolCall, not
            // FunctionCall — codex's tool registry checks the payload variant against the
            // registered tool kind and rejects mismatches with "incompatible payload".
            let is_freeform = freeform_names_for_task.contains(&acc.name);
            dbg_log!(
                "[sqagent] emitting tool_call name={} is_freeform={} args_len={}",
                acc.name, is_freeform, acc.arguments.len()
            );
            if is_freeform {
                let added_item = ResponseItem::CustomToolCall {
                    id: None,
                    status: None,
                    call_id: call_id.clone(),
                    name: acc.name.clone(),
                    input: String::new(),
                };
                let _ = tx
                    .send(Ok(ResponseEvent::OutputItemAdded(added_item)))
                    .await;
                let item = ResponseItem::CustomToolCall {
                    id: None,
                    status: None,
                    call_id,
                    name: acc.name,
                    input: acc.arguments,
                };
                let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
            } else {
                let added_item = ResponseItem::FunctionCall {
                    id: None,
                    name: acc.name.clone(),
                    namespace: None,
                    arguments: String::new(),
                    call_id: call_id.clone(),
                };
                let _ = tx
                    .send(Ok(ResponseEvent::OutputItemAdded(added_item)))
                    .await;
                let item = ResponseItem::FunctionCall {
                    id: None,
                    name: acc.name,
                    namespace: None,
                    arguments: acc.arguments,
                    call_id,
                };
                let _ = tx.send(Ok(ResponseEvent::OutputItemDone(item))).await;
            }
        }
        drop(fb_map);

        // Map OnlySQ/chat-completions finish_reason to codex's end_turn semantics.
        //   "stop"          -> turn ended naturally.
        //   "content_filter" -> turn ended (filter triggered).
        //   "tool_calls"    -> turn NOT ended; codex needs to dispatch the tool and continue.
        //   "length"        -> turn NOT ended; max_tokens hit mid-response, give the agent
        //                       another chance to keep going on the next request.
        //   null/unknown    -> stream was cut short. Surface a stream error so codex retries
        //                       instead of silently truncating and giving up.
        match finish_reason.as_deref() {
            None => {
                if !done {
                    let _ = tx
                        .send(Err(CodexErr::Stream(
                            "chat-completions stream ended without finish_reason or [DONE]".to_string(),
                            None,
                        )))
                        .await;
                    return;
                }
                let _ = tx
                    .send(Ok(ResponseEvent::Completed {
                        response_id: response_id.unwrap_or_default(),
                        token_usage,
                        end_turn: None,
                    }))
                    .await;
            }
            Some(fr) => {
                let end_turn = match fr {
                    "stop" | "content_filter" => Some(true),
                    "tool_calls" | "length" => Some(false),
                    _ => None,
                };
                let _ = tx
                    .send(Ok(ResponseEvent::Completed {
                        response_id: response_id.unwrap_or_default(),
                        token_usage,
                        end_turn,
                    }))
                    .await;
            }
        }
    });

    Ok(ResponseStream {
        rx_event,
        consumer_dropped,
    })
}
