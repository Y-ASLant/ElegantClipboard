#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
pub use ios::{ClipboardContext, ClipboardWatcherContext, WatcherShutdown};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{ClipboardContext, ClipboardWatcherContext, WatcherShutdown};
#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "windows")]
pub use win::{
	ClipboardAccessError, ClipboardContext, ClipboardFormatUnsupportedError,
	ClipboardWatcherContext, WatcherShutdown,
};

// Linux: runtime detection between Wayland and X11
#[cfg(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	))
))]
mod x11;

#[cfg(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	)),
	feature = "wayland"
))]
mod wayland;

#[cfg(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	))
))]
pub use x11::ClipboardContextX11Options;

#[cfg(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	))
))]
mod linux_clipboard {
	#[cfg(feature = "image")]
	use crate::RustImageData;
	use crate::{common::Result, Clipboard, ClipboardContent, ClipboardHandler, ContentFormat};

	pub enum ClipboardContext {
		X11(super::x11::ClipboardContext),
		#[cfg(feature = "wayland")]
		Wayland(super::wayland::ClipboardContext),
	}

	impl ClipboardContext {
		pub fn new() -> Result<Self> {
			#[cfg(feature = "wayland")]
			{
				if std::env::var_os("WAYLAND_DISPLAY").is_some() {
					match super::wayland::ClipboardContext::new() {
						Ok(ctx) => return Ok(Self::Wayland(ctx)),
						Err(e) => {
							eprintln!("Wayland clipboard init failed, falling back to X11: {}", e);
						}
					}
				}
			}
			Ok(Self::X11(super::x11::ClipboardContext::new()?))
		}

		/// Creates an X11 context with explicit read options.
		pub fn new_with_options(options: super::x11::ClipboardContextX11Options) -> Result<Self> {
			Ok(Self::X11(super::x11::ClipboardContext::new_with_options(
				options,
			)?))
		}
	}

	macro_rules! dispatch {
		($self:expr, $method:ident $(, $arg:expr)*) => {
			match $self {
				ClipboardContext::X11(ctx) => ctx.$method($($arg),*),
				#[cfg(feature = "wayland")]
				ClipboardContext::Wayland(ctx) => ctx.$method($($arg),*),
			}
		};
	}

	impl Clipboard for ClipboardContext {
		fn available_formats(&self) -> Result<Vec<String>> {
			dispatch!(self, available_formats)
		}

		fn has(&self, format: ContentFormat) -> bool {
			dispatch!(self, has, format)
		}

		fn clear(&self) -> Result<()> {
			dispatch!(self, clear)
		}

		fn get_buffer(&self, format: &str) -> Result<Vec<u8>> {
			dispatch!(self, get_buffer, format)
		}

		fn get_text(&self) -> Result<String> {
			dispatch!(self, get_text)
		}

		fn get_rich_text(&self) -> Result<String> {
			dispatch!(self, get_rich_text)
		}

		fn get_html(&self) -> Result<String> {
			dispatch!(self, get_html)
		}

		#[cfg(feature = "image")]
		fn get_image(&self) -> Result<RustImageData> {
			dispatch!(self, get_image)
		}

		#[cfg(feature = "image")]
		fn get_image_dib(&self) -> Result<(RustImageData, Option<Vec<u8>>)> {
			dispatch!(self, get_image_dib)
		}

		fn get_files(&self) -> Result<Vec<String>> {
			dispatch!(self, get_files)
		}

		fn get(&self, formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
			dispatch!(self, get, formats)
		}

		fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()> {
			dispatch!(self, set_buffer, format, buffer)
		}

		fn set_text(&self, text: String) -> Result<()> {
			dispatch!(self, set_text, text)
		}

		fn set_rich_text(&self, text: String) -> Result<()> {
			dispatch!(self, set_rich_text, text)
		}

		fn set_html(&self, html: String) -> Result<()> {
			dispatch!(self, set_html, html)
		}

		#[cfg(feature = "image")]
		fn set_image(&self, image: RustImageData) -> Result<()> {
			dispatch!(self, set_image, image)
		}

		#[cfg(feature = "image")]
		fn set_image_with_dib(&self, image: RustImageData, dib_data: Option<&[u8]>) -> Result<()> {
			dispatch!(self, set_image_with_dib, image, dib_data)
		}

		fn set_files(&self, files: Vec<String>) -> Result<()> {
			dispatch!(self, set_files, files)
		}

		fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
			dispatch!(self, set, contents)
		}
	}

	pub enum ClipboardWatcherContext<T: ClipboardHandler> {
		X11(super::x11::ClipboardWatcherContext<T>),
		#[cfg(feature = "wayland")]
		Wayland(super::wayland::ClipboardWatcherContext<T>),
	}

	impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
		pub fn new() -> Result<Self> {
			#[cfg(feature = "wayland")]
			{
				if std::env::var_os("WAYLAND_DISPLAY").is_some() {
					match super::wayland::ClipboardWatcherContext::new() {
						Ok(ctx) => return Ok(Self::Wayland(ctx)),
						Err(e) => {
							eprintln!("Wayland watcher init failed, falling back to X11: {}", e);
						}
					}
				}
			}
			Ok(Self::X11(super::x11::ClipboardWatcherContext::new()?))
		}

		/// Creates a watcher with a custom poll `interval`, selecting the Wayland
		/// or X11 backend the same way as [`Self::new`].
		pub fn new_with_interval(interval: std::time::Duration) -> Result<Self> {
			#[cfg(feature = "wayland")]
			{
				if std::env::var_os("WAYLAND_DISPLAY").is_some() {
					match super::wayland::ClipboardWatcherContext::new_with_interval(interval) {
						Ok(ctx) => return Ok(Self::Wayland(ctx)),
						Err(e) => {
							eprintln!("Wayland watcher init failed, falling back to X11: {}", e);
						}
					}
				}
			}
			Ok(Self::X11(
				super::x11::ClipboardWatcherContext::new_with_interval(interval)?,
			))
		}
	}

	impl<T: ClipboardHandler + Send> crate::ClipboardWatcher<T> for ClipboardWatcherContext<T> {
		fn add_handler(&mut self, f: T) -> &mut Self {
			match self {
				Self::X11(ctx) => {
					ctx.handlers.push(f);
				}
				#[cfg(feature = "wayland")]
				Self::Wayland(ctx) => {
					ctx.handlers.push(f);
				}
			}
			self
		}

		fn start_watch(&mut self) {
			match self {
				Self::X11(ctx) => ctx.start_watch_inner(),
				#[cfg(feature = "wayland")]
				Self::Wayland(ctx) => ctx.start_watch_inner(),
			}
		}

		fn get_shutdown_channel(&self) -> WatcherShutdown {
			match self {
				Self::X11(ctx) => WatcherShutdown {
					_shutdown: Box::new(ctx.get_shutdown_channel()),
				},
				#[cfg(feature = "wayland")]
				Self::Wayland(ctx) => WatcherShutdown {
					_shutdown: Box::new(ctx.get_shutdown_channel()),
				},
			}
		}
	}

	pub struct WatcherShutdown {
		_shutdown: Box<dyn std::any::Any + Send>,
	}

	impl WatcherShutdown {
		pub fn stop(self) {
			drop(self);
		}
	}
}

#[cfg(all(
	unix,
	not(any(
		target_os = "macos",
		target_os = "ios",
		target_os = "android",
		target_os = "emscripten"
	))
))]
pub use linux_clipboard::{ClipboardContext, ClipboardWatcherContext, WatcherShutdown};
