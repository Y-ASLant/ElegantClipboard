use super::file_cards::copy_or_paste_label;
use super::*;

/// 按钮栏的上下文标志，由 render_preview 一次判定后传入。
pub(super) struct ToolbarContext {
    pub(super) id: i64,
    pub(super) ready: bool,
    pub(super) image: bool,
    pub(super) files: bool,
    pub(super) rich: bool,
    pub(super) save_as_name: Option<String>,
    pub(super) save_as_label: &'static str,
}

impl ClipboardView {
    pub(super) fn render_preview_toolbar(
        &self,
        context: ToolbarContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let ToolbarContext {
            id,
            ready,
            image,
            files,
            rich,
            save_as_name,
            save_as_label,
        } = context;
        let html = matches!(self.preview.result, Some(Ok(PreviewContent::Html(_))));
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
                                .accessibility_label(tr(self.language, "缩小图片", "Zoom out"))
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
                                .accessibility_label(tr(self.language, "放大图片", "Zoom in"))
                                .disabled(self.image_zoom_percent >= 400)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.zoom_image(10, cx);
                                })),
                        ),
                )
            })
            .when(!self.preview_editing && self.preview_editable(), |bar| {
                bar.child(
                    Button::new("preview-edit")
                        .outline()
                        .label(tr(self.language, "编辑", "Edit"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.begin_preview_edit(window, cx);
                        })),
                )
            })
            .when(!self.preview_editing && (rich || html), |bar| {
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
                        } else if rich || html {
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
            })
    }
}
