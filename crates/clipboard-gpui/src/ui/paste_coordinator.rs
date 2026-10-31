use super::paste_requests::PendingRequest;
use clipboard_platform::{Command, CopyMode, CopyPurpose};

type Target = (isize, u32);

#[derive(Clone, Copy)]
enum DeliveryPhase {
    Copying,
    Delivering,
}

struct Delivery {
    target: Target,
    phase: DeliveryPhase,
}

struct Sequence {
    ids: Vec<i64>,
    target: Target,
    delivered: usize,
    request: Option<(u64, i64)>,
}

#[derive(Default)]
pub(super) struct PasteCoordinator {
    target: Option<Target>,
    pending: PendingRequest<Delivery>,
    quick: PendingRequest<Target>,
    sequence: Option<Sequence>,
    merging: bool,
}

#[derive(Default)]
pub(super) struct Interrupted {
    pub request_id: Option<u64>,
    pub merge: bool,
    pub sequence: bool,
}

impl PasteCoordinator {
    pub fn target(&self) -> Option<Target> {
        self.target
    }

    pub fn set_target(&mut self, target: Option<Target>) {
        self.target = target;
    }

    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn quick_pending(&self) -> bool {
        self.quick.is_some()
    }

    pub fn merging(&self) -> bool {
        self.merging
    }

    pub fn sequence_remaining(&self) -> Option<usize> {
        self.sequence
            .as_ref()
            .map(|queue| queue.ids.len() - queue.delivered)
    }

    pub fn sequence_target(&self) -> Option<Target> {
        self.sequence.as_ref().map(|queue| queue.target)
    }

    pub fn begin_copy(&mut self, target: Target) -> u64 {
        self.pending.begin(Delivery {
            target,
            phase: DeliveryPhase::Copying,
        })
    }

    pub fn mark_merge_started(&mut self) {
        self.merging = true;
    }

    pub fn finish_merge(&mut self) {
        self.merging = false;
    }

    pub fn begin_quick(&mut self, target: Target) -> u64 {
        self.quick.begin(target)
    }

    pub fn resolve_quick(&mut self, request_id: u64) -> Option<Target> {
        self.quick.complete(request_id)
    }

    pub fn cancel_quick(&mut self) {
        self.quick.take();
    }

    pub fn fail(&mut self, request_id: u64) -> Option<Interrupted> {
        self.pending.get(request_id)?;
        Some(self.interrupt(None))
    }

    /// A receipt moves one request into delivery exactly once; stale receipts cannot reenter.
    pub fn receive(&mut self, request_id: u64) -> Option<Target> {
        let pending = self.pending.get_mut(request_id)?;
        if !matches!(pending.phase, DeliveryPhase::Copying) {
            return None;
        }
        pending.phase = DeliveryPhase::Delivering;
        Some(pending.target)
    }

    pub fn delivering(&self, request_id: u64) -> bool {
        self.pending
            .get(request_id)
            .is_some_and(|pending| matches!(pending.phase, DeliveryPhase::Delivering))
    }

    /// Queue ownership and an individual delivery are independent: after delivery the queue waits.
    pub fn complete_delivery(&mut self, request_id: u64) -> Option<usize> {
        if !self.delivering(request_id) {
            return None;
        }
        self.pending.complete(request_id);
        let mut finished = None;
        if let Some(queue) = &mut self.sequence
            && queue.request.map(|(request, _)| request) == Some(request_id)
        {
            queue.request = None;
            queue.delivered += 1;
            if queue.delivered == queue.ids.len() {
                finished = Some(queue.delivered);
                self.sequence = None;
            }
        }
        if self.sequence.is_none() {
            self.target = None;
        }
        finished
    }

    pub fn start_sequence(&mut self, ids: Vec<i64>, target: Target) -> bool {
        if self.pending() || self.quick_pending() || self.sequence.is_some() || ids.len() < 2 {
            return false;
        }
        self.sequence = Some(Sequence {
            ids,
            target,
            delivered: 0,
            request: None,
        });
        true
    }

    pub fn advance_sequence(&mut self) -> Option<Command> {
        if self.pending() || self.quick_pending() {
            return None;
        }
        let queue = self.sequence.as_ref()?;
        if queue.request.is_some() {
            return None;
        }
        let id = *queue.ids.get(queue.delivered)?;
        let target = queue.target;
        let request_id = self.begin_copy(target);
        self.sequence.as_mut()?.request = Some((request_id, id));
        Some(Command::Copy {
            id,
            mode: CopyMode::Original,
            purpose: CopyPurpose::Paste { request_id },
        })
    }

    pub fn sequence_keeps_target(&self, target: Option<Target>) -> bool {
        self.sequence_target()
            .is_some_and(|owned| Some(owned) == target)
    }

    pub fn keeps_foreground(&self, target: Option<Target>) -> bool {
        self.sequence_keeps_target(target)
            || self
                .pending
                .request_id()
                .and_then(|id| self.pending.get(id))
                .is_some_and(|pending| Some(pending.target) == target)
            || self
                .quick
                .request_id()
                .and_then(|id| self.quick.get(id))
                .is_some_and(|pending| Some(*pending) == target)
    }

    pub fn deactivate(&mut self, foreground: Option<Target>) -> Option<(Interrupted, bool)> {
        if self.keeps_foreground(foreground) {
            return None;
        }
        let was_active = self.target.take().is_some()
            || self.pending()
            || self.quick_pending()
            || self.sequence.is_some();
        self.cancel_quick();
        Some((self.interrupt(None), was_active))
    }

    pub fn hide(&mut self) -> Option<Interrupted> {
        self.target = None;
        if self.pending() {
            return None;
        }
        self.cancel_quick();
        Some(self.interrupt(None))
    }

    pub fn show(&mut self) -> Interrupted {
        self.cancel_quick();
        self.interrupt(None)
    }

    /// Called before enqueueing a clipboard replacement, including the first sequence copy.
    pub fn replace(&mut self, command: &Command) -> Option<Interrupted> {
        let keep_request = match command {
            Command::Copy { purpose, .. } | Command::Merge { purpose, .. } => purpose.request_id(),
            Command::ResolveQuickPaste { .. } => None,
            _ => return None,
        };
        let sequence_owned = matches!(command,
            Command::Copy { id, mode: CopyMode::Original, purpose: CopyPurpose::Paste { request_id } }
                if self.sequence.as_ref().is_some_and(|queue| queue.request == Some((*request_id, *id))));
        let interrupted = if sequence_owned {
            Interrupted::default()
        } else {
            self.interrupt(keep_request)
        };
        if !matches!(command, Command::ResolveQuickPaste { .. }) {
            self.cancel_quick();
        }
        Some(interrupted)
    }

    pub fn interrupt(&mut self, keep_request: Option<u64>) -> Interrupted {
        let request_id = self
            .pending
            .request_id()
            .filter(|id| Some(*id) != keep_request);
        if request_id.is_some() {
            self.pending.take();
        }
        Interrupted {
            request_id,
            merge: std::mem::take(&mut self.merging),
            sequence: self.sequence.take().is_some(),
        }
    }
}

#[cfg(test)]
#[path = "paste_coordinator_tests.rs"]
mod tests;
