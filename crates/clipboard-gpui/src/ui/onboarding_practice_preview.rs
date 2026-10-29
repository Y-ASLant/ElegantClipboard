use super::*;

const IMAGE_WIDTH: u32 = 640;
const IMAGE_HEIGHT: u32 = 360;

pub(super) fn sample_image() -> Arc<RenderImage> {
    let pixels = image::ImageBuffer::from_fn(IMAGE_WIDTH, IMAGE_HEIGHT, |x, y| {
        let horizon = 190 + (x as i32 - 320).abs() / 4;
        let sun = (x as i32 - 465).pow(2) + (y as i32 - 95).pow(2) < 38 * 38;
        let rgba = if sun {
            [255, 225, 157, 255]
        } else if y as i32 > horizon {
            [42, 67, 94, 255]
        } else if y > 210 - x / 5 {
            [80, 94, 129, 255]
        } else {
            [
                224 - (y / 5) as u8,
                146 + (y / 8) as u8,
                128 + (y / 4) as u8,
                255,
            ]
        };
        image::Rgba([rgba[2], rgba[1], rgba[0], rgba[3]])
    });
    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
}

impl OnboardingPractice {
    pub(super) fn open_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.active || self.popup.is_some() || self.opening || !window.is_visible() {
            return;
        }
        if !Sample::Image.matches(&self.search.read(cx).value(), self.language) {
            return;
        }
        let Some(source) = self.source_bounds.get() else {
            return;
        };
        #[cfg(windows)]
        let (owner, screen, scale_factor, display_id) =
            match crate::native::preview::geometry(window) {
                Ok(geometry) => (
                    geometry.owner,
                    geometry.work_area,
                    geometry.scale_factor,
                    Some(geometry.display_id),
                ),
                Err(error) => {
                    tracing::warn!(%error, "Reading demo preview geometry failed");
                    return;
                }
            };
        #[cfg(not(windows))]
        let (owner, screen, scale_factor, display_id) = (
            window.bounds(),
            window
                .display(cx)
                .map_or(window.bounds(), |display| display.visible_bounds()),
            window.scale_factor(),
            window.display(cx).map(|display| display.id()),
        );
        let preference = HoverPreviewPreference::default();
        let dimensions = (i64::from(IMAGE_WIDTH), i64::from(IMAGE_HEIGHT));
        let anchor_y = owner.top() + window.mouse_position().y;
        let Some(initial) = crate::popup_geometry::hover_popup_bounds(
            owner,
            screen,
            preference.position,
            Some(dimensions),
            preference.expanded_image,
            anchor_y,
        ) else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.hovered = true;
        self.opening = true;
        let mut preview = HoverPreviewWindowView::new(
            (0, generation),
            self.language,
            Ok(PreviewContent::Text(String::new())),
            preference.zoom_step,
            hover_window::HoverPreviewSetup {
                geometry: crate::popup_geometry::PopupZoomGeometry {
                    initial,
                    owner,
                    display: screen,
                    anchor_y,
                    scale_factor,
                },
                source_bounds: Bounds::new(owner.origin + source.origin, source.size),
                information: hover_information::HoverInformation::in_memory_image(dimensions),
                image_dimensions: Some(dimensions),
            },
        );
        preview.memory_image = Some(self.image.clone());
        #[cfg(windows)]
        let main_native = native_window::window_id(window);
        cx.spawn(async move |owner, cx| {
            let valid = owner
                .update(cx, |this, _| {
                    this.active && this.generation == generation && this.hovered
                })
                .unwrap_or(false);
            if !valid {
                return;
            }
            match hover_popup::open_overlay(preview, display_id, cx) {
                Ok(handle) => {
                    let accepted = owner
                        .update(cx, |this, cx| {
                            if !this.active || this.generation != generation || !this.hovered {
                                return false;
                            }
                            this.opening = false;
                            this.popup = Some(handle);
                            let _ = handle.update(cx, |_, window, _| window.refresh());
                            #[cfg(windows)]
                            if let Some(main) = main_native {
                                this.track_pointer(main, scale_factor, cx);
                            }
                            true
                        })
                        .unwrap_or(false);
                    if !accepted {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Opening demo preview failed");
                    let _ = owner.update(cx, |this, cx| {
                        if this.generation == generation {
                            this.close_preview(cx);
                        }
                    });
                }
            }
        })
        .detach();
    }

    pub(super) fn update_anchor(&self, y: Pixels, cx: &mut Context<Self>) {
        if let Some(handle) = self.popup {
            let _ = handle.update(cx, |preview, _, cx| {
                let anchor = preview.zoom_geometry.owner.top() + y;
                if preview.zoom_geometry.anchor_y != anchor {
                    preview.zoom_geometry.anchor_y = anchor;
                    cx.notify();
                }
            });
        }
    }

    #[cfg(windows)]
    fn track_pointer(&mut self, main: isize, scale: f32, cx: &mut Context<Self>) {
        let generation = self.generation;
        self.pointer_task = Some(cx.spawn(async move |view, cx| {
            use clipboard_platform::external_drag as native;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let Ok(pointer) = native::poll_drag_pointer() else {
                    continue;
                };
                let position = native::drag_pointer_in_client(main, pointer, scale);
                let valid = view
                    .update(cx, |this, cx| {
                        if !this.active || this.generation != generation {
                            return false;
                        }
                        let Ok((x, y)) = position else {
                            this.close_preview(cx);
                            return false;
                        };
                        if !this
                            .source_bounds
                            .get()
                            .is_some_and(|bounds| bounds.contains(&point(px(x), px(y))))
                        {
                            this.close_preview(cx);
                            return false;
                        }
                        this.update_anchor(px(y), cx);
                        true
                    })
                    .unwrap_or(false);
                if !valid {
                    break;
                }
            }
        }));
    }
}
