use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// 内容寻址的外部大格式存储：`<root>/<blake3 hex>`。
///
/// 写入永远走 pending→commit 两阶段：pending 期间文件对 GC 不可见，
/// commit 用 `persist_noclobber` 保证并发写入与 GC 删除竞态下引用最终有效。
/// 压缩使用 zstd 等级 3；哈希按未压缩字节计算。
pub struct BlobStore {
    root: PathBuf,
}

const PENDING_PREFIX: &str = ".pending-";
/// 哈希为 64 位十六进制 blake3 摘要，用于把可删文件限制在本存储管理范围内。
const HASH_HEX_LEN: usize = 64;
/// 单个 blob 解压上限：额外格式单条上限 8 MiB，留出裕量防压缩炸弹。
const MAX_BLOB_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

impl BlobStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, hash: &str) -> PathBuf {
        self.root.join(hash)
    }
    /// 压缩并写入 pending 文件，返回（哈希，pending 路径）。
    /// pending 文件必须交给 [`BlobStore::commit`] 或手动删除。
    pub fn write_pending(&self, data: &[u8]) -> Result<(String, PathBuf)> {
        fs::create_dir_all(&self.root).context("无法创建 blob 存储目录")?;
        let hash = crate::clipboard::hash_with_prefix(b"blob:", data);
        let compressed = zstd::bulk::compress(data, 3).context("压缩 blob 数据失败")?;
        let pending = self.root.join(format!("{PENDING_PREFIX}{hash}.tmp"));
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.root).context("无法创建 blob 临时文件")?;
        temporary.write_all(&compressed)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(&pending)
            .map_err(|error| error.error)
            .with_context(|| format!("无法保存 blob 临时文件 {}", pending.display()))?;
        Ok((hash, pending))
    }

    /// 把 pending 文件提交为正式内容；目标已存在时删除 pending（内容相同）。
    pub fn commit(&self, hash: &str, pending: &Path) -> Result<()> {
        let target = self.path(hash);
        match fs::rename(pending, &target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(pending);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // 目标存在而 pending 缺失＝另一写回已提交同一内容，视为成功。
                if target.is_file() {
                    Ok(())
                } else {
                    Err(error).context("blob pending 文件缺失，提交失败")
                }
            }
            Err(error) => Err(error).with_context(|| format!("无法提交 blob {}", target.display())),
        }
    }

    /// 读取并解压指定内容。
    pub fn read(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path(hash);
        let compressed =
            fs::read(&path).with_context(|| format!("读取 blob 失败：{}", path.display()))?;
        zstd::bulk::decompress(&compressed, MAX_BLOB_DECOMPRESSED_BYTES)
            .with_context(|| format!("解压 blob 失败：{}", path.display()))
    }

    /// 删除本存储内未被引用的正式内容；pending 文件由 [`BlobStore::sweep_pending`] 清理。
    /// 仅删除名称为合法哈希的文件，绝不触碰其他内容。
    pub fn collect_garbage(&self, referenced: &HashSet<String>) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error).context("无法读取 blob 存储目录");
            }
        };
        let mut removed = Vec::new();
        for entry in entries {
            let entry = entry.context("读取 blob 目录项失败")?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !is_blob_name(name) || referenced.contains(name) {
                continue;
            }
            fs::remove_file(entry.path())
                .with_context(|| format!("删除无引用 blob 失败：{}", name))?;
            removed.push(name.to_owned());
        }
        Ok(removed)
    }

    /// 启动清理：删除上次运行遗留的 pending 临时文件与普通临时文件。
    pub fn sweep_pending(&self) -> Result<()> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("无法读取 blob 存储目录"),
        };
        for entry in entries {
            let entry = entry.context("读取 blob 目录项失败")?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with(PENDING_PREFIX) || name.starts_with(".tmp") {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

fn is_blob_name(name: &str) -> bool {
    name.len() == HASH_HEX_LEN && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 供平台层启动清扫：删除指定 blob 根目录下的 pending 临时文件。
pub fn sweep_pending_at(root: &Path) -> Result<()> {
    BlobStore::new(root.to_path_buf()).sweep_pending()
}

#[cfg(test)]
mod tests;
