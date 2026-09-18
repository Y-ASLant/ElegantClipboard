#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
