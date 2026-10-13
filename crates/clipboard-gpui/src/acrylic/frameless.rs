//! 无边框子类：让窗口客户区铺满整个窗口矩形，并补回边缘缩放。
//!
//! 去掉 caption 之后 DWM 才会把系统背板铺进客户区，但窗口也失去了系统提供的
//! 非客户区与缩放边框。这里用一个 comctl32 子类补两件事：
//!
//! - `WM_NCCALCSIZE`：把四边都还给窗口矩形，客户区因此等于整个窗口；
//! - `WM_NCHITTEST`：本来就带 `WS_THICKFRAME` 的窗口，在边缘返回缩放码，
//!   把系统不再提供的边缘缩放补回来。
//!
//! 子类只在效果开启时安装；`FRAMELESS` 为假时两条消息都完全放行，窗口外观与
//! 未启用效果时一致。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClientRect, GetWindowLongW, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT,
    HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, NCCALCSIZE_PARAMS, SM_CXPADDEDBORDER,
    SM_CXSIZEFRAME, SM_CYSIZEFRAME, WINDOW_STYLE, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCHITTEST,
    WS_THICKFRAME,
};

const SUBCLASS_ID: usize = 1;

thread_local! {
    /// 效果是否开启。所有窗口共用同一设置，所以一个开关足够；
    /// 关闭时子类完全放行，窗口外观回到未启用效果时的样子。
    static FRAMELESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 安装子类并把开关置为 `enabled`。重复调用是安全的：
/// `SetWindowSubclass` 对同一 (过程, id) 只算替换而非叠加。
pub(super) fn set_frameless(enabled: bool, hwnd: HWND) {
    FRAMELESS.with(|flag| flag.set(enabled));
    if enabled {
        let _ = unsafe { SetWindowSubclass(hwnd, Some(frameless_subclass_proc), SUBCLASS_ID, 0) };
    }
}

/// # Safety
///
/// 这是 `SUBCLASS_ID` 注册的窗口子类过程，只能由 comctl32 子类机制回调，
/// 不得直接调用。
unsafe extern "system" fn frameless_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _ref_data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCALCSIZE => {
                if wparam.0 == 0 {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                }
                let Some(params) = (lparam.0 as *mut NCCALCSIZE_PARAMS).as_mut() else {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                };
                // 先走 GPUI 的处理（它会恢复 top，最大化时补 frame 厚度），
                // 再按需把四边都还给窗口矩形。
                let window_rect = params.rgrc[0];
                let handled = DefSubclassProc(hwnd, msg, wparam, lparam);
                if !FRAMELESS.with(|flag| flag.get()) || IsZoomed(hwnd).as_bool() {
                    return handled;
                }
                params.rgrc[0] = window_rect;
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let hit = DefSubclassProc(hwnd, msg, wparam, lparam);
                if !FRAMELESS.with(|flag| flag.get()) || hit.0 as u32 != HTCLIENT {
                    return hit;
                }
                // 只有本就带 `WS_THICKFRAME` 的窗口才补边缘缩放。悬停预览是
                // `WindowKind::PopUp`（样式 0x0，`is_resizable: false`），若照补会让它
                // 被拖边缩放，与「不可缩放」的设定相悖。
                if IsZoomed(hwnd).as_bool() || !has_thick_frame(hwnd) {
                    return hit;
                }
                match edge_hit(hwnd, lparam) {
                    Some(code) => LRESULT(code as isize),
                    None => hit,
                }
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(frameless_subclass_proc), id);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

/// 窗口是否自带可缩放边框。GPUI 只给可缩放窗口加 `WS_THICKFRAME`，
/// 悬停预览这类 `PopUp` 没有，据此判断要不要接管边缘命中测试。
fn has_thick_frame(hwnd: HWND) -> bool {
    let style = WINDOW_STYLE(unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as _);
    style.0 & WS_THICKFRAME.0 != 0
}

/// 命中测试返回的边缘缩放码；不在边缘上时返回 `None`。
fn edge_hit(hwnd: HWND, lparam: LPARAM) -> Option<u32> {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    // WM_NCHITTEST 的坐标打包在屏幕坐标里，低位 x、高位 y，均为有符号 16 位。
    let mut point = POINT {
        x: (lparam.0 & 0xFFFF) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    };
    let _ = unsafe { ScreenToClient(hwnd, &mut point) };
    let mut rect = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut rect) };
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);

    let frame_x = unsafe { GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) };
    let padding = unsafe { GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) };
    let frame_y = unsafe { GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) };
    let thickness_x = (frame_x + padding).max(1);
    let thickness_y = (frame_y + padding).max(1);

    let left = point.x < thickness_x;
    let right = point.x >= width - thickness_x;
    let top = point.y < thickness_y;
    let bottom = point.y >= height - thickness_y;

    let code = if top && left {
        HTTOPLEFT
    } else if top && right {
        HTTOPRIGHT
    } else if bottom && left {
        HTBOTTOMLEFT
    } else if bottom && right {
        HTBOTTOMRIGHT
    } else if top {
        HTTOP
    } else if bottom {
        HTBOTTOM
    } else if left {
        HTLEFT
    } else if right {
        HTRIGHT
    } else {
        return None;
    };
    Some(code)
}
