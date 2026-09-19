# ElegantClipboard

English | [中文](README.md)

ElegantClipboard is a native Windows clipboard manager built with Rust, GPUI, and gpui-kit. This branch contains only the GPUI application and does not require Node.js, WebView, or Tauri.

## Current features

- Capture and search text, URLs, HTML/RTF, images, and file paths
- Favorites, pins, groups, drag sorting, batch deletion, and history cleanup
- Text editing, rich text/image/file previews, copy, plain-text copy, and automatic paste
- Global shortcut, system tray, autostart, capture pause, and single-instance activation
- Simplified Chinese and English UI (Chinese by default), light/dark themes, always-on-top mode, and remembered window size
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
.\scripts\bump-version.ps1 0.2.0
```

The script updates the root Cargo workspace version and lockfile. Release tags must match the Cargo version, for example `v0.2.0`.

## License

[MIT License](LICENSE)
