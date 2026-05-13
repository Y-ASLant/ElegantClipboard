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

pub fn tr<'a>(language: Language, key: &'a str) -> &'a str {
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
            "与呼出快捷键 {} 冲突" => "Conflicts with activation shortcut {}",
            "程序已在后台运行，按 {} 打开剪贴板" => "ElegantClipboard is running in the background. Press {} to open it.",
            "v{} → v{}，可在设置中查看详情" => "v{} -> v{}, view details in Settings",
            "创建设置窗口失败: {}" => "Failed to create settings window: {}",
            "源文件不存在" => "Source file does not exist",
            "保存失败: {}" => "Save failed: {}",
            "创建预览窗口失败: {}" => "Failed to create preview window: {}",
            "创建文本预览窗口失败: {}" => "Failed to create text preview window: {}",
            "创建编辑器窗口失败: {}" => "Failed to create editor window: {}",
            "TaskService 创建失败: {e}" => "Failed to create TaskService: {e}",
            "TaskService 连接失败: {e}" => "Failed to connect TaskService: {e}",
            "获取根文件夹失败: {e}" => "Failed to get root folder: {e}",
            "创建任务定义失败: {e}" => "Failed to create task definition: {e}",
            "注册任务失败: {e}" => "Failed to register task: {e}",
            "仅限 Windows" => "Windows only",
            "删除计划任务失败: {e}" => "Failed to delete scheduled task: {e}",
            "解析响应失败: {}" => "Failed to parse response: {}",
            "GitHub API 请求限额已用尽，请稍后再试" => "GitHub API rate limit exceeded. Please try again later.",
            "未找到发布版本" => "No release found",
            "GitHub API 返回错误: {}" => "GitHub API returned error: {}",
            "网络连接失败: {}" => "Network connection failed: {}",
            "下载服务器返回错误 (HTTP {})" => "Download server returned an error (HTTP {})",
            "网络连接失败，请检查网络后重试" => "Network connection failed. Check your connection and try again.",
            "创建临时目录失败: {}" => "Failed to create temporary directory: {}",
            "创建文件失败: {}" => "Failed to create file: {}",
            "读取数据失败: {}" => "Failed to read data: {}",
            "下载已取消" => "Download canceled",
            "写入文件失败: {}" => "Failed to write file: {}",
            "启动安装程序失败: {}" => "Failed to start installer: {}",
            "无法打开注册表项: {}" => "Failed to open registry key: {}",
            "无法设置注册表值: {}" => "Failed to set registry value: {}",
            "无法更新注册表值: {}" => "Failed to update registry value: {}",
            "无法启动Explorer进程: {}" => "Failed to start Explorer process: {}",
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
