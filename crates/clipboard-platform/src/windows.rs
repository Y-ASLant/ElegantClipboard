use crate::{
    autostart,
    instance::{self, InstanceSignal},
};
use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::{
    History, MAX_FILE_PATHS, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_PATH_LIST_BYTES,
    MAX_TEXT_BYTES, PAGE_SIZE, PreviewContent,
    backup::{BackupReport, RestoreReport, restore_backup},
    database::{ClipboardItem, Database, Group},
    import::{ImportReport, import_legacy_database},
    legacy_backup::{LegacyBackupReport, import_legacy_backup},
    preferences::{HotkeyPreference, Preferences, ThemePreference},
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
        group_id: Option<i64>,
        limit: i64,
        generation: u64,
    },
    Copy(i64),
    CopyPlainText(i64),
    CopyForPaste(i64),
    CreateGroup(String),
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
    TogglePin(i64),
    ToggleFavorite(i64),
    Pause(bool),
    SetTheme(ThemePreference),
    SetHotkey(HotkeyPreference),
    SetAutostart(bool),
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
}

pub enum Event {
    ShowWindow,
    Groups(Vec<Group>),
    GroupCreated(Result<Group, String>),
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
        message: String,
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
    TextEdited {
        id: i64,
        generation: u64,
        result: Result<bool, String>,
    },
    HistoryCleared(Result<i64, String>),
    BatchDeleted(Result<i64, String>),
    Paused(bool),
    ThemeSaved(Result<ThemePreference, String>),
    HotkeySaved(Result<HotkeyPreference, String>),
    AutostartSaved(Result<bool, String>),
    BackupExported(Result<BackupReport, String>),
    Error(String),
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
            let _ = self.events.try_send(Event::Error(error.to_string()));
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
                .try_send(Command::CaptureFiles(paths))
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
                    .try_send(Command::CaptureRich { html, rtf, text })
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
                .try_send(Command::Capture(text))
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
                .try_send(Command::CaptureImage {
                    png: bytes.to_vec(),
                    width,
                    height,
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
    pub initial_hotkey: HotkeyPreference,
    pub initial_paused: bool,
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
        let initial_hotkey = preferences.hotkey()?;
        let initial_paused = preferences.capture_paused()?;
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
            initial_hotkey,
            initial_paused,
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
                    search: String::new(),
                    favorite_only: false,
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
                            if let Err(error) = worker.handle(command) {
                                let _ = worker.events.try_send(Event::Error(error.to_string()));
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
                            let _ = watcher_events
                                .try_send(Event::Error("剪贴板监听已停止，请重启应用".into()));
                        }
                    })?,
            );
        }
        match InstanceSignal::start(&service.data_dir, events.clone()) {
            Ok(signal) => service.instance_signal = Some(signal),
            Err(error) => {
                let _ = events.try_send(Event::Error(format!("重复启动唤出不可用：{error}")));
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

struct Worker {
    history: History,
    preferences: Preferences,
    clipboard: ClipboardContext,
    images_dir: PathBuf,
    staged_dir: PathBuf,
    data_dir: PathBuf,
    state: Arc<Mutex<CaptureState>>,
    search: String,
    favorite_only: bool,
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
                items: self.history.list_in_group(
                    &self.search,
                    self.limit,
                    self.favorite_only,
                    self.group_id,
                )?,
                total: self.history.count_in_group(
                    &self.search,
                    self.favorite_only,
                    self.group_id,
                )?,
                generation: self.generation,
            })
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    fn handle(&mut self, command: Command) -> Result<()> {
        let for_paste = matches!(&command, Command::CopyForPaste(_));
        let plain_only = matches!(&command, Command::CopyPlainText(_));
        match command {
            Command::Query {
                search,
                favorite_only,
                group_id,
                limit,
                generation,
            } => {
                self.search = search;
                self.favorite_only = favorite_only;
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
            Command::Copy(id) | Command::CopyPlainText(id) | Command::CopyForPaste(id) => {
                let item = self.history.item(id)?;
                let plain_text = plain_only
                    .then(|| plain_text_for_copy(&item).map(str::to_owned))
                    .transpose()?;
                let is_rich = !plain_only && matches!(item.content_type.as_str(), "html" | "rtf");
                let mut rich_contents = Vec::new();
                if is_rich {
                    if let Some(text) = item
                        .text_content
                        .as_deref()
                        .filter(|value| !value.is_empty())
                    {
                        rich_contents.push(ClipboardContent::Text(text.to_owned()));
                    }
                    if let Some(html) = item
                        .html_content
                        .as_deref()
                        .filter(|value| !value.is_empty())
                    {
                        rich_contents.push(ClipboardContent::Html(html.to_owned()));
                    }
                    if let Some(bytes) = item
                        .rtf_content
                        .as_deref()
                        .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
                    {
                        rich_contents
                            .push(ClipboardContent::Other("Rich Text Format".into(), bytes));
                    }
                    if rich_contents.is_empty() {
                        bail!("富文本记录没有可写回的有效格式");
                    }
                }
                let files = if item.content_type == "files" {
                    let paths = self.history.files_for_copy(id, &self.staged_dir)?;
                    if paths.iter().any(|path| !Path::new(path).exists()) {
                        bail!("源文件或文件夹已不存在，无法复制");
                    }
                    Some(paths)
                } else {
                    None
                };
                let image = if item.content_type == "image" {
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
                if let Some(files) = files {
                    self.clipboard
                        .set_files(files)
                        .map_err(|error| anyhow!("复制文件失败：{error}"))?;
                } else if let Some(image) = image {
                    self.clipboard
                        .set_image(image)
                        .map_err(|error| anyhow!("复制图片失败：{error}"))?;
                } else if is_rich {
                    self.clipboard
                        .set(rich_contents)
                        .map_err(|error| anyhow!("复制富文本失败：{error}"))?;
                    state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
                    rich_preserved = item.html_content.as_deref().is_some_and(|html| {
                        !html.is_empty()
                            && self
                                .clipboard
                                .get_html()
                                .ok()
                                .is_some_and(|read| !read.is_empty())
                    }) || item.rtf_content.as_deref().is_some_and(|rtf| {
                        clipboard_core::rich::decode_rtf_for_clipboard(rtf).is_some()
                            && self
                                .clipboard
                                .get_buffer("Rich Text Format")
                                .ok()
                                .is_some_and(|bytes| !bytes.is_empty())
                    });
                    let text_copied = item.text_content.as_deref().is_some_and(|text| {
                        !text.is_empty()
                            && self
                                .clipboard
                                .get_text()
                                .ok()
                                .is_some_and(|read| !read.is_empty())
                    });
                    if !rich_preserved && !text_copied {
                        bail!("富文本写回后未检测到可用格式");
                    }
                } else {
                    let text = plain_text
                        .or(item.text_content)
                        .context("记录没有可复制的文本")?;
                    self.clipboard
                        .set_text(text)
                        .map_err(|error| anyhow!("复制失败：{error}"))?;
                }
                state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
                self.events
                    .send_blocking(Event::Copied {
                        id,
                        for_paste,
                        message: match item.content_type.as_str() {
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
                            .preview_content(id)
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
            Command::ToggleFavorite(id) => {
                self.history.toggle_favorite(id)?;
            }
            Command::TogglePin(id) => {
                self.history.toggle_pin(id)?;
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
            Command::SetHotkey(hotkey) => {
                let result = self
                    .preferences
                    .set_hotkey(hotkey)
                    .map(|_| hotkey)
                    .map_err(|error| error.to_string());
                self.events
                    .send_blocking(Event::HotkeySaved(result))
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
        drop(service);
        let (_service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(next_snapshot(&events, 0)[0].id, first);
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
            group_id: None,
            limit: PAGE_SIZE,
            generation: 2,
        })?;
        assert_eq!(next_snapshot(&events, 2).len(), 2);
        Ok(())
    }

    #[test]
    fn image_capture_is_persisted_and_delete_removes_managed_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        let png = include_bytes!("../../../clipboard-rs/tests/test.png");
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
                    assert_eq!(result.unwrap(), PreviewContent::Files(vec![path.clone()]));
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
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(Event::Error(message)) = events.try_recv() {
                assert!(message.contains("已不存在"));
                break;
            }
            assert!(std::time::Instant::now() < deadline, "copy error timed out");
            thread::sleep(Duration::from_millis(10));
        }
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
