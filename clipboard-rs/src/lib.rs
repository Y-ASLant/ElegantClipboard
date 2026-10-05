pub mod common;
mod platform;
#[cfg(feature = "image")]
pub use common::RustImageData;
pub use common::{ClipboardContent, ClipboardHandler, ContentFormat, Result};
#[cfg(feature = "image")]
pub use image::imageops::FilterType;
#[cfg(target_os = "linux")]
pub use platform::ClipboardContextX11Options;
#[cfg(target_os = "windows")]
pub use platform::{ClipboardAccessError, ClipboardFormatUnsupportedError};
pub use platform::{ClipboardContext, ClipboardWatcherContext, WatcherShutdown};

pub trait Clipboard: Send {
	/// zh: 获得剪切板当前内容的所有格式
	/// en: Get all formats of the current content in the clipboard
	fn available_formats(&self) -> Result<Vec<String>>;

	fn has(&self, format: ContentFormat) -> bool;

	/// zh: 清空剪切板
	/// en: clear clipboard
	fn clear(&self) -> Result<()>;

	/// zh: 获得指定格式的数据，以字节数组形式返回
	/// en: Get the data in the specified format in the clipboard as a byte array
	fn get_buffer(&self, format: &str) -> Result<Vec<u8>>;

	/// zh: 仅获得无格式纯文本，以字符串形式返回
	/// en: Get plain text content in the clipboard as string
	fn get_text(&self) -> Result<String>;

	/// zh: 获得剪贴板中的富文本内容，以字符串形式返回
	/// en: Get the rich text content in the clipboard as string
	fn get_rich_text(&self) -> Result<String>;

	/// zh: 获得剪贴板中的html内容，以字符串形式返回
	/// en: Get the html format content in the clipboard as string
	fn get_html(&self) -> Result<String>;

	#[cfg(feature = "image")]
	fn get_image(&self) -> Result<RustImageData>;

	/// Like `get_image`, but also returns the raw Windows CF_DIB bytes (if available)
	/// for lossless round-trips. Other platforms return the decoded image and `None`.
	#[cfg(feature = "image")]
	fn get_image_dib(&self) -> Result<(RustImageData, Option<Vec<u8>>)>;

	fn get_files(&self) -> Result<Vec<String>>;

	fn get(&self, formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>>;

	/// Windows rejects empty raw buffers before changing the clipboard.
	fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()>;

	fn set_text(&self, text: String) -> Result<()>;

	fn set_rich_text(&self, text: String) -> Result<()>;

	fn set_html(&self, html: String) -> Result<()>;

	#[cfg(feature = "image")]
	fn set_image(&self, image: RustImageData) -> Result<()>;

	/// Like `set_image`, but also writes the original Windows CF_DIB bytes (if provided).
	/// Other platforms reject `Some` DIB data without modifying the clipboard.
	#[cfg(feature = "image")]
	fn set_image_with_dib(&self, image: RustImageData, dib_data: Option<&[u8]>) -> Result<()>;

	fn set_files(&self, files: Vec<String>) -> Result<()>;

	/// set image will clear clipboard
	fn set(&self, contents: Vec<ClipboardContent>) -> Result<()>;
}

pub trait ClipboardWatcher<T: ClipboardHandler>: Send {
	/// zh: 添加一个剪切板变化处理器，可以添加多个处理器，处理器需要实现 [`ClipboardHandler`] 这个trait
	/// en: Add a clipboard change handler, you can add multiple handlers, the handler needs to implement the trait [`ClipboardHandler`]
	fn add_handler(&mut self, handler: T) -> &mut Self;

	/// zh: 开始监视剪切板变化，这是一个阻塞方法，直到监视结束，或者调用了stop方法，所以建议在单独的线程中调用
	/// en: Start monitoring clipboard changes, this is a blocking method, until the monitoring ends, or the stop method is called, so it is recommended to call it in a separate thread
	fn start_watch(&mut self);

	/// zh: 获得停止监视的通道，可以通过这个通道停止监视
	/// en: Get the channel to stop monitoring, you can stop monitoring through this channel
	fn get_shutdown_channel(&self) -> WatcherShutdown;
}

#[cfg(not(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	))
)))]
impl WatcherShutdown {
	/// zh: 停止监视
	///
	/// en: stop watching
	pub fn stop(self) {
		drop(self);
	}
}
