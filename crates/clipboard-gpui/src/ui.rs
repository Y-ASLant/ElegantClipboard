mod confirmation;
mod content;
mod data_import;
mod drag;
mod drag_visual;
mod events;
mod external_drag;
mod file_cards;
mod file_preview;
mod group_select;
mod groups;
mod history_card;
mod history_events;
mod history_list;
mod hover_lifecycle;
mod hover_window;
mod image_assets;
mod navigation;
mod onboarding;
mod paste_actions;
mod paste_requests;
mod preference_events;
mod presentation;
mod preview_actions;
mod preview_render;
mod row_actions;
mod row_menu;
mod search_input;
mod selection;
mod settings_about;
mod settings_app_filter;
mod settings_audio;
mod settings_common;
mod settings_data;
mod settings_display;
mod settings_file_staging;
mod settings_general;
mod settings_hover;
mod settings_layout;
mod settings_retention;
mod settings_shortcuts;
mod settings_window;
mod settings_window_effect;
mod shortcut_events;
mod shortcut_labels;
mod shortcuts;
mod startup;
mod storage;
mod thumbnail_cache;
mod view_init;
mod window_lifecycle;
mod window_render;

pub use startup::run;

use file_cards::{image_file_dimensions, single_file_image_path, single_image_file_paths};
use navigation::{history_row_height, history_top_row};
use presentation::{
    apply_theme, format_bytes, init_window_effect, localize_service_message, tr, window_shell,
};
use shortcut_labels::{
    hotkey_label, paste_registration_summary, shortcut_from_keystroke, shortcut_kbd,
};
use window_lifecycle::update_clipboard_window;

use crate::acrylic;
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
        next_drag_scroll_index, reorder_pixel_offsets, selection_range_ids, source_app_parts,
    },
};
use clipboard_core::{
    ContentCategory, FilePreviewEntry, HISTORY_LIMIT, PAGE_SIZE, PreviewContent,
    backup::BackupReport,
    database::{ClipboardItem as DbClipboardItem, Group},
    preferences::{
        AppFilterMode, AppFilterPreference, AudioPreference, CardDensity, DisplayPreference,
        HotkeyPreference, HoverPreviewPosition, HoverPreviewPreference, LanguagePreference,
        MonitorTypesPreference, PasteKeyPreference, PasteShortcutConfig, SoundTiming,
        SourceAppDisplay, ThemePreference, TimeFormat, WindowEffect, WindowEffectPreference,
        WindowPositionPreference, WindowSizePreference,
    },
};
use clipboard_platform::hotkey::{
    Hotkey, PasteHotkeyEvent, PasteHotkeys, PasteRegistrationWarning, normalize_paste_shortcut,
    validate_paste_shortcuts,
};
use clipboard_platform::outside_click::OutsideClickMonitor;
use clipboard_platform::source_app::RunningApp;
use clipboard_platform::{Command, DataSizeInfo, Event, FailureKind, Service};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::scroll::{ScrollableElement, ScrollbarAxis};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
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

const CATEGORY_FILTERS: [Option<ContentCategory>; 4] = [
    Some(ContentCategory::All),
    None,
    Some(ContentCategory::Text),
    Some(ContentCategory::Other),
];

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

#[derive(Clone)]
struct HistoryDrag {
    id: i64,
    pinned: bool,
    favorite_only: bool,
    group_id: Option<i64>,
    generation: u64,
    card: drag_visual::HistoryCardSnapshot,
    size: Rc<Cell<gpui_kit::Size<Pixels>>>,
    native_preview: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropTarget {
    id: i64,
    after: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DragPlacement {
    source: usize,
    insertion: usize,
}

#[derive(Default)]
struct DragMotionState {
    request: Option<(i64, DropTarget)>,
    hovered: Option<i64>,
    started: Option<std::time::Instant>,
    from_offsets: HashMap<usize, f32>,
    to: Option<DragPlacement>,
    progress: Rc<visual::DragProgress>,
}

enum HistoryMenuAction {
    Paste,
    PastePlainText,
    Copy,
    CopyPath,
    CopyAsFile,
    Preview,
    Edit,
    Reveal,
    SaveAs(String),
    ToggleFavorite,
    TogglePin,
    MoveToGroup,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryConfirmation {
    DeleteGroup(i64),
    ClearHistory(Option<i64>),
    DeleteSelected,
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
    group_delete_pending: bool,
    clear_pending: bool,
    clear_all_error: Option<String>,
    clear_all_pending: bool,
    batch_mode: bool,
    selected_ids: HashSet<i64>,
    selection_anchor: Option<i64>,
    batch_pending: bool,
    batch_paste_pending: Option<(isize, u32)>,
    confirmation: Option<HistoryConfirmation>,
    confirmation_error: Option<String>,
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
    /// 可见区及预取区的图片路径，另保留拖动和预览使用的资源。
    rendered_images: HashSet<PathBuf>,
    image_visible_range: std::ops::Range<usize>,
    image_sync_pending: bool,
    image_release_task: Option<Task<()>>,
    thumbnails: thumbnail_cache::ThumbnailCache,
    thumbnail_scale: f32,
    _thumbnail_maintenance: Task<()>,
    hover_preview: PreviewState,
    hover_popup: Option<AnyWindowHandle>,
    hover_popup_opening: Option<(i64, u64)>,
    hover_source_active: bool,
    hover_popup_active: bool,
    hover_task: Option<Task<()>>,
    hover_close_task: Option<Task<()>>,
    /// 悬停预览「无操作自动关闭」的计时任务；每次鼠标活动都会重置它。
    hover_autoclose_task: Option<Task<()>>,
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
    window_effect: WindowEffectPreference,
    window_effect_pending: bool,
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
    quick_paste_pending: paste_requests::PendingRequest<(isize, u32)>,
    autostart: PendingPref<bool>,
    data_size: Option<DataSizeInfo>,
    daily_counts: Option<Vec<(String, i64)>>,
    data_size_pending: bool,
    database_maintenance_pending: bool,
    export_pending: bool,
    data_import_pending: bool,
    data_import_result: Option<data_import::ImportOutcome>,
    window_position: PendingPref<WindowPositionPreference>,
    persist_window_size: bool,
    persist_window_size_pending: bool,
    auto_reset_state: PendingPref<bool>,
    search_auto_focus: PendingPref<bool>,
    search_auto_clear: PendingPref<bool>,
    skip_clear_confirm: PendingPref<bool>,
    paste_close_window: PendingPref<bool>,
    paste_key: PendingPref<PasteKeyPreference>,
    paste_move_to_top: PendingPref<bool>,
    display: DisplayPreference,
    display_pending: bool,
    audio: PendingPref<AudioPreference>,
    retention: PendingPref<clipboard_core::retention::RetentionPolicy>,
    retention_saved: bool,
    stage_files: PendingPref<bool>,
    monitor_types: PendingPref<MonitorTypesPreference>,
    app_filter: PendingPref<AppFilterPreference>,
    running_apps: Vec<RunningApp>,
    running_apps_pending: bool,
    last_window_size: Option<WindowSizePreference>,
    paste_target: Option<(isize, u32)>,
    paste_pending: paste_requests::PendingRequest<(isize, u32)>,
    paste_delivery_task: Option<Task<()>>,
    reorder_pending: bool,
    drop_target: Option<DropTarget>,
    drag_motion: DragMotionState,
    reorder_before: Option<Vec<i64>>,
    reorder_offsets: HashMap<i64, f32>,
    feedback_revision: usize,
    history_drag_direction: i8,
    external_drag: Option<external_drag::CardDragSession>,
    drag_preview_window: Option<WindowHandle<external_drag::DragPreview>>,
    drag_preparation: Option<external_drag::DragPreparation>,
    drag_snapshot: Option<(Vec<DbClipboardItem>, i64, u64)>,
    drag_deleted_ids: HashSet<i64>,
    drag_viewport: Rc<Cell<Option<Bounds<Pixels>>>>,
    drag_release_pending: bool,
    drag_token: u64,
    paused: bool,
    pause_pending: bool,
    message: String,
    is_error: bool,
    feedback_pending: bool,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
    search_task: Option<Task<()>>,
    feedback_task: Option<Task<()>>,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShortcutTarget {
    Main,
    Paste(bool, u8),
}

struct SettingsWindowView {
    owner: WeakEntity<ClipboardView>,
    _owner_subscription: Subscription,
    _owner_release_subscription: Subscription,
    language: LanguagePreference,
    page: SettingsPage,
    shortcut_input: Option<Entity<InputState>>,
    shortcut_editing: Option<ShortcutTarget>,
    shortcut_capture_focus: FocusHandle,
    shortcut_recording: bool,
    shortcut_capture_error: Option<String>,
    recent_shortcuts_expanded: bool,
    favorite_shortcuts_expanded: bool,
    app_filter_input: Entity<InputState>,
    _app_filter_input_subscription: Subscription,
    retention_days_input: Entity<InputState>,
    retention_count_input: Entity<InputState>,
    retention_error: Option<String>,
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
    /// 纯文本预览正文（文本/富文本内容，或文件路径列表）。
    ///
    /// 刻意**不用** `Textarea`：悬停预览是「看一眼就走」的表面，用可聚焦的文本框会
    /// 一点击就抢焦点、变出可编辑的样子，还会带上竖直/水平滚动条（就是被当成
    /// 「右下多两条灰边」的东西）。要选中/编辑请用主预览（空格或「查看详情」）。
    text: SharedString,
    /// 图片缩放倍率，1.0 = 100%。
    zoom: f32,
    /// 拖拽平移的起点与该次拖拽开始时的滚动偏移。
    pan_from: Option<(Point<Pixels>, Point<Pixels>)>,
    image_scroll: ScrollHandle,
    zoom_step: u8,
}

/// 单值偏好及其在途保存标记：`value` 是当前生效值，
/// `pending` 表示保存命令已发出但尚未收到确认。
#[derive(Debug, Clone, Copy)]
struct PendingPref<T> {
    value: T,
    pending: bool,
}

/// 主窗口上可被 Esc 逐层关闭的覆盖层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Confirmation(HistoryConfirmation),
    GroupMove,
    GroupEditor,
    BatchMode,
}

/// 音效设置的通道：复制与粘贴音效共用同一套界面，仅 id、
/// 字段与试听音不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundChannel {
    Copy,
    Paste,
}
