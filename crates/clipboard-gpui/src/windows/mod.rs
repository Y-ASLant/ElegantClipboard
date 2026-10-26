//! Windows integration for GPUI windows, shell actions, and native input.
//!
//! This module is compiled only on Windows. The application still requires the
//! Windows clipboard-platform service; no substitute backend is provided.

pub(crate) mod acrylic;
pub(crate) mod input;
pub(crate) mod links;
pub(crate) mod paste;
pub(crate) mod position;
pub(crate) mod sound;
pub(crate) mod tray;
pub(crate) mod window;
