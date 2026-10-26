#[cfg(windows)]
pub mod autostart;
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
#[cfg(windows)]
pub mod outside_click;
#[cfg(windows)]
pub mod source_app;
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
pub use sequence::clipboard_sequence;
pub use service::*;
