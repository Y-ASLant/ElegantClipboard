//! 应用配置管理
//!
//! 处理数据库初始化之前需要读取的配置（如数据库路径）。
//! 配置以 JSON 文件存储在 `%LOCALAPPDATA%\ElegantClipboard\config.json`。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// 日志文件大小上限：10 MB
pub const DEFAULT_LOG_MAX_SIZE: u64 = 10 * 1024 * 1024;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 自定义数据目录（包含数据库和图片），为 None 时使用默认路径
    #[serde(default)]
    pub data_path: Option<String>,

    /// 是否将日志写入文件（默认 false）
    #[serde(default)]
    pub log_to_file: Option<bool>,

    /// 是否以管理员权限运行（默认 false）
    /// 启用后应用在启动时通过计划任务或 UAC 弹窗自行提权
    #[serde(default)]
    pub run_as_admin: Option<bool>,
}

impl AppConfig {
    /// 从配置文件加载
    pub fn load() -> Self {
        let config_path = get_config_path();

        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(config) => {
                        debug!("Configuration loaded from {:?}", config_path);
                        return config;
                    }
                    Err(e) => {
                        warn!("Failed to parse config file: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read config file: {}", e);
                }
            }
        }

        debug!("Using default configuration");
        Self::default()
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<(), String> {
        let config_path = get_config_path();

        // 确保父目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;

        fs::write(&config_path, content).map_err(|e| e.to_string())?;

        info!("Configuration saved to {:?}", config_path);
        Ok(())
    }

    /// 获取数据库路径
    pub fn get_db_path(&self) -> PathBuf {
        if let Some(ref custom_path) = self.data_path {
            if !custom_path.is_empty() {
                return PathBuf::from(custom_path).join("clipboard.db");
            }
        }
        crate::database::get_default_db_path()
    }

    /// 获取图片存储路径
    pub fn get_images_path(&self) -> PathBuf {
        if let Some(ref custom_path) = self.data_path {
            if !custom_path.is_empty() {
                return PathBuf::from(custom_path).join("images");
            }
        }
        crate::database::get_default_images_path()
    }

    /// 获取日志文件路径
    pub fn get_log_path(&self) -> PathBuf {
        self.get_data_dir().join("app.log")
    }

    /// 是否启用文件日志
    pub fn is_log_to_file(&self) -> bool {
        self.log_to_file.unwrap_or(false)
    }

    /// 获取数据目录路径
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

/// 获取配置文件路径（固定在默认位置）
fn get_config_path() -> PathBuf {
    let app_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    app_data.join("ElegantClipboard").join("config.json")
}

/// 将数据从旧路径迁移到新路径
pub fn migrate_data(old_path: &PathBuf, new_path: &PathBuf) -> Result<MigrationResult, String> {
    info!("正在迁移数据: {:?} -> {:?}", old_path, new_path);

    // 确保新目录存在
    fs::create_dir_all(new_path).map_err(|e| format!("创建新目录失败: {}", e))?;

    let mut result = MigrationResult::default();

    // 迁移数据库文件
    let old_db = old_path.join("clipboard.db");
    let new_db = new_path.join("clipboard.db");
    if old_db.exists() {
        // 复制数据库相关文件（db, db-wal, db-shm）
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
                        result
                            .errors
                            .push(format!("Failed to copy {:?}: {}", old_file, e));
                    }
                }
            }
        }
        result.db_migrated = new_db.exists();
    }

    // 迁移图片目录
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
                            result
                                .errors
                                .push(format!("Failed to copy {:?}: {}", file_name, e));
                        }
                    }
                }
            }
        }
        result.images_migrated = new_images.exists();
    }

    info!(
        "Migration complete: {} files, {} bytes",
        result.files_copied, result.bytes_copied
    );
    Ok(result)
}

/// 数据迁移结果
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
