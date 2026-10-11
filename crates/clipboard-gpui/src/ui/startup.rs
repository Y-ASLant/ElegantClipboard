use super::*;
use clipboard_platform::InstanceBusy;
pub fn run(options: Options) -> anyhow::Result<()> {
    let data_dir = options.data_dir.clone();
    if clipboard_platform::show_existing_instance(data_dir.clone())? {
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
    let initial_window_size = service.initial_window_size.unwrap_or_default();
    let smoke_test = options.smoke_test;
    let startup_error = Rc::new(RefCell::new(None));
    let window_error = startup_error.clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
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
                KeyBinding::new("space", PreviewSelected, Some("HistoryList")),
                KeyBinding::new("escape", ClosePreview, Some("Preview")),
                KeyBinding::new("escape", ClosePreview, Some("Preview > Input")),
                KeyBinding::new("escape", DismissOrHide, Some("ClipboardApp")),
                KeyBinding::new("ctrl-f", FocusSearch, Some("ClipboardApp")),
                KeyBinding::new("down", FocusFirstHistoryItem, Some("HistorySearch > Input")),
            ]);
            cx.on_window_closed(|cx, _| {
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
            if let Err(error) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ElegantClipboard".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(420.), px(520.))),
                    focus: !startup.hidden,
                    show: !startup.hidden,
                    app_id: Some("com.aslant.elegant-clipboard-gpui".into()),
                    ..WindowOptions::default()
                },
                cx,
                |window, cx| {
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
                        if exiting.get() || !tray_enabled.get() {
                            return true;
                        }
                        close_view.update(cx, |this, cx| {
                            if this.batch_pending
                                || this.paste_pending.is_some()
                                || this.batch_paste_pending.is_some()
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
                *window_error.borrow_mut() =
                    Some(anyhow::anyhow!("无法创建 ElegantClipboard 窗口：{error:?}"));
                cx.quit();
                return;
            }
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
