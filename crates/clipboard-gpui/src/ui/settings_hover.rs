use super::settings_common::{
    choice_tabs, owner_state, pref_checkbox, settings_field, settings_field_help, settings_inline,
    settings_number_field, settings_stack,
};
use super::*;
use gpui_kit::component::input::NumberInput;

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
        let hover_preference = owner_state.hover_preference.value;
        let hover_pending = owner_state.hover_preference.pending;
        settings_stack()
            .child(
                settings_field(tr(language, "预览内容", "Preview content")).child(
                    settings_inline().children(
                        [
                            (
                                "hover-image",
                                tr(language, "图片", "Images"),
                                hover_preference.image,
                                (|next: &mut HoverPreviewPreference, checked| next.image = checked)
                                    as fn(&mut HoverPreviewPreference, bool),
                            ),
                            (
                                "hover-text",
                                tr(language, "文本", "Text"),
                                hover_preference.text,
                                |next: &mut HoverPreviewPreference, checked| next.text = checked,
                            ),
                            (
                                "hover-files",
                                tr(language, "文件", "Files"),
                                hover_preference.files,
                                |next: &mut HoverPreviewPreference, checked| next.files = checked,
                            ),
                        ]
                        .map(|(id, label, selected, set_checked)| {
                            pref_checkbox(
                                cx,
                                id,
                                label,
                                selected,
                                hover_pending,
                                &self.owner,
                                |owner| owner.hover_preference.value,
                                set_checked,
                                ClipboardView::save_hover_preference,
                            )
                            .min_w_0()
                        }),
                    ),
                ),
            )
            .child(
                pref_checkbox(
                    cx,
                    "hover-expanded-image",
                    tr(language, "大图使用更大浮窗", "Expand image hover window"),
                    hover_preference.expanded_image,
                    hover_pending || !hover_preference.image,
                    &self.owner,
                    |owner| owner.hover_preference.value,
                    |next, checked| next.expanded_image = checked,
                    ClipboardView::save_hover_preference,
                )
                .min_w_0(),
            )
            .child(
                settings_field_help(
                    tr(language, "停留延时", "Hover delay"),
                    tr(language, "100–2000 毫秒。", "100–2000 ms."),
                    "hover-delay-help",
                )
                .child(settings_number_field(
                    NumberInput::new(self.hover_delay.input()),
                    hover_pending,
                )),
            )
            .child(
                settings_field(tr(language, "浮窗位置", "Position")).child(choice_tabs(
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
                    |owner| owner.hover_preference.value.position,
                    |owner, position, cx| {
                        owner.save_hover_preference(
                            HoverPreviewPreference {
                                position,
                                ..owner.hover_preference.value
                            },
                            cx,
                        );
                    },
                    cx,
                )),
            )
            .child(
                settings_field_help(
                    tr(language, "图片缩放步进", "Image zoom step"),
                    tr(language, "5–50。", "5–50."),
                    "hover-zoom-help",
                )
                .child(settings_number_field(
                    NumberInput::new(self.hover_zoom.input()),
                    hover_pending,
                )),
            )
            .child(
                settings_field(tr(language, "预览信息", "Preview details")).child(
                    settings_inline().children(
                        detail_toggles(language, hover_preference.details)
                            .into_iter()
                            .map(|toggle| {
                                pref_checkbox(
                                    cx,
                                    toggle.id,
                                    toggle.label,
                                    toggle.selected,
                                    hover_pending || toggle.disabled,
                                    &self.owner,
                                    |owner| owner.hover_preference.value,
                                    move |next, checked| (toggle.set)(&mut next.details, checked),
                                    ClipboardView::save_hover_preference,
                                )
                                .min_w_0()
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
    disabled: bool,
    set: fn(&mut HoverPreviewDetails, bool),
}

fn detail_toggle(
    id: &'static str,
    label: &'static str,
    selected: bool,
    set: fn(&mut HoverPreviewDetails, bool),
) -> DetailToggle {
    DetailToggle {
        id,
        label,
        selected,
        disabled: false,
        set,
    }
}

fn detail_toggles(language: LanguagePreference, details: HoverPreviewDetails) -> Vec<DetailToggle> {
    vec![
        detail_toggle(
            "hover-detail-kind",
            tr(language, "类型", "Type"),
            details.kind,
            |details, checked| details.kind = checked,
        ),
        detail_toggle(
            "hover-detail-size",
            tr(language, "存储大小", "Stored size"),
            details.size,
            |details, checked| details.size = checked,
        ),
        detail_toggle(
            "hover-detail-resolution",
            tr(language, "分辨率", "Resolution"),
            details.resolution,
            |details, checked| details.resolution = checked,
        ),
        detail_toggle(
            "hover-detail-amount",
            tr(language, "数量", "Counts"),
            details.amount,
            |details, checked| details.amount = checked,
        ),
        detail_toggle(
            "hover-detail-name",
            tr(language, "名称", "Name"),
            details.name,
            |details, checked| details.name = checked,
        ),
        detail_toggle(
            "hover-detail-path",
            tr(language, "路径", "Path"),
            details.path,
            |details, checked| details.path = checked,
        ),
        detail_toggle(
            "hover-detail-extension",
            tr(language, "扩展名", "Extension"),
            details.extension,
            |details, checked| details.extension = checked,
        ),
        detail_toggle(
            "hover-detail-formats",
            tr(language, "数据格式", "Data format"),
            details.formats,
            |details, checked| details.formats = checked,
        ),
        detail_toggle(
            "hover-detail-source",
            tr(language, "来源", "Source"),
            details.source,
            |details, checked| details.source = checked,
        ),
        DetailToggle {
            id: "hover-detail-source-url",
            label: tr(language, "来源网址", "Source URL"),
            selected: details.source_url,
            disabled: !details.source,
            set: |details, checked| details.source_url = checked,
        },
        detail_toggle(
            "hover-detail-time",
            tr(language, "采集时间", "Captured at"),
            details.captured_at,
            |details, checked| details.captured_at = checked,
        ),
        detail_toggle(
            "hover-detail-hint",
            tr(language, "操作提示", "Hint"),
            details.hint,
            |details, checked| details.hint = checked,
        ),
    ]
}
