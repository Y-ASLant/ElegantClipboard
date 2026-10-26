//! Windows 播放后端：PlaySoundW 直接读取进程内 WAVE 内存；共享定义在 `crate::sound_core`。

use ::windows::{
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCWSTR,
};
use std::sync::OnceLock;

pub use crate::sound_core::{Sound, enabled};

pub fn play(sound: Sound) -> bool {
    static COPY: OnceLock<Vec<u16>> = OnceLock::new();
    static PASTE: OnceLock<Vec<u16>> = OnceLock::new();
    let wave = match sound {
        Sound::Copy => COPY.get_or_init(|| wave_words(sound.tones())),
        Sound::Paste => PASTE.get_or_init(|| wave_words(sound.tones())),
    };
    // PlaySound keeps reading an asynchronous SND_MEMORY buffer until playback ends.
    // OnceLock owns the aligned buffer for the remaining process lifetime.
    unsafe {
        PlaySoundW(
            PCWSTR(wave.as_ptr()),
            None,
            SND_ASYNC | SND_MEMORY | SND_NODEFAULT,
        )
    }
    .as_bool()
}

/// 合成 WAVE 字节并重排为 `u16` 数组：PlaySoundW 按自然对齐读取内存。
fn wave_words(tones: &[(f32, usize)]) -> Vec<u16> {
    let bytes = crate::sound_core::wave_bytes(tones);
    let (pairs, remainder) = bytes.as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect()
}
