use super::*;
use clipboard_platform::external_drag::{
    self as native, ExternalDragOptions, ExternalDragOutcome, PreparedExternalDrag,
};

pub(super) struct CardDragSession {
    token: u64,
    drag: HistoryDrag,
    popup: AnyWindowHandle,
    source_hwnd: isize,
    preview_hwnd: isize,
    offset: (i32, i32),
    payload: Option<PreparedExternalDrag>,
    native_active: bool,
}

impl ClipboardView {
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
        let Ok(pointer) = native::poll_drag_pointer() else {
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
        let bounds = Bounds::new(
            point(
                px((pointer.x - offset.0) as f32 / scale),
                px((pointer.y - offset.1) as f32 / scale),
            ),
            dimensions,
        );
        let mut preview = drag.clone();
        preview.native_preview = false;
        let display_id = window.display(cx).map(|display| display.id());
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: None,
                kind: WindowKind::PopUp,
                focus: false,
                show: false,
                is_movable: false,
                is_resizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                app_id: Some("com.aslant.elegant-clipboard-gpui.drag".into()),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| preview),
        );
        let Ok(popup) = opened else {
            return false;
        };
        let popup = AnyWindowHandle::from(popup);
        let initialized = popup.update(cx, |_, window, _| {
            let hwnd =
                tray::window_hwnd(window).ok_or_else(|| anyhow::anyhow!("拖拽预览窗口不可用"))?;
            Ok::<_, anyhow::Error>(hwnd.0 as isize)
        });
        let Ok(Ok(preview_hwnd)) = initialized else {
            let _ = popup.update(cx, |_, window, _| window.remove_window());
            return false;
        };
        self.drag_token = self.drag_token.wrapping_add(1);
        let token = self.drag_token;
        self.external_drag = Some(CardDragSession {
            token,
            drag: drag.clone(),
            popup,
            source_hwnd: source.0 as isize,
            preview_hwnd,
            offset,
            payload: None,
            native_active: false,
        });
        self.send(Command::PrepareExternalDrag { id: drag.id, token }, cx);
        let main = window.window_handle();
        cx.spawn(async move |view, cx| {
            // Native styles/motion can synchronously notify GPUI of display changes.
            // Like OLE, these calls must run without an App or Window borrow.
            let configured = native::configure_drag_preview(preview_hwnd);
            if let Err(error) = configured {
                let _ = update_clipboard_window(&view, main, cx, |this, window, cx| {
                    cx.stop_active_drag(window);
                    this.finish_external_drag(cx);
                    this.set_feedback_message(format!(
                        "{}: {error:#}",
                        tr(
                            this.language,
                            "无法显示拖动预览",
                            "Unable to show drag preview"
                        )
                    ));
                    this.is_error = true;
                });
                return;
            }
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let state = update_clipboard_window(&view, main, cx, |this, window, cx| {
                    let session = this.external_drag.as_ref().filter(|s| s.token == token)?;
                    let pointer = native::poll_drag_pointer().ok();
                    if !cx.has_active_drag()
                        || !this.valid_drag(&session.drag)
                        || pointer.is_none_or(|p| !p.left_button_down)
                    {
                        cx.stop_active_drag(window);
                        this.finish_external_drag(cx);
                        return None;
                    }
                    let pointer = pointer.expect("checked above");
                    let session = this.external_drag.as_mut().expect("checked above");
                    let motion = (
                        session.preview_hwnd,
                        pointer.x - session.offset.0,
                        pointer.y - session.offset.1,
                    );
                    let outside = native::pointer_outside_source(session.source_hwnd, pointer)
                        .unwrap_or(false);
                    let prepared = if outside {
                        session.payload.clone().map(|payload| {
                            session.native_active = true;
                            (
                                payload,
                                ExternalDragOptions {
                                    source_window: session.source_hwnd,
                                    preview_window: Some(session.preview_hwnd),
                                    preview_offset: session.offset,
                                },
                            )
                        })
                    } else {
                        None
                    };
                    Some((motion, prepared))
                });
                let Ok(Some(((preview_hwnd, x, y), prepared))) = state else {
                    break;
                };
                if let Err(error) = native::move_drag_preview(preview_hwnd, x, y) {
                    let _ = update_clipboard_window(&view, main, cx, |this, window, cx| {
                        cx.stop_active_drag(window);
                        this.finish_external_drag(cx);
                        this.set_feedback_message(format!(
                            "{}: {error:#}",
                            tr(
                                this.language,
                                "无法移动拖动预览",
                                "Unable to move drag preview"
                            )
                        ));
                        this.is_error = true;
                    });
                    break;
                }
                let Some((payload, options)) = prepared else {
                    continue;
                };
                // OLE pumps window messages. No GPUI App/entity borrow may survive this call.
                let result = native::run_external_drag(payload, options, |_| {});
                let returned = matches!(result, Ok(ExternalDragOutcome::ReturnedToSource));
                let _ = update_clipboard_window(&view, main, cx, |this, window, cx| {
                    if this.external_drag.as_ref().is_none_or(|s| s.token != token) {
                        return;
                    }
                    if returned {
                        if let Some(session) = this.external_drag.as_mut() {
                            session.native_active = false;
                        }
                    } else {
                        cx.stop_active_drag(window);
                        this.finish_external_drag(cx);
                    }
                    this.cancel_pending_row_click();
                    if let Err(error) = result {
                        this.set_feedback_message(format!(
                            "{}: {error:#}",
                            tr(this.language, "无法拖出卡片", "Unable to drag card out")
                        ));
                        this.is_error = true;
                    }
                    cx.notify();
                });
                if !returned {
                    break;
                }
            }
        })
        .detach();
        true
    }

    pub(super) fn apply_external_drag_prepared(
        &mut self,
        id: i64,
        token: u64,
        result: Result<PreparedExternalDrag, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .external_drag
            .as_mut()
            .filter(|s| s.token == token && s.drag.id == id)
        else {
            return;
        };
        match result {
            Ok(payload) => session.payload = Some(payload),
            Err(error) => {
                self.set_feedback_message(error);
                self.is_error = true;
                cx.notify();
            }
        }
    }

    pub(super) fn finish_external_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.external_drag.take() {
            let _ = session
                .popup
                .update(cx, |_, window, _| window.remove_window());
        }
        if !self.reorder_pending {
            self.drop_target = None;
        }
        self.history_drag_direction = 0;
        self.cancel_pending_row_click();
        cx.notify();
    }
}
