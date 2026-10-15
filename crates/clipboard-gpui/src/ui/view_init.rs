use super::*;

impl ClipboardView {
    pub(super) fn new(
        service: Service,
        events: async_channel::Receiver<Event>,
        startup: StartupMode,
        tray_enabled: Rc<Cell<bool>>,
        exiting: Rc<Cell<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language = service.initial_language;
        let window_position = service.initial_window_position;
        let hover_preference = service.initial_hover_preview;
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(
                language,
                "搜索剪贴板历史…",
                "Search clipboard history…",
            ))
        });
        let group_select = cx.new(|cx| {
            SelectState::new(
                group_options(&[], language, None),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let group_subscription = cx.subscribe_in(
            &group_select,
            window,
            |this, _, event: &SelectEvent<Vec<GroupOption>>, window, cx| {
                if let SelectEvent::Confirm(Some(choice)) = event {
                    this.choose_group(*choice, window, cx);
                }
            },
        );
        let subscription = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.cancel_pending_row_click();
                this.close_hover_preview(cx);
                if matches!(
                    this.confirmation,
                    Some(HistoryConfirmation::ClearHistory(_))
                ) {
                    this.close_history_confirmation(window, cx);
                }
                this.reset_selection();
                this.sync_confirmation_dialog(window, cx);
                this.history.begin_search();
                this.scroll_to_top();
                this.search_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(150))
                        .await;
                    let _ = view.update(cx, |this, cx| this.query(cx));
                }));
                cx.notify();
            } else if matches!(event, InputEvent::Focus) {
                this.cancel_pending_row_click();
            } else if matches!(event, InputEvent::PressEnter { .. })
                && !this.batch_mode
                && let Some(id) = this.history.selected
            {
                this.send(Command::Copy(id), cx);
            }
        });
        let main_window = window.window_handle();
        let event_task = cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = events.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    this.apply_event(event, window, cx)
                })
                .is_err()
                {
                    break;
                }
            }
        });
        let (tray_sender, tray_receiver) = async_channel::bounded(8);
        let tray = tray::create(
            tray_sender.clone(),
            language,
            service.initial_paused,
            startup.monitoring,
        );
        tray_enabled.set(tray.is_ok());
        if startup.hidden && tray.is_err() {
            tray::set_window_visible(window, true);
        }
        let tray_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(command) = tray_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| match command
                {
                    TrayCommand::Toggle => {
                        if tray::is_window_shown(window) && !tray::is_window_minimized(window) {
                            this.hide_visible_window(window, cx);
                        } else {
                            this.paste_target = None;
                            this.show_window(window, cx);
                            cx.notify();
                        }
                    }
                    TrayCommand::Settings => this.open_settings_window(SettingsPage::General, cx),
                    TrayCommand::ClearHistory => {
                        this.paste_target = None;
                        this.show_window(window, cx);
                        this.open_clear_history(window, cx);
                    }
                    TrayCommand::TogglePause => {
                        if this.monitoring
                            && !this.pause_pending
                            && this.send(Command::Pause(!this.paused), cx)
                        {
                            this.pause_pending = true;
                            cx.notify();
                        }
                    }
                    TrayCommand::Quit => {
                        this.exiting.set(true);
                        cx.quit();
                    }
                })
                .is_err()
                {
                    break;
                }
            }
        });
        let tray_error = tray.as_ref().err().map(ToString::to_string);
        // The hotkey thread uses try_send; a full channel must not discard the
        // second press while the first press is still waking the window.
        let (hotkey_sender, hotkey_receiver) = async_channel::unbounded();
        let hotkey_choice = service.initial_hotkey;
        let hotkey = if hotkey_choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(hotkey_choice, hotkey_sender.clone()).map(Some)
        };
        let hotkey_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(target) = hotkey_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    if tray::is_window_shown(window) && !tray::is_window_minimized(window) {
                        this.hide_visible_window(window, cx);
                        return;
                    }
                    this.paste_target = paste::is_external_target(window, target).then_some(target);
                    this.show_window(window, cx);
                    cx.notify();
                })
                .is_err()
                {
                    break;
                }
            }
        });
        let hotkey_error = hotkey.as_ref().err().map(ToString::to_string);
        let quick_paste_enabled = service.initial_quick_paste_enabled;
        let paste_shortcuts = service.initial_paste_shortcuts.clone();
        let (paste_hotkey_sender, paste_hotkey_receiver) = async_channel::bounded(16);
        let paste_hotkeys = if startup.monitoring && quick_paste_enabled {
            PasteHotkeys::start(paste_hotkey_sender.clone(), &paste_shortcuts, hotkey_choice)
                .map(Some)
        } else {
            Ok(None)
        };
        let paste_hotkey_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = paste_hotkey_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    this.handle_quick_hotkey(event, window, cx);
                })
                .is_err()
                {
                    break;
                }
            }
        });
        let paste_hotkey_error = paste_hotkeys.as_ref().err().map(ToString::to_string);
        let paste_hotkey_warnings = paste_hotkeys
            .as_ref()
            .ok()
            .and_then(|result| result.as_ref().map(|(_, warnings)| warnings.clone()))
            .unwrap_or_default();
        let (outside_click_sender, outside_click_receiver) = async_channel::bounded(16);
        let outside_click_monitor = if tray.is_ok() {
            tray::window_hwnd(window)
                .ok_or_else(|| anyhow::anyhow!("无法取得历史窗口句柄"))
                .and_then(|hwnd| OutsideClickMonitor::start(outside_click_sender, hwnd.0 as isize))
                .map(Some)
        } else {
            Ok(None)
        };
        let outside_click_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(position) = outside_click_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    this.handle_outside_click(position, window, cx);
                })
                .is_err()
                {
                    break;
                }
            }
        });
        let outside_click_error = outside_click_monitor
            .as_ref()
            .err()
            .map(ToString::to_string);
        let autostart = clipboard_platform::autostart::enabled(&service.data_dir);
        let mut startup_errors = Vec::new();
        if let Some(error) = tray_error {
            startup_errors.push(format!(
                "{}: {error}",
                tr(
                    language,
                    "托盘不可用，关闭窗口将退出",
                    "The tray is unavailable; closing the window will exit"
                )
            ));
        }
        if let Some(error) = hotkey_error {
            startup_errors.push(error);
        }
        if let Some(error) = paste_hotkey_error {
            startup_errors.push(error);
        }
        if let Some(warning) = paste_registration_summary(language, &paste_hotkey_warnings) {
            startup_errors.push(warning);
        }
        if let Some(error) = outside_click_error {
            startup_errors.push(error);
        }
        if let Err(error) = &autostart {
            startup_errors.push(format!(
                "{}: {error}",
                tr(
                    language,
                    "无法读取开机启动设置",
                    "Unable to read startup settings"
                )
            ));
        }
        let theme = service.initial_theme;
        let window_effect = service.initial_window_effect;
        let paused = service.initial_paused;
        let initial_window_size = service.initial_window_size;
        let persist_window_size = service.initial_persist_window_size;
        let auto_reset_state = service.initial_auto_reset_state;
        let search_auto_focus = service.initial_search_auto_focus;
        let search_auto_clear = service.initial_search_auto_clear;
        let skip_clear_confirm = service.initial_skip_clear_confirm;
        let paste_close_window = service.initial_paste_close_window;
        let paste_key = service.initial_paste_key;
        let paste_move_to_top = service.initial_paste_move_to_top;
        let display = service.initial_display;
        let audio = service.initial_audio;
        let monitor_types = service.initial_monitor_types;
        let app_filter = service.initial_app_filter.clone();
        let onboarding_completed = service.initial_onboarding_completed;
        if (!startup.hidden || tray.is_err())
            && let Err(error) = position::position_window(window, window_position)
        {
            startup_errors.push(error.to_string());
        }
        init_window_effect(window_effect, window, cx);
        apply_theme(theme, window, cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme == ThemePreference::System {
                apply_theme(this.theme, window, cx);
            }
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.handle_window_deactivated(cx);
            }
        });
        let bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.window_bounds_changed(window, cx);
        });
        let feedback = cx.observe_self(move |this, cx| {
            if this.feedback_pending {
                Self::schedule_feedback(main_window, cx);
            }
        });
        Self::schedule_feedback(main_window, cx);
        let list_focus = cx.focus_handle();
        if startup.show_onboarding && !onboarding_completed {
            window.focus(&list_focus, cx);
        } else if search_auto_focus && (!startup.hidden || tray.is_err()) {
            search.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&list_focus, cx);
        }
        Self {
            service,
            _tray: tray.ok(),
            tray_sender,
            tray_enabled,
            _tray_events: tray_events,
            hotkey: hotkey.ok().flatten(),
            hotkey_sender,
            _hotkey_events: hotkey_events,
            paste_hotkeys: paste_hotkeys.ok().flatten().map(|(hotkeys, _)| hotkeys),
            paste_hotkey_sender,
            _paste_hotkey_events: paste_hotkey_events,
            _outside_click_monitor: outside_click_monitor.ok().flatten(),
            _outside_click_events: outside_click_events,
            exiting,
            history: HistoryState::default(),
            file_card_info: HashMap::new(),
            file_card_checked: HashMap::new(),
            groups: Vec::new(),
            search,
            group_select,
            group_name_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr(language, "分组名称", "Group name"))
            }),
            group_editor_open: false,
            group_rename_id: None,
            group_save_pending: false,
            group_edit_error: None,
            group_reorder_pending: false,
            group_delete_pending: false,
            clear_pending: false,
            clear_all_error: None,
            clear_all_pending: false,
            batch_mode: false,
            selected_ids: HashSet::new(),
            selection_anchor: None,
            batch_pending: false,
            batch_paste_pending: None,
            confirmation: None,
            confirmation_error: None,
            group_move_id: None,
            group_move_pending: false,
            preview: PreviewState::default(),
            preview_input: cx.new(|cx| TextareaState::new(window, cx)),
            preview_source_hash: None,
            preview_editing: false,
            preview_edit_requested: false,
            preview_save_pending: false,
            image_zoom_percent: 100,
            image_scroll: ScrollHandle::default(),
            rendered_images: HashSet::new(),
            image_release_task: None,
            hover_preview: PreviewState::default(),
            hover_popup: None,
            hover_popup_opening: None,
            hover_source_active: false,
            hover_popup_active: false,
            hover_task: None,
            hover_close_task: None,
            hover_autoclose_task: None,
            hover_preference,
            hover_preference_pending: false,
            pending_row_click: None,
            row_click_task: None,
            save_as_pending: None,
            list_focus,
            scroll: VirtualListScrollHandle::new(),
            row_sizes: Rc::new(Vec::new()),
            row_offsets: vec![px(0.)],
            monitoring: startup.monitoring,
            onboarding_completed,
            onboarding_step: 0,
            onboarding_pending: false,
            show_onboarding: startup.show_onboarding,
            settings_window: None,
            settings_window_opening: false,
            theme,
            theme_pending: false,
            window_effect,
            window_effect_pending: false,
            language,
            language_pending: false,
            hotkey_choice,
            hotkey_pending: false,
            quick_paste_enabled,
            quick_paste_pending_setting: false,
            quick_paste_registration_warning: None,
            paste_shortcuts,
            paste_shortcuts_pending: None,
            paste_shortcut_status: None,
            quick_paste_pending: None,
            autostart: PendingPref {
                value: autostart.unwrap_or(false),
                pending: false,
            },
            data_size: None,
            daily_counts: None,
            data_size_pending: false,
            database_maintenance_pending: false,
            export_pending: false,
            window_position: PendingPref {
                value: window_position,
                pending: false,
            },
            persist_window_size,
            persist_window_size_pending: false,
            auto_reset_state: PendingPref {
                value: auto_reset_state,
                pending: false,
            },
            search_auto_focus: PendingPref {
                value: search_auto_focus,
                pending: false,
            },
            search_auto_clear: PendingPref {
                value: search_auto_clear,
                pending: false,
            },
            skip_clear_confirm: PendingPref {
                value: skip_clear_confirm,
                pending: false,
            },
            paste_close_window: PendingPref {
                value: paste_close_window,
                pending: false,
            },
            paste_key: PendingPref {
                value: paste_key,
                pending: false,
            },
            paste_move_to_top: PendingPref {
                value: paste_move_to_top,
                pending: false,
            },
            display,
            display_pending: false,
            audio: PendingPref {
                value: audio,
                pending: false,
            },
            monitor_types: PendingPref {
                value: monitor_types,
                pending: false,
            },
            app_filter: PendingPref {
                value: app_filter,
                pending: false,
            },
            running_apps: Vec::new(),
            running_apps_pending: false,
            last_window_size: initial_window_size,
            paste_target: None,
            paste_pending: None,
            reorder_pending: false,
            drop_target: None,
            drag_motion: DragMotionState::default(),
            reorder_before: None,
            reorder_offsets: HashMap::new(),
            feedback_revision: 0,
            history_drag_direction: 0,
            external_drag: None,
            drag_token: 0,
            paused,
            pause_pending: false,
            message: if startup_errors.is_empty() {
                String::new()
            } else {
                startup_errors.join("；")
            },
            is_error: !startup_errors.is_empty(),
            feedback_pending: !startup_errors.is_empty(),
            _subscriptions: vec![
                subscription,
                group_subscription,
                appearance,
                activation,
                bounds,
                feedback,
            ],
            _events: event_task,
            search_task: None,
            feedback_task: None,
            history_drag_scroll_task: None,
            window_size_task: None,
        }
    }
}
