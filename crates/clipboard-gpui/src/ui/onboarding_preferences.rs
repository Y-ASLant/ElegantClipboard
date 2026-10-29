use super::*;
use gpui_kit::component::switch::Switch;

impl ClipboardView {
    pub(super) fn onboarding_preferences(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let pending = self.onboarding_saving();
        let owner = cx.weak_entity();
        settings_common::settings_stack()
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
                                        "登录后自动启动，随时准备记录。",
                                        "Start at login and stay ready to capture.",
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
                        "历史默认保存在本机；普通文件只记录路径，不自动复制文件本体。可在托盘暂停记录，并在设置中调整应用过滤和保留期限。",
                        "History stays on this device. Ordinary files are recorded as paths, not copied. Pause capture from the tray; adjust app filters and retention in Settings.",
                    )),
            )
            .when(!self.monitoring || self.paused, |content| {
                content.child(Alert::warning(
                    "onboarding-capture-status",
                    if !self.monitoring {
                        tr(language,
                            "监听未启用，目前只能体验演示和已有历史；演示操作不会开启真实记录。",
                            "Capture is disabled. You can use demos and existing history; the exercise does not enable capture.")
                    } else {
                        tr(language,
                            "真实记录已暂停；完成引导后可从托盘恢复。演示不会改变暂停状态。",
                            "Capture is paused. Resume it from the tray after the guide; the exercise leaves it paused.")
                    },
                ).small())
            })
    }
}
