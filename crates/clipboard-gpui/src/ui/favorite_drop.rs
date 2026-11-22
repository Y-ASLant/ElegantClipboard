use super::*;

#[derive(Default)]
pub(super) struct FavoriteDropZone {
    pub(super) bounds: Cell<Option<Bounds<Pixels>>>,
    pub(super) hovered: Cell<bool>,
}

impl FavoriteDropZone {
    pub(super) fn contains(&self, pointer: Point<Pixels>) -> bool {
        self.bounds
            .get()
            .is_some_and(|bounds| bounds.contains(&pointer))
    }

    pub(super) fn strip(
        self: &Rc<Self>,
        language: LanguagePreference,
        removing: bool,
        cx: &App,
    ) -> Stateful<Div> {
        let zone = self.clone();
        let hovered = self.hovered.get();
        let theme = cx.theme();
        let icon = if removing {
            IconName::HeartOff
        } else {
            IconName::Heart
        };
        let label = if hovered {
            release_hint(language, removing)
        } else if removing {
            tr(
                language,
                "拖到这里取消收藏",
                "Drop to remove from favorites",
            )
        } else {
            tr(language, "拖到这里收藏", "Drop to favorite")
        };
        div()
            .id("favorite-drop")
            .relative()
            .w_full()
            .h(px(visual::CONTROL_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(visual::SPACE_2))
            .rounded(theme.radius)
            .border(px(visual::BORDER_WIDTH))
            .border_dashed()
            .border_color(theme.ring.opacity(if hovered { 1. } else { 0.7 }))
            .text_color(theme.accent_foreground)
            .text_size(visual::FILTER_LABEL_SIZE)
            .when(hovered, |target| target.bg(theme.accent))
            .child(Icon::new(icon).size(px(visual::ICON_SIZE)))
            .child(label)
            .child(
                canvas(
                    move |bounds, window, _| {
                        zone.bounds
                            .set(Some(bounds.intersect(&window.content_mask().bounds)));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }
}

pub(super) fn release_hint(language: LanguagePreference, removing: bool) -> &'static str {
    if removing {
        tr(language, "松手取消收藏", "Release to remove")
    } else {
        tr(language, "松手收藏", "Release to add")
    }
}
