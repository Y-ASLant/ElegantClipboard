use super::*;

#[path = "onboarding_scenes.rs"]
mod scenes;

/// 引导专用示意内容，不访问系统剪贴板或创建真实历史记录。
struct Preview {
    step: usize,
    language: LanguagePreference,
    shortcut: String,
    compact: bool,
    background: Hsla,
    foreground: Hsla,
    muted: Hsla,
    muted_foreground: Hsla,
    border: Hsla,
    blue: Hsla,
    green: Hsla,
    amber: Hsla,
}

pub(super) fn render(view: &ClipboardView, compact: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let preview = Preview {
        step: view.onboarding_step.min(3),
        language: view.language,
        shortcut: if view.hotkey_choice == HotkeyPreference::Disabled {
            tr(view.language, "从托盘打开", "Open from the tray").to_owned()
        } else {
            view.hotkey_choice.label().to_owned()
        },
        compact,
        background: theme.background,
        foreground: theme.foreground,
        muted: theme.muted,
        muted_foreground: theme.muted_foreground,
        border: theme.border,
        blue: theme.blue,
        green: theme.green,
        amber: theme.yellow,
    };
    if cx.reduce_motion() {
        return preview.frame(None).into_any_element();
    }
    let demo = div().with_animation(
        ("onboarding-demo", view.onboarding_revision),
        Animation::new(Duration::from_millis(5200))
            .repeat()
            .with_max_fps(30.),
        move |_, phase| preview.frame(Some(phase)),
    );
    div()
        .relative()
        .child(demo)
        .with_animation(
            ("onboarding-scene-enter", view.onboarding_revision),
            Animation::new(Duration::from_millis(420)).with_easing(ease_out_quint()),
            |element, progress| element.top(px(14. * (1. - progress))).opacity(progress),
        )
        .into_any_element()
}

impl Preview {
    fn text(&self, chinese: &'static str, english: &'static str) -> &'static str {
        tr(self.language, chinese, english)
    }

    // None 直接呈现完成状态，不依赖循环中的某个采样时刻。
    fn frame(&self, phase: Option<f32>) -> Div {
        let height = if self.compact { 164. } else { 222. };
        let content = match self.step {
            1 => self.search(phase),
            2 => self.favorites(phase),
            3 => self.paste(phase),
            _ => self.capture(phase),
        };
        div()
            .relative()
            .w_full()
            .h(px(height))
            .rounded_xl()
            .overflow_hidden()
            .bg(self.background.blend(self.blue.opacity(0.055)))
            .border_1()
            .border_color(self.blue.opacity(0.12))
            .child(
                div()
                    .absolute()
                    .size(px(170.))
                    .rounded_full()
                    .right(px(-45.))
                    .top(px(-90.))
                    .bg(self.blue.opacity(0.06)),
            )
            .child(
                div()
                    .absolute()
                    .size(px(100.))
                    .rounded_full()
                    .left(px(-35.))
                    .bottom(px(-55.))
                    .bg(self.blue.opacity(0.045)),
            )
            .child(content)
            .child(
                div()
                    .absolute()
                    .bottom(px(9.))
                    .right(px(12.))
                    .text_size(px(10.))
                    .text_color(self.muted_foreground)
                    .opacity(0.7)
                    .child(self.text("操作预览", "HOW IT WORKS")),
            )
    }

    fn surface(&self) -> Div {
        div()
            .absolute()
            .rounded_lg()
            .border_1()
            .border_color(self.border)
            .bg(self.background)
            .text_color(self.foreground)
            .overflow_hidden()
    }

    fn heading(&self, label: &'static str) -> Div {
        div()
            .h(px(30.))
            .px_3()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(self.border.opacity(0.65))
            .children(
                [self.muted_foreground.opacity(0.25); 3]
                    .into_iter()
                    .map(|color| div().size(px(4.)).rounded_full().bg(color)),
            )
            .child(
                div()
                    .ml_2()
                    .text_size(px(10.))
                    .text_color(self.muted_foreground)
                    .child(label),
            )
    }

    fn history_card(
        &self,
        title: &'static str,
        detail: &'static str,
        icon: IconName,
        active: bool,
    ) -> Div {
        div()
            .w_full()
            .h(px(43.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(if active {
                self.blue.opacity(0.35)
            } else {
                self.border.opacity(0.65)
            })
            .bg(if active {
                self.background.blend(self.blue.opacity(0.08))
            } else {
                self.background
            })
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .rounded_md()
                    .bg(if active {
                        self.blue.opacity(0.12)
                    } else {
                        self.muted
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).small().text_color(if active {
                        self.blue
                    } else {
                        self.muted_foreground
                    })),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .line_clamp(1)
                            .font_medium()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .line_clamp(1)
                            .text_color(self.muted_foreground)
                            .child(detail),
                    ),
            )
    }
}
