use crate::service::read_content::RTF_FORMAT;
use anyhow::{Context, Result};
use clipboard_rs::ClipboardContent;

pub(super) fn rtf_for_copy(stored: Option<&str>) -> Result<ClipboardContent> {
    let stored = stored
        .filter(|value| !value.is_empty())
        .context("该记录没有 RTF 格式内容")?;
    let bytes =
        clipboard_core::rich::decode_rtf_for_clipboard(stored).context("RTF 内容解码失败")?;
    Ok(ClipboardContent::Other(RTF_FORMAT.into(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::History;

    #[test]
    fn single_format_rtf_copy_decodes_persisted_non_utf8_bytes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let original = b"{\\rtf1\\ansi \\bin2 \x81\xfe}";
        let id = history.capture_rich(None, Some(original), None, directory.path())?;
        let item = history.item(id)?;
        let ClipboardContent::Other(_, bytes) = rtf_for_copy(item.rtf_content.as_deref())? else {
            panic!("RTF copy must preserve its binary payload");
        };
        assert_eq!(bytes.strip_suffix(&[0]).unwrap(), original);
        Ok(())
    }

    #[test]
    fn single_format_rtf_copy_rejects_missing_and_corrupt_payloads() {
        for stored in [
            None,
            Some(""),
            Some("b64:!bad"),
            Some("b64:"),
            Some("{\\rtf1}"),
        ] {
            assert!(rtf_for_copy(stored).is_err());
        }
    }
}
