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
        #[cfg(target_os = "linux")]
        Self::watch_native_listeners(cx);
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
            this.handle_search_input(event, window, cx);
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
            native_window::set_window_visible(window, true);
        }
        let tray_events = Self::spawn_tray_events(tray_receiver, main_window, window, cx);
        let tray_error = tray.as_ref().err().map(ToString::to_string);
        // The hotkey thread uses try_send; a full channel must not discard the
        // second press while the first press is still waking the window.
        let (hotkey_sender, hotkey_receiver) = async_channel::unbounded();
        let hotkey_choice = service.initial_hotkey.clone();
        let hotkey = if hotkey_choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(&hotkey_choice, hotkey_sender.clone()).map(Some)
        };
        let hotkey_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(target) = hotkey_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    if native_window::is_window_shown(window)
                        && !native_window::is_window_minimized(window)
                    {
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
        let initial_dedup_strategy = service.initial_dedup_strategy;
        let initial_restore_clipboard = service.initial_restore_clipboard;
        let initial_sequence_hotkey = service.initial_sequence_hotkey.clone();
        let channels = Self::register_paste_hotkeys(
            startup.monitoring,
            quick_paste_enabled,
            &paste_shortcuts,
            &initial_sequence_hotkey,
            &hotkey_choice,
        );
        let paste_hotkey_sender = channels.paste_sender;
        let sequence_hotkey_sender = channels.sequence_sender;
        let paste_hotkeys = channels.result;
        let (paste_hotkey_events, _sequence_hotkey_events_task) = (
            cx.spawn_in(window, async move |view, cx| {
                while let Ok(event) = channels.paste_receiver.recv().await {
                    if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                        this.handle_quick_hotkey(event, window, cx);
                    })
                    .is_err()
                    {
                        break;
                    }
                }
            }),
            cx.spawn_in(window, async move |view, cx| {
                while let Ok(event) = channels.sequence_receiver.recv().await {
                    if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                        this.handle_sequence_advance(event.target, window, cx);
                    })
                    .is_err()
                    {
                        break;
                    }
                }
            }),
        );
        let paste_hotkey_error = paste_hotkeys.as_ref().err().map(ToString::to_string);
        let paste_hotkey_warnings = paste_hotkeys
            .as_ref()
            .ok()
            .and_then(|result| result.as_ref().map(|(_, warnings)| warnings.clone()))
            .unwrap_or_default();
        let (outside_click_sender, outside_click_receiver) = async_channel::bounded(16);
        let outside_click_monitor = if tray.is_ok() {
            native_window::window_id(window)
                .ok_or_else(|| anyhow::anyhow!("无法取得历史窗口句柄"))
                .and_then(|handle| OutsideClickMonitor::start(outside_click_sender, handle))
                .map(Some)
        } else {
            Ok(None)
        };
        let outside_click_events = Self::outside_click_events(outside_click_receiver, window, cx);
        let outside_click_error = outside_click_monitor
            .as_ref()
            .err()
            .map(ToString::to_string);
        let autostart = clipboard_platform::autostart::enabled(&service.data_dir);
        let mut startup_errors: Vec<_> = [
            tray_error.map(|error| {
                format!(
                    "{}: {error}",
                    tr(
                        language,
                        "托盘不可用，关闭窗口将退出",
                        "The tray is unavailable; closing the window will exit"
                    )
                )
            }),
            hotkey_error,
            paste_hotkey_error,
            paste_registration_summary(language, &paste_hotkey_warnings),
            outside_click_error,
            Self::autostart_startup_error(language, &autostart),
            service.initial_data_warning.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let theme = service.initial_theme;
        let corner_radius = service.initial_corner_radius;
        crate::design_system::set_corner_radius(corner_radius, cx);
        let window_effect = service.initial_window_effect;
        let paused = service.initial_paused;
        let (initial_window_size, persist_window_size) = (
            service.initial_window_size,
            service.initial_persist_window_size,
        );
        let auto_reset_state = service.initial_auto_reset_state;
        let search_auto_focus = service.initial_search_auto_focus;
        let search_auto_clear = service.initial_search_auto_clear;
        let skip_clear_confirm = service.initial_skip_clear_confirm;
        let (paste_close_window, paste_key, paste_move_to_top) = (
            service.initial_paste_close_window,
            service.initial_paste_key,
            service.initial_paste_move_to_top,
        );
        let display = service.initial_display;
        let audio = service.initial_audio;
        let (retention, retention_saved) =
            (service.initial_retention, service.initial_retention_saved);
        let transient_filter = service.initial_transient_filter;
        let monitor_types = service.initial_monitor_types;
        let app_filter = service.initial_app_filter.clone();
        let onboarding_completed = service.initial_onboarding_completed;
        if (!startup.hidden || tray.is_err())
            && let Err(error) = Self::position_window_deferred(window, window_position, cx)
        {
            startup_errors.push(error.to_string());
        }
        acrylic::init_window_effect(window_effect, window, cx);
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
                Self::schedule_feedback(cx);
            }
        });
        Self::schedule_feedback(cx);
        cx.on_release(|this, cx| {
            for image in this.thumbnails.drain_images() {
                cx.drop_image(image, None);
            }
        })
        .detach();
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
            sequence_hotkey_sender,
            _sequence_hotkey_events: _sequence_hotkey_events_task,
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
            editor: PreviewState::default(),
            editor_input: cx.new(|cx| TextareaState::new(window, cx)),
            note_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr(
                    language,
                    "添加备注（参与搜索）",
                    "Add a note (searchable)",
                ))
            }),
            note_save_pending: false,
            editor_source_hash: None,
            editor_save_pending: false,
            editor_error: None,
            rendered_images: HashSet::new(),
            image_visible_range: 0..0,
            image_sync_pending: false,
            image_release_task: None,
            thumbnails: thumbnail_cache::ThumbnailCache::default(),
            thumbnail_scale: window.scale_factor(),
            _thumbnail_maintenance: Self::maintain_thumbnails(cx),
            hover_preview: PreviewState::default(),
            hover_popup: None,
            hover_popup_opening: None,
            hover_source_active: false,
            hover_task: None,
            hover_preference,
            hover_preference_pending: false,
            pending_row_click: None,
            row_click_task: None,
            save_as_pending: None,
            list_focus,
            scroll: VirtualListScrollHandle::new(),
            scroll_to_top_task: None,
            row_sizes: Rc::new(Vec::new()),
            row_offsets: vec![px(0.)],
            monitoring: startup.monitoring,
            onboarding_completed,
            onboarding_step: 0,
            onboarding_revision: 0,
            onboarding_pending: false,
            show_onboarding: startup.show_onboarding,
            settings_window: None,
            settings_window_opening: false,
            theme,
            theme_pending: false,
            corner_radius: PendingPref {
                value: corner_radius,
                pending: false,
            },
            window_effect,
            window_effect_pending: false,
            language,
            language_pending: false,
            hotkey_choice,
            hotkey_pending: false,
            shortcut_change: None,
            quick_paste_enabled,
            quick_paste_pending_setting: false,
            quick_paste_registration_warning: None,
            paste_shortcuts,
            paste_shortcuts_pending: None,
            paste_shortcut_status: None,
            quick_paste_pending: Default::default(),
            autostart: PendingPref {
                value: autostart.unwrap_or(false),
                pending: false,
            },
            data_size: None,
            daily_counts: None,
            data_size_pending: false,
            database_maintenance_pending: false,
            export_pending: false,
            data_import_pending: false,
            data_import_result: None,
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
            dedup_strategy: PendingPref {
                value: initial_dedup_strategy,
                pending: false,
            },
            restore_clipboard: PendingPref {
                value: initial_restore_clipboard,
                pending: false,
            },
            sequence_queue: None,
            sequence_hotkey: initial_sequence_hotkey,
            sequence_hotkey_pending: false,
            display,
            display_pending: false,
            retention_saved,
            retention: PendingPref {
                value: retention,
                pending: false,
            },
            audio: PendingPref {
                value: audio,
                pending: false,
            },
            transient_filter: PendingPref {
                value: transient_filter,
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
            paste_pending: Default::default(),
            paste_delivery_task: None,
            reorder_pending: false,
            drop_target: None,
            drag_motion: DragMotionState::default(),
            reorder_before: None,
            reorder_offsets: HashMap::new(),
            feedback_revision: 0,
            history_drag_direction: 0,
            external_drag: None,
            drag_preview_window: None,
            drag_preparation: None,
            drag_snapshot: None,
            drag_deleted_ids: HashSet::new(),
            drag_viewport: Rc::new(Cell::new(None)),
            drag_release_pending: false,
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
