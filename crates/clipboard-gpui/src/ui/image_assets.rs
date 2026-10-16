use super::*;
use gpui_kit::ImageSource;
use std::ops::Range;

const IMAGE_RELEASE_DELAY: Duration = Duration::from_secs(30);
const IMAGE_PREFETCH_ROWS: usize = 4;

fn image_cache_range(visible: Range<usize>, len: usize) -> Range<usize> {
    if visible.is_empty() || visible.start >= len {
        return 0..0;
    }
    visible.start.saturating_sub(IMAGE_PREFETCH_ROWS)
        ..visible.end.saturating_add(IMAGE_PREFETCH_ROWS).min(len)
}

fn retained_image_paths(
    rows: impl Iterator<Item = (Option<PathBuf>, Option<PathBuf>)>,
    pinned: impl IntoIterator<Item = PathBuf>,
) -> HashSet<PathBuf> {
    rows.flat_map(|(image, file)| image.into_iter().chain(file))
        .chain(pinned)
        .collect()
}

fn preview_image_path(preview: &PreviewState) -> Option<PathBuf> {
    match preview.result.as_ref()? {
        Ok(PreviewContent::Image(path)) => Some(path.clone()),
        Ok(PreviewContent::Files(entries)) => single_file_image_path(entries),
        _ => None,
    }
}

impl ClipboardView {
    pub(super) fn update_image_visible_range(
        &mut self,
        visible: Range<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.image_visible_range = visible;
        // VirtualList also invokes the renderer for a single measurement row.
        // Coalesce both callbacks before evicting any asset used by this frame.
        if !self.image_sync_pending {
            self.image_sync_pending = true;
            cx.defer_in(window, |this, _, cx| {
                this.image_sync_pending = false;
                this.sync_image_assets(cx);
            });
        }
    }

    /// Keep a small margin for reverse scrolling, plus resources shared by open previews.
    pub(super) fn sync_image_assets(&mut self, cx: &mut App) {
        let range = image_cache_range(self.image_visible_range.clone(), self.history.items.len());
        let rows = self.history.items[range].iter().map(|item| {
            let image = (item.content_type == "image")
                .then(|| item.image_path.as_ref().map(PathBuf::from))
                .flatten();
            let file = self
                .file_card_info
                .get(&item.id)
                .and_then(|info| info.image_path.clone());
            (image, file)
        });
        let mut pinned: Vec<_> = [&self.preview, &self.hover_preview]
            .into_iter()
            .filter_map(preview_image_path)
            .collect();
        if let Some(path) = self.drag_image_path() {
            pinned.push(path);
        }
        if cx.has_active_drag()
            && let Some(id) = self.drag_source_id()
        {
            if let Some(path) = self
                .history
                .items
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| item.image_path.as_ref())
            {
                pinned.push(PathBuf::from(path));
            }
            if let Some(path) = self
                .file_card_info
                .get(&id)
                .and_then(|info| info.image_path.clone())
            {
                pinned.push(path);
            }
        }
        let needed = retained_image_paths(rows, pinned);
        for path in self.rendered_images.difference(&needed) {
            ImageSource::from(path.clone()).remove_asset(cx);
        }
        self.rendered_images = needed;
    }

    pub(super) fn schedule_image_release(&mut self, cx: &mut Context<Self>) {
        self.image_release_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(IMAGE_RELEASE_DELAY).await;
            let _ = view.update(cx, |this, cx| {
                for path in std::mem::take(&mut this.rendered_images) {
                    ImageSource::from(path).remove_asset(cx);
                }
                this.image_visible_range = 0..0;
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::{image_cache_range, retained_image_paths};
    use std::{collections::HashSet, path::PathBuf};

    #[test]
    fn scrolling_releases_distant_images_but_keeps_shared_preview_paths() {
        let paths: Vec<_> = (0..30)
            .map(|id| PathBuf::from(format!("{id}.png")))
            .collect();
        let retain = |visible, pinned: Vec<PathBuf>| {
            retained_image_paths(
                paths[image_cache_range(visible, paths.len())]
                    .iter()
                    .map(|path| (Some(path.clone()), Some(path.clone()))),
                pinned,
            )
        };
        let first = retain(0..4, vec![]);
        assert_eq!(first.len(), 8);
        let second = retain(12..16, vec![paths[0].clone(), paths[0].clone()]);
        assert!(second.contains(&paths[0]));
        assert!(!second.contains(&paths[1]));
        assert!(second.contains(&paths[8]));
        assert!(second.contains(&paths[19]));
        assert_eq!(second.len(), 13);
        assert!(!retain(12..16, vec![]).contains(&paths[0]));
    }

    #[test]
    fn filtering_and_empty_viewports_do_not_retain_old_rows() {
        assert_eq!(image_cache_range(20..25, 3), 0..0);
        assert_eq!(image_cache_range(0..0, 100), 0..0);
        assert_eq!(image_cache_range(1..4, 5), 0..5);
        assert_eq!(image_cache_range(1..usize::MAX, 5), 0..5);
        let pinned = PathBuf::from("preview.png");
        assert_eq!(
            retained_image_paths(std::iter::empty(), [pinned.clone()]),
            HashSet::from([pinned])
        );
    }
}
