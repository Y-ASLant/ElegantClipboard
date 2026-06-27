/// 窗口 WS_EX_LAYERED 扩展样式操作工具函数。
///
/// 三处使用场景：
/// - window.rs: 切换窗口特效时，有特效→移除，无特效→添加
/// - preview.rs: 预览窗口应用特效前移除
/// - lib.rs: 主窗口启动时添加（防止 Win10 无 DWM 特效时闪烁）
#[cfg(target_os = "windows")]
pub(crate) fn set_ws_ex_layered(hwnd: windows::Win32::Foundation::HWND, enable: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowLongW, SetWindowPos, WS_EX_LAYERED,
    };

    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let has_layered = (ex_style as u32) & WS_EX_LAYERED.0 != 0;

        if enable && !has_layered {
            SetWindowLongW(
                hwnd,
                GWL_EXSTYLE,
                ((ex_style as u32) | WS_EX_LAYERED.0) as i32,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        } else if !enable && has_layered {
            SetWindowLongW(
                hwnd,
                GWL_EXSTYLE,
                ((ex_style as u32) & !WS_EX_LAYERED.0) as i32,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}
