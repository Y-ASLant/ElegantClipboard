use super::*;

impl ClipboardView {
    pub(super) fn render_file_preview(
        &self,
        entries: &[FilePreviewEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("file-details-scroll")
            .size_full()
            .p_1()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scrollbar()
            .when_some(single_file_image_path(entries), |body, path| {
                let unavailable = tr(self.language, "图片无法显示", "Image unavailable");
                body.child(
                    div()
                        .w_full()
                        .h(px(260.))
                        .flex_none()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted)
                        .overflow_hidden()
                        .child(
                            img(path)
                                .size_full()
                                .object_fit(ObjectFit::Contain)
                                .with_fallback(move || div().child(unavailable).into_any_element()),
                        ),
                )
            })
            .children(entries.iter().enumerate().map(|(index, entry)| {
                let name = std::path::Path::new(&entry.original_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| entry.original_path.clone());
                let unavailable = !entry.exists || entry.metadata_error.is_some();
                let status = if entry.metadata_error.is_some() {
                    tr(self.language, "无法读取", "Unreadable").to_owned()
                } else if !entry.exists {
                    tr(self.language, "已失效", "Missing").to_owned()
                } else if entry.is_dir {
                    tr(self.language, "文件夹", "Folder").to_owned()
                } else if let Some(size) = entry.size {
                    format_bytes(size)
                } else {
                    tr(self.language, "文件", "File").to_owned()
                };
                div()
                    .id(("file-detail", index))
                    .flex_none()
                    .rounded_md()
                    .border_1()
                    .border_color(if unavailable {
                        cx.theme().danger
                    } else {
                        cx.theme().border
                    })
                    .bg(cx.theme().background)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_semibold()
                                    .line_clamp(1)
                                    .text_ellipsis()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(if unavailable {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().border
                                    })
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .text_color(if unavailable {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(status),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_clamp(2)
                            .text_ellipsis()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.language == LanguagePreference::English {
                                format!("Original path: {}", entry.original_path)
                            } else {
                                format!("原始路径：{}", entry.original_path)
                            }),
                    )
                    .when_some(entry.metadata_error.clone(), |row, error| {
                        row.child(div().text_xs().text_color(cx.theme().danger).child(
                            if self.language == LanguagePreference::English {
                                format!("Unable to read metadata: {error}")
                            } else {
                                format!("无法读取元数据：{error}")
                            },
                        ))
                    })
            }))
            .into_any_element()
    }
}
