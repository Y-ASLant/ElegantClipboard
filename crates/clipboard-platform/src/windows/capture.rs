use super::*;

struct ClipboardSnapshot {
    content: Option<CapturedClipboard>,
    source: Option<source_app::SourceApp>,
    warning: Option<&'static str>,
}

fn read_stable_capture(
    state: &CaptureState,
    mut sequence: impl FnMut() -> u32,
    mut read: impl FnMut() -> Result<ClipboardSnapshot>,
) -> Result<Option<(u32, ClipboardSnapshot)>> {
    for _ in 0..4 {
        let before = sequence();
        if before == 0 {
            bail!("无法确认剪贴板序列号，本次复制未保存");
        }
        if state.paused || before == state.ignored_sequence || before == state.last_sequence {
            return Ok(None);
        }
        let snapshot = read();
        if sequence() == before {
            return snapshot.map(|snapshot| Some((before, snapshot)));
        }
        thread::yield_now();
    }
    bail!("读取过程中剪贴板持续变化，本次复制未保存，请重新复制")
}

impl WatchHandler {
    pub(super) fn read_change(&mut self) -> Result<()> {
        // Keep our own writes serialized with the entire snapshot, including retries.
        let mut state = lock_capture_state(&self.state)?;
        let Some((sequence, snapshot)) = read_stable_capture(
            &state,
            || unsafe { GetClipboardSequenceNumber() },
            || read_clipboard(&self.clipboard),
        )?
        else {
            return Ok(());
        };
        if let Some(content) = snapshot.content {
            self.commands.try_send(Command::ObservedCapture {
                content,
                source: snapshot.source,
            })?;
        }
        state.last_sequence = sequence;
        drop(state);
        if let Some(warning) = snapshot.warning {
            self.feedback.status(warning.into());
        }
        Ok(())
    }
}

fn read_clipboard(clipboard: &ClipboardContext) -> Result<ClipboardSnapshot> {
    let source = source_app::clipboard_source();
    let (content, warning) = read_content(clipboard)?;
    Ok(ClipboardSnapshot {
        content,
        source,
        warning,
    })
}

fn read_content(
    clipboard: &ClipboardContext,
) -> Result<(Option<CapturedClipboard>, Option<&'static str>)> {
    if clipboard.has(ContentFormat::Files) {
        let paths = clipboard
            .get_files()
            .map_err(|error| anyhow!("读取剪贴板文件失败：{error}"))?;
        if paths.is_empty()
            || paths.len() > MAX_FILE_PATHS
            || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_LIST_BYTES
        {
            bail!("文件路径列表为空或超过限制，本次复制未保存");
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
            bail!("富文本超过 1 MiB 且没有纯文本，未保存");
        }
        warning = Some("富文本超过 1 MiB，按纯文本保存");
    }
    if let Some(text) = text {
        if text.len() > MAX_TEXT_BYTES {
            bail!("文本超过 1 MiB，未保存");
        }
        return Ok((Some(CapturedClipboard::Text(text)), warning));
    }
    if clipboard.has(ContentFormat::Image) {
        let image = clipboard
            .get_image()
            .map_err(|error| anyhow!("读取剪贴板图片失败：{error}"))?;
        let (width, height) = image.get_size();
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
            bail!("图片尺寸无效或超过 2500 万像素");
        }
        let png = image
            .to_png()
            .map_err(|error| anyhow!("编码剪贴板图片失败：{error}"))?;
        if png.get_bytes().len() > MAX_IMAGE_BYTES {
            bail!("图片超过 50 MiB，未保存");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rich_snapshot(label: &str) -> ClipboardSnapshot {
        ClipboardSnapshot {
            content: Some(CapturedClipboard::Rich {
                html: Some(format!("<b>{label}</b>")),
                rtf: Some(format!("{{\\rtf1 {label}}}").into_bytes()),
                text: Some(label.into()),
            }),
            source: Some(source_app::SourceApp {
                name: label.into(),
                executable: None,
            }),
            warning: None,
        }
    }

    #[test]
    fn changed_clipboard_retries_all_formats_and_source_together() -> Result<()> {
        let mut sequences = [1, 2, 2, 2].into_iter();
        let mut reads = 0;
        let (sequence, snapshot) = read_stable_capture(
            &CaptureState::default(),
            || sequences.next().unwrap(),
            || {
                reads += 1;
                Ok(rich_snapshot(if reads == 1 { "mixed" } else { "stable" }))
            },
        )?
        .unwrap();
        assert_eq!(reads, 2);
        assert_eq!(sequence, 2);
        assert_eq!(snapshot.source.unwrap().name, "stable");
        let Some(CapturedClipboard::Rich { html, rtf, text }) = snapshot.content else {
            panic!("expected rich snapshot");
        };
        assert_eq!(html.as_deref(), Some("<b>stable</b>"));
        assert_eq!(rtf.as_deref(), Some(b"{\\rtf1 stable}".as_slice()));
        assert_eq!(text.as_deref(), Some("stable"));
        Ok(())
    }

    #[test]
    fn changed_clipboard_retries_a_read_error_but_stable_errors_are_reported() -> Result<()> {
        let mut sequences = [1, 2, 2, 2].into_iter();
        let mut reads = 0;
        assert!(
            read_stable_capture(
                &CaptureState::default(),
                || sequences.next().unwrap(),
                || {
                    reads += 1;
                    if reads == 1 {
                        bail!("old clipboard disappeared");
                    }
                    Ok(rich_snapshot("new"))
                },
            )?
            .is_some()
        );
        let result = read_stable_capture(
            &CaptureState::default(),
            || 3,
            || bail!("stable read failure"),
        );
        assert_eq!(result.err().unwrap().to_string(), "stable read failure");
        Ok(())
    }

    #[test]
    fn continuously_changing_clipboard_is_rejected_after_bounded_retries() {
        let mut sequence = 0;
        let mut reads = 0;
        let result = read_stable_capture(
            &CaptureState::default(),
            || {
                sequence += 1;
                sequence
            },
            || {
                reads += 1;
                Ok(rich_snapshot("unstable"))
            },
        );
        assert!(result.is_err());
        assert_eq!(reads, 4);
    }

    #[test]
    fn paused_duplicate_self_written_and_unknown_sequences_are_not_read() {
        for state in [
            CaptureState {
                paused: true,
                ..Default::default()
            },
            CaptureState {
                last_sequence: 7,
                ..Default::default()
            },
            CaptureState {
                ignored_sequence: 7,
                ..Default::default()
            },
        ] {
            assert!(
                read_stable_capture(&state, || 7, || panic!("must not read"))
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            read_stable_capture(&CaptureState::default(), || 0, || panic!("must not read"))
                .is_err()
        );
    }
}
