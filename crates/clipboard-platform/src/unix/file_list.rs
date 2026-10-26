use anyhow::{Context, Result, anyhow, bail};
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
use std::path::Path;
use url::Url;

pub(super) fn read_files(clipboard: &ClipboardContext) -> Result<Vec<String>> {
    let bytes = clipboard
        .get_buffer("text/uri-list")
        .map_err(|error| anyhow!("读取剪贴板文件失败：{error}"))?;
    decode_files(&bytes)
}

fn decode_files(bytes: &[u8]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(bytes).context("文件 URI 列表不是 UTF-8")?;
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let uri = Url::parse(line).context("剪贴板包含无效文件 URI")?;
            let path = uri
                .to_file_path()
                .map_err(|_| anyhow!("剪贴板包含非本地文件 URI"))?;
            path.into_os_string()
                .into_string()
                .map_err(|_| anyhow!("剪贴板文件路径不是 UTF-8"))
        })
        .collect()
}

pub(super) fn encode_contents(contents: Vec<ClipboardContent>) -> Result<Vec<ClipboardContent>> {
    let has_text = contents
        .iter()
        .any(|content| matches!(content, ClipboardContent::Text(_)));
    let mut encoded = Vec::with_capacity(contents.len() + 3);
    for content in contents {
        match content {
            ClipboardContent::Files(files) => {
                if files.is_empty() {
                    bail!("文件列表为空");
                }
                let uris = files
                    .iter()
                    .map(|file| {
                        Url::from_file_path(Path::new(file))
                            .map(|url| url.to_string())
                            .map_err(|_| anyhow!("文件路径必须是绝对路径"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                encoded.push(ClipboardContent::Other(
                    "text/uri-list".into(),
                    uris.join("\r\n").into_bytes(),
                ));
                let copied = format!("copy\n{}", uris.join("\n")).into_bytes();
                encoded.push(ClipboardContent::Other(
                    "x-special/gnome-copied-files".into(),
                    copied.clone(),
                ));
                encoded.push(ClipboardContent::Other(
                    "x-special/nautilus-clipboard".into(),
                    copied,
                ));
                if !has_text {
                    encoded.push(ClipboardContent::Text(files.join("\n")));
                }
            }
            content => encoded.push(content),
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_roundtrip_preserves_reserved_characters_and_unicode() -> Result<()> {
        let files = vec!["/tmp/中文 space,%#?.txt".into(), "/tmp/line\nbreak".into()];
        let encoded = encode_contents(vec![ClipboardContent::Files(files.clone())])?;
        let bytes = encoded
            .iter()
            .find_map(|content| match content {
                ClipboardContent::Other(name, bytes) if name == "text/uri-list" => Some(bytes),
                _ => None,
            })
            .context("missing URI payload")?;
        assert_eq!(decode_files(bytes)?, files);
        assert_eq!(
            decode_files(b"# comment\r\nfile:///tmp/a%20b\r\n")?,
            ["/tmp/a b"]
        );
        assert!(decode_files(b"https://example.com/file").is_err());
        assert!(decode_files(b"file://remote-host/tmp/file").is_err());
        Ok(())
    }
}
