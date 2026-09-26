use crate::links::{self, ProjectLink};
use crate::paste;
use crate::position;
use crate::sound::{self, Sound};
use crate::tray::{self, TrayCommand};
use crate::visual::{self, CONTROL_HEIGHT, GROUP_BAR_HEIGHT, PAGE_PADDING};
use crate::{
    options::Options,
    state::{
        HistoryState, PreviewState, adjacent_group_id, drag_edge_target_index,
        drag_reorder_offsets, format_card_time, next_drag_scroll_index, reorder_offsets,
        search_excerpt, search_highlight_ranges, selection_range_ids, should_hide_after_paste,
        source_app_parts,
    },
};
use clipboard_core::{
    ContentCategory, FilePreviewEntry, HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    database::Group,
    preferences::{
        AppFilterMode, AppFilterPreference, AudioPreference, CardDensity, DisplayPreference,
        HotkeyPreference, HoverPreviewPosition, HoverPreviewPreference, LanguagePreference,
        MonitorTypesPreference, PasteKeyPreference, PasteShortcutConfig, SoundTiming,
        SourceAppDisplay, ThemePreference, TimeFormat, ToolbarButton, ToolbarPreference,
        WindowPositionPreference, WindowSizePreference,
    },
};
use clipboard_platform::hotkey::{
    Hotkey, PasteHotkeyEvent, PasteHotkeys, normalize_paste_shortcut, validate_paste_shortcuts,
};
use clipboard_platform::outside_click::OutsideClickMonitor;
use clipboard_platform::source_app::RunningApp;
use clipboard_platform::{Command, DataSizeInfo, Event, FailureKind, InstanceBusy, Service};
use directories::UserDirs;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
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
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};
use tray_icon::TrayIcon;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetDoubleClickTime, VK_SHIFT};

fn tr(language: LanguagePreference, chinese: &'static str, english: &'static str) -> &'static str {
    match language {
        LanguagePreference::Chinese => chinese,
        LanguagePreference::English => english,
    }
}

fn hover_zoom_percent(current: u16, change: i32) -> u16 {
    (i32::from(current) + change).clamp(50, 400) as u16
}

#[cfg(test)]
mod hover_zoom_tests {
    use super::hover_zoom_percent;

    #[test]
    fn zoom_stays_within_visible_range() {
        assert_eq!(hover_zoom_percent(100, 20), 120);
        assert_eq!(hover_zoom_percent(50, -50), 50);
        assert_eq!(hover_zoom_percent(400, 50), 400);
    }
}

fn single_file_image_path(entries: &[FilePreviewEntry]) -> Option<PathBuf> {
    let [entry] = entries else { return None };
    if !entry.exists || entry.is_dir || entry.metadata_error.is_some() {
        return None;
    }
    let size = entry.size?;
    let limit = file_image_preview_limit(&entry.resolved_path);
    if size == 0 || size > limit {
        return None;
    }
    if !supported_image_file(&entry.resolved_path) {
        return None;
    }
    Some(PathBuf::from(&entry.resolved_path))
}

fn file_image_preview_limit(path: &str) -> u64 {
    if path.starts_with(r"\\") {
        10 * 1024 * 1024
    } else {
        50 * 1024 * 1024
    }
}

fn supported_image_file(path: &str) -> bool {
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileCardAvailability {
    Available,
    Missing,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileCardKind {
    File,
    Folder,
    Multiple,
}

#[derive(Clone, Debug)]
struct FileCardInfo {
    availability: FileCardAvailability,
    kind: FileCardKind,
    image_path: Option<PathBuf>,
    image_too_large: bool,
    total_size: Option<u64>,
}

fn inspect_file_card(raw_paths: &str) -> FileCardInfo {
    let mut info = FileCardInfo {
        availability: FileCardAvailability::Available,
        kind: FileCardKind::File,
        image_path: None,
        image_too_large: false,
        total_size: Some(0),
    };
    let Ok(paths) = serde_json::from_str::<Vec<String>>(raw_paths) else {
        info.availability = FileCardAvailability::Unreadable;
        info.total_size = None;
        return info;
    };
    if paths.is_empty() {
        info.availability = FileCardAvailability::Unreadable;
        info.total_size = None;
        return info;
    }
    if paths.len() > 1 {
        info.kind = FileCardKind::Multiple;
    }
    for path in &paths {
        if !Path::new(path).is_absolute() {
            info.availability = FileCardAvailability::Unreadable;
            info.total_size = None;
            continue;
        }
        match std::fs::metadata(path) {
            Ok(metadata) if paths.len() == 1 && metadata.is_file() => {
                info.total_size = info
                    .total_size
                    .and_then(|total| total.checked_add(metadata.len()));
                info.image_too_large =
                    supported_image_file(path) && metadata.len() > file_image_preview_limit(path);
                info.image_path = single_file_image_path(&[FilePreviewEntry {
                    original_path: path.clone(),
                    resolved_path: path.clone(),
                    exists: true,
                    is_dir: false,
                    size: Some(metadata.len()),
                    recovered: false,
                    metadata_error: None,
                }]);
            }
            Ok(metadata) if paths.len() == 1 && metadata.is_dir() => {
                info.kind = FileCardKind::Folder;
                info.total_size = None;
            }
            Ok(metadata) if metadata.is_file() => {
                info.total_size = info
                    .total_size
                    .and_then(|total| total.checked_add(metadata.len()));
            }
            Ok(_) => info.total_size = None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                info.total_size = None;
                if info.availability == FileCardAvailability::Available {
                    info.availability = FileCardAvailability::Missing;
                }
            }
            Err(_) => {
                info.availability = FileCardAvailability::Unreadable;
                info.total_size = None;
            }
        }
    }
    info
}

#[cfg(test)]
mod file_image_preview_tests {
    use super::{
        FileCardAvailability, FileCardKind, FilePreviewEntry, inspect_file_card,
        single_file_image_path,
    };
    use std::path::PathBuf;

    fn entry(path: &str, size: Option<u64>) -> FilePreviewEntry {
        FilePreviewEntry {
            original_path: path.into(),
            resolved_path: path.into(),
            exists: true,
            is_dir: false,
            size,
            recovered: false,
            metadata_error: None,
        }
    }

    #[test]
    fn only_single_readable_bounded_image_is_decoded() {
        let image = entry(r"C:\sample.PNG", Some(2_000));
        assert_eq!(
            single_file_image_path(std::slice::from_ref(&image)),
            Some(PathBuf::from(r"C:\sample.PNG"))
        );
        assert!(single_file_image_path(&[image.clone(), image]).is_none());
        assert!(single_file_image_path(&[entry(r"C:\sample.txt", Some(2_000))]).is_none());
        assert!(single_file_image_path(&[entry(r"C:\sample.png", None)]).is_none());
        assert!(
            single_file_image_path(&[entry(r"C:\sample.png", Some(50 * 1024 * 1024 + 1))])
                .is_none()
        );
        assert!(
            single_file_image_path(&[entry(
                r"\\server\share\sample.png",
                Some(10 * 1024 * 1024 + 1)
            )])
            .is_none()
        );
    }

    #[test]
    fn missing_unreadable_and_directory_paths_keep_file_details() {
        let mut image = entry(r"C:\sample.png", Some(2_000));
        image.exists = false;
        assert!(single_file_image_path(std::slice::from_ref(&image)).is_none());
        image.exists = true;
        image.is_dir = true;
        assert!(single_file_image_path(std::slice::from_ref(&image)).is_none());
        image.is_dir = false;
        image.metadata_error = Some("unavailable".into());
        assert!(single_file_image_path(&[image]).is_none());
    }

    #[test]
    fn card_file_probe_reports_missing_and_oversized_paths() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("sample.png");
        std::fs::write(&image, b"small synthetic payload").unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![image.clone()]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert_eq!(checked.kind, FileCardKind::File);
        assert_eq!(checked.image_path, Some(image));
        assert_eq!(checked.total_size, Some(23));
        let missing = dir.path().join("missing.png");
        let checked = inspect_file_card(&serde_json::to_string(&vec![missing]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Missing);
        assert!(checked.image_path.is_none());
        assert_eq!(checked.total_size, None);
        assert_eq!(
            inspect_file_card("not json").availability,
            FileCardAvailability::Unreadable
        );
        let folder = dir.path().join("folder.png");
        std::fs::create_dir(&folder).unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![folder.clone()]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert_eq!(checked.kind, FileCardKind::Folder);
        assert!(checked.image_path.is_none());
        assert_eq!(checked.total_size, None);
        let mixed = serde_json::to_string(&vec![folder, dir.path().join("missing.png")]);
        let checked = inspect_file_card(&mixed.unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Missing);
        assert_eq!(checked.kind, FileCardKind::Multiple);
        assert_eq!(checked.total_size, None);
        let large = dir.path().join("large.png");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(50 * 1024 * 1024 + 1)
            .unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![large]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert!(checked.image_path.is_none());
        assert!(checked.image_too_large);
    }
}

fn hotkey_label(language: LanguagePreference, choice: HotkeyPreference) -> &'static str {
    if choice == HotkeyPreference::Disabled {
        tr(language, "关闭", "Disabled")
    } else {
        choice.label()
    }
}

fn shortcut_from_keystroke(keystroke: &Keystroke, shift_down: bool) -> Result<String, String> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.function {
        return Err("快速粘贴不支持 Win 或 Fn 修饰键".into());
    }
    if !modifiers.control && !modifiers.alt {
        return Err("快速粘贴快捷键至少需要 Ctrl 或 Alt".into());
    }
    let key = if shift_down {
        match keystroke.key.as_str() {
            "!" => "1",
            "@" => "2",
            "#" => "3",
            "$" => "4",
            "%" => "5",
            "^" => "6",
            "&" => "7",
            "*" => "8",
            "(" => "9",
            ")" => "0",
            key => key,
        }
    } else {
        keystroke.key.as_str()
    };
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if shift_down {
        parts.push("Shift");
    }
    parts.push(key);
    normalize_paste_shortcut(&parts.join("+")).map_err(|error| error.to_string())
}

#[cfg(test)]
mod shortcut_capture_tests {
    use super::shortcut_from_keystroke;
    use gpui_kit::{Keystroke, Modifiers};

    #[test]
    fn gpui_keystrokes_become_paste_shortcuts() {
        let stroke = Keystroke {
            modifiers: Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
            key: "z".into(),
            ..Default::default()
        };
        assert_eq!(
            shortcut_from_keystroke(&stroke, false).unwrap(),
            "Ctrl+Alt+Z"
        );
        let shifted_digit = Keystroke {
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
            key: "!".into(),
            ..Default::default()
        };
        assert_eq!(
            shortcut_from_keystroke(&shifted_digit, true).unwrap(),
            "Alt+Shift+1"
        );
        assert!(shortcut_from_keystroke(&stroke, true).is_ok());
        let bare = Keystroke {
            key: "z".into(),
            ..Default::default()
        };
        assert!(shortcut_from_keystroke(&bare, false).is_err());
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
        PreviousCategory,
        NextCategory,
        PreviousGroup,
        NextGroup,
        First,
        Last,
        PageUp,
        PageDown,
        SelectAllLoaded,
        ActivateSelected,
        PastePlainTextSelected,
        PasteSelected,
        DeleteSelected,
        FocusSearch,
        FocusFirstHistoryItem,
        PreviewSelected,
        ClosePreview,
        DismissOrHide
    ]
);

#[derive(Clone, Copy)]
struct StartupMode {
    monitoring: bool,
    hidden: bool,
    show_onboarding: bool,
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
                    let close_view = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        if exiting.get() || !tray_enabled.get() {
                            return true;
                        }
                        let can_hide = close_view.update(cx, |this, cx| {
                            if this.batch_pending
                                || this.paste_pending.is_some()
                                || this.batch_paste_pending.is_some()
                            {
                                return false;
                            }
                            this.prepare_to_hide(window, cx);
                            true
                        });
                        if !can_hide {
                            return false;
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

#[derive(Clone)]
struct ToolbarDrag {
    button: ToolbarButton,
    label: &'static str,
    toolbar: ToolbarPreference,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropTarget {
    id: i64,
    after: bool,
    allowed: bool,
}

enum HistoryMenuAction {
    Paste,
    PastePlainText,
    Copy,
    CopyPath,
    Preview,
    Edit,
    Reveal,
    SaveAs(String),
    ToggleFavorite,
    TogglePin,
    MoveToGroup,
    Delete,
}

fn history_menu_item(
    label: &'static str,
    disabled: bool,
    view: Entity<ClipboardView>,
    id: i64,
    action: HistoryMenuAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            view.update(cx, |this, cx| {
                this.perform_history_menu_action(id, &action, window, cx);
            });
        })
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

impl Render for ToolbarDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().primary)
            .bg(cx.theme().background)
            .shadow_md()
            .text_sm()
            .child(format!("⠿  {}", self.label))
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
    paste_hotkeys: Option<PasteHotkeys>,
    paste_hotkey_sender: async_channel::Sender<PasteHotkeyEvent>,
    _paste_hotkey_events: Task<()>,
    _outside_click_monitor: Option<OutsideClickMonitor>,
    _outside_click_events: Task<()>,
    exiting: Rc<Cell<bool>>,
    history: HistoryState,
    file_card_info: HashMap<i64, FileCardInfo>,
    file_card_checked: HashMap<i64, String>,
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
    batch_mode: bool,
    selected_ids: HashSet<i64>,
    selection_anchor: Option<i64>,
    batch_confirm_open: bool,
    batch_pending: bool,
    batch_paste_pending: Option<(isize, u32)>,
    group_move_id: Option<i64>,
    group_move_pending: bool,
    preview: PreviewState,
    preview_input: Entity<TextareaState>,
    preview_source_hash: Option<String>,
    preview_editing: bool,
    preview_edit_requested: bool,
    preview_save_pending: bool,
    image_zoom_percent: u16,
    hover_preview: PreviewState,
    hover_popup: Option<AnyWindowHandle>,
    hover_popup_opening: Option<(i64, u64)>,
    hover_source_active: bool,
    hover_popup_active: bool,
    hover_task: Option<Task<()>>,
    hover_close_task: Option<Task<()>>,
    hover_preference: HoverPreviewPreference,
    hover_preference_pending: bool,
    pending_row_click: Option<(i64, u64)>,
    row_click_task: Option<Task<()>>,
    save_as_pending: Option<i64>,
    list_focus: FocusHandle,
    scroll: UniformListScrollHandle,
    monitoring: bool,
    onboarding_completed: bool,
    onboarding_step: usize,
    onboarding_pending: bool,
    show_onboarding: bool,
    settings_window: Option<AnyWindowHandle>,
    settings_window_opening: bool,
    theme: ThemePreference,
    theme_pending: bool,
    language: LanguagePreference,
    language_pending: bool,
    hotkey_choice: HotkeyPreference,
    hotkey_pending: bool,
    quick_paste_enabled: bool,
    quick_paste_pending_setting: bool,
    quick_paste_registration_warning: Option<String>,
    paste_shortcuts: PasteShortcutConfig,
    paste_shortcuts_pending: Option<PasteShortcutConfig>,
    paste_shortcut_status: Option<(String, bool)>,
    quick_paste_pending: Option<(u8, bool, (isize, u32))>,
    autostart: bool,
    autostart_pending: bool,
    data_size: Option<DataSizeInfo>,
    data_size_pending: bool,
    database_maintenance_pending: bool,
    export_pending: bool,
    window_pinned: bool,
    window_position: WindowPositionPreference,
    window_position_pending: bool,
    persist_window_size: bool,
    persist_window_size_pending: bool,
    auto_reset_state: bool,
    auto_reset_state_pending: bool,
    search_auto_focus: bool,
    search_auto_focus_pending: bool,
    search_auto_clear: bool,
    search_auto_clear_pending: bool,
    skip_clear_confirm: bool,
    skip_clear_confirm_pending: bool,
    paste_close_window: bool,
    paste_close_window_pending: bool,
    paste_key: PasteKeyPreference,
    paste_key_pending: bool,
    paste_move_to_top: bool,
    paste_move_to_top_pending: bool,
    toolbar: ToolbarPreference,
    toolbar_pending: bool,
    display: DisplayPreference,
    display_pending: bool,
    audio: AudioPreference,
    audio_pending: bool,
    monitor_types: MonitorTypesPreference,
    monitor_types_pending: bool,
    app_filter: AppFilterPreference,
    app_filter_pending: bool,
    running_apps: Vec<RunningApp>,
    running_apps_pending: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    General,
    Display,
    Theme,
    Data,
    AppFilter,
    Audio,
    Shortcuts,
    About,
}

impl SettingsPage {
    const ALL: [Self; 8] = [
        Self::General,
        Self::Display,
        Self::Theme,
        Self::Data,
        Self::AppFilter,
        Self::Audio,
        Self::Shortcuts,
        Self::About,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Display => "display",
            Self::Theme => "theme",
            Self::Data => "data",
            Self::AppFilter => "app-filter",
            Self::Audio => "audio",
            Self::Shortcuts => "shortcuts",
            Self::About => "about",
        }
    }

    fn label(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::General => tr(language, "常规", "General"),
            Self::Display => tr(language, "显示", "Display"),
            Self::Theme => tr(language, "外观", "Appearance"),
            Self::Data => tr(language, "数据", "Data"),
            Self::AppFilter => tr(language, "应用过滤", "App filter"),
            Self::Audio => tr(language, "音效", "Audio"),
            Self::Shortcuts => tr(language, "快捷键", "Shortcuts"),
            Self::About => tr(language, "关于", "About"),
        }
    }
}

struct SettingsWindowView {
    owner: WeakEntity<ClipboardView>,
    _owner_subscription: Subscription,
    page: SettingsPage,
    shortcut_input: Option<Entity<InputState>>,
    shortcut_editing: Option<(bool, u8)>,
    shortcut_capture_focus: FocusHandle,
    shortcut_recording: bool,
    shortcut_capture_error: Option<String>,
    recent_shortcuts_expanded: bool,
    favorite_shortcuts_expanded: bool,
    app_filter_input: Entity<InputState>,
    _app_filter_input_subscription: Subscription,
    app_picker_open: bool,
    app_filter_error: bool,
    link_error: Option<String>,
    toolbar_drop_target: Option<(ToolbarButton, bool)>,
}

struct HoverPreviewWindowView {
    owner: WeakEntity<ClipboardView>,
    id: i64,
    generation: u64,
    language: LanguagePreference,
    content: Result<PreviewContent, String>,
    text_input: Entity<TextareaState>,
    zoom_percent: u16,
    zoom_step: u8,
}

impl HoverPreviewWindowView {
    fn change_zoom(&mut self, change: i32, cx: &mut Context<Self>) {
        let next = hover_zoom_percent(self.zoom_percent, change);
        if next != self.zoom_percent {
            self.zoom_percent = next;
            cx.notify();
        }
    }

    fn new(
        owner: WeakEntity<ClipboardView>,
        request: (i64, u64),
        language: LanguagePreference,
        content: Result<PreviewContent, String>,
        zoom_step: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = match &content {
            Ok(PreviewContent::Text(text) | PreviewContent::RichText(text)) => text.clone(),
            Ok(PreviewContent::Files(entries)) => entries
                .iter()
                .map(|entry| entry.original_path.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => error.clone(),
            Ok(PreviewContent::Image(_)) => String::new(),
        };
        let text_input = cx.new(|cx| TextareaState::new(window, cx));
        text_input.update(cx, |input, cx| input.set_value(text, window, cx));
        Self {
            owner,
            id: request.0,
            generation: request.1,
            language,
            content,
            text_input,
            zoom_percent: 100,
            zoom_step,
        }
    }
}

impl Render for HoverPreviewWindowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match &self.content {
            Ok(PreviewContent::Text(_)) => tr(self.language, "文本预览", "Text preview"),
            Ok(PreviewContent::RichText(_)) => tr(self.language, "富文本预览", "Rich-text preview"),
            Ok(PreviewContent::Image(_)) => tr(self.language, "图片预览", "Image preview"),
            Ok(PreviewContent::Files(_)) => tr(self.language, "文件预览", "File preview"),
            Err(_) => tr(self.language, "预览失败", "Preview unavailable"),
        };
        let image_preview = matches!(&self.content, Ok(PreviewContent::Image(_)))
            || matches!(
                &self.content,
                Ok(PreviewContent::Files(entries)) if single_file_image_path(entries).is_some()
            );
        let body: AnyElement = match &self.content {
            Ok(PreviewContent::Image(path)) => div()
                .id("hover-image-viewport")
                .size_full()
                .overflow_scroll()
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                    if event.modifiers.control {
                        let delta = event.delta.pixel_delta(window.line_height()).y;
                        let change = if delta > px(0.) {
                            i32::from(this.zoom_step)
                        } else {
                            -i32::from(this.zoom_step)
                        };
                        this.change_zoom(change, cx);
                        cx.stop_propagation();
                    }
                }))
                .when(self.zoom_percent <= 100, |viewport| {
                    viewport.flex().items_center().justify_center()
                })
                .child(
                    div()
                        .w(relative(f32::from(self.zoom_percent) / 100.0))
                        .h(relative(f32::from(self.zoom_percent) / 100.0))
                        .flex_none()
                        .child(img(path.clone()).size_full().object_fit(ObjectFit::Contain)),
                )
                .into_any_element(),
            Ok(PreviewContent::Files(entries)) if single_file_image_path(entries).is_some() => {
                let path = single_file_image_path(entries).expect("checked above");
                let unavailable = tr(self.language, "图片无法显示", "Image unavailable");
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("hover-file-image-viewport")
                            .flex_1()
                            .min_h_0()
                            .overflow_scroll()
                            .on_scroll_wheel(cx.listener(
                                |this, event: &ScrollWheelEvent, window, cx| {
                                    if event.modifiers.control {
                                        let delta = event.delta.pixel_delta(window.line_height()).y;
                                        let change = if delta > px(0.) {
                                            i32::from(this.zoom_step)
                                        } else {
                                            -i32::from(this.zoom_step)
                                        };
                                        this.change_zoom(change, cx);
                                        cx.stop_propagation();
                                    }
                                },
                            ))
                            .when(self.zoom_percent <= 100, |viewport| {
                                viewport.flex().items_center().justify_center()
                            })
                            .child(
                                div()
                                    .w(relative(f32::from(self.zoom_percent) / 100.0))
                                    .h(relative(f32::from(self.zoom_percent) / 100.0))
                                    .flex_none()
                                    .child(
                                        img(path)
                                            .size_full()
                                            .object_fit(ObjectFit::Contain)
                                            .with_fallback(move || {
                                                div().child(unavailable).into_any_element()
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .line_clamp(1)
                            .text_ellipsis()
                            .child(entries[0].original_path.clone()),
                    )
                    .into_any_element()
            }
            _ => Textarea::new(&self.text_input)
                .readonly(true)
                .size_full()
                .aria_label(title)
                .into_any_element(),
        };
        let owner = self.owner.clone();
        let id = self.id;
        let generation = self.generation;
        div()
            .id("hover-preview-window")
            .size_full()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .shadow_lg()
            .on_hover(move |hovered, _, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.set_hover_popup_active(id, generation, *hovered, cx);
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_sm()
                    .font_semibold()
                    .child(title)
                    .when(image_preview, |header| {
                        header.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("hover-zoom-out")
                                        .xsmall()
                                        .ghost()
                                        .label("−")
                                        .accessibility_label(tr(
                                            self.language,
                                            "缩小悬浮图片",
                                            "Zoom out hover image",
                                        ))
                                        .disabled(self.zoom_percent <= 50)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.change_zoom(-i32::from(this.zoom_step), cx);
                                        })),
                                )
                                .child(
                                    Button::new("hover-zoom-reset")
                                        .xsmall()
                                        .ghost()
                                        .label(format!("{}%", self.zoom_percent))
                                        .accessibility_label(tr(
                                            self.language,
                                            "重置悬浮图片缩放",
                                            "Reset hover image zoom",
                                        ))
                                        .disabled(self.zoom_percent == 100)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_percent = 100;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("hover-zoom-in")
                                        .xsmall()
                                        .ghost()
                                        .label("+")
                                        .accessibility_label(tr(
                                            self.language,
                                            "放大悬浮图片",
                                            "Zoom in hover image",
                                        ))
                                        .disabled(self.zoom_percent >= 400)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.change_zoom(i32::from(this.zoom_step), cx);
                                        })),
                                ),
                        )
                    }),
            )
            .child(div().flex_1().min_h_0().child(body))
    }
}

impl SettingsWindowView {
    fn new(owner: WeakEntity<ClipboardView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let entity = owner
            .upgrade()
            .expect("settings window requires its owner view");
        let subscription = cx.observe(&entity, |_, _, cx| cx.notify());
        let app_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("notepad.exe or *browser*"));
        let app_filter_input_subscription =
            cx.subscribe_in(&app_filter_input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.add_app_filter_rule(window, cx);
                }
            });
        Self {
            owner,
            _owner_subscription: subscription,
            page: SettingsPage::General,
            shortcut_input: None,
            shortcut_editing: None,
            shortcut_capture_focus: cx.focus_handle(),
            shortcut_recording: false,
            shortcut_capture_error: None,
            recent_shortcuts_expanded: false,
            favorite_shortcuts_expanded: false,
            app_filter_input,
            _app_filter_input_subscription: app_filter_input_subscription,
            app_picker_open: false,
            app_filter_error: false,
            link_error: None,
            toolbar_drop_target: None,
        }
    }

    fn add_app_filter_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = self.app_filter_input.read(cx).value().trim().to_owned();
        if rule.is_empty() {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let accepted = owner.update(cx, |owner, cx| {
            let Some(next) = owner.app_filter.clone().with_rule(&rule) else {
                owner.message = tr(
                    owner.language,
                    "规则无效或已达到上限",
                    "Invalid rule or rule limit reached",
                )
                .into();
                owner.is_error = true;
                cx.notify();
                return false;
            };
            if next == owner.app_filter {
                return true;
            }
            owner.save_app_filter(next, cx);
            owner.app_filter_pending
        });
        if accepted {
            self.app_filter_error = false;
            self.app_filter_input
                .update(cx, |input, cx| input.set_value(String::new(), window, cx));
            cx.notify();
        } else {
            self.app_filter_error = true;
            cx.notify();
        }
    }

    fn begin_shortcut_edit(
        &mut self,
        favorite: bool,
        slot: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let shortcut = owner
            .read(cx)
            .paste_shortcuts
            .slot(favorite, slot)
            .unwrap_or_default()
            .to_owned();
        let shortcut_input = self.shortcut_input.get_or_insert_with(|| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Ctrl+Alt+1"))
        });
        shortcut_input.update(cx, |input, cx| input.set_value(shortcut, window, cx));
        self.shortcut_editing = Some((favorite, slot));
        self.shortcut_recording = false;
        self.shortcut_capture_error = None;
        shortcut_input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn save_shortcut_edit(&mut self, cx: &mut Context<Self>) {
        let Some((favorite, slot)) = self.shortcut_editing else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(shortcut_input) = &self.shortcut_input else {
            return;
        };
        let shortcut = shortcut_input.read(cx).value().to_string();
        let accepted = owner.update(cx, |owner, cx| {
            owner.save_paste_shortcut(favorite, slot, &shortcut, cx);
            owner.paste_shortcuts_pending.is_some()
        });
        if accepted {
            self.shortcut_editing = None;
            self.shortcut_recording = false;
            cx.notify();
        }
    }

    fn start_shortcut_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcut_recording = true;
        self.shortcut_capture_error = None;
        self.shortcut_capture_focus.focus(window, cx);
        cx.notify();
    }

    fn capture_shortcut_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.shortcut_recording || event.is_held {
            return;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape")
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.platform
        {
            self.shortcut_recording = false;
            self.shortcut_capture_error = None;
            cx.notify();
            return;
        }
        let shift_down = event.keystroke.modifiers.shift
            || unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) } < 0;
        match shortcut_from_keystroke(&event.keystroke, shift_down) {
            Ok(shortcut) => {
                if let Some(input) = &self.shortcut_input {
                    input.update(cx, |input, cx| input.set_value(shortcut, window, cx));
                }
                self.shortcut_recording = false;
                self.shortcut_capture_error = None;
            }
            Err(error) => self.shortcut_capture_error = Some(error),
        }
        cx.notify();
    }

    fn shortcut_editor(
        &self,
        favorite: bool,
        slot: u8,
        language: LanguagePreference,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(format!(
                "{} {slot}: {}",
                tr(
                    language,
                    if favorite { "收藏" } else { "普通" },
                    if favorite { "Favorite" } else { "Recent" }
                ),
                tr(
                    language,
                    "输入组合键，例如 Ctrl+Alt+Z",
                    "Enter a shortcut, for example Ctrl+Alt+Z"
                )
            ))
            .child(Input::new(
                self.shortcut_input.as_ref().expect("editing input exists"),
            ))
            .when(self.shortcut_recording, |panel| {
                panel.child(
                    div()
                        .id("paste-shortcut-recorder")
                        .track_focus(&self.shortcut_capture_focus)
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().accent)
                        .p_2()
                        .child(tr(
                            language,
                            "请按组合键；按 Esc 取消录制",
                            "Press a shortcut; press Esc to cancel recording",
                        ))
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            this.capture_shortcut_key(event, window, cx);
                        })),
                )
            })
            .when_some(self.shortcut_capture_error.clone(), |panel, error| {
                panel.child(div().text_xs().text_color(cx.theme().danger).child(error))
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("paste-shortcut-record")
                            .small()
                            .outline()
                            .label(if self.shortcut_recording {
                                tr(language, "录制中", "Recording")
                            } else {
                                tr(language, "录制", "Record")
                            })
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_shortcut_recording(window, cx);
                            })),
                    )
                    .child(
                        Button::new("paste-shortcut-save")
                            .small()
                            .label(tr(language, "保存", "Save"))
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_shortcut_edit(cx);
                            })),
                    )
                    .child(
                        Button::new("paste-shortcut-cancel")
                            .small()
                            .ghost()
                            .label(tr(language, "取消", "Cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.shortcut_editing = None;
                                this.shortcut_recording = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn paste_slot_rows(
        &self,
        favorite: bool,
        shortcuts: &PasteShortcutConfig,
        pending: bool,
        language: LanguagePreference,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children((1..=10u8).map(|slot| {
                let shortcut = shortcuts.slot(favorite, slot).unwrap_or_default();
                let shortcut_label = if shortcut.is_empty() {
                    tr(language, "未设置", "Not set").to_owned()
                } else {
                    shortcut.to_owned()
                };
                let default = PasteShortcutConfig::default()
                    .slot(favorite, slot)
                    .unwrap_or_default()
                    .to_owned();
                let owner_disable = self.owner.clone();
                let owner_reset = self.owner.clone();
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(96.)).text_xs().child(format!(
                                "{} {slot}",
                                tr(
                                    language,
                                    if favorite { "收藏" } else { "普通" },
                                    if favorite { "Favorite" } else { "Recent" }
                                )
                            )))
                            .child(div().text_sm().child(shortcut_label)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new(format!("paste-slot-edit-{favorite}-{slot}"))
                                    .outline()
                                    .small()
                                    .label(tr(language, "修改", "Edit"))
                                    .disabled(pending)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.begin_shortcut_edit(favorite, slot, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(format!("paste-slot-disable-{favorite}-{slot}"))
                                    .ghost()
                                    .small()
                                    .label(tr(language, "停用", "Disable"))
                                    .disabled(pending || shortcut.is_empty())
                                    .on_click(move |_, _, cx| {
                                        let _ = owner_disable.update(cx, |owner, cx| {
                                            owner.save_paste_shortcut(favorite, slot, "", cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new(format!("paste-slot-reset-{favorite}-{slot}"))
                                    .ghost()
                                    .small()
                                    .label(tr(language, "默认", "Default"))
                                    .disabled(pending || shortcut == default)
                                    .on_click(move |_, _, cx| {
                                        let _ = owner_reset.update(cx, |owner, cx| {
                                            owner.save_paste_shortcut(favorite, slot, &default, cx);
                                        });
                                    }),
                            ),
                    )
                    .when(self.shortcut_editing == Some((favorite, slot)), |row| {
                        row.child(self.shortcut_editor(favorite, slot, language, pending, cx))
                    })
            }))
    }

    fn paste_group_actions(
        &self,
        favorite: bool,
        shortcuts: &PasteShortcutConfig,
        pending: bool,
        language: LanguagePreference,
    ) -> Div {
        let defaults = PasteShortcutConfig::default();
        let (current, defaults) = if favorite {
            (&shortcuts.favorites, &defaults.favorites)
        } else {
            (&shortcuts.recent, &defaults.recent)
        };
        let reset_owner = self.owner.clone();
        let disable_owner = self.owner.clone();
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new(format!("paste-{favorite}-reset-all"))
                    .outline()
                    .small()
                    .label(tr(language, "全部恢复默认", "Restore all defaults"))
                    .disabled(pending || current == defaults)
                    .on_click(move |_, _, cx| {
                        let _ = reset_owner.update(cx, |owner, cx| {
                            owner.set_paste_shortcut_group_defaults(favorite, true, cx);
                        });
                    }),
            )
            .child(
                Button::new(format!("paste-{favorite}-disable-all"))
                    .outline()
                    .small()
                    .label(tr(language, "全部停用", "Disable all"))
                    .disabled(pending || current.iter().all(String::is_empty))
                    .on_click(move |_, _, cx| {
                        let _ = disable_owner.update(cx, |owner, cx| {
                            owner.set_paste_shortcut_group_defaults(favorite, false, cx);
                        });
                    }),
            )
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
        let window_position = service.initial_window_position;
        let hover_preference = service.initial_hover_preview;
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr(
                language,
                "搜索剪贴板历史…",
                "Search clipboard history…",
            ))
        });
        let subscription = cx.subscribe_in(&search, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.cancel_pending_row_click();
                this.close_hover_preview(cx);
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
            } else if matches!(event, InputEvent::Focus) {
                this.cancel_pending_row_click();
            } else if matches!(event, InputEvent::PressEnter { .. })
                && !this.batch_mode
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
                            this.show_window(window, cx);
                            cx.notify();
                        }
                        TrayCommand::Toggle => {
                            if tray::is_window_shown(window) && !tray::is_window_minimized(window) {
                                this.hide_visible_window(window, cx);
                            } else {
                                this.paste_target = None;
                                this.show_window(window, cx);
                                cx.notify();
                            }
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
                        if tray::is_window_shown(window) && !tray::is_window_minimized(window) {
                            this.hide_visible_window(window, cx);
                            return;
                        }
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
                if view
                    .update_in(cx, |this, window, cx| {
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
                if view
                    .update_in(cx, |this, window, cx| {
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
        if !paste_hotkey_warnings.is_empty() {
            startup_errors.push(format!(
                "部分快速粘贴快捷键不可用：{}",
                paste_hotkey_warnings
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
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
        let toolbar = service.initial_toolbar;
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
        apply_theme(theme, window, cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme == ThemePreference::System {
                apply_theme(this.theme, window, cx);
            }
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.cancel_pending_row_click();
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
            batch_mode: false,
            selected_ids: HashSet::new(),
            selection_anchor: None,
            batch_confirm_open: false,
            batch_pending: false,
            batch_paste_pending: None,
            group_move_id: None,
            group_move_pending: false,
            preview: PreviewState::default(),
            preview_input: cx.new(|cx| TextareaState::new(window, cx)),
            preview_source_hash: None,
            preview_editing: false,
            preview_edit_requested: false,
            preview_save_pending: false,
            image_zoom_percent: 100,
            hover_preview: PreviewState::default(),
            hover_popup: None,
            hover_popup_opening: None,
            hover_source_active: false,
            hover_popup_active: false,
            hover_task: None,
            hover_close_task: None,
            hover_preference,
            hover_preference_pending: false,
            pending_row_click: None,
            row_click_task: None,
            save_as_pending: None,
            list_focus,
            scroll: UniformListScrollHandle::new(),
            monitoring: startup.monitoring,
            onboarding_completed,
            onboarding_step: 0,
            onboarding_pending: false,
            show_onboarding: startup.show_onboarding,
            settings_window: None,
            settings_window_opening: false,
            theme,
            theme_pending: false,
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
            autostart: autostart.unwrap_or(false),
            autostart_pending: false,
            data_size: None,
            data_size_pending: false,
            database_maintenance_pending: false,
            export_pending: false,
            window_pinned: false,
            window_position,
            window_position_pending: false,
            persist_window_size,
            persist_window_size_pending: false,
            auto_reset_state,
            auto_reset_state_pending: false,
            search_auto_focus,
            search_auto_focus_pending: false,
            search_auto_clear,
            search_auto_clear_pending: false,
            skip_clear_confirm,
            skip_clear_confirm_pending: false,
            paste_close_window,
            paste_close_window_pending: false,
            paste_key,
            paste_key_pending: false,
            paste_move_to_top,
            paste_move_to_top_pending: false,
            toolbar,
            toolbar_pending: false,
            display,
            display_pending: false,
            audio,
            audio_pending: false,
            monitor_types,
            monitor_types_pending: false,
            app_filter,
            app_filter_pending: false,
            running_apps: Vec::new(),
            running_apps_pending: false,
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
        let copy_requested = matches!(
            &command,
            Command::Copy(_)
                | Command::CopyPlainText(_)
                | Command::CopyPlainTextForPaste(_)
                | Command::CopyForPaste(_)
                | Command::CopyPath(_)
                | Command::CopyPathForPaste(_)
                | Command::MergeCopy(_)
                | Command::MergeForPaste(_)
        );
        if let Err(error) = self.service.send(command) {
            self.message = error.to_string();
            self.is_error = true;
            cx.notify();
            return false;
        }
        if copy_requested && sound::enabled(self.audio, Sound::Copy, SoundTiming::Immediate) {
            sound::play(Sound::Copy);
        }
        true
    }

    fn window_bounds_changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.persist_window_size || self.persist_window_size_pending {
            return;
        }
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
                if this.persist_window_size && !this.persist_window_size_pending {
                    this.send(Command::SetWindowSize(size), cx);
                }
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
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
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
                let settings =
                    cx.new(|cx| SettingsWindowView::new(settings_owner.clone(), window, cx));
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
            || self.paste_shortcuts_pending.is_some()
            || (choice == self.hotkey_choice
                && (choice == HotkeyPreference::Disabled || self.hotkey.is_some()))
        {
            return;
        }
        if let Err(error) = validate_paste_shortcuts(&self.paste_shortcuts, choice) {
            self.message = error.to_string();
            self.is_error = true;
            cx.notify();
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

    fn select_quick_paste_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if !self.monitoring
            || self.quick_paste_pending_setting
            || self.paste_shortcuts_pending.is_some()
            || enabled == self.quick_paste_enabled
        {
            return;
        }
        if enabled {
            match PasteHotkeys::start(
                self.paste_hotkey_sender.clone(),
                &self.paste_shortcuts,
                self.hotkey_choice,
            ) {
                Ok((hotkeys, warnings)) => {
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning = (!warnings.is_empty()).then(|| {
                        format!(
                            "{}: {}",
                            tr(
                                self.language,
                                "部分快速粘贴快捷键不可用",
                                "Some quick paste shortcuts are unavailable",
                            ),
                            warnings
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("; ")
                        )
                    });
                }
                Err(error) => {
                    self.message = error.to_string();
                    self.is_error = true;
                    cx.notify();
                    return;
                }
            }
        } else {
            self.paste_hotkeys = None;
            self.quick_paste_registration_warning = None;
        }
        if self.send(Command::SetQuickPasteEnabled(enabled), cx) {
            self.quick_paste_pending_setting = true;
        } else if enabled {
            self.paste_hotkeys = None;
            self.quick_paste_registration_warning = None;
        } else if let Ok((hotkeys, _)) = PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            &self.paste_shortcuts,
            self.hotkey_choice,
        ) {
            self.paste_hotkeys = Some(hotkeys);
        }
        cx.notify();
    }

    fn restore_paste_hotkeys(&mut self) -> Option<String> {
        if !self.monitoring || !self.quick_paste_enabled {
            return None;
        }
        match PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            &self.paste_shortcuts,
            self.hotkey_choice,
        ) {
            Ok((hotkeys, warnings)) => {
                self.paste_hotkeys = Some(hotkeys);
                (!warnings.is_empty()).then(|| {
                    warnings
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                })
            }
            Err(error) => Some(error.to_string()),
        }
    }

    fn save_paste_shortcut(
        &mut self,
        favorite: bool,
        slot: u8,
        shortcut: &str,
        cx: &mut Context<Self>,
    ) {
        let next = (|| {
            let normalized = normalize_paste_shortcut(shortcut)?;
            let mut next = self.paste_shortcuts.clone();
            next.set_slot(favorite, slot, normalized)?;
            Ok::<_, anyhow::Error>(next)
        })();
        match next {
            Ok(next) => self.save_paste_shortcuts(next, cx),
            Err(error) => {
                self.message = error.to_string();
                self.is_error = true;
                self.paste_shortcut_status = Some((self.message.clone(), true));
                cx.notify();
            }
        }
    }

    fn save_paste_shortcuts(&mut self, next: PasteShortcutConfig, cx: &mut Context<Self>) {
        if self.paste_shortcuts_pending.is_some()
            || self.quick_paste_pending_setting
            || self.hotkey_pending
        {
            return;
        }
        self.paste_shortcut_status = None;
        if let Err(error) = validate_paste_shortcuts(&next, self.hotkey_choice) {
            self.message = error.to_string();
            self.is_error = true;
            self.paste_shortcut_status = Some((self.message.clone(), true));
            cx.notify();
            return;
        }
        if next == self.paste_shortcuts {
            return;
        }
        self.paste_hotkeys = None;
        if self.monitoring && self.quick_paste_enabled {
            match PasteHotkeys::start(self.paste_hotkey_sender.clone(), &next, self.hotkey_choice) {
                Ok((hotkeys, warnings)) => {
                    let changed_slot_failed = warnings.iter().any(|warning| {
                        next.slot(warning.favorite, warning.slot)
                            != self.paste_shortcuts.slot(warning.favorite, warning.slot)
                    });
                    if changed_slot_failed {
                        drop(hotkeys);
                        let restore_error = self.restore_paste_hotkeys();
                        self.message = warnings
                            .iter()
                            .filter(|warning| {
                                next.slot(warning.favorite, warning.slot)
                                    != self.paste_shortcuts.slot(warning.favorite, warning.slot)
                            })
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("; ");
                        if let Some(error) = restore_error {
                            self.message.push_str(&format!("; {error}"));
                        }
                        self.is_error = true;
                        self.paste_shortcut_status = Some((self.message.clone(), true));
                        cx.notify();
                        return;
                    }
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning = (!warnings.is_empty()).then(|| {
                        warnings
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("; ")
                    });
                }
                Err(error) => {
                    let restore_error = self.restore_paste_hotkeys();
                    self.message = error.to_string();
                    if let Some(error) = restore_error {
                        self.message.push_str(&format!("; {error}"));
                    }
                    self.is_error = true;
                    self.paste_shortcut_status = Some((self.message.clone(), true));
                    cx.notify();
                    return;
                }
            }
        }
        let previous = self.paste_shortcuts.clone();
        if self.send(Command::SetPasteShortcuts(Box::new(next.clone())), cx) {
            self.paste_shortcuts = next;
            self.paste_shortcuts_pending = Some(previous);
        } else {
            self.paste_hotkeys = None;
            if let Some(error) = self.restore_paste_hotkeys() {
                self.message.push_str(&format!("; {error}"));
            }
            self.paste_shortcut_status = Some((self.message.clone(), true));
            self.quick_paste_registration_warning = None;
        }
        cx.notify();
    }

    fn set_paste_shortcut_group_defaults(
        &mut self,
        favorite: bool,
        reset: bool,
        cx: &mut Context<Self>,
    ) {
        let mut next = self.paste_shortcuts.clone();
        let defaults = PasteShortcutConfig::default();
        let group = if favorite {
            &mut next.favorites
        } else {
            &mut next.recent
        };
        for (index, shortcut) in group.iter_mut().enumerate() {
            *shortcut = if reset {
                if favorite {
                    defaults.favorites[index].clone()
                } else {
                    defaults.recent[index].clone()
                }
            } else {
                String::new()
            };
        }
        self.save_paste_shortcuts(next, cx);
    }

    fn handle_quick_hotkey(
        &mut self,
        event: PasteHotkeyEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring
            || !self.quick_paste_enabled
            || self.paste_hotkeys.is_none()
            || self.quick_paste_pending.is_some()
            || self.paste_pending.is_some()
            || self.batch_pending
            || self.batch_paste_pending.is_some()
            || !paste::is_external_target(window, event.target)
        {
            return;
        }
        if self.send(
            Command::ResolveQuickPaste {
                slot: event.slot,
                favorite: event.favorite,
                group_id: self.history.group_id,
            },
            cx,
        ) {
            self.quick_paste_pending = Some((event.slot, event.favorite, event.target));
        }
    }

    fn query(&mut self, cx: &mut Context<Self>) {
        let generation = self.history.generation;
        if !self.send(
            Command::Query {
                search: self.search.read(cx).value().to_string(),
                limit: self.history.limit,
                favorite_only: self.history.favorite_only,
                category: self.history.category,
                group_id: self.history.group_id,
                generation,
            },
            cx,
        ) {
            self.history.fail_query(generation);
        }
    }

    fn select_group(&mut self, group_id: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.group_id == group_id
            && !self.history.favorite_only
            && self.history.category == ContentCategory::All
        {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
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

    fn select_category(
        &mut self,
        category: Option<ContentCategory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_delete_pending || self.clear_pending || self.group_reorder_pending {
            return;
        }
        let selected = match category {
            None => self.history.favorite_only && self.history.group_id.is_none(),
            Some(category) => {
                !self.history.favorite_only
                    && self.history.category == category
                    && self.history.group_id.is_none()
            }
        };
        if selected {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.clear_confirm_open = false;
        self.group_move_id = None;
        self.group_delete_id = None;
        self.reset_selection();
        self.search_task = None;
        if let Some(category) = category {
            self.history.set_category(category);
        } else {
            self.history.set_favorite_filter(true);
        }
        self.group_scroll.scroll_to_item(4);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.query(cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn select_adjacent_category(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.display.show_category_filter {
            return;
        }
        let current: usize = if self.history.favorite_only {
            1
        } else {
            match self.history.category {
                ContentCategory::All => 0,
                ContentCategory::Text => 2,
                ContentCategory::Other => 3,
            }
        };
        let categories = [
            Some(ContentCategory::All),
            None,
            Some(ContentCategory::Text),
            Some(ContentCategory::Other),
        ];
        if let Some(next) = current.checked_add_signed(direction)
            && let Some(&category) = categories.get(next)
        {
            self.select_category(category, window, cx);
        }
    }

    fn select_adjacent_group(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_delete_pending || self.clear_pending || self.group_reorder_pending {
            return;
        }
        let ids: Vec<_> = self.groups.iter().map(|group| group.id).collect();
        let Some(group_id) = adjacent_group_id(&ids, self.history.group_id, direction) else {
            return;
        };
        // Four group actions and the default pill precede custom group pills.
        let pill_index = group_id
            .and_then(|id| ids.iter().position(|candidate| *candidate == id))
            .map_or(4, |index| 5 + index);
        self.group_scroll.scroll_to_item(pill_index);
        self.select_group(group_id, window, cx);
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
        if (!self.clear_confirm_open && !self.skip_clear_confirm) || self.clear_pending {
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
        self.batch_mode = false;
        self.selected_ids.clear();
        self.selection_anchor = None;
        self.batch_confirm_open = false;
    }

    fn save_toolbar(&mut self, toolbar: ToolbarPreference, cx: &mut Context<Self>) {
        if !self.toolbar_pending
            && toolbar != self.toolbar
            && self.send(Command::SetToolbar(toolbar), cx)
        {
            self.toolbar_pending = true;
            cx.notify();
        }
    }

    fn save_display(&mut self, display: DisplayPreference, cx: &mut Context<Self>) {
        if !self.display_pending
            && display != self.display
            && self.send(Command::SetDisplay(display), cx)
        {
            self.display_pending = true;
            cx.notify();
        }
    }

    fn save_audio(&mut self, audio: AudioPreference, cx: &mut Context<Self>) {
        if !self.audio_pending && audio != self.audio && self.send(Command::SetAudio(audio), cx) {
            self.audio_pending = true;
            cx.notify();
        }
    }

    fn save_monitor_types(
        &mut self,
        monitor_types: MonitorTypesPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.monitor_types_pending
            && monitor_types.valid()
            && monitor_types != self.monitor_types
            && self.send(Command::SetMonitorTypes(monitor_types), cx)
        {
            self.monitor_types_pending = true;
            cx.notify();
        }
    }

    fn save_app_filter(&mut self, preference: AppFilterPreference, cx: &mut Context<Self>) {
        if !self.app_filter_pending
            && preference.valid()
            && preference != self.app_filter
            && self.send(Command::SetAppFilter(Box::new(preference)), cx)
        {
            self.app_filter_pending = true;
            cx.notify();
        }
    }

    fn load_running_apps(&mut self, cx: &mut Context<Self>) {
        if !self.running_apps_pending && self.send(Command::ListRunningApps, cx) {
            self.running_apps_pending = true;
            cx.notify();
        }
    }

    fn toggle_batch_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.reorder_pending || self.history.loading {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        let enabled = !self.batch_mode;
        self.reset_selection();
        self.batch_mode = enabled;
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn clear_batch_selection(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending {
            return;
        }
        self.selected_ids.clear();
        self.selection_anchor = None;
        self.batch_confirm_open = false;
        cx.notify();
    }

    fn handle_outside_click(
        &mut self,
        position: windows::Win32::Foundation::POINT,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !tray::is_window_shown(window)
            || self.window_pinned
            || self._tray.is_none()
            || self.settings_window.is_some()
            || self.settings_window_opening
            || self.paste_pending.is_some()
            || self.batch_paste_pending.is_some()
            || self.batch_pending
            || self
                ._tray
                .as_ref()
                .is_some_and(|tray| tray::is_tray_click(tray, position))
        {
            return;
        }
        self.prepare_to_hide(window, cx);
        tray::set_window_visible(window, false);
        cx.notify();
    }

    fn show_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_minimized = tray::is_window_minimized(window);
        if was_minimized {
            tray::set_window_visible(window, true);
        }
        if let Err(error) = position::position_window(window, self.window_position) {
            self.message = format!(
                "{}: {error}",
                tr(self.language, "无法定位窗口", "Failed to position window")
            );
            self.is_error = true;
        }
        if !was_minimized {
            tray::set_window_visible(window, true);
        }
        if self.search_auto_clear && !self.search.read(cx).value().is_empty() {
            self.search_task = None;
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.history.begin_search();
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
            self.query(cx);
        }
        if self.preview_editing || self.preview_save_pending {
            return;
        }
        if self.search_auto_focus {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        } else if self.preview.id.is_none() {
            window.focus(&self.list_focus, cx);
        }
    }

    fn hide_visible_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.paste_pending.is_some() || self.batch_paste_pending.is_some() || self.batch_pending
        {
            return;
        }
        self.prepare_to_hide(window, cx);
        tray::set_window_visible(window, false);
        cx.notify();
    }

    fn prepare_to_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.paste_target = None;
        self.reset_selection();
        if self.auto_reset_state {
            self.search_task = None;
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.history.set_group(None);
            self.group_scroll.scroll_to_item(0);
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
            if !self.preview_editing && !self.preview_save_pending {
                self.preview.close();
                self.preview_source_hash = None;
                self.preview_edit_requested = false;
            }
            self.query(cx);
        }
    }

    fn onboarding_visible(&self) -> bool {
        self.show_onboarding && !self.onboarding_completed
    }

    fn advance_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_pending {
            return;
        }
        if self.onboarding_step < 3 {
            self.onboarding_step += 1;
            cx.notify();
        } else {
            self.complete_onboarding(cx);
        }
    }

    fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        if self.onboarding_visible()
            && !self.onboarding_pending
            && self.send(Command::CompleteOnboarding, cx)
        {
            self.onboarding_pending = true;
            cx.notify();
        }
    }

    fn dismiss_or_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding_visible() {
            self.complete_onboarding(cx);
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        if cx.has_active_drag() {
            cx.stop_active_drag(window);
            self.drop_target = None;
            self.group_drop_target = None;
            self.group_drag_direction = 0;
            self.history_drag_direction = 0;
            cx.notify();
            return;
        }
        if self.clear_all_confirm_open {
            if !self.clear_all_pending {
                self.clear_all_confirm_open = false;
                window.focus(&self.list_focus, cx);
                cx.notify();
            }
            return;
        }
        if self.group_delete_id.is_some() {
            if !self.group_delete_pending {
                self.group_delete_id = None;
                cx.notify();
            }
            return;
        }
        if self.clear_confirm_open {
            if !self.clear_pending {
                self.clear_confirm_open = false;
                cx.notify();
            }
            return;
        }
        if self.batch_confirm_open {
            if !self.batch_pending {
                self.batch_confirm_open = false;
                cx.notify();
            }
            return;
        }
        if self.group_move_id.is_some() {
            if !self.group_move_pending {
                self.group_move_id = None;
                cx.notify();
            }
            return;
        }
        if self.group_editor_open {
            if !self.group_save_pending {
                self.group_editor_open = false;
                self.group_rename_id = None;
                self.group_name_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                window.focus(&self.list_focus, cx);
                cx.notify();
            }
            return;
        }
        if self.batch_mode {
            if !self.batch_pending {
                self.reset_selection();
                cx.notify();
            }
            return;
        }
        if self.paste_pending.is_some() || self.batch_paste_pending.is_some() {
            return;
        }
        self.paste_target = None;
        if self._tray.is_some() {
            self.prepare_to_hide(window, cx);
            tray::set_window_visible(window, false);
            cx.notify();
        } else {
            self.exiting.set(true);
            window.remove_window();
        }
    }

    fn toggle_selection(&mut self, id: i64, extend_range: bool, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        if self.batch_pending || self.history.loading {
            return;
        }
        if !self.history.items.iter().any(|item| item.id == id) {
            return;
        }
        self.close_hover_preview(cx);
        self.batch_mode = true;
        let ids: Vec<_> = self.history.items.iter().map(|item| item.id).collect();
        let range = extend_range
            .then_some(self.selection_anchor)
            .flatten()
            .and_then(|anchor| selection_range_ids(&ids, anchor, id));
        if let Some(range) = range {
            self.selected_ids.extend(range.iter().copied());
        } else if !self.selected_ids.insert(id) {
            self.selected_ids.remove(&id);
        }
        self.selection_anchor = Some(id);
        self.batch_confirm_open = false;
        cx.notify();
    }

    fn cancel_pending_row_click(&mut self) {
        self.pending_row_click = None;
        self.row_click_task = None;
    }

    fn handle_row_click(
        &mut self,
        id: i64,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reorder_pending
            || self.history.loading
            || self.batch_pending
            || self.paste_pending.is_some()
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        let editable = self.history.items.iter().any(|item| {
            item.id == id && matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf")
        });
        if self.batch_mode || !editable || event.is_keyboard() {
            self.cancel_pending_row_click();
            self.activate_row(id, event.modifiers().shift, window, cx);
            return;
        }
        if event.click_count() >= 2 {
            self.cancel_pending_row_click();
            if !self.history.loading && !self.batch_pending && self.paste_pending.is_none() {
                self.open_preview_for_edit(id, window, cx);
            }
            return;
        }
        self.cancel_pending_row_click();
        let generation = self.history.generation;
        self.close_hover_preview(cx);
        self.pending_row_click = Some((id, generation));
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        let delay = unsafe { GetDoubleClickTime() }.max(1).saturating_add(30);
        self.row_click_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(u64::from(delay)))
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                if this.pending_row_click == Some((id, generation))
                    && this.history.generation == generation
                    && this.preview.id.is_none()
                    && !this.batch_mode
                {
                    this.cancel_pending_row_click();
                    this.activate_row(id, false, window, cx);
                }
            });
        }));
        cx.notify();
    }

    fn activate_row(
        &mut self,
        id: i64,
        extend_range: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reorder_pending
            || self.history.loading
            || self.batch_pending
            || self.paste_pending.is_some()
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        if self.batch_mode {
            self.toggle_selection(id, extend_range, cx);
            return;
        }
        let can_paste = self.paste_target.is_some_and(|target| {
            self._tray.is_some() && paste::is_external_target(window, target)
        });
        if can_paste {
            self.paste_selected(id, window, cx);
        } else {
            self.paste_target = None;
            self.send(Command::Copy(id), cx);
        }
        cx.notify();
    }

    fn perform_history_menu_action(
        &mut self,
        id: i64,
        action: &HistoryMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.loading
            || self.batch_pending
            || self.batch_mode
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        self.cancel_pending_row_click();
        self.history.selected = Some(id);
        match action {
            HistoryMenuAction::Paste => self.paste_selected(id, window, cx),
            HistoryMenuAction::PastePlainText => self.copy_or_paste_plain_text(id, window, cx),
            HistoryMenuAction::Copy => {
                self.send(Command::Copy(id), cx);
            }
            HistoryMenuAction::CopyPath => self.copy_or_paste_path(id, window, cx),
            HistoryMenuAction::Preview => self.open_preview(id, window, cx),
            HistoryMenuAction::Edit => self.open_preview_for_edit(id, window, cx),
            HistoryMenuAction::Reveal => {
                self.send(Command::RevealInExplorer(id), cx);
            }
            HistoryMenuAction::SaveAs(name) => self.start_save_as(id, name.clone(), cx),
            HistoryMenuAction::ToggleFavorite => {
                self.send(Command::ToggleFavorite(id), cx);
            }
            HistoryMenuAction::TogglePin => {
                self.send(Command::TogglePin(id), cx);
            }
            HistoryMenuAction::MoveToGroup => {
                if !self.group_move_pending {
                    self.group_move_id = Some(id);
                }
            }
            HistoryMenuAction::Delete => {
                self.send(Command::Delete(id), cx);
            }
        }
        cx.notify();
    }

    fn select_all_loaded(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.batch_mode = true;
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
                self.show_window(window, cx);
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
                        self.reset_selection();
                        self.group_move_id = None;
                        self.preview.close();
                        self.preview_source_hash = None;
                        self.preview_editing = false;
                        self.preview_edit_requested = false;
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
                        self.reset_selection();
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
                if applied
                    && self
                        .hover_preview
                        .id
                        .is_some_and(|id| !self.history.items.iter().any(|item| item.id == id))
                {
                    self.close_hover_preview(cx);
                }
                if applied {
                    self.refresh_file_card_info(cx);
                    self.selected_ids
                        .retain(|id| self.history.items.iter().any(|item| item.id == *id));
                    if self
                        .selection_anchor
                        .is_some_and(|id| !self.history.items.iter().any(|item| item.id == id))
                    {
                        self.selection_anchor = None;
                    }
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
                        if self.preview_edit_requested {
                            self.begin_preview_edit(window, cx);
                        }
                    }
                    self.preview_edit_requested = false;
                }
            }
            Event::HoverPreview {
                id,
                generation,
                result,
            } => {
                if self.hover_preview.apply(id, generation, result)
                    && self.hover_source_active
                    && self.preview.id.is_none()
                {
                    self.open_hover_popup(window, cx);
                }
            }
            Event::HoverPreviewSaved(result) => {
                self.hover_preference_pending = false;
                match result {
                    Ok(preference) => {
                        self.hover_preference = preference;
                        self.close_hover_preview(cx);
                        self.message = tr(
                            self.language,
                            "悬停预览设置已保存",
                            "Hover preview preferences saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "悬停预览设置保存失败",
                                "Failed to save hover preview preferences",
                            )
                        );
                        self.is_error = true;
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
                        self.preview_edit_requested = false;
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
                if sound::enabled(self.audio, Sound::Copy, SoundTiming::AfterSuccess) {
                    sound::play(Sound::Copy);
                }
                if let Some((pending_id, target)) = self.paste_pending.take() {
                    if pending_id == id && for_paste {
                        self.return_to_paste_target(
                            target,
                            clipboard_sequence,
                            Some(id),
                            window,
                            cx,
                        );
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
                self.reset_selection();
                if sound::enabled(self.audio, Sound::Copy, SoundTiming::AfterSuccess) {
                    sound::play(Sound::Copy);
                }
                let target = self.batch_paste_pending.take();
                if for_paste {
                    if let Some(target) = target {
                        self.return_to_paste_target(target, clipboard_sequence, None, window, cx);
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
                        self.close_hover_preview(cx);
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
                        self.close_hover_preview(cx);
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
            Event::PersistWindowSizeSaved(result) => {
                self.persist_window_size_pending = false;
                match result {
                    Ok(enabled) => {
                        self.persist_window_size = enabled;
                        self.last_window_size = None;
                        if enabled {
                            self.window_bounds_changed(window, cx);
                        }
                        self.message = tr(
                            self.language,
                            "窗口大小设置已保存",
                            "Window size setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存窗口大小设置失败",
                                "Failed to save window size setting"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::AutoResetStateSaved(result) => {
                self.auto_reset_state_pending = false;
                match result {
                    Ok(enabled) => {
                        self.auto_reset_state = enabled;
                        self.message = tr(
                            self.language,
                            "隐藏时重置设置已保存",
                            "Reset-on-hide setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存隐藏设置失败",
                                "Failed to save hide setting"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::SearchAutoFocusSaved(result) => {
                self.search_auto_focus_pending = false;
                match result {
                    Ok(enabled) => {
                        self.search_auto_focus = enabled;
                        self.message = tr(
                            self.language,
                            "搜索焦点设置已保存",
                            "Search focus setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存搜索焦点失败",
                                "Failed to save search focus"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::SearchAutoClearSaved(result) => {
                self.search_auto_clear_pending = false;
                match result {
                    Ok(enabled) => {
                        self.search_auto_clear = enabled;
                        self.message = tr(
                            self.language,
                            "搜索清空设置已保存",
                            "Search clearing setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存搜索清空失败",
                                "Failed to save search clearing"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::SkipClearConfirmSaved(result) => {
                self.skip_clear_confirm_pending = false;
                match result {
                    Ok(enabled) => {
                        self.skip_clear_confirm = enabled;
                        self.message = tr(
                            self.language,
                            "清理确认设置已保存",
                            "Clear confirmation setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存清理确认设置失败",
                                "Failed to save clear confirmation setting"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::PasteCloseWindowSaved(result) => {
                self.paste_close_window_pending = false;
                match result {
                    Ok(enabled) => {
                        self.paste_close_window = enabled;
                        self.message = tr(
                            self.language,
                            "粘贴后关闭窗口设置已保存",
                            "Close-after-paste setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存粘贴后窗口行为失败",
                                "Failed to save paste window behavior",
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::PasteKeySaved(result) => {
                self.paste_key_pending = false;
                match result {
                    Ok(key) => {
                        self.paste_key = key;
                        self.message = tr(
                            self.language,
                            "粘贴按键设置已保存",
                            "Paste key setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存粘贴按键失败",
                                "Failed to save paste key"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::PasteMoveToTopSaved(result) => {
                self.paste_move_to_top_pending = false;
                match result {
                    Ok(enabled) => {
                        self.paste_move_to_top = enabled;
                        self.message = tr(
                            self.language,
                            "粘贴后移到首位设置已保存",
                            "Move-to-top-after-paste setting saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存粘贴后排序设置失败",
                                "Failed to save paste sorting setting",
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::QuickPasteEnabledSaved(result) => {
                self.quick_paste_pending_setting = false;
                match result {
                    Ok(enabled) => {
                        self.quick_paste_enabled = enabled;
                        if let Some(warning) = self.quick_paste_registration_warning.take() {
                            self.message = warning;
                            self.is_error = true;
                        } else {
                            self.message = tr(
                                self.language,
                                "快速粘贴快捷键设置已保存",
                                "Quick paste shortcuts setting saved",
                            )
                            .into();
                            self.is_error = false;
                        }
                    }
                    Err(error) => {
                        self.quick_paste_registration_warning = None;
                        if self.quick_paste_enabled && self.paste_hotkeys.is_none() {
                            if let Ok((hotkeys, _)) = PasteHotkeys::start(
                                self.paste_hotkey_sender.clone(),
                                &self.paste_shortcuts,
                                self.hotkey_choice,
                            ) {
                                self.paste_hotkeys = Some(hotkeys);
                            }
                        } else if !self.quick_paste_enabled {
                            self.paste_hotkeys = None;
                        }
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存快速粘贴快捷键设置失败",
                                "Failed to save quick paste shortcuts setting",
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::PasteShortcutsSaved(result) => {
                let Some(previous) = self.paste_shortcuts_pending.take() else {
                    return;
                };
                match result {
                    Ok(shortcuts) => {
                        self.paste_shortcuts = *shortcuts;
                        if let Some(warning) = self.quick_paste_registration_warning.take() {
                            self.message = warning;
                            self.is_error = true;
                        } else {
                            self.message = tr(
                                self.language,
                                "快速粘贴槽位快捷键已保存",
                                "Quick paste slot shortcut saved",
                            )
                            .into();
                            self.is_error = false;
                        }
                        self.paste_shortcut_status = Some((self.message.clone(), self.is_error));
                    }
                    Err(error) => {
                        self.paste_shortcuts = previous;
                        self.paste_hotkeys = None;
                        self.quick_paste_registration_warning = None;
                        let restore_error = self.restore_paste_hotkeys();
                        self.message = error;
                        if let Some(error) = restore_error {
                            self.message.push_str(&format!("; {error}"));
                        }
                        self.is_error = true;
                        self.paste_shortcut_status = Some((self.message.clone(), true));
                    }
                }
            }
            Event::QuickPasteResolved {
                slot,
                favorite,
                result,
            } => {
                let Some((pending_slot, pending_favorite, target)) =
                    self.quick_paste_pending.take()
                else {
                    return;
                };
                if slot != pending_slot || favorite != pending_favorite {
                    self.quick_paste_pending = Some((pending_slot, pending_favorite, target));
                    return;
                }
                match result {
                    Ok(id) if paste::is_external_target(window, target) => {
                        if self.send(Command::CopyForPaste(id), cx) {
                            self.paste_pending = Some((id, target));
                        }
                    }
                    Ok(_) => {
                        self.message = tr(
                            self.language,
                            "目标窗口已变化，快速粘贴已取消",
                            "Target window changed; quick paste was canceled",
                        )
                        .into();
                        self.is_error = true;
                    }
                    Err(error) => {
                        self.message = error;
                        self.is_error = true;
                    }
                }
            }
            Event::ToolbarSaved(result) => {
                self.toolbar_pending = false;
                match result {
                    Ok(toolbar) => {
                        self.toolbar = toolbar;
                        self.message =
                            tr(self.language, "工具栏设置已保存", "Toolbar saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(self.language, "保存工具栏失败", "Failed to save toolbar")
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::DisplaySaved(result) => {
                self.display_pending = false;
                match result {
                    Ok(display) => {
                        self.display = display;
                        if !display.show_category_filter {
                            self.select_category(Some(ContentCategory::All), window, cx);
                        }
                        self.message =
                            tr(self.language, "显示设置已保存", "Display settings saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存显示设置失败",
                                "Failed to save display settings"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::AudioSaved(result) => {
                self.audio_pending = false;
                match result {
                    Ok(audio) => {
                        self.audio = audio;
                        self.message =
                            tr(self.language, "音效设置已保存", "Audio settings saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存音效设置失败",
                                "Failed to save audio settings"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::MonitorTypesSaved(result) => {
                self.monitor_types_pending = false;
                match result {
                    Ok(preference) => {
                        self.monitor_types = preference;
                        self.message =
                            tr(self.language, "监听类型已保存", "Capture types saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存监听类型失败",
                                "Failed to save capture types"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::AppFilterSaved(result) => {
                self.app_filter_pending = false;
                match result {
                    Ok(preference) => {
                        self.app_filter = *preference;
                        self.message =
                            tr(self.language, "应用过滤设置已保存", "App filter saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存应用过滤失败",
                                "Failed to save app filter"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::RunningApps(apps) => {
                self.running_apps_pending = false;
                self.running_apps = apps;
            }
            Event::OnboardingCompleted(result) => {
                self.onboarding_pending = false;
                match result {
                    Ok(()) => {
                        self.onboarding_completed = true;
                        self.message = tr(
                            self.language,
                            "欢迎使用 ElegantClipboard",
                            "Welcome to ElegantClipboard",
                        )
                        .into();
                        self.is_error = false;
                        if self.search_auto_focus {
                            self.search.update(cx, |input, cx| input.focus(window, cx));
                        } else {
                            window.focus(&self.list_focus, cx);
                        }
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存引导状态失败",
                                "Failed to save onboarding state"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Event::WindowPositionSaved(result) => {
                self.window_position_pending = false;
                match result {
                    Ok(position) => {
                        self.window_position = position;
                        self.message = tr(
                            self.language,
                            "窗口唤出位置已保存",
                            "Window position mode saved",
                        )
                        .into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                self.language,
                                "保存窗口位置失败",
                                "Failed to save window position"
                            )
                        );
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
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.history.selected = Some(id);
        self.preview_source_hash = self
            .history
            .items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.content_hash.clone());
        self.preview_editing = false;
        self.preview_edit_requested = false;
        self.preview_save_pending = false;
        self.image_zoom_percent = 100;
        let generation = self.preview.open(id);
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if !self.send(Command::Preview { id, generation }, cx) {
            self.preview
                .apply(id, generation, Err(self.message.clone()));
        }
        cx.notify();
    }

    fn open_preview_for_edit(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.open_preview(id, window, cx);
        self.preview_edit_requested = true;
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
        self.preview_edit_requested = false;
        self.image_zoom_percent = 100;
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

    fn zoom_image(&mut self, change: i16, cx: &mut Context<Self>) {
        if !matches!(self.preview.result, Some(Ok(PreviewContent::Image(_)))) {
            return;
        }
        let next = (i32::from(self.image_zoom_percent) + i32::from(change)).clamp(50, 400) as u16;
        if next != self.image_zoom_percent {
            self.image_zoom_percent = next;
            cx.notify();
        }
    }

    fn close_hover_preview(&mut self, cx: &mut Context<Self>) {
        self.hover_task = None;
        self.hover_close_task = None;
        self.hover_preview.close();
        self.hover_popup_opening = None;
        self.hover_source_active = false;
        self.hover_popup_active = false;
        if let Some(handle) = self.hover_popup.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        cx.notify();
    }

    fn schedule_hover_close(&mut self, cx: &mut Context<Self>) {
        self.hover_close_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
            let _ = view.update(cx, |this, cx| {
                if !this.hover_source_active && !this.hover_popup_active {
                    this.close_hover_preview(cx);
                }
            });
        }));
    }

    fn set_hover_source(&mut self, id: i64, entered: bool, cx: &mut Context<Self>) {
        if entered {
            if self.batch_mode
                || self.history.loading
                || self.preview.id.is_some()
                || self.pending_row_click.is_some()
            {
                return;
            }
            let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
                return;
            };
            if !self.hover_preference.allows(&item.content_type) {
                if self.hover_preview.id.is_some() {
                    self.close_hover_preview(cx);
                }
                return;
            }
            if self.hover_preview.id != Some(id) {
                self.close_hover_preview(cx);
                let generation = self.hover_preview.open(id);
                let delay = self.hover_preference.delay_ms;
                self.hover_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(u64::from(delay)))
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        if this.hover_preview.id == Some(id)
                            && this.hover_preview.generation == generation
                            && this.hover_source_active
                        {
                            this.send(Command::HoverPreview { id, generation }, cx);
                        }
                    });
                }));
            }
            self.hover_source_active = true;
            self.hover_close_task = None;
        } else if self.hover_preview.id == Some(id) {
            self.hover_source_active = false;
            self.schedule_hover_close(cx);
        }
    }

    fn set_hover_popup_active(
        &mut self,
        id: i64,
        generation: u64,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        if self.hover_preview.id != Some(id) || self.hover_preview.generation != generation {
            return;
        }
        self.hover_popup_active = entered;
        if entered {
            self.hover_close_task = None;
        } else if !self.hover_source_active {
            self.schedule_hover_close(cx);
        }
    }

    fn save_hover_preference(
        &mut self,
        preference: HoverPreviewPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.hover_preference_pending
            && self.hover_preference != preference
            && self.send(Command::SetHoverPreview(preference), cx)
        {
            self.hover_preference_pending = true;
            cx.notify();
        }
    }

    fn open_hover_popup(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.hover_popup.is_some()
            || self.hover_popup_opening.is_some()
            || !self.hover_source_active
        {
            return;
        }
        let (Some(id), Some(result)) = (self.hover_preview.id, self.hover_preview.result.clone())
        else {
            return;
        };
        let generation = self.hover_preview.generation;
        let screen = window.display(cx).map(|display| display.bounds());
        let image_dimensions = (self.hover_preference.expanded_image
            && matches!(&result, Ok(PreviewContent::Image(_))))
        .then(|| {
            self.history
                .items
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| Some((item.image_width?, item.image_height?)))
        })
        .flatten();
        let bounds = position::hover_popup_bounds(
            point(
                window.bounds().origin.x + window.mouse_position().x,
                window.bounds().origin.y + window.mouse_position().y,
            ),
            screen.unwrap_or_else(|| window.bounds()),
            self.hover_preference.position,
            image_dimensions,
        );
        let theme = self.theme;
        let language = self.language;
        let zoom_step = self.hover_preference.zoom_step;
        self.hover_popup_opening = Some((id, generation));
        cx.spawn(async move |owner, cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let valid = owner_entity.update(cx, |this, _| {
                this.hover_preview.id == Some(id)
                    && this.hover_preview.generation == generation
                    && (this.hover_source_active || this.hover_popup_active)
            });
            if !valid {
                owner_entity.update(cx, |this, _| {
                    if this.hover_popup_opening == Some((id, generation)) {
                        this.hover_popup_opening = None;
                    }
                });
                return;
            }
            let popup_owner = owner.clone();
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    focus: false,
                    show: true,
                    is_movable: false,
                    is_resizable: false,
                    app_id: Some("com.aslant.elegant-clipboard-gpui.hover".into()),
                    ..Default::default()
                },
                move |window, cx| {
                    apply_theme(theme, window, cx);
                    let view = cx.new(|cx| {
                        HoverPreviewWindowView::new(
                            popup_owner.clone(),
                            (id, generation),
                            language,
                            result,
                            zoom_step,
                            window,
                            cx,
                        )
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner_entity.update(cx, |this, cx| {
                if this.hover_popup_opening == Some((id, generation)) {
                    this.hover_popup_opening = None;
                }
                if let Ok(handle) = opened {
                    if this.hover_preview.id == Some(id)
                        && this.hover_preview.generation == generation
                        && (this.hover_source_active || this.hover_popup_active)
                    {
                        this.hover_popup = Some(handle.into());
                    } else {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();
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
            .when_some(single_file_image_path(entries), |body, path| {
                let unavailable = tr(self.language, "图片无法显示", "Image unavailable");
                body.child(
                    div()
                        .w_full()
                        .h(px(260.))
                        .flex_none()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted)
                        .overflow_hidden()
                        .child(
                            img(path)
                                .size_full()
                                .object_fit(ObjectFit::Contain)
                                .with_fallback(move || div().child(unavailable).into_any_element()),
                        ),
                )
            })
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
                .id("image-preview-viewport")
                .size_full()
                .overflow_scroll()
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                    if event.modifiers.control {
                        let delta = event.delta.pixel_delta(window.line_height()).y;
                        if delta > px(0.) {
                            this.zoom_image(10, cx);
                        } else if delta < px(0.) {
                            this.zoom_image(-10, cx);
                        }
                        cx.stop_propagation();
                    }
                }))
                .when(self.image_zoom_percent <= 100, |viewport| {
                    viewport.flex().items_center().justify_center()
                })
                .child(
                    div()
                        .w(relative(f32::from(self.image_zoom_percent) / 100.0))
                        .h(relative(f32::from(self.image_zoom_percent) / 100.0))
                        .flex_none()
                        .child(
                            img(path.clone())
                                .size_full()
                                .object_fit(ObjectFit::Contain)
                                .with_fallback(move || {
                                    div().child(image_unavailable).into_any_element()
                                }),
                        ),
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
                    .when(image, |bar| {
                        bar.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("image-zoom-out")
                                        .small()
                                        .outline()
                                        .label("−")
                                        .accessibility_label(tr(
                                            self.language,
                                            "缩小图片",
                                            "Zoom out",
                                        ))
                                        .disabled(self.image_zoom_percent <= 50)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_image(-10, cx);
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-reset")
                                        .small()
                                        .ghost()
                                        .label(format!("{}%", self.image_zoom_percent))
                                        .accessibility_label(tr(
                                            self.language,
                                            "重置图片缩放",
                                            "Reset zoom",
                                        ))
                                        .disabled(self.image_zoom_percent == 100)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.image_zoom_percent = 100;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-in")
                                        .small()
                                        .outline()
                                        .label("+")
                                        .accessibility_label(tr(
                                            self.language,
                                            "放大图片",
                                            "Zoom in",
                                        ))
                                        .disabled(self.image_zoom_percent >= 400)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_image(10, cx);
                                        })),
                                ),
                        )
                    })
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
                                .label(if self.paste_target.is_some() {
                                    tr(self.language, "粘贴纯文本", "Paste plain text")
                                } else {
                                    tr(self.language, "复制纯文本", "Copy plain text")
                                })
                                .disabled(!ready || self.paste_pending.is_some())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.copy_or_paste_plain_text(id, window, cx);
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
        self.cancel_pending_row_click();
        if let Some(index) = self.history.select_relative(direction) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    fn select_previous_or_focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.items.first().map(|item| item.id) == self.history.selected
            && self.history.selected.is_some()
        {
            self.search
                .update(cx, |search, cx| search.focus(window, cx));
        } else {
            self.select(-1, cx);
        }
    }

    fn focus_first_history_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.select_index(0, ScrollStrategy::Top, cx);
        window.focus(&self.list_focus, cx);
    }

    fn select_index(&mut self, index: usize, strategy: ScrollStrategy, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
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
                ((f32::from(size.item.height)
                    / visual::row_height(self.display.card_density, self.display.card_max_lines))
                .floor() as usize)
                    .saturating_sub(1)
                    .max(1) as isize
            })
            .unwrap_or(1)
    }

    fn paste_selected(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
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

    fn copy_or_paste_plain_text(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
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
            Command::CopyPlainTextForPaste(id)
        } else {
            Command::CopyPlainText(id)
        };
        if self.send(command, cx)
            && let Some(target) = target
        {
            self.paste_pending = Some((id, target));
            self.message = tr(
                self.language,
                "正在复制纯文本并返回原窗口…",
                "Copying plain text and returning to the previous window…",
            )
            .into();
            self.is_error = false;
            cx.notify();
        }
    }

    fn copy_or_paste_path(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
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
        pasted_id: Option<i64>,
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
        let hide_window = should_hide_after_paste(self.paste_close_window, self.window_pinned)
            && tray::is_window_shown(window);
        if hide_window {
            self.prepare_to_hide(window, cx);
            tray::set_window_visible(window, false);
        }
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                if sound::enabled(this.audio, Sound::Paste, SoundTiming::Immediate) {
                    sound::play(Sound::Paste);
                }
                match paste::send_to_target(target, clipboard_sequence, this.paste_key) {
                    Ok(()) => {
                        if sound::enabled(this.audio, Sound::Paste, SoundTiming::AfterSuccess) {
                            sound::play(Sound::Paste);
                        }
                        this.message = tr(
                            this.language,
                            "已发送粘贴快捷键，请检查目标应用",
                            "Paste shortcut sent; check the target app.",
                        )
                        .into();
                        this.is_error = false;
                        if let Some(id) = pasted_id.filter(|_| this.paste_move_to_top) {
                            this.send(Command::BumpToTop(id), cx);
                        }
                    }
                    Err(error) => {
                        if hide_window {
                            this.show_window(window, cx);
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

    fn refresh_file_card_info(&mut self, cx: &mut Context<Self>) {
        let loaded_paths: HashMap<_, _> = self
            .history
            .items
            .iter()
            .filter(|item| item.content_type == "files")
            .map(|item| (item.id, item.file_paths.as_deref().unwrap_or_default()))
            .collect();
        self.file_card_checked
            .retain(|id, raw| loaded_paths.get(id).is_some_and(|current| *current == raw));
        self.file_card_info
            .retain(|id, _| self.file_card_checked.contains_key(id));
        let candidates: Vec<_> = self
            .history
            .items
            .iter()
            .filter(|item| {
                item.content_type == "files" && !self.file_card_checked.contains_key(&item.id)
            })
            .map(|item| (item.id, item.file_paths.clone().unwrap_or_default()))
            .collect();
        if candidates.is_empty() {
            return;
        }
        self.file_card_checked.extend(candidates.iter().cloned());
        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let (local, network): (Vec<_>, Vec<_>) =
                candidates.into_iter().partition(|(_, raw)| {
                    !serde_json::from_str::<Vec<String>>(raw)
                        .unwrap_or_default()
                        .iter()
                        .any(|path| path.starts_with(r"\\"))
                });
            let local_results: Vec<_> = local
                .into_iter()
                .map(|(id, raw)| (id, raw.clone(), inspect_file_card(&raw)))
                .collect();
            if !local_results.is_empty() && sender.send_blocking(local_results).is_err() {
                return;
            }
            for (id, raw) in network {
                let result = (id, raw.clone(), inspect_file_card(&raw));
                if sender.send_blocking(vec![result]).is_err() {
                    return;
                }
            }
        });
        cx.spawn(async move |view, cx| {
            while let Ok(results) = receiver.recv().await {
                let _ = view.update(cx, |this, cx| {
                    let mut changed = false;
                    for (id, source, info) in results {
                        if this.history.items.iter().any(|item| {
                            item.id == id
                                && item.content_type == "files"
                                && item.file_paths.as_deref().unwrap_or_default() == source
                        }) {
                            this.file_card_info.insert(id, info);
                            changed = true;
                        }
                    }
                    if changed {
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.history.items[index];
        let id = item.id;
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let file_info = self.file_card_info.get(&id);
        let thumbnail_path = if is_image {
            item.image_path.as_ref().map(PathBuf::from)
        } else {
            file_info.and_then(|info| info.image_path.clone())
        };
        let show_file_icon = is_files && thumbnail_path.is_none();
        let file_warning = file_info.and_then(|info| match info.availability {
            FileCardAvailability::Available => None,
            FileCardAvailability::Missing => {
                Some(tr(self.language, "源文件已失效", "Source file missing"))
            }
            FileCardAvailability::Unreadable => {
                Some(tr(self.language, "文件无法读取", "File unreadable"))
            }
        });
        let file_icon = if file_warning.is_some() {
            IconName::TriangleAlert
        } else if file_info
            .is_some_and(|info| matches!(info.kind, FileCardKind::Folder | FileCardKind::Multiple))
        {
            IconName::Folder
        } else {
            IconName::File
        };
        let image_too_large = file_info.is_some_and(|info| info.image_too_large);
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let kind = match item.content_type.as_str() {
            "image" => tr(self.language, "图片", "Image"),
            "files" => tr(self.language, "文件", "Files"),
            "html" => "HTML",
            "rtf" => "RTF",
            "url" => tr(self.language, "网址", "URL"),
            _ => tr(self.language, "文本", "Text"),
        };
        let pinned = item.is_pinned;
        let detail = if is_image {
            match (item.image_width, item.image_height) {
                (Some(width), Some(height)) => Some(format!("{width} × {height}")),
                _ => Some(tr(self.language, "尺寸未知", "Unknown size").into()),
            }
        } else if is_files {
            let count = item
                .file_paths
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .map_or(0, |paths| paths.len());
            if count == 0 {
                Some(tr(self.language, "路径不可用", "Paths unavailable").into())
            } else if self.language == LanguagePreference::English {
                Some(format!("{count} items"))
            } else {
                Some(format!("{count} 项"))
            }
        } else if !self.display.show_char_count {
            None
        } else if matches!(item.content_type.as_str(), "html" | "rtf") && item.char_count.is_none()
        {
            Some(tr(self.language, "纯文本未知", "Plain text unavailable").into())
        } else if self.language == LanguagePreference::English {
            Some(format!("{} characters", item.char_count.unwrap_or(0)))
        } else {
            Some(format!("{} 字符", item.char_count.unwrap_or(0)))
        };
        let mut summary = if pinned {
            format!("{} · {kind}", tr(self.language, "置顶", "Pinned"))
        } else {
            kind.to_owned()
        };
        let source_icon = item
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) = if self.display.show_source_app {
            source_app_parts(self.display.source_app_display, source_icon.is_some())
        } else {
            (false, false)
        };
        if show_source_name
            && let Some(name) = item
                .source_app_name
                .as_deref()
                .filter(|name| !name.is_empty())
        {
            summary.push_str(" · ");
            summary.push_str(name);
        }
        if let Some(detail) = detail {
            summary.push_str(" · ");
            summary.push_str(&detail);
        }
        if self.display.show_byte_size {
            let size = if is_files {
                file_info.and_then(|info| info.total_size)
            } else {
                Some(item.byte_size.max(0) as u64)
            };
            if let Some(size) = size {
                summary.push_str(" · ");
                summary.push_str(&format_bytes(size));
            }
        }
        if let Some(warning) = file_warning {
            summary.push_str(" · ");
            summary.push_str(warning);
        } else if image_too_large {
            summary.push_str(" · ");
            summary.push_str(tr(self.language, "图片过大", "Image too large"));
        }
        let selected = !self.batch_mode && self.history.selected == Some(id);
        let marked = self.selected_ids.contains(&id);
        let favorite = item.is_favorite;
        let saved_preview = item.preview.as_deref().unwrap_or(kind);
        let search = self.search.read(cx).value();
        let file_names = is_files
            .then_some(item.file_paths.as_deref())
            .flatten()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .filter(|paths| paths.len() > 1)
            .map(|paths| {
                let mut names = paths
                    .iter()
                    .take(3)
                    .map(|path| {
                        Path::new(path)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(if self.language == LanguagePreference::Chinese {
                        "、"
                    } else {
                        ", "
                    });
                if paths.len() > 3 {
                    names.push('…');
                }
                names
            });
        let preview_text = file_names.unwrap_or_else(|| {
            item.text_content
                .as_deref()
                .and_then(|full| search_excerpt(full, saved_preview, &search))
                .unwrap_or_else(|| saved_preview.to_owned())
        });
        let highlights = search_highlight_ranges(&preview_text, &search)
            .into_iter()
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        color: Some(cx.theme().primary),
                        background_color: Some(cx.theme().primary.opacity(0.18)),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                )
            })
            .collect::<Vec<_>>();
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
        let left_drag_entity = cx.entity();
        let right_drag_entity = cx.entity();
        let drag_enabled = !self.reorder_pending && !self.history.loading && !self.batch_mode;
        let show_drag_area_indicator = self.display.show_drag_area_indicator;
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
        let card_spacing = visual::card_spacing(self.display.card_density);
        let (thumbnail_width, thumbnail_height) = visual::thumbnail_size(self.display.card_density);
        let row = div()
            .id(("history-row", id as usize))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, _| this.cancel_pending_row_click()),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hover_source(id, *hovered, cx);
            }))
            .h(px(visual::row_height(self.display.card_density, self.display.card_max_lines)))
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
            .py(px(card_spacing))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if cx.has_active_drag() {
                    return;
                }
                this.handle_row_click(id, event, window, cx);
            }))
            .child(
                div()
                    .h_full()
                    .when(self.batch_mode, |card| card.px_3())
                    .when(!self.batch_mode, |card| card.pl(px(38.)).pr(px(38.)))
                    .py(px(card_spacing))
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
                    .gap(px(card_spacing))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if file_warning.is_some() {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::new(IconName::EllipsisVertical)
                                            .xsmall()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .when_some(
                                        show_source_icon.then_some(source_icon).flatten(),
                                        |header, path| {
                                            header.child(
                                                img(std::path::PathBuf::from(path))
                                                    .w(px(16.))
                                                    .h(px(16.))
                                                    .object_fit(ObjectFit::Contain)
                                                    .with_fallback(|| div().into_any_element()),
                                            )
                                        },
                                    )
                                    .child(summary),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.cancel_pending_row_click();
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(
                                                Button::new(("select", id as usize))
                                                    .ghost()
                                                    .xsmall()
                                                    .h(px(24.))
                                                    .icon(IconName::Check)
                                                    .tooltip(if marked {
                                                        tr(self.language, "取消选择；Shift 点击可连选", "Deselect; Shift-click to select a range")
                                                    } else {
                                                        tr(self.language, "选择；Shift 点击可连选", "Select; Shift-click to select a range")
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
                                                        move |this, event: &ClickEvent, _, cx| {
                                                            cx.stop_propagation();
                                                            this.toggle_selection(
                                                                id,
                                                                event.modifiers().shift,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            ),
                                    )
                                    .when(self.display.show_time, |header| {
                                        header.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format_card_time(
                                                    &item.created_at,
                                                    self.display.time_format,
                                                    self.language,
                                                    chrono::Local::now().naive_local(),
                                                )),
                                        )
                                    }),
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
                            .py(px(card_spacing))
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(show_file_icon, |body| {
                                body.child(
                                    div()
                                        .w(px(thumbnail_width))
                                        .h(px(thumbnail_height))
                                        .flex_none()
                                        .rounded_sm()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(if file_warning.is_some() {
                                            cx.theme().danger.opacity(0.12)
                                        } else {
                                            cx.theme().primary.opacity(0.12)
                                        })
                                        .child(Icon::new(file_icon).large().text_color(
                                            if file_warning.is_some() {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().primary
                                            },
                                        )),
                                )
                            })
                            .when(thumbnail_path.is_some(), |body| {
                                body.child(
                                    div()
                                        .w(px(thumbnail_width))
                                        .h(px(thumbnail_height))
                                        .flex_none()
                                        .rounded_sm()
                                        .overflow_hidden()
                                        .when_some(thumbnail_path, |box_, path| {
                                            box_.child(
                                                img(path)
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
                                    .text_color(if file_warning.is_some() {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().foreground
                                    })
                                    .line_height(px(20.))
                                    .line_clamp(self.display.card_max_lines as usize)
                                    .text_ellipsis()
                                    .child(StyledText::new(preview_text).with_highlights(highlights)),
                            ),
                    )
                    .when(!self.batch_mode, |card| card.child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_1()
                            .justify_end()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.cancel_pending_row_click();
                                    cx.stop_propagation();
                                }),
                            )
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
                                            .tooltip(if self.paste_target.is_some() {
                                                tr(self.language, "粘贴纯文本", "Paste plain text")
                                            } else {
                                                tr(self.language, "复制纯文本", "Copy plain text")
                                            })
                                            .accessibility_label(if self.paste_target.is_some() {
                                                tr(self.language, "粘贴纯文本", "Paste plain text")
                                            } else {
                                                tr(self.language, "复制纯文本", "Copy plain text")
                                            })
                                            .disabled(self.paste_pending.is_some())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.copy_or_paste_plain_text(id, window, cx);
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
                    ))
                    .when(!self.batch_mode, |card| {
                        card.child(
                            div()
                                .id(("history-drag-left", id as usize))
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(show_drag_area_indicator, |handle| {
                                    handle
                                        .bg(cx.theme().accent)
                                        .text_color(cx.theme().primary)
                                        .child("⠿")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_pending_row_click();
                                    cx.stop_propagation();
                                }))
                                .when(drag_enabled, |handle| {
                                    handle.cursor_grab().on_drag(
                                        drag.clone(),
                                        move |drag, _, _, cx| {
                                            left_drag_entity.update(cx, |this, cx| {
                                                this.cancel_pending_row_click();
                                                this.close_hover_preview(cx);
                                                this.history.selected = Some(drag.id);
                                                this.drop_target = None;
                                                this.start_history_drag_scroll(cx);
                                                cx.notify();
                                            });
                                            cx.new(|_| drag.clone())
                                        },
                                    )
                                }),
                        )
                        .child(
                            div()
                                .id(("history-drag-right", id as usize))
                                .absolute()
                                .right_0()
                                .top_0()
                                .bottom_0()
                                .w(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(show_drag_area_indicator, |handle| {
                                    handle
                                        .bg(cx.theme().accent)
                                        .text_color(cx.theme().primary)
                                        .child("⠿")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_pending_row_click();
                                    cx.stop_propagation();
                                }))
                                .when(drag_enabled, |handle| {
                                    handle.cursor_grab().on_drag(
                                        drag.clone(),
                                        move |drag, _, _, cx| {
                                            right_drag_entity.update(cx, |this, cx| {
                                                this.cancel_pending_row_click();
                                                this.close_hover_preview(cx);
                                                this.history.selected = Some(drag.id);
                                                this.drop_target = None;
                                                this.start_history_drag_scroll(cx);
                                                cx.notify();
                                            });
                                            cx.new(|_| drag.clone())
                                        },
                                    )
                                }),
                        )
                    }),
            );
        let entity = cx.entity();
        let text_like = matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf");
        let language = self.language;
        let batch_mode = self.batch_mode;
        let can_paste = self.paste_target.is_some() && self._tray.is_some();
        let paste_pending = self.paste_pending.is_some();
        let save_as_pending = self.save_as_pending.is_some();
        let group_move_disabled =
            self.group_move_pending || (self.groups.is_empty() && self.history.group_id.is_none());
        let save_as_name = if is_image {
            item.image_path.as_deref().and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
        } else if is_files {
            item.file_paths
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .and_then(|paths| paths.into_iter().next())
                .and_then(|path| {
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
        } else {
            None
        };
        let row = if batch_mode {
            row.into_any_element()
        } else {
            row.context_menu(move |menu, _, _| {
                let menu = menu
                    .item(history_menu_item(
                        tr(language, "粘贴到原窗口", "Paste to previous window"),
                        !can_paste || paste_pending,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Paste,
                    ))
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            if can_paste {
                                tr(language, "粘贴纯文本", "Paste plain text")
                            } else {
                                tr(language, "复制纯文本", "Copy plain text")
                            },
                            paste_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::PastePlainText,
                        ))
                    })
                    .item(history_menu_item(
                        tr(language, "复制", "Copy"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Copy,
                    ));
                let menu = if is_image || is_files {
                    let menu = menu.item(history_menu_item(
                        if can_paste {
                            tr(language, "粘贴路径", "Paste paths")
                        } else {
                            tr(language, "复制路径", "Copy paths")
                        },
                        paste_pending,
                        entity.clone(),
                        id,
                        HistoryMenuAction::CopyPath,
                    ));
                    let menu = menu.item(history_menu_item(
                        tr(language, "在资源管理器中显示", "Show in File Explorer"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Reveal,
                    ));
                    if let Some(name) = save_as_name.clone() {
                        menu.item(history_menu_item(
                            tr(language, "另存为", "Save as"),
                            save_as_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::SaveAs(name),
                        ))
                    } else {
                        menu
                    }
                } else {
                    menu
                };
                menu.separator()
                    .item(history_menu_item(
                        tr(language, "查看详情", "View details"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Preview,
                    ))
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            tr(language, "编辑", "Edit"),
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::Edit,
                        ))
                    })
                    .separator()
                    .item(history_menu_item(
                        if favorite {
                            tr(language, "取消收藏", "Unfavorite")
                        } else {
                            tr(language, "收藏", "Favorite")
                        },
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::ToggleFavorite,
                    ))
                    .item(history_menu_item(
                        if pinned {
                            tr(language, "取消置顶", "Unpin")
                        } else {
                            tr(language, "置顶", "Pin")
                        },
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::TogglePin,
                    ))
                    .item(history_menu_item(
                        tr(language, "移动到分组", "Move to group"),
                        group_move_disabled,
                        entity.clone(),
                        id,
                        HistoryMenuAction::MoveToGroup,
                    ))
                    .separator()
                    .item(history_menu_item(
                        tr(language, "删除", "Delete"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Delete,
                    ))
            })
            .into_any_element()
        };
        let row = div()
            .when(live_drag_source, |row| row.opacity(0.0))
            .child(row);
        if let Some(offset) = self.reorder_offsets.get(&id) {
            visual::reflow(
                row,
                *offset as f32
                    * visual::row_height(self.display.card_density, self.display.card_max_lines),
                format!("reorder-feedback-{}-{id}", self.feedback_revision),
                cx,
            )
        } else if let Some(offset) = live_offset {
            row.relative()
                .top(px(offset as f32
                    * visual::row_height(
                        self.display.card_density,
                        self.display.card_max_lines,
                    )))
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

impl SettingsWindowView {
    fn audio_settings_card(
        &self,
        copy: bool,
        audio: AudioPreference,
        pending: bool,
        language: LanguagePreference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = if copy { "copy" } else { "paste" };
        let enabled = if copy {
            audio.copy_enabled
        } else {
            audio.paste_enabled
        };
        let timing = if copy {
            audio.copy_timing
        } else {
            audio.paste_timing
        };
        let toggle_owner = self.owner.clone();
        let immediate_owner = self.owner.clone();
        let success_owner = self.owner.clone();
        let preview_owner = self.owner.clone();
        settings_window_card(
            if copy {
                IconName::Copy
            } else {
                IconName::SquareTerminal
            },
            if copy {
                tr(language, "复制音效", "Copy sound")
            } else {
                tr(language, "粘贴音效", "Paste sound")
            },
            if copy {
                tr(
                    language,
                    "复制历史记录时播放反馈音",
                    "Play a sound when copying a history item",
                )
            } else {
                tr(
                    language,
                    "向原窗口发送粘贴快捷键时播放反馈音",
                    "Play a sound when sending paste to the previous window",
                )
            },
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    Button::new(format!("audio-{kind}-enabled"))
                        .outline()
                        .small()
                        .label(tr(language, "启用", "Enable"))
                        .selected(enabled)
                        .disabled(pending)
                        .on_click(move |_, _, cx| {
                            let _ = toggle_owner.update(cx, |owner, cx| {
                                let mut next = owner.audio;
                                if copy {
                                    next.copy_enabled = !next.copy_enabled;
                                } else {
                                    next.paste_enabled = !next.paste_enabled;
                                }
                                owner.save_audio(next, cx);
                            });
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(tr(language, "播放时机", "Timing"))
                        .child(
                            Button::new(format!("audio-{kind}-immediate"))
                                .outline()
                                .small()
                                .label(tr(language, "立即", "Immediate"))
                                .selected(timing == SoundTiming::Immediate)
                                .disabled(pending || !enabled)
                                .on_click(move |_, _, cx| {
                                    let _ = immediate_owner.update(cx, |owner, cx| {
                                        let mut next = owner.audio;
                                        if copy {
                                            next.copy_timing = SoundTiming::Immediate;
                                        } else {
                                            next.paste_timing = SoundTiming::Immediate;
                                        }
                                        owner.save_audio(next, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new(format!("audio-{kind}-success"))
                                .outline()
                                .small()
                                .label(tr(language, "成功后", "After success"))
                                .selected(timing == SoundTiming::AfterSuccess)
                                .disabled(pending || !enabled)
                                .on_click(move |_, _, cx| {
                                    let _ = success_owner.update(cx, |owner, cx| {
                                        let mut next = owner.audio;
                                        if copy {
                                            next.copy_timing = SoundTiming::AfterSuccess;
                                        } else {
                                            next.paste_timing = SoundTiming::AfterSuccess;
                                        }
                                        owner.save_audio(next, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new(format!("audio-{kind}-preview"))
                                .outline()
                                .small()
                                .label(tr(language, "试听", "Preview"))
                                .on_click(move |_, _, cx| {
                                    let played =
                                        sound::play(if copy { Sound::Copy } else { Sound::Paste });
                                    if !played {
                                        let _ = preview_owner.update(cx, |owner, cx| {
                                            owner.message = tr(
                                                owner.language,
                                                "音频设备不可用，无法试听",
                                                "Audio device unavailable; preview could not play",
                                            )
                                            .into();
                                            owner.is_error = true;
                                            cx.notify();
                                        });
                                    }
                                }),
                        ),
                ),
            cx,
        )
        .into_any_element()
    }

    fn quick_paste_settings_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (
            language,
            quick_paste_enabled,
            quick_paste_pending_setting,
            paste_key,
            paste_key_pending,
            paste_shortcuts,
            paste_shortcuts_pending,
            shortcut_actions_pending,
            paste_shortcut_status,
            monitoring,
        ) = {
            let state = owner.read(cx);
            (
                state.language,
                state.quick_paste_enabled,
                state.quick_paste_pending_setting,
                state.paste_key,
                state.paste_key_pending,
                state.paste_shortcuts.clone(),
                state.paste_shortcuts_pending.is_some(),
                state.paste_shortcuts_pending.is_some()
                    || state.quick_paste_pending_setting
                    || state.hotkey_pending,
                state.paste_shortcut_status.clone(),
                state.monitoring,
            )
        };
        let quick_paste_owner = self.owner.clone();
        let recent_shortcut_rows = self.recent_shortcuts_expanded.then(|| {
            self.paste_slot_rows(
                false,
                &paste_shortcuts,
                shortcut_actions_pending,
                language,
                cx,
            )
            .into_any_element()
        });
        let favorite_shortcut_rows = self.favorite_shortcuts_expanded.then(|| {
            self.paste_slot_rows(
                true,
                &paste_shortcuts,
                shortcut_actions_pending,
                language,
                cx,
            )
            .into_any_element()
        });
        settings_window_card(
            IconName::SquareTerminal,
            tr(language, "快速粘贴", "Quick paste"),
            tr(
                language,
                "从其他应用直接粘贴当前分组中排在前面的记录",
                "Paste top items in the selected group from another app",
            ),
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap_2()
                .child(
                    Button::new("quick-paste-enabled")
                        .outline()
                        .small()
                        .label(tr(language, "启用快速粘贴快捷键", "Enable quick paste shortcuts"))
                        .selected(quick_paste_enabled)
                        .disabled(quick_paste_pending_setting || paste_shortcuts_pending || !monitoring)
                        .on_click(move |_, _, cx| {
                            let _ = quick_paste_owner.update(cx, |owner, cx| {
                                owner.select_quick_paste_enabled(!owner.quick_paste_enabled, cx);
                            });
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_xs().child(tr(
                            language,
                            "自动粘贴使用按键",
                            "Key sent for automatic paste",
                        )))
                        .child(div().flex().gap_2().children([
                            ("paste-key-ctrl-v", "Ctrl+V", PasteKeyPreference::CtrlV),
                            (
                                "paste-key-shift-insert",
                                "Shift+Insert",
                                PasteKeyPreference::ShiftInsert,
                            ),
                        ]
                        .map(|(id, label, key)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .outline()
                                .small()
                                .label(label)
                                .selected(paste_key == key)
                                .disabled(paste_key_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        if owner.paste_key != key
                                            && owner.send(Command::SetPasteKey(key), cx)
                                        {
                                            owner.paste_key_pending = true;
                                            cx.notify();
                                        }
                                    });
                                })
                        }))),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(
                            language,
                            "可逐项修改、停用或恢复默认快捷键；数字键自动支持数字小键盘",
                            "Edit, disable, or restore each shortcut; digit keys also work on the numpad",
                        )),
                )
                .when_some(paste_shortcut_status, |panel, (message, is_error)| {
                    panel.child(
                        div()
                            .text_xs()
                            .text_color(if is_error {
                                cx.theme().danger
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(message),
                    )
                })
                .child(
                    Button::new("paste-recent-expand")
                        .outline()
                        .small()
                        .label(tr(language, "普通记录槽位（10）", "Recent slots (10)"))
                        .selected(self.recent_shortcuts_expanded)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recent_shortcuts_expanded = !this.recent_shortcuts_expanded;
                            cx.notify();
                        })),
                )
                .when(self.recent_shortcuts_expanded, |panel| {
                    panel.child(self.paste_group_actions(
                        false,
                        &paste_shortcuts,
                        shortcut_actions_pending,
                        language,
                    ))
                })
                .when_some(recent_shortcut_rows, |panel, rows| panel.child(rows))
                .child(
                    Button::new("paste-favorite-expand")
                        .outline()
                        .small()
                        .label(tr(language, "收藏槽位（10）", "Favorite slots (10)"))
                        .selected(self.favorite_shortcuts_expanded)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.favorite_shortcuts_expanded = !this.favorite_shortcuts_expanded;
                            cx.notify();
                        })),
                )
                .when(self.favorite_shortcuts_expanded, |panel| {
                    panel.child(self.paste_group_actions(
                        true,
                        &paste_shortcuts,
                        shortcut_actions_pending,
                        language,
                    ))
                })
                .when_some(favorite_shortcut_rows, |panel, rows| panel.child(rows)),
            cx,
        ).into_any_element()
    }
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
        let hover_preference = owner_state.hover_preference;
        let hover_pending = owner_state.hover_preference_pending;
        let hotkey_choice = owner_state.hotkey_choice;
        let hotkey_pending = owner_state.hotkey_pending;
        let window_position = owner_state.window_position;
        let window_position_pending = owner_state.window_position_pending;
        let persist_window_size = owner_state.persist_window_size;
        let persist_window_size_pending = owner_state.persist_window_size_pending;
        let auto_reset_state = owner_state.auto_reset_state;
        let auto_reset_state_pending = owner_state.auto_reset_state_pending;
        let search_auto_focus = owner_state.search_auto_focus;
        let search_auto_focus_pending = owner_state.search_auto_focus_pending;
        let search_auto_clear = owner_state.search_auto_clear;
        let search_auto_clear_pending = owner_state.search_auto_clear_pending;
        let skip_clear_confirm = owner_state.skip_clear_confirm;
        let skip_clear_confirm_pending = owner_state.skip_clear_confirm_pending;
        let paste_close_window = owner_state.paste_close_window;
        let paste_close_window_pending = owner_state.paste_close_window_pending;
        let paste_move_to_top = owner_state.paste_move_to_top;
        let paste_move_to_top_pending = owner_state.paste_move_to_top_pending;
        let toolbar = owner_state.toolbar;
        let toolbar_pending = owner_state.toolbar_pending;
        let display = owner_state.display;
        let display_pending = owner_state.display_pending;
        let audio = owner_state.audio;
        let audio_pending = owner_state.audio_pending;
        let monitor_types = owner_state.monitor_types;
        let monitor_types_pending = owner_state.monitor_types_pending;
        let app_filter = owner_state.app_filter.clone();
        let app_filter_pending = owner_state.app_filter_pending;
        let running_apps = owner_state.running_apps.clone();
        let running_apps_pending = owner_state.running_apps_pending;
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

        let expanded_hover_owner = self.owner.clone();
        let hover = settings_window_card(
            IconName::Eye,
            tr(language, "悬停预览", "Hover preview"),
            tr(
                language,
                "分别控制内容类型、延时、位置和图片缩放",
                "Choose content types, delay, position and image zoom",
            ),
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (
                                "hover-image",
                                tr(language, "图片", "Images"),
                                hover_preference.image,
                                HoverPreviewPreference {
                                    image: !hover_preference.image,
                                    ..hover_preference
                                },
                            ),
                            (
                                "hover-text",
                                tr(language, "文本", "Text"),
                                hover_preference.text,
                                HoverPreviewPreference {
                                    text: !hover_preference.text,
                                    ..hover_preference
                                },
                            ),
                            (
                                "hover-files",
                                tr(language, "文件", "Files"),
                                hover_preference.files,
                                HoverPreviewPreference {
                                    files: !hover_preference.files,
                                    ..hover_preference
                                },
                            ),
                        ]
                        .map(|(id, label, selected, preference)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .outline()
                                .small()
                                .label(label)
                                .selected(selected)
                                .disabled(hover_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_hover_preference(preference, cx);
                                    });
                                })
                        }),
                    ),
                )
                .child(
                    Button::new("hover-expanded-image")
                        .outline()
                        .small()
                        .label(tr(
                            language,
                            "大图使用更大浮窗",
                            "Expand image hover window",
                        ))
                        .selected(hover_preference.expanded_image)
                        .disabled(hover_pending || !hover_preference.image)
                        .on_click(move |_, _, cx| {
                            let _ = expanded_hover_owner.update(cx, |owner, cx| {
                                owner.save_hover_preference(
                                    HoverPreviewPreference {
                                        expanded_image: !hover_preference.expanded_image,
                                        ..hover_preference
                                    },
                                    cx,
                                );
                            });
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .child(tr(language, "停留延时", "Hover delay")),
                )
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (250, "250 ms"),
                            (500, "500 ms"),
                            (750, "750 ms"),
                            (1000, "1000 ms"),
                        ]
                        .map(|(delay, label)| {
                            let owner = self.owner.clone();
                            Button::new(("hover-delay", usize::from(delay)))
                                .outline()
                                .small()
                                .label(label)
                                .selected(hover_preference.delay_ms == delay)
                                .disabled(hover_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_hover_preference(
                                            HoverPreviewPreference {
                                                delay_ms: delay,
                                                ..hover_preference
                                            },
                                            cx,
                                        );
                                    });
                                })
                        }),
                    ),
                )
                .child(div().text_xs().child(tr(language, "浮窗位置", "Position")))
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (
                                "hover-auto",
                                tr(language, "自动", "Auto"),
                                HoverPreviewPosition::Auto,
                            ),
                            (
                                "hover-left",
                                tr(language, "左侧", "Left"),
                                HoverPreviewPosition::Left,
                            ),
                            (
                                "hover-right",
                                tr(language, "右侧", "Right"),
                                HoverPreviewPosition::Right,
                            ),
                        ]
                        .map(|(id, label, position)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .outline()
                                .small()
                                .label(label)
                                .selected(hover_preference.position == position)
                                .disabled(hover_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_hover_preference(
                                            HoverPreviewPreference {
                                                position,
                                                ..hover_preference
                                            },
                                            cx,
                                        );
                                    });
                                })
                        }),
                    ),
                )
                .child(
                    div()
                        .text_xs()
                        .child(tr(language, "图片缩放步进", "Image zoom step")),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children([5, 10, 20, 50].map(|zoom_step| {
                            let owner = self.owner.clone();
                            Button::new(("hover-zoom-step", usize::from(zoom_step)))
                                .outline()
                                .small()
                                .label(format!("{zoom_step}%"))
                                .selected(hover_preference.zoom_step == zoom_step)
                                .disabled(hover_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_hover_preference(
                                            HoverPreviewPreference {
                                                zoom_step,
                                                ..hover_preference
                                            },
                                            cx,
                                        );
                                    });
                                })
                        })),
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

        let quick_shortcuts = self.quick_paste_settings_card(cx);

        let positioning = settings_window_card(
            IconName::LayoutDashboard,
            tr(language, "窗口位置", "Window position"),
            tr(
                language,
                "选择每次唤出历史窗口时的位置",
                "Choose where the history window opens",
            ),
            div().flex().flex_wrap().gap_2().children(
                [
                    (
                        "window-position-cursor",
                        tr(language, "跟随光标", "Follow cursor"),
                        WindowPositionPreference::FollowCursor,
                    ),
                    (
                        "window-position-center",
                        tr(language, "当前屏幕居中", "Center on screen"),
                        WindowPositionPreference::ScreenCenter,
                    ),
                    (
                        "window-position-fixed",
                        tr(language, "保持位置", "Keep position"),
                        WindowPositionPreference::FixedPosition,
                    ),
                ]
                .map(|(id, label, preference)| {
                    let owner = self.owner.clone();
                    Button::new(id)
                        .outline()
                        .small()
                        .label(label)
                        .selected(window_position == preference)
                        .disabled(window_position_pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                if owner.window_position != preference
                                    && owner.send(Command::SetWindowPosition(preference), cx)
                                {
                                    owner.window_position_pending = true;
                                    cx.notify();
                                }
                            });
                        })
                }),
            ),
            cx,
        );

        let persist_owner = self.owner.clone();
        let reset_owner = self.owner.clone();
        let search_focus_owner = self.owner.clone();
        let search_clear_owner = self.owner.clone();
        let skip_clear_owner = self.owner.clone();
        let paste_close_owner = self.owner.clone();
        let paste_move_owner = self.owner.clone();
        let behavior = settings_window_card(
            IconName::LayoutDashboard,
            tr(language, "窗口行为", "Window behavior"),
            tr(
                language,
                "控制窗口大小记忆及搜索和隐藏行为",
                "Control size memory, search and hide behavior",
            ),
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap_2()
                .child(
                    Button::new("persist-window-size")
                        .outline()
                        .small()
                        .label(tr(language, "记住窗口大小", "Remember window size"))
                        .selected(persist_window_size)
                        .disabled(persist_window_size_pending)
                        .on_click(move |_, _, cx| {
                            let _ = persist_owner.update(cx, |owner, cx| {
                                if owner.send(
                                    Command::SetPersistWindowSize(!owner.persist_window_size),
                                    cx,
                                ) {
                                    owner.persist_window_size_pending = true;
                                    owner.window_size_task = None;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("auto-reset-state")
                        .outline()
                        .small()
                        .label(tr(
                            language,
                            "隐藏时重置搜索、筛选和滚动",
                            "Reset search, filters and scroll on hide",
                        ))
                        .selected(auto_reset_state)
                        .disabled(auto_reset_state_pending)
                        .on_click(move |_, _, cx| {
                            let _ = reset_owner.update(cx, |owner, cx| {
                                if owner
                                    .send(Command::SetAutoResetState(!owner.auto_reset_state), cx)
                                {
                                    owner.auto_reset_state_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("search-auto-focus")
                        .outline()
                        .small()
                        .label(tr(language, "唤出时聚焦搜索", "Focus search when shown"))
                        .selected(search_auto_focus)
                        .disabled(search_auto_focus_pending)
                        .on_click(move |_, _, cx| {
                            let _ = search_focus_owner.update(cx, |owner, cx| {
                                if owner
                                    .send(Command::SetSearchAutoFocus(!owner.search_auto_focus), cx)
                                {
                                    owner.search_auto_focus_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("search-auto-clear")
                        .outline()
                        .small()
                        .label(tr(language, "唤出时清空搜索", "Clear search when shown"))
                        .selected(search_auto_clear)
                        .disabled(search_auto_clear_pending)
                        .on_click(move |_, _, cx| {
                            let _ = search_clear_owner.update(cx, |owner, cx| {
                                if owner
                                    .send(Command::SetSearchAutoClear(!owner.search_auto_clear), cx)
                                {
                                    owner.search_auto_clear_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("skip-clear-confirm")
                        .outline()
                        .small()
                        .label(tr(
                            language,
                            "清理历史免确认",
                            "Clear history without confirmation",
                        ))
                        .selected(skip_clear_confirm)
                        .disabled(skip_clear_confirm_pending)
                        .on_click(move |_, _, cx| {
                            let _ = skip_clear_owner.update(cx, |owner, cx| {
                                if owner.send(
                                    Command::SetSkipClearConfirm(!owner.skip_clear_confirm),
                                    cx,
                                ) {
                                    owner.skip_clear_confirm_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("paste-close-window")
                        .outline()
                        .small()
                        .label(tr(language, "粘贴后关闭窗口", "Close after paste"))
                        .selected(paste_close_window)
                        .disabled(paste_close_window_pending)
                        .on_click(move |_, _, cx| {
                            let _ = paste_close_owner.update(cx, |owner, cx| {
                                if owner.send(
                                    Command::SetPasteCloseWindow(!owner.paste_close_window),
                                    cx,
                                ) {
                                    owner.paste_close_window_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                )
                .child(
                    Button::new("paste-move-to-top")
                        .outline()
                        .small()
                        .label(tr(
                            language,
                            "粘贴后移到列表首位",
                            "Move to top after paste",
                        ))
                        .selected(paste_move_to_top)
                        .disabled(paste_move_to_top_pending)
                        .on_click(move |_, _, cx| {
                            let _ = paste_move_owner.update(cx, |owner, cx| {
                                if owner
                                    .send(Command::SetPasteMoveToTop(!owner.paste_move_to_top), cx)
                                {
                                    owner.paste_move_to_top_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                ),
            cx,
        );

        let toolbar_settings =
            settings_window_card(
                IconName::LayoutDashboard,
                tr(language, "工具栏", "Toolbar"),
                tr(
                    language,
                    "选择按钮并调整顺序",
                    "Show buttons and change their order",
                ),
                div().flex().flex_col().gap_2().children(
                    toolbar.items.into_iter().enumerate().map(|(index, item)| {
                        let button = item.button;
                        let label = match button {
                            ToolbarButton::Clear => tr(language, "清理历史", "Clear history"),
                            ToolbarButton::Batch => tr(language, "批量选择", "Batch select"),
                            ToolbarButton::Pin => tr(language, "置顶窗口", "Pin window"),
                            ToolbarButton::Settings => tr(language, "设置", "Settings"),
                        };
                        let toggle_owner = self.owner.clone();
                        let up_owner = self.owner.clone();
                        let down_owner = self.owner.clone();
                        let drag = ToolbarDrag {
                            button,
                            label,
                            toolbar,
                        };
                        let drag_entity = cx.entity();
                        let target = cx
                            .has_active_drag()
                            .then_some(self.toolbar_drop_target)
                            .flatten()
                            .filter(|(target, _)| *target == button);
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap_2()
                            .on_drag_move(cx.listener(
                                move |this, event: &DragMoveEvent<ToolbarDrag>, _, cx| {
                                    if !event.bounds.contains(&event.event.position) {
                                        if this
                                            .toolbar_drop_target
                                            .is_some_and(|(target, _)| target == button)
                                        {
                                            this.toolbar_drop_target = None;
                                            cx.notify();
                                        }
                                        return;
                                    }
                                    let drag = event.drag(cx);
                                    let valid = this.owner.upgrade().is_some_and(|owner| {
                                        let owner = owner.read(cx);
                                        !owner.toolbar_pending && owner.toolbar == drag.toolbar
                                    });
                                    let next = (valid && drag.button != button && item.visible)
                                        .then_some((
                                            button,
                                            event.event.position.y > event.bounds.center().y,
                                        ));
                                    if this.toolbar_drop_target != next {
                                        this.toolbar_drop_target = next;
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_drop(cx.listener(move |this, drag: &ToolbarDrag, _, cx| {
                                let target = this.toolbar_drop_target.take();
                                let Some((target_button, after)) =
                                    target.filter(|(target_button, _)| *target_button == button)
                                else {
                                    cx.notify();
                                    return;
                                };
                                if let Some(owner) = this.owner.upgrade() {
                                    owner.update(cx, |owner, cx| {
                                        if !owner.toolbar_pending
                                            && owner.toolbar == drag.toolbar
                                            && let Some(next) = drag.toolbar.move_before_or_after(
                                                drag.button,
                                                target_button,
                                                after,
                                            )
                                        {
                                            owner.save_toolbar(next, cx);
                                        }
                                    });
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id(("toolbar-drag-handle", index))
                                    .w(px(22.))
                                    .h(px(28.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("⠿")
                                    .when(item.visible && !toolbar_pending, |handle| {
                                        handle.cursor_grab().on_drag(
                                            drag.clone(),
                                            move |drag, _, _, cx| {
                                                drag_entity.update(cx, |this, cx| {
                                                    this.toolbar_drop_target = None;
                                                    cx.notify();
                                                });
                                                cx.new(|_| drag.clone())
                                            },
                                        )
                                    }),
                            )
                            .child(
                                Button::new(("toolbar-visible", index))
                                    .small()
                                    .outline()
                                    .label(label)
                                    .selected(item.visible)
                                    .disabled(toolbar_pending || button == ToolbarButton::Settings)
                                    .on_click(move |_, _, cx| {
                                        if let Some(next) =
                                            toolbar.with_visibility(button, !item.visible)
                                        {
                                            let _ = toggle_owner.update(cx, |owner, cx| {
                                                owner.save_toolbar(next, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new(("toolbar-up", index))
                                    .small()
                                    .ghost()
                                    .label(tr(language, "上移", "Up"))
                                    .disabled(toolbar_pending || index == 0)
                                    .on_click(move |_, _, cx| {
                                        if let Some(next) = toolbar.move_button(button, -1) {
                                            let _ = up_owner.update(cx, |owner, cx| {
                                                owner.save_toolbar(next, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new(("toolbar-down", index))
                                    .small()
                                    .ghost()
                                    .label(tr(language, "下移", "Down"))
                                    .disabled(toolbar_pending || index + 1 == toolbar.items.len())
                                    .on_click(move |_, _, cx| {
                                        if let Some(next) = toolbar.move_button(button, 1) {
                                            let _ = down_owner.update(cx, |owner, cx| {
                                                owner.save_toolbar(next, cx);
                                            });
                                        }
                                    }),
                            )
                            .when_some(target, |row, (_, after)| {
                                row.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .h(px(2.))
                                        .bg(cx.theme().primary)
                                        .when(after, |line| line.bottom_0())
                                        .when(!after, |line| line.top_0()),
                                )
                            })
                    }),
                ),
                cx,
            );

        let category_owner = self.owner.clone();
        let drag_indicator_owner = self.owner.clone();
        let fewer_lines_owner = self.owner.clone();
        let more_lines_owner = self.owner.clone();
        let display_settings = settings_window_card(
            IconName::LayoutDashboard,
            tr(language, "列表显示", "List display"),
            tr(
                language,
                "调整分类栏和卡片间距",
                "Adjust filters and card spacing",
            ),
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap_2()
                .child(
                    Button::new("show-category-filter")
                        .small()
                        .outline()
                        .label(tr(language, "显示分类筛选", "Show category filters"))
                        .selected(display.show_category_filter)
                        .disabled(display_pending)
                        .on_click(move |_, _, cx| {
                            let next = DisplayPreference {
                                show_category_filter: !display.show_category_filter,
                                ..display
                            };
                            let _ = category_owner.update(cx, |owner, cx| {
                                owner.save_display(next, cx);
                            });
                        }),
                )
                .child(
                    Button::new("show-drag-area-indicator")
                        .small()
                        .outline()
                        .label(tr(language, "显示卡片拖动区域", "Show card drag areas"))
                        .selected(display.show_drag_area_indicator)
                        .disabled(display_pending)
                        .on_click(move |_, _, cx| {
                            let next = DisplayPreference {
                                show_drag_area_indicator: !display.show_drag_area_indicator,
                                ..display
                            };
                            let _ = drag_indicator_owner.update(cx, |owner, cx| {
                                owner.save_display(next, cx);
                            });
                        }),
                )
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (CardDensity::Compact, tr(language, "紧凑", "Compact")),
                            (CardDensity::Standard, tr(language, "标准", "Standard")),
                            (CardDensity::Spacious, tr(language, "宽松", "Spacious")),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (density, label))| {
                            let owner = self.owner.clone();
                            Button::new(("card-density", index))
                                .small()
                                .outline()
                                .label(label)
                                .selected(display.card_density == density)
                                .disabled(display_pending)
                                .on_click(move |_, _, cx| {
                                    let next = DisplayPreference {
                                        card_density: density,
                                        ..display
                                    };
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                })
                        }),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(tr(language, "卡片预览行数", "Card preview lines"))
                        .child(
                            Button::new("card-lines-less")
                                .small()
                                .outline()
                                .label("−")
                                .accessibility_label(tr(
                                    language,
                                    "减少预览行数",
                                    "Fewer preview lines",
                                ))
                                .disabled(display_pending || display.card_max_lines == 1)
                                .on_click(move |_, _, cx| {
                                    let next = DisplayPreference {
                                        card_max_lines: display.card_max_lines - 1,
                                        ..display
                                    };
                                    let _ = fewer_lines_owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                }),
                        )
                        .child(display.card_max_lines.to_string())
                        .child(
                            Button::new("card-lines-more")
                                .small()
                                .outline()
                                .label("+")
                                .accessibility_label(tr(
                                    language,
                                    "增加预览行数",
                                    "More preview lines",
                                ))
                                .disabled(display_pending || display.card_max_lines == 10)
                                .on_click(move |_, _, cx| {
                                    let next = DisplayPreference {
                                        card_max_lines: display.card_max_lines + 1,
                                        ..display
                                    };
                                    let _ = more_lines_owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                }),
                        ),
                )
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (
                                "show-card-time",
                                tr(language, "显示时间", "Show time"),
                                display.show_time,
                                DisplayPreference {
                                    show_time: !display.show_time,
                                    ..display
                                },
                            ),
                            (
                                "show-card-characters",
                                tr(language, "显示字符数", "Show character count"),
                                display.show_char_count,
                                DisplayPreference {
                                    show_char_count: !display.show_char_count,
                                    ..display
                                },
                            ),
                            (
                                "show-card-size",
                                tr(language, "显示大小", "Show size"),
                                display.show_byte_size,
                                DisplayPreference {
                                    show_byte_size: !display.show_byte_size,
                                    ..display
                                },
                            ),
                            (
                                "show-card-source",
                                tr(language, "显示来源应用", "Show source app"),
                                display.show_source_app,
                                DisplayPreference {
                                    show_source_app: !display.show_source_app,
                                    ..display
                                },
                            ),
                        ]
                        .into_iter()
                        .map(|(id, label, selected, next)| {
                            let owner = self.owner.clone();
                            Button::new(id)
                                .small()
                                .outline()
                                .label(label)
                                .selected(selected)
                                .disabled(display_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                })
                        }),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(tr(language, "时间格式", "Time format"))
                        .children(
                            [
                                (TimeFormat::Absolute, tr(language, "绝对时间", "Absolute")),
                                (TimeFormat::Relative, tr(language, "相对时间", "Relative")),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (format, label))| {
                                let owner = self.owner.clone();
                                Button::new(("time-format", index))
                                    .small()
                                    .outline()
                                    .label(label)
                                    .selected(display.time_format == format)
                                    .disabled(display_pending || !display.show_time)
                                    .on_click(move |_, _, cx| {
                                        let next = DisplayPreference {
                                            time_format: format,
                                            ..display
                                        };
                                        let _ = owner.update(cx, |owner, cx| {
                                            owner.save_display(next, cx);
                                        });
                                    })
                            }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(tr(language, "来源显示", "Source display"))
                        .children(
                            [
                                (
                                    SourceAppDisplay::Both,
                                    tr(language, "名称和图标", "Name and icon"),
                                ),
                                (SourceAppDisplay::Name, tr(language, "仅名称", "Name only")),
                                (SourceAppDisplay::Icon, tr(language, "仅图标", "Icon only")),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (mode, label))| {
                                let owner = self.owner.clone();
                                Button::new(("source-app-display", index))
                                    .small()
                                    .outline()
                                    .label(label)
                                    .selected(display.source_app_display == mode)
                                    .disabled(display_pending || !display.show_source_app)
                                    .on_click(move |_, _, cx| {
                                        let next = DisplayPreference {
                                            source_app_display: mode,
                                            ..display
                                        };
                                        let _ = owner.update(cx, |owner, cx| {
                                            owner.save_display(next, cx);
                                        });
                                    })
                            }),
                        ),
                ),
            cx,
        );

        let monitor_settings = settings_window_card(
            IconName::Eye,
            tr(language, "监听内容类型", "Capture content types"),
            tr(
                language,
                "选择保存到历史的内容类型；至少保留一种",
                "Choose which content types enter history; keep at least one",
            ),
            div().flex().flex_wrap().gap_2().children(
                [
                    (
                        "monitor-text",
                        tr(language, "文本", "Text"),
                        monitor_types.text,
                        MonitorTypesPreference {
                            text: !monitor_types.text,
                            ..monitor_types
                        },
                    ),
                    (
                        "monitor-url",
                        tr(language, "网址", "URL"),
                        monitor_types.url,
                        MonitorTypesPreference {
                            url: !monitor_types.url,
                            ..monitor_types
                        },
                    ),
                    (
                        "monitor-html",
                        "HTML",
                        monitor_types.html,
                        MonitorTypesPreference {
                            html: !monitor_types.html,
                            ..monitor_types
                        },
                    ),
                    (
                        "monitor-rtf",
                        "RTF",
                        monitor_types.rtf,
                        MonitorTypesPreference {
                            rtf: !monitor_types.rtf,
                            ..monitor_types
                        },
                    ),
                    (
                        "monitor-image",
                        tr(language, "图片", "Images"),
                        monitor_types.image,
                        MonitorTypesPreference {
                            image: !monitor_types.image,
                            ..monitor_types
                        },
                    ),
                    (
                        "monitor-files",
                        tr(language, "文件", "Files"),
                        monitor_types.files,
                        MonitorTypesPreference {
                            files: !monitor_types.files,
                            ..monitor_types
                        },
                    ),
                ]
                .into_iter()
                .map(|(id, label, selected, next)| {
                    let owner = self.owner.clone();
                    Button::new(id)
                        .small()
                        .outline()
                        .label(label)
                        .selected(selected)
                        .disabled(monitor_types_pending || !next.valid())
                        .on_click(move |_, _, cx| {
                            let _ =
                                owner.update(cx, |owner, cx| owner.save_monitor_types(next, cx));
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

        let filter_enable_owner = self.owner.clone();
        let app_filter_settings = settings_window_card(
            IconName::Eye,
            tr(language, "来源应用过滤", "Source app filter"),
            tr(
                language,
                "按来源应用决定是否保存新复制的内容",
                "Choose which source apps can add new history",
            ),
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    Button::new("app-filter-enabled")
                        .small()
                        .outline()
                        .label(tr(language, "启用应用过滤", "Enable app filter"))
                        .selected(app_filter.enabled)
                        .disabled(app_filter_pending)
                        .on_click(move |_, _, cx| {
                            let _ = filter_enable_owner.update(cx, |owner, cx| {
                                let mut next = owner.app_filter.clone();
                                next.enabled = !next.enabled;
                                owner.save_app_filter(next, cx);
                            });
                        }),
                )
                .child(div().flex().flex_wrap().gap_2().children(
                    [
                        (AppFilterMode::Blacklist, "app-filter-blacklist", tr(language, "黑名单", "Blocklist")),
                        (AppFilterMode::Whitelist, "app-filter-whitelist", tr(language, "白名单", "Allowlist")),
                    ]
                    .into_iter()
                    .map(|(mode, id, label)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .small()
                            .outline()
                            .label(label)
                            .selected(app_filter.mode == mode)
                            .disabled(app_filter_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let mut next = owner.app_filter.clone();
                                    next.mode = mode;
                                    owner.save_app_filter(next, cx);
                                });
                            })
                    }),
                ))
                .child(
                    div().text_xs().text_color(cx.theme().muted_foreground).child(
                        if app_filter.mode == AppFilterMode::Blacklist {
                            tr(language, "匹配规则的应用不记录；来源未知时继续记录", "Matching apps are skipped; unknown sources are recorded")
                        } else {
                            tr(language, "只记录匹配规则的应用；规则为空或来源未知时继续记录", "Only matching apps are recorded; empty rules or unknown sources are recorded")
                        },
                    ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&self.app_filter_input)))
                        .child(
                            Button::new("app-filter-add-rule")
                                .small()
                                .outline()
                                .label(tr(language, "添加", "Add"))
                                .disabled(app_filter_pending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.add_app_filter_rule(window, cx);
                                })),
                        ),
                )
                .when(self.app_filter_error, |panel| {
                    panel.child(
                        div().text_xs().text_color(cx.theme().danger).child(
                            tr(language, "规则无效、数量已达上限或保存尚未完成", "Invalid rule, rule limit reached, or save still in progress"),
                        ),
                    )
                })
                .when(app_filter.rules.is_empty(), |panel| {
                    panel.child(
                        div().text_xs().text_color(cx.theme().muted_foreground).child(
                            tr(language, "尚无规则，可输入进程名或 *、? 通配符", "No rules yet. Enter a process name or use * and ? wildcards"),
                        ),
                    )
                })
                .children(app_filter.rules.iter().enumerate().map(|(index, rule)| {
                    let owner = self.owner.clone();
                    let remove_rule = rule.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().text_sm().text_ellipsis().child(rule.clone()))
                        .child(
                            Button::new(format!("app-filter-remove-{index}"))
                                .small()
                                .ghost()
                                .label(tr(language, "移除", "Remove"))
                                .disabled(app_filter_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        let next = owner.app_filter.clone().without_rule(&remove_rule);
                                        owner.save_app_filter(next, cx);
                                    });
                                }),
                        )
                }))
                .child(
                    Button::new("app-filter-pick-running")
                        .small()
                        .outline()
                        .label(if self.app_picker_open {
                            tr(language, "收起运行中应用", "Hide running apps")
                        } else {
                            tr(language, "选择运行中应用", "Choose a running app")
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.app_picker_open = !this.app_picker_open;
                            if this.app_picker_open
                                && let Some(owner) = this.owner.upgrade()
                            {
                                owner.update(cx, |owner, cx| owner.load_running_apps(cx));
                            }
                            cx.notify();
                        })),
                )
                .when(self.app_picker_open, |panel| {
                    panel.child(
                        div()
                            .max_h(px(240.))
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when(running_apps_pending, |list| {
                                list.child(tr(language, "正在读取运行中应用…", "Loading running apps…"))
                            })
                            .when(!running_apps_pending && running_apps.is_empty(), |list| {
                                list.child(tr(language, "没有找到可选应用", "No running apps found"))
                            })
                            .children(running_apps.into_iter().enumerate().map(|(index, app)| {
                                let owner = self.owner.clone();
                                let process = app.process.clone();
                                let already_added = app_filter.rules.iter().any(|rule| rule.eq_ignore_ascii_case(&process));
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when_some(app.icon, |row, path| {
                                        row.child(img(std::path::PathBuf::from(path)).w(px(18.)).h(px(18.)).object_fit(ObjectFit::Contain).with_fallback(|| div().into_any_element()))
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                Button::new(format!("app-filter-pick-{index}"))
                                                    .small()
                                                    .ghost()
                                                    .label(app.process)
                                                    .disabled(app_filter_pending || already_added)
                                                    .on_click(move |_, _, cx| {
                                                        let _ = owner.update(cx, |owner, cx| {
                                                            if let Some(next) = owner.app_filter.clone().with_rule(&process) {
                                                                owner.save_app_filter(next, cx);
                                                            }
                                                        });
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .text_ellipsis()
                                                    .child(app.name),
                                            ),
                                    )
                            })),
                    )
                }),
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
        let copy_sound = self.audio_settings_card(true, audio, audio_pending, language, cx);
        let paste_sound = self.audio_settings_card(false, audio, audio_pending, language, cx);

        let about_details = settings_window_card(
            IconName::Star,
            tr(language, "作者与项目", "Author & project"),
            tr(
                language,
                "在浏览器中打开项目主页或提交问题",
                "Open the project page or report an issue in your browser",
            ),
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(
                    [
                        (
                            "about-author",
                            tr(language, "作者", "Author"),
                            "ASLant",
                            ProjectLink::Author,
                        ),
                        (
                            "about-repository",
                            "GitHub",
                            "ElegantClipboard",
                            ProjectLink::Repository,
                        ),
                        (
                            "about-issues",
                            tr(language, "反馈", "Feedback"),
                            tr(language, "提交问题", "Submit issue"),
                            ProjectLink::Issues,
                        ),
                    ]
                    .map(|(id, caption, label, link)| {
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(caption),
                            )
                            .child(
                                Button::new(id)
                                    .small()
                                    .ghost()
                                    .icon(IconName::ExternalLink)
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.link_error =
                                            links::open(link).err().map(|error| error.to_string());
                                        cx.notify();
                                    })),
                            )
                    }),
                )
                .when_some(self.link_error.clone(), |panel, error| {
                    panel.child(div().text_xs().text_color(cx.theme().danger).child(error))
                }),
            cx,
        );
        let content: Vec<AnyElement> = match self.page {
            SettingsPage::General => vec![
                positioning.into_any_element(),
                behavior.into_any_element(),
                startup.into_any_element(),
            ],
            SettingsPage::Display => vec![
                toolbar_settings.into_any_element(),
                display_settings.into_any_element(),
                hover.into_any_element(),
            ],
            SettingsPage::Theme => vec![appearance.into_any_element()],
            SettingsPage::Data => vec![
                monitor_settings.into_any_element(),
                storage.into_any_element(),
                privacy.into_any_element(),
            ],
            SettingsPage::AppFilter => vec![app_filter_settings.into_any_element()],
            SettingsPage::Audio => vec![copy_sound, paste_sound],
            SettingsPage::Shortcuts => vec![shortcut.into_any_element(), quick_shortcuts],
            SettingsPage::About => vec![
                settings_window_card(
                    IconName::Info,
                    tr(language, "关于 ElegantClipboard", "About ElegantClipboard"),
                    tr(
                        language,
                        "Windows 原生 GPUI 剪贴板管理器",
                        "Native GPUI clipboard manager for Windows",
                    ),
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .child(tr(
                            language,
                            "数据保存在本机；可在“数据”中导出备份。",
                            "Data stays on this device; export a backup from Data.",
                        )),
                    cx,
                )
                .into_any_element(),
                about_details.into_any_element(),
            ],
        };
        let page = self.page;

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
                            .w(px(154.))
                            .flex_shrink_0()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(SettingsPage::ALL.into_iter().map(|item| {
                                Button::new(format!("settings-page-{}", item.id()))
                                    .outline()
                                    .small()
                                    .label(item.label(language))
                                    .selected(page == item)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.page != item {
                                            this.page = item;
                                            this.shortcut_recording = false;
                                            this.shortcut_editing = None;
                                            this.shortcut_capture_error = None;
                                            cx.notify();
                                        }
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .id(format!("settings-content-{}", page.id()))
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .children(content),
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
                        .child(div().text_sm().font_semibold().child("ElegantClipboard")),
                ),
            )
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
    }
}

impl ClipboardView {
    fn render_onboarding(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let steps = [
            (
                IconName::Copy,
                tr(
                    language,
                    "欢迎使用 ElegantClipboard",
                    "Welcome to ElegantClipboard",
                ),
                tr(
                    language,
                    "复制的内容保存在本机，随时可从历史中查找。",
                    "Copied content stays on this device and is easy to find later.",
                ),
                tr(
                    language,
                    "复制文本、图片或文件即可开始记录",
                    "Copy text, images, or files to start recording",
                ),
            ),
            (
                IconName::Eye,
                tr(language, "快速搜索", "Quick search"),
                tr(
                    language,
                    "在顶部搜索框输入关键词，历史卡片会显示匹配内容。",
                    "Search from the top field and see matches in your history cards.",
                ),
                tr(
                    language,
                    "按 Ctrl+F 可以聚焦搜索框",
                    "Press Ctrl+F to focus search",
                ),
            ),
            (
                IconName::Star,
                tr(language, "置顶与收藏", "Pin and favorite"),
                tr(
                    language,
                    "把常用记录置顶或收藏，也可以通过卡片按钮查看详情。",
                    "Pin or favorite useful items and open card details when needed.",
                ),
                tr(
                    language,
                    "从目标应用用快捷键唤出后，点击卡片可自动粘贴",
                    "Open from a target app with the shortcut, then click a card to paste",
                ),
            ),
            (
                IconName::SquareTerminal,
                tr(language, "键盘快捷键", "Keyboard shortcuts"),
                tr(
                    language,
                    "方向键选择记录，Enter 复制或粘贴，Delete 删除；左右键切换分类。",
                    "Use arrows to select, Enter to copy or paste, Delete to remove, and Left/Right to switch categories.",
                ),
                tr(
                    language,
                    "Shift+Enter 使用纯文本表示",
                    "Shift+Enter uses the plain-text representation",
                ),
            ),
        ];
        let (icon, title, description, tip) = steps[self.onboarding_step.min(3)].clone();
        let is_last = self.onboarding_step == 3;
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .p_4()
            .flex()
            .items_center()
            .justify_center()
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .child(
                div()
                    .w_full()
                    .max_w(px(390.))
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .p_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(64.))
                            .h(px(64.))
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(icon).text_color(cx.theme().primary)),
                    )
                    .child(div().text_lg().font_semibold().child(title))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(description),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().primary)
                            .child(tip),
                    )
                    .child(div().flex().gap_2().children((0..4).map(|index| {
                        div()
                            .w(px(if index == self.onboarding_step {
                                24.
                            } else {
                                10.
                            }))
                            .h(px(6.))
                            .rounded_md()
                            .bg(if index <= self.onboarding_step {
                                cx.theme().primary
                            } else {
                                cx.theme().muted
                            })
                    })))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .child(
                                Button::new("onboarding-skip")
                                    .small()
                                    .ghost()
                                    .label(tr(language, "跳过", "Skip"))
                                    .disabled(self.onboarding_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.complete_onboarding(cx);
                                    })),
                            )
                            .child(
                                Button::new("onboarding-next")
                                    .small()
                                    .primary()
                                    .label(if is_last {
                                        tr(language, "开始使用", "Get started")
                                    } else {
                                        tr(language, "下一步", "Next")
                                    })
                                    .disabled(self.onboarding_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.advance_onboarding(cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "按 Esc 跳过引导", "Press Esc to skip")),
                    ),
            )
    }

    fn render_clear_all_confirmation(&self, cx: &mut Context<Self>) -> Div {
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .p(px(PAGE_PADDING))
            .flex()
            .flex_col()
            .gap_3()
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
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

    fn render_toolbar_button(&self, button: ToolbarButton, cx: &mut Context<Self>) -> AnyElement {
        match button {
            ToolbarButton::Clear => Button::new("toolbar-clear")
                .small()
                .outline()
                .label(tr(self.language, "清理历史", "Clear history"))
                .disabled(
                    self.group_save_pending || self.group_delete_pending || self.clear_pending,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.reset_selection();
                    this.group_delete_id = None;
                    this.group_editor_open = false;
                    this.group_rename_id = None;
                    this.group_move_id = None;
                    window.focus(&this.list_focus, cx);
                    if this.skip_clear_confirm {
                        this.clear_confirm_open = false;
                        this.clear_history(cx);
                    } else {
                        this.clear_confirm_open = true;
                    }
                    cx.notify();
                }))
                .into_any_element(),
            ToolbarButton::Batch => Button::new("toolbar-batch")
                .small()
                .outline()
                .label(if self.batch_mode {
                    tr(self.language, "退出批量", "Exit batch")
                } else {
                    tr(self.language, "批量选择", "Batch select")
                })
                .selected(self.batch_mode)
                .disabled(self.batch_pending || self.reorder_pending || self.history.loading)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_batch_mode(window, cx);
                }))
                .into_any_element(),
            ToolbarButton::Pin => Button::new("toolbar-pin")
                .small()
                .outline()
                .label(if self.window_pinned {
                    tr(self.language, "已置顶", "Pinned")
                } else {
                    tr(self.language, "置顶窗口", "Pin window")
                })
                .selected(self.window_pinned)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_window_pin(window, cx);
                }))
                .into_any_element(),
            ToolbarButton::Settings => Button::new("toolbar-settings")
                .small()
                .outline()
                .label(tr(self.language, "设置", "Settings"))
                .on_click(cx.listener(|this, _, _, cx| this.open_settings_window(cx)))
                .into_any_element(),
        }
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.onboarding_visible() {
            return self.render_onboarding(cx).into_any_element();
        }
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
            } else if self.history.category == ContentCategory::Text {
                tr(self.language, "还没有文本记录", "No text items yet.")
            } else if self.history.category == ContentCategory::Other {
                tr(self.language, "还没有其他类型记录", "No other items yet.")
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
        let toolbar_buttons = self
            .toolbar
            .items
            .into_iter()
            .filter(|item| item.visible)
            .map(|item| self.render_toolbar_button(item.button, cx))
            .collect::<Vec<_>>();
        div()
            .key_context("ClipboardApp")
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family("Microsoft YaHei UI")
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .child(
                div()
                    .key_context("HistorySearch")
                    .px(px(PAGE_PADDING))
                    .pt_3()
                    .pb_2()
                    .on_action(cx.listener(|this, _: &FocusFirstHistoryItem, window, cx| {
                        this.focus_first_history_item(window, cx);
                    }))
                    .child(Input::new(&self.search).cleanable(true)),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .children(toolbar_buttons),
                    )
                    .when(self.display.show_category_filter, |bar| bar.child(div().w_full().flex_none().flex().items_center().gap_2().children(
                        [
                            (Some(ContentCategory::All), "filter-all", tr(self.language, "全部", "All")),
                            (None, "filter-favorites", tr(self.language, "收藏", "Favorites")),
                            (Some(ContentCategory::Text), "filter-text", tr(self.language, "文本", "Text")),
                            (Some(ContentCategory::Other), "filter-other", tr(self.language, "其他", "Other")),
                        ]
                        .map(|(category, id, label)| {
                            let selected = match category {
                                None => self.history.favorite_only && self.history.group_id.is_none(),
                                Some(category) => {
                                    !self.history.favorite_only
                                        && self.history.category == category
                                        && self.history.group_id.is_none()
                                }
                            };
                            Button::new(id)
                            .small()
                            .outline()
                            .label(label)
                            .selected(selected)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.select_category(category, window, cx);
                                },
                            ))
                        }),
                    )))
                    .child(
                        div()
                            .id("group-bar")
                            .h(px(GROUP_BAR_HEIGHT))
                            .w_full()
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
                        "Clear unpinned, non-favorite items from “{name}”? Search, favorites, and type filters do not change the scope."
                    )
                } else {
                    format!(
                        "清理「{name}」中未置顶且未收藏的记录？搜索、收藏和类型筛选不影响清理范围。"
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
            .when(self.batch_mode, |container| {
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
                                .disabled(self.batch_pending || self.selected_ids.is_empty())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_batch_selection(cx);
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
                                .disabled(self.batch_pending || self.selected_ids.is_empty())
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
                    .flex_wrap()
                    .justify_between()
                    .gap_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.language == LanguagePreference::English {
                        format!("{} items", self.history.total)
                    } else {
                        format!("{} 条记录", self.history.total)
                    })
                    .when(self.history.items.len() > 8, |bar| {
                        bar.child(
                            Button::new("history-scroll-to-top")
                                .small()
                                .ghost()
                                .label(tr(self.language, "返回顶部 ↑", "Back to top ↑"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                                    cx.notify();
                                })),
                        )
                    })
                    .child(tr(
                        self.language,
                        "点击卡片或 Enter 复制/粘贴 · Shift+Enter 纯文本 · ←→ 分类 · Ctrl+←→ 分组",
                        "Click a card or Enter to copy/paste · Shift+Enter plain text · ←→ categories · Ctrl+←→ groups",
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
                        if !this.batch_mode && let Some(id) = this.history.selected {
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
                    .on_action(cx.listener(|this, _: &Previous, window, cx| {
                        this.select_previous_or_focus_search(window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &PreviousCategory, window, cx| {
                        this.select_adjacent_category(-1, window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &NextCategory, window, cx| {
                        this.select_adjacent_category(1, window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &PreviousGroup, window, cx| {
                        this.select_adjacent_group(-1, window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &NextGroup, window, cx| {
                        this.select_adjacent_group(1, window, cx);
                    }))
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
                    .on_action(cx.listener(|this, _: &ActivateSelected, window, cx| {
                        if !this.batch_mode
                            && let Some(id) = this.history.selected
                        {
                            this.activate_row(id, false, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &PastePlainTextSelected, window, cx| {
                        if !this.batch_mode && let Some(id) = this.history.selected {
                            this.copy_or_paste_plain_text(id, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &PasteSelected, window, cx| {
                        if !this.batch_mode && let Some(id) = this.history.selected {
                            this.paste_selected(id, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                        if !this.batch_mode && let Some(id) = this.history.selected {
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
