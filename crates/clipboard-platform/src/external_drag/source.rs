use super::{
    DragPointerState, ExternalDragOptions, ExternalDragOutcome, contains, move_drag_preview,
    poll_drag_pointer,
};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, RECT, S_OK,
        },
        System::{
            Ole::{DROPEFFECT, IDropSource, IDropSource_Impl},
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
    },
    core::{BOOL, HRESULT, implement},
};

#[implement(IDropSource)]
pub(super) struct DragSource {
    pub rect: RECT,
    pub has_left: Cell<bool>,
    pub returned: Rc<Cell<Option<ExternalDragOutcome>>>,
    pub failed: Rc<Cell<bool>>,
    pub last_preview_position: Cell<Option<(i32, i32)>>,
    pub options: ExternalDragOptions,
}

impl DragSource {
    fn update_preview(&self, pointer: DragPointerState) -> bool {
        // Keep native preview updates from unwinding through the COM ABI.
        let result = catch_unwind(AssertUnwindSafe(|| {
            if let Some(hwnd) = self.options.preview_window {
                let position = (
                    pointer.x.saturating_sub(self.options.preview_offset.0),
                    pointer.y.saturating_sub(self.options.preview_offset.1),
                );
                if self.last_preview_position.get() != Some(position) {
                    move_drag_preview(hwnd, position.0, position.1)?;
                    self.last_preview_position.set(Some(position));
                }
            }
            anyhow::Ok(())
        }));
        let ok = matches!(result, Ok(Ok(())));
        if !ok {
            self.failed.set(true);
        }
        ok
    }

    pub(super) fn continue_at(&self, escape: bool, pointer: DragPointerState) -> HRESULT {
        if escape || self.failed.get() {
            return DRAGDROP_S_CANCEL;
        }
        let inside = contains(self.rect, pointer);
        // Re-entry wins over release: never drop into the source window even
        // when release and the final move arrive in the same OLE iteration.
        if inside && (self.has_left.get() || !pointer.left_button_down) {
            self.returned.set(Some(if pointer.left_button_down {
                ExternalDragOutcome::ReturnedToSource
            } else {
                ExternalDragOutcome::ReleasedInSource
            }));
            return DRAGDROP_S_CANCEL;
        }
        if !inside {
            self.has_left.set(true);
        }
        if !pointer.left_button_down {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }
}

impl IDropSource_Impl for DragSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() || self.failed.get() {
            return DRAGDROP_S_CANCEL;
        }
        let Ok(mut pointer) = poll_drag_pointer() else {
            self.failed.set(true);
            return DRAGDROP_S_CANCEL;
        };
        pointer.left_button_down = keys.0 & MK_LBUTTON.0 != 0;
        if !self.update_preview(pointer) {
            return DRAGDROP_S_CANCEL;
        }
        self.continue_at(false, pointer)
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        if let Ok(pointer) = poll_drag_pointer() {
            self.update_preview(pointer);
        }
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
