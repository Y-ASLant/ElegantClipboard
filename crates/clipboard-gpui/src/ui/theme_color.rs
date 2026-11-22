use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

const COLORS: [ThemeColor; 4] = [
    ThemeColor::VibrantOrange,
    ThemeColor::Cyan,
    ThemeColor::GoogleBlue,
    ThemeColor::Rouge,
];

fn color_name(language: LanguagePreference, color: ThemeColor) -> &'static str {
    match color {
        ThemeColor::VibrantOrange => tr(language, "赭石", "Ochre"),
        ThemeColor::Cyan => tr(language, "鸭青", "Mallard"),
        ThemeColor::GoogleBlue => tr(language, "群青", "Ultramarine"),
        ThemeColor::Rouge => tr(language, "胭脂", "Rouge"),
    }
}

impl ClipboardView {
    pub(super) fn theme_color_tabs(
        &self,
        id: &'static str,
        owner: &WeakEntity<Self>,
        disabled: bool,
        cx: &App,
    ) -> TabBar {
        let language = self.language;
        settings_common::choice_tabs(
            id,
            owner,
            COLORS,
            COLORS.map(|color| color_name(language, color)),
            self.theme_color,
            disabled || self.theme_color_pending,
            |owner| owner.theme_color,
            Self::save_theme_color,
            cx,
        )
    }

    pub(super) fn onboarding_theme_color(
        &self,
        available: Pixels,
        window: &Window,
        pending: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let language = self.language;
        let owner = cx.weak_entity();
        // A large Kit segmented rail adds 8px of insets, 2px gaps, and
        // 16px on both sides of each label.
        let chrome = px(8. + 2. * (COLORS.len() - 1) as f32 + 32. * COLORS.len() as f32);
        let required = COLORS.iter().fold(chrome, |width, &color| {
            width
                + visual::toolbar_text_width(
                    color_name(language, color).into(),
                    px(visual::TEXT_BODY),
                    window,
                    cx,
                )
        });
        if required <= available {
            return self
                .theme_color_tabs("onboarding-theme-color", &owner, pending, cx)
                .into_any_element();
        }

        let selected = self.theme_color;
        let name = color_name(language, selected);
        let label = tr(language, "主题色", "Theme color");
        Button::new("onboarding-theme-color-menu")
            .outline()
            .w_full()
            .h(px(visual::CONTROL_HEIGHT_LARGE))
            .disabled(pending)
            .dropdown_caret(true)
            .accessibility_label(format!("{label}: {name}"))
            .child(div().flex_1().min_w_0().truncate().child(name))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.min_w(available).max_w(available);
                for color in COLORS {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(color_name(language, color))
                            .checked(color == selected)
                            .on_click(move |_, _, cx| {
                                let _ =
                                    owner.update(cx, |view, cx| view.save_theme_color(color, cx));
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn save_theme_color(&mut self, color: ThemeColor, cx: &mut Context<Self>) {
        if self.theme_color != color
            && !self.theme_color_pending
            && self.send(Command::SetThemeColor(color), cx)
        {
            self.theme_color_pending = true;
            cx.notify();
        }
    }

    pub(super) fn apply_theme_color_saved(
        &mut self,
        result: Result<ThemeColor, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.theme_color_pending = false;
        match result {
            Ok(color) => {
                self.close_hover_preview("theme_color_changed", cx);
                self.theme_color = color;
                crate::design_system::set_theme_color(color, cx);
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "主题色保存失败",
                        "Failed to save theme color"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }
}
