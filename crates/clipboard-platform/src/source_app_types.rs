//! 三平台共用的来源应用协议类型；采集与图标提取由各平台原生实现。

/// 一次复制事件的尽力归因来源（应用名 + 可执行文件路径）。
#[derive(Clone)]
pub struct SourceApp {
    pub name: String,
    pub executable: Option<String>,
}

/// 「运行中应用」选择器的列表项。
#[derive(Clone, Debug)]
pub struct RunningApp {
    pub name: String,
    pub process: String,
    pub icon: Option<String>,
}
