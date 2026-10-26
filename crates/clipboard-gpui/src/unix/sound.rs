//! Unix 播放后端：缓存 WAVE 文件并调用 afplay（macOS）或 paplay（Linux）；共享定义在 `crate::sound_core`。

use std::{path::PathBuf, sync::LazyLock};

pub use crate::sound_core::{Sound, enabled};

pub fn play(sound: Sound) -> bool {
    static COPY: LazyLock<Option<PathBuf>> =
        LazyLock::new(|| cache_sound("copy", Sound::Copy.tones()));
    static PASTE: LazyLock<Option<PathBuf>> =
        LazyLock::new(|| cache_sound("paste", Sound::Paste.tones()));
    let path = match sound {
        Sound::Copy => &*COPY,
        Sound::Paste => &*PASTE,
    };
    let Some(path) = path else {
        return false;
    };
    let player = if cfg!(target_os = "macos") {
        "afplay"
    } else {
        "paplay"
    };
    match std::process::Command::new(player).arg(path).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

fn cache_sound(name: &str, tones: &[(f32, usize)]) -> Option<PathBuf> {
    let directories = directories::ProjectDirs::from("com", "aslant", "ElegantClipboard")?;
    let directory = directories.cache_dir().join("sounds");
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("{name}.wav"));
    std::fs::write(&path, crate::sound_core::wave_bytes(tones)).ok()?;
    Some(path)
}
