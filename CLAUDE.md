# ElegantClipboard development guide

This branch is a Rust-only GPUI application for Windows. Do not add Tauri, React, TypeScript, Node.js, WebView, or frontend build dependencies.

## Workspace

- `crates/clipboard-core`: platform-neutral data and business logic
- `crates/clipboard-platform`: Windows clipboard and operating-system integration
- `crates/clipboard-gpui`: GPUI application and interface
- `docs`: design decisions and verification evidence
- `scripts`: Windows packaging and measurement helpers

Dependency direction is `clipboard-gpui -> clipboard-platform -> clipboard-core`; `clipboard-gpui` also uses `clipboard-core` directly. Core must not depend on GPUI or Windows APIs.

## Required checks

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p elegant-clipboard-gpui --release --locked
```

Use `make check`, `make test`, and `make build` for the same normal workflow. Package only on Windows x64 with `make package`.

Tests must use isolated temporary data and must not write to the user's real clipboard unless the test explicitly documents and contains that scope.
