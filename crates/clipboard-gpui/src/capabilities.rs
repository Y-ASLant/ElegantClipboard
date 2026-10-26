pub fn desktop_unavailable_reason() -> Option<&'static str> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        clipboard_platform::desktop::unavailable_reason()
    }
    #[cfg(windows)]
    {
        None
    }
}

pub fn positioning_unavailable_reason() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        clipboard_platform::desktop::unavailable_reason()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub fn source_unavailable_reason() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        clipboard_platform::desktop::unavailable_reason()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
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
