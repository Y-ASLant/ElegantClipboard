use super::settings_common::{owner_language, owner_state, settings_stack};
use super::update_events::UpdateCheck;
use super::*;
use gpui_kit::component::alert::Alert;

impl SettingsWindowView {
    /// 「关于」页：居中图标、名称、版本与一句话介绍，下面列出项目链接。
    pub(super) fn settings_about_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(language) = owner_language(&self.owner, cx) else {
            return div().into_any_element();
        };

        let intro = settings_stack()
            .items_center()
            .gap_3()
            .py_4()
            .child(
                div()
                    .size(px(64.))
                    .rounded_md()
                    .overflow_hidden()
                    .child(img("app-icon.png").size_full().object_fit(ObjectFit::Contain)),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_semibold()
                            .child("ElegantClipboard"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "跨平台剪贴板管理器，随时找回并复用复制的内容",
                        "A cross-platform clipboard manager for finding and reusing what you have copied.",
                    )),
            );

        // 「图标 + muted 标题 | 右侧控件」的行骨架，链接行和版本行共用。
        let row = |icon: IconName, caption: SharedString, right: AnyElement| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .py_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(
                            Icon::new(icon)
                                .small()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        ),
                )
                .child(right)
                .into_any_element()
        };

        let link_row = |id: &'static str,
                        caption: SharedString,
                        label: SharedString,
                        icon: IconName,
                        link: ProjectLink| {
            row(
                icon,
                caption,
                Button::new(id)
                    .h(px(visual::CONTROL_HEIGHT_COMPACT))
                    .flex_shrink_0()
                    .ghost()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.link_error = links::open(link).err().map(|error| error.to_string());
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };

        let update_row = {
            let state = owner_state(&self.owner, cx).map(|state| &state.update_check);
            let status = match state {
                Some(UpdateCheck::Checking) => Some(tr(language, "检查中…", "Checking…")),
                Some(UpdateCheck::Downloading) => Some(tr(language, "下载中…", "Downloading…")),
                Some(UpdateCheck::UpToDate) => Some(tr(language, "已是最新", "Up to date")),
                _ => None,
            };
            let right = match (status, state) {
                (Some(text), _) => div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text)
                    .into_any_element(),
                (None, Some(UpdateCheck::Available(release))) => {
                    let label: SharedString = if language == LanguagePreference::English {
                        format!("Download v{}", release.version).into()
                    } else {
                        format!("下载 v{}", release.version).into()
                    };
                    Button::new("about-update-download")
                        .h(px(visual::CONTROL_HEIGHT_COMPACT))
                        .flex_shrink_0()
                        .ghost()
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let _ = this.owner.update(cx, |owner, cx| {
                                owner.start_update_download(cx);
                            });
                        }))
                        .into_any_element()
                }
                (None, Some(UpdateCheck::Ready(_))) => Button::new("about-update-install")
                    .h(px(visual::CONTROL_HEIGHT_COMPACT))
                    .flex_shrink_0()
                    .primary()
                    .label(tr(language, "立即安装", "Install now"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let _ = this.owner.update(cx, |owner, cx| {
                            owner.start_update_install(cx);
                        });
                    }))
                    .into_any_element(),
                _ => Button::new("about-update-check")
                    .h(px(visual::CONTROL_HEIGHT_COMPACT))
                    .flex_shrink_0()
                    .ghost()
                    .label(tr(language, "检查更新", "Check for updates"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        #[cfg(windows)]
                        let _ = this.owner.update(cx, |owner, cx| {
                            owner.start_update_check(cx);
                        });
                        #[cfg(not(windows))]
                        {
                            this.link_error = links::open(ProjectLink::Releases)
                                .err()
                                .map(|error| error.to_string());
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
            };
            row(
                IconName::RefreshCw,
                tr(language, "版本", "Version").into(),
                right,
            )
        };

        let links = settings_stack().gap_0().children([
            link_row(
                "about-author",
                tr(language, "作者", "Author").into(),
                "ASLant".into(),
                IconName::User,
                ProjectLink::Author,
            ),
            link_row(
                "about-repository",
                "GitHub".into(),
                "ElegantClipboard".into(),
                IconName::Github,
                ProjectLink::Repository,
            ),
            link_row(
                "about-issues",
                tr(language, "反馈", "Feedback").into(),
                tr(language, "提交问题", "Submit issue").into(),
                IconName::TriangleAlert,
                ProjectLink::Issues,
            ),
            update_row,
        ]);

        settings_stack()
            .child(intro)
            .child(links)
            .when_some(self.link_error.clone(), |panel, error| {
                panel.child(Alert::error("about-link-error", error).small())
            })
            .into_any_element()
    }
}
