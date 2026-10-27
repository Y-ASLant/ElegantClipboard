# ElegantClipboard

English | [中文](README.md)

ElegantClipboard is a native clipboard manager built with Rust, GPUI, and gpui-kit. Windows, macOS, and Linux share the UI and data service, with separate native integrations. This branch does not require Node.js, WebView, or Tauri. Platform capabilities and verification limits are described below.

## Current features

- Capture and search text, URLs, HTML/RTF, images, and file paths
- A four-step Chinese/English first-run guide with animated previews for capture, search, favorites and shortcuts; navigate back and forward, then select Get started on the final step
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
- Regular-file history stores original paths without copying file contents; moved or deleted sources are reported as unavailable. Image history still preserves pixels and can be copied as files; HTML is converted when plain text is missing
- Verify clipboard contents after writing, wait a bounded time for modifier release before pasting, validate the target, and honor producer opt-outs from local clipboard history

See [Windows GPUI status](docs/WINDOWS_MVP.md) for Windows implementation details and historical verification. Features above are subject to these platform limits.

## Platform differences

| Platform | Integration and limits |
|---|---|
| Windows | Existing native clipboard formats, hotkeys, tray, OLE drag-and-drop, controlled paste, and DWM materials are retained |
| macOS | Carbon hotkeys, menu-bar tray, LaunchAgent login startup, and AppKit drag-and-drop. Automatic paste uses `Cmd+V` and requires Accessibility permission; `Shift+Insert` is unavailable. Source attribution uses the frontmost app on a best-effort basis, not a reliable clipboard-owner identity. Blur is supported; Windows Mica is not |
| Linux X11 | X11 hotkeys, XTEST paste, XInput2 outside clicks, XDND, and XDG autostart. Source attribution prefers the selection owner's XRes PID and remains unknown when unresolved. The X server must provide the required extensions |
| Linux Wayland | Clipboard capture requires compositor data-control support; an incomplete XWayland fallback is rejected. Global shortcuts, automatic paste, source identification, and exact window/hover-popup positioning are unavailable. Internal sorting and native file/image drag-out remain available; text/rich-text drag-out is unavailable. Polling cannot distinguish repeated copies of identical content |

Linux needs a StatusNotifier/XEmbed tray host (GNOME usually needs an AppIndicator extension); the app avoids hiding into an inaccessible tray when no host is found. Audio uses `paplay` (`pulseaudio-utils`, also compatible with PipeWire Pulse), and opening/revealing files uses `xdg-open` / `dbus-send`. macOS uses built-in `open` / `afplay`.

The new backends have not yet undergone native macOS/Linux desktop acceptance. Windows native-window and isolated clipboard roundtrip checks have run; desktop permissions, interactions, and package artifacts on other systems still require native verification. CI now has native OS jobs; configuration alone is not evidence of a successful CI run.

## Repository layout

```text
crates/
  clipboard-core/      Data model, SQLite, queries, backups, and business rules
  clipboard-platform/  Shared clipboard service and Windows/macOS/Linux integrations
  clipboard-gpui/      GPUI application entry point, state, and UI
scripts/               Packaging and verification helpers
docs/                  Architecture decisions, feature status, and QA evidence
```

Dependencies flow from `clipboard-gpui` to `clipboard-platform` to `clipboard-core`; the UI may also use core directly. Core must not depend on GPUI or Windows APIs. The tray belongs to `clipboard-gpui`, while platform handles OS integration.

Every Rust source, example, and test file is limited to **500 physical lines**, including blank lines and comments. Split by responsibility into private submodules and keep tests alongside their owning module; do not bypass the limit with numbered fragments or `include!`. Both `just check` and GPUI CI run `scripts/check-source-size.ps1`. See the [engineering documentation](docs/README.md) for module responsibilities.

## Run from source

Requirements:

- Windows 10/11 x64 with MSVC; or macOS with Xcode Command Line Tools; or Linux with the dependencies below
- Rust 1.98 or newer with the native toolchain for the current OS
- PowerShell 7 (`pwsh`) for source-size checks and packaging
- [`just`](https://github.com/casey/just) (optional, for the shortcut commands below)

On macOS, install development tools using `xcode-select --install`. On Ubuntu 24.04:

```sh
sudo apt-get update
sudo apt-get install build-essential clang cmake pkg-config libssl-dev libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libasound2-dev libudev-dev mesa-vulkan-drivers xdg-utils dbus-x11 pulseaudio-utils
```

Linux requires a graphical session and a working Vulkan driver; a headless SSH shell alone cannot run the GUI.

```powershell
cargo run -p elegant-clipboard-gpui --locked
```

Or use:

```powershell
just run
```

Default data folders use only the application name:

| Platform | Default data directory |
|---|---|
| Windows | `%LOCALAPPDATA%\ElegantClipboard` |
| macOS | `~/Library/Application Support/ElegantClipboard` |
| Linux | `$XDG_DATA_HOME/elegantclipboard`, defaulting to `~/.local/share/elegantclipboard` |

Upgrades migrate the previous GPUI default directory, including the database, images, icons and native-format blobs. Managed paths and login startup entries are updated before migration cleanup completes. Startup removes legacy `staged` file copies and their database references while retaining original paths and history metadata; importing or restoring an old backup does not recreate these copies. Old directories and empty dedicated parents are removed after success. If both folders contain data, the new folder is used and the old folder is preserved with a warning; histories are never overwritten or silently merged. A running old instance is activated first, and migration waits until it exits. An explicit `--data-dir` pointing to the old default is redirected for compatibility; other custom directories are unchanged.

For an isolated run, specify a directory explicitly:

```powershell
cargo run -p elegant-clipboard-gpui --locked -- --no-monitor --data-dir .\target\gpui-check
```

## Build and verify

```powershell
just check
just test
just build
```

The release executable is `target/release/elegant-clipboard-gpui.exe` on Windows, or `target/release/elegant-clipboard-gpui` on macOS/Linux.

Create an unsigned Windows x64 standalone ZIP:

```powershell
just package
```

Package **on the native macOS or Linux host**:

```sh
just package-unix
# or pwsh -NoProfile -File scripts/package-gpui-unix.ps1
```

macOS creates `ElegantClipboard-v<version>-<arch>-apple-darwin.dmg` containing `ElegantClipboard.app` and an `/Applications` shortcut. Open the DMG, drag the app to Applications, eject the image, then launch it from Applications. The app is only ad-hoc signed, without Developer ID signing or Apple notarization. If macOS blocks the first launch, verify the download source before using **System Settings → Privacy & Security → Open Anyway**; do not disable system-wide security checks. Automatic paste requires Accessibility permission.

Linux creates a `.tar.gz` containing the executable, license, and instructions, without bundling system libraries. Keep the runtime dependencies listed above installed. Filenames include the actual Rust host target; the script accepts native x86_64 and aarch64 macOS/Linux toolchains. macOS CI and release jobs verify and mount the DMG read-only, check the Applications shortcut and app signature, then launch the packaged application with isolated smoke-test data.

Packages and SHA-256 files are written locally to `target/packages/`; CI artifacts and GitHub Releases upload only platform packages, not `.sha256` files. Installation wizards and automatic updates are not provided. CI/release jobs build on Windows, Ubuntu, and macOS runners rather than pretending to generate native Unix packages on Windows.

## Versioning

```powershell
.\scripts\bump-version.ps1 2.0.1
```

The script updates the root Cargo workspace version and lockfile. The GPUI rewrite starts at `2.0.0`; subsequent release tags must match the Cargo version, for example `v2.0.1`. See the [changelog](docs/CHANGELOG.md).

## License

[MIT License](LICENSE)
