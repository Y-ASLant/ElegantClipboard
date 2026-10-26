pub use super::drag_payload::{PreparedExternalDrag, prepare_external_drag};
use super::x11::X11;
use anyhow::{Result, bail};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{ConnectionExt as _, SK, SO},
        xproto::*,
    },
    wrapper::ConnectionExt as _,
};

pub use crate::drag_types::{DragPointerState, ExternalDragOptions, ExternalDragOutcome};

pub fn poll_drag_pointer() -> Result<DragPointerState> {
    pointer(&X11::connect()?)
}

pub(super) fn pointer(x: &X11) -> Result<DragPointerState> {
    let state = x.connection.query_pointer(x.root)?.reply()?;
    Ok(DragPointerState {
        x: i32::from(state.root_x),
        y: i32::from(state.root_y),
        left_button_down: u16::from(state.mask) & 256 != 0,
    })
}

pub fn drag_escape_pressed() -> bool {
    X11::connect().and_then(|x| escape(&x)).unwrap_or(false)
}

pub(super) fn escape(x: &X11) -> Result<bool> {
    let key = x.keycode(0xff1b)?;
    let keys = x.connection.query_keymap()?.reply()?.keys;
    Ok(keys[usize::from(key / 8)] & (1 << (key % 8)) != 0)
}

pub fn drag_pointer_in_client(
    window: isize,
    pointer: DragPointerState,
    scale_factor: f32,
) -> Result<(f32, f32)> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        bail!("Invalid window scale");
    }
    let x = X11::connect()?;
    let position = x
        .connection
        .translate_coordinates(x.root, window as u32, pointer.x as i16, pointer.y as i16)?
        .reply()?;
    Ok((
        f32::from(position.dst_x) / scale_factor,
        f32::from(position.dst_y) / scale_factor,
    ))
}

pub fn pointer_outside_source(window: isize, pointer: DragPointerState) -> Result<bool> {
    outside(&X11::connect()?, window, pointer)
}

pub(super) fn outside(x: &X11, window: isize, pointer: DragPointerState) -> Result<bool> {
    let geometry = x.connection.get_geometry(window as u32)?.reply()?;
    let position = x
        .connection
        .translate_coordinates(x.root, window as u32, pointer.x as i16, pointer.y as i16)?
        .reply()?;
    Ok(position.dst_x < 0
        || position.dst_y < 0
        || position.dst_x as u16 >= geometry.width
        || position.dst_y as u16 >= geometry.height)
}

pub fn configure_drag_preview(window: isize) -> Result<()> {
    let x = X11::connect()?;
    x.connection.shape_query_version()?.reply()?;
    x.connection
        .shape_rectangles(
            SO::SET,
            SK::INPUT,
            ClipOrdering::UNSORTED,
            window as u32,
            0,
            0,
            &[],
        )?
        .check()?;
    x.connection
        .change_window_attributes(
            window as u32,
            &ChangeWindowAttributesAux::new().override_redirect(1),
        )?
        .check()?;
    x.connection
        .change_property32(
            PropMode::REPLACE,
            window as u32,
            x.atom("_NET_WM_WINDOW_TYPE")?,
            AtomEnum::ATOM,
            &[x.atom("_NET_WM_WINDOW_TYPE_DND")?],
        )?
        .check()?;
    x.connection.flush()?;
    Ok(())
}

pub fn move_drag_preview(window: isize, left: i32, top: i32) -> Result<()> {
    let x = X11::connect()?;
    x.connection
        .configure_window(
            window as u32,
            &ConfigureWindowAux::new()
                .x(left)
                .y(top)
                .stack_mode(StackMode::ABOVE),
        )?
        .check()?;
    x.connection.flush()?;
    Ok(())
}

pub fn show_drag_preview(window: isize) -> Result<()> {
    let x = X11::connect()?;
    x.connection.map_window(window as u32)?.check()?;
    x.connection.flush()?;
    Ok(())
}

pub fn hide_drag_preview(window: isize) -> Result<()> {
    let x = X11::connect()?;
    x.connection.unmap_window(window as u32)?.check()?;
    x.connection.flush()?;
    Ok(())
}

pub fn drag_preview_scale(window: isize) -> Result<f32> {
    let x = X11::connect()?;
    x.connection.get_geometry(window as u32)?.reply()?;
    // GPUI's X11 scale follows Xft.dpi; absent a resource, X11 uses 96 dpi.
    let resource = x
        .connection
        .get_property(
            false,
            x.root,
            x.atom("RESOURCE_MANAGER")?,
            AtomEnum::STRING,
            0,
            262_144,
        )?
        .reply()?;
    let resources = String::from_utf8_lossy(&resource.value);
    Ok(resources
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "Xft.dpi")
                .then(|| value.trim().parse::<f32>().ok())
                .flatten()
        })
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .unwrap_or(96.0)
        / 96.0)
}

pub fn resize_drag_preview(window: isize, width: i32, height: i32) -> Result<()> {
    if width <= 0 || height <= 0 {
        bail!("Invalid drag preview dimensions");
    }
    let x = X11::connect()?;
    x.connection
        .configure_window(
            window as u32,
            &ConfigureWindowAux::new()
                .width(width as u32)
                .height(height as u32),
        )?
        .check()?;
    x.connection.flush()?;
    Ok(())
}

pub fn run_external_drag(
    payload: PreparedExternalDrag,
    options: ExternalDragOptions,
) -> Result<ExternalDragOutcome> {
    super::xdnd::run(payload, options)
}
