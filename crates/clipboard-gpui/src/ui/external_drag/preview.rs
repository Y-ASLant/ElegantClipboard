use super::*;

pub(in crate::ui) struct DragPreview {
    pub(super) card: Option<HistoryDrag>,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.card.as_ref() {
            Some(card) => card.render_card(cx).into_any_element(),
            None => div().into_any_element(),
        }
    }
}

impl ClipboardView {
    pub(super) fn open_drag_preview(
        &mut self,
        drag: &HistoryDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<isize> {
        let mut card = drag.clone();
        card.native_preview = false;
        if let Some(popup) = self.drag_preview_window
            && let Ok(hwnd) = popup.update(cx, |preview, window, cx| {
                preview.card = Some(card.clone());
                cx.notify();
                native_window::window_id(window)
            })
        {
            return hwnd;
        }
        let display_id = window.display(cx).map(|display| display.id());
        let popup = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        drag.size.get(),
                    ))),
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
                move |_, cx| cx.new(|_| DragPreview { card: Some(card) }),
            )
            .ok()?;
        let hwnd = popup
            .update(cx, |_, window, _| native_window::window_id(window))
            .ok()
            .flatten()?;
        self.drag_preview_window = Some(popup);
        Some(hwnd)
    }
}
