use super::settings_common::{
    choice_tabs, owner_state, settings_column, settings_column_field, settings_columns,
    settings_field, settings_inline,
};
use super::*;
use gpui_kit::component::checkbox::Checkbox;

impl SettingsWindowView {
    pub(super) fn settings_hover_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        if let Some(reason) = crate::capabilities::positioning_unavailable_reason(language) {
            return Alert::warning(
                "hover-position-capability",
                format!(
                    "{reason}\n{}",
                    tr(
                        language,
                        "悬停浮窗不可定位；仍可使用卡片预览按钮。",
                        "Use the card preview button instead of a positioned hover window.",
                    )
                ),
            )
            .small()
            .into_any_element();
        }
        let hover_preference = owner_state.hover_preference;
        let hover_pending = owner_state.hover_preference_pending;
        let expanded_hover_owner = self.owner.clone();
        settings_columns()
            .child(
                settings_column()
                    .child(
                        settings_field(tr(language, "预览内容", "Preview content")).child(
                            settings_inline().children(
                                [
                                    (
                                        "hover-image",
                                        tr(language, "图片", "Images"),
                                        hover_preference.image,
                                        (|next: &mut HoverPreviewPreference, checked| {
                                            next.image = checked
                                        })
                                            as fn(&mut HoverPreviewPreference, bool),
                                    ),
                                    (
                                        "hover-text",
                                        tr(language, "文本", "Text"),
                                        hover_preference.text,
                                        |next: &mut HoverPreviewPreference, checked| {
                                            next.text = checked
                                        },
                                    ),
                                    (
                                        "hover-files",
                                        tr(language, "文件", "Files"),
                                        hover_preference.files,
                                        |next: &mut HoverPreviewPreference, checked| {
                                            next.files = checked
                                        },
                                    ),
                                ]
                                .map(
                                    |(id, label, selected, set_checked)| {
                                        let owner = self.owner.clone();
                                        Checkbox::new(id)
                                            .min_w_0()
                                            .min_h(px(32.))
                                            .py(px(6.))
                                            .text_size(px(14.))
                                            .whitespace_normal()
                                            .label(label)
                                            .checked(selected)
                                            .disabled(hover_pending)
                                            .on_change(move |checked, _, cx| {
                                                let _ = owner.update(cx, |owner, cx| {
                                                    let mut next = owner.hover_preference;
                                                    set_checked(&mut next, *checked);
                                                    owner.save_hover_preference(next, cx);
                                                });
                                            })
                                    },
                                ),
                            ),
                        ),
                    )
                    .child(
                        Checkbox::new("hover-expanded-image")
                            .min_w_0()
                            .min_h(px(32.))
                            .py(px(6.))
                            .text_size(px(14.))
                            .whitespace_normal()
                            .label(tr(
                                language,
                                "大图使用更大浮窗",
                                "Expand image hover window",
                            ))
                            .checked(hover_preference.expanded_image)
                            .disabled(hover_pending || !hover_preference.image)
                            .on_change(move |checked, _, cx| {
                                let _ = expanded_hover_owner.update(cx, |owner, cx| {
                                    owner.save_hover_preference(
                                        HoverPreviewPreference {
                                            expanded_image: *checked,
                                            ..owner.hover_preference
                                        },
                                        cx,
                                    );
                                });
                            }),
                    ),
            )
            .child(
                settings_column_field(tr(language, "停留延时", "Hover delay")).child(choice_tabs(
                    "hover-delay",
                    &self.owner,
                    [250, 500, 750, 1000],
                    ["250 ms", "500 ms", "750 ms", "1000 ms"],
                    hover_preference.delay_ms,
                    hover_pending,
                    |owner| owner.hover_preference.delay_ms,
                    |owner, delay, cx| {
                        owner.save_hover_preference(
                            HoverPreviewPreference {
                                delay_ms: delay,
                                ..owner.hover_preference
                            },
                            cx,
                        );
                    },
                )),
            )
            .child(
                settings_column_field(tr(language, "浮窗位置", "Position")).child(choice_tabs(
                    "hover-position",
                    &self.owner,
                    [
                        HoverPreviewPosition::Auto,
                        HoverPreviewPosition::Left,
                        HoverPreviewPosition::Right,
                    ],
                    [
                        tr(language, "自动", "Auto"),
                        tr(language, "左侧", "Left"),
                        tr(language, "右侧", "Right"),
                    ],
                    hover_preference.position,
                    hover_pending,
                    |owner| owner.hover_preference.position,
                    |owner, position, cx| {
                        owner.save_hover_preference(
                            HoverPreviewPreference {
                                position,
                                ..owner.hover_preference
                            },
                            cx,
                        );
                    },
                )),
            )
            .child(
                settings_column_field(tr(language, "图片缩放步进", "Image zoom step")).child(
                    choice_tabs(
                        "hover-zoom-step",
                        &self.owner,
                        [5, 10, 20, 50],
                        ["5%", "10%", "20%", "50%"],
                        hover_preference.zoom_step,
                        hover_pending,
                        |owner| owner.hover_preference.zoom_step,
                        |owner, zoom_step, cx| {
                            owner.save_hover_preference(
                                HoverPreviewPreference {
                                    zoom_step,
                                    ..owner.hover_preference
                                },
                                cx,
                            );
                        },
                    ),
                ),
            )
            .child(
                settings_column_field(tr(language, "预览信息", "Preview details")).child(
                    settings_inline().children(
                        detail_toggles(language, hover_preference.details)
                            .into_iter()
                            .map(|toggle| {
                                let owner = self.owner.clone();
                                Checkbox::new(toggle.id)
                                    .min_w_0()
                                    .min_h(px(32.))
                                    .py(px(6.))
                                    .text_size(px(14.))
                                    .whitespace_normal()
                                    .label(toggle.label)
                                    .checked(toggle.selected)
                                    .disabled(hover_pending)
                                    .on_change(move |checked, _, cx| {
                                        let _ = owner.update(cx, |owner, cx| {
                                            let mut details = owner.hover_preference.details;
                                            (toggle.set)(&mut details, *checked);
                                            owner.save_hover_preference(
                                                HoverPreviewPreference {
                                                    details,
                                                    ..owner.hover_preference
                                                },
                                                cx,
                                            );
                                        });
                                    })
                            }),
                    ),
                ),
            )
            .into_any_element()
    }
}

struct DetailToggle {
    id: &'static str,
    label: &'static str,
    selected: bool,
    set: fn(&mut HoverPreviewDetails, bool),
}

fn detail_toggles(language: LanguagePreference, details: HoverPreviewDetails) -> Vec<DetailToggle> {
    vec![
        DetailToggle {
            id: "hover-detail-kind",
            label: tr(language, "类型", "Type"),
            selected: details.kind,
            set: |details, checked| details.kind = checked,
        },
        DetailToggle {
            id: "hover-detail-size",
            label: tr(language, "存储大小", "Stored size"),
            selected: details.size,
            set: |details, checked| details.size = checked,
        },
        DetailToggle {
            id: "hover-detail-resolution",
            label: tr(language, "分辨率", "Resolution"),
            selected: details.resolution,
            set: |details, checked| details.resolution = checked,
        },
        DetailToggle {
            id: "hover-detail-amount",
            label: tr(language, "数量", "Counts"),
            selected: details.amount,
            set: |details, checked| details.amount = checked,
        },
        DetailToggle {
            id: "hover-detail-name",
            label: tr(language, "名称", "Name"),
            selected: details.name,
            set: |details, checked| details.name = checked,
        },
        DetailToggle {
            id: "hover-detail-path",
            label: tr(language, "路径", "Path"),
            selected: details.path,
            set: |details, checked| details.path = checked,
        },
        DetailToggle {
            id: "hover-detail-extension",
            label: tr(language, "扩展名", "Extension"),
            selected: details.extension,
            set: |details, checked| details.extension = checked,
        },
        DetailToggle {
            id: "hover-detail-formats",
            label: tr(language, "数据格式", "Data format"),
            selected: details.formats,
            set: |details, checked| details.formats = checked,
        },
        DetailToggle {
            id: "hover-detail-source",
            label: tr(language, "来源", "Source"),
            selected: details.source,
            set: |details, checked| details.source = checked,
        },
        DetailToggle {
            id: "hover-detail-time",
            label: tr(language, "采集时间", "Captured at"),
            selected: details.captured_at,
            set: |details, checked| details.captured_at = checked,
        },
        DetailToggle {
            id: "hover-detail-hint",
            label: tr(language, "操作提示", "Hint"),
            selected: details.hint,
            set: |details, checked| details.hint = checked,
        },
    ]
}
