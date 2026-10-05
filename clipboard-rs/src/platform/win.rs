use std::collections::HashMap;
#[cfg(feature = "image")]
use std::io::{self, BufRead, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::{ContentData, Result};
#[cfg(feature = "image")]
use crate::common::{RustImage, RustImageData};
use crate::{Clipboard, ClipboardContent, ClipboardHandler, ClipboardWatcher, ContentFormat};
use clipboard_win::raw::{set_file_list_with, set_string_with, set_without_clear};
use clipboard_win::types::c_uint;
use clipboard_win::{
	formats, get, options, raw, set, Clipboard as ClipboardWin, ErrorCode, Monitor, SysResult,
};
#[cfg(feature = "image")]
use image::codecs::bmp::BmpDecoder;
#[cfg(feature = "image")]
use image::DynamicImage;

/// The system clipboard could not be opened for an operation.
/// Distinct from format registration, decoding, and clipboard write failures.
#[derive(Debug)]
pub struct ClipboardAccessError {
	source: ErrorCode,
}

impl std::fmt::Display for ClipboardAccessError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "Open clipboard error, code = {}", self.source)
	}
}

impl std::error::Error for ClipboardAccessError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		Some(&self.source)
	}
}

fn open_clipboard() -> Result<ClipboardWin> {
	Ok(ClipboardWin::new_attempts(10).map_err(|source| ClipboardAccessError { source })?)
}

/// This native HGLOBAL clipboard API cannot publish zero-byte raw formats.
#[derive(Debug)]
pub struct ClipboardFormatUnsupportedError;

impl std::fmt::Display for ClipboardFormatUnsupportedError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(
			"Windows HGLOBAL clipboard publication does not support zero-byte raw formats",
		)
	}
}

impl std::error::Error for ClipboardFormatUnsupportedError {}

fn require_raw_payload(bytes: &[u8]) -> Result<()> {
	if bytes.is_empty() {
		return Err(ClipboardFormatUnsupportedError.into());
	}
	Ok(())
}

pub struct WatcherShutdown {
	state: Arc<Mutex<ShutdownState>>,
}

static UNKNOWN_FORMAT: &str = "unknown format";
static CF_RTF: &str = "Rich Text Format";
static CF_HTML: &str = "HTML Format";
#[cfg(feature = "image")]
static CF_PNG: &str = "PNG";

pub struct ClipboardContext {
	format_map: HashMap<&'static str, c_uint>,
	html_format: formats::Html,
}

/// Shared shutdown state between a [`WatcherShutdown`] handle and the running
/// watch loop.
///
/// The `clipboard_win` [`Monitor`] (and thus its `Shutdown`) must be created on
/// the thread that runs `start_watch`, but `get_shutdown_channel` is typically
/// called earlier on another thread. This state bridges that gap and is guarded
/// by a mutex so the handoff is race-free: storing the live `Shutdown` and
/// observing a pre-start stop request both happen under the same lock.
enum ShutdownState {
	/// Watch loop has not published its `Shutdown` yet.
	NotStarted,
	/// Watch loop is running; dropping this `Shutdown` interrupts `recv`.
	Running(clipboard_win::monitor::Shutdown),
	/// Stop was requested before the watch loop published its `Shutdown`.
	StopRequested,
}

pub struct ClipboardWatcherContext<T: ClipboardHandler> {
	handlers: Vec<T>,
	state: Arc<Mutex<ShutdownState>>,
	running: bool,
}

impl ClipboardContext {
	pub fn new() -> Result<ClipboardContext> {
		let (format_map, html_format) = {
			let cf_html_format = formats::Html::new();
			let cf_rtf_uint =
				clipboard_win::register_format(CF_RTF).ok_or("register rich text format error")?;
			#[cfg(feature = "image")]
			let cf_png_uint = clipboard_win::register_format(CF_PNG).ok_or("register PNG format error")?;
			let mut m: HashMap<&str, c_uint> = HashMap::new();
			if let Some(cf_html) = cf_html_format {
				m.insert(CF_HTML, cf_html.code());
			}
			m.insert(CF_RTF, cf_rtf_uint.get());
			#[cfg(feature = "image")]
			m.insert(CF_PNG, cf_png_uint.get());
			(m, cf_html_format)
		};
		Ok(ClipboardContext {
			format_map,
			html_format: html_format.ok_or("register html format error")?,
		})
	}

	fn get_format(&self, format: &ContentFormat) -> Result<c_uint> {
		Ok(match format {
			ContentFormat::Text => formats::CF_UNICODETEXT,
			ContentFormat::Rtf => *self.format_map.get(CF_RTF).unwrap(),
			ContentFormat::Html => self.html_format.code(),
			#[cfg(feature = "image")]
			ContentFormat::Image => formats::CF_DIB,
			ContentFormat::Files => formats::CF_HDROP,
			ContentFormat::Other(format) => clipboard_win::register_format(format)
				.ok_or("register format error")?
				.get(),
		})
	}
}

impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
	pub fn new() -> Result<Self> {
		Ok(Self {
			handlers: Vec::new(),
			state: Arc::new(Mutex::new(ShutdownState::NotStarted)),
			running: false,
		})
	}

	/// Creates a watcher. Provided for cross-platform API symmetry with the
	/// macOS backend; `_interval` is ignored on Windows because changes are
	/// delivered by the OS via `WM_CLIPBOARDUPDATE` rather than polled.
	pub fn new_with_interval(_interval: Duration) -> Result<Self> {
		Self::new()
	}
}

impl Clipboard for ClipboardContext {
	fn available_formats(&self) -> Result<Vec<String>> {
		let _clip = open_clipboard()?;
		let format_count = clipboard_win::count_formats();
		if format_count.is_none() {
			return Ok(Vec::new());
		}
		let mut res = Vec::new();
		let enum_formats = clipboard_win::raw::EnumFormats::new();
		enum_formats.into_iter().for_each(|format| {
			let f_name = raw::format_name_big(format);
			match f_name {
				Some(name) => res.push(name),
				None => {
					res.push(UNKNOWN_FORMAT.to_string());
				}
			}
		});
		Ok(res)
	}

	fn has(&self, format: ContentFormat) -> bool {
		match format {
			ContentFormat::Text => clipboard_win::is_format_avail(formats::CF_UNICODETEXT),
			ContentFormat::Rtf => {
				let cf_rtf_uint = self.format_map.get(CF_RTF).unwrap();
				clipboard_win::is_format_avail(*cf_rtf_uint)
			}
			ContentFormat::Html => {
				let cf_html_uint = self.format_map.get(CF_HTML).unwrap();
				clipboard_win::is_format_avail(*cf_html_uint)
			}
			#[cfg(feature = "image")]
			ContentFormat::Image => {
				let cf_png_uint = self.format_map.get(CF_PNG).unwrap();
				clipboard_win::is_format_avail(*cf_png_uint)
					|| clipboard_win::is_format_avail(formats::CF_DIB)
					|| clipboard_win::is_format_avail(formats::CF_DIBV5)
			}
			ContentFormat::Files => clipboard_win::is_format_avail(formats::CF_HDROP),
			ContentFormat::Other(format) => {
				let format_uint = clipboard_win::register_format(format.as_str());
				if let Some(format_uint) = format_uint {
					return clipboard_win::is_format_avail(format_uint.get());
				}
				false
			}
		}
	}

	fn clear(&self) -> Result<()> {
		let _clip = open_clipboard()?;
		let res = clipboard_win::empty();
		if let Err(e) = res {
			return Err(format!("Empty clipboard error, code = {e}").into());
		}
		Ok(())
	}

	fn get_buffer(&self, format: &str) -> Result<Vec<u8>> {
		let format_uint = clipboard_win::register_format(format);
		if format_uint.is_none() {
			return Err("register format error".into());
		}
		let format_uint = format_uint.unwrap().get();
		let _clip = open_clipboard()?;
		let buffer = get(formats::RawData(format_uint));
		match buffer {
			Ok(data) => Ok(data),
			Err(e) => Err(format!("Get buffer error, code = {e}").into()),
		}
	}

	fn get_text(&self) -> Result<String> {
		let _clip = open_clipboard()?;
		let string: SysResult<String> = get(formats::Unicode);
		match string {
			Ok(s) => Ok(s),
			Err(e) => Err(format!("Get text error, code = {e}").into()),
		}
	}

	fn get_rich_text(&self) -> Result<String> {
		let rtf_raw_data = self.get_buffer(CF_RTF)?;
		Ok(String::from_utf8_lossy(&rtf_raw_data).to_string())
	}

	fn get_html(&self) -> Result<String> {
		let _clip = open_clipboard()?;
		let buffer = get(formats::RawData(self.html_format.code()));
		match buffer {
			Ok(data) => {
				let html_res = String::from_utf8(data);
				if let Ok(html_full_str) = html_res {
					let html = extract_html_from_clipboard_data(html_full_str.as_str());
					if let Ok(html) = html {
						return Ok(html);
					}
				}
				Err("Get html error".into())
			}
			Err(e) => Err(format!("Get buffer error, code = {e}").into()),
		}
	}

	#[cfg(feature = "image")]
	fn get_image(&self) -> Result<RustImageData> {
		let _clip = open_clipboard()?;
		self.get_image_inner()
	}

	fn get_files(&self) -> Result<Vec<String>> {
		let _clip = open_clipboard()?;
		let files: SysResult<Vec<String>> = get(formats::FileList);
		match files {
			Ok(f) => Ok(f),
			Err(e) => Err(format!("Get files error, code = {e}").into()),
		}
	}

	fn get(&self, formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
		let _clip = open_clipboard()?;
		let mut res = Vec::new();
		for format in formats {
			match format {
				ContentFormat::Text => {
					let r = get(formats::Unicode);
					match r {
						Ok(txt) => {
							res.push(ClipboardContent::Text(txt));
						}
						Err(_) => continue,
					}
				}
				ContentFormat::Rtf => {
					let format_uint = self.get_format(format)?;
					let buffer = get(formats::RawData(format_uint));
					match buffer {
						Ok(buffer) => {
							let rtf = String::from_utf8_lossy(&buffer);
							res.push(ClipboardContent::Rtf(rtf.to_string()));
						}
						Err(_) => continue,
					}
				}
				ContentFormat::Html => {
					let html_buffer = get(formats::RawData(self.html_format.code()));
					match html_buffer {
						Ok(html) => {
							let html_res = String::from_utf8(html);
							if let Ok(html_full_str) = html_res {
								let html = extract_html_from_clipboard_data(html_full_str.as_str());
								if let Ok(html) = html {
									res.push(ClipboardContent::Html(html));
								}
							}
						}
						Err(_) => continue,
					}
				}
				#[cfg(feature = "image")]
				ContentFormat::Image => {
					let img = self.get_image_inner();
					match img {
						Ok(img) => {
							res.push(ClipboardContent::Image(img));
						}
						Err(_) => continue,
					}
				}
				ContentFormat::Other(fmt) => {
					let format_uint = self.get_format(format)?;
					let buffer = get(formats::RawData(format_uint));
					match buffer {
						Ok(buffer) => {
							res.push(ClipboardContent::Other(fmt.clone(), buffer));
						}
						Err(_) => continue,
					}
				}
				ContentFormat::Files => {
					let files: SysResult<Vec<String>> = get(formats::FileList);
					match files {
						Ok(files) => {
							res.push(ClipboardContent::Files(files));
						}
						Err(_) => continue,
					}
				}
			}
		}
		Ok(res)
	}

	fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()> {
		require_raw_payload(&buffer)?;
		let format_uint = clipboard_win::register_format(format);
		if format_uint.is_none() {
			return Err("register format error".into());
		}
		let format_uint = format_uint.unwrap().get();
		let _clip = open_clipboard()?;
		clipboard_win::empty().map_err(|e| format!("Empty clipboard error, code = {e}"))?;
		set_without_clear(format_uint, &buffer)
			.map_err(|e| format!("Set buffer error, code = {e}").into())
	}

	fn set_text(&self, text: String) -> Result<()> {
		let _clip = open_clipboard()?;
		let res = set(formats::Unicode, text);
		res.map_err(|e| format!("set text error, code = {e}").into())
	}

	fn set_rich_text(&self, text: String) -> Result<()> {
		self.set_buffer(CF_RTF, text.into_bytes())
	}

	fn set_html(&self, html: String) -> Result<()> {
		let cf_html = plain_html_to_cf_html(&html);
		let _clip = open_clipboard()?;
		let res = set(
			formats::RawData(self.html_format.code()),
			cf_html.as_bytes(),
		);
		res.map_err(|e| format!("set html error, code = {e}").into())
	}

	#[cfg(feature = "image")]
	fn set_image(&self, image: RustImageData) -> Result<()> {
		self.set_image_with_dib(image, None)
	}

	#[cfg(feature = "image")]
	fn get_image_dib(&self) -> Result<(RustImageData, Option<Vec<u8>>)> {
		let _clip = open_clipboard()?;
		let png_format = *self.format_map.get(CF_PNG).unwrap();
		let has_png = clipboard_win::is_format_avail(png_format);
		let has_dib = clipboard_win::is_format_avail(formats::CF_DIB);
		let has_dib_v5 = clipboard_win::is_format_avail(formats::CF_DIBV5);
		let image_format = if has_png {
			png_format
		} else if has_dib_v5 {
			formats::CF_DIBV5
		} else if has_dib {
			formats::CF_DIB
		} else {
			return Err("No image data in clipboard".into());
		};
		let data: Vec<u8> = get(formats::RawData(image_format))
			.map_err(|e| format!("Get image error, code = {e}"))?;
		let image = if has_png {
			RustImageData::from_bytes(&data)?
		} else {
			decode_dib(&data)?
		};
		let dib_format = if has_dib {
			Some(formats::CF_DIB)
		} else if has_dib_v5 {
			Some(formats::CF_DIBV5)
		} else {
			None
		};
		let dib = if dib_format == Some(image_format) {
			Some(data)
		} else {
			dib_format
				.map(|format| get(formats::RawData(format)))
				.transpose()
				.map_err(|e| format!("Get CF_DIB error, code = {e}"))?
		};
		Ok((image, dib))
	}

	#[cfg(feature = "image")]
	fn set_image_with_dib(&self, image: RustImageData, dib_data: Option<&[u8]>) -> Result<()> {
		if let Some(dib) = dib_data {
			require_raw_payload(dib)?;
		}
		let png = image.to_png()?;
		let bmp = image.to_bitmap()?;
		let _clip = open_clipboard()?;
		clipboard_win::empty().map_err(|e| format!("Empty clipboard error, code = {e}"))?;
		let cf_png = self.format_map.get(CF_PNG).unwrap();
		set_without_clear(*cf_png, png.get_bytes())
			.map_err(|e| format!("Set PNG error, code = {e}"))?;
		set_bitmap_inner(bmp.get_bytes())?;
		if let Some(dib) = dib_data {
			set_without_clear(formats::CF_DIB, dib)
				.map_err(|e| format!("Set CF_DIB error, code = {e}"))?;
		}
		Ok(())
	}

	fn set_files(&self, files: Vec<String>) -> Result<()> {
		let _clip = open_clipboard()?;
		let res = set_file_list_with(&files, options::DoClear);
		res.map_err(|e| format!("set files error, code = {e}").into())
	}

	fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
		for content in &contents {
			if matches!(
				content,
				ClipboardContent::Rtf(_) | ClipboardContent::Other(_, _)
			) {
				require_raw_payload(content.as_bytes())?;
			}
		}
		let _clip = open_clipboard()?;
		let res = clipboard_win::empty();
		if let Err(e) = res {
			return Err(format!("Empty clipboard error, code = {e}").into());
		}
		for content in contents {
			match content {
				ClipboardContent::Text(txt) => {
					set_string_with(txt.as_str(), options::NoClear)
						.map_err(|e| format!("Set text error, code = {e}"))?;
				}
				ClipboardContent::Html(html) => {
					let cf_html = plain_html_to_cf_html(&html);
					set_without_clear(self.html_format.code(), cf_html.as_bytes())
						.map_err(|e| format!("Set HTML error, code = {e}"))?;
				}
				#[cfg(feature = "image")]
				ClipboardContent::Image(image) => {
					let png = image.to_png()?;
					let bmp = image.to_bitmap()?;
					set_without_clear(*self.format_map.get(CF_PNG).unwrap(), png.get_bytes())
						.map_err(|e| format!("Set PNG error, code = {e}"))?;
					set_bitmap_inner(bmp.get_bytes())?;
				}
				ClipboardContent::Rtf(_) | ClipboardContent::Other(_, _) => {
					let format_uint = self.get_format(&content.get_format())?;
					set_without_clear(format_uint, content.as_bytes())
						.map_err(|e| format!("Set buffer error, code = {e}"))?;
				}
				ClipboardContent::Files(file_list) => {
					set_file_list_with(&file_list, options::NoClear)
						.map_err(|e| format!("Set files error, code = {e}"))?;
				}
			}
		}
		Ok(())
	}
}

impl ClipboardContext {
	// The caller holds the clipboard open for the entire snapshot.
	#[cfg(feature = "image")]
	fn get_image_inner(&self) -> Result<RustImageData> {
		let png_format = *self.format_map.get(CF_PNG).unwrap();
		if clipboard_win::is_format_avail(png_format) {
			let data: Vec<u8> = get(formats::RawData(png_format))
				.map_err(|e| format!("Get PNG error, code = {e}"))?;
			return RustImageData::from_bytes(&data);
		}
		let dib_format = if clipboard_win::is_format_avail(formats::CF_DIBV5) {
			formats::CF_DIBV5
		} else if clipboard_win::is_format_avail(formats::CF_DIB) {
			formats::CF_DIB
		} else {
			return Err("No image data in clipboard".into());
		};
		let data: Vec<u8> = get(formats::RawData(dib_format))
			.map_err(|e| format!("Get CF_DIB error, code = {e}"))?;
		decode_dib(&data)
	}

	/// Read raw CF_HDROP bytes for lossless file clipboard round-trips.
	pub fn get_hdrop_raw(&self) -> Result<Vec<u8>> {
		let _clip = open_clipboard()?;
		get(formats::RawData(formats::CF_HDROP))
			.map_err(|e| format!("Get CF_HDROP error, code = {e}").into())
	}

	/// Write raw CF_HDROP bytes without clearing the clipboard.
	pub fn set_hdrop_raw(&self, data: &[u8]) -> Result<()> {
		require_raw_payload(data)?;
		let _clip = open_clipboard()?;
		set_without_clear(formats::CF_HDROP, data)
			.map_err(|e| format!("Set CF_HDROP error, code = {e}").into())
	}

	/// Write a registered or standard format without clearing the clipboard.
	pub fn set_raw_no_clear(&self, format: &str, data: &[u8]) -> Result<()> {
		require_raw_payload(data)?;
		let _clip = open_clipboard()?;
		let format_uint = clipboard_win::register_format(format)
			.ok_or_else(|| "register format error".to_string())?
			.get();
		set_without_clear(format_uint, data)
			.map_err(|e| format!("Set raw format error, code = {e}").into())
	}
}

impl<T: ClipboardHandler + Send> ClipboardWatcher<T> for ClipboardWatcherContext<T> {
	fn add_handler(&mut self, f: T) -> &mut Self {
		self.handlers.push(f);
		self
	}

	fn start_watch(&mut self) {
		if self.running {
			println!("already start watch!");
			return;
		}
		if self.handlers.is_empty() {
			println!("no handler, no need to start watch!");
			return;
		}
		self.running = true;
		let mut monitor = Monitor::new().expect("create monitor error");

		// Publish the live Shutdown so a WatcherShutdown can interrupt `recv`.
		// If stop was already requested before we got here, bail out at once.
		{
			let mut state = self.state.lock().unwrap();
			if matches!(*state, ShutdownState::StopRequested) {
				self.running = false;
				return;
			}
			*state = ShutdownState::Running(monitor.shutdown_channel());
		}

		loop {
			match monitor.recv() {
				// New clipboard update.
				Ok(true) => {
					self.handlers.iter_mut().for_each(|f| {
						f.on_clipboard_change();
					});
				}
				// Shutdown requested (Shutdown handle dropped).
				Ok(false) => break,
				Err(e) => {
					eprintln!("watch error, code = {e}");
					break;
				}
			}
		}
		*self.state.lock().unwrap() = ShutdownState::NotStarted;
		self.running = false;
	}

	fn get_shutdown_channel(&self) -> WatcherShutdown {
		WatcherShutdown {
			state: self.state.clone(),
		}
	}
}

impl Drop for WatcherShutdown {
	fn drop(&mut self) {
		// Take the current state, marking a stop request, then act on what we
		// took after releasing the lock.
		let taken = {
			let mut state = self.state.lock().unwrap();
			std::mem::replace(&mut *state, ShutdownState::StopRequested)
		};
		match taken {
			// Loop is blocked in `recv`; dropping its Shutdown interrupts it.
			ShutdownState::Running(shutdown) => drop(shutdown),
			// Loop has not started (or already stopped): StopRequested, written
			// above, makes it bail out before entering `recv`.
			ShutdownState::NotStarted | ShutdownState::StopRequested => {}
		}
	}
}

// 将输入的 UTF-8 字符串转换为宽字符（UTF-16）字符串
// fn utf8_to_utf16(input: &str) -> Vec<u16> {
// 	let mut vec: Vec<u16> = input.encode_utf16().collect();
// 	vec.push(0);
// 	vec
// }

// https://learn.microsoft.com/en-us/windows/win32/dataxchg/html-clipboard-format
// The description header includes the clipboard version number and offsets, indicating where the context and the fragment start and end. The description is a list of ASCII text keywords followed by a string and separated by a colon (:).
// Version: vv version number of the clipboard. Starting version is . As of Windows 10 20H2 this is now .Version:0.9Version:1.0
// StartHTML: Offset (in bytes) from the beginning of the clipboard to the start of the context, or if no context.-1
// EndHTML: Offset (in bytes) from the beginning of the clipboard to the end of the context, or if no context.-1
// StartFragment: Offset (in bytes) from the beginning of the clipboard to the start of the fragment.
// EndFragment: Offset (in bytes) from the beginning of the clipboard to the end of the fragment.
// StartSelection: Optional. Offset (in bytes) from the beginning of the clipboard to the start of the selection.
// EndSelection: Optional. Offset (in bytes) from the beginning of the clipboard to the end of the selection.
// The and keywords are optional and must both be omitted if you do not want the application to generate this information.StartSelectionEndSelection
// Future revisions of the clipboard format may extend the header, for example, since the HTML starts at the offset then multiple and pairs could be added later to support noncontiguous selection of fragments.CF_HTMLStartHTMLStartFragmentEndFragment
// example:
// html=Version:1.0
// StartHTML:000000096
// EndHTML:000000375
// StartFragment:000000096
// EndFragment:000000375
// <html><head><meta http-equiv="content-type" content="text/html; charset=UTF-8"></head><body><div style="background-color:#2b2b2b;color:#a9b7c6;font-family:'JetBrains Mono',monospace;font-size:9.8pt;"><pre><span style="color:#9876aa;">sellChannel</span></pre></div></body></html>
// cp from https://github.com/Devolutions/IronRDP/blob/37aa6426dba3272f38a2bb46a513144a326854ee/crates/ironrdp-cliprdr-format/src/html.rs#L91
fn plain_html_to_cf_html(fragment: &str) -> String {
	const POS_PLACEHOLDER: &str = "0000000000";

	let mut buffer = String::new();

	let mut write_header = |key: &str, value: &str| {
		let size = key.len() + value.len() + ":\r\n".len();
		buffer.reserve(size);

		buffer.push_str(key);
		buffer.push(':');
		let value_pos = buffer.len();
		buffer.push_str(value);
		buffer.push_str("\r\n");

		value_pos
	};

	write_header("Version", "0.9");

	let start_html_header_value_pos = write_header("StartHTML", POS_PLACEHOLDER);
	let end_html_header_value_pos = write_header("EndHTML", POS_PLACEHOLDER);
	let start_fragment_header_value_pos = write_header("StartFragment", POS_PLACEHOLDER);
	let end_fragment_header_value_pos = write_header("EndFragment", POS_PLACEHOLDER);

	let start_html_pos = buffer.len();
	if !fragment.starts_with("<html>") {
		buffer.push_str("<html>\r\n<body>\r\n<!--StartFragment-->");
	}

	let start_fragment_pos = buffer.len();
	buffer.push_str(fragment);

	let end_fragment_pos = buffer.len();
	if !fragment.ends_with("</html>") {
		buffer.push_str("<!--EndFragment-->\r\n</body>\r\n</html>");
	}

	let end_html_pos = buffer.len();

	let start_html_pos_value = format!("{start_html_pos:0>10}");
	let end_html_pos_value = format!("{end_html_pos:0>10}");
	let start_fragment_pos_value = format!("{start_fragment_pos:0>10}");
	let end_fragment_pos_value = format!("{end_fragment_pos:0>10}");

	let mut replace_placeholder = |value_begin_idx: usize, header_value: &str| {
		let value_end_idx = value_begin_idx + POS_PLACEHOLDER.len();
		buffer.replace_range(value_begin_idx..value_end_idx, header_value);
	};

	replace_placeholder(start_html_header_value_pos, &start_html_pos_value);
	replace_placeholder(end_html_header_value_pos, &end_html_pos_value);
	replace_placeholder(start_fragment_header_value_pos, &start_fragment_pos_value);
	replace_placeholder(end_fragment_header_value_pos, &end_fragment_pos_value);

	buffer
}

const SEP: char = ':';
const START_HTML: &str = "StartHTML";
const END_HTML: &str = "EndHTML";

fn extract_html_from_clipboard_data(data: &str) -> Result<String> {
	let mut start_idx = 0usize;
	let mut end_idx = data.len();
	for line in data.lines() {
		let mut split = line.split(SEP);
		let key = match split.next() {
			Some(key) => key,
			None => break,
		};
		let value = match split.next() {
			Some(value) => value,
			//Reached HTML
			None => break,
		};
		match key {
			START_HTML => match value.trim_start_matches('0').parse() {
				Ok(value) => {
					start_idx = value;
					continue;
				}
				//Should not really happen
				Err(_) => break,
			},
			END_HTML => match value.trim_start_matches('0').parse() {
				Ok(value) => {
					end_idx = value;
					continue;
				}
				//Should not really happen
				Err(_) => break,
			},
			_ => continue,
		}
	}
	//Make sure HTML writer didn't screw up offsets of fragment
	// Check that start_idx is within bounds
	if start_idx > data.len() {
		return Err("Invalid HTML offsets: start index exceeds data length".into());
	}
	// Check that end_idx is within bounds
	if end_idx > data.len() {
		return Err("Invalid HTML offsets: end index exceeds data length".into());
	}
	// Check that end_idx >= start_idx
	if end_idx < start_idx {
		return Err("Invalid HTML offsets: end index before start index".into());
	}
	data.get(start_idx..end_idx)
		.map(str::to_owned)
		.ok_or_else(|| "Invalid HTML offsets: offsets split a UTF-8 character".into())
}

#[cfg(test)]
mod html_tests {
	use super::{extract_html_from_clipboard_data, plain_html_to_cf_html};

	#[test]
	fn extracts_unicode_html_using_byte_offsets() {
		let html = "<html><body>中文 café 😀</body></html>";
		let data = plain_html_to_cf_html(html);
		assert_eq!(extract_html_from_clipboard_data(&data).unwrap(), html);
	}

	#[test]
	fn rejects_invalid_html_byte_offsets() {
		let data = plain_html_to_cf_html("<html><body>中文😀</body></html>");
		let html_start = data.find("<html>").unwrap();
		let unicode_start = data.find('中').unwrap();
		let original_start = format!("StartHTML:{html_start:010}");
		let original_end = format!("EndHTML:{:010}", data.len());

		for (start, end) in [
			(unicode_start + 1, data.len()),
			(html_start, unicode_start + 1),
			(data.len() + 1, data.len()),
			(html_start, data.len() + 1),
			(data.len(), html_start),
		] {
			let malformed = data
				.replace(&original_start, &format!("StartHTML:{start:010}"))
				.replace(&original_end, &format!("EndHTML:{end:010}"));
			assert!(
				extract_html_from_clipboard_data(&malformed).is_err(),
				"accepted invalid offsets {start}..{end}"
			);
		}
	}
}

#[cfg(feature = "image")]
fn decode_dib(data: &[u8]) -> Result<RustImageData> {
	let size_bytes: [u8; 4] = data.get(..4).ok_or("Invalid DIB header")?.try_into()?;
	let header_size = u32::from_le_bytes(size_bytes);
	let header = data
		.get(..header_size as usize)
		.ok_or("Truncated DIB header")?;
	let (bits, colors, palette_entry_size, external_masks) = match header_size {
		12 => (u16::from_le_bytes(header[10..12].try_into()?), 0, 3, 0),
		40 | 52 | 56 | 108 | 124 => {
			let bits = u16::from_le_bytes(header[14..16].try_into()?);
			let compression = u32::from_le_bytes(header[16..20].try_into()?);
			let colors = u32::from_le_bytes(header[32..36].try_into()?);
			let masks = if header_size == 40 {
				match compression {
					3 => 12,
					6 => 16,
					_ => 0,
				}
			} else {
				0
			};
			(bits, colors, 4, masks)
		}
		_ => return Err("Unsupported DIB header".into()),
	};
	let colors = if colors == 0 && bits <= 8 {
		1_u32 << bits
	} else {
		colors
	};
	let pixel_offset = colors
		.checked_mul(palette_entry_size)
		.and_then(|palette| {
			header_size
				.checked_add(external_masks)?
				.checked_add(palette)
		})
		.ok_or("DIB metadata size overflow")?;
	if pixel_offset as usize > data.len() {
		return Err("Truncated DIB metadata".into());
	}
	let file_size = u32::try_from(data.len())?
		.checked_add(14)
		.ok_or("DIB size overflow")?;
	let mut file_header = [0_u8; 14];
	file_header[..2].copy_from_slice(b"BM");
	file_header[2..6].copy_from_slice(&file_size.to_le_bytes());
	file_header[10..14].copy_from_slice(&(pixel_offset + 14).to_le_bytes());
	// The normal BMP decoder honors bfOffBits. The headerless decoder incorrectly
	// skips external masks for V4/V5 even though their masks are inside the header.
	let decoder = BmpDecoder::new(DibBitmapReader {
		header: file_header,
		data,
		position: 0,
	})?;
	Ok(RustImageData::from_dynamic_image(
		DynamicImage::from_decoder(decoder)?,
	))
}

// Present a BMP file header before borrowed DIB bytes without allocating or
// copying the clipboard payload. The image decoder requires BufRead + Seek.
#[cfg(feature = "image")]
struct DibBitmapReader<'a> {
	header: [u8; 14],
	data: &'a [u8],
	position: u64,
}

#[cfg(feature = "image")]
impl Read for DibBitmapReader<'_> {
	fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
		let bytes = self.fill_buf()?;
		let count = output.len().min(bytes.len());
		output[..count].copy_from_slice(&bytes[..count]);
		self.consume(count);
		Ok(count)
	}
}

#[cfg(feature = "image")]
impl BufRead for DibBitmapReader<'_> {
	fn fill_buf(&mut self) -> io::Result<&[u8]> {
		let Ok(position) = usize::try_from(self.position) else {
			return Ok(&[]);
		};
		Ok(if position < self.header.len() {
			&self.header[position..]
		} else {
			self.data
				.get(position - self.header.len()..)
				.unwrap_or_default()
		})
	}

	fn consume(&mut self, count: usize) {
		self.position = self.position.saturating_add(count as u64);
	}
}

#[cfg(feature = "image")]
impl Seek for DibBitmapReader<'_> {
	fn seek(&mut self, offset: SeekFrom) -> io::Result<u64> {
		let position = match offset {
			SeekFrom::Start(position) => Some(position),
			SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
			SeekFrom::End(offset) => {
				(self.header.len() as u64 + self.data.len() as u64).checked_add_signed(offset)
			}
		}
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid bitmap seek"))?;
		self.position = position;
		Ok(position)
	}
}

#[cfg(feature = "image")]
fn set_bitmap_inner(data: &[u8]) -> Result<()> {
	// BMP differs from CF_DIB only by its 14-byte file header. Preserve the
	// complete encoded DIB header and masks, including for tiny images.
	let dib = data
		.strip_prefix(b"BM")
		.and_then(|data| data.get(12..))
		.ok_or("Invalid bitmap data")?;
	set_without_clear(formats::CF_DIB, dib)
		.map_err(|e| format!("Set CF_DIB error, code = {e}").into())
}

#[cfg(all(test, feature = "image"))]
mod dib_tests {
	use super::decode_dib;
	use crate::common::{RustImage, RustImageData};
	use image::{DynamicImage, Rgba, RgbaImage};

	#[test]
	fn decodes_headerless_dib_preserving_masks_dimensions_and_rgba_rows() {
		for (width, height) in [(1, 1), (3, 2)] {
			let pixels = RgbaImage::from_fn(width, height, |x, y| {
				Rgba([
					(32 + x * 64) as u8,
					(64 + y * 64) as u8,
					128,
					(16 + (x + y) * 32) as u8,
				])
			});
			let image = RustImageData::from_dynamic_image(DynamicImage::ImageRgba8(pixels.clone()));
			let bitmap = image.to_bitmap().unwrap();
			let dib = &bitmap.get_bytes()[14..];
			assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 108);
			assert_eq!(u32::from_le_bytes(dib[4..8].try_into().unwrap()), width);
			assert_eq!(u32::from_le_bytes(dib[8..12].try_into().unwrap()), height);
			assert_eq!(u32::from_le_bytes(dib[16..20].try_into().unwrap()), 3);
			for (bytes, expected_mask) in dib[40..56].chunks_exact(4).zip([
				0x00ff_0000_u32,
				0x0000_ff00,
				0x0000_00ff,
				0xff00_0000,
			]) {
				assert_eq!(u32::from_le_bytes(bytes.try_into().unwrap()), expected_mask);
			}
			assert_eq!(decode_dib(dib).unwrap().to_rgba8().unwrap(), pixels);
			let mut v5 = Vec::with_capacity(dib.len() + 16);
			v5.extend_from_slice(&dib[..108]);
			v5[..4].copy_from_slice(&124_u32.to_le_bytes());
			v5.extend_from_slice(&[0; 16]);
			v5.extend_from_slice(&dib[108..]);
			assert_eq!(decode_dib(&v5).unwrap().to_rgba8().unwrap(), pixels);
			for header_size in [52_u32, 56] {
				let mut embedded_masks = dib[..header_size as usize].to_vec();
				embedded_masks[..4].copy_from_slice(&header_size.to_le_bytes());
				embedded_masks.extend_from_slice(&dib[108..]);
				let mut expected = pixels.clone();
				if header_size == 52 {
					for pixel in expected.pixels_mut() {
						pixel[3] = 255;
					}
				}
				assert_eq!(
					decode_dib(&embedded_masks).unwrap().to_rgba8().unwrap(),
					expected
				);
			}
			let mut top_down = dib[..108].to_vec();
			top_down[8..12].copy_from_slice(&(-(height as i32)).to_le_bytes());
			for row in dib[108..].chunks_exact((width * 4) as usize).rev() {
				top_down.extend_from_slice(row);
			}
			assert_eq!(decode_dib(&top_down).unwrap().to_rgba8().unwrap(), pixels);
		}
	}

	#[test]
	fn decodes_info_header_with_external_rgb565_masks() {
		let mut dib = vec![0; 40];
		dib[..4].copy_from_slice(&40_u32.to_le_bytes());
		dib[4..8].copy_from_slice(&2_i32.to_le_bytes());
		dib[8..12].copy_from_slice(&1_i32.to_le_bytes());
		dib[12..14].copy_from_slice(&1_u16.to_le_bytes());
		dib[14..16].copy_from_slice(&16_u16.to_le_bytes());
		dib[16..20].copy_from_slice(&3_u32.to_le_bytes());
		dib[20..24].copy_from_slice(&4_u32.to_le_bytes());
		for mask in [0xf800_u32, 0x07e0, 0x001f] {
			dib.extend_from_slice(&mask.to_le_bytes());
		}
		dib.extend_from_slice(&[0x00, 0xf8, 0x1f, 0x00]);
		let pixels = decode_dib(&dib).unwrap().to_rgba8().unwrap();
		assert_eq!(pixels.dimensions(), (2, 1));
		assert_eq!(*pixels.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
		assert_eq!(*pixels.get_pixel(1, 0), Rgba([0, 0, 255, 255]));
	}

	#[test]
	fn decodes_core_header_with_three_byte_palette_entries() {
		let mut dib = vec![0; 12];
		dib[..4].copy_from_slice(&12_u32.to_le_bytes());
		dib[4..6].copy_from_slice(&2_u16.to_le_bytes());
		dib[6..8].copy_from_slice(&1_u16.to_le_bytes());
		dib[8..10].copy_from_slice(&1_u16.to_le_bytes());
		dib[10..12].copy_from_slice(&1_u16.to_le_bytes());
		dib.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
		dib.extend_from_slice(&[0x40, 0, 0, 0]);
		let pixels = decode_dib(&dib).unwrap().to_rgba8().unwrap();
		assert_eq!(pixels.dimensions(), (2, 1));
		assert_eq!(*pixels.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
		assert_eq!(*pixels.get_pixel(1, 0), Rgba([255, 255, 255, 255]));
	}

	#[test]
	fn decodes_top_down_info_header_with_explicit_palette_size() {
		let mut dib = vec![0; 40];
		dib[..4].copy_from_slice(&40_u32.to_le_bytes());
		dib[4..8].copy_from_slice(&2_i32.to_le_bytes());
		dib[8..12].copy_from_slice(&(-2_i32).to_le_bytes());
		dib[12..14].copy_from_slice(&1_u16.to_le_bytes());
		dib[14..16].copy_from_slice(&8_u16.to_le_bytes());
		dib[20..24].copy_from_slice(&8_u32.to_le_bytes());
		dib[32..36].copy_from_slice(&2_u32.to_le_bytes());
		dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
		dib.extend_from_slice(&[0, 1, 0, 0, 1, 0, 0, 0]);
		let pixels = decode_dib(&dib).unwrap().to_rgba8().unwrap();
		assert_eq!(pixels.dimensions(), (2, 2));
		assert_eq!(*pixels.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
		assert_eq!(*pixels.get_pixel(1, 0), Rgba([0, 0, 255, 255]));
		assert_eq!(*pixels.get_pixel(0, 1), Rgba([0, 0, 255, 255]));
		assert_eq!(*pixels.get_pixel(1, 1), Rgba([255, 0, 0, 255]));
	}

	#[test]
	fn rejects_overflowed_palette_metadata() {
		let mut dib = vec![0; 40];
		dib[..4].copy_from_slice(&40_u32.to_le_bytes());
		dib[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
		assert!(decode_dib(&dib).is_err());
	}

	#[test]
	fn rejects_truncated_dib_headers() {
		for data in [b"".as_slice(), b"BM".as_slice(), &[40, 0, 0, 0]] {
			assert!(decode_dib(data).is_err());
		}
	}
}

#[cfg(test)]
mod access_error_tests {
	use super::{ClipboardAccessError, ErrorCode};

	#[test]
	fn boxed_access_error_preserves_native_cause_without_classifying_text() {
		let error: Box<dyn std::error::Error + Send + Sync> = ClipboardAccessError {
			source: ErrorCode::new_system(5),
		}
		.into();
		assert!(error.is::<ClipboardAccessError>());
		assert_eq!(
			error
				.source()
				.unwrap()
				.downcast_ref::<ErrorCode>()
				.unwrap()
				.raw_code(),
			5
		);
		let diagnostic: Box<dyn std::error::Error + Send + Sync> =
			"Open clipboard error, code = 5".into();
		assert!(!diagnostic.is::<ClipboardAccessError>());
	}
}
