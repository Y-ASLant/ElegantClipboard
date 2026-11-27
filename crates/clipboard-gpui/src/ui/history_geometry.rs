use super::*;
use crate::state::HistoryItems;
use std::ops::Range;

fn history_row_height(
    item: &DbClipboardItem,
    display: DisplayPreference,
    font_size: u8,
    search: &str,
) -> f32 {
    let density = display.card_density;
    let note = card_note::note_title(item);
    let (thumbnail_width, thumbnail_height) = match item.content_type.as_str() {
        "image" => visual::thumbnail_size(density),
        "files" => (visual::FILE_THUMBNAIL_WIDTH, visual::FILE_THUMBNAIL_HEIGHT),
        _ => (0., 0.),
    };
    let font_size = f32::from(font_size);
    let title_lines = if note.is_none() && item.content_type == "image" {
        1
    } else {
        // The thumbnail shares the title row; the note marker lives in the footer.
        let mut width = visual::TEXT_COLUMNS_PER_LINE as f32 * visual::TEXT_BODY / 2.;
        if thumbnail_width > 0. {
            width -= thumbnail_width + visual::SPACE_3;
        }
        let columns = (width / (font_size / 2.)).ceil() as usize;
        visual::capped_preview_lines(
            note.or(item.preview.as_deref()).unwrap_or_default(),
            display.card_max_lines,
            columns,
        )
    };
    let mut text_height = f32::from(title_lines) * font_size * visual::COPY_LINE_HEIGHT;
    if !search.is_empty() && card_note::matching_source(item, search).is_some() {
        text_height += visual::PREVIEW_LINE_HEIGHT;
    }
    visual::row_height_for_body(density, thumbnail_height.max(text_height))
}

fn history_row_layout(
    items: &HistoryItems,
    display: DisplayPreference,
    font_size: u8,
    search: &str,
) -> (Rc<Vec<gpui_kit::Size<Pixels>>>, Vec<Pixels>) {
    let mut sizes = Vec::with_capacity(items.len());
    let mut offsets = Vec::with_capacity(items.len() + 1);
    let mut top = px(0.);
    offsets.push(top);
    let trailing = px(visual::last_row_extra(display.card_density));
    // Rows beyond the loaded pages are at least one preview line tall: the
    // estimate self-corrects as pages arrive, and the error amortizes across
    // the position independent of scroll direction.
    let gap = px(visual::row_height_for_body(
        display.card_density,
        f32::from(font_size) * visual::COPY_LINE_HEIGHT,
    ));
    for (index, item) in items.iter().enumerate() {
        let mut height = item.as_ref().map_or(gap, |item| {
            px(history_row_height(item, display, font_size, search))
        });
        if index + 1 == items.len() {
            height += trailing;
        }
        sizes.push(size(px(0.), height));
        top += height;
        offsets.push(top);
    }
    (Rc::new(sizes), offsets)
}

impl ClipboardView {
    /// The trailing page inset belongs to the virtual list, not the hover source.
    pub(super) fn history_hover_row(&self, index: usize) -> Option<Range<Pixels>> {
        let top = *self.row_offsets.get(index)?;
        let mut bottom = *self.row_offsets.get(index + 1)?;
        if index + 1 == self.row_sizes.len() {
            bottom -= px(visual::last_row_extra(self.display.value.card_density));
        }
        Some(top..bottom)
    }

    pub(super) fn rebuild_row_layout(&mut self) {
        (self.row_sizes, self.row_offsets) = history_row_layout(
            &self.history.items,
            self.display.value,
            self.fonts.value.card_font_size,
            &self.row_search,
        );
        self.drag_motion = DragMotionState::default();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn note_title_and_separate_match_share_the_body_with_the_thumbnail() {
        use super::{CardDensity, DisplayPreference, history_row_height, visual};
        use clipboard_core::History;

        let dir = tempfile::tempdir().unwrap();
        let db = History::open(dir.path().join("clipboard.db")).unwrap();
        let mut item = db
            .item(db.capture("source word").unwrap().unwrap())
            .unwrap();
        let display = DisplayPreference {
            card_max_lines: 3,
            card_density: CardDensity::Compact,
            ..Default::default()
        };
        let line = visual::PREVIEW_LINE_HEIGHT;
        let height = |body| visual::row_height_for_body(display.card_density, body);
        item.note = Some("title\nsecond\nthird\nclamped".into());
        for kind in ["text", "image", "files"] {
            item.content_type = kind.into();
            for (query, lines) in [("", 3.), ("title", 3.), ("word", 4.)] {
                let actual = history_row_height(&item, display, 14, query);
                assert!(
                    (actual - height(lines * line)).abs() < 0.001,
                    "{kind} {query}"
                );
            }
        }
        item.content_type = "image".into();
        item.note = Some("short".into());
        let thumbnail = visual::thumbnail_size(display.card_density).1;
        assert_eq!(
            history_row_height(&item, display, 14, "word"),
            height(thumbnail)
        );
    }
}
