# ElegantClipboard

English | [中文](README.md)

ElegantClipboard is a native clipboard manager built with Rust, GPUI, and gpui-kit. Windows, macOS, and Linux share the UI and data service, with separate native integrations. This branch ships only the GPUI application — no Node.js, WebView, or Tauri.

## Features

- Capture and search text, URLs, HTML/RTF, images, and file paths
- Fidelity paste-back: every other native format (including application-registered ones) is captured subject to a built-in blocklist and written back with the main format; large formats move to content-addressed blob storage (zstd), reclaimed with deletion and retention, and included in backups
- Dedup and merge: identical plain text re-copied as HTML/RTF merges into the existing entry; a repeated copy moves the entry to the top by default, or keeps a separate entry if switched in Data settings; a 300 ms confirmation window filters transient fragments (can be disabled); item notes are editable and searchable
- Rich text can be copied or pasted back to the original window as a single format (plain text/HTML/RTF), with an explicit error when the chosen format is missing; plain text supports transformed copy/paste (trim, remove line breaks, case changes); restoring the original clipboard after paste can be enabled (off by default)
- Sequence paste: multi-select records and paste them into the original window one by one via a global advance key (Ctrl+Shift+G by default); the queue ends when exhausted and is cancelled by leaving the panel or copying/deleting
- HTML entries render rich-text layout (with local images) in a hover preview while the list keeps plain-text summaries; images can be pasted into chat windows as images or into Explorer as files
- A three-step first-run guide in a new data directory: pick a language, inspect and record shortcuts, configure theme and autostart; demo records are isolated from real history; Settings → About reopens the guide
- Settings organized as General, Display, Appearance, Data, App filter, Audio, Shortcuts, and About: per-type capture toggles (at least one stays on), source-app filtering by rules or running apps, and copy/paste sound options
- Groups and entries: favorites, pins, and groups that can be created, renamed, deleted, and reordered; cards sort by dragging either edge, batch-select for deletion or cleanup; card preview lines, time format, character count, size, and source display are all configurable
- Standalone hover previews: a transparent, mouse-passthrough, nonactivating layer placed beside the main window; Ctrl+wheel zooms images or scrolls text and leaving the source card closes it; regular files store only their original paths, reporting moved or deleted sources as unavailable
- Interface: Simplified Chinese/English, light/dark themes, corner-radius options, frosted-glass/mica/opaque background materials (per platform capability), always-on-top, hide-on-outside-click and opening-position options; the borderless main window drags by a grip above the search box and offers a return-to-top button after scrolling; quick-paste shortcuts are off by default and can enable Alt+1–0 for the ten most recent records and Ctrl+Alt+1–3 for the top three favorites
- Data management: backups export as GPUI ZIP and can restore GPUI/legacy backups or import an old database into a separate empty directory; per-group count and age limits protect favorites and pinned items; global shortcut, tray, autostart, capture pause, and single-instance activation

Known limitations: remote http(s) images in HTML previews do not load; editing an HTML entry saves it as plain text; paste-back temporary image files live under `image_tmp/` in the data directory and are swept at startup.

See [cross-platform GPUI status](docs/WINDOWS_MVP.md) for implementation details, dependencies and verification commands; historical verification records live under `docs/evidence/`.

## Platform differences

| Platform | Integration and limits |
|---|---|
| Windows | Native clipboard formats, hotkeys, tray, OLE drag-and-drop, controlled paste, and DWM materials |
| macOS | Carbon hotkeys, menu-bar tray, LaunchAgent login startup, AppKit drag-and-drop; automatic paste uses `Cmd+V` and needs Accessibility permission (`Shift+Insert` unavailable); source attribution is best-effort from the frontmost app; blur supported, Mica not |
| Linux X11 | X11 hotkeys, XTEST paste, XInput2 outside clicks, XDND drag, XDG autostart; source attribution prefers the selection owner's XRes PID and stays unknown when unresolved; requires an X server with the needed extensions |
| Linux Wayland | Needs the compositor's data-control protocol — no XWayland fallback; global shortcuts, automatic paste, source identification, and exact positioning are unavailable; internal sorting plus file/image drag-out remain; repeated copies of identical content cannot be detected as new events |

The Linux tray needs a StatusNotifier/XEmbed host (GNOME usually needs an AppIndicator extension); audio needs `paplay` (`pulseaudio-utils`), and opening/revealing files needs `xdg-open` / `dbus-send`. macOS uses the built-in `open` / `afplay`.

The project compiles for all three platforms, but native acceptance so far covers Windows only (native-window and isolated clipboard roundtrips verified); macOS and Linux desktop interaction, permissions, and packages still need on-host verification.

## Repository layout

```text
crates/
  clipboard-core/      Data model, SQLite, queries, backups, business rules
  clipboard-platform/  Shared clipboard service and per-OS integrations
  clipboard-gpui/      GPUI application entry point, state, and UI
scripts/               Packaging and verification helpers
docs/                  Architecture decisions, feature status, QA evidence
```

Dependency direction: `clipboard-gpui -> clipboard-platform -> clipboard-core`; core never depends on GPUI or OS APIs. Every Rust source, example, and test file is limited to **640 physical lines**, enforced by `just check` and CI. See the [engineering documentation](docs/README.md) for module responsibilities.

## Run from source

Requirements:

- Windows 10/11 x64 with MSVC; or macOS with Xcode Command Line Tools; or Linux with the dependencies below
- Rust 1.98 or newer with the native toolchain
- PowerShell 7 (`pwsh`) for checks and packaging
- [`just`](https://github.com/casey/just) (optional, for the shortcut commands below)

On macOS run `xcode-select --install` first. On Ubuntu 24.04:

```sh
sudo apt-get update
sudo apt-get install build-essential clang cmake pkg-config libssl-dev libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libasound2-dev libudev-dev mesa-vulkan-drivers xdg-utils dbus-x11 pulseaudio-utils
```

Linux needs a graphical session and a working Vulkan driver; a headless SSH shell cannot run the GUI.

```powershell
just run    # or cargo run -p elegant-clipboard-gpui --locked
```

Default data directories:

| Platform | Path |
|---|---|
| Windows | `%LOCALAPPDATA%\ElegantClipboard` |
| macOS | `~/Library/Application Support/ElegantClipboard` |
| Linux | `$XDG_DATA_HOME/elegantclipboard` (default `~/.local/share/elegantclipboard`) |

Old data directories and login entries are not migrated automatically; history from the latest 1.x release can be imported from its database or a legacy ZIP. Startup removes legacy `staged` file copies and their database references while keeping original paths. An explicit `--data-dir` is used as given:

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## Diagnostic logs

Diagnostic logging is off by default and can be enabled in General settings. Logs go to `logs` inside the data directory (one session directory per launch) and record version, process, startup/shutdown, display coordinates and DPI, and window/hover-preview lifecycle.

- `application.log` rotates by size (4 MiB × 3 backups); `panic.log` captures the panic thread, location, and backtrace
- Clipboard text, search terms, image pixels, and group names are not logged; error messages and backtraces may contain local paths — review before sharing
- Forced termination, power loss, and fail-fast errors can bypass the panic hook — a missing `panic.log` does not rule out a crash; keep the affected session directory and attach the matching Windows Event Viewer application-log entry

## Windows administrator launch

Windows alone offers "Run as administrator" in General settings: it relaunches the process elevated via UAC so it can monitor and paste into other elevated apps (some games, terminals); declining UAC keeps running unelevated.

## Build and verify

```powershell
just check
just test
just build
```

The release executable is `target/release/elegant-clipboard-gpui[.exe]`.

Packaging:

| Platform | Command | Output |
|---|---|---|
| Windows | `just package` | x64 ZIP and NSIS installer (requires NSIS 3 on the host) |
| macOS | `just package-unix` | ad-hoc-signed `.dmg` |
| Linux | `just package-unix` | `.tar.gz`, `.deb`, `.rpm` (requires `dpkg-dev` and `rpm`) |

Packaging must run on the matching native host; artifacts land in `target/packages/`. macOS builds are not notarized — if the first launch is blocked, use **System Settings → Privacy & Security → Open Anyway** after verifying the source. Uninstallers do not delete history data. Automatic updates are not provided.

## Versioning

```powershell
.\scripts\bump-version.ps1 2.0.1
```

The script updates the root Cargo workspace version and lockfile; release tags must match the Cargo version, e.g. `v2.0.1`. See the [changelog](docs/CHANGELOG.md).

## License

[MIT License](LICENSE)
