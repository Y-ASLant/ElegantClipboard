use crate::{
    HISTORY_LIMIT, History,
    database::{ContentType, NewClipboardItem},
};
use anyhow::{Result, bail};
use std::path::Path;

pub const MAX_FILE_PATHS: usize = 256;
pub const MAX_PATH_LIST_BYTES: usize = 1024 * 1024;

pub(crate) fn parse_file_paths(raw: Option<&str>) -> Result<Vec<String>> {
    let paths: Vec<String> =
        serde_json::from_str(raw.ok_or_else(|| anyhow::anyhow!("文件路径缺失"))?)?;
    if paths.is_empty() || paths.len() > MAX_FILE_PATHS || paths.iter().any(|path| path.is_empty())
    {
        bail!("文件路径列表无效");
    }
    Ok(paths)
}

impl History {
    pub fn capture_files(&self, paths: &[String], images_dir: &Path) -> Result<i64> {
        if paths.is_empty()
            || paths.len() > MAX_FILE_PATHS
            || paths.iter().any(|path| path.is_empty())
        {
            bail!("文件路径列表为空或超过 256 项");
        }
        let serialized = serde_json::to_vec(paths)?;
        if serialized.len() > MAX_PATH_LIST_BYTES {
            bail!("文件路径列表超过 1 MiB");
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"files:");
        hasher.update(&serialized);
        let hash = hasher.finalize().to_hex().to_string();
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(id);
        }
        let preview = if paths.len() == 1 {
            Path::new(&paths[0])
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| paths[0].clone())
        } else {
            format!("{} 个文件或文件夹", paths.len())
        };
        let id = self.repo.insert(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(paths.to_vec()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            preview: Some(preview.chars().take(240).collect()),
            byte_size: serialized.len() as i64,
            ..Default::default()
        })?;
        let (_, deleted_images, _) = self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        self.cleanup_images(deleted_images, images_dir);
        Ok(id)
    }

    pub fn files(&self, id: i64) -> Result<Vec<String>> {
        let item = self.item(id)?;
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        parse_file_paths(item.file_paths.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PAGE_SIZE, PreviewContent};

    #[test]
    fn file_paths_are_saved_deduplicated_and_reordered_with_text() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let files = vec![
            directory
                .path()
                .join("a.txt")
                .to_string_lossy()
                .into_owned(),
        ];
        let id = history.capture_files(&files, &directory.path().join("images"))?;
        let text = history.capture("text")?.unwrap();
        assert_eq!(
            history.capture_files(&files, &directory.path().join("images"))?,
            id
        );
        assert_eq!(history.files(id)?, files);
        assert_eq!(history.preview_content(id)?, PreviewContent::Files(files));
        history.reorder(text, id, false, false)?;
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, text);
        assert_eq!(history.count("", false)?, 2);
        Ok(())
    }

    #[test]
    fn rejects_empty_and_oversized_path_lists() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        assert!(history.capture_files(&[], &images).is_err());
        assert!(
            history
                .capture_files(&vec!["x".into(); 257], &images)
                .is_err()
        );
        assert!(history.capture_files(&["".into()], &images).is_err());
        assert!(
            history
                .capture_files(&["x".repeat(MAX_PATH_LIST_BYTES)], &images)
                .is_err()
        );
        Ok(())
    }
}
