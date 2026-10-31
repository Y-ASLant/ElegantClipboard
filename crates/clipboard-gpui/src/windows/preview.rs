//! Preview geometry uses one source-monitor coordinate space, never a popup's stale DPI.
use anyhow::{Context, Result, ensure};
use gpui_kit::{Bounds, DisplayId, Pixels, Window, point, px, size};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_DONOTROUND, DwmSetWindowAttribute,
        },
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromWindow,
        },
    },
    System::Threading::GetCurrentThreadId,
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, HTTRANSPARENT, HWND_TOPMOST, IsWindow, MA_NOACTIVATE,
            NCCALCSIZE_PARAMS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SetWindowLongPtrW, SetWindowPos, WM_MOUSEACTIVATE, WM_NCCALCSIZE, WM_NCDESTROY,
            WM_NCHITTEST, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_POPUP, WS_VISIBLE,
        },
    },
};

pub(crate) struct PreviewGeometry {
    pub owner: Bounds<Pixels>,
    pub work_area: Bounds<Pixels>,
    pub scale_factor: f32,
    pub display_id: DisplayId,
}

pub(crate) fn geometry(window: &Window) -> Result<PreviewGeometry> {
    let hwnd = super::window::window_hwnd(window).context("预览来源窗口句柄不可用")?;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    ensure!(dpi != 0, "无法读取预览来源 DPI");
    let scale = dpi as f32 / 96.;
    let owner = client_rect(hwnd)?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    ensure!(
        unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool(),
        "无法读取预览显示器工作区"
    );
    let normalize = |rect: RECT| {
        Bounds::new(
            point(px(rect.left as f32 / scale), px(rect.top as f32 / scale)),
            size(
                px((rect.right - rect.left) as f32 / scale),
                px((rect.bottom - rect.top) as f32 / scale),
            ),
        )
    };
    Ok(PreviewGeometry {
        owner: normalize(owner),
        work_area: normalize(info.rcWork),
        scale_factor: scale,
        // gpui-pre-windows identifies displays by HMONITOR, not enumeration order.
        display_id: DisplayId::new(monitor.0 as u64),
    })
}

fn client_rect(hwnd: HWND) -> Result<RECT> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect) }?;
    let mut origin = POINT::default();
    ensure!(
        unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool(),
        "无法读取预览客户区原点"
    );
    Ok(RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right,
        bottom: origin.y + rect.bottom,
    })
}

fn checked_window(handle: isize) -> Result<HWND> {
    let hwnd = HWND(handle as *mut _);
    ensure!(unsafe { IsWindow(Some(hwnd)) }.as_bool(), "预览窗口已关闭");
    ensure!(
        unsafe { GetWindowThreadProcessId(hwnd, None) } == unsafe { GetCurrentThreadId() },
        "预览窗口线程不匹配"
    );
    Ok(hwnd)
}

/// All mutations run on the owning UI thread, outside GPUI borrows.
pub(crate) fn configure_overlay(handle: isize) -> Result<()> {
    configure_popup(handle, false)
}

fn configure_popup(handle: isize, interactive: bool) -> Result<()> {
    let hwnd = checked_window(handle)?;
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex =
            (ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        ensure!(
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) == ex,
            "无法设置预览窗口扩展样式"
        );
        // Per-window subclassing never changes main/settings framing.
        ensure!(
            SetWindowSubclass(hwnd, Some(preview_proc), 2, usize::from(interactive)).as_bool(),
            "无法设置预览窗口消息处理"
        );
        let style = (GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_VISIBLE.0 as isize)
            | (WS_POPUP | WS_CLIPSIBLINGS).0 as isize;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        ensure!(
            GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 == style as u32,
            "无法移除预览窗口边框: requested={style:#x}, actual={:#x}",
            GetWindowLongPtrW(hwnd, GWL_STYLE)
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &DWMNCRP_DISABLED as *const _ as *const _,
            size_of_val(&DWMNCRP_DISABLED) as u32,
        )?;
        // Older Windows does not implement rounded-corner preference; no native
        // frame remains there either, so unsupported corner hints are harmless.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &DWMWCP_DONOTROUND as *const _ as *const _,
            size_of_val(&DWMWCP_DONOTROUND) as u32,
        );
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )?;
    }
    Ok(())
}

/// Remove the host frame while retaining menu input and avoiding activation.
pub(crate) fn suppress_activation(handle: isize) -> Result<()> {
    configure_popup(handle, true)
}

/// Set the physical CLIENT rectangle, correcting once after a synchronous DPI change.
pub(crate) fn set_bounds(handle: isize, bounds: Bounds<Pixels>, scale: f32) -> Result<()> {
    ensure!(scale.is_finite() && scale > 0., "预览缩放比例无效");
    let hwnd = checked_window(handle)?;
    let x = (f32::from(bounds.origin.x) * scale).round() as i32;
    let y = (f32::from(bounds.origin.y) * scale).round() as i32;
    let width = (f32::from(bounds.size.width) * scale).round().max(1.) as i32;
    let height = (f32::from(bounds.size.height) * scale).round().max(1.) as i32;
    for _ in 0..2 {
        let client = client_rect(hwnd)?;
        if client.left == x
            && client.top == y
            && client.right - client.left == width
            && client.bottom - client.top == height
        {
            break;
        }
        let mut outer = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut outer) }?;
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x - (client.left - outer.left),
                y - (client.top - outer.top),
                width + (outer.right - outer.left) - (client.right - client.left),
                height + (outer.bottom - outer.top) - (client.bottom - client.top),
                SWP_NOACTIVATE,
            )
        }?;
    }
    Ok(())
}

unsafe extern "system" fn preview_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    interactive: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_NCHITTEST if interactive == 0 => LRESULT(HTTRANSPARENT as isize),
            WM_NCCALCSIZE if w.0 != 0 && l.0 != 0 => {
                let params = l.0 as *mut NCCALCSIZE_PARAMS;
                let rect = std::ptr::addr_of_mut!((*params).rgrc[0]);
                let bounds = rect.read();
                DefSubclassProc(hwnd, msg, w, l);
                rect.write(bounds);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(preview_proc), id);
                DefSubclassProc(hwnd, msg, w, l)
            }
            _ => DefSubclassProc(hwnd, msg, w, l),
        }
    }
}
