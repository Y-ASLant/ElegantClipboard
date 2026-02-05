use super::{ClipboardContent, ClipboardHandler};
use crate::database::Database;
use clipboard_master::{CallbackResult, ClipboardHandler as CMHandler, Master};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tauri::{AppHandle, Emitter};
use tracing::{debug, error, info, warn};

/// Clipboard monitor service
#[derive(Clone)]
pub struct ClipboardMonitor {
    running: Arc<AtomicBool>,
    /// Pause counter: when > 0, clipboard changes are ignored
    /// This prevents race conditions when multiple copy operations overlap
    pause_count: Arc<AtomicU32>,
    handler: Arc<Mutex<Option<ClipboardHandler>>>,
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            pause_count: Arc::new(AtomicU32::new(0)),
            handler: Arc::new(Mutex::new(None)),
            thread_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the monitor with database and images path
    pub fn init(&self, db: &Database, images_path: std::path::PathBuf) {
        let handler = ClipboardHandler::new(db, images_path);
        *self.handler.lock() = Some(handler);
        info!("Clipboard monitor initialized");
    }

    /// Start monitoring clipboard changes
    pub fn start(&self, app_handle: AppHandle) {
        // Use compare_exchange to avoid race condition
        if self.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            warn!("Clipboard monitor already running");
            return;
        }

        let running = self.running.clone();
        let pause_count = self.pause_count.clone();
        let handler = self.handler.clone();

        let handle = std::thread::spawn(move || {
            info!("Clipboard monitor thread started");

            let clipboard_handler = MonitorHandler {
                running: running.clone(),
                pause_count,
                handler,
                app_handle,
            };

            // Start the clipboard master
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

        // Store thread handle for cleanup
        *self.thread_handle.lock() = Some(handle);
    }

    /// Stop monitoring and wait for thread to finish
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("Clipboard monitor stopping");
        
        // Wait for thread to finish (with timeout)
        if let Some(handle) = self.thread_handle.lock().take() {
            // Don't block indefinitely - the thread should stop on its own
            // when running flag is set to false
            let _ = handle.join();
        }
    }

    /// Pause monitoring (increments pause counter)
    /// Multiple concurrent pauses are supported - monitoring resumes only when all are released
    pub fn pause(&self) {
        let count = self.pause_count.fetch_add(1, Ordering::SeqCst);
        debug!("Clipboard monitor paused (count: {})", count + 1);
    }

    /// Resume monitoring (decrements pause counter)
    /// Monitoring only actually resumes when counter reaches 0
    pub fn resume(&self) {
        let prev = self.pause_count.fetch_sub(1, Ordering::SeqCst);
        if prev == 0 {
            // Counter was already 0, restore it to avoid underflow
            self.pause_count.store(0, Ordering::SeqCst);
            warn!("Resume called when not paused");
        } else {
            debug!("Clipboard monitor resume (count: {})", prev - 1);
        }
    }

    /// Check if paused (pause count > 0)
    pub fn is_paused(&self) -> bool {
        self.pause_count.load(Ordering::SeqCst) > 0
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for clipboard-master
struct MonitorHandler {
    running: Arc<AtomicBool>,
    pause_count: Arc<AtomicU32>,
    handler: Arc<Mutex<Option<ClipboardHandler>>>,
    app_handle: AppHandle,
}

impl CMHandler for MonitorHandler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        // Check if we should stop
        if !self.running.load(Ordering::SeqCst) {
            return CallbackResult::Stop;
        }

        // Check if paused (pause_count > 0)
        if self.pause_count.load(Ordering::SeqCst) > 0 {
            debug!("Clipboard change ignored (paused)");
            return CallbackResult::Next;
        }

        // Read clipboard content using arboard
        let content = match read_clipboard_content() {
            Some(c) => c,
            None => return CallbackResult::Next,
        };

        // Process the content
        if let Some(ref handler) = *self.handler.lock() {
            match handler.process(content) {
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

/// Read current clipboard content using clipboard-rs (better Windows compatibility)
fn read_clipboard_content() -> Option<ClipboardContent> {
    use clipboard_rs::{Clipboard, ClipboardContext};
    use clipboard_rs::common::RustImage;
    
    let ctx = match ClipboardContext::new() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create clipboard context: {}", e);
            return None;
        }
    };

    // Try to get files first (file paths from explorer copy)
    if let Ok(files) = ctx.get_files() {
        if !files.is_empty() {
            debug!("Got {} files from clipboard", files.len());
            return Some(ClipboardContent::Files(files));
        }
    }

    // Try to get image using clipboard-rs
    if let Ok(img) = ctx.get_image() {
        let (width, height) = img.get_size();
        debug!("Got image from clipboard: {}x{}", width, height);
        
        // Get PNG bytes directly from clipboard-rs
        if let Ok(png_buffer) = img.to_png() {
            let bytes: Vec<u8> = png_buffer.get_bytes().to_vec();
            debug!("Got PNG image: {} bytes", bytes.len());
            return Some(ClipboardContent::Image(bytes));
        }
        
        warn!("Failed to convert image to PNG");
    }

    // Try to get text using arboard (more reliable for text)
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(text) = clipboard.get_text() {
            if !text.is_empty() {
                return Some(ClipboardContent::Text(text));
            }
        }
    }

    None
}
