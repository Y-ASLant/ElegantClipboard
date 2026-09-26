use crate::{
    autostart,
    instance::{self, InstanceSignal},
    source_app,
};
use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Context, Result, anyhow, bail};
#[cfg(test)]
use clipboard_core::FilePreviewEntry;
use clipboard_core::{
    ContentCategory, History, MAX_FILE_PATHS, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS,
    MAX_PATH_LIST_BYTES, MAX_TEXT_BYTES, PAGE_SIZE, PreviewContent,
    backup::{BackupReport, RestoreReport, restore_backup},
    database::{ClipboardItem, Database, Group},
    import::{ImportReport, import_legacy_database},
    is_url_text,
    legacy_backup::{LegacyBackupReport, import_legacy_backup},
    preferences::{
        AppFilterPreference, AudioPreference, DisplayPreference, HotkeyPreference,
        HoverPreviewPreference, LanguagePreference, MonitorTypesPreference, PasteKeyPreference,
        PasteShortcutConfig, Preferences, ThemePreference, ToolbarPreference,
        WindowPositionPreference, WindowSizePreference,
    },
};
use clipboard_rs::{
    Clipboard, ClipboardContent, ClipboardContext, ClipboardHandler, ClipboardWatcher,
    ClipboardWatcherContext, ContentFormat, RustImageData, WatcherShutdown, common::RustImage,
};
use directories::ProjectDirs;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub enum Command {
    Query {
        search: String,
        favorite_only: bool,
        category: ContentCategory,
        group_id: Option<i64>,
        limit: i64,
        generation: u64,
    },
    Copy(i64),
    CopyPlainText(i64),
    CopyPlainTextForPaste(i64),
    CopyForPaste(i64),
    CopyPath(i64),
    CopyPathForPaste(i64),
    RevealInExplorer(i64),
    SaveAs {
        id: i64,
        destination: PathBuf,
    },
    MergeCopy(Vec<i64>),
    MergeForPaste(Vec<i64>),
    CreateGroup(String),
    ReorderGroup {
        from: i64,
        to: i64,
        after: bool,
    },
    RenameGroup {
        id: i64,
        name: String,
    },
    DeleteGroup {
        id: i64,
        generation: u64,
    },
    MoveToGroup {
        id: i64,
        source_group_id: Option<i64>,
        target_group_id: Option<i64>,
        generation: u64,
    },
    Preview {
        id: i64,
        generation: u64,
    },
    HoverPreview {
        id: i64,
        generation: u64,
    },
    EditText {
        id: i64,
        expected_hash: String,
        new_text: String,
        generation: u64,
    },
    Delete(i64),
    DeleteBatch {
        ids: Vec<i64>,
        group_id: Option<i64>,
        generation: u64,
    },
    ClearHistory {
        group_id: Option<i64>,
        generation: u64,
    },
    ClearAllHistory,
    TogglePin(i64),
    ToggleFavorite(i64),
    Pause(bool),
    SetTheme(ThemePreference),
    SetLanguage(LanguagePreference),
    SetHotkey(HotkeyPreference),
    SetWindowSize(WindowSizePreference),
    SetPersistWindowSize(bool),
    SetAutoResetState(bool),
    SetSearchAutoFocus(bool),
    SetSearchAutoClear(bool),
    SetSkipClearConfirm(bool),
    SetPasteCloseWindow(bool),
    SetPasteKey(PasteKeyPreference),
    SetPasteMoveToTop(bool),
    SetQuickPasteEnabled(bool),
    SetPasteShortcuts(Box<PasteShortcutConfig>),
    SetWindowPosition(WindowPositionPreference),
    SetHoverPreview(HoverPreviewPreference),
    SetToolbar(ToolbarPreference),
    SetDisplay(DisplayPreference),
    SetAudio(AudioPreference),
    SetMonitorTypes(MonitorTypesPreference),
    SetAppFilter(Box<AppFilterPreference>),
    ListRunningApps,
    CompleteOnboarding,
    BumpToTop(i64),
    ResolveQuickPaste {
        slot: u8,
        favorite: bool,
        group_id: Option<i64>,
    },
    SetAutostart(bool),
    QueryDataSize,
    OptimizeDatabase,
    OpenDataDirectory,
    ExportBackup(PathBuf),
    Reorder {
        from: i64,
        to: i64,
        after: bool,
        favorite_only: bool,
        group_id: Option<i64>,
        generation: u64,
    },
    Capture(String),
    CaptureRich {
        html: Option<String>,
        rtf: Option<Vec<u8>>,
        text: Option<String>,
    },
    CaptureFiles(Vec<String>),
    CaptureImage {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
    ObservedCapture {
        content: CapturedClipboard,
        source: Option<source_app::SourceApp>,
    },
}

pub enum CapturedClipboard {
    Text(String),
    Rich {
        html: Option<String>,
        rtf: Option<Vec<u8>>,
        text: Option<String>,
    },
    Files(Vec<String>),
    Image {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
}

fn filter_observed_capture(
    content: CapturedClipboard,
    allowed: MonitorTypesPreference,
) -> Option<CapturedClipboard> {
    match content {
        CapturedClipboard::Text(text) => {
            let enabled = if is_url_text(&text) {
                allowed.url
            } else {
                allowed.text
            };
            enabled.then_some(CapturedClipboard::Text(text))
        }
        CapturedClipboard::Rich { html, rtf, text } => {
            let html = html.filter(|_| allowed.html);
            let rtf = rtf.filter(|_| allowed.rtf);
            if html.is_some() || rtf.is_some() {
                Some(CapturedClipboard::Rich { html, rtf, text })
            } else {
                text.and_then(|text| {
                    filter_observed_capture(CapturedClipboard::Text(text), allowed)
                })
            }
        }
        CapturedClipboard::Image { .. } if !allowed.image => None,
        CapturedClipboard::Files(_) if !allowed.files => None,
        content => Some(content),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Query(u64),
    Paste(i64),
    Merge,
    GroupSave,
    GroupDelete,
    GroupMove,
    ClearHistory,
    ClearAllHistory,
    BatchDelete,
    EditText { id: i64, generation: u64 },
    SaveAs(i64),
    DataSize,
    DatabaseMaintenance,
    Pause,
    Other,
}

impl Command {
    fn failure_kind(&self) -> Option<FailureKind> {
        match self {
            Self::Capture(_)
            | Self::CaptureRich { .. }
            | Self::CaptureFiles(_)
            | Self::CaptureImage { .. }
            | Self::ObservedCapture { .. } => None,
            Self::Query { generation, .. } => Some(FailureKind::Query(*generation)),
            Self::CopyForPaste(id)
            | Self::CopyPlainTextForPaste(id)
            | Self::CopyPathForPaste(id) => Some(FailureKind::Paste(*id)),
            Self::SaveAs { id, .. } => Some(FailureKind::SaveAs(*id)),
            Self::MergeCopy(_) | Self::MergeForPaste(_) => Some(FailureKind::Merge),
            Self::CreateGroup(_) | Self::RenameGroup { .. } => Some(FailureKind::GroupSave),
            Self::ReorderGroup { .. } => Some(FailureKind::Other),
            Self::DeleteGroup { .. } => Some(FailureKind::GroupDelete),
            Self::MoveToGroup { .. } => Some(FailureKind::GroupMove),
            Self::ClearHistory { .. } => Some(FailureKind::ClearHistory),
            Self::ClearAllHistory => Some(FailureKind::ClearAllHistory),
            Self::DeleteBatch { .. } => Some(FailureKind::BatchDelete),
            Self::EditText { id, generation, .. } => Some(FailureKind::EditText {
                id: *id,
                generation: *generation,
            }),
            Self::Pause(_) => Some(FailureKind::Pause),
            Self::QueryDataSize => Some(FailureKind::DataSize),
            Self::OptimizeDatabase => Some(FailureKind::DatabaseMaintenance),
            Self::Copy(_)
            | Self::CopyPlainText(_)
            | Self::CopyPath(_)
            | Self::RevealInExplorer(_)
            | Self::Preview { .. }
            | Self::HoverPreview { .. }
            | Self::Delete(_)
            | Self::TogglePin(_)
            | Self::ToggleFavorite(_)
            | Self::Reorder { .. }
            | Self::SetTheme(_)
            | Self::SetLanguage(_)
            | Self::SetHotkey(_)
            | Self::SetWindowSize(_)
            | Self::SetPersistWindowSize(_)
            | Self::SetAutoResetState(_)
            | Self::SetSearchAutoFocus(_)
            | Self::SetSearchAutoClear(_)
            | Self::SetSkipClearConfirm(_)
            | Self::SetPasteCloseWindow(_)
            | Self::SetPasteKey(_)
            | Self::SetPasteMoveToTop(_)
            | Self::SetQuickPasteEnabled(_)
            | Self::SetPasteShortcuts(_)
            | Self::SetWindowPosition(_)
            | Self::SetHoverPreview(_)
            | Self::SetToolbar(_)
            | Self::SetDisplay(_)
            | Self::SetAudio(_)
            | Self::SetMonitorTypes(_)
            | Self::SetAppFilter(_)
            | Self::ListRunningApps
            | Self::CompleteOnboarding
            | Self::BumpToTop(_)
            | Self::ResolveQuickPaste { .. }
            | Self::SetAutostart(_)
            | Self::OpenDataDirectory
            | Self::ExportBackup(_) => Some(FailureKind::Other),
        }
    }
}

pub enum Event {
    ShowWindow,
    Groups(Vec<Group>),
    GroupCreated(Result<Group, String>),
    GroupReordered {
        from: i64,
        result: Result<(), String>,
    },
    GroupRenamed(Result<Group, String>),
    GroupDeleted(Result<usize, String>),
    ItemMoved {
        id: i64,
        result: Result<(), String>,
    },
    Snapshot {
        items: Vec<ClipboardItem>,
        total: i64,
        generation: u64,
    },
    Status(String),
    Copied {
        id: i64,
        for_paste: bool,
        clipboard_sequence: u32,
        message: String,
    },
    Merged {
        for_paste: bool,
        clipboard_sequence: u32,
        item_count: usize,
    },
    Reordered {
        from: i64,
        generation: u64,
        result: Result<(), String>,
    },
    Preview {
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
    },
    HoverPreview {
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
    },
    TextEdited {
        id: i64,
        generation: u64,
        result: Result<bool, String>,
    },
    SavedAs {
        id: i64,
        result: Result<PathBuf, String>,
    },
    HistoryCleared(Result<i64, String>),
    AllHistoryCleared(Result<i64, String>),
    BatchDeleted(Result<i64, String>),
    Paused(bool),
    ThemeSaved(Result<ThemePreference, String>),
    LanguageSaved(Result<LanguagePreference, String>),
    HotkeySaved(Result<HotkeyPreference, String>),
    WindowSizeSaved(Result<WindowSizePreference, String>),
    PersistWindowSizeSaved(Result<bool, String>),
    AutoResetStateSaved(Result<bool, String>),
    SearchAutoFocusSaved(Result<bool, String>),
    SearchAutoClearSaved(Result<bool, String>),
    SkipClearConfirmSaved(Result<bool, String>),
    PasteCloseWindowSaved(Result<bool, String>),
    PasteKeySaved(Result<PasteKeyPreference, String>),
    PasteMoveToTopSaved(Result<bool, String>),
    QuickPasteEnabledSaved(Result<bool, String>),
    PasteShortcutsSaved(Result<Box<PasteShortcutConfig>, String>),
    QuickPasteResolved {
        slot: u8,
        favorite: bool,
        result: Result<i64, String>,
    },
    WindowPositionSaved(Result<WindowPositionPreference, String>),
    HoverPreviewSaved(Result<HoverPreviewPreference, String>),
    ToolbarSaved(Result<ToolbarPreference, String>),
    DisplaySaved(Result<DisplayPreference, String>),
    AudioSaved(Result<AudioPreference, String>),
    MonitorTypesSaved(Result<MonitorTypesPreference, String>),
    AppFilterSaved(Result<Box<AppFilterPreference>, String>),
    RunningApps(Vec<source_app::RunningApp>),
    OnboardingCompleted(Result<(), String>),
    AutostartSaved(Result<bool, String>),
    DataSize(Result<DataSizeInfo, String>),
    DatabaseOptimized(DataSizeInfo),
    BackupExported(Result<BackupReport, String>),
    BackgroundError(String),
    CommandFailed {
        kind: FailureKind,
        message: String,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataSizeInfo {
    pub database_bytes: u64,
    pub image_bytes: u64,
    pub image_count: u64,
    pub staged_bytes: u64,
    pub staged_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug)]
pub struct InstanceBusy;

impl std::fmt::Display for InstanceBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("该数据目录已由另一个基础版实例使用")
    }
}

impl std::error::Error for InstanceBusy {}

#[derive(Default)]
struct CaptureState {
    paused: bool,
    ignored_sequence: u32,
    last_sequence: u32,
    pending_image_bytes: usize,
}

const MAX_PENDING_IMAGE_BYTES: usize = 100 * 1024 * 1024;

struct WatchHandler {
    clipboard: ClipboardContext,
    state: Arc<Mutex<CaptureState>>,
    commands: SyncSender<Command>,
    events: async_channel::Sender<Event>,
}

impl ClipboardHandler for WatchHandler {
    fn on_clipboard_change(&mut self) {
        let result = self.read_change();
        if let Err(error) = result {
            let _ = self
                .events
                .try_send(Event::BackgroundError(error.to_string()));
        }
    }
}

impl WatchHandler {
    fn read_change(&mut self) -> Result<()> {
        // Serialize our writes with capture so their sequence is suppressed before
        // a delayed WM_CLIPBOARDUPDATE callback can observe the new text.
        let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if state.paused || sequence == state.ignored_sequence || sequence == state.last_sequence {
            return Ok(());
        }
        let source = source_app::clipboard_source();
        if self.clipboard.has(ContentFormat::Files) {
            let paths = self
                .clipboard
                .get_files()
                .map_err(|error| anyhow!("读取剪贴板文件失败：{error}"))?;
            if paths.is_empty()
                || paths.len() > MAX_FILE_PATHS
                || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_LIST_BYTES
            {
                state.last_sequence = sequence;
                bail!("文件路径列表为空或超过限制，本次复制未保存");
            }
            self.commands
                .try_send(Command::ObservedCapture {
                    content: CapturedClipboard::Files(paths),
                    source,
                })
                .map_err(|_| anyhow!("采集队列已满或已停止，本次文件未保存"))?;
            state.last_sequence = sequence;
            return Ok(());
        }
        let html = self
            .clipboard
            .has(ContentFormat::Html)
            .then(|| self.clipboard.get_html().ok())
            .flatten()
            .filter(|value| !value.is_empty());
        let rtf = self
            .clipboard
            .has(ContentFormat::Rtf)
            .then(|| self.clipboard.get_buffer("Rich Text Format").ok())
            .flatten()
            .filter(|value| !value.is_empty());
        let text = self
            .clipboard
            .has(ContentFormat::Text)
            .then(|| self.clipboard.get_text().ok())
            .flatten()
            .filter(|value| !value.is_empty());
        if html.is_some() || rtf.is_some() {
            let bytes = html.as_ref().map_or(0, String::len)
                + rtf.as_ref().map_or(0, Vec::len)
                + text.as_ref().map_or(0, String::len);
            if bytes <= MAX_TEXT_BYTES {
                self.commands
                    .try_send(Command::ObservedCapture {
                        content: CapturedClipboard::Rich { html, rtf, text },
                        source,
                    })
                    .map_err(|_| anyhow!("采集队列已满或已停止，本次富文本未保存"))?;
                state.last_sequence = sequence;
                return Ok(());
            }
            if text.is_none() {
                state.last_sequence = sequence;
                bail!("富文本超过 1 MiB 且没有纯文本，未保存");
            }
            let _ = self
                .events
                .try_send(Event::Status("富文本超过 1 MiB，按纯文本保存".into()));
        }
        if let Some(text) = text {
            if text.len() > MAX_TEXT_BYTES {
                state.last_sequence = sequence;
                bail!("文本超过 1 MiB，未保存");
            }
            self.commands
                .try_send(Command::ObservedCapture {
                    content: CapturedClipboard::Text(text),
                    source,
                })
                .map_err(|_| anyhow!("采集队列已满或已停止，本次复制未保存"))?;
            state.last_sequence = sequence;
            return Ok(());
        }
        if self.clipboard.has(ContentFormat::Image) {
            let image = self
                .clipboard
                .get_image()
                .map_err(|error| anyhow!("读取剪贴板图片失败：{error}"))?;
            let (width, height) = image.get_size();
            if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
            {
                state.last_sequence = sequence;
                bail!("图片尺寸无效或超过 2500 万像素");
            }
            let png = image
                .to_png()
                .map_err(|error| anyhow!("编码剪贴板图片失败：{error}"))?;
            let bytes = png.get_bytes();
            if bytes.len() > MAX_IMAGE_BYTES {
                state.last_sequence = sequence;
                bail!("图片超过 50 MiB，未保存");
            }
            if state.pending_image_bytes.saturating_add(bytes.len()) > MAX_PENDING_IMAGE_BYTES {
                state.last_sequence = sequence;
                bail!("图片采集队列已满，本次复制未保存");
            }
            state.pending_image_bytes += bytes.len();
            if self
                .commands
                .try_send(Command::ObservedCapture {
                    content: CapturedClipboard::Image {
                        png: bytes.to_vec(),
                        width,
                        height,
                    },
                    source,
                })
                .is_err()
            {
                state.pending_image_bytes -= bytes.len();
                bail!("采集队列已满或已停止，本次图片未保存");
            }
            state.last_sequence = sequence;
            return Ok(());
        }
        state.last_sequence = sequence;
        Ok(())
    }
}

pub struct Service {
    commands: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    shutdown: Option<WatcherShutdown>,
    watcher: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
    instance_signal: Option<InstanceSignal>,
    events: async_channel::Sender<Event>,
    _instance_lock: File,
    pub data_dir: PathBuf,
    pub initial_theme: ThemePreference,
    pub initial_language: LanguagePreference,
    pub initial_hotkey: HotkeyPreference,
    pub initial_paused: bool,
    pub initial_window_size: Option<WindowSizePreference>,
    pub initial_persist_window_size: bool,
    pub initial_auto_reset_state: bool,
    pub initial_search_auto_focus: bool,
    pub initial_search_auto_clear: bool,
    pub initial_skip_clear_confirm: bool,
    pub initial_paste_close_window: bool,
    pub initial_paste_key: PasteKeyPreference,
    pub initial_paste_move_to_top: bool,
    pub initial_quick_paste_enabled: bool,
    pub initial_paste_shortcuts: PasteShortcutConfig,
    pub initial_window_position: WindowPositionPreference,
    pub initial_hover_preview: HoverPreviewPreference,
    pub initial_toolbar: ToolbarPreference,
    pub initial_display: DisplayPreference,
    pub initial_audio: AudioPreference,
    pub initial_monitor_types: MonitorTypesPreference,
    pub initial_app_filter: AppFilterPreference,
    pub initial_onboarding_completed: bool,
}

fn resolve_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf> {
    match data_dir {
        Some(path) => Ok(path),
        None => Ok(ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")
            .context("无法确定用户数据目录")?
            .data_local_dir()
            .to_owned()),
    }
}

fn lock_instance(data_dir: &Path) -> Result<File> {
    std::fs::create_dir_all(data_dir).context("无法创建数据目录")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_dir.join("instance.lock"))?;
    file.try_lock().map_err(|_| InstanceBusy)?;
    Ok(file)
}

pub fn show_existing_instance(data_dir: Option<PathBuf>) -> Result<bool> {
    instance::show_existing(&resolve_data_dir(data_dir)?)
}

pub fn import_legacy_data(
    source: &Path,
    data_dir: Option<PathBuf>,
) -> Result<(PathBuf, ImportReport)> {
    let data_dir = resolve_data_dir(data_dir)?;
    let _instance_lock = lock_instance(&data_dir)?;
    let destination = data_dir.join("clipboard.db");
    let report = import_legacy_database(source, &destination)?;
    Ok((destination, report))
}

pub fn restore_backup_data(source: &Path, data_dir: PathBuf) -> Result<(PathBuf, RestoreReport)> {
    let _instance_lock = lock_instance(&data_dir)?;
    let data_dir = data_dir.canonicalize().context("无法解析恢复目录")?;
    let report = restore_backup(source, &data_dir)?;
    Ok((data_dir.join("clipboard.db"), report))
}

pub fn import_legacy_backup_data(
    source: &Path,
    data_dir: PathBuf,
) -> Result<(PathBuf, LegacyBackupReport)> {
    let _instance_lock = lock_instance(&data_dir)?;
    let data_dir = data_dir.canonicalize().context("无法解析导入目录")?;
    let report = import_legacy_backup(source, &data_dir)?;
    Ok((data_dir.join("clipboard.db"), report))
}

impl Service {
    pub fn start(
        data_dir: Option<PathBuf>,
        monitor: bool,
    ) -> Result<(Self, async_channel::Receiver<Event>)> {
        let data_dir = resolve_data_dir(data_dir)?;
        let instance_lock = lock_instance(&data_dir)?;
        // Startup happens before entering the GUI event loop; subsequent DB work is
        // exclusively owned by the worker thread.
        let db = Database::new(data_dir.join("clipboard.db"))?;
        let history = History::new(&db);
        let preferences = Preferences::new(&db);
        let initial_theme = preferences.theme()?;
        let initial_language = preferences.language()?;
        let initial_hotkey = preferences.hotkey()?;
        let initial_paused = preferences.capture_paused()?;
        let initial_window_size = preferences.window_size()?;
        let initial_persist_window_size = preferences.persist_window_size()?;
        let initial_auto_reset_state = preferences.auto_reset_state()?;
        let initial_search_auto_focus = preferences.search_auto_focus()?;
        let initial_search_auto_clear = preferences.search_auto_clear()?;
        let initial_skip_clear_confirm = preferences.skip_clear_confirm()?;
        let initial_paste_close_window = preferences.paste_close_window()?;
        let initial_paste_key = preferences.paste_key()?;
        let initial_paste_move_to_top = preferences.paste_move_to_top()?;
        let initial_quick_paste_enabled = preferences.quick_paste_enabled()?;
        let initial_paste_shortcuts = preferences.paste_shortcuts()?;
        let initial_window_position = preferences.window_position()?;
        let initial_hover_preview = preferences.hover_preview()?;
        let initial_toolbar = preferences.toolbar()?;
        let initial_display = preferences.display()?;
        let initial_audio = preferences.audio()?;
        let initial_monitor_types = preferences.monitor_types()?;
        let initial_app_filter = preferences.app_filter()?;
        let initial_onboarding_completed = if preferences.onboarding_completed()? {
            true
        } else if history.count("", false)? > 0
            || history.groups()?.iter().any(|group| group.item_count > 0)
        {
            // Existing GPUI data predates the introduction of onboarding.
            preferences.set_onboarding_completed()?;
            true
        } else {
            false
        };
        let writer =
            ClipboardContext::new().map_err(|error| anyhow!("初始化剪贴板失败：{error}"))?;
        let (commands, incoming) = mpsc::sync_channel(64);
        let (events, outgoing) = async_channel::bounded(64);
        let state = Arc::new(Mutex::new(CaptureState {
            paused: initial_paused,
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut service = Self {
            commands: commands.clone(),
            stop: stop.clone(),
            shutdown: None,
            watcher: None,
            worker: None,
            instance_signal: None,
            events: events.clone(),
            _instance_lock: instance_lock,
            initial_theme,
            initial_language,
            initial_hotkey,
            initial_paused,
            initial_window_size,
            initial_persist_window_size,
            initial_auto_reset_state,
            initial_search_auto_focus,
            initial_search_auto_clear,
            initial_skip_clear_confirm,
            initial_paste_close_window,
            initial_paste_key,
            initial_paste_move_to_top,
            initial_quick_paste_enabled,
            initial_paste_shortcuts,
            initial_window_position,
            initial_hover_preview,
            initial_toolbar,
            initial_display,
            initial_audio,
            initial_monitor_types,
            initial_app_filter: initial_app_filter.clone(),
            initial_onboarding_completed,
            data_dir,
        };
        let worker_state = state.clone();
        let worker_events = events.clone();
        let images_dir = service.data_dir.join("images");
        let staged_dir = service.data_dir.join("staged");
        let worker_data_dir = service.data_dir.clone();
        service.worker = Some(thread::Builder::new().name("history-worker".into()).spawn(
            move || {
                let mut worker = Worker {
                    history,
                    preferences,
                    clipboard: writer,
                    images_dir,
                    staged_dir,
                    data_dir: worker_data_dir,
                    state: worker_state,
                    monitor_types: initial_monitor_types,
                    app_filter: initial_app_filter,
                    search: String::new(),
                    favorite_only: false,
                    category: ContentCategory::All,
                    group_id: None,
                    limit: PAGE_SIZE,
                    generation: 0,
                    events: worker_events,
                };
                if let Err(error) = worker.send_groups().and_then(|_| worker.snapshot()) {
                    let _ = worker.events.try_send(Event::Error(error.to_string()));
                }
                while !stop.load(Ordering::Acquire) {
                    match incoming.recv_timeout(Duration::from_millis(250)) {
                        Ok(command) => {
                            let failure_kind = command.failure_kind();
                            if let Err(error) = worker.handle(command) {
                                let event = match failure_kind {
                                    None => Event::BackgroundError(error.to_string()),
                                    Some(kind) => Event::CommandFailed {
                                        kind,
                                        message: error.to_string(),
                                    },
                                };
                                let _ = worker.events.try_send(event);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            },
        )?);
        if monitor {
            let clipboard =
                ClipboardContext::new().map_err(|error| anyhow!("初始化监听失败：{error}"))?;
            let mut watcher = ClipboardWatcherContext::new()
                .map_err(|error| anyhow!("初始化监听失败：{error}"))?;
            watcher.add_handler(WatchHandler {
                clipboard,
                state,
                commands,
                events: events.clone(),
            });
            service.shutdown = Some(watcher.get_shutdown_channel());
            let stop = service.stop.clone();
            let watcher_events = events.clone();
            service.watcher = Some(
                thread::Builder::new()
                    .name("clipboard-watcher".into())
                    .spawn(move || {
                        watcher.start_watch();
                        if !stop.load(Ordering::Acquire) {
                            let _ = watcher_events.try_send(Event::BackgroundError(
                                "剪贴板监听已停止，请重启应用".into(),
                            ));
                        }
                    })?,
            );
        }
        match InstanceSignal::start(&service.data_dir, events.clone()) {
            Ok(signal) => service.instance_signal = Some(signal),
            Err(error) => {
                let _ = events.try_send(Event::BackgroundError(format!(
                    "重复启动唤出不可用：{error}"
                )));
            }
        }
        Ok((service, outgoing))
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| anyhow!("后台任务繁忙或已停止，请稍后重试"))
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.instance_signal = None;
        self.stop.store(true, Ordering::Release);
        // Release a worker waiting for a slow/closed UI before joining it.
        self.events.close();
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.stop();
        }
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn plain_text_for_copy(item: &ClipboardItem) -> Result<&str> {
    if !matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf") {
        bail!("该记录不支持纯文本复制");
    }
    item.text_content
        .as_deref()
        .filter(|text| !text.is_empty())
        .context("记录没有可复制的纯文本")
}

fn item_paths_for_action(
    history: &History,
    item: &ClipboardItem,
    staged_dir: &Path,
) -> Result<Vec<String>> {
    let paths = match item.content_type.as_str() {
        "files" => history.files_for_copy(item.id, staged_dir)?,
        "image" => vec![item.image_path.clone().context("图片文件路径缺失")?],
        _ => bail!("该记录不是文件或图片"),
    };
    if paths.iter().any(|path| !Path::new(path).exists()) {
        bail!("源文件、文件夹或图片已不存在");
    }
    Ok(paths)
}

fn reveal_in_explorer(path: &Path) -> Result<()> {
    explorer_command(path)?
        .spawn()
        .context("无法启动 Windows 资源管理器")?;
    Ok(())
}

fn explorer_command(path: &Path) -> Result<std::process::Command> {
    let mut command = std::process::Command::new(explorer_executable()?);
    command.arg("/select,").arg(path);
    Ok(command)
}

fn explorer_executable() -> Result<PathBuf> {
    let system_root = std::env::var_os("SystemRoot").context("无法确定 Windows 系统目录")?;
    let explorer = PathBuf::from(system_root).join("explorer.exe");
    if !explorer.is_file() {
        bail!("找不到 Windows 资源管理器");
    }
    Ok(explorer)
}

fn open_data_directory(data_dir: &Path) -> Result<()> {
    data_directory_command(data_dir)?
        .spawn()
        .context("无法打开数据目录")?;
    Ok(())
}

fn data_directory_command(data_dir: &Path) -> Result<std::process::Command> {
    if !data_dir.is_dir() {
        bail!("数据目录不存在");
    }
    let mut command = std::process::Command::new(explorer_executable()?);
    command.arg(data_dir);
    Ok(command)
}

fn file_size_if_present(path: &Path) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => bail!("{} 不是普通文件", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).with_context(|| format!("无法读取 {}", path.display())),
    }
}

fn directory_size_and_count(root: &Path) -> Result<(u64, u64)> {
    if !root.exists() {
        return Ok((0, 0));
    }
    if !root.is_dir() {
        bail!("{} 不是目录", root.display());
    }
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .with_context(|| format!("无法读取目录 {}", directory.display()))?
        {
            let entry = entry.with_context(|| format!("无法读取目录项 {}", directory.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("无法读取文件类型 {}", entry.path().display()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let length = entry
                    .metadata()
                    .with_context(|| format!("无法读取文件信息 {}", entry.path().display()))?
                    .len();
                bytes = bytes
                    .checked_add(length)
                    .context("数据目录大小超过可表示范围")?;
                count = count
                    .checked_add(1)
                    .context("数据目录文件数超过可表示范围")?;
            }
        }
    }
    Ok((bytes, count))
}

fn data_size_info(data_dir: &Path, images_dir: &Path, staged_dir: &Path) -> Result<DataSizeInfo> {
    let database_bytes = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"]
        .into_iter()
        .try_fold(0u64, |total, name| {
            total
                .checked_add(file_size_if_present(&data_dir.join(name))?)
                .context("数据库大小超过可表示范围")
        })?;
    let (image_bytes, image_count) = directory_size_and_count(images_dir)?;
    let (staged_bytes, staged_count) = directory_size_and_count(staged_dir)?;
    let total_bytes = database_bytes
        .checked_add(image_bytes)
        .and_then(|total| total.checked_add(staged_bytes))
        .context("数据占用超过可表示范围")?;
    Ok(DataSizeInfo {
        database_bytes,
        image_bytes,
        image_count,
        staged_bytes,
        staged_count,
        total_bytes,
    })
}

fn save_item_as(
    history: &History,
    item: &ClipboardItem,
    staged_dir: &Path,
    destination: &Path,
) -> Result<u64> {
    if !destination.is_absolute() {
        bail!("另存为目标必须是绝对路径");
    }
    let destination_name = destination.file_name().context("另存为目标缺少文件名")?;
    let destination_parent = destination.parent().context("另存为目标缺少父目录")?;
    if !destination_parent.is_dir() {
        bail!("另存为目标目录不存在");
    }
    if destination.is_dir() {
        bail!("另存为目标不能是文件夹");
    }

    let paths = item_paths_for_action(history, item, staged_dir)?;
    let source = Path::new(&paths[0]);
    if !source.is_file() {
        bail!("文件夹不支持另存为，请先在资源管理器中复制");
    }
    let canonical_source = source.canonicalize().context("无法解析另存为源文件")?;
    let canonical_destination = if destination.exists() {
        destination
            .canonicalize()
            .context("无法解析另存为目标文件")?
    } else {
        destination_parent
            .canonicalize()
            .context("无法解析另存为目标目录")?
            .join(destination_name)
    };
    if canonical_source == canonical_destination {
        bail!("不能将文件另存为其自身");
    }
    std::fs::copy(&canonical_source, destination).with_context(|| {
        format!(
            "无法将 {} 另存为 {}",
            canonical_source.display(),
            destination.display()
        )
    })
}

fn write_rich_clipboard(item: &ClipboardItem, ignored_sequence: &mut u32) -> Result<()> {
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let rtf = item
        .rtf_content
        .as_deref()
        .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
    if text.is_none() && html.is_none() && rtf.is_none() {
        bail!("富文本记录没有可写回的有效格式");
    }

    // clipboard-rs::set suppresses failures of individual formats. Keep one
    // clipboard open so every requested format is written or reported as failed.
    let mut touched = false;
    let result = (|| {
        let clipboard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
        clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
        touched = true;
        if let Some(text) = text {
            clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入纯文本格式失败：{error}"))?;
        }
        if let Some(html) = html {
            let format = clipboard_win::register_format("HTML Format")
                .context("注册 HTML 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_html_with(format, html, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入 HTML 格式失败：{error}"))?;
        }
        if let Some(rtf) = &rtf {
            let format = clipboard_win::register_format("Rich Text Format")
                .context("注册 RTF 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_without_clear(format, rtf)
                .map_err(|error| anyhow!("写入 RTF 格式失败：{error}"))?;
        }
        drop(clipboard);
        Ok(())
    })();
    if touched {
        // Capture the final sequence after the clipboard guard has closed, even
        // when a later format failed and left a partial write behind.
        *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    }
    result
}

fn rich_clipboard_matches(clipboard: &ClipboardContext, item: &ClipboardItem) -> bool {
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let rtf = item
        .rtf_content
        .as_deref()
        .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html_matches = html.is_none_or(|expected| {
        clipboard
            .get_html()
            .ok()
            .is_some_and(|actual| actual.contains(expected))
    });
    let rtf_matches = rtf.as_deref().is_none_or(|expected| {
        clipboard
            .get_buffer("Rich Text Format")
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    let text_matches = text.is_none_or(|expected| {
        clipboard
            .get_text()
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    html_matches && rtf_matches && text_matches
}

fn verified_rich_sequence(clipboard: &ClipboardContext, item: &ClipboardItem) -> Result<u32> {
    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        if !rich_clipboard_matches(clipboard, item) {
            bail!("富文本写回后格式校验失败，请重试");
        }
        let after = unsafe { GetClipboardSequenceNumber() };
        if before == after {
            return Ok(after);
        }
        thread::yield_now();
    }
    bail!("富文本写回后剪贴板持续变化，请重试")
}

fn write_merged_clipboard(
    clipboard: &ClipboardContext,
    merged: &clipboard_core::MergedContent,
    ignored_sequence: &mut u32,
) -> Result<u32> {
    let mut contents = Vec::with_capacity(2);
    if !merged.files.is_empty() {
        contents.push(ClipboardContent::Files(merged.files.clone()));
    }
    if let Some(text) = &merged.text {
        contents.push(ClipboardContent::Text(text.clone()));
    }
    let write_result = clipboard
        .set(contents)
        .map_err(|error| anyhow!("写入合并内容失败：{error}"));
    *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    write_result?;
    if *ignored_sequence == 0 {
        bail!("无法确认合并内容的剪贴板序列号，请重试");
    }

    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        let text_matches = merged.text.as_ref().is_none_or(|expected| {
            clipboard
                .get_text()
                .ok()
                .is_some_and(|actual| actual == expected.as_str())
        });
        let files_match = merged.files.is_empty()
            || clipboard
                .get_files()
                .ok()
                .is_some_and(|actual| actual.as_slice() == merged.files.as_slice());
        if !text_matches || !files_match {
            bail!("合并内容写回后格式校验失败，请重试");
        }
        let after = unsafe { GetClipboardSequenceNumber() };
        if before == after {
            *ignored_sequence = after;
            return Ok(after);
        }
        thread::yield_now();
    }
    bail!("合并内容写回后剪贴板持续变化，请重试")
}

struct Worker {
    history: History,
    preferences: Preferences,
    clipboard: ClipboardContext,
    images_dir: PathBuf,
    staged_dir: PathBuf,
    data_dir: PathBuf,
    state: Arc<Mutex<CaptureState>>,
    monitor_types: MonitorTypesPreference,
    app_filter: AppFilterPreference,
    search: String,
    favorite_only: bool,
    category: ContentCategory,
    group_id: Option<i64>,
    limit: i64,
    generation: u64,
    events: async_channel::Sender<Event>,
}

impl Worker {
    fn send_groups(&self) -> Result<()> {
        self.events
            .send_blocking(Event::Groups(self.history.groups()?))
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    fn snapshot(&self) -> Result<()> {
        self.events
            .send_blocking(Event::Snapshot {
                items: self
                    .history
                    .list_filtered_in_group(
                        &self.search,
                        self.limit,
                        self.favorite_only,
                        self.group_id,
                        self.category,
                    )
                    .map_err(|error| anyhow!("读取历史记录失败：{error}"))?,
                total: self.history.count_filtered_in_group(
                    &self.search,
                    self.favorite_only,
                    self.group_id,
                    self.category,
                )?,
                generation: self.generation,
            })
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    fn handle(&mut self, command: Command) -> Result<()> {
        let for_paste = matches!(
            &command,
            Command::CopyForPaste(_)
                | Command::CopyPlainTextForPaste(_)
                | Command::CopyPathForPaste(_)
        );
        let plain_only = matches!(
            &command,
            Command::CopyPlainText(_) | Command::CopyPlainTextForPaste(_)
        );
        let path_only = matches!(
            &command,
            Command::CopyPath(_) | Command::CopyPathForPaste(_)
        );
        let merge_for_paste = matches!(&command, Command::MergeForPaste(_));
        match command {
            Command::Query {
                search,
                favorite_only,
                category,
                group_id,
                limit,
                generation,
            } => {
                self.search = search;
                self.favorite_only = favorite_only;
                self.category = category;
                self.group_id = group_id;
                self.limit = limit;
                self.generation = generation;
            }
            Command::Capture(text) => {
                self.history.capture_with_media(&text, &self.images_dir)?;
            }
            Command::CaptureRich { html, rtf, text } => {
                self.history.capture_rich(
                    html.as_deref(),
                    rtf.as_deref(),
                    text.as_deref(),
                    &self.images_dir,
                )?;
            }
            Command::CaptureFiles(paths) => {
                self.history.capture_files(&paths, &self.images_dir)?;
            }
            Command::CaptureImage { png, width, height } => {
                let result = self
                    .history
                    .capture_image(&png, width, height, &self.images_dir);
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                state.pending_image_bytes = state.pending_image_bytes.saturating_sub(png.len());
                drop(state);
                result?;
            }
            Command::ObservedCapture { content, source } => {
                let image_bytes = match &content {
                    CapturedClipboard::Image { png, .. } => png.len(),
                    _ => 0,
                };
                let excluded = self.app_filter.excludes(
                    source.as_ref().map(|app| app.name.as_str()),
                    source.as_ref().and_then(|app| app.executable.as_deref()),
                );
                let filtered = (!excluded)
                    .then(|| filter_observed_capture(content, self.monitor_types))
                    .flatten();
                let Some(content) = filtered else {
                    if image_bytes != 0 {
                        let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                        state.pending_image_bytes =
                            state.pending_image_bytes.saturating_sub(image_bytes);
                    }
                    return Ok(());
                };
                let id = match content {
                    CapturedClipboard::Text(text) => {
                        self.history.capture_with_media(&text, &self.images_dir)?
                    }
                    CapturedClipboard::Rich { html, rtf, text } => {
                        Some(self.history.capture_rich(
                            html.as_deref(),
                            rtf.as_deref(),
                            text.as_deref(),
                            &self.images_dir,
                        )?)
                    }
                    CapturedClipboard::Files(paths) => {
                        Some(self.history.capture_files(&paths, &self.images_dir)?)
                    }
                    CapturedClipboard::Image { png, width, height } => {
                        let result =
                            self.history
                                .capture_image(&png, width, height, &self.images_dir);
                        let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                        state.pending_image_bytes =
                            state.pending_image_bytes.saturating_sub(png.len());
                        drop(state);
                        Some(result?)
                    }
                };
                if let (Some(id), Some(source)) = (id, source) {
                    let icon = source.executable.as_deref().and_then(|executable| {
                        source_app::extract_and_cache_icon(executable, &self.data_dir.join("icons"))
                    });
                    self.history
                        .set_source_app(id, &source.name, icon.as_deref())?;
                }
            }
            Command::MergeCopy(ids) | Command::MergeForPaste(ids) => {
                let item_count = ids.len();
                let merged = self.history.merge_content(&ids, &self.staged_dir)?;
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                let clipboard_sequence =
                    write_merged_clipboard(&self.clipboard, &merged, &mut state.ignored_sequence)?;
                drop(state);
                self.events
                    .send_blocking(Event::Merged {
                        for_paste: merge_for_paste,
                        clipboard_sequence,
                        item_count,
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::RevealInExplorer(id) => {
                let item = self.history.item(id)?;
                let paths = item_paths_for_action(&self.history, &item, &self.staged_dir)?;
                reveal_in_explorer(Path::new(&paths[0]))?;
                self.events
                    .send_blocking(Event::Status(if paths.len() == 1 {
                        "已在资源管理器中定位".into()
                    } else {
                        format!("已在资源管理器中定位第一项，共 {} 项", paths.len())
                    }))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SaveAs { id, destination } => {
                let result = self
                    .history
                    .item(id)
                    .and_then(|item| {
                        save_item_as(&self.history, &item, &self.staged_dir, &destination)
                    })
                    .map(|_| destination)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::SavedAs { id, result })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Copy(id)
            | Command::CopyPlainText(id)
            | Command::CopyPlainTextForPaste(id)
            | Command::CopyForPaste(id)
            | Command::CopyPath(id)
            | Command::CopyPathForPaste(id) => {
                let item = self.history.item(id)?;
                let plain_text = plain_only
                    .then(|| plain_text_for_copy(&item).map(str::to_owned))
                    .transpose()?;
                let path_text = path_only
                    .then(|| {
                        item_paths_for_action(&self.history, &item, &self.staged_dir)
                            .map(|paths| paths.join("\n"))
                    })
                    .transpose()?;
                let is_rich = !plain_only
                    && !path_only
                    && matches!(item.content_type.as_str(), "html" | "rtf");
                let files = if !path_only && item.content_type == "files" {
                    let paths = self.history.files_for_copy(id, &self.staged_dir)?;
                    if paths.iter().any(|path| !Path::new(path).exists()) {
                        bail!("源文件或文件夹已不存在，无法复制");
                    }
                    Some(paths)
                } else {
                    None
                };
                let image = if !path_only && item.content_type == "image" {
                    let path = item.image_path.as_deref().context("图片文件路径缺失")?;
                    if !Path::new(path).is_file() {
                        bail!("图片文件已丢失，无法复制");
                    }
                    let image = RustImageData::from_path(path)
                        .map_err(|error| anyhow!("打开图片失败：{error}"))?;
                    // clipboard-rs clears the clipboard before converting to PNG/BMP.
                    // Check both conversions before touching the user's current contents.
                    image
                        .to_png()
                        .map_err(|error| anyhow!("图片 PNG 编码失败：{error}"))?;
                    image
                        .to_bitmap()
                        .map_err(|error| anyhow!("图片位图编码失败：{error}"))?;
                    Some(image)
                } else {
                    None
                };
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                let mut rich_preserved = false;
                if let Some(path_text) = path_text {
                    self.clipboard
                        .set_text(path_text)
                        .map_err(|error| anyhow!("复制路径失败：{error}"))?;
                } else if let Some(files) = files {
                    self.clipboard
                        .set_files(files)
                        .map_err(|error| anyhow!("复制文件失败：{error}"))?;
                } else if let Some(image) = image {
                    self.clipboard
                        .set_image(image)
                        .map_err(|error| anyhow!("复制图片失败：{error}"))?;
                } else if is_rich {
                    // Record partial self-writes as ignored before an error is
                    // returned, so the listener cannot add them to history.
                    write_rich_clipboard(&item, &mut state.ignored_sequence)?;
                    let rtf = item
                        .rtf_content
                        .as_deref()
                        .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
                    state.ignored_sequence = verified_rich_sequence(&self.clipboard, &item)?;
                    rich_preserved = item
                        .html_content
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                        || rtf.is_some();
                } else {
                    let text = plain_text
                        .or(item.text_content)
                        .context("记录没有可复制的文本")?;
                    self.clipboard
                        .set_text(text)
                        .map_err(|error| anyhow!("复制失败：{error}"))?;
                }
                let clipboard_sequence = if is_rich {
                    state.ignored_sequence
                } else {
                    let sequence = unsafe { GetClipboardSequenceNumber() };
                    state.ignored_sequence = sequence;
                    sequence
                };
                if unsafe { GetClipboardSequenceNumber() } != clipboard_sequence {
                    bail!("复制过程中剪贴板已被其他应用修改，请重试");
                }
                drop(state);
                self.events
                    .send_blocking(Event::Copied {
                        id,
                        for_paste,
                        clipboard_sequence,
                        message: match item.content_type.as_str() {
                            _ if path_only => "路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                            _ if plain_only => "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                            "image" => "图片已复制，可切换到目标应用按 Ctrl+V 粘贴",
                            "files" => "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                            "html" | "rtf" if rich_preserved => {
                                "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴"
                            }
                            "html" | "rtf" => "富文本格式未写回，已按纯文本复制",
                            _ => "已复制，可切换到目标应用按 Ctrl+V 粘贴",
                        }
                        .into(),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Preview { id, generation } => {
                self.events
                    .send_blocking(Event::Preview {
                        id,
                        generation,
                        result: self
                            .history
                            .preview_content_with_staged(id, &self.staged_dir)
                            .map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::HoverPreview { id, generation } => {
                self.events
                    .send_blocking(Event::HoverPreview {
                        id,
                        generation,
                        result: self
                            .history
                            .preview_content_with_staged(id, &self.staged_dir)
                            .map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::EditText {
                id,
                expected_hash,
                new_text,
                generation,
            } => {
                let result = self
                    .history
                    .edit_text(id, &expected_hash, &new_text, &self.images_dir)
                    .map_err(|error| error.to_string());
                if matches!(&result, Ok(true)) {
                    self.snapshot()?;
                }
                self.events
                    .send_blocking(Event::TextEdited {
                        id,
                        generation,
                        result,
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Delete(id) => self.history.delete_with_media(id, &self.images_dir)?,
            Command::DeleteBatch {
                ids,
                group_id,
                generation,
            } => {
                let result = if group_id != self.group_id || generation != self.generation {
                    Err(anyhow!("列表已切换，请重新选择要删除的记录"))
                } else {
                    self.history
                        .delete_batch_with_media(&ids, group_id, &self.images_dir)
                };
                if result.is_ok() {
                    self.send_groups()?;
                    self.snapshot()?;
                }
                self.events
                    .send_blocking(Event::BatchDeleted(
                        result.map_err(|error| error.to_string()),
                    ))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ClearHistory {
                group_id,
                generation,
            } => {
                let result = if group_id != self.group_id || generation != self.generation {
                    Err(anyhow!("列表已切换，请重新选择要清理的分组"))
                } else {
                    self.history
                        .clear_history_with_media(group_id, &self.images_dir)
                };
                if result.is_ok() {
                    self.send_groups()?;
                    self.snapshot()?;
                }
                self.events
                    .send_blocking(Event::HistoryCleared(
                        result.map_err(|error| error.to_string()),
                    ))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ClearAllHistory => {
                let result = self
                    .history
                    .clear_all_with_media(&self.images_dir)
                    .and_then(|count| {
                        self.send_groups()
                            .context("全部历史已删除，但分组计数刷新失败，请重启应用后查看")?;
                        self.snapshot()
                            .context("全部历史已删除，但列表刷新失败，请重启应用后查看")?;
                        Ok(count)
                    });
                self.events
                    .send_blocking(Event::AllHistoryCleared(
                        result.map_err(|error| error.to_string()),
                    ))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ToggleFavorite(id) => {
                self.history.toggle_favorite(id)?;
            }
            Command::TogglePin(id) => {
                self.history.toggle_pin(id)?;
            }
            Command::BumpToTop(id) => {
                self.history.bump_to_top(id)?;
                self.snapshot()?;
                return Ok(());
            }
            Command::ResolveQuickPaste {
                slot,
                favorite,
                group_id,
            } => {
                let result = self
                    .history
                    .quick_paste_item_id(slot, favorite, group_id)
                    .map_err(|error| error.to_string())
                    .and_then(|item| item.ok_or_else(|| format!("槽位 {slot} 没有可用的历史记录")));
                self.events
                    .send_blocking(Event::QuickPasteResolved {
                        slot,
                        favorite,
                        result,
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Reorder {
                from,
                to,
                after,
                favorite_only,
                group_id,
                generation,
            } => {
                let result = if generation != self.generation
                    || favorite_only != self.favorite_only
                    || group_id != self.group_id
                {
                    Err(anyhow!("列表已切换，请重新拖动"))
                } else {
                    self.history
                        .reorder_in_group(from, to, after, favorite_only, group_id)
                };
                let result = result.and_then(|_| {
                    self.snapshot()
                        .context("顺序已保存，但列表刷新失败，请重启应用后查看")
                });
                self.events
                    .send_blocking(Event::Reordered {
                        from,
                        generation,
                        result: result.map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::CreateGroup(name) => {
                let result = self
                    .history
                    .create_group(&name)
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    self.send_groups()?;
                }
                self.events
                    .send_blocking(Event::GroupCreated(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ReorderGroup { from, to, after } => {
                let result = self.history.reorder_group(from, to, after).and_then(|_| {
                    self.send_groups()
                        .context("分组顺序已保存，但列表刷新失败，请重启应用后查看")
                });
                self.events
                    .send_blocking(Event::GroupReordered {
                        from,
                        result: result.map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::RenameGroup { id, name } => {
                let result = self
                    .history
                    .rename_group(id, &name)
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    self.send_groups()?;
                }
                self.events
                    .send_blocking(Event::GroupRenamed(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::DeleteGroup { id, generation } => {
                let result = if self.group_id != Some(id) || self.generation != generation {
                    Err(anyhow!("列表已切换，请重新选择分组"))
                } else {
                    self.history.delete_group_preserving_items(id)
                }
                .map_err(|error| error.to_string());
                if result.is_ok() {
                    self.group_id = None;
                    self.send_groups()?;
                }
                self.events
                    .send_blocking(Event::GroupDeleted(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::MoveToGroup {
                id,
                source_group_id,
                target_group_id,
                generation,
            } => {
                let result = if generation != self.generation || source_group_id != self.group_id {
                    Err(anyhow!("列表已切换，请重新选择记录"))
                } else {
                    self.history
                        .move_to_group(id, source_group_id, target_group_id)
                };
                if result.is_ok() {
                    self.send_groups()?;
                    self.snapshot()?;
                }
                self.events
                    .send_blocking(Event::ItemMoved {
                        id,
                        result: result.map_err(|error| error.to_string()),
                    })
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetTheme(theme) => {
                let result = self
                    .preferences
                    .set_theme(theme)
                    .map(|_| theme)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::ThemeSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetLanguage(language) => {
                let result = self
                    .preferences
                    .set_language(language)
                    .map(|_| language)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::LanguageSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetHotkey(hotkey) => {
                let result = self
                    .preferences
                    .paste_shortcuts()
                    .and_then(|shortcuts| {
                        crate::hotkey::validate_paste_shortcuts(&shortcuts, hotkey)
                    })
                    .and_then(|_| self.preferences.set_hotkey(hotkey))
                    .map(|_| hotkey)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::HotkeySaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetWindowSize(size) => {
                let result = self
                    .preferences
                    .set_window_size(size)
                    .map(|_| size)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::WindowSizeSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetPersistWindowSize(enabled) => {
                let result = self
                    .preferences
                    .set_persist_window_size(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::PersistWindowSizeSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetAutoResetState(enabled) => {
                let result = self
                    .preferences
                    .set_auto_reset_state(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::AutoResetStateSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetSearchAutoFocus(enabled) => {
                let result = self
                    .preferences
                    .set_search_auto_focus(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::SearchAutoFocusSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetSearchAutoClear(enabled) => {
                let result = self
                    .preferences
                    .set_search_auto_clear(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::SearchAutoClearSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetSkipClearConfirm(enabled) => {
                let result = self
                    .preferences
                    .set_skip_clear_confirm(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::SkipClearConfirmSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetPasteCloseWindow(enabled) => {
                let result = self
                    .preferences
                    .set_paste_close_window(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::PasteCloseWindowSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetPasteKey(key) => {
                let result = self
                    .preferences
                    .set_paste_key(key)
                    .map(|_| key)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::PasteKeySaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetPasteMoveToTop(enabled) => {
                let result = self
                    .preferences
                    .set_paste_move_to_top(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::PasteMoveToTopSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetQuickPasteEnabled(enabled) => {
                let result = self
                    .preferences
                    .set_quick_paste_enabled(enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::QuickPasteEnabledSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetPasteShortcuts(shortcuts) => {
                let result =
                    crate::hotkey::validate_paste_shortcuts(&shortcuts, self.preferences.hotkey()?)
                        .and_then(|_| self.preferences.set_paste_shortcuts(&shortcuts))
                        .map(|_| shortcuts)
                        .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::PasteShortcutsSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetWindowPosition(position) => {
                let result = self
                    .preferences
                    .set_window_position(position)
                    .map(|_| position)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::WindowPositionSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetHoverPreview(preference) => {
                let result = self
                    .preferences
                    .set_hover_preview(preference)
                    .map(|_| preference)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::HoverPreviewSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetToolbar(preference) => {
                let result = self
                    .preferences
                    .set_toolbar(preference)
                    .map(|_| preference)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::ToolbarSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetDisplay(preference) => {
                let result = self
                    .preferences
                    .set_display(preference)
                    .map(|_| preference)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::DisplaySaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetAudio(preference) => {
                let result = self
                    .preferences
                    .set_audio(preference)
                    .map(|_| preference)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::AudioSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetMonitorTypes(preference) => {
                let result = self
                    .preferences
                    .set_monitor_types(preference)
                    .map(|_| {
                        self.monitor_types = preference;
                        preference
                    })
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::MonitorTypesSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetAppFilter(preference) => {
                let result = self
                    .preferences
                    .set_app_filter(&preference)
                    .map(|_| {
                        self.app_filter = *preference.clone();
                        preference
                    })
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::AppFilterSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ListRunningApps => {
                let apps = source_app::running_apps(&self.data_dir.join("icons"));
                self.events
                    .send_blocking(Event::RunningApps(apps))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::CompleteOnboarding => {
                let result = self
                    .preferences
                    .set_onboarding_completed()
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::OnboardingCompleted(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::SetAutostart(enabled) => {
                let result = autostart::set_enabled(&self.data_dir, enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::AutostartSaved(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::QueryDataSize => {
                let result = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::DataSize(result))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::OptimizeDatabase => {
                self.history.optimize_storage()?;
                let size = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .context("数据库已整理，但刷新数据占用失败")?;
                self.events
                    .send_blocking(Event::DatabaseOptimized(size))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::OpenDataDirectory => {
                open_data_directory(&self.data_dir)?;
                self.events
                    .send_blocking(Event::Status("已打开数据目录".into()))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::ExportBackup(destination) => {
                self.events
                    .send_blocking(Event::BackupExported(
                        self.history
                            .export_backup(&destination)
                            .map_err(|error| error.to_string()),
                    ))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
            Command::Pause(paused) => {
                self.preferences
                    .set_capture_paused(paused)
                    .context("无法保存暂停状态")?;
                let mut state = self.state.lock().map_err(|_| anyhow!("剪贴板状态异常"))?;
                state.paused = paused;
                state.last_sequence = unsafe { GetClipboardSequenceNumber() };
                drop(state);
                self.events
                    .send_blocking(Event::Paused(paused))
                    .map_err(|_| anyhow!("窗口已关闭"))?;
                return Ok(());
            }
        }
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::database::GroupRepository;

    #[test]
    fn observed_capture_filter_distinguishes_url_and_preserves_allowed_rich_format() {
        let allowed = MonitorTypesPreference {
            text: false,
            url: true,
            html: false,
            rtf: true,
            image: false,
            files: false,
        };
        assert!(
            filter_observed_capture(CapturedClipboard::Text("plain text".into()), allowed)
                .is_none()
        );
        assert!(matches!(
            filter_observed_capture(
                CapturedClipboard::Text("https://example.com".into()),
                allowed
            ),
            Some(CapturedClipboard::Text(_))
        ));
        assert!(matches!(
            filter_observed_capture(
                CapturedClipboard::Rich {
                    html: Some("<b>rich</b>".into()),
                    rtf: Some(b"{\\rtf1 rich}".to_vec()),
                    text: Some("rich".into()),
                },
                allowed
            ),
            Some(CapturedClipboard::Rich {
                html: None,
                rtf: Some(_),
                ..
            })
        ));
        assert!(
            filter_observed_capture(
                CapturedClipboard::Files(vec!["C:\\test.txt".into()]),
                allowed
            )
            .is_none()
        );
        assert!(
            filter_observed_capture(
                CapturedClipboard::Image {
                    png: vec![1],
                    width: 1,
                    height: 1
                },
                allowed
            )
            .is_none()
        );
    }

    #[test]
    fn monitor_types_change_applies_to_later_observed_captures() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let allowed = MonitorTypesPreference {
            text: false,
            ..MonitorTypesPreference::default()
        };
        service.send(Command::SetMonitorTypes(allowed))?;
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("ordinary text".into()),
            source: None,
        })?;
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("https://example.com".into()),
            source: None,
        })?;
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        let items = next_snapshot(&events, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].preview.as_deref(), Some("https://example.com"));
        drop(service);
        let db = Database::new(directory.path().join("clipboard.db"))?;
        assert_eq!(Preferences::new(&db).monitor_types()?, allowed);
        Ok(())
    }

    #[test]
    fn app_filter_changes_apply_to_observed_sources() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let blacklist = AppFilterPreference {
            enabled: true,
            mode: clipboard_core::preferences::AppFilterMode::Blacklist,
            rules: vec!["notepad.exe".into()],
        };
        service.send(Command::SetAppFilter(Box::new(blacklist.clone())))?;
        let notepad = source_app::SourceApp {
            name: "Notepad".into(),
            executable: Some("C:\\Windows\\notepad.exe".into()),
        };
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("blocked note".into()),
            source: Some(notepad.clone()),
        })?;
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("unknown source".into()),
            source: None,
        })?;
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        let items = next_snapshot(&events, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].preview.as_deref(), Some("unknown source"));

        let whitelist = AppFilterPreference {
            mode: clipboard_core::preferences::AppFilterMode::Whitelist,
            ..blacklist
        };
        service.send(Command::SetAppFilter(Box::new(whitelist.clone())))?;
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("allowed note".into()),
            source: Some(notepad),
        })?;
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("blocked browser".into()),
            source: Some(source_app::SourceApp {
                name: "Browser".into(),
                executable: Some("C:\\Apps\\browser.exe".into()),
            }),
        })?;
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        let items = next_snapshot(&events, 2);
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|item| item.preview.as_deref() == Some("allowed note"))
        );
        assert!(
            !items
                .iter()
                .any(|item| item.preview.as_deref() == Some("blocked browser"))
        );
        drop(service);
        let db = Database::new(directory.path().join("clipboard.db"))?;
        assert_eq!(Preferences::new(&db).app_filter()?, whitelist);
        Ok(())
    }

    #[test]
    fn onboarding_completion_and_existing_history_are_recognized() -> Result<()> {
        let fresh = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(fresh.path().to_owned()), false)?;
        assert!(!service.initial_onboarding_completed);
        next_snapshot(&events, 0);
        service.send(Command::CompleteOnboarding)?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::OnboardingCompleted(Ok(()))
        ));
        drop(service);
        let (service, _) = Service::start(Some(fresh.path().to_owned()), false)?;
        assert!(service.initial_onboarding_completed);
        drop(service);

        let existing = tempfile::tempdir()?;
        let history = History::open(existing.path().join("clipboard.db"))?;
        let id = history.capture("existing history")?.expect("new item");
        let group = history.create_group("Existing group")?;
        history.move_to_group(id, None, Some(group.id))?;
        assert_eq!(history.count("", false)?, 0);
        drop(history);
        let (service, _) = Service::start(Some(existing.path().to_owned()), false)?;
        assert!(service.initial_onboarding_completed);
        drop(service);
        let db = Database::new(existing.path().join("clipboard.db"))?;
        assert!(Preferences::new(&db).onboarding_completed()?);
        Ok(())
    }

    #[test]
    fn plain_text_paste_failure_is_routed_to_its_pending_item() {
        assert_eq!(
            Command::CopyPlainTextForPaste(42).failure_kind(),
            Some(FailureKind::Paste(42))
        );
        assert_eq!(
            Command::CopyPlainText(42).failure_kind(),
            Some(FailureKind::Other)
        );
    }

    #[test]
    fn plain_text_copy_uses_only_stored_text_and_rejects_missing_representation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let rich = history.capture_rich(Some("<b>格式</b>"), None, Some("格式"), &images)?;
        assert_eq!(plain_text_for_copy(&history.item(rich)?)?, "格式");
        let no_text = history.capture_rich(Some("<i>only html</i>"), None, None, &images)?;
        assert!(plain_text_for_copy(&history.item(no_text)?).is_err());
        let file = history.capture_files(&["C:\\example.txt".into()], &images)?;
        assert!(plain_text_for_copy(&history.item(file)?).is_err());
        Ok(())
    }

    #[test]
    fn file_path_actions_use_resolved_existing_sources() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let source = directory.path().join("中文 file.txt");
        std::fs::write(&source, "content")?;
        let id = history.capture_files(
            &[source.to_string_lossy().into_owned()],
            &directory.path().join("images"),
        )?;
        assert_eq!(
            item_paths_for_action(
                &history,
                &history.item(id)?,
                &directory.path().join("staged")
            )?,
            vec![source.to_string_lossy().into_owned()]
        );

        let command = explorer_command(&source)?;
        assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("/select,"), source.as_os_str()]
        );

        std::fs::remove_file(&source)?;
        assert!(
            item_paths_for_action(
                &history,
                &history.item(id)?,
                &directory.path().join("staged")
            )
            .is_err()
        );
        let text = history.capture("not a file")?.unwrap();
        assert!(
            item_paths_for_action(
                &history,
                &history.item(text)?,
                &directory.path().join("staged")
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn save_as_copies_the_first_resolved_file_and_rejects_unsafe_targets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let staged = directory.path().join("staged");
        let source = directory.path().join("中文 source.txt");
        std::fs::write(&source, "saved content")?;
        let file_id = history.capture_files(&[source.to_string_lossy().into_owned()], &images)?;
        let destination = directory.path().join("另存 file.txt");
        assert_eq!(
            save_item_as(&history, &history.item(file_id)?, &staged, &destination)?,
            13
        );
        assert_eq!(std::fs::read_to_string(&destination)?, "saved content");
        assert!(save_item_as(&history, &history.item(file_id)?, &staged, &source).is_err());

        let folder = directory.path().join("folder");
        std::fs::create_dir(&folder)?;
        let folder_id = history.capture_files(&[folder.to_string_lossy().into_owned()], &images)?;
        assert!(
            save_item_as(
                &history,
                &history.item(folder_id)?,
                &staged,
                &directory.path().join("folder-copy")
            )
            .is_err()
        );

        let image_id = history.capture_image(b"\x89PNG\r\n\x1a\nsynthetic", 1, 1, &images)?;
        let image_destination = directory.path().join("image-copy.png");
        save_item_as(
            &history,
            &history.item(image_id)?,
            &staged,
            &image_destination,
        )?;
        assert_eq!(
            std::fs::read(&image_destination)?,
            b"\x89PNG\r\n\x1a\nsynthetic"
        );
        Ok(())
    }

    #[test]
    fn data_size_counts_database_and_managed_media() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let images = directory.path().join("images");
        let nested_images = images.join("nested");
        let staged = directory.path().join("staged");
        std::fs::create_dir_all(&nested_images)?;
        std::fs::create_dir_all(&staged)?;
        std::fs::write(directory.path().join("clipboard.db"), [0u8; 3])?;
        std::fs::write(directory.path().join("clipboard.db-wal"), [0u8; 5])?;
        std::fs::write(images.join("one.png"), [0u8; 7])?;
        std::fs::write(nested_images.join("two.png"), [0u8; 11])?;
        std::fs::write(staged.join("payload.bin"), [0u8; 13])?;
        std::fs::write(directory.path().join("unmanaged.txt"), [0u8; 17])?;

        assert_eq!(
            data_size_info(directory.path(), &images, &staged)?,
            DataSizeInfo {
                database_bytes: 8,
                image_bytes: 18,
                image_count: 2,
                staged_bytes: 13,
                staged_count: 1,
                total_bytes: 39,
            }
        );
        let command = data_directory_command(directory.path())?;
        assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
        assert_eq!(command.get_args().collect::<Vec<_>>(), [directory.path()]);
        Ok(())
    }

    #[test]
    fn database_maintenance_preserves_history_and_returns_updated_size() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("clipboard.db");
        let history = History::open(database.clone())?;
        let id = history.capture("keep after maintenance")?.unwrap();
        drop(history);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::OptimizeDatabase)?;
        match events.recv_blocking()? {
            Event::DatabaseOptimized(size) => {
                assert!(size.database_bytes > 0);
                assert_eq!(size.total_bytes, size.database_bytes);
            }
            Event::CommandFailed { message, .. } | Event::Error(message) => panic!("{message}"),
            _ => panic!("unexpected database maintenance event"),
        }
        drop(service);

        assert_eq!(History::open(database)?.text(id)?, "keep after maintenance");
        Ok(())
    }

    #[test]
    fn failed_paste_copy_identifies_only_its_own_request() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::CopyForPaste(424242))?;
        service.send(Command::CopyPathForPaste(424243))?;
        service.send(Command::SetTheme(ThemePreference::Dark))?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::CommandFailed {
                kind: FailureKind::Paste(424242),
                ..
            }
        ));
        assert!(matches!(
            events.recv_blocking()?,
            Event::CommandFailed {
                kind: FailureKind::Paste(424243),
                ..
            }
        ));
        assert!(matches!(
            events.recv_blocking()?,
            Event::ThemeSaved(Ok(ThemePreference::Dark))
        ));
        Ok(())
    }

    #[test]
    fn failed_merge_identifies_only_the_batch_request() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::MergeForPaste(vec![424242, 424243]))?;
        service.send(Command::SetTheme(ThemePreference::Dark))?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::CommandFailed {
                kind: FailureKind::Merge,
                ..
            }
        ));
        assert!(matches!(
            events.recv_blocking()?,
            Event::ThemeSaved(Ok(ThemePreference::Dark))
        ));
        Ok(())
    }

    fn next_snapshot(
        events: &async_channel::Receiver<Event>,
        generation: u64,
    ) -> Vec<ClipboardItem> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::Snapshot {
                    items,
                    generation: actual,
                    ..
                }) if actual == generation => return items,
                Ok(Event::Error(message)) => panic!("{message}"),
                Ok(Event::CommandFailed { message, .. } | Event::BackgroundError(message)) => {
                    panic!("{message}")
                }
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not respond"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn invalid_history_row_reports_startup_error() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let id = History::new(&db).capture("invalid row")?.unwrap();
        db.write_connection().lock().execute(
            &format!("UPDATE clipboard_items SET byte_size = X'01' WHERE id = {id}"),
            [],
        )?;
        drop(db);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::Error(message)) => {
                    assert!(message.contains("读取历史记录失败"), "{message}");
                    break;
                }
                Ok(Event::Snapshot { .. }) => bail!("损坏记录被错误地显示为正常快照"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "启动错误未上报");
            thread::sleep(Duration::from_millis(10));
        }
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 7,
        })?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::CommandFailed {
                    kind: FailureKind::Query(7),
                    message,
                }) => {
                    assert!(message.contains("读取历史记录失败"), "{message}");
                    break;
                }
                Ok(Event::Snapshot { .. }) => bail!("损坏记录被错误地显示为正常快照"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "查询错误未携带请求代次"
            );
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    #[test]
    fn imported_groups_can_be_browsed_and_reordered_without_mixing_default_history() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let default_item = history.capture("default item")?.unwrap();
        let first = history.capture("custom first")?.unwrap();
        let second = history.capture("custom second")?.unwrap();
        let groups = GroupRepository::new(&db);
        let group = groups.create("导入分组", None)?;
        groups.move_item_to_group(first, Some(group.id))?;
        groups.move_item_to_group(second, Some(group.id))?;
        drop(history);
        drop(db);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        let Event::Groups(available) = events.recv_blocking()? else {
            bail!("启动时没有发送分组列表");
        };
        assert_eq!(available[0].id, group.id);
        assert_eq!(next_snapshot(&events, 0)[0].id, default_item);
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: Some(group.id),
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        let rows = next_snapshot(&events, 1);
        assert_eq!(
            rows.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![second, first]
        );
        service.send(Command::Reorder {
            from: first,
            to: second,
            after: false,
            favorite_only: false,
            group_id: Some(group.id),
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1)[0].id, first);
        let Event::Reordered { result, .. } = events.recv_blocking()? else {
            bail!("没有排序确认");
        };
        result.map_err(anyhow::Error::msg)?;
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2)[0].id, default_item);
        Ok(())
    }

    #[test]
    fn create_group_acknowledges_only_persisted_names() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        next_snapshot(&events, 0);
        service.send(Command::CreateGroup("  工作  ".into()))?;
        let Event::Groups(groups) = events.recv_blocking()? else {
            bail!("创建后没有刷新分组列表");
        };
        assert_eq!(groups[0].name, "工作");
        let Event::GroupCreated(Ok(group)) = events.recv_blocking()? else {
            bail!("创建后没有成功确认");
        };
        assert_eq!(group.id, groups[0].id);
        service.send(Command::CreateGroup("工作".into()))?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupCreated(Err(_))
        ));
        Ok(())
    }

    #[test]
    fn group_reorder_acknowledges_saved_order_and_rejects_missing_target() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let first = history.create_group("工作")?.id;
        let second = history.create_group("归档")?.id;
        drop(history);
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        next_snapshot(&events, 0);
        service.send(Command::ReorderGroup {
            from: first,
            to: second,
            after: true,
        })?;
        let Event::Groups(groups) = events.recv_blocking()? else {
            bail!("排序后没有刷新分组列表");
        };
        assert_eq!(
            groups.iter().map(|group| group.id).collect::<Vec<_>>(),
            vec![second, first]
        );
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupReordered { result: Ok(()), .. }
        ));
        service.send(Command::ReorderGroup {
            from: first,
            to: -1,
            after: false,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupReordered { result: Err(_), .. }
        ));
        assert_eq!(
            History::open(directory.path().join("clipboard.db"))?
                .groups()?
                .iter()
                .map(|group| group.id)
                .collect::<Vec<_>>(),
            vec![second, first]
        );
        Ok(())
    }

    #[test]
    fn rename_group_acknowledges_saved_name_and_rejects_conflict() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let group = history.create_group("工作")?;
        history.create_group("归档")?;
        drop(history);
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        next_snapshot(&events, 0);
        service.send(Command::RenameGroup {
            id: group.id,
            name: "  常用  ".into(),
        })?;
        let Event::Groups(groups) = events.recv_blocking()? else {
            bail!("没有刷新分组列表");
        };
        assert_eq!(groups[0].name, "常用");
        assert!(
            matches!(events.recv_blocking()?, Event::GroupRenamed(Ok(ref renamed)) if renamed.id == group.id && renamed.name == "常用")
        );
        service.send(Command::RenameGroup {
            id: group.id,
            name: "归档".into(),
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupRenamed(Err(_))
        ));
        Ok(())
    }

    #[test]
    fn delete_group_preserves_items_and_rejects_stale_view() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let item = history.capture("keep this")?.unwrap();
        let group = history.create_group("工作")?;
        history.move_to_group(item, None, Some(group.id))?;
        drop(history);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        next_snapshot(&events, 0);
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: Some(group.id),
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1)[0].id, item);
        service.send(Command::DeleteGroup {
            id: group.id,
            generation: 0,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupDeleted(Err(_))
        ));
        service.send(Command::DeleteGroup {
            id: group.id,
            generation: 1,
        })?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(ref groups) if groups.is_empty()));
        assert!(matches!(
            events.recv_blocking()?,
            Event::GroupDeleted(Ok(1))
        ));
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2)[0].id, item);
        assert_eq!(History::open(path)?.item(item)?.group_id, None);
        Ok(())
    }

    #[test]
    fn text_edit_refreshes_snapshot_and_reports_stale_content() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let id = history.capture("before edit")?.unwrap();
        let hash = history.item(id)?.content_hash;
        drop(history);
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        next_snapshot(&events, 0);
        service.send(Command::EditText {
            id,
            expected_hash: hash.clone(),
            new_text: "after edit".into(),
            generation: 7,
        })?;
        assert_eq!(
            next_snapshot(&events, 0)[0].preview.as_deref(),
            Some("after edit")
        );
        assert!(matches!(
            events.recv_blocking()?,
            Event::TextEdited { id: edited, generation: 7, result: Ok(true) } if edited == id
        ));
        service.send(Command::EditText {
            id,
            expected_hash: hash,
            new_text: "stale overwrite".into(),
            generation: 8,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::TextEdited { result: Err(_), .. }
        ));
        assert_eq!(History::open(path)?.text(id)?, "after edit");
        Ok(())
    }

    #[test]
    fn move_to_group_updates_visible_history_and_rejects_stale_view() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let id = history.capture("move me")?.unwrap();
        let group = history.create_group("工作")?;
        drop(history);
        drop(db);
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        assert_eq!(next_snapshot(&events, 0)[0].id, id);
        service.send(Command::MoveToGroup {
            id,
            source_group_id: None,
            target_group_id: Some(group.id),
            generation: 0,
        })?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        assert!(next_snapshot(&events, 0).is_empty());
        assert!(
            matches!(events.recv_blocking()?, Event::ItemMoved { id: moved, result: Ok(()) } if moved == id)
        );
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: Some(group.id),
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1)[0].id, id);
        service.send(Command::MoveToGroup {
            id,
            source_group_id: Some(group.id),
            target_group_id: None,
            generation: 0,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::ItemMoved { result: Err(_), .. }
        ));
        assert_eq!(
            History::open(directory.path().join("clipboard.db"))?
                .item(id)?
                .group_id,
            Some(group.id)
        );
        Ok(())
    }

    #[test]
    fn worker_serializes_capture_search_and_delete_without_touching_clipboard() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("中文%_ test".into()))?;
        service.send(Command::Capture("another record".into()))?;
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        let rows = next_snapshot(&events, 1);
        assert_eq!(rows.len(), 1);
        service.send(Command::Delete(rows[0].id))?;
        service.send(Command::Query {
            search: "".into(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2).len(), 1);
        drop(service);
        let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(next_snapshot(&events, 0).len(), 1);
        drop(reopened);
        Ok(())
    }

    #[test]
    fn capture_error_does_not_replace_an_unrelated_settings_acknowledgement() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);

        service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
        service.send(Command::SetTheme(ThemePreference::Dark))?;
        assert!(matches!(events.recv_blocking()?, Event::BackgroundError(_)));
        assert!(matches!(
            events.recv_blocking()?,
            Event::ThemeSaved(Ok(ThemePreference::Dark))
        ));
        Ok(())
    }

    #[test]
    fn reorder_acknowledges_success_and_rejects_stale_view() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("first".into()))?;
        let first = next_snapshot(&events, 0)[0].id;
        service.send(Command::Capture("second".into()))?;
        let second = next_snapshot(&events, 0)[0].id;
        for generation in [0, 99] {
            service.send(Command::Reorder {
                from: first,
                to: second,
                after: false,
                favorite_only: false,
                group_id: None,
                generation,
            })?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Reordered {
                        from,
                        generation: actual,
                        result,
                    }) => {
                        assert_eq!((from, actual), (first, generation));
                        assert_eq!(result.is_ok(), generation == 0);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
        }
        service.send(Command::TogglePin(second))?;
        let rows = next_snapshot(&events, 0);
        assert!(rows[0].is_pinned);
        assert_eq!(rows[0].id, second);
        service.send(Command::Reorder {
            from: first,
            to: second,
            after: false,
            favorite_only: false,
            group_id: None,
            generation: 0,
        })?;
        let rows = next_snapshot(&events, 0);
        assert_eq!(
            rows.iter()
                .map(|item| (item.id, item.is_pinned))
                .collect::<Vec<_>>(),
            vec![(first, true), (second, true)]
        );
        assert!(matches!(
            events.recv_blocking()?,
            Event::Reordered { result: Ok(()), .. }
        ));
        drop(service);
        let (_service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        let rows = next_snapshot(&events, 0);
        assert_eq!(rows[0].id, first);
        assert!(rows[0].is_pinned);
        Ok(())
    }

    #[test]
    fn theme_acknowledgement_is_persisted_before_reopening() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_theme, ThemePreference::System);
        next_snapshot(&events, 0);
        for theme in [
            ThemePreference::Light,
            ThemePreference::System,
            ThemePreference::Dark,
        ] {
            service.send(Command::SetTheme(theme))?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::ThemeSaved(result)) => {
                        assert_eq!(result.unwrap(), theme);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline, "theme timed out");
                thread::sleep(Duration::from_millis(10));
            }
        }
        drop(service);
        let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_theme, ThemePreference::Dark);
        assert!(next_snapshot(&events, 0).is_empty());
        Ok(())
    }

    #[test]
    fn language_defaults_to_chinese_and_is_loaded_after_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_language, LanguagePreference::Chinese);
        next_snapshot(&events, 0);
        service.send(Command::SetLanguage(LanguagePreference::English))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::LanguageSaved(result)) => {
                    assert_eq!(result.unwrap(), LanguagePreference::English);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "language save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_language, LanguagePreference::English);
        Ok(())
    }

    #[test]
    fn hotkey_change_is_acknowledged_and_loaded_on_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_hotkey, HotkeyPreference::CtrlShiftV);
        next_snapshot(&events, 0);
        service.send(Command::SetHotkey(HotkeyPreference::AltC))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::HotkeySaved(result)) => {
                    assert_eq!(result.unwrap(), HotkeyPreference::AltC);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "hotkey save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_hotkey, HotkeyPreference::AltC);
        Ok(())
    }

    #[test]
    fn window_size_is_acknowledged_and_loaded_on_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_window_size, None);
        next_snapshot(&events, 0);
        let size = WindowSizePreference::new(960, 720).unwrap();
        service.send(Command::SetWindowSize(size))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::WindowSizeSaved(result)) => {
                    assert_eq!(result.unwrap(), size);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "window size save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_window_size, Some(size));
        Ok(())
    }

    #[test]
    fn window_position_is_acknowledged_and_loaded_on_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(
            service.initial_window_position,
            WindowPositionPreference::FollowCursor
        );
        next_snapshot(&events, 0);
        service.send(Command::SetWindowPosition(
            WindowPositionPreference::ScreenCenter,
        ))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::WindowPositionSaved(result)) => {
                    assert_eq!(result.unwrap(), WindowPositionPreference::ScreenCenter);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "window position save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(
            reopened.initial_window_position,
            WindowPositionPreference::ScreenCenter
        );
        Ok(())
    }

    #[test]
    fn window_behavior_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(service.initial_persist_window_size);
        assert!(!service.initial_auto_reset_state);
        assert!(!service.initial_search_auto_focus);
        assert!(service.initial_search_auto_clear);
        assert!(!service.initial_skip_clear_confirm);
        assert!(service.initial_paste_close_window);
        assert_eq!(service.initial_paste_key, PasteKeyPreference::CtrlV);
        assert!(service.initial_paste_move_to_top);
        assert!(service.initial_quick_paste_enabled);
        next_snapshot(&events, 0);
        service.send(Command::SetPersistWindowSize(false))?;
        service.send(Command::SetAutoResetState(true))?;
        service.send(Command::SetSearchAutoFocus(true))?;
        service.send(Command::SetSearchAutoClear(false))?;
        service.send(Command::SetSkipClearConfirm(true))?;
        service.send(Command::SetPasteCloseWindow(false))?;
        service.send(Command::SetPasteKey(PasteKeyPreference::ShiftInsert))?;
        service.send(Command::SetPasteMoveToTop(false))?;
        service.send(Command::SetQuickPasteEnabled(false))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut size_acknowledged = false;
        let mut reset_acknowledged = false;
        let mut focus_acknowledged = false;
        let mut clear_acknowledged = false;
        let mut skip_clear_acknowledged = false;
        let mut paste_acknowledged = false;
        let mut paste_key_acknowledged = false;
        let mut move_acknowledged = false;
        let mut quick_acknowledged = false;
        while !size_acknowledged
            || !reset_acknowledged
            || !focus_acknowledged
            || !clear_acknowledged
            || !skip_clear_acknowledged
            || !paste_acknowledged
            || !paste_key_acknowledged
            || !move_acknowledged
            || !quick_acknowledged
        {
            match events.try_recv() {
                Ok(Event::PersistWindowSizeSaved(result)) => {
                    assert!(!result.unwrap());
                    size_acknowledged = true;
                }
                Ok(Event::AutoResetStateSaved(result)) => {
                    assert!(result.unwrap());
                    reset_acknowledged = true;
                }
                Ok(Event::SearchAutoFocusSaved(result)) => {
                    assert!(result.unwrap());
                    focus_acknowledged = true;
                }
                Ok(Event::SearchAutoClearSaved(result)) => {
                    assert!(!result.unwrap());
                    clear_acknowledged = true;
                }
                Ok(Event::SkipClearConfirmSaved(result)) => {
                    assert!(result.unwrap());
                    skip_clear_acknowledged = true;
                }
                Ok(Event::PasteCloseWindowSaved(result)) => {
                    assert!(!result.unwrap());
                    paste_acknowledged = true;
                }
                Ok(Event::PasteKeySaved(result)) => {
                    assert_eq!(result.unwrap(), PasteKeyPreference::ShiftInsert);
                    paste_key_acknowledged = true;
                }
                Ok(Event::PasteMoveToTopSaved(result)) => {
                    assert!(!result.unwrap());
                    move_acknowledged = true;
                }
                Ok(Event::QuickPasteEnabledSaved(result)) => {
                    assert!(!result.unwrap());
                    quick_acknowledged = true;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "window behavior save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(!reopened.initial_persist_window_size);
        assert!(reopened.initial_auto_reset_state);
        assert!(reopened.initial_search_auto_focus);
        assert!(!reopened.initial_search_auto_clear);
        assert!(reopened.initial_skip_clear_confirm);
        assert!(!reopened.initial_paste_close_window);
        assert_eq!(reopened.initial_paste_key, PasteKeyPreference::ShiftInsert);
        assert!(!reopened.initial_paste_move_to_top);
        assert!(!reopened.initial_quick_paste_enabled);
        Ok(())
    }

    #[test]
    fn paste_shortcuts_save_atomically_and_reject_window_hotkey_conflicts() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let mut shortcuts = service.initial_paste_shortcuts.clone();
        shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
        service.send(Command::SetPasteShortcuts(Box::new(shortcuts.clone())))?;
        let result = loop {
            match events.recv_blocking()? {
                Event::PasteShortcutsSaved(result) => break result,
                Event::Error(message) => anyhow::bail!("{message}"),
                _ => {}
            }
        };
        assert_eq!(*result.unwrap(), shortcuts);
        let mut invalid = shortcuts.clone();
        invalid.set_slot(true, 10, "Ctrl+Shift+V".into())?;
        service.send(Command::SetPasteShortcuts(Box::new(invalid)))?;
        let result = loop {
            match events.recv_blocking()? {
                Event::PasteShortcutsSaved(result) => break result,
                Event::Error(message) => anyhow::bail!("{message}"),
                _ => {}
            }
        };
        assert!(result.is_err());
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_paste_shortcuts, shortcuts);
        Ok(())
    }

    #[test]
    fn bump_to_top_refreshes_the_visible_history_without_clipboard_access() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let older = history.capture("older item")?.unwrap();
        let newer = history.capture("newer item")?.unwrap();
        drop(history);
        drop(db);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(next_snapshot(&events, 0)[0].id, newer);
        service.send(Command::BumpToTop(older))?;
        assert_eq!(next_snapshot(&events, 0)[0].id, older);
        Ok(())
    }

    #[test]
    fn quick_paste_resolves_recent_and_favorite_slots_with_group_isolation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let favorite = history.capture("favorite")?.unwrap();
        history.toggle_favorite(favorite)?;
        let recent = history.capture("recent")?.unwrap();
        let grouped = history.capture("grouped")?.unwrap();
        let groups = GroupRepository::new(&db);
        let group = groups.create("Group", None)?;
        groups.move_item_to_group(grouped, Some(group.id))?;
        drop(history);
        drop(db);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        for (slot, is_favorite, group_id, expected) in [
            (1, false, None, Some(recent)),
            (1, true, None, Some(favorite)),
            (1, false, Some(group.id), Some(grouped)),
            (2, false, Some(group.id), None),
            (0, false, None, None),
        ] {
            service.send(Command::ResolveQuickPaste {
                slot,
                favorite: is_favorite,
                group_id,
            })?;
            let Event::QuickPasteResolved {
                slot: actual_slot,
                favorite: actual_favorite,
                result,
            } = events.recv_blocking()?
            else {
                bail!("没有返回快速粘贴槽位结果");
            };
            assert_eq!((actual_slot, actual_favorite), (slot, is_favorite));
            assert_eq!(result.ok(), expected);
        }
        Ok(())
    }

    #[test]
    fn toolbar_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
        use clipboard_core::preferences::ToolbarButton;

        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_toolbar, ToolbarPreference::default());
        next_snapshot(&events, 0);
        let toolbar = ToolbarPreference::default()
            .move_button(ToolbarButton::Settings, -1)
            .unwrap()
            .with_visibility(ToolbarButton::Clear, false)
            .unwrap();
        service.send(Command::SetToolbar(toolbar))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::ToolbarSaved(result)) => {
                    assert_eq!(result.unwrap(), toolbar);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "toolbar save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_toolbar, toolbar);
        Ok(())
    }

    #[test]
    fn display_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
        use clipboard_core::preferences::CardDensity;

        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_display, DisplayPreference::default());
        next_snapshot(&events, 0);
        let display = DisplayPreference {
            show_category_filter: false,
            show_drag_area_indicator: false,
            card_density: CardDensity::Spacious,
            card_max_lines: 5,
            show_time: false,
            time_format: clipboard_core::preferences::TimeFormat::Relative,
            show_char_count: false,
            show_byte_size: false,
            show_source_app: false,
            source_app_display: clipboard_core::preferences::SourceAppDisplay::Icon,
        };
        service.send(Command::SetDisplay(display))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::DisplaySaved(result)) => {
                    assert_eq!(result.unwrap(), display);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "display save timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_display, display);
        Ok(())
    }

    #[test]
    fn audio_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
        use clipboard_core::preferences::SoundTiming;

        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_audio, AudioPreference::default());
        next_snapshot(&events, 0);
        let audio = AudioPreference {
            copy_enabled: true,
            copy_timing: SoundTiming::AfterSuccess,
            paste_enabled: true,
            paste_timing: SoundTiming::AfterSuccess,
        };
        service.send(Command::SetAudio(audio))?;
        assert!(matches!(events.recv_blocking()?, Event::AudioSaved(Ok(saved)) if saved == audio));
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_audio, audio);
        Ok(())
    }

    #[test]
    fn observed_captures_keep_the_source_with_each_record() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);

        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("source test".into()),
            source: Some(source_app::SourceApp {
                name: "Editor".into(),
                executable: None,
            }),
        })?;
        let items = next_snapshot(&events, 0);
        assert_eq!(items[0].source_app_name.as_deref(), Some("Editor"));

        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("source test".into()),
            source: Some(source_app::SourceApp {
                name: "Browser".into(),
                executable: None,
            }),
        })?;
        let items = next_snapshot(&events, 0);
        assert_eq!(items[0].source_app_name.as_deref(), Some("Browser"));

        for (content, source) in [
            (
                CapturedClipboard::Rich {
                    html: Some("<b>rich source</b>".into()),
                    rtf: None,
                    text: Some("rich source".into()),
                },
                "Mail",
            ),
            (
                CapturedClipboard::Files(vec![r"C:\source-test.txt".into()]),
                "Explorer",
            ),
            (
                CapturedClipboard::Image {
                    png: include_bytes!("../tests/fixtures/test.png").to_vec(),
                    width: 128,
                    height: 128,
                },
                "Paint",
            ),
        ] {
            service.send(Command::ObservedCapture {
                content,
                source: Some(source_app::SourceApp {
                    name: source.into(),
                    executable: None,
                }),
            })?;
            let items = next_snapshot(&events, 0);
            assert_eq!(items[0].source_app_name.as_deref(), Some(source));
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires a Windows system executable with an icon"]
    fn observed_capture_persists_a_cached_source_icon() -> Result<()> {
        let system_root = std::env::var("SystemRoot")?;
        let executable = Path::new(&system_root).join("System32/notepad.exe");
        anyhow::ensure!(executable.is_file(), "system executable is unavailable");
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::ObservedCapture {
            content: CapturedClipboard::Text("icon source test".into()),
            source: Some(source_app::SourceApp {
                name: "Notepad".into(),
                executable: Some(executable.to_string_lossy().into_owned()),
            }),
        })?;
        let items = next_snapshot(&events, 0);
        assert_eq!(items[0].source_app_name.as_deref(), Some("Notepad"));
        let icon = items[0]
            .source_app_icon
            .as_deref()
            .context("missing icon path")?;
        assert!(Path::new(icon).starts_with(directory.path().join("icons")));
        assert!(Path::new(icon).is_file());
        Ok(())
    }

    #[test]
    fn hover_preview_preferences_are_acknowledged_and_loaded_on_restart() -> Result<()> {
        use clipboard_core::preferences::HoverPreviewPosition;

        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(
            service.initial_hover_preview,
            HoverPreviewPreference::default()
        );
        next_snapshot(&events, 0);
        let preference = HoverPreviewPreference {
            text: true,
            expanded_image: true,
            delay_ms: 750,
            position: HoverPreviewPosition::Left,
            zoom_step: 20,
            ..HoverPreviewPreference::default()
        };
        service.send(Command::SetHoverPreview(preference))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::HoverPreviewSaved(result)) => {
                    assert_eq!(result.unwrap(), preference);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "hover settings timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_hover_preview, preference);
        Ok(())
    }

    #[test]
    fn worker_keeps_favorite_filter_during_capture_and_toggle() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("favorite %_".into()))?;
        let id = next_snapshot(&events, 0)[0].id;
        service.send(Command::ToggleFavorite(id))?;
        assert!(next_snapshot(&events, 0)[0].is_favorite);
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: true,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1).len(), 1);
        service.send(Command::Capture("ordinary %_".into()))?;
        assert_eq!(next_snapshot(&events, 1)[0].id, id);
        service.send(Command::ToggleFavorite(id))?;
        assert!(next_snapshot(&events, 1).is_empty());
        service.send(Command::Query {
            search: "%_".into(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2).len(), 2);
        Ok(())
    }

    #[test]
    fn worker_keeps_category_filter_when_history_changes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("first text".into()))?;
        next_snapshot(&events, 0);
        service.send(Command::CaptureImage {
            png: include_bytes!("../tests/fixtures/test.png").to_vec(),
            width: 128,
            height: 128,
        })?;
        assert_eq!(next_snapshot(&events, 0).len(), 2);
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::Text,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1).len(), 1);
        service.send(Command::Capture("second text".into()))?;
        let rows = next_snapshot(&events, 1);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|item| item.content_type == "text"));
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::Other,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        let rows = next_snapshot(&events, 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content_type, "image");
        Ok(())
    }

    #[test]
    fn image_capture_is_persisted_and_delete_removes_managed_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let png = include_bytes!("../tests/fixtures/test.png");
        service.send(Command::CaptureImage {
            png: png.to_vec(),
            width: 128,
            height: 128,
        })?;
        let rows = next_snapshot(&events, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content_type, "image");
        let image_path = PathBuf::from(rows[0].image_path.as_ref().unwrap());
        assert_eq!(std::fs::read(&image_path)?, png);
        let image = RustImageData::from_path(image_path.to_str().unwrap())
            .map_err(|error| anyhow!("打开测试图片失败：{error}"))?;
        assert_eq!(image.get_size(), (128, 128));
        service.send(Command::Delete(rows[0].id))?;
        assert!(next_snapshot(&events, 0).is_empty());
        assert!(!image_path.exists());
        Ok(())
    }

    #[test]
    fn file_paths_are_saved_and_missing_sources_do_not_touch_clipboard() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("sample.txt");
        std::fs::write(&path, "sample")?;
        let path = path.to_string_lossy().into_owned();
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::CaptureFiles(vec![path.clone()]))?;
        let rows = next_snapshot(&events, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content_type, "files");
        let id = rows[0].id;
        service.send(Command::Preview { id, generation: 1 })?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::Preview { result, .. }) => {
                    assert_eq!(
                        result.unwrap(),
                        PreviewContent::Files(vec![FilePreviewEntry {
                            original_path: path.clone(),
                            resolved_path: path.clone(),
                            exists: true,
                            is_dir: false,
                            size: Some(6),
                            recovered: false,
                            metadata_error: None,
                        }])
                    );
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "preview timed out");
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_file(path)?;
        service.send(Command::Copy(id))?;
        service.send(Command::SetTheme(ThemePreference::Dark))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::CommandFailed {
                    kind: FailureKind::Other,
                    message,
                }) => {
                    assert!(message.contains("已不存在"));
                    break;
                }
                Ok(Event::Error(message) | Event::BackgroundError(message)) => bail!("{message}"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "copy error timed out");
            thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(
            events.recv_blocking()?,
            Event::ThemeSaved(Ok(ThemePreference::Dark))
        ));
        Ok(())
    }

    #[test]
    fn rich_capture_keeps_html_and_binary_rtf_for_preview() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let rtf = b"{\\rtf1\\bin2 \x00\x01}".to_vec();
        service.send(Command::CaptureRich {
            html: Some("<b>中文😀</b>".into()),
            rtf: Some(rtf.clone()),
            text: Some("中文😀".into()),
        })?;
        let rows = next_snapshot(&events, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content_type, "html");
        let stored = History::open(directory.path().join("clipboard.db"))?.item(rows[0].id)?;
        assert_eq!(stored.html_content.as_deref(), Some("<b>中文😀</b>"));
        assert_eq!(
            stored
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
                .unwrap(),
            [rtf, vec![0]].concat()
        );
        service.send(Command::Preview {
            id: rows[0].id,
            generation: 1,
        })?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::Preview { result, .. }) => {
                    assert_eq!(result.unwrap(), PreviewContent::RichText("中文😀".into()));
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "preview timed out");
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    #[test]
    fn instance_lock_and_shutdown_work_even_with_a_full_event_queue() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, _events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(Service::start(Some(directory.path().to_owned()), false).is_err());
        for generation in 1..=160 {
            // Deliberately create UI backpressure. A full command queue is expected.
            let _ = service.send(Command::Query {
                search: "".into(),
                favorite_only: false,
                category: ContentCategory::All,
                group_id: None,
                limit: PAGE_SIZE,
                generation,
            });
            thread::sleep(Duration::from_millis(1));
        }
        drop(service);
        let (service, _) = Service::start(Some(directory.path().to_owned()), false)?;
        drop(service);
        Ok(())
    }

    #[test]
    fn second_launch_signal_is_scoped_to_data_directory() -> Result<()> {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        assert!(!show_existing_instance(Some(first.path().to_owned()))?);
        let (service, events) = Service::start(Some(first.path().to_owned()), false)?;
        assert!(show_existing_instance(Some(first.path().to_owned()))?);
        assert!(!show_existing_instance(Some(second.path().to_owned()))?);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut received = false;
        while std::time::Instant::now() < deadline {
            if let Ok(Event::ShowWindow) = events.try_recv() {
                received = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(received);
        drop(service);
        assert!(!show_existing_instance(Some(first.path().to_owned()))?);
        Ok(())
    }

    #[test]
    fn preview_returns_full_text_and_reports_deleted_records() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let text = format!("中文😀\r\n{}\n末尾", "long text ".repeat(600));
        service.send(Command::Capture(text.clone()))?;
        let rows = next_snapshot(&events, 0);
        let id = rows[0].id;
        assert!(rows[0].text_content.is_none());
        for generation in 1..=2 {
            if generation == 2 {
                service.send(Command::Delete(id))?;
                assert!(next_snapshot(&events, 0).is_empty());
            }
            service.send(Command::Preview { id, generation })?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Preview {
                        id: actual,
                        generation: request,
                        result,
                    }) => {
                        assert_eq!((actual, request), (id, generation));
                        if generation == 1 {
                            assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                        } else {
                            assert!(result.is_err());
                        }
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(std::time::Instant::now() < deadline, "preview timed out");
                thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(())
    }

    #[test]
    fn hover_preview_has_its_own_response_and_reads_full_text() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let text = format!("悬停预览\n{}", "正文".repeat(500));
        service.send(Command::Capture(text.clone()))?;
        let id = next_snapshot(&events, 0)[0].id;
        service.send(Command::HoverPreview { id, generation: 7 })?;
        service.send(Command::Preview { id, generation: 3 })?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut hover_seen = false;
        let mut preview_seen = false;
        while !hover_seen || !preview_seen {
            match events.try_recv() {
                Ok(Event::HoverPreview {
                    id: actual,
                    generation,
                    result,
                }) => {
                    assert_eq!((actual, generation), (id, 7));
                    assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                    hover_seen = true;
                }
                Ok(Event::Preview {
                    id: actual,
                    generation,
                    result,
                }) => {
                    assert_eq!((actual, generation), (id, 3));
                    assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                    preview_seen = true;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "preview timed out");
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    #[test]
    fn pause_acknowledgement_is_ordered_and_does_not_change_history() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(!service.initial_paused);
        next_snapshot(&events, 0);
        for paused in [true, false, true] {
            service.send(Command::Pause(paused))?;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match events.try_recv() {
                    Ok(Event::Paused(actual)) => {
                        assert_eq!(actual, paused);
                        break;
                    }
                    Ok(Event::Error(message)) => panic!("{message}"),
                    _ => {}
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "pause was not acknowledged"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: None,
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert!(next_snapshot(&events, 1).is_empty());
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert!(reopened.initial_paused);
        Ok(())
    }

    #[test]
    fn worker_exports_backup_and_cli_restores_into_isolated_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        let restored = directory.path().join("restored");
        let backup = directory.path().join("history.zip");
        let (service, events) = Service::start(Some(source), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Capture("worker backup".into()))?;
        let original = next_snapshot(&events, 0)[0].id;
        service.send(Command::ExportBackup(backup.clone()))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(Event::BackupExported(result)) => {
                    let report = result.map_err(anyhow::Error::msg)?;
                    assert_eq!(report.total_items, 1);
                    break;
                }
                Ok(Event::Error(message)) => panic!("{message}"),
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "backup timed out");
            thread::sleep(Duration::from_millis(10));
        }
        let (database, report) = restore_backup_data(&backup, restored)?;
        assert_eq!(report.total_items, 1);
        assert_eq!(History::open(database)?.text(original)?, "worker backup");
        Ok(())
    }

    #[test]
    fn clear_history_preserves_favorites_and_rejects_stale_group_view() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let group = history.create_group("Work")?;
        let removed = history.capture("remove")?.unwrap();
        history.move_to_group(removed, None, Some(group.id))?;
        let favorite = history.capture("favorite")?.unwrap();
        history.move_to_group(favorite, None, Some(group.id))?;
        history.toggle_favorite(favorite)?;
        let default = history.capture("default")?.unwrap();
        drop(history);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Query {
            search: "favorite".into(),
            favorite_only: true,
            category: ContentCategory::All,
            group_id: Some(group.id),
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1).len(), 1);
        service.send(Command::ClearHistory {
            group_id: Some(group.id),
            generation: 0,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::HistoryCleared(Err(_))
        ));
        service.send(Command::ClearHistory {
            group_id: Some(group.id),
            generation: 1,
        })?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        assert_eq!(next_snapshot(&events, 1).len(), 1);
        assert!(matches!(
            events.recv_blocking()?,
            Event::HistoryCleared(Ok(1))
        ));
        let history = History::open(directory.path().join("clipboard.db"))?;
        assert!(history.item(removed).is_err());
        assert!(history.item(favorite)?.is_favorite);
        assert_eq!(history.text(default)?, "default");
        Ok(())
    }

    #[test]
    fn clear_all_history_removes_protected_items_across_groups() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let group = history.create_group("Keep group")?;
        let pinned = history.capture("pinned")?.unwrap();
        history.toggle_pin(pinned)?;
        let favorite = history.capture("favorite")?.unwrap();
        history.toggle_favorite(favorite)?;
        history.move_to_group(favorite, None, Some(group.id))?;
        drop(history);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::ClearAllHistory)?;
        assert!(next_snapshot(&events, 0).is_empty());
        assert!(matches!(
            events.recv_blocking()?,
            Event::AllHistoryCleared(Ok(2))
        ));
        drop(service);

        let history = History::open(directory.path().join("clipboard.db"))?;
        assert_eq!(history.count("", false)?, 0);
        assert_eq!(history.count_in_group("", false, Some(group.id))?, 0);
        assert!(history.groups()?.iter().any(|item| item.id == group.id));
        Ok(())
    }

    #[test]
    fn batch_delete_rejects_stale_or_cross_group_selection() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let group = history.create_group("Work")?;
        let first = history.capture("first")?.unwrap();
        let second = history.capture("second")?.unwrap();
        history.move_to_group(first, None, Some(group.id))?;
        history.move_to_group(second, None, Some(group.id))?;
        let other = history.capture("other")?.unwrap();
        drop(history);

        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::Query {
            search: String::new(),
            favorite_only: false,
            category: ContentCategory::All,
            group_id: Some(group.id),
            limit: PAGE_SIZE,
            generation: 1,
        })?;
        assert_eq!(next_snapshot(&events, 1).len(), 2);
        for (ids, generation) in [(vec![first, second], 0), (vec![first, other], 1)] {
            service.send(Command::DeleteBatch {
                ids,
                group_id: Some(group.id),
                generation,
            })?;
            assert!(matches!(
                events.recv_blocking()?,
                Event::BatchDeleted(Err(_))
            ));
        }
        service.send(Command::DeleteBatch {
            ids: vec![first, second],
            group_id: Some(group.id),
            generation: 1,
        })?;
        assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
        assert!(next_snapshot(&events, 1).is_empty());
        assert!(matches!(
            events.recv_blocking()?,
            Event::BatchDeleted(Ok(2))
        ));
        let history = History::open(directory.path().join("clipboard.db"))?;
        assert!(history.item(first).is_err());
        assert!(history.item(second).is_err());
        assert_eq!(history.text(other)?, "other");
        Ok(())
    }
}
