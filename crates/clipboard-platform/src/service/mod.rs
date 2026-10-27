mod capture;
mod data;
mod data_directory;
mod file_actions;
#[cfg(target_os = "linux")]
#[path = "../unix/file_list.rs"]
mod file_list;
mod lifecycle;
#[cfg(windows)]
#[path = "../windows/privacy.rs"]
mod privacy;
mod protocol;
mod queue;
mod read_content;
#[cfg(windows)]
#[path = "../windows/read_formats.rs"]
mod read_formats;
#[cfg(unix)]
#[path = "../unix/read_formats.rs"]
pub(crate) mod read_formats;
pub(crate) mod read_html;
mod watcher;
mod worker;

pub use data::{
    InstanceBusy, import_legacy_backup_data, import_legacy_data, restore_backup_data,
    show_existing_instance,
};
pub use lifecycle::Service;
pub use protocol::{CapturedClipboard, Command, CopyFormat, DataSizeInfo, Event, FailureKind};

#[cfg(all(test, windows))]
#[path = "../windows/tests/mod.rs"]
mod tests;
