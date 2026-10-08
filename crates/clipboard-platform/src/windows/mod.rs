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
        PasteShortcutConfig, Preferences, ThemePreference, WindowPositionPreference,
        WindowSizePreference,
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
    QueryDailyCounts(String),
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
    /// 手动剪贴板注入入口：生产路径不发送，仅供本文件测试与
    /// examples/isolated_clipboard_smoke.rs 绕过监视器直接投喂样本。
    Capture(String),
    /// 同 `Capture`：测试/QA 注入口（见上方说明）。
    CaptureRich {
        html: Option<String>,
        rtf: Option<Vec<u8>>,
        text: Option<String>,
    },
    /// 同 `Capture`：测试/QA 注入口（见上方说明）。
    CaptureFiles(Vec<String>),
    /// 同 `Capture`：测试/QA 注入口（见上方说明）。
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
            Self::QueryDailyCounts(_) => Some(FailureKind::Other),
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
    DisplaySaved(Result<DisplayPreference, String>),
    AudioSaved(Result<AudioPreference, String>),
    MonitorTypesSaved(Result<MonitorTypesPreference, String>),
    AppFilterSaved(Result<Box<AppFilterPreference>, String>),
    RunningApps(Vec<source_app::RunningApp>),
    OnboardingCompleted(Result<(), String>),
    AutostartSaved(Result<bool, String>),
    DataSize(Result<DataSizeInfo, String>),
    DailyCounts(Result<Vec<(String, i64)>, String>),
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

fn lock_capture_state(
    state: &Mutex<CaptureState>,
) -> Result<std::sync::MutexGuard<'_, CaptureState>> {
    state.lock().map_err(|_| anyhow!("剪贴板状态异常"))
}

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
        let mut state = lock_capture_state(&self.state)?;
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
        let icons_dir = service.data_dir.join("icons");
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
                    icons_dir,
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
                    let _ = worker.events.send_blocking(Event::Error(error.to_string()));
                }
                while !stop.load(Ordering::Acquire) {
                    match incoming.recv_timeout(Duration::from_millis(250)) {
                        Ok(command) => {
                            let failure_kind = command.failure_kind();
                            let observed_capture =
                                matches!(command, Command::ObservedCapture { .. });
                            if let Err(error) = worker.handle(command) {
                                match failure_kind {
                                    None if observed_capture => {
                                        let _ = worker
                                            .events
                                            .try_send(Event::BackgroundError(error.to_string()));
                                    }
                                    None => {
                                        let _ = worker.events.send_blocking(
                                            Event::BackgroundError(error.to_string()),
                                        );
                                    }
                                    Some(kind) => {
                                        let _ = worker.events.send_blocking(Event::CommandFailed {
                                            kind,
                                            message: error.to_string(),
                                        });
                                    }
                                }
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
        self.stop.store(true, Ordering::Release);
        // Closing first releases both the worker and the instance signal under UI backpressure.
        self.events.close();
        self.instance_signal = None;
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

fn item_paths_for_action(item: &ClipboardItem, staged_dir: &Path) -> Result<Vec<String>> {
    let paths = match item.content_type.as_str() {
        "files" => History::file_paths_for_copy(item, staged_dir)?,
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

fn save_item_as(item: &ClipboardItem, staged_dir: &Path, destination: &Path) -> Result<u64> {
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

    let paths = item_paths_for_action(item, staged_dir)?;
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

fn write_rich_clipboard(
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
    ignored_sequence: &mut u32,
) -> Result<()> {
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
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
        if let Some(rtf) = rtf {
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

fn rich_clipboard_matches(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
) -> bool {
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
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
    let rtf_matches = rtf.is_none_or(|expected| {
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

fn verified_rich_sequence(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
) -> Result<u32> {
    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        if !rich_clipboard_matches(clipboard, item, rtf) {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyMode {
    Original,
    PlainText,
    Paths,
}

struct Worker {
    history: History,
    preferences: Preferences,
    clipboard: ClipboardContext,
    images_dir: PathBuf,
    staged_dir: PathBuf,
    icons_dir: PathBuf,
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
    /// Blocking event delivery; the only failure mode is a closed UI receiver.
    fn send_event(&self, event: Event) -> Result<()> {
        self.events
            .send_blocking(event)
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    fn send_groups(&self) -> Result<()> {
        self.send_event(Event::Groups(self.history.groups()?))
    }

    fn save_setting<T: Copy>(
        &self,
        value: T,
        save: fn(&Preferences, T) -> Result<()>,
        event: fn(std::result::Result<T, String>) -> Event,
    ) -> Result<()> {
        let result = save(&self.preferences, value)
            .map(|_| value)
            .map_err(|error| error.to_string());
        self.send_event(event(result))
    }

    fn snapshot(&self) -> Result<()> {
        self.send_event(Event::Snapshot {
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
    }

    fn handle(&mut self, command: Command) -> Result<()> {
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
                self.capture_image(&png, width, height)?;
            }
            Command::ObservedCapture { content, source } => {
                self.handle_observed_capture(content, source)?;
            }
            Command::MergeCopy(ids) => return self.handle_merge(ids, false),
            Command::MergeForPaste(ids) => return self.handle_merge(ids, true),
            Command::RevealInExplorer(id) => {
                let item = self.history.item(id)?;
                let paths = item_paths_for_action(&item, &self.staged_dir)?;
                reveal_in_explorer(Path::new(&paths[0]))?;
                self.send_event(Event::Status(if paths.len() == 1 {
                    "已在资源管理器中定位".into()
                } else {
                    format!("已在资源管理器中定位第一项，共 {} 项", paths.len())
                }))?;
                return Ok(());
            }
            Command::SaveAs { id, destination } => {
                let result = self
                    .history
                    .item(id)
                    .and_then(|item| save_item_as(&item, &self.staged_dir, &destination))
                    .map(|_| destination)
                    .map_err(|error| error.to_string());
                self.send_event(Event::SavedAs { id, result })?;
                return Ok(());
            }
            Command::Copy(id) => return self.handle_copy(id, false, CopyMode::Original),
            Command::CopyPlainText(id) => return self.handle_copy(id, false, CopyMode::PlainText),
            Command::CopyPlainTextForPaste(id) => {
                return self.handle_copy(id, true, CopyMode::PlainText);
            }
            Command::CopyForPaste(id) => return self.handle_copy(id, true, CopyMode::Original),
            Command::CopyPath(id) => return self.handle_copy(id, false, CopyMode::Paths),
            Command::CopyPathForPaste(id) => return self.handle_copy(id, true, CopyMode::Paths),
            Command::Preview { id, generation } => {
                self.send_event(Event::Preview {
                    id,
                    generation,
                    result: self
                        .history
                        .preview_content_with_staged(id, &self.staged_dir)
                        .map_err(|error| error.to_string()),
                })?;
                return Ok(());
            }
            Command::HoverPreview { id, generation } => {
                self.send_event(Event::HoverPreview {
                    id,
                    generation,
                    result: self
                        .history
                        .preview_content_with_staged(id, &self.staged_dir)
                        .map_err(|error| error.to_string()),
                })?;
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
                self.send_event(Event::TextEdited {
                    id,
                    generation,
                    result,
                })?;
                return Ok(());
            }
            Command::Delete(id) => self.history.delete_with_media(id, &self.images_dir)?,
            Command::DeleteBatch {
                ids,
                group_id,
                generation,
            } => return self.handle_delete_batch(ids, group_id, generation),
            Command::ClearHistory {
                group_id,
                generation,
            } => return self.handle_clear_history(group_id, generation),
            Command::ClearAllHistory => return self.handle_clear_all(),
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
                self.send_event(Event::QuickPasteResolved {
                    slot,
                    favorite,
                    result,
                })?;
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
                return self.handle_reorder(from, to, after, favorite_only, group_id, generation);
            }
            Command::CreateGroup(name) => return self.handle_create_group(name),
            Command::ReorderGroup { from, to, after } => {
                return self.handle_reorder_group(from, to, after);
            }
            Command::RenameGroup { id, name } => return self.handle_rename_group(id, name),
            Command::DeleteGroup { id, generation } => {
                return self.handle_delete_group(id, generation);
            }
            Command::MoveToGroup {
                id,
                source_group_id,
                target_group_id,
                generation,
            } => {
                return self.handle_move_to_group(id, source_group_id, target_group_id, generation);
            }
            Command::SetTheme(theme) => {
                return self.save_setting(theme, Preferences::set_theme, Event::ThemeSaved);
            }
            Command::SetLanguage(language) => {
                return self.save_setting(
                    language,
                    Preferences::set_language,
                    Event::LanguageSaved,
                );
            }
            Command::SetHotkey(hotkey) => return self.handle_set_hotkey(hotkey),
            Command::SetWindowSize(size) => {
                return self.save_setting(
                    size,
                    Preferences::set_window_size,
                    Event::WindowSizeSaved,
                );
            }
            Command::SetPersistWindowSize(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_persist_window_size,
                    Event::PersistWindowSizeSaved,
                );
            }
            Command::SetAutoResetState(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_auto_reset_state,
                    Event::AutoResetStateSaved,
                );
            }
            Command::SetSearchAutoFocus(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_search_auto_focus,
                    Event::SearchAutoFocusSaved,
                );
            }
            Command::SetSearchAutoClear(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_search_auto_clear,
                    Event::SearchAutoClearSaved,
                );
            }
            Command::SetSkipClearConfirm(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_skip_clear_confirm,
                    Event::SkipClearConfirmSaved,
                );
            }
            Command::SetPasteCloseWindow(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_paste_close_window,
                    Event::PasteCloseWindowSaved,
                );
            }
            Command::SetPasteKey(key) => {
                return self.save_setting(key, Preferences::set_paste_key, Event::PasteKeySaved);
            }
            Command::SetPasteMoveToTop(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_paste_move_to_top,
                    Event::PasteMoveToTopSaved,
                );
            }
            Command::SetQuickPasteEnabled(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_quick_paste_enabled,
                    Event::QuickPasteEnabledSaved,
                );
            }
            Command::SetPasteShortcuts(shortcuts) => {
                return self.handle_set_paste_shortcuts(shortcuts);
            }
            Command::SetWindowPosition(position) => {
                return self.save_setting(
                    position,
                    Preferences::set_window_position,
                    Event::WindowPositionSaved,
                );
            }
            Command::SetHoverPreview(preference) => {
                return self.save_setting(
                    preference,
                    Preferences::set_hover_preview,
                    Event::HoverPreviewSaved,
                );
            }
            Command::SetDisplay(preference) => {
                return self.save_setting(
                    preference,
                    Preferences::set_display,
                    Event::DisplaySaved,
                );
            }
            Command::SetAudio(preference) => {
                return self.save_setting(preference, Preferences::set_audio, Event::AudioSaved);
            }
            Command::SetMonitorTypes(preference) => {
                return self.handle_set_monitor_types(preference);
            }
            Command::SetAppFilter(preference) => return self.handle_set_app_filter(preference),
            Command::ListRunningApps => {
                let apps = source_app::running_apps(&self.icons_dir);
                self.send_event(Event::RunningApps(apps))?;
                return Ok(());
            }
            Command::CompleteOnboarding => {
                let result = self
                    .preferences
                    .set_onboarding_completed()
                    .map_err(|error| error.to_string());
                self.send_event(Event::OnboardingCompleted(result))?;
                return Ok(());
            }
            Command::SetAutostart(enabled) => {
                let result = autostart::set_enabled(&self.data_dir, enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.send_event(Event::AutostartSaved(result))?;
                return Ok(());
            }
            Command::QueryDataSize => {
                let result = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .map_err(|error| error.to_string());
                self.send_event(Event::DataSize(result))?;
                return Ok(());
            }
            Command::QueryDailyCounts(start_date) => {
                let result = self
                    .history
                    .daily_counts(&start_date)
                    .map_err(|error| error.to_string());
                self.send_event(Event::DailyCounts(result))?;
                return Ok(());
            }
            Command::OptimizeDatabase => {
                self.history.optimize_storage()?;
                let size = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .context("数据库已整理，但刷新数据占用失败")?;
                self.send_event(Event::DatabaseOptimized(size))?;
                return Ok(());
            }
            Command::OpenDataDirectory => {
                open_data_directory(&self.data_dir)?;
                self.send_event(Event::Status("已打开数据目录".into()))?;
                return Ok(());
            }
            Command::ExportBackup(destination) => {
                self.send_event(Event::BackupExported(
                    self.history
                        .export_backup(&destination)
                        .map_err(|error| error.to_string()),
                ))?;
                return Ok(());
            }
            Command::Pause(paused) => return self.handle_pause(paused),
        }
        self.snapshot()
    }

    /// 列表视图是否仍是命令发起时的视图（分组与代次一致）。
    fn view_is_current(&self, group_id: Option<i64>, generation: u64) -> bool {
        group_id == self.group_id && generation == self.generation
    }

    fn release_pending_image_bytes(&self, bytes: usize) -> Result<()> {
        let mut state = lock_capture_state(&self.state)?;
        state.pending_image_bytes = state.pending_image_bytes.saturating_sub(bytes);
        Ok(())
    }

    fn capture_image(&self, png: &[u8], width: u32, height: u32) -> Result<i64> {
        let result = self
            .history
            .capture_image(png, width, height, &self.images_dir);
        self.release_pending_image_bytes(png.len())?;
        result
    }

    fn handle_observed_capture(
        &mut self,
        content: CapturedClipboard,
        source: Option<source_app::SourceApp>,
    ) -> Result<()> {
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
                self.release_pending_image_bytes(image_bytes)?;
            }
            return Ok(());
        };
        let id = match content {
            CapturedClipboard::Text(text) => {
                self.history.capture_with_media(&text, &self.images_dir)?
            }
            CapturedClipboard::Rich { html, rtf, text } => Some(self.history.capture_rich(
                html.as_deref(),
                rtf.as_deref(),
                text.as_deref(),
                &self.images_dir,
            )?),
            CapturedClipboard::Files(paths) => {
                Some(self.history.capture_files(&paths, &self.images_dir)?)
            }
            CapturedClipboard::Image { png, width, height } => {
                Some(self.capture_image(&png, width, height)?)
            }
        };
        if let (Some(id), Some(source)) = (id, source) {
            let icon = source.executable.as_deref().and_then(|executable| {
                source_app::extract_and_cache_icon(executable, &self.icons_dir)
            });
            self.history
                .set_source_app(id, &source.name, icon.as_deref())?;
        }
        Ok(())
    }

    fn handle_merge(&mut self, ids: Vec<i64>, for_paste: bool) -> Result<()> {
        let item_count = ids.len();
        let merged = self.history.merge_content(&ids, &self.staged_dir)?;
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_merged_clipboard(&self.clipboard, &merged, &mut state.ignored_sequence)?;
        drop(state);
        self.send_event(Event::Merged {
            for_paste,
            clipboard_sequence,
            item_count,
        })
    }

    fn handle_copy(&mut self, id: i64, for_paste: bool, mode: CopyMode) -> Result<()> {
        let item = self.history.item(id)?;
        let plain_text = (mode == CopyMode::PlainText)
            .then(|| plain_text_for_copy(&item).map(str::to_owned))
            .transpose()?;
        let path_text = (mode == CopyMode::Paths)
            .then(|| item_paths_for_action(&item, &self.staged_dir).map(|paths| paths.join("\n")))
            .transpose()?;
        let is_rich =
            mode == CopyMode::Original && matches!(item.content_type.as_str(), "html" | "rtf");
        let files = if mode == CopyMode::Original && item.content_type == "files" {
            let paths = History::file_paths_for_copy(&item, &self.staged_dir)?;
            if paths.iter().any(|path| !Path::new(path).exists()) {
                bail!("源文件或文件夹已不存在，无法复制");
            }
            Some(paths)
        } else {
            None
        };
        let image = if mode == CopyMode::Original && item.content_type == "image" {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            if !Path::new(path).is_file() {
                bail!("图片文件已丢失，无法复制");
            }
            let image =
                RustImageData::from_path(path).map_err(|error| anyhow!("打开图片失败：{error}"))?;
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
        let mut state = lock_capture_state(&self.state)?;
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
            let rtf = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
            write_rich_clipboard(&item, rtf.as_deref(), &mut state.ignored_sequence)?;
            state.ignored_sequence =
                verified_rich_sequence(&self.clipboard, &item, rtf.as_deref())?;
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
        self.send_event(Event::Copied {
            id,
            for_paste,
            clipboard_sequence,
            message: match item.content_type.as_str() {
                _ if mode == CopyMode::Paths => "路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::PlainText => "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "image" => "图片已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "files" => "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" if rich_preserved => "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" => "富文本格式未写回，已按纯文本复制",
                _ => "已复制，可切换到目标应用按 Ctrl+V 粘贴",
            }
            .into(),
        })
    }

    fn handle_delete_batch(
        &mut self,
        ids: Vec<i64>,
        group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result = if self.view_is_current(group_id, generation) {
            self.history
                .delete_batch_with_media(&ids, group_id, &self.images_dir)
        } else {
            Err(anyhow!("列表已切换，请重新选择要删除的记录"))
        };
        if result.is_ok() {
            self.send_groups()?;
            self.snapshot()?;
        }
        self.send_event(Event::BatchDeleted(
            result.map_err(|error| error.to_string()),
        ))
    }

    fn handle_clear_history(&mut self, group_id: Option<i64>, generation: u64) -> Result<()> {
        let result = if self.view_is_current(group_id, generation) {
            self.history
                .clear_history_with_media(group_id, &self.images_dir)
        } else {
            Err(anyhow!("列表已切换，请重新选择要清理的分组"))
        };
        if result.is_ok() {
            self.send_groups()?;
            self.snapshot()?;
        }
        self.send_event(Event::HistoryCleared(
            result.map_err(|error| error.to_string()),
        ))
    }

    fn handle_clear_all(&mut self) -> Result<()> {
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
        self.send_event(Event::AllHistoryCleared(
            result.map_err(|error| error.to_string()),
        ))
    }

    fn handle_reorder(
        &mut self,
        from: i64,
        to: i64,
        after: bool,
        favorite_only: bool,
        group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result =
            if self.view_is_current(group_id, generation) && favorite_only == self.favorite_only {
                self.history
                    .reorder_in_group(from, to, after, favorite_only, group_id)
            } else {
                Err(anyhow!("列表已切换，请重新拖动"))
            };
        let result = result.and_then(|_| {
            self.snapshot()
                .context("顺序已保存，但列表刷新失败，请重启应用后查看")
        });
        self.send_event(Event::Reordered {
            from,
            generation,
            result: result.map_err(|error| error.to_string()),
        })
    }

    fn handle_create_group(&mut self, name: String) -> Result<()> {
        let result = self
            .history
            .create_group(&name)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.send_groups()?;
        }
        self.send_event(Event::GroupCreated(result))
    }

    fn handle_reorder_group(&mut self, from: i64, to: i64, after: bool) -> Result<()> {
        let result = self.history.reorder_group(from, to, after).and_then(|_| {
            self.send_groups()
                .context("分组顺序已保存，但列表刷新失败，请重启应用后查看")
        });
        self.send_event(Event::GroupReordered {
            from,
            result: result.map_err(|error| error.to_string()),
        })
    }

    fn handle_rename_group(&mut self, id: i64, name: String) -> Result<()> {
        let result = self
            .history
            .rename_group(id, &name)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.send_groups()?;
        }
        self.send_event(Event::GroupRenamed(result))
    }

    fn handle_delete_group(&mut self, id: i64, generation: u64) -> Result<()> {
        let result = if self.view_is_current(Some(id), generation) {
            self.history.delete_group_preserving_items(id)
        } else {
            Err(anyhow!("列表已切换，请重新选择分组"))
        }
        .map_err(|error| error.to_string());
        if result.is_ok() {
            self.group_id = None;
            self.send_groups()?;
        }
        self.send_event(Event::GroupDeleted(result))
    }

    fn handle_move_to_group(
        &mut self,
        id: i64,
        source_group_id: Option<i64>,
        target_group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result = if self.view_is_current(source_group_id, generation) {
            self.history
                .move_to_group(id, source_group_id, target_group_id)
        } else {
            Err(anyhow!("列表已切换，请重新选择记录"))
        };
        if result.is_ok() {
            self.send_groups()?;
            self.snapshot()?;
        }
        self.send_event(Event::ItemMoved {
            id,
            result: result.map_err(|error| error.to_string()),
        })
    }

    fn handle_set_hotkey(&mut self, hotkey: HotkeyPreference) -> Result<()> {
        let result = self
            .preferences
            .paste_shortcuts()
            .and_then(|shortcuts| crate::hotkey::validate_paste_shortcuts(&shortcuts, hotkey))
            .and_then(|_| self.preferences.set_hotkey(hotkey))
            .map(|_| hotkey)
            .map_err(|error| error.to_string());
        self.send_event(Event::HotkeySaved(result))
    }

    fn handle_set_paste_shortcuts(&mut self, shortcuts: Box<PasteShortcutConfig>) -> Result<()> {
        let result =
            crate::hotkey::validate_paste_shortcuts(&shortcuts, self.preferences.hotkey()?)
                .and_then(|_| self.preferences.set_paste_shortcuts(&shortcuts))
                .map(|_| shortcuts)
                .map_err(|error| error.to_string());
        self.send_event(Event::PasteShortcutsSaved(result))
    }

    fn handle_set_monitor_types(&mut self, preference: MonitorTypesPreference) -> Result<()> {
        let result = self
            .preferences
            .set_monitor_types(preference)
            .map(|_| {
                self.monitor_types = preference;
                preference
            })
            .map_err(|error| error.to_string());
        self.send_event(Event::MonitorTypesSaved(result))
    }

    fn handle_set_app_filter(&mut self, preference: Box<AppFilterPreference>) -> Result<()> {
        let result = self
            .preferences
            .set_app_filter(&preference)
            .map(|_| {
                self.app_filter = *preference.clone();
                preference
            })
            .map_err(|error| error.to_string());
        self.send_event(Event::AppFilterSaved(result))
    }

    fn handle_pause(&mut self, paused: bool) -> Result<()> {
        self.preferences
            .set_capture_paused(paused)
            .context("无法保存暂停状态")?;
        let mut state = lock_capture_state(&self.state)?;
        state.paused = paused;
        state.last_sequence = unsafe { GetClipboardSequenceNumber() };
        drop(state);
        self.send_event(Event::Paused(paused))
    }
}

#[cfg(test)]
mod tests;
