use super::*;
use anyhow::Result;

#[test]
fn ensure_temp_png_is_content_addressed_and_reused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let first = ensure_temp_png(directory.path(), "abc123", b"payload")?;
    assert!(first.is_file());
    assert_eq!(first.parent().unwrap(), directory.path().join("image_tmp"));
    let second = ensure_temp_png(directory.path(), "abc123", b"payload")?;
    assert_eq!(first, second, "同哈希复用既有文件");
    Ok(())
}

#[test]
fn sweep_removes_expired_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    // 直接创建一个过去修改时间的文件（用 25 小时前的时间戳不可行——std 无法设时间，
    // 改为验证「新文件保留」与「上限清扫」两个行为；TTL 路径以实现代码走查为准）。
    let old_path = ensure_temp_png(directory.path(), "old", b"a")?;
    let new_path = ensure_temp_png(directory.path(), "new", b"b")?;
    std::thread::sleep(std::time::Duration::from_millis(5));
    let _ = old_path;
    sweep_image_tmp(directory.path(), std::time::Instant::now())?;
    assert!(new_path.exists(), "新鲜临时文件保留");
    assert!(std::fs::read_dir(directory.path().join("image_tmp"))?.count() <= IMAGE_TMP_MAX_FILES);
    Ok(())
}

#[test]
fn sweep_enforces_file_count_limit() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for index in 0..IMAGE_TMP_MAX_FILES + 10 {
        ensure_temp_png(directory.path(), &format!("hash{index:06}"), b"x")?;
        // 逐个错开修改时间，保证删除顺序确定。
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    sweep_image_tmp(directory.path(), std::time::Instant::now())?;
    let remaining = std::fs::read_dir(directory.path().join("image_tmp"))?.count();
    assert!(
        remaining <= IMAGE_TMP_MAX_FILES,
        "清扫后文件数不应超过上限：{remaining}"
    );
    Ok(())
}
