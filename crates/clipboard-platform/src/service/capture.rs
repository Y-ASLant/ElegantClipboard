use super::{
    Command, Event,
    queue::{CaptureFeedback, CommandQueue},
    read_content::ClipboardSnapshot,
};
use crate::sequence::current_sequence;
use anyhow::{Result, anyhow, bail};
use clipboard_rs::{ClipboardContext, ClipboardHandler};
use std::{
    sync::{Arc, Mutex},
    thread,
};

#[derive(Default)]
pub(super) struct CaptureState {
    pub(super) paused: bool,
    pub(super) ignored_sequence: u32,
    pub(super) last_sequence: u32,
}

pub(super) fn lock_capture_state(
    state: &Mutex<CaptureState>,
) -> Result<std::sync::MutexGuard<'_, CaptureState>> {
    state.lock().map_err(|_| anyhow!("剪贴板状态异常"))
}

pub(super) struct WatchHandler {
    pub(super) clipboard: ClipboardContext,
    pub(super) state: Arc<Mutex<CaptureState>>,
    pub(super) commands: Arc<CommandQueue>,
    pub(super) events: async_channel::Sender<Event>,
    pub(super) feedback: Arc<CaptureFeedback>,
}

impl ClipboardHandler for WatchHandler {
    fn on_clipboard_change(&mut self) {
        if let Err(error) = self.read_change() {
            self.feedback.error(error.to_string());
        }
        self.feedback.flush(&self.events);
    }
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
        if state.paused {
            return Ok(());
        }
        let Some((sequence, snapshot)) = read_stable_capture(
            &state,
            || current_sequence(&self.clipboard).unwrap_or(0),
            || super::read_formats::read_clipboard(&self.clipboard),
        )?
        else {
            return Ok(());
        };
        if let Some(content) = snapshot.content {
            self.commands.try_send(Command::ObservedCapture {
                sequence,
                content,
                source: snapshot.source,
                extra: snapshot.extra,
                deferred: true,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapturedClipboard, source_app};

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
            extra: Vec::new(),
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
