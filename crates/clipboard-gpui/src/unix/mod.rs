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
