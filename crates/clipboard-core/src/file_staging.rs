use crate::{History, files::file_capture_item};
use anyhow::{Context, Result, bail, ensure};
use parking_lot::Mutex;
use rusqlite::{OptionalExtension, params};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_STAGED_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_STAGED_DIRECTORY_BYTES: u64 = 256 * 1024 * 1024;
// Captures in different History instances share the same directory budget.
static STAGING_LOCK: Mutex<()> = Mutex::new(());

pub struct FileCaptureOutcome {
    pub id: i64,
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct NewCopies(Vec<PathBuf>);

impl Drop for NewCopies {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

impl History {
    pub fn capture_files_with_staging(
        &self,
        paths: &[String],
        images_dir: &Path,
    ) -> Result<FileCaptureOutcome> {
        let mut item = file_capture_item(paths)?;
        let _lock = STAGING_LOCK.lock();
        let mut copies = NewCopies::default();
        let mut warnings = Vec::new();
        let mut entries = Vec::new();
        let setup = prepare_directory(images_dir);
        match setup {
            Ok((root, mut used)) => {
                for original in paths {
                    match stage_file(Path::new(original), &root, &mut used) {
                        Ok(staged) => {
                            entries.push(serde_json::json!({
                                "original": original,
                                "staged": staged.to_string_lossy(),
                            }));
                            copies.0.push(staged);
                        }
                        Err(error) => warnings.push(format!("{original}: {error:#}")),
                    }
                }
            }
            Err(error) => warnings.push(format!("{error:#}")),
        }
        // An attempted refresh that cannot stage a file must not keep its stale copy.
        item.file_payload = Some(serde_json::json!({"staged": entries}).to_string());
        let (id, old_payload) = {
            let connection = self.db.write_connection();
            let mut conn = connection.lock();
            let tx = conn.transaction()?;
            let existing: Option<(i64, Option<String>)> = tx
                .query_row(
                    "SELECT id, file_payload FROM clipboard_items
                     WHERE content_hash = ?1 AND group_id IS NULL
                     ORDER BY sort_order DESC, created_at DESC, id DESC LIMIT 1",
                    [&item.content_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let id = if let Some((id, _)) = &existing {
                tx.execute(
                    "UPDATE clipboard_items SET file_payload = ?1,
                     access_count = access_count + 1,
                     last_accessed_at = datetime('now', 'localtime'),
                     updated_at = datetime('now', 'localtime'),
                     sort_order = (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_items)
                     WHERE id = ?2",
                    params![item.file_payload, id],
                )?;
                *id
            } else {
                tx.execute(
                    "INSERT INTO clipboard_items
                     (content_type, file_paths, file_payload, content_hash, semantic_hash,
                      preview, byte_size, sort_order)
                     VALUES ('files', ?1, ?2, ?3, ?3, ?4, ?5,
                     (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_items))",
                    params![
                        serde_json::to_string(paths)?,
                        item.file_payload,
                        item.content_hash,
                        item.preview,
                        item.byte_size
                    ],
                )?;
                tx.last_insert_rowid()
            };
            tx.commit()?;
            (id, existing.and_then(|(_, payload)| payload))
        };
        // From this point the committed rows own the copies, including if retention fails.
        copies.0.clear();
        if let Some(payload) = old_payload
            && let Err(error) = self.cleanup_staged(vec![payload], images_dir)
        {
            warnings.push(format!("回收旧暂存副本失败：{error:#}"));
        }
        self.enforce_history_limit(Some(images_dir))?;
        Ok(FileCaptureOutcome { id, warnings })
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn checked_path(path: &Path) -> Result<PathBuf> {
    for ancestor in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        ensure!(
            !is_link(&fs::symlink_metadata(ancestor)?),
            "不暂存符号链接或重解析路径"
        );
    }
    Ok(fs::canonicalize(path)?)
}

fn prepare_directory(images_dir: &Path) -> Result<(PathBuf, u64)> {
    let parent = images_dir.parent().context("无法确定暂存目录")?;
    let parent = checked_path(parent)?;
    let directory = parent.join("staged");
    match fs::create_dir(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let root = checked_path(&directory)?;
    ensure!(
        root.parent() == Some(parent.as_path()),
        "暂存目录超出数据目录"
    );
    let mut used = 0_u64;
    for entry in fs::read_dir(&root)? {
        let metadata = fs::symlink_metadata(entry?.path())?;
        ensure!(
            metadata.is_file() && !is_link(&metadata),
            "暂存目录含非普通文件，已停止新增副本"
        );
        used = used.saturating_add(metadata.len());
    }
    Ok((root, used))
}

fn stage_file(source: &Path, root: &Path, used: &mut u64) -> Result<PathBuf> {
    let source = checked_path(source)?;
    let metadata = fs::symlink_metadata(&source)?;
    ensure!(
        metadata.is_file() && !is_link(&metadata),
        "仅支持普通文件，不递归复制文件夹"
    );
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open a replaced final reparse point itself, then reject its metadata.
        options.custom_flags(0x0020_0000);
    }
    let mut input = options.open(&source)?;
    let metadata = input.metadata()?;
    ensure!(
        metadata.is_file() && !is_link(&metadata),
        "仅支持普通文件，不递归复制文件夹"
    );
    ensure!(metadata.len() <= MAX_STAGED_FILE_BYTES, "文件超过 50 MiB");
    let available = MAX_STAGED_DIRECTORY_BYTES.saturating_sub(*used);
    ensure!(metadata.len() <= available, "暂存目录预算 256 MiB 已用尽");
    let maximum = available.min(MAX_STAGED_FILE_BYTES);
    let mut temporary = tempfile::Builder::new()
        .prefix(".pending-")
        .tempfile_in(root)?;
    let copied = std::io::copy(&mut (&mut input).take(maximum), &mut temporary)?;
    ensure!(
        input.read(&mut [0_u8; 1])? == 0,
        "复制期间文件增大，超过暂存大小或目录预算"
    );
    ensure!(
        copied == metadata.len() && input.metadata()?.modified()? == metadata.modified()?,
        "复制期间源文件发生变化"
    );
    ensure!(checked_path(&source)? == source, "复制期间源路径发生变化");
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    let token = temporary
        .path()
        .file_name()
        .context("临时文件名缺失")?
        .to_string_lossy();
    let suffix = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.len() <= 32 && value.chars().all(|ch| ch.is_ascii_alphanumeric()));
    let name = match suffix {
        Some(suffix) => format!("file-{}.{}", token.trim_start_matches(".pending-"), suffix),
        None => format!("file-{}", token.trim_start_matches(".pending-")),
    };
    let destination = root.join(name);
    if destination.parent() != Some(root) {
        bail!("暂存路径超出目录");
    }
    temporary.persist_noclobber(&destination)?;
    *used += copied;
    Ok(destination)
}

#[cfg(test)]
mod tests;
