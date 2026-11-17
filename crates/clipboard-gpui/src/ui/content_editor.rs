use super::editor_window::{
    CONTEXT, editor_title, markdown_preview_source, memo_kind_toggle, save_shortcut_label,
};
use super::editor_window::{CancelContentEditor, SaveContentEditor};
use super::*;
use gpui_kit::component::input::{Editor, EditorState, Escape as EditorEscape};
use gpui_kit::component::text::TextView;

pub(super) struct ContentEditorWindow {
    owner: WeakEntity<ClipboardView>,
    editor: Entity<EditorState>,
    language: LanguagePreference,
    /// 全文只在第一次到达时写入，避免后续刷新覆盖用户正在改的内容。
    loaded: bool,
    pinned: bool,
    markdown_preview: bool,
    _owner_subscription: Subscription,
    _owner_release_subscription: Subscription,
    _editor_subscription: Subscription,
}

impl ContentEditorWindow {
    pub(super) fn new(
        owner: WeakEntity<ClipboardView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity = owner
            .upgrade()
            .expect("content editor requires its owner view");
        let language = entity.read(cx).language;
        let subscription = cx.observe_in(&entity, window, |this, owner, window, cx| {
            let owner = owner.read(cx);
            let language = owner.language;
            let adding_todo = owner.todo_draft.as_ref().map(|draft| draft.id.is_none());
            if language != this.language || adding_todo.is_some() {
                this.language = language;
                window.set_window_title(editor_title(language, adding_todo));
            }
            cx.notify();
        });
        let release_subscription = cx.observe_release(&entity, |_, _, cx| cx.notify());
        let editor = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let editor_subscription = cx.observe(&editor, |_, _, cx| cx.notify());
        window_lifecycle::bind_satellite_release(owner.clone(), window, cx);
        Self {
            owner,
            editor,
            language,
            loaded: false,
            pinned: false,
            markdown_preview: false,
            _owner_subscription: subscription,
            _owner_release_subscription: release_subscription,
            _editor_subscription: editor_subscription,
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
        // Cancel/Esc uses remove_window (no should_close); clear session first.
        let closed = self
            .owner
            .update(cx, |owner, cx| owner.close_content_editor_from_window(cx))
            .unwrap_or(true);
        if closed {
            window.remove_window();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).value().to_string();
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.todo_draft.is_some() {
                owner.save_todo_editor(text, cx);
            } else {
                owner.save_content_editor(text, cx);
            }
        });
    }

    fn title_bar(
        &self,
        adding_todo: Option<bool>,
        language: LanguagePreference,
        cx: &mut Context<Self>,
    ) -> TitleBar {
        let pinned = self.pinned;
        let preview = self.markdown_preview;
        TitleBar::new().bg(cx.theme().sidebar).child(
            div()
                .w_full()
                .flex()
                .flex_1()
                .items_center()
                .justify_between()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .text_size(cx.theme().font_size)
                        .font_family(cx.theme().font_family.clone())
                        .font_medium()
                        .child(editor_title(language, adding_todo)),
                )
                .child(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(editor_title_button(
                            "editor-pin",
                            gpui_kit::assets::IconName::Pin,
                            if pinned {
                                tr(language, "取消置顶", "Unpin window")
                            } else {
                                tr(language, "置顶窗口", "Pin window")
                            },
                            pinned,
                            cx.listener(|this, _, window, cx| {
                                this.pinned = !this.pinned;
                                native_window::set_above(window, this.pinned);
                                if this.pinned {
                                    native_window::raise_front(window);
                                }
                                cx.notify();
                            }),
                        ))
                        .child(editor_title_button(
                            "editor-preview",
                            IconName::Eye,
                            if preview {
                                tr(language, "隐藏预览", "Hide preview")
                            } else {
                                tr(language, "Markdown 预览", "Markdown preview")
                            },
                            preview,
                            cx.listener(|this, _, _, cx| {
                                this.markdown_preview = !this.markdown_preview;
                                cx.notify();
                            }),
                        )),
                ),
        )
    }
}

impl Render for ContentEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return unavailable_editor(self.language, cx);
        };
        let (language, pending, ready, loading, rich, adding_todo, memo_kind, error, initial_text) = {
            let view = owner.read(cx);
            let editing_todo = view.todo_draft.is_some();
            let memo_kind = view.todo_draft.as_ref().map(|draft| draft.kind);
            let adding_todo = view.todo_draft.as_ref().map(|draft| draft.id.is_none());
            let initial_text = if self.loaded {
                None
            } else if let Some(draft) = &view.todo_draft {
                Some(draft.text.clone())
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
                view.editor_pending() || view.todo_edit_saving,
                editing_todo
                    || matches!(
                        view.editor.result,
                        Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
                    ),
                !editing_todo && view.editor.result.is_none(),
                !editing_todo
                    && matches!(view.editor.result, Some(Ok(PreviewContent::RichText(_)))),
                adding_todo,
                memo_kind,
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
            .child(self.title_bar(adding_todo, language, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div().flex_1().min_h(px(120.)).min_w_0().when(
                            !self.markdown_preview,
                            |area| {
                                area.child(
                                    Editor::new(&self.editor)
                                        .appearance(false)
                                        .bordered(false)
                                        .h_full()
                                        .font_family(cx.theme().font_family.clone())
                                        .text_size(cx.theme().font_size)
                                        .readonly(!editable)
                                        .aria_label(tr(language, "编辑文本", "Edit text")),
                                )
                            },
                        )
                        .when(self.markdown_preview, |area| {
                            let preview = markdown_preview_source(
                                self.editor.read(cx).value().as_ref(),
                            );
                            area.child(
                                div().size_full().overflow_y_scrollbar().child(
                                    div()
                                        .px(px(visual::SPACE_4))
                                        .py(px(visual::SPACE_3))
                                        .child(
                                            TextView::markdown("editor-markdown-preview", preview)
                                                .selectable(false)
                                                .scrollable(false),
                                        ),
                                ),
                            )
                        }),
                    )
                    .when(loading || rich || error.is_some(), |body| {
                        body.child(
                            div()
                                .px(px(visual::SPACE_4))
                                .py(px(visual::SPACE_2))
                                .flex()
                                .flex_col()
                                .gap(px(visual::SPACE_2))
                                .when(loading, |status| {
                                    status.child(
                                        div()
                                            .text_size(px(visual::TEXT_SMALL))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(tr(language, "正在加载…", "Loading…")),
                                    )
                                })
                                .when(rich, |status| {
                                    status.child(
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
                                .when_some(error, |status, error| {
                                    status.child(
                                        Alert::error("editor-error", error)
                                            .small()
                                            .min_w_0()
                                            .whitespace_normal(),
                                    )
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .flex_shrink_0()
                    .px(px(visual::SPACE_4))
                    .py(px(visual::SPACE_3))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(memo_kind_toggle(
                        adding_todo == Some(true),
                        memo_kind,
                        language,
                        self.owner.clone(),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(visual::SPACE_2))
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
                    ),
            )
    }
}

fn editor_title_bar(language: LanguagePreference, adding_todo: Option<bool>, cx: &App) -> TitleBar {
    TitleBar::new().bg(cx.theme().sidebar).child(
        div()
            .text_size(cx.theme().font_size)
            .font_family(cx.theme().font_family.clone())
            .font_medium()
            .child(editor_title(language, adding_todo)),
    )
}

fn editor_title_button(
    id: &'static str,
    icon: impl Into<Icon>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .w(px(34.))
        .h(px(34.))
        .p_0()
        .rounded(px(0.))
        .selected(selected)
        .icon(icon.into().small())
        .tooltip(label.clone())
        .accessibility_label(label)
        .on_click(on_click)
}

fn unavailable_editor(language: LanguagePreference, cx: &mut App) -> Div {
    window_shell(cx)
        .child(editor_title_bar(language, None, cx))
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
