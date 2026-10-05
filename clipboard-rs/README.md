# clipboard-rs

[![Latest version](https://img.shields.io/crates/v/clipboard-rs?color=mediumvioletred)](https://crates.io/crates/clipboard-rs)
[![Documentation](https://docs.rs/clipboard-rs/badge.svg)](https://docs.rs/clipboard-rs)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/ChurchTao/clipboard-rs/test.yml)
![Declared rust-version](https://img.shields.io/badge/manifest_rust--version-1.67.0-blue.svg)
![GitHub License](https://img.shields.io/github/license/ChurchTao/clipboard-rs)

clipboard-rs is a Rust library for reading and writing the system clipboard. This directory contains ElegantClipboard's **local fork**, whose package version remains **0.3.5**. The badges, crates.io package, docs.rs API documentation and upstream repository links above describe [ChurchTao/clipboard-rs](https://github.com/ChurchTao/clipboard-rs), not the additional APIs in this checkout.

[简体中文](README_ZH.md)

## Local Fork Scope

ElegantClipboard depends on this directory through `src-tauri/Cargo.toml`, not on the crates.io release. Existing Windows fork behavior includes:

- Image reads select PNG, then `CF_DIBV5`, then `CF_DIB`. `get_image_dib()` returns the decoded image plus available raw DIB bytes (preferring raw `CF_DIB`, otherwise `CF_DIBV5`). `set_image()` and `set_image_with_dib()` publish PNG and raw `CF_DIB`, preserving the complete encoded DIB header and masks. Windows synthesizes `CF_BITMAP` from `CF_DIB`; the fork no longer uses a separate GDI bitmap writer. A supplied DIB replaces the generated `CF_DIB` payload.
- DIB decoding presents a 14-byte BMP file header with an explicit pixel offset over borrowed DIB bytes. It accounts for header size, palettes and external masks without allocating/copying a second whole-DIB buffer just to prepend the header. Clipboard snapshot reads and decoded pixels still require their own storage.
- `ClipboardContext::get_hdrop_raw()` reads raw `CF_HDROP`; `set_hdrop_raw()` restores it without clearing other formats. `set_raw_no_clear()` writes a registered format by name without clearing existing contents; it is not a numeric standard-format API.
- RTF reads and writes use raw `Rich Text Format` buffers. For binary RTF, use `get_buffer("Rich Text Format")` / `set_buffer()` or `ClipboardContent::Other`; the dedicated string API does not represent arbitrary bytes.
- Windows `ClipboardAccessError` is exported at the crate root and remains downcastable through the boxed `Result` error. It identifies failure of the guarded `OpenClipboard` acquisition and retains the native cause via `Error::source()`; decode, registration and write failures are not reclassified as access failures by matching error text.
- Zero-byte raw payloads are **unsupported by this Windows HGLOBAL publication path**: publishing a zero-size movable allocation with `SetClipboardData` failed on Windows. The fork rejects empty buffers with the exported, downcastable `ClipboardFormatUnsupportedError` before clipboard mutation, including `set_buffer()`, raw RTF/custom entries in `set()`, no-clear raw/HDROP writes and supplied empty DIB. ElegantClipboard preflight likewise rejects empty raw formats, including zero-byte virtual `FileContents`, with `unsupported_content`. It does not pad the data, return a successful no-op, substitute delayed rendering or implement an OLE alternative. Nonempty raw formats remain supported; `set_buffer()` clears existing formats, while `set_raw_no_clear()` preserves them. Final native nonempty/empty-rejection verification is not claimed here.

These fork APIs and implementation details are documented by `src/lib.rs` and `src/platform/win.rs`; upstream docs.rs should not be used as their reference. Low-level multi-format writes are not transactional and may fail after clearing or partially publishing contents; application-level operation preflight is implemented separately in ElegantClipboard.

These are existing fork capabilities, not a new upstream release. [CHANGELOG](CHANGELOG.md) preserves upstream release history.

## Function Support

- Plain text, HTML and RTF
- Images through `RustImageData` and `common::RustImage` (`image` feature, enabled by default), including PNG export
- File lists as `Vec<String>`: Windows/macOS return local paths; the X11/Wayland backends return `file://` URIs
- Raw registered/native formats through `get_buffer()` / `set_buffer()`; `available_formats()` lists format names or MIME types, but does not read their data. On Windows unnamed standard formats may appear as `unknown format` and cannot be retrieved by that placeholder name.
- Clipboard change handlers through `ClipboardHandler` and `ClipboardWatcher`

### Platform Backend Implementations

This table describes the checked-in source, not cross-platform runtime verification or ElegantClipboard application support.

| Type | Windows | macOS | X11 backend | Wayland backend | iOS (experimental) |
| --- | --- | --- | --- | --- | --- |
| Plain text | Read/write | Read/write | Read/write | Read/write | Read/write |
| HTML / RTF | Read/write | Read/write | Read/write | Read/write | Read; dedicated setters unsupported¹ |
| Image² | PNG → DIBV5 → DIB | PNG → TIFF | PNG | PNG | PNG |
| File list | Local paths / CF_HDROP | Local paths / file URLs | URI list | URI list | Unsupported |
| Custom format | Registered name | Pasteboard type | Atom name | MIME type | Pasteboard type |
| Watch changes | OS notifications | Polling | XFixes events | Polling | Polling |

¹ iOS `set_html()`, `set_rich_text()` and multi-format `get()` return `Not supported`; its `set(Vec<ClipboardContent>)` has separate HTML/RTF handling. iOS is not presented as production support.

² Image APIs require the `image` feature. The public Linux dispatcher forwards `get_image_dib()` / `set_image_with_dib()` to the selected X11/Wayland backend. macOS/iOS/X11/Wayland return the decoded image and `None` for raw DIB; `set_image_with_dib(image, None)` uses their normal image writer, while `Some(dib)` is rejected before clipboard mutation rather than silently discarded. These are source-level implementation descriptions, not Linux/macOS/iOS build or runtime proof. No Android backend is implemented.

The optional `wayland` feature enables `wl-clipboard-rs`. With it enabled, constructors try Wayland when `WAYLAND_DISPLAY` is present and fall back to X11 if initialization fails; otherwise they select X11. This describes backend selection, not a guarantee of compositor compatibility.

## Usage

For this fork, use the path dependency from ElegantClipboard's backend manifest:

```toml
[dependencies]
clipboard-rs = { path = "../clipboard-rs" }
```

The relative path assumes a manifest in `src-tauri/`. To use **upstream instead**, `clipboard-rs = "0.3.5"` selects the registry package and does not include these local changes.

The manifest declares edition 2021 and `rust-version = "1.67.0"`; this metadata is not a verified minimum toolchain for all current transitive dependencies. `image` is the only default feature; `wayland` is opt-in.

## [CHANGELOG](CHANGELOG.md)

## Examples

### All Usage Examples

[Examples](examples)

### Reading Text, HTML and RTF

```rust
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};

fn main() {
	let ctx = ClipboardContext::new().unwrap();
	let types = ctx.available_formats().unwrap();
	println!("{:?}", types);

	let has_rtf = ctx.has(ContentFormat::Rtf);
	println!("has_rtf={}", has_rtf);

	let rtf = ctx.get_rich_text().unwrap_or_default();

	println!("rtf={}", rtf);

	let has_html = ctx.has(ContentFormat::Html);
	println!("has_html={}", has_html);

	let html = ctx.get_html().unwrap_or_default();

	println!("html={}", html);

	let content = ctx.get_text().unwrap_or_default();

	println!("txt={}", content);
}

```

### Reading Images

```rust
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};

// Requires the default `image` feature (or explicitly enabling it).
fn main() {
	let ctx = ClipboardContext::new().unwrap();
	let types = ctx.available_formats().unwrap();
	println!("{:?}", types);

	let img = ctx.get_image();

	match img {
		Ok(img) => {
			let path = std::env::temp_dir().join("clipboard-rs-test.png");
			img.save_to_path(path.to_str().unwrap()).unwrap();

			let resize_img = img.thumbnail(300, 300).unwrap();

			let thumbnail_path = std::env::temp_dir().join("clipboard-rs-thumbnail.png");
			resize_img.save_to_path(thumbnail_path.to_str().unwrap()).unwrap();
		}
		Err(err) => {
			println!("err={}", err);
		}
	}
}

```

### Reading Any Format

Format identifiers are platform-specific. This example reads raw HTML bytes; use `get_html()` when you want the library's HTML string handling instead. Arbitrary formats need not be UTF-8.

```rust
use clipboard_rs::{Clipboard, ClipboardContext};

fn main() {
    let ctx = ClipboardContext::new().unwrap();
    let types = ctx.available_formats().unwrap();
    println!("{:?}", types);

    let format = if cfg!(target_os = "windows") {
        "HTML Format"
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        "public.html"
    } else {
        "text/html"
    };
    let buffer = ctx.get_buffer(format).unwrap();
    println!("{}: {} bytes", format, buffer.len());
}

```

### Handling Clipboard Access Errors

The typed access error is Windows-only; other platforms still use their native errors. Do not classify clipboard contention by searching display strings.

```rust
use clipboard_rs::{Clipboard, ClipboardContext, Result};

fn main() -> Result<()> {
    let ctx = ClipboardContext::new()?;
    match ctx.get_text() {
        Ok(text) => println!("{text}"),
        Err(error) => {
            #[cfg(target_os = "windows")]
            if error.is::<clipboard_rs::ClipboardAccessError>() {
                eprintln!("System clipboard could not be opened");
            }
            return Err(error);
        }
    }
    Ok(())
}
```

### Listening to Clipboard Changes

`start_watch()` blocks until the shutdown channel is stopped; run it on a worker thread when the caller must remain responsive. Clipboard changes may contain no text, so the callback below handles a failed text read.

```rust
use clipboard_rs::{
	Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
};
use std::{thread, time::Duration};

struct Manager {
	ctx: ClipboardContext,
}

impl Manager {
	pub fn new() -> Self {
		let ctx = ClipboardContext::new().unwrap();
		Manager { ctx }
	}
}

impl ClipboardHandler for Manager {
	fn on_clipboard_change(&mut self) {
		println!(
			"on_clipboard_change, txt = {}",
			self.ctx.get_text().unwrap_or_default()
		);
	}
}

fn main() {
	let manager = Manager::new();

	let mut watcher = ClipboardWatcherContext::new().unwrap();

	let watcher_shutdown = watcher.add_handler(manager).get_shutdown_channel();

	thread::spawn(move || {
		thread::sleep(Duration::from_secs(5));
		println!("stop watch!");
		watcher_shutdown.stop();
	});

	println!("start watch!");
	watcher.start_watch();
}


```

## X11 Read Timeout

The private X11 backend uses a 500 ms read timeout by default and has a `new_with_options(ClipboardContextX11Options)` constructor. In this checkout, the exported Linux `ClipboardContext` is a runtime dispatcher that only exposes `new()`; it does **not** expose `new_with_options`. The exported options type alone does not make timeout customization available through the public context. The timeout does not describe Windows/macOS/iOS or Wayland reads.

## Local Fork Checks

From the ElegantClipboard repository root, the following commands exercise the default-feature and image-disabled configurations on the **current host**, including declared examples/test targets:

```sh
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features
cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features -- -D warnings
cargo fmt --manifest-path clipboard-rs/Cargo.toml -- --check
```

Pure content, image codec, Windows CF_HTML/DIB header and typed-error tests run normally; image-specific tests are feature-gated and Windows-specific pure tests only exist on Windows. Tests that touch the OS-global clipboard or create a native watcher are ignored by default. Run them only in an isolated native desktop session with exclusive clipboard access; they overwrite the real clipboard and may clear it:

```sh
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets -- --ignored --test-threads=1
```

`--all-targets` means Cargo targets for the selected host, not all operating systems. `--no-default-features` checks the image-disabled build, not Wayland. Wayland requires its opt-in feature and an appropriate Linux environment. These instructions do not assert that checks or native scenarios have run on untested platforms.

## Contributing

For upstream contributions, submit PRs and issues to [ChurchTao/clipboard-rs](https://github.com/ChurchTao/clipboard-rs). Changes specific to this vendored fork belong in ElegantClipboard. Original upstream attribution and contact details are retained below.

## Thanks

- API design is inspired by [electron](https://www.electronjs.org/zh/docs/latest/api/clipboard)
- Linux part of the project code is referenced from [x11-clipboard](https://github.com/quininer/x11-clipboard/tree/master)

## Upstream Author Contact

if you have any questions, you can contact me by email: `swkzymlyy@gmail.com`

Chinese users can also contact me by wechatNo: `uniq_idx_church_lynn`

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
