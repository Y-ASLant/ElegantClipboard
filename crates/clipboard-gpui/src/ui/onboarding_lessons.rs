use super::*;

pub(super) fn lesson_card(cx: &App) -> Div {
    div()
        .w_full()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().group_box)
        .p(px(visual::SPACE_4))
        .flex()
        .flex_col()
        .gap(px(visual::SPACE_4))
}

fn key_instruction(shortcut: &str, title: &'static str, detail: &'static str, cx: &App) -> Div {
    div()
        .w_full()
        .flex()
        .items_start()
        .gap_3()
        .child(
            div().w(px(104.)).flex_none().child(
                shortcut_kbd(shortcut)
                    .expect("onboarding instruction key")
                    .outline()
                    .px_2()
                    .py_1(),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(14.)).font_medium().child(title))
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(relative(1.5))
                        .text_color(cx.theme().muted_foreground)
                        .child(detail),
                ),
        )
}

impl ClipboardView {
    pub(super) fn onboarding_shortcut_lesson(&self, compact: bool, cx: &App) -> Div {
        let language = self.language;
        let registered = self.hotkey.is_some();
        let shortcut = match &self.hotkey_choice {
            HotkeyPreference::Shortcut(shortcut) => Some(shortcut.as_str()),
            HotkeyPreference::Disabled => None,
        };
        let key = shortcut.map(|shortcut| {
            shortcut_kbd(shortcut)
                .expect("onboarding configured shortcut")
                .outline()
                .text_size(px(if compact { 28. } else { 36. }))
                .text_color(cx.theme().foreground)
                .px_5()
                .py_3()
                .rounded(cx.theme().radius)
        });
        let key = div()
            .relative()
            .children(key)
            .when(shortcut.is_none(), |key| {
                key.child(div().text_xl().font_semibold().child(tr(
                    language,
                    "快捷键已关闭",
                    "Shortcut is disabled",
                )))
            });
        let description = if registered {
            tr(
                language,
                "按一次唤出 · 再按一次隐藏",
                "Press to open · press again to hide",
            )
        } else {
            tr(
                language,
                "快捷键不可用，请从托盘打开",
                "Global shortcut is unavailable. Open from the tray.",
            )
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                lesson_card(cx)
                    .py(px(if compact { 18. } else { 28. }))
                    .items_center()
                    .gap_3()
                    .child(key)
                    .child(div().text_size(px(14.)).text_center().child(description))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "可在设置中修改", "Change it in Settings")),
                    ),
            )
            .when(!registered, |body| {
                body.child(
                    Alert::warning(
                        "onboarding-shortcut-unavailable",
                        crate::capabilities::global_shortcut_unavailable_reason(language)
                            .unwrap_or(tr(
                                language,
                                "请在设置中启用或修改快捷键。",
                                "Enable or change the shortcut in Settings.",
                            )),
                    )
                    .small(),
                )
            })
    }

    pub(super) fn onboarding_capture_lesson(&self, cx: &App) -> Div {
        let language = self.language;
        let copy_key = if cfg!(target_os = "macos") {
            "Cmd+C"
        } else {
            "Ctrl+C"
        };
        lesson_card(cx)
            .child(key_instruction(
                copy_key,
                tr(language, "复制一段文字、图片或文件", "Copy text, an image or a file"),
                tr(language, "文本、图片和文件均可记录。", "Save text, images and file paths."),
                cx,
            ))
            .child(key_instruction(
                if cfg!(target_os = "macos") { "Cmd+F" } else { "Ctrl+F" },
                tr(language, "在历史中聚焦搜索", "Focus search in history"),
                tr(language, "输入关键词查找记录。", "Type keywords to find an item."),
                cx,
            ))
            .child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_size(px(12.))
                    .line_height(relative(1.6))
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, "历史保存在本机；普通文件仅记录路径。常用记录可收藏或置顶。", "History stays on this device; files are stored as paths. Favorite or pin items you use often.")),
            )
            .when(!self.monitoring || self.paused, |body| {
                body.child(
                    Alert::warning(
                        "onboarding-capture-status",
                        if !self.monitoring {
                            tr(language, "监听未启用，不会自动记录新复制内容；已有历史仍可使用。", "Capture is not enabled. New copies are not saved; existing history is still available.")
                        } else {
                            tr(language, "记录已暂停，可从托盘菜单恢复。", "Capture is paused. Resume it from the tray menu.")
                        },
                    )
                    .small(),
                )
            })
    }

    pub(super) fn onboarding_paste_lesson(&self, cx: &App) -> Div {
        let language = self.language;
        lesson_card(cx)
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(relative(1.5))
                    .child(tr(language, "先在目标应用定位光标，再用快捷键唤出。", "Place your cursor, then open history with your shortcut.")),
            )
            .child(key_instruction(
                "Up",
                tr(language, "方向键选择，也可以点击记录", "Use arrow keys or click an item"),
                tr(language, "选中想要使用的记录。", "Choose the item you want to use."),
                cx,
            ))
            .child(key_instruction(
                "Enter",
                tr(language, "使用选中的记录", "Use the selected item"),
                tr(language, "从目标应用唤出时自动回贴；从托盘打开时只复制。", "From a target app: paste back. From the tray: copy only."),
                cx,
            ))
            .child(key_instruction(
                "Shift+Enter",
                tr(language, "去掉格式，使用纯文本", "Use plain text"),
                tr(language, "适用于文字、网页和富文本记录。", "For text, links and rich text items."),
                cx,
            ))
            .child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_size(px(12.))
                    .line_height(relative(1.5))
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, "提示：从托盘打开后，复制记录，再回到目标应用手动粘贴。", "Tip: when opening from the tray, copy an item, return to your app and paste.")),
            )
            .when_some(crate::capabilities::paste_unavailable_reason(language), |body, reason| {
                body.child(Alert::warning("onboarding-paste-capability", reason).small())
            })
    }
}
