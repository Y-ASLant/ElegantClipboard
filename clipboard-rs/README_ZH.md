# clipboard-rs

clipboard-rs 是用于读写系统剪贴板的 Rust 库。本目录是 ElegantClipboard 使用的 **本地分叉**，包版本仍为 **0.3.5**。英文 README 的 crates.io、docs.rs、构建状态和上游仓库链接指向 [ChurchTao/clipboard-rs](https://github.com/ChurchTao/clipboard-rs)，不代表本地新增 API。

[English](README.md)

## 本地分叉范围

ElegantClipboard 通过 `src-tauri/Cargo.toml` 的路径依赖使用本目录，而不是 crates.io 发布包。已有的 Windows 分叉行为包括：

- 图片读取按 PNG、`CF_DIBV5`、`CF_DIB` 的顺序选择格式。`get_image_dib()` 返回解码图片与可用的原始 DIB 字节（原始数据优先选择 `CF_DIB`，其次 `CF_DIBV5`）。`set_image()`、`set_image_with_dib()` 发布 PNG 与原始 `CF_DIB`，保留完整的编码 DIB 头与掩码。Windows 从 `CF_DIB` 合成 `CF_BITMAP`；分叉不再使用单独的 GDI 位图写入器。传入的 DIB 会替换生成的 `CF_DIB` 数据。
- DIB 解码在借用的原始字节前提供 14 字节 BMP 文件头与明确的像素偏移，计算头大小、调色板和外置掩码，无需仅为添加文件头再分配、复制一份完整 DIB。读取剪贴板快照与解码后的像素仍需要各自的存储。
- `ClipboardContext::get_hdrop_raw()` 读取原始 `CF_HDROP`；`set_hdrop_raw()` 写回数据且不清空其它格式。`set_raw_no_clear()` 按名称写入注册格式而不清空已有内容，不是接收标准格式数值编号的 API。
- RTF 通过原始 `Rich Text Format` buffer 读写。二进制 RTF 请使用 `get_buffer("Rich Text Format")` / `set_buffer()` 或 `ClipboardContent::Other`；专用字符串 API 不能表示任意字节。
- Windows 的 `ClipboardAccessError` 从 crate 根导出，在 `Result` 的 boxed error 中仍可 downcast。它标识持有剪贴板守卫前的 `OpenClipboard` 获取失败，并通过 `Error::source()` 保留原生原因；不会靠匹配错误文本把解码、格式注册、写入错误误归类为访问失败。
- 零字节原始 payload **不受此 Windows HGLOBAL 发布路径支持**：在 Windows 上通过 `SetClipboardData` 发布零字节可移动 allocation 实际失败。分叉在修改剪贴板之前以从 crate 根导出、可 downcast 的 `ClipboardFormatUnsupportedError` 拒绝空 buffer，包括 `set_buffer()`、`set()` 中的原始 RTF/自定义条目、不清空的原始格式/HDROP 写入及传入的空 DIB。ElegantClipboard 预检同样以 `unsupported_content` 拒绝空原始格式，包括零字节虚拟文件 `FileContents`。不会填充数据、成功但无操作、替换为延迟渲染或另加 OLE 实现。非空原始格式仍支持；`set_buffer()` 清空已有格式，`set_raw_no_clear()` 保留已有格式。这里不宣称已完成非空写入/空数据拒绝的最终原生验证。

这些分叉 API 与实现细节以 `src/lib.rs`、`src/platform/win.rs` 为准，不能用上游 docs.rs 替代。底层多格式写入不是事务，可能在清空或部分发布后失败；应用级操作预检由 ElegantClipboard 单独实现。

这些是已有分叉能力，不是一次新的上游发布。[更新日志](CHANGELOG.md) 保留上游历史发布记录。

## 功能支持

- 纯文本、HTML、RTF
- 使用 `RustImageData`、`common::RustImage` 处理图片并导出 PNG（需 `image` feature，默认启用）
- 文件列表以 `Vec<String>` 返回：Windows/macOS 返回本地路径，X11/Wayland 后端返回 `file://` URI
- 通过 `get_buffer()` / `set_buffer()` 读写原始注册格式或平台格式；`available_formats()` 仅列出格式名或 MIME 类型，不读取数据。Windows 未命名的标准格式可能显示为 `unknown format`，不能用这个占位名称读取数据
- 通过 `ClipboardHandler`、`ClipboardWatcher` 监听变化

### 平台后端实现

下表描述当前源码，不代表跨平台运行验证或 ElegantClipboard 应用的平台支持承诺。

| 类型 | Windows | macOS | X11 后端 | Wayland 后端 | iOS（实验性） |
| --- | --- | --- | --- | --- | --- |
| 纯文本 | 读写 | 读写 | 读写 | 读写 | 读写 |
| HTML / RTF | 读写 | 读写 | 读写 | 读写 | 可读；专用 setter 不支持¹ |
| 图片² | PNG → DIBV5 → DIB | PNG → TIFF | PNG | PNG | PNG |
| 文件列表 | 本地路径 / CF_HDROP | 本地路径 / 文件 URL | URI 列表 | URI 列表 | 不支持 |
| 自定义格式 | 注册名称 | Pasteboard 类型 | Atom 名称 | MIME 类型 | Pasteboard 类型 |
| 监听变化 | 系统通知 | 轮询 | XFixes 事件 | 轮询 | 轮询 |

¹ iOS 的 `set_html()`、`set_rich_text()` 及多格式 `get()` 返回 `Not supported`；其 `set(Vec<ClipboardContent>)` 另有 HTML/RTF 写入处理。iOS 不作为生产支持承诺。

² 图片 API 需要 `image` feature。公开的 Linux 分发层把 `get_image_dib()` / `set_image_with_dib()` 转发到选定的 X11/Wayland 后端。macOS/iOS/X11/Wayland 返回解码图片与 `None` 原始 DIB；`set_image_with_dib(image, None)` 使用其普通图片写入逻辑，而 `Some(dib)` 在改变剪贴板之前返回错误，不再静默丢弃。以上描述源码实现，不是 Linux/macOS/iOS 的构建或运行验证结论。本目录未实现 Android 后端。

可选 `wayland` feature 启用 `wl-clipboard-rs`。启用后，构造函数在存在 `WAYLAND_DISPLAY` 时尝试 Wayland，初始化失败则回退 X11；其它情况选择 X11。这是后端选择逻辑，不保证兼容所有 compositor。

## 使用方法

使用本地分叉时，采用 ElegantClipboard 后端的路径依赖：

```toml
[dependencies]
clipboard-rs = { path = "../clipboard-rs" }
```

相对路径以 `src-tauri/` 内的 manifest 为基准。若改用 **上游**，`clipboard-rs = "0.3.5"` 会选择注册表包，不包含本地修改。

manifest 声明 edition 2021、`rust-version = "1.67.0"`；该元数据不是对所有当前传递依赖最低工具链的验证结论。默认 feature 只有 `image`，`wayland` 需显式启用。

## [更新日志](CHANGELOG.md)

## 示例

### 所有使用示例

[Examples](examples)

### 读取文本、HTML 和 RTF

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

### 读取图片

```rust
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};

// 需要默认的 `image` feature（或显式启用）。
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

### 读取任意类型

格式标识符因平台而异。下例读取原始 HTML 字节；需要库处理后的 HTML 字符串时请使用 `get_html()`。任意格式数据不一定是 UTF-8。

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

### 处理剪贴板访问错误

带类型的访问错误仅用于 Windows；其它平台仍使用各自的原生错误。不要通过搜索显示字符串判断剪贴板占用。

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

### 监听剪贴板变化

`start_watch()` 会阻塞直到停止监听；调用方需要保持响应时应在工作线程运行。剪贴板变化不一定包含文本，因此回调需处理文本读取失败。

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

## X11 读取超时

私有 X11 后端默认读取超时为 500 ms，并具有 `new_with_options(ClipboardContextX11Options)` 构造函数。但当前导出的 Linux `ClipboardContext` 是运行时分发层，只公开 `new()`，**没有**公开 `new_with_options`。仅导出 options 类型不意味着可通过公开 context 自定义超时。该超时不描述 Windows/macOS/iOS 或 Wayland 读取行为。

## 本地分叉检查

在 ElegantClipboard 仓库根目录运行以下命令，可检查 **当前主机** 的默认 feature 与禁用图片配置，包含声明的示例和测试 target：

```sh
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features
cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path clipboard-rs/Cargo.toml --all-targets --no-default-features -- -D warnings
cargo fmt --manifest-path clipboard-rs/Cargo.toml -- --check
```

纯内容、图片编解码、Windows CF_HTML/DIB 头及带类型错误测试正常运行；图片测试受 feature 控制，Windows 专属纯测试仅在 Windows 编译。访问系统全局剪贴板或创建原生 watcher 的测试默认忽略。仅在隔离的原生桌面会话、独占剪贴板的情况下主动运行；这些测试会覆盖真实剪贴板，也可能清空它：

```sh
cargo test --manifest-path clipboard-rs/Cargo.toml --all-targets -- --ignored --test-threads=1
```

`--all-targets` 指所选主机的 Cargo target，不是所有操作系统。`--no-default-features` 检查禁用图片的构建，不是 Wayland。Wayland 需要显式启用其 feature，并提供合适的 Linux 环境。这些命令说明不表示已在未测试的平台运行检查或原生场景。

## 贡献

上游贡献请向 [ChurchTao/clipboard-rs](https://github.com/ChurchTao/clipboard-rs) 提交 PR 或 issue。本地分叉特有修改应提交给 ElegantClipboard。以下保留原上游归属与作者联系方式。

## 感谢

- API 设计灵感来自于 [electron](https://www.electronjs.org/zh/docs/latest/api/clipboard)
- Linux 部分项目代码参考自 [x11-clipboard](https://github.com/quininer/x11-clipboard/tree/master)

## 上游作者联系方式

邮箱: `swkzymlyy@gmail.com`

微信号: `uniq_idx_church_lynn`

## 许可证

本项目遵循 MIT 许可证。详情请参阅 [LICENSE](LICENSE) 文件。
