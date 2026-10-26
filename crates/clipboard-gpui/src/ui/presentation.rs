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
}

/// 所有窗口统一使用的界面字体。
pub(super) const UI_FONT_FAMILY: &str = "Microsoft YaHei UI";

/// 所有窗口共用的外壳：仅此处绘制材质染色并统一界面字体，内容表面保留原有主题。
pub(super) fn window_shell(cx: &App) -> Div {
    let background =
        window_effect::shell_background(cx.theme().background, window_effect::active(cx));
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(background)
        .text_color(cx.theme().foreground)
        .font_family(UI_FONT_FAMILY)
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
        _ => message,
    }
}
