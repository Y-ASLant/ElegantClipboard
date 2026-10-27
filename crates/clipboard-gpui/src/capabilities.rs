use clipboard_core::preferences::LanguagePreference;

pub fn wayland_session() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|value| value == "wayland"))
}

#[cfg(target_os = "linux")]
fn x11_unavailable_reason(language: LanguagePreference) -> Option<&'static str> {
    match language {
        LanguagePreference::Chinese if wayland_session() => Some(
            "Wayland 不支持全局窗口查询、模拟粘贴和 X11 全局快捷键；请切换至 X11 会话使用这些功能。",
        ),
        LanguagePreference::English if wayland_session() => Some(
            "Wayland does not permit global window inspection, synthetic paste, or X11 global shortcuts. Use an X11 session for these features.",
        ),
        LanguagePreference::Chinese if std::env::var_os("DISPLAY").is_none() => {
            Some("没有可用的 X11 显示服务。")
        }
        LanguagePreference::English if std::env::var_os("DISPLAY").is_none() => {
            Some("No X11 display is available.")
        }
        _ => None,
    }
}

pub fn global_shortcut_unavailable_reason(_language: LanguagePreference) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        x11_unavailable_reason(_language)
    }
    #[cfg(not(target_os = "linux"))]
    None
}

pub fn paste_unavailable_reason(_language: LanguagePreference) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        x11_unavailable_reason(_language)
    }
    #[cfg(target_os = "macos")]
    {
        clipboard_platform::desktop::unavailable_reason().map(|_| match _language {
            LanguagePreference::Chinese => "自动粘贴需要 macOS 系统设置 → 隐私与安全性 → 辅助功能权限",
            LanguagePreference::English => "Automatic paste requires Accessibility permission in macOS System Settings → Privacy & Security.",
        })
    }
    #[cfg(windows)]
    None
}

pub fn source_app_unavailable_reason(_language: LanguagePreference) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        x11_unavailable_reason(_language)
    }
    #[cfg(not(target_os = "linux"))]
    None
}

pub fn positioning_unavailable_reason(_language: LanguagePreference) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        x11_unavailable_reason(_language)
    }
    #[cfg(not(target_os = "linux"))]
    None
}

pub fn paste_key_supported(key: clipboard_core::preferences::PasteKeyPreference) -> bool {
    !cfg!(target_os = "macos") || key == clipboard_core::preferences::PasteKeyPreference::CtrlV
}

pub fn paste_key_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+V"
    } else {
        "Ctrl+V"
    }
}

pub fn audio_unavailable_reason() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        static AVAILABLE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var_os("PATH").is_some_and(|path| {
                std::env::split_paths(&path).any(|directory| directory.join("paplay").is_file())
            })
        });
        if !*AVAILABLE {
            return Some(
                "音效需要 paplay（pulseaudio-utils）；安装后重启应用。 / Audio requires paplay (pulseaudio-utils); install it and restart.",
            );
        }
    }
    None
}
