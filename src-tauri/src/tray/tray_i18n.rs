/// 托盘菜单国际化文本
///
/// 托盘菜单由 Rust 原生渲染，不经过 React，因此需要独立的翻译表。
/// 支持三种语言：zh-CN（默认）、en、zh-TW。
pub struct TrayI18n {
    pub pause_monitor: String,
    pub resume_monitor: String,
    pub disable_shortcuts: String,
    pub restore_shortcuts: String,
    pub settings: String,
    pub restart: String,
    pub quit: String,
    pub paused_tip: String,
}

impl TrayI18n {
    /// 根据语言设置创建翻译实例，未知语言回退到 zh-CN
    pub fn from_locale(locale: &str) -> Self {
        match locale {
            "en" => Self {
                pause_monitor: "Pause Monitoring".into(),
                resume_monitor: "Resume Monitoring".into(),
                disable_shortcuts: "Disable Shortcuts".into(),
                restore_shortcuts: "Restore Shortcuts".into(),
                settings: "Settings".into(),
                restart: "Restart".into(),
                quit: "Quit".into(),
                paused_tip: "ElegantClipboard (Paused)".into(),
            },
            "zh-TW" => Self {
                pause_monitor: "暫停監控".into(),
                resume_monitor: "恢復監控".into(),
                disable_shortcuts: "停用快捷鍵".into(),
                restore_shortcuts: "恢復快捷鍵".into(),
                settings: "設定".into(),
                restart: "重新啟動".into(),
                quit: "結束程式".into(),
                paused_tip: "ElegantClipboard（已暫停）".into(),
            },
            _ => Self::zh_cn(),
        }
    }

    pub fn zh_cn() -> Self {
        Self {
            pause_monitor: "暂停监控".into(),
            resume_monitor: "恢复监控".into(),
            disable_shortcuts: "禁用快捷键".into(),
            restore_shortcuts: "恢复快捷键".into(),
            settings: "设置".into(),
            restart: "重启程序".into(),
            quit: "退出程序".into(),
            paused_tip: "ElegantClipboard (已暂停)".into(),
        }
    }
}
