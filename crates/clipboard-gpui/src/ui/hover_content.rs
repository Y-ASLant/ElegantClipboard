use super::*;
use gpui_kit::Size;
use gpui_kit::component::text::TextView;

/// HTML/Markdown 预览的宽度上限：TextView 在不定宽容器中会退化成 min-content
/// 宽度（逐字换行），所以始终给它一个确定宽度而不是 `max_w`。
const HTML_PREVIEW_MAX_WIDTH: f32 = 480.;

pub(super) fn fitted_size(
    view: &HoverPreviewWindowView,
    maximum: Size<Pixels>,
    window: &Window,
    cx: &App,
) -> Option<Size<Pixels>> {
    let body = match &view.content {
        Ok(PreviewContent::Html(_)) => view.content_size?,
        Ok(PreviewContent::Files(entries)) => size(
            px(460.).min(maximum.width),
            px(entries.len() as f32 * 58. + 12.),
        ),
        _ => shaped_size(&view.text, maximum.width, 22.4, window, cx),
    };
    let note_width = view.information.note.as_ref().map_or(px(0.), |note| {
        shaped_size(note, maximum.width, 20., window, cx).width
    });
    let width = body.width.max(note_width).min(maximum.width);
    let note = note_height(
        view.information.note.as_ref(),
        size(width, maximum.height),
        window,
        cx,
    );
    Some(size(width, (body.height + note).min(maximum.height)))
}

fn shaped_size(
    text: &SharedString,
    maximum_width: Pixels,
    line_height: f32,
    window: &Window,
    cx: &App,
) -> Size<Pixels> {
    let mut style = window.text_style();
    style.font_family = cx.theme().font_family.clone();
    let runs = [style.to_run(text.len())];
    let (width, count) = window
        .text_system()
        .shape_text(
            text.clone(),
            px(visual::TEXT_BODY),
            &runs,
            Some((maximum_width - px(visual::SPACE_3 * 2.)).max(px(1.))),
            None,
        )
        .map(|lines| {
            let width = lines
                .iter()
                .map(|line| line.width())
                .fold(px(0.), Pixels::max);
            let count = lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum::<usize>();
            (width, count)
        })
        .unwrap_or((px(0.), 1));
    size(
        // Text layout rounds its measured width up; keep the fitted surface at
        // least that wide so its final glyph cannot wrap after pixel snapping.
        (width.ceil() + px(visual::SPACE_3 * 2.))
            .max(px(48.))
            .min(maximum_width),
        px(count.max(1) as f32 * line_height + visual::SPACE_2 * 2.).max(px(46.)),
    )
}

pub(super) fn html_content(
    view: &HoverPreviewWindowView,
    maximum_width: Pixels,
    cx: &Context<HoverPreviewWindowView>,
) -> AnyElement {
    let entity = cx.entity().downgrade();
    let generation = view.generation;
    let width = maximum_width.min(px(HTML_PREVIEW_MAX_WIDTH));
    div()
        .relative()
        .flex_none()
        .w(width)
        .px(px(visual::SPACE_3))
        .py(px(visual::SPACE_2))
        .text_size(px(visual::TEXT_BODY))
        .line_height(px(22.4))
        .child(
            TextView::markdown("hover-preview-rich-text", view.text.clone())
                .selectable(false)
                .scrollable(false),
        )
        .child(
            canvas(
                move |bounds, window, cx| {
                    let Some(entity) = entity.upgrade() else {
                        return;
                    };
                    let view = entity.read(cx);
                    if view.generation != generation || view.content_size == Some(bounds.size) {
                        return;
                    }
                    window.on_next_frame(move |_, cx| {
                        entity.update(cx, |view, cx| {
                            if view.generation == generation
                                && view.content_size != Some(bounds.size)
                            {
                                view.content_size = Some(bounds.size);
                                cx.notify();
                            }
                        });
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .into_any_element()
}

pub(super) fn note_height(
    note: Option<&SharedString>,
    surface: Size<Pixels>,
    window: &Window,
    cx: &App,
) -> Pixels {
    let Some(note) = note.filter(|note| !note.is_empty()) else {
        return px(0.);
    };
    let mut style = window.text_style();
    style.font_family = cx.theme().font_family.clone();
    let runs = [style.to_run(note.len())];
    let lines = window
        .text_system()
        .shape_text(
            note.clone(),
            px(visual::TEXT_BODY),
            &runs,
            Some((surface.width - px(visual::SPACE_3 * 2.)).max(px(1.))),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum::<usize>()
        })
        .unwrap_or(1);
    px(lines.clamp(1, 6) as f32 * 20. + visual::SPACE_2 * 2.).min(surface.height * 0.3)
}

pub(super) fn render_content(
    view: &HoverPreviewWindowView,
    image: Option<PathBuf>,
    dimensions: Size<Pixels>,
    preferred: Size<Pixels>,
    cx: &Context<HoverPreviewWindowView>,
) -> AnyElement {
    let image = view
        .memory_image
        .clone()
        .map(ImageSource::from)
        .or_else(|| image.map(ImageSource::from));
    if let Some(image) = image {
        return img(image)
            .flex_none()
            .w(dimensions.width)
            .h(dimensions.height)
            .rounded_b(cx.theme().radius_lg)
            .when(view.information.note.is_none(), |this| {
                this.rounded_t(cx.theme().radius_lg)
            })
            .object_fit(ObjectFit::Contain)
            .into_any_element();
    }
    let body = div()
        .id("hover-preview-content")
        .flex_none()
        .w(dimensions.width)
        .h(dimensions.height)
        .overflow_y_scroll()
        .track_scroll(&view.scroll)
        .when(
            !matches!(view.content, Ok(PreviewContent::Html(_))),
            |this| this.px(px(visual::SPACE_3)).py(px(visual::SPACE_2)),
        )
        .text_size(px(visual::TEXT_BODY))
        .line_height(px(22.4))
        .whitespace_normal();
    match &view.content {
        Ok(PreviewContent::Html(_)) => body
            .child(html_content(view, preferred.width, cx))
            .into_any_element(),
        Ok(PreviewContent::Files(entries)) => body
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_1))
            .children(
                entries
                    .iter()
                    .map(|entry| file_row(entry, view.language, cx)),
            )
            .into_any_element(),
        _ => body
            .when(view.content.is_err(), |this| {
                this.text_color(cx.theme().danger)
            })
            .child(view.text.clone())
            .into_any_element(),
    }
}

fn file_row(entry: &FilePreviewEntry, language: LanguagePreference, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let unavailable = !entry.exists || entry.metadata_error.is_some();
    let color = if unavailable {
        theme.danger
    } else {
        theme.foreground
    };
    let path = Path::new(&entry.original_path);
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    let status = if !entry.exists {
        tr(language, "文件不存在", "Missing file")
    } else if entry.metadata_error.is_some() {
        tr(language, "无法读取", "Unavailable")
    } else if entry.is_dir {
        tr(language, "文件夹", "Folder")
    } else {
        tr(language, "文件", "File")
    };
    div()
        .flex_none()
        .w_full()
        .min_h(px(54.))
        .flex()
        .items_center()
        .gap(px(visual::SPACE_2))
        .px(px(visual::SPACE_2))
        .py(px(visual::SPACE_1))
        .rounded(theme.radius)
        .border(px(visual::BORDER_WIDTH))
        .border_color(if unavailable {
            theme.danger.opacity(0.55)
        } else {
            theme.border
        })
        .bg(if unavailable {
            theme.danger.opacity(0.12)
        } else {
            theme.group_box
        })
        .child(
            Icon::new(if entry.is_dir {
                IconName::Folder
            } else {
                IconName::File
            })
            .size(px(visual::SPACE_6))
            .text_color(color),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(visual::TEXT_BODY))
                        .text_color(color)
                        .child(name.into_owned()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(visual::TEXT_SMALL))
                        .text_color(theme.muted_foreground)
                        .child(entry.original_path.clone()),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .text_size(px(visual::TEXT_SMALL))
                .text_color(if unavailable {
                    theme.danger
                } else {
                    theme.muted_foreground
                })
                .when_some(entry.size, |this, bytes| this.child(format_bytes(bytes)))
                .child(status),
        )
        .into_any_element()
}
