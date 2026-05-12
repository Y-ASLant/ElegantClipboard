use crate::database::SettingsRepository;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    ZhCn,
    EnUs,
}

impl Language {
    pub fn from_setting(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("en-US") => Self::EnUs,
            _ => Self::ZhCn,
        }
    }
}

pub const LANGUAGE_SETTING_KEY: &str = "interface_language";

pub fn current_language(repo: &SettingsRepository) -> Language {
    Language::from_setting(repo.get(LANGUAGE_SETTING_KEY).ok().flatten())
}

pub fn tr(language: Language, key: &str) -> &'static str {
    match language {
        Language::ZhCn => key,
        Language::EnUs => match key {
            "槽位" => "Slot",
            "收藏槽位" => "Favorite slot",
            "暂停监控" => "Pause monitoring",
            "恢复监控" => "Resume monitoring",
            "禁用快捷键" => "Disable shortcuts",
            "恢复快捷键" => "Resume shortcuts",
            "设置" => "Settings",
            "重启程序" => "Restart",
            "退出程序" => "Quit",
            "ElegantClipboard (已暂停)" => "ElegantClipboard (Paused)",
            "选择数据存储文件夹" => "Select data storage folder",
            "编辑" => "Edit",
            "另存为" => "Save as",
            "导出数据" => "Export data",
            "导入数据" => "Import data",
            "程序已在运行中" => "The app is already running",
            "ElegantClipboard 已启动" => "ElegantClipboard started",
            "发现新版本" => "New version available",
            "快捷键格式无效" => "has an invalid shortcut format",
            "快捷键至少包含一个修饰键 (Ctrl/Alt/Win)" => "A shortcut must include at least one modifier key (Ctrl/Alt/Win)",
            "快速粘贴不支持 Win 修饰键（Win+数字 是系统任务栏快捷键）" => "Quick paste does not support the Win modifier (Win+number is reserved by the taskbar)",
            "快捷键至少包含一个修饰键 (Ctrl/Alt)" => "A shortcut must include at least one modifier key (Ctrl/Alt)",
            _ => key,
        },
    }
}

pub fn format<'a>(language: Language, zh: &'a str, en: &'a str) -> &'a str {
    match language {
        Language::ZhCn => zh,
        Language::EnUs => en,
    }
}
