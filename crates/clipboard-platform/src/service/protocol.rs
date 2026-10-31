use crate::source_app;
use anyhow::Result;
use clipboard_core::{
    ContentCategory, DeletionOutcome, NativeFormatCapture, PreviewContent, TextTransform,
    backup::BackupReport,
    database::{ClipboardItem, Group},
    preferences::{
        AppFilterPreference, AudioPreference, CornerRadiusPreference, DedupStrategy,
        DisplayPreference, HotkeyPreference, HoverPreviewPreference, LanguagePreference,
        MonitorTypesPreference, PasteKeyPreference, PasteShortcutConfig, ThemeColor,
        ThemePreference, WindowEffectPreference, WindowPositionPreference, WindowSizePreference,
    },
    retention::RetentionPolicy,
};
use std::path::PathBuf;

/// 按格式粘贴的目标格式（富文本条目）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFormat {
    Html,
    Rtf,
}

/// 选择写入剪贴板的内容集合；各平台负责实际写入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    Original,
    PlainText,
    Paths,
    Files,
    /// 单一富文本格式；缺少该格式时报错，不降级。
    Html,
    Rtf,
    Transform(TextTransform),
}

impl From<CopyFormat> for CopyMode {
    fn from(format: CopyFormat) -> Self {
        match format {
            CopyFormat::Html => Self::Html,
            CopyFormat::Rtf => Self::Rtf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyPurpose {
    Copy,
    Paste { request_id: u64 },
}

impl CopyPurpose {
    pub fn request_id(self) -> Option<u64> {
        match self {
            Self::Copy => None,
            Self::Paste { request_id } => Some(request_id),
        }
    }
}

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
    Copy {
        id: i64,
        mode: CopyMode,
        purpose: CopyPurpose,
    },
    /// 对应粘贴输入投递成功后请求恢复；输入投递不代表目标应用已消费内容。
    RestoreStashedClipboard {
        request_id: u64,
        clipboard_sequence: u32,
    },
    /// 取消粘贴时丢弃该请求的快照，不改写剪贴板。
    CancelPaste {
        request_id: u64,
    },
    /// 保存条目备注；空串清空（上限在 core 校验）。
    SetNote {
        id: i64,
        note: String,
    },
    RevealInExplorer(i64),
    SaveAs {
        id: i64,
        destination: PathBuf,
    },
    Merge {
        ids: Vec<i64>,
        purpose: CopyPurpose,
    },
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
    /// Load the stored plain text for the editing dialog without rendering HTML.
    EditContent {
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
    SetThemeColor(ThemeColor),
    SetCornerRadius(CornerRadiusPreference),
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
    SetRetention {
        policy: RetentionPolicy,
        confirmed: bool,
    },
    SetMonitorTypes(MonitorTypesPreference),
    SetAppFilter(Box<AppFilterPreference>),
    SetWindowEffect(WindowEffectPreference),
    SetDedupStrategy(DedupStrategy),
    SetRestoreClipboard(bool),
    /// 序列粘贴推进键（决策 0004）；空串停用。校验失败拒绝并回滚。
    SetSequenceHotkey(String),
    ListRunningApps,
    CompleteOnboarding,
    BumpToTop(i64),
    ResolveQuickPaste {
        request_id: u64,
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
        sequence: u32,
        content: CapturedClipboard,
        source: Option<source_app::SourceApp>,
        extra: Vec<NativeFormatCapture>,
        /// 实际采集的原生格式名称，与可还原的额外载荷独立。
        formats: Vec<String>,
        /// true 仅来自真实 watcher：先进 300ms 瞬时确认窗口；
        /// 测试与 smoke 注入 false 直接入库。
        deferred: bool,
    },
    SetTransientFilter(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    Paste(u64),
    QuickPaste(u64),
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
            Self::Copy {
                purpose: CopyPurpose::Paste { request_id },
                ..
            }
            | Self::Merge {
                purpose: CopyPurpose::Paste { request_id },
                ..
            } => Some(FailureKind::Paste(*request_id)),
            Self::ResolveQuickPaste { request_id, .. } => {
                Some(FailureKind::QuickPaste(*request_id))
            }
            Self::SaveAs { id, .. } => Some(FailureKind::SaveAs(*id)),
            Self::RestoreStashedClipboard { .. } | Self::CancelPaste { .. } => None,
            Self::SetNote { .. } => Some(FailureKind::Other),
            Self::Merge { .. } => Some(FailureKind::Merge),
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
            // 其余命令（各种复制、设置项、预览、删除等）失败时统一归为 Other。
            _ => Some(FailureKind::Other),
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
        purpose: CopyPurpose,
        clipboard_sequence: u32,
        message: String,
    },
    Merged {
        purpose: CopyPurpose,
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
    ThemeColorSaved(Result<ThemeColor, String>),
    CornerRadiusSaved(Result<CornerRadiusPreference, String>),
    TransientFilterSaved(Result<bool, String>),
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
        request_id: u64,
        result: Result<i64, String>,
    },
    WindowPositionSaved(Result<WindowPositionPreference, String>),
    HoverPreviewSaved(Result<HoverPreviewPreference, String>),
    DisplaySaved(Result<DisplayPreference, String>),
    AudioSaved(Result<AudioPreference, String>),
    RetentionSaved(Result<(RetentionPolicy, DeletionOutcome), String>),
    MonitorTypesSaved(Result<MonitorTypesPreference, String>),
    AppFilterSaved(Result<Box<AppFilterPreference>, String>),
    WindowEffectSaved(Result<WindowEffectPreference, String>),
    DedupStrategySaved(Result<DedupStrategy, String>),
    RestoreClipboardSaved(Result<bool, String>),
    SequenceHotkeySaved(Result<String, String>),
    NoteSaved(Result<i64, String>),
    /// 恢复完成；Ok(false) 包括无匹配快照、已取消或剪贴板已改变。
    ClipboardRestored(Result<bool, String>),
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
