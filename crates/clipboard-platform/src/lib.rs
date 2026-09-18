#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
mod instance;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
