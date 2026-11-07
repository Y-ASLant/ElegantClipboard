#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
#[path = "windows/cf_dib.rs"]
mod cf_dib;
pub mod drag_types;
#[cfg(windows)]
#[path = "windows/elevation.rs"]
pub mod elevation;
#[cfg(windows)]
pub mod external_drag;
#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
mod instance;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod notification;
#[cfg(windows)]
pub mod outside_click;
pub mod paste_delivery;
pub mod paste_types;
#[cfg(windows)]
mod shortcut_parse;
#[cfg(windows)]
pub mod source_app;
pub mod source_app_types;
pub mod window_effect;
#[cfg(target_os = "linux")]
pub use linux::{autostart, desktop, external_drag, hotkey, outside_click, source_app};
#[cfg(target_os = "macos")]
pub use macos::{autostart, desktop, external_drag, hotkey, outside_click, source_app};
#[cfg(unix)]
#[path = "unix/instance.rs"]
mod instance;
mod sequence;
mod service;
pub mod update;
pub use sequence::clipboard_sequence;
pub use service::*;
