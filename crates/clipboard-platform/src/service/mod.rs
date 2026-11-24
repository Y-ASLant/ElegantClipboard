mod capture;
mod data;
mod data_directory;
mod file_actions;
#[cfg(windows)]
#[path = "../windows/read_files.rs"]
mod file_list;
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
#[cfg(windows)]
#[path = "../windows/read_image.rs"]
mod read_image;
mod watcher;
mod worker;

pub use crate::update::ReleaseInfo;
pub use data::{
    InstanceBusy, import_legacy_backup_data, resolve_data_directory, restore_backup_data,
    show_existing_instance,
};
pub use data_directory::{
    PendingImport, apply_pending_import, apply_pending_import_with_progress, apply_startup_jobs,
    control_root, display_path, has_pending_import, queue_pending_import, queue_pending_relocate,
    reset_location_to_default, set_data_location,
};
pub use lifecycle::Service;
pub use protocol::{
    CapturedClipboard, Command, CopyFormat, CopyMode, CopyPurpose, DataSizeInfo, Event,
    FailureKind, LegacyMigrationReport, LegacyMigrationStatus, UpdateStatus,
};

#[cfg(test)]
mod tests;
