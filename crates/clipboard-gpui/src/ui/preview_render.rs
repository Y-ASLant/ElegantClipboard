use super::*;
use gpui_kit::component::text::TextView;

/// HTML 富文本预览的滚动容器：完整预览页与悬停浮窗共用，仅元素 id 不同。
pub(super) fn html_preview_body(id: &'static str, source: impl Into<SharedString>) -> AnyElement {
    div()
        .id(id)
        .size_full()
        .overflow_scroll()
        .child(
            TextView::html(id, source.into())
                .selectable(true)
                .scrollable(true),
        )
        .into_any_element()
}

impl ClipboardView {
    /// 编辑入口判定：Html 原文不支持编辑（保存即降级纯文本，对 HTML 无意义）。
    pub(super) fn preview_editable(&self) -> bool {
        matches!(
            self.preview.result,
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
        ) && self.preview_source_hash.is_some()
    }

    pub(super) fn render_preview(&self, cx: &mut Context<Self>) -> Div {
        let id = self.preview.id.expect("preview is open");
        let image = matches!(self.preview.result, Some(Ok(PreviewContent::Image(_))));
        let files = matches!(self.preview.result, Some(Ok(PreviewContent::Files(_))));
        let rich = matches!(self.preview.result, Some(Ok(PreviewContent::RichText(_))));
        let html = matches!(self.preview.result, Some(Ok(PreviewContent::Html(_))));
        let image_unavailable = tr(self.language, "图片无法显示", "Image unavailable");
        let message = if self.preview_editing && rich {
            tr(
                self.language,
                "保存后将变为纯文本，原富文本格式不会保留",
                "Saving converts this to plain text and removes rich-text formatting.",
            )
            .into()
        } else if self.preview_editing {
            tr(
                self.language,
                "编辑文本；保存后搜索结果会更新",
                "Edit the text; search results update after saving.",
            )
            .into()
        } else {
            match &self.preview.result {
                None => tr(self.language, "正在加载完整内容…", "Loading full content…").to_owned(),
                Some(Err(error)) => error.clone(),
                Some(Ok(PreviewContent::Text(text)))
                    if self.language == LanguagePreference::English =>
                {
                    format!(
                        "{} characters · {} bytes · read-only",
                        text.chars().count(),
                        text.len()
                    )
                }
                Some(Ok(PreviewContent::Text(text))) => {
                    format!("{} 字符 · {} 字节 · 只读", text.chars().count(), text.len())
                }
                Some(Ok(PreviewContent::Image(_))) => tr(
                    self.language,
                    "图片预览 · 保持原始比例",
                    "Image preview · Original aspect ratio",
                )
                .into(),
                Some(Ok(PreviewContent::Html(_))) => tr(
                    self.language,
                    "HTML 预览 · 远程图片不加载",
                    "HTML preview · Remote images are not loaded",
                )
                .into(),
                Some(Ok(PreviewContent::RichText(_))) => tr(
                    self.language,
                    "富文本 · 纯文本预览",
                    "Rich text · Plain-text preview",
                )
                .into(),
                Some(Ok(PreviewContent::Files(entries))) => {
                    let directories = entries
                        .iter()
                        .filter(|entry| entry.exists && entry.is_dir)
                        .count();
                    let missing = entries
                        .iter()
                        .filter(|entry| !entry.exists && entry.metadata_error.is_none())
                        .count();
                    let unreadable = entries
                        .iter()
                        .filter(|entry| entry.metadata_error.is_some())
                        .count();
                    let recovered = entries.iter().filter(|entry| entry.recovered).count();
                    let mut details = vec![if self.language == LanguagePreference::English {
                        format!("{} items", entries.len())
                    } else {
                        format!("{} 项", entries.len())
                    }];
                    if directories > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{directories} folders")
                        } else {
                            format!("{directories} 个文件夹")
                        });
                    }
                    if missing > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{missing} missing")
                        } else {
                            format!("{missing} 项已失效")
                        });
                    }
                    if unreadable > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{unreadable} unreadable")
                        } else {
                            format!("{unreadable} 项无法读取")
                        });
                    }
                    if recovered > 0 {
                        details.push(if self.language == LanguagePreference::English {
                            format!("{recovered} from backup copies")
                        } else {
                            format!("{recovered} 项使用备份副本")
                        });
                    }
                    details.join(" · ")
                }
            }
        };
        let body: AnyElement = match &self.preview.result {
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_))) => {
                Textarea::new(&self.preview_input)
                    .readonly(!self.preview_editing || self.preview_save_pending)
                    .h_full()
                    .aria_label(if self.preview_editing {
                        tr(self.language, "编辑文本", "Edit text")
                    } else {
                        tr(self.language, "完整文本内容", "Full text content")
                    })
                    .into_any_element()
            }
            Some(Ok(PreviewContent::Html(source))) => {
                html_preview_body("preview-html", source.clone())
            }
            Some(Ok(PreviewContent::Files(entries))) => self.render_file_preview(entries, cx),
            Some(Ok(PreviewContent::Image(path))) => div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("image-preview-viewport")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&self.image_scroll)
                        .on_scroll_wheel(cx.listener(
                            |this, event: &ScrollWheelEvent, window, cx| {
                                if event.modifiers.control {
                                    let delta = event.delta.pixel_delta(window.line_height()).y;
                                    if delta > px(0.) {
                                        this.zoom_image(10, cx);
                                    } else if delta < px(0.) {
                                        this.zoom_image(-10, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .when(self.image_zoom_percent <= 100, |viewport| {
                            viewport.flex().items_center().justify_center()
                        })
                        .child(
                            div()
                                .w(relative(f32::from(self.image_zoom_percent) / 100.0))
                                .h(relative(f32::from(self.image_zoom_percent) / 100.0))
                                .flex_none()
                                .child(
                                    img(path.clone())
                                        .size_full()
                                        .object_fit(ObjectFit::Contain)
                                        .with_fallback(move || {
                                            div().child(image_unavailable).into_any_element()
                                        }),
                                ),
                        ),
                )
                .scrollbar(&self.image_scroll, ScrollbarAxis::Both)
                .into_any_element(),
            _ => div().into_any_element(),
        };
        div()
            .key_context("Preview")
            .track_focus(&self.list_focus)
            .flex()
            .flex_col()
            .size_full()
            .p(px(PAGE_PADDING))
            .gap_3()
            .on_action(
                cx.listener(|this, _: &ClosePreview, window, cx| this.close_preview(window, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(if self.preview_editing {
                                tr(self.language, "编辑内容", "Edit content")
                            } else if image {
                                tr(self.language, "图片预览", "Image preview")
                            } else if files {
                                tr(self.language, "文件详情", "File details")
                            } else if html {
                                tr(self.language, "HTML 预览", "HTML preview")
                            } else if rich {
                                tr(self.language, "富文本预览", "Rich-text preview")
                            } else {
                                tr(self.language, "完整内容", "Full content")
                            }),
                    )
                    .child(
                        Button::new("preview-close")
                            .ghost()
                            .label(if self.preview_editing {
                                tr(self.language, "取消编辑 (Esc)", "Cancel editing (Esc)")
                            } else {
                                tr(self.language, "返回列表 (Esc)", "Back to list (Esc)")
                            })
                            .disabled(self.preview_save_pending)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_preview(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            )
            .child(div().flex_1().min_h_0().child(body))
            .child(self.render_preview_note(cx))
            .child(self.render_preview_toolbar(id, cx))
    }

    /// 预览页底部的备注编辑行：单行输入 + 保存按钮。
    fn render_preview_note(&self, cx: &mut Context<Self>) -> Div {
        let note_id = format!("preview-note-{}", self.preview.id.unwrap_or(0));
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Input::new(&self.note_input)
                    .id(SharedString::from(note_id))
                    .small(),
            )
            .child(
                Button::new("preview-note-save")
                    .small()
                    .outline()
                    .label(if self.note_save_pending {
                        tr(self.language, "保存中…", "Saving…")
                    } else {
                        tr(self.language, "保存备注", "Save note")
                    })
                    .disabled(self.note_save_pending)
                    .on_click(cx.listener(|this, _, _, cx| this.save_preview_note(cx))),
            )
    }
}
