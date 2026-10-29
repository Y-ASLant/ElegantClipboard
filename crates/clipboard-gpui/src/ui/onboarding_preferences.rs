use super::*;
use gpui_kit::component::switch::Switch;

impl ClipboardView {
    pub(super) fn onboarding_preferences(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let pending = self.onboarding_saving();
        let owner = cx.weak_entity();
        lessons::lesson_card(cx)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(14.)).font_medium().child(tr(
                        language,
                        "外观",
                        "Appearance",
                    )))
                    .child(settings_common::choice_tabs(
                        "onboarding-theme",
                        &owner,
                        [
                            ThemePreference::System,
                            ThemePreference::Light,
                            ThemePreference::Dark,
                        ],
                        [
                            tr(language, "跟随系统", "System"),
                            tr(language, "浅色", "Light"),
                            tr(language, "深色", "Dark"),
                        ],
                        self.theme,
                        pending,
                        |owner| owner.theme,
                        |owner, theme, cx| {
                            if owner.send(Command::SetTheme(theme), cx) {
                                owner.theme_pending = true;
                                cx.notify();
                            }
                        },
                    )),
            )
            .child(
                div()
                    .py_3()
                    .border_t_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_size(px(14.)).font_medium().child(tr(
                                language,
                                "登录后自动启动",
                                "Launch at login",
                            )))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(relative(1.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(
                                        language,
                                        "后台待命，用快捷键即可唤出。",
                                        "Stay ready in the tray for your shortcut.",
                                    )),
                            ),
                    )
                    .child(
                        Switch::new("onboarding-autostart")
                            .accessibility_label(tr(language, "登录后自动启动", "Launch at login"))
                            .checked(self.autostart.value)
                            .disabled(pending)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                if !this.onboarding_saving()
                                    && this.send(Command::SetAutostart(*checked), cx)
                                {
                                    this.autostart.pending = true;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_size(px(14.)).font_medium().child(tr(
                                language,
                                "唤出后直接搜索",
                                "Focus search on open",
                            )))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(relative(1.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(
                                        language,
                                        "打开窗口后，直接输入关键词。",
                                        "Start typing as soon as history opens.",
                                    )),
                            ),
                    )
                    .child(
                        Switch::new("onboarding-search-focus")
                            .accessibility_label(tr(
                                language,
                                "唤出后直接搜索",
                                "Focus search on open",
                            ))
                            .checked(self.search_auto_focus.value)
                            .disabled(pending)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                if !this.onboarding_saving()
                                    && this.send(Command::SetSearchAutoFocus(*checked), cx)
                                {
                                    this.search_auto_focus.pending = true;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_size(px(12.))
                    .line_height(relative(1.5))
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "更多选项：右键托盘图标 → 设置。",
                        "More options: tray icon → Settings.",
                    )),
            )
    }
}
