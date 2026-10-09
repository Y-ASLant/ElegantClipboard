# ElegantClipboard

English | [中文](README.md)

## Versions and Branches

- **`main` branch**: The existing **1.x version**, built with Tauri + React.
- **[`gpui` branch](https://github.com/Y-ASLant/ElegantClipboard/tree/gpui)**: A **complete 2.0.0 rewrite built with Rust + GPUI**, developed separately from the existing Tauri version.

The features, screenshots, and build instructions below apply to the `main` branch. For the 2.0.0 implementation and build instructions, see the documentation on the [`gpui` branch](https://github.com/Y-ASLant/ElegantClipboard/tree/gpui).

> Note: UI screenshots in this document may be outdated and were captured on **v0.5.0**.

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="ElegantClipboard" width="128" height="128">
</p>
<p align="center">
  Low footprint · High performance · Modern · Privacy first clipboard.
</p>


<p align="center">
  <a href="https://github.com/Y-ASLant/ElegantClipboard/releases"><img src="https://img.shields.io/github/v/release/Y-ASLant/ElegantClipboard?label=version&color=blue" alt="version"></a>
  <a href="https://github.com/Y-ASLant/ElegantClipboard/releases"><img src="https://img.shields.io/github/downloads/Y-ASLant/ElegantClipboard/total?label=downloads&color=brightgreen" alt="downloads"></a>
  <img src="https://img.shields.io/badge/platform-Windows-lightgrey.svg" alt="platform">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="license">
  <a href="https://github.com/Y-ASLant/ElegantClipboard/actions/workflows/ci.yml"><img src="https://github.com/Y-ASLant/ElegantClipboard/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

## UI Screenshots (v0.5.0)

### Themes

#### System Accent Color

![System Theme](img/theme_0.png)

| Classic B&W | Jade Green | Sky Cyan |
|:-:|:-:|:-:|
| ![Classic](img/theme_1.png) | ![Jade](img/theme_2.png) | ![Sky](img/theme_3.png) |

#### Dark Mode

Automatically follows system dark/light mode, real-time switching

### Settings

| Data Management | Display | Shortcuts |
|:-:|:-:|:-:|
| ![Data](img/setting_1.png) | ![Display](img/setting_2.png) | ![Shortcuts](img/setting_3.png) |

### Hover Image Preview

![Preview](img/preview_mode.png)

### Hover Text Preview

Hover text preview shares the preview position and delay settings with hover image preview (current default: 128ms; both image and text previews are enabled by default).

### Startup Notification

![Notification](img/startup_notification.png)

## Design Philosophy

**Low footprint · High performance · Modern · Privacy first**

- **Low footprint** - Tray resident; the main window does not steal focus by default. Outside-click handling and keyboard navigation depend on window state; clipboard recording continues in the background unless paused
- **High performance** - LIKE substring search (including CJK text), virtual lists, background image-file writes, content hash deduplication
- **Modern** - Tauri 2 + React 19 + Tailwind CSS 4, TypeScript and Rust
- **Privacy first** - History stored locally by default; WebDAV sync and online translation are optional. Update checking is enabled by default and can be disabled
- **Multilingual UI** - Simplified Chinese / English / Traditional Chinese, switch in settings, synced across windows

## Features

See [FEATURES_EN.md](FEATURES_EN.md) for complete feature list and terminology.

File history stores the original copied paths and may include a usable staged fallback. If an original is moved or deleted, fallback is possible only while a staged copy remains available—not every file can be recovered. Visible file and cached-image cards share unknown, checking, available, and unavailable resource states, with clipboard-payload capability checked separately. Missing resources show an invalid marker; an unsupported companion format disables copy/paste without marking an existing physical file missing. Image-load errors only show a preview failure, without marking an existing file unavailable. Preview size limits do not bypass existence checks. Availability does not guarantee actual readability or decodable content: execution also checks the required content. A readable original can still be saved as raw bytes or located in Explorer even if its preview fails or companion format is unsupported. Save As and Show in Explorer resolve original/staged resources from the current entry instead of using an old path cached by a card. See [File Management](FEATURES_EN.md#file-management).

Copy/paste from cards, menus, keyboard actions, global quick/favorite/repeat shortcuts, merges, and translation results shares backend preflight checks. Failed preflight leaves the existing clipboard and main-window visibility unchanged. Only backend success triggers success feedback and post-operation selection/ordering updates. Cancelling Save As produces no success or error message; a later list-refresh failure is reported separately. Operation errors use safe localized reasons, with raw diagnostics logged only. Successful paste means clipboard writing and input simulation succeeded, not that the target application accepted the content. See [Operation Checks and Feedback](FEATURES_EN.md#operation-checks-and-feedback).

## Shortcuts

### Global Shortcuts

| Shortcut | Action |
|----------|--------|
| `Alt+C` | Show/hide window (default, customizable) |
| `Win+V` | Show/hide window (optional, requires enable in settings) |

### In-Window Shortcuts

Up/down navigation, category switching, Enter/Shift+Enter, and Delete require keyboard navigation to be enabled in Settings → Shortcuts (disabled by default). Category switching also requires the category filter bar to be visible.

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Navigate up/down |
| `←` / `→` | Switch content category (All / Text / Other), not custom groups |
| `Enter` | Paste selected item |
| `Shift+Enter` | Paste as plain text |
| `Delete` | Delete selected item |
| `ESC` | Close dialog / hide window |
| `Ctrl+Scroll` | In the main window, zoom hover image preview / scroll hover text preview |

## Tech Stack

| Category | Technology |
|----------|------------|
| **Framework** | Tauri 2 |
| **Frontend** | React 19 + TypeScript |
| **Build** | Vite 8 |
| **Styling** | Tailwind CSS 4 |
| **Components** | shadcn/ui (Radix UI) + Fluent UI Icons |
| **State** | Zustand 5 (settings persisted via the backend database + multi-window event sync) |
| **Virtual List** | react-virtuoso |
| **Drag & Drop** | @dnd-kit |
| **Backend** | Rust |
| **Database** | SQLite (rusqlite) + optimized LIKE (CJK support) |
| **Hash** | BLAKE3 (content deduplication) |
| **Locking** | parking_lot (high-performance Mutex/RwLock) |
| **Parallel** | rayon (parallel file checking) |
| **Clipboard** | Local clipboard-rs 0.3.5 fork (text / HTML / RTF / image / files / watcher) |
| **Window Effects** | window-vibrancy (Mica/Acrylic/Tabbed) |
| **Keyboard Simulation** | Windows SendInput (Ctrl+V / Shift+Insert) |
| **Input Monitoring** | Win32 LL Hook (outside-click detection + visible main-window keyboard handling, separate from background clipboard watching) |
| **Auto Update** | GitHub Release based check & download (system proxy supported) |
| **CI/CD** | GitHub Actions (CI + Tag triggers Release) |
| **Checks & Tests** | ESLint 10 + eslint-plugin-import-x, TypeScript 6, Vitest 4, Playwright |

## Installation

### Download Installer

Download the latest version from [Releases](https://github.com/Y-ASLant/ElegantClipboard/releases):

- **Installer** (recommended): `ElegantClipboard_x.x.x_<arch>-setup.exe`
- **Portable**: `ElegantClipboard_x.x.x_<arch>_portable.exe` (no installation required)

The Release workflow builds x64 and arm64 separately; choose the asset matching your device. The UI requires Microsoft Edge WebView2 Runtime. The installer includes its bootstrapper; portable users need the runtime already installed.

After a WebView2 update, an existing environment can continue using its loaded version. Version differences, channel suffixes, and update events alone do not restart the app or block window creation. Automatic recovery remains limited to actual failures such as process crashes, disconnections, and window readiness timeouts.

### winget

```powershell
winget install Y-ASLant.ElegantClipboard
```

### Scoop

```powershell
scoop bucket add elegantclipboard https://github.com/Y-ASLant/ElegantClipboard
scoop install elegantclipboard
```

### Build from Source

#### Requirements

- Node.js 20.19+ (20.x), 22.13+ (22.x), or 24+; current LTS recommended (subject to dependency engines requirements)
- Current stable Rust (edition 2024) and the matching Windows MSVC toolchain
- Windows 10/11, Microsoft C++ Build Tools, and WebView2 Runtime
- PowerShell; GNU Make is also required to use the `make` commands

Rust's `*-pc-windows-msvc` targets require **MSVC x64/x86 compile/link tools** and a **Windows SDK**. Rust, VS Code, or WebView2 alone is not sufficient. For `link.exe not found`, select both components in Microsoft's Build Tools installer, or run:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --override "--add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --passive --wait --norestart"
```

Installation requires administrator approval. Reopen the terminal afterward. If ordinary PowerShell still lacks the compiler environment, open **x64 Native Tools Command Prompt for VS 2022** from the Start menu, switch to the project directory, and build there. This configures compiler, linker, header, and SDK library paths together; do not merely add an arbitrary `link.exe` to PATH. See Microsoft's [Build Tools component documentation](https://learn.microsoft.com/en-us/visualstudio/install/workload-component-id-vs-build-tools?view=vs-2022).

#### Build Steps

```bash
# Clone repository
git clone https://github.com/Y-ASLant/ElegantClipboard.git
cd ElegantClipboard

# Install dependencies
npm install

# Build frontend only (dist/)
npm run build

# Development mode
npm run tauri dev

# Build production (current machine architecture)
npm run tauri build

# Before cross-architecture builds, install Rust targets and the matching MSVC compile/link tools
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc

# Build x64 / arm64 separately (run twice)
npm run tauri build -- --target x86_64-pc-windows-msvc
npm run tauri build -- --target aarch64-pc-windows-msvc

# Frontend types and zero-warning lint (including tests, configuration, and e2e)
npm run typecheck
npm run lint

# Full quality gate / formatting fixes (Makefile uses PowerShell)
make check
make format

# Frontend, app Rust, and clipboard-fork pure tests (default / no-default features)
make test
# Or run separately:
npm test
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features

# Performance benchmarks / frontend end-to-end tests (run separately)
npm run bench
npx playwright install chromium
npm run test:e2e
```

Notes:
- `npm run build` only runs `tsc && vite build` for frontend assets, no installer generated.
- Installers are generated by `npm run tauri build`; without `--target` it only builds for current architecture.
- To publish both x64 and arm64, run the target-specific build commands twice (or use CI with separate builds).
- `npm run typecheck` checks application, test, build-configuration, and e2e TypeScript; `npm run lint` covers the same scope and rejects warnings with `--max-warnings=0`. The frontend build is not a replacement for full type checking.
- `make check` includes frontend types/ESLint, Rustfmt checks for the app and clipboard fork, app `cargo check --all-targets`, and app/fork `cargo clippy --all-targets -- -D warnings`; the fork also receives `--no-default-features` Clippy. `make format` runs frontend autofixes and `cargo fmt` for both the app and fork, using the current Rust toolchain rather than a hardcoded rustup toolchain path.
- `make test` runs frontend and app Rust tests plus clipboard-fork pure tests with default and `--no-default-features` configurations. Fork tests requiring the system clipboard or a desktop session are ignored by default. Only in a real desktop session prepared for exclusive clipboard access, explicitly run `cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets -- --ignored --test-threads=1` (reads/writes the system clipboard; not part of the default quality checks).
- `npm run test:e2e` runs the actual frontend app with Chromium, a Vite dev server, and an explicit Tauri IPC fixture to check pending/rejected/cancelled operations and feedback, rather than filtering out “missing Tauri” errors. It does not start a native desktop or verify the real clipboard, system dialogs, global hooks, target-application paste acceptance, or runtime behavior on other platforms; these require separate native desktop validation.

#### Version Management

The next release is **v1.2.10**. The repository keeps `0.0.0` as the development version; the Release workflow injects the official version from its tag. To validate a local installer displaying `1.2.10`, first run:

```powershell
# Change the app version in package.json, tauri.conf.json, and Cargo.toml
.\scripts\bump-version.ps1 1.2.10
```

The script changes only these three files. If committing a manual version bump, also synchronize the top-level and root-package versions in `package-lock.json` and the `elegant-clipboard` version in `src-tauri/Cargo.lock`; leave dependency versions unchanged.

Once the intended release changes are committed, push a tag to trigger x64 / arm64 builds:

```powershell
git tag v1.2.10
git push origin v1.2.10
```

Before releasing, run `make check`, `make test`, and `make build`, then verify copy/paste, both paste-key modes, missing-resource rechecks, Save As cancellation, and upgrading from v1.2.9 on a native Windows desktop. To run Makefile commands with PowerShell 7, specify the shell, for example `make SHELL=pwsh check`. Do not upload development-placeholder builds as official assets.

This release also needs WebView2 update validation:

- With beta / dev / canary runtimes, and when the loaded and available versions differ, verify that settings, translation, image/text preview, and editor windows still open.
- Keep the app running for more than ten minutes after a runtime update and verify that version differences or update events alone cause no restart, recovery notification, or fuse activation.
- In an isolated test session, verify that actual process failures or disconnections still request recovery, with at most two automatic attempts for consecutive failures; a successful version check after normal operation can clear old recovery counts.

These are acceptance requirements before publishing. Unit tests and browser E2E cannot replace native Windows desktop validation.

The workflow creates a **draft release** and does not automatically import the changelog. Once all four x64 / arm64 installer and portable assets are complete, add this version's [release notes](docs/Changlog.md#v1210) to the draft, verify versions and assets, finish acceptance checks, and then publish. Replace the changelog's pending status with the actual publication date and set its comparison link to `v1.2.9...v1.2.10`. The Scoop manifest is updated after publication using actual asset hashes. GPUI 2.0 remains a separate branch; v1.2.10 continues to use Tauri + React 1.x.

## Data Storage

The default data directory is the **executable directory**. Change it in Settings → Data → Data storage location; migration or a path change takes effect after restarting.

| Type | Path |
|---|---|
| Startup config (fixed location) | `<exe dir>\config.json` |
| Database | `<data dir>\clipboard.db` |
| Image cache | `<data dir>\images\` |
| Source app icons | `<data dir>\icons\` |
| Staged file copies (not available for every file) | `<data dir>\staged\` |
| File log (can be disabled) | `<data dir>\app.log` |

The installer uses a current-user installation. Whether writing requires administrator privileges depends on the selected directory's Windows permissions; installing does not inherently require admin access. Portable mode is detected by the absence of `uninstall.exe` beside the executable. Both variants need a writable config directory and data directory.

ZIP backups and data migration include the database and app-managed images, icons, and staged files. Original paths in ordinary file history do not thereby become complete file backups.

## License

[MIT License](LICENSE)
