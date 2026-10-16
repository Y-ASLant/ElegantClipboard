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
- Favorites, pins, group switching via Select with creation and renaming in a GPUI Kit Dialog (plus deletion and Up/Down ordering), sorting by dragging either card edge, batch deletion, and history cleanup; drag area indicators can be hidden in Display settings
- Text editing, rich text/image/file previews, single-image file card thumbnails and missing-source warnings, copy, plain-text copy, and automatic paste
- Configurable hover previews: images, text, URLs and HTML/RTF are on by default; file previews can be enabled separately. Text popups load the full content after a delay and are scrollable. Configure delay, position, image zoom and larger image windows; explicitly saved disabled settings remain unchanged. Image popups support continuous wheel zoom and left-button drag-to-pan, the whole popup can be dragged by its title, and it closes itself five seconds after appearing
- Global shortcut to toggle the window, system tray, autostart, capture pause, and single-instance activation
- Quick-paste shortcuts are off by default and can be enabled or disabled in Settings → Shortcuts. Record or type a shortcut for each slot, or disable or restore all ten recent or favorite slots together. When enabled, the preset keys are Alt+1 through Alt+0 for recent items and Ctrl+Alt+1 through 3 for favorites; digit keys support the numpad. Automatic paste can send Ctrl+V or Shift+Insert
- Simplified Chinese and English UI (Chinese by default), light/dark themes, always-on-top mode, hide on outside click, and opening-position, close-after-paste, and move-to-top settings
- System-capable window effects for the main window, settings window, and hover preview: off, Acrylic, Mica, or Mica Alt. Unsupported saved choices are retained with an explicit fallback notice. The 40%/55%/72%/85% background tint only repaints the background without reapplying native materials. Controls, cards, and notifications retain their theme surfaces; enabled effects keep a borderless frame with resizable edges
- Borderless main window: drag the small grip above search to move it (the top edge still resizes); search occupies its own row, with category tabs and the GPUI Kit group Select on the next row. Enter batch selection from a card or Ctrl+A and exit from the batch toolbar; Clear History, Pin Window, and Settings remain in the tray menu
- Optional category filters and compact, standard, or spacious history cards. Two-line text previews are shorter than image cards by default; text or a centered thumbnail sits above time, size, and source app, while hovering reveals card actions
- A floating return-to-top button at the lower right of long lists; the item count stays in a fixed footer below the scrolling history
- Static list images use background thumbnails sized for the card and DPI, with bounded concurrency and a viewport cache; full previews, copying and export retain the originals
- Configurable card preview lines, absolute/relative time, character-count, size, and source-app display by name, icon, or both; source icons are cached as PNG
- Data settings export GPUI backups and restore GPUI/legacy ZIPs or older databases into a separate empty folder, with a result report and an action to open the restored data
- Combine group, category and keyword filters; record or type the main shortcut, with conflict rejection and rollback on save failure
- Per-group count and age limits protect pinned and favorite items; first activation and tighter policies require confirmation, without expanding cleanup automatically on upgrade
- Optional regular-file copies are off by default (50 MiB per file, 256 MiB total); copy images as files and convert HTML when plain text is missing
- Verify clipboard contents after writing, wait a bounded time for modifier release before pasting, validate the target, and honor producer opt-outs from local clipboard history

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

Dependencies flow from `clipboard-gpui` to `clipboard-platform` to `clipboard-core`; the UI may also use core directly. Core must not depend on GPUI or Windows APIs. The tray belongs to `clipboard-gpui`, while platform handles OS integration.

Every Rust source, example, and test file is limited to **500 physical lines**, including blank lines and comments. Split by responsibility into private submodules and keep tests alongside their owning module; do not bypass the limit with numbered fragments or `include!`. Both `just check` and GPUI CI run `scripts/check-source-size.ps1`. See the [engineering documentation](docs/README.md) for module responsibilities.

## Run from source

Requirements:

- Windows 10/11 x64
- Rust 1.98 or newer with the MSVC toolchain
- PowerShell 7 (`pwsh`) for packaging
- [`just`](https://github.com/casey/just) (optional, for the shortcut commands below)

```powershell
cargo run -p elegant-clipboard-gpui --locked
```

Or use:

```powershell
just run
```

By default, data is stored in the `ElegantClipboard-GPUI` application directory under the current user's LocalAppData. For an isolated run, specify a directory explicitly:

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## Build and verify

```powershell
just check
just test
just build
```

The release executable is written to `target\release\elegant-clipboard-gpui.exe`.

Create an unsigned Windows x64 standalone ZIP:

```powershell
just package
```

The archive and its SHA-256 file are written to `target\packages\`. Installers, automatic updates, and ARM64 artifacts are not currently provided.

## Versioning

```powershell
.\scripts\bump-version.ps1 2.0.1
```

The script updates the root Cargo workspace version and lockfile. The GPUI rewrite starts at `2.0.0`; subsequent release tags must match the Cargo version, for example `v2.0.1`. See the [changelog](docs/CHANGELOG.md).

## License

[MIT License](LICENSE)
