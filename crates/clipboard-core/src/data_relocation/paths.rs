use anyhow::{Context, Result, ensure};
use std::path::{Component, Path, PathBuf};

pub(super) struct Relocation {
    destination: PathBuf,
    roots: Vec<PathBuf>,
    current_roots: Vec<PathBuf>,
}

impl Relocation {
    pub(super) fn new(data_dir: &Path, previous_roots: &[PathBuf]) -> Result<Self> {
        ensure!(data_dir.is_absolute(), "迁移目标目录必须是绝对路径");
        let destination = std::fs::canonicalize(data_dir).context("无法确定迁移目标目录")?;
        ensure!(destination.to_str().is_some(), "迁移目标目录不是有效 UTF-8");
        let mut current_roots = vec![normalize(data_dir).context("迁移目标路径无效")?];
        current_roots.push(normalize(&destination).context("迁移目标路径无效")?);
        let mut roots = Vec::new();
        for root in previous_roots {
            ensure!(root.to_str().is_some(), "迁移源目录不是有效 UTF-8");
            let root = normalize(root).context("迁移源目录必须是有效绝对路径")?;
            ensure!(root.parent().is_some(), "不能重定位文件系统根目录");
            roots.push(root);
        }
        // 调用方提供重叠别名时，优先使用范围更小的根目录。
        roots.sort_by_key(|root| std::cmp::Reverse(root.components().count()));
        Ok(Self {
            destination,
            roots,
            current_roots,
        })
    }

    pub(super) fn map(&self, raw: &str) -> Option<String> {
        let path = normalize(Path::new(raw))?;
        // 新目录位于旧根内部时，也要保证重试不会再次添加目标前缀。
        if self
            .current_roots
            .iter()
            .any(|root| relative_to(&path, root).is_some())
        {
            return None;
        }
        for root in &self.roots {
            if let Some(relative) = relative_to(&path, root) {
                let mapped = self.destination.join(relative).to_str()?.to_owned();
                return (mapped != raw).then_some(mapped);
            }
        }
        None
    }
}

fn normalize(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                #[cfg(windows)]
                {
                    use std::path::Prefix;
                    match prefix.kind() {
                        Prefix::Disk(disk) | Prefix::VerbatimDisk(disk) => {
                            normalized.push(format!("{}:", char::from(disk)));
                        }
                        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                            normalized.push(format!(r"\\{}\{}", server.to_str()?, share.to_str()?));
                        }
                        _ => return None,
                    }
                }
                #[cfg(not(windows))]
                normalized.push(prefix.as_os_str());
            }
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
            Component::Normal(part) => {
                // verbatim 路径也可能把点组件作为 Normal 返回。
                if part == ".." {
                    if !normalized.pop() {
                        return None;
                    }
                } else if part != "." {
                    normalized.push(part);
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }
    Some(normalized)
}

fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    for expected in root.components() {
        let actual = parts.next()?;
        #[cfg(windows)]
        let matches = actual.as_os_str().to_str()?.to_lowercase()
            == expected.as_os_str().to_str()?.to_lowercase();
        #[cfg(not(windows))]
        let matches = actual == expected;
        if !matches {
            return None;
        }
    }
    Some(parts.as_path().to_path_buf())
}
