//! Native material support and conservative fallback policy, separate from saved preferences.

use clipboard_core::preferences::WindowEffect;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEffectBackend {
    Opaque,
    LegacyAcrylic,
    Acrylic,
    NativeBlur,
    Mica,
}

impl WindowEffectBackend {
    pub fn effect(self) -> WindowEffect {
        match self {
            Self::Opaque => WindowEffect::Opaque,
            Self::LegacyAcrylic | Self::Acrylic | Self::NativeBlur => WindowEffect::Frosted,
            Self::Mica => WindowEffect::Mica,
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
        *CURRENT.get_or_init(|| Self::from_platform(current_build(), cfg!(target_os = "macos")))
    }

    fn from_platform(build: u32, native_blur: bool) -> Self {
        Self { build, native_blur }
    }

    pub fn supports(self, effect: WindowEffect) -> bool {
        match effect {
            WindowEffect::Opaque => true,
            WindowEffect::Frosted => self.native_blur || self.build >= 17_763,
            WindowEffect::Mica => self.supports_system_backdrop(),
        }
    }

    /// Resolve a saved preference without rewriting it when this machine lacks support.
    pub fn resolve(self, effect: WindowEffect) -> WindowEffectBackend {
        match effect {
            WindowEffect::Opaque => WindowEffectBackend::Opaque,
            WindowEffect::Mica if self.supports_system_backdrop() => WindowEffectBackend::Mica,
            _ if self.native_blur => WindowEffectBackend::NativeBlur,
            _ if self.supports_system_backdrop() => WindowEffectBackend::Acrylic,
            _ if self.build >= 17_763 => WindowEffectBackend::LegacyAcrylic,
            _ => WindowEffectBackend::Opaque,
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

    const EFFECTS: [WindowEffect; 3] = [
        WindowEffect::Opaque,
        WindowEffect::Frosted,
        WindowEffect::Mica,
    ];

    #[test]
    fn windows_build_boundaries_select_supported_backends_and_safe_fallbacks() {
        use WindowEffectBackend::{Acrylic, LegacyAcrylic, Mica, Opaque};

        let cases = [
            (0, [Opaque; 3], [true, false, false], false),
            (17_762, [Opaque; 3], [true, false, false], false),
            (
                17_763,
                [Opaque, LegacyAcrylic, LegacyAcrylic],
                [true, true, false],
                false,
            ),
            (
                22_620,
                [Opaque, LegacyAcrylic, LegacyAcrylic],
                [true, true, false],
                false,
            ),
            (22_621, [Opaque, Acrylic, Mica], [true; 3], true),
            (u32::MAX, [Opaque, Acrylic, Mica], [true; 3], true),
        ];
        for (build, backends, supported, system_backdrop) in cases {
            let capabilities = WindowEffectCapabilities::from_platform(build, false);
            assert_eq!(capabilities.supports_system_backdrop(), system_backdrop);
            for ((effect, backend), supported) in EFFECTS.into_iter().zip(backends).zip(supported) {
                assert_eq!(capabilities.resolve(effect), backend, "build {build}");
                assert_eq!(capabilities.supports(effect), supported, "build {build}");
                assert!(capabilities.supports(backend.effect()), "build {build}");
                if supported {
                    assert_eq!(backend.effect(), effect, "build {build}");
                }
            }
        }
    }

    #[test]
    fn macos_uses_native_blur_without_advertising_mica() {
        let capabilities = WindowEffectCapabilities::from_platform(0, true);
        assert!(!capabilities.supports_system_backdrop());
        assert!(capabilities.supports(WindowEffect::Opaque));
        assert!(capabilities.supports(WindowEffect::Frosted));
        assert!(!capabilities.supports(WindowEffect::Mica));
        assert_eq!(
            capabilities.resolve(WindowEffect::Opaque),
            WindowEffectBackend::Opaque
        );
        for effect in [WindowEffect::Frosted, WindowEffect::Mica] {
            let backend = capabilities.resolve(effect);
            assert_eq!(backend, WindowEffectBackend::NativeBlur);
            assert_eq!(backend.effect(), WindowEffect::Frosted);
        }
    }

    #[test]
    fn linux_without_verified_compositor_blur_only_supports_opaque() {
        let capabilities = WindowEffectCapabilities::from_platform(0, false);
        assert!(!capabilities.supports_system_backdrop());
        for effect in EFFECTS {
            assert_eq!(capabilities.resolve(effect), WindowEffectBackend::Opaque);
            assert_eq!(
                capabilities.supports(effect),
                effect == WindowEffect::Opaque
            );
        }
    }
}
