use super::*;
use gpui_kit::component::stepper::{Stepper, StepperItem};

#[path = "onboarding_preview.rs"]
mod preview;

impl ClipboardView {
    pub(super) fn onboarding_visible(&self) -> bool {
        self.show_onboarding && !self.onboarding_completed
    }

    fn select_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        let step = step.min(3);
        if self.onboarding_visible() && !self.onboarding_pending && self.onboarding_step != step {
            self.onboarding_step = step;
            self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
            cx.notify();
        }
    }

    pub(super) fn advance_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_pending {
            return;
        }
        if self.onboarding_step < 3 {
            self.select_onboarding_step(self.onboarding_step + 1, cx);
        } else {
            self.complete_onboarding(cx);
        }
    }

    fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        if self.onboarding_visible()
            && self.onboarding_step == 3
            && !self.onboarding_pending
            && self.send(Command::CompleteOnboarding, cx)
        {
            self.onboarding_pending = true;
            cx.notify();
        }
    }

    pub(super) fn render_onboarding(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let step = self.onboarding_step.min(3);
        let compact = window.viewport_size().height < px(640.);
        let accent = cx.theme().blue;
        let steps = [
            (
                "采集",
                "Capture",
                "复制一次，随时找回",
                "Copy once. Find it later.",
                "文本、图片和文件路径，自动整理在你的剪贴板历史中。",
                "Text, images and file paths, together in your clipboard history.",
            ),
            (
                "搜索",
                "Search",
                "想找的内容，一搜就到",
                "Less searching. More finding.",
                "输入几个关键词，从历史里找回刚才那段文字。",
                "A few keywords bring the right item back into view.",
            ),
            (
                "收藏",
                "Save",
                "常用的，留在手边",
                "Keep the useful things close.",
                "收藏常用内容，置顶重要记录。下次需要，不必重新寻找。",
                "Favorite everyday snippets and pin important items for next time.",
            ),
            (
                "快捷键",
                "Paste",
                "回到原处，接着输入",
                "Back to where you were.",
                "在目标应用中用快捷键唤出历史，选择记录，粘贴回原窗口。",
                "Open history from your target app, choose an item and paste it back.",
            ),
        ];
        let (_, _, title_cn, title_en, description_cn, description_en) = steps[step];
        let description = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(if compact { 23. } else { 27. }))
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(tr(language, title_cn, title_en)),
            )
            .child(
                div()
                    .text_sm()
                    .line_height(relative(1.65))
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(language, description_cn, description_en)),
            );
        let description = if cx.reduce_motion() {
            description.into_any_element()
        } else {
            description
                .with_animation(
                    ("onboarding-copy", self.onboarding_revision),
                    Animation::new(Duration::from_millis(320)).with_easing(ease_out_quint()),
                    |element, progress| element.top(px(10. * (1. - progress))).opacity(progress),
                )
                .into_any_element()
        };
        let tip = self.onboarding_tip(cx);
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .px(px(if compact { 20. } else { 28. }))
            .pt(px(if compact { 8. } else { 16. }))
            .pb(px(if compact { 12. } else { 24. }))
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .max_w(px(440.))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(if compact { 10. } else { 16. }))
                    .child(preview::render(self, compact, cx))
                    .child(description)
                    .child(tip)
                    .child(
                        div()
                            .w_full()
                            .rounded_lg()
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.7))
                            .bg(cx.theme().muted.opacity(0.35))
                            .px_2()
                            .py(px(if compact { 8. } else { 12. }))
                            .child(
                                Stepper::new("onboarding-steps")
                                    .text_center(true)
                                    .selected_index(step)
                                    .disabled(self.onboarding_pending)
                                    .items(steps.iter().enumerate().map(|(index, (cn, en, ..))| {
                                        StepperItem::new()
                                            .when(index < step, |item| {
                                                item.icon(Icon::new(IconName::Check).xsmall())
                                            })
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py(px(2.))
                                                    .rounded_md()
                                                    .text_xs()
                                                    .text_color(if index == step {
                                                        accent
                                                    } else if index < step {
                                                        cx.theme().foreground
                                                    } else {
                                                        cx.theme().muted_foreground
                                                    })
                                                    .when(index == step, |label| {
                                                        label
                                                            .bg(accent.opacity(0.1))
                                                            .font_semibold()
                                                    })
                                                    .child(tr(language, cn, en)),
                                            )
                                    }))
                                    .on_click(cx.listener(|this, index, _, cx| {
                                        this.select_onboarding_step(*index, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                Button::new("onboarding-back")
                                    .ghost()
                                    .icon(IconName::ArrowLeft)
                                    .label(tr(language, "上一步", "Back"))
                                    .disabled(step == 0 || self.onboarding_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.select_onboarding_step(
                                            this.onboarding_step.saturating_sub(1),
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("onboarding-next")
                                    .primary()
                                    .h(px(40.))
                                    .px_5()
                                    .rounded_lg()
                                    .label(if self.onboarding_pending {
                                        tr(language, "正在准备…", "Preparing…")
                                    } else if step == 3 {
                                        tr(language, "开始使用", "Get started")
                                    } else {
                                        tr(language, "下一步", "Continue")
                                    })
                                    .child(Icon::new(IconName::ArrowRight).small())
                                    .disabled(self.onboarding_pending)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.advance_onboarding(cx)),
                                    ),
                            ),
                    ),
            )
    }

    fn onboarding_tip(&self, cx: &App) -> Div {
        let language = self.language;
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground);
        let (row, text) = match self.onboarding_step {
            1 => (
                row.child(
                    shortcut_kbd(if cfg!(target_os = "macos") {
                        "Cmd+F"
                    } else {
                        "Ctrl+F"
                    })
                    .expect("onboarding search key"),
                ),
                tr(language, "快速聚焦搜索", "Jump straight to search"),
            ),
            2 => (
                row.child(Icon::new(IconName::Star).small()),
                tr(
                    language,
                    "在记录上选择「收藏」即可保留常用内容",
                    "Favorite a history item to keep it handy",
                ),
            ),
            3 => (
                row.child(shortcut_kbd("Shift+Enter").expect("onboarding plain text key")),
                tr(
                    language,
                    "需要去掉格式时，使用纯文本粘贴",
                    "Paste as plain text when formatting gets in the way",
                ),
            ),
            _ => (
                row.child(Icon::new(IconName::Check).small()),
                tr(
                    language,
                    "历史保存在本机，普通文件仅记录路径",
                    "History stays local; files are stored as paths",
                ),
            ),
        };
        row.child(div().flex_1().min_w_0().child(text))
    }
}
