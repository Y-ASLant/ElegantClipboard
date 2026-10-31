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
        let language = service.initial_preferences.language;
        let window_position = service.initial_preferences.window_position;
        let hover_preference = service.initial_preferences.hover_preview;
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
            service.initial_preferences.paused,
            startup.monitoring,
        );
        tray_enabled.set(tray.is_ok());
        let tray_events = Self::spawn_tray_events(tray_receiver, main_window, window, cx);
        let tray_error = tray.as_ref().err().map(ToString::to_string);
        // The hotkey thread uses try_send; a full channel must not discard the
        // second press while the first press is still waking the window.
        let (hotkey_sender, hotkey_receiver) = async_channel::unbounded();
        let hotkey_choice = service.initial_preferences.hotkey.clone();
        let hotkey = if hotkey_choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(&hotkey_choice, hotkey_sender.clone()).map(Some)
        };
        let hotkey_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(target) = hotkey_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    if this.onboarding_visible() && !this.shortcut_change_pending() {
                        this.onboarding_hotkey_used = true;
                    }
                    if this.window_show_task.is_some()
                        || (native_window::is_window_shown(window)
                            && !native_window::is_window_minimized(window))
                    {
                        this.hide_visible_window(window, cx);
                        return;
                    }
                    this.paste_state
                        .set_target(paste::is_external_target(window, target).then_some(target));
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
        let quick_paste_enabled = service.initial_preferences.quick_paste_enabled;
        let paste_shortcuts = service.initial_preferences.paste_shortcuts.clone();
        let initial_dedup_strategy = service.initial_preferences.dedup_strategy;
        let initial_restore_clipboard = service.initial_preferences.restore_clipboard;
        let initial_sequence_hotkey = service.initial_preferences.sequence_hotkey.clone();
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
        let startup_errors: Vec<_> = [
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
        let theme = service.initial_preferences.theme;
        let theme_color = service.initial_preferences.theme_color;
        crate::design_system::set_theme_color(theme_color, cx);
        let corner_radius = service.initial_preferences.corner_radius;
        crate::design_system::set_corner_radius(corner_radius, cx);
        let window_effect = service.initial_preferences.window_effect;
        let paused = service.initial_preferences.paused;
        let (initial_window_size, persist_window_size) = (
            service.initial_preferences.window_size,
            service.initial_preferences.persist_window_size,
        );
        let auto_reset_state = service.initial_preferences.auto_reset_state;
        let search_auto_focus = service.initial_preferences.search_auto_focus;
        let search_auto_clear = service.initial_preferences.search_auto_clear;
        let skip_clear_confirm = service.initial_preferences.skip_clear_confirm;
        let (paste_close_window, paste_key, paste_move_to_top) = (
            service.initial_preferences.paste_close_window,
            service.initial_preferences.paste_key,
            service.initial_preferences.paste_move_to_top,
        );
        let display = service.initial_preferences.display;
        let audio = service.initial_preferences.audio;
        let (retention, retention_saved) = (
            service.initial_preferences.retention,
            service.initial_preferences.retention_saved,
        );
        let transient_filter = service.initial_preferences.transient_filter;
        let monitor_types = service.initial_preferences.monitor_types;
        let app_filter = service.initial_preferences.app_filter.clone();
        let onboarding_completed = service.initial_preferences.onboarding_completed;
        if !startup.hidden || tray.is_err() {
            cx.defer_in(window, |this, window, cx| this.show_window(window, cx));
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
            } else if this.onboarding_visible() {
                cx.notify();
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
            this.suspend_onboarding(cx);
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
            main_window,
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
            window_show_task: None,
            #[cfg(windows)]
            initial_window_bounds: None,
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
            show_onboarding: startup.show_onboarding && !onboarding_completed,
            onboarding_hotkey_used: false,
            onboarding_error: None,
            onboarding_shortcut: None,
            onboarding_practice: None,
            settings_window: None,
            settings_window_opening: false,
            theme,
            theme_pending: false,
            theme_color,
            theme_color_pending: false,
            corner_radius: PendingPref::new(corner_radius),
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
            autostart: PendingPref::new(autostart.unwrap_or(false)),
            data_size: None,
            daily_counts: None,
            data_size_pending: false,
            database_maintenance_pending: false,
            export_pending: false,
            data_import_pending: false,
            data_import_result: None,
            window_position: PendingPref::new(window_position),
            persist_window_size,
            persist_window_size_pending: false,
            auto_reset_state: PendingPref::new(auto_reset_state),
            search_auto_focus: PendingPref::new(search_auto_focus),
            search_auto_clear: PendingPref::new(search_auto_clear),
            skip_clear_confirm: PendingPref::new(skip_clear_confirm),
            paste_close_window: PendingPref::new(paste_close_window),
            paste_key: PendingPref::new(paste_key),
            paste_move_to_top: PendingPref::new(paste_move_to_top),
            dedup_strategy: PendingPref::new(initial_dedup_strategy),
            restore_clipboard: PendingPref::new(initial_restore_clipboard),
            sequence_hotkey: initial_sequence_hotkey,
            sequence_hotkey_pending: false,
            display,
            display_pending: false,
            retention_saved,
            retention: PendingPref::new(retention),
            audio: PendingPref::new(audio),
            transient_filter: PendingPref::new(transient_filter),
            monitor_types: PendingPref::new(monitor_types),
            app_filter: PendingPref::new(app_filter),
            running_apps: Vec::new(),
            running_apps_pending: false,
            last_window_size: initial_window_size,
            paste_state: Default::default(),
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
