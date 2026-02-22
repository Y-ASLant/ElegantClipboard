//! 快捷键窗口状态管理（切换显示/隐藏）
//!
//! 实际按键处理由 Tauri global_shortcut 插件完成。

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

// 窗口状态枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Hidden,
    Visible,
}

static WINDOW_STATE: LazyLock<RwLock<WindowState>> =
    LazyLock::new(|| RwLock::new(WindowState::Hidden));

/// 获取当前窗口状态
pub fn get_window_state() -> WindowState {
    *WINDOW_STATE.read()
}

/// 窗口上次隐藏的时间戳（毫秒），用于托盘点击防抖
static LAST_HIDDEN_AT: AtomicU64 = AtomicU64::new(0);

/// 设置窗口状态
pub fn set_window_state(state: WindowState) {
    if state == WindowState::Hidden {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        LAST_HIDDEN_AT.store(now, Ordering::Relaxed);
    }
    *WINDOW_STATE.write() = state;
}

/// 窗口是否在指定毫秒内刚被隐藏
pub fn was_recently_hidden(ms: u64) -> bool {
    let last = LAST_HIDDEN_AT.load(Ordering::Relaxed);
    if last == 0 { return false; }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now.saturating_sub(last) < ms
}
