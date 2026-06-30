use super::{ClipboardContent, ClipboardHandler};
use crate::database::Database;
use clipboard_master::{CallbackResult, ClipboardHandler as CMHandler, Master};
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

    /// 启动剪贴板监听
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

            let clipboard_handler = MonitorHandler {
                running: running.clone(),
                pause_count,
                user_paused,
                handler,
                app_handle,
                active_group_id,
            };

            // 启动剪贴板监听
            match Master::new(clipboard_handler) {
                Ok(mut master) => {
                    if let Err(e) = master.run() {
                        error!("Clipboard monitor error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to create clipboard master: {}", e);
                }
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

/// clipboard-master 事件处理器
struct MonitorHandler {
    running: Arc<AtomicBool>,
    pause_count: Arc<AtomicU32>,
    user_paused: Arc<AtomicBool>,
    handler: Arc<Mutex<Option<ClipboardHandler>>>,
    app_handle: AppHandle,
    active_group_id: Arc<Mutex<Option<i64>>>,
}

impl CMHandler for MonitorHandler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        // 检查是否应停止
        if !self.running.load(Ordering::SeqCst) {
            return CallbackResult::Stop;
        }

        // 检查是否已暂停（内部计数或用户手动）
        if self.pause_count.load(Ordering::SeqCst) > 0 || self.user_paused.load(Ordering::SeqCst) {
            debug!("Clipboard change ignored (paused)");
            return CallbackResult::Next;
        }

        // 先获取来源应用（在读取内容之前）
        let source = super::source_app::get_clipboard_source_app();

        // 同一次加锁内完成排除判断与限制读取，避免多次 lock
        let max_image_bytes = {
            let guard = self.handler.lock();
            if let Some(ref handler) = *guard {
                if handler.is_source_app_excluded(&source) {
                    debug!(
                        "Clipboard change ignored (source app excluded: {:?})",
                        source.as_ref().map(|s| &s.app_name)
                    );
                    return CallbackResult::Next;
                }
                handler.get_max_image_size()
            } else {
                0
            }
        };

        // 读取剪贴板内容（带重试，应对剪贴板锁竞争）
        let Some(content) = read_clipboard_content_with_retry(max_image_bytes) else {
            return CallbackResult::Next;
        };

        // 读取当前活动分组
        let group_id = *self.active_group_id.lock();

        // 检查内容类型 + 处理内容（单次加锁）
        if let Some(ref handler) = *self.handler.lock() {
            if !handler.is_content_type_allowed(&content) {
                debug!("Clipboard change ignored (content type not allowed)");
                return CallbackResult::Next;
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

        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        error!("Clipboard error: {}", error);
        CallbackResult::Next
    }
}

/// 带重试的剪贴板读取，应对剪贴板锁竞争（如截图工具延迟渲染）
/// `max_image_bytes` 为 0 时不限制；非零时先按原始像素尺寸预判，避免对超大图进行 PNG 编码
fn read_clipboard_content_with_retry(max_image_bytes: usize) -> Option<ClipboardContent> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 50;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(
                RETRY_DELAY_MS * u64::from(attempt),
            ));
            debug!("Clipboard read retry {}/{}", attempt + 1, MAX_RETRIES);
        }

        match read_clipboard_content(max_image_bytes) {
            Some(content) => return Some(content),
            None if attempt + 1 < MAX_RETRIES => {
                debug!("Clipboard read returned nothing, will retry");
                continue;
            }
            None => {
                warn!("Clipboard read failed after {} attempts", MAX_RETRIES);
                return None;
            }
        }
    }
    None
}

/// 读取当前剪贴板内容（单次尝试）
fn read_clipboard_content(max_image_bytes: usize) -> Option<ClipboardContent> {
    let mut clipboard = match arboard::Clipboard::new() {
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
    match clipboard.get().file_list() {
        Ok(files) if !files.is_empty() => {
            let file_strings: Vec<String> = files
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            debug!("Got {} files from clipboard", file_strings.len());
            return Some(ClipboardContent::Files(file_strings));
        }
        Ok(_) => {} // 空文件列表，继续尝试其他格式
        Err(e) => debug!("Clipboard get_files failed: {}", e),
    }

    // 尝试获取图片
    match clipboard.get_image() {
        Ok(img) => {
            let width = img.width as u32;
            let height = img.height as u32;
            debug!("Got image from clipboard: {}x{}", width, height);

            // 在 PNG 编码前估算最终字节大小，超限则跳过
            if max_image_bytes > 0 {
                let estimated_png_bytes = u64::from(width).saturating_mul(u64::from(height));
                if estimated_png_bytes > max_image_bytes as u64 {
                    warn!(
                        "Clipboard image {}x{} (~{} bytes estimated PNG) exceeds max {} bytes, skipping",
                        width, height, estimated_png_bytes, max_image_bytes
                    );
                    return None;
                }
            }

            // arboard 返回 RGBA 像素，用 image crate 编码为 PNG
            match encode_rgba_to_png(&img.bytes, width, height) {
                Ok(png_bytes) => {
                    debug!("Got PNG image: {} bytes", png_bytes.len());
                    return Some(ClipboardContent::Image(png_bytes));
                }
                Err(e) => warn!("Failed to convert clipboard image to PNG: {}", e),
            }
        }
        Err(e) => debug!(
            "Clipboard get_image failed: {} (may not contain image data or format unsupported)",
            e
        ),
    }

    // 尝试获取 HTML
    match clipboard.get().html() {
        Ok(html) if !html.is_empty() => {
            let text = clipboard.get_text().ok().filter(|t| !t.is_empty());
            debug!("Got HTML from clipboard: {} bytes", html.len());
            return Some(ClipboardContent::Html { html, text });
        }
        Ok(_) => {}
        Err(e) => debug!("Clipboard get_html failed: {}", e),
    }

    // 尝试获取 RTF 富文本（arboard 不支持，用 Windows API）
    #[cfg(target_os = "windows")]
    if let Some(rtf) = read_rtf_from_clipboard() {
        if !rtf.is_empty() {
            let text = clipboard.get_text().ok().filter(|t| !t.is_empty());
            debug!("Got RTF from clipboard: {} bytes", rtf.len());
            return Some(ClipboardContent::Rtf { rtf, text });
        }
    }

    // 尝试获取纯文本
    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => {
            return Some(ClipboardContent::Text(text));
        }
        Ok(_) => debug!("Clipboard text is empty"),
        Err(e) => debug!("Clipboard get_text failed: {}", e),
    }

    debug!("No recognizable content in clipboard");
    None
}

/// 将 RGBA 像素数据编码为 PNG 字节
fn encode_rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Rgba};

    let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba.to_vec())
        .ok_or("Failed to create image buffer from RGBA data")?;

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;

    Ok(buf.into_inner())
}

/// 使用 Windows API 从剪贴板读取 RTF 格式
#[cfg(target_os = "windows")]
fn read_rtf_from_clipboard() -> Option<String> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatA,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::core::PCSTR;

    unsafe {
        // CF_RTF 是注册的剪贴板格式，需要先获取其 ID
        let cf_rtf = RegisterClipboardFormatA(PCSTR(b"Rich Text Format\0".as_ptr()));
        if cf_rtf == 0 {
            return None;
        }

        // 必须先打开剪贴板，否则 GetClipboardData 会失败
        if OpenClipboard(None).is_err() {
            return None;
        }

        let result = (|| -> Option<String> {
            let handle = GetClipboardData(cf_rtf).ok()?;
            let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
            if hglobal.is_invalid() {
                return None;
            }

            let ptr = GlobalLock(hglobal) as *const u8;
            if ptr.is_null() {
                return None;
            }

            // 读取 null 结尾的 C 字符串
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
                if len > 10_000_000 {
                    GlobalUnlock(hglobal).ok();
                    return None;
                }
            }

            let slice = std::slice::from_raw_parts(ptr, len);
            let rtf = String::from_utf8_lossy(slice).to_string();

            GlobalUnlock(hglobal).ok();

            Some(rtf)
        })();

        CloseClipboard().ok();

        result
    }
}
