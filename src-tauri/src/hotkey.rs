//! 热键注册抽象层（Register 模式，基于 tauri-plugin-global-shortcut）

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

pub type ShortcutCallback = Arc<dyn Fn(&tauri::AppHandle, KeyState) + Send + Sync>;

static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// 初始化热键系统，保存 AppHandle 供后续使用
pub fn start(app: tauri::AppHandle) {
    let _ = APP_HANDLE.set(app);
}

/// 注册快捷键
pub fn register(shortcut_str: &str, callback: ShortcutCallback) -> bool {
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
    match app
        .global_shortcut()
        .on_shortcut(parsed, move |app, _sc, event| {
            let state = match event.state {
                tauri_plugin_global_shortcut::ShortcutState::Pressed => KeyState::Pressed,
                tauri_plugin_global_shortcut::ShortcutState::Released => KeyState::Released,
            };
            cb(app, state);
        }) {
        Ok(_) => {
            tracing::info!("热键: 已注册 '{}'", shortcut_str);
            true
        }
        Err(e) => {
            tracing::warn!("热键: 注册失败 '{}': {}", shortcut_str, e);
            false
        }
    }
}

/// 注销快捷键
pub fn unregister(shortcut_str: &str) {
    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => return,
    };
    if let Some(parsed) = crate::shortcut::parse_shortcut(shortcut_str) {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let _ = app.global_shortcut().unregister(parsed);
        tracing::info!("热键: 已注销 '{}'", shortcut_str);
    }
}
