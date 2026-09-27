# ElegantClipboard

English | [中文](README.md)

ElegantClipboard is a native Windows clipboard manager built with Rust, GPUI, and gpui-kit. This branch contains only the GPUI application and does not require Node.js, WebView, or Tauri.

## Current features

- Capture and search text, URLs, HTML/RTF, images, and file paths
- A four-step Chinese/English first-run guide for capture, search, favorites, and shortcuts, with Skip and Esc
- Settings navigation for General, Display, Appearance, Data, App filter, Audio, Shortcuts, and About
- General settings can skip confirmation when clearing the current group's history from the tray menu; confirmation remains the default
- Separate copy and controlled-paste sound settings with immediate or success timing and preview buttons
- The About page opens the author's profile, project repository, and issue tracker in the default browser
- Enable or disable history capture for text, URLs, HTML, RTF, images, and files in settings; at least one type stays enabled
- Filter new captures by source app with blocklists, allowlists, manual rules, and a running-app picker; rules match names, processes, or paths and support `*` and `?`
- Favorites, pins, group creation and switching via Select (rename, delete, and Up/Down ordering), sorting by dragging either card edge, batch deletion, and history cleanup; drag area indicators can be hidden in Display settings
- Text editing, rich text/image/file previews, single-image file card thumbnails and missing-source warnings, copy, plain-text copy, and automatic paste
- Configurable hover previews: images on by default, with separate text and file switches, delay, position, image zoom step, zoom buttons and percentage reset, and a larger window based on image dimensions
- Global shortcut to toggle the window, system tray, autostart, capture pause, and single-instance activation
- Quick-paste shortcuts are off by default and can be enabled or disabled in Settings → Shortcuts. Record or type a shortcut for each slot, or disable or restore all ten recent or favorite slots together. When enabled, the preset keys are Alt+1 through Alt+0 for recent items and Ctrl+Alt+1 through 3 for favorites; digit keys support the numpad. Automatic paste can send Ctrl+V or Shift+Insert
- Simplified Chinese and English UI (Chinese by default), light/dark themes, always-on-top mode, hide on outside click, and opening-position, close-after-paste, and move-to-top settings
- Borderless main window: drag the small grip above search to move it (the top edge still resizes); a batch-selection icon beside search, category tabs and the GPUI Kit group Select on one row; Clear History, Pin Window, and Settings in the tray menu
- Reorder tray actions by dragging or Up/Down controls and configure their visibility in Settings (Settings remains available); optional category filters and compact, standard, or spacious history cards
- A return-to-top control in the fixed header for long history lists
- Configurable card preview lines, absolute/relative time, character-count, size, and source-app display by name, icon, or both; source icons are cached as PNG
- GPUI ZIP backup and restore, plus import of an older database or legacy ZIP backup

See [Windows GPUI status](docs/WINDOWS_MVP.md) for implementation details and known limits.

## Repository layout

```text
crates/
  clipboard-core/      Data model, SQLite, queries, backups, and business rules
  clipboard-platform/  Windows clipboard, shortcuts, tray, and OS integration
  clipboard-gpui/      GPUI application entry point, state, and UI
scripts/               Packaging and verification helpers
docs/                  Architecture decisions, feature status, and QA evidence
```

## Run from source

Requirements:

- Windows 10/11 x64
- Rust 1.98 or newer with the MSVC toolchain
- PowerShell 7 (`pwsh`) for packaging

```powershell
cargo run -p elegant-clipboard-gpui --locked
```

Or use:

```powershell
make run
```

By default, data is stored in the `ElegantClipboard-GPUI` application directory under the current user's LocalAppData. For an isolated run, specify a directory explicitly:

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## Build and verify

```powershell
make check
make test
make build
```

The release executable is written to `target\release\elegant-clipboard-gpui.exe`.

Create an unsigned Windows x64 standalone ZIP:

```powershell
make package
```

The archive and its SHA-256 file are written to `target\packages\`. Installers, automatic updates, and ARM64 artifacts are not currently provided.

## Versioning

```powershell
.\scripts\bump-version.ps1 2.0.1
```

The script updates the root Cargo workspace version and lockfile. The GPUI rewrite starts at `2.0.0`; subsequent release tags must match the Cargo version, for example `v2.0.1`. See the [changelog](docs/CHANGELOG.md).

## License

[MIT License](LICENSE)
