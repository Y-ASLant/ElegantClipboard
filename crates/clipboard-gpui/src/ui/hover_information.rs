use super::presentation::{content_type_label, format_bytes, tr};
use crate::visual;
use clipboard_core::{database::ClipboardItem, preferences::LanguagePreference};
use gpui_kit::{
    component::{ActiveTheme, Sizable, tag::Tag},
    prelude::FluentBuilder,
    *,
};
use std::path::Path;

mod layout;

const GAP: f32 = visual::SPACE_2;
const ROW_HEIGHT: f32 = 22.0;
const CHIP_PADDING: f32 = visual::SPACE_3 / 2.;

struct InformationField {
    chinese: &'static str,
    english: &'static str,
    value: SharedString,
}

pub(super) struct HoverInformation {
    pub(super) note: Option<SharedString>,
    pub(super) image_dimensions: Option<(i64, i64)>,
    content_type: SharedString,
    fields: Vec<InformationField>,
    image_preview: bool,
}

/// Windows `canonicalize` 写入存储路径的 `\\?\`（及 `\\?\UNC\`）前缀仅服务文件系统，
/// 展示时还原为普通路径与 UNC 形式。
fn display_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    }
}

impl HoverInformation {
    pub(super) fn in_memory_image(dimensions: (i64, i64)) -> Self {
        Self {
            note: None,
            image_dimensions: Some(dimensions),
            content_type: "image".into(),
            fields: Vec::new(),
            image_preview: true,
        }
    }

    pub(super) fn new(item: &ClipboardItem, image_dimensions: Option<(i64, i64)>) -> Self {
        let dimensions = image_dimensions
            .or_else(|| item.image_width.zip(item.image_height))
            .filter(|(width, height)| *width > 0 && *height > 0);
        let image_preview = item.content_type == "image"
            || (item.content_type == "files" && image_dimensions.is_some());
        let mut information = Self {
            note: item
                .note
                .as_deref()
                .filter(|note| !note.trim().is_empty())
                .map(|note| SharedString::from(note.to_owned())),
            image_dimensions: dimensions,
            content_type: item.content_type.clone().into(),
            fields: Vec::new(),
            image_preview,
        };
        if let Ok(bytes) = u64::try_from(item.byte_size) {
            let size = if bytes >= 1024 {
                format!("{} · {bytes} B", format_bytes(bytes))
            } else {
                format_bytes(bytes)
            };
            // This is the persisted payload size, not an inferred original CF_DIB/file size.
            information.push("存储大小", "Stored size", size);
        }
        if item.content_type == "files" {
            let paths = clipboard_core::parse_file_paths_for_display(item.file_paths.as_deref());
            if !paths.is_empty() {
                information.push("文件数", "Files", paths.len().to_string());
                if let [path] = paths.as_slice() {
                    information.push_path(path, false);
                }
            }
        } else if item.content_type == "image" {
            if let Some(path) = item.image_path.as_deref().filter(|path| !path.is_empty()) {
                information.push_path(path, true);
            }
        } else {
            let chars = item
                .char_count
                .filter(|count| *count >= 0)
                .map(|count| count.to_string())
                .or_else(|| {
                    item.text_content
                        .as_deref()
                        .map(|text| text.chars().count().to_string())
                });
            if let Some(chars) = chars {
                information.push("字符数", "Characters", chars);
            }
            if let Some(text) = item.text_content.as_deref() {
                information.push("行数", "Lines", text.split('\n').count().to_string());
            }
            let format = match item.content_type.as_str() {
                "html" if item.html_content.is_some() => Some("HTML"),
                "rtf" if item.rtf_content.is_some() => Some("RTF"),
                "text" | "url" if item.text_content.is_some() => Some("UTF-8"),
                _ => None,
            };
            if let Some(format) = format {
                information.push("存储格式", "Stored format", format.to_owned());
            }
        }
        // 「数据格式」先列主剪贴板格式，再拼接捕获时保存的额外格式名。
        let mut data_formats = Vec::new();
        match item.content_type.as_str() {
            "image" => data_formats.push("PNG"),
            "files" => data_formats.push(if cfg!(windows) {
                "CF_HDROP"
            } else {
                "File URL"
            }),
            "html" => {
                if item.html_content.is_some() {
                    data_formats.push("HTML Format");
                }
                if item.rtf_content.is_some() {
                    data_formats.push("Rich Text Format");
                }
            }
            "rtf" => data_formats.push("Rich Text Format"),
            "text" | "url" => data_formats.push("Unicode Text"),
            _ => {}
        }
        for name in &item.formats {
            if !data_formats.contains(&name.as_str()) {
                data_formats.push(name.as_str());
            }
        }
        if !data_formats.is_empty() {
            information.push("数据格式", "Data format", data_formats.join("、"));
        }
        if let Some(source) = item
            .source_app_name
            .as_deref()
            .filter(|source| !source.is_empty())
        {
            information.push("来源", "Source", source.to_owned());
        }
        if !item.created_at.is_empty() {
            information.push("采集时间", "Captured at", item.created_at.clone());
        }
        information
    }

    fn push(&mut self, chinese: &'static str, english: &'static str, value: String) {
        self.fields.push(InformationField {
            chinese,
            english,
            value: value.into(),
        });
    }

    fn push_path(&mut self, path: &str, stored: bool) {
        let path_value = Path::new(path);
        if let Some(name) = path_value.file_name().and_then(|name| name.to_str()) {
            self.push("名称", "Name", name.to_owned());
        }
        self.push(
            if stored { "存储路径" } else { "路径" },
            if stored { "Stored path" } else { "Path" },
            display_path(path),
        );
        if let Some(extension) = path_value
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
        {
            self.push(
                if stored {
                    "存储格式"
                } else {
                    "文件扩展名"
                },
                if stored {
                    "Stored format"
                } else {
                    "File extension"
                },
                extension.to_uppercase(),
            );
        }
    }

    fn tags(
        &self,
        language: LanguagePreference,
        percentage: Option<i32>,
    ) -> Vec<(SharedString, bool)> {
        let kind = content_type_label(language, &self.content_type);
        let mut tags = Vec::with_capacity(self.fields.len() + 3);
        tags.push((SharedString::from(kind.to_owned()), false));
        let field_tag = |field: &InformationField| {
            (
                format!(
                    "{}: {}",
                    tr(language, field.chinese, field.english),
                    field.value
                )
                .into(),
                false,
            )
        };
        let mut fields = self.fields.iter();
        if let Some(size) = fields.next() {
            tags.push(field_tag(size));
        }
        if let Some((width, height)) = self.image_dimensions {
            let mut dimensions = format!(
                "{}: {width} × {height} px",
                tr(language, "分辨率", "Resolution")
            );
            if let Some(percentage) = percentage {
                use std::fmt::Write as _;
                let _ = write!(dimensions, " · {percentage}%");
            }
            tags.push((dimensions.into(), false));
        }
        tags.extend(fields.map(field_tag));
        let hint = if self.image_preview {
            tr(language, "Ctrl + 滚轮缩放", "Ctrl + wheel to zoom")
        } else {
            tr(language, "Ctrl + 滚轮滚动", "Ctrl + wheel to scroll")
        };
        tags.push((hint.into(), true));
        tags
    }
}

pub(super) struct InformationPlacement {
    pub(super) surface: Bounds<Pixels>,
    pub(super) owner: Bounds<Pixels>,
    pub(super) viewport: Size<Pixels>,
    pub(super) percentage: Option<i32>,
    pub(super) language: LanguagePreference,
}

pub(super) fn render_information(
    info: &HoverInformation,
    placement: InformationPlacement,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    let tags = info.tags(placement.language, placement.percentage);
    let mut text_style = window.text_style();
    text_style.font_family = cx.theme().font_family.clone();
    let widths: Vec<f32> = tags
        .iter()
        .map(|(text, _)| {
            let line = window.text_system().shape_line(
                text.clone(),
                px(visual::TEXT_SMALL),
                &[text_style.to_run(text.len())],
                None,
            );
            (f32::from(line.width).ceil() + (CHIP_PADDING + visual::BORDER_WIDTH) * 2.0).min(420.0)
        })
        .collect();
    let Some(layout) = layout::place_information(&placement, &widths) else {
        // Never cover the owner or the preview when no readable strip fits.
        return div().into_any_element();
    };
    let theme = cx.theme();
    div()
        .id("hover-information")
        .absolute()
        .left(px(layout.left))
        .top(px(layout.top))
        .w(px(layout.width))
        .h(px(layout.height))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .font_family(theme.font_family.clone())
        .children(layout.rows.into_iter().map(|range| {
            div()
                .flex()
                .flex_none()
                .h(px(ROW_HEIGHT))
                .gap(px(GAP))
                .when(layout.align_end, |row| row.justify_end())
                .children(range.map(|index| {
                    let (text, hint) = &tags[index];
                    Tag::custom(
                        theme.popover,
                        if *hint {
                            theme.muted_foreground
                        } else {
                            theme.popover_foreground
                        },
                        theme.border,
                    )
                    .small()
                    .border(px(visual::BORDER_WIDTH))
                    .rounded(theme.radius)
                    .w(px(widths[index].min(layout.width)))
                    .h(px(ROW_HEIGHT))
                    .flex_none()
                    .px(px(CHIP_PADDING))
                    .py(px(0.0))
                    .text_size(px(visual::TEXT_SMALL))
                    .line_height(px(ROW_HEIGHT - visual::BORDER_WIDTH * 2.))
                    .shadow(visual::control_shadow())
                    .child(div().min_w_0().truncate().child(text.clone()))
                }))
        }))
        .into_any_element()
}
