//! Native outbound OLE drag. Never writes to the Windows clipboard.
mod data_object;
mod payload;
mod source;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, bail};
pub use payload::{PreparedExternalDrag, prepare_external_drag};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, RECT},
    Graphics::{
        Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute},
        Gdi::ClientToScreen,
    },
    System::{
        Ole::{
            DROPEFFECT_COPY, DROPEFFECT_NONE, DoDragDrop, IDropSource, OleInitialize,
            OleUninitialize,
        },
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_LBUTTON,
        },
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowThreadProcessId,
            HWND_TOPMOST, IsWindow, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        },
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragPointerState {
    pub x: i32,
    pub y: i32,
    pub left_button_down: bool,
}

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
}

pub fn poll_drag_pointer() -> Result<DragPointerState> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.context("无法读取拖放指针位置")?;
    Ok(DragPointerState {
        x: point.x,
        y: point.y,
        left_button_down: unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0,
    })
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

/// Move a dedicated preview using physical screen coordinates, without activation.
pub fn move_drag_preview(hwnd: isize, x: i32, y: i32) -> Result<()> {
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )
    }?;
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
/// The optional callback must also avoid those borrows; native preview motion
/// is performed here before it is called. Preparation can run on a worker.
pub fn run_external_drag(
    payload: PreparedExternalDrag,
    options: ExternalDragOptions,
    on_move: impl FnMut(DragPointerState) + 'static,
) -> Result<ExternalDragOutcome> {
    validate_window_thread(options.source_window)?;
    let _ole = OleGuard::initialize()?;
    if let Some(preview) = options.preview_window {
        configure_drag_preview(preview)?;
    }
    let pointer = poll_drag_pointer()?;
    if !pointer.left_button_down {
        return Ok(ExternalDragOutcome::Cancelled);
    }
    let rect = source_rect(options.source_window)?;
    let returned = Rc::new(Cell::new(false));
    let failed = Rc::new(Cell::new(false));
    let data = data_object::data_object(payload);
    let source: IDropSource = source::DragSource {
        rect,
        has_left: Cell::new(!contains(rect, pointer)),
        returned: returned.clone(),
        failed: failed.clone(),
        options,
        on_move: RefCell::new(Box::new(on_move)),
    }
    .into();
    let source_hwnd = HWND(options.source_window as *mut _);
    let had_capture = unsafe { GetCapture() } == source_hwnd;
    if had_capture {
        unsafe { ReleaseCapture() }?;
    }
    let mut effect = DROPEFFECT_NONE;
    let result = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
    if returned.get()
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
    if returned.get() {
        return Ok(ExternalDragOutcome::ReturnedToSource);
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
