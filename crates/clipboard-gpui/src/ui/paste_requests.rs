/// Correlates asynchronous paste results across cancellation and reentry.
#[derive(Default)]
pub(super) struct PendingRequest<T> {
    generation: u64,
    pending: Option<T>,
}

impl<T> PendingRequest<T> {
    pub(super) fn begin(&mut self, value: T) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("request ID exhausted");
        self.pending = Some(value);
        self.generation
    }

    pub(super) fn complete(&mut self, request_id: u64) -> Option<T> {
        if request_id != self.generation {
            return None;
        }
        self.take()
    }

    pub(super) fn take(&mut self) -> Option<T> {
        self.pending.take()
    }

    pub(super) fn is_some(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn is_none(&self) -> bool {
        self.pending.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::PendingRequest;

    #[test]
    fn cancelled_copy_success_and_failure_leave_same_record_retry_pending() {
        let mut pending = PendingRequest::default();
        let old = pending.begin((42, "old target"));
        pending.take();
        assert_eq!(pending.complete(old), None);
        let current = pending.begin((42, "new target"));
        // Success and failure handlers share this request correlation gate.
        assert_eq!(pending.complete(old), None);
        assert_eq!(pending.complete(current), Some((42, "new target")));
        assert_eq!(pending.complete(current), None);
    }

    #[test]
    fn cancelled_quick_resolution_cannot_consume_same_slot_in_another_group() {
        let mut pending = PendingRequest::default();
        let old = pending.begin((1, false, "group A"));
        pending.take();
        let current = pending.begin((1, false, "group B"));
        assert_eq!(pending.complete(old), None);
        assert_eq!(pending.complete(current), Some((1, false, "group B")));
    }
}
