use crate::paste;
use crate::tray::{self, TrayCommand};
use crate::visual::{self, CONTROL_HEIGHT, GROUP_BAR_HEIGHT, PAGE_PADDING, ROW_HEIGHT};
use crate::{
    options::Options,
    state::{
        HistoryState, PreviewState, drag_edge_target_index, drag_reorder_offsets,
        next_drag_scroll_index, reorder_offsets,
    },
};
use clipboard_core::{
    FilePreviewEntry, HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    database::Group,
    preferences::{HotkeyPreference, LanguagePreference, ThemePreference, WindowSizePreference},
};
use clipboard_platform::hotkey::Hotkey;
use clipboard_platform::{Command, DataSizeInfo, Event, FailureKind, InstanceBusy, Service};
use directories::UserDirs;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::*,
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        *,
    },
    *,
};
use std::time::Duration;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};
use tray_icon::TrayIcon;

fn tr(language: LanguagePreference, chinese: &'static str, english: &'static str) -> &'static str {
    match language {
        LanguagePreference::Chinese => chinese,
        LanguagePreference::English => english,
    }
}

fn hotkey_label(language: LanguagePreference, choice: HotkeyPreference) -> &'static str {
    if choice == HotkeyPreference::Disabled {
        tr(language, "关闭", "Disabled")
    } else {
        choice.label()
    }
}

fn localize_service_message(language: LanguagePreference, message: String) -> String {
    if language == LanguagePreference::Chinese {
        return message;
    }
    match message.as_str() {
        "富文本超过 1 MiB，按纯文本保存" => {
            "Rich text exceeded 1 MiB and was saved as plain text".into()
        }
        "已在资源管理器中定位" => "Located in File Explorer".into(),
        "已打开数据目录" => "Data folder opened".into(),
        "路径已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Paths copied; switch to the target app and press Ctrl+V".into()
        }
        "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Plain text copied; switch to the target app and press Ctrl+V".into()
        }
        "图片已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Image copied; switch to the target app and press Ctrl+V".into()
        }
        "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Files copied; switch to the target app and press Ctrl+V".into()
        }
        "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Rich text copied; switch to the target app and press Ctrl+V".into()
        }
        "富文本格式未写回，已按纯文本复制" => {
            "Rich-text formatting could not be restored; copied as plain text".into()
        }
        "已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Copied; switch to the target app and press Ctrl+V".into()
        }
        _ => message,
    }
}

gpui_kit::actions!(
    history,
    [
        Next,
        Previous,
        First,
        Last,
        PageUp,
        PageDown,
        SelectAllLoaded,
        CopySelected,
        PasteSelected,
        DeleteSelected,
        FocusSearch,
        PreviewSelected,
        ClosePreview,
        CancelDrag
    ]
);

#[derive(Clone, Copy)]
struct StartupMode {
    monitoring: bool,
    hidden: bool,
}

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
                KeyBinding::new("home", First, Some("HistoryList")),
                KeyBinding::new("end", Last, Some("HistoryList")),
                KeyBinding::new("pageup", PageUp, Some("HistoryList")),
                KeyBinding::new("pagedown", PageDown, Some("HistoryList")),
                KeyBinding::new("ctrl-a", SelectAllLoaded, Some("HistoryList")),
                KeyBinding::new("enter", CopySelected, Some("HistoryList")),
                KeyBinding::new("ctrl-enter", PasteSelected, Some("HistoryList")),
                KeyBinding::new("delete", DeleteSelected, Some("HistoryList")),
                KeyBinding::new("space", PreviewSelected, Some("HistoryList")),
                KeyBinding::new("escape", ClosePreview, Some("Preview")),
                KeyBinding::new("escape", ClosePreview, Some("Preview > Input")),
                KeyBinding::new("escape", CancelDrag, Some("ClipboardApp")),
                KeyBinding::new("ctrl-f", FocusSearch, Some("ClipboardApp")),
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
            if let Err(error) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ElegantClipboard".into()),
                        ..TitleBar::title_bar_options()
                    }),
                    window_min_size: Some(size(px(420.), px(520.))),
                    focus: !startup.hidden,
                    show: !startup.hidden,
                    app_id: Some("com.aslant.elegant-clipboard-gpui".into()),
                    ..TitleBar::window_options()
                },
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
                                view.open_settings_window(cx);
                            });
                        })
                        .detach();
                    }
                    window.on_window_should_close(cx, move |window, _| {
                        if exiting.get() || !tray_enabled.get() {
                            return true;
                        }
                        tray::set_window_visible(window, false);
                        false
                    });
                    cx.new(|cx| Root::new(view, window, cx))
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

#[derive(Clone)]
struct HistoryDrag {
    id: i64,
    pinned: bool,
    favorite_only: bool,
    group_id: Option<i64>,
    generation: u64,
    kind: &'static str,
    preview: String,
    language: LanguagePreference,
}

#[derive(Clone)]
struct GroupDrag {
    id: i64,
    name: String,
    group_ids: Vec<i64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropTarget {
    id: i64,
    after: bool,
    allowed: bool,
}

impl Render for HistoryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        visual::reveal(
            div()
                .w(px(280.))
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .shadow_md()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "⠿  {}{}",
                            if self.pinned {
                                tr(self.language, "置顶 · ", "Pinned · ")
                            } else {
                                ""
                            },
                            self.kind
                        )),
                )
                .child(
                    div()
                        .rounded_sm()
                        .bg(cx.theme().muted)
                        .px_2()
                        .py_1()
                        .text_sm()
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(self.preview.clone()),
                ),
            ("drag-preview", self.id as usize),
            cx,
        )
    }
}

impl Render for GroupDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        visual::reveal(
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().background)
                .shadow_md()
                .text_sm()
                .child(format!("⠿  {}", self.name)),
            ("group-drag-preview", self.id as usize),
            cx,
        )
    }
}

struct ClipboardView {
    service: Service,
    _tray: Option<TrayIcon>,
    tray_sender: async_channel::Sender<TrayCommand>,
    tray_enabled: Rc<Cell<bool>>,
    _tray_events: Task<()>,
    hotkey: Option<Hotkey>,
    hotkey_sender: async_channel::Sender<(isize, u32)>,
    _hotkey_events: Task<()>,
    exiting: Rc<Cell<bool>>,
    history: HistoryState,
    groups: Vec<Group>,
    search: Entity<InputState>,
    group_name_input: Entity<InputState>,
    group_editor_open: bool,
    group_rename_id: Option<i64>,
    group_save_pending: bool,
    group_reorder_pending: bool,
    group_drop_target: Option<DropTarget>,
    group_reorder_before: Option<Vec<i64>>,
    group_feedback_ids: HashSet<i64>,
    group_feedback_revision: usize,
    group_scroll: ScrollHandle,
    group_drag_direction: i8,
    group_delete_id: Option<i64>,
    group_delete_pending: bool,
    clear_confirm_open: bool,
    clear_pending: bool,
    clear_all_confirm_open: bool,
    clear_all_pending: bool,
    selected_ids: HashSet<i64>,
    batch_confirm_open: bool,
    batch_pending: bool,
    batch_paste_pending: Option<(isize, u32)>,
    group_move_id: Option<i64>,
    group_move_pending: bool,
    preview: PreviewState,
    preview_input: Entity<TextareaState>,
    preview_source_hash: Option<String>,
    preview_editing: bool,
    preview_save_pending: bool,
    save_as_pending: Option<i64>,
    list_focus: FocusHandle,
    scroll: UniformListScrollHandle,
    monitoring: bool,
    settings_window: Option<AnyWindowHandle>,
    settings_window_opening: bool,
    theme: ThemePreference,
    theme_pending: bool,
    language: LanguagePreference,
    language_pending: bool,
    hotkey_choice: HotkeyPreference,
    hotkey_pending: bool,
    autostart: bool,
    autostart_pending: bool,
    data_size: Option<DataSizeInfo>,
    data_size_pending: bool,
    database_maintenance_pending: bool,
    export_pending: bool,
    window_pinned: bool,
    last_window_size: Option<WindowSizePreference>,
    paste_target: Option<(isize, u32)>,
    paste_pending: Option<(i64, (isize, u32))>,
    reorder_pending: bool,
    drop_target: Option<DropTarget>,
    reorder_before: Option<Vec<i64>>,
    reorder_offsets: HashMap<i64, isize>,
    feedback_revision: usize,
    history_drag_direction: i8,
    paused: bool,
    pause_pending: bool,
    message: String,
    is_error: bool,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
    search_task: Option<Task<()>>,
    feedback_task: Option<Task<()>>,
    group_feedback_task: Option<Task<()>>,
    group_drag_scroll_task: Option<Task<()>>,
    history_drag_scroll_task: Option<Task<()>>,
    window_size_task: Option<Task<()>>,
}

struct SettingsWindowView {
    owner: WeakEntity<ClipboardView>,
    _owner_subscription: Subscription,
}

impl SettingsWindowView {
    fn new(owner: WeakEntity<ClipboardView>, cx: &mut Context<Self>) -> Self {
        let entity = owner
            .upgrade()
            .expect("settings window requires its owner view");
        let subscription = cx.observe(&entity, |_, _, cx| cx.notify());
        Self {
            owner,
            _owner_subscription: subscription,
        }
    }
}

impl ClipboardView {
    fn new(
        service: Service,
        events: async_channel::Receiver<Event>,
        startup: StartupMode,
        tray_enabled: Rc<Cell<bool>>,
        exiting: Rc<Cell<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language = service.initial_language;
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(
                language,
                "搜索剪贴板历史…",
                "Search clipboard history…",
            ))
        });
        let subscription = cx.subscribe_in(&search, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.clear_confirm_open = false;
                this.reset_selection();
                this.history.begin_search();
                this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                this.search_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(150))
                        .await;
                    let _ = view.update(cx, |this, cx| this.query(cx));
                }));
                cx.notify();
            } else if matches!(event, InputEvent::PressEnter { .. })
                && let Some(id) = this.history.selected
            {
                this.send(Command::Copy(id), cx);
            }
        });
        let event_task = cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = events.recv().await {
                if view
                    .update_in(cx, |this, window, cx| this.apply_event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let (tray_sender, tray_receiver) = async_channel::bounded(8);
        let tray = tray::create(tray_sender.clone(), language);
        tray_enabled.set(tray.is_ok());
        if startup.hidden && tray.is_err() {
            tray::set_window_visible(window, true);
        }
        let tray_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(command) = tray_receiver.recv().await {
                if view
                    .update_in(cx, |this, window, cx| match command {
                        TrayCommand::Show => {
                            this.paste_target = None;
                            tray::set_window_visible(window, true);
                        }
                        TrayCommand::Settings => this.open_settings_window(cx),
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
        let (hotkey_sender, hotkey_receiver) = async_channel::bounded(1);
        let hotkey_choice = service.initial_hotkey;
        let hotkey = if hotkey_choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(hotkey_choice, hotkey_sender.clone()).map(Some)
        };
        let hotkey_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(target) = hotkey_receiver.recv().await {
                if view
                    .update_in(cx, |this, window, cx| {
                        this.paste_target =
                            paste::is_external_target(window, target).then_some(target);
                        if this.paste_target.is_some() {
                            this.message = tr(
                                this.language,
                                "已从原窗口唤出，可选择记录并粘贴",
                                "Opened from the previous window; select an item to paste.",
                            )
                            .into();
                            this.is_error = false;
                        }
                        tray::set_window_visible(window, true);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let hotkey_error = hotkey.as_ref().err().map(ToString::to_string);
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
        let paused = service.initial_paused;
        let initial_window_size = service.initial_window_size;
        apply_theme(theme, window, cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme == ThemePreference::System {
                apply_theme(this.theme, window, cx);
            }
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                let had_target = this.paste_target.take().is_some();
                let had_pending = this.paste_pending.take().is_some();
                let had_batch_pending = this.batch_paste_pending.take().is_some();
                if had_target || had_pending || had_batch_pending {
                    this.message = tr(
                        this.language,
                        "已离开历史窗口；如需自动粘贴，请从目标应用重新唤出",
                        "The history window lost focus. Open it again from the target app to paste automatically.",
                    )
                    .into();
                    this.is_error = false;
                    cx.notify();
                }
            }
        });
        let bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.window_bounds_changed(window, cx);
        });
        let list_focus = cx.focus_handle();
        window.focus(&list_focus, cx);
        Self {
            service,
            _tray: tray.ok(),
            tray_sender,
            tray_enabled,
            _tray_events: tray_events,
            hotkey: hotkey.ok().flatten(),
            hotkey_sender,
            _hotkey_events: hotkey_events,
            exiting,
            history: HistoryState::default(),
            groups: Vec::new(),
            search,
            group_name_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr(language, "分组名称", "Group name"))
            }),
            group_editor_open: false,
            group_rename_id: None,
            group_save_pending: false,
            group_reorder_pending: false,
            group_drop_target: None,
            group_reorder_before: None,
            group_feedback_ids: HashSet::new(),
            group_feedback_revision: 0,
            group_scroll: ScrollHandle::new(),
            group_drag_direction: 0,
            group_delete_id: None,
            group_delete_pending: false,
            clear_confirm_open: false,
            clear_pending: false,
            clear_all_confirm_open: false,
            clear_all_pending: false,
            selected_ids: HashSet::new(),
            batch_confirm_open: false,
            batch_pending: false,
            batch_paste_pending: None,
            group_move_id: None,
            group_move_pending: false,
            preview: PreviewState::default(),
            preview_input: cx.new(|cx| TextareaState::new(window, cx)),
            preview_source_hash: None,
            preview_editing: false,
            preview_save_pending: false,
            save_as_pending: None,
            list_focus,
            scroll: UniformListScrollHandle::new(),
            monitoring: startup.monitoring,
            settings_window: None,
            settings_window_opening: false,
            theme,
            theme_pending: false,
            language,
            language_pending: false,
            hotkey_choice,
            hotkey_pending: false,
            autostart: autostart.unwrap_or(false),
            autostart_pending: false,
            data_size: None,
            data_size_pending: false,
            database_maintenance_pending: false,
            export_pending: false,
            window_pinned: false,
            last_window_size: initial_window_size,
            paste_target: None,
            paste_pending: None,
            reorder_pending: false,
            drop_target: None,
            reorder_before: None,
            reorder_offsets: HashMap::new(),
            feedback_revision: 0,
            history_drag_direction: 0,
            paused,
            pause_pending: false,
            message: if startup_errors.is_empty() {
                String::new()
            } else {
                startup_errors.join("；")
            },
            is_error: !startup_errors.is_empty(),
            _subscriptions: vec![subscription, appearance, activation, bounds],
            _events: event_task,
            search_task: None,
            feedback_task: None,
            group_feedback_task: None,
            group_drag_scroll_task: None,
            history_drag_scroll_task: None,
            window_size_task: None,
        }
    }

    fn send(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.service.send(command) {
            self.message = error.to_string();
            self.is_error = true;
            cx.notify();
            return false;
        }
        true
    }

    fn window_bounds_changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        let WindowBounds::Windowed(bounds) = window.window_bounds() else {
            return;
        };
        let Some(size) = WindowSizePreference::new(
            f32::from(bounds.size.width).round() as u32,
            f32::from(bounds.size.height).round() as u32,
        ) else {
            return;
        };
        if self.last_window_size == Some(size) {
            return;
        }
        self.window_size_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = view.update(cx, |this, cx| {
                this.send(Command::SetWindowSize(size), cx);
            });
        }));
    }

    fn refresh_data_size(&mut self, cx: &mut Context<Self>) {
        if self.data_size_pending {
            return;
        }
        if self.send(Command::QueryDataSize, cx) {
            self.data_size_pending = true;
            cx.notify();
        }
    }

    fn open_settings_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
            self.settings_window = None;
        }
        if self.settings_window_opening {
            return;
        }

        self.settings_window_opening = true;
        self.refresh_data_size(cx);
        cx.notify();
        cx.spawn(async move |owner, cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let (theme, language) =
                owner_entity.update(cx, |owner, _| (owner.theme, owner.language));
            let options = cx.update(|cx| WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(820.), px(500.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(tr(language, "设置", "Settings").into()),
                    ..TitleBar::title_bar_options()
                }),
                window_min_size: Some(size(px(700.), px(420.))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.elegant-clipboard-gpui.settings".into()),
                ..TitleBar::window_options()
            });
            let settings_owner = owner.clone();
            let close_owner = owner.clone();
            let opened = cx.open_window(options, move |window, cx| {
                apply_theme(theme, window, cx);
                window.set_window_title(tr(language, "设置", "Settings"));
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = close_owner.update(cx, |owner, cx| {
                        owner.settings_window = None;
                        cx.notify();
                    });
                    true
                });
                let settings = cx.new(|cx| SettingsWindowView::new(settings_owner.clone(), cx));
                cx.new(|cx| Root::new(settings, window, cx))
            });
            owner_entity.update(cx, |owner, cx| {
                owner.settings_window_opening = false;
                match opened {
                    Ok(handle) => {
                        owner.settings_window = Some(handle.into());
                        owner.is_error = false;
                    }
                    Err(error) => {
                        owner.message = format!(
                            "{}: {error:?}",
                            tr(
                                owner.language,
                                "无法打开设置窗口",
                                "Unable to open settings"
                            )
                        );
                        owner.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_export(&mut self, cx: &mut Context<Self>) {
        if self.export_pending {
            return;
        }
        let directory = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|path| path.to_path_buf()))
            .unwrap_or_else(|| self.service.data_dir.clone());
        self.export_pending = true;
        cx.notify();
        let selection = cx.prompt_for_new_path(&directory, Some("ElegantClipboard_backup.zip"));
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(path))) => {
                        if !this.send(Command::ExportBackup(path), cx) {
                            this.export_pending = false;
                        }
                    }
                    Ok(Ok(None)) => this.export_pending = false,
                    Ok(Err(error)) => {
                        this.export_pending = false;
                        this.message = format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "无法选择备份路径",
                                "Unable to select a backup path"
                            )
                        );
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.export_pending = false;
                        this.message = format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "备份路径选择中断",
                                "Backup path selection was interrupted"
                            )
                        );
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_save_as(&mut self, id: i64, suggested_name: String, cx: &mut Context<Self>) {
        if self.save_as_pending.is_some() {
            return;
        }
        let directory = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|path| path.to_path_buf()))
            .unwrap_or_else(|| self.service.data_dir.clone());
        self.save_as_pending = Some(id);
        cx.notify();
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(destination))) => {
                        if !this.send(Command::SaveAs { id, destination }, cx) {
                            this.save_as_pending = None;
                        }
                    }
                    Ok(Ok(None)) => this.save_as_pending = None,
                    Ok(Err(error)) => {
                        this.save_as_pending = None;
                        this.message = format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "无法选择另存为路径",
                                "Unable to select a destination"
                            )
                        );
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.save_as_pending = None;
                        this.message = format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "另存为路径选择中断",
                                "Destination selection was interrupted"
                            )
                        );
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn restore_hotkey(&mut self) -> Option<String> {
        if self.hotkey_choice == HotkeyPreference::Disabled {
            return None;
        }
        match Hotkey::start(self.hotkey_choice, self.hotkey_sender.clone()) {
            Ok(hotkey) => {
                self.hotkey = Some(hotkey);
                None
            }
            Err(error) => Some(format!(
                "{}: {error}",
                tr(
                    self.language,
                    "原快捷键也无法恢复",
                    "The previous shortcut could not be restored"
                )
            )),
        }
    }

    fn select_hotkey(&mut self, choice: HotkeyPreference, cx: &mut Context<Self>) {
        if self.hotkey_pending
            || (choice == self.hotkey_choice
                && (choice == HotkeyPreference::Disabled || self.hotkey.is_some()))
        {
            return;
        }
        self.hotkey = None;
        let registration = if choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(choice, self.hotkey_sender.clone()).map(Some)
        };
        match registration {
            Ok(hotkey) => self.hotkey = hotkey,
            Err(error) => {
                let restore_error = self.restore_hotkey();
                self.message = format!(
                    "{}: {error}",
                    tr(self.language, "快捷键切换失败", "Failed to change shortcut")
                );
                if let Some(restore_error) = restore_error {
                    self.message.push_str(&format!("；{restore_error}"));
                }
                self.is_error = true;
                cx.notify();
                return;
            }
        }
        if self.send(Command::SetHotkey(choice), cx) {
            self.hotkey_pending = true;
        } else {
            self.hotkey = None;
            if let Some(error) = self.restore_hotkey() {
                self.message.push_str(&format!("；{error}"));
            }
        }
        cx.notify();
    }

    fn query(&mut self, cx: &mut Context<Self>) {
        let generation = self.history.generation;
        if !self.send(
            Command::Query {
                search: self.search.read(cx).value().to_string(),
                limit: self.history.limit,
                favorite_only: self.history.favorite_only,
                group_id: self.history.group_id,
                generation,
            },
            cx,
        ) {
            self.history.fail_query(generation);
        }
    }

    fn select_group(&mut self, group_id: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.group_id == group_id {
            return;
        }
        self.search_task = None;
        self.group_move_id = None;
        self.group_delete_id = None;
        self.clear_confirm_open = false;
        self.reset_selection();
        self.history.set_group(group_id);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.query(cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn save_group(&mut self, cx: &mut Context<Self>) {
        if self.group_save_pending {
            return;
        }
        let name = self.group_name_input.read(cx).value().to_string();
        let command = match self.group_rename_id {
            Some(id) => Command::RenameGroup { id, name },
            None => Command::CreateGroup(name),
        };
        if self.send(command, cx) {
            self.group_save_pending = true;
            cx.notify();
        }
    }

    fn delete_group(&mut self, cx: &mut Context<Self>) {
        if self.group_delete_pending {
            return;
        }
        let Some(id) = self
            .group_delete_id
            .filter(|id| self.history.group_id == Some(*id))
        else {
            return;
        };
        if self.send(
            Command::DeleteGroup {
                id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.group_delete_pending = true;
            cx.notify();
        }
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        if !self.clear_confirm_open || self.clear_pending {
            return;
        }
        if self.send(
            Command::ClearHistory {
                group_id: self.history.group_id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.clear_pending = true;
            cx.notify();
        }
    }

    fn clear_all_history(&mut self, cx: &mut Context<Self>) {
        if !self.clear_all_confirm_open || self.clear_all_pending {
            return;
        }
        if self.send(Command::ClearAllHistory, cx) {
            self.clear_all_pending = true;
            cx.notify();
        }
    }

    fn reset_selection(&mut self) {
        self.selected_ids.clear();
        self.batch_confirm_open = false;
    }

    fn toggle_selection(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading {
            return;
        }
        if !self.history.items.iter().any(|item| item.id == id) {
            return;
        }
        if !self.selected_ids.insert(id) {
            self.selected_ids.remove(&id);
        }
        self.batch_confirm_open = false;
        cx.notify();
    }

    fn select_all_loaded(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.selected_ids
            .extend(self.history.items.iter().map(|item| item.id));
        self.batch_confirm_open = false;
        cx.notify();
    }

    fn merge_selected(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.selected_ids.len() < 2 || self.paste_pending.is_some() {
            return;
        }
        let ids: Vec<_> = self
            .history
            .items
            .iter()
            .filter_map(|item| self.selected_ids.contains(&item.id).then_some(item.id))
            .collect();
        if ids.len() < 2 {
            return;
        }
        let target = self
            .paste_target
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target));
        if self.paste_target.is_some() && target.is_none() {
            self.paste_target = None;
        }
        let command = if target.is_some() {
            Command::MergeForPaste(ids)
        } else {
            Command::MergeCopy(ids)
        };
        if self.send(command, cx) {
            self.batch_pending = true;
            self.batch_paste_pending = target;
            self.batch_confirm_open = false;
            self.message = if target.is_some() {
                tr(
                    self.language,
                    "正在合并并返回原窗口…",
                    "Merging and returning to the previous window…",
                )
            } else {
                tr(
                    self.language,
                    "正在合并所选记录…",
                    "Merging selected items…",
                )
            }
            .into();
            self.is_error = false;
            cx.notify();
        }
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if !self.batch_confirm_open || self.batch_pending || self.selected_ids.is_empty() {
            return;
        }
        let mut ids: Vec<_> = self.selected_ids.iter().copied().collect();
        ids.sort_unstable();
        if self.send(
            Command::DeleteBatch {
                ids,
                group_id: self.history.group_id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.batch_pending = true;
            cx.notify();
        }
    }

    fn move_to_group(&mut self, id: i64, target_group_id: Option<i64>, cx: &mut Context<Self>) {
        if self.group_move_pending || target_group_id == self.history.group_id {
            return;
        }
        if self.send(
            Command::MoveToGroup {
                id,
                source_group_id: self.history.group_id,
                target_group_id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.group_move_pending = true;
            cx.notify();
        }
    }

    fn apply_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::ShowWindow => {
                self.paste_target = None;
                tray::set_window_visible(window, true);
            }
            Event::Groups(groups) => {
                if self.history.group_id.is_some()
                    && !groups
                        .iter()
                        .any(|group| Some(group.id) == self.history.group_id)
                {
                    self.reset_selection();
                    self.history.set_group(None);
                    self.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    self.query(cx);
                }
                self.groups = groups;
            }
            Event::GroupReordered { from, result } => {
                self.group_reorder_pending = false;
                self.group_drop_target = None;
                self.group_drag_direction = 0;
                let before = self.group_reorder_before.take();
                match result {
                    Ok(()) => {
                        self.group_feedback_revision += 1;
                        self.group_feedback_ids = before
                            .map(|before| {
                                let after: Vec<_> =
                                    self.groups.iter().map(|group| group.id).collect();
                                reorder_offsets(&before, &after).into_keys().collect()
                            })
                            .unwrap_or_default();
                        self.group_feedback_ids.insert(from);
                        self.group_feedback_task = Some(cx.spawn(async move |view, cx| {
                            cx.background_executor()
                                .timer(visual::MOTION_DURATION)
                                .await;
                            let _ = view.update(cx, |this, cx| {
                                this.group_feedback_ids.clear();
                                cx.notify();
                            });
                        }));
                        self.message =
                            tr(self.language, "分组顺序已保存", "Group order saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.group_feedback_ids.clear();
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "分组排序失败", "Failed to reorder groups")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::GroupCreated(result) => {
                self.group_save_pending = false;
                match result {
                    Ok(group) => {
                        self.group_editor_open = false;
                        self.group_rename_id = None;
                        self.group_name_input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                        self.select_group(Some(group.id), window, cx);
                        self.message = if self.language == LanguagePreference::English {
                            format!("Group created: {}", group.name)
                        } else {
                            format!("已创建分组：{}", group.name)
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "创建分组失败", "Failed to create group")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::GroupRenamed(result) => {
                self.group_save_pending = false;
                match result {
                    Ok(group) => {
                        self.group_editor_open = false;
                        self.group_rename_id = None;
                        self.group_name_input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                        window.focus(&self.list_focus, cx);
                        self.message = if self.language == LanguagePreference::English {
                            format!("Group renamed to: {}", group.name)
                        } else {
                            format!("分组已重命名为：{}", group.name)
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "重命名分组失败", "Failed to rename group")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::GroupDeleted(result) => {
                self.group_delete_pending = false;
                self.group_delete_id = None;
                match result {
                    Ok(count) => {
                        window.focus(&self.list_focus, cx);
                        self.message = if self.language == LanguagePreference::English {
                            format!("Group deleted; {count} items moved to the default group")
                        } else {
                            format!("已删除分组，{count} 条记录移至默认分组")
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "删除分组失败", "Failed to delete group")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::HistoryCleared(result) => {
                self.clear_pending = false;
                self.clear_confirm_open = false;
                match result {
                    Ok(count) => {
                        self.message = if self.language == LanguagePreference::English {
                            format!("Cleared {count} unpinned, non-favorite items")
                        } else {
                            format!("已清理 {count} 条未置顶且未收藏的记录")
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "清理历史失败", "Failed to clear history")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::AllHistoryCleared(result) => {
                self.clear_all_pending = false;
                match result {
                    Ok(count) => {
                        self.clear_all_confirm_open = false;
                        self.selected_ids.clear();
                        self.batch_confirm_open = false;
                        self.group_move_id = None;
                        self.preview.close();
                        self.preview_source_hash = None;
                        self.preview_editing = false;
                        self.preview_save_pending = false;
                        self.message = if self.language == LanguagePreference::English {
                            format!("Deleted all {count} items; settings and groups were kept")
                        } else {
                            format!("已删除全部 {count} 条历史，设置和分组已保留")
                        };
                        self.is_error = false;
                        window.focus(&self.list_focus, cx);
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "删除全部历史失败",
                                "Failed to delete all history"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::BatchDeleted(result) => {
                self.batch_pending = false;
                self.batch_confirm_open = false;
                match result {
                    Ok(count) => {
                        self.selected_ids.clear();
                        self.message = if self.language == LanguagePreference::English {
                            format!("Deleted {count} selected items")
                        } else {
                            format!("已删除 {count} 条选中记录")
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "批量删除失败",
                                "Failed to delete selected items"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::ItemMoved { result, .. } => {
                self.group_move_pending = false;
                match result {
                    Ok(()) => {
                        self.group_move_id = None;
                        self.message = tr(
                            self.language,
                            "已移动到目标分组",
                            "Moved to the selected group",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "移动分组失败", "Failed to move item")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::Snapshot {
                items,
                total,
                generation,
            } => {
                let applied = self.history.apply(items, total, generation);
                if applied {
                    self.selected_ids
                        .retain(|id| self.history.items.iter().any(|item| item.id == *id));
                    if self.selected_ids.is_empty() {
                        self.batch_confirm_open = false;
                    }
                }
                if applied
                    && !self.group_move_pending
                    && self
                        .group_move_id
                        .is_some_and(|id| !self.history.items.iter().any(|item| item.id == id))
                {
                    self.group_move_id = None;
                }
            }
            Event::Preview {
                id,
                generation,
                result,
            } => {
                if self.preview.apply(id, generation, result) {
                    let text = match &self.preview.result {
                        Some(Ok(PreviewContent::Text(text))) => Some(text.clone()),
                        Some(Ok(PreviewContent::RichText(text))) => Some(text.clone()),
                        _ => None,
                    };
                    if let Some(text) = text {
                        self.preview_input.update(cx, |input, cx| {
                            input.set_value(text, window, cx);
                            input.focus(window, cx);
                        });
                    }
                }
            }
            Event::TextEdited {
                id,
                generation,
                result,
            } => {
                if self.preview.id != Some(id) || self.preview.generation != generation {
                    return;
                }
                self.preview_save_pending = false;
                match result {
                    Ok(changed) => {
                        self.preview.close();
                        self.preview_source_hash = None;
                        self.preview_editing = false;
                        self.preview_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        window.focus(&self.list_focus, cx);
                        self.message = if changed {
                            tr(
                                self.language,
                                "内容已保存为纯文本",
                                "Content saved as plain text",
                            )
                        } else {
                            tr(self.language, "内容未修改", "Content was not changed")
                        }
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "保存编辑失败", "Failed to save changes")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::SavedAs { id, result } => {
                if self.save_as_pending != Some(id) {
                    return;
                }
                self.save_as_pending = None;
                match result {
                    Ok(destination) => {
                        self.message = if self.language == LanguagePreference::English {
                            format!("Saved to {}", destination.display())
                        } else {
                            format!("已另存到 {}", destination.display())
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "另存为失败", "Save as failed")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::Reordered {
                from,
                generation,
                result,
            } => {
                self.reorder_pending = false;
                self.drop_target = None;
                self.history_drag_direction = 0;
                let before = self.reorder_before.take();
                match result {
                    Ok(()) if generation == self.history.generation => {
                        self.feedback_revision += 1;
                        if let Some(before) = before {
                            let after: Vec<_> =
                                self.history.items.iter().map(|item| item.id).collect();
                            self.reorder_offsets = reorder_offsets(&before, &after);
                        }
                        if self.history.items.iter().any(|item| item.id == from) {
                            self.history.selected = Some(from);
                        }
                        self.feedback_task = Some(cx.spawn(async move |view, cx| {
                            cx.background_executor()
                                .timer(visual::MOTION_DURATION)
                                .await;
                            let _ = view.update(cx, |this, cx| {
                                this.reorder_offsets.clear();
                                cx.notify();
                            });
                        }));
                        self.message = tr(self.language, "顺序已保存", "Order saved").into();
                        self.is_error = false;
                    }
                    Ok(()) => {}
                    Err(error) => {
                        self.message = error;
                        self.is_error = true;
                    }
                }
            }
            Event::Status(message) => {
                self.message = localize_service_message(self.language, message);
                self.is_error = false;
            }
            Event::Copied {
                id,
                for_paste,
                clipboard_sequence,
                message,
            } => {
                self.message = localize_service_message(self.language, message);
                self.is_error = false;
                if let Some((pending_id, target)) = self.paste_pending.take() {
                    if pending_id == id && for_paste {
                        self.return_to_paste_target(target, clipboard_sequence, window, cx);
                    } else {
                        self.paste_pending = Some((pending_id, target));
                    }
                }
            }
            Event::Merged {
                for_paste,
                clipboard_sequence,
                item_count,
            } => {
                self.batch_pending = false;
                self.batch_confirm_open = false;
                self.selected_ids.clear();
                let target = self.batch_paste_pending.take();
                if for_paste {
                    if let Some(target) = target {
                        self.return_to_paste_target(target, clipboard_sequence, window, cx);
                    } else {
                        self.message = if self.language == LanguagePreference::English {
                            format!("Merged and copied {item_count} items; paste manually")
                        } else {
                            format!("已合并 {item_count} 条记录并复制，请手动粘贴")
                        };
                        self.is_error = true;
                    }
                } else {
                    self.message = if self.language == LanguagePreference::English {
                        format!("Merged and copied {item_count} items")
                    } else {
                        format!("已合并 {item_count} 条记录并复制")
                    };
                    self.is_error = false;
                }
            }
            Event::ThemeSaved(result) => {
                self.theme_pending = false;
                match result {
                    Ok(theme) => {
                        self.theme = theme;
                        apply_theme(theme, window, cx);
                        self.message = tr(
                            self.language,
                            "外观设置已保存",
                            "Appearance preference saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "外观保存失败", "Failed to save appearance")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::LanguageSaved(result) => {
                self.language_pending = false;
                match result {
                    Ok(language) => {
                        self.language = language;
                        self.search.update(cx, |input, cx| {
                            input.set_placeholder(
                                tr(language, "搜索剪贴板历史…", "Search clipboard history…"),
                                window,
                                cx,
                            );
                        });
                        self.group_name_input.update(cx, |input, cx| {
                            input.set_placeholder(
                                tr(language, "分组名称", "Group name"),
                                window,
                                cx,
                            );
                        });
                        match tray::create(self.tray_sender.clone(), language) {
                            Ok(tray) => {
                                self._tray = Some(tray);
                                self.tray_enabled.set(true);
                                self.message =
                                    tr(language, "语言设置已保存", "Language preference saved")
                                        .into();
                                self.is_error = false;
                            }
                            Err(error) => {
                                self._tray = None;
                                self.tray_enabled.set(false);
                                self.message = format!(
                                    "{}: {error}",
                                    tr(
                                        language,
                                        "语言已保存，但托盘菜单更新失败",
                                        "Language saved, but the tray menu could not be updated"
                                    )
                                );
                                self.is_error = true;
                            }
                        }
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "语言保存失败", "Failed to save language")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::HotkeySaved(result) => {
                self.hotkey_pending = false;
                match result {
                    Ok(choice) => {
                        self.hotkey_choice = choice;
                        self.message = if choice == HotkeyPreference::Disabled {
                            tr(
                                self.language,
                                "全局快捷键已关闭，可从托盘唤出窗口",
                                "The global shortcut is disabled; open the window from the tray.",
                            )
                            .into()
                        } else if self.language == LanguagePreference::English {
                            format!("Shortcut saved: {}", hotkey_label(self.language, choice))
                        } else {
                            format!("已保存唤出快捷键：{}", choice.label())
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.hotkey = None;
                        let restore_error = self.restore_hotkey();
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "快捷键保存失败", "Failed to save shortcut")
                        );
                        if let Some(restore_error) = restore_error {
                            self.message.push_str(&format!("; {restore_error}"));
                        }
                        self.is_error = true;
                    }
                }
            }
            Event::WindowSizeSaved(result) => match result {
                Ok(size) => self.last_window_size = Some(size),
                Err(error) => {
                    self.message = format!(
                        "{}: {error}",
                        tr(
                            self.language,
                            "保存窗口大小失败",
                            "Failed to save window size"
                        )
                    );
                    self.is_error = true;
                }
            },
            Event::AutostartSaved(result) => {
                self.autostart_pending = false;
                match result {
                    Ok(enabled) => {
                        self.autostart = enabled;
                        self.message = if enabled {
                            tr(self.language, "已开启开机启动", "Startup enabled")
                        } else {
                            tr(self.language, "已关闭开机启动", "Startup disabled")
                        }
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "开机启动设置失败",
                                "Failed to update startup"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::DataSize(result) => {
                self.data_size_pending = false;
                match result {
                    Ok(size) => {
                        self.data_size = Some(size);
                        self.message =
                            tr(self.language, "数据占用已更新", "Storage usage updated").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "统计数据占用失败",
                                "Failed to calculate storage usage"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::DatabaseOptimized(size) => {
                self.database_maintenance_pending = false;
                self.data_size = Some(size);
                self.message = tr(
                    self.language,
                    "数据库已整理，数据占用已更新",
                    "Database optimized and storage usage updated",
                )
                .into();
                self.is_error = false;
            }
            Event::Paused(paused) => {
                self.paused = paused;
                self.pause_pending = false;
                self.is_error = false;
                self.message = if paused {
                    tr(
                        self.language,
                        "已暂停记录，已有历史仍可使用",
                        "Recording paused; existing history is still available",
                    )
                } else {
                    tr(self.language, "已恢复记录", "Recording resumed")
                }
                .into();
            }
            Event::BackupExported(result) => {
                self.export_pending = false;
                match result {
                    Ok(report) => {
                        let included =
                            report.included_images + report.included_icons + report.included_staged;
                        let missing =
                            report.missing_images + report.missing_icons + report.missing_staged;
                        self.message = if self.language == LanguagePreference::English {
                            format!(
                                "Backed up {} items and {} attachments to {}{}",
                                report.total_items,
                                included,
                                report.destination.display(),
                                if missing == 0 {
                                    String::new()
                                } else {
                                    format!("; {missing} source attachments were not included")
                                }
                            )
                        } else {
                            format!(
                                "已备份 {} 条记录、{} 个附件到 {}{}",
                                report.total_items,
                                included,
                                report.destination.display(),
                                if missing == 0 {
                                    String::new()
                                } else {
                                    format!("；{missing} 个源附件未包含在备份中")
                                }
                            )
                        };
                        self.is_error = missing != 0;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "导出备份失败", "Failed to export backup")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::CommandFailed { kind, message } => {
                if let FailureKind::Query(generation) = kind
                    && !self.history.fail_query(generation)
                {
                    return;
                }
                if let FailureKind::EditText { id, generation } = kind
                    && (self.preview.id != Some(id) || self.preview.generation != generation)
                {
                    return;
                }
                match kind {
                    FailureKind::Query(_) => {}
                    FailureKind::Paste(id)
                        if self
                            .paste_pending
                            .is_some_and(|(pending_id, _)| pending_id == id) =>
                    {
                        self.paste_pending = None;
                    }
                    FailureKind::Merge => {
                        self.batch_pending = false;
                        self.batch_paste_pending = None;
                    }
                    FailureKind::GroupSave => self.group_save_pending = false,
                    FailureKind::GroupDelete => {
                        self.group_delete_pending = false;
                        self.group_delete_id = None;
                    }
                    FailureKind::GroupMove => self.group_move_pending = false,
                    FailureKind::ClearHistory => self.clear_pending = false,
                    FailureKind::ClearAllHistory => self.clear_all_pending = false,
                    FailureKind::BatchDelete => {
                        self.batch_pending = false;
                        self.batch_confirm_open = false;
                    }
                    FailureKind::EditText { .. } => self.preview_save_pending = false,
                    FailureKind::SaveAs(id) if self.save_as_pending == Some(id) => {
                        self.save_as_pending = None;
                    }
                    FailureKind::DataSize => self.data_size_pending = false,
                    FailureKind::DatabaseMaintenance => {
                        self.database_maintenance_pending = false;
                    }
                    FailureKind::Pause => self.pause_pending = false,
                    FailureKind::Other | FailureKind::Paste(_) | FailureKind::SaveAs(_) => {}
                }
                self.message = message;
                self.is_error = true;
            }
            Event::Error(message) => {
                self.message = message;
                self.is_error = true;
                self.history.loading = false;
            }
            Event::BackgroundError(message) => {
                self.message = message;
                self.is_error = true;
            }
        }
        cx.notify();
    }

    fn open_preview(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.history.selected = Some(id);
        self.preview_source_hash = self
            .history
            .items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.content_hash.clone());
        self.preview_editing = false;
        self.preview_save_pending = false;
        let generation = self.preview.open(id);
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if !self.send(Command::Preview { id, generation }, cx) {
            self.preview
                .apply(id, generation, Err(self.message.clone()));
        }
        cx.notify();
    }

    fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_save_pending {
            return;
        }
        if self.preview_editing {
            self.preview_editing = false;
            let original = match &self.preview.result {
                Some(Ok(PreviewContent::Text(text) | PreviewContent::RichText(text))) => {
                    Some(text.clone())
                }
                _ => None,
            };
            if let Some(original) = original {
                self.preview_input
                    .update(cx, |input, cx| input.set_value(original, window, cx));
            }
            window.focus(&self.list_focus, cx);
            cx.notify();
            return;
        }
        self.preview.close();
        self.preview_source_hash = None;
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn begin_preview_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_source_hash.is_some()
            && matches!(
                self.preview.result,
                Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
            )
        {
            self.preview_editing = true;
            self.preview_input
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    fn save_preview_edit(&mut self, cx: &mut Context<Self>) {
        if !self.preview_editing || self.preview_save_pending {
            return;
        }
        let (Some(id), Some(expected_hash)) = (self.preview.id, self.preview_source_hash.clone())
        else {
            return;
        };
        let new_text = self.preview_input.read(cx).value().to_string();
        if self.send(
            Command::EditText {
                id,
                expected_hash,
                new_text,
                generation: self.preview.generation,
            },
            cx,
        ) {
            self.preview_save_pending = true;
            cx.notify();
        }
    }

    fn render_file_preview(
        &self,
        entries: &[FilePreviewEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("file-details-scroll")
            .size_full()
            .p_1()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scrollbar()
            .children(entries.iter().enumerate().map(|(index, entry)| {
                let name = std::path::Path::new(&entry.original_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| entry.original_path.clone());
                let unavailable = !entry.exists || entry.metadata_error.is_some();
                let status = if entry.metadata_error.is_some() {
                    tr(self.language, "无法读取", "Unreadable").to_owned()
                } else if !entry.exists {
                    tr(self.language, "已失效", "Missing").to_owned()
                } else if entry.is_dir {
                    tr(self.language, "文件夹", "Folder").to_owned()
                } else if let Some(size) = entry.size {
                    format_bytes(size)
                } else {
                    tr(self.language, "文件", "File").to_owned()
                };
                div()
                    .id(("file-detail", index))
                    .flex_none()
                    .rounded_md()
                    .border_1()
                    .border_color(if unavailable {
                        cx.theme().danger
                    } else {
                        cx.theme().border
                    })
                    .bg(cx.theme().background)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_semibold()
                                    .line_clamp(1)
                                    .text_ellipsis()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(if unavailable {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().border
                                    })
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .text_color(if unavailable {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_clamp(2)
                            .text_ellipsis()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.language == LanguagePreference::English {
                                format!("Original path: {}", entry.original_path)
                            } else {
                                format!("原始路径：{}", entry.original_path)
                            }),
                    )
                    .when(entry.recovered, |row| {
                        row.child(
                            div()
                                .text_xs()
                                .line_clamp(2)
                                .text_ellipsis()
                                .text_color(cx.theme().primary)
                                .child(if self.language == LanguagePreference::English {
                                    format!("Backup copy: {}", entry.resolved_path)
                                } else {
                                    format!("备份副本：{}", entry.resolved_path)
                                }),
                        )
                    })
                    .when_some(entry.metadata_error.clone(), |row, error| {
                        row.child(div().text_xs().text_color(cx.theme().danger).child(
                            if self.language == LanguagePreference::English {
                                format!("Unable to read metadata: {error}")
                            } else {
                                format!("无法读取元数据：{error}")
                            },
                        ))
                    })
            }))
            .into_any_element()
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> Div {
        let id = self.preview.id.expect("preview is open");
        let ready = matches!(self.preview.result, Some(Ok(_)));
        let image = matches!(self.preview.result, Some(Ok(PreviewContent::Image(_))));
        let files = matches!(self.preview.result, Some(Ok(PreviewContent::Files(_))));
        let rich = matches!(self.preview.result, Some(Ok(PreviewContent::RichText(_))));
        let image_unavailable = tr(self.language, "图片无法显示", "Image unavailable");
        let save_as_name = match &self.preview.result {
            Some(Ok(PreviewContent::Image(path))) => path.file_name(),
            Some(Ok(PreviewContent::Files(entries))) => entries
                .first()
                .and_then(|entry| std::path::Path::new(&entry.original_path).file_name()),
            _ => None,
        }
        .map(|name| name.to_string_lossy().into_owned());
        let save_as_label = match &self.preview.result {
            Some(Ok(PreviewContent::Files(paths))) if paths.len() > 1 => {
                tr(self.language, "另存第一项", "Save first item as")
            }
            _ => tr(self.language, "另存为", "Save as"),
        };
        let editable = matches!(
            self.preview.result,
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
        ) && self.preview_source_hash.is_some();
        let message = if self.preview_editing && rich {
            tr(
                self.language,
                "保存后将变为纯文本，原富文本格式不会保留",
                "Saving converts this to plain text and removes rich-text formatting.",
            )
            .into()
        } else if self.preview_editing {
            tr(
                self.language,
                "编辑文本；保存后搜索结果会更新",
                "Edit the text; search results update after saving.",
            )
            .into()
        } else {
            match &self.preview.result {
                None => tr(self.language, "正在加载完整内容…", "Loading full content…").to_owned(),
                Some(Err(error)) => error.clone(),
                Some(Ok(PreviewContent::Text(text)))
                    if self.language == LanguagePreference::English =>
                {
                    format!(
                        "{} characters · {} bytes · read-only",
                        text.chars().count(),
                        text.len()
                    )
                }
                Some(Ok(PreviewContent::Text(text))) => {
                    format!("{} 字符 · {} 字节 · 只读", text.chars().count(), text.len())
                }
                Some(Ok(PreviewContent::Image(_))) => tr(
                    self.language,
                    "图片预览 · 保持原始比例",
                    "Image preview · Original aspect ratio",
                )
                .into(),
                Some(Ok(PreviewContent::RichText(_))) => tr(
                    self.language,
                    "富文本 · 纯文本预览",
                    "Rich text · Plain-text preview",
                )
                .into(),
                Some(Ok(PreviewContent::Files(entries))) => {
                    let directories = entries
                        .iter()
                        .filter(|entry| entry.exists && entry.is_dir)
                        .count();
                    let missing = entries
                        .iter()
                        .filter(|entry| !entry.exists && entry.metadata_error.is_none())
                        .count();
                    let unreadable = entries
                        .iter()
                        .filter(|entry| entry.metadata_error.is_some())
                        .count();
                    let recovered = entries.iter().filter(|entry| entry.recovered).count();
                    let mut details = vec![if self.language == LanguagePreference::English {
                        format!("{} items", entries.len())
                    } else {
                        format!("{} 项", entries.len())
                    }];
                    if directories > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{directories} folders")
                        } else {
                            format!("{directories} 个文件夹")
                        });
                    }
                    if missing > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{missing} missing")
                        } else {
                            format!("{missing} 项已失效")
                        });
                    }
                    if unreadable > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{unreadable} unreadable")
                        } else {
                            format!("{unreadable} 项无法读取")
                        });
                    }
                    if recovered > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{recovered} from backup copies")
                        } else {
                            format!("{recovered} 项使用备份副本")
                        });
                    }
                    details.join(" · ")
                }
            }
        };
        let body: AnyElement = match &self.preview.result {
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_))) => {
                Textarea::new(&self.preview_input)
                    .readonly(!self.preview_editing || self.preview_save_pending)
                    .h_full()
                    .aria_label(if self.preview_editing {
                        tr(self.language, "编辑文本", "Edit text")
                    } else {
                        tr(self.language, "完整文本内容", "Full text content")
                    })
                    .into_any_element()
            }
            Some(Ok(PreviewContent::Files(entries))) => self.render_file_preview(entries, cx),
            Some(Ok(PreviewContent::Image(path))) => div()
                .flex()
                .items_center()
                .justify_center()
                .size_full()
                .overflow_hidden()
                .child(
                    img(path.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(move || div().child(image_unavailable).into_any_element()),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        };
        div()
            .key_context("Preview")
            .track_focus(&self.list_focus)
            .flex()
            .flex_col()
            .size_full()
            .p(px(PAGE_PADDING))
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family("Microsoft YaHei UI")
            .on_action(
                cx.listener(|this, _: &ClosePreview, window, cx| this.close_preview(window, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(if self.preview_editing {
                                tr(self.language, "编辑内容", "Edit content")
                            } else if image {
                                tr(self.language, "图片预览", "Image preview")
                            } else if files {
                                tr(self.language, "文件详情", "File details")
                            } else if rich {
                                tr(self.language, "富文本预览", "Rich-text preview")
                            } else {
                                tr(self.language, "完整内容", "Full content")
                            }),
                    )
                    .child(
                        Button::new("preview-close")
                            .ghost()
                            .label(if self.preview_editing {
                                tr(self.language, "取消编辑 (Esc)", "Cancel editing (Esc)")
                            } else {
                                tr(self.language, "返回列表 (Esc)", "Back to list (Esc)")
                            })
                            .disabled(self.preview_save_pending)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_preview(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            )
            .child(div().flex_1().min_h_0().child(body))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_2()
                    .when(!self.preview_editing && editable, |bar| {
                        bar.child(
                            Button::new("preview-edit")
                                .outline()
                                .label(tr(self.language, "编辑", "Edit"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_preview_edit(window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing && rich, |bar| {
                        bar.child(
                            Button::new("preview-copy-plain")
                                .outline()
                                .label(tr(self.language, "复制纯文本", "Copy plain text"))
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::CopyPlainText(id), cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing && (image || files), |bar| {
                        bar.when_some(save_as_name, |bar, suggested_name| {
                            bar.child(
                                Button::new("preview-save-as")
                                    .outline()
                                    .label(save_as_label)
                                    .disabled(!ready || self.save_as_pending.is_some())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.start_save_as(id, suggested_name.clone(), cx);
                                    })),
                            )
                        })
                        .child(
                            Button::new("preview-reveal")
                                .outline()
                                .label(tr(
                                    self.language,
                                    "在资源管理器中显示",
                                    "Show in File Explorer",
                                ))
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::RevealInExplorer(id), cx);
                                })),
                        )
                        .child(
                            Button::new("preview-copy-path")
                                .outline()
                                .label(if self.paste_target.is_some() && self._tray.is_some() {
                                    tr(self.language, "粘贴路径", "Paste paths")
                                } else {
                                    tr(self.language, "复制路径", "Copy paths")
                                })
                                .disabled(!ready || self.paste_pending.is_some())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.copy_or_paste_path(id, window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing, |bar| {
                        bar.child(
                            Button::new("preview-paste")
                                .outline()
                                .label(tr(
                                    self.language,
                                    "粘贴到原窗口",
                                    "Paste to previous window",
                                ))
                                .disabled(
                                    !ready
                                        || self.paste_target.is_none()
                                        || self.paste_pending.is_some()
                                        || self._tray.is_none(),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.paste_selected(id, window, cx);
                                })),
                        )
                        .child(
                            Button::new("preview-copy")
                                .primary()
                                .label(if image {
                                    tr(self.language, "复制图片", "Copy image")
                                } else if files {
                                    tr(self.language, "复制文件", "Copy files")
                                } else if rich {
                                    tr(self.language, "复制富文本", "Copy rich text")
                                } else {
                                    tr(self.language, "复制全文", "Copy all text")
                                })
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::Copy(id), cx);
                                })),
                        )
                    })
                    .when(self.preview_editing, |bar| {
                        bar.child(
                            Button::new("preview-cancel-edit")
                                .ghost()
                                .label(tr(self.language, "取消", "Cancel"))
                                .disabled(self.preview_save_pending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_preview(window, cx);
                                })),
                        )
                        .child(
                            Button::new("preview-save-edit")
                                .primary()
                                .label(tr(self.language, "保存文本", "Save text"))
                                .disabled(self.preview_save_pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.save_preview_edit(cx);
                                })),
                        )
                    }),
            )
    }

    fn select(&mut self, direction: isize, cx: &mut Context<Self>) {
        if let Some(index) = self.history.select_relative(direction) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    fn select_index(&mut self, index: usize, strategy: ScrollStrategy, cx: &mut Context<Self>) {
        if let Some(index) = self.history.select_index(index) {
            self.scroll.scroll_to_item(index, strategy);
        }
        cx.notify();
    }

    fn page_step(&self) -> isize {
        self.scroll
            .0
            .borrow()
            .last_item_size
            .map(|size| {
                ((f32::from(size.item.height) / ROW_HEIGHT).floor() as usize)
                    .saturating_sub(1)
                    .max(1) as isize
            })
            .unwrap_or(1)
    }

    fn paste_selected(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        let Some(target) = self.paste_target else {
            self.message = tr(
                self.language,
                "请从目标应用按全局快捷键唤出，再使用粘贴",
                "Open the clipboard with the global shortcut from the target app before pasting.",
            )
            .into();
            self.is_error = true;
            cx.notify();
            return;
        };
        if self._tray.is_none() || !paste::is_external_target(window, target) {
            self.paste_target = None;
            self.message = tr(
                self.language,
                "原窗口不可用，仍可使用复制后手动粘贴",
                "The previous window is unavailable; copy and paste manually instead.",
            )
            .into();
            self.is_error = true;
            cx.notify();
            return;
        }
        if self.paste_pending.is_none() && self.send(Command::CopyForPaste(id), cx) {
            self.paste_pending = Some((id, target));
            self.message = tr(
                self.language,
                "正在复制并返回原窗口…",
                "Copying and returning to the previous window…",
            )
            .into();
            self.is_error = false;
            cx.notify();
        }
    }

    fn copy_or_paste_path(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        if self.paste_pending.is_some() {
            return;
        }
        let target = self
            .paste_target
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target));
        if self.paste_target.is_some() && target.is_none() {
            self.paste_target = None;
        }
        let command = if target.is_some() {
            Command::CopyPathForPaste(id)
        } else {
            Command::CopyPath(id)
        };
        if self.send(command, cx)
            && let Some(target) = target
        {
            self.paste_pending = Some((id, target));
            self.message = tr(
                self.language,
                "正在复制路径并返回原窗口…",
                "Copying paths and returning to the previous window…",
            )
            .into();
            self.is_error = false;
            cx.notify();
        }
    }

    fn return_to_paste_target(
        &mut self,
        target: (isize, u32),
        clipboard_sequence: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !paste::is_external_target(window, target) {
            self.message = tr(
                self.language,
                "目标窗口已变化，内容已复制，请手动粘贴",
                "The target window changed. The content was copied; paste it manually.",
            )
            .into();
            self.is_error = true;
            return;
        }
        let hide_window = !self.window_pinned;
        if hide_window {
            tray::set_window_visible(window, false);
        }
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                match paste::send_to_target(target, clipboard_sequence) {
                    Ok(()) => {
                        this.message = tr(
                            this.language,
                            "已发送粘贴快捷键，请检查目标应用",
                            "Paste shortcut sent; check the target app.",
                        )
                        .into();
                        this.is_error = false;
                    }
                    Err(error) => {
                        if hide_window {
                            tray::set_window_visible(window, true);
                        }
                        this.message = error.to_string();
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_window_pin(&mut self, window: &Window, cx: &mut Context<Self>) {
        let pinned = !self.window_pinned;
        match tray::set_window_topmost(window, pinned) {
            Ok(()) => {
                self.window_pinned = pinned;
                self.message = if pinned {
                    tr(
                        self.language,
                        "窗口已置顶；粘贴后保持可见",
                        "Window pinned; it will stay visible after pasting.",
                    )
                    .into()
                } else {
                    tr(self.language, "已取消窗口置顶", "Window unpinned").into()
                };
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "更新窗口置顶失败",
                        "Failed to update window pinning"
                    )
                );
                self.is_error = true;
            }
        }
        cx.notify();
    }

    fn valid_drag(&self, drag: &HistoryDrag) -> bool {
        !self.reorder_pending
            && !self.history.loading
            && drag.generation == self.history.generation
            && drag.favorite_only == self.history.favorite_only
            && drag.group_id == self.history.group_id
            && self
                .history
                .items
                .iter()
                .any(|item| item.id == drag.id && item.is_pinned == drag.pinned)
    }

    fn valid_group_drag(&self, drag: &GroupDrag) -> bool {
        !self.group_reorder_pending
            && !self.group_delete_pending
            && !self.group_save_pending
            && !self.clear_pending
            && drag.group_ids == self.groups.iter().map(|group| group.id).collect::<Vec<_>>()
    }

    fn start_group_drag_scroll(&mut self, cx: &mut Context<Self>) {
        self.group_drag_direction = 0;
        self.group_drag_scroll_task = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(visual::DRAG_SCROLL_INTERVAL)
                    .await;
                let Ok(active) = view.update(cx, |this, cx| {
                    if !cx.has_active_drag() {
                        this.group_drag_direction = 0;
                        this.group_drop_target = None;
                        cx.notify();
                        return false;
                    }
                    if this.group_drag_direction != 0 {
                        let offset = this.group_scroll.offset();
                        let next = (offset.x
                            + px(f32::from(this.group_drag_direction)
                                * visual::GROUP_DRAG_SCROLL_STEP))
                        .clamp(-this.group_scroll.max_offset().x, px(0.));
                        if next != offset.x {
                            this.group_scroll.set_offset(point(next, offset.y));
                            cx.notify();
                        }
                    }
                    true
                }) else {
                    break;
                };
                if !active {
                    break;
                }
            }
        }));
    }

    fn start_history_drag_scroll(&mut self, cx: &mut Context<Self>) {
        self.history_drag_direction = 0;
        self.history_drag_scroll_task = Some(cx.spawn(async move |view, cx| {
            let mut ticks = 0usize;
            let mut scroll_target = None;
            loop {
                cx.background_executor()
                    .timer(visual::DRAG_SCROLL_INTERVAL)
                    .await;
                let Ok(active) = view.update(cx, |this, cx| {
                    if !cx.has_active_drag() {
                        this.history_drag_direction = 0;
                        this.drop_target = None;
                        cx.notify();
                        return false;
                    }
                    if this.history_drag_direction == 0 {
                        ticks = 0;
                        scroll_target = None;
                        return true;
                    }
                    ticks += 1;
                    if !ticks.is_multiple_of(visual::HISTORY_DRAG_SCROLL_TICKS) {
                        return true;
                    }
                    let current = scroll_target.unwrap_or_else(|| {
                        this.scroll.0.borrow().base_handle.logical_scroll_top().0
                    });
                    let next = next_drag_scroll_index(
                        current,
                        this.history.items.len(),
                        this.history_drag_direction,
                    );
                    if next != current {
                        scroll_target = Some(next);
                        this.scroll.scroll_to_item_strict(next, ScrollStrategy::Top);
                    }
                    let target = drag_edge_target_index(
                        next,
                        this.history.items.len(),
                        this.page_step().max(1) as usize,
                        this.history_drag_direction,
                    )
                    .and_then(|index| this.history.items.get(index))
                    .and_then(|target| {
                        let source = this
                            .history
                            .selected
                            .and_then(|id| this.history.items.iter().find(|item| item.id == id))?;
                        (source.id != target.id).then_some(DropTarget {
                            id: target.id,
                            after: this.history_drag_direction > 0,
                            allowed: source.is_pinned == target.is_pinned,
                        })
                    });
                    if next != current || this.drop_target != target {
                        this.drop_target = target;
                        cx.notify();
                    }
                    true
                }) else {
                    break;
                };
                if !active {
                    break;
                }
            }
        }));
    }

    fn render_group(&self, group: &Group, cx: &mut Context<Self>) -> AnyElement {
        let id = group.id;
        let drag = GroupDrag {
            id,
            name: group.name.clone(),
            group_ids: self.groups.iter().map(|group| group.id).collect(),
        };
        let entity = cx.entity();
        let active_drop = cx
            .has_active_drag()
            .then_some(self.group_drop_target)
            .flatten();
        let target = active_drop.filter(|target| target.id == id);
        let group_id = Some(id);
        let pill = div()
            .id(("group-drag", id as usize))
            .relative()
            .flex()
            .items_center()
            .flex_none()
            .h(px(CONTROL_HEIGHT))
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<GroupDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        if this.group_drop_target.is_some_and(|target| target.id == id) {
                            this.group_drop_target = None;
                            cx.notify();
                        }
                        return;
                    }
                    let target = (event.drag(cx).id != id).then_some(DropTarget {
                        id,
                        after: event.event.position.x > event.bounds.center().x,
                        allowed: this.valid_group_drag(event.drag(cx)),
                    });
                    if this.group_drop_target != target {
                        this.group_drop_target = target;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, drag: &GroupDrag, _, cx| {
                if drag.id == id {
                    this.group_drag_direction = 0;
                    return;
                }
                if !this.valid_group_drag(drag) {
                    this.group_drop_target = None;
                    this.group_drag_direction = 0;
                    this.message = tr(
                        this.language,
                        "分组列表已变化，请重新拖动",
                        "The group list changed; drag again.",
                    )
                    .into();
                    this.is_error = true;
                    cx.notify();
                    return;
                }
                let Some(target) = this.group_drop_target.filter(|target| target.id == id) else {
                    return;
                };
                if this.send(
                    Command::ReorderGroup {
                        from: drag.id,
                        to: id,
                        after: target.after,
                    },
                    cx,
                ) {
                    this.group_reorder_pending = true;
                    this.group_reorder_before =
                        Some(this.groups.iter().map(|group| group.id).collect());
                }
                this.group_drop_target = None;
                this.group_drag_direction = 0;
                cx.notify();
            }))
            .child(
                div()
                    .id(("group-handle", id as usize))
                    .w(px(22.))
                    .h(px(CONTROL_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_grab()
                    .child("⠿")
                    .when(self.valid_group_drag(&drag), |handle| {
                        handle.on_drag(drag.clone(), move |drag, _, _, cx| {
                            entity.update(cx, |this, cx| {
                                this.group_drop_target = None;
                                this.start_group_drag_scroll(cx);
                                cx.notify();
                            });
                            cx.new(|_| drag.clone())
                        })
                    }),
            )
            .child(
                Button::new(("group", id as usize))
                    .small()
                    .outline()
                    .h(px(CONTROL_HEIGHT))
                    .label(group.name.clone())
                    .selected(self.history.group_id == group_id)
                    .disabled(
                        self.group_delete_pending
                            || self.clear_pending
                            || self.group_reorder_pending,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_group(group_id, window, cx);
                    })),
            )
            .when_some(target, |pill, target| {
                pill.child(visual::reveal(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(visual::DROP_MARKER_HEIGHT))
                        .bg(if target.allowed {
                            cx.theme().primary
                        } else {
                            cx.theme().danger
                        })
                        .when(target.after, |line| line.right_0())
                        .when(!target.after, |line| line.left_0()),
                    (
                        "group-drop-marker",
                        id as usize * 2 + usize::from(target.after),
                    ),
                    cx,
                ))
            });
        if self.group_feedback_ids.contains(&id) {
            visual::reveal(
                div().child(pill),
                format!(
                    "group-reorder-feedback-{}-{id}",
                    self.group_feedback_revision
                ),
                cx,
            )
        } else {
            pill.into_any_element()
        }
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.history.items[index];
        let id = item.id;
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let kind = match item.content_type.as_str() {
            "image" => tr(self.language, "图片", "Image"),
            "files" => tr(self.language, "文件", "Files"),
            "html" => "HTML",
            "rtf" => "RTF",
            "url" => tr(self.language, "网址", "URL"),
            _ => tr(self.language, "文本", "Text"),
        };
        let detail = if is_image {
            match (item.image_width, item.image_height) {
                (Some(width), Some(height)) => format!("{width} × {height}"),
                _ => tr(self.language, "尺寸未知", "Unknown size").into(),
            }
        } else if is_files {
            let count = item
                .file_paths
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .map_or(0, |paths| paths.len());
            if count == 0 {
                tr(self.language, "路径不可用", "Paths unavailable").into()
            } else if self.language == LanguagePreference::English {
                format!("{count} items")
            } else {
                format!("{count} 项")
            }
        } else if matches!(item.content_type.as_str(), "html" | "rtf") && item.char_count.is_none()
        {
            tr(self.language, "纯文本未知", "Plain text unavailable").into()
        } else if self.language == LanguagePreference::English {
            format!("{} characters", item.char_count.unwrap_or(0))
        } else {
            format!("{} 字符", item.char_count.unwrap_or(0))
        };
        let selected = self.history.selected == Some(id);
        let marked = self.selected_ids.contains(&id);
        let pinned = item.is_pinned;
        let favorite = item.is_favorite;
        let drag = HistoryDrag {
            id,
            pinned,
            favorite_only: self.history.favorite_only,
            group_id: self.history.group_id,
            generation: self.history.generation,
            kind,
            preview: item.preview.clone().unwrap_or_else(|| kind.to_owned()),
            language: self.language,
        };
        let entity = cx.entity();
        let active_drop = cx.has_active_drag().then_some(self.drop_target).flatten();
        let live_offset = active_drop
            .filter(|target| target.allowed)
            .and_then(|target| {
                let source = self.history.selected?;
                let ids: Vec<_> = self.history.items.iter().map(|item| item.id).collect();
                drag_reorder_offsets(&ids, source, target.id, target.after)
                    .get(&id)
                    .copied()
            });
        let live_drag_source = cx.has_active_drag() && self.history.selected == Some(id);
        let color = if selected || marked {
            cx.theme().accent
        } else {
            cx.theme().background
        };
        let row = div()
            .id(("history-row", id as usize))
            .when(!self.reorder_pending && !self.history.loading, |row| {
                row.on_drag(drag.clone(), move |drag, _, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.history.selected = Some(drag.id);
                        this.drop_target = None;
                        this.start_history_drag_scroll(cx);
                        cx.notify();
                    });
                    cx.new(|_| drag.clone())
                })
            })
            .h(px(ROW_HEIGHT))
            .px(px(PAGE_PADDING))
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        if this.drop_target.is_some_and(|target| target.id == id) {
                            this.drop_target = None;
                            cx.notify();
                        }
                        return;
                    }
                    let after = event.event.position.y > event.bounds.center().y;
                    let target = (event.drag(cx).id != id).then_some(DropTarget {
                        id,
                        after,
                        allowed: this.valid_drag(event.drag(cx)),
                    });
                    if this.drop_target != target {
                        this.drop_target = target;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, drag: &HistoryDrag, _, cx| {
                if drag.id == id {
                    this.drop_target = None;
                    this.history_drag_direction = 0;
                    cx.notify();
                    return;
                }
                if !this.valid_drag(drag) {
                    this.drop_target = None;
                    this.history_drag_direction = 0;
                    this.message = tr(
                        this.language,
                        "列表已变化，请重新拖动",
                        "The list changed; drag again.",
                    )
                    .into();
                    this.is_error = true;
                    cx.notify();
                    return;
                }
                let Some(target) = this.drop_target.filter(|target| target.id == id) else {
                    this.drop_target = None;
                    this.history_drag_direction = 0;
                    cx.notify();
                    return;
                };
                if this.send(
                    Command::Reorder {
                        from: drag.id,
                        to: id,
                        after: target.after,
                        favorite_only: drag.favorite_only,
                        group_id: drag.group_id,
                        generation: drag.generation,
                    },
                    cx,
                ) {
                    this.reorder_pending = true;
                    this.reorder_before =
                        Some(this.history.items.iter().map(|item| item.id).collect());
                }
                this.drop_target = None;
                this.history_drag_direction = 0;
                cx.notify();
            }))
            .py_1()
            .cursor_grab()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.history.selected = Some(id);
                window.focus(&this.list_focus, cx);
                cx.notify();
            }))
            .child(
                div()
                    .h_full()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(match active_drop.filter(|target| target.id == id) {
                        Some(target) if target.allowed => cx.theme().primary,
                        Some(_) => cx.theme().danger,
                        None if selected || marked => cx.theme().primary,
                        None => cx.theme().border,
                    })
                    .bg(color)
                    .relative()
                    .when(active_drop.is_some_and(|target| target.id == id), |card| {
                        card.child(visual::reveal(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .h(px(visual::DROP_MARKER_HEIGHT))
                                .bg(if active_drop.is_some_and(|target| target.allowed) {
                                    cx.theme().primary
                                } else {
                                    cx.theme().danger
                                })
                                .when(active_drop.is_some_and(|target| target.after), |line| {
                                    line.bottom_0()
                                })
                                .when(active_drop.is_some_and(|target| !target.after), |line| {
                                    line.top_0()
                                }),
                            (
                                "drop-marker",
                                id as usize * 2
                                    + usize::from(active_drop.is_some_and(|target| target.after)),
                            ),
                            cx,
                        ))
                    })
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::new(IconName::EllipsisVertical)
                                            .xsmall()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(format!(
                                        "{}{} · {}",
                                        if pinned {
                                            tr(self.language, "置顶 · ", "Pinned · ")
                                        } else {
                                            ""
                                        },
                                        kind,
                                        detail
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation()
                                            })
                                            .child(
                                                Button::new(("select", id as usize))
                                                    .ghost()
                                                    .xsmall()
                                                    .h(px(24.))
                                                    .icon(IconName::Check)
                                                    .tooltip(if marked {
                                                        tr(self.language, "取消选择", "Deselect")
                                                    } else {
                                                        tr(self.language, "选择", "Select")
                                                    })
                                                    .accessibility_label(if marked {
                                                        tr(self.language, "取消选择", "Deselect")
                                                    } else {
                                                        tr(self.language, "选择", "Select")
                                                    })
                                                    .selected(marked)
                                                    .disabled(
                                                        self.batch_pending || self.history.loading,
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.toggle_selection(id, cx);
                                                        },
                                                    )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(item.created_at.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_sm()
                            .bg(cx.theme().muted)
                            .px_2()
                            .py_1()
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(is_image, |body| {
                                body.child(
                                    div()
                                        .w(px(visual::THUMBNAIL_WIDTH))
                                        .h(px(visual::THUMBNAIL_HEIGHT))
                                        .flex_none()
                                        .rounded_sm()
                                        .overflow_hidden()
                                        .when_some(item.image_path.as_ref(), |box_, path| {
                                            box_.child(
                                                img(std::path::PathBuf::from(path))
                                                    .size_full()
                                                    .object_fit(ObjectFit::Contain)
                                                    .with_fallback(move || {
                                                        div()
                                                            .child(image_unavailable)
                                                            .into_any_element()
                                                    }),
                                            )
                                        }),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .line_height(px(20.))
                                    .line_clamp(2)
                                    .text_ellipsis()
                                    .child(item.preview.clone().unwrap_or_else(|| kind.into())),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_1()
                            .justify_end()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Button::new(("preview", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(IconName::Eye)
                                    .tooltip(tr(self.language, "查看", "View"))
                                    .accessibility_label(tr(self.language, "查看", "View"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_preview(id, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("favorite", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(if favorite {
                                        IconName::HeartOff
                                    } else {
                                        IconName::Heart
                                    })
                                    .tooltip(if favorite {
                                        tr(self.language, "取消收藏", "Unfavorite")
                                    } else {
                                        tr(self.language, "收藏", "Favorite")
                                    })
                                    .accessibility_label(if favorite {
                                        tr(self.language, "取消收藏", "Unfavorite")
                                    } else {
                                        tr(self.language, "收藏", "Favorite")
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::ToggleFavorite(id), cx);
                                    })),
                            )
                            .child(
                                Button::new(("pin", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(if pinned {
                                        IconName::StarOff
                                    } else {
                                        IconName::Star
                                    })
                                    .tooltip(if pinned {
                                        tr(self.language, "取消置顶", "Unpin")
                                    } else {
                                        tr(self.language, "置顶", "Pin")
                                    })
                                    .accessibility_label(if pinned {
                                        tr(self.language, "取消置顶", "Unpin")
                                    } else {
                                        tr(self.language, "置顶", "Pin")
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::TogglePin(id), cx);
                                    })),
                            )
                            .child(
                                Button::new(("move-group", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(IconName::Folder)
                                    .tooltip(tr(self.language, "移动到分组", "Move to group"))
                                    .accessibility_label(tr(
                                        self.language,
                                        "移动到分组",
                                        "Move to group",
                                    ))
                                    .disabled(
                                        self.group_move_pending
                                            || (self.groups.is_empty()
                                                && self.history.group_id.is_none()),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.group_move_id = Some(id);
                                        this.history.selected = Some(id);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("delete", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(IconName::Delete)
                                    .tooltip(tr(self.language, "删除", "Delete"))
                                    .accessibility_label(tr(self.language, "删除", "Delete"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::Delete(id), cx);
                                    })),
                            )
                            .when(is_files, |bar| {
                                bar.child(
                                    Button::new(("copy-path", id as usize))
                                        .ghost()
                                        .xsmall()
                                        .h(px(24.))
                                        .icon(IconName::Copy)
                                        .tooltip(
                                            if self.paste_target.is_some() && self._tray.is_some() {
                                                tr(self.language, "粘贴路径", "Paste paths")
                                            } else {
                                                tr(self.language, "复制路径", "Copy paths")
                                            },
                                        )
                                        .accessibility_label(
                                            if self.paste_target.is_some() && self._tray.is_some() {
                                                tr(self.language, "粘贴路径", "Paste paths")
                                            } else {
                                                tr(self.language, "复制路径", "Copy paths")
                                            },
                                        )
                                        .disabled(self.paste_pending.is_some())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.copy_or_paste_path(id, window, cx);
                                        })),
                                )
                            })
                            .child(
                                Button::new(("paste", id as usize))
                                    .outline()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(IconName::Replace)
                                    .tooltip(tr(self.language, "粘贴", "Paste"))
                                    .accessibility_label(tr(self.language, "粘贴", "Paste"))
                                    .disabled(
                                        self.paste_target.is_none()
                                            || self.paste_pending.is_some()
                                            || self._tray.is_none(),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.paste_selected(id, window, cx);
                                    })),
                            )
                            .when(
                                matches!(item.content_type.as_str(), "html" | "rtf"),
                                |bar| {
                                    bar.child(
                                        Button::new(("copy-plain", id as usize))
                                            .outline()
                                            .xsmall()
                                            .h(px(24.))
                                            .icon(IconName::FileText)
                                            .tooltip(tr(
                                                self.language,
                                                "复制纯文本",
                                                "Copy plain text",
                                            ))
                                            .accessibility_label(tr(
                                                self.language,
                                                "复制纯文本",
                                                "Copy plain text",
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.send(Command::CopyPlainText(id), cx);
                                            })),
                                    )
                                },
                            )
                            .child(
                                Button::new(("copy", id as usize))
                                    .outline()
                                    .xsmall()
                                    .h(px(24.))
                                    .icon(IconName::Copy)
                                    .tooltip(tr(self.language, "复制", "Copy"))
                                    .accessibility_label(tr(self.language, "复制", "Copy"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::Copy(id), cx);
                                    })),
                            ),
                    ),
            );
        let row = div()
            .when(live_drag_source, |row| row.opacity(0.0))
            .child(row);
        if let Some(offset) = self.reorder_offsets.get(&id) {
            visual::reflow(
                row,
                *offset as f32 * ROW_HEIGHT,
                format!("reorder-feedback-{}-{id}", self.feedback_revision),
                cx,
            )
        } else if let Some(offset) = live_offset {
            row.relative()
                .top(px(offset as f32 * ROW_HEIGHT))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

fn settings_window_card(
    icon: IconName,
    title: &'static str,
    description: &'static str,
    content: impl IntoElement,
    cx: &App,
) -> Div {
    div()
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .p_3()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Icon::new(icon)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().font_semibold().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(description),
                        ),
                ),
        )
        .child(content)
}

impl Render for SettingsWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div()
                .size_full()
                .child("The main window is no longer available");
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let language_pending = owner_state.language_pending;
        window.set_window_title(tr(language, "设置", "Settings"));
        let theme = owner_state.theme;
        let theme_pending = owner_state.theme_pending;
        let hotkey_choice = owner_state.hotkey_choice;
        let hotkey_pending = owner_state.hotkey_pending;
        let autostart = owner_state.autostart;
        let autostart_pending = owner_state.autostart_pending;
        let data_size = owner_state.data_size;
        let data_size_pending = owner_state.data_size_pending;
        let maintenance_pending = owner_state.database_maintenance_pending;
        let export_pending = owner_state.export_pending;
        let clear_all_pending = owner_state.clear_all_pending;
        let data_size_detail = data_size.map_or_else(
            || {
                tr(
                    language,
                    "尚未统计数据占用",
                    "Usage has not been calculated",
                )
                .to_owned()
            },
            |size| {
                if language == LanguagePreference::English {
                    format!(
                        "Total {} · Database {} · Images {} / {} · Staged {} / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes),
                        size.staged_count,
                        format_bytes(size.staged_bytes)
                    )
                } else {
                    format!(
                        "共 {} · 数据库 {} · 图片 {} 个 / {} · 暂存 {} 个 / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes),
                        size.staged_count,
                        format_bytes(size.staged_bytes)
                    )
                }
            },
        );

        let appearance = settings_window_card(
            IconName::Palette,
            tr(language, "外观与语言", "Appearance & language"),
            tr(
                language,
                "选择界面语言和明暗主题",
                "Choose the interface language and theme",
            ),
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div().flex().gap_2().children(
                        [
                            (
                                "settings-window-language-zh",
                                "简体中文",
                                LanguagePreference::Chinese,
                            ),
                            (
                                "settings-window-language-en",
                                "English",
                                LanguagePreference::English,
                            ),
                        ]
                        .map(|(id, label, preference)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .outline()
                                .small()
                                .label(label)
                                .selected(language == preference)
                                .disabled(language_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        if owner.language != preference
                                            && owner.send(Command::SetLanguage(preference), cx)
                                        {
                                            owner.language_pending = true;
                                            cx.notify();
                                        }
                                    });
                                })
                        }),
                    ),
                )
                .child(
                    div().flex().gap_2().children(
                        [
                            (
                                "settings-window-theme-system",
                                tr(language, "跟随系统", "System"),
                                IconName::LayoutDashboard,
                                ThemePreference::System,
                            ),
                            (
                                "settings-window-theme-light",
                                tr(language, "浅色", "Light"),
                                IconName::Sun,
                                ThemePreference::Light,
                            ),
                            (
                                "settings-window-theme-dark",
                                tr(language, "深色", "Dark"),
                                IconName::Moon,
                                ThemePreference::Dark,
                            ),
                        ]
                        .map(|(id, label, icon, preference)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .outline()
                                .small()
                                .icon(icon)
                                .tooltip(label)
                                .accessibility_label(label)
                                .selected(theme == preference)
                                .disabled(theme_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        if owner.theme != preference
                                            && owner.send(Command::SetTheme(preference), cx)
                                        {
                                            owner.theme_pending = true;
                                            cx.notify();
                                        }
                                    });
                                })
                        }),
                    ),
                ),
            cx,
        );

        let shortcut = settings_window_card(
            IconName::SquareTerminal,
            tr(language, "唤出快捷键", "Shortcut"),
            tr(
                language,
                "用于打开剪贴板窗口的全局快捷键",
                "Global shortcut used to open the clipboard",
            ),
            div().flex().flex_wrap().gap_2().children(
                [
                    (
                        "settings-window-hotkey-ctrl-shift-v",
                        HotkeyPreference::CtrlShiftV,
                    ),
                    ("settings-window-hotkey-alt-c", HotkeyPreference::AltC),
                    (
                        "settings-window-hotkey-ctrl-alt-v",
                        HotkeyPreference::CtrlAltV,
                    ),
                    (
                        "settings-window-hotkey-disabled",
                        HotkeyPreference::Disabled,
                    ),
                ]
                .map(|(id, choice)| {
                    let owner = self.owner.clone();
                    Button::new(id)
                        .outline()
                        .small()
                        .label(hotkey_label(language, choice))
                        .selected(hotkey_choice == choice)
                        .disabled(hotkey_pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                owner.select_hotkey(choice, cx);
                            });
                        })
                }),
            ),
            cx,
        );

        let startup_owner = self.owner.clone();
        let startup = settings_window_card(
            IconName::Play,
            tr(language, "开机启动", "Startup"),
            tr(
                language,
                "登录 Windows 后自动启动",
                "Launch automatically after signing in to Windows",
            ),
            div().child(
                Button::new("settings-window-autostart")
                    .outline()
                    .small()
                    .icon(if autostart {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .label(if autostart {
                        tr(language, "已开启", "Enabled")
                    } else {
                        tr(language, "已关闭", "Disabled")
                    })
                    .selected(autostart)
                    .disabled(autostart_pending)
                    .on_click(move |_, _, cx| {
                        let _ = startup_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetAutostart(!owner.autostart), cx) {
                                owner.autostart_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            ),
            cx,
        );

        let refresh_owner = self.owner.clone();
        let optimize_owner = self.owner.clone();
        let folder_owner = self.owner.clone();
        let export_owner = self.owner.clone();
        let storage = settings_window_card(
            IconName::HardDrive,
            tr(language, "本地数据", "Storage"),
            tr(
                language,
                "数据库、受管图片和暂存文件",
                "Database, managed images and staged files",
            ),
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(data_size_detail),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("settings-window-refresh")
                                .outline()
                                .small()
                                .icon(IconName::RotateCw)
                                .tooltip(tr(language, "刷新占用", "Refresh usage"))
                                .accessibility_label(tr(language, "刷新占用", "Refresh usage"))
                                .disabled(data_size_pending || maintenance_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = refresh_owner.update(cx, |owner, cx| {
                                        owner.refresh_data_size(cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("settings-window-optimize")
                                .outline()
                                .small()
                                .icon(IconName::HardDrive)
                                .tooltip(tr(language, "整理数据库", "Optimize database"))
                                .accessibility_label(tr(
                                    language,
                                    "整理数据库",
                                    "Optimize database",
                                ))
                                .disabled(data_size_pending || maintenance_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = optimize_owner.update(cx, |owner, cx| {
                                        if owner.send(Command::OptimizeDatabase, cx) {
                                            owner.database_maintenance_pending = true;
                                            cx.notify();
                                        }
                                    });
                                }),
                        )
                        .child(
                            Button::new("settings-window-folder")
                                .outline()
                                .small()
                                .icon(IconName::FolderOpen)
                                .tooltip(tr(language, "打开数据目录", "Open data folder"))
                                .accessibility_label(tr(
                                    language,
                                    "打开数据目录",
                                    "Open data folder",
                                ))
                                .on_click(move |_, _, cx| {
                                    let _ = folder_owner.update(cx, |owner, cx| {
                                        owner.send(Command::OpenDataDirectory, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("settings-window-export")
                                .outline()
                                .small()
                                .icon(IconName::ExternalLink)
                                .tooltip(tr(language, "导出备份", "Export backup"))
                                .accessibility_label(tr(language, "导出备份", "Export backup"))
                                .disabled(export_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = export_owner.update(cx, |owner, cx| {
                                        owner.start_export(cx);
                                    });
                                }),
                        ),
                ),
            cx,
        );

        let privacy_owner = self.owner.clone();
        let privacy = settings_window_card(
            IconName::CircleX,
            tr(language, "隐私清理", "Privacy"),
            tr(
                language,
                "永久删除全部剪贴板历史",
                "Permanently delete all clipboard history",
            ),
            div().child(
                Button::new("settings-window-clear-all")
                    .danger()
                    .small()
                    .icon(IconName::Delete)
                    .label(tr(language, "删除全部历史", "Delete all history"))
                    .disabled(clear_all_pending)
                    .on_click(move |_, window, cx| {
                        let _ = privacy_owner.update(cx, |owner, cx| {
                            owner.settings_window = None;
                            owner.clear_all_confirm_open = true;
                            owner.clear_confirm_open = false;
                            owner.group_delete_id = None;
                            owner.group_move_id = None;
                            owner.reset_selection();
                            cx.notify();
                        });
                        window.remove_window();
                    }),
            ),
            cx,
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family("Microsoft YaHei UI")
            .child(
                TitleBar::new().bg(cx.theme().background).child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(language, "设置", "Settings")),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(appearance)
                            .child(shortcut)
                            .child(startup),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(storage)
                            .child(privacy),
                    ),
            )
    }
}

impl Render for ClipboardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family("Microsoft YaHei UI")
            .child(
                TitleBar::new().bg(cx.theme().background).child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().font_semibold().child("ElegantClipboard"))
                        .child(
                            div()
                                .h_full()
                                .pr_1()
                                .flex()
                                .items_center()
                                .gap_1()
                                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                })
                                .child(
                                    Button::new("window-pin")
                                        .ghost()
                                        .xsmall()
                                        .h(px(CONTROL_HEIGHT))
                                        .icon(if self.window_pinned {
                                            IconName::StarFill
                                        } else {
                                            IconName::Star
                                        })
                                        .tooltip(if self.window_pinned {
                                            tr(self.language, "取消置顶窗口", "Unpin window")
                                        } else {
                                            tr(self.language, "置顶窗口", "Pin window")
                                        })
                                        .accessibility_label(if self.window_pinned {
                                            tr(self.language, "取消置顶窗口", "Unpin window")
                                        } else {
                                            tr(self.language, "置顶窗口", "Pin window")
                                        })
                                        .selected(self.window_pinned)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.toggle_window_pin(window, cx);
                                        })),
                                ),
                        ),
                ),
            )
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
    }
}

impl ClipboardView {
    fn render_clear_all_confirmation(&self, cx: &mut Context<Self>) -> Div {
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .p(px(PAGE_PADDING))
            .flex()
            .flex_col()
            .gap_3()
            .on_action(cx.listener(|this, _: &CancelDrag, window, cx| {
                if !this.clear_all_pending {
                    this.clear_all_confirm_open = false;
                    window.focus(&this.list_focus, cx);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .text_color(cx.theme().danger)
                    .child(tr(self.language, "删除全部历史", "Delete all history")),
            )
            .child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().danger)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(
                                self.language,
                                "删除所有分组中的全部历史？置顶和收藏也会删除。",
                                "Delete all history from every group? Pinned and favorite items will also be deleted.",
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(
                                self.language,
                                "设置和自定义分组会保留；内容及受管媒体无法恢复，可先导出备份。",
                                "Settings and custom groups will be kept. Content and managed media cannot be recovered; export a backup first if needed.",
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("clear-all-history-cancel")
                            .small()
                            .ghost()
                            .label(tr(self.language, "取消", "Cancel"))
                            .disabled(self.clear_all_pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_all_confirm_open = false;
                                window.focus(&this.list_focus, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("clear-all-history-confirm")
                            .small()
                            .danger()
                            .label(if self.clear_all_pending {
                                tr(self.language, "正在删除…", "Deleting…")
                            } else {
                                tr(
                                    self.language,
                                    "确认删除全部历史",
                                    "Delete all history",
                                )
                            })
                            .disabled(self.clear_all_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clear_all_history(cx);
                            })),
                    ),
            )
    }

    fn render_status(&self, cx: &Context<Self>) -> Div {
        if self.message.is_empty() && !self.reorder_pending {
            return div();
        }
        div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .px(px(PAGE_PADDING))
            .py_1()
            .text_xs()
            .text_color(if self.is_error {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            })
            .child(if self.reorder_pending {
                tr(self.language, "正在保存顺序…", "Saving order…").to_owned()
            } else {
                self.message.clone()
            })
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.clear_all_confirm_open {
            return visual::reveal(
                self.render_clear_all_confirmation(cx),
                "clear-all-history-confirmation",
                cx,
            );
        }
        if self.preview.id.is_some() {
            return visual::reveal(
                self.render_preview(cx),
                ("preview-enter", self.preview.generation as usize),
                cx,
            );
        }
        let view = cx.entity();
        let empty_message = if self.history.loading {
            tr(self.language, "正在加载…", "Loading…")
        } else if self.search.read(cx).value().is_empty() {
            if self.history.favorite_only {
                tr(
                    self.language,
                    "还没有收藏，点击记录上的“收藏”保留常用文本",
                    "No favorites yet. Mark an item as a favorite to keep it handy.",
                )
            } else if self.history.group_id.is_some() {
                tr(
                    self.language,
                    "该分组暂无可显示的记录",
                    "There are no items in this group.",
                )
            } else {
                tr(
                    self.language,
                    "复制一段文本，它会出现在这里",
                    "Copy something and it will appear here.",
                )
            }
        } else {
            tr(
                self.language,
                "没有匹配的记录，试试其他关键词",
                "No matching items. Try another search.",
            )
        };
        div()
            .key_context("ClipboardApp")
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family("Microsoft YaHei UI")
            .on_action(cx.listener(|this, _: &CancelDrag, window, cx| {
                let was_dragging = cx.has_active_drag();
                cx.stop_active_drag(window);
                this.drop_target = None;
                this.group_drop_target = None;
                this.group_drag_direction = 0;
                this.history_drag_direction = 0;
                if !was_dragging && !this.selected_ids.is_empty() && !this.batch_pending {
                    this.reset_selection();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pt_3()
                    .pb_2()
                    .child(Input::new(&self.search).cleanable(true)),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .h(px(GROUP_BAR_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_none().flex().gap_2().children(
                        [
                            (false, tr(self.language, "全部", "All")),
                            (true, tr(self.language, "收藏记录", "Favorites")),
                        ]
                        .map(|(favorite_only, label)| {
                            Button::new(if favorite_only {
                                "filter-favorites"
                            } else {
                                "filter-all"
                            })
                            .small()
                            .outline()
                            .label(label)
                            .selected(self.history.favorite_only == favorite_only)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if this.history.favorite_only != favorite_only {
                                        this.clear_confirm_open = false;
                                        this.reset_selection();
                                        this.search_task = None;
                                        this.history.set_favorite_filter(favorite_only);
                                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                                        this.query(cx);
                                        window.focus(&this.list_focus, cx);
                                        cx.notify();
                                    }
                                },
                            ))
                        }),
                    ))
                    .child(
                        div()
                            .id("group-bar")
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .overflow_x_scroll()
                            .track_scroll(&self.group_scroll)
                            .horizontal_scrollbar(&self.group_scroll)
                            .on_drag_move(cx.listener(
                                |this, event: &DragMoveEvent<GroupDrag>, _, cx| {
                                    let x = event.event.position.x;
                                    let direction = if !event.bounds.contains(&event.event.position)
                                    {
                                        0
                                    } else if x < event.bounds.left() + px(visual::DRAG_EDGE_ZONE) {
                                        1
                                    } else if x > event.bounds.right() - px(visual::DRAG_EDGE_ZONE)
                                    {
                                        -1
                                    } else {
                                        0
                                    };
                                    if this.group_drag_direction != direction {
                                        this.group_drag_direction = direction;
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(
                                Button::new("group-create-toggle")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "＋ 新建", "+ New"))
                                    .disabled(
                                        self.group_save_pending
                                            || self.group_delete_pending
                                            || self.clear_pending,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.group_editor_open = !this.group_editor_open
                                            || this.group_rename_id.is_some();
                                        this.group_rename_id = None;
                                        if this.group_editor_open {
                                            this.group_name_input.update(cx, |input, cx| {
                                                input.set_value("", window, cx);
                                                input.focus(window, cx);
                                            });
                                        } else {
                                            window.focus(&this.list_focus, cx);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("group-rename-toggle")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "重命名", "Rename"))
                                    .disabled(
                                        self.history.group_id.is_none()
                                            || self.group_save_pending
                                            || self.group_delete_pending
                                            || self.clear_pending,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let Some(id) = this.history.group_id else {
                                            return;
                                        };
                                        let Some(name) = this
                                            .groups
                                            .iter()
                                            .find(|group| group.id == id)
                                            .map(|group| group.name.clone())
                                        else {
                                            return;
                                        };
                                        this.group_rename_id = Some(id);
                                        this.group_editor_open = true;
                                        this.group_name_input.update(cx, |input, cx| {
                                            input.set_value(name, window, cx);
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("group-delete-toggle")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "删除分组", "Delete group"))
                                    .disabled(
                                        self.history.group_id.is_none()
                                            || self.group_save_pending
                                            || self.group_delete_pending
                                            || self.clear_pending,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.group_delete_id = this.history.group_id;
                                        this.reset_selection();
                                        this.clear_confirm_open = false;
                                        this.group_editor_open = false;
                                        this.group_rename_id = None;
                                        this.group_move_id = None;
                                        window.focus(&this.list_focus, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("clear-history-toggle")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "清理历史", "Clear history"))
                                    .disabled(
                                        self.group_save_pending
                                            || self.group_delete_pending
                                            || self.clear_pending,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.clear_confirm_open = true;
                                        this.reset_selection();
                                        this.group_delete_id = None;
                                        this.group_editor_open = false;
                                        this.group_rename_id = None;
                                        this.group_move_id = None;
                                        window.focus(&this.list_focus, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("group-default")
                                    .small()
                                    .outline()
                                    .h(px(CONTROL_HEIGHT))
                                    .label(tr(self.language, "默认分组", "Default"))
                                    .selected(self.history.group_id.is_none())
                                    .disabled(self.group_delete_pending || self.clear_pending)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.select_group(None, window, cx);
                                    })),
                            )
                            .children(self.groups.iter().map(|group| self.render_group(group, cx))),
                    ),
            )
            .when(self.group_editor_open, |container| {
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Input::new(&self.group_name_input)),
                        )
                        .child(
                            Button::new("group-save-submit")
                                .small()
                                .outline()
                                .label(if self.group_rename_id.is_some() {
                                    tr(self.language, "保存名称", "Save name")
                                } else {
                                    tr(self.language, "创建", "Create")
                                })
                                .disabled(self.group_save_pending)
                                .on_click(cx.listener(|this, _, _, cx| this.save_group(cx))),
                        )
                        .child(
                            Button::new("group-save-cancel")
                                .small()
                                .ghost()
                                .label(tr(self.language, "取消", "Cancel"))
                                .disabled(self.group_save_pending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.group_editor_open = false;
                                    this.group_rename_id = None;
                                    window.focus(&this.list_focus, cx);
                                    cx.notify();
                                })),
                        ),
                    ("group-editor", self.group_rename_id.unwrap_or(0) as usize),
                    cx,
                ))
            })
            .when_some(self.group_delete_id, |container, id| {
                let (name, count) = self
                    .groups
                    .iter()
                    .find(|group| group.id == id)
                    .map(|group| (group.name.clone(), group.item_count))
                    .unwrap_or_else(|| {
                        (tr(self.language, "该分组", "this group").into(), 0)
                    });
                let prompt = if self.language == LanguagePreference::English {
                    format!("Delete “{name}”? {count} items will move to the default group.")
                } else {
                    format!("删除「{name}」？{count} 条记录将移到默认分组。")
                };
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .child(prompt),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("group-delete-cancel")
                                        .small()
                                        .ghost()
                                        .label(tr(self.language, "取消", "Cancel"))
                                        .disabled(self.group_delete_pending)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.group_delete_id = None;
                                            window.focus(&this.list_focus, cx);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("group-delete-confirm")
                                        .small()
                                        .danger()
                                        .label(tr(
                                            self.language,
                                            "保留记录并删除分组",
                                            "Delete group and keep items",
                                        ))
                                        .disabled(self.group_delete_pending)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.delete_group(cx)),
                                        ),
                                ),
                        ),
                    ("group-delete", id as usize),
                    cx,
                ))
            })
            .when(self.clear_confirm_open, |container| {
                let name = self
                    .history
                    .group_id
                    .and_then(|id| self.groups.iter().find(|group| group.id == id))
                    .map(|group| group.name.as_str())
                    .unwrap_or(tr(self.language, "默认分组", "Default"));
                let prompt = if self.language == LanguagePreference::English {
                    format!(
                        "Clear unpinned, non-favorite items from “{name}”? Search and favorite filters do not change the scope."
                    )
                } else {
                    format!(
                        "清理「{name}」中未置顶且未收藏的记录？搜索和收藏筛选不影响清理范围。"
                    )
                };
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child(prompt))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(
                                    self.language,
                                    "此操作无法撤销；可先导出备份。",
                                    "This cannot be undone. Export a backup first if needed.",
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("clear-history-cancel")
                                        .small()
                                        .ghost()
                                        .label(tr(self.language, "取消", "Cancel"))
                                        .disabled(self.clear_pending)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.clear_confirm_open = false;
                                            window.focus(&this.list_focus, cx);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("clear-history-confirm")
                                        .small()
                                        .danger()
                                        .label(if self.clear_pending {
                                            tr(self.language, "正在清理…", "Clearing…")
                                        } else {
                                            tr(self.language, "确认清理", "Clear")
                                        })
                                        .disabled(self.clear_pending)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.clear_history(cx)),
                                        ),
                                ),
                        ),
                    ("clear-history", self.history.group_id.unwrap_or(0) as usize),
                    cx,
                ))
            })
            .when_some(self.group_move_id, |container, id| {
                container.child(visual::reveal(
                    div().child(
                        div()
                            .px(px(PAGE_PADDING))
                            .pb_2()
                            .h(px(GROUP_BAR_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .overflow_x_scrollbar()
                            .child(
                                Button::new("move-group-cancel")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "取消移动", "Cancel move"))
                                    .disabled(self.group_move_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.group_move_id = None;
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_xs().child(tr(self.language, "移至", "Move to")))
                            .child(
                                Button::new("move-group-default")
                                    .small()
                                    .outline()
                                    .label(tr(self.language, "默认分组", "Default"))
                                    .disabled(
                                        self.group_move_pending || self.history.group_id.is_none(),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_to_group(id, None, cx);
                                    })),
                            )
                            .children(self.groups.iter().map(|group| {
                                let target = Some(group.id);
                                Button::new(("move-group", group.id as usize))
                                    .small()
                                    .outline()
                                    .label(group.name.clone())
                                    .disabled(
                                        self.group_move_pending || self.history.group_id == target,
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_to_group(id, target, cx);
                                    }))
                            })),
                    ),
                    ("group-move", id as usize),
                    cx,
                ))
            })
            .when(!self.selected_ids.is_empty(), |container| {
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if self.language == LanguagePreference::English {
                                    format!("{} selected", self.selected_ids.len())
                                } else {
                                    format!("已选 {} 条", self.selected_ids.len())
                                }),
                        )
                        .child(
                            Button::new("batch-select-loaded")
                                .small()
                                .ghost()
                                .label(tr(self.language, "全选已加载", "Select loaded"))
                                .disabled(
                                    self.batch_pending
                                        || self.history.loading
                                        || self.selected_ids.len() == self.history.items.len(),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.select_all_loaded(cx))),
                        )
                        .child(
                            Button::new("batch-clear-selection")
                                .small()
                                .ghost()
                                .label(tr(self.language, "取消选择", "Clear selection"))
                                .disabled(self.batch_pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reset_selection();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("batch-merge")
                                .small()
                                .outline()
                                .label(if self.paste_target.is_some() && self._tray.is_some() {
                                    tr(self.language, "合并粘贴", "Merge and paste")
                                } else {
                                    tr(self.language, "合并复制", "Merge and copy")
                                })
                                .disabled(
                                    self.batch_pending
                                        || self.paste_pending.is_some()
                                        || self.selected_ids.len() < 2,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.merge_selected(window, cx);
                                })),
                        )
                        .child(
                            Button::new("batch-delete-open")
                                .small()
                                .danger()
                                .label(tr(self.language, "删除选中", "Delete selected"))
                                .disabled(self.batch_pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.batch_confirm_open = true;
                                    this.clear_confirm_open = false;
                                    this.group_delete_id = None;
                                    cx.notify();
                                })),
                        ),
                    ("batch-toolbar", self.history.group_id.unwrap_or(0) as usize),
                    cx,
                ))
            })
            .when(self.batch_confirm_open, |container| {
                let prompt = if self.language == LanguagePreference::English {
                    format!(
                        "Delete the {} selected items? This includes pinned and favorite items and cannot be undone.",
                        self.selected_ids.len()
                    )
                } else {
                    format!(
                        "确定删除选中的 {} 条记录？包含置顶和收藏，无法撤销。",
                        self.selected_ids.len()
                    )
                };
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_sm().child(prompt))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("batch-delete-cancel")
                                        .small()
                                        .ghost()
                                        .label(tr(self.language, "取消", "Cancel"))
                                        .disabled(self.batch_pending)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.batch_confirm_open = false;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("batch-delete-confirm")
                                        .small()
                                        .danger()
                                        .label(if self.batch_pending {
                                            tr(self.language, "正在删除…", "Deleting…").to_owned()
                                        } else if self.language == LanguagePreference::English {
                                            format!("Delete {} items", self.selected_ids.len())
                                        } else {
                                            format!("确认删除 {} 条", self.selected_ids.len())
                                        })
                                        .disabled(self.batch_pending)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.delete_selected(cx)),
                                        ),
                                ),
                        ),
                    ("batch-confirm", self.selected_ids.len()),
                    cx,
                ))
            })
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.language == LanguagePreference::English {
                        format!("{} items", self.history.total)
                    } else {
                        format!("{} 条记录", self.history.total)
                    })
                    .child(tr(
                        self.language,
                        "↑↓/PgUp/PgDn/Home/End · Enter 复制",
                        "↑↓/PgUp/PgDn/Home/End · Enter to copy",
                    )),
            )
            .child(
                div()
                    .key_context("HistoryList")
                    .track_focus(&self.list_focus)
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_action(cx.listener(|this, _: &PreviewSelected, window, cx| {
                        if let Some(id) = this.history.selected {
                            this.open_preview(id, window, cx);
                        }
                    }))
                    .on_drag_move(
                        cx.listener(|this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
                            let y = event.event.position.y;
                            let direction = if !event.bounds.contains(&event.event.position) {
                                0
                            } else if y < event.bounds.top() + px(visual::DRAG_EDGE_ZONE) {
                                -1
                            } else if y > event.bounds.bottom() - px(visual::DRAG_EDGE_ZONE) {
                                1
                            } else {
                                0
                            };
                            if this.history_drag_direction != direction {
                                this.history_drag_direction = direction;
                                cx.notify();
                            }
                        }),
                    )
                    .on_action(cx.listener(|this, _: &Next, _, cx| this.select(1, cx)))
                    .on_action(cx.listener(|this, _: &Previous, _, cx| this.select(-1, cx)))
                    .on_action(cx.listener(|this, _: &First, _, cx| {
                        this.select_index(0, ScrollStrategy::Top, cx);
                    }))
                    .on_action(cx.listener(|this, _: &Last, _, cx| {
                        this.select_index(usize::MAX, ScrollStrategy::Bottom, cx);
                    }))
                    .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                        this.select(-this.page_step(), cx);
                    }))
                    .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                        this.select(this.page_step(), cx);
                    }))
                    .on_action(cx.listener(|this, _: &SelectAllLoaded, _, cx| {
                        this.select_all_loaded(cx);
                    }))
                    .on_action(cx.listener(|this, _: &CopySelected, _, cx| {
                        if let Some(id) = this.history.selected {
                            this.send(Command::Copy(id), cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &PasteSelected, window, cx| {
                        if let Some(id) = this.history.selected {
                            this.paste_selected(id, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                        if let Some(id) = this.history.selected {
                            this.send(Command::Delete(id), cx);
                        }
                    }))
                    .when(self.history.items.is_empty(), |container| {
                        container.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(empty_message),
                        )
                    })
                    .when(!self.history.items.is_empty(), |container| {
                        container.child(
                            uniform_list(
                                "history",
                                self.history.items.len(),
                                move |range, _, cx| {
                                    view.update(cx, |this, cx| {
                                        range.map(|index| this.render_row(index, cx)).collect()
                                    })
                                },
                            )
                            .size_full()
                            .track_scroll(&self.scroll),
                        )
                    }),
            )
            .when(
                (self.history.items.len() as i64) < self.history.total
                    && self.history.limit < HISTORY_LIMIT,
                |container| {
                    container.child(
                        div().px(px(PAGE_PADDING)).py_2().child(
                            Button::new("more")
                                .ghost()
                                .small()
                                .label(tr(self.language, "加载更多", "Load more"))
                                .disabled(self.history.loading)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history.limit =
                                        (this.history.limit + PAGE_SIZE).min(HISTORY_LIMIT);
                                    this.history.generation += 1;
                                    this.history.loading = true;
                                    this.query(cx);
                                    cx.notify();
                                })),
                        ),
                    )
                },
            )
            .into_any_element()
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    let (unit_bytes, unit) = if bytes >= GIB {
        (GIB, "GiB")
    } else if bytes >= MIB {
        (MIB, "MiB")
    } else if bytes >= KIB {
        (KIB, "KiB")
    } else {
        return format!("{bytes} B");
    };
    format!("{:.1} {unit}", bytes as f64 / unit_bytes as f64)
}

fn apply_theme(preference: ThemePreference, window: &mut Window, cx: &mut App) {
    match preference {
        ThemePreference::System => Theme::sync_system_appearance(Some(window), cx),
        ThemePreference::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
}
