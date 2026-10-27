//! 三平台快捷键槽位共用的事件与注册警告类型。

/// 快速粘贴槽位触发事件：目标为触发时刻的前台窗口。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteHotkeyEvent {
    pub slot: u8,
    pub favorite: bool,
    pub target: (isize, u32),
}

/// 序列粘贴推进键触发事件（决策 0004）：目标为触发时刻的前台窗口。
/// UI 依据自身序列状态决定推进或忽略。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequenceAdvanceEvent {
    pub target: (isize, u32),
}

/// 槽位注册失败（与逐槽位警告一起汇总报告）。
#[derive(Clone, Debug)]
pub struct PasteRegistrationWarning {
    pub slot: u8,
    pub favorite: bool,
    pub message: String,
}

impl std::fmt::Display for PasteRegistrationWarning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
