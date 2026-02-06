pub mod clipboard;
pub mod file_ops;
pub mod settings;

use crate::clipboard::ClipboardMonitor;
use crate::database::Database;

/// App state containing database and clipboard monitor
pub struct AppState {
    pub db: Database,
    pub monitor: ClipboardMonitor,
}
