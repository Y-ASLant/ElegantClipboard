use crate::source_app;
use anyhow::Result;
use clipboard_core::{
    ContentCategory, DeletionOutcome, PreviewContent,
    backup::BackupReport,
    database::{ClipboardItem, Group},
    preferences::{
        AppFilterPreference, AudioPreference, DisplayPreference, HotkeyPreference,
        HoverPreviewPreference, LanguagePreference, MonitorTypesPreference, PasteKeyPreference,
        PasteShortcutConfig, ThemePreference, WindowEffectPreference, WindowPositionPreference,
        WindowSizePreference,
    },
};
use std::path::PathBuf;

pub enum Command {
    Query {
        search: String,
        favorite_only: bool,
        category: ContentCategory,
        group_id: Option<i64>,
        limit: i64,
        generation: u64,
    },
    PrepareExternalDrag {
        id: i64,
        token: u64,
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
    SetWindowEffect(WindowEffectPreference),
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
    /// 手动剪贴板注入入口：生产路径不发送，仅供服务测试与
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
    pub(super) fn failure_kind(&self) -> Option<FailureKind> {
        match self {
            Self::PrepareExternalDrag { .. }
            | Self::Capture(_)
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
            | Self::SetWindowEffect(_)
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
    ExternalDragPrepared {
        id: i64,
        token: u64,
        result: Result<crate::external_drag::PreparedExternalDrag, String>,
    },
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
    ItemDeleted {
        id: i64,
        outcome: DeletionOutcome,
    },
    HistoryCleared(Result<DeletionOutcome, String>),
    AllHistoryCleared(Result<DeletionOutcome, String>),
    BatchDeleted(Result<DeletionOutcome, String>),
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
    WindowEffectSaved(Result<WindowEffectPreference, String>),
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
