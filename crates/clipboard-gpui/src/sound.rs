use clipboard_core::preferences::{AudioPreference, SoundTiming};
use std::sync::OnceLock;
use windows::{
    Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    core::PCWSTR,
};

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
    static COPY: OnceLock<Vec<u16>> = OnceLock::new();
    static PASTE: OnceLock<Vec<u16>> = OnceLock::new();
    let wave = match sound {
        Sound::Copy => COPY.get_or_init(|| wave_image(&[(880., 60), (1100., 60)])),
        Sound::Paste => PASTE.get_or_init(|| wave_image(&[(660., 80)])),
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

fn wave_image(tones: &[(f32, usize)]) -> Vec<u16> {
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
    let (pairs, remainder) = bytes.as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect()
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
            let wave = wave_image(tones);
            let bytes: Vec<u8> = wave.iter().flat_map(|value| value.to_le_bytes()).collect();
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
