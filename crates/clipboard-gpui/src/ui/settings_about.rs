use super::settings_common::{owner_language, settings_description, settings_stack};
use super::*;
use gpui_kit::component::alert::Alert;

/// 「关于」页的简要功能介绍。
struct FeatureGroup {
    chinese: &'static str,
    english: &'static str,
    items: &'static [(&'static str, &'static str)],
}

const FEATURE_GROUPS: &[FeatureGroup] = &[
    FeatureGroup {
        chinese: "记录与查找",
        english: "Capture and find",
        items: &[(
            "保存文本、图片和文件路径等复制记录，随时搜索与预览。",
            "Keep copied text, images, file paths, and more, ready to search and preview.",
        )],
    },
    FeatureGroup {
        chinese: "整理与复用",
        english: "Organize and reuse",
        items: &[(
            "通过收藏、置顶和分组整理常用内容，使用快捷键快速粘贴。",
            "Organize frequently used content with favorites, pins, and groups, and paste quickly with shortcuts.",
        )],
    },
    FeatureGroup {
        chinese: "按需设置",
        english: "Make it yours",
        items: &[(
            "自定义外观与采集规则，管理历史保留时间并备份数据。",
            "Customize appearance and capture rules, manage how long history is kept, and back up your data.",
        )],
    },
];

impl SettingsWindowView {
    pub(super) fn settings_about_details_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(language) = owner_language(&self.owner, cx) else {
            return div().into_any_element();
        };

        settings_stack()
            .gap_1()
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
                        .py_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        )
                        .child(
                            Button::new(id)
                                .h(px(32.))
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
        let Some(language) = owner_language(&self.owner, cx) else {
            return div().into_any_element();
        };

        settings_stack()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                settings_description(tr(
                    language,
                    "适用于 Windows、Linux 和 macOS 的剪贴板管理器，帮助你轻松找回和复用复制过的内容。",
                    "A clipboard manager for Windows, Linux and macOS that helps you find and reuse what you have copied.",
                ), cx),
            )
            .children(FEATURE_GROUPS.iter().map(|group| {
                settings_stack()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_semibold()
                            .child(tr(language, group.chinese, group.english)),
                    )
                    .children(group.items.iter().map(|(chinese, english)| {
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                Icon::new(IconName::CircleCheck)
                                    .small()
                                    .text_color(cx.theme().primary)
                                    .mt(px(3.)),
                            )
                            .child(
                                settings_description(tr(language, chinese, english), cx)
                                    .flex_1(),
                            )
                    }))
            }))
            .child(
                Button::new("about-review-onboarding")
                    .outline()
                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                    .w(px(240.))
                    .label(tr(language, "重新查看快速上手", "Review quick start"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let Some(owner) = this.owner.upgrade() else { return };
                        let main_window = owner.read(cx).main_window;
                        if let Err(error) = main_window.update(cx, |_, window, cx| {
                            owner.update(cx, |owner, cx| owner.restart_onboarding(window, cx));
                        }) {
                            this.link_error = Some(format!(
                                "{}: {error}",
                                tr(this.language, "无法打开引导", "Could not open the guide")
                            ));
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }
}
