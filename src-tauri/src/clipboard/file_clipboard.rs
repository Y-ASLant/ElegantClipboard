//! Windows 文件剪贴板：捕获/还原 CF_HDROP 与伴生格式，支持文件内容 staging。

use crate::operation_error::{OperationError, OperationErrorCode, readable_resource};
use base64::Engine;
use clipboard_rs::{Clipboard as ClipboardTrait, ClipboardContext, ContentFormat};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;

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

fn parse_file_resources(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
) -> Result<(Vec<String>, Option<FilePayload>), OperationError> {
    let paths = file_paths
        .filter(|raw| !raw.is_empty())
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| {
            OperationError::new(OperationErrorCode::InvalidContent, "invalid file path list")
        })?
        .unwrap_or_default();
    let payload = file_payload
        .filter(|raw| !raw.is_empty())
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| {
            OperationError::new(OperationErrorCode::InvalidContent, "invalid file payload")
        })?;
    Ok((paths, payload))
}

pub fn prepare_path_text(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
) -> Result<String, OperationError> {
    let (paths, payload) = parse_file_resources(file_paths, file_payload)?;
    let resolved = resolve_paths(&paths, payload.as_ref());
    if resolved.is_empty() || resolved.iter().any(|path| path.is_empty()) {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "no file path text",
        ));
    }
    Ok(resolved.join("\n"))
}

pub fn resolve_item_paths(file_paths: Option<&str>, file_payload: Option<&str>) -> Vec<String> {
    let paths = parse_file_paths(file_paths);
    let payload = decode_payload(file_payload);
    resolve_paths(&paths, payload.as_ref())
}

pub fn file_status_resources(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
) -> (Vec<String>, Vec<String>, bool) {
    let (originals, payload) = match parse_file_resources(file_paths, file_payload) {
        Ok(resources) => resources,
        Err(_) => {
            let originals = parse_file_paths(file_paths);
            let resolved = originals.clone();
            return (originals, resolved, false);
        }
    };
    let mut descriptors = Vec::new();
    let mut has_contents = false;
    let mut formats_usable = true;
    if let Some(payload) = &payload {
        for extra in &payload.extra {
            if extra.name.is_empty() || extra.b64.is_empty() {
                formats_usable = false;
                break;
            }
            if extra.name == "FileGroupDescriptorW" || extra.name == "FileGroupDescriptor" {
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&extra.b64) else {
                    formats_usable = false;
                    break;
                };
                if !valid_descriptor(&extra.name, &bytes) {
                    formats_usable = false;
                    break;
                }
                descriptors.push((extra.name.clone(), bytes));
            } else {
                // Validate content without allocating or copying its decoded bytes.
                let mut decoder = base64::read::DecoderReader::new(
                    extra.b64.as_bytes(),
                    &base64::engine::general_purpose::STANDARD,
                );
                if std::io::copy(&mut decoder, &mut std::io::sink()).is_err() {
                    formats_usable = false;
                    break;
                }
                has_contents |= extra.name == "FileContents";
            }
        }
    }
    let names = descriptor_file_names(&descriptors);
    if formats_usable && usable_virtual_files(&originals, &names, has_contents, payload.as_ref()) {
        return (originals, Vec::new(), true);
    }
    let staged_originals;
    let paths = if originals.is_empty() {
        staged_originals = payload
            .as_ref()
            .map(|payload| {
                payload
                    .staged
                    .iter()
                    .map(|entry| entry.staged.clone())
                    .collect()
            })
            .unwrap_or_default();
        &staged_originals
    } else {
        &originals
    };
    let resolved = resolve_paths(paths, payload.as_ref());
    let mut usable =
        formats_usable && !resolved.is_empty() && resolved.iter().all(|path| !path.is_empty());
    if !originals.is_empty()
        && paths == &resolved
        && let Some(raw) = payload
            .as_ref()
            .and_then(|payload| payload.hdrop_b64.as_ref())
    {
        usable &= base64::engine::general_purpose::STANDARD
            .decode(raw)
            .is_ok_and(|raw| {
                valid_hdrop(&raw)
                    && (raw[16..20] == [0, 0, 0, 0] || hdrop_matches_paths(&raw, paths))
            });
    }
    (originals, resolved, usable)
}

fn valid_descriptor(name: &str, bytes: &[u8]) -> bool {
    let count = bytes
        .get(..4)
        .map(|count| u32::from_le_bytes(count.try_into().expect("four bytes")) as usize)
        .unwrap_or(0);
    let size = if name == "FileGroupDescriptorW" {
        FILEDESCRIPTORW_SIZE
    } else {
        FILEDESCRIPTORA_SIZE
    };
    count > 0 && count <= bytes.len().saturating_sub(4) / size
}

fn usable_virtual_files(
    paths: &[String],
    names: &[String],
    has_contents: bool,
    payload: Option<&FilePayload>,
) -> bool {
    !names.is_empty()
        && has_contents
        && (paths.is_empty() || paths == names)
        && payload.is_none_or(|payload| payload.staged.is_empty())
}

fn valid_hdrop(raw: &[u8]) -> bool {
    let offset = raw
        .get(..4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")) as usize)
        .unwrap_or(0);
    let wide = raw
        .get(16..20)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")) != 0)
        .unwrap_or(false);
    let terminated = if wide {
        raw.len() >= 4
            && raw.ends_with(&[0, 0, 0, 0])
            && (raw.len() - offset.min(raw.len())).is_multiple_of(2)
    } else {
        raw.ends_with(&[0, 0])
    };
    offset >= 20 && offset < raw.len() && terminated
}

fn hdrop_matches_paths(raw: &[u8], paths: &[String]) -> bool {
    if !valid_hdrop(raw) {
        return false;
    }
    let offset = u32::from_le_bytes(raw[..4].try_into().expect("valid header")) as usize;
    let mut remaining = &raw[offset..];
    for path in paths {
        let units = remaining.as_chunks::<2>().0;
        let Some(end) = units.iter().position(|unit| *unit == [0, 0]) else {
            return false;
        };
        if !path
            .encode_utf16()
            .eq(units[..end].iter().map(|unit| u16::from_le_bytes(*unit)))
        {
            return false;
        }
        remaining = &remaining[end * 2 + 2..];
    }
    !remaining.is_empty() && remaining.iter().all(|byte| *byte == 0)
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

pub struct PreparedFiles {
    pub paths: Vec<String>,
    pub raw_hdrop: Option<Vec<u8>>,
    pub extras: Vec<(String, Vec<u8>)>,
}

pub fn prepare_resource_paths(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
) -> Result<Vec<String>, OperationError> {
    let (paths, payload) = parse_file_resources(file_paths, file_payload)?;
    if paths.is_empty()
        && payload.as_ref().is_some_and(|payload| {
            payload.staged.is_empty()
                && payload
                    .extra
                    .iter()
                    .any(|format| format.name == "FileContents")
        })
    {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "virtual file has no disk path",
        ));
    }
    resolve_readable_paths(paths, payload.as_ref()).map(|(paths, _)| paths)
}

fn resolve_readable_paths(
    mut paths: Vec<String>,
    payload: Option<&FilePayload>,
) -> Result<(Vec<String>, bool), OperationError> {
    let mut all_originals = true;
    if paths.is_empty() {
        paths = payload
            .map(|payload| {
                payload
                    .staged
                    .iter()
                    .map(|entry| entry.staged.clone())
                    .collect()
            })
            .unwrap_or_default();
        all_originals = false;
    }
    if paths.is_empty() {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "file item has no disk resources",
        ));
    }
    for path in &mut paths {
        if path.is_empty() {
            return Err(OperationError::new(
                OperationErrorCode::InvalidContent,
                "empty source path",
            ));
        }
        if let Err(error) = readable_resource(Path::new(path)) {
            let staged = payload
                .and_then(|payload| payload.staged.iter().find(|entry| entry.original == *path));
            let Some(staged) = staged else {
                return Err(error);
            };
            readable_resource(Path::new(&staged.staged))?;
            all_originals = false;
            *path = staged.staged.clone();
        }
    }
    Ok((paths, all_originals))
}

pub fn prepare_files(
    file_paths: Option<&str>,
    file_payload: Option<&str>,
) -> Result<PreparedFiles, OperationError> {
    let (paths, payload) = parse_file_resources(file_paths, file_payload)?;
    let mut extras = Vec::new();
    if let Some(payload) = &payload {
        for extra in &payload.extra {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&extra.b64)
                .map_err(|_| {
                    OperationError::new(
                        OperationErrorCode::InvalidContent,
                        "invalid file format encoding",
                    )
                })?;
            if extra.name.is_empty() {
                return Err(OperationError::new(
                    OperationErrorCode::InvalidContent,
                    "empty file format name",
                ));
            }
            if bytes.is_empty() {
                return Err(OperationError::new(
                    OperationErrorCode::UnsupportedContent,
                    "zero-byte native file formats cannot be published",
                ));
            }
            if (extra.name == "FileGroupDescriptorW" || extra.name == "FileGroupDescriptor")
                && !valid_descriptor(&extra.name, &bytes)
            {
                return Err(OperationError::new(
                    OperationErrorCode::InvalidContent,
                    "truncated virtual file descriptor",
                ));
            }
            extras.push((extra.name.clone(), bytes));
        }
    }
    let names = descriptor_file_names(&extras);
    let virtual_files = usable_virtual_files(
        &paths,
        &names,
        extras.iter().any(|(name, _)| name == "FileContents"),
        payload.as_ref(),
    );
    if virtual_files {
        return Ok(PreparedFiles {
            paths: Vec::new(),
            raw_hdrop: None,
            extras,
        });
    }
    let (resolved, all_originals) = resolve_readable_paths(paths, payload.as_ref())?;
    let mut raw_hdrop = if all_originals {
        payload
            .as_ref()
            .and_then(|payload| payload.hdrop_b64.as_ref())
            .map(|raw| {
                base64::engine::general_purpose::STANDARD
                    .decode(raw)
                    .map_err(|_| {
                        OperationError::new(
                            OperationErrorCode::InvalidContent,
                            "invalid HDROP encoding",
                        )
                    })
            })
            .transpose()?
    } else {
        None
    };
    if raw_hdrop.as_ref().is_some_and(Vec::is_empty) {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "zero-byte native HDROP cannot be published",
        ));
    }
    if raw_hdrop.as_ref().is_some_and(|raw| !valid_hdrop(raw)) {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "invalid HDROP data",
        ));
    }
    if let Some(raw) = &raw_hdrop {
        if raw[16..20] == [0, 0, 0, 0] {
            // Rebuild ANSI HDROP from authoritative Unicode paths, retaining companion formats.
            raw_hdrop = None;
        } else if !hdrop_matches_paths(raw, &resolved) {
            return Err(OperationError::new(
                OperationErrorCode::InvalidContent,
                "HDROP paths do not match source resources",
            ));
        }
    }
    Ok(PreparedFiles {
        paths: resolved,
        raw_hdrop,
        extras,
    })
}

impl PreparedFiles {
    pub fn write(self, ctx: &mut ClipboardContext) -> Result<(), OperationError> {
        if let Some(raw) = self.raw_hdrop {
            ctx.clear()
                .map_err(|error| OperationError::clipboard("clear files", error))?;
            ctx.set_hdrop_raw(&raw)
                .map_err(|error| OperationError::clipboard("write HDROP", error))?;
        } else if !self.paths.is_empty() {
            ctx.set_files(self.paths)
                .map_err(|error| OperationError::clipboard("write files", error))?;
        } else {
            ctx.clear()
                .map_err(|error| OperationError::clipboard("clear virtual files", error))?;
        }
        for (name, bytes) in self.extras {
            ctx.set_raw_no_clear(&name, &bytes).map_err(|error| {
                OperationError::clipboard(format_args!("write file format {name}"), error)
            })?;
        }
        Ok(())
    }
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
        assert_eq!(
            effective_file_paths(&capture),
            vec!["C:\\a.txt".to_string()]
        );
    }

    #[test]
    fn preparation_uses_staged_source_and_rejects_missing_member() {
        let staged =
            std::env::temp_dir().join(format!("ec_preflight_stage_{}.txt", std::process::id()));
        std::fs::write(&staged, b"source").unwrap();
        let original = staged
            .with_file_name("__ec_missing_original__.txt")
            .to_string_lossy()
            .into_owned();
        let payload = encode_payload(&FilePayload {
            hdrop_b64: Some("invalid raw should not be used for staged paths".into()),
            staged: vec![StagedFile {
                original: original.clone(),
                staged: staged.to_string_lossy().into_owned(),
                size: 6,
            }],
            ..Default::default()
        });
        let paths = serde_json::to_string(&vec![original.clone()]).unwrap();
        let prepared = prepare_files(Some(&paths), Some(&payload)).unwrap();
        assert_eq!(prepared.paths, vec![staged.to_string_lossy().into_owned()]);
        assert!(prepared.raw_hdrop.is_none());
        let multiple = serde_json::to_string(&vec![
            original,
            staged
                .with_file_name("__ec_missing_member__.txt")
                .to_string_lossy()
                .into_owned(),
        ])
        .unwrap();
        assert!(matches!(
            prepare_files(Some(&multiple), Some(&payload)),
            Err(OperationError {
                code: OperationErrorCode::ResourceMissing,
                ..
            })
        ));
        let disk_paths =
            serde_json::to_string(&vec![staged.to_string_lossy().into_owned()]).unwrap();
        let broken_formats = r#"{"extra":[{"name":"FileContents","b64":"!"}]}"#;
        assert!(prepare_files(Some(&disk_paths), Some(broken_formats)).is_err());
        assert!(prepare_resource_paths(Some(&disk_paths), Some(broken_formats)).is_ok());
        for empty_format in [
            r#"{"extra":[{"name":"Preferred DropEffect","b64":""}]}"#,
            r#"{"hdrop_b64":""}"#,
        ] {
            assert!(matches!(
                prepare_files(Some(&disk_paths), Some(empty_format)),
                Err(OperationError {
                    code: OperationErrorCode::UnsupportedContent,
                    ..
                })
            ));
            let (_, resolved, clipboard_available) =
                file_status_resources(Some(&disk_paths), Some(empty_format));
            assert!(!clipboard_available);
            assert_eq!(resolved, vec![staged.to_string_lossy().into_owned()]);
            assert!(prepare_resource_paths(Some(&disk_paths), Some(empty_format)).is_ok());
        }
        std::fs::remove_file(staged).unwrap();
    }

    #[test]
    fn preparation_preserves_virtual_empty_path_payload_and_rejects_broken_encoding() {
        let mut descriptor = vec![1, 0, 0, 0];
        descriptor.resize(4 + FILEDESCRIPTORW_SIZE, 0);
        let start = 4 + FILEDESCRIPTORW_NAME_OFFSET;
        descriptor[start..start + 2].copy_from_slice(&('a' as u16).to_le_bytes());
        let payload = encode_payload(&FilePayload {
            extra: vec![
                FormatBlob {
                    name: "FileGroupDescriptorW".into(),
                    b64: base64::engine::general_purpose::STANDARD.encode(descriptor),
                },
                FormatBlob {
                    name: "FileContents".into(),
                    b64: base64::engine::general_purpose::STANDARD.encode(b"contents"),
                },
            ],
            ..Default::default()
        });
        let prepared = prepare_files(Some("[]"), Some(&payload)).unwrap();
        assert!(prepared.paths.is_empty());
        assert_eq!(prepared.extras.len(), 2);
        assert!(file_status_resources(Some("[]"), Some(&payload)).2);
        let mut empty_contents = decode_payload(Some(&payload)).unwrap();
        empty_contents.extra[1].b64.clear();
        let empty_contents = encode_payload(&empty_contents);
        assert!(!file_status_resources(Some("[]"), Some(&empty_contents)).2);
        assert!(matches!(
            prepare_files(Some("[]"), Some(&empty_contents)),
            Err(OperationError {
                code: OperationErrorCode::UnsupportedContent,
                ..
            })
        ));
        let mut descriptor_only = decode_payload(Some(&payload)).unwrap();
        descriptor_only.extra.pop();
        let descriptor_only = encode_payload(&descriptor_only);
        assert!(!file_status_resources(Some("[]"), Some(&descriptor_only)).2);
        assert!(prepare_files(Some("[]"), Some(&descriptor_only)).is_err());
        assert!(
            prepare_files(
                Some("[]"),
                Some(r#"{"extra":[{"name":"FileContents","b64":"!"}]}"#)
            )
            .is_err()
        );
        assert!(prepare_files(Some("[]"), None).is_err());
        assert!(prepare_files(Some("not json"), None).is_err());
    }

    #[test]
    fn path_text_does_not_require_original_files_to_exist() {
        let paths = r#"["C:\\__ec_path_text_missing__\\file.txt"]"#;
        assert_eq!(
            prepare_path_text(Some(paths), None).unwrap(),
            "C:\\__ec_path_text_missing__\\file.txt"
        );
        assert!(prepare_files(Some(paths), None).is_err());
        assert!(prepare_path_text(Some("[]"), None).is_err());
    }

    #[test]
    fn raw_hdrop_rejects_truncated_and_unterminated_data() {
        assert!(!valid_hdrop(&[0; 4]));
        let mut raw = vec![0; 24];
        raw[..4].copy_from_slice(&20u32.to_le_bytes());
        raw[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert!(valid_hdrop(&raw));
        raw[23] = 1;
        assert!(!valid_hdrop(&raw));
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
