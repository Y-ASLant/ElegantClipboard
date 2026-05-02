//! 热键注册抽象层
//!
//! 支持两种模式：
//! - `Register`（默认）: 使用 `tauri-plugin-global-shortcut`（底层 RegisterHotKey API）。
//!   无法穿透全屏应用，因此不需要游戏模式。
//! - `LowLevel`: 使用 `WH_KEYBOARD_LL` 低级键盘钩子。
//!   可穿透全屏应用，需要游戏模式来抑制快捷键。

use std::sync::Arc;

pub use crate::low_level_shortcut::KeyState;
pub type ShortcutCallback = Arc<dyn Fn(&tauri::AppHandle, KeyState) + Send + Sync>;

/// 热键注册模式
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HotkeyMode {
    /// RegisterHotKey API（默认，不穿透全屏）
    Register,
    /// WH_KEYBOARD_LL 低级键盘钩子（穿透全屏）
    LowLevel,
}

impl HotkeyMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "hook" | "low_level" => HotkeyMode::LowLevel,
            _ => HotkeyMode::Register,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyMode::Register => "register",
            HotkeyMode::LowLevel => "hook",
        }
    }
}

static MODE: parking_lot::RwLock<HotkeyMode> = parking_lot::RwLock::new(HotkeyMode::Register);
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

// ── 公开 API ──────────────────────────────────────────────────────────

/// 初始化热键系统
pub fn start(app: tauri::AppHandle, mode: HotkeyMode) {
    let _ = APP_HANDLE.set(app.clone());
    *MODE.write() = mode;
    if mode == HotkeyMode::LowLevel {
        crate::low_level_shortcut::start(app);
    }
    tracing::info!("热键模式: {:?}", mode);
}

/// 注册快捷键
pub fn register(shortcut_str: &str, callback: ShortcutCallback) -> bool {
    match *MODE.read() {
        HotkeyMode::Register => register_via_plugin(shortcut_str, callback),
        HotkeyMode::LowLevel => crate::low_level_shortcut::register(shortcut_str, callback),
    }
}

/// 注销快捷键
pub fn unregister(shortcut_str: &str) {
    match *MODE.read() {
        HotkeyMode::Register => unregister_via_plugin(shortcut_str),
        HotkeyMode::LowLevel => crate::low_level_shortcut::unregister(shortcut_str),
    }
}

/// 临时禁用所有快捷键（游戏模式用，仅 LowLevel 模式有效）
pub fn disable_all() {
    if *MODE.read() == HotkeyMode::LowLevel {
        crate::low_level_shortcut::disable_all();
    }
}

/// 重新启用所有快捷键
pub fn enable_all() {
    if *MODE.read() == HotkeyMode::LowLevel {
        crate::low_level_shortcut::enable_all();
    }
}

/// 获取当前模式
pub fn get_mode() -> HotkeyMode {
    *MODE.read()
}

/// 切换模式（由 lib.rs 在注销所有快捷键后调用）
pub fn switch_mode(app: &tauri::AppHandle, new_mode: HotkeyMode) {
    let old_mode = *MODE.read();
    if old_mode == new_mode {
        return;
    }

    // 停止旧系统
    if old_mode == HotkeyMode::LowLevel {
        crate::low_level_shortcut::stop();
    } else {
        // 清除 global-shortcut 插件注册的所有快捷键
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let _ = app.global_shortcut().unregister_all();
    }

    // 启动新系统
    *MODE.write() = new_mode;
    if new_mode == HotkeyMode::LowLevel {
        crate::low_level_shortcut::start(app.clone());
    }
    tracing::info!("热键模式已切换: {:?} -> {:?}", old_mode, new_mode);
}

// ── RegisterHotKey 模式实现 ─────────────────────────────────────────

fn register_via_plugin(shortcut_str: &str, callback: ShortcutCallback) -> bool {
    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => return false,
    };
    let parsed = match crate::shortcut::parse_shortcut(shortcut_str) {
        Some(s) => s,
        None => {
            tracing::warn!("热键: 无法解析快捷键 '{}'", shortcut_str);
            return false;
        }
    };
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let cb = callback.clone();
    match app.global_shortcut().on_shortcut(parsed, move |app, _sc, event| {
        let state = match event.state {
            tauri_plugin_global_shortcut::ShortcutState::Pressed => KeyState::Pressed,
            tauri_plugin_global_shortcut::ShortcutState::Released => KeyState::Released,
        };
        cb(app, state);
    }) {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("热键: 注册失败 '{}': {}", shortcut_str, e);
            false
        }
    }
}

fn unregister_via_plugin(shortcut_str: &str) {
    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => return,
    };
    if let Some(parsed) = crate::shortcut::parse_shortcut(shortcut_str) {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let _ = app.global_shortcut().unregister(parsed);
    }
}
