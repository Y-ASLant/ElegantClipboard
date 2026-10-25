use super::CapturedClipboard;
use crate::source_app;
use anyhow::Result;
use clipboard_core::{
    MAX_FILE_PATHS, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_NATIVE_FORMAT_BYTES,
    MAX_NATIVE_FORMAT_TOTAL_BYTES, MAX_PATH_LIST_BYTES, MAX_TEXT_BYTES, NativeFormatCapture,
};
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat, common::RustImage};

/// 一次监听线程读取的完整快照：主内容、来源应用与额外原生格式。
pub(super) struct ClipboardSnapshot {
    pub(super) content: Option<CapturedClipboard>,
    pub(super) source: Option<source_app::SourceApp>,
    pub(super) warning: Option<&'static str>,
    pub(super) extra: Vec<NativeFormatCapture>,
}

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
pub(super) fn read_extra_formats(guard: &clipboard_win::Clipboard) -> Vec<NativeFormatCapture> {
    let _ = guard;
    let mut captures = Vec::new();
    let mut total = 0usize;
    for format in clipboard_win::EnumFormats::new() {
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

pub(super) fn read_clipboard(clipboard: &ClipboardContext) -> Result<ClipboardSnapshot> {
    if super::privacy::excludes_history(clipboard)? {
        return Ok(ClipboardSnapshot {
            content: None,
            source: None,
            warning: None,
            extra: Vec::new(),
        });
    }
    let source = source_app::clipboard_source();
    let (content, warning) = read_content(clipboard)?;
    // 主内容为 None（剪贴板只有不支持的私有格式）时快照会被丢弃，无需枚举额外格式。
    let extra = if content.is_some() {
        read_extra_in_session()
    } else {
        Vec::new()
    };
    Ok(ClipboardSnapshot {
        content,
        source,
        warning,
        extra,
    })
}

/// 额外格式枚举需要独立短会话；主读取（clipboard_rs）已关闭其会话，
/// 序列号稳定性由调用方的 read_stable_capture 保证。
fn read_extra_in_session() -> Vec<NativeFormatCapture> {
    let Ok(guard) = clipboard_win::Clipboard::new_attempts(10) else {
        return Vec::new();
    };
    let captures = read_extra_formats(&guard);
    drop(guard);
    captures
}

/// 瞬时确认窗口的重读入口：读取当前剪贴板的主内容（不含额外格式枚举——
/// 若内容与 pending 相同则直接交付 pending 携带的 extra，无需再枚举）。
pub(super) fn read_current_content(
    clipboard: &ClipboardContext,
) -> Result<Option<CapturedClipboard>> {
    if super::privacy::excludes_history(clipboard)? {
        return Ok(None);
    }
    Ok(read_content(clipboard)?.0)
}

fn read_content(
    clipboard: &ClipboardContext,
) -> Result<(Option<CapturedClipboard>, Option<&'static str>)> {
    if clipboard.has(ContentFormat::Files) {
        let paths = clipboard
            .get_files()
            .map_err(|error| anyhow::anyhow!("读取剪贴板文件失败：{error}"))?;
        if paths.is_empty()
            || paths.len() > MAX_FILE_PATHS
            || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_LIST_BYTES
        {
            anyhow::bail!("文件路径列表为空或超过限制，本次复制未保存");
        }
        return Ok((Some(CapturedClipboard::Files(paths)), None));
    }
    let html = clipboard
        .has(ContentFormat::Html)
        .then(|| clipboard.get_html().ok())
        .flatten()
        .filter(|value| !value.is_empty());
    let rtf = clipboard
        .has(ContentFormat::Rtf)
        .then(|| clipboard.get_buffer("Rich Text Format").ok())
        .flatten()
        .filter(|value| !value.is_empty());
    let text = clipboard
        .has(ContentFormat::Text)
        .then(|| clipboard.get_text().ok())
        .flatten()
        .filter(|value| !value.is_empty());
    let mut warning = None;
    if html.is_some() || rtf.is_some() {
        let bytes = html.as_ref().map_or(0, String::len)
            + rtf.as_ref().map_or(0, Vec::len)
            + text.as_ref().map_or(0, String::len);
        if bytes <= MAX_TEXT_BYTES {
            return Ok((Some(CapturedClipboard::Rich { html, rtf, text }), None));
        }
        if text.is_none() {
            anyhow::bail!("富文本超过 1 MiB 且没有纯文本，未保存");
        }
        warning = Some("富文本超过 1 MiB，按纯文本保存");
    }
    if let Some(text) = text {
        if text.len() > MAX_TEXT_BYTES {
            anyhow::bail!("文本超过 1 MiB，未保存");
        }
        return Ok((Some(CapturedClipboard::Text(text)), warning));
    }
    if clipboard.has(ContentFormat::Image) {
        let image = clipboard
            .get_image()
            .map_err(|error| anyhow::anyhow!("读取剪贴板图片失败：{error}"))?;
        let (width, height) = image.get_size();
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
            anyhow::bail!("图片尺寸无效或超过 2500 万像素");
        }
        let png = image
            .to_png()
            .map_err(|error| anyhow::anyhow!("编码剪贴板图片失败：{error}"))?;
        if png.get_bytes().len() > MAX_IMAGE_BYTES {
            anyhow::bail!("图片超过 50 MiB，未保存");
        }
        return Ok((
            Some(CapturedClipboard::Image {
                png: png.get_bytes().to_vec(),
                width,
                height,
            }),
            None,
        ));
    }
    Ok((None, None))
}
