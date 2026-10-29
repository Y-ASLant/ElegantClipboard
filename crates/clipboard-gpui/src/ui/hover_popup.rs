use super::*;

impl ClipboardView {
    pub(super) fn open_hover_popup(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.hover_popup.is_some()
            || self.hover_popup_opening.is_some()
            || !self.hover_source_active
        {
            return;
        }
        let (Some(id), Some(result)) = (self.hover_preview.id, self.hover_preview.result.clone())
        else {
            return;
        };
        let generation = self.hover_preview.generation;
        #[cfg(windows)]
        let (owner_bounds, screen, scale_factor, display_id) =
            match crate::native::preview::geometry(window) {
                Ok(geometry) => (
                    geometry.owner,
                    geometry.work_area,
                    geometry.scale_factor,
                    Some(geometry.display_id),
                ),
                Err(error) => {
                    tracing::warn!(id, generation, %error, "Reading preview work area failed");
                    return;
                }
            };
        #[cfg(not(windows))]
        let (owner_bounds, screen, scale_factor, display_id) = (
            window.bounds(),
            window
                .display(cx)
                .map_or(window.bounds(), |display| display.visible_bounds()),
            window.scale_factor(),
            window.display(cx).map(|display| display.id()),
        );
        let Some(index) = self.history.items.iter().position(|item| item.id == id) else {
            return;
        };
        let item = &self.history.items[index];
        let image_path = match &result {
            Ok(PreviewContent::Image(path)) => Some(path.clone()),
            Ok(PreviewContent::Files(entries)) => single_file_image_path(entries),
            _ => None,
        };
        let image_dimensions = image_path.as_deref().and_then(|path| {
            item.image_width
                .zip(item.image_height)
                .or_else(|| image_file_dimensions(path))
        });
        let (Some(viewport), Some(row_offset)) =
            (self.drag_viewport.get(), self.row_offsets.get(index))
        else {
            return;
        };
        let spacing = px(visual::card_spacing(self.display.card_density));
        let top = viewport.top() + *row_offset + self.scroll.offset().y + spacing;
        let bottom = top + px(history_row_height(item, self.display)) - spacing * 2.;
        let visible_top = top.max(viewport.top());
        let visible_bottom = bottom.min(viewport.bottom());
        if visible_bottom <= visible_top {
            return;
        }
        let source_bounds = Bounds::new(
            point(
                owner_bounds.left() + px(PAGE_PADDING),
                owner_bounds.top() + visible_top,
            ),
            size(
                owner_bounds.size.width - px(PAGE_PADDING * 2.),
                visible_bottom - visible_top,
            ),
        );
        let anchor_y = owner_bounds.top() + window.mouse_position().y;
        let Some(bounds) = crate::popup_geometry::hover_popup_bounds(
            owner_bounds,
            screen,
            self.hover_preference.position,
            image_dimensions,
            self.hover_preference.expanded_image,
            anchor_y,
        ) else {
            return;
        };
        let geometry = crate::popup_geometry::PopupZoomGeometry {
            initial: bounds,
            owner: owner_bounds,
            display: screen,
            anchor_y,
            scale_factor,
        };
        let setup = hover_window::HoverPreviewSetup {
            geometry,
            source_bounds,
            image_dimensions,
            information: hover_information::HoverInformation::new(item, image_dimensions),
        };
        let language = self.language;
        let zoom_step = self.hover_preference.zoom_step;
        #[cfg(windows)]
        let main_native = native_window::window_id(window);
        tracing::debug!(
            id,
            generation,
            ?screen,
            ?bounds,
            ?source_bounds,
            scale_factor,
            "Opening passive preview overlay"
        );
        self.hover_popup_opening = Some((id, generation));
        cx.spawn(async move |owner, cx| {
            let Some(owner) = owner.upgrade() else { return };
            if !owner.update(cx, |this, _| {
                this.hover_preview.id == Some(id)
                    && this.hover_preview.generation == generation
                    && this.hover_source_active
            }) {
                return;
            }
            let opened = open_overlay(
                HoverPreviewWindowView::new((id, generation), language, result, zoom_step, setup),
                display_id,
                cx,
            );
            owner.update(cx, |this, cx| {
                if this.hover_popup_opening == Some((id, generation)) {
                    this.hover_popup_opening = None;
                }
                match opened {
                    Ok(handle)
                        if this.hover_preview.id == Some(id)
                            && this.hover_preview.generation == generation
                            && this.hover_source_active =>
                    {
                        this.hover_popup = Some(handle.into());
                        let _ = handle.update(cx, |_, window, cx| {
                            window.refresh();
                            window_lifecycle::log_window_geometry(
                                window,
                                cx,
                                "preview_overlay_ready",
                            );
                        });
                        #[cfg(windows)]
                        if let Some(main) = main_native {
                            this.track_hover_pointer(main, scale_factor, cx);
                        }
                    }
                    Ok(handle) => {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    Err(error) => {
                        tracing::error!(id, generation, %error, "Preview overlay creation failed")
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn update_hover_anchor(&self, id: i64, pointer_y: Pixels, cx: &mut Context<Self>) {
        if self.hover_preview.id != Some(id) || !self.hover_source_active {
            return;
        }
        if let Some(handle) = self
            .hover_popup
            .and_then(|handle| handle.downcast::<HoverPreviewWindowView>())
        {
            let _ = handle.update(cx, |view, _, cx| {
                let anchor_y = view.zoom_geometry.owner.top() + pointer_y;
                if view.zoom_geometry.anchor_y != anchor_y {
                    view.zoom_geometry.anchor_y = anchor_y;
                    cx.notify();
                }
            });
        }
    }
}

/// Both history and the isolated onboarding exercise use the same passive
/// native overlay. Callers retain ownership and reject stale async requests.
pub(super) fn open_overlay(
    view: HoverPreviewWindowView,
    display_id: Option<DisplayId>,
    cx: &mut AsyncApp,
) -> anyhow::Result<WindowHandle<HoverPreviewWindowView>> {
    let screen = view.zoom_geometry.display;
    let scale_factor = view.zoom_geometry.scale_factor;
    // No Root/window-border plugin: the transparent work area contains the
    // surface, connector and information together.
    let handle = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(screen)),
                display_id,
                titlebar: None,
                kind: WindowKind::PopUp,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_min_size: Some(size(px(1.), px(1.))),
                inactive_frame_interval: None,
                app_id: Some("com.aslant.elegant-clipboard-gpui.hover".into()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| view),
        )
    })?;
    // Every native mutation is outside App/Window/entity borrows.
    let prepared = (|| -> anyhow::Result<()> {
        let native = handle
            .update(cx, |_, window, _| native_window::window_id(window))?
            .ok_or_else(|| anyhow::anyhow!("Preview native handle unavailable"))?;
        #[cfg(windows)]
        crate::native::preview::configure_overlay(native)?;
        clipboard_platform::external_drag::configure_drag_preview(native)?;
        #[cfg(windows)]
        crate::native::preview::set_bounds(native, screen, scale_factor)?;
        #[cfg(not(windows))]
        {
            use clipboard_platform::external_drag as platform;
            platform::move_drag_preview(
                native,
                (f32::from(screen.left()) * scale_factor).round() as i32,
                (f32::from(screen.top()) * scale_factor).round() as i32,
            )?;
            platform::resize_drag_preview(
                native,
                (f32::from(screen.size.width) * scale_factor).round() as i32,
                (f32::from(screen.size.height) * scale_factor).round() as i32,
            )?;
        }
        clipboard_platform::external_drag::show_drag_preview(native)?;
        Ok(())
    })();
    if let Err(error) = prepared {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return Err(error);
    }
    Ok(handle)
}
