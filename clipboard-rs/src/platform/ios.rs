use crate::{
	common::Result, Clipboard, ClipboardContent, ClipboardHandler, ClipboardWatcher, ContentFormat,
};
#[cfg(feature = "image")]
use crate::{common::RustImage, RustImageData};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{ns_string, NSArray, NSData, NSDictionary, NSString};
use objc2_ui_kit::UIPasteboard;
#[cfg(feature = "image")]
use objc2_ui_kit::{UIImage, UIImagePNGRepresentation};
use std::{
	sync::mpsc::{self, Receiver, Sender},
	time::Duration,
};

pub struct ClipboardContext {
	clipboard: Retained<UIPasteboard>,
}
pub struct ClipboardWatcherContext<T: ClipboardHandler> {
	clipboard: Retained<UIPasteboard>,
	handlers: Vec<T>,
	running: bool,
	stop_signal: Sender<()>,
	stop_receiver: Receiver<()>,
	interval: Duration,
}

impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
	pub fn new() -> Result<Self> {
		Self::new_with_interval(Duration::from_millis(500))
	}

	/// Creates a watcher that polls the pasteboard at the supplied interval.
	pub fn new_with_interval(interval: Duration) -> Result<Self> {
		let clipboard = unsafe { UIPasteboard::generalPasteboard() };
		let (tx, rx) = mpsc::channel();
		Ok(Self {
			clipboard,
			handlers: Vec::new(),
			running: false,
			stop_signal: tx,
			stop_receiver: rx,
			interval,
		})
	}
}

unsafe impl<T: ClipboardHandler + Send> Send for ClipboardWatcherContext<T> {}

impl<T: ClipboardHandler + Send> ClipboardWatcher<T> for ClipboardWatcherContext<T> {
	fn add_handler(&mut self, handler: T) -> &mut Self {
		self.handlers.push(handler);
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
		let mut last_change_count = unsafe { self.clipboard.changeCount() };
		loop {
			// if receive stop signal, break loop
			if self.stop_receiver.recv_timeout(self.interval).is_ok() {
				break;
			}
			let change_count = unsafe { self.clipboard.changeCount() };
			if last_change_count == 0 {
				last_change_count = change_count;
			} else if change_count != last_change_count {
				self.handlers
					.iter_mut()
					.for_each(|handler| handler.on_clipboard_change());
				last_change_count = change_count;
			}
		}
		self.running = false;
	}

	fn get_shutdown_channel(&self) -> WatcherShutdown {
		WatcherShutdown {
			stop_signal: self.stop_signal.clone(),
		}
	}
}

pub struct WatcherShutdown {
	stop_signal: Sender<()>,
}

impl Drop for WatcherShutdown {
	fn drop(&mut self) {
		let _ = self.stop_signal.send(());
	}
}

impl ClipboardContext {
	pub fn new() -> Result<Self> {
		let clipboard = unsafe { UIPasteboard::generalPasteboard() };
		Ok(Self { clipboard })
	}

	fn write_to_clipboard(&self, data: &[ClipboardContent]) -> Result<()> {
		let items = data
			.iter()
			.map(|content| match content {
				ClipboardContent::Text(text) => {
					let ns_text = NSString::from_str(text);
					let pair = unsafe {
						NSDictionary::dictionaryWithObject_forKey(
							ns_text.as_ref(),
							ProtocolObject::from_ref(ns_string!("public.utf8-plain-text")),
						)
					};
					Some(pair)
				}
				ClipboardContent::Rtf(rtf) => {
					let ns_rtf = NSString::from_str(rtf);
					let pair = unsafe {
						NSDictionary::dictionaryWithObject_forKey(
							ns_rtf.as_ref(),
							ProtocolObject::from_ref(ns_string!("public.rtf")),
						)
					};
					Some(pair)
				}
				ClipboardContent::Html(html) => {
					let ns_html = NSString::from_str(html);
					let pair = unsafe {
						NSDictionary::dictionaryWithObject_forKey(
							ns_html.as_ref(),
							ProtocolObject::from_ref(ns_string!("public.html")),
						)
					};
					Some(pair)
				}
				#[cfg(feature = "image")]
				ClipboardContent::Image(image) => {
					let png = image.to_png().unwrap();
					let ns_data = NSData::with_bytes(png.get_bytes());
					let image = unsafe { UIImage::imageWithData(&ns_data) };
					if let Some(image) = image {
						let pair = unsafe {
							NSDictionary::dictionaryWithObject_forKey(
								image.as_ref(),
								ProtocolObject::from_ref(ns_string!("public.png")),
							)
						};
						Some(pair)
					} else {
						None
					}
				}
				_ => None,
			})
			.filter_map(|item| item)
			.collect::<Vec<_>>();
		unsafe {
			self.clipboard
				.setItems(&NSArray::from_retained_slice(&items))
		};
		Ok(())
	}
}

impl Clipboard for ClipboardContext {
	fn available_formats(&self) -> Result<Vec<String>> {
		let formats = unsafe { self.clipboard.pasteboardTypes() };
		Ok(formats.iter().map(|f| f.to_string()).collect())
	}

	fn has(&self, format: ContentFormat) -> bool {
		match format {
			ContentFormat::Text => unsafe { self.clipboard.hasStrings() },
			#[cfg(feature = "image")]
			ContentFormat::Image => unsafe { self.clipboard.hasImages() },
			ContentFormat::Rtf => unsafe {
				self.clipboard
					.containsPasteboardTypes(&NSArray::from_slice(&[ns_string!("public.rtf")]))
			},
			ContentFormat::Html => unsafe {
				self.clipboard
					.containsPasteboardTypes(&NSArray::from_slice(&[ns_string!("public.html")]))
			},
			ContentFormat::Files => false,
			ContentFormat::Other(format) => unsafe {
				self.clipboard
					.containsPasteboardTypes(&NSArray::from_retained_slice(&[NSString::from_str(
						&format,
					)]))
			},
		}
	}

	fn clear(&self) -> Result<()> {
		unsafe { self.clipboard.setItems(&NSArray::from_slice(&[])) };
		Ok(())
	}

	fn get_buffer(&self, format: &str) -> Result<Vec<u8>> {
		let ns_format = NSString::from_str(format);
		let data = unsafe { self.clipboard.dataForPasteboardType(&ns_format) };
		if let Some(data) = data {
			Ok(data.to_vec())
		} else {
			Err("No data found".into())
		}
	}

	fn get_text(&self) -> Result<String> {
		let text = unsafe { self.clipboard.string() };
		if let Some(text) = text {
			Ok(text.to_string())
		} else {
			Err("No text found".into())
		}
	}

	fn get_rich_text(&self) -> Result<String> {
		let buffer = self.get_buffer("public.rtf")?;
		Ok(String::from_utf8_lossy(&buffer).to_string())
	}

	fn get_html(&self) -> Result<String> {
		let buffer = self.get_buffer("public.html")?;
		Ok(String::from_utf8_lossy(&buffer).to_string())
	}

	#[cfg(feature = "image")]
	fn get_image(&self) -> Result<RustImageData> {
		let image = unsafe { self.clipboard.image() };
		if let Some(image) = image {
			let data = unsafe { UIImagePNGRepresentation(&image) };
			if let Some(data) = data {
				let bytes = unsafe { data.as_bytes_unchecked() };
				Ok(RustImageData::from_bytes(bytes)?)
			} else {
				Err("No image data found".into())
			}
		} else {
			Err("No image found".into())
		}
	}

	#[cfg(feature = "image")]
	fn get_image_dib(&self) -> Result<(RustImageData, Option<Vec<u8>>)> {
		let img = self.get_image()?;
		Ok((img, None))
	}

	fn get_files(&self) -> Result<Vec<String>> {
		Err("Not supported".into())
	}

	fn get(&self, _formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
		Err("Not supported".into())
	}

	fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()> {
		let ns_format = NSString::from_str(format);
		let ns_data = NSData::with_bytes(&buffer);
		unsafe {
			self.clipboard
				.setData_forPasteboardType(&ns_data, &ns_format)
		}
		Ok(())
	}

	fn set_text(&self, text: String) -> Result<()> {
		unsafe {
			self.clipboard
				.setString(Some(&NSString::from_str(text.as_str())))
		};
		Ok(())
	}

	fn set_rich_text(&self, _text: String) -> Result<()> {
		Err("Not supported".into())
	}

	fn set_html(&self, _html: String) -> Result<()> {
		Err("Not supported".into())
	}

	#[cfg(feature = "image")]
	fn set_image(&self, image: RustImageData) -> Result<()> {
		if image.is_empty() {
			Err("Image is empty".into())
		} else {
			let png = image.to_png()?;
			let ns_data = NSData::with_bytes(png.get_bytes());
			let image = unsafe { UIImage::imageWithData(&ns_data) };
			if let Some(image) = image {
				unsafe { self.clipboard.setImage(Some(&image)) };
				Ok(())
			} else {
				Err("Failed to create image".into())
			}
		}
	}

	#[cfg(feature = "image")]
	fn set_image_with_dib(&self, image: RustImageData, dib_data: Option<&[u8]>) -> Result<()> {
		if dib_data.is_some() {
			return Err("CF_DIB is only supported on Windows".into());
		}
		self.set_image(image)
	}

	fn set_files(&self, _files: Vec<String>) -> Result<()> {
		Err("Not supported".into())
	}

	fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
		self.write_to_clipboard(&contents)?;
		Ok(())
	}
}
