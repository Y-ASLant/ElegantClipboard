use super::super::super::{CapturedClipboard, source_app};
use anyhow::Result;
use clipboard_core::NativeFormatCapture;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// 瞬时内容确认窗口时长。
pub(crate) const TRANSIENT_WINDOW: Duration = Duration::from_millis(300);
/// pending 上限：超过后立即确认最旧条目（退化为旧行为，不丢数据）。
pub(crate) const MAX_PENDING: usize = 8;
/// 确认重读的最大尝试次数。
pub(crate) const MAX_ATTEMPTS: u8 = 3;
/// 重读失败后的顺延间隔。
const RETRY_DELAY: Duration = Duration::from_millis(250);

pub(crate) struct PendingCapture {
    pub(crate) sequence: u32,
    pub(crate) content: CapturedClipboard,
    pub(crate) extra: Vec<NativeFormatCapture>,
    pub(crate) source: Option<source_app::SourceApp>,
    pub(crate) due: Instant,
    pub(crate) attempts: u8,
}

/// 300ms 瞬时内容确认窗口。
///
/// 划词工具/Excel 碎片会快速改写剪贴板再还原：条目到期时若序列号已变，
/// 重读当前内容判定——为空（被还原）或不同（被取代）则丢弃，相同则照常入库。
/// 被自身写回（粘贴回贴）取代的条目仍入库，避免「复制后立刻从本应用粘贴」丢数据。
pub(crate) struct TransientWindow {
    enabled: bool,
    pending: VecDeque<PendingCapture>,
}

pub(crate) enum PollOutcome {
    /// 内容应入库（载荷在配对的 PendingCapture 中）。
    Deliver,
    /// 内容被瞬时覆盖或还原，丢弃。
    Discard,
}

impl TransientWindow {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            pending: VecDeque::new(),
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// 登记一条待确认采集；窗口关闭或溢出时立即返回应入库的条目。
    pub(crate) fn push(&mut self, entry: PendingCapture) -> Vec<PendingCapture> {
        if !self.enabled {
            return vec![entry];
        }
        let mut immediate = Vec::new();
        while self.pending.len() >= MAX_PENDING {
            if let Some(oldest) = self.pending.pop_front() {
                immediate.push(oldest);
            }
        }
        self.pending.push_back(entry);
        immediate
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.front().map(|entry| entry.due)
    }

    /// 取出到期条目并判定。`reread` 在 CaptureState 临界区内重读当前剪贴板。
    /// 同一批次内缓存首次重读结果：序列号未变时后续条目直接复用，避免重复全量读取。
    pub(crate) fn poll_due(
        &mut self,
        now: Instant,
        ignored_sequence: u32,
        reread: &mut dyn FnMut() -> Result<(u32, Option<CapturedClipboard>), anyhow::Error>,
    ) -> Vec<(PendingCapture, PollOutcome)> {
        let mut results = Vec::new();
        let mut cached: Option<Result<(u32, Option<CapturedClipboard>), ()>> = None;
        while self.pending.front().is_some_and(|front| front.due <= now) {
            let mut entry = self.pending.pop_front().expect("checked front above");
            let outcome = {
                let read = match &cached {
                    Some(Ok(cached_read)) => Ok(cached_read.clone()),
                    Some(Err(())) => Err(()),
                    None => match reread() {
                        Ok(read) => {
                            cached = Some(Ok(read.clone()));
                            Ok(read)
                        }
                        Err(_) => {
                            cached = Some(Err(()));
                            Err(())
                        }
                    },
                };
                match read {
                    Ok((current, content)) => {
                        if current == entry.sequence
                            || (ignored_sequence != 0 && current == ignored_sequence)
                        {
                            // 序列号未变（确认成功）或被自身回贴取代：照常入库。
                            PollOutcome::Deliver
                        } else {
                            match content {
                                // 内容为空/不同：被还原或取代，丢弃。
                                None => PollOutcome::Discard,
                                Some(current_content) if current_content == entry.content => {
                                    PollOutcome::Deliver
                                }
                                Some(_) => PollOutcome::Discard,
                            }
                        }
                    }
                    Err(()) => {
                        entry.attempts += 1;
                        if entry.attempts >= MAX_ATTEMPTS {
                            PollOutcome::Discard
                        } else {
                            entry.due = now + RETRY_DELAY;
                            self.pending.push_back(entry);
                            continue;
                        }
                    }
                }
            };
            results.push((entry, outcome));
        }
        results
    }

    /// 退出时清空全部未确认条目（<300ms 窗口内，重启后无损失）。
    pub(crate) fn drain(&mut self) {
        self.pending.clear();
    }
}

pub(crate) fn new_pending(
    sequence: u32,
    content: CapturedClipboard,
    extra: Vec<NativeFormatCapture>,
    source: Option<source_app::SourceApp>,
    now: Instant,
) -> PendingCapture {
    PendingCapture {
        sequence,
        content,
        extra,
        source,
        due: now + TRANSIENT_WINDOW,
        attempts: 0,
    }
}

#[cfg(test)]
mod tests;
