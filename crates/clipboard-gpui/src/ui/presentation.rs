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

/// 开窗时套用窗口效果：先 `record` 让外壳底色算对，再由 `apply` 改 DWM 与窗口样式。
/// 必须在窗口首次绘制之前调用，否则第一帧仍是不透明背景。
pub(super) fn init_window_effect(
    preference: WindowEffectPreference,
    window: &mut Window,
    cx: &mut App,
) {
    acrylic::record(preference, cx);
    acrylic::apply(preference, window);
}

/// 所有窗口共用的外壳：仅此处绘制材质染色，内容表面保留原有主题。
pub(super) fn window_shell(cx: &App) -> Div {
    let background = acrylic::shell_background(cx.theme().background, acrylic::active(cx));
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(background)
        .text_color(cx.theme().foreground)
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
    if language == LanguagePreference::Chinese {
        return message;
    }
    match message.as_str() {
        "富文本超过 1 MiB，按纯文本保存" => {
            "Rich text exceeded 1 MiB and was saved as plain text".into()
        }
        "已在资源管理器中定位" => "Located in File Explorer".into(),
        "已打开数据目录" => "Data folder opened".into(),
        "路径已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Paths copied; switch to the target app and press Ctrl+V".into()
        }
        "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴" => {
            "Plain text copied; switch to the target app and press Ctrl+V".into()
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
