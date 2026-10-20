use super::file_cards::{copy_or_paste_label, save_as_suggested_name};
use super::*;

impl ClipboardView {
    pub(super) fn render_preview(&self, cx: &mut Context<Self>) -> Div {
        let id = self.preview.id.expect("preview is open");
        let ready = matches!(self.preview.result, Some(Ok(_)));
        let image = matches!(self.preview.result, Some(Ok(PreviewContent::Image(_))));
        let files = matches!(self.preview.result, Some(Ok(PreviewContent::Files(_))));
        let rich = matches!(self.preview.result, Some(Ok(PreviewContent::RichText(_))));
        let image_unavailable = tr(self.language, "图片无法显示", "Image unavailable");
        let save_as_name = match &self.preview.result {
            Some(Ok(PreviewContent::Image(path))) => save_as_suggested_name(path),
            Some(Ok(PreviewContent::Files(entries))) => entries
                .first()
                .and_then(|entry| save_as_suggested_name(&entry.original_path)),
            _ => None,
        };
        let save_as_label = match &self.preview.result {
            Some(Ok(PreviewContent::Files(paths))) if paths.len() > 1 => {
                tr(self.language, "另存第一项", "Save first item as")
            }
            _ => tr(self.language, "另存为", "Save as"),
        };
        let editable = matches!(
            self.preview.result,
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
        ) && self.preview_source_hash.is_some();
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
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_2()
                    .when(image, |bar| {
                        bar.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("image-zoom-out")
                                        .small()
                                        .outline()
                                        .icon(IconName::Minus)
                                        .accessibility_label(tr(
                                            self.language,
                                            "缩小图片",
                                            "Zoom out",
                                        ))
                                        .disabled(self.image_zoom_percent <= 50)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_image(-10, cx);
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-reset")
                                        .small()
                                        .ghost()
                                        .label(format!("{}%", self.image_zoom_percent))
                                        .accessibility_label(tr(
                                            self.language,
                                            "重置图片缩放",
                                            "Reset zoom",
                                        ))
                                        .disabled(self.image_zoom_percent == 100)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.image_zoom_percent = 100;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-in")
                                        .small()
                                        .outline()
                                        .icon(IconName::Plus)
                                        .accessibility_label(tr(
                                            self.language,
                                            "放大图片",
                                            "Zoom in",
                                        ))
                                        .disabled(self.image_zoom_percent >= 400)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_image(10, cx);
                                        })),
                                ),
                        )
                    })
                    .when(!self.preview_editing && editable, |bar| {
                        bar.child(
                            Button::new("preview-edit")
                                .outline()
                                .label(tr(self.language, "编辑", "Edit"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_preview_edit(window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing && rich, |bar| {
                        let plain_label = copy_or_paste_label(
                            self.language,
                            self.paste_target.is_some(),
                            "纯文本",
                            "plain text",
                        );
                        bar.child(
                            Button::new("preview-copy-plain")
                                .outline()
                                .label(plain_label)
                                .disabled(!ready || self.paste_pending.is_some())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.copy_or_paste_plain_text(id, window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing && (image || files), |bar| {
                        bar.when_some(save_as_name, |bar, suggested_name| {
                            bar.child(
                                Button::new("preview-save-as")
                                    .outline()
                                    .label(save_as_label)
                                    .disabled(!ready || self.save_as_pending.is_some())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.start_save_as(id, suggested_name.clone(), cx);
                                    })),
                            )
                        })
                        .child(
                            Button::new("preview-reveal")
                                .outline()
                                .label(tr(
                                    self.language,
                                    "在资源管理器中显示",
                                    "Show in File Explorer",
                                ))
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::RevealInExplorer(id), cx);
                                })),
                        )
                        .child(
                            Button::new("preview-copy-path")
                                .outline()
                                .label(copy_or_paste_label(
                                    self.language,
                                    self.paste_target.is_some() && self._tray.is_some(),
                                    "路径",
                                    "paths",
                                ))
                                .disabled(!ready || self.paste_pending.is_some())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.copy_or_paste_path(id, window, cx);
                                })),
                        )
                    })
                    .when(!self.preview_editing, |bar| {
                        bar.child(
                            Button::new("preview-paste")
                                .outline()
                                .label(tr(
                                    self.language,
                                    "粘贴到原窗口",
                                    "Paste to previous window",
                                ))
                                .disabled(
                                    !ready
                                        || self.paste_target.is_none()
                                        || self.paste_pending.is_some()
                                        || self._tray.is_none(),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.paste_selected(id, window, cx);
                                })),
                        )
                        .child(
                            Button::new("preview-copy")
                                .primary()
                                .label(if image {
                                    tr(self.language, "复制图片", "Copy image")
                                } else if files {
                                    tr(self.language, "复制文件", "Copy files")
                                } else if rich {
                                    tr(self.language, "复制富文本", "Copy rich text")
                                } else {
                                    tr(self.language, "复制全文", "Copy all text")
                                })
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::Copy(id), cx);
                                })),
                        )
                    })
                    .when(self.preview_editing, |bar| {
                        bar.child(
                            Button::new("preview-cancel-edit")
                                .ghost()
                                .label(tr(self.language, "取消", "Cancel"))
                                .disabled(self.preview_save_pending)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_preview(window, cx);
                                })),
                        )
                        .child(
                            Button::new("preview-save-edit")
                                .primary()
                                .label(tr(self.language, "保存文本", "Save text"))
                                .disabled(self.preview_save_pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.save_preview_edit(cx);
                                })),
                        )
                    }),
            )
    }
}
