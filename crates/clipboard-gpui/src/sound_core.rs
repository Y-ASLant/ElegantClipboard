//! 平台无关的音效定义与 WAVE 合成；播放后端分别在 `windows/` 与 `unix/` 的 `sound.rs`。

use clipboard_core::preferences::{AudioPreference, SoundTiming};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Copy,
    Paste,
}

impl Sound {
    /// 每个音效的正弦音调表 `(频率 Hz, 时长 ms)`。
    pub fn tones(self) -> &'static [(f32, usize)] {
        match self {
            Sound::Copy => &[(880., 60), (1100., 60)],
            Sound::Paste => &[(660., 80)],
        }
    }
}

pub fn enabled(audio: AudioPreference, sound: Sound, timing: SoundTiming) -> bool {
    match sound {
        Sound::Copy => audio.copy_enabled && audio.copy_timing == timing,
        Sound::Paste => audio.paste_enabled && audio.paste_timing == timing,
    }
}

/// 按音调表合成 16 位单声道 22_050Hz PCM 的 RIFF WAVE 文件字节。
pub fn wave_bytes(tones: &[(f32, usize)]) -> Vec<u8> {
    const SAMPLE_RATE: usize = 22_050;
    let sample_count: usize = tones
        .iter()
        .map(|(_, millis)| SAMPLE_RATE * millis / 1000)
        .sum();
    let data_bytes = (sample_count * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    bytes.extend_from_slice(&((SAMPLE_RATE * 2) as u32).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for &(frequency, millis) in tones {
        let count = SAMPLE_RATE * millis / 1000;
        let fade = (SAMPLE_RATE / 200).min(count / 2).max(1);
        for index in 0..count {
            let envelope = (index.min(count - 1 - index) as f32 / fade as f32).min(1.);
            let phase = std::f32::consts::TAU * frequency * index as f32 / SAMPLE_RATE as f32;
            let sample = (phase.sin() * envelope * 0.15 * i16::MAX as f32) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_sounds_have_consistent_pcm_wave_headers() {
        for (tones, expected_samples) in [
            (&[(880., 60), (1100., 60)][..], 22_050 * 120 / 1000),
            (&[(660., 80)][..], 22_050 * 80 / 1000),
        ] {
            let bytes = wave_bytes(tones);
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            assert_eq!(&bytes[36..40], b"data");
            assert_eq!(
                u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
                (expected_samples * 2) as u32
            );
            assert_eq!(bytes.len(), 44 + expected_samples * 2);
            assert!(bytes[44..].iter().any(|value| *value != 0));
        }
    }

    #[test]
    fn playback_respects_each_sound_and_timing() {
        let audio = AudioPreference {
            copy_enabled: true,
            copy_timing: SoundTiming::AfterSuccess,
            paste_enabled: true,
            paste_timing: SoundTiming::Immediate,
        };
        assert!(enabled(audio, Sound::Copy, SoundTiming::AfterSuccess));
        assert!(!enabled(audio, Sound::Copy, SoundTiming::Immediate));
        assert!(enabled(audio, Sound::Paste, SoundTiming::Immediate));
        assert!(!enabled(audio, Sound::Paste, SoundTiming::AfterSuccess));
        assert!(!enabled(
            AudioPreference::default(),
            Sound::Copy,
            SoundTiming::Immediate
        ));
    }
}
