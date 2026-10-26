//! Native material support and conservative fallback policy, separate from saved preferences.

use clipboard_core::preferences::WindowEffect;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEffectBackend {
    Opaque,
    LegacyAcrylic,
    Acrylic,
    Mica,
    MicaAlt,
}

impl WindowEffectBackend {
    pub fn effect(self) -> WindowEffect {
        match self {
            Self::Opaque => WindowEffect::None,
            Self::LegacyAcrylic | Self::Acrylic => WindowEffect::Acrylic,
            Self::Mica => WindowEffect::Mica,
            Self::MicaAlt => WindowEffect::MicaAlt,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WindowEffectCapabilities {
    build: u32,
    native_blur: bool,
}

impl WindowEffectCapabilities {
    /// Probe the actual OS version once, independently of application compatibility manifests.
    pub fn current() -> Self {
        static CURRENT: OnceLock<WindowEffectCapabilities> = OnceLock::new();
        *CURRENT.get_or_init(|| Self {
            native_blur: cfg!(target_os = "macos"),
            ..Self::from_build(current_build())
        })
    }

    fn from_build(build: u32) -> Self {
        Self {
            build,
            native_blur: false,
        }
    }

    pub fn supports(self, effect: WindowEffect) -> bool {
        match effect {
            WindowEffect::None => true,
            WindowEffect::Acrylic => self.native_blur || self.build >= 17_763,
            WindowEffect::Mica | WindowEffect::MicaAlt => self.supports_system_backdrop(),
        }
    }

    /// Resolve a saved preference without rewriting it when this machine lacks support.
    pub fn resolve(self, effect: WindowEffect) -> WindowEffectBackend {
        if self.native_blur && effect != WindowEffect::None {
            return WindowEffectBackend::Acrylic;
        }
        if effect == WindowEffect::None || self.build < 17_763 {
            return WindowEffectBackend::Opaque;
        }
        if !self.supports_system_backdrop() {
            return WindowEffectBackend::LegacyAcrylic;
        }
        match effect {
            WindowEffect::None => WindowEffectBackend::Opaque,
            WindowEffect::Acrylic => WindowEffectBackend::Acrylic,
            WindowEffect::Mica => WindowEffectBackend::Mica,
            WindowEffect::MicaAlt => WindowEffectBackend::MicaAlt,
        }
    }

    /// DWM system backdrops are available starting with Windows 11 build 22621.
    pub fn supports_system_backdrop(self) -> bool {
        self.build >= 22_621
    }
}

#[cfg(windows)]
fn current_build() -> u32 {
    use ::windows::{
        Wdk::System::SystemServices::RtlGetVersion,
        Win32::System::SystemInformation::OSVERSIONINFOW,
    };

    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: RtlGetVersion receives an initialized, correctly sized writable structure.
    let status = unsafe { RtlGetVersion(&mut version) };
    if status.0 >= 0 {
        version.dwBuildNumber
    } else {
        0
    }
}

#[cfg(not(windows))]
fn current_build() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::{WindowEffectBackend, WindowEffectCapabilities};
    use clipboard_core::preferences::WindowEffect;

    const EFFECTS: [WindowEffect; 4] = [
        WindowEffect::None,
        WindowEffect::Acrylic,
        WindowEffect::Mica,
        WindowEffect::MicaAlt,
    ];

    #[test]
    fn build_boundaries_select_supported_backends_and_safe_fallbacks() {
        use WindowEffectBackend::{Acrylic, LegacyAcrylic, Mica, MicaAlt, Opaque};

        let cases = [
            (17_762, [Opaque; 4], [true, false, false, false], false),
            (
                17_763,
                [Opaque, LegacyAcrylic, LegacyAcrylic, LegacyAcrylic],
                [true, true, false, false],
                false,
            ),
            (
                22_620,
                [Opaque, LegacyAcrylic, LegacyAcrylic, LegacyAcrylic],
                [true, true, false, false],
                false,
            ),
            (22_621, [Opaque, Acrylic, Mica, MicaAlt], [true; 4], true),
        ];
        for (build, backends, supported, system_backdrop) in cases {
            let capabilities = WindowEffectCapabilities::from_build(build);
            assert_eq!(capabilities.supports_system_backdrop(), system_backdrop);
            for ((effect, backend), supported) in EFFECTS.into_iter().zip(backends).zip(supported) {
                assert_eq!(capabilities.resolve(effect), backend, "build {build}");
                assert_eq!(capabilities.supports(effect), supported, "build {build}");
                assert!(capabilities.supports(backend.effect()), "build {build}");
            }
        }
    }

    #[test]
    fn unknown_version_only_supports_opaque_material() {
        let capabilities = WindowEffectCapabilities::from_build(0);
        assert!(!capabilities.supports_system_backdrop());
        for effect in EFFECTS {
            assert_eq!(capabilities.resolve(effect), WindowEffectBackend::Opaque);
            assert_eq!(capabilities.supports(effect), effect == WindowEffect::None);
        }
    }

    #[test]
    fn supported_preferences_keep_their_material_including_legacy_acrylic() {
        for build in [0, 17_762, 17_763, 22_620, 22_621, u32::MAX] {
            let capabilities = WindowEffectCapabilities::from_build(build);
            for effect in EFFECTS {
                if capabilities.supports(effect) {
                    assert_eq!(
                        capabilities.resolve(effect).effect(),
                        effect,
                        "build {build}"
                    );
                }
            }
        }
    }
}
