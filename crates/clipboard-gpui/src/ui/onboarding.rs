use super::*;

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
            || self.paste_state.sequence_remaining().is_some()
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
        self.paste_state.set_target(None);
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
        self.paste_state.set_target(None);
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
                tr(language, "设置唤出快捷键", "Set your shortcut"),
                tr(
                    language,
                    "在任何应用中，一键打开剪贴板历史。",
                    "Open clipboard history from any app.",
                ),
            ),
            1 => (
                tr(language, "体验搜索与粘贴", "Find and paste"),
                tr(
                    language,
                    "试试搜索、预览和插入，全程使用演示内容。",
                    "Try search, previews and insertion with sample content.",
                ),
            ),
            _ => (
                tr(language, "选择使用偏好", "Choose your preferences"),
                tr(
                    language,
                    "选好外观和常用选项，之后可在设置中调整。",
                    "Choose your appearance and defaults. Change them later in Settings.",
                ),
            ),
        };
        let header = div()
            .w_full()
            .max_w(px(visual::CHOICE_MAX_WIDTH))
            .mx_auto()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_2))
            .pb(px(visual::SPACE_2))
            .child(
                div()
                    .text_size(px(22.))
                    .font_semibold()
                    .line_height(px(30.))
                    .child(title),
            )
            .child(settings_common::settings_description(detail, cx));
        let content = settings_common::settings_stack()
            .gap(px(visual::SPACE_4))
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
            .child(header)
            .child(
                div()
                    .id(("onboarding-scroll", step))
                    .w_full()
                    .max_w(px(visual::CHOICE_MAX_WIDTH))
                    .mx_auto()
                    .flex_1()
                    .min_h_0()
                    // Official scrollbars overlay the scroll area. Reserve a
                    // gutter in the content so controls and focus rings stay clear.
                    .pr(px(visual::SPACE_4))
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
