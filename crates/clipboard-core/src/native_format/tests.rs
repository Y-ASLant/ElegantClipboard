use super::*;
use crate::History;
use anyhow::Result;

fn capture(name: &str, data: &[u8]) -> NativeFormatCapture {
    NativeFormatCapture {
        name: name.to_owned(),
        data: data.to_vec(),
    }
}

#[test]
fn inline_formats_are_stored_and_read_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host item")?.unwrap();
    let skips = history.attach_native_formats(
        id,
        vec![
            capture("Custom Format", b"payload"),
            capture("Another", b"x"),
        ],
    )?;
    assert!(skips.is_empty());
    let formats = history.native_formats_for_copy(id)?;
    assert_eq!(formats.len(), 2);
    assert!(formats.contains(&("Custom Format".to_owned(), b"payload".to_vec())));
    Ok(())
}

#[test]
fn oversize_and_invalid_formats_are_skipped_with_reason() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host")?.unwrap();
    let skips = history.attach_native_formats(
        id,
        vec![
            capture("Big", &vec![0u8; MAX_NATIVE_FORMAT_BYTES + 1]),
            capture("", b"no name"),
            capture("Empty", b""),
            capture("ctl\nname", b"x"),
        ],
    )?;
    assert_eq!(skips.len(), 4);
    assert!(skips.iter().all(|skip| skip.reason == "oversize"
        || skip.reason == "invalid-name"
        || skip.reason == "empty"));
    assert!(history.native_formats_for_copy(id)?.is_empty());
    Ok(())
}

#[test]
fn total_cap_stops_accumulation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host")?.unwrap();
    // 每条 ≤8MiB；四个 7MiB 条目累计超过 32MiB 后第五条被拒。
    let chunk = 7 * 1024 * 1024;
    let captures: Vec<NativeFormatCapture> = (0..5)
        .map(|index| capture(&format!("F{index}"), &vec![index as u8; chunk]))
        .collect();
    let skips = history.attach_native_formats(id, captures)?;
    assert_eq!(skips.len(), 1);
    assert_eq!(skips[0].name, "F4");
    assert_eq!(skips[0].reason, "oversize");
    let formats = history.native_formats_for_copy(id)?;
    assert_eq!(formats.len(), 4);
    Ok(())
}

#[test]
fn same_name_replaces_previous_payload() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host")?.unwrap();
    history.attach_native_formats(id, vec![capture("Fmt", b"first")])?;
    history.attach_native_formats(id, vec![capture("Fmt", b"second")])?;
    let formats = history.native_formats_for_copy(id)?;
    assert_eq!(formats, vec![("Fmt".to_owned(), b"second".to_vec())]);
    Ok(())
}

#[test]
fn large_formats_land_in_blob_store_and_gc_after_delete() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let id = history.capture("blob host")?.unwrap();
    let payload = vec![7u8; NATIVE_INLINE_MAX_BYTES * 4];
    history.attach_native_formats(id, vec![capture("Huge", &payload)])?;
    let formats = history.native_formats_for_copy(id)?;
    assert_eq!(formats, vec![("Huge".to_owned(), payload.clone())]);
    // blobs 目录有且只有一个文件。
    let blobs_dir = directory.path().join("blobs");
    let count = std::fs::read_dir(&blobs_dir)?.count();
    assert_eq!(count, 1, "blob 文件应存在");
    // 删除条目后 blob 被回收。
    history.delete_with_media(id, &images)?;
    let count = std::fs::read_dir(&blobs_dir)?.count();
    assert_eq!(count, 0, "删除提交后无引用 blob 应被回收");
    Ok(())
}

#[test]
fn missing_blob_file_degrades_instead_of_failing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host")?.unwrap();
    let payload = vec![7u8; NATIVE_INLINE_MAX_BYTES * 2];
    history.attach_native_formats(id, vec![capture("Blob", &payload)])?;
    // 手动删除 blob 文件模拟损坏。
    for entry in std::fs::read_dir(directory.path().join("blobs"))? {
        std::fs::remove_file(entry?.path())?;
    }
    assert!(history.native_formats_for_copy(id)?.is_empty());
    Ok(())
}

#[test]
fn attach_to_missing_item_fails() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert!(
        history
            .attach_native_formats(999, vec![capture("Fmt", b"x")])
            .is_err()
    );
    Ok(())
}

#[test]
fn batch_delete_and_clear_all_release_blobs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let first = history.capture("one")?.unwrap();
    let second = history.capture("two")?.unwrap();
    let payload = vec![1u8; NATIVE_INLINE_MAX_BYTES * 2];
    history.attach_native_formats(first, vec![capture("F1", &payload)])?;
    // 不同内容的 payload → 两个不同的 blob 文件。
    history.attach_native_formats(second, vec![capture("F2", &vec![2u8; payload.len()])])?;
    let blobs_dir = directory.path().join("blobs");
    assert_eq!(std::fs::read_dir(&blobs_dir)?.count(), 2);
    history.delete_batch_with_media(&[first, second], None, &images)?;
    assert_eq!(
        std::fs::read_dir(&blobs_dir)?.count(),
        0,
        "批量删除后 blob 应全部回收"
    );
    // clear-all 路径。
    let third = history.capture("three")?.unwrap();
    history.attach_native_formats(third, vec![capture("F3", &payload)])?;
    history.clear_all_with_media(&images)?;
    assert_eq!(std::fs::read_dir(&blobs_dir)?.count(), 0);
    Ok(())
}
