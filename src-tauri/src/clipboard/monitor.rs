use super::handler::{cleanup_capture_content, cleanup_stale_capture_files};
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
        let worker_running = running.clone();
        let worker_app = app_handle.clone();
        std::thread::Builder::new()
            .name("clipboard-worker".into())
            .spawn(move || {
                Self::run_capture_worker(rx, worker_handler, worker_running, worker_app);
            })
            .expect("failed to spawn clipboard-worker thread");

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
                    app_handle: app_handle.clone(),
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

    /// 处理 worker 主循环：串行处理剪贴板内容，快速合并连续事件
    fn run_capture_worker(
        rx: mpsc::Receiver<CaptureWorkItem>,
        handler: Arc<RwLock<Option<Arc<ClipboardHandler>>>>,
        running: Arc<AtomicBool>,
        app_handle: AppHandle,
    ) {
        info!("Clipboard worker thread started");

        while running.load(Ordering::SeqCst) {
            let Ok(mut item) = rx.recv() else {
                break;
            };

            while let Ok(newer) = rx.try_recv() {
                cleanup_capture_content(&item.content);
                item = newer;
            }

            let handler = handler.read().clone();
            let Some(h) = handler else {
                cleanup_capture_content(&item.content);
                continue;
            };

            if !h.is_content_type_allowed(&item.content) {
                cleanup_capture_content(&item.content);
                debug!("Clipboard change ignored (content type not allowed)");
                continue;
            }

            // catch_unwind 防止单条异常数据的 panic 杀死整个进程（panic=abort）
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                h.process(item.content, item.source, item.group_id)
            }));

            match result {
                Ok(Ok(Some(id))) => {
                    debug!("Processed clipboard item: {}", id);
                    let _ = app_handle.emit("clipboard-updated", id);
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
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
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
    #[allow(dead_code)]
    app_handle: AppHandle,
    active_group_id: Arc<Mutex<Option<i64>>>,
    work_tx: mpsc::Sender<CaptureWorkItem>,
    capture_dir: Arc<RwLock<PathBuf>>,
    clip_change_settings: Arc<RwLock<ClipChangeSettings>>,
}

impl CRHandler for MonitorHandler {
    fn on_clipboard_change(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        if self.pause_count.load(Ordering::SeqCst) > 0 || self.user_paused.load(Ordering::SeqCst) {
            debug!("Clipboard change ignored (paused)");
            return;
        }

        let source = super::source_app::get_clipboard_source_app();

        let settings = self.clip_change_settings.read().clone();
        if settings.is_source_app_excluded(&source) {
            debug!(
                "Clipboard change ignored (source app excluded: {:?})",
                source.as_ref().map(|s| &s.app_name)
            );
            return;
        }
        let max_image_bytes = settings.max_image_bytes;
        let capture_dir = self.capture_dir.read().clone();

        let Some(content) = read_clipboard_content_with_retry(max_image_bytes, &capture_dir) else {
            return;
        };

        let group_id = *self.active_group_id.lock();

        let item = CaptureWorkItem {
            content,
            source,
            group_id,
        };
        if self.work_tx.send(item).is_err() {
            warn!("Clipboard worker channel closed, dropping event");
        }
    }
}

fn read_clipboard_content_with_retry(
    max_image_bytes: usize,
    capture_dir: &std::path::Path,
) -> Option<ClipboardContent> {
    const RETRY_DELAYS_MS: [u64; 7] = [0, 40, 80, 140, 220, 360, 560];

    for (attempt, &delay) in RETRY_DELAYS_MS.iter().enumerate() {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            debug!(
                "Clipboard read retry {}/{}",
                attempt + 1,
                RETRY_DELAYS_MS.len()
            );
        }

        match read_clipboard_content(max_image_bytes, capture_dir) {
            Some(content) => return Some(content),
            None if attempt + 1 < RETRY_DELAYS_MS.len() => {
                debug!("Clipboard read returned nothing, will retry");
                continue;
            }
            None => {
                warn!(
                    "Clipboard read failed after {} attempts",
                    RETRY_DELAYS_MS.len()
                );
                return None;
            }
        }
    }
    None
}

fn read_clipboard_content(
    max_image_bytes: usize,
    capture_dir: &std::path::Path,
) -> Option<ClipboardContent> {
    const MAX_RETRIES: u32 = 2;

    for attempt in 0..=MAX_RETRIES {
        #[cfg(target_os = "windows")]
        let seq_before =
            unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };

        let result = read_clipboard_content_inner(max_image_bytes, capture_dir);

        #[cfg(target_os = "windows")]
        {
            let seq_after =
                unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };
            if seq_before != seq_after && attempt < MAX_RETRIES {
                debug!(
                    "Clipboard changed during read (attempt {}/{}), retrying",
                    attempt + 1,
                    MAX_RETRIES + 1
                );
                continue;
            }
        }

        return result;
    }
    None
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

    match ctx.get_files() {
        Ok(files) if !files.is_empty() => {
            debug!("Got {} files from clipboard", files.len());
            return Some(ClipboardContent::Files(files));
        }
        Ok(_) => {}
        Err(e) => debug!("Clipboard get_files failed: {}", e),
    }

    match ctx.get_image() {
        Ok(img) => {
            if let Some(content) = write_image_capture(img, max_image_bytes, capture_dir) {
                return Some(content);
            }
        }
        Err(e) => debug!(
            "Clipboard get_image failed: {} (may not contain image data or format unsupported)",
            e
        ),
    }

    match ctx.get_html() {
        Ok(html) if !html.is_empty() => {
            let text = ctx.get_text().ok().filter(|t| !t.is_empty());
            let rtf = read_rtf_from_context(&ctx);
            debug!(
                "Got HTML from clipboard: {} bytes, rtf={}",
                html.len(),
                rtf.is_some()
            );
            return Some(ClipboardContent::Html { html, text, rtf });
        }
        Ok(_) => {}
        Err(e) => debug!("Clipboard get_html failed: {}", e),
    }

    if let Some(rtf) = read_rtf_from_context(&ctx) {
        let text = ctx.get_text().ok().filter(|t| !t.is_empty());
        debug!("Got RTF from clipboard: {} bytes", rtf.len());
        return Some(ClipboardContent::Rtf { rtf, text });
    }

    match ctx.get_text() {
        Ok(text) if !text.is_empty() => {
            return Some(ClipboardContent::Text(text));
        }
        Ok(_) => debug!("Clipboard text is empty"),
        Err(e) => debug!("Clipboard get_text failed: {}", e),
    }

    debug!("No recognizable content in clipboard");
    None
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
