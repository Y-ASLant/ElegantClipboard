//! Native desktop integrations shared by the macOS and Linux GPUI surfaces.
pub(crate) mod acrylic;
pub(crate) mod input;
pub(crate) mod links;
pub(crate) mod paste;
pub(crate) mod position;
pub(crate) mod sound;
#[cfg(target_os = "linux")]
#[path = "../linux/window.rs"]
pub(crate) mod window;
#[cfg(target_os = "macos")]
#[path = "../macos/window.rs"]
pub(crate) mod window;

/// 前台窗口属于本进程（两平台的粘贴目标实现均委托 clipboard_platform::desktop）。
pub(crate) fn foreground_window_is_ours() -> bool {
    clipboard_platform::desktop::foreground_target()
        .is_some_and(|target| target.1 == std::process::id())
}
