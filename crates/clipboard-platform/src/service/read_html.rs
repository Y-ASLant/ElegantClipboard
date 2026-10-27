use anyhow::Result;
use clipboard_rs::{Clipboard, ClipboardContext};

pub(crate) fn read_html(clipboard: &ClipboardContext) -> Result<String> {
    #[cfg(windows)]
    {
        let bytes = clipboard
            .get_buffer("HTML Format")
            .map_err(|error| anyhow::anyhow!("读取剪贴板 HTML 失败：{error}"))?;
        parse_cf_html(&bytes)
    }
    #[cfg(not(windows))]
    {
        clipboard
            .get_html()
            .map_err(|error| anyhow::anyhow!("读取剪贴板 HTML 失败：{error}"))
    }
}

#[cfg(any(windows, test))]
fn parse_cf_html(bytes: &[u8]) -> Result<String> {
    use anyhow::{Context, bail};

    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    let mut start_html = None;
    let mut end_html = None;
    let mut start_fragment = None;
    let mut end_fragment = None;
    for line in bytes.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b"<") {
            break;
        }
        let Some(separator) = line.iter().position(|byte| *byte == b':') else {
            break;
        };
        let slot = match &line[..separator] {
            b"StartHTML" => &mut start_html,
            b"EndHTML" => &mut end_html,
            b"StartFragment" => &mut start_fragment,
            b"EndFragment" => &mut end_fragment,
            _ => continue,
        };
        if slot.is_some() {
            bail!("剪贴板 HTML 偏移字段重复");
        }
        let value = std::str::from_utf8(&line[separator + 1..])?
            .trim()
            .parse::<i64>()
            .context("剪贴板 HTML 偏移无效")?;
        *slot = Some(value);
    }
    let offsets = match (start_html, end_html) {
        (Some(start), Some(end)) if start >= 0 && end >= 0 => Some((start, end)),
        (None, None) if bytes.starts_with(b"<") => None,
        (Some(-1), Some(-1)) | (None, None) => Some((
            start_fragment.context("剪贴板 HTML 缺少起始偏移")?,
            end_fragment.context("剪贴板 HTML 缺少结束偏移")?,
        )),
        _ => bail!("剪贴板 HTML 起止偏移不完整"),
    };
    let html = if let Some((start, end)) = offsets {
        let start = usize::try_from(start).context("剪贴板 HTML 起始偏移无效")?;
        let end = usize::try_from(end).context("剪贴板 HTML 结束偏移无效")?;
        bytes.get(start..end).context("剪贴板 HTML 偏移超出范围")?
    } else {
        bytes
    };
    // CF_HTML 偏移以字节计数；在选定范围转换 UTF-8，拒绝落在字符中间的偏移。
    Ok(std::str::from_utf8(html)
        .context("剪贴板 HTML 偏移不在 UTF-8 字符边界")?
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::parse_cf_html;

    fn payload(html: &str) -> (Vec<u8>, usize) {
        let header = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\n";
        let start = header.len();
        let bytes = format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{:010}\r\n{html}\0",
            start + html.len()
        )
        .into_bytes();
        (bytes, start)
    }

    #[test]
    fn preserves_unicode_and_rejects_partial_characters_and_invalid_ranges() {
        let html = "<p>中文😀</p>";
        let (bytes, start) = payload(html);
        assert_eq!(parse_cf_html(&bytes).unwrap(), html);
        for (begin, end) in [
            (start + 4, start + html.len()),
            (start, start + 4),
            (start + 8, start + 3),
            (start, bytes.len() + 10),
        ] {
            let modified = String::from_utf8(bytes.clone())
                .unwrap()
                .replacen(
                    &format!("StartHTML:{start:010}"),
                    &format!("StartHTML:{begin:010}"),
                    1,
                )
                .replacen(
                    &format!("EndHTML:{:010}", start + html.len()),
                    &format!("EndHTML:{end:010}"),
                    1,
                );
            assert!(parse_cf_html(modified.as_bytes()).is_err());
        }
    }

    #[test]
    fn accepts_fragment_without_html_context_and_rejects_incomplete_headers() {
        let header = "Version:1.0\nStartHTML:-1\nEndHTML:-1\nStartFragment:0000000000\nEndFragment:0000000000\n";
        let start = header.len();
        let bytes = format!(
            "Version:1.0\nStartHTML:-1\nEndHTML:-1\nStartFragment:{start:010}\nEndFragment:{:010}\n<b>片段</b>",
            start + "<b>片段</b>".len()
        );
        assert_eq!(parse_cf_html(bytes.as_bytes()).unwrap(), "<b>片段</b>");
        assert_eq!(parse_cf_html(b"<p>plain</p>\0").unwrap(), "<p>plain</p>");
        for bytes in [
            b"StartHTML:0\n<p>missing end</p>".as_slice(),
            b"StartHTML:nope\nEndHTML:20\n".as_slice(),
            b"StartHTML:0\nStartHTML:0\nEndHTML:0\n".as_slice(),
        ] {
            assert!(parse_cf_html(bytes).is_err());
        }
    }
}
