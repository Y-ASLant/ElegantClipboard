//! 三平台外部拖放共用的事件与结果类型；坐标语义见各平台实现注释。

/// 拖拽期间轮询到的指针状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragPointerState {
    pub x: i32,
    pub y: i32,
    pub left_button_down: bool,
}

/// 拖放会话参数：源窗口、预览浮窗及其相对光标的偏移。
#[derive(Clone, Copy, Debug)]
pub struct ExternalDragOptions {
    pub source_window: isize,
    pub preview_window: Option<isize>,
    /// Cursor minus preview origin, in physical screen pixels.
    pub preview_offset: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalDragOutcome {
    Dropped,
    Cancelled,
    ReturnedToSource,
    /// OLE cancelled its self-drop; the UI should commit its internal drop.
    ReleasedInSource,
}
