use super::*;
use gpui_kit::Size;

pub(super) struct HoverPreviewSetup {
    pub geometry: crate::popup_geometry::PopupZoomGeometry,
    pub source_bounds: Bounds<Pixels>,
    pub information: hover_information::HoverInformation,
    pub image_dimensions: Option<(i64, i64)>,
}

pub(super) struct HoverPreviewWindowView {
    pub id: i64,
    pub generation: u64,
    pub language: LanguagePreference,
    pub content: Result<PreviewContent, String>,
    pub text: SharedString,
    pub zoom: f32,
    pub zoom_geometry: crate::popup_geometry::PopupZoomGeometry,
    pub zoom_step: u8,
    pub image_scale: Option<f32>,
    pub source_bounds: Bounds<Pixels>,
    pub information: hover_information::HoverInformation,
    pub scroll: ScrollHandle,
    pub surface_bounds: Option<Bounds<Pixels>>,
    pub content_size: Option<Size<Pixels>>,
}

impl HoverPreviewWindowView {
    /// Source-card Ctrl+wheel adjusts the visible image or scrolls text. The
    /// transparent native window remains work-area sized throughout the gesture.
    pub(super) fn zoom_by_wheel(
        &mut self,
        delta: Pixels,
        _: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let direction = f32::from(delta).signum();
        if !direction.is_finite() || delta == px(0.) {
            return false;
        }
        if self.image_scale.is_some() {
            let base = f32::from(
                self.zoom_geometry
                    .initial
                    .size
                    .width
                    .max(self.zoom_geometry.initial.size.height),
            );
            // QcClip changes its 480px base by 48px per wheel step. Existing
            // user sensitivity remains meaningful (10 => the same 48px step).
            let requested = self.zoom + direction * 4.8 * f32::from(self.zoom_step) / base;
            let (_, next) = self.zoom_geometry.scaled(requested);
            self.zoom = next;
            tracing::debug!(
                id = self.id,
                generation = self.generation,
                zoom = next,
                "Preview surface zoom changed"
            );
        } else {
            let offset = self.scroll.offset();
            self.scroll.set_offset(point(
                offset.x,
                (offset.y + px(direction * 120.)).min(px(0.)),
            ));
        }
        cx.notify();
        true
    }

    pub(super) fn new(
        request: (i64, u64),
        language: LanguagePreference,
        content: Result<PreviewContent, String>,
        zoom_step: u8,
        setup: HoverPreviewSetup,
    ) -> Self {
        let text = match &content {
            Ok(
                PreviewContent::Text(text)
                | PreviewContent::RichText(text)
                | PreviewContent::Html(text),
            ) => text.clone().into(),
            Ok(PreviewContent::Files(entries)) => entries
                .iter()
                .map(|entry| entry.original_path.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
            Err(error) => error.clone().into(),
            Ok(PreviewContent::Image(_)) => SharedString::default(),
        };
        Self {
            id: request.0,
            generation: request.1,
            language,
            content,
            text,
            zoom: 1.,
            zoom_geometry: setup.geometry,
            zoom_step,
            image_scale: setup
                .image_dimensions
                .filter(|(width, height)| *width > 0 && *height > 0)
                .map(|(width, _)| {
                    f32::from(setup.geometry.initial.size.width) * setup.geometry.scale_factor
                        / width as f32
                }),
            source_bounds: setup.source_bounds,
            information: setup.information,
            content_size: None,
            scroll: ScrollHandle::new(),
            surface_bounds: None,
        }
    }
}

impl Render for HoverPreviewWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        hover_surface::render_preview(self, window, cx)
    }
}
