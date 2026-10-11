use super::{CapturedClipboard, Command, Event};
use anyhow::{Result, bail};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};

const MAX_PENDING_COMMANDS: usize = 64;
const MAX_PENDING_CAPTURES: usize = 1024;
const MAX_PENDING_CAPTURE_BYTES: usize = 100 * 1024 * 1024;

#[derive(Default)]
struct QueueState {
    commands: VecDeque<Command>,
    pending_commands: usize,
    pending_captures: usize,
    capture_bytes: usize,
    closed: bool,
}

pub(super) struct CommandQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
    capture_limit: usize,
    byte_limit: usize,
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self {
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
            capture_limit: MAX_PENDING_CAPTURES,
            byte_limit: MAX_PENDING_CAPTURE_BYTES,
        }
    }
}

fn capture_bytes(command: &Command) -> Option<usize> {
    let Command::ObservedCapture { content, source } = command else {
        return None;
    };
    let bytes = match content {
        CapturedClipboard::Text(text) => text.capacity(),
        CapturedClipboard::Rich { html, rtf, text } => {
            html.as_ref().map_or(0, String::capacity)
                + rtf.as_ref().map_or(0, Vec::capacity)
                + text.as_ref().map_or(0, String::capacity)
        }
        CapturedClipboard::Files(paths) => {
            paths.capacity() * size_of::<String>()
                + paths.iter().map(String::capacity).sum::<usize>()
        }
        CapturedClipboard::Image { png, .. } => png.capacity(),
    };
    Some(
        bytes
            + source.as_ref().map_or(0, |source| {
                source.name.capacity() + source.executable.as_ref().map_or(0, String::capacity)
            }),
    )
}

// Retain the reservation while the worker owns the payload, not just while queued.
pub(super) struct CommandPermit {
    queue: Arc<CommandQueue>,
    capture_bytes: Option<usize>,
}

impl Drop for CommandPermit {
    fn drop(&mut self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(bytes) = self.capture_bytes {
            state.pending_captures -= 1;
            state.capture_bytes -= bytes;
        } else {
            state.pending_commands -= 1;
        }
    }
}

impl CommandQueue {
    pub(super) fn try_send(&self, command: Command) -> Result<()> {
        let bytes = capture_bytes(&command);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            bail!("后台任务已停止");
        }
        if let Some(bytes) = bytes {
            if state.pending_captures >= self.capture_limit
                || state.capture_bytes.saturating_add(bytes) > self.byte_limit
            {
                bail!("待保存的剪贴板内容达到数量或内存上限，本次复制未保存，请稍后重新复制");
            }
            state.pending_captures += 1;
            state.capture_bytes += bytes;
        } else {
            if state.pending_commands >= MAX_PENDING_COMMANDS {
                bail!("后台任务繁忙，请稍后重试");
            }
            state.pending_commands += 1;
        }
        state.commands.push_back(command);
        self.ready.notify_one();
        Ok(())
    }

    pub(super) fn recv_timeout(
        self: &Arc<Self>,
        timeout: Duration,
    ) -> std::result::Result<(Command, CommandPermit), mpsc::RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(command) = state.commands.pop_front() {
                let permit = CommandPermit {
                    queue: self.clone(),
                    capture_bytes: capture_bytes(&command),
                };
                return Ok((command, permit));
            }
            if state.closed {
                return Err(mpsc::RecvTimeoutError::Disconnected);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
            (state, _) = self
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub(super) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        self.ready.notify_all();
    }
}

#[derive(Default)]
struct PendingFeedback {
    error_count: u64,
    last_error: Option<String>,
    status: Option<String>,
}

#[derive(Default)]
pub(super) struct CaptureFeedback(Mutex<PendingFeedback>);

impl CaptureFeedback {
    pub(super) fn error(&self, message: String) {
        let mut pending = self.0.lock().unwrap_or_else(|error| error.into_inner());
        pending.error_count = pending.error_count.saturating_add(1);
        pending.last_error = Some(message);
    }

    pub(super) fn status(&self, message: String) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .status = Some(message);
    }

    pub(super) fn flush(&self, events: &async_channel::Sender<Event>) {
        let mut pending = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(message) = &pending.last_error {
            let mut message = if pending.error_count == 1 {
                message.clone()
            } else {
                format!(
                    "剪贴板采集出现 {} 次问题；最近一次：{message}",
                    pending.error_count
                )
            };
            if let Some(status) = &pending.status {
                message.push('\n');
                message.push_str(status);
            }
            if events.try_send(Event::BackgroundError(message)).is_err() {
                return;
            }
            pending.last_error = None;
            pending.error_count = 0;
            pending.status = None;
            return;
        }
        if let Some(message) = pending.status.take()
            && let Err(async_channel::TrySendError::Full(Event::Status(message))) =
                events.try_send(Event::Status(message))
        {
            pending.status = Some(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::preferences::ThemePreference;

    fn capture(text: &str) -> Command {
        Command::ObservedCapture {
            content: CapturedClipboard::Text(text.into()),
            source: None,
        }
    }

    #[test]
    fn full_ui_quota_preserves_capture_capacity_and_fifo_order() -> Result<()> {
        let queue = Arc::new(CommandQueue::default());
        for _ in 0..MAX_PENDING_COMMANDS {
            queue.try_send(Command::SetTheme(ThemePreference::Dark))?;
        }
        assert!(queue.try_send(Command::Pause(true)).is_err());
        for i in 0..128 {
            queue.try_send(capture(&i.to_string()))?;
        }
        for _ in 0..MAX_PENDING_COMMANDS {
            let (command, _permit) = queue.recv_timeout(Duration::ZERO)?;
            assert!(matches!(command, Command::SetTheme(_)));
        }
        for i in 0..128 {
            let (command, _permit) = queue.recv_timeout(Duration::ZERO)?;
            assert!(matches!(command, Command::ObservedCapture {
                content: CapturedClipboard::Text(text), ..
            } if text == i.to_string()));
        }
        Ok(())
    }

    #[test]
    fn capture_limits_include_in_flight_payload_and_release_after_failure() -> Result<()> {
        let queue = Arc::new(CommandQueue {
            capture_limit: 2,
            byte_limit: 8,
            ..Default::default()
        });
        queue.try_send(capture("12345678"))?;
        let (command, permit) = queue.recv_timeout(Duration::ZERO)?;
        assert!(queue.try_send(capture("x")).is_err());
        drop(command);
        drop(permit);
        queue.try_send(capture("a"))?;
        queue.try_send(capture("b"))?;
        assert!(queue.try_send(capture("c")).is_err());
        assert!(queue.try_send(Command::Pause(true)).is_ok());
        Ok(())
    }

    #[test]
    fn closed_queue_rejects_new_work_but_drains_accepted_commands() -> Result<()> {
        let queue = Arc::new(CommandQueue::default());
        queue.try_send(capture("keep"))?;
        queue.close();
        assert!(queue.try_send(capture("reject")).is_err());
        assert!(matches!(
            queue.recv_timeout(Duration::ZERO)?.0,
            Command::ObservedCapture { .. }
        ));
        assert!(matches!(
            queue.recv_timeout(Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        Ok(())
    }

    #[test]
    fn capture_errors_and_warnings_survive_a_full_event_queue() -> Result<()> {
        let (events, incoming) = async_channel::bounded(1);
        events.try_send(Event::ShowWindow).ok().unwrap();
        let feedback = CaptureFeedback::default();
        feedback.error("first".into());
        feedback.error("last".into());
        feedback.status("plain-text fallback".into());
        feedback.flush(&events);
        assert!(matches!(incoming.try_recv()?, Event::ShowWindow));
        feedback.flush(&events);
        assert!(
            matches!(incoming.try_recv()?, Event::BackgroundError(message)
            if message.contains("2") && message.contains("last") && message.contains("plain-text fallback"))
        );
        feedback.flush(&events);
        assert!(
            incoming.is_empty(),
            "a stale status must not replace the error"
        );
        events.try_send(Event::ShowWindow).ok().unwrap();
        feedback.status("new warning".into());
        feedback.flush(&events);
        assert!(matches!(incoming.try_recv()?, Event::ShowWindow));
        feedback.flush(&events);
        assert!(matches!(incoming.try_recv()?, Event::Status(message) if message == "new warning"));
        Ok(())
    }
}
