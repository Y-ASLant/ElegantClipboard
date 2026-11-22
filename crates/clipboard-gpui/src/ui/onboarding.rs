use super::*;

#[path = "onboarding_favorites.rs"]
pub(super) mod favorites;
#[path = "onboarding_practice.rs"]
pub(super) mod practice;
#[path = "onboarding_preferences.rs"]
mod preferences;
#[path = "onboarding_shortcut.rs"]
pub(super) mod shortcut;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OnboardingMode {
    FirstRun,
    Review,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OnboardingStep {
    Shortcut,
    FindAndPaste,
    Favorites,
    Preferences,
}

impl OnboardingStep {
    const ALL: [Self; 4] = [
        Self::Shortcut,
        Self::FindAndPaste,
        Self::Favorites,
        Self::Preferences,
    ];

    fn next(self) -> Option<Self> {
        Self::ALL.get(self as usize + 1).copied()
    }

    fn previous(self) -> Option<Self> {
        (self as usize).checked_sub(1).map(|index| Self::ALL[index])
    }

    fn title_and_description(self, language: LanguagePreference) -> (&'static str, &'static str) {
        match self {
            Self::Shortcut => (
                tr(language, "设置唤出快捷键", "Set your shortcut"),
                tr(
                    language,
                    "在任何应用中，一键打开剪贴板历史。",
                    "Open clipboard history from any app.",
                ),
            ),
            Self::FindAndPaste => (
                tr(language, "体验搜索与粘贴", "Find and paste"),
                tr(
                    language,
                    "用示例记录试试搜索、悬停预览和插入。",
                    "Try search, hover previews, and insertion with sample records.",
                ),
            ),
            Self::Favorites => (
                tr(language, "练习拖拽收藏", "Organize with favorites"),
                tr(
                    language,
                    "在「全部」拖入可以收藏，在「收藏」拖入可以取消。",
                    "Drag a card from All to save it. Drag it from Favorites to remove it.",
                ),
            ),
            Self::Preferences => (
                tr(language, "选择使用偏好", "Choose your preferences"),
                tr(
                    language,
                    "选好外观和常用选项，之后可在设置中调整。",
                    "Choose your appearance and defaults. Change them later in Settings.",
                ),
            ),
        }
    }
}

impl ClipboardView {
    pub(super) fn onboarding_visible(&self) -> bool {
        self.onboarding_mode.is_some()
    }

    fn reviewing_onboarding(&self) -> bool {
        self.onboarding_mode == Some(OnboardingMode::Review)
    }

    pub(super) fn onboarding_saving(&self) -> bool {
        self.onboarding_pending
            || self.shortcut_change_pending()
            || self.theme_pending
            || self.theme_color_pending
            || self.corner_radius.pending
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

    pub(super) fn cancel_onboarding_drag(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.onboarding_favorites.as_ref().is_some_and(|practice| {
            practice.update(cx, |practice, cx| practice.cancel_drag(window, cx))
        })
    }

    pub(super) fn dismiss_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cancel_onboarding_drag(window, cx)
            && self.reviewing_onboarding()
            && !self.onboarding_saving()
        {
            self.leave_onboarding(window, cx);
        }
    }

    pub(super) fn open_onboarding(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        if !self.onboarding_visible() {
            // Don't replace a live editor or interrupt work in the history panel.
            if self.editor.id.is_some()
                || self.editor_window.is_some()
                || self.todo_draft.is_some()
                || self.active_overlay().is_some()
                || self.card_drag_active(cx)
                || self.paste_state.pending()
                || self.paste_state.merging()
                || self.paste_state.quick_pending()
                || self.paste_state.sequence_remaining().is_some()
            {
                return Err(tr(
                    self.language,
                    "请先完成历史面板中的操作或关闭编辑器，再打开引导。",
                    "Finish the current history operation or close the editor before opening the guide.",
                ));
            }
            self.close_history_menu(cx);
            self.close_hover_preview("onboarding_opening", cx);
            self.onboarding_step = OnboardingStep::Shortcut;
            self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
            self.onboarding_hotkey_used = false;
            self.onboarding_mode = Some(OnboardingMode::Review);
        }
        self.paste_state.set_target(None);
        self.show_window(window, cx);
        cx.notify();
        Ok(())
    }

    fn select_onboarding_step(
        &mut self,
        step: OnboardingStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.onboarding_visible() && !self.onboarding_saving() && self.onboarding_step != step {
            self.cancel_onboarding_drag(window, cx);
            self.suspend_onboarding(cx);
            self.onboarding_step = step;
            self.onboarding_revision = self.onboarding_revision.wrapping_add(1);
            window.focus(&self.list_focus, cx);
            cx.notify();
        }
    }

    pub(super) fn leave_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_onboarding_drag(window, cx);
        self.suspend_onboarding(cx);
        self.onboarding_shortcut = None;
        self.onboarding_practice = None;
        self.onboarding_favorites = None;
        self.onboarding_mode = None;
        self.onboarding_error = None;
        self.paste_state.set_target(None);
        if self.search_auto_focus.value {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.list_focus, cx);
        }
        cx.notify();
    }

    fn advance_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_saving() {
            return;
        }
        if let Some(next) = self.onboarding_step.next() {
            self.select_onboarding_step(next, window, cx);
            return;
        }
        if self.reviewing_onboarding() {
            self.leave_onboarding(window, cx);
            return;
        }
        if self.send(Command::CompleteOnboarding, cx) {
            self.onboarding_pending = true;
            self.onboarding_error = None;
        } else {
            self.onboarding_error = Some(self.message.clone());
        }
        cx.notify();
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

    fn render_onboarding_step(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language;
        match self.onboarding_step {
            OnboardingStep::Shortcut => {
                let owner = cx.weak_entity();
                let initial = self.hotkey_choice.clone();
                let shortcut = self
                    .onboarding_shortcut
                    .get_or_insert_with(|| {
                        cx.new(|cx| shortcut::OnboardingShortcut::new(owner, initial, window, cx))
                    })
                    .clone();
                settings_common::settings_stack()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(visual::SPACE_2))
                            .child(tr(language, "界面语言", "Language"))
                            .child(self.onboarding_language_control(cx)),
                    )
                    .child(shortcut)
                    .into_any_element()
            }
            OnboardingStep::FindAndPaste => {
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
            OnboardingStep::Favorites => {
                let practice = self
                    .onboarding_favorites
                    .get_or_insert_with(|| {
                        cx.new(|cx| favorites::OnboardingFavorites::new(language, window, cx))
                    })
                    .clone();
                practice.update(cx, |practice, cx| practice.set_language(language, cx));
                practice.into_any_element()
            }
            OnboardingStep::Preferences => {
                self.onboarding_preferences(window, cx).into_any_element()
            }
        }
    }

    fn onboarding_navigation(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let step = self.onboarding_step;
        let pending = self.onboarding_saving();
        let number = step as usize + 1;
        let total = OnboardingStep::ALL.len();
        let progress = match language {
            LanguagePreference::Chinese => format!("第 {number} / {total} 步"),
            LanguagePreference::English => format!("Step {number} of {total}"),
        };
        div()
            .w_full()
            .flex_none()
            .pt(px(visual::SPACE_3))
            .border_t(px(visual::BORDER_WIDTH))
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .gap(px(visual::SPACE_2))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(visual::TEXT_SMALL))
                    .text_color(cx.theme().muted_foreground)
                    .child(progress),
            )
            .when_some(step.previous(), |nav, previous| {
                nav.child(
                    Button::new("onboarding-back")
                        .ghost()
                        .h(px(visual::CONTROL_HEIGHT_LARGE))
                        .label(tr(language, "上一步", "Back"))
                        .disabled(pending)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_onboarding_step(previous, window, cx);
                        })),
                )
            })
            .child(
                Button::new("onboarding-next")
                    .primary()
                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                    .label(if pending {
                        tr(language, "正在保存…", "Saving…")
                    } else if step != OnboardingStep::Preferences {
                        tr(language, "下一步", "Continue")
                    } else if self.reviewing_onboarding() {
                        tr(language, "退出引导", "Exit guide")
                    } else {
                        tr(language, "开始使用", "Get started")
                    })
                    .disabled(pending)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.advance_onboarding(window, cx)),
                    ),
            )
    }

    pub(super) fn render_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let step = self.onboarding_step;
        let (title, detail) = step.title_and_description(self.language);
        let header = div()
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
        let content = div()
            .w_full()
            .child(self.render_onboarding_step(window, cx));
        let content = visual::reveal(
            content,
            ("onboarding-content", self.onboarding_revision),
            cx,
        );
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .max_w(px(visual::CHOICE_MAX_WIDTH + 2. * visual::SPACE_4))
            .mx_auto()
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
                    .id(("onboarding-scroll", step as usize))
                    .w_full()
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
            .child(self.onboarding_navigation(cx))
    }
}
