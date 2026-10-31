use super::CapturedClipboard;
use super::read_content::{ClipboardSnapshot, read_content};
use crate::source_app;
use anyhow::Result;
use clipboard_core::{MAX_NATIVE_FORMAT_BYTES, MAX_NATIVE_FORMAT_TOTAL_BYTES, NativeFormatCapture};
use clipboard_rs::ClipboardContext;

/// 内部格式注册名黑名单：OLE 私有、Shell、浏览器内部与隐私标记格式。
/// 这些格式要么无法跨应用还原，要么会膨胀数据库，不参与保真采集。
const NATIVE_FORMAT_BLACKLIST: &[&str] = &[
    // 5 类高层格式已由主采集表示，不重复保存。
    "HTML Format",
    "Rich Text Format",
    "PNG",
    // 隐私标记（privacy.rs 已检查，绝不保存）。
    "CanIncludeInClipboardHistory",
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanUploadToCloudClipboard",
    // OLE/COM 结构化存储私有格式。
    "Ole Private Data",
    "Object Descriptor",
    "ObjectLink",
    "OwnerLink",
    "Link Source",
    "Link Source Descriptor",
    "Embedded Object",
    "Embed Source",
    "Native",
    "Link",
    // Shell/文件拖放私有数据（CF_HDROP 已由 Files 主采集表示）。
    "Shell IDList Array",
    "FileDrop",
    "FileName",
    "FileNameW",
    "DropDescription",
    "DropHandlerClipboardFormat",
    "DragContext",
    "DragImageBits",
    "InShellDragLoop",
    "Shell Object Offsets",
    // Chromium/浏览器内部格式。
    "Chromium internal source URL",
    "Chromium Web Custom MIME Data Format",
    "Chromium data flavor web custom format",
    // Firefox 内部格式。
    "application/x-moz-nativeimage",
    "application/x-moz-file",
    "text/x-moz-url-priv",
    // Office 挥发格式（很大且仅在本机 Office 内有意义）。
    "Artful Glk Paste",
    "Microsoft Word 8.0 Document",
    "Microsoft Word Document",
];

/// 标准 CF_* 常量格式 ID：EnumClipboardFormats 对它们的 GetClipboardFormatName
/// 返回值不可靠，黑名单按 ID 匹配。CF_HDROP(15)/CF_DIB(8)/CF_DIBV5(17)/CF_BITMAP(2)
/// 已由主采集（文件/图片）表示；文本类 1/7/13 由 Text 表示；其余为元数据或设备格式。
const BLACKLISTED_STANDARD_IDS: &[u32] = &[
    1,  // CF_TEXT
    2,  // CF_BITMAP
    3,  // CF_METAFILEPICT
    7,  // CF_OEMTEXT
    8,  // CF_DIB
    9,  // CF_PALETTE
    13, // CF_UNICODETEXT
    14, // CF_ENHMETAFILE
    15, // CF_HDROP
    16, // CF_LOCALE
    17, // CF_DIBV5
];

/// 在已打开的剪贴板会话内枚举并读取全部非黑名单格式。
/// 五类主格式已由 BLACKLISTED_STANDARD_IDS 与名称黑名单覆盖，无需额外的
/// consumed 名单——传入的 guard 仅用于满足枚举的「剪贴板已打开」前置条件。
// read_extra_formats 逻辑已合并到 enum_formats_once；此函数保留以备测试调用。
#[allow(dead_code)]
pub(super) fn read_extra_formats(guard: &clipboard_win::Clipboard) -> Vec<NativeFormatCapture> {
    let _ = guard;
    let (captures, _) = enum_formats_once();
    captures
}

fn format_name(format: u32) -> Option<String> {
    let mut buffer = [0u16; 256];
    // SAFETY: buffer 是有效的输出缓冲；格式必须已打开剪贴板后枚举。
    let length = unsafe {
        windows::Win32::System::DataExchange::GetClipboardFormatNameW(format, &mut buffer)
    };
    if length == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn is_blacklisted_name(name: &str) -> bool {
    NATIVE_FORMAT_BLACKLIST
        .iter()
        .any(|item| item.eq_ignore_ascii_case(name))
}

fn captured_format_name(format: u32) -> Option<String> {
    let standard = match format {
        1 => "CF_TEXT",
        2 => "CF_BITMAP",
        3 => "CF_METAFILEPICT",
        4 => "CF_SYLK",
        5 => "CF_DIF",
        6 => "CF_TIFF",
        7 => "CF_OEMTEXT",
        8 => "CF_DIB",
        9 => "CF_PALETTE",
        10 => "CF_PENDATA",
        11 => "CF_RIFF",
        12 => "CF_WAVE",
        13 => "CF_UNICODETEXT",
        14 => "CF_ENHMETAFILE",
        15 => "CF_HDROP",
        17 => "CF_DIBV5",
        0x0081 => "CF_DSPTEXT",
        0x0082 => "CF_DSPBITMAP",
        0x0083 => "CF_DSPMETAFILEPICT",
        0x008e => "CF_DSPENHMETAFILE",
        // Locale and owner-display are clipboard metadata, not content.
        16 | 0x0080 => return None,
        _ => {
            let name = format_name(format)?;
            return (!is_blacklisted_name(&name)
                || ["HTML Format", "Rich Text Format", "PNG"]
                    .iter()
                    .any(|primary| primary.eq_ignore_ascii_case(&name)))
            .then_some(name);
        }
    };
    Some(standard.into())
}

fn captured_formats(_guard: &clipboard_win::Clipboard) -> Vec<String> {
    clipboard_win::EnumFormats::new()
        .filter_map(captured_format_name)
        .collect()
}

pub(super) fn read_clipboard(clipboard: &ClipboardContext) -> Result<ClipboardSnapshot> {
    if super::privacy::excludes_history(clipboard)? {
        return Ok(ClipboardSnapshot {
            content: None,
            source: None,
            warning: None,
            extra: Vec::new(),
            formats: Vec::new(),
        });
    }
    let source = source_app::clipboard_source();
    let (content, warning) = read_content(clipboard)?;
    // 主内容为空时快照会被丢弃，无需枚举格式。
    let (extra, formats) = if content.is_some() {
        read_extra_in_session()
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(ClipboardSnapshot {
        content,
        source,
        warning,
        extra,
        formats,
    })
}

/// 额外格式枚举需要独立短会话；主读取（clipboard_rs）已关闭其会话，
/// 序列号稳定性由调用方的 read_stable_capture 保证。
fn read_extra_in_session() -> (Vec<NativeFormatCapture>, Vec<String>) {
    let Ok(guard) = clipboard_win::Clipboard::new_attempts(10) else {
        return (Vec::new(), Vec::new());
    };
    let _ = guard;
    let (captures, formats) = enum_formats_once();
    (captures, formats)
}

/// 单次枚举同时产出 captured_formats（显示名）与 read_extra_formats（载荷），
/// 避免 EnumFormats + GetClipboardFormatNameW 各跑两遍。
fn enum_formats_once() -> (Vec<NativeFormatCapture>, Vec<String>) {
    let mut formats = Vec::new();
    let mut captures = Vec::new();
    let mut total = 0usize;
    for format in clipboard_win::EnumFormats::new() {
        if let Some(name) = captured_format_name(format) {
            formats.push(name);
        }
        if BLACKLISTED_STANDARD_IDS.contains(&format) {
            continue;
        }
        let Some(name) = format_name(format) else {
            continue;
        };
        if is_blacklisted_name(&name) {
            continue;
        }
        let mut data = Vec::new();
        if clipboard_win::raw::get_vec(format, &mut data).is_err() {
            continue;
        }
        if data.is_empty()
            || data.len() > MAX_NATIVE_FORMAT_BYTES
            || total + data.len() > MAX_NATIVE_FORMAT_TOTAL_BYTES
        {
            continue;
        }
        total += data.len();
        captures.push(NativeFormatCapture { name, data });
    }
    (captures, formats)
}

/// 瞬时确认只重读主内容与实际格式名；额外载荷沿用原始快照。
pub(super) fn read_current_capture(
    clipboard: &ClipboardContext,
) -> Result<(Option<CapturedClipboard>, Vec<String>)> {
    if super::privacy::excludes_history(clipboard)? {
        return Ok((None, Vec::new()));
    }
    let content = read_content(clipboard)?.0;
    let formats = clipboard_win::Clipboard::new_attempts(10)
        .map(|guard| captured_formats(&guard))
        .unwrap_or_default();
    Ok((content, formats))
}
