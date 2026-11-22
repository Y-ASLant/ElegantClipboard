#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod capabilities;
mod card_radius;
mod design_system;
mod diagnostics;
mod fonts;
mod links_core;
#[cfg(windows)]
#[path = "windows/mod.rs"]
mod native;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "unix/mod.rs"]
mod native;
mod options;
mod popup_geometry;
mod sound_core;
mod state;
mod tray;
mod ui;
mod visual;
mod window_effect;

fn main() -> anyhow::Result<()> {
    let mut options = options::Options::from_env()?;
    let data_dir = clipboard_platform::resolve_data_directory(options.data_dir.as_deref())?;
    options.wait_for_instance = clipboard_platform::launch_env::take_wait_for_instance(&data_dir);
    // Keep Options.data_dir as the resolved path so Service and UI share one root.
    options.data_dir = Some(data_dir.clone());
    let diagnostics = diagnostics::initialize(&data_dir);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        pid = std::process::id(),
        data_dir = %data_dir.display(),
        "application starting"
    );
    let result = run(options, diagnostics);
    match &result {
        Ok(()) => tracing::info!("application shutdown completed"),
        Err(error) => {
            tracing::error!(error = %format_args!("{error:#}"), "application shutdown failed")
        }
    }
    result
}

fn run(
    options: options::Options,
    diagnostics: Option<std::sync::Arc<diagnostics::Diagnostics>>,
) -> anyhow::Result<()> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        ui::run(options, diagnostics)
    }
}
