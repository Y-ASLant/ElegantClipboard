//! 快捷键窗口状态管理（切换显示/隐藏）
//!
//! 实际按键处理由 Tauri global_shortcut 插件完成。

use parking_lot::RwLock;
use std::sync::LazyLock;

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

/// 设置窗口状态
pub fn set_window_state(state: WindowState) {
    *WINDOW_STATE.write() = state;
}
