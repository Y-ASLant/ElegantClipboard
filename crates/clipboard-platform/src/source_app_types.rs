//! 三平台共用的来源应用协议类型；采集与图标提取由各平台原生实现。

/// 一次复制事件的尽力归因来源（应用名、可执行路径、可选显示细节）。
#[derive(Clone)]
pub struct SourceApp {
    pub name: String,
    pub executable: Option<String>,
    /// 网页地址或文档名；只影响历史展示标签，不参与应用过滤。
    pub detail: Option<String>,
}

impl SourceApp {
    /// 写入历史的来源标签；core 拒绝超过 128 字符的名称。
    pub fn history_label(&self) -> String {
        let Some(detail) = self
            .detail
            .as_deref()
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
        else {
            return self.name.clone();
        };
        let label = format!("{}：{detail}", self.name);
        if label.chars().count() <= 128 {
            label
        } else {
            label.chars().take(128).collect()
        }
    }
}

/// 「运行中应用」选择器的列表项。
#[derive(Clone, Debug)]
pub struct RunningApp {
    pub name: String,
    pub process: String,
    pub icon: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::SourceApp;

    #[test]
    fn history_label_uses_name_when_detail_missing() {
        let source = SourceApp {
            name: "Editor".into(),
            executable: None,
            detail: None,
        };
        assert_eq!(source.history_label(), "Editor");
    }

    #[test]
    fn history_label_joins_name_and_detail() {
        let source = SourceApp {
            name: "chrome".into(),
            executable: None,
            detail: Some("https://example.com/path".into()),
        };
        assert_eq!(source.history_label(), "chrome：https://example.com/path");
    }

    #[test]
    fn history_label_truncates_to_core_name_limit() {
        let source = SourceApp {
            name: "app".into(),
            executable: None,
            detail: Some("u".repeat(200)),
        };
        let label = source.history_label();
        assert_eq!(label.chars().count(), 128);
        assert!(label.starts_with("app："));
    }
}
