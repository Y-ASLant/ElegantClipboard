use super::{
    drag_payload::PreparedExternalDrag,
    drag_selection::Selection,
    external_drag::{self, ExternalDragOptions, ExternalDragOutcome},
    x11::X11,
};
use anyhow::{Result, bail};
use std::time::{Duration, Instant};
use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
    wrapper::ConnectionExt as _,
};

struct Atoms {
    aware: u32,
    enter: u32,
    leave: u32,
    position: u32,
    status: u32,
    drop: u32,
    finished: u32,
    copy: u32,
    selection: u32,
}
impl Atoms {
    fn new(x: &X11) -> Result<Self> {
        Ok(Self {
            aware: x.atom("XdndAware")?,
            enter: x.atom("XdndEnter")?,
            leave: x.atom("XdndLeave")?,
            position: x.atom("XdndPosition")?,
            status: x.atom("XdndStatus")?,
            drop: x.atom("XdndDrop")?,
            finished: x.atom("XdndFinished")?,
            copy: x.atom("XdndActionCopy")?,
            selection: x.atom("XdndSelection")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    window: u32,
    recipient: u32,
    version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DragLocation {
    pointer: external_drag::DragPointerState,
    target: Option<Target>,
    timestamp: u32,
}

#[derive(Default)]
struct ReleaseLatch {
    frozen: Option<(DragLocation, Instant)>,
}

impl ReleaseLatch {
    fn observe(&mut self, location: DragLocation, now: Instant) -> DragLocation {
        if let Some((frozen, _)) = self.frozen {
            return frozen;
        }
        if !location.pointer.left_button_down {
            self.frozen = Some((location, now));
        }
        location
    }
}

fn target(x: &X11, atoms: &Atoms, source: isize, preview: Option<isize>) -> Result<Option<Target>> {
    let mut window = x.connection.query_pointer(x.root)?.reply()?.child;
    let mut found = None;
    while window != 0 {
        if window == source as u32 || preview == Some(window as isize) {
            return Ok(None);
        }
        let aware = x
            .connection
            .get_property(false, window, atoms.aware, AtomEnum::ATOM, 0, 1)?
            .reply()?;
        if let Some(version) = aware
            .value32()
            .and_then(|mut values| values.next())
            .filter(|version| *version >= 3)
        {
            let mut recipient = window;
            if let Some(proxy) = x.property(window, "XdndProxy")?.first().copied()
                && x.property(proxy, "XdndProxy")?.first() == Some(&proxy)
            {
                recipient = proxy;
            }
            found = Some(Target {
                window,
                recipient,
                version: version.min(5),
            });
        }
        window = x.connection.query_pointer(window)?.reply()?.child;
    }
    Ok(found)
}

fn message(x: &X11, target: Target, kind: u32, data: [u32; 5]) -> Result<()> {
    x.connection
        .send_event(
            false,
            target.recipient,
            EventMask::NO_EVENT,
            ClientMessageEvent::new(32, target.window, kind, data),
        )?
        .check()?;
    Ok(())
}

pub(super) fn run(
    payload: PreparedExternalDrag,
    options: ExternalDragOptions,
) -> Result<ExternalDragOutcome> {
    let x = X11::connect_events()?;
    let atoms = Atoms::new(&x)?;
    let mut selection = Selection::new(&x, payload)?;
    let owner = x.connection.generate_id()?;
    x.connection
        .create_window(
            0,
            owner,
            x.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?
        .check()?;
    // The private source window belongs to this connection: selection requests
    // cannot be consumed by GPUI's event loop. Disconnect destroys it on all exits.
    x.connection
        .change_property32(
            PropMode::REPLACE,
            owner,
            x.atom("XdndTypeList")?,
            AtomEnum::ATOM,
            &selection.types,
        )?
        .check()?;
    x.connection
        .change_property32(PropMode::REPLACE, owner, atoms.aware, AtomEnum::ATOM, &[5])?
        .check()?;
    x.connection
        .set_selection_owner(owner, atoms.selection, x11rb::CURRENT_TIME)?
        .check()?;
    if x.connection
        .get_selection_owner(atoms.selection)?
        .reply()?
        .owner
        != owner
    {
        bail!("Cannot acquire the XDND selection");
    }
    x.connection.flush()?;
    let mut current = None;
    let mut accepted = false;
    let mut pending = false;
    let mut release = ReleaseLatch::default();
    let mut dropped = None;
    let mut timestamp = 0;
    let mut last_position = None;
    loop {
        while let Some(event) = x.connection.poll_for_event()? {
            match event {
                Event::PropertyNotify(event) => {
                    timestamp = event.time;
                    selection.property(&x, event)?;
                }
                Event::SelectionRequest(event) if event.selection == atoms.selection => {
                    selection.request(&x, event)?
                }
                Event::SelectionClear(event) if event.selection == atoms.selection => {
                    bail!("Another drag replaced the XDND selection")
                }
                Event::ClientMessage(event) if event.window == owner => {
                    let data = event.data.as_data32();
                    if current.is_some_and(|target: Target| target.window == data[0]) {
                        if event.type_ == atoms.status {
                            accepted = data[1] & 1 != 0 && data[4] == atoms.copy;
                            pending = false;
                        } else if event.type_ == atoms.finished && dropped.is_some() {
                            return Ok(
                                if current.is_some_and(|target| target.version < 5)
                                    || (data[1] & 1 != 0 && data[2] == atoms.copy)
                                {
                                    ExternalDragOutcome::Dropped
                                } else {
                                    ExternalDragOutcome::Cancelled
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(started) = dropped {
            if Instant::now().duration_since(started) > Duration::from_secs(30) {
                bail!("The drop target did not finish receiving the dragged content");
            }
        } else {
            let was_released = release.frozen.is_some();
            let location = match release.frozen {
                Some((frozen, _)) => frozen,
                None => {
                    let pointer = external_drag::pointer(&x)?;
                    let target = target(&x, &atoms, options.source_window, options.preview_window)?;
                    release.observe(
                        DragLocation {
                            pointer,
                            target,
                            timestamp,
                        },
                        Instant::now(),
                    )
                }
            };
            let pointer = location.pointer;
            if external_drag::escape(&x)? {
                if let Some(target) = current {
                    message(&x, target, atoms.leave, [owner, 0, 0, 0, 0])?;
                }
                x.connection.flush()?;
                return Ok(ExternalDragOutcome::Cancelled);
            }
            if !was_released && !external_drag::outside(&x, options.source_window, pointer)? {
                if let Some(target) = current {
                    message(&x, target, atoms.leave, [owner, 0, 0, 0, 0])?;
                }
                x.connection.flush()?;
                return Ok(if pointer.left_button_down {
                    ExternalDragOutcome::ReturnedToSource
                } else {
                    ExternalDragOutcome::ReleasedInSource
                });
            }
            if let Some(preview) = options.preview_window {
                x.connection
                    .configure_window(
                        preview as u32,
                        &ConfigureWindowAux::new()
                            .x(pointer.x - options.preview_offset.0)
                            .y(pointer.y - options.preview_offset.1)
                            .stack_mode(StackMode::ABOVE),
                    )?
                    .check()?;
            }
            let next = location.target;
            if next != current {
                if let Some(target) = current {
                    message(&x, target, atoms.leave, [owner, 0, 0, 0, 0])?;
                }
                current = next;
                accepted = false;
                pending = false;
                last_position = None;
                if let Some(target) = current {
                    let mut data = [
                        owner,
                        (target.version << 24) | u32::from(selection.types.len() > 3),
                        0,
                        0,
                        0,
                    ];
                    for (slot, atom) in data[2..].iter_mut().zip(&selection.types) {
                        *slot = *atom;
                    }
                    message(&x, target, atoms.enter, data)?;
                }
            }
            let position = (pointer.x, pointer.y);
            if !pending
                && last_position != Some(position)
                && let Some(target) = current
            {
                let packed = ((pointer.x as u32 & 0xffff) << 16) | (pointer.y as u32 & 0xffff);
                message(
                    &x,
                    target,
                    atoms.position,
                    [owner, 0, packed, location.timestamp, atoms.copy],
                )?;
                pending = true;
                last_position = Some(position);
            }
            if let Some((_, since)) = release.frozen {
                if pending && since.elapsed() < Duration::from_secs(2) {
                    // Wait for the target's response to the final position.
                } else if accepted
                    && !pending
                    && let Some(target) = current
                {
                    message(&x, target, atoms.drop, [owner, 0, location.timestamp, 0, 0])?;
                    dropped = Some(Instant::now());
                } else {
                    if let Some(target) = current {
                        message(&x, target, atoms.leave, [owner, 0, 0, 0, 0])?;
                    }
                    x.connection.flush()?;
                    return Ok(ExternalDragOutcome::Cancelled);
                }
            }
        }
        x.connection.flush()?;
        std::thread::sleep(Duration::from_millis(8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(window: u32, x: i32, down: bool) -> DragLocation {
        DragLocation {
            pointer: external_drag::DragPointerState {
                x,
                y: 40,
                left_button_down: down,
            },
            target: Some(Target {
                window,
                recipient: window,
                version: 5,
            }),
            timestamp: x as u32,
        }
    }

    #[test]
    fn release_freezes_target_position_time_and_deadline_until_finish() {
        let mut latch = ReleaseLatch::default();
        let now = Instant::now();
        assert_eq!(
            latch
                .observe(location(10, 20, true), now)
                .target
                .unwrap()
                .window,
            10
        );
        assert_eq!(
            latch
                .observe(location(11, 30, true), now)
                .target
                .unwrap()
                .window,
            11
        );
        assert!(latch.frozen.is_none());
        let released = location(11, 35, false);
        assert_eq!(latch.observe(released, now), released);
        // Moving over another target, returning to the source, or pressing again
        // while waiting for XdndStatus cannot change the destination or deadline.
        assert_eq!(
            latch.observe(location(12, 100, false), now + Duration::from_secs(1)),
            released
        );
        let mut over_source = location(0, 1, false);
        over_source.target = None;
        assert_eq!(
            latch.observe(over_source, now + Duration::from_secs(2)),
            released
        );
        assert_eq!(
            latch.observe(location(13, 120, true), now + Duration::from_secs(3)),
            released
        );
        assert_eq!(latch.frozen.unwrap().1, now);
    }
}
