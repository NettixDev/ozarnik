//! Fetches the OnlySQ model catalog and converts it to `ModelPreset`s for the /model picker.
//!
//! OnlySQ exposes JSON at https://api.onlysq.ru/ai/models with the shape:
//!     { "api-version": "...",
//!       "models": { "<id>": { "name": "...", "description": "...",
//!                              "can-tools": bool, "can-stream": bool,
//!                              "can-think": bool, "status": "work", ... } } }
//!
//! Uses async reqwest. Callers must invoke from a tokio runtime.

use codex_model_provider_info::ONLYSQ_MODELS_URL;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct OnlySQModelsResponse {
    #[serde(default)]
    models: BTreeMap<String, OnlySQModel>,
}

#[derive(Debug, Deserialize)]
struct OnlySQModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "can-think")]
    can_think: bool,
    #[serde(default)]
    status: String,
}

/// Fetch OnlySQ model catalog asynchronously and convert to `ModelPreset`s.
pub async fn fetch_onlysq_presets() -> anyhow::Result<Vec<ModelPreset>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(ONLYSQ_MODELS_URL)
        .send()
        .await?
        .error_for_status()?;
    let parsed: OnlySQModelsResponse = resp.json().await?;
    let mut presets: Vec<ModelPreset> = parsed
        .models
        .into_iter()
        .filter(|(_, m)| m.status.eq_ignore_ascii_case("work") || m.status.is_empty())
        .map(|(id, m)| {
            let default_effort = if m.can_think {
                ReasoningEffort::Medium
            } else {
                ReasoningEffort::None
            };
            ModelPreset {
                id: id.clone(),
                model: id.clone(),
                display_name: if m.name.is_empty() {
                    id.clone()
                } else {
                    m.name.clone()
                },
                description: m.description.clone(),
                default_reasoning_effort: default_effort,
                supported_reasoning_efforts: Vec::new(),
                supports_personality: false,
                additional_speed_tiers: Vec::new(),
                service_tiers: Vec::new(),
                default_service_tier: None,
                is_default: false,
                upgrade: None,
                show_in_picker: true,
                availability_nux: None,
                supported_in_api: true,
                input_modalities: Vec::new(),
            }
        })
        .collect();
    presets.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(presets)
}

/// Synchronous wrapper: blocks on the tokio runtime to fetch presets.
/// Returns `None` on any error.
pub fn fetch_onlysq_presets_blocking() -> Option<Vec<ModelPreset>> {
    let handle = tokio::runtime::Handle::try_current().ok()?;
    tokio::task::block_in_place(|| handle.block_on(fetch_onlysq_presets()).ok())
}
