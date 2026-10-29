use super::*;
pub(super) fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    let (unit_bytes, unit) = if bytes >= GIB {
        (GIB, "GiB")
    } else if bytes >= MIB {
        (MIB, "MiB")
    } else if bytes >= KIB {
        (KIB, "KiB")
    } else {
        return format!("{bytes} B");
    };
    format!("{:.1} {unit}", bytes as f64 / unit_bytes as f64)
}

pub(super) fn apply_theme(preference: ThemePreference, window: &mut Window, cx: &mut App) {
    match preference {
        ThemePreference::System => Theme::sync_system_appearance(Some(window), cx),
        ThemePreference::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        ThemePreference::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    crate::design_system::apply(cx);
    #[cfg(windows)]
    acrylic::sync_theme(window, cx);
}

pub(super) use crate::design_system::UI_FONT_FAMILY;

/// 所有窗口共用材质画布；内部表面由主题角色统一投影，浮层保持清晰。
pub(super) fn window_shell(cx: &App) -> Div {
    let background = crate::design_system::canvas_background(cx);
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(background)
        .text_color(cx.theme().foreground)
        .font_family(UI_FONT_FAMILY)
        .text_size(px(visual::TEXT_BODY))
        .line_height(relative(1.5))
}

pub(super) fn tr(
    language: LanguagePreference,
    chinese: &'static str,
    english: &'static str,
) -> &'static str {
    match language {
        LanguagePreference::Chinese => chinese,
        LanguagePreference::English => english,
    }
}

/// 内容类型的本地化标签；历史卡片与悬停信息共用，避免译名分叉。
pub(super) fn content_type_label(language: LanguagePreference, content_type: &str) -> &'static str {
    match content_type {
        "image" => tr(language, "图片", "Image"),
        "files" => tr(language, "文件", "Files"),
        "html" => "HTML",
        "rtf" => tr(language, "富文本", "Rich text"),
        "url" => tr(language, "链接", "Link"),
        _ => tr(language, "文本", "Text"),
    }
}
pub(super) fn localize_service_message(language: LanguagePreference, message: String) -> String {
    // 含 "Ctrl+V" 的消息只产自 Windows 平台层；macOS 的按键符号由
    // capabilities::paste_key_label 持有，这里不重复替换。
    if language == LanguagePreference::Chinese {
        return message;
    }
    match message.as_str() {
        "目标窗口已关闭，内容已复制，请手动粘贴" =>
            "The target window has closed. The content was copied; paste it manually.".into(),
        "前台窗口已变化，已取消自动粘贴，请检查后手动粘贴" =>
            "The active window changed. Automatic paste was cancelled; check the target and paste manually.".into(),
        "剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴" =>
            "The clipboard changed. Automatic paste was cancelled; check its contents before pasting manually.".into(),
        "无法聚焦原窗口，内容已复制，请手动粘贴" =>
            "Unable to focus the previous window. The content was copied; paste it manually.".into(),
        "目标窗口未接受粘贴快捷键，内容已复制，请手动粘贴" =>
            "The paste shortcut could not be sent. The content was copied; paste it manually.".into(),
        "等待修饰键释放超时，已取消自动粘贴；请松开按键后手动粘贴" =>
            "Timed out waiting for modifier keys. Automatic paste was cancelled; release the keys and paste manually.".into(),
        "富文本超过 1 MiB，按纯文本保存" => {
            "Rich text exceeded 1 MiB and was saved as plain text".into()
        }
        "已在资源管理器中定位" => "Located in file manager".into(),
        "已打开数据目录" => "Data folder opened".into(),
        "路径已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Paths copied; switch to the target app and press Ctrl+V".into()
        }
        "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Plain text copied; switch to the target app and press Ctrl+V".into()
        }
        "图片文件已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Image file copied; switch to the target app and press Ctrl+V".into()
        }
        "图片已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Image copied; switch to the target app and press Ctrl+V".into()
        }
        "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Files copied; switch to the target app and press Ctrl+V".into()
        }
        "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Rich text copied; switch to the target app and press Ctrl+V".into()
        }
        "富文本格式未写回，已按纯文本复制" => {
            "Rich-text formatting could not be restored; copied as plain text".into()
        }
        "已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Copied; switch to the target app and press Ctrl+V".into()
        }
        "HTML 已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "HTML copied; switch to the target app and press Ctrl+V".into()
        }
        "RTF 已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "RTF copied; switch to the target app and press Ctrl+V".into()
        }
        "转换后的文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Transformed text copied; switch to the target app and press Ctrl+V".into()
        }
        // Unix 复制路径的短形式消息（不带「可切换…」后缀）。
        "内容已复制" => "Content copied".into(),
        "富文本已复制" => "Rich text copied".into(),
        "HTML 已复制" => "HTML copied".into(),
        "RTF 已复制" => "RTF copied".into(),
        "转换后的文本已复制" => "Transformed text copied".into(),
        "恢复原剪贴板失败" => "Failed to restore the previous clipboard".into(),
        _ if message.starts_with("恢复原剪贴板失败：") => message.replacen(
            "恢复原剪贴板失败：",
            "Failed to restore the previous clipboard: ",
            1,
        ),
        _ if message.starts_with("部分原生格式未写回：") => message.replacen(
            "部分原生格式未写回：",
            "Some native formats were not restored: ",
            1,
        ),
        _ if message.starts_with("部分原生格式未保存：") => message.replacen(
            "部分原生格式未保存：",
            "Some native formats were not saved: ",
            1,
        ),
        _ if message.starts_with("已在资源管理器中定位第一项，共") => message.replacen(
            "已在资源管理器中定位第一项，共",
            "Located the first item in the file manager; ",
            1,
        ).replacen("项", " items total", 1),
        _ if message.starts_with("警告：文件路径已保存，但部分暂存操作未完成：") => message.replacen(
            "警告：文件路径已保存，但部分暂存操作未完成：",
            "Warning: file paths were saved, but some staging steps failed: ",
            1,
        ),
        _ => message,
    }
}
