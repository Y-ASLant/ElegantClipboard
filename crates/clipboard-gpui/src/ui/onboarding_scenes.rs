use super::*;

impl Preview {
    pub(super) fn capture(&self, phase: Option<f32>) -> Div {
        let progress = phase.map_or(1., |phase| {
            ease_out_quint()(((phase - 0.13) / 0.25).clamp(0., 1.))
        });
        let fade = phase.map_or(1., |phase| ((1. - phase) / 0.12).clamp(0., 1.));
        let y = if self.compact { 45. } else { 67. };
        div()
            .size_full()
            .relative()
            .child(
                self.surface()
                    .left(px(14.))
                    .top(px(y))
                    .w(px(104.))
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Icon::new(IconName::File).small().text_color(self.blue))
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_medium()
                            .child(self.text("一段灵感", "A good idea")),
                    )
                    .child(div().h(px(3.)).rounded_full().w_full().bg(self.border))
                    .child(div().h(px(3.)).rounded_full().w(px(48.)).bg(self.border))
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.))
                            .text_color(self.blue)
                            .child(self.text("复制", "Copy")),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(126.))
                    .top(px(y + 34. - 4. * progress))
                    .opacity(progress * fade)
                    .child(
                        Icon::new(IconName::ArrowRight)
                            .small()
                            .text_color(self.blue),
                    ),
            )
            .child(
                self.surface()
                    .left(px(150.))
                    .right(px(14.))
                    .top(px(if self.compact { 18. } else { 30. }))
                    .child(self.heading(self.text("剪贴板历史", "Clipboard history")))
                    .child(
                        div()
                            .p_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .relative()
                                    .top(px(12. * (1. - progress)))
                                    .opacity(0.15 + 0.85 * progress * fade)
                                    .child(self.history_card(
                                        self.text("一段灵感", "A good idea"),
                                        self.text("刚刚 · 文本", "Just now · Text"),
                                        IconName::Copy,
                                        true,
                                    )),
                            )
                            .child(self.history_card(
                                self.text("设计参考.png", "Reference.png"),
                                self.text("图片", "Image"),
                                IconName::File,
                                false,
                            ))
                            .when(!self.compact, |cards| {
                                cards.child(self.history_card(
                                    self.text("项目文件", "Project files"),
                                    self.text("文件路径", "File paths"),
                                    IconName::Folder,
                                    false,
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(20.))
                    .top(px(y - 26.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .opacity(progress * fade)
                    .text_size(px(10.))
                    .text_color(self.green)
                    .child(Icon::new(IconName::Check).xsmall())
                    .child(self.text("已记入历史", "Saved locally")),
            )
    }

    pub(super) fn search(&self, phase: Option<f32>) -> Div {
        let query = self.text("会议", "meeting");
        let input = phase.map_or(1., |phase| (phase / 0.3).clamp(0., 1.));
        let typed: String = query
            .chars()
            .take((input * query.chars().count() as f32).ceil() as usize)
            .collect();
        let hit = phase.map_or(1., |phase| {
            ease_out_quint()(((phase - 0.25) / 0.2).clamp(0., 1.))
        });
        let caret_visible = phase.is_none_or(|phase| phase % 0.18 < 0.09);
        self.surface()
            .left(px(22.))
            .right(px(22.))
            .top(px(if self.compact { 15. } else { 30. }))
            .child(self.heading(self.text("搜索你的历史", "Search your history")))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(self.border.opacity(0.7))
                    .child(Icon::new(IconName::Search).small().text_color(self.blue))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .child(if typed.is_empty() {
                                self.text("输入关键词…", "Type to search…").to_owned()
                            } else {
                                typed
                            }),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(13.))
                            .bg(self.blue)
                            .opacity(if caret_visible { 1. } else { 0. }),
                    ),
            )
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .relative()
                            .top(px(7. * (1. - hit)))
                            .child(self.history_card(
                                self.text("明天下午的会议纪要", "Tomorrow's meeting notes"),
                                self.text("找到匹配内容", "A match from your history"),
                                IconName::Copy,
                                hit > 0.3,
                            )),
                    )
                    .when(!self.compact, |cards| {
                        cards.child(div().opacity(1. - 0.7 * hit).child(self.history_card(
                            self.text("一个有趣的想法", "Something worth keeping"),
                            self.text("文本", "Text"),
                            IconName::Copy,
                            false,
                        )))
                    }),
            )
    }

    pub(super) fn favorites(&self, phase: Option<f32>) -> Div {
        let saved = phase.map_or(1., |phase| {
            ease_out_quint()(((phase - 0.18) / 0.25).clamp(0., 1.))
        });
        let height = if self.compact { 126. } else { 160. };
        self.surface()
            .left(px(24.))
            .right(px(24.))
            .top(px(if self.compact { 14. } else { 28. }))
            .h(px(height))
            .child(self.heading(self.text("常用内容", "Your everyday favorites")))
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .right(px(10.))
                    .top(px(39. + 49. * (1. - saved)))
                    .child(self.history_card(
                        self.text("常用的收件信息", "Your delivery details"),
                        self.text("随时可用", "Ready when you need it"),
                        IconName::Copy,
                        true,
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .right(px(10.))
                    .top(px(39. + 49. * saved))
                    .opacity(0.8)
                    .child(self.history_card(
                        self.text("临时记下的内容", "A passing thought"),
                        self.text("文本", "Text"),
                        IconName::Copy,
                        false,
                    )),
            )
            .child(
                div()
                    .absolute()
                    .right(px(21.))
                    .top(px(51. + 49. * (1. - saved)))
                    .text_color(self.amber)
                    .opacity(saved)
                    .child(Icon::new(IconName::Star).small()),
            )
    }

    pub(super) fn paste(&self, phase: Option<f32>) -> Div {
        let delivered = phase.map_or(1., |phase| {
            ease_out_quint()(((phase - 0.35) / 0.23).clamp(0., 1.))
        });
        let sentence = self.text("准备好了，继续。", "Ready to go.");
        let typed: String = sentence
            .chars()
            .take((delivered * sentence.chars().count() as f32) as usize)
            .collect();
        let header = if let Some(kbd) = shortcut_kbd(&self.shortcut) {
            div().child(kbd)
        } else {
            div().text_size(px(11.)).child(self.shortcut.clone())
        };
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .top(px(13.))
                    .left(px(20.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(header)
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(self.muted_foreground)
                            .child(self.text("唤出历史", "Open history")),
                    ),
            )
            .child(
                self.surface()
                    .left(px(16.))
                    .top(px(if self.compact { 49. } else { 68. }))
                    .w(px(171.))
                    .child(self.heading("ElegantClipboard"))
                    .child(div().p_2().child(self.history_card(
                        sentence,
                        self.text("选中后粘贴", "Select to paste"),
                        IconName::Copy,
                        true,
                    ))),
            )
            .child(
                self.surface()
                    .left(px(200.))
                    .right(px(16.))
                    .top(px(if self.compact { 70. } else { 105. }))
                    .h(px(78.))
                    .child(self.heading(self.text("你的应用", "Your app")))
                    .child(div().p_2().text_size(px(11.)).child(typed)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(181. + delivered * 12.))
                    .top(px(if self.compact { 94. } else { 132. }))
                    .text_color(self.blue)
                    .opacity(delivered)
                    .child(Icon::new(IconName::ArrowRight).small()),
            )
    }
}
