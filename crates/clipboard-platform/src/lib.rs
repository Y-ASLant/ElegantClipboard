#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
mod instance;
#[cfg(windows)]
pub mod outside_click;
#[cfg(windows)]
pub mod source_app;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
