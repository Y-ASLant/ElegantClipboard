//! Tauri / WebView2 窗口操作必须在主线程执行。
//! 全局快捷键、低级钩子、托盘 WndProc、粘贴工作线程等都会在非主线程触发 UI 逻辑。

use std::sync::atomic::{AtomicU32, Ordering};

static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// 在 `setup` 主线程回调开头调用一次。
pub fn init() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        MAIN_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    }
}

pub fn is_main_thread() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        let main = MAIN_THREAD_ID.load(Ordering::SeqCst);
        main != 0 && main == unsafe { GetCurrentThreadId() }
    }
}

/// 若已在主线程则同步执行，否则阻塞等待主线程完成（粘贴等流程依赖顺序）。
pub fn run_on_ui_thread<R: tauri::Runtime, T: Send + 'static>(
    app: &tauri::AppHandle<R>,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    if is_main_thread() {
        Ok(f())
    } else {
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_thread_id_uninitialized_is_not_main() {
        // 单测进程未调用 init() 时，不应误判为已在主线程。
        assert!(!is_main_thread());
    }
}
