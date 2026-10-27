use super::Worker;
use crate::{sequence::current_sequence, service::read_content::ClipboardSnapshot};
use clipboard_core::NativeFormatCapture;

/// 仅成功复制可保存快照，并绑定请求编号和写后序列。
pub(in crate::service) struct StashedClipboard {
    request_id: u64,
    clipboard_sequence: u32,
    pub(super) content: super::CapturedClipboard,
    pub(super) extra: Vec<NativeFormatCapture>,
}

impl StashedClipboard {
    pub(super) fn is_current(&self, sequence: u32) -> bool {
        sequence != 0 && sequence == self.clipboard_sequence
    }

    pub(super) fn cancel(slot: &mut Option<Self>, request_id: u64) {
        if slot
            .as_ref()
            .is_some_and(|stash| stash.request_id == request_id)
        {
            *slot = None;
        }
    }

    pub(super) fn take_for_restore(
        slot: &mut Option<Self>,
        request_id: u64,
        clipboard_sequence: u32,
    ) -> Option<Self> {
        if slot.as_ref()?.request_id != request_id {
            return None;
        }
        // 同一请求即使序列无效也消费其快照，避免后续错误重试。
        slot.take()
            .filter(|stash| stash.is_current(clipboard_sequence))
    }
}

impl Worker {
    /// 先保留为局部变量，读取或写入失败时不留下可恢复的快照。
    pub(super) fn snapshot_clipboard_for_paste(&self) -> Option<ClipboardSnapshot> {
        if !self.preferences.restore_clipboard().ok()? {
            return None;
        }
        let before = current_sequence(&self.clipboard).ok()?;
        if before == 0 {
            return None;
        }
        let snapshot = super::super::read_formats::read_clipboard(&self.clipboard).ok()?;
        let after = current_sequence(&self.clipboard).ok()?;
        (before == after && snapshot.content.is_some()).then_some(snapshot)
    }

    pub(super) fn remember_paste_stash(
        &mut self,
        request_id: Option<u64>,
        clipboard_sequence: u32,
        snapshot: Option<ClipboardSnapshot>,
    ) {
        self.clipboard_stash = request_id.zip(snapshot).and_then(|(request_id, snapshot)| {
            Some(StashedClipboard {
                request_id,
                clipboard_sequence,
                content: snapshot.content?,
                extra: snapshot.extra,
            })
        });
    }

    pub(super) fn take_stash_for_restore(
        &mut self,
        request_id: u64,
        clipboard_sequence: u32,
    ) -> Option<StashedClipboard> {
        if !self.preferences.restore_clipboard().unwrap_or(false) {
            self.clipboard_stash = None;
            return None;
        }
        StashedClipboard::take_for_restore(
            &mut self.clipboard_stash,
            request_id,
            clipboard_sequence,
        )
    }

    pub(super) fn cancel_paste_stash(&mut self, request_id: u64) {
        StashedClipboard::cancel(&mut self.clipboard_stash, request_id);
    }

    pub(super) fn discard_changed_stash(&mut self) {
        if self.clipboard_stash.as_ref().is_some_and(|stash| {
            !current_sequence(&self.clipboard).is_ok_and(|sequence| stash.is_current(sequence))
        }) {
            self.clipboard_stash = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::CapturedClipboard;

    fn stash(request_id: u64, clipboard_sequence: u32) -> StashedClipboard {
        StashedClipboard {
            request_id,
            clipboard_sequence,
            content: CapturedClipboard::Text("user clipboard".into()),
            extra: Vec::new(),
        }
    }

    #[test]
    fn stale_restore_and_cancel_cannot_consume_a_new_requests_snapshot() {
        let mut slot = Some(stash(2, 20));
        assert!(StashedClipboard::take_for_restore(&mut slot, 1, 10).is_none());
        StashedClipboard::cancel(&mut slot, 1);
        let snapshot = StashedClipboard::take_for_restore(&mut slot, 2, 20).unwrap();
        assert_eq!(
            snapshot.content,
            CapturedClipboard::Text("user clipboard".into())
        );
        assert!(slot.is_none());
        assert!(StashedClipboard::take_for_restore(&mut slot, 2, 20).is_none());
    }

    #[test]
    fn failed_or_cancelled_delivery_has_no_restorable_snapshot() {
        let mut slot = Some(stash(7, 70));
        StashedClipboard::cancel(&mut slot, 7);
        assert!(StashedClipboard::take_for_restore(&mut slot, 7, 70).is_none());
        for token_sequence in [0, 69, 71] {
            let mut slot = Some(stash(7, 70));
            assert!(StashedClipboard::take_for_restore(&mut slot, 7, token_sequence).is_none());
            assert!(slot.is_none());
        }
    }

    #[test]
    fn newer_clipboard_content_disqualifies_even_a_matching_request_token() {
        let mut slot = Some(stash(3, 30));
        let snapshot = StashedClipboard::take_for_restore(&mut slot, 3, 30).unwrap();
        assert!(snapshot.is_current(30));
        assert!(!snapshot.is_current(31));
        assert!(!snapshot.is_current(0));
    }
}
