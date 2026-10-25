use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// 图片回贴临时文件的保留时长；超时清扫。
pub(super) const IMAGE_TMP_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// 临时文件数量上限；清扫时按修改时间从旧到新删除。
pub(super) const IMAGE_TMP_MAX_FILES: usize = 64;

/// 返回内容寻址的临时 PNG 路径（复用 core 的 save_png：已存在则直接复用）。
pub(super) fn ensure_temp_png(data_dir: &Path, hash: &str, png: &[u8]) -> Result<PathBuf> {
    let saved = clipboard_core::save_png(png, hash, &data_dir.join("image_tmp"))
        .context("无法保存图片临时文件")?;
    Ok(saved.path)
}

/// 启动清扫：删除过期（TTL）与超量的临时图片。
pub(super) fn sweep_image_tmp(data_dir: &Path) -> Result<()> {
    let root = data_dir.join("image_tmp");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("无法读取图片临时目录"),
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry.context("读取图片临时目录项失败")?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|part| part.to_str()) != Some("png") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        files.push((modified, path));
    }
    files.sort();
    let mut kept = 0usize;
    for (modified, path) in files {
        let age = std::time::SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();
        if age > IMAGE_TMP_TTL || kept >= IMAGE_TMP_MAX_FILES {
            let _ = fs::remove_file(&path);
        } else {
            kept += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
