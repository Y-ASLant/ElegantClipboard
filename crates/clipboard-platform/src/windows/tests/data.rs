use super::super::{
    DataSizeInfo, data::data_size_info, file_actions::data_directory_command, restore_backup_data,
};
use super::*;
use std::path::Path;

#[test]
fn data_size_counts_database_and_managed_media() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let images = directory.path().join("images");
    let nested_images = images.join("nested");
    let staged = directory.path().join("staged");
    std::fs::create_dir_all(&nested_images)?;
    std::fs::create_dir_all(&staged)?;
    std::fs::write(directory.path().join("clipboard.db"), [0u8; 3])?;
    std::fs::write(directory.path().join("clipboard.db-wal"), [0u8; 5])?;
    std::fs::write(images.join("one.png"), [0u8; 7])?;
    std::fs::write(nested_images.join("two.png"), [0u8; 11])?;
    std::fs::write(staged.join("payload.bin"), [0u8; 13])?;
    std::fs::write(directory.path().join("unmanaged.txt"), [0u8; 17])?;

    assert_eq!(
        data_size_info(directory.path(), &images, &staged)?,
        DataSizeInfo {
            database_bytes: 8,
            image_bytes: 18,
            image_count: 2,
            staged_bytes: 13,
            staged_count: 1,
            total_bytes: 39,
        }
    );
    let command = data_directory_command(directory.path())?;
    assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
    assert_eq!(command.get_args().collect::<Vec<_>>(), [directory.path()]);
    Ok(())
}

#[test]
fn database_maintenance_preserves_history_and_returns_updated_size() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("clipboard.db");
    let history = History::open(database.clone())?;
    let id = history.capture("keep after maintenance")?.unwrap();
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::OptimizeDatabase)?;
    match events.recv_blocking()? {
        Event::DatabaseOptimized(size) => {
            assert!(size.database_bytes > 0);
            assert_eq!(size.total_bytes, size.database_bytes);
        }
        Event::CommandFailed { message, .. } | Event::Error(message) => panic!("{message}"),
        _ => panic!("unexpected database maintenance event"),
    }
    drop(service);

    assert_eq!(History::open(database)?.text(id)?, "keep after maintenance");
    Ok(())
}

#[test]
fn worker_exports_backup_and_cli_restores_into_isolated_directory() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    let restored = directory.path().join("restored");
    let backup = directory.path().join("history.zip");
    let (service, events) = Service::start(Some(source), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("worker backup".into()))?;
    let original = next_snapshot(&events, 0)[0].id;
    service.send(Command::ExportBackup(backup.clone()))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "backup timed out",
        |event| match event {
            Event::BackupExported(result) => {
                let report = result.unwrap_or_else(|error| panic!("backup failed: {error}"));
                assert_eq!(report.total_items, 1);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    let (database, report) = restore_backup_data(&backup, restored)?;
    assert_eq!(report.total_items, 1);
    assert_eq!(History::open(database)?.text(original)?, "worker backup");
    Ok(())
}
