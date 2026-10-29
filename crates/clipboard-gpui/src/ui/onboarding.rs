use super::*;
use gpui_kit::component::stepper::{Stepper, StepperItem};

#[path = "onboarding_lessons.rs"]
mod lessons;
#[path = "onboarding_preferences.rs"]
mod preferences;

impl ClipboardView {
    pub(super) fn onboarding_visible(&self) -> bool {
        self.show_onboarding && !self.onboarding_completed
    }

    fn onboarding_saving(&self) -> bool {
        self.onboarding_pending
            || self.shortcut_change_pending()
            || self.theme_pending
            || self.corner_radius.pending
            || self.language_pending
            || self.autostart.pending
            || self.search_auto_focus.pending
    }

    fn select_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        let step = step.min(3);
        if self.onboarding_visible() && !self.onboarding_saving() && self.onboarding_step != step {
            self.onboarding_step = step;
            self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
            cx.notify();
        }
    }

    pub(super) fn advance_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_saving() {
            return;
        }
        if self.onboarding_step < 3 {
            self.select_onboarding_step(self.onboarding_step + 1, cx);
        } else if self.send(Command::CompleteOnboarding, cx) {
            self.onboarding_pending = true;
            cx.notify();
        }
    }

    pub(super) fn render_onboarding(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let step = self.onboarding_step.min(3);
        let compact = window.viewport_size().height < px(640.);
        let pending = self.onboarding_saving();
        let steps = [
            ("唤出", "Open"),
            ("记录", "Copy"),
            ("粘贴", "Paste"),
            ("偏好", "Setup"),
        ];
        let (title_cn, title_en, detail_cn, detail_en) = match step {
            0 => (
                "唤出剪贴板",
                "Open clipboard history",
                "使用快捷键快速打开或隐藏。",
                "Use your shortcut to show or hide history.",
            ),
            1 => (
                "复制与查找",
                "Copy and find",
                "正常复制，自动保存到历史。",
                "Copy normally. Find it in history.",
            ),
            2 => (
                "选择与粘贴",
                "Choose and paste",
                "从目标应用唤出，选中记录即可使用。",
                "Open from your app and choose an item.",
            ),
            _ => (
                "初始偏好",
                "Your preferences",
                "自动保存，随时可在设置中调整。",
                "Saved automatically. Change anytime in Settings.",
            ),
        };
        let content = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(if compact { 16. } else { 24. }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(if compact { 20. } else { 24. }))
                            .line_height(relative(1.25))
                            .font_semibold()
                            .child(tr(language, title_cn, title_en)),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(relative(1.5))
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, detail_cn, detail_en)),
                    ),
            )
            .child(match step {
                0 => self.onboarding_shortcut_lesson(compact, cx),
                1 => self.onboarding_capture_lesson(cx),
                2 => self.onboarding_paste_lesson(cx),
                _ => self.onboarding_preferences(cx),
            });
        let content = visual::reveal(
            content,
            ("onboarding-content", self.onboarding_revision),
            cx,
        );
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .px(px(if compact { 20. } else { 32. }))
            .pt(px(if compact { 10. } else { 20. }))
            .pb(px(if compact { 16. } else { 24. }))
            .flex()
            .flex_col()
            .gap(px(if compact { 18. } else { 28. }))
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .child(
                div()
                    .w_full()
                    .max_w(px(480.))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(if compact { 14. } else { 20. }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pb_3()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .text_size(px(12.))
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "快速上手", "GETTING STARTED"))
                            .child(format!("{} / 4", step + 1)),
                    )
                    .child(
                        Stepper::new("onboarding-steps")
                            .small()
                            .text_center(true)
                            .selected_index(step)
                            .disabled(pending)
                            .items(steps.iter().enumerate().map(|(index, (cn, en))| {
                                StepperItem::new()
                                    .when(index < step, |item| {
                                        item.icon(Icon::new(IconName::Check).xsmall())
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if index == step {
                                                cx.theme().primary
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .when(index == step, |label| label.font_semibold())
                                            .child(tr(language, cn, en)),
                                    )
                            }))
                            .on_click(cx.listener(|this, index, _, cx| {
                                this.select_onboarding_step(*index, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(480.))
                    .mx_auto()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .id(("onboarding-scroll", step))
                    .child(content),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(480.))
                    .mx_auto()
                    .flex_none()
                    .pt_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("onboarding-back")
                            .ghost()
                            .h(px(40.))
                            .label(tr(language, "上一步", "Back"))
                            .icon(IconName::ArrowLeft)
                            .disabled(step == 0 || pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select_onboarding_step(
                                    this.onboarding_step.saturating_sub(1),
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("onboarding-next")
                            .primary()
                            .h(px(40.))
                            .px_5()
                            .rounded(cx.theme().radius)
                            .label(if pending {
                                tr(language, "正在保存…", "Saving…")
                            } else if step == 3 {
                                tr(language, "开始使用", "Get started")
                            } else {
                                tr(language, "下一步", "Continue")
                            })
                            .child(Icon::new(IconName::ArrowRight).small())
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, _, cx| this.advance_onboarding(cx))),
                    ),
            )
    }
}
