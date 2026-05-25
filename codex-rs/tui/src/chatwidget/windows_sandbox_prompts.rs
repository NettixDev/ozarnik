//! Windows sandbox prompts and warning surfaces for `ChatWidget`.

use super::*;

impl ChatWidget {
    #[cfg(target_os = "windows")]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        if self
            .config
            .notices
            .hide_world_writable_warning
            .unwrap_or(false)
        {
            return None;
        }
        let cwd = self.config.cwd.clone();
        let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
        let policy = self.config.legacy_sandbox_policy();
        match codex_windows_sandbox::apply_world_writable_scan_and_denies(
            self.config.codex_home.as_path(),
            cwd.as_path(),
            &env_map,
            &policy,
            Some(self.config.codex_home.as_path()),
        ) {
            Ok(_) => None,
            Err(_) => Some((Vec::new(), 0, true)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn world_writable_warning_details(&self) -> Option<(Vec<String>, usize, bool)> {
        None
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        preset: Option<ApprovalPreset>,
        profile_selection: Option<PermissionProfileSelection>,
        sample_paths: Vec<String>,
        extra_count: usize,
        failed_scan: bool,
    ) {
        let (approval, permission_profile, active_permission_profile) = match &preset {
            Some(p) => (
                Some(AskForApproval::from(p.approval)),
                Some(p.permission_profile.clone()),
                Some(p.active_permission_profile.clone()),
            ),
            None => (None, None, None),
        };
        let mut header_children: Vec<Box<dyn Renderable>> = Vec::new();
        let describe_profile = |profile: &PermissionProfile| {
            if matches!(profile, PermissionProfile::Disabled) {
                "режим Full Access"
            } else if profile
                .file_system_sandbox_policy()
                .can_write_path_with_cwd(self.config.cwd.as_path(), self.config.cwd.as_path())
            {
                "режим Agent"
            } else {
                "режим Read-Only"
            }
        };
        let mode_label = preset
            .as_ref()
            .map(|p| describe_profile(&p.permission_profile))
            .unwrap_or_else(|| {
                describe_profile(&self.config.permissions.effective_permission_profile())
            });
        let info_line = if failed_scan {
            Line::from(vec![
                "Не удалось завершить сканирование world-writable папок, поэтому защита не подтверждена. "
                    .into(),
                format!("Windows-песочница не может гарантировать защиту в режиме {mode_label}.")
                    .fg(Color::Red),
            ])
        } else {
            Line::from(vec![
                "Windows-песочница не может защитить запись в папки, доступные всем (Everyone).".into(),
                " Уберите доступ на запись для Everyone у этих папок:".into(),
            ])
        };
        header_children.push(Box::new(
            Paragraph::new(vec![info_line]).wrap(Wrap { trim: false }),
        ));

        if !sample_paths.is_empty() {
            // Show up to three examples and optionally an "and X more" line.
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(""));
            for p in &sample_paths {
                lines.push(Line::from(format!("  - {p}")));
            }
            if extra_count > 0 {
                lines.push(Line::from(format!("и ещё {extra_count}")));
            }
            header_children.push(Box::new(Paragraph::new(lines).wrap(Wrap { trim: false })));
        }
        let header = ColumnRenderable::with(header_children);

        // Build actions ensuring acknowledgement happens before applying the
        // new permission profile, so downstream policy-change hooks don't
        // re-trigger the warning.
        let mut accept_actions: Vec<SelectionAction> = Vec::new();
        // Suppress the immediate re-scan only when a preset will be applied via
        // /permissions, to avoid duplicate warnings from the ensuing policy change.
        if preset.is_some() {
            accept_actions.push(Box::new(|tx| {
                tx.send(AppEvent::SkipNextWorldWritableScan);
            }));
        }
        if let Some(selection) = profile_selection.clone() {
            accept_actions.extend(Self::permission_profile_selection_actions(selection));
        } else if let (Some(approval), Some(permission_profile), Some(active_permission_profile)) = (
            approval,
            permission_profile.clone(),
            active_permission_profile.clone(),
        ) {
            accept_actions.extend(Self::approval_preset_actions(
                approval,
                permission_profile,
                active_permission_profile,
                mode_label.to_string(),
                ApprovalsReviewer::User,
            ));
        }

        let mut accept_and_remember_actions: Vec<SelectionAction> = Vec::new();
        accept_and_remember_actions.push(Box::new(|tx| {
            tx.send(AppEvent::UpdateWorldWritableWarningAcknowledged(true));
            tx.send(AppEvent::PersistWorldWritableWarningAcknowledged);
        }));
        if let Some(selection) = profile_selection {
            accept_and_remember_actions
                .extend(Self::permission_profile_selection_actions(selection));
        } else if let (Some(approval), Some(permission_profile), Some(active_permission_profile)) =
            (approval, permission_profile, active_permission_profile)
        {
            accept_and_remember_actions.extend(Self::approval_preset_actions(
                approval,
                permission_profile,
                active_permission_profile,
                mode_label.to_string(),
                ApprovalsReviewer::User,
            ));
        }

        let items = vec![
            SelectionItem {
                name: "Продолжить".to_string(),
                description: Some(format!("Применить {mode_label} для этой сессии")),
                actions: accept_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Продолжить и больше не предупреждать".to_string(),
                description: Some(format!("Включить {mode_label} и запомнить выбор")),
                actions: accept_and_remember_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_world_writable_warning_confirmation(
        &mut self,
        _preset: Option<ApprovalPreset>,
        _profile_selection: Option<PermissionProfileSelection>,
        _sample_paths: Vec<String>,
        _extra_count: usize,
        _failed_scan: bool,
    ) {
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_sandbox_enable_prompt(
        &mut self,
        preset: ApprovalPreset,
        profile_selection: Option<PermissionProfileSelection>,
    ) {
        use ratatui_macros::line;

        if !crate::legacy_core::windows_sandbox::ELEVATED_SANDBOX_NUX_ENABLED {
            // Legacy flow (pre-NUX): explain the experimental sandbox and let the user enable it
            // directly (no elevation prompts).
            let mut header = ColumnRenderable::new();
            header.push(*Box::new(
                Paragraph::new(vec![
                    line!["Режим Agent на Windows использует экспериментальную песочницу для ограничения сети и файловой системы.".bold()],
                    line!["Подробнее: https://developers.openai.com/codex/windows"],
                ])
                .wrap(Wrap { trim: false }),
            ));

            let preset_clone = preset;
            let items = vec![
                SelectionItem {
                    name: "Включить экспериментальную песочницу".to_string(),
                    description: None,
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::EnableWindowsSandboxForAgentMode {
                            preset: preset_clone.clone(),
                            mode: WindowsSandboxEnableMode::Legacy,
                            profile_selection: profile_selection.clone(),
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Назад".to_string(),
                    description: None,
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::OpenApprovalsPopup);
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ];

            self.bottom_pane.show_selection_view(SelectionViewParams {
                title: None,
                footer_hint: Some(standard_popup_hint_line()),
                items,
                header: Box::new(header),
                ..Default::default()
            });
            return;
        }

        self.session_telemetry.counter(
            "codex.windows_sandbox.elevated_prompt_shown",
            /*inc*/ 1,
            &[],
        );

        let mut header = ColumnRenderable::new();
        header.push(*Box::new(
            Paragraph::new(vec![
                line!["Настройте песочницу ОЗАРНИК, чтобы защитить файлы и контролировать сетевой доступ. Подробнее <https://developers.openai.com/codex/windows>"],
            ])
            .wrap(Wrap { trim: false }),
        ));

        let accept_otel = self.session_telemetry.clone();
        let legacy_otel = self.session_telemetry.clone();
        let legacy_preset = preset.clone();
        let legacy_profile_selection = profile_selection.clone();
        let quit_otel = self.session_telemetry.clone();
        let items = vec![
            SelectionItem {
                name: "Настроить песочницу по умолчанию (требуются права администратора)".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    accept_otel.counter(
                        "codex.windows_sandbox.elevated_prompt_accept",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::BeginWindowsSandboxElevatedSetup {
                        preset: preset.clone(),
                        profile_selection: profile_selection.clone(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Использовать песочницу без админа (выше риск при prompt-инъекции)".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    legacy_otel.counter(
                        "codex.windows_sandbox.elevated_prompt_use_legacy",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::BeginWindowsSandboxLegacySetup {
                        preset: legacy_preset.clone(),
                        profile_selection: legacy_profile_selection.clone(),
                    });
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Выйти".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    quit_otel.counter(
                        "codex.windows_sandbox.elevated_prompt_quit",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::Exit(ExitMode::ShutdownFirst));
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_windows_sandbox_enable_prompt(
        &mut self,
        _preset: ApprovalPreset,
        _profile_selection: Option<PermissionProfileSelection>,
    ) {
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_sandbox_fallback_prompt(
        &mut self,
        preset: ApprovalPreset,
        profile_selection: Option<PermissionProfileSelection>,
    ) {
        use ratatui_macros::line;

        let mut lines = Vec::new();
        lines.push(line![
            "Не удалось настроить песочницу с правами администратора".bold()
        ]);
        lines.push(line![""]);
        lines.push(line![
            "Можно использовать ОЗАРНИК в песочнице без админа. Выше риск при prompt-инъекции."
        ]);
        lines.push(line![
            "Подробнее <https://developers.openai.com/codex/windows>"
        ]);

        let mut header = ColumnRenderable::new();
        header.push(*Box::new(Paragraph::new(lines).wrap(Wrap { trim: false })));

        let elevated_preset = preset.clone();
        let legacy_preset = preset;
        let elevated_profile_selection = profile_selection.clone();
        let legacy_profile_selection = profile_selection;
        let quit_otel = self.session_telemetry.clone();
        let items = vec![
            SelectionItem {
                name: "Попробовать настроить с правами админа ещё раз".to_string(),
                description: None,
                actions: vec![Box::new({
                    let otel = self.session_telemetry.clone();
                    let preset = elevated_preset;
                    move |tx| {
                        otel.counter(
                            "codex.windows_sandbox.fallback_retry_elevated",
                            /*inc*/ 1,
                            &[],
                        );
                        tx.send(AppEvent::BeginWindowsSandboxElevatedSetup {
                            preset: preset.clone(),
                            profile_selection: elevated_profile_selection.clone(),
                        });
                    }
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Использовать ОЗАРНИК с песочницей без админа".to_string(),
                description: None,
                actions: vec![Box::new({
                    let otel = self.session_telemetry.clone();
                    let preset = legacy_preset;
                    move |tx| {
                        otel.counter(
                            "codex.windows_sandbox.fallback_use_legacy",
                            /*inc*/ 1,
                            &[],
                        );
                        tx.send(AppEvent::BeginWindowsSandboxLegacySetup {
                            preset: preset.clone(),
                            profile_selection: legacy_profile_selection.clone(),
                        });
                    }
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Выйти".to_string(),
                description: None,
                actions: vec![Box::new(move |tx| {
                    quit_otel.counter(
                        "codex.windows_sandbox.fallback_prompt_quit",
                        /*inc*/ 1,
                        &[],
                    );
                    tx.send(AppEvent::Exit(ExitMode::ShutdownFirst));
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: None,
            footer_hint: Some(standard_popup_hint_line()),
            items,
            header: Box::new(header),
            ..Default::default()
        });
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn open_windows_sandbox_fallback_prompt(
        &mut self,
        _preset: ApprovalPreset,
        _profile_selection: Option<PermissionProfileSelection>,
    ) {
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self, show_now: bool) {
        if show_now
            && WindowsSandboxLevel::from_config(&self.config) == WindowsSandboxLevel::Disabled
            && let Some(preset) = builtin_approval_presets()
                .into_iter()
                .find(|preset| preset.id == "auto")
        {
            self.open_windows_sandbox_enable_prompt(preset, /*profile_selection*/ None);
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn maybe_prompt_windows_sandbox_enable(&mut self, _show_now: bool) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn show_windows_sandbox_setup_status(&mut self) {
        // While elevated sandbox setup runs, prevent typing so the user doesn't
        // accidentally queue messages that will run under an unexpected mode.
        self.bottom_pane.set_composer_input_enabled(
            /*enabled*/ false,
            Some("Ввод отключён, пока идёт настройка.".to_string()),
        );
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane
            .set_interrupt_hint_visible(/*visible*/ false);
        self.set_status(
            "Настраиваю песочницу...".to_string(),
            Some("Подождите немного, это может занять несколько минут".to_string()),
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );
        self.request_redraw();
    }

    #[cfg(not(target_os = "windows"))]
    #[allow(dead_code)]
    pub(crate) fn show_windows_sandbox_setup_status(&mut self) {}

    #[cfg(target_os = "windows")]
    pub(crate) fn clear_windows_sandbox_setup_status(&mut self) {
        self.bottom_pane
            .set_composer_input_enabled(/*enabled*/ true, /*placeholder*/ None);
        self.bottom_pane.hide_status_indicator();
        self.request_redraw();
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn clear_windows_sandbox_setup_status(&mut self) {}
}
