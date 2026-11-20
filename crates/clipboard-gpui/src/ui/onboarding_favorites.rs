use super::super::*;
use drag_visual::HistoryCardSnapshot;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Progress {
    #[default]
    Ready,
    Saved,
    Removed,
}

pub(in crate::ui) struct OnboardingFavorites {
    sample: Rc<HistoryCardSnapshot>,
    sample_size: Rc<Cell<gpui_kit::Size<Pixels>>>,
    drop_zone: Rc<FavoriteDropZone>,
    favorites_only: bool,
    progress: Progress,
    dragging: bool,
    _subscriptions: [Subscription; 2],
}

impl OnboardingFavorites {
    pub(super) fn new(
        language: LanguagePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = [
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_drag(window, cx);
                }
            }),
            cx.observe_window_visibility(window, |this, visibility, window, cx| {
                if !visibility.is_visible() {
                    this.cancel_drag(window, cx);
                }
            }),
        ];
        Self {
            sample: demo_card(language),
            sample_size: Rc::new(Cell::new(size(px(0.), px(0.)))),
            drop_zone: Rc::new(FavoriteDropZone::default()),
            favorites_only: false,
            progress: Progress::Ready,
            dragging: false,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn set_language(&mut self, language: LanguagePreference, cx: &mut Context<Self>) {
        if self.sample.language != language {
            self.sample = demo_card(language);
            cx.notify();
        }
    }

    pub(super) fn cancel_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !std::mem::take(&mut self.dragging) {
            return false;
        }
        self.drop_zone.hovered.set(false);
        self.drop_zone.bounds.set(None);
        cx.stop_active_drag(window);
        cx.notify();
        true
    }

    fn set_favorite_from_tab(&mut self, cx: &mut Context<Self>) {
        self.progress = match (self.favorites_only, self.progress) {
            (true, Progress::Saved) => Progress::Removed,
            (false, Progress::Ready | Progress::Removed) => Progress::Saved,
            _ => return,
        };
        cx.notify();
    }

    fn instruction(&self) -> &'static str {
        let language = self.sample.language;
        match (self.favorites_only, self.progress) {
            (false, Progress::Saved) => tr(
                language,
                "已收藏。切到「收藏」，再拖一次试试。",
                "Saved. Switch to Favorites, then try dragging the card again.",
            ),
            (true, Progress::Saved) => tr(
                language,
                "把卡片拖回上方虚线区，取消收藏。",
                "Drag the card to the dashed area above to remove it.",
            ),
            (true, Progress::Removed) => tr(
                language,
                "已取消收藏，卡片仍在「全部」。",
                "Removed from Favorites. The card is still in All.",
            ),
            (true, Progress::Ready) => tr(
                language,
                "回到「全部」，先收藏一张示例卡片。",
                "Go to All to add the sample card first.",
            ),
            (false, _) => tr(
                language,
                "按住卡片，拖到上方出现的虚线区后松手。",
                "Hold the card, drag it to the dashed area above, then release.",
            ),
        }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let language = self.sample.language;
        if self.dragging {
            self.drop_zone
                .strip(language, self.favorites_only, cx)
                .into_any_element()
        } else {
            TabBar::new("onboarding-favorite-tabs")
                .segmented()
                .w_full()
                .h(px(visual::CONTROL_HEIGHT))
                .last_empty_space(div().w_0())
                .selected_index(usize::from(self.favorites_only))
                .children(
                    [
                        tr(language, "全部", "All"),
                        tr(language, "收藏", "Favorites"),
                    ]
                    .map(|label| Tab::new().label(label).flex_1()),
                )
                .on_click(cx.listener(|this, index: &usize, _, cx| {
                    this.favorites_only = *index == 1;
                    cx.notify();
                }))
                .into_any_element()
        }
    }

    fn card(&self, cx: &mut Context<Self>) -> AnyElement {
        let dimensions = self.sample_size.clone();
        let owner = cx.entity();
        div()
            .id("onboarding-favorite-sample")
            .relative()
            .size_full()
            .cursor_grab()
            .on_drag(FavoriteSample, move |_, _, _, cx| {
                let preview = owner.update(cx, |this, cx| {
                    this.dragging = true;
                    cx.notify();
                    FavoritePreview {
                        sample: this.sample.clone(),
                        dimensions: this.sample_size.get(),
                        zone: this.drop_zone.clone(),
                        removing: this.favorites_only,
                    }
                });
                cx.new(|_| preview)
            })
            .child(self.sample.surface(false, false, None, cx))
            .child(
                canvas(
                    move |bounds, _, _| dimensions.set(bounds.size),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(self.dragging, |source| source.opacity(0.))
            .into_any_element()
    }
}

impl Render for OnboardingFavorites {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.sample.language;
        let saved = self.progress == Progress::Saved;
        let can_change = self.favorites_only == saved;
        let owner = cx.weak_entity();
        settings_common::settings_stack()
            .id("onboarding-favorite-practice")
            .gap(px(visual::SPACE_3))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<FavoriteSample>, window, cx| {
                    if event.event.pressed_button != Some(MouseButton::Left) {
                        this.cancel_drag(window, cx);
                    } else {
                        let hovered = this.drop_zone.contains(event.event.position);
                        if this.drop_zone.hovered.replace(hovered) != hovered {
                            cx.notify();
                        }
                    }
                }),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let owner = owner.clone();
                        // Observe release outside the practice area too; only the visible strip accepts it.
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.dragging && this.drop_zone.contains(event.position) {
                                        this.set_favorite_from_tab(cx);
                                    }
                                    this.cancel_drag(window, cx);
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size(px(0.)),
            )
            .child(
                settings_common::settings_stack()
                    .gap(px(visual::SPACE_2))
                    .p(px(visual::SPACE_3))
                    .rounded(cx.theme().radius)
                    .border(px(visual::BORDER_WIDTH))
                    .border_color(crate::design_system::container_border(cx))
                    .bg(cx.theme().group_box)
                    .child(self.toolbar(cx))
                    .child(
                        div()
                            .h(px(visual::row_height(self.sample.display.card_density, 2)))
                            .child(if !self.favorites_only || saved {
                                self.card(cx)
                            } else {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(visual::TEXT_SMALL))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(language, "暂无示例收藏", "No favorite samples"))
                                    .into_any_element()
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(visual::SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(settings_common::settings_description(
                                self.instruction(),
                                cx,
                            )),
                    )
                    .when(can_change, |row| {
                        row.child(
                            Button::new("onboarding-favorite-action")
                                .small()
                                .outline()
                                .flex_none()
                                .disabled(self.dragging)
                                .label(if self.favorites_only {
                                    tr(language, "取消收藏", "Remove favorite")
                                } else {
                                    tr(language, "收藏示例", "Favorite sample")
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.set_favorite_from_tab(cx)),
                                ),
                        )
                    }),
            )
            .child(settings_common::settings_description(
                tr(
                    language,
                    "仅供练习，不改动真实历史或系统剪贴板。",
                    "Practice only. Your history and system clipboard stay unchanged.",
                ),
                cx,
            ))
    }
}

struct FavoriteSample;

struct FavoritePreview {
    sample: Rc<HistoryCardSnapshot>,
    dimensions: gpui_kit::Size<Pixels>,
    zone: Rc<FavoriteDropZone>,
    removing: bool,
}

impl Render for FavoritePreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hint = self
            .zone
            .hovered
            .get()
            .then(|| favorite_drop::release_hint(self.sample.language, self.removing));
        div()
            .w(self.dimensions.width)
            .h(self.dimensions.height)
            .child(self.sample.surface(true, false, hint, cx))
    }
}

fn demo_card(language: LanguagePreference) -> Rc<HistoryCardSnapshot> {
    Rc::new(HistoryCardSnapshot {
        is_image: false,
        is_files: false,
        is_pinned: false,
        card_index: 0,
        source_app_name: None,
        source_app_icon: None,
        display: DisplayPreference {
            card_max_lines: 2,
            show_source_app: false,
            ..DisplayPreference::default()
        },
        language,
        summary: tr(language, "示例记录 · 文字", "Demo record · Text").into(),
        preview_text: tr(
            language,
            "项目灵感：整理常用链接",
            "Project ideas: collect useful links.",
        )
        .into(),
        highlights: Vec::new(),
        thumbnail_path: None,
        thumbnail: None,
        file_warning: false,
        file_icon: IconName::FileText,
        selected: false,
    })
}
