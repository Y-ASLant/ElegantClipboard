use super::CapturedClipboard;
use crate::source_app;
use anyhow::Result;
use clipboard_core::NativeFormatCapture;
use clipboard_core::preferences::CaptureFormatPreference;
use clipboard_core::{
    MAX_FILE_PATHS, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_PATH_LIST_BYTES, MAX_TEXT_BYTES,
};
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat, common::RustImage};
use std::{
    thread,
    time::{Duration, Instant},
};

const READ_RETRY_BUDGET: Duration = Duration::from_millis(250);
const READ_RETRY_DELAY: Duration = Duration::from_millis(20);

/// 重试完整读取；每次尝试都必须重新打开会话，并在返回前关闭。
/// Windows 的 5 表示打开时被占用，1418 表示读取时没有有效的打开会话。
/// 不在外层持有 clipboard_win guard：嵌套会话的关闭会释放系统剪贴板锁。
pub(crate) fn retry_clipboard_read<T, E: ToString>(
    mut read: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let deadline = Instant::now() + READ_RETRY_BUDGET;
    loop {
        match read() {
            Err(error) if is_retryable_read_error(&error) => {
                let now = Instant::now();
                if now >= deadline {
                    return Err(error);
                }
                thread::sleep(READ_RETRY_DELAY.min(deadline - now));
            }
            result => return result,
        }
    }
}

#[cfg(windows)]
fn is_retryable_read_error<E: ToString>(error: &E) -> bool {
    // clipboard-rs 将 Win32 错误转换成字符串，按完整错误码匹配，不依赖系统语言。
    let message = error.to_string();
    ["OSError(5)", "OSError(1418)"]
        .iter()
        .any(|code| message.contains(code))
}

#[cfg(not(windows))]
fn is_retryable_read_error<E: ToString>(_: &E) -> bool {
    false
}

#[cfg(windows)]
pub(crate) const RTF_FORMAT: &str = "Rich Text Format";
#[cfg(target_os = "linux")]
pub(crate) const RTF_FORMAT: &str = "text/rtf";
#[cfg(target_os = "macos")]
pub(crate) const RTF_FORMAT: &str = "public.rtf";
/// 一次监听线程读取的完整快照：主内容、来源应用与额外原生格式。
pub(super) struct ClipboardSnapshot {
    pub(super) content: Option<CapturedClipboard>,
    pub(super) source: Option<source_app::SourceApp>,
    pub(super) warning: Option<&'static str>,
    pub(super) extra: Vec<NativeFormatCapture>,
    pub(super) formats: Vec<String>,
}
pub(super) fn read_content(
    clipboard: &ClipboardContext,
    allowed: &CaptureFormatPreference,
    #[cfg(target_os = "linux")] formats: Option<&[String]>,
) -> Result<(Option<CapturedClipboard>, Option<&'static str>)> {
    if allowed.allows_files() && clipboard.has(ContentFormat::Files) {
        #[cfg(target_os = "linux")]
        let paths = super::file_list::read_files(clipboard)?;
        #[cfg(windows)]
        let paths = super::file_list::read_files()?;
        #[cfg(target_os = "macos")]
        let paths = retry_clipboard_read(|| clipboard.get_files())
            .map_err(|error| anyhow::anyhow!("读取剪贴板文件失败：{error}"))?;
        if paths.is_empty()
            || paths.len() > MAX_FILE_PATHS
            || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_LIST_BYTES
        {
            anyhow::bail!("文件路径列表为空或超过限制，本次复制未保存");
        }
        return Ok((Some(CapturedClipboard::Files(paths)), None));
    }
    let html = allowed
        .allows_html()
        .then(|| {
            clipboard
                .has(ContentFormat::Html)
                .then(|| super::read_html::read_html(clipboard).ok())
                .flatten()
                .filter(|value| !value.is_empty())
        })
        .flatten();
    let rtf = allowed
        .allows_rtf()
        .then(|| {
            clipboard
                .has(ContentFormat::Rtf)
                .then(|| retry_clipboard_read(|| clipboard.get_buffer(RTF_FORMAT)).ok())
                .flatten()
                .filter(|value| !value.is_empty())
        })
        .flatten();
    #[cfg(target_os = "linux")]
    let text = allowed
        .allows_text()
        .then(|| linux_text(clipboard, formats))
        .transpose()?
        .flatten();
    #[cfg(not(target_os = "linux"))]
    let text = allowed
        .allows_text()
        .then(|| {
            clipboard
                .has(ContentFormat::Text)
                .then(|| retry_clipboard_read(|| clipboard.get_text()).ok())
                .flatten()
                .filter(|value| !value.is_empty())
        })
        .flatten();
    if allowed.allows_image()
        && prefer_image(html.as_deref(), rtf.as_deref(), text.as_deref())
        && clipboard.has(ContentFormat::Image)
        && let Ok(image) = read_image(clipboard)
    {
        return Ok((Some(image), None));
    }
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
    if allowed.allows_image() && clipboard.has(ContentFormat::Image) {
        return Ok((Some(read_image(clipboard)?), None));
    }
    Ok((None, None))
}

fn prefer_image(html: Option<&str>, rtf: Option<&[u8]>, text: Option<&str>) -> bool {
    rtf.is_none()
        && text.is_none()
        && html.is_some_and(|html| {
            clipboard_core::html_text::is_image_only_html(html).unwrap_or(false)
        })
}

fn read_image(clipboard: &ClipboardContext) -> Result<CapturedClipboard> {
    let image = retry_clipboard_read(|| clipboard.get_image())
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
    Ok(CapturedClipboard::Image {
        png: png.get_bytes().to_vec(),
        width,
        height,
    })
}

#[cfg(target_os = "linux")]
fn linux_text(clipboard: &ClipboardContext, formats: Option<&[String]>) -> Result<Option<String>> {
    let fetched;
    let formats = match formats {
        Some(formats) => formats,
        None => {
            fetched = clipboard
                .available_formats()
                .map_err(|error| anyhow::anyhow!("读取文本格式失败：{error}"))?;
            &fetched
        }
    };
    for format in [
        "UTF8_STRING",
        "text/plain;charset=utf-8",
        "text/plain;charset=UTF-8",
        "text/plain",
        "STRING",
    ] {
        if !formats.iter().any(|available| available == format) {
            continue;
        }
        let bytes = clipboard
            .get_buffer(format)
            .map_err(|error| anyhow::anyhow!("读取剪贴板文本失败：{error}"))?;
        if bytes.len() > MAX_TEXT_BYTES {
            anyhow::bail!("文本超过 1 MiB，未保存");
        }
        let text = if format == "STRING" {
            bytes.into_iter().map(char::from).collect()
        } else {
            String::from_utf8(bytes)
                .map_err(|error| anyhow::anyhow!("剪贴板文本不是 UTF-8：{error}"))?
        };
        return Ok((!text.is_empty()).then_some(text));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::prefer_image;

    #[cfg(windows)]
    #[test]
    fn clipboard_read_recovers_from_access_denied_and_missing_session() {
        use super::retry_clipboard_read;

        for codes in [[5, 1418], [1418, 5]] {
            let mut calls = 0;
            let result = retry_clipboard_read(|| {
                let error = codes.get(calls).copied();
                calls += 1;
                match error {
                    Some(code) => Err(format!("Get files error, code = OSError({code})")),
                    None => Ok(7),
                }
            });
            assert_eq!(result, Ok(7));
            assert_eq!(calls, 3);
        }
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_read_stops_on_nonretryable_errors_even_after_a_retry() {
        use super::retry_clipboard_read;

        for error in ["invalid file list", "OSError(50)", "OSError(14180)"] {
            for retry_first in [false, true] {
                let mut calls = 0;
                let result: Result<(), &str> = retry_clipboard_read(|| {
                    calls += 1;
                    if retry_first && calls == 1 {
                        Err("Get files error, code = OSError(1418)")
                    } else {
                        Err(error)
                    }
                });
                assert_eq!(result, Err(error));
                assert_eq!(calls, if retry_first { 2 } else { 1 });
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn persistent_missing_session_returns_the_last_error_within_the_retry_budget() {
        use super::retry_clipboard_read;

        let mut calls = 0;
        let result: Result<(), String> = retry_clipboard_read(|| {
            calls += 1;
            Err(format!("attempt {calls}: OSError(1418)"))
        });
        assert!(calls > 1);
        assert_eq!(result, Err(format!("attempt {calls}: OSError(1418)")));
    }

    #[test]
    fn image_markup_preserves_pixels_without_displacing_document_text_or_rtf() {
        let html = Some("<html><body><img src='blob:image'></body></html>");
        assert!(prefer_image(html, None, None));
        assert!(!prefer_image(html, None, Some(" \n")));
        assert!(!prefer_image(html, None, Some("正文")));
        assert!(!prefer_image(html, Some(b"{\\rtf1 image}"), None));
        assert!(!prefer_image(Some("<p>正文<img src=x></p>"), None, None));
        assert!(!prefer_image(None, None, None));
    }
}
