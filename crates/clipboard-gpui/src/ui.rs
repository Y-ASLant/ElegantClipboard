mod group_select;
mod settings_data;
mod settings_display;
mod settings_general;
mod settings_layout;

use crate::links::{self, ProjectLink};
use crate::paste;
use crate::position;
use crate::sound::{self, Sound};
use crate::tray::{self, TrayCommand};
use crate::visual::{self, GROUP_BAR_HEIGHT, PAGE_PADDING};
use crate::{
    options::Options,
    state::{
        HistoryState, PreviewState, drag_edge_target_index, format_card_time,
        next_drag_scroll_index, reorder_pixel_offsets, search_excerpt, search_highlight_ranges,
        selection_range_ids, should_hide_after_paste, source_app_parts,
    },
};
use clipboard_core::{
    ContentCategory, FilePreviewEntry, HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    database::Group,
    preferences::{
        AppFilterMode, AppFilterPreference, AudioPreference, CardDensity, DisplayPreference,
        HotkeyPreference, HoverPreviewPosition, HoverPreviewPreference, LanguagePreference,
        MonitorTypesPreference, PasteKeyPreference, PasteShortcutConfig, SoundTiming,
        SourceAppDisplay, ThemePreference, TimeFormat, WindowPositionPreference,
        WindowSizePreference,
    },
};
use clipboard_platform::hotkey::{
    Hotkey, PasteHotkeyEvent, PasteHotkeys, PasteRegistrationWarning, normalize_paste_shortcut,
    validate_paste_shortcuts,
};
use clipboard_platform::outside_click::OutsideClickMonitor;
use clipboard_platform::source_app::RunningApp;
use clipboard_platform::{Command, DataSizeInfo, Event, FailureKind, InstanceBusy, Service};
use directories::UserDirs;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::chart::BarChart;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::scroll::{ScrollableElement, ScrollbarAxis};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::stepper::{Stepper, StepperItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::*,
        input::{
            Input, InputEvent, InputState, NumberInputEvent, StepAction, Textarea, TextareaState,
        },
        *,
    },
    *,
};
use group_select::{GroupChoice, GroupOption, group_options};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};
use tray_icon::TrayIcon;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetDoubleClickTime, VK_SHIFT};

fn window_shell(cx: &App) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
}

fn history_row_height(
    item: &clipboard_core::database::ClipboardItem,
    display: DisplayPreference,
) -> f32 {
    if item.content_type == "image" {
        visual::image_row_height(display.card_density)
    } else {
        visual::row_height(display.card_density, display.card_max_lines)
    }
}

fn history_row_layout(
    items: &[clipboard_core::database::ClipboardItem],
    display: DisplayPreference,
) -> (Rc<Vec<gpui_kit::Size<Pixels>>>, Vec<Pixels>) {
    let mut sizes = Vec::with_capacity(items.len());
    let mut offsets = Vec::with_capacity(items.len() + 1);
    let mut top = px(0.);
    offsets.push(top);
    for item in items {
        let height = px(history_row_height(item, display));
        sizes.push(size(px(0.), height));
        top += height;
        offsets.push(top);
    }
    (Rc::new(sizes), offsets)
}

fn history_top_row(offsets: &[Pixels], scroll_y: Pixels) -> (usize, Pixels) {
    let top = -scroll_y;
    let index = offsets
        .partition_point(|offset| *offset <= top)
        .saturating_sub(1);
    (
        index.min(offsets.len().saturating_sub(2)),
        top - offsets[index],
    )
}

fn history_drop_row(
    offsets: &[Pixels],
    scroll_y: Pixels,
    viewport_top: Pixels,
    pointer_y: Pixels,
) -> Option<(usize, bool)> {
    if offsets.len() < 2 {
        return None;
    }
    let content_y = pointer_y - viewport_top - scroll_y;
    if content_y < px(0.) || content_y >= *offsets.last()? {
        return None;
    }
    let index = offsets.partition_point(|offset| *offset <= content_y) - 1;
    let midpoint = offsets[index] + px(f32::from(offsets[index + 1] - offsets[index]) / 2.);
    Some((index, content_y > midpoint))
}

#[cfg(test)]
mod history_virtual_list_tests {
    use super::{DragPlacement, history_drop_row, history_top_row, px};

    #[test]
    fn mixed_height_rows_preserve_partial_position_across_resizing_and_pagination() {
        let old = [px(0.), px(88.), px(236.), px(324.)];
        let (row, partial) = history_top_row(&old, px(-125.));
        assert_eq!((row, partial), (1, px(37.)));

        let resized = [px(0.), px(112.), px(260.), px(348.), px(460.)];
        let restored = -(resized[row] + partial);
        assert_eq!(history_top_row(&resized, restored), (1, px(37.)));
        assert_eq!(history_top_row(&resized, px(-348.)), (3, px(0.)));
    }

    #[test]
    fn dragging_text_past_an_image_yields_exactly_the_source_height() {
        let offsets = [px(0.), px(96.), px(220.), px(308.), px(456.)];
        let down = DragPlacement::new(0, 2, true).unwrap();
        assert_eq!(
            std::array::from_fn(|index| down.shift(index, &offsets)),
            [212., -96., -96., 0.]
        );
        let up = DragPlacement::new(3, 1, false).unwrap();
        assert_eq!(
            std::array::from_fn(|index| up.shift(index, &offsets)),
            [0., 148., 148., -212.]
        );
        assert!(DragPlacement::new(0, 1, false).is_none());
        assert!(DragPlacement::new(2, 1, true).is_none());
    }

    #[test]
    fn drop_target_uses_virtual_slots_not_displaced_card_hitboxes() {
        let offsets = [px(0.), px(96.), px(220.), px(308.), px(456.)];
        let scroll = px(-80.);
        let top = px(100.);
        assert_eq!(
            history_drop_row(&offsets, scroll, top, px(100.)),
            Some((0, true))
        );
        assert_eq!(
            history_drop_row(&offsets, scroll, top, px(120.)),
            Some((1, false))
        );
        assert_eq!(
            history_drop_row(&offsets, scroll, top, px(200.)),
            Some((1, true))
        );
        assert_eq!(history_drop_row(&offsets, scroll, top, px(19.)), None);
        assert_eq!(history_drop_row(&offsets, scroll, top, px(476.)), None);
        assert_eq!(history_drop_row(&[px(0.)], scroll, top, px(100.)), None);
    }
}

fn tr(language: LanguagePreference, chinese: &'static str, english: &'static str) -> &'static str {
    match language {
        LanguagePreference::Chinese => chinese,
        LanguagePreference::English => english,
    }
}

fn paste_registration_summary(
    language: LanguagePreference,
    warnings: &[PasteRegistrationWarning],
) -> Option<String> {
    let first = warnings.first()?;
    Some(match language {
        LanguagePreference::Chinese => format!(
            "{} 个快速粘贴组合键不可用（如 {first}）；可在设置 → 快捷键中更换或关闭快速粘贴",
            warnings.len()
        ),
        LanguagePreference::English => format!(
            "{} quick paste shortcuts are unavailable (e.g. {first}); change or disable them in Settings → Shortcuts",
            warnings.len()
        ),
    })
}

#[cfg(test)]
mod paste_registration_tests {
    use super::{LanguagePreference, PasteRegistrationWarning, paste_registration_summary};

    #[test]
    fn many_failures_produce_one_bounded_status_with_a_representative_error() {
        let warnings = (0..32)
            .map(|index| PasteRegistrationWarning {
                slot: 1,
                favorite: false,
                message: format!("conflict-{index}"),
            })
            .collect::<Vec<_>>();
        for language in [LanguagePreference::Chinese, LanguagePreference::English] {
            assert!(paste_registration_summary(language, &[]).is_none());
            let summary = paste_registration_summary(language, &warnings).unwrap();
            assert!(summary.contains("32"));
            assert!(summary.contains("conflict-0"));
            assert!(!summary.contains("conflict-31"));
            assert!(summary.chars().count() < 170);
        }
    }
}

const CATEGORY_FILTERS: [Option<ContentCategory>; 4] = [
    Some(ContentCategory::All),
    None,
    Some(ContentCategory::Text),
    Some(ContentCategory::Other),
];

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

fn shortcut_kbd(shortcut: &str) -> Option<Kbd> {
    let mut parts = shortcut.split('+');
    let key = parts.next_back()?.trim();
    if key.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::default();
    for part in parts {
        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            modifiers.control = true;
        } else if part.eq_ignore_ascii_case("alt") {
            modifiers.alt = true;
        } else if part.eq_ignore_ascii_case("shift") {
            modifiers.shift = true;
        } else {
            return None;
        }
    }
    Some(Kbd::new(Keystroke {
        key: key.to_ascii_lowercase(),
        modifiers,
        ..Default::default()
    }))
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
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(420.), px(520.))),
                    focus: !startup.hidden,
                    show: !startup.hidden,
                    app_id: Some("com.aslant.elegant-clipboard-gpui".into()),
                    ..WindowOptions::default()
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropTarget {
    id: i64,
    after: bool,
    allowed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DragPlacement {
    source: usize,
    insertion: usize,
}

impl DragPlacement {
    fn new(source: usize, target: usize, after: bool) -> Option<Self> {
        let insertion = target + usize::from(after);
        let insertion = insertion - usize::from(source < insertion);
        (source != insertion).then_some(Self { source, insertion })
    }

    fn shift(self, index: usize, offsets: &[Pixels]) -> f32 {
        let source_height = f32::from(offsets[self.source + 1] - offsets[self.source]);
        if index == self.source {
            if self.insertion > self.source {
                f32::from(offsets[self.insertion + 1] - offsets[self.source + 1])
            } else {
                f32::from(offsets[self.insertion] - offsets[self.source])
            }
        } else if (self.source + 1..=self.insertion).contains(&index) {
            -source_height
        } else if (self.insertion..self.source).contains(&index) {
            source_height
        } else {
            0.
        }
    }
}

#[derive(Default)]
struct DragMotionState {
    request: Option<(i64, DropTarget)>,
    from: Option<DragPlacement>,
    to: Option<DragPlacement>,
    revision: usize,
}

impl DragMotionState {
    fn update(
        &mut self,
        request: Option<(i64, DropTarget)>,
        items: &[clipboard_core::database::ClipboardItem],
    ) {
        if self.request == request {
            return;
        }
        self.from = self.to;
        self.to = request.and_then(|(source, target)| {
            let source = items.iter().position(|item| item.id == source)?;
            let target_index = items.iter().position(|item| item.id == target.id)?;
            DragPlacement::new(source, target_index, target.after)
        });
        self.request = request;
        self.revision = self.revision.wrapping_add(1);
    }

    fn offsets(&self, index: usize, row_offsets: &[Pixels]) -> (f32, f32) {
        (
            self.from.map_or(0., |from| from.shift(index, row_offsets)),
            self.to.map_or(0., |to| to.shift(index, row_offsets)),
        )
    }
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
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(Icon::new(gpui_kit::assets::IconName::GripVertical).xsmall())
                        .child(format!(
                            "{}{}",
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryConfirmation {
    DeleteGroup(i64),
    ClearHistory(Option<i64>),
    DeleteSelected,
}

struct HistoryDialogLayer;

impl Render for HistoryDialogLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .children(Root::render_dialog_layer(window, cx))
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
    group_select: Entity<SelectState<Vec<GroupOption>>>,
    search: Entity<InputState>,
    group_name_input: Entity<InputState>,
    group_editor_open: bool,
    group_rename_id: Option<i64>,
    group_save_pending: bool,
    group_edit_error: Option<String>,
    group_reorder_pending: bool,
    group_delete_id: Option<i64>,
    group_delete_pending: bool,
    clear_confirm_open: bool,
    clear_pending: bool,
    clear_all_error: Option<String>,
    clear_all_pending: bool,
    batch_mode: bool,
    selected_ids: HashSet<i64>,
    selection_anchor: Option<i64>,
    batch_confirm_open: bool,
    batch_pending: bool,
    batch_paste_pending: Option<(isize, u32)>,
    confirmation: Option<HistoryConfirmation>,
    confirmation_error: Option<String>,
    dialog_layer: Entity<HistoryDialogLayer>,
    group_move_id: Option<i64>,
    group_move_pending: bool,
    preview: PreviewState,
    preview_input: Entity<TextareaState>,
    preview_source_hash: Option<String>,
    preview_editing: bool,
    preview_edit_requested: bool,
    preview_save_pending: bool,
    image_zoom_percent: u16,
    image_scroll: ScrollHandle,
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
    scroll: VirtualListScrollHandle,
    row_sizes: Rc<Vec<gpui_kit::Size<Pixels>>>,
    row_offsets: Vec<Pixels>,
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
    daily_counts: Option<Vec<(String, i64)>>,
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
    drag_motion: DragMotionState,
    reorder_before: Option<Vec<i64>>,
    reorder_offsets: HashMap<i64, f32>,
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
    history_drag_scroll_task: Option<Task<()>>,
    window_size_task: Option<Task<()>>,
}

// Settings also observes the view, so its implicit GPUI window association can
// point at the settings window. Flush effects even when the main window is hidden.
fn update_clipboard_window<R>(
    view: &WeakEntity<ClipboardView>,
    handle: AnyWindowHandle,
    cx: &AsyncApp,
    update: impl FnOnce(&mut ClipboardView, &mut Window, &mut Context<ClipboardView>) -> R,
) -> anyhow::Result<R> {
    cx.update(|cx| {
        handle.update(cx, |_, window, cx| {
            view.update(cx, |view, cx| update(view, window, cx))
        })?
    })
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

    fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Display => IconName::Eye,
            Self::Theme => IconName::Palette,
            Self::Data => IconName::HardDrive,
            Self::AppFilter => IconName::Search,
            Self::Audio => IconName::Bell,
            Self::Shortcuts => IconName::SquareTerminal,
            Self::About => IconName::Info,
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
    preview_lines_input: Entity<InputState>,
    _preview_lines_input_subscription: Subscription,
    _preview_lines_step_subscription: Subscription,
    preview_lines_last_value: u8,
    preview_lines_last_pending: bool,
    app_picker_open: bool,
    app_filter_error: bool,
    link_error: Option<String>,
}

struct HoverPreviewWindowView {
    owner: WeakEntity<ClipboardView>,
    id: i64,
    generation: u64,
    language: LanguagePreference,
    content: Result<PreviewContent, String>,
    text_input: Entity<TextareaState>,
    zoom_percent: u16,
    image_scroll: ScrollHandle,
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
            image_scroll: ScrollHandle::default(),
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
                .relative()
                .size_full()
                .child(
                    div()
                        .id("hover-image-viewport")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&self.image_scroll)
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
                                    img(path.clone()).size_full().object_fit(ObjectFit::Contain),
                                ),
                        ),
                )
                .scrollbar(&self.image_scroll, ScrollbarAxis::Both)
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
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id("hover-file-image-viewport")
                                    .size_full()
                                    .overflow_scroll()
                                    .track_scroll(&self.image_scroll)
                                    .on_scroll_wheel(cx.listener(
                                        |this, event: &ScrollWheelEvent, window, cx| {
                                            if event.modifiers.control {
                                                let delta =
                                                    event.delta.pixel_delta(window.line_height()).y;
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
                            .scrollbar(&self.image_scroll, ScrollbarAxis::Both),
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
        window_shell(cx)
            .id("hover-preview-window")
            .p_3()
            .gap_2()
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
                                        .icon(IconName::Minus)
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
                                        .icon(IconName::Plus)
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
    fn new(
        owner: WeakEntity<ClipboardView>,
        page: SettingsPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let preview_lines_last_value = entity.read(cx).display.card_max_lines;
        let preview_lines_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(preview_lines_last_value.to_string())
        });
        // Disable the input's default text-changing step so button presses emit Step events;
        // our save handler then applies bounds and the same pending/ACK policy as typed edits.
        preview_lines_input.update(cx, |input, cx| input.set_step(None, window, cx));
        let preview_lines_input_subscription = cx.subscribe_in(
            &preview_lines_input,
            window,
            |this, input, event, window, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let (saved, pending) = {
                    let owner = owner.read(cx);
                    (owner.display.card_max_lines, owner.display_pending)
                };
                if pending {
                    return;
                }
                let value = input.read(cx).value();
                let parsed = value.parse::<i32>().ok();
                let lines = parsed
                    .map(|value| value.clamp(1, 10) as u8)
                    .unwrap_or(saved);
                if parsed != Some(i32::from(lines)) {
                    input.update(cx, |input, cx| {
                        input.set_value(lines.to_string(), window, cx)
                    });
                }
                this.save_preview_lines(lines, cx);
            },
        );
        let preview_lines_step_subscription = cx.subscribe_in(
            &preview_lines_input,
            window,
            |this, input, event: &NumberInputEvent, window, cx| {
                let NumberInputEvent::Step(action) = event;
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let (saved, pending) = {
                    let owner = owner.read(cx);
                    (owner.display.card_max_lines, owner.display_pending)
                };
                if pending {
                    return;
                }
                let value = input.read(cx).value();
                let value = value.parse::<i32>().unwrap_or(i32::from(saved));
                let lines = match action {
                    StepAction::Increment => value.saturating_add(1),
                    StepAction::Decrement => value.saturating_sub(1),
                }
                .clamp(1, 10) as u8;
                if value != i32::from(lines) {
                    input.update(cx, |input, cx| {
                        input.set_value(lines.to_string(), window, cx)
                    });
                }
                this.save_preview_lines(lines, cx);
            },
        );
        Self {
            owner,
            _owner_subscription: subscription,
            page,
            shortcut_input: None,
            shortcut_editing: None,
            shortcut_capture_focus: cx.focus_handle(),
            shortcut_recording: false,
            shortcut_capture_error: None,
            recent_shortcuts_expanded: false,
            favorite_shortcuts_expanded: false,
            app_filter_input,
            _app_filter_input_subscription: app_filter_input_subscription,
            preview_lines_input,
            _preview_lines_input_subscription: preview_lines_input_subscription,
            _preview_lines_step_subscription: preview_lines_step_subscription,
            preview_lines_last_value,
            preview_lines_last_pending: false,
            app_picker_open: false,
            app_filter_error: false,
            link_error: None,
        }
    }

    fn save_preview_lines(&mut self, lines: u8, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |owner, cx| {
            if !owner.display_pending && owner.display.card_max_lines != lines {
                owner.save_display(
                    DisplayPreference {
                        card_max_lines: lines,
                        ..owner.display
                    },
                    cx,
                );
            }
        });
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
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(format!(
                        "{} {slot}: {}",
                        tr(
                            language,
                            if favorite { "收藏" } else { "普通" },
                            if favorite { "Favorite" } else { "Recent" }
                        ),
                        tr(
                            language,
                            "输入组合键，例如",
                            "Enter a shortcut, for example"
                        )
                    ))
                    .child(shortcut_kbd("Ctrl+Alt+Z").expect("shortcut example")),
            )
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
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_1()
                                .child(tr(language, "请按组合键；按", "Press a shortcut; press"))
                                .child(shortcut_kbd("Esc").expect("recording cancel key"))
                                .child(tr(language, "取消录制", "to cancel recording")),
                        )
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            this.capture_shortcut_key(event, window, cx);
                        })),
                )
            })
            .when_some(self.shortcut_capture_error.clone(), |panel, error| {
                panel.child(Alert::error("shortcut-capture-error", error).small())
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
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
        let defaults = PasteShortcutConfig::default();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children((1..=10u8).map(|slot| {
                let shortcut = shortcuts.slot(favorite, slot).unwrap_or_default();
                let shortcut_content = shortcut_kbd(shortcut).map_or_else(
                    || {
                        div()
                            .text_sm()
                            .child(if shortcut.is_empty() {
                                tr(language, "未设置", "Not set").to_owned()
                            } else {
                                shortcut.to_owned()
                            })
                            .into_any_element()
                    },
                    |kbd| kbd.into_any_element(),
                );
                let default = defaults.slot(favorite, slot).unwrap_or_default().to_owned();
                let owner_disable = self.owner.clone();
                let owner_reset = self.owner.clone();
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w(px(220.))
                            .gap_2()
                            .child(div().w(px(96.)).text_xs().child(format!(
                                "{} {slot}",
                                tr(
                                    language,
                                    if favorite { "收藏" } else { "普通" },
                                    if favorite { "Favorite" } else { "Recent" }
                                )
                            )))
                            .child(shortcut_content),
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
                this.clear_confirm_open = false;
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
        let tray = tray::create(tray_sender.clone(), language, false);
        tray_enabled.set(tray.is_ok());
        if startup.hidden && tray.is_err() {
            tray::set_window_visible(window, true);
        }
        let tray_events = cx.spawn_in(window, async move |view, cx| {
            while let Ok(command) = tray_receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| match command
                {
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
                    TrayCommand::Settings => this.open_settings_window(SettingsPage::General, cx),
                    TrayCommand::ClearHistory => {
                        this.paste_target = None;
                        this.show_window(window, cx);
                        this.open_clear_history(window, cx);
                    }
                    TrayCommand::TogglePin => this.toggle_window_pin(window, cx),
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
            group_select,
            group_name_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(tr(language, "分组名称", "Group name"))
            }),
            group_editor_open: false,
            group_rename_id: None,
            group_save_pending: false,
            group_edit_error: None,
            group_reorder_pending: false,
            group_delete_id: None,
            group_delete_pending: false,
            clear_confirm_open: false,
            clear_pending: false,
            clear_all_error: None,
            clear_all_pending: false,
            batch_mode: false,
            selected_ids: HashSet::new(),
            selection_anchor: None,
            batch_confirm_open: false,
            batch_pending: false,
            batch_paste_pending: None,
            confirmation: None,
            confirmation_error: None,
            dialog_layer: cx.new(|_| HistoryDialogLayer),
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
            daily_counts: None,
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
            drag_motion: DragMotionState::default(),
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
            _subscriptions: vec![
                subscription,
                group_subscription,
                appearance,
                activation,
                bounds,
            ],
            _events: event_task,
            search_task: None,
            feedback_task: None,
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

    fn refresh_daily_counts(&mut self, cx: &mut Context<Self>) {
        let start = chrono::Local::now().date_naive() - chrono::Duration::days(6);
        self.send(Command::QueryDailyCounts(start.to_string()), cx);
    }

    fn open_settings_window(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| tray::set_window_visible(window, true))
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
        self.refresh_daily_counts(cx);
        cx.notify();
        let owner = cx.weak_entity();
        // Run after this view's update releases its borrow; unlike a spawned task,
        // a deferred effect also runs when the main window is hidden in the tray.
        cx.defer(move |cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let (theme, language) =
                owner_entity.update(cx, |owner, _| (owner.theme, owner.language));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(720.)),
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
            };
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
                    cx.new(|cx| SettingsWindowView::new(settings_owner.clone(), page, window, cx));
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
        });
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
                    self.quick_paste_registration_warning =
                        paste_registration_summary(self.language, &warnings);
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
                paste_registration_summary(self.language, &warnings)
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
                    self.quick_paste_registration_warning =
                        paste_registration_summary(self.language, &warnings);
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

    fn refresh_group_select(&self, window: &mut Window, cx: &mut Context<Self>) {
        let choice = self
            .history
            .group_id
            .map_or(GroupChoice::Default, GroupChoice::Existing);
        let options = group_options(&self.groups, self.language, self.history.group_id);
        self.group_select.update(cx, |select, cx| {
            select.set_items(options, window, cx);
            select.set_selected_value(&choice, window, cx);
        });
    }

    fn choose_group(&mut self, choice: GroupChoice, window: &mut Window, cx: &mut Context<Self>) {
        if self.group_save_pending
            || self.group_delete_pending
            || self.clear_pending
            || self.batch_pending
            || self.group_reorder_pending
        {
            cx.defer_in(window, |this, window, cx| {
                this.refresh_group_select(window, cx)
            });
            return;
        }
        match choice {
            GroupChoice::Default => self.select_group(None, window, cx),
            GroupChoice::Existing(id) => self.select_group(Some(id), window, cx),
            GroupChoice::Create => {
                self.group_rename_id = None;
                self.group_name_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.open_group_editor(window, cx);
            }
            GroupChoice::Rename => {
                if let Some(group) = self
                    .groups
                    .iter()
                    .find(|group| Some(group.id) == self.history.group_id)
                {
                    self.group_rename_id = Some(group.id);
                    self.group_name_input.update(cx, |input, cx| {
                        input.set_value(group.name.clone(), window, cx);
                    });
                    self.open_group_editor(window, cx);
                }
            }
            GroupChoice::Delete => {
                self.group_delete_id = self.history.group_id;
                self.reset_selection();
                self.clear_confirm_open = false;
                self.group_editor_open = false;
                self.group_rename_id = None;
                self.group_move_id = None;
                window.focus(&self.list_focus, cx);
                if let Some(id) = self.group_delete_id {
                    self.open_history_confirmation(
                        HistoryConfirmation::DeleteGroup(id),
                        window,
                        cx,
                    );
                }
            }
            GroupChoice::MoveUp | GroupChoice::MoveDown => {
                if let Some(id) = self.history.group_id
                    && let Some(index) = self.groups.iter().position(|group| group.id == id)
                    && let Some(target_index) =
                        index.checked_add_signed(if choice == GroupChoice::MoveUp { -1 } else { 1 })
                    && let Some(target) = self.groups.get(target_index)
                    && self.send(
                        Command::ReorderGroup {
                            from: id,
                            to: target.id,
                            after: choice == GroupChoice::MoveDown,
                        },
                        cx,
                    )
                {
                    self.group_reorder_pending = true;
                }
            }
        }
        cx.defer_in(window, |this, window, cx| {
            this.refresh_group_select(window, cx)
        });
        cx.notify();
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
        self.sync_confirmation_dialog(window, cx);
        self.history.set_group(group_id);
        cx.defer_in(window, |this, window, cx| {
            this.refresh_group_select(window, cx)
        });
        self.scroll_to_top();
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
        let selected = self.history.group_id.is_none()
            && match category {
                None => self.history.favorite_only,
                Some(category) => !self.history.favorite_only && self.history.category == category,
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
        self.sync_confirmation_dialog(window, cx);
        self.search_task = None;
        if let Some(category) = category {
            self.history.set_category(category);
        } else {
            self.history.set_favorite_filter(true);
        }
        self.refresh_group_select(window, cx);
        self.scroll_to_top();
        self.query(cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn category_tab_index(&self) -> usize {
        if self.history.favorite_only {
            1
        } else {
            match self.history.category {
                ContentCategory::All => 0,
                ContentCategory::Text => 2,
                ContentCategory::Other => 3,
            }
        }
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
        if let Some(next) = self.category_tab_index().checked_add_signed(direction)
            && let Some(&category) = CATEGORY_FILTERS.get(next)
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
        let index = self
            .history
            .group_id
            .and_then(|id| self.groups.iter().position(|group| group.id == id))
            .map_or(0, |index| index + 1);
        let Some(next) = index.checked_add_signed(direction) else {
            return;
        };
        if next == index || next > self.groups.len() {
            return;
        }
        self.select_group((next > 0).then(|| self.groups[next - 1].id), window, cx);
    }

    fn open_group_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.group_editor_open = true;
        self.group_edit_error = None;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let entity = owner.upgrade().expect("group dialog requires its owner");
            let view = entity.read(cx);
            let language = view.language;
            let renaming = view.group_rename_id.is_some();
            let pending = view.group_save_pending;
            let input = view.group_name_input.clone();
            let error = view.group_edit_error.clone();
            let keyboard_owner = owner.clone();
            let cancel_owner = owner.clone();
            let cancel_button_owner = owner.clone();
            let submit_owner = owner.clone();
            dialog
                .title(if renaming {
                    tr(language, "重命名分组", "Rename group")
                } else {
                    tr(language, "新建分组", "Create group")
                })
                .margin_top((window.viewport_size().height - px(180.)) / 2.)
                .close_button(false)
                .overlay_closable(false)
                .on_ok(move |_, _, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| view.save_group(cx));
                    false
                })
                .on_cancel(move |_, window, cx| {
                    cancel_owner
                        .update(cx, |view, cx| view.cancel_group_edit(window, cx))
                        .unwrap_or(true)
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(tr(language, "分组名称", "Group name"))
                        .child(Input::new(&input))
                        .when_some(error, |body, error| {
                            body.child(Alert::error("group-edit-error", error).small())
                        }),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("group-save-cancel")
                                .outline()
                                .label(tr(language, "取消", "Cancel"))
                                .disabled(pending)
                                .on_click(move |_, window, cx| {
                                    if cancel_button_owner
                                        .update(cx, |view, cx| view.cancel_group_edit(window, cx))
                                        .unwrap_or(false)
                                    {
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("group-save-submit")
                                .primary()
                                .label(if renaming {
                                    tr(language, "保存名称", "Save name")
                                } else {
                                    tr(language, "创建", "Create")
                                })
                                .disabled(pending)
                                .on_click(move |_, _, cx| {
                                    let _ = submit_owner.update(cx, |view, cx| view.save_group(cx));
                                }),
                        ),
                )
        });
        cx.defer_in(window, |view, window, cx| {
            view.group_name_input
                .update(cx, |input, cx| input.focus(window, cx));
        });
        cx.notify();
    }

    fn cancel_group_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.group_save_pending {
            return false;
        }
        self.group_editor_open = false;
        self.group_rename_id = None;
        self.group_edit_error = None;
        self.group_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.list_focus, cx);
        cx.notify();
        true
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
            self.group_edit_error = None;
        } else {
            self.group_edit_error = Some(self.message.clone());
        }
        self.dialog_layer.update(cx, |_, cx| cx.notify());
        cx.notify();
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
        if self.clear_all_pending {
            return;
        }
        self.clear_all_error = None;
        if self.send(Command::ClearAllHistory, cx) {
            self.clear_all_pending = true;
        } else {
            self.clear_all_error = Some(self.message.clone());
        }
        cx.notify();
    }

    fn reset_selection(&mut self) {
        self.batch_mode = false;
        self.selected_ids.clear();
        self.selection_anchor = None;
        self.batch_confirm_open = false;
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

    fn exit_batch_mode(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending {
            return;
        }
        self.reset_selection();
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
            || self.confirmation.is_some()
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
            self.scroll_to_top();
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
        if self.paste_pending.is_some()
            || self.batch_paste_pending.is_some()
            || self.batch_pending
            || self.group_delete_pending
            || self.clear_pending
        {
            return;
        }
        if let Some(kind) = self.confirmation {
            self.cancel_confirmation(kind, cx);
            window.close_dialog(cx);
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
            self.refresh_group_select(window, cx);
            self.scroll_to_top();
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
            self.history_drag_direction = 0;
            cx.notify();
            return;
        }
        if self.group_delete_id.is_some() {
            if !self.group_delete_pending {
                self.group_delete_id = None;
                self.sync_confirmation_dialog(window, cx);
                cx.notify();
            }
            return;
        }
        if self.clear_confirm_open {
            if !self.clear_pending {
                self.clear_confirm_open = false;
                self.sync_confirmation_dialog(window, cx);
                cx.notify();
            }
            return;
        }
        if self.batch_confirm_open {
            if !self.batch_pending {
                self.batch_confirm_open = false;
                self.sync_confirmation_dialog(window, cx);
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
            if self.cancel_group_edit(window, cx) {
                window.close_dialog(cx);
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
        let main_window = window.window_handle();
        self.row_click_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(u64::from(delay)))
                .await;
            let _ = update_clipboard_window(&view, main_window, cx, |this, window, cx| {
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
                    self.scroll_to_top();
                    self.query(cx);
                }
                self.groups = groups;
                self.refresh_group_select(window, cx);
            }
            Event::GroupReordered { result, .. } => {
                self.group_reorder_pending = false;
                match result {
                    Ok(()) => {
                        self.message =
                            tr(self.language, "分组顺序已保存", "Group order saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
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
                        self.group_edit_error = None;
                        window.close_dialog(cx);
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
                        self.group_edit_error = Some(self.message.clone());
                        self.dialog_layer.update(cx, |_, cx| cx.notify());
                    }
                }
            }
            Event::GroupRenamed(result) => {
                self.group_save_pending = false;
                match result {
                    Ok(group) => {
                        self.group_editor_open = false;
                        self.group_rename_id = None;
                        self.group_edit_error = None;
                        window.close_dialog(cx);
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
                        self.group_edit_error = Some(self.message.clone());
                        self.dialog_layer.update(cx, |_, cx| cx.notify());
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
                        self.clear_all_error = None;
                        if let Some(handle) = self.settings_window {
                            cx.spawn(async move |_, cx| {
                                cx.update(|cx| {
                                    let _ =
                                        handle.update(cx, |_, window, cx| window.close_dialog(cx));
                                });
                            })
                            .detach();
                        }
                        self.refresh_daily_counts(cx);
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
                        self.clear_all_error = Some(self.message.clone());
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
                if applied {
                    let (top_index, partial) =
                        history_top_row(&self.row_offsets, self.scroll.offset().y);
                    self.rebuild_row_layout();
                    let new_top = self.row_offsets[top_index.min(self.history.items.len())];
                    self.scroll.set_offset(point(px(0.), -(new_top + partial)));
                }
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
                    if self.selected_ids.is_empty() && !self.batch_pending {
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
                            let after: Vec<_> = self
                                .history
                                .items
                                .iter()
                                .map(|item| (item.id, history_row_height(item, self.display)))
                                .collect();
                            self.reorder_offsets = reorder_pixel_offsets(&before, &after);
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
                        self.refresh_group_select(window, cx);
                        let menu_result = if let Some(tray) = &self._tray {
                            tray::update_menu(tray, language, self.window_pinned).map(|_| None)
                        } else {
                            tray::create(self.tray_sender.clone(), language, self.window_pinned)
                                .map(Some)
                        };
                        match menu_result {
                            Ok(tray) => {
                                if let Some(tray) = tray {
                                    self._tray = Some(tray);
                                    self.tray_enabled.set(true);
                                }
                                self.message =
                                    tr(language, "语言设置已保存", "Language preference saved")
                                        .into();
                                self.is_error = false;
                            }
                            Err(error) => {
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
            Event::DisplaySaved(result) => {
                self.display_pending = false;
                match result {
                    Ok(display) => {
                        let (top_index, partial) =
                            history_top_row(&self.row_offsets, self.scroll.offset().y);
                        self.display = display;
                        self.rebuild_row_layout();
                        self.scroll.set_offset(point(
                            px(0.),
                            -(self.row_offsets[top_index.min(self.history.items.len())] + partial),
                        ));
                        if self
                            .history
                            .should_reset_category_filter(display.show_category_filter)
                        {
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
            Event::DailyCounts(result) => match result {
                Ok(counts) => self.daily_counts = Some(counts),
                Err(error) => {
                    self.daily_counts = None;
                    self.message = format!(
                        "{}: {error}",
                        tr(
                            self.language,
                            "统计每日历史失败",
                            "Failed to count daily history"
                        )
                    );
                    self.is_error = true;
                }
            },
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
                    FailureKind::ClearHistory => {
                        self.clear_pending = false;
                        self.confirmation_error = Some(message.clone());
                    }
                    FailureKind::ClearAllHistory => {
                        self.clear_all_pending = false;
                        self.clear_all_error = Some(message.clone());
                    }
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
        self.sync_confirmation_dialog(window, cx);
        if self.confirmation.is_some() {
            self.dialog_layer.update(cx, |_, cx| cx.notify());
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
                .relative()
                .size_full()
                .child(
                    div()
                        .id("image-preview-viewport")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&self.image_scroll)
                        .on_scroll_wheel(cx.listener(
                            |this, event: &ScrollWheelEvent, window, cx| {
                                if event.modifiers.control {
                                    let delta = event.delta.pixel_delta(window.line_height()).y;
                                    if delta > px(0.) {
                                        this.zoom_image(10, cx);
                                    } else if delta < px(0.) {
                                        this.zoom_image(-10, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            },
                        ))
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
                        ),
                )
                .scrollbar(&self.image_scroll, ScrollbarAxis::Both)
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
                                        .icon(IconName::Minus)
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
                                        .icon(IconName::Plus)
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

    fn scroll_to_top(&self) {
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    fn scroll_to_row(&self, index: usize) {
        if let Some(offset) = self.row_offsets.get(index) {
            self.scroll.set_offset(point(px(0.), -*offset));
        }
    }

    fn rebuild_row_layout(&mut self) {
        (self.row_sizes, self.row_offsets) = history_row_layout(&self.history.items, self.display);
        self.drag_motion = DragMotionState::default();
    }

    fn select(&mut self, direction: isize, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        if let Some(index) = self.history.select_relative(direction) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Top);
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
            if strategy == ScrollStrategy::Top {
                self.scroll_to_row(index);
            } else {
                self.scroll.scroll_to_item(index, strategy);
            }
        }
        cx.notify();
    }

    fn page_step(&self) -> isize {
        let (top, _) = history_top_row(&self.row_offsets, self.scroll.offset().y);
        let viewport_end = -self.scroll.offset().y + self.scroll.bounds().size.height;
        let visible = self
            .row_offsets
            .partition_point(|offset| *offset < viewport_end);
        visible.saturating_sub(top + 2).max(1) as isize
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
        let main_window = window.window_handle();
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let _ = update_clipboard_window(&view, main_window, cx, |this, window, cx| {
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

    fn refresh_tray_menu(&self) -> anyhow::Result<()> {
        if let Some(tray) = &self._tray {
            tray::update_menu(tray, self.language, self.window_pinned)?;
        }
        Ok(())
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
                if let Err(error) = self.refresh_tray_menu() {
                    self.message = format!(
                        "{}: {error}",
                        tr(
                            self.language,
                            "窗口状态已改变，但托盘菜单更新失败",
                            "Window pin changed, but the tray menu could not be updated"
                        )
                    );
                    self.is_error = true;
                }
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
                let _ = self.refresh_tray_menu();
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

    fn drop_history_card(&mut self, drag: &HistoryDrag, cx: &mut Context<Self>) {
        self.history_drag_direction = 0;
        let target = self.drop_target.take();
        if !self.valid_drag(drag) {
            self.message = tr(
                self.language,
                "列表已变化，请重新拖动",
                "The list changed; drag again.",
            )
            .into();
            self.is_error = true;
        } else if let Some(target) = target.filter(|target| target.allowed && target.id != drag.id)
            && self.send(
                Command::Reorder {
                    from: drag.id,
                    to: target.id,
                    after: target.after,
                    favorite_only: drag.favorite_only,
                    group_id: drag.group_id,
                    generation: drag.generation,
                },
                cx,
            )
        {
            self.reorder_pending = true;
            self.reorder_before = Some(self.history.items.iter().map(|item| item.id).collect());
        }
        cx.notify();
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
                        history_top_row(&this.row_offsets, this.scroll.offset().y).0
                    });
                    let next = next_drag_scroll_index(
                        current,
                        this.history.items.len(),
                        this.history_drag_direction,
                    );
                    if next != current {
                        scroll_target = Some(next);
                        this.scroll_to_row(next);
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
            None
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
        let mut summary = if self.display.show_time {
            format_card_time(
                &item.created_at,
                self.display.time_format,
                self.language,
                chrono::Local::now().naive_local(),
            )
        } else {
            kind.to_owned()
        };
        if pinned {
            summary.push_str(tr(self.language, " · 置顶", " · Pinned"));
        }
        if self.display.show_time
            && !is_image
            && !matches!(item.content_type.as_str(), "text" | "url")
        {
            summary.push_str(" · ");
            summary.push_str(kind);
        }
        let source_icon = item
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) = if self.display.show_source_app {
            source_app_parts(self.display.source_app_display, source_icon.is_some())
        } else {
            (false, false)
        };

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
        let (preview_text, highlights) = if is_image {
            (String::new(), Vec::new())
        } else {
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
            (preview_text, highlights)
        };
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
        let live_drag_source = cx.has_active_drag() && self.history.selected == Some(id);
        let color = if selected || marked {
            cx.theme().accent
        } else {
            cx.theme().background
        };
        let card_spacing = visual::card_spacing(self.display.card_density);
        let (thumbnail_width, thumbnail_height) = if is_image {
            visual::thumbnail_size(self.display.card_density)
        } else {
            (48., 36.)
        };
        let row = div()
            .id(("history-row", id as usize))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, _| this.cancel_pending_row_click()),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hover_source(id, *hovered, cx);
            }))
            .h(px(history_row_height(item, self.display)))
            .px(px(PAGE_PADDING))
            .py(px(card_spacing))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if cx.has_active_drag() {
                    return;
                }
                this.handle_row_click(id, event, window, cx);
            }))
            .child(
                div()
                    .h_full()
                    .group("")
                    .when(self.batch_mode, |card| card.px_3())
                    .when(!self.batch_mode, |card| card.pl(px(22.)).pr(px(22.)))
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
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_sm()
                            .when(is_image, |body| body.bg(cx.theme().muted))
                            .px_2()
                            .py(px(card_spacing))
                            .flex()
                            .when(is_image, |body| body.justify_center())
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
                            .when(is_image && thumbnail_path.is_none(), |body| {
                                body.child(image_unavailable)
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
                            .when(!is_image, |body| {
                                body.child(
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
                                        .child(
                                            StyledText::new(preview_text)
                                                .with_highlights(highlights),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(24.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .text_xs()
                            .text_color(if file_warning.is_some() {
                                cx.theme().danger
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(summary),
                            )
                            .when(self.display.show_source_app, |footer| {
                                footer.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .max_w(px(118.))
                                        .overflow_hidden()
                                        .group_hover("", |source| source.invisible())
                                        .when_some(
                                            show_source_icon.then_some(source_icon).flatten(),
                                            |source, path| {
                                                source.child(
                                                    img(PathBuf::from(path))
                                                        .w(px(16.))
                                                        .h(px(16.))
                                                        .flex_none()
                                                        .object_fit(ObjectFit::Contain)
                                                        .with_fallback(|| div().into_any_element()),
                                                )
                                            },
                                        )
                                        .when(
                                            show_source_name
                                                && item
                                                    .source_app_name
                                                    .as_deref()
                                                    .is_some_and(|name| !name.is_empty()),
                                            |source| {
                                                source.child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .text_ellipsis()
                                                        .child(
                                                            item.source_app_name
                                                                .clone()
                                                                .unwrap_or_default(),
                                                        ),
                                                )
                                            },
                                        ),
                                )
                            }),
                    )
                    .when(!self.batch_mode, |card| {
                        card.child(
                            div()
                                .absolute()
                                .bottom(px(card_spacing))
                                .right(px(22.))
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .rounded_sm()
                                .bg(cx.theme().background)
                                .invisible()
                                .group_hover("", |bar| bar.visible())
                                .when(selected, |bar| bar.visible())
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
                                            tr(
                                                self.language,
                                                "取消选择；Shift 点击可连选",
                                                "Deselect; Shift-click to select a range",
                                            )
                                        } else {
                                            tr(
                                                self.language,
                                                "选择；Shift 点击可连选",
                                                "Select; Shift-click to select a range",
                                            )
                                        })
                                        .accessibility_label(if marked {
                                            tr(self.language, "取消选择", "Deselect")
                                        } else {
                                            tr(self.language, "选择", "Select")
                                        })
                                        .selected(marked)
                                        .disabled(self.batch_pending || self.history.loading)
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
                                                if self.paste_target.is_some()
                                                    && self._tray.is_some()
                                                {
                                                    tr(self.language, "粘贴路径", "Paste paths")
                                                } else {
                                                    tr(self.language, "复制路径", "Copy paths")
                                                },
                                            )
                                            .accessibility_label(
                                                if self.paste_target.is_some()
                                                    && self._tray.is_some()
                                                {
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
                                                    tr(
                                                        self.language,
                                                        "粘贴纯文本",
                                                        "Paste plain text",
                                                    )
                                                } else {
                                                    tr(
                                                        self.language,
                                                        "复制纯文本",
                                                        "Copy plain text",
                                                    )
                                                })
                                                .accessibility_label(
                                                    if self.paste_target.is_some() {
                                                        tr(
                                                            self.language,
                                                            "粘贴纯文本",
                                                            "Paste plain text",
                                                        )
                                                    } else {
                                                        tr(
                                                            self.language,
                                                            "复制纯文本",
                                                            "Copy plain text",
                                                        )
                                                    },
                                                )
                                                .disabled(self.paste_pending.is_some())
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.copy_or_paste_plain_text(
                                                            id, window, cx,
                                                        );
                                                    },
                                                )),
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
                        )
                    })
                    .when(!self.batch_mode, |card| {
                        card.child(
                            div()
                                .id(("history-drag-left", id as usize))
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(show_drag_area_indicator, |handle| {
                                    handle
                                        .invisible()
                                        .group_hover("", |handle| handle.visible())
                                        .bg(cx.theme().accent)
                                        .text_color(cx.theme().primary)
                                        .child(
                                            Icon::new(gpui_kit::assets::IconName::GripVertical)
                                                .xsmall(),
                                        )
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
                                .w(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(show_drag_area_indicator, |handle| {
                                    handle
                                        .invisible()
                                        .group_hover("", |handle| handle.visible())
                                        .bg(cx.theme().accent)
                                        .text_color(cx.theme().primary)
                                        .child(
                                            Icon::new(gpui_kit::assets::IconName::GripVertical)
                                                .xsmall(),
                                        )
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
        if let Some(offset) = self
            .reorder_offsets
            .get(&id)
            .filter(|_| !cx.has_active_drag())
        {
            visual::reflow(
                row,
                *offset,
                format!("reorder-feedback-{}-{id}", self.feedback_revision),
                cx,
            )
        } else {
            let (from, to) = self.drag_motion.offsets(index, &self.row_offsets);
            if from != 0. || to != 0. {
                visual::drag_yield(
                    row,
                    from,
                    to,
                    format!("drag-yield-{}-{id}", self.drag_motion.revision),
                    cx,
                )
            } else {
                row.into_any_element()
            }
        }
    }
}

impl Render for ClipboardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .confirmation
            .is_some_and(|kind| !self.confirmation_active(kind) && !self.confirmation_pending(kind))
        {
            cx.defer_in(window, |view, window, cx| {
                view.sync_confirmation_dialog(window, cx);
            });
        }
        let drag_request = if cx.has_active_drag() {
            self.history
                .selected
                .zip(self.drop_target.filter(|target| target.allowed))
        } else {
            None
        };
        self.drag_motion.update(drag_request, &self.history.items);
        window_shell(cx)
            .font_family("Microsoft YaHei UI")
            .child(
                div()
                    .id("window-drag-handle")
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_grab()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        div()
                            .w(px(36.))
                            .h(px(4.))
                            .rounded_full()
                            .bg(cx.theme().border),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
            .child(self.dialog_layer.clone())
    }
}

impl ClipboardView {
    fn render_onboarding(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let steps = [
            (
                IconName::Copy,
                tr(language, "采集", "Capture"),
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
                tr(language, "搜索", "Search"),
                tr(language, "快速搜索", "Quick search"),
                tr(
                    language,
                    "在顶部搜索框输入关键词，历史卡片会显示匹配内容。",
                    "Search from the top field and see matches in your history cards.",
                ),
                tr(language, "聚焦搜索框", "to focus search"),
            ),
            (
                IconName::Star,
                tr(language, "收藏", "Favorites"),
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
                tr(language, "快捷键", "Shortcuts"),
                tr(language, "键盘快捷键", "Keyboard shortcuts"),
                tr(
                    language,
                    "使用键盘管理历史",
                    "Manage history with the keyboard",
                ),
                tr(
                    language,
                    "使用纯文本表示",
                    "uses the plain-text representation",
                ),
            ),
        ];
        let (icon, _, title, description, tip) = steps[self.onboarding_step.min(3)].clone();
        let is_last = self.onboarding_step == 3;
        let description_content = if self.onboarding_step == 3 {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(language, "方向键选择记录；", "Arrow keys select items;"))
                .child(shortcut_kbd("Enter").expect("onboarding enter key"))
                .child(tr(language, "复制或粘贴；", "to copy or paste;"))
                .child(shortcut_kbd("Delete").expect("onboarding delete key"))
                .child(tr(language, "删除；", "to delete;"))
                .child(shortcut_kbd("Left").expect("onboarding left key"))
                .child(tr(language, "和", "and"))
                .child(shortcut_kbd("Right").expect("onboarding right key"))
                .child(tr(language, "切换分类。", "change categories."))
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description)
                .into_any_element()
        };
        let tip_content = match self.onboarding_step {
            1 => div()
                .flex()
                .items_center()
                .gap_1()
                .child(tr(language, "按", "Press"))
                .child(shortcut_kbd("Ctrl+F").expect("onboarding search key"))
                .child(tip)
                .into_any_element(),
            3 => div()
                .flex()
                .items_center()
                .gap_1()
                .child(shortcut_kbd("Shift+Enter").expect("onboarding plain text key"))
                .child(tip)
                .into_any_element(),
            _ => div().child(tip).into_any_element(),
        };
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
                    .child(description_content)
                    .child(
                        div()
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().primary)
                            .child(tip_content),
                    )
                    .child(
                        Stepper::new("onboarding-steps")
                            .small()
                            .text_center(true)
                            .selected_index(self.onboarding_step)
                            .disabled(self.onboarding_pending)
                            .items(
                                steps
                                    .iter()
                                    .map(|(_, label, _, _, _)| StepperItem::new().child(*label)),
                            )
                            .on_click(cx.listener(|this, index, _, cx| {
                                if this.onboarding_visible() && !this.onboarding_pending {
                                    this.onboarding_step = *index;
                                    cx.notify();
                                }
                            })),
                    )
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
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "按", "Press"))
                            .child(shortcut_kbd("Esc").expect("onboarding skip key"))
                            .child(tr(language, "跳过引导", "to skip onboarding")),
                    ),
            )
    }

    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        if self.message.is_empty() && !self.reorder_pending {
            return div().into_any_element();
        }
        StatusBar::new()
            .flex_shrink_0()
            .px(px(PAGE_PADDING))
            .left(
                div()
                    .text_color(if self.is_error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(if self.reorder_pending {
                        tr(self.language, "正在保存顺序…", "Saving order…").to_owned()
                    } else {
                        self.message.clone()
                    }),
            )
            .into_any_element()
    }

    fn confirmation_active(&self, kind: HistoryConfirmation) -> bool {
        match kind {
            HistoryConfirmation::DeleteGroup(id) => {
                self.group_delete_id == Some(id) && self.history.group_id == Some(id)
            }
            HistoryConfirmation::ClearHistory(group_id) => {
                self.clear_confirm_open && self.history.group_id == group_id
            }
            HistoryConfirmation::DeleteSelected => self.batch_confirm_open,
        }
    }

    fn confirmation_pending(&self, kind: HistoryConfirmation) -> bool {
        match kind {
            HistoryConfirmation::DeleteGroup(_) => self.group_delete_pending,
            HistoryConfirmation::ClearHistory(_) => self.clear_pending,
            HistoryConfirmation::DeleteSelected => self.batch_pending,
        }
    }

    fn cancel_confirmation(&mut self, kind: HistoryConfirmation, cx: &mut Context<Self>) -> bool {
        if self.confirmation != Some(kind) || self.confirmation_pending(kind) {
            return false;
        }
        match kind {
            HistoryConfirmation::DeleteGroup(_) => self.group_delete_id = None,
            HistoryConfirmation::ClearHistory(_) => self.clear_confirm_open = false,
            HistoryConfirmation::DeleteSelected => self.batch_confirm_open = false,
        }
        self.confirmation = None;
        self.dialog_layer.update(cx, |_, cx| cx.notify());
        cx.notify();
        true
    }

    fn confirm_history_action(&mut self, kind: HistoryConfirmation, cx: &mut Context<Self>) {
        if self.confirmation != Some(kind)
            || !self.confirmation_active(kind)
            || self.confirmation_pending(kind)
        {
            return;
        }
        match kind {
            HistoryConfirmation::DeleteGroup(_) => self.delete_group(cx),
            HistoryConfirmation::ClearHistory(_) => self.clear_history(cx),
            HistoryConfirmation::DeleteSelected => self.delete_selected(cx),
        }
        if matches!(kind, HistoryConfirmation::ClearHistory(_))
            && !self.clear_pending
            && self.is_error
        {
            self.confirmation_error = Some(self.message.clone());
        }
        self.dialog_layer.update(cx, |_, cx| cx.notify());
    }

    fn sync_confirmation_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kind) = self.confirmation
            && !self.confirmation_active(kind)
            && !self.confirmation_pending(kind)
        {
            self.confirmation = None;
            window.close_dialog(cx);
            self.dialog_layer.update(cx, |_, cx| cx.notify());
        }
    }

    fn open_history_confirmation(
        &mut self,
        kind: HistoryConfirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation == Some(kind) {
            return;
        }
        if self.confirmation.is_some() {
            window.close_dialog(cx);
        }
        self.confirmation = Some(kind);
        self.confirmation_error = None;
        self.dialog_layer.update(cx, |_, cx| cx.notify());
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let view = owner.upgrade().expect("history dialog requires its owner");
            let view = view.read(cx);
            let language = view.language;
            let pending = view.confirmation_pending(kind);
            let error = view.confirmation_error.clone();
            let (title, prompt, confirm_label) = match kind {
                HistoryConfirmation::DeleteGroup(id) => {
                    let (name, count) = view.groups.iter()
                        .find(|group| group.id == id)
                        .map(|group| (group.name.as_str(), group.item_count))
                        .unwrap_or((tr(language, "该分组", "this group"), 0));
                    let prompt = if language == LanguagePreference::English {
                        format!("Delete “{name}”? {count} items will move to the default group.")
                    } else {
                        format!("删除「{name}」？{count} 条记录将移到默认分组。")
                    };
                    (tr(language, "删除分组", "Delete group"), prompt,
                     tr(language, "保留记录并删除分组", "Delete group and keep items").to_owned())
                }
                HistoryConfirmation::ClearHistory(group_id) => {
                    let name = group_id
                        .and_then(|id| view.groups.iter().find(|group| group.id == id))
                        .map(|group| group.name.as_str())
                        .unwrap_or(tr(language, "默认分组", "Default"));
                    let prompt = if language == LanguagePreference::English {
                        format!("Clear unpinned, non-favorite items from “{name}”? Search, favorites, and type filters do not change the scope.")
                    } else {
                        format!("清理「{name}」中未置顶且未收藏的记录？搜索、收藏和类型筛选不影响清理范围。")
                    };
                    (tr(language, "清理历史", "Clear history"), prompt,
                     tr(language, "确认清理", "Clear").to_owned())
                }
                HistoryConfirmation::DeleteSelected => {
                    let count = view.selected_ids.len();
                    let prompt = if language == LanguagePreference::English {
                        format!("Delete the {count} selected items? This includes pinned and favorite items and cannot be undone.")
                    } else {
                        format!("确定删除选中的 {count} 条记录？包含置顶和收藏，无法撤销。")
                    };
                    let confirm = if language == LanguagePreference::English {
                        format!("Delete {count} items")
                    } else {
                        format!("确认删除 {count} 条")
                    };
                    (tr(language, "删除选中", "Delete selected"), prompt, confirm)
                }
            };
            let confirm_owner = owner.clone();
            let keyboard_owner = owner.clone();
            let cancel_owner = owner.clone();
            dialog
                .title(title)
                .close_button(false)
                .overlay_closable(false)
                .on_ok(move |_, _, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| view.confirm_history_action(kind, cx));
                    false
                })
                .on_cancel(move |_, _, cx| {
                    cancel_owner.update(cx, |view, cx| view.cancel_confirmation(kind, cx)).unwrap_or(true)
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(prompt)
                        .when(matches!(kind, HistoryConfirmation::ClearHistory(_)), |body| {
                            body.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(
                                        language,
                                        "此操作无法撤销；可先导出备份。",
                                        "This cannot be undone. Export a backup first if needed.",
                                    )),
                            )
                        })
                        .when_some(error, |body, error| {
                            body.child(Alert::error("history-confirm-error", error).small())
                        }),
                )
                .footer(DialogFooter::new()
                    .child(Button::new("history-confirm-cancel")
                        .outline()
                        .label(tr(language, "取消", "Cancel"))
                        .disabled(pending)
                        .on_click({
                            let owner = owner.clone();
                            move |_, window, cx| {
                                if owner.update(cx, |view, cx| view.cancel_confirmation(kind, cx)).unwrap_or(false) {
                                    window.close_dialog(cx);
                                }
                            }
                        }))
                    .child(Button::new("history-confirm-submit")
                        .danger()
                        .label(if pending {
                            match kind {
                                HistoryConfirmation::DeleteGroup(_) => tr(
                                    language,
                                    "正在删除分组…",
                                    "Deleting group…",
                                ),
                                HistoryConfirmation::ClearHistory(_) => {
                                    tr(language, "正在清理…", "Clearing…")
                                }
                                HistoryConfirmation::DeleteSelected => {
                                    tr(language, "正在删除…", "Deleting…")
                                }
                            }
                            .to_owned()
                        } else {
                            confirm_label
                        })
                        .disabled(pending)
                        .on_click(move |_, _, cx| {
                            let _ = confirm_owner.update(cx, |view, cx| view.confirm_history_action(kind, cx));
                        })))
        });
        cx.notify();
    }

    fn open_clear_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group_save_pending
            || self.group_delete_pending
            || self.clear_pending
            || self.batch_pending
        {
            return;
        }
        self.reset_selection();
        self.group_delete_id = None;
        self.group_editor_open = false;
        self.group_rename_id = None;
        self.group_move_id = None;
        self.preview.close();
        window.focus(&self.list_focus, cx);
        if self.skip_clear_confirm {
            self.clear_confirm_open = false;
            self.sync_confirmation_dialog(window, cx);
            self.clear_history(cx);
        } else {
            self.clear_confirm_open = true;
            self.open_history_confirmation(
                HistoryConfirmation::ClearHistory(self.history.group_id),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.onboarding_visible() {
            return self.render_onboarding(cx).into_any_element();
        }
        if self.preview.id.is_some() {
            return visual::reveal(
                self.render_preview(cx),
                ("preview-enter", self.preview.generation as usize),
                cx,
            );
        }
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
        div()
            .key_context("ClipboardApp")
            .flex()
            .flex_col()
            .size_full()
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
                    .pb_2()
                    .on_action(cx.listener(|this, _: &FocusFirstHistoryItem, window, cx| {
                        this.focus_first_history_item(window, cx);
                    }))
                    .child(Input::new(&self.search).cleanable(true)),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .w_full()
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
                            .items_center()
                            .gap_2()
                            .when(self.display.show_category_filter, |row| {
                                let selected_index = self
                                    .history
                                    .group_id
                                    .is_none()
                                    .then(|| self.category_tab_index());
                                row.child(
                                    div().flex_1().min_w_0().child(
                                        TabBar::new("history-category-tabs")
                                            .w_full()
                                            .segmented()
                                            .when_some(selected_index, |tabs, index| {
                                                tabs.selected_index(index)
                                            })
                                            .on_click(cx.listener(
                                                |this, index: &usize, window, cx| {
                                                    if let Some(&category) =
                                                        CATEGORY_FILTERS.get(*index)
                                                    {
                                                        this.select_category(category, window, cx);
                                                    }
                                                },
                                            ))
                                            .children(
                                                [
                                                    ("全部", "All", "All"),
                                                    ("收藏", "Favs", "Favorites"),
                                                    ("文本", "Text", "Text"),
                                                    ("其他", "Other", "Other"),
                                                ]
                                                .map(|(chinese, english, accessible)| {
                                                    Tab::new()
                                                        .label(tr(self.language, chinese, english))
                                                        .aria_label(tr(
                                                            self.language,
                                                            chinese,
                                                            accessible,
                                                        ))
                                                        .flex_1()
                                                }),
                                            ),
                                    ),
                                )
                            })
                            .child(
                                div().w(px(138.)).flex_none().child(
                                    Select::new(&self.group_select)
                                        .w_full()
                                        .menu_width(px(190.))
                                        .menu_max_h(px(300.))
                                        .placeholder(tr(self.language, "选择分组", "Select group"))
                                        .accessibility_label(tr(self.language, "分组", "Group"))
                                        .disabled(
                                            self.group_save_pending
                                                || self.group_delete_pending
                                                || self.clear_pending
                                                || self.group_reorder_pending,
                                        ),
                                ),
                            ),
                    ),
            )
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
                            Button::new("batch-exit")
                                .small()
                                .ghost()
                                .label(tr(self.language, "退出多选", "Exit selection"))
                                .disabled(self.batch_pending)
                                .on_click(cx.listener(|this, _, _, cx| this.exit_batch_mode(cx))),
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
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.batch_confirm_open = true;
                                    this.clear_confirm_open = false;
                                    this.group_delete_id = None;
                                    this.open_history_confirmation(
                                        HistoryConfirmation::DeleteSelected,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                    ("batch-toolbar", self.history.group_id.unwrap_or(0) as usize),
                    cx,
                ))
            })
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex_1()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .window_control_area(WindowControlArea::Drag)
                            .child(if self.language == LanguagePreference::English {
                                format!("{} items", self.history.total)
                            } else {
                                format!("{} 条记录", self.history.total)
                            }),
                    )
                    .when(self.history.items.len() > 8, |bar| {
                        bar.child(
                            Button::new("history-scroll-to-top")
                                .small()
                                .ghost()
                                .label(tr(self.language, "返回顶部 ↑", "Back to top ↑"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.scroll_to_top();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("history-content")
                    .key_context("HistoryList")
                    .track_focus(&self.list_focus)
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.window_pinned
                            && this._tray.is_some()
                            && this.settings_window.is_none()
                            && !this.settings_window_opening
                        {
                            this.hide_visible_window(window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &PreviewSelected, window, cx| {
                        if !this.batch_mode
                            && let Some(id) = this.history.selected
                        {
                            this.open_preview(id, window, cx);
                        }
                    }))
                    .on_drag_move(
                        cx.listener(|this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
                            let y = event.event.position.y;
                            let inside = event.bounds.contains(&event.event.position);
                            let direction = if !inside {
                                0
                            } else if y < event.bounds.top() + px(visual::DRAG_EDGE_ZONE) {
                                -1
                            } else if y > event.bounds.bottom() - px(visual::DRAG_EDGE_ZONE) {
                                1
                            } else {
                                0
                            };
                            let target = if inside {
                                history_drop_row(
                                    &this.row_offsets,
                                    this.scroll.offset().y,
                                    event.bounds.top(),
                                    y,
                                )
                                .and_then(|(index, after)| {
                                    let id = this.history.items.get(index)?.id;
                                    (id != event.drag(cx).id).then_some(DropTarget {
                                        id,
                                        after,
                                        allowed: this.valid_drag(event.drag(cx)),
                                    })
                                })
                            } else {
                                None
                            };
                            if this.history_drag_direction != direction
                                || this.drop_target != target
                            {
                                this.history_drag_direction = direction;
                                this.drop_target = target;
                                cx.notify();
                            }
                        }),
                    )
                    .on_drop(cx.listener(|this, drag: &HistoryDrag, _, cx| {
                        this.drop_history_card(drag, cx);
                    }))
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
                        if !this.batch_mode
                            && let Some(id) = this.history.selected
                        {
                            this.copy_or_paste_plain_text(id, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &PasteSelected, window, cx| {
                        if !this.batch_mode
                            && let Some(id) = this.history.selected
                        {
                            this.paste_selected(id, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                        if !this.batch_mode
                            && let Some(id) = this.history.selected
                        {
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
                            div()
                                .relative()
                                .size_full()
                                .child(
                                    v_virtual_list(
                                        cx.entity().clone(),
                                        "history-virtual-list",
                                        self.row_sizes.clone(),
                                        |this, visible, _, cx| {
                                            visible
                                                .map(|index| this.render_row(index, cx))
                                                .collect()
                                        },
                                    )
                                    .track_scroll(&self.scroll)
                                    .size_full(),
                                )
                                .vertical_scrollbar(&self.scroll),
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
