use super::*;
use anyhow::Result;
use clipboard_core::{History, database::ClipboardItem};
use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DV_E_DVASPECT, DV_E_FORMATETC, DV_E_LINDEX,
            DV_E_TYMED, E_POINTER, S_FALSE, S_OK,
        },
        System::{
            Com::{DATADIR_GET, FORMATETC, IDataObject, STGMEDIUM, TYMED_ISTREAM},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT, ReleaseStgMedium},
        },
        UI::Shell::{DragQueryFileW, HDROP},
    },
    core::Interface,
};

struct Medium(STGMEDIUM);
impl Medium {
    fn bytes(&self) -> Vec<u8> {
        unsafe {
            let handle = self.0.u.hGlobal;
            let pointer = GlobalLock(handle);
            assert!(!pointer.is_null());
            let bytes =
                std::slice::from_raw_parts(pointer.cast::<u8>(), GlobalSize(handle)).to_vec();
            let _ = GlobalUnlock(handle);
            bytes
        }
    }
}
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe {
            ReleaseStgMedium(&mut self.0);
        }
    }
}
fn get(object: &IDataObject, format: u16) -> Medium {
    Medium(unsafe { object.GetData(&data_object::format_etc(format)) }.unwrap())
}
fn text_item(text: &str) -> Result<ClipboardItem> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture(text)?.unwrap();
    history.item(id)
}

#[test]
fn data_medium_outlives_object_and_repeated_requests_are_independent() -> Result<()> {
    let item = text_item("文本 😀\r\nsecond line")?;
    let payload = prepare_external_drag(&item)?;
    let object = data_object::data_object(payload);
    let first = get(&object, CF_UNICODETEXT.0);
    let second = get(&object, CF_UNICODETEXT.0);
    assert_ne!(unsafe { first.0.u.hGlobal }, unsafe { second.0.u.hGlobal });
    drop(object);
    let expected: Vec<u8> = item
        .text_content
        .unwrap()
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(&first.bytes()[..expected.len()], expected);
    drop(first); // ReleaseStgMedium must not invalidate the other allocation.
    assert_eq!(&second.bytes()[..expected.len()], expected);
    Ok(())
}

#[test]
fn com_requests_reject_unsupported_aspects_indices_media_and_formats() -> Result<()> {
    let object = data_object::data_object(prepare_external_drag(&text_item("hello")?)?);
    for (field, expected) in [
        (0, DV_E_DVASPECT),
        (1, DV_E_LINDEX),
        (2, DV_E_TYMED),
        (3, DV_E_FORMATETC),
    ] {
        let mut format = data_object::format_etc(CF_UNICODETEXT.0);
        match field {
            0 => format.dwAspect = 2,
            1 => format.lindex = 0,
            2 => format.tymed = TYMED_ISTREAM.0 as u32,
            _ => format.cfFormat = CF_HDROP.0,
        }
        assert_eq!(unsafe { object.QueryGetData(&format) }, expected);
        assert_eq!(
            unsafe { object.GetData(&format) }.err().unwrap().code(),
            expected
        );
    }
    assert_eq!(unsafe { object.QueryGetData(std::ptr::null()) }, E_POINTER);
    Ok(())
}

#[test]
fn enumeration_clones_position_and_preserves_partial_fetch_and_skip_results() -> Result<()> {
    let object = data_object::data_object(PreparedExternalDrag {
        formats: std::sync::Arc::new(vec![
            (CF_UNICODETEXT.0, vec![0, 0].into()),
            (CF_DIB.0, vec![1].into()),
        ]),
    });
    let enumeration = unsafe { object.EnumFormatEtc(DATADIR_GET.0 as u32) }?;
    let mut one = [FORMATETC::default()];
    assert_eq!(unsafe { enumeration.Next(&mut one, None) }, S_OK);
    assert_eq!(one[0].cfFormat, CF_UNICODETEXT.0);
    let clone = unsafe { enumeration.Clone() }?;
    assert_eq!(unsafe { enumeration.Next(&mut one, None) }, S_OK);
    let mut many = [FORMATETC::default(); 3];
    let mut fetched = 99;
    assert_eq!(
        unsafe { clone.Next(&mut many, Some(&mut fetched)) },
        S_FALSE
    );
    assert_eq!(fetched, 1);
    assert_eq!(many[0].cfFormat, CF_DIB.0);
    assert!(many[0].ptd.is_null());
    assert_eq!(unsafe { object.QueryGetData(&many[0]) }, S_OK);
    assert_eq!(unsafe { clone.Next(&mut many, None) }, E_POINTER);
    unsafe {
        clone.Reset()?;
    }
    // The generated wrapper maps S_FALSE to Ok; verify the ABI return directly.
    assert_eq!(unsafe { (clone.vtable().Skip)(clone.as_raw(), 3) }, S_FALSE);
    assert_eq!(unsafe { clone.Next(&mut one, None) }, S_FALSE);
    Ok(())
}

#[test]
fn rich_payload_preserves_utf8_fragment_boundaries_and_rtf_bytes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let fragment = "<b>中文 😀</b>";
    let id = history.capture_rich(
        Some(fragment),
        Some(b"{\\rtf1\\ansi text}"),
        Some("中文 😀"),
        directory.path(),
    )?;
    let item = history.item(id)?;
    let object = data_object::data_object(prepare_external_drag(&item)?);
    let html_id = clipboard_win::register_format("HTML Format").unwrap().get() as u16;
    let html = get(&object, html_id).bytes();
    let text = std::str::from_utf8(&html)?.trim_end_matches('\0');
    let offset = |key: &str| -> usize {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .unwrap()
            .parse()
            .unwrap()
    };
    assert_eq!(
        &html[offset("StartFragment:")..offset("EndFragment:")],
        fragment.as_bytes()
    );
    assert_eq!(offset("EndHTML:"), text.len());
    assert!(text[offset("StartHTML:")..].starts_with("<html>"));
    let rtf_id = clipboard_win::register_format("Rich Text Format")
        .unwrap()
        .get() as u16;
    assert!(
        get(&object, rtf_id)
            .bytes()
            .starts_with(b"{\\rtf1\\ansi text}\0")
    );
    Ok(())
}

#[test]
fn image_supplies_original_file_and_decodable_bitmap_without_clipboard_writes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("图像.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([20, 40, 60, 255])).save(&path)?;
    let mut item = text_item("image fixture")?;
    item.content_type = "image".into();
    item.image_path = Some(path.to_string_lossy().into_owned());
    let object = data_object::data_object(prepare_external_drag(&item)?);
    let medium = get(&object, CF_HDROP.0);
    let hdrop = HDROP(unsafe { medium.0.u.hGlobal }.0);
    assert_eq!(unsafe { DragQueryFileW(hdrop, u32::MAX, None) }, 1);
    let length = unsafe { DragQueryFileW(hdrop, 0, None) } as usize;
    let mut name = vec![0; length + 1];
    unsafe {
        DragQueryFileW(hdrop, 0, Some(&mut name));
    }
    assert_eq!(String::from_utf16(&name[..length])?, path.to_string_lossy());
    let dib = get(&object, CF_DIB.0).bytes();
    let decoder =
        image::codecs::bmp::BmpDecoder::new_without_file_header(std::io::Cursor::new(dib))?;
    let decoded = image::DynamicImage::from_decoder(decoder)?.to_rgba8();
    assert_eq!(decoded.dimensions(), (3, 2));
    assert_eq!(decoded.get_pixel(2, 1).0[..3], [20, 40, 60]);
    std::fs::remove_file(path)?;
    // Successful deferred rendering is cached and each consumer still owns its medium.
    let first = get(&object, CF_DIB.0);
    let second = get(&object, CF_DIB.0);
    assert_ne!(unsafe { first.0.u.hGlobal }, unsafe { second.0.u.hGlobal });
    assert_eq!(first.bytes(), second.bytes());
    assert!(prepare_external_drag(&item).is_err());
    Ok(())
}

#[test]
fn escape_release_and_source_reentry_have_distinct_drag_transitions() {
    let source = source::DragSource {
        rect: RECT {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        },
        has_left: Cell::new(false),
        returned: Rc::new(Cell::new(None)),
        failed: Rc::new(Cell::new(false)),
        last_preview_position: Cell::new(None),
        options: ExternalDragOptions {
            source_window: 0,
            preview_window: None,
            preview_offset: (0, 0),
        },
    };
    let inside = DragPointerState {
        x: 20,
        y: 20,
        left_button_down: true,
    };
    let outside = DragPointerState { x: 100, ..inside };
    assert_eq!(source.continue_at(false, inside), S_OK);
    assert_eq!(source.continue_at(false, outside), S_OK);
    assert_eq!(
        source.continue_at(
            false,
            DragPointerState {
                left_button_down: false,
                ..outside
            }
        ),
        DRAGDROP_S_DROP
    );
    assert_eq!(source.returned.get(), None);
    assert_eq!(source.continue_at(true, inside), DRAGDROP_S_CANCEL);
    assert_eq!(source.returned.get(), None);
    assert_eq!(
        source.continue_at(
            false,
            DragPointerState {
                left_button_down: false,
                ..inside
            }
        ),
        DRAGDROP_S_CANCEL
    );
    assert_eq!(
        source.returned.get(),
        Some(ExternalDragOutcome::ReleasedInSource)
    );
    source.returned.set(None);
    assert_eq!(source.continue_at(false, inside), DRAGDROP_S_CANCEL);
    assert_eq!(
        source.returned.get(),
        Some(ExternalDragOutcome::ReturnedToSource)
    );
    source.returned.set(None);
    source.has_left.set(false);
    assert_eq!(
        source.continue_at(
            false,
            DragPointerState {
                left_button_down: false,
                ..inside
            }
        ),
        DRAGDROP_S_CANCEL
    );
    assert_eq!(
        source.returned.get(),
        Some(ExternalDragOutcome::ReleasedInSource)
    );
}

#[test]
fn worker_preparation_loads_full_rich_data_and_correlates_missing_item_failure() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let fragment = format!("<b>{}</b>", "中文😀".repeat(600));
    let full_text = "中文😀 beyond preview ".repeat(600);
    let id = history.capture_rich(Some(&fragment), None, Some(&full_text), directory.path())?;
    let (service, events) = crate::Service::start(Some(directory.path().to_owned()), false)?;
    loop {
        if let crate::Event::Snapshot { items, .. } = events.recv_blocking()? {
            assert!(items[0].html_content.is_none());
            assert!(items[0].text_content.is_none());
            break;
        }
    }
    for (requested, token) in [(id, 41), (i64::MAX, 42)] {
        service.send(crate::Command::PrepareExternalDrag {
            id: requested,
            token,
        })?;
        let crate::Event::ExternalDragPrepared {
            id: actual,
            token: actual_token,
            result,
        } = events.recv_blocking()?
        else {
            panic!("preparation must send a correlated completion event");
        };
        assert_eq!((actual, actual_token), (requested, token));
        if requested == id {
            let object = data_object::data_object(result.map_err(anyhow::Error::msg)?);
            let expected: Vec<u8> = full_text
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            assert_eq!(
                &get(&object, CF_UNICODETEXT.0).bytes()[..expected.len()],
                expected
            );
            let html_id = clipboard_win::register_format("HTML Format").unwrap().get() as u16;
            assert!(
                get(&object, html_id)
                    .bytes()
                    .windows(fragment.len())
                    .any(|bytes| bytes == fragment.as_bytes())
            );
        } else {
            assert!(result.is_err());
        }
    }
    Ok(())
}

#[test]
fn file_drop_encodes_multiple_paths_and_rejects_partial_missing_batches() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = [
        directory.path().join("first.txt"),
        directory.path().join("目录"),
    ];
    std::fs::write(&paths[0], b"sample")?;
    std::fs::create_dir(&paths[1])?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let names: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let id = history.capture_files(&names, directory.path())?;
    let item = history.item(id)?;
    let object = data_object::data_object(prepare_external_drag(&item)?);
    let medium = get(&object, CF_HDROP.0);
    let hdrop = HDROP(unsafe { medium.0.u.hGlobal }.0);
    assert_eq!(unsafe { DragQueryFileW(hdrop, u32::MAX, None) }, 2);
    for (index, expected) in names.iter().enumerate() {
        let mut name = vec![0; unsafe { DragQueryFileW(hdrop, index as u32, None) } as usize + 1];
        let length = unsafe { DragQueryFileW(hdrop, index as u32, Some(&mut name)) } as usize;
        assert_eq!(&String::from_utf16(&name[..length])?, expected);
    }
    std::fs::remove_file(&paths[0])?;
    assert!(prepare_external_drag(&item).is_err());
    Ok(())
}

#[test]
fn oversized_image_attachment_is_rejected_before_decode() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("oversized.png");
    std::fs::File::create(&path)?.set_len(clipboard_core::MAX_IMAGE_BYTES as u64 + 1)?;
    let mut item = text_item("image fixture")?;
    item.content_type = "image".into();
    item.image_path = Some(path.to_string_lossy().into_owned());
    assert!(prepare_external_drag(&item).is_err());
    Ok(())
}

#[test]
fn deferred_dib_failure_preserves_file_format_and_is_cached() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid.png");
    std::fs::write(&path, b"invalid image")?;
    let mut item = text_item("image fixture")?;
    item.content_type = "image".into();
    item.image_path = Some(path.to_string_lossy().into_owned());
    let object = data_object::data_object(prepare_external_drag(&item)?);
    let dib_format = data_object::format_etc(CF_DIB.0);
    assert_eq!(unsafe { object.QueryGetData(&dib_format) }, S_OK);
    let files = get(&object, CF_HDROP.0);
    assert_eq!(
        unsafe { DragQueryFileW(HDROP(files.0.u.hGlobal.0), u32::MAX, None) },
        1
    );
    let error = unsafe { object.GetData(&dib_format) }.err().unwrap();
    // Replacing a corrupt source must not restart rendering on every consumer request.
    image::RgbImage::new(1, 1).save(&path)?;
    let repeated = unsafe { object.GetData(&dib_format) }.err().unwrap();
    assert_eq!(error.code(), repeated.code());
    assert_eq!(error.message(), repeated.message());
    Ok(())
}

#[test]
fn deferred_dib_rejects_excessive_dimensions_before_pixel_allocation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("oversized.bmp");
    image::RgbImage::new(1, 1).save(&path)?;
    let mut bytes = std::fs::read(&path)?;
    bytes[18..22].copy_from_slice(&5001i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&5000i32.to_le_bytes());
    std::fs::write(&path, bytes)?;
    let mut item = text_item("image fixture")?;
    item.content_type = "image".into();
    item.image_path = Some(path.to_string_lossy().into_owned());
    let object = data_object::data_object(prepare_external_drag(&item)?);
    let error = unsafe { object.GetData(&data_object::format_etc(CF_DIB.0)) }
        .err()
        .unwrap();
    assert!(error.message().contains("2500"));
    Ok(())
}
