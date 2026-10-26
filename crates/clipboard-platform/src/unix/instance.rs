use crate::Event;
use anyhow::{Context, Result};
use async_channel::Sender;
use std::{
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

fn socket_path(data_dir: &Path) -> Result<PathBuf> {
    let canonical = data_dir.canonicalize().context("无法解析实例数据目录")?;
    use std::os::unix::ffi::OsStrExt;
    let hash = clipboard_core::backup::hash_token(canonical.as_os_str().as_bytes());
    Ok(std::env::temp_dir().join(format!("elegant-clipboard-{}.sock", &hash[..24])))
}

pub fn show_existing(data_dir: &Path) -> Result<bool> {
    if !data_dir.is_dir() {
        return Ok(false);
    }
    let socket = UnixDatagram::unbound().context("无法创建实例唤出连接")?;
    match socket.send_to(b"show", socket_path(data_dir)?) {
        Ok(_) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error).context("无法唤出正在运行的窗口"),
    }
}

pub struct InstanceSignal {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl InstanceSignal {
    // The caller holds the shared data-directory file lock before replacing a stale socket.
    pub fn start(data_dir: &Path, events: Sender<Event>) -> Result<Self> {
        let path = socket_path(data_dir)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("无法清理旧实例连接"),
        }
        let socket = UnixDatagram::bind(&path).context("无法创建实例唤出连接")?;
        let setup = (|| -> Result<_> {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            socket.set_read_timeout(Some(Duration::from_millis(200)))?;
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = stop.clone();
            let worker = thread::Builder::new()
                .name("instance-signal".into())
                .spawn(move || {
                    let mut message = [0u8; 16];
                    let mut pending = false;
                    while !worker_stop.load(Ordering::Acquire) {
                        match socket.recv(&mut message) {
                            Ok(4) if &message[..4] == b"show" => pending = true,
                            Ok(_) => {}
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock
                                        | std::io::ErrorKind::TimedOut
                                        | std::io::ErrorKind::Interrupted
                                ) => {}
                            Err(_) => break,
                        }
                        if pending {
                            match events.try_send(Event::ShowWindow) {
                                Ok(()) => pending = false,
                                Err(async_channel::TrySendError::Closed(_)) => break,
                                Err(async_channel::TrySendError::Full(_)) => {}
                            }
                        }
                    }
                })?;
            Ok((stop, worker))
        })();
        match setup {
            Ok((stop, worker)) => Ok(Self {
                path,
                stop,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = std::fs::remove_file(path);
                Err(error)
            }
        }
    }
}

impl Drop for InstanceSignal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wakeups_survive_backpressure_and_stay_in_their_data_directory() -> Result<()> {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        let (sender, receiver) = async_channel::bounded(1);
        sender.try_send(Event::Status("occupied".into()))?;
        let signal = InstanceSignal::start(first.path(), sender)?;
        assert!(!show_existing(second.path())?);
        assert!(show_existing(first.path())?);
        std::thread::sleep(Duration::from_millis(250));
        assert!(matches!(receiver.try_recv()?, Event::Status(_)));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(event) = receiver.try_recv() {
                assert!(matches!(event, Event::ShowWindow));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "wake request was lost under backpressure"
            );
            thread::sleep(Duration::from_millis(10));
        }
        drop(signal);
        assert!(!show_existing(first.path())?);
        Ok(())
    }
}
