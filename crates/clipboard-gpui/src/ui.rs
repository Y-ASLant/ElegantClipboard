use crate::paste;
use crate::tray::{self, TrayCommand};
use crate::visual::{self, CONTROL_HEIGHT, PAGE_PADDING, ROW_HEIGHT};
use crate::{
    options::Options,
    state::{HistoryState, PreviewState},
};
use clipboard_core::{
    HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    preferences::{HotkeyPreference, ThemePreference},
};
use clipboard_platform::hotkey::Hotkey;
use clipboard_platform::{Command, Event, InstanceBusy, Service};
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
use std::{cell::Cell, rc::Rc};
use tray_icon::TrayIcon;

gpui_kit::actions!(
    history,
    [
        Next,
        Previous,
        CopySelected,
        PasteSelected,
        DeleteSelected,
        FocusSearch,
        PreviewSelected,
        ClosePreview,
        CancelDrag
    ]
);

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
    let monitoring = options.monitor;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.bind_keys([
                KeyBinding::new("down", Next, Some("HistoryList")),
                KeyBinding::new("up", Previous, Some("HistoryList")),
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
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ElegantClipboard".into()),
                        ..TitleBar::title_bar_options()
                    }),
                    window_min_size: Some(size(px(420.), px(520.))),
                    app_id: Some("com.aslant.elegant-clipboard-gpui".into()),
                    ..TitleBar::window_options()
                },
                |window, cx| {
                    let view = cx.new(|cx| {
                        ClipboardView::new(
                            service,
                            events,
                            monitoring,
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
            )
            .expect("无法创建 ElegantClipboard 窗口");
            cx.activate(true);
            if options.smoke_test {
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(3)).await;
                    smoke_exiting.set(true);
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
    Ok(())
}

#[derive(Clone)]
struct HistoryDrag {
    id: i64,
    pinned: bool,
    favorite_only: bool,
    generation: u64,
    preview: String,
}
impl Render for HistoryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.))
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().primary)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .shadow_md()
            .child(div().text_ellipsis().child(self.preview.clone()))
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
    search: Entity<InputState>,
    preview: PreviewState,
    preview_input: Entity<TextareaState>,
    list_focus: FocusHandle,
    scroll: UniformListScrollHandle,
    monitoring: bool,
    theme: ThemePreference,
    theme_pending: bool,
    hotkey_choice: HotkeyPreference,
    hotkey_pending: bool,
    paste_target: Option<(isize, u32)>,
    paste_pending: Option<(i64, (isize, u32))>,
    reorder_pending: bool,
    drop_target: Option<(i64, bool)>,
    reordered: Option<(i64, usize)>,
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
}

impl ClipboardView {
    fn new(
        service: Service,
        events: async_channel::Receiver<Event>,
        monitoring: bool,
        tray_enabled: Rc<Cell<bool>>,
        exiting: Rc<Cell<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索剪贴板历史…"));
        let subscription = cx.subscribe_in(&search, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
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
        let mut startup_errors = Vec::new();
        if let Some(error) = tray_error {
            startup_errors.push(format!("托盘不可用：{error}；关闭窗口将退出"));
        }
        if let Some(error) = hotkey_error {
            startup_errors.push(error);
        }
        let theme = service.initial_theme;
        let paused = service.initial_paused;
        apply_theme(theme, window, cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme == ThemePreference::System {
                apply_theme(this.theme, window, cx);
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
            search,
            preview: PreviewState::default(),
            preview_input: cx.new(|cx| TextareaState::new(window, cx)),
            list_focus,
            scroll: UniformListScrollHandle::new(),
            monitoring,
            theme,
            theme_pending: false,
            hotkey_choice,
            hotkey_pending: false,
            paste_target: None,
            paste_pending: None,
            reorder_pending: false,
            drop_target: None,
            reordered: None,
            feedback_revision: 0,
            last_drag_scroll: Instant::now(),
            paused,
            pause_pending: false,
            message: if startup_errors.is_empty() {
                let activity = if !monitoring {
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
            _subscriptions: vec![subscription, appearance],
            _events: event_task,
            search_task: None,
            feedback_task: None,
        }
    }

    fn send(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.service.send(command) {
            self.message = error.to_string();
            self.is_error = true;
            self.history.loading = false;
            cx.notify();
            return false;
        }
        true
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
        self.send(
            Command::Query {
                search: self.search.read(cx).value().to_string(),
                limit: self.history.limit,
                favorite_only: self.history.favorite_only,
                generation: self.history.generation,
            },
            cx,
        );
    }

    fn apply_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::ShowWindow => {
                self.paste_target = None;
                tray::set_window_visible(window, true);
            }
            Event::Snapshot {
                items,
                total,
                generation,
            } => {
                self.history.apply(items, total, generation);
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
            Event::Reordered {
                from,
                generation,
                result,
            } => {
                self.reorder_pending = false;
                self.drop_target = None;
                match result {
                    Ok(()) if generation == self.history.generation => {
                        self.feedback_revision += 1;
                        self.reordered = Some((from, self.feedback_revision));
                        if self.history.items.iter().any(|item| item.id == from) {
                            self.history.selected = Some(from);
                        }
                        self.feedback_task = Some(cx.spawn(async move |view, cx| {
                            cx.background_executor()
                                .timer(visual::FEEDBACK_DURATION)
                                .await;
                            let _ = view.update(cx, |this, cx| {
                                this.reordered = None;
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
                message,
            } => {
                self.message = message;
                self.is_error = false;
                if let Some((pending_id, target)) = self.paste_pending.take() {
                    if pending_id == id && for_paste {
                        if paste::is_external_target(window, target) {
                            tray::set_window_visible(window, false);
                            cx.spawn_in(window, async move |view, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(60))
                                    .await;
                                let _ = view.update_in(cx, |this, window, cx| {
                                    match paste::send_to_target(target) {
                                        Ok(()) => {
                                            this.message =
                                                "已发送粘贴快捷键，请检查目标应用".into();
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
                        } else {
                            self.message = "目标窗口已变化，内容已复制，请手动粘贴".into();
                            self.is_error = true;
                        }
                    } else {
                        self.paste_pending = Some((pending_id, target));
                    }
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
            Event::Error(message) => {
                self.paste_pending = None;
                self.message = message;
                self.is_error = true;
                self.history.loading = false;
                self.pause_pending = false;
            }
        }
        cx.notify();
    }

    fn open_preview(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.history.selected = Some(id);
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
        self.preview.close();
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> Div {
        let id = self.preview.id.expect("preview is open");
        let ready = matches!(self.preview.result, Some(Ok(_)));
        let image = matches!(self.preview.result, Some(Ok(PreviewContent::Image(_))));
        let files = matches!(self.preview.result, Some(Ok(PreviewContent::Files(_))));
        let rich = matches!(self.preview.result, Some(Ok(PreviewContent::RichText(_))));
        let message = match &self.preview.result {
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
        };
        let body: AnyElement = match &self.preview.result {
            Some(Ok(
                PreviewContent::Text(_) | PreviewContent::Files(_) | PreviewContent::RichText(_),
            )) => Textarea::new(&self.preview_input)
                .readonly(true)
                .h_full()
                .aria_label(if files {
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
                    .child(div().text_lg().font_semibold().child(if image {
                        "图片预览"
                    } else if files {
                        "文件路径"
                    } else if rich {
                        "富文本预览"
                    } else {
                        "完整内容"
                    }))
                    .child(
                        Button::new("preview-close")
                            .ghost()
                            .label("返回列表 (Esc)")
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
                    .justify_end()
                    .gap_2()
                    .child(
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
                    ),
            )
    }

    fn select(&mut self, direction: isize, cx: &mut Context<Self>) {
        if let Some(index) = self.history.select_relative(direction) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
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

    fn valid_drag(&self, drag: &HistoryDrag, pinned: bool) -> bool {
        !self.reorder_pending
            && !self.history.loading
            && drag.generation == self.history.generation
            && drag.favorite_only == self.history.favorite_only
            && drag.pinned == pinned
            && self
                .history
                .items
                .iter()
                .any(|item| item.id == drag.id && item.is_pinned == pinned)
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.history.items[index];
        let id = item.id;
        let is_image = item.content_type == "image";
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
        } else if item.content_type == "files" {
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
        let pinned = item.is_pinned;
        let favorite = item.is_favorite;
        let drag = HistoryDrag {
            id,
            pinned,
            favorite_only: self.history.favorite_only,
            generation: self.history.generation,
            preview: item.preview.clone().unwrap_or_default(),
        };
        let entity = cx.entity();
        let active_drop = cx.has_active_drag().then_some(self.drop_target).flatten();
        let color = if selected {
            cx.theme().accent
        } else {
            cx.theme().background
        };
        let row = div()
            .id(("history-row", id as usize))
            .h(px(ROW_HEIGHT))
            .px(px(PAGE_PADDING))
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
                    if !event.bounds.contains(&event.event.position) {
                        if this.drop_target.is_some_and(|(target, _)| target == id) {
                            this.drop_target = None;
                            cx.notify();
                        }
                        return;
                    }
                    let after = event.event.position.y > event.bounds.center().y;
                    let target = (this.valid_drag(event.drag(cx), pinned)
                        && event.drag(cx).id != id)
                        .then_some((id, after));
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
                    this.message = "列表已变化，或跨越了置顶区域，请重新拖动".into();
                    this.is_error = true;
                    cx.notify();
                    return;
                }
                let after = this
                    .drop_target
                    .filter(|(target, _)| *target == id)
                    .is_some_and(|(_, after)| after);
                if this.send(
                    Command::Reorder {
                        from: drag.id,
                        to: id,
                        after,
                        favorite_only: drag.favorite_only,
                        generation: drag.generation,
                    },
                    cx,
                ) {
                    this.reorder_pending = true;
                }
                this.drop_target = None;
                cx.notify();
            }))
            .py_2()
            .cursor_pointer()
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
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .bg(color)
                    .relative()
                    .when(
                        active_drop.is_some_and(|(target, _)| target == id),
                        |card| {
                            card.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .h(px(3.))
                                    .bg(cx.theme().primary)
                                    .when(active_drop == Some((id, true)), |line| line.bottom_0())
                                    .when(active_drop == Some((id, false)), |line| line.top_0()),
                            )
                        },
                    )
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(("drag-handle", id as usize))
                                            .px_1()
                                            .cursor_pointer()
                                            .child("⠿")
                                            .when(
                                                !self.reorder_pending && !self.history.loading,
                                                |handle| {
                                                    handle.on_drag(
                                                        drag.clone(),
                                                        move |drag, _, _, cx| {
                                                            entity.update(cx, |this, cx| {
                                                                this.history.selected =
                                                                    Some(drag.id);
                                                                this.drop_target = None;
                                                                cx.notify();
                                                            });
                                                            cx.new(|_| drag.clone())
                                                        },
                                                    )
                                                },
                                            ),
                                    )
                                    .child(format!(
                                        "{}{} · {}",
                                        if pinned { "置顶" } else { "" },
                                        kind,
                                        detail
                                    )),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(item.created_at.clone()),
                            ),
                    )
                    .child(if is_image {
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(64.))
                                    .h(px(48.))
                                    .flex_none()
                                    .rounded_sm()
                                    .overflow_hidden()
                                    .bg(cx.theme().muted)
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
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(item.preview.clone().unwrap_or_else(|| "图片".into())),
                            )
                    } else {
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_sm()
                            .line_height(px(20.))
                            .line_clamp(2)
                            .text_ellipsis()
                            .child(item.preview.clone().unwrap_or_default())
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .justify_end()
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
        if let Some((moved, revision)) = self.reordered
            && moved == id
        {
            visual::reveal(row, ("reorder-feedback", revision), cx)
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
                            .map(|(id, label, theme)| {
                                Button::new(id)
                                    .ghost()
                                    .xsmall()
                                    .h(px(CONTROL_HEIGHT))
                                    .label(label)
                                    .selected(self.theme == theme)
                                    .disabled(self.theme_pending)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.theme != theme
                                            && this.send(Command::SetTheme(theme), cx)
                                        {
                                            this.theme_pending = true;
                                            cx.notify();
                                        }
                                    }))
                            }),
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
                            .map(|(id, choice)| {
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
                            }),
                        ),
                    ),
            )
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
                cx.stop_active_drag(window);
                this.drop_target = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pt_5()
                    .pb_3()
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
                                    .child("拖动 ⠿ 调整顺序 · 置顶项单独排序"),
                            ),
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
            )
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb_3()
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
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{} 条记录", self.history.total))
                    .child("↑↓ 选择 · Enter 复制"),
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
