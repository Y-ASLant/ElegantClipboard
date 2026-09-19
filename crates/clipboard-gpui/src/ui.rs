use crate::paste;
use crate::tray::{self, TrayCommand};
use crate::visual::{self, CONTROL_HEIGHT, GROUP_BAR_HEIGHT, PAGE_PADDING, ROW_HEIGHT};
use crate::{
    options::Options,
    state::{HistoryState, PreviewState, reorder_offsets},
};
use clipboard_core::{
    HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    database::Group,
    preferences::{HotkeyPreference, ThemePreference},
};
use clipboard_platform::hotkey::Hotkey;
use clipboard_platform::{Command, Event, FailureKind, InstanceBusy, Service};
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
use std::time::{Duration, Instant};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};
use tray_icon::TrayIcon;

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
            let bounds = Bounds::centered(None, size(px(560.), px(760.)), cx);
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
                            if self.pinned { "置顶 · " } else { "" },
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
    settings_open: bool,
    theme: ThemePreference,
    theme_pending: bool,
    hotkey_choice: HotkeyPreference,
    hotkey_pending: bool,
    autostart: bool,
    autostart_pending: bool,
    export_pending: bool,
    paste_target: Option<(isize, u32)>,
    paste_pending: Option<(i64, (isize, u32))>,
    reorder_pending: bool,
    drop_target: Option<DropTarget>,
    reorder_before: Option<Vec<i64>>,
    reorder_offsets: HashMap<i64, isize>,
    feedback_revision: usize,
    last_drag_scroll: Instant,
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
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索剪贴板历史…"));
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
        let tray = tray::create(tray_sender);
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
                            this.message = "已从原窗口唤出，可选择记录并粘贴".into();
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
            startup_errors.push(format!("托盘不可用：{error}；关闭窗口将退出"));
        }
        if let Some(error) = hotkey_error {
            startup_errors.push(error);
        }
        if let Err(error) = &autostart {
            startup_errors.push(format!("无法读取开机启动设置：{error}"));
        }
        let theme = service.initial_theme;
        let paused = service.initial_paused;
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
                    this.message = "已离开历史窗口；如需自动粘贴，请从目标应用重新唤出".into();
                    this.is_error = false;
                    cx.notify();
                }
            }
        });
        let list_focus = cx.focus_handle();
        window.focus(&list_focus, cx);
        Self {
            service,
            _tray: tray.ok(),
            _tray_events: tray_events,
            hotkey: hotkey.ok().flatten(),
            hotkey_sender,
            _hotkey_events: hotkey_events,
            exiting,
            history: HistoryState::default(),
            groups: Vec::new(),
            search,
            group_name_input: cx.new(|cx| InputState::new(window, cx).placeholder("分组名称")),
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
            settings_open: false,
            theme,
            theme_pending: false,
            hotkey_choice,
            hotkey_pending: false,
            autostart: autostart.unwrap_or(false),
            autostart_pending: false,
            export_pending: false,
            paste_target: None,
            paste_pending: None,
            reorder_pending: false,
            drop_target: None,
            reorder_before: None,
            reorder_offsets: HashMap::new(),
            feedback_revision: 0,
            last_drag_scroll: Instant::now(),
            paused,
            pause_pending: false,
            message: if startup_errors.is_empty() {
                let activity = if !startup.monitoring {
                    "采集已禁用"
                } else if paused {
                    "已暂停记录"
                } else {
                    "正在记录文本"
                };
                if hotkey_choice == HotkeyPreference::Disabled {
                    format!("{activity}；可从托盘唤出窗口")
                } else {
                    format!("{activity}；{} 可唤出窗口", hotkey_choice.label())
                }
            } else {
                startup_errors.join("；")
            },
            is_error: !startup_errors.is_empty(),
            _subscriptions: vec![subscription, appearance, activation],
            _events: event_task,
            search_task: None,
            feedback_task: None,
            group_feedback_task: None,
            group_drag_scroll_task: None,
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
                        this.message = format!("无法选择备份路径：{error}");
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.export_pending = false;
                        this.message = format!("备份路径选择中断：{error}");
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
                        this.message = format!("无法选择另存为路径：{error}");
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.save_as_pending = None;
                        this.message = format!("另存为路径选择中断：{error}");
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
            Err(error) => Some(format!("原快捷键也无法恢复：{error}")),
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
                self.message = format!("快捷键切换失败：{error}");
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
                "正在合并并返回原窗口…"
            } else {
                "正在合并所选记录…"
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
                        self.message = "分组顺序已保存".into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.group_feedback_ids.clear();
                        self.message = format!("分组排序失败：{error}");
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
                        self.message = format!("已创建分组：{}", group.name);
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("创建分组失败：{error}");
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
                        self.message = format!("分组已重命名为：{}", group.name);
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("重命名分组失败：{error}");
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
                        self.message = format!("已删除分组，{count} 条记录移至默认分组");
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("删除分组失败：{error}");
                        self.is_error = true;
                    }
                }
            }
            Event::HistoryCleared(result) => {
                self.clear_pending = false;
                self.clear_confirm_open = false;
                match result {
                    Ok(count) => {
                        self.message = format!("已清理 {count} 条未置顶且未收藏的记录");
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("清理历史失败：{error}");
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
                        self.message = format!("已删除 {count} 条选中记录");
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("批量删除失败：{error}");
                        self.is_error = true;
                    }
                }
            }
            Event::ItemMoved { result, .. } => {
                self.group_move_pending = false;
                match result {
                    Ok(()) => {
                        self.group_move_id = None;
                        self.message = "已移动到目标分组".into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("移动分组失败：{error}");
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
                        Some(Ok(PreviewContent::Files(paths))) => Some(paths.join("\n")),
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
                            "内容已保存为纯文本"
                        } else {
                            "内容未修改"
                        }
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("保存编辑失败：{error}");
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
                        self.message = format!("已另存到 {}", destination.display());
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("另存为失败：{error}");
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
                        self.message = "顺序已保存".into();
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
                self.message = message;
                self.is_error = false;
            }
            Event::Copied {
                id,
                for_paste,
                clipboard_sequence,
                message,
            } => {
                self.message = message;
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
                        self.message = format!("已合并 {item_count} 条记录并复制，请手动粘贴");
                        self.is_error = true;
                    }
                } else {
                    self.message = format!("已合并 {item_count} 条记录并复制");
                    self.is_error = false;
                }
            }
            Event::ThemeSaved(result) => {
                self.theme_pending = false;
                match result {
                    Ok(theme) => {
                        self.theme = theme;
                        apply_theme(theme, window, cx);
                        self.message = "外观设置已保存".into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("外观保存失败：{error}");
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
                            "全局快捷键已关闭，可从托盘唤出窗口".into()
                        } else {
                            format!("已保存唤出快捷键：{}", choice.label())
                        };
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.hotkey = None;
                        let restore_error = self.restore_hotkey();
                        self.message = format!("快捷键保存失败：{error}");
                        if let Some(restore_error) = restore_error {
                            self.message.push_str(&format!("；{restore_error}"));
                        }
                        self.is_error = true;
                    }
                }
            }
            Event::AutostartSaved(result) => {
                self.autostart_pending = false;
                match result {
                    Ok(enabled) => {
                        self.autostart = enabled;
                        self.message = if enabled {
                            "已开启开机启动"
                        } else {
                            "已关闭开机启动"
                        }
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!("开机启动设置失败：{error}");
                        self.is_error = true;
                    }
                }
            }
            Event::Paused(paused) => {
                self.paused = paused;
                self.pause_pending = false;
                self.is_error = false;
                self.message = if paused {
                    "已暂停记录，已有历史仍可使用"
                } else {
                    "已恢复记录"
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
                        self.message = format!(
                            "已备份 {} 条记录、{} 个附件到 {}{}",
                            report.total_items,
                            included,
                            report.destination.display(),
                            if missing == 0 {
                                String::new()
                            } else {
                                format!("；{missing} 个源附件未包含在备份中")
                            }
                        );
                        self.is_error = missing != 0;
                    }
                    Err(error) => {
                        self.message = format!("导出备份失败：{error}");
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
                    FailureKind::BatchDelete => {
                        self.batch_pending = false;
                        self.batch_confirm_open = false;
                    }
                    FailureKind::EditText { .. } => self.preview_save_pending = false,
                    FailureKind::SaveAs(id) if self.save_as_pending == Some(id) => {
                        self.save_as_pending = None;
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

    fn render_preview(&self, cx: &mut Context<Self>) -> Div {
        let id = self.preview.id.expect("preview is open");
        let ready = matches!(self.preview.result, Some(Ok(_)));
        let image = matches!(self.preview.result, Some(Ok(PreviewContent::Image(_))));
        let files = matches!(self.preview.result, Some(Ok(PreviewContent::Files(_))));
        let rich = matches!(self.preview.result, Some(Ok(PreviewContent::RichText(_))));
        let save_as_name = match &self.preview.result {
            Some(Ok(PreviewContent::Image(path))) => path.file_name(),
            Some(Ok(PreviewContent::Files(paths))) => paths
                .first()
                .and_then(|path| std::path::Path::new(path).file_name()),
            _ => None,
        }
        .map(|name| name.to_string_lossy().into_owned());
        let save_as_label = match &self.preview.result {
            Some(Ok(PreviewContent::Files(paths))) if paths.len() > 1 => "另存第一项",
            _ => "另存为",
        };
        let editable = matches!(
            self.preview.result,
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
        ) && self.preview_source_hash.is_some();
        let message = if self.preview_editing && rich {
            "保存后将变为纯文本，原富文本格式不会保留".into()
        } else if self.preview_editing {
            "编辑文本；保存后搜索结果会更新".into()
        } else {
            match &self.preview.result {
                None => "正在加载完整内容…".to_owned(),
                Some(Err(error)) => error.clone(),
                Some(Ok(PreviewContent::Text(text))) => {
                    format!("{} 字符 · {} 字节 · 只读", text.chars().count(), text.len())
                }
                Some(Ok(PreviewContent::Image(_))) => "图片预览 · 保持原始比例".into(),
                Some(Ok(PreviewContent::RichText(_))) => "富文本 · 纯文本预览".into(),
                Some(Ok(PreviewContent::Files(paths))) => {
                    format!("{} 个文件或文件夹 · 仅保存原始路径", paths.len())
                }
            }
        };
        let body: AnyElement = match &self.preview.result {
            Some(Ok(
                PreviewContent::Text(_) | PreviewContent::Files(_) | PreviewContent::RichText(_),
            )) => Textarea::new(&self.preview_input)
                .readonly(!self.preview_editing || self.preview_save_pending)
                .h_full()
                .aria_label(if self.preview_editing {
                    "编辑文本"
                } else if files {
                    "文件路径"
                } else {
                    "完整文本内容"
                })
                .into_any_element(),
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
                        .with_fallback(|| div().child("图片无法显示").into_any_element()),
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
                                "编辑内容"
                            } else if image {
                                "图片预览"
                            } else if files {
                                "文件路径"
                            } else if rich {
                                "富文本预览"
                            } else {
                                "完整内容"
                            }),
                    )
                    .child(
                        Button::new("preview-close")
                            .ghost()
                            .label(if self.preview_editing {
                                "取消编辑 (Esc)"
                            } else {
                                "返回列表 (Esc)"
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
                                .label("编辑")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_preview_edit(window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing && rich, |bar| {
                        bar.child(
                            Button::new("preview-copy-plain")
                                .outline()
                                .label("复制纯文本")
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
                                .label("在资源管理器中显示")
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::RevealInExplorer(id), cx);
                                })),
                        )
                        .child(
                            Button::new("preview-copy-path")
                                .outline()
                                .label(if self.paste_target.is_some() && self._tray.is_some() {
                                    "粘贴路径"
                                } else {
                                    "复制路径"
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
                                .label("粘贴到原窗口")
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
                                    "复制图片"
                                } else if files {
                                    "复制文件"
                                } else if rich {
                                    "复制富文本"
                                } else {
                                    "复制全文"
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
                                .label("取消")
                                .disabled(self.preview_save_pending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_preview(window, cx);
                                })),
                        )
                        .child(
                            Button::new("preview-save-edit")
                                .primary()
                                .label("保存文本")
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
            self.message = "请从目标应用按全局快捷键唤出，再使用粘贴".into();
            self.is_error = true;
            cx.notify();
            return;
        };
        if self._tray.is_none() || !paste::is_external_target(window, target) {
            self.paste_target = None;
            self.message = "原窗口不可用，仍可使用复制后手动粘贴".into();
            self.is_error = true;
            cx.notify();
            return;
        }
        if self.paste_pending.is_none() && self.send(Command::CopyForPaste(id), cx) {
            self.paste_pending = Some((id, target));
            self.message = "正在复制并返回原窗口…".into();
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
            self.message = "正在复制路径并返回原窗口…".into();
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
            self.message = "目标窗口已变化，内容已复制，请手动粘贴".into();
            self.is_error = true;
            return;
        }
        tray::set_window_visible(window, false);
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                match paste::send_to_target(target, clipboard_sequence) {
                    Ok(()) => {
                        this.message = "已发送粘贴快捷键，请检查目标应用".into();
                        this.is_error = false;
                    }
                    Err(error) => {
                        tray::set_window_visible(window, true);
                        this.message = error.to_string();
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn valid_drag(&self, drag: &HistoryDrag, pinned: bool) -> bool {
        !self.reorder_pending
            && !self.history.loading
            && drag.generation == self.history.generation
            && drag.favorite_only == self.history.favorite_only
            && drag.group_id == self.history.group_id
            && drag.pinned == pinned
            && self
                .history
                .items
                .iter()
                .any(|item| item.id == drag.id && item.is_pinned == pinned)
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
                    .timer(Duration::from_millis(60))
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
                        let next = (offset.x + px(f32::from(this.group_drag_direction) * 18.))
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
                    this.message = "分组列表已变化，请重新拖动".into();
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
        let kind = match item.content_type.as_str() {
            "image" => "图片",
            "files" => "文件",
            "html" => "HTML",
            "rtf" => "RTF",
            "url" => "网址",
            _ => "文本",
        };
        let detail = if is_image {
            match (item.image_width, item.image_height) {
                (Some(width), Some(height)) => format!("{width} × {height}"),
                _ => "尺寸未知".into(),
            }
        } else if is_files {
            let count = item
                .file_paths
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .map_or(0, |paths| paths.len());
            if count == 0 {
                "路径不可用".into()
            } else {
                format!("{count} 项")
            }
        } else if matches!(item.content_type.as_str(), "html" | "rtf") && item.char_count.is_none()
        {
            "纯文本未知".into()
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
        };
        let entity = cx.entity();
        let active_drop = cx.has_active_drag().then_some(self.drop_target).flatten();
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
                        allowed: this.valid_drag(event.drag(cx), pinned),
                    });
                    if this.drop_target != target {
                        this.drop_target = target;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, drag: &HistoryDrag, _, cx| {
                if drag.id == id {
                    return;
                }
                if !this.valid_drag(drag, pinned) {
                    this.drop_target = None;
                    this.message = "列表已变化，或跨越了置顶区域，请重新拖动".into();
                    this.is_error = true;
                    cx.notify();
                    return;
                }
                let Some(target) = this.drop_target.filter(|target| target.id == id) else {
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
                cx.notify();
            }))
            .py_2()
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
                    .py_2()
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
                    .gap_2()
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
                                    .gap_2()
                                    .child(div().px_1().child("⠿"))
                                    .child(format!(
                                        "{}{} · {}",
                                        if pinned { "置顶 · " } else { "" },
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
                                                    .h(px(CONTROL_HEIGHT))
                                                    .label(if marked { "已选" } else { "选择" })
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
                            .px_3()
                            .py_2()
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
                                                    .with_fallback(|| {
                                                        div().child("无法显示").into_any_element()
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
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .justify_end()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Button::new(("preview", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(CONTROL_HEIGHT))
                                    .label("查看")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_preview(id, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("favorite", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(CONTROL_HEIGHT))
                                    .label(if favorite { "取消收藏" } else { "收藏" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::ToggleFavorite(id), cx);
                                    })),
                            )
                            .child(
                                Button::new(("pin", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(CONTROL_HEIGHT))
                                    .label(if pinned { "取消置顶" } else { "置顶" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::TogglePin(id), cx);
                                    })),
                            )
                            .child(
                                Button::new(("move-group", id as usize))
                                    .ghost()
                                    .xsmall()
                                    .h(px(CONTROL_HEIGHT))
                                    .label("分组")
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
                                    .h(px(CONTROL_HEIGHT))
                                    .label("删除")
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
                                        .h(px(CONTROL_HEIGHT))
                                        .label(
                                            if self.paste_target.is_some() && self._tray.is_some() {
                                                "粘贴路径"
                                            } else {
                                                "复制路径"
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
                                    .h(px(CONTROL_HEIGHT))
                                    .label("粘贴")
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
                                            .h(px(CONTROL_HEIGHT))
                                            .label("纯文本")
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
                                    .h(px(CONTROL_HEIGHT))
                                    .label("复制")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.send(Command::Copy(id), cx);
                                    })),
                            ),
                    ),
            );
        let row = div().child(row);
        if let Some(offset) = self.reorder_offsets.get(&id) {
            visual::reflow(
                row,
                *offset as f32 * ROW_HEIGHT,
                format!("reorder-feedback-{}-{id}", self.feedback_revision),
                cx,
            )
        } else {
            row.into_any_element()
        }
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
                TitleBar::new()
                    .bg(cx.theme().background)
                    .child(div().text_sm().font_semibold().child("ElegantClipboard")),
            )
            .child(
                div()
                    .h(px(32.))
                    .flex_none()
                    .px(px(PAGE_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("窗口设置"),
                    )
                    .child(
                        Button::new("settings-toggle")
                            .ghost()
                            .xsmall()
                            .h(px(CONTROL_HEIGHT))
                            .label(if self.settings_open {
                                "收起"
                            } else {
                                "展开"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_open = !this.settings_open;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.settings_open, |view| {
                view.child(visual::reveal(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(PAGE_PADDING))
                                .py_1()
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("外观"),
                                )
                                .child(
                                    div().flex().gap_1().children(
                                        [
                                            ("theme-system", "跟随系统", ThemePreference::System),
                                            ("theme-light", "浅色", ThemePreference::Light),
                                            ("theme-dark", "深色", ThemePreference::Dark),
                                        ]
                                        .map(
                                            |(id, label, theme)| {
                                                Button::new(id)
                                                    .ghost()
                                                    .xsmall()
                                                    .h(px(CONTROL_HEIGHT))
                                                    .label(label)
                                                    .selected(self.theme == theme)
                                                    .disabled(self.theme_pending)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if this.theme != theme
                                                            && this
                                                                .send(Command::SetTheme(theme), cx)
                                                        {
                                                            this.theme_pending = true;
                                                            cx.notify();
                                                        }
                                                    }))
                                            },
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .px(px(PAGE_PADDING))
                                .py_1()
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("唤出"),
                                )
                                .child(
                                    div().flex().gap_1().children(
                                        [
                                            ("hotkey-ctrl-shift-v", HotkeyPreference::CtrlShiftV),
                                            ("hotkey-alt-c", HotkeyPreference::AltC),
                                            ("hotkey-ctrl-alt-v", HotkeyPreference::CtrlAltV),
                                            ("hotkey-disabled", HotkeyPreference::Disabled),
                                        ]
                                        .map(
                                            |(id, choice)| {
                                                Button::new(id)
                                                    .ghost()
                                                    .xsmall()
                                                    .h(px(CONTROL_HEIGHT))
                                                    .label(choice.label())
                                                    .selected(self.hotkey_choice == choice)
                                                    .disabled(self.hotkey_pending)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.select_hotkey(choice, cx);
                                                    }))
                                            },
                                        ),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .px(px(PAGE_PADDING))
                                .py_1()
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("开机启动"),
                                )
                                .child(
                                    Button::new("autostart-toggle")
                                        .ghost()
                                        .xsmall()
                                        .h(px(CONTROL_HEIGHT))
                                        .label(if self.autostart { "关闭" } else { "开启" })
                                        .disabled(self.autostart_pending)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if this.send(Command::SetAutostart(!this.autostart), cx)
                                            {
                                                this.autostart_pending = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        ),
                    "window-settings",
                    cx,
                ))
            })
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
    }
}

impl ClipboardView {
    fn render_status(&self, cx: &Context<Self>) -> Div {
        div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .px(px(PAGE_PADDING))
            .py_3()
            .text_xs()
            .text_color(if self.is_error {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            })
            .child(if self.reorder_pending {
                "正在保存顺序…".to_owned()
            } else {
                self.message.clone()
            })
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.preview.id.is_some() {
            return visual::reveal(
                self.render_preview(cx),
                ("preview-enter", self.preview.generation as usize),
                cx,
            );
        }
        let view = cx.entity();
        let empty_message = if self.history.loading {
            "正在加载…"
        } else if self.search.read(cx).value().is_empty() {
            if self.history.favorite_only {
                "还没有收藏，点击记录上的“收藏”保留常用文本"
            } else if self.history.group_id.is_some() {
                "该分组暂无可显示的记录"
            } else {
                "复制一段文本，它会出现在这里"
            }
        } else {
            "没有匹配的记录，试试其他关键词"
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
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_lg().font_semibold().child("剪贴板历史"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("卡片/分组可拖动 · 红线不可放置"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("export-backup")
                                    .outline()
                                    .small()
                                    .label(if self.export_pending {
                                        "正在备份…"
                                    } else {
                                        "导出备份"
                                    })
                                    .disabled(self.export_pending)
                                    .on_click(cx.listener(|this, _, _, cx| this.start_export(cx))),
                            )
                            .child(
                                Button::new("pause")
                                    .outline()
                                    .small()
                                    .label(if !self.monitoring {
                                        "采集未启用"
                                    } else if self.paused {
                                        "恢复记录"
                                    } else {
                                        "暂停记录"
                                    })
                                    .disabled(!self.monitoring || self.pause_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.send(Command::Pause(!this.paused), cx) {
                                            this.pause_pending = true;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .child(Input::new(&self.search).cleanable(true)),
            )
            .child(div().px(px(PAGE_PADDING)).pb_2().flex().gap_2().children(
                [(false, "全部"), (true, "收藏记录")].map(|(favorite_only, label)| {
                    Button::new(if favorite_only {
                        "filter-favorites"
                    } else {
                        "filter-all"
                    })
                    .small()
                    .outline()
                    .label(label)
                    .selected(self.history.favorite_only == favorite_only)
                    .on_click(cx.listener(move |this, _, window, cx| {
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
                    }))
                }),
            ))
            .child(
                div()
                    .id("group-bar")
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .h(px(GROUP_BAR_HEIGHT))
                    .flex_none()
                    .flex()
                    .gap_2()
                    .overflow_x_scroll()
                    .track_scroll(&self.group_scroll)
                    .horizontal_scrollbar(&self.group_scroll)
                    .on_drag_move(
                        cx.listener(|this, event: &DragMoveEvent<GroupDrag>, _, cx| {
                            let x = event.event.position.x;
                            let direction = if !event.bounds.contains(&event.event.position) {
                                0
                            } else if x < event.bounds.left() + px(28.) {
                                1
                            } else if x > event.bounds.right() - px(28.) {
                                -1
                            } else {
                                0
                            };
                            if this.group_drag_direction != direction {
                                this.group_drag_direction = direction;
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        Button::new("group-create-toggle")
                            .small()
                            .ghost()
                            .label("＋ 新建")
                            .disabled(
                                self.group_save_pending
                                    || self.group_delete_pending
                                    || self.clear_pending,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.group_editor_open =
                                    !this.group_editor_open || this.group_rename_id.is_some();
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
                            .label("重命名")
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
                            .label("删除分组")
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
                            .label("清理历史")
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
                            .label("默认分组")
                            .selected(self.history.group_id.is_none())
                            .disabled(self.group_delete_pending || self.clear_pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.select_group(None, window, cx);
                            })),
                    )
                    .children(self.groups.iter().map(|group| self.render_group(group, cx))),
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
                                    "保存名称"
                                } else {
                                    "创建"
                                })
                                .disabled(self.group_save_pending)
                                .on_click(cx.listener(|this, _, _, cx| this.save_group(cx))),
                        )
                        .child(
                            Button::new("group-save-cancel")
                                .small()
                                .ghost()
                                .label("取消")
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
                    .unwrap_or_else(|| ("该分组".into(), 0));
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
                                .child(format!("删除「{name}」？{count} 条记录将移到默认分组。")),
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
                                        .label("取消")
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
                                        .label("保留记录并删除分组")
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
                    .unwrap_or("默认分组");
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child(format!(
                            "清理「{name}」中未置顶且未收藏的记录？搜索和收藏筛选不影响清理范围。"
                        )))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("此操作无法撤销；可先导出备份。"),
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
                                        .label("取消")
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
                                            "正在清理…"
                                        } else {
                                            "确认清理"
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
                                    .label("取消移动")
                                    .disabled(self.group_move_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.group_move_id = None;
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_xs().child("移至"))
                            .child(
                                Button::new("move-group-default")
                                    .small()
                                    .outline()
                                    .label("默认分组")
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
                                .child(format!("已选 {} 条", self.selected_ids.len())),
                        )
                        .child(
                            Button::new("batch-select-loaded")
                                .small()
                                .ghost()
                                .label("全选已加载")
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
                                .label("取消选择")
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
                                    "合并粘贴"
                                } else {
                                    "合并复制"
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
                                .label("删除选中")
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
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_sm().child(format!(
                            "确定删除选中的 {} 条记录？包含置顶和收藏，无法撤销。",
                            self.selected_ids.len()
                        )))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("batch-delete-cancel")
                                        .small()
                                        .ghost()
                                        .label("取消")
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
                                            "正在删除…".to_owned()
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
                    .child(format!("{} 条记录", self.history.total))
                    .child("↑↓/PgUp/PgDn/Home/End · Enter 复制"),
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
                            if !event.bounds.contains(&event.event.position) {
                                return;
                            }
                            if this.last_drag_scroll.elapsed() < Duration::from_millis(120) {
                                return;
                            }
                            let top = this.scroll.0.borrow().base_handle.logical_scroll_top().0;
                            let next = if event.event.position.y < event.bounds.top() + px(36.) {
                                top.saturating_sub(1)
                            } else if event.event.position.y > event.bounds.bottom() - px(36.) {
                                (top + 1).min(this.history.items.len().saturating_sub(1))
                            } else {
                                return;
                            };
                            this.last_drag_scroll = Instant::now();
                            this.scroll.scroll_to_item_strict(next, ScrollStrategy::Top);
                            cx.notify();
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
                                .label("加载更多")
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

fn apply_theme(preference: ThemePreference, window: &mut Window, cx: &mut App) {
    match preference {
        ThemePreference::System => Theme::sync_system_appearance(Some(window), cx),
        ThemePreference::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
}
