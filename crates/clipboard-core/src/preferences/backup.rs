use super::*;
use rusqlite::{Connection, params_from_iter};
use std::sync::LazyLock;

// Only consumed preferences and migration state belong in portable backups.
// In particular, a gpui_ prefix alone does not authorize an obsolete or unknown key.
const BACKUP_KEYS: &[&str] = &[
    THEME_KEY,
    THEME_COLOR_KEY,
    CORNER_RADIUS_KEY,
    LANGUAGE_KEY,
    HOTKEY_KEY,
    CAPTURE_PAUSED_KEY,
    TRANSIENT_FILTER_KEY,
    WINDOW_SIZE_KEY,
    PERSIST_WINDOW_SIZE_KEY,
    AUTO_RESET_STATE_KEY,
    SEARCH_AUTO_FOCUS_KEY,
    SEARCH_AUTO_CLEAR_KEY,
    SKIP_CLEAR_CONFIRM_KEY,
    PASTE_CLOSE_WINDOW_KEY,
    PASTE_KEY_KEY,
    PASTE_MOVE_TO_TOP_KEY,
    QUICK_PASTE_ENABLED_KEY,
    PASTE_SHORTCUTS_KEY,
    WINDOW_POSITION_KEY,
    HOVER_PREVIEW_KEY,
    DISPLAY_KEY,
    MONITOR_TYPES_KEY,
    APP_FILTER_KEY,
    ONBOARDING_COMPLETED_KEY,
    AUDIO_KEY,
    WINDOW_EFFECT_KEY,
    RETENTION_KEY,
    DEDUP_STRATEGY_KEY,
    RESTORE_CLIPBOARD_KEY,
    SEQUENCE_HOTKEY_KEY,
    "_migration_url_content_type",
    "_migration_backfill_semantic_hash",
];

pub(crate) fn prune_non_gpui_settings(connection: &Connection) -> Result<()> {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        let mut sql = String::from("DELETE FROM settings WHERE key NOT IN (");
        for index in 0..BACKUP_KEYS.len() {
            if index > 0 {
                sql.push(',');
            }
            sql.push('?');
        }
        sql.push(')');
        sql
    });
    connection.execute(&SQL, params_from_iter(BACKUP_KEYS.iter()))?;
    Ok(())
}
