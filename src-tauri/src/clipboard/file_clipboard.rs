//! Windows 文件剪贴板：捕获/还原 CF_HDROP 与伴生格式，支持文件内容 staging。

use base64::Engine;
use clipboard_rs::{Clipboard as ClipboardTrait, ClipboardContext, ContentFormat};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, warn};

/// 复制时从剪贴板捕获的文件数据（尚未 staging）
#[derive(Debug, Clone, Default)]
pub struct FileCaptureData {
    pub paths: Vec<String>,
    pub hdrop_raw: Option<Vec<u8>>,
    pub extra_formats: Vec<(String, Vec<u8>)>,
}

/// 持久化到数据库的文件剪贴板 payload
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdrop_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<FormatBlob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staged: Vec<StagedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormatBlob {
    pub name: String,
    pub b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedFile {
    pub original: String,
    pub staged: String,
    pub size: u64,
}

const FILE_EXTRA_FORMATS: &[&str] = &[
    "Preferred DropEffect",
    "Shell IDList Array",
    "UsingDefaultDragImage",
    "DragImageBits",
    "DragContext",
    "InShellDragLoop",
    "FileGroupDescriptorW",
    "FileGroupDescriptor",
    "FileContents",
];

const DEFAULT_MAX_STAGE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_EXTRA_FORMAT_BYTES: usize = 64 * 1024;

pub fn staged_paths_from_payload(raw: Option<&str>) -> Vec<String> {
    decode_payload(raw)
        .map(|payload| payload.staged.iter().map(|s| s.staged.clone()).collect())
        .unwrap_or_default()
}

pub fn originals_all_exist(paths: &[String]) -> bool {
    !paths.is_empty() && paths.iter().all(|p| Path::new(p).exists())
}

/// 仅当原始路径均存在时才还原 capture 时的 CF_HDROP blob。
pub fn should_use_raw_hdrop(original_paths: &[String], payload: Option<&FilePayload>) -> bool {
    payload.and_then(|p| p.hdrop_b64.as_ref()).is_some() && originals_all_exist(original_paths)
}

pub fn resolve_item_paths(file_paths: Option<&str>, file_payload: Option<&str>) -> Vec<String> {
    let paths = parse_file_paths(file_paths);
    let payload = decode_payload(file_payload);
    resolve_paths(&paths, payload.as_ref())
}

pub fn item_files_all_exist(file_paths: Option<&str>, file_payload: Option<&str>) -> bool {
    let originals = parse_file_paths(file_paths);
    if originals.is_empty() {
        return decode_payload(file_payload)
            .is_some_and(|p| p.hdrop_b64.is_some() || !p.extra.is_empty());
    }
    resolve_item_paths(file_paths, file_payload)
        .iter()
        .all(|p| Path::new(p).exists())
}

pub fn write_payload_extras(
    ctx: &mut ClipboardContext,
    payload: Option<&FilePayload>,
) -> Result<(), String> {
    write_extra_formats(ctx, payload)
}

pub fn encode_payload(payload: &FilePayload) -> String {
    serde_json::to_string(payload).unwrap_or_default()
}

pub fn decode_payload(raw: Option<&str>) -> Option<FilePayload> {
    let raw = raw.filter(|s| !s.is_empty())?;
    serde_json::from_str(raw).ok()
}

pub fn capture_from_clipboard(ctx: &ClipboardContext) -> Option<FileCaptureData> {
    let paths = ctx.get_files().ok().unwrap_or_default();
    let hdrop_raw = ctx.get_hdrop_raw().ok().filter(|b| !b.is_empty());
    let extra_formats = capture_extra_formats(ctx);

    let has_virtual = extra_formats
        .iter()
        .any(|(name, _)| name.contains("FileGroupDescriptor"));

    if paths.is_empty() && !has_virtual {
        return None;
    }

    Some(FileCaptureData {
        paths,
        hdrop_raw,
        extra_formats,
    })
}

pub fn clipboard_has_pending_files(ctx: &ClipboardContext) -> bool {
    if ctx.has(ContentFormat::Files) {
        return true;
    }
    FILE_EXTRA_FORMATS
        .iter()
        .any(|name| name.contains("FileGroupDescriptor") && ctx.get_buffer(name).is_ok())
}

fn capture_extra_formats(ctx: &ClipboardContext) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for name in FILE_EXTRA_FORMATS {
        if *name == "FileContents" {
            continue;
        }
        if let Ok(bytes) = ctx.get_buffer(name)
            && !bytes.is_empty()
            && bytes.len() <= MAX_EXTRA_FORMAT_BYTES
        {
            out.push(((*name).to_string(), bytes));
        }
    }
    out
}

pub fn build_payload(
    capture: &FileCaptureData,
    staged_dir: &Path,
    max_stage_bytes: u64,
) -> FilePayload {
    let mut payload = FilePayload {
        hdrop_b64: capture
            .hdrop_raw
            .as_ref()
            .map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        extra: capture
            .extra_formats
            .iter()
            .map(|(name, data)| FormatBlob {
                name: name.clone(),
                b64: base64::engine::general_purpose::STANDARD.encode(data),
            })
            .collect(),
        staged: Vec::new(),
    };

    if capture.paths.is_empty() {
        return payload;
    }

    let stage_limit = if max_stage_bytes > 0 {
        max_stage_bytes
    } else {
        DEFAULT_MAX_STAGE_BYTES
    };
    std::fs::create_dir_all(staged_dir).ok();

    for path in &capture.paths {
        let src = Path::new(path);
        if !src.is_file() {
            continue;
        }
        let Ok(meta) = std::fs::metadata(src) else {
            continue;
        };
        let size = meta.len();
        if size > stage_limit {
            debug!("Skip staging large file {} ({} bytes)", path, size);
            continue;
        }

        let file_name = src.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let path_hash = &blake3::hash(path.as_bytes()).to_hex()[..8];
        let staged_path = staged_dir.join(format!("{path_hash}_{file_name}"));
        if staged_path.exists() {
            payload.staged.push(StagedFile {
                original: path.clone(),
                staged: staged_path.to_string_lossy().to_string(),
                size,
            });
            continue;
        }
        if std::fs::copy(src, &staged_path).is_ok() {
            payload.staged.push(StagedFile {
                original: path.clone(),
                staged: staged_path.to_string_lossy().to_string(),
                size,
            });
        }
    }

    payload
}

pub fn parse_file_paths(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    serde_json::from_str(raw).unwrap_or_default()
}

const FILEDESCRIPTORW_SIZE: usize = 592;
const FILEDESCRIPTORW_NAME_OFFSET: usize = 72;
const FILEDESCRIPTORA_SIZE: usize = 332;
const FILEDESCRIPTORA_NAME_OFFSET: usize = 72;

fn decode_utf16_null_terminated(bytes: &[u8]) -> String {
    let mut units = Vec::new();
    for chunk in bytes.chunks(2) {
        if chunk.len() < 2 {
            break;
        }
        let u = u16::from_le_bytes([chunk[0], chunk[1]]);
        if u == 0 {
            break;
        }
        units.push(u);
    }
    String::from_utf16_lossy(&units)
}

/// 从 FileGroupDescriptor(W) 伴生格式解析虚拟文件名（RDP 等场景）
pub fn parse_file_group_descriptor_w(data: &[u8]) -> Vec<String> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut names = Vec::new();
    for i in 0..count.min(32) {
        let base = 4 + i * FILEDESCRIPTORW_SIZE;
        if base + FILEDESCRIPTORW_SIZE > data.len() {
            break;
        }
        let name_start = base + FILEDESCRIPTORW_NAME_OFFSET;
        let name_end = base + FILEDESCRIPTORW_SIZE;
        let name = decode_utf16_null_terminated(&data[name_start..name_end]);
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

fn decode_ascii_null_terminated(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// 从 FileGroupDescriptor（ANSI）伴生格式解析虚拟文件名
pub fn parse_file_group_descriptor_a(data: &[u8]) -> Vec<String> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut names = Vec::new();
    for i in 0..count.min(32) {
        let base = 4 + i * FILEDESCRIPTORA_SIZE;
        if base + FILEDESCRIPTORA_SIZE > data.len() {
            break;
        }
        let name_start = base + FILEDESCRIPTORA_NAME_OFFSET;
        let name_end = base + FILEDESCRIPTORA_SIZE;
        let name = decode_ascii_null_terminated(&data[name_start..name_end]);
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

pub fn descriptor_file_names(extra: &[(String, Vec<u8>)]) -> Vec<String> {
    for (name, data) in extra {
        if name.contains("FileGroupDescriptorW") {
            return parse_file_group_descriptor_w(data);
        }
    }
    for (name, data) in extra {
        if name.contains("FileGroupDescriptor") {
            return parse_file_group_descriptor_a(data);
        }
    }
    Vec::new()
}

/// 用于入库展示的路径列表：优先真实路径，否则用虚拟文件名
pub fn effective_file_paths(capture: &FileCaptureData) -> Vec<String> {
    if !capture.paths.is_empty() {
        return capture.paths.clone();
    }
    descriptor_file_names(&capture.extra_formats)
}

/// 文件条目卡片 preview 文案（仅文件名，不含完整路径）
pub fn file_entry_preview(paths: &[String], extra: &[(String, Vec<u8>)]) -> String {
    let display = if !paths.is_empty() {
        paths.to_vec()
    } else {
        descriptor_file_names(extra)
    };
    match display.len() {
        0 => "[文件]".to_string(),
        1 => Path::new(&display[0])
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| display[0].clone()),
        n => format!("{n} files"),
    }
}

pub fn resolve_paths(paths: &[String], payload: Option<&FilePayload>) -> Vec<String> {
    let staged_map: std::collections::HashMap<&str, &str> = payload
        .map(|p| {
            p.staged
                .iter()
                .map(|s| (s.original.as_str(), s.staged.as_str()))
                .collect()
        })
        .unwrap_or_default();

    paths
        .iter()
        .map(|path| {
            let p = Path::new(path);
            if p.exists() {
                return path.clone();
            }
            if let Some(staged) = staged_map.get(path.as_str()) {
                return (*staged).to_string();
            }
            path.clone()
        })
        .collect()
}

pub fn write_files_to_clipboard(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
    ctx: &mut ClipboardContext,
) -> Result<(), String> {
    let paths = parse_file_paths(file_paths);
    let payload = decode_payload(file_payload);
    let resolved = resolve_paths(&paths, payload.as_ref());

    if resolved.is_empty() && payload.as_ref().is_none_or(|p| p.hdrop_b64.is_none()) {
        return Err("Item has no file paths".to_string());
    }

    ctx.clear()
        .map_err(|e| format!("Failed to clear clipboard: {e}"))?;

    if should_use_raw_hdrop(&paths, payload.as_ref())
        && let Some(raw) = decode_b64(payload.as_ref().and_then(|p| p.hdrop_b64.as_ref()))
    {
        if ctx.set_hdrop_raw(&raw).is_ok() {
            write_extra_formats(ctx, payload.as_ref())?;
            debug!(
                "Restored file clipboard from raw CF_HDROP ({} paths)",
                resolved.len()
            );
            return Ok(());
        }
        warn!("Raw CF_HDROP restore failed, falling back to path list");
    }

    if !resolved.is_empty() {
        ctx.set_files(resolved.clone())
            .map_err(|e| format!("Failed to set clipboard files: {e}"))?;
        write_extra_formats(ctx, payload.as_ref())?;
        debug!("Restored file clipboard from {} path(s)", resolved.len());
        return Ok(());
    }

    write_extra_formats(ctx, payload.as_ref())?;
    Ok(())
}

fn write_extra_formats(
    ctx: &mut ClipboardContext,
    payload: Option<&FilePayload>,
) -> Result<(), String> {
    let Some(payload) = payload else {
        return Ok(());
    };
    for extra in &payload.extra {
        let Some(bytes) = decode_b64(Some(&extra.b64)) else {
            continue;
        };
        if let Err(e) = ctx.set_raw_no_clear(&extra.name, &bytes) {
            debug!("Skip extra format {}: {}", extra.name, e);
        }
    }
    Ok(())
}

fn decode_b64(raw: Option<&String>) -> Option<Vec<u8>> {
    let raw = raw?;
    base64::engine::general_purpose::STANDARD.decode(raw).ok()
}

pub fn merge_file_paths(items: &[(Option<&str>, Option<&str>)]) -> Vec<String> {
    let mut merged = Vec::new();
    for (paths_json, payload_raw) in items {
        let paths = parse_file_paths(*paths_json);
        let payload = decode_payload(*payload_raw);
        let resolved = resolve_paths(&paths, payload.as_ref());
        for path in resolved {
            if !merged.iter().any(|p| p == &path) {
                merged.push(path);
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_roundtrip() {
        let payload = FilePayload {
            hdrop_b64: Some("aGk=".into()),
            extra: vec![FormatBlob {
                name: "Preferred DropEffect".into(),
                b64: "AQAAAA==".into(),
            }],
            staged: vec![StagedFile {
                original: "C:\\a.txt".into(),
                staged: "D:\\staged\\a.txt".into(),
                size: 3,
            }],
        };
        let json = encode_payload(&payload);
        assert_eq!(decode_payload(Some(&json)), Some(payload));
    }

    #[test]
    fn parse_file_group_descriptor_a_extracts_name() {
        let mut data = vec![1u8, 0, 0, 0];
        data.resize(4 + FILEDESCRIPTORA_SIZE, 0);
        let name = b"legacy.txt";
        let name_start = 4 + FILEDESCRIPTORA_NAME_OFFSET;
        data[name_start..name_start + name.len()].copy_from_slice(name);

        let names = parse_file_group_descriptor_a(&data);
        assert_eq!(names, vec!["legacy.txt".to_string()]);
    }

    #[test]
    fn parse_file_group_descriptor_w_extracts_name() {
        let mut data = vec![1u8, 0, 0, 0]; // count = 1
        data.resize(4 + FILEDESCRIPTORW_SIZE, 0);
        let name = "photo.png";
        let name_bytes: Vec<u8> = name
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .chain(std::iter::once(0).flat_map(|_| [0u8, 0u8]))
            .collect();
        let name_start = 4 + FILEDESCRIPTORW_NAME_OFFSET;
        data[name_start..name_start + name_bytes.len()].copy_from_slice(&name_bytes);

        let names = parse_file_group_descriptor_w(&data);
        assert_eq!(names, vec!["photo.png".to_string()]);
    }

    #[test]
    fn file_entry_preview_uses_descriptor_when_paths_empty() {
        let mut data = vec![1u8, 0, 0, 0];
        data.resize(4 + FILEDESCRIPTORW_SIZE, 0);
        let name = "report.docx";
        let name_bytes: Vec<u8> = name
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .chain(std::iter::once(0).flat_map(|_| [0u8, 0u8]))
            .collect();
        let name_start = 4 + FILEDESCRIPTORW_NAME_OFFSET;
        data[name_start..name_start + name_bytes.len()].copy_from_slice(&name_bytes);

        let extra = vec![("FileGroupDescriptorW".into(), data)];
        assert_eq!(file_entry_preview(&[], &extra), "report.docx");
    }

    #[test]
    fn effective_file_paths_prefers_real_paths() {
        let capture = FileCaptureData {
            paths: vec!["C:\\a.txt".into()],
            extra_formats: vec![("FileGroupDescriptorW".into(), vec![1, 0, 0, 0])],
            hdrop_raw: None,
        };
        assert_eq!(effective_file_paths(&capture), vec!["C:\\a.txt".to_string()]);
    }

    #[test]
    fn merge_paths_dedupes() {
        let items = [
            (Some(r#"["C:\\a.txt","C:\\b.txt"]"#), None as Option<&str>),
            (Some(r#"["C:\\b.txt","C:\\c.txt"]"#), None),
        ];
        let merged = merge_file_paths(&items);
        assert_eq!(
            merged,
            vec![
                "C:\\a.txt".to_string(),
                "C:\\b.txt".to_string(),
                "C:\\c.txt".to_string()
            ]
        );
    }

    #[test]
    fn should_use_raw_hdrop_only_when_originals_exist() {
        let payload = FilePayload {
            hdrop_b64: Some("aGk=".into()),
            ..Default::default()
        };
        assert!(!should_use_raw_hdrop(
            &["C:\\__ec_test_missing__\\nope.txt".to_string()],
            Some(&payload)
        ));

        let temp = std::env::temp_dir().join(format!("ec_hdrop_test_{}.txt", std::process::id()));
        std::fs::write(&temp, b"1").unwrap();
        let path = temp.to_string_lossy().to_string();
        assert!(should_use_raw_hdrop(&[path], Some(&payload)));
        let _ = std::fs::remove_file(temp);
    }

    #[test]
    fn resolve_uses_staged_when_missing() {
        let payload = FilePayload {
            staged: vec![StagedFile {
                original: "C:\\missing.txt".into(),
                staged: "D:\\staged\\missing.txt".into(),
                size: 1,
            }],
            ..Default::default()
        };
        let resolved = resolve_paths(&["C:\\missing.txt".to_string()], Some(&payload));
        assert_eq!(resolved, vec!["D:\\staged\\missing.txt".to_string()]);
    }

    #[test]
    fn staged_paths_from_payload_extracts_paths() {
        let payload = FilePayload {
            staged: vec![
                StagedFile {
                    original: "C:\\a.txt".into(),
                    staged: "D:\\staged\\a.txt".into(),
                    size: 1,
                },
                StagedFile {
                    original: "C:\\b.txt".into(),
                    staged: "D:\\staged\\b.txt".into(),
                    size: 2,
                },
            ],
            ..Default::default()
        };
        let json = encode_payload(&payload);
        assert_eq!(
            staged_paths_from_payload(Some(&json)),
            vec![
                "D:\\staged\\a.txt".to_string(),
                "D:\\staged\\b.txt".to_string()
            ]
        );
    }
}
