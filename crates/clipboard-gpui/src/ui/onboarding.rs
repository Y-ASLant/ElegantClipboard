use super::*;
use gpui_kit::component::stepper::{Stepper, StepperItem};

impl ClipboardView {
    pub(super) fn onboarding_visible(&self) -> bool {
        self.show_onboarding && !self.onboarding_completed
    }

    pub(super) fn advance_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.onboarding_visible() || self.onboarding_pending {
            return;
        }
        if self.onboarding_step < 3 {
            self.onboarding_step += 1;
            cx.notify();
        } else {
            self.complete_onboarding(cx);
        }
    }

    pub(super) fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        if self.onboarding_visible()
            && !self.onboarding_pending
            && self.send(Command::CompleteOnboarding, cx)
        {
            self.onboarding_pending = true;
            cx.notify();
        }
    }
    pub(super) fn render_onboarding(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let steps = [
            (
                IconName::Copy,
                tr(language, "采集", "Capture"),
                tr(
                    language,
                    "欢迎使用 ElegantClipboard",
                    "Welcome to ElegantClipboard",
                ),
                tr(
                    language,
                    "复制的内容保存在本机，随时可从历史中查找。",
                    "Copied content stays on this device and is easy to find later.",
                ),
                tr(
                    language,
                    "复制文本、图片或文件即可开始记录",
                    "Copy text, images, or files to start recording",
                ),
            ),
            (
                IconName::Eye,
                tr(language, "搜索", "Search"),
                tr(language, "快速搜索", "Quick search"),
                tr(
                    language,
                    "在顶部搜索框输入关键词，历史卡片会显示匹配内容。",
                    "Search from the top field and see matches in your history cards.",
                ),
                tr(language, "聚焦搜索框", "to focus search"),
            ),
            (
                IconName::Star,
                tr(language, "收藏", "Favorites"),
                tr(language, "置顶与收藏", "Pin and favorite"),
                tr(
                    language,
                    "把常用记录置顶或收藏，也可以通过卡片按钮查看详情。",
                    "Pin or favorite useful items and open card details when needed.",
                ),
                tr(
                    language,
                    "从目标应用用快捷键唤出后，点击卡片可自动粘贴",
                    "Open from a target app with the shortcut, then click a card to paste",
                ),
            ),
            (
                IconName::SquareTerminal,
                tr(language, "快捷键", "Shortcuts"),
                tr(language, "键盘快捷键", "Keyboard shortcuts"),
                tr(
                    language,
                    "使用键盘管理历史",
                    "Manage history with the keyboard",
                ),
                tr(
                    language,
                    "使用纯文本表示",
                    "uses the plain-text representation",
                ),
            ),
        ];
        let (icon, _, title, description, tip) = steps[self.onboarding_step.min(3)].clone();
        let is_last = self.onboarding_step == 3;
        let description_content = if self.onboarding_step == 3 {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(language, "方向键选择记录；", "Arrow keys select items;"))
                .child(shortcut_kbd("Enter").expect("onboarding enter key"))
                .child(tr(language, "复制或粘贴；", "to copy or paste;"))
                .child(shortcut_kbd("Delete").expect("onboarding delete key"))
                .child(tr(language, "删除；", "to delete;"))
                .child(shortcut_kbd("Left").expect("onboarding left key"))
                .child(tr(language, "和", "and"))
                .child(shortcut_kbd("Right").expect("onboarding right key"))
                .child(tr(language, "切换分类。", "change categories."))
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description)
                .into_any_element()
        };
        let tip_content = match self.onboarding_step {
            1 => div()
                .flex()
                .items_center()
                .gap_1()
                .child(tr(language, "按", "Press"))
                .child(shortcut_kbd("Ctrl+F").expect("onboarding search key"))
                .child(tip)
                .into_any_element(),
            3 => div()
                .flex()
                .items_center()
                .gap_1()
                .child(shortcut_kbd("Shift+Enter").expect("onboarding plain text key"))
                .child(tip)
                .into_any_element(),
            _ => div().child(tip).into_any_element(),
        };
        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .p_4()
            .flex()
            .items_center()
            .justify_center()
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .child(
                div()
                    .w_full()
                    .max_w(px(390.))
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(panel_background(cx))
                    .p_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(64.))
                            .h(px(64.))
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(icon).text_color(cx.theme().primary)),
                    )
                    .child(div().text_lg().font_semibold().child(title))
                    .child(description_content)
                    .child(
                        div()
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().primary)
                            .child(tip_content),
                    )
                    .child(
                        Stepper::new("onboarding-steps")
                            .small()
                            .text_center(true)
                            .selected_index(self.onboarding_step)
                            .disabled(self.onboarding_pending)
                            .items(
                                steps
                                    .iter()
                                    .map(|(_, label, _, _, _)| StepperItem::new().child(*label)),
                            )
                            .on_click(cx.listener(|this, index, _, cx| {
                                if this.onboarding_visible() && !this.onboarding_pending {
                                    this.onboarding_step = *index;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .child(
                                Button::new("onboarding-skip")
                                    .small()
                                    .ghost()
                                    .label(tr(language, "跳过", "Skip"))
                                    .disabled(self.onboarding_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.complete_onboarding(cx);
                                    })),
                            )
                            .child(
                                Button::new("onboarding-next")
                                    .small()
                                    .primary()
                                    .label(if is_last {
                                        tr(language, "开始使用", "Get started")
                                    } else {
                                        tr(language, "下一步", "Next")
                                    })
                                    .disabled(self.onboarding_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.advance_onboarding(cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(language, "按", "Press"))
                            .child(shortcut_kbd("Esc").expect("onboarding skip key"))
                            .child(tr(language, "跳过引导", "to skip onboarding")),
                    ),
            )
    }
}
