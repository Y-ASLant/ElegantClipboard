use super::{ClipboardContent, ClipboardHandler};
use crate::database::Database;
use clipboard_rs::{Clipboard as ClipboardTrait, ClipboardContext, ClipboardHandler as CRHandler, ClipboardWatcher, ClipboardWatcherContext};
use clipboard_rs::common::RustImage;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    handler: Arc<Mutex<Option<ClipboardHandler>>>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// 当前活动分组（None = 默认分组），与 AppState 共享
    active_group_id: Arc<Mutex<Option<i64>>>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            pause_count: Arc::new(AtomicU32::new(0)),
            user_paused: Arc::new(AtomicBool::new(false)),
            handler: Arc::new(Mutex::new(None)),
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
        let handler = ClipboardHandler::new(db, images_path);
        *self.handler.lock() = Some(handler);
        info!("Clipboard monitor initialized");
    }

    /// 启动剪贴板监听（带自动重启）
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
                    handler: handler.clone(),
                    app_handle: app_handle.clone(),
                    active_group_id: active_group_id.clone(),
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
                let backoff =
                    (100 * 2u64.pow(consecutive_failures.min(6))).min(MAX_BACKOFF_MS);
                warn!(
                    "Clipboard watcher exited, restarting in {}ms (failure #{})",
                    backoff, consecutive_failures
                );
                std::thread::sleep(std::time::Duration::from_millis(backoff));

                // 成功运行一段时间后重置失败计数（watcher 至少活了 30 秒算稳定）
                // 这里简化：每次重启都递增 backoff，不会重置
            }

            running.store(false, Ordering::SeqCst);
            info!("Clipboard monitor thread stopped");
        });

        // 保存线程句柄以便清理
        *self.thread_handle.lock() = Some(handle);
    }

    /// 暂停监控（递增暂停计数，支持多个并发暂停）
    pub fn pause(&self) {
        let count = self.pause_count.fetch_add(1, Ordering::SeqCst);
        debug!("Clipboard monitor paused (count: {})", count + 1);
    }

    /// 恢复监控（递减暂停计数，归零时真正恢复）
    pub fn resume(&self) {
        // 原子递减，仅当 > 0 时执行，避免 u32 下溢
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

    /// 是否已暂停（计数 > 0）
    pub fn is_paused(&self) -> bool {
        self.pause_count.load(Ordering::SeqCst) > 0
    }

    /// 是否运行中
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 用户手动切换暂停状态，返回切换后的暂停状态
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

/// clipboard-rs 事件处理器
struct MonitorHandler {
    running: Arc<AtomicBool>,
    pause_count: Arc<AtomicU32>,
    user_paused: Arc<AtomicBool>,
    handler: Arc<Mutex<Option<ClipboardHandler>>>,
    app_handle: AppHandle,
    active_group_id: Arc<Mutex<Option<i64>>>,
}

impl CRHandler for MonitorHandler {
    fn on_clipboard_change(&mut self) {
        // 检查是否应停止
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        // 检查是否已暂停（内部计数或用户手动）
        if self.pause_count.load(Ordering::SeqCst) > 0
            || self.user_paused.load(Ordering::SeqCst)
        {
            debug!("Clipboard change ignored (paused)");
            return;
        }

        // 先获取来源应用（在读取内容之前）
        let source = super::source_app::get_clipboard_source_app();

        // 单次批量查询获取热路径所需设置（替代原先 4 次独立 DB 查询）
        let max_image_bytes = {
            let guard = self.handler.lock();
            if let Some(handler) = &*guard {
                let settings = handler.get_clip_change_settings();
                if handler.is_source_app_excluded(&source, &settings) {
                    debug!(
                        "Clipboard change ignored (source app excluded: {:?})",
                        source.as_ref().map(|s| &s.app_name)
                    );
                    return;
                }
                settings.max_image_bytes
            } else {
                0
            }
        };

        // 读取剪贴板内容（带重试，应对剪贴板锁竞争）
        let Some(content) = read_clipboard_content_with_retry(max_image_bytes) else {
            return;
        };

        // 读取当前活动分组
        let group_id = *self.active_group_id.lock();

        // 检查内容类型 + 处理内容（单次加锁）
        if let Some(handler) = &*self.handler.lock() {
            if !handler.is_content_type_allowed(&content) {
                debug!("Clipboard change ignored (content type not allowed)");
                return;
            }
            match handler.process(content, source, group_id) {
                Ok(Some(id)) => {
                    debug!("Processed clipboard item: {}", id);
                    let _ = self.app_handle.emit("clipboard-updated", id);
                }
                Ok(None) => {
                    debug!("Clipboard content already exists");
                }
                Err(e) => {
                    error!("Failed to process clipboard: {}", e);
                }
            }
        }
    }
}

/// 带重试的剪贴板读取，应对剪贴板锁竞争（如截图工具延迟渲染）
/// `max_image_bytes` 为 0 时不限制；非零时先按原始像素尺寸预判，避免对超大图进行 PNG 编码
fn read_clipboard_content_with_retry(max_image_bytes: usize) -> Option<ClipboardContent> {
    // 渐进退避：[0, 40, 80, 140, 220, 360, 560]ms，总计最多 1400ms
    const RETRY_DELAYS_MS: [u64; 7] = [0, 40, 80, 140, 220, 360, 560];

    for (attempt, &delay) in RETRY_DELAYS_MS.iter().enumerate() {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            debug!("Clipboard read retry {}/{}", attempt + 1, RETRY_DELAYS_MS.len());
        }

        match read_clipboard_content(max_image_bytes) {
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

/// 读取当前剪贴板内容（单次尝试，检测 TOCTOU 并重试）
fn read_clipboard_content(max_image_bytes: usize) -> Option<ClipboardContent> {
    const MAX_RETRIES: u32 = 2;

    for attempt in 0..=MAX_RETRIES {
        #[cfg(target_os = "windows")]
        let seq_before =
            unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() };

        let result = read_clipboard_content_inner(max_image_bytes);

        // 检测剪贴板是否在读取过程中被修改（TOCTOU）
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

/// 实际读取剪贴板内容（内部函数）
/// 使用 clipboard-rs 的 ClipboardContext，支持格式探测和 RTF 原生读取
fn read_clipboard_content_inner(max_image_bytes: usize) -> Option<ClipboardContent> {
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

    // 优先尝试获取文件
    match ctx.get_files() {
        Ok(files) if !files.is_empty() => {
            debug!("Got {} files from clipboard", files.len());
            return Some(ClipboardContent::Files(files));
        }
        Ok(_) => {} // 空文件列表，继续尝试其他格式
        Err(e) => debug!("Clipboard get_files failed: {}", e),
    }

    // 尝试获取图片（clipboard-rs 返回 RustImageData，可直接转 PNG）
    match ctx.get_image() {
        Ok(img) => {
            let (width, height) = img.get_size();
            debug!("Got image from clipboard: {}x{}", width, height);

            // 在 PNG 编码前按 RGBA 字节数预判，超限则跳过
            if max_image_bytes > 0 {
                let rgba_bytes = (width as u64).saturating_mul(height as u64).saturating_mul(4);
                if rgba_bytes > max_image_bytes as u64 {
                    warn!(
                        "Clipboard image {}x{} (~{} bytes RGBA) exceeds max {} bytes, skipping",
                        width, height, rgba_bytes, max_image_bytes
                    );
                    return None;
                }
            }

            // clipboard-rs 内置 PNG 转换
            match img.to_png() {
                Ok(png_bytes) => {
                    debug!("Got PNG image: {} bytes", png_bytes.get_bytes().len());
                    return Some(ClipboardContent::Image(png_bytes.get_bytes().to_vec()));
                }
                Err(e) => warn!("Failed to convert clipboard image to PNG: {}", e),
            }
        }
        Err(e) => debug!(
            "Clipboard get_image failed: {} (may not contain image data or format unsupported)",
            e
        ),
    }

    // 尝试获取 HTML（同时尝试读取伴生 RTF，便于完整回写）
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

    // 尝试获取 RTF 富文本（clipboard-rs 原生支持通过 get_buffer）
    if let Some(rtf) = read_rtf_from_context(&ctx) {
        let text = ctx.get_text().ok().filter(|t| !t.is_empty());
        debug!("Got RTF from clipboard: {} bytes", rtf.len());
        return Some(ClipboardContent::Rtf { rtf, text });
    }

    // 尝试获取纯文本
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

/// 通过 clipboard-rs 读取 RTF 格式内容
fn read_rtf_from_context(ctx: &ClipboardContext) -> Option<String> {
    let bytes = ctx.get_buffer("Rich Text Format").ok()?;
    if bytes.is_empty() {
        return None;
    }
    // 去掉尾部 null 终止符
    let trimmed = if bytes.last() == Some(&0) {
        &bytes[..bytes.len() - 1]
    } else {
        &bytes
    };
    Some(String::from_utf8_lossy(trimmed).into_owned())
}
