use super::settings_common::{
    owner_state, settings_description, settings_field, settings_inline, settings_number_field,
    settings_stack,
};
use super::*;
use clipboard_core::preferences::FontPreference;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::NumberInput;
use gpui_kit::component::searchable_list::{SearchableListItem, SearchableVec};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FontPickerTarget {
    Ui,
    Card,
    Preview,
}

impl FontPickerTarget {
    fn default_label(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::Ui => tr(language, "系统默认", "System default"),
            Self::Card => tr(language, "跟随界面字体", "Follow interface font"),
            Self::Preview => tr(language, "跟随卡片字体", "Follow card font"),
        }
    }
}

/// 下拉条目：空字体名保留为默认或继承选项的存储值。
#[derive(Clone)]
pub(super) struct FontItem {
    family: SharedString,
    label: SharedString,
}

impl SearchableListItem for FontItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.family
    }
}

pub(super) type FontSelectState = SelectState<SearchableVec<FontItem>>;

fn font_items(default_label: &'static str, saved_family: &str) -> Vec<FontItem> {
    let families = crate::fonts::system_font_families();
    let families = &families[..families.len().min(400)];
    let include_saved =
        !saved_family.is_empty() && !families.iter().any(|name| name.as_str() == saved_family);
    let mut items = Vec::with_capacity(families.len() + 1 + usize::from(include_saved));
    items.push(FontItem {
        family: "".into(),
        label: default_label.into(),
    });
    // Keep saved fonts selectable even if uninstalled or outside the displayed list.
    if include_saved {
        let family: SharedString = saved_family.into();
        items.push(FontItem {
            label: family.clone(),
            family,
        });
    }
    items.extend(families.iter().map(|name| FontItem {
        family: name.clone(),
        label: name.clone(),
    }));
    items
}

fn saved_family(fonts: &FontPreference, target: FontPickerTarget) -> &str {
    match target {
        FontPickerTarget::Ui => &fonts.ui_font,
        FontPickerTarget::Card => &fonts.card_font,
        FontPickerTarget::Preview => &fonts.preview_font,
    }
}

fn set_saved_family(fonts: &mut FontPreference, target: FontPickerTarget, family: &str) {
    match target {
        FontPickerTarget::Ui => fonts.ui_font = family.to_string(),
        FontPickerTarget::Card => fonts.card_font = family.to_string(),
        FontPickerTarget::Preview => fonts.preview_font = family.to_string(),
    }
}

impl SettingsWindowView {
    /// 建一个字体选择器并订阅确认事件；返回实体与必须存活的订阅。
    /// `language` 只需首屏值，后续语言切换由 `sync_font_selects` 重建列表。
    pub(super) fn bind_font_select(
        owner: &WeakEntity<ClipboardView>,
        fonts: &FontPreference,
        language: LanguagePreference,
        target: FontPickerTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<FontSelectState>, Subscription) {
        let saved = saved_family(fonts, target);
        let items = font_items(target.default_label(language), saved);
        let selected = items
            .iter()
            .position(|item| item.family.as_str() == saved)
            .unwrap_or(0);
        let select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(items),
                Some(gpui_kit::component::IndexPath::new(selected)),
                window,
                cx,
            )
            .searchable(true)
        });
        let owner = owner.clone();
        let subscription = cx.subscribe_in(
            &select,
            window,
            move |_this, _select, event: &SelectEvent<SearchableVec<FontItem>>, _window, cx| {
                let SelectEvent::Confirm(family) = event;
                let _ = owner.update(cx, |owner, cx| {
                    let mut next = owner.fonts.value.clone();
                    set_saved_family(&mut next, target, family.as_deref().unwrap_or(""));
                    owner.save_fonts(next, cx);
                });
            },
        );
        (select, subscription)
    }

    /// 偏好保存（含恢复默认）或语言变化后把三个选择器对齐到已存值。
    /// `language_changed` 时重建条目列表，让首项占位标签跟随新语言。
    pub(super) fn sync_font_selects(
        &mut self,
        fonts: &FontPreference,
        language: LanguagePreference,
        language_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pairs = [
            (FontPickerTarget::Ui, &self.ui_font_select),
            (FontPickerTarget::Card, &self.card_font_select),
            (FontPickerTarget::Preview, &self.preview_font_select),
        ];
        for (target, select) in pairs {
            let saved = saved_family(fonts, target);
            select.update(cx, |select, cx| {
                if language_changed {
                    select.set_items(
                        SearchableVec::new(font_items(target.default_label(language), saved)),
                        window,
                        cx,
                    );
                }
                if select.selected_value().map(|v| v.as_str()) != Some(saved) {
                    select.set_selected_value(&saved.into(), window, cx);
                }
            });
        }
    }

    pub(super) fn settings_fonts_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let pending = owner_state.fonts.pending;
        settings_stack()
            .child(
                settings_field(tr(language, "字体", "Fonts")).child(
                    settings_inline()
                        .flex_nowrap()
                        .justify_between()
                        .child(
                            div().flex_1().min_w_0().child(settings_description(
                                tr(
                                    language,
                                    "卡片可跟随界面字体，悬停预览可跟随卡片字体。三处字号独立设置。",
                                    "Cards can follow the interface font, and hover previews can follow the card font. Font sizes are set separately.",
                                ),
                                cx,
                            )),
                        )
                        .child(
                            Button::new("reset-fonts")
                                .flex_shrink_0()
                                .label(tr(language, "恢复默认", "Reset to defaults"))
                                .h(px(visual::CONTROL_HEIGHT))
                                .ghost()
                                .disabled(pending)
                                .on_click({
                                    let owner = self.owner.clone();
                                    move |_, _, cx| {
                                        let _ = owner.update(cx, |owner, cx| {
                                            owner.save_fonts(FontPreference::default(), cx);
                                        });
                                    }
                                }),
                        ),
                ),
            )
            .child(self.font_group(
                tr(language, "界面字体", "Interface font"),
                &self.ui_font_select,
                &self.ui_font_size,
                pending,
                cx,
            ))
            .child(self.font_group(
                tr(language, "卡片字体", "Card font"),
                &self.card_font_select,
                &self.card_font_size,
                pending,
                cx,
            ))
            .child(self.font_group(
                tr(language, "预览字体", "Preview font"),
                &self.preview_font_select,
                &self.preview_font_size,
                pending,
                cx,
            ))
            .into_any_element()
    }

    /// 字体下拉与字号同一行：下拉占满剩余宽度，步进器靠右固定宽（三位数 + `px`）。
    fn font_group(
        &self,
        label: &'static str,
        select: &Entity<FontSelectState>,
        size_input: &settings_window::BoundedNumberInput,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language;
        settings_field(label).child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(visual::SPACE_2))
                .child(
                    Select::new(select)
                        .w_full()
                        .h(px(visual::CONTROL_HEIGHT_LARGE))
                        .disabled(disabled)
                        .accessibility_label(label)
                        .search_placeholder(tr(language, "搜索字体", "Search fonts")),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(visual::FONT_SIZE_STEPPER_WIDTH))
                        .child(settings_number_field(
                            NumberInput::new(size_input.input())
                                .accessibility_label(label)
                                .suffix(
                                    div()
                                        .text_size(px(visual::TEXT_SMALL))
                                        .text_color(cx.theme().muted_foreground)
                                        .child("px"),
                                ),
                            disabled,
                        )),
                ),
        )
    }
}
