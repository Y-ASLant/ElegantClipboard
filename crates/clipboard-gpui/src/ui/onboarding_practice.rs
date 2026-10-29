use super::super::*;
use clipboard_core::search::search_highlight_ranges;
use gpui_kit::component::input::{Copy, Cut, Paste};
use std::sync::Arc;

#[path = "onboarding_practice_cards.rs"]
mod cards;
#[path = "onboarding_practice_preview.rs"]
mod preview;

pub(in crate::ui) struct OnboardingPractice {
    language: LanguagePreference,
    search: Entity<InputState>,
    input: Entity<TextareaState>,
    image: Arc<RenderImage>,
    source_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    popup: Option<WindowHandle<HoverPreviewWindowView>>,
    generation: u64,
    // Retained between steps; stale hover events must not reopen an unmounted exercise.
    active: bool,
    hovered: bool,
    opening: bool,
    pointer_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy)]
enum Sample {
    Text,
    Image,
    RichText,
}

impl Sample {
    fn label(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::Text => tr(language, "演示 · 文字", "Demo · Text"),
            Self::Image => tr(language, "演示 · 图片", "Demo · Image"),
            Self::RichText => tr(language, "演示 · 富文本", "Demo · Rich text"),
        }
    }

    fn text(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::Text => tr(
                language,
                "今天的灵感，随时找回。",
                "Find today's inspiration, anytime.",
            ),
            Self::Image => tr(
                language,
                "山间日落 · 悬停预览，Ctrl + 滚轮缩放",
                "Mountain sunset · Hover to preview; Ctrl + wheel to zoom",
            ),
            Self::RichText => tr(
                language,
                "项目笔记：保留灵感，专注创作。",
                "Project notes: Keep inspiration. Stay creative.",
            ),
        }
    }

    fn html(language: LanguagePreference) -> &'static str {
        tr(
            language,
            "<p><strong>项目笔记：</strong><em>保留灵感</em>，专注创作。</p>",
            "<p><strong>Project notes:</strong> <em>Keep inspiration.</em> Stay creative.</p>",
        )
    }

    fn matches(self, query: &str, language: LanguagePreference) -> bool {
        query.trim().is_empty()
            || !search_highlight_ranges(self.label(language), query).is_empty()
            || !search_highlight_ranges(self.text(language), query).is_empty()
    }
}

impl OnboardingPractice {
    pub(in crate::ui) fn new(
        language: LanguagePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).context_menu(false));
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(3).context_menu(false));
        let subscriptions = vec![
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.source_bounds.set(None);
                    this.close_preview(cx);
                }
            }),
            cx.observe_window_visibility(window, |this, visibility, _, cx| {
                if !visibility.is_visible() {
                    this.suspend(cx);
                }
            }),
            cx.observe_window_bounds(window, |this, _, cx| {
                this.source_bounds.set(None);
                this.close_preview(cx);
            }),
        ];
        cx.on_release(|this, cx| {
            if let Some(handle) = this.popup.take() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            cx.drop_image(this.image.clone(), None);
        })
        .detach();
        let mut this = Self {
            language,
            search,
            input,
            image: preview::sample_image(),
            source_bounds: Rc::new(Cell::new(None)),
            popup: None,
            generation: 0,
            active: true,
            hovered: false,
            opening: false,
            pointer_task: None,
            _subscriptions: subscriptions,
        };
        this.update_placeholders(window, cx);
        this
    }

    pub(in crate::ui) fn set_language(
        &mut self,
        language: LanguagePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.language == language {
            return;
        }
        self.language = language;
        self.source_bounds.set(None);
        self.update_placeholders(window, cx);
        self.close_preview(cx);
    }

    fn update_placeholders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language;
        self.search.update(cx, |state, cx| {
            state.set_placeholder(
                tr(
                    language,
                    "搜索演示记录，如「灵感」",
                    "Search demo records, e.g. inspiration",
                ),
                window,
                cx,
            );
        });
        self.input.update(cx, |state, cx| {
            state.set_placeholder(
                tr(
                    language,
                    "在此练习输入或插入样例",
                    "Type here or insert a sample",
                ),
                window,
                cx,
            );
        });
    }

    pub(in crate::ui) fn activate(&mut self) {
        self.active = true;
    }

    pub(in crate::ui) fn suspend(&mut self, cx: &mut Context<Self>) {
        self.active = false;
        self.source_bounds.set(None);
        self.close_preview(cx);
    }

    pub(in crate::ui) fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.hovered = false;
        self.opening = false;
        self.pointer_task = None;
        if let Some(handle) = self.popup.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        cx.notify();
    }

    fn insert(&mut self, sample: Sample, window: &mut Window, cx: &mut Context<Self>) {
        self.close_preview(cx);
        let text = if matches!(sample, Sample::RichText) {
            std::borrow::Cow::Owned(
                clipboard_core::html_text::html_to_text(Sample::html(self.language))
                    .expect("built-in HTML sample is within the plain-text conversion limit"),
            )
        } else {
            std::borrow::Cow::Borrowed(sample.text(self.language))
        };
        self.input.update(cx, |state, cx| {
            state.insert(text.as_ref(), window, cx);
            state.focus(window, cx);
        });
    }
}

impl Render for OnboardingPractice {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let paste_help = if crate::capabilities::paste_unavailable_reason(language).is_some() {
            tr(
                language,
                "当前环境不支持自动回贴；正常使用时请复制后回到目标应用手动粘贴。",
                "Automatic paste-back is unavailable here. In normal use, copy an item and paste it manually in your app.",
            )
        } else {
            tr(
                language,
                "实际使用：先在目标应用放好光标，再用快捷键唤出并选择记录回贴。Shift+Enter 使用纯文本；从托盘打开时只复制，需手动粘贴。",
                "In real use, place the cursor in your app, open history with the shortcut, then choose an item to paste back. Shift+Enter uses plain text. Opening from the tray copies only; paste manually.",
            )
        };
        let query = self.search.read(cx).value();
        let samples = [Sample::Text, Sample::Image, Sample::RichText];
        let cards = samples
            .into_iter()
            .filter(|sample| sample.matches(&query, language))
            .map(|sample| self.card(sample, &query, cx))
            .collect::<Vec<_>>();
        let empty = cards.is_empty();
        // Capture before the input's own handlers: even explicit copy/paste
        // shortcuts stay isolated. Context menus are disabled on both states.
        div().id("onboarding-practice").w_full().flex().flex_col().gap(px(visual::SPACE_2))
            .capture_action(|_: &Copy, _, cx| cx.stop_propagation())
            .capture_action(|_: &Cut, _, cx| cx.stop_propagation())
            .capture_action(|_: &Paste, _, cx| cx.stop_propagation())
            .on_scroll(cx.listener(|this, _: &ScrollEvent, _, cx| this.close_preview(cx)))
            .child(div().text_size(px(visual::TEXT_SMALL)).text_color(cx.theme().muted_foreground)
                .child(tr(language,
                    "演示数据不写入历史；练习区不读取或改写系统剪贴板，禁用系统复制粘贴。",
                    "Demo only: no saved history or system clipboard access. System copy/paste is disabled in this exercise.")))
            .child(Input::new(&self.search).cleanable(true).aria_label(tr(language, "搜索演示记录", "Search demo records")))
            .children(cards)
            .when(empty, |body| body.child(div().text_size(px(visual::TEXT_SMALL))
                .child(tr(language, "没有匹配的演示记录，请清空搜索重试。", "No demo matches. Clear the search to try again."))))
            .child(div().font_semibold().child(tr(language, "练习输入框 · 非系统回贴", "Practice field · Not system paste-back")))
            .child(Textarea::new(&self.input).h(px(76.)).aria_label(tr(language, "隔离的练习输入框", "Isolated practice field")))
            .child(settings_common::settings_description(paste_help, cx))
    }
}
