use super::settings_common::owner_language;
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
            "保存文本、图片和文件等复制记录，随时搜索与预览。",
            "Keep copied text, images, files, and more, ready to search and preview.",
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
        let Some(language) = owner_language(&self.owner, cx) else {
            return div().into_any_element();
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "适用于 Windows、Linux 和 macOS 的剪贴板管理器，帮助你轻松找回和复用复制过的内容。",
                        "A clipboard manager for Windows, Linux and macOS that helps you find and reuse what you have copied.",
                    )),
            )
            .children(FEATURE_GROUPS.iter().map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
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
                                div()
                                    .min_w_0()
                                    .text_sm()
                                    .child(tr(language, chinese, english)),
                            )
                    }))
            }))
            .into_any_element()
    }
}
