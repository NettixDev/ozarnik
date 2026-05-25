use strum::IntoEnumIterator;
use strum_macros::AsRefStr;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

/// Commands that can be invoked by starting a message with a leading slash.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    Model,
    Ide,
    Permissions,
    Keymap,
    Vim,
    #[strum(serialize = "setup-default-sandbox")]
    ElevateSandbox,
    #[strum(serialize = "sandbox-add-read-dir")]
    SandboxReadRoot,
    Experimental,
    #[strum(to_string = "approve")]
    AutoReview,
    Memories,
    Skills,
    Hooks,
    Review,
    Rename,
    New,
    Resume,
    Fork,
    Init,
    Compact,
    Plan,
    Goal,
    Agent,
    Side,
    Btw,
    Copy,
    Raw,
    Diff,
    Mention,
    Status,
    DebugConfig,
    Title,
    Statusline,
    Theme,
    #[strum(to_string = "pets", serialize = "pet")]
    Pets,
    Mcp,
    Apps,
    Plugins,
    Logout,
    Quit,
    Exit,
    Feedback,
    Rollout,
    Ps,
    #[strum(to_string = "stop", serialize = "clean")]
    Stop,
    Clear,
    Personality,
    Realtime,
    Settings,
    TestApproval,
    #[strum(serialize = "subagents")]
    MultiAgents,
    // Debugging commands.
    #[strum(serialize = "debug-m-drop")]
    MemoryDrop,
    #[strum(serialize = "debug-m-update")]
    MemoryUpdate,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::Feedback => "отправить логи разработчикам",
            SlashCommand::New => "начать новый чат во время разговора",
            SlashCommand::Init => "создать файл AGENTS.md с инструкциями для ОЗАРНИК",
            SlashCommand::Compact => "сжать переписку, чтобы не упереться в лимит контекста",
            SlashCommand::Review => "проверить текущие изменения и найти проблемы",
            SlashCommand::Rename => "переименовать текущий тред",
            SlashCommand::Resume => "возобновить сохранённый чат",
            SlashCommand::Clear => "очистить терминал и начать новый чат",
            SlashCommand::Fork => "разветвить текущий чат",
            SlashCommand::Quit | SlashCommand::Exit => "выйти из ОЗАРНИК",
            SlashCommand::Copy => "скопировать последний ответ в markdown",
            SlashCommand::Raw => "переключить raw-режим прокрутки для удобного выделения мышью",
            SlashCommand::Diff => "показать git diff (включая неотслеживаемые файлы)",
            SlashCommand::Mention => "упомянуть файл",
            SlashCommand::Skills => "использовать навыки для улучшения работы ОЗАРНИК над конкретными задачами",
            SlashCommand::Hooks => "посмотреть и настроить lifecycle-хуки",
            SlashCommand::Status => "показать настройки сессии и использование токенов",
            SlashCommand::DebugConfig => "показать слои конфига и источники требований (для отладки)",
            SlashCommand::Title => "настроить что показывать в заголовке терминала",
            SlashCommand::Statusline => "настроить что показывать в строке статуса",
            SlashCommand::Theme => "выбрать тему подсветки синтаксиса",
            SlashCommand::Pets => "выбрать или скрыть питомца в терминале",
            SlashCommand::Ps => "список фоновых терминалов",
            SlashCommand::Stop => "остановить все фоновые терминалы",
            SlashCommand::MemoryDrop => "НЕ ИСПОЛЬЗОВАТЬ",
            SlashCommand::MemoryUpdate => "НЕ ИСПОЛЬЗОВАТЬ",
            SlashCommand::Model => "выбрать модель и уровень reasoning",
            SlashCommand::Ide => {
                "подтянуть текущее выделение, открытые файлы и другой контекст из вашей IDE"
            }
            SlashCommand::Personality => "выбрать стиль общения ОЗАРНИК",
            SlashCommand::Realtime => "переключить голосовой режим (экспериментально)",
            SlashCommand::Settings => "настроить микрофон/динамик realtime",
            SlashCommand::Plan => "перейти в режим Plan",
            SlashCommand::Goal => "задать или посмотреть цель для долгой задачи",
            SlashCommand::Agent | SlashCommand::MultiAgents => "переключить активный agent-тред",
            SlashCommand::Side | SlashCommand::Btw => {
                "начать побочный разговор в эфемерном форке"
            }
            SlashCommand::Permissions => "выбрать что ОЗАРНИК разрешено делать",
            SlashCommand::Keymap => "переназначить горячие клавиши TUI",
            SlashCommand::Vim => "переключить Vim-режим в редакторе ввода",
            SlashCommand::ElevateSandbox => "настроить песочницу с повышенными правами",
            SlashCommand::SandboxReadRoot => {
                "разрешить песочнице читать директорию: /sandbox-add-read-dir <абсолютный_путь>"
            }
            SlashCommand::Experimental => "переключить экспериментальные функции",
            SlashCommand::AutoReview => "одобрить один повтор недавнего auto-review отказа",
            SlashCommand::Memories => "настроить использование и генерацию памяти",
            SlashCommand::Mcp => "список настроенных MCP-инструментов; /mcp verbose для подробностей",
            SlashCommand::Apps => "управление приложениями",
            SlashCommand::Plugins => "просмотр плагинов",
            SlashCommand::Logout => "выйти из ОЗАРНИК",
            SlashCommand::Rollout => "показать путь к rollout-файлу",
            SlashCommand::TestApproval => "тестовый запрос подтверждения",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }

    /// Whether this command supports inline args (for example `/review ...`).
    pub fn supports_inline_args(self) -> bool {
        matches!(
            self,
            SlashCommand::Review
                | SlashCommand::Rename
                | SlashCommand::Plan
                | SlashCommand::Goal
                | SlashCommand::Ide
                | SlashCommand::Keymap
                | SlashCommand::Mcp
                | SlashCommand::Raw
                | SlashCommand::Pets
                | SlashCommand::Side
                | SlashCommand::Btw
                | SlashCommand::Resume
                | SlashCommand::SandboxReadRoot
        )
    }

    /// Whether this command remains available inside an active side conversation.
    pub fn available_in_side_conversation(self) -> bool {
        matches!(
            self,
            SlashCommand::Copy
                | SlashCommand::Raw
                | SlashCommand::Diff
                | SlashCommand::Mention
                | SlashCommand::Status
                | SlashCommand::Ide
        )
    }

    /// Whether this command can be run while a task is in progress.
    pub fn available_during_task(self) -> bool {
        match self {
            SlashCommand::New
            | SlashCommand::Resume
            | SlashCommand::Fork
            | SlashCommand::Init
            | SlashCommand::Compact
            | SlashCommand::Model
            | SlashCommand::Personality
            | SlashCommand::Permissions
            | SlashCommand::Keymap
            | SlashCommand::Vim
            | SlashCommand::ElevateSandbox
            | SlashCommand::SandboxReadRoot
            | SlashCommand::Experimental
            | SlashCommand::Memories
            | SlashCommand::Review
            | SlashCommand::Plan
            | SlashCommand::Clear
            | SlashCommand::Logout
            | SlashCommand::MemoryDrop
            | SlashCommand::MemoryUpdate => false,
            SlashCommand::Diff
            | SlashCommand::Copy
            | SlashCommand::Raw
            | SlashCommand::Rename
            | SlashCommand::Mention
            | SlashCommand::Skills
            | SlashCommand::Hooks
            | SlashCommand::Status
            | SlashCommand::DebugConfig
            | SlashCommand::Ps
            | SlashCommand::Stop
            | SlashCommand::Goal
            | SlashCommand::Mcp
            | SlashCommand::Apps
            | SlashCommand::Plugins
            | SlashCommand::Title
            | SlashCommand::Statusline
            | SlashCommand::AutoReview
            | SlashCommand::Feedback
            | SlashCommand::Ide
            | SlashCommand::Quit
            | SlashCommand::Exit
            | SlashCommand::Side
            | SlashCommand::Btw => true,
            SlashCommand::Rollout => true,
            SlashCommand::TestApproval => true,
            SlashCommand::Realtime => true,
            SlashCommand::Settings => true,
            SlashCommand::Agent | SlashCommand::MultiAgents => true,
            SlashCommand::Theme | SlashCommand::Pets => false,
        }
    }

    fn is_visible(self) -> bool {
        match self {
            SlashCommand::SandboxReadRoot => cfg!(target_os = "windows"),
            SlashCommand::Copy => !cfg!(target_os = "android"),
            SlashCommand::Rollout | SlashCommand::TestApproval => cfg!(debug_assertions),
            _ => true,
        }
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    SlashCommand::iter()
        .filter(|command| command.is_visible())
        .map(|c| (c.command(), c))
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    use super::SlashCommand;

    #[test]
    fn stop_command_is_canonical_name() {
        assert_eq!(SlashCommand::Stop.command(), "stop");
    }

    #[test]
    fn clean_alias_parses_to_stop_command() {
        assert_eq!(SlashCommand::from_str("clean"), Ok(SlashCommand::Stop));
    }

    #[test]
    fn pet_alias_parses_to_pets_command() {
        assert_eq!(SlashCommand::Pets.command(), "pets");
        assert_eq!(SlashCommand::from_str("pet"), Ok(SlashCommand::Pets));
    }

    #[test]
    fn certain_commands_are_available_during_task() {
        assert!(SlashCommand::Goal.available_during_task());
        assert!(SlashCommand::Ide.available_during_task());
        assert!(SlashCommand::Title.available_during_task());
        assert!(SlashCommand::Statusline.available_during_task());
        assert!(SlashCommand::Raw.available_during_task());
        assert!(SlashCommand::Raw.available_in_side_conversation());
        assert!(SlashCommand::Raw.supports_inline_args());
    }

    #[test]
    fn auto_review_command_is_approve() {
        assert_eq!(SlashCommand::AutoReview.command(), "approve");
        assert_eq!(
            SlashCommand::from_str("approve"),
            Ok(SlashCommand::AutoReview)
        );
    }
}
