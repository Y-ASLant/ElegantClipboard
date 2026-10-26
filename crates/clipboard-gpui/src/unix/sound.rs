use clipboard_core::preferences::{AudioPreference, SoundTiming};
use std::{path::PathBuf, sync::LazyLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Copy,
    Paste,
}

pub fn enabled(audio: AudioPreference, sound: Sound, timing: SoundTiming) -> bool {
    match sound {
        Sound::Copy => audio.copy_enabled && audio.copy_timing == timing,
        Sound::Paste => audio.paste_enabled && audio.paste_timing == timing,
    }
}

pub fn play(sound: Sound) -> bool {
    static COPY: LazyLock<Option<PathBuf>> =
        LazyLock::new(|| cache_sound("copy", &[(880., 60), (1100., 60)]));
    static PASTE: LazyLock<Option<PathBuf>> = LazyLock::new(|| cache_sound("paste", &[(660., 80)]));
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
    const RATE: usize = 22_050;
    let samples: usize = tones.iter().map(|(_, millis)| RATE * millis / 1000).sum();
    let data_bytes = (samples * 2) as u32;
    let mut wave = Vec::with_capacity(44 + data_bytes as usize);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&(RATE as u32).to_le_bytes());
    wave.extend_from_slice(&((RATE * 2) as u32).to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&data_bytes.to_le_bytes());
    for &(frequency, millis) in tones {
        let count = RATE * millis / 1000;
        let fade = (RATE / 200).min(count / 2).max(1);
        for index in 0..count {
            let envelope = (index.min(count - 1 - index) as f32 / fade as f32).min(1.);
            let phase = std::f32::consts::TAU * frequency * index as f32 / RATE as f32;
            let sample = (phase.sin() * envelope * 0.15 * i16::MAX as f32) as i16;
            wave.extend_from_slice(&sample.to_le_bytes());
        }
    }
    std::fs::write(&path, wave).ok()?;
    Some(path)
}
