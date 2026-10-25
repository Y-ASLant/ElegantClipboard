use super::*;
use anyhow::Result;

#[test]
fn pending_commit_roundtrip_preserves_bytes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = BlobStore::new(directory.path().join("blobs"));
    let payload: Vec<u8> = (0..100_000u32).map(|value| value as u8).collect();
    let (hash, pending) = store.write_pending(&payload)?;
    assert!(!store.path(&hash).exists());
    assert!(pending.is_file());
    store.commit(&hash, &pending)?;
    assert!(!pending.exists());
    assert_eq!(store.read(&hash)?, payload);
    assert_eq!(
        store.collect_garbage(&HashSet::from([hash.clone()]))?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn commit_is_idempotent_when_target_exists() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = BlobStore::new(directory.path().join("blobs"));
    let payload = b"same bytes".to_vec();
    let (hash, pending) = store.write_pending(&payload)?;
    store.commit(&hash, &pending)?;
    // 第二次写入同一内容：pending 被丢弃，原文件保留。
    let (_, pending) = store.write_pending(&payload)?;
    store.commit(&hash, &pending)?;
    assert_eq!(store.read(&hash)?, payload);
    Ok(())
}

#[test]
fn garbage_collection_keeps_referenced_and_removes_unreferenced() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let store = BlobStore::new(directory.path().join("blobs"));
    let (kept, _) = store.write_pending(b"kept")?;
    store.commit(&kept, &store.root().join(format!(".pending-{kept}.tmp")))?;
    let (dropped, _) = store.write_pending(b"dropped")?;
    store.commit(
        &dropped,
        &store.root().join(format!(".pending-{dropped}.tmp")),
    )?;
    let removed = store.collect_garbage(&HashSet::from([kept.clone()]))?;
    assert_eq!(removed, vec![dropped.clone()]);
    assert!(store.path(&kept).is_file());
    assert!(!store.path(&dropped).exists());
    // 未知文件名与 pending 残留不被触碰。
    fs::write(directory.path().join("blobs").join("readme.txt"), b"x")?;
    store.sweep_pending()?;
    assert!(directory.path().join("blobs").join("readme.txt").is_file());
    Ok(())
}

#[test]
fn sweep_pending_removes_leftovers_from_previous_runs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("blobs");
    fs::create_dir_all(&root)?;
    fs::write(root.join(".pending-abc123.tmp"), b"leftover")?;
    fs::write(root.join(".tmpXYZ"), b"stale temp")?;
    let store = BlobStore::new(root.clone());
    store.sweep_pending()?;
    assert!(fs::read_dir(&root)?.next().is_none());
    Ok(())
}

#[test]
fn read_reports_missing_blob() {
    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    assert!(store.read(&"0".repeat(64)).is_err());
}
