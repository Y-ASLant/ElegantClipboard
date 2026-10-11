use super::*;
use gpui_kit::component::alert::Alert;

impl SettingsWindowView {
    pub(super) fn settings_about_details_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(
                [
                    (
                        "about-author",
                        tr(language, "作者", "Author"),
                        "ASLant",
                        IconName::User,
                        ProjectLink::Author,
                    ),
                    (
                        "about-repository",
                        "GitHub",
                        "ElegantClipboard",
                        IconName::Github,
                        ProjectLink::Repository,
                    ),
                    (
                        "about-issues",
                        tr(language, "反馈", "Feedback"),
                        tr(language, "提交问题", "Submit issue"),
                        IconName::TriangleAlert,
                        ProjectLink::Issues,
                    ),
                ]
                .map(|(id, caption, label, icon, link)| {
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        )
                        .child(
                            Button::new(id)
                                .small()
                                .ghost()
                                .icon(icon)
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.link_error =
                                        links::open(link).err().map(|error| error.to_string());
                                    cx.notify();
                                })),
                        )
                }),
            )
            .when_some(self.link_error.clone(), |panel, error| {
                panel.child(Alert::error("about-link-error", error).small())
            })
            .into_any_element()
    }

    pub(super) fn settings_about_intro_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(format!("v{}", env!("CARGO_PKG_VERSION")))
            .child(
                div().flex().flex_col().gap_1().children(
                    [
                        (
                            "使用 Rust、GPUI 和 GPUI Kit 重构 Windows 原生界面",
                            "Rebuilt the native Windows interface with Rust, GPUI, and GPUI Kit",
                        ),
                        (
                            "采集、搜索和管理文本、图片、文件等剪贴板历史",
                            "Capture, search, and organize text, image, and file clipboard history",
                        ),
                        (
                            "全局快捷键与快速粘贴，支持 ZIP 备份与恢复",
                            "Global shortcuts and quick paste, with ZIP backup and restore",
                        ),
                    ]
                    .map(|(chinese, english)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(IconName::CircleCheck)
                                    .small()
                                    .text_color(cx.theme().primary),
                            )
                            .child(div().min_w_0().child(tr(language, chinese, english)))
                    }),
                ),
            )
            .into_any_element()
    }
}
