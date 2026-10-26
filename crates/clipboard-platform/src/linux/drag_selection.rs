use super::{drag_payload::PreparedExternalDrag, x11::X11};
use anyhow::Result;
use x11rb::{protocol::xproto::*, wrapper::ConnectionExt as _};

const CHUNK: usize = 64 * 1024;

struct Transfer {
    requestor: u32,
    property: u32,
    format: usize,
    offset: usize,
}

pub(super) struct Selection {
    payload: PreparedExternalDrag,
    pub types: Vec<u32>,
    targets: u32,
    incr: u32,
    transfers: Vec<Transfer>,
}

impl Selection {
    pub fn new(x: &X11, payload: PreparedExternalDrag) -> Result<Self> {
        let types = payload
            .formats
            .iter()
            .map(|(name, _)| x.atom(name))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            payload,
            types,
            targets: x.atom("TARGETS")?,
            incr: x.atom("INCR")?,
            transfers: Vec::new(),
        })
    }

    pub fn request(&mut self, x: &X11, request: SelectionRequestEvent) -> Result<()> {
        let property = if request.property == 0 {
            request.target
        } else {
            request.property
        };
        let mut accepted = 0;
        if request.target == self.targets {
            let mut types = self.types.clone();
            types.push(self.targets);
            x.connection
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &types,
                )?
                .check()?;
            accepted = property;
        } else if let Some(index) = self.types.iter().position(|atom| *atom == request.target) {
            let bytes = &self.payload.formats[index].1;
            if bytes.len() <= CHUNK {
                x.connection
                    .change_property8(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        request.target,
                        bytes,
                    )?
                    .check()?;
            } else {
                x.connection
                    .change_window_attributes(
                        request.requestor,
                        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                    )?
                    .check()?;
                x.connection
                    .change_property32(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        self.incr,
                        &[u32::try_from(bytes.len())?],
                    )?
                    .check()?;
                self.transfers.retain(|transfer| {
                    transfer.requestor != request.requestor || transfer.property != property
                });
                self.transfers.push(Transfer {
                    requestor: request.requestor,
                    property,
                    format: index,
                    offset: 0,
                });
            }
            accepted = property;
        }
        let event = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: accepted,
        };
        x.connection
            .send_event(false, request.requestor, EventMask::NO_EVENT, event)?
            .check()?;
        Ok(())
    }

    pub fn property(&mut self, x: &X11, event: PropertyNotifyEvent) -> Result<()> {
        if event.state != Property::DELETE {
            return Ok(());
        }
        let Some(index) = self.transfers.iter().position(|transfer| {
            transfer.requestor == event.window && transfer.property == event.atom
        }) else {
            return Ok(());
        };
        let transfer = &mut self.transfers[index];
        let bytes = &self.payload.formats[transfer.format].1;
        let end = (transfer.offset + CHUNK).min(bytes.len());
        x.connection
            .change_property8(
                PropMode::REPLACE,
                transfer.requestor,
                transfer.property,
                self.types[transfer.format],
                &bytes[transfer.offset..end],
            )?
            .check()?;
        if transfer.offset == bytes.len() {
            self.transfers.swap_remove(index);
        } else {
            transfer.offset = end;
        }
        Ok(())
    }
}
