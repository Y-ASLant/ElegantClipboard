use crate::{common::Result, Clipboard, ClipboardContent, ClipboardHandler, ContentFormat};

#[cfg(feature = "image")]
use crate::{common::RustImage, RustImageData};

use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use wl_clipboard_rs::{
	copy::{self, MimeSource, MimeType, Options, Source},
	paste::{self, get_contents, get_mime_types, ClipboardType, Seat},
	utils::is_primary_selection_supported,
};

const MIME_HTML: &str = "text/html";
const MIME_RTF: &str = "text/rtf";
#[cfg(feature = "image")]
const MIME_PNG: &str = "image/png";
const MIME_URI_LIST: &str = "text/uri-list";
const FILE_PATH_PREFIX: &str = "file://";

fn read_wayland_clipboard(mime: paste::MimeType) -> Result<Vec<u8>> {
	let result = get_contents(ClipboardType::Regular, Seat::Unspecified, mime);
	match result {
		Ok((mut pipe, _)) => {
			let mut buffer = vec![];
			pipe.read_to_end(&mut buffer).map_err(
				|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() },
			)?;
			Ok(buffer)
		}
		Err(paste::Error::ClipboardEmpty) | Err(paste::Error::NoMimeType) => {
			Err("Clipboard is empty or content type not available".into())
		}
		Err(e) => Err(e.to_string().into()),
	}
}

fn write_wayland_clipboard(sources: Vec<MimeSource>) -> Result<()> {
	let mut opts = Options::new();
	opts.foreground(false);
	opts.clipboard(copy::ClipboardType::Regular);
	opts.copy_multi(sources)
		.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
	Ok(())
}

fn get_offered_mime_types() -> Result<std::collections::HashSet<String>> {
	get_mime_types(ClipboardType::Regular, Seat::Unspecified)
		.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
}

pub struct ClipboardContext;

impl ClipboardContext {
	pub fn new() -> Result<Self> {
		match is_primary_selection_supported() {
			Ok(_) => Ok(Self),
			Err(e) => Err(e.to_string().into()),
		}
	}
}

impl Clipboard for ClipboardContext {
	fn available_formats(&self) -> Result<Vec<String>> {
		let mime_types = get_offered_mime_types()?;
		Ok(mime_types.into_iter().collect())
	}

	fn has(&self, format: ContentFormat) -> bool {
		#[allow(unreachable_patterns)]
		let mime_to_check = match format {
			ContentFormat::Text => "text/plain",
			ContentFormat::Html => MIME_HTML,
			ContentFormat::Rtf => MIME_RTF,
			#[cfg(feature = "image")]
			ContentFormat::Image => MIME_PNG,
			ContentFormat::Files => MIME_URI_LIST,
			ContentFormat::Other(ref s) => s.as_str(),
			#[cfg(not(feature = "image"))]
			_ => return false,
		};
		if let Ok(mime_types) = get_offered_mime_types() {
			// For text, also check text/plain;charset=utf-8
			if mime_to_check == "text/plain" {
				mime_types.iter().any(|m| m.starts_with("text/plain"))
			} else {
				mime_types.contains(mime_to_check)
			}
		} else {
			false
		}
	}

	fn clear(&self) -> Result<()> {
		copy::clear(copy::ClipboardType::Regular, copy::Seat::All)
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
		Ok(())
	}

	fn get_buffer(&self, format: &str) -> Result<Vec<u8>> {
		read_wayland_clipboard(paste::MimeType::Specific(format))
	}

	fn get_text(&self) -> Result<String> {
		let bytes = read_wayland_clipboard(paste::MimeType::Text)?;
		String::from_utf8(bytes)
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
	}

	fn get_rich_text(&self) -> Result<String> {
		let bytes = read_wayland_clipboard(paste::MimeType::Specific(MIME_RTF))?;
		String::from_utf8(bytes)
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
	}

	fn get_html(&self) -> Result<String> {
		let bytes = read_wayland_clipboard(paste::MimeType::Specific(MIME_HTML))?;
		String::from_utf8(bytes)
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
	}

	#[cfg(feature = "image")]
	fn get_image(&self) -> Result<RustImageData> {
		let bytes = read_wayland_clipboard(paste::MimeType::Specific(MIME_PNG))?;
		RustImageData::from_bytes(&bytes)
	}

	#[cfg(feature = "image")]
	fn get_image_dib(&self) -> Result<(RustImageData, Option<Vec<u8>>)> {
		let img = self.get_image()?;
		Ok((img, None))
	}

	fn get_files(&self) -> Result<Vec<String>> {
		let bytes = read_wayland_clipboard(paste::MimeType::Specific(MIME_URI_LIST))?;
		let text = String::from_utf8(bytes)
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
		Ok(text
			.lines()
			.map(|line| line.trim_end_matches('\r'))
			.filter(|line| {
				!line.starts_with('#') && !line.is_empty() && line.starts_with(FILE_PATH_PREFIX)
			})
			.map(|line| line.to_string())
			.collect())
	}

	fn get(&self, formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
		let mut contents = Vec::new();
		for format in formats {
			match format {
				ContentFormat::Text => {
					if let Ok(text) = self.get_text() {
						contents.push(ClipboardContent::Text(text));
					}
				}
				ContentFormat::Html => {
					if let Ok(html) = self.get_html() {
						contents.push(ClipboardContent::Html(html));
					}
				}
				ContentFormat::Rtf => {
					if let Ok(rtf) = self.get_rich_text() {
						contents.push(ClipboardContent::Rtf(rtf));
					}
				}
				#[cfg(feature = "image")]
				ContentFormat::Image => {
					if let Ok(image) = self.get_image() {
						contents.push(ClipboardContent::Image(image));
					}
				}
				ContentFormat::Files => {
					if let Ok(files) = self.get_files() {
						contents.push(ClipboardContent::Files(files));
					}
				}
				ContentFormat::Other(mime) => {
					if let Ok(data) = self.get_buffer(mime) {
						contents.push(ClipboardContent::Other(mime.clone(), data));
					}
				}
			}
		}
		Ok(contents)
	}

	fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()> {
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(buffer.into_boxed_slice()),
			mime_type: MimeType::Specific(format.to_string()),
		}])
	}

	fn set_text(&self, text: String) -> Result<()> {
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(text.into_bytes().into_boxed_slice()),
			mime_type: MimeType::Text,
		}])
	}

	fn set_rich_text(&self, text: String) -> Result<()> {
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(text.into_bytes().into_boxed_slice()),
			mime_type: MimeType::Specific(MIME_RTF.to_string()),
		}])
	}

	fn set_html(&self, html: String) -> Result<()> {
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(html.into_bytes().into_boxed_slice()),
			mime_type: MimeType::Specific(MIME_HTML.to_string()),
		}])
	}

	#[cfg(feature = "image")]
	fn set_image(&self, image: RustImageData) -> Result<()> {
		let bytes = image.to_png()?;
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(bytes.get_bytes().into()),
			mime_type: MimeType::Specific(MIME_PNG.to_string()),
		}])
	}

	#[cfg(feature = "image")]
	fn set_image_with_dib(&self, image: RustImageData, dib_data: Option<&[u8]>) -> Result<()> {
		if dib_data.is_some() {
			return Err("CF_DIB is only supported on Windows".into());
		}
		self.set_image(image)
	}

	fn set_files(&self, files: Vec<String>) -> Result<()> {
		// Normalize paths to file:// URIs and use CRLF for text/uri-list
		let uri_list: Vec<String> = files
			.into_iter()
			.map(|f| {
				if f.starts_with(FILE_PATH_PREFIX) {
					f
				} else {
					format!("{FILE_PATH_PREFIX}{f}")
				}
			})
			.collect();
		let data = uri_list.join("\r\n");
		write_wayland_clipboard(vec![MimeSource {
			source: Source::Bytes(data.into_bytes().into_boxed_slice()),
			mime_type: MimeType::Specific(MIME_URI_LIST.to_string()),
		}])
	}

	fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
		let mut sources = Vec::new();
		for content in contents {
			match content {
				ClipboardContent::Text(text) => {
					sources.push(MimeSource {
						source: Source::Bytes(text.into_bytes().into_boxed_slice()),
						mime_type: MimeType::Text,
					});
				}
				ClipboardContent::Html(html) => {
					sources.push(MimeSource {
						source: Source::Bytes(html.into_bytes().into_boxed_slice()),
						mime_type: MimeType::Specific(MIME_HTML.to_string()),
					});
				}
				ClipboardContent::Rtf(rtf) => {
					sources.push(MimeSource {
						source: Source::Bytes(rtf.into_bytes().into_boxed_slice()),
						mime_type: MimeType::Specific(MIME_RTF.to_string()),
					});
				}
				#[cfg(feature = "image")]
				ClipboardContent::Image(image) => {
					let bytes = image.to_png()?;
					sources.push(MimeSource {
						source: Source::Bytes(bytes.get_bytes().into()),
						mime_type: MimeType::Specific(MIME_PNG.to_string()),
					});
				}
				ClipboardContent::Files(files) => {
					let uri_list: Vec<String> = files
						.into_iter()
						.map(|f| {
							if f.starts_with(FILE_PATH_PREFIX) {
								f
							} else {
								format!("{FILE_PATH_PREFIX}{f}")
							}
						})
						.collect();
					let data = uri_list.join("\r\n");
					sources.push(MimeSource {
						source: Source::Bytes(data.into_bytes().into_boxed_slice()),
						mime_type: MimeType::Specific(MIME_URI_LIST.to_string()),
					});
				}
				ClipboardContent::Other(mime, data) => {
					sources.push(MimeSource {
						source: Source::Bytes(data.into_boxed_slice()),
						mime_type: MimeType::Specific(mime),
					});
				}
			}
		}
		write_wayland_clipboard(sources)
	}
}

// Polling-based clipboard watcher for Wayland
pub struct ClipboardWatcherContext<T: ClipboardHandler> {
	pub(crate) handlers: Vec<T>,
	pub(crate) stop_signal: Sender<()>,
	stop_receiver: Receiver<()>,
	interval: Duration,
}

/// Default polling interval. The Wayland backend has no change-notification
/// protocol, so it polls the clipboard; this is the wait between polls.
const DEFAULT_WATCH_INTERVAL: Duration = Duration::from_millis(500);

impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
	pub fn new() -> Result<Self> {
		Self::new_with_interval(DEFAULT_WATCH_INTERVAL)
	}

	/// Creates a watcher that polls the clipboard every `interval`.
	///
	/// Wayland exposes no clipboard-change notification, so changes are detected
	/// by polling. A smaller `interval` lowers detection latency at the cost of
	/// more frequent wake-ups; `new` uses [`DEFAULT_WATCH_INTERVAL`].
	pub fn new_with_interval(interval: Duration) -> Result<Self> {
		// Verify data-control protocol is available (same check as ClipboardContext)
		is_primary_selection_supported()
			.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
		let (tx, rx) = mpsc::channel();
		Ok(Self {
			handlers: Vec::new(),
			stop_signal: tx,
			stop_receiver: rx,
			interval,
		})
	}
}

impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
	pub(crate) fn get_shutdown_channel(&self) -> WatcherShutdown {
		WatcherShutdown {
			sender: self.stop_signal.clone(),
		}
	}

	pub(crate) fn start_watch_inner(&mut self) {
		let mut last_mime_types: Vec<String> = Vec::new();
		let mut last_text = String::new();

		// Get initial clipboard state
		if let Ok(types) = get_offered_mime_types() {
			last_mime_types = {
				let mut v: Vec<String> = types.into_iter().collect();
				v.sort();
				v
			};
		}
		if let Ok(text) = read_wayland_clipboard(paste::MimeType::Text) {
			if let Ok(s) = String::from_utf8(text) {
				last_text = s;
			}
		}

		loop {
			if self.stop_receiver.recv_timeout(self.interval).is_ok() {
				break;
			}

			let changed = if let Ok(types) = get_offered_mime_types() {
				let mut current_types: Vec<String> = types.into_iter().collect();
				current_types.sort();

				if current_types != last_mime_types {
					// MIME types changed, clipboard definitely changed
					last_mime_types = current_types;
					// Update text cache too
					if let Ok(bytes) = read_wayland_clipboard(paste::MimeType::Text) {
						if let Ok(s) = String::from_utf8(bytes) {
							last_text = s;
						}
					} else {
						last_text.clear();
					}
					true
				} else if let Ok(bytes) = read_wayland_clipboard(paste::MimeType::Text) {
					// Same MIME types, check if text content changed
					if let Ok(current) = String::from_utf8(bytes) {
						if current != last_text {
							last_text = current;
							true
						} else {
							false
						}
					} else {
						false
					}
				} else if !last_text.is_empty() {
					last_text.clear();
					true
				} else {
					false
				}
			} else {
				// No clipboard content available
				if !last_mime_types.is_empty() {
					last_mime_types.clear();
					last_text.clear();
					true
				} else {
					false
				}
			};

			if changed {
				self.handlers
					.iter_mut()
					.for_each(|handler| handler.on_clipboard_change());
			}
		}
	}
}

pub struct WatcherShutdown {
	pub(crate) sender: Sender<()>,
}

impl Drop for WatcherShutdown {
	fn drop(&mut self) {
		let _ = self.sender.send(());
	}
}
