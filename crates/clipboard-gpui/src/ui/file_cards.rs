use super::*;

/// 文件卡片可用性（历史卡片与预检共用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileCardAvailability {
    Available,
    Missing,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileCardKind {
    File,
    Folder,
    Multiple,
}

#[derive(Clone, Debug)]
pub(super) struct FileCardInfo {
    pub(super) availability: FileCardAvailability,
    pub(super) kind: FileCardKind,
    pub(super) image_path: Option<PathBuf>,
    pub(super) image_too_large: bool,
    pub(super) total_size: Option<u64>,
}

pub(super) fn single_file_image_path(entries: &[FilePreviewEntry]) -> Option<PathBuf> {
    let [entry] = entries else { return None };
    if !entry.exists || entry.is_dir || entry.metadata_error.is_some() {
        return None;
    }
    let size = entry.size?;
    let limit = file_image_preview_limit(&entry.resolved_path);
    if size == 0 || size > limit {
        return None;
    }
    if !supported_image_file(&entry.resolved_path) {
        return None;
    }
    Some(PathBuf::from(&entry.resolved_path))
}

fn file_image_preview_limit(path: &str) -> u64 {
    if path.starts_with(r"\\") {
        10 * 1024 * 1024
    } else {
        50 * 1024 * 1024
    }
}

fn supported_image_file(path: &str) -> bool {
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return false;
    };
    ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
        .iter()
        .any(|supported| extension.eq_ignore_ascii_case(supported))
}

/// `files` 记录是否恰好是一张受支持的图片文件。
///
/// 复制截图工具（PixPin 等）产出的 JPG 会以「文件」形式进入历史，用户在悬停设置里
/// 看到的却是「图片」开关，所以这类记录要按图片对待。
pub(super) fn single_image_file_paths(raw: Option<&str>) -> bool {
    let paths = clipboard_core::parse_file_paths_for_display(raw);
    matches!(paths.as_slice(), [path] if supported_image_file(path))
}

/// 读本地图片文件的尺寸。只解析文件头，不解码像素，开销极小。
///
/// 网络路径（`\\`）直接放弃：文件头读取会阻塞调用线程，而这个调用来自 UI 线程。
pub(super) fn image_file_dimensions(path: &Path) -> Option<(i64, i64)> {
    if path.to_string_lossy().starts_with(r"\\") {
        return None;
    }
    let (width, height) = image::image_dimensions(path).ok()?;
    Some((i64::from(width), i64::from(height)))
}

/// 「复制/粘贴」操作的条件文案：可粘贴时为「粘贴+名词」，否则为「复制+名词」。
pub(crate) fn copy_or_paste_label(
    language: LanguagePreference,
    can_paste: bool,
    zh_noun: &str,
    en_noun: &str,
) -> String {
    let verb = tr(
        language,
        if can_paste { "粘贴" } else { "复制" },
        if can_paste { "Paste" } else { "Copy" },
    );
    if language == LanguagePreference::English {
        format!("{verb} {en_noun}")
    } else {
        format!("{verb}{zh_noun}")
    }
}

/// 「另存为」的默认文件名：取路径的最后一段文件名；路径无文件名时返回 `None`。
pub(crate) fn save_as_suggested_name(path: impl AsRef<Path>) -> Option<String> {
    Path::new(path.as_ref())
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}
fn inspect_file_card(raw_paths: &str) -> FileCardInfo {
    let mut info = FileCardInfo {
        availability: FileCardAvailability::Available,
        kind: FileCardKind::File,
        image_path: None,
        image_too_large: false,
        total_size: Some(0),
    };
    let paths = clipboard_core::parse_file_paths_for_display(Some(raw_paths));
    if paths.is_empty() {
        info.availability = FileCardAvailability::Unreadable;
        info.total_size = None;
        return info;
    }
    if paths.len() > 1 {
        info.kind = FileCardKind::Multiple;
    }
    for path in &paths {
        if !Path::new(path).is_absolute() {
            info.availability = FileCardAvailability::Unreadable;
            info.total_size = None;
            continue;
        }
        match std::fs::metadata(path) {
            Ok(metadata) if paths.len() == 1 && metadata.is_file() => {
                info.total_size = info
                    .total_size
                    .and_then(|total| total.checked_add(metadata.len()));
                info.image_too_large =
                    supported_image_file(path) && metadata.len() > file_image_preview_limit(path);
                info.image_path = single_file_image_path(&[FilePreviewEntry {
                    original_path: path.clone(),
                    resolved_path: path.clone(),
                    exists: true,
                    is_dir: false,
                    size: Some(metadata.len()),
                    metadata_error: None,
                }]);
            }
            Ok(metadata) if paths.len() == 1 && metadata.is_dir() => {
                info.kind = FileCardKind::Folder;
                info.total_size = None;
            }
            Ok(metadata) if metadata.is_file() => {
                info.total_size = info
                    .total_size
                    .and_then(|total| total.checked_add(metadata.len()));
            }
            Ok(_) => info.total_size = None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                info.total_size = None;
                if info.availability == FileCardAvailability::Available {
                    info.availability = FileCardAvailability::Missing;
                }
            }
            Err(_) => {
                info.availability = FileCardAvailability::Unreadable;
                info.total_size = None;
            }
        }
    }
    info
}

impl ClipboardView {
    pub(super) fn refresh_file_card_info(&mut self, cx: &mut Context<Self>) {
        let loaded_paths: HashMap<_, _> = self
            .history
            .items
            .iter()
            .filter(|item| item.content_type == "files")
            .map(|item| (item.id, item.file_paths.as_deref().unwrap_or_default()))
            .collect();
        self.file_card_checked
            .retain(|id, raw| loaded_paths.get(id).is_some_and(|current| *current == raw));
        self.file_card_info
            .retain(|id, _| self.file_card_checked.contains_key(id));
        let candidates: Vec<_> = self
            .history
            .items
            .iter()
            .filter(|item| {
                item.content_type == "files" && !self.file_card_checked.contains_key(&item.id)
            })
            .map(|item| (item.id, item.file_paths.clone().unwrap_or_default()))
            .collect();
        if candidates.is_empty() {
            return;
        }
        self.file_card_checked.extend(candidates.iter().cloned());
        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let (local, network): (Vec<_>, Vec<_>) =
                candidates.into_iter().partition(|(_, raw)| {
                    !clipboard_core::parse_file_paths_for_display(Some(raw))
                        .iter()
                        .any(|path| path.starts_with(r"\\"))
                });
            let local_results: Vec<_> = local
                .into_iter()
                .map(|(id, raw)| {
                    let info = inspect_file_card(&raw);
                    (id, raw, info)
                })
                .collect();
            if !local_results.is_empty() && sender.send_blocking(local_results).is_err() {
                return;
            }
            for (id, raw) in network {
                let info = inspect_file_card(&raw);
                let result = (id, raw, info);
                if sender.send_blocking(vec![result]).is_err() {
                    return;
                }
            }
        });
        cx.spawn(async move |view, cx| {
            while let Ok(results) = receiver.recv().await {
                let _ = view.update(cx, |this, cx| {
                    let mut changed = false;
                    for (id, source, info) in results {
                        if this.history.items.iter().any(|item| {
                            item.id == id
                                && item.content_type == "files"
                                && item.file_paths.as_deref().unwrap_or_default() == source
                        }) {
                            this.file_card_info.insert(id, info);
                            changed = true;
                        }
                    }
                    if changed {
                        this.sync_image_assets(cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}
#[cfg(test)]
mod file_image_preview_tests {
    use super::{
        FileCardAvailability, FileCardKind, FilePreviewEntry, inspect_file_card,
        single_file_image_path, supported_image_file,
    };
    use std::path::PathBuf;

    fn entry(path: &str, size: Option<u64>) -> FilePreviewEntry {
        FilePreviewEntry {
            original_path: path.into(),
            resolved_path: path.into(),
            exists: true,
            is_dir: false,
            size,
            metadata_error: None,
        }
    }

    #[test]
    fn supported_image_extensions_are_ascii_case_insensitive() {
        for extension in ["png", "jpg", "jpeg", "gif", "webp", "bmp"] {
            assert!(supported_image_file(&format!(r"C:\sample.{extension}")));
            assert!(supported_image_file(&format!(
                r"C:\sample.{}",
                extension.to_ascii_uppercase()
            )));
        }
        for path in [
            r"C:\sample",
            r"C:\sample.txt",
            r"C:\sample.pngx",
            r"C:\sample.ＰＮＧ",
        ] {
            assert!(!supported_image_file(path));
        }
    }

    #[test]
    fn only_single_readable_bounded_image_is_decoded() {
        let image = entry(r"C:\sample.PNG", Some(2_000));
        assert_eq!(
            single_file_image_path(std::slice::from_ref(&image)),
            Some(PathBuf::from(r"C:\sample.PNG"))
        );
        assert!(single_file_image_path(&[image.clone(), image]).is_none());
        assert!(single_file_image_path(&[entry(r"C:\sample.txt", Some(2_000))]).is_none());
        assert!(single_file_image_path(&[entry(r"C:\sample.png", None)]).is_none());
        assert!(
            single_file_image_path(&[entry(r"C:\sample.png", Some(50 * 1024 * 1024 + 1))])
                .is_none()
        );
        assert!(
            single_file_image_path(&[entry(
                r"\\server\share\sample.png",
                Some(10 * 1024 * 1024 + 1)
            )])
            .is_none()
        );
    }

    #[test]
    fn missing_unreadable_and_directory_paths_keep_file_details() {
        let mut image = entry(r"C:\sample.png", Some(2_000));
        image.exists = false;
        assert!(single_file_image_path(std::slice::from_ref(&image)).is_none());
        image.exists = true;
        image.is_dir = true;
        assert!(single_file_image_path(std::slice::from_ref(&image)).is_none());
        image.is_dir = false;
        image.metadata_error = Some("unavailable".into());
        assert!(single_file_image_path(&[image]).is_none());
    }

    #[test]
    fn card_file_probe_reports_missing_and_oversized_paths() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("sample.png");
        std::fs::write(&image, b"small synthetic payload").unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![image.clone()]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert_eq!(checked.kind, FileCardKind::File);
        assert_eq!(checked.image_path, Some(image));
        assert_eq!(checked.total_size, Some(23));
        let missing = dir.path().join("missing.png");
        let checked = inspect_file_card(&serde_json::to_string(&vec![missing]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Missing);
        assert!(checked.image_path.is_none());
        assert_eq!(checked.total_size, None);
        assert_eq!(
            inspect_file_card("not json").availability,
            FileCardAvailability::Unreadable
        );
        let folder = dir.path().join("folder.png");
        std::fs::create_dir(&folder).unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![folder.clone()]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert_eq!(checked.kind, FileCardKind::Folder);
        assert!(checked.image_path.is_none());
        assert_eq!(checked.total_size, None);
        let mixed = serde_json::to_string(&vec![folder, dir.path().join("missing.png")]);
        let checked = inspect_file_card(&mixed.unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Missing);
        assert_eq!(checked.kind, FileCardKind::Multiple);
        assert_eq!(checked.total_size, None);
        let large = dir.path().join("large.png");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(50 * 1024 * 1024 + 1)
            .unwrap();
        let checked = inspect_file_card(&serde_json::to_string(&vec![large]).unwrap());
        assert_eq!(checked.availability, FileCardAvailability::Available);
        assert!(checked.image_path.is_none());
        assert!(checked.image_too_large);
    }
}
