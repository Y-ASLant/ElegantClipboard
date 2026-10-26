//! Native outbound OLE drag. Never writes to the Windows clipboard.
mod data_object;
mod dib;
mod payload;
mod source;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, bail};
pub use payload::{PreparedExternalDrag, prepare_external_drag};
use std::{cell::Cell, rc::Rc};
use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, RECT},
    Graphics::{
        Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute},
        Gdi::{ClientToScreen, ScreenToClient},
    },
    System::{
        Ole::{
            DROPEFFECT_COPY, DROPEFFECT_NONE, DoDragDrop, IDropSource, OleInitialize,
            OleUninitialize,
        },
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE, VK_LBUTTON,
        },
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect,
            GetWindowThreadProcessId, HWND_TOPMOST, IsWindow, LWA_ALPHA, SWP_HIDEWINDOW,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetLayeredWindowAttributes,
            SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TRANSPARENT,
        },
    },
};

pub use crate::drag_types::{DragPointerState, ExternalDragOptions, ExternalDragOutcome};

pub fn poll_drag_pointer() -> Result<DragPointerState> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.context("无法读取拖放指针位置")?;
    Ok(DragPointerState {
        x: point.x,
        y: point.y,
        left_button_down: unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0,
    })
}

pub fn drag_escape_pressed() -> bool {
    unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0 }
}

/// Convert physical screen coordinates to GPUI logical client coordinates.
/// Supply the source window's current scale factor (including per-monitor DPI).
pub fn drag_pointer_in_client(
    hwnd: isize,
    pointer: DragPointerState,
    scale_factor: f32,
) -> Result<(f32, f32)> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        bail!("拖放窗口缩放比例无效");
    }
    let mut point = POINT {
        x: pointer.x,
        y: pointer.y,
    };
    if !unsafe { ScreenToClient(HWND(hwnd as *mut _), &mut point) }.as_bool() {
        return Err(windows::core::Error::from_thread().into());
    }
    Ok((point.x as f32 / scale_factor, point.y as f32 / scale_factor))
}

pub fn pointer_outside_source(hwnd: isize, pointer: DragPointerState) -> Result<bool> {
    Ok(!contains(source_rect(hwnd)?, pointer))
}

fn source_rect(hwnd: isize) -> Result<RECT> {
    let mut rect = RECT::default();
    let hwnd = HWND(hwnd as *mut _);
    unsafe { GetClientRect(hwnd, &mut rect) }.context("无法读取拖放源窗口范围")?;
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err(windows::core::Error::from_thread().into());
    }
    Ok(RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right,
        bottom: origin.y + rect.bottom,
    })
}

fn contains(rect: RECT, pointer: DragPointerState) -> bool {
    pointer.x >= rect.left
        && pointer.x < rect.right
        && pointer.y >= rect.top
        && pointer.y < rect.bottom
}

/// Configure a dedicated preview popup to ignore input and activation.
/// The caller owns its lifetime and must close/hide it after dragging.
/// Call every native preview mutation outside all GPUI App/Window/entity borrows:
/// SetWindowPos and style changes can synchronously dispatch window messages.
pub fn configure_drag_preview(hwnd: isize) -> Result<()> {
    validate_window_thread(hwnd)?;
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &DWMNCRP_DISABLED as *const _ as *const std::ffi::c_void,
            size_of_val(&DWMNCRP_DISABLED) as u32,
        )?;
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style
                | (WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT).0
                    as isize,
        );
        // Layered windows remain invisible until their attributes are initialized.
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )?;
    }
    Ok(())
}

/// Move without showing or activating. Coordinates are physical screen pixels.
/// Call on the owning UI thread, outside every GPUI borrow.
pub fn move_drag_preview(hwnd: isize, x: i32, y: i32) -> Result<()> {
    validate_window_thread(hwnd)?;
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE,
        )
    }?;
    Ok(())
}

/// Show an already configured/positioned preview. Call outside all GPUI borrows.
pub fn show_drag_preview(hwnd: isize) -> Result<()> {
    set_preview_visibility(hwnd, true)
}

/// Hide for reuse without destroying the popup. Call outside all GPUI borrows.
pub fn hide_drag_preview(hwnd: isize) -> Result<()> {
    set_preview_visibility(hwnd, false)
}

fn set_preview_visibility(hwnd: isize, visible: bool) -> Result<()> {
    validate_window_thread(hwnd)?;
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE
                | SWP_NOMOVE
                | SWP_NOSIZE
                | if visible {
                    SWP_SHOWWINDOW
                } else {
                    SWP_HIDEWINDOW
                },
        )?;
    }
    Ok(())
}

/// The preview's current monitor scale, after any synchronous DPI change.
pub fn drag_preview_scale(hwnd: isize) -> Result<f32> {
    validate_window_thread(hwnd)?;
    let dpi = unsafe { GetDpiForWindow(HWND(hwnd as *mut _)) };
    if dpi == 0 {
        bail!("无法读取拖放预览 DPI");
    }
    Ok(dpi as f32 / 96.)
}

/// Set CLIENT size in physical pixels without moving/showing, outside GPUI borrows.
/// A GPUI popup can still have a nonclient frame; SetWindowPos expects outer size.
pub fn resize_drag_preview(hwnd: isize, width: i32, height: i32) -> Result<()> {
    validate_window_thread(hwnd)?;
    if width <= 0 || height <= 0 {
        bail!("拖放预览窗口尺寸无效");
    }
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let mut outer = RECT::default();
        let mut client = RECT::default();
        GetWindowRect(hwnd, &mut outer)?;
        GetClientRect(hwnd, &mut client)?;
        let frame_width = (outer.right - outer.left) - (client.right - client.left);
        let frame_height = (outer.bottom - outer.top) - (client.bottom - client.top);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            width + frame_width,
            height + frame_height,
            SWP_NOACTIVATE | SWP_NOMOVE,
        )?;
    }
    Ok(())
}

fn validate_window_thread(hwnd: isize) -> Result<()> {
    let hwnd = HWND(hwnd as *mut _);
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
        || thread_id != unsafe { GetCurrentThreadId() }
        || process_id != unsafe { GetCurrentProcessId() }
    {
        bail!("原生拖放必须在源窗口所属的 UI 线程执行");
    }
    Ok(())
}

struct OleGuard;
impl OleGuard {
    fn initialize() -> Result<Self> {
        unsafe { OleInitialize(None) }.context("初始化 OLE 拖放失败")?;
        Ok(Self)
    }
}
impl Drop for OleGuard {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

/// Blocks in the OLE message loop. Call on the source window's main thread,
/// outside all GPUI entity/App/Window borrows: OLE dispatches nested messages.
/// Native preview motion is performed here. Preparation can run on a worker.
pub fn run_external_drag(
    payload: PreparedExternalDrag,
    options: ExternalDragOptions,
) -> Result<ExternalDragOutcome> {
    validate_window_thread(options.source_window)?;
    let _ole = OleGuard::initialize()?;
    if let Some(preview) = options.preview_window {
        validate_window_thread(preview)?;
    }
    let pointer = poll_drag_pointer()?;
    let rect = source_rect(options.source_window)?;
    if drag_escape_pressed() {
        return Ok(ExternalDragOutcome::Cancelled);
    }
    if !pointer.left_button_down {
        return Ok(if contains(rect, pointer) {
            ExternalDragOutcome::ReleasedInSource
        } else {
            // A release before OLE starts cannot deliver data to another app.
            ExternalDragOutcome::Cancelled
        });
    }
    let returned = Rc::new(Cell::new(None));
    let failed = Rc::new(Cell::new(false));
    let data = data_object::data_object(payload);
    let source: IDropSource = source::DragSource {
        rect,
        has_left: Cell::new(!contains(rect, pointer)),
        returned: returned.clone(),
        failed: failed.clone(),
        last_preview_position: Cell::new(None),
        options,
    }
    .into();
    let source_hwnd = HWND(options.source_window as *mut _);
    let had_capture = unsafe { GetCapture() } == source_hwnd;
    if had_capture {
        unsafe { ReleaseCapture() }?;
    }
    let mut effect = DROPEFFECT_NONE;
    let result = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
    if returned.get() == Some(ExternalDragOutcome::ReturnedToSource)
        && had_capture
        && poll_drag_pointer().is_ok_and(|p| p.left_button_down)
        && unsafe { IsWindow(Some(source_hwnd)) }.as_bool()
    {
        unsafe {
            SetCapture(source_hwnd);
        }
    }
    if failed.get() {
        bail!("拖放预览更新失败");
    }
    if let Some(outcome) = returned.get() {
        return Ok(outcome);
    }
    if result == DRAGDROP_S_DROP && effect.0 & DROPEFFECT_COPY.0 != 0 {
        Ok(ExternalDragOutcome::Dropped)
    } else if result == DRAGDROP_S_CANCEL || result == DRAGDROP_S_DROP {
        Ok(ExternalDragOutcome::Cancelled)
    } else {
        result.ok().context("原生拖放失败")?;
        Ok(ExternalDragOutcome::Cancelled)
    }
}
