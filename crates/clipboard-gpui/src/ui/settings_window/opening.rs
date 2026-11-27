use super::*;
use gpui_kit::component::notification::Notification;

pub(super) fn present(
    owner: &Entity<ClipboardView>,
    page: SettingsPage,
    main: &mut Window,
    cx: &mut App,
) -> anyhow::Result<()> {
    if let Some(message) = owner.read(cx).main_hide_blocker(cx) {
        main.push_notification(Notification::warning(message), cx);
        return Ok(());
    }

    let existing = owner.read(cx).settings_window;
    let show = |handle: AnyWindowHandle, cx: &mut App| {
        handle.update(cx, |_, window, _| {
            native_window::set_window_visible(window, true)
        })
    };
    if !existing.is_some_and(|handle| show(handle, cx).is_ok()) {
        if existing.is_some() {
            owner.update(cx, |this, _| this.settings_window = None);
        }
        let handle = create(owner, page, cx)?;
        show(handle, cx)?;
        owner.update(cx, |this, cx| {
            this.settings_window = Some(handle);
            this.refresh_data_size(cx);
            this.refresh_daily_counts(cx);
        });
    }

    // The history panel floats above settings. Only dismiss its clean note and
    // hide it once settings actually exists, never to prepare for a fallible open.
    owner.update(cx, |this, cx| this.hide_window(main, cx));
    cx.activate(true);
    Ok(())
}

fn create(
    owner: &Entity<ClipboardView>,
    page: SettingsPage,
    cx: &mut App,
) -> anyhow::Result<AnyWindowHandle> {
    let view = owner.read(cx);
    let theme = view.theme;
    let language = view.language;
    let window_effect = view.window_effect.value;
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(800.), px(540.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(tr(language, "设置", "Settings").into()),
            ..TitleBar::title_bar_options()
        }),
        window_min_size: Some(size(px(700.), px(420.))),
        focus: false,
        show: false,
        app_id: Some("com.aslant.ElegantClipboard.settings".into()),
        ..TitleBar::window_options()
    };
    let owner = owner.downgrade();
    let (handle, _) = gpui_kit::open_window(options, cx, move |window, cx| {
        apply_theme(theme, window, cx);
        acrylic::init_window_effect(window_effect, window, cx);
        window.set_window_title(tr(language, "设置", "Settings"));
        window.on_window_should_close(cx, |window, _| {
            // Release cleanup clears only the matching owner handle.
            window.remove_window();
            false
        });
        cx.new(|cx| SettingsWindowView::new(owner, page, window, cx))
    })?;
    Ok(handle)
}
