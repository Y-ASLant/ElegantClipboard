use super::*;
use gpui_kit::component::stepper::{Stepper, StepperItem};

#[path = "onboarding_practice.rs"]
pub(super) mod practice;
#[path = "onboarding_preferences.rs"]
mod preferences;
#[path = "onboarding_shortcut.rs"]
pub(super) mod shortcut;

impl ClipboardView {
    pub(super) fn onboarding_visible(&self) -> bool {
        self.show_onboarding
    }

    pub(super) fn onboarding_saving(&self) -> bool {
        self.onboarding_pending
            || self.shortcut_change_pending()
            || self.theme_pending
            || self.language_pending
            || self.autostart.pending
            || self.search_auto_focus.pending
    }

    pub(super) fn suspend_onboarding(&mut self, cx: &mut App) {
        if let Some(practice) = &self.onboarding_practice {
            practice.update(cx, |practice, cx| practice.suspend(cx));
        }
        if let Some(shortcut) = &self.onboarding_shortcut {
            shortcut.update(cx, |shortcut, cx| shortcut.suspend(cx));
        }
    }

    pub(super) fn dismiss_onboarding_transients(&mut self, cx: &mut App) {
        if let Some(practice) = &self.onboarding_practice {
            practice.update(cx, |practice, cx| practice.close_preview(cx));
        }
        if let Some(shortcut) = &self.onboarding_shortcut {
            shortcut.update(cx, |shortcut, cx| shortcut.suspend(cx));
        }
    }

    fn select_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        let step = step.min(2);
        if self.onboarding_visible() && !self.onboarding_saving() && self.onboarding_step != step {
            self.suspend_onboarding(cx);
            self.onboarding_step = step;
            self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
            cx.notify();
        }
    }

    pub(super) fn restart_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding_saving() {
            return;
        }
        if self.active_overlay().is_some()
            || self.card_drag_active(cx)
            || self.sequence_queue.is_some()
        {
            self.set_feedback_message(
                tr(
                    self.language,
                    "请先结束主窗口中的编辑、拖动或粘贴队列，再查看引导。",
                    "Finish editing, dragging or the paste queue before reopening the guide.",
                )
                .into(),
            );
            self.is_error = true;
            cx.notify();
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview("onboarding_reopened", cx);
        self.suspend_onboarding(cx);
        self.onboarding_shortcut = None;
        self.onboarding_practice = None;
        self.onboarding_step = 0;
        self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
        self.onboarding_hotkey_used = false;
        self.onboarding_error = None;
        self.show_onboarding = true;
        self.paste_target = None;
        self.show_window(window, cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    pub(super) fn leave_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.suspend_onboarding(cx);
        self.onboarding_shortcut = None;
        self.onboarding_practice = None;
        self.show_onboarding = false;
        self.onboarding_error = None;
        self.paste_target = None;
        if self.search_auto_focus.value {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.list_focus, cx);
        }
        cx.notify();
    }

    fn finish_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_saving() || self.onboarding_step != 2 {
            return;
        }
        if self.onboarding_completed {
            self.leave_onboarding(window, cx);
        } else if self.send(Command::CompleteOnboarding, cx) {
            self.onboarding_pending = true;
            self.onboarding_error = None;
            self.suspend_onboarding(cx);
            cx.notify();
        } else {
            self.onboarding_error = Some(self.message.clone());
            cx.notify();
        }
    }

    fn advance_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_saving() {
            return;
        }
        if self.onboarding_step < 2 {
            self.select_onboarding_step(self.onboarding_step + 1, cx);
        } else {
            self.finish_onboarding(window, cx);
        }
    }

    fn onboarding_language_control(&self, cx: &mut Context<Self>) -> TabBar {
        let pending = self.onboarding_saving();
        TabBar::new("onboarding-language")
            .segmented()
            .h(px(visual::CONTROL_HEIGHT))
            .w(px(176.))
            .selected_index(usize::from(self.language == LanguagePreference::English))
            .children(["简体中文", "English"].map(|label| {
                Tab::new()
                    .label(label)
                    .text_size(px(visual::TEXT_SMALL))
                    .disabled(pending)
                    .flex_1()
            }))
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                let language = if *index == 0 {
                    LanguagePreference::Chinese
                } else {
                    LanguagePreference::English
                };
                if !this.onboarding_saving()
                    && this.language != language
                    && this.send(Command::SetLanguage(language), cx)
                {
                    this.language_pending = true;
                    cx.notify();
                }
            }))
    }

    pub(super) fn render_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let step = self.onboarding_step.min(2);
        let pending = self.onboarding_saving();
        let compact = window.viewport_size().height < px(640.);
        let body = match step {
            0 => {
                let owner = cx.weak_entity();
                let initial = self.hotkey_choice.clone();
                self.onboarding_shortcut
                    .get_or_insert_with(|| {
                        cx.new(|cx| shortcut::OnboardingShortcut::new(owner, initial, window, cx))
                    })
                    .clone()
                    .into_any_element()
            }
            1 => {
                let practice = self
                    .onboarding_practice
                    .get_or_insert_with(|| {
                        cx.new(|cx| practice::OnboardingPractice::new(language, window, cx))
                    })
                    .clone();
                practice.update(cx, |practice, cx| {
                    practice.activate();
                    practice.set_language(language, window, cx);
                });
                practice.into_any_element()
            }
            _ => self.onboarding_preferences(cx).into_any_element(),
        };
        let (title, detail) = match step {
            0 => (
                tr(
                    language,
                    "刚才复制的，随时找回来",
                    "Find what you copied, anytime",
                ),
                tr(
                    language,
                    "先记住唤出方式，再体验搜索和预览。",
                    "Learn how to open history, then try search and previews.",
                ),
            ),
            1 => (
                tr(
                    language,
                    "找回内容，试一次就会",
                    "Try finding it for yourself",
                ),
                tr(
                    language,
                    "用隔离的演示记录练习，不影响你的剪贴板。",
                    "Practice with isolated examples, without changing your clipboard.",
                ),
            ),
            _ => (
                tr(language, "按你的习惯开始", "Make it yours"),
                tr(
                    language,
                    "只设置常用选项，其他功能使用时再发现。",
                    "Choose the essentials. Discover the rest as you go.",
                ),
            ),
        };
        let content = settings_common::settings_stack()
            .gap(px(visual::SPACE_4))
            .child(
                div()
                    .text_size(px(if compact { 20. } else { 24. }))
                    .font_semibold()
                    .line_height(relative(1.25))
                    .child(title),
            )
            .child(settings_common::settings_description(detail, cx))
            .when(step == 0, |content| {
                content.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(visual::SPACE_2))
                        .child(div().text_size(px(visual::TEXT_BODY)).child(tr(
                            language,
                            "界面语言",
                            "Language",
                        )))
                        .child(self.onboarding_language_control(cx)),
                )
            })
            .child(body);
        let content = visual::reveal(
            content,
            ("onboarding-content", self.onboarding_revision),
            cx,
        );
        let steps = [
            ("随时唤出", "Open"),
            ("找回内容", "Try it"),
            ("开始使用", "Setup"),
        ];
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .px(px(visual::SPACE_4))
            .pb(px(visual::SPACE_4))
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_3))
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .child(
                Stepper::new("onboarding-steps")
                    .small()
                    .text_center(true)
                    .selected_index(step)
                    .disabled(pending)
                    .items(steps.iter().enumerate().map(|(index, (cn, en))| {
                        StepperItem::new()
                            .when(index < step, |item| {
                                item.icon(Icon::new(IconName::Check).size(px(visual::ICON_SMALL)))
                            })
                            .child(
                                div()
                                    .text_size(px(visual::TEXT_SMALL))
                                    .text_color(if index == step {
                                        cx.theme().primary
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(tr(language, cn, en)),
                            )
                    }))
                    .on_click(
                        cx.listener(|this, index, _, cx| this.select_onboarding_step(*index, cx)),
                    ),
            )
            .child(
                div()
                    .id(("onboarding-scroll", step))
                    .w_full()
                    .max_w(px(visual::CHOICE_MAX_WIDTH))
                    .mx_auto()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(content),
            )
            .when_some(self.onboarding_error.clone(), |view, error| {
                view.child(Alert::error("onboarding-save-error", error).small())
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(visual::CHOICE_MAX_WIDTH))
                    .mx_auto()
                    .flex_none()
                    .pt(px(visual::SPACE_3))
                    .border_t(px(visual::BORDER_WIDTH))
                    .border_color(cx.theme().border)
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(visual::SPACE_2))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(visual::SPACE_2))
                            .when(step > 0, |row| {
                                row.child(
                                    Button::new("onboarding-back")
                                        .ghost()
                                        .h(px(visual::CONTROL_HEIGHT_LARGE))
                                        .label(tr(language, "上一步", "Back"))
                                        .disabled(pending)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.select_onboarding_step(
                                                this.onboarding_step.saturating_sub(1),
                                                cx,
                                            );
                                        })),
                                )
                            })
                            .child(
                                Button::new("onboarding-next")
                                    .primary()
                                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                                    .label(if pending {
                                        tr(language, "正在保存…", "Saving…")
                                    } else if step == 2 {
                                        tr(language, "开始使用", "Get started")
                                    } else {
                                        tr(language, "下一步", "Continue")
                                    })
                                    .disabled(pending)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.advance_onboarding(window, cx)
                                    })),
                            ),
                    ),
            )
    }
}
