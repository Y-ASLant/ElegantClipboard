use super::*;
use clipboard_platform::InstanceBusy;

gpui_kit::assets::icon_assets!(ActionIcons, [Trash, ArrowUpToLine]);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match ActionIcons.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ActionIcons.list(path)?);
        Ok(paths)
    }
}

pub fn run(options: Options) -> anyhow::Result<()> {
    tracing::info!(
        monitoring = options.monitor,
        hidden = options.start_hidden,
        "UI startup requested"
    );
    let data_dir = options.data_dir.clone();
    if clipboard_platform::show_existing_instance(data_dir.clone())? {
        tracing::info!("Existing instance activated");
        return Ok(());
    }
    let (service, events) = match Service::start(options.data_dir, options.monitor) {
        Ok(started) => started,
        Err(error) if error.is::<InstanceBusy>() => {
            for _ in 0..10 {
                if clipboard_platform::show_existing_instance(data_dir.clone())? {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let startup = StartupMode {
        monitoring: options.monitor,
        hidden: options.start_hidden,
        show_onboarding: !options.smoke_test,
    };
    let initial_window_size = service.initial_preferences.window_size.unwrap_or_default();
    let smoke_test = options.smoke_test;
    let startup_error = Rc::new(RefCell::new(None));
    let window_error = startup_error.clone();
    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            crate::design_system::apply(cx);
            for monitor in cx.displays() {
                tracing::info!(
                    display_id = ?monitor.id(), bounds = ?monitor.bounds(),
                    visible_bounds = ?monitor.visible_bounds(), "Startup display"
                );
            }
            #[cfg(target_os = "linux")]
            cx.spawn(async move |cx| {
                // tray-icon uses GTK, while GPUI owns the platform event loop.
                // Both creation and dispatch stay on this foreground thread.
                loop {
                    let context = gtk::glib::MainContext::default();
                    for _ in 0..64 {
                        if !context.pending() {
                            break;
                        }
                        context.iteration(false);
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                }
            })
            .detach();
            // 晚于 gpui-component 注册，才能在根 div 上覆盖它刷的不透明背景。
            window_effect::register_root_plugin(cx);
            cx.bind_keys([
                KeyBinding::new("down", Next, Some("HistoryList")),
                KeyBinding::new("up", Previous, Some("HistoryList")),
                KeyBinding::new("left", PreviousCategory, Some("HistoryList")),
                KeyBinding::new("right", NextCategory, Some("HistoryList")),
                KeyBinding::new("ctrl-left", PreviousGroup, Some("HistoryList")),
                KeyBinding::new("ctrl-right", NextGroup, Some("HistoryList")),
                KeyBinding::new("home", First, Some("HistoryList")),
                KeyBinding::new("end", Last, Some("HistoryList")),
                KeyBinding::new("pageup", PageUp, Some("HistoryList")),
                KeyBinding::new("pagedown", PageDown, Some("HistoryList")),
                KeyBinding::new("ctrl-a", SelectAllLoaded, Some("HistoryList")),
                KeyBinding::new("enter", ActivateSelected, Some("HistoryList")),
                KeyBinding::new("shift-enter", PastePlainTextSelected, Some("HistoryList")),
                KeyBinding::new("ctrl-enter", PasteSelected, Some("HistoryList")),
                KeyBinding::new("delete", DeleteSelected, Some("HistoryList")),
                KeyBinding::new("escape", DismissOrHide, Some("ClipboardApp")),
                KeyBinding::new("ctrl-f", FocusSearch, Some("ClipboardApp")),
                KeyBinding::new("down", FocusFirstHistoryItem, Some("HistorySearch > Input")),
            ]);
            #[cfg(target_os = "macos")]
            cx.bind_keys([
                KeyBinding::new("cmd-left", PreviousGroup, Some("HistoryList")),
                KeyBinding::new("cmd-right", NextGroup, Some("HistoryList")),
                KeyBinding::new("cmd-a", SelectAllLoaded, Some("HistoryList")),
                KeyBinding::new("cmd-enter", PasteSelected, Some("HistoryList")),
                KeyBinding::new("cmd-f", FocusSearch, Some("ClipboardApp")),
            ]);
            cx.on_window_closed(|cx, _| {
                tracing::debug!(remaining_windows = cx.windows().len(), "Window closed");
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(
                None,
                size(
                    px(initial_window_size.width as f32),
                    px(initial_window_size.height as f32),
                ),
                cx,
            );
            let tray_enabled = Rc::new(Cell::new(false));
            let exiting = Rc::new(Cell::new(false));
            let smoke_exiting = exiting.clone();
            tracing::info!(?bounds, hidden = startup.hidden, "Opening main window");
            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ElegantClipboard".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(400.), px(520.))),
                    focus: !cfg!(windows) && !startup.hidden,
                    show: !cfg!(windows) && !startup.hidden,
                    app_id: Some("com.aslant.elegant-clipboard-gpui".into()),
                    ..WindowOptions::default()
                },
                cx,
                |window, cx| {
                    window_lifecycle::log_window_geometry(window, cx, "main_created");
                    let view = cx.new(|cx| {
                        ClipboardView::new(
                            service,
                            events,
                            startup,
                            tray_enabled.clone(),
                            exiting.clone(),
                            window,
                            cx,
                        )
                    });
                    #[cfg(windows)]
                    view.update(cx, |view, _| view.initial_window_bounds = Some(bounds));
                    if smoke_test {
                        let settings_view = view.clone();
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(250))
                                .await;
                            settings_view.update(cx, |view, cx| {
                                view.open_settings_window(SettingsPage::Data, cx);
                            });
                        })
                        .detach();
                    }
                    let close_view = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        tracing::debug!(
                            exiting = exiting.get(),
                            tray_enabled = tray_enabled.get(),
                            "Main window close requested"
                        );
                        if exiting.get() {
                            return true;
                        }
                        if !close_view.update(cx, |this, cx| this.close_editor(window, cx)) {
                            return false;
                        }
                        if !tray_enabled.get() {
                            return true;
                        }
                        close_view.update(cx, |this, cx| {
                            if this.batch_pending
                                || this.paste_state.pending()
                                || this.paste_state.merging()
                            {
                                return;
                            }
                            this.hide_window(window, cx);
                        });
                        false
                    });
                    view
                },
            ) {
                tracing::error!(error = %format_args!("{error:#}"), "Main window creation failed");
                *window_error.borrow_mut() =
                    Some(anyhow::anyhow!("无法创建 ElegantClipboard 窗口：{error:?}"));
                cx.quit();
                return;
            }
            #[cfg(not(windows))]
            if !startup.hidden {
                cx.activate(true);
            }
            if smoke_test {
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(3)).await;
                    smoke_exiting.set(true);
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
    let error = startup_error.borrow_mut().take();
    error.map_or(Ok(()), Err)
}
