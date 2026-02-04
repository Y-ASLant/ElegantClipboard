//! Application configuration management
//!
//! This module handles configuration that needs to be read before the database is initialized,
//! such as the database path itself. Configuration is stored in a JSON file.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn, error};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Custom data path (directory containing the database and images)
    /// If None, use the default path
    #[serde(default)]
    pub data_path: Option<String>,
}

impl AppConfig {
    /// Load configuration from file
    pub fn load() -> Self {
        let config_path = get_config_path();
        
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(config) => {
                            info!("Configuration loaded from {:?}", config_path);
                            return config;
                        }
                        Err(e) => {
                            warn!("Failed to parse config file: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read config file: {}", e);
                }
            }
        }
        
        info!("Using default configuration");
        Self::default()
    }
    
    /// Save configuration to file
    pub fn save(&self) -> Result<(), String> {
        let config_path = get_config_path();
        
        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| e.to_string())?;
        
        fs::write(&config_path, content).map_err(|e| e.to_string())?;
        
        info!("Configuration saved to {:?}", config_path);
        Ok(())
    }
    
    /// Get the database path based on configuration
    pub fn get_db_path(&self) -> PathBuf {
        if let Some(ref custom_path) = self.data_path {
            if !custom_path.is_empty() {
                return PathBuf::from(custom_path).join("clipboard.db");
            }
        }
        crate::database::get_default_db_path()
    }
    
    /// Get the images path based on configuration
    #[allow(dead_code)]
    pub fn get_images_path(&self) -> PathBuf {
        if let Some(ref custom_path) = self.data_path {
            if !custom_path.is_empty() {
                return PathBuf::from(custom_path).join("images");
            }
        }
        crate::database::get_images_path()
    }
    
    /// Get the data directory path
    pub fn get_data_dir(&self) -> PathBuf {
        if let Some(ref custom_path) = self.data_path {
            if !custom_path.is_empty() {
                return PathBuf::from(custom_path);
            }
        }
        crate::database::get_default_db_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Get the configuration file path (always in the default location)
fn get_config_path() -> PathBuf {
    let app_data = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    app_data.join("ClipboardManager").join("config.json")
}

/// Migrate data from old path to new path
pub fn migrate_data(old_path: &PathBuf, new_path: &PathBuf) -> Result<MigrationResult, String> {
    info!("Migrating data from {:?} to {:?}", old_path, new_path);
    
    // Ensure new directory exists
    fs::create_dir_all(new_path).map_err(|e| format!("Failed to create new directory: {}", e))?;
    
    let mut result = MigrationResult::default();
    
    // Migrate database file
    let old_db = old_path.join("clipboard.db");
    let new_db = new_path.join("clipboard.db");
    if old_db.exists() {
        // Copy database files (db, db-wal, db-shm)
        for ext in &["", "-wal", "-shm"] {
            let old_file = old_path.join(format!("clipboard.db{}", ext));
            let new_file = new_path.join(format!("clipboard.db{}", ext));
            if old_file.exists() {
                match fs::copy(&old_file, &new_file) {
                    Ok(bytes) => {
                        info!("Copied {:?} ({} bytes)", old_file, bytes);
                        result.files_copied += 1;
                        result.bytes_copied += bytes;
                    }
                    Err(e) => {
                        error!("Failed to copy {:?}: {}", old_file, e);
                        result.errors.push(format!("Failed to copy {:?}: {}", old_file, e));
                    }
                }
            }
        }
        result.db_migrated = new_db.exists();
    }
    
    // Migrate images folder
    let old_images = old_path.join("images");
    let new_images = new_path.join("images");
    if old_images.exists() && old_images.is_dir() {
        fs::create_dir_all(&new_images).ok();
        if let Ok(entries) = fs::read_dir(&old_images) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let old_file = entry.path();
                let new_file = new_images.join(&file_name);
                
                if old_file.is_file() {
                    match fs::copy(&old_file, &new_file) {
                        Ok(bytes) => {
                            result.files_copied += 1;
                            result.bytes_copied += bytes;
                        }
                        Err(e) => {
                            result.errors.push(format!("Failed to copy {:?}: {}", file_name, e));
                        }
                    }
                }
            }
        }
        result.images_migrated = new_images.exists();
    }
    
    info!("Migration complete: {} files, {} bytes", result.files_copied, result.bytes_copied);
    Ok(result)
}

/// Result of data migration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationResult {
    pub db_migrated: bool,
    pub images_migrated: bool,
    pub files_copied: usize,
    pub bytes_copied: u64,
    pub errors: Vec<String>,
}

impl MigrationResult {
    pub fn success(&self) -> bool {
        self.errors.is_empty() && self.db_migrated
    }
}
