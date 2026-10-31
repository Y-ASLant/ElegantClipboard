# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# ElegantClipboard development guide

This branch is a Rust-only GPUI application for Windows, macOS, and Linux. Do not add Tauri, React, TypeScript, Node.js, WebView, or frontend build dependencies.

## Workspace

- `crates/clipboard-core`: platform-neutral data and business logic (SQLite, query, retention, backup, preferences)
- `crates/clipboard-platform`: the shared `Service` plus per-OS integration
- `crates/clipboard-gpui` (package `elegant-clipboard-gpui`): GPUI application and interface
- `docs`: design decisions and verification evidence; `docs/README.md` maps modules to responsibilities
- `scripts`: packaging, smoke, measurement, and the source-size check

Dependency direction is `clipboard-gpui -> clipboard-platform -> clipboard-core`; `clipboard-gpui` also uses `clipboard-core` directly. Core must not depend on GPUI or any OS API. The tray lives in `clipboard-gpui`; the platform crate supplies the OS integration behind it.

Version lives only in the root `Cargo.toml` `[workspace.package]` and `Cargo.lock`. Bump it with `scripts/bump-version.ps1`; a release tag must match, e.g. `v2.0.1`.

## Runtime shape

`clipboard-platform/src/service/` holds the one `Service`, command protocol, queue, and worker shared by all three platforms. `Command::Copy { id, mode, purpose }` and `Command::Merge { ids, purpose }` are the copy protocol; `CopyPurpose` distinguishes a plain copy from a numbered paste-back, and there are no paired `*ForPaste` commands. `Service.initial_preferences` carries the core `PreferenceSnapshot` loaded at startup.

Platform integration is compiled per target, not selected at runtime: `windows/` for Win32 clipboard fidelity, hotkeys, OLE drag, and DWM; `macos/` for Carbon hotkeys, AppKit drag, Accessibility paste, and LaunchAgent; `linux/` for X11/XRes/XTEST/XDND and XDG autostart. Wayland needs the compositor's data-control protocol and does not silently fall back to XWayland capture.

`clipboard-gpui/src/main.rs` dispatches to `windows/` or `unix/`; window code splits further into `macos/` and `linux/`, with shared popup geometry in `popup_geometry.rs`. `ui/`, `state.rs`, `visual.rs`, and `window_effect.rs` are shared across platforms and must not call Win32 directly. `design_system.rs` holds theme tokens; `PasteCoordinator` owns paste targets, requests, and the sequence queue so entry points update it through transitions instead of writing its fields.

## UI components

The UI uses GPUI Kit. For every UI component provided in the [official GPUI Kit component catalog](https://gpui-kit.com/zh-CN/component), use the official component rather than implementing or introducing a replacement. Use GPUI primitives for layout and application-specific content that the catalog does not provide; do not add another component library. Check the catalog before adding or replacing UI controls.

Use the component that matches the interaction, not just its appearance: transient operation feedback must use GPUI Kit `Notification` through `WindowExt::push_notification` (Root owns the overlay and auto-hide lifecycle), not a permanently mounted `Alert` footer. Reserve inline `Alert` for contextual conditions that remain actionable. Successful automatic background queries should update their displayed data silently; do not overwrite unrelated errors with success banners.

## Code quality and test policy

- Prefer direct, responsibility-focused code and the existing dependency boundaries. Do not add abstractions, forwarding layers, compatibility aliases, or configuration without a current consumer.
- Keep every Rust source, example, and test file within 500 physical lines, including comments and blank lines. Split by responsibility; do not use numbered fragments or `include!` to evade the limit. `scripts/check-source-size.ps1` is the only line-limit check; `just check` and `.github/workflows/gpui.yml` run it, and a plain `cargo check` does not.
- Add a permanent regression test only for a plausible consumer-visible bug or uncertain contract: behavior, boundaries, state transitions, precedence, invalid input, persistence, concurrency, or safety invariants.
- Do not add tests that merely repeat source text, labels, static URLs/constants, incidental defaults, widget wiring, forwarding, or mock outputs. Bare “does not throw”, non-empty, and duplicate same-path assertions are not sufficient.
- Remove existing implementation/wording-pinning tests instead of updating expected strings after a refactor. In mixed tests, retain meaningful behavior checks; do not delete persistence, rollback, compatibility, or isolation coverage merely to reduce test count.
- Use temporary smoke scripts for straightforward wiring and visual checks; delete the scripts after verification. UI changes require running the actual native surface and observing its behavior, not only passing unit tests.
- Delete dead code, unused imports, and obsolete test-only helpers only with reference evidence. Update affected documentation; avoid unrelated cleanup and do not suppress compiler or Clippy warnings.

## Commands

Recipes live in the hidden `.justfile`. `just check`, `just test`, and `just build` cover the normal workflow:

```powershell
just check    # source-size, rustfmt, cargo check, clippy -D warnings
just test     # cargo test --workspace --locked
just build    # cargo build -p elegant-clipboard-gpui --release --locked
just run      # cargo run -p elegant-clipboard-gpui --locked
just format
```

Run one test by name:

```powershell
cargo test -p clipboard-core --lib file_copy_cleanup --locked
cargo test -p clipboard-platform --locked hotkey_change_is_acknowledged -- --test-threads=8
```

Package only on the matching OS. `just package` builds the Windows x64 ZIP; `just package-unix` (macOS/Linux only) builds the DMG, or the Linux `.tar.gz`, `.deb`, and `.rpm`. Linux installers need `dpkg-dev` and `rpm`. Do not cross-build a Mac or Linux package from Windows.

CI (`.github/workflows/gpui.yml`) runs the same checks on `windows-latest`, `ubuntu-24.04`, and `macos-15`. Linux tests run under `dbus-run-session -- xvfb-run -a`; the Windows and macOS test steps call `cargo test --workspace --locked` directly.

Tests must use isolated temporary data and must not write to the user's real clipboard unless the test explicitly documents and contains that scope.

<!-- rtk-instructions v2 -->
# Command output

Command output here is condensed to save tokens, keeping every signal and
dropping costly noise. Treat it as the complete result: run commands
normally, and batch related commands into one call to avoid extra turns.
Truncated results state their recovery path in their own output. Re-run a
command as `rtk bypass <cmd>` only when its result is unusable: empty when
output was clearly expected, contradicting its exit code, or garbled.
<!-- /rtk-instructions -->
