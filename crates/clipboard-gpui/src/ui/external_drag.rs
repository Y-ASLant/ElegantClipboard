use super::*;
use clipboard_platform::external_drag::{
    self as native, ExternalDragOptions, ExternalDragOutcome, PreparedExternalDrag,
};

mod preview;
pub(super) use preview::DragPreview;

pub(super) struct DragPreparation {
    id: i64,
    token: u64,
    result: Option<Result<PreparedExternalDrag, String>>,
}

pub(super) struct CardDragSession {
    token: u64,
    drag: Rc<HistoryDrag>,
    source_hwnd: isize,
    preview_hwnd: isize,
    offset: (i32, i32),
    native_active: bool,
}

impl ClipboardView {
    pub(super) fn card_drag_active(&self, cx: &App) -> bool {
        self.external_drag.is_some() || cx.has_active_drag()
    }

    pub(super) fn suppress_drag_click(&self, cx: &App) -> bool {
        self.card_drag_active(cx) || self.drag_release_pending
    }

    pub(super) fn drag_source_id(&self) -> Option<i64> {
        self.external_drag
            .as_ref()
            .map(|session| session.drag.id)
            .or(self.history.selected)
    }

    pub(super) fn native_drag_active(&self) -> bool {
        self.external_drag
            .as_ref()
            .is_some_and(|session| session.native_active)
    }

    pub(super) fn prepare_card_drag(&mut self, id: i64, cx: &mut Context<Self>) {
        self.drag_token = self.drag_token.wrapping_add(1);
        let token = self.drag_token;
        self.drag_preparation = Some(DragPreparation {
            id,
            token,
            result: None,
        });
        self.send(Command::PrepareExternalDrag { id, token }, cx);
    }

    pub(super) fn begin_external_drag(
        &mut self,
        drag: &HistoryDrag,
        cursor_offset: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.finish_external_drag(cx);
        let Some(source) = tray::window_hwnd(window) else {
            return false;
        };
        let dimensions = drag.size.get();
        if dimensions.width <= px(0.) || dimensions.height <= px(0.) {
            return false;
        }
        let scale = window.scale_factor();
        let offset = (
            (f32::from(cursor_offset.x) * scale).round() as i32,
            (f32::from(cursor_offset.y) * scale).round() as i32,
        );
        let Some(preview_hwnd) = self.open_drag_preview(drag, window, cx) else {
            return false;
        };
        if self
            .drag_preparation
            .as_ref()
            .is_none_or(|prepared| prepared.id != drag.id)
        {
            self.prepare_card_drag(drag.id, cx);
        }
        let token = self
            .drag_preparation
            .as_ref()
            .expect("prepared above")
            .token;
        self.external_drag = Some(CardDragSession {
            token,
            drag: Rc::new(drag.clone()),
            source_hwnd: source.0 as isize,
            preview_hwnd,
            offset,
            native_active: false,
        });
        self.drag_release_pending = false;
        let main = window.window_handle();
        cx.spawn(async move |view, cx| {
            // Start after GPUI's on_drag callback installs its temporary drag.
            // Our session owns input from here, including OLE re-entry and release.
            let owned = update_clipboard_window(&view, main, cx, |this, window, cx| {
                if this.external_drag.as_ref().is_none_or(|session| session.token != token) { return false; }
                cx.stop_active_drag(window);
                true
            }).unwrap_or(false);
            if !owned { return; }
            let configured = (|| -> anyhow::Result<()> {
                native::configure_drag_preview(preview_hwnd)?;
                let pointer = native::poll_drag_pointer()?;
                // Move first: a reused popup may still belong to another DPI monitor.
                native::move_drag_preview(preview_hwnd, pointer.x - offset.0, pointer.y - offset.1)?;
                let preview_scale = native::drag_preview_scale(preview_hwnd)?;
                native::resize_drag_preview(preview_hwnd,
                    (f32::from(dimensions.width) * preview_scale).ceil() as i32,
                    (f32::from(dimensions.height) * preview_scale).ceil() as i32)
            })();
            if let Err(error) = configured {
                let _ = view.update(cx, |this, cx| this.fail_card_drag(format!("{error:#}"), cx));
                return;
            }
            let mut last_position = None;
            let (sender, receiver) = async_channel::bounded(1);
            let frame_pending = Rc::new(Cell::new(false));
            loop {
                let pointer = match native::poll_drag_pointer() {
                    Ok(pointer) => pointer,
                    Err(error) => {
                        let _ = view.update(cx, |this, cx| this.fail_card_drag(format!("{error:#}"), cx));
                        break;
                    }
                };
                let state = update_clipboard_window(&view, main, cx, |this, window, cx| {
                    let session = this.external_drag.as_ref().filter(|session| session.token == token)?;
                    let source = session.source_hwnd;
                    let drag = session.drag.clone();
                    if native::drag_escape_pressed() {
                        this.finish_external_drag(cx);
                        return None;
                    }
                    if !this.valid_drag(&drag) {
                        this.fail_card_drag(tr(this.language,
                            "列表已变化，拖动已取消", "The list changed; drag cancelled.").into(), cx);
                        return None;
                    }
                    let outside = match native::pointer_outside_source(source, pointer) {
                        Ok(outside) => outside,
                        Err(error) => {
                            this.fail_card_drag(format!("{error:#}"), cx);
                            return None;
                        }
                    };
                    if !outside {
                        if let Ok((x, y)) = native::drag_pointer_in_client(source, pointer, window.scale_factor()) {
                            this.update_card_drag_target(&drag, point(px(x), px(y)), cx);
                        }
                    } else {
                        this.history_drag_direction = 0;
                    }
                    if !pointer.left_button_down {
                        // Release is committed here even if GPUI never received MouseUp.
                        if !outside { this.drop_history_card(&drag, cx); }
                        else { this.set_feedback_message(tr(this.language,
                            "拖放尚未就绪，请按住卡片移出窗口后再松开",
                            "Drag was not ready. Hold the card outside the window before releasing.").into());
                            this.is_error = true; }
                        this.finish_external_drag(cx);
                        return None;
                    }
                    let payload = if outside {
                        this.drag_preparation.as_ref().and_then(|prepared| prepared.result.clone())
                    } else { None };
                    let payload = match payload {
                        Some(Ok(payload)) => Some(payload),
                        Some(Err(error)) => { this.fail_card_drag(error, cx); return None; }
                        None => None,
                    };
                    let session = this.external_drag.as_mut()?;
                    session.native_active = payload.is_some();
                    Some((session.preview_hwnd, session.offset, payload, ExternalDragOptions {
                        source_window: source, preview_window: Some(session.preview_hwnd),
                        preview_offset: session.offset,
                    }))
                });
                let Ok(Some((preview, offset, payload, options))) = state else { break; };
                let position = (pointer.x - offset.0, pointer.y - offset.1);
                // Native movement may re-enter GPUI. Never hold App/Window borrows here.
                if last_position != Some(position) {
                    let movement = native::move_drag_preview(preview, position.0, position.1)
                        .and_then(|_| if last_position.is_none() { native::show_drag_preview(preview) } else { Ok(()) });
                    if let Err(error) = movement {
                        let _ = view.update(cx, |this, cx| this.fail_card_drag(format!("{error:#}"), cx));
                        break;
                    }
                    last_position = Some(position);
                }
                if let Some(payload) = payload {
                    let result = native::run_external_drag(payload, options);
                    let keep = update_clipboard_window(&view, main, cx, |this, window, cx| {
                        let Some(session) = this.external_drag.as_mut().filter(|session| session.token == token) else { return false; };
                        session.native_active = false;
                        let drag = session.drag.clone();
                        // OLE may have delivered framework file-drop events. They never own our session.
                        cx.stop_active_drag(window);
                        match result {
                            Ok(ExternalDragOutcome::ReturnedToSource) => true,
                            Ok(ExternalDragOutcome::ReleasedInSource) => {
                                if let Ok(pointer) = native::poll_drag_pointer()
                                    && let Ok((x, y)) = native::drag_pointer_in_client(options.source_window, pointer, window.scale_factor()) {
                                    this.update_card_drag_target(&drag, point(px(x), px(y)), cx);
                                    this.drop_history_card(&drag, cx);
                                }
                                this.finish_external_drag(cx);
                                false
                            }
                            Ok(_) => { this.finish_external_drag(cx); false }
                            Err(error) => { this.fail_card_drag(format!("{error:#}"), cx); false }
                        }
                    }).unwrap_or(false);
                    if !keep { break; }
                    last_position = None;
                }
                if !frame_pending.replace(true) {
                    let signal = sender.clone();
                    let pending = frame_pending.clone();
                    if main.update(cx, |_, window, _| {
                        window.on_next_frame(move |_, _| {
                            pending.set(false);
                            let _ = signal.try_send(());
                        });
                    }).is_err() { break; }
                }
                let mut frame = std::pin::pin!(receiver.recv());
                let mut watchdog = std::pin::pin!(cx.background_executor().timer(Duration::from_millis(100)));
                std::future::poll_fn(|context| {
                    use std::{future::Future, task::Poll};
                    if frame.as_mut().poll(context).is_ready() || watchdog.as_mut().poll(context).is_ready() {
                        Poll::Ready(())
                    } else { Poll::Pending }
                }).await;
            }
            let may_hide = view.update(cx, |this, cx| {
                if this.external_drag.as_ref().is_some_and(|session| session.token != token) { return false; }
                if this.external_drag.is_some() { this.finish_external_drag(cx); }
                true
            }).unwrap_or(true);
            if may_hide { let _ = native::hide_drag_preview(preview_hwnd); }
        }).detach();
        true
    }

    fn update_card_drag_target(
        &mut self,
        drag: &HistoryDrag,
        pointer: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.drag_viewport.get() else {
            return;
        };
        let inside = bounds.contains(&pointer);
        let direction = if !inside {
            0
        } else if pointer.y < bounds.top() + px(visual::DRAG_EDGE_ZONE) {
            -1
        } else if pointer.y > bounds.bottom() - px(visual::DRAG_EDGE_ZONE) {
            1
        } else {
            0
        };
        let target = if inside {
            self.history_drag_target(drag, bounds.top(), pointer.y)
        } else {
            self.drag_motion.hovered = None;
            None
        };
        if self.drop_target != target || self.history_drag_direction != direction {
            self.drop_target = target;
            self.history_drag_direction = direction;
            cx.notify();
        }
    }

    pub(super) fn apply_external_drag_prepared(
        &mut self,
        id: i64,
        token: u64,
        result: Result<PreparedExternalDrag, String>,
        _cx: &mut Context<Self>,
    ) {
        if let Some(prepared) = self
            .drag_preparation
            .as_mut()
            .filter(|prepared| prepared.id == id && prepared.token == token)
        {
            prepared.result = Some(result);
        }
    }

    fn fail_card_drag(&mut self, error: String, cx: &mut Context<Self>) {
        self.set_feedback_message(error);
        self.is_error = true;
        self.finish_external_drag(cx);
    }

    pub(super) fn finish_external_drag(&mut self, cx: &mut Context<Self>) {
        if self.external_drag.take().is_some() {
            self.drag_preparation = None;
            self.drag_release_pending = true;
            if let Some(popup) = self.drag_preview_window {
                let _ = popup.update(cx, |preview, _, cx| {
                    preview.card = None;
                    cx.notify();
                });
            }
        }
        if !self.reorder_pending {
            self.drop_target = None;
        }
        self.history_drag_direction = 0;
        self.cancel_pending_row_click();
        if let Some((items, total, generation)) = self.drag_snapshot.take()
            && !self.reorder_pending
        {
            self.apply_snapshot(items, total, generation, cx);
        }
        cx.notify();
    }
}
