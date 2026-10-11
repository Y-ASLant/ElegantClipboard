use super::settings_common::choice_tabs;
use super::*;
use gpui_kit::component::switch::Switch;
impl SoundChannel {
    fn ids(self) -> (&'static str, &'static str, &'static str) {
        match self {
            SoundChannel::Copy => (
                "audio-copy-enabled",
                "audio-copy-timing",
                "audio-copy-preview",
            ),
            SoundChannel::Paste => (
                "audio-paste-enabled",
                "audio-paste-timing",
                "audio-paste-preview",
            ),
        }
    }

    fn enabled(self, audio: AudioPreference) -> bool {
        match self {
            SoundChannel::Copy => audio.copy_enabled,
            SoundChannel::Paste => audio.paste_enabled,
        }
    }

    fn timing(self, audio: AudioPreference) -> SoundTiming {
        match self {
            SoundChannel::Copy => audio.copy_timing,
            SoundChannel::Paste => audio.paste_timing,
        }
    }

    fn set_timing(self, audio: &mut AudioPreference, timing: SoundTiming) {
        match self {
            SoundChannel::Copy => audio.copy_timing = timing,
            SoundChannel::Paste => audio.paste_timing = timing,
        }
    }

    fn set_enabled(self, audio: &mut AudioPreference, enabled: bool) {
        match self {
            SoundChannel::Copy => audio.copy_enabled = enabled,
            SoundChannel::Paste => audio.paste_enabled = enabled,
        }
    }

    fn sound(self) -> Sound {
        match self {
            SoundChannel::Copy => Sound::Copy,
            SoundChannel::Paste => Sound::Paste,
        }
    }
}

impl SettingsWindowView {
    pub(super) fn settings_audio_content(
        &self,
        channel: SoundChannel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let audio = owner_state.audio;
        let pending = owner_state.audio_pending;
        let (switch_id, timing_id, preview_id) = channel.ids();
        let enabled = channel.enabled(audio);
        let timing = channel.timing(audio);
        let toggle_owner = self.owner.clone();
        let preview_owner = self.owner.clone();
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().child(tr(language, "播放时机", "Timing")))
            .child(div().flex_1().min_w(px(0.)).child(choice_tabs(
                timing_id,
                &self.owner,
                [SoundTiming::Immediate, SoundTiming::AfterSuccess],
                [
                    tr(language, "立即", "Immediate"),
                    tr(language, "成功后", "After success"),
                ],
                timing,
                pending || !enabled,
                move |owner: &ClipboardView| channel.timing(owner.audio),
                move |owner, timing, cx| {
                    let mut next = owner.audio;
                    channel.set_timing(&mut next, timing);
                    owner.save_audio(next, cx);
                },
            )))
            .child(
                Button::new(preview_id)
                    .outline()
                    .small()
                    .label(tr(language, "试听", "Preview"))
                    .on_click(move |_, _, cx| {
                        let played = sound::play(channel.sound());
                        if !played {
                            let _ = preview_owner.update(cx, |owner, cx| {
                                owner.message = tr(
                                    owner.language,
                                    "音频设备不可用，无法试听",
                                    "Audio device unavailable; preview could not play",
                                )
                                .into();
                                owner.is_error = true;
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                Switch::new(switch_id)
                    .label(tr(language, "启用", "Enable"))
                    .checked(enabled)
                    .disabled(pending)
                    .on_change(move |checked, _, cx| {
                        let _ = toggle_owner.update(cx, |owner, cx| {
                            let mut next = owner.audio;
                            channel.set_enabled(&mut next, *checked);
                            owner.save_audio(next, cx);
                        });
                    }),
            )
            .into_any_element()
    }
}
