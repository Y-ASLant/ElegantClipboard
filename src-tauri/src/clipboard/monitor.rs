use super::file_clipboard;
use super::handler::{
    cleanup_capture_content, cleanup_legacy_dib_companions, cleanup_stale_capture_files,
};
use super::source_app::SourceAppInfo;
use super::{ClipChangeSettings, ClipboardContent, ClipboardHandler, ImageCapture};
use crate::database::Database;
use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard as ClipboardTrait, ClipboardContext, ClipboardHandler as CRHandler, ClipboardWatcher,
    ClipboardWatcherContext,
};
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use tauri::{AppHandle, Emitter};
use tracing::{debug, error, info, warn};

/// 剪贴板监听服务
#[derive(Clone)]
pub struct ClipboardMonitor {
    running: Arc<AtomicBool>,
    /// 暂停计数器：> 0 时忽略剪贴板变化，防止并发复制操作竞态
    pause_count: Arc<AtomicU32>,
    /// 用户手动暂停（托盘菜单），独立于内部 pause_count
    user_paused: Arc<AtomicBool>,
    handler: Arc<RwLock<Option<Arc<ClipboardHandler>>>>,
    /// watcher 热路径读取，避免与 worker 争用 handler 锁
    clip_change_settings: Arc<RwLock<ClipChangeSettings>>,
    /// watcher 写入图片临时文件的目录
    capture_dir: Arc<RwLock<PathBuf>>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// 当前活动分组（None = 默认分组），与 AppState 共享
    active_group_id: Arc<Mutex<Option<i64>>>,
}

/// worker 线程接收的待处理剪贴板内容
struct CaptureWorkItem {
    content: ClipboardContent,
    source: Option<SourceAppInfo>,
    group_id: Option<i64>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            pause_count: Arc::new(AtomicU32::new(0)),
            user_paused: Arc::new(AtomicBool::new(false)),
            handler: Arc::new(RwLock::new(None)),
            clip_change_settings: Arc::new(RwLock::new(ClipChangeSettings::default())),
            capture_dir: Arc::new(RwLock::new(PathBuf::new())),
            thread_handle: Arc::new(Mutex::new(None)),
            active_group_id: Arc::new(Mutex::new(None)),
        }
    }

    /// 返回活动分组 Arc，供 AppState 共享
    pub fn active_group_id(&self) -> Arc<Mutex<Option<i64>>> {
        self.active_group_id.clone()
    }

    /// 初始化监控器（数据库与图片路径）
    pub fn init(&self, db: &Database, images_path: std::path::PathBuf) {
        let capture_dir = images_path.join("captures");
        std::fs::create_dir_all(&capture_dir).ok();
        cleanup_stale_capture_files(&capture_dir);
        cleanup_legacy_dib_companions(&images_path);

        let handler = Arc::new(ClipboardHandler::new(db, images_path));
        *self.clip_change_settings.write() = handler.get_clip_change_settings();
        *self.handler.write() = Some(handler);
        *self.capture_dir.write() = capture_dir;
        info!("Clipboard monitor initialized");
    }

    /// 设置变更后刷新 watcher 热路径缓存
    pub fn refresh_clip_change_settings(&self) {
        if let Some(handler) = self.handler.read().as_ref() {
            *self.clip_change_settings.write() = handler.get_clip_change_settings();
        }
    }

    /// 启动剪贴板监听（带自动重启 + 异步处理 worker）
    pub fn start(&self, app_handle: AppHandle) {
        // 用 compare_exchange 避免竞态
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            warn!("Clipboard monitor already running");
            return;
        }

        let running = self.running.clone();
        let pause_count = self.pause_count.clone();
        let user_paused = self.user_paused.clone();
        let handler = self.handler.clone();
        let active_group_id = self.active_group_id.clone();
        let capture_dir = self.capture_dir.clone();
        let clip_change_settings = self.clip_change_settings.clone();

        // ── 处理 worker 线程：从 channel 接收内容，串行处理 ──
        let (tx, rx) = mpsc::channel::<CaptureWorkItem>();

        let worker_handler = handler.clone();
        let worker_clip_settings = clip_change_settings.clone();
        let worker_running = running.clone();
        let worker_app = app_handle.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("clipboard-worker".into())
            .spawn(move || {
                Self::run_capture_worker(
                    rx,
                    worker_handler,
                    worker_clip_settings,
                    worker_running,
                    |id| {
                        let _ = worker_app.emit("clipboard-updated", id);
                    },
                );
            })
        {
            error!("Failed to spawn clipboard-worker thread: {e}");
            return;
        }

        // ── watcher 线程：OS 事件监听 + 快速校验 → 发送到 channel ──
        let handle = std::thread::spawn(move || {
            info!("Clipboard monitor thread started");

            // 带自动重启的监听循环
            let mut consecutive_failures: u32 = 0;
            const MAX_BACKOFF_MS: u64 = 5_000;

            while running.load(Ordering::SeqCst) {
                let clipboard_handler = MonitorHandler {
                    running: running.clone(),
                    pause_count: pause_count.clone(),
                    user_paused: user_paused.clone(),
                    active_group_id: active_group_id.clone(),
                    work_tx: tx.clone(),
                    capture_dir: capture_dir.clone(),
                    clip_change_settings: clip_change_settings.clone(),
                };

                let mut watcher = match ClipboardWatcherContext::new() {
                    Ok(w) => w,
                    Err(e) => {
                        error!("Failed to create clipboard watcher: {}", e);
                        break;
                    }
                };
                watcher.add_handler(clipboard_handler);

                info!("Clipboard watcher started");
                // start_watch() 阻塞直到 Stop 回调或内部错误
                watcher.start_watch();
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                // 异常退出 → 重启
                consecutive_failures += 1;
                let backoff = (100 * 2u64.pow(consecutive_failures.min(6))).min(MAX_BACKOFF_MS);
                warn!(
                    "Clipboard watcher exited, restarting in {}ms (failure #{})",
                    backoff, consecutive_failures
                );
                std::thread::sleep(std::time::Duration::from_millis(backoff));
            }

            // tx drop → worker 线程的 rx.recv() 返回 Err → worker 退出
            drop(tx);
            running.store(false, Ordering::SeqCst);
            info!("Clipboard monitor thread stopped");
        });

        // 保存线程句柄以便清理
        *self.thread_handle.lock() = Some(handle);
    }

    /// 处理 worker 主循环：按捕获顺序处理，内容去重由 handler 的策略决定。
    fn run_capture_worker(
        rx: mpsc::Receiver<CaptureWorkItem>,
        handler: Arc<RwLock<Option<Arc<ClipboardHandler>>>>,
        clip_change_settings: Arc<RwLock<ClipChangeSettings>>,
        running: Arc<AtomicBool>,
        mut emit_update: impl FnMut(i64),
    ) {
        info!("Clipboard worker thread started");

        while running.load(Ordering::SeqCst) {
            let Ok(item) = rx.recv() else {
                break;
            };

            if !clip_change_settings
                .read()
                .is_content_type_allowed(&item.content)
            {
                cleanup_capture_content(&item.content);
                debug!("Clipboard change ignored (content type not allowed)");
                continue;
            }

            let handler = handler.read().clone();
            let Some(h) = handler else {
                cleanup_capture_content(&item.content);
                continue;
            };

            // unwind 构建下隔离单条处理异常；release 的 panic=abort 不会执行此恢复路径。
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                h.process(item.content, item.source, item.group_id)
            }));

            match result {
                Ok(Ok(Some(id))) => {
                    debug!("Processed clipboard item: {}", id);
                    emit_update(id);
                }
                Ok(Ok(None)) => {
                    debug!("Clipboard content already exists");
                }
                Ok(Err(e)) => {
                    error!("Failed to process clipboard: {}", e);
                }
                Err(panic_info) => {
                    let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    error!("Clipboard worker panic during process(): {}", msg);
                }
            }
        }

        info!("Clipboard worker thread stopped");
    }

    /// 暂停监控（递增暂停计数，支持多个并发暂停）
    pub fn pause(&self) {
        let count = self.pause_count.fetch_add(1, Ordering::SeqCst);
        debug!("Clipboard monitor paused (count: {})", count + 1);
    }

    /// 恢复监控（递减暂停计数，归零时真正恢复）
    pub fn resume(&self) {
        if let Ok(prev) =
            self.pause_count
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                    if current > 0 { Some(current - 1) } else { None }
                })
        {
            debug!("Clipboard monitor resume (count: {})", prev - 1);
        } else {
            warn!("Resume called when not paused");
        }
    }

    pub fn is_paused(&self) -> bool {
        self.pause_count.load(Ordering::SeqCst) > 0
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn toggle_user_pause(&self) -> bool {
        let was = self.user_paused.fetch_xor(true, Ordering::SeqCst);
        let now = !was;
        info!("Clipboard monitor user pause toggled: {}", now);
        now
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

struct MonitorHandler {
    running: Arc<AtomicBool>,
    pause_count: Arc<AtomicU32>,
    user_paused: Arc<AtomicBool>,
    active_group_id: Arc<Mutex<Option<i64>>>,
    work_tx: mpsc::Sender<CaptureWorkItem>,
    capture_dir: Arc<RwLock<PathBuf>>,
    clip_change_settings: Arc<RwLock<ClipChangeSettings>>,
}

impl MonitorHandler {
    fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
            && self.pause_count.load(Ordering::SeqCst) == 0
            && !self.user_paused.load(Ordering::SeqCst)
    }

    fn capture_with_retry(
        &self,
        mut sequence_number: impl FnMut() -> u32,
        mut read_source: impl FnMut() -> Option<SourceAppInfo>,
        mut read_content: impl FnMut(usize) -> Option<ClipboardContent>,
        mut wait: impl FnMut(u64),
    ) -> Option<CaptureWorkItem> {
        const RETRY_DELAYS_MS: [u64; 7] = [0, 40, 80, 140, 220, 360, 560];

        for (attempt, &delay) in RETRY_DELAYS_MS.iter().enumerate() {
            if delay > 0 {
                wait(delay);
            }
            if !self.is_active() {
                debug!("Clipboard change ignored (stopped or paused)");
                return None;
            }

            // 来源查询与所有格式读取必须属于同一个剪贴板序列。
            let seq_before = sequence_number();
            let source = read_source();
            let max_image_bytes = {
                let settings = self.clip_change_settings.read();
                if settings.is_source_app_excluded(&source) {
                    if sequence_number() != seq_before {
                        continue;
                    }
                    debug!("Clipboard change ignored (source app excluded)");
                    return None;
                }
                settings.max_image_bytes
            };
            if !self.is_active() {
                return None;
            }

            let content = read_content(max_image_bytes);
            if !self.is_active() {
                return None;
            }
            if sequence_number() != seq_before {
                debug!(
                    "Clipboard changed during capture (attempt {}/{}), retrying",
                    attempt + 1,
                    RETRY_DELAYS_MS.len()
                );
                continue;
            }

            if let Some(content) = content {
                return Some(CaptureWorkItem {
                    content,
                    source,
                    group_id: *self.active_group_id.lock(),
                });
            }
            debug!("Clipboard read returned nothing, will retry");
        }

        warn!(
            "Clipboard capture failed after {} attempts",
            RETRY_DELAYS_MS.len()
        );
        None
    }

    fn enqueue_capture(&self, item: CaptureWorkItem) {
        // 暂停或过滤规则可能在读取大图片/等待重试期间发生变化。
        if !self.is_active()
            || self
                .clip_change_settings
                .read()
                .is_source_app_excluded(&item.source)
        {
            return;
        }
        if self.work_tx.send(item).is_err() {
            warn!("Clipboard worker channel closed, dropping event");
        }
    }
}

impl CRHandler for MonitorHandler {
    fn on_clipboard_change(&mut self) {
        if !self.is_active() {
            return;
        }
        let capture_dir = self.capture_dir.read().clone();
        if let Some(item) = self.capture_with_retry(
            clipboard_sequence_number,
            super::source_app::get_clipboard_source_app,
            |max_image_bytes| read_clipboard_content_inner(max_image_bytes, &capture_dir),
            |delay| std::thread::sleep(std::time::Duration::from_millis(delay)),
        ) {
            self.enqueue_capture(item);
        }
    }
}

fn clipboard_sequence_number() -> u32 {
    #[cfg(target_os = "windows")]
    {
        unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

fn read_clipboard_content_inner(
    max_image_bytes: usize,
    capture_dir: &std::path::Path,
) -> Option<ClipboardContent> {
    let ctx = match ClipboardContext::new() {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Failed to create clipboard context: {} (clipboard may be locked by another app)",
                e
            );
            return None;
        }
    };

    // ── 1. 文件：捕获 CF_HDROP 原始数据 + 伴生格式 ──
    if let Some(file_capture) = file_clipboard::capture_from_clipboard(&ctx) {
        debug!(
            "Got {} file(s) from clipboard (hdrop={}, extras={})",
            file_capture.paths.len(),
            file_capture.hdrop_raw.is_some(),
            file_capture.extra_formats.len()
        );
        return Some(ClipboardContent::Files(file_capture));
    }

    // ── 2. 探测所有文本格式（不短路） ──
    let html: Option<String> = ctx.get_html().ok().filter(|h| !h.is_empty());
    let rtf: Option<String> = read_rtf_from_context(&ctx);
    let text: Option<String> = ctx.get_text().ok().filter(|t| !t.is_empty());

    let has_text = html.is_some() || rtf.is_some() || text.is_some();

    // ── 3. 探测图片格式 ──
    let image_result = ctx.get_image().ok();

    // ── 4. 按语义决定类型 ──
    // Word/WPS/浏览器等复制富文本时会同时放 CF_DIB（文字位图预览），
    // 此时应优先作为富文本存储，而非图片。
    // 例外：浏览器“复制图片”只带一段单图 HTML（无任何文字），仍按图片处理。
    let image_only_html = image_result.is_some()
        && rtf.is_none()
        && text.is_none()
        && html.as_deref().is_some_and(is_image_only_html);
    if has_text && !image_only_html {
        if let Some(html) = html {
            debug!(
                "Got HTML from clipboard: {} bytes, rtf={}, text={}",
                html.len(),
                rtf.is_some(),
                text.is_some()
            );
            return Some(ClipboardContent::Html { html, text, rtf });
        }
        if let Some(rtf) = rtf {
            debug!("Got RTF from clipboard: {} bytes", rtf.len());
            return Some(ClipboardContent::Rtf { rtf, text });
        }
        if let Some(text) = text {
            debug!("Got text from clipboard: {} bytes", text.len());
            return Some(ClipboardContent::Text(text));
        }
    }

    // ── 5. 纯图片（无文本格式） ──
    if let Some(img) = image_result
        && let Some(content) = write_image_capture(img, max_image_bytes, capture_dir)
    {
        return Some(content);
    }

    if file_clipboard::clipboard_has_pending_files(&ctx) {
        debug!("Clipboard file data pending, will retry");
    }
    debug!("No recognizable content in clipboard");
    None
}

/// HTML 片段是否只含图片、没有任何可见文字（浏览器“复制图片”的典型形态）。
/// 有 StartFragment/EndFragment 标记时只看标记之间的内容。
fn is_image_only_html(html: &str) -> bool {
    const START: &str = "<!--StartFragment-->";
    const END: &str = "<!--EndFragment-->";
    let fragment = match (html.find(START), html.find(END)) {
        (Some(s), Some(e)) if s + START.len() <= e => &html[s + START.len()..e],
        _ => html,
    };

    let mut has_img = false;
    let mut rest = fragment;
    while let Some(lt) = rest.find('<') {
        if !rest[..lt].trim().is_empty() {
            return false;
        }
        let Some(gt) = rest[lt..].find('>') else {
            return false;
        };
        let tag = rest[lt + 1..lt + gt].trim_start();
        let name_len = tag
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(tag.len());
        if tag[..name_len].eq_ignore_ascii_case("img") {
            has_img = true;
        }
        rest = &rest[lt + gt + 1..];
    }
    has_img && rest.trim().is_empty()
}

/// 将剪贴板图片编码为 PNG 写入临时文件，避免大 Vec 在 channel/worker 间传递
fn write_image_capture(
    img: impl RustImage,
    max_image_bytes: usize,
    capture_dir: &std::path::Path,
) -> Option<ClipboardContent> {
    let (width, height) = img.get_size();
    debug!("Got image from clipboard: {}x{}", width, height);

    if max_image_bytes > 0 {
        let rgba_bytes = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(4);
        if rgba_bytes > max_image_bytes as u64 {
            warn!(
                "Clipboard image {}x{} (~{} bytes RGBA) exceeds max {} bytes, skipping",
                width, height, rgba_bytes, max_image_bytes
            );
            return None;
        }
    }

    let png_bytes = match img.to_png() {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Failed to convert clipboard image to PNG: {}", e);
            return None;
        }
    };
    let bytes = png_bytes.get_bytes();
    let byte_size = bytes.len();

    let temp_path = capture_dir.join(format!(
        "cap_{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if std::fs::write(&temp_path, bytes).is_err() {
        return None;
    }
    debug!(
        "Wrote capture temp PNG: {} bytes -> {:?}",
        byte_size, temp_path
    );

    Some(ClipboardContent::ImageFile(ImageCapture {
        temp_path,
        width,
        height,
        byte_size,
    }))
}

fn read_rtf_from_context(ctx: &ClipboardContext) -> Option<String> {
    let bytes = ctx.get_buffer("Rich Text Format").ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(super::rtf_storage::encode_rtf_for_storage(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::SettingsRepository;
    use std::cell::Cell;

    fn with_test_database(test: impl FnOnce(&Database, &std::path::Path)) {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "ec_capture_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        {
            let db = Database::new(root.join("clipboard.db")).unwrap();
            test(&db, &root);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    fn test_monitor() -> (MonitorHandler, mpsc::Receiver<CaptureWorkItem>) {
        let (work_tx, rx) = mpsc::channel();
        (
            MonitorHandler {
                running: Arc::new(AtomicBool::new(true)),
                pause_count: Arc::new(AtomicU32::new(0)),
                user_paused: Arc::new(AtomicBool::new(false)),
                active_group_id: Arc::new(Mutex::new(None)),
                work_tx,
                capture_dir: Arc::new(RwLock::new(PathBuf::new())),
                clip_change_settings: Arc::new(RwLock::new(ClipChangeSettings::default())),
            },
            rx,
        )
    }

    fn source(name: &str) -> Option<SourceAppInfo> {
        Some(SourceAppInfo {
            app_name: name.into(),
            exe_path: format!("{name}.exe"),
            icon_cache_key: name.into(),
        })
    }

    fn queued_history(strategy: &str, captures: &[&str]) -> Vec<String> {
        let mut history = Vec::new();
        with_test_database(|db, root| {
            SettingsRepository::new(db)
                .set("dedup_strategy", strategy)
                .unwrap();
            let handler = Arc::new(ClipboardHandler::new(db, root.join("images")));
            let (tx, rx) = mpsc::channel();
            for text in captures {
                assert!(
                    tx.send(CaptureWorkItem {
                        content: ClipboardContent::Text((*text).into()),
                        source: None,
                        group_id: None,
                    })
                    .is_ok()
                );
            }
            drop(tx);
            ClipboardMonitor::run_capture_worker(
                rx,
                Arc::new(RwLock::new(Some(handler))),
                Arc::new(RwLock::new(ClipChangeSettings::default())),
                Arc::new(AtomicBool::new(true)),
                |_| {},
            );
            let conn = db.read_connection();
            let conn = conn.lock();
            history = conn
                .prepare("SELECT text_content FROM clipboard_items ORDER BY id")
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        });
        history
    }

    #[test]
    fn queued_captures_preserve_fifo_and_handler_deduplication() {
        assert_eq!(
            queued_history("move_to_top", &["first", "first", "second", "third"]),
            ["first", "second", "third"]
        );
    }

    #[test]
    fn queued_captures_preserve_repeated_copies_with_always_new() {
        assert_eq!(
            queued_history("always_new", &["first", "first", "second"]),
            ["first", "first", "second"]
        );
    }

    #[test]
    fn retry_binds_source_and_content_to_the_same_sequence() {
        let (monitor, _) = test_monitor();
        let sequence = Cell::new(1);
        let item = monitor
            .capture_with_retry(
                || sequence.get(),
                || {
                    source(if sequence.get() == 1 {
                        "first"
                    } else {
                        "second"
                    })
                },
                |_| {
                    if sequence.replace(2) == 1 {
                        Some(ClipboardContent::Text("mixed snapshot".into()))
                    } else {
                        Some(ClipboardContent::Text("second snapshot".into()))
                    }
                },
                |_| {},
            )
            .unwrap();
        assert_eq!(item.source.unwrap().app_name, "second");
        assert!(matches!(
            &item.content,
            ClipboardContent::Text(text) if text == "second snapshot"
        ));
    }

    #[test]
    fn retry_rechecks_source_exclusion_after_clipboard_changes() {
        with_test_database(|db, root| {
            let settings = SettingsRepository::new(db);
            settings.set("app_filter_enabled", "true").unwrap();
            settings.set("app_filter_list", "private").unwrap();
            let handler = ClipboardHandler::new(db, root.join("images"));
            for changes_during_read in [false, true] {
                let (monitor, _) = test_monitor();
                *monitor.clip_change_settings.write() = handler.get_clip_change_settings();
                let sequence = Cell::new(1);
                let reads = Cell::new(0);
                let item = monitor.capture_with_retry(
                    || sequence.get(),
                    || {
                        source(if sequence.get() == 1 {
                            "public"
                        } else {
                            "private"
                        })
                    },
                    |_| {
                        reads.set(reads.get() + 1);
                        if changes_during_read {
                            sequence.set(2);
                            Some(ClipboardContent::Text("private content".into()))
                        } else {
                            None
                        }
                    },
                    |_| sequence.set(2),
                );
                assert!(item.is_none());
                assert_eq!(reads.get(), 1, "excluded content must not be read again");
            }
        });
    }

    #[test]
    fn unstable_final_attempt_is_discarded_and_capture_file_is_removed() {
        with_test_database(|_, root| {
            let (monitor, _) = test_monitor();
            let sequence = Cell::new(0);
            let temp_path = root.join("unstable.tmp");
            let item = monitor.capture_with_retry(
                || sequence.get(),
                || None,
                |_| {
                    sequence.set(sequence.get() + 1);
                    std::fs::write(&temp_path, b"captured image").unwrap();
                    Some(ClipboardContent::ImageFile(ImageCapture {
                        temp_path: temp_path.clone(),
                        ..ImageCapture::default()
                    }))
                },
                |_| {},
            );
            assert!(item.is_none());
            assert_eq!(
                sequence.get(),
                7,
                "unstable captures must have a retry limit"
            );
            assert!(!temp_path.exists());
        });
    }

    #[test]
    fn pause_during_retry_delay_prevents_another_content_read() {
        for user_pause in [false, true] {
            let (monitor, _) = test_monitor();
            let reads = Cell::new(0);
            let item = monitor.capture_with_retry(
                || 1,
                || None,
                |_| {
                    reads.set(reads.get() + 1);
                    None
                },
                |_| {
                    if user_pause {
                        monitor.user_paused.store(true, Ordering::SeqCst);
                    } else {
                        monitor.pause_count.store(1, Ordering::SeqCst);
                    }
                },
            );
            assert!(item.is_none());
            assert_eq!(reads.get(), 1);
        }
    }

    #[test]
    fn pause_during_capture_discards_the_snapshot() {
        let (monitor, _) = test_monitor();
        let item = monitor.capture_with_retry(
            || 1,
            || None,
            |_| {
                monitor.pause_count.store(1, Ordering::SeqCst);
                Some(ClipboardContent::Text("internal clipboard write".into()))
            },
            |_| {},
        );
        assert!(item.is_none());
    }

    #[test]
    fn enqueue_rechecks_pause_and_running_state() {
        for state in 0..3 {
            let (monitor, rx) = test_monitor();
            let item = monitor
                .capture_with_retry(
                    || 1,
                    || None,
                    |_| Some(ClipboardContent::Text("captured".into())),
                    |_| {},
                )
                .unwrap();
            match state {
                0 => monitor.pause_count.store(1, Ordering::SeqCst),
                1 => monitor.user_paused.store(true, Ordering::SeqCst),
                _ => monitor.running.store(false, Ordering::SeqCst),
            }
            monitor.enqueue_capture(item);
            assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        }
    }

    #[test]
    fn enqueue_rechecks_changed_source_filter() {
        with_test_database(|db, root| {
            let (monitor, rx) = test_monitor();
            let item = monitor
                .capture_with_retry(
                    || 1,
                    || source("private"),
                    |_| Some(ClipboardContent::Text("private content".into())),
                    |_| {},
                )
                .unwrap();
            let settings = SettingsRepository::new(db);
            settings.set("app_filter_enabled", "true").unwrap();
            settings.set("app_filter_list", "private").unwrap();
            *monitor.clip_change_settings.write() =
                ClipboardHandler::new(db, root.join("images")).get_clip_change_settings();
            monitor.enqueue_capture(item);
            assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        });
    }

    #[test]
    fn single_image_fragment() {
        let html = r#"<html><body><!--StartFragment--><img src="https://example.invalid/a.png" alt="image"><!--EndFragment--></body></html>"#;
        assert!(is_image_only_html(html));
    }

    #[test]
    fn image_without_fragment_markers() {
        assert!(is_image_only_html(
            r#"<html><body><img src="a.png"/></body></html>"#
        ));
        assert!(is_image_only_html(r#"<IMG SRC="a.png">"#));
    }

    #[test]
    fn image_wrapped_in_link() {
        let html = r#"<!--StartFragment--><a href="x"><img src="a.png"></a><!--EndFragment-->"#;
        assert!(is_image_only_html(html));
    }

    #[test]
    fn image_with_text_is_rich_text() {
        let html = r#"<html><body><!--StartFragment--><p>说明</p><img src="a.png"><!--EndFragment--></body></html>"#;
        assert!(!is_image_only_html(html));
        let trailing = r#"<!--StartFragment--><img src="a.png"> caption<!--EndFragment-->"#;
        assert!(!is_image_only_html(trailing));
    }

    #[test]
    fn text_only_or_empty_is_not_image() {
        assert!(!is_image_only_html(
            "<!--StartFragment--><b>hi</b><!--EndFragment-->"
        ));
        assert!(!is_image_only_html(
            "<!--StartFragment--><!--EndFragment-->"
        ));
        assert!(!is_image_only_html(""));
    }

    #[test]
    fn tag_names_starting_with_img_are_not_images() {
        assert!(!is_image_only_html(
            "<!--StartFragment--><imgx src=a><!--EndFragment-->"
        ));
    }

    #[test]
    fn unclosed_tag_is_not_image() {
        assert!(!is_image_only_html(r#"<img src="a.png""#));
    }
}
