use super::*;
use gpui_kit::component::input::{Editor, EditorState, Escape as EditorEscape};

const CONTEXT: &str = "ContentEditor";
const EDITOR_WIDTH: f32 = 640.;
const EDITOR_HEIGHT: f32 = 480.;
const EDITOR_MIN_WIDTH: f32 = 420.;
const EDITOR_MIN_HEIGHT: f32 = 280.;

actions!(content_editor, [SaveContentEditor, CancelContentEditor]);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelContentEditor, Some(CONTEXT)),
        KeyBinding::new("ctrl-enter", SaveContentEditor, Some(CONTEXT)),
        KeyBinding::new("ctrl-s", SaveContentEditor, Some(CONTEXT)),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SaveContentEditor, Some(CONTEXT)),
        KeyBinding::new("cmd-s", SaveContentEditor, Some(CONTEXT)),
    ]);
}

fn editor_title(language: LanguagePreference) -> &'static str {
    tr(language, "编辑内容", "Edit content")
}

fn save_shortcut_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘S"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+S"
    }
}

pub(super) struct ContentEditorWindow {
    owner: WeakEntity<ClipboardView>,
    editor: Entity<EditorState>,
    language: LanguagePreference,
    /// 全文只在第一次到达时写入，避免后续刷新覆盖用户正在改的内容。
    loaded: bool,
    _owner_subscription: Subscription,
    _owner_release_subscription: Subscription,
}

impl ContentEditorWindow {
    fn new(owner: WeakEntity<ClipboardView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let entity = owner
            .upgrade()
            .expect("content editor requires its owner view");
        let language = entity.read(cx).language;
        let subscription = cx.observe_in(&entity, window, |this, owner, window, cx| {
            let language = owner.read(cx).language;
            if language != this.language {
                this.language = language;
                window.set_window_title(editor_title(language));
            }
            cx.notify();
        });
        let release_subscription = cx.observe_release(&entity, |_, _, cx| cx.notify());
        Self {
            owner,
            editor: cx.new(|cx| EditorState::new(window, cx).line_number(true)),
            language,
            loaded: false,
            _owner_subscription: subscription,
            _owner_release_subscription: release_subscription,
        }
    }

    fn load_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.editor
            .update(cx, |editor, cx| editor.set_value(text, window, cx));
        cx.defer_in(window, |this, window, cx| {
            this.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        });
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self
            .owner
            .update(cx, |owner, _| owner.editor_pending())
            .unwrap_or(false);
        if !pending {
            window.remove_window();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).value().to_string();
        let _ = self
            .owner
            .update(cx, |owner, cx| owner.save_content_editor(text, cx));
    }
}

impl ClipboardView {
    /// 历史面板太小，正文编辑使用可缩放的独立窗口，而不是嵌在面板里的对话框。
    pub(super) fn open_content_editor(
        &mut self,
        id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display_id = window.display(cx).map(|display| display.id());
        let bounds = Bounds::centered(display_id, size(px(EDITOR_WIDTH), px(EDITOR_HEIGHT)), cx);
        let theme = self.theme;
        let language = self.language;
        let window_effect = self.window_effect;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let still_waiting = owner_entity.update(cx, |view, _| {
                view.editor.id == Some(id) && view.editor_window.is_none()
            });
            if !still_waiting {
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: Some(TitlebarOptions {
                    title: Some(editor_title(language).into()),
                    ..TitleBar::title_bar_options()
                }),
                window_min_size: Some(size(px(EDITOR_MIN_WIDTH), px(EDITOR_MIN_HEIGHT))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.elegant-clipboard-gpui.editor".into()),
                ..TitleBar::window_options()
            };
            let editor_owner = owner.clone();
            let close_owner = owner.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                apply_theme(theme, window, cx);
                acrylic::init_window_effect(window_effect, window, cx);
                window.set_window_title(editor_title(language));
                window.on_window_should_close(cx, move |_, cx| {
                    close_owner
                        .update(cx, |owner, cx| owner.close_content_editor_from_window(cx))
                        .unwrap_or(true)
                });
                cx.new(|cx| ContentEditorWindow::new(editor_owner, window, cx))
            });
            owner_entity.update(cx, |owner, cx| {
                match opened {
                    Ok((handle, _)) => owner.editor_window = Some(handle),
                    Err(error) => owner.fail_content_editor_open(error, cx),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn close_content_editor_from_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.editor_pending() {
            return false;
        }
        self.editor_window = None;
        if self.editor.id.take().is_none() {
            return true;
        }
        self.release_editor_session();
        let focus = self.list_focus.clone();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| window.focus(&focus, cx));
        });
        cx.notify();
        true
    }

    fn fail_content_editor_open(&mut self, error: impl std::fmt::Debug, cx: &mut Context<Self>) {
        self.editor_window = None;
        if self.editor.id.take().is_some() {
            self.release_editor_session();
        }
        self.set_feedback_message(format!(
            "{}: {error:?}",
            tr(
                self.language,
                "无法打开编辑窗口",
                "Unable to open the editor"
            )
        ));
        self.is_error = true;
        cx.notify();
    }
}

impl Render for ContentEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return unavailable_editor(self.language, cx);
        };
        let (language, pending, ready, loading, rich, error, initial_text) = {
            let view = owner.read(cx);
            let initial_text = if self.loaded {
                None
            } else {
                match view.editor.result.as_ref() {
                    Some(Ok(PreviewContent::Text(text) | PreviewContent::RichText(text))) => {
                        Some(text.clone())
                    }
                    _ => None,
                }
            };
            let error = view.editor_error.clone().or_else(|| {
                view.editor
                    .result
                    .as_ref()
                    .and_then(|result| result.as_ref().err().cloned())
            });
            (
                view.language,
                view.editor_pending(),
                matches!(
                    view.editor.result,
                    Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
                ),
                view.editor.result.is_none(),
                matches!(view.editor.result, Some(Ok(PreviewContent::RichText(_)))),
                error,
                initial_text,
            )
        };
        if let Some(text) = initial_text {
            self.load_text(text, window, cx);
        }
        self.language = language;
        let editable = self.loaded && ready && !pending;
        let this = cx.weak_entity();
        let cancel = this.clone();
        let save = this.clone();
        window_shell(cx)
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &CancelContentEditor, window, cx| {
                this.cancel(window, cx);
            }))
            // 编辑器先用 Esc 关掉查找和多余光标；没有这些时才把 Esc 传上来取消编辑。
            .on_action(cx.listener(|this, _: &EditorEscape, window, cx| {
                this.cancel(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveContentEditor, _, cx| this.save(cx)))
            .child(editor_title_bar(language, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .p(px(visual::SPACE_4))
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_3))
                    .child(
                        div().flex_1().min_h(px(120.)).min_w_0().child(
                            Editor::new(&self.editor)
                                .flex_1()
                                .h_full()
                                // 输入框默认圆角偏大；编辑区跟随卡片和预览的圆角设置。
                                .rounded(crate::card_radius::card_radius(cx))
                                // 编辑器默认等宽字体。剪贴板正文跟随界面字体和字号。
                                .font_family(cx.theme().font_family.clone())
                                .text_size(cx.theme().font_size)
                                .readonly(!editable)
                                .aria_label(tr(language, "编辑文本", "Edit text")),
                        ),
                    )
                    .when(loading, |body| {
                        body.child(
                            div()
                                .text_size(px(visual::TEXT_SMALL))
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(language, "正在加载…", "Loading…")),
                        )
                    })
                    .when(rich, |body| {
                        body.child(
                            div()
                                .text_size(px(visual::TEXT_SMALL))
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(
                                    language,
                                    "保存后将变为纯文本，原富文本格式不会保留",
                                    "Saving converts this to plain text and removes rich-text formatting.",
                                )),
                        )
                    })
                    .when_some(error, |body, error| {
                        body.child(
                            Alert::error("editor-error", error)
                                .small()
                                .min_w_0()
                                .whitespace_normal(),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(visual::SPACE_2))
                    .px(px(visual::SPACE_4))
                    .py(px(visual::SPACE_3))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("editor-cancel")
                            .outline()
                            .label(tr(language, "取消", "Cancel"))
                            .disabled(pending)
                            .on_click(move |_, window, cx| {
                                let _ = cancel.update(cx, |view, cx| view.cancel(window, cx));
                            }),
                    )
                    .child(
                        Button::new("editor-save")
                            .primary()
                            .label(if pending {
                                tr(language, "保存中…", "Saving…")
                            } else {
                                tr(language, "保存", "Save")
                            })
                            .disabled(!editable)
                            .tooltip(save_shortcut_label())
                            .on_click(move |_, _, cx| {
                                let _ = save.update(cx, |view, cx| view.save(cx));
                            }),
                    ),
            )
    }
}

fn editor_title_bar(language: LanguagePreference, cx: &App) -> TitleBar {
    TitleBar::new().bg(cx.theme().sidebar).child(
        div()
            .text_size(cx.theme().font_size)
            .font_family(cx.theme().font_family.clone())
            .font_medium()
            .child(editor_title(language)),
    )
}

fn unavailable_editor(language: LanguagePreference, cx: &mut App) -> Div {
    window_shell(cx)
        .child(editor_title_bar(language, cx))
        .child(
            div().p(px(visual::SPACE_3)).child(
                Alert::error(
                    "editor-owner-unavailable",
                    tr(
                        language,
                        "主窗口已关闭，无法保存编辑。请关闭此窗口。",
                        "The main window is no longer available. Close this window; changes were not saved.",
                    ),
                )
                .small()
                .min_w_0()
                .whitespace_normal(),
            ),
        )
}
