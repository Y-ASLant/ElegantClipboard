# ElegantClipboard development guide

This branch is a Rust-only GPUI application for Windows. Do not add Tauri, React, TypeScript, Node.js, WebView, or frontend build dependencies.

## Workspace

- `crates/clipboard-core`: platform-neutral data and business logic
- `crates/clipboard-platform`: Windows clipboard and operating-system integration
- `crates/clipboard-gpui`: GPUI application and interface
- `docs`: design decisions and verification evidence
- `scripts`: Windows packaging and measurement helpers

Dependency direction is `clipboard-gpui -> clipboard-platform -> clipboard-core`; `clipboard-gpui` also uses `clipboard-core` directly. Core must not depend on GPUI or Windows APIs.

## UI components

The Windows UI uses GPUI Kit. For every UI component provided in the [official GPUI Kit component catalog](https://gpui-kit.com/zh-CN/component), use the official component rather than implementing or introducing a replacement. Use GPUI primitives for layout and application-specific content that the catalog does not provide; do not add another component library. Check the catalog before adding or replacing UI controls.

Use the component that matches the interaction, not just its appearance: transient operation feedback must use GPUI Kit `Notification` through `WindowExt::push_notification` (Root owns the overlay and auto-hide lifecycle), not a permanently mounted `Alert` footer. Reserve inline `Alert` for contextual conditions that remain actionable. Successful automatic background queries should update their displayed data silently; do not overwrite unrelated errors with success banners.

## Code quality and test policy

- Prefer direct, responsibility-focused code and the existing dependency boundaries. Do not add abstractions, forwarding layers, compatibility aliases, or configuration without a current consumer.
- Keep every Rust source, example, and test file within 500 physical lines, including comments and blank lines. Split by responsibility; do not use numbered fragments or `include!` to evade the limit. Run `scripts/check-source-size.ps1` through `just check` and CI.
- Add a permanent regression test only for a plausible consumer-visible bug or uncertain contract: behavior, boundaries, state transitions, precedence, invalid input, persistence, concurrency, or safety invariants.
- Do not add tests that merely repeat source text, labels, static URLs/constants, incidental defaults, widget wiring, forwarding, or mock outputs. Bare “does not throw”, non-empty, and duplicate same-path assertions are not sufficient.
- Remove existing implementation/wording-pinning tests instead of updating expected strings after a refactor. In mixed tests, retain meaningful behavior checks; do not delete persistence, rollback, compatibility, or isolation coverage merely to reduce test count.
- Use temporary smoke scripts for straightforward wiring and visual checks; delete the scripts after verification. UI changes require running the actual native surface and observing its behavior, not only passing unit tests.
- Delete dead code, unused imports, and obsolete test-only helpers only with reference evidence. Update affected documentation; avoid unrelated cleanup and do not suppress compiler or Clippy warnings.

## Required checks

```powershell
.\scripts\check-source-size.ps1
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p elegant-clipboard-gpui --release --locked
```

Use `just check`, `just test`, and `just build` for the same normal workflow. Package only on Windows x64 with `just package`.

Tests must use isolated temporary data and must not write to the user's real clipboard unless the test explicitly documents and contains that scope.
