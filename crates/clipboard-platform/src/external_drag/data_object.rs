use super::PreparedExternalDrag;
use std::{cell::Cell, sync::Arc};
use windows::{
    Win32::{
        Foundation::{
            DATA_S_SAMEFORMATETC, DV_E_DVASPECT, DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED,
            E_NOTIMPL, E_POINTER, GlobalFree, OLE_E_ADVISENOTSUPPORTED, S_FALSE, S_OK,
        },
        System::{
            Com::{
                DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject,
                IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl, IEnumSTATDATA, STGMEDIUM,
                STGMEDIUM_0, TYMED_HGLOBAL,
            },
            Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
    },
    core::{BOOL, HRESULT, Ref, Result, implement},
};

#[implement(IDataObject)]
struct DragDataObject {
    formats: Arc<Vec<(u16, Vec<u8>)>>,
}

pub(super) fn data_object(payload: PreparedExternalDrag) -> IDataObject {
    DragDataObject {
        formats: payload.formats,
    }
    .into()
}

impl DragDataObject {
    fn lookup(&self, format: *const FORMATETC) -> Result<&[u8]> {
        let format = unsafe { format.as_ref() }.ok_or(E_POINTER)?;
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return Err(DV_E_DVASPECT.into());
        }
        if format.lindex != -1 {
            return Err(DV_E_LINDEX.into());
        }
        if format.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
            return Err(DV_E_TYMED.into());
        }
        self.formats
            .iter()
            .find(|(id, _)| *id == format.cfFormat)
            .map(|(_, bytes)| bytes.as_slice())
            .ok_or_else(|| DV_E_FORMATETC.into())
    }
}

impl IDataObject_Impl for DragDataObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> Result<STGMEDIUM> {
        let bytes = self.lookup(format)?;
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len())?;
            let pointer = GlobalLock(handle);
            if pointer.is_null() {
                let error = windows::core::Error::from_thread();
                let _ = GlobalFree(Some(handle));
                return Err(error);
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
            let _ = GlobalUnlock(handle);
            // Every GetData transfers a fresh allocation to the consumer, which
            // frees it with ReleaseStgMedium independently of this data object.
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: handle },
                pUnkForRelease: Default::default(),
            })
        }
    }
    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(DV_E_TYMED.into())
    }
    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        self.lookup(format)
            .map_or_else(|error| error.code(), |_| S_OK)
    }
    fn GetCanonicalFormatEtc(&self, input: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        if input.is_null() || output.is_null() {
            return E_POINTER;
        }
        unsafe {
            *output = *input;
            (*output).ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }
    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        Ok(FormatEnumerator {
            formats: self.formats.iter().map(|(id, _)| *id).collect(),
            position: Cell::new(0),
        }
        .into())
    }
    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

pub(super) fn format_etc(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

#[implement(IEnumFORMATETC)]
struct FormatEnumerator {
    formats: Vec<u16>,
    position: Cell<usize>,
}

impl IEnumFORMATETC_Impl for FormatEnumerator_Impl {
    fn Next(&self, count: u32, output: *mut FORMATETC, fetched: *mut u32) -> HRESULT {
        if !fetched.is_null() {
            unsafe {
                *fetched = 0;
            }
        }
        if output.is_null() || (count != 1 && fetched.is_null()) {
            return E_POINTER;
        }
        let start = self.position.get();
        let take = (count as usize).min(self.formats.len() - start);
        for (offset, id) in self.formats[start..start + take].iter().enumerate() {
            unsafe {
                output.add(offset).write(format_etc(*id));
            }
        }
        self.position.set(start + take);
        if !fetched.is_null() {
            unsafe {
                *fetched = take as u32;
            }
        }
        if take == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn Skip(&self, count: u32) -> Result<()> {
        let start = self.position.get();
        let take = (count as usize).min(self.formats.len() - start);
        self.position.set(start + take);
        if take == count as usize {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }
    fn Reset(&self) -> Result<()> {
        self.position.set(0);
        Ok(())
    }
    fn Clone(&self) -> Result<IEnumFORMATETC> {
        Ok(FormatEnumerator {
            formats: self.formats.clone(),
            position: Cell::new(self.position.get()),
        }
        .into())
    }
}
