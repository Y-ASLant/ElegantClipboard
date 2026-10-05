#[cfg(feature = "image")]
use image::imageops::FilterType;
#[cfg(feature = "image")]
use image::{ColorType, DynamicImage, GenericImageView, ImageFormat, RgbaImage};
use std::error::Error;
#[cfg(feature = "image")]
use std::io::Cursor;
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

pub trait ContentData {
	fn get_format(&self) -> ContentFormat;

	fn as_bytes(&self) -> &[u8];

	fn as_str(&self) -> Result<&str>;
}

pub trait ClipboardHandler {
	fn on_clipboard_change(&mut self);
}

pub enum ClipboardContent {
	Text(String),
	Rtf(String),
	Html(String),
	#[cfg(feature = "image")]
	Image(RustImageData),
	Files(Vec<String>),
	Other(String, Vec<u8>),
}

impl ContentData for ClipboardContent {
	fn get_format(&self) -> ContentFormat {
		match self {
			ClipboardContent::Text(_) => ContentFormat::Text,
			ClipboardContent::Rtf(_) => ContentFormat::Rtf,
			ClipboardContent::Html(_) => ContentFormat::Html,
			#[cfg(feature = "image")]
			ClipboardContent::Image(_) => ContentFormat::Image,
			ClipboardContent::Files(_) => ContentFormat::Files,
			ClipboardContent::Other(format, _) => ContentFormat::Other(format.clone()),
		}
	}

	fn as_bytes(&self) -> &[u8] {
		match self {
			ClipboardContent::Text(data) => data.as_bytes(),
			ClipboardContent::Rtf(data) => data.as_bytes(),
			ClipboardContent::Html(data) => data.as_bytes(),
			// dynamic image is not supported to as bytes
			#[cfg(feature = "image")]
			ClipboardContent::Image(_) => &[],
			ClipboardContent::Files(data) => {
				// use first file path as data
				if let Some(path) = data.first() {
					path.as_bytes()
				} else {
					&[]
				}
			}
			ClipboardContent::Other(_, data) => data.as_slice(),
		}
	}

	fn as_str(&self) -> Result<&str> {
		match self {
			ClipboardContent::Text(data) => Ok(data),
			ClipboardContent::Rtf(data) => Ok(data),
			ClipboardContent::Html(data) => Ok(data),
			#[cfg(feature = "image")]
			ClipboardContent::Image(_) => Err("can't convert image to string".into()),
			ClipboardContent::Files(data) => {
				// use first file path as data
				if let Some(path) = data.first() {
					Ok(path)
				} else {
					Err("content is empty".into())
				}
			}
			ClipboardContent::Other(_, data) => std::str::from_utf8(data).map_err(|e| e.into()),
		}
	}
}

#[derive(Clone)]
pub enum ContentFormat {
	Text,
	Rtf,
	Html,
	#[cfg(feature = "image")]
	Image,
	Files,
	Other(String),
}

#[cfg(feature = "image")]
pub struct RustImageData {
	width: u32,
	height: u32,
	data: Option<DynamicImage>,
}

/// 此处的 `RustImageBuffer` 已经是带有图片格式的字节流，例如 png,jpeg;
#[cfg(feature = "image")]
pub struct RustImageBuffer(Vec<u8>);

#[cfg(feature = "image")]
pub trait RustImage: Sized {
	/// create an empty image
	fn empty() -> Self;

	fn is_empty(&self) -> bool;

	/// Read image from file path
	fn from_path(path: &str) -> Result<Self>;

	/// Create a new image from a byte slice
	fn from_bytes(bytes: &[u8]) -> Result<Self>;

	fn from_dynamic_image(image: DynamicImage) -> Self;

	/// width and height
	fn get_size(&self) -> (u32, u32);

	/// Scale this image down to fit within a specific size.
	/// Returns a new image. The image's aspect ratio is preserved.
	/// The image is scaled to the maximum possible size that fits
	/// within the bounds specified by `nwidth` and `nheight`.
	///
	/// This method uses a fast integer algorithm where each source
	/// pixel contributes to exactly one target pixel.
	/// May give aliasing artifacts if new size is close to old size.
	fn thumbnail(&self, width: u32, height: u32) -> Result<Self>;

	/// en: Adjust the size of the image without retaining the aspect ratio
	/// zh: 调整图片大小，不保留长宽比
	fn resize(&self, width: u32, height: u32, filter: FilterType) -> Result<Self>;

	fn encode_image(
		&self,
		target_color_type: ColorType,
		format: ImageFormat,
	) -> Result<RustImageBuffer>;

	fn to_jpeg(&self) -> Result<RustImageBuffer>;

	/// en: Convert to png format, the returned image is a new image, and the data itself will not be modified
	/// zh: 转为 png 格式,返回的为新的图片，本身数据不会修改
	fn to_png(&self) -> Result<RustImageBuffer>;

	#[cfg(target_os = "windows")]
	fn to_bitmap(&self) -> Result<RustImageBuffer>;

	fn save_to_path(&self, path: &str) -> Result<()>;

	fn get_dynamic_image(&self) -> Result<DynamicImage>;

	fn to_rgba8(&self) -> Result<RgbaImage>;
}

#[cfg(feature = "image")]
impl RustImage for RustImageData {
	fn empty() -> Self {
		RustImageData {
			width: 0,
			height: 0,
			data: None,
		}
	}

	fn is_empty(&self) -> bool {
		self.data.is_none()
	}

	fn from_path(path: &str) -> Result<Self> {
		let image = image::open(path)?;
		let (width, height) = image.dimensions();
		Ok(RustImageData {
			width,
			height,
			data: Some(image),
		})
	}

	fn from_bytes(bytes: &[u8]) -> Result<Self> {
		let image = image::load_from_memory(bytes)?;
		let (width, height) = image.dimensions();
		Ok(RustImageData {
			width,
			height,
			data: Some(image),
		})
	}

	fn from_dynamic_image(image: DynamicImage) -> Self {
		let (width, height) = image.dimensions();
		RustImageData {
			width,
			height,
			data: Some(image),
		}
	}

	fn get_size(&self) -> (u32, u32) {
		(self.width, self.height)
	}

	fn thumbnail(&self, width: u32, height: u32) -> Result<Self> {
		match &self.data {
			Some(image) => {
				let resized = image.thumbnail(width, height);
				Ok(RustImageData {
					width: resized.width(),
					height: resized.height(),
					data: Some(resized),
				})
			}
			None => Err("image is empty".into()),
		}
	}

	fn resize(&self, width: u32, height: u32, filter: FilterType) -> Result<Self> {
		match &self.data {
			Some(image) => {
				let resized = image.resize_exact(width, height, filter);
				Ok(RustImageData {
					width: resized.width(),
					height: resized.height(),
					data: Some(resized),
				})
			}
			None => Err("image is empty".into()),
		}
	}

	fn save_to_path(&self, path: &str) -> Result<()> {
		match &self.data {
			Some(image) => {
				image.save(path)?;
				Ok(())
			}
			None => Err("image is empty".into()),
		}
	}

	fn get_dynamic_image(&self) -> Result<DynamicImage> {
		match &self.data {
			Some(image) => Ok(image.clone()),
			None => Err("image is empty".into()),
		}
	}

	fn to_rgba8(&self) -> Result<RgbaImage> {
		match &self.data {
			Some(image) => Ok(image.to_rgba8()),
			None => Err("image is empty".into()),
		}
	}

	// Convert only when the encoder's target color differs from the source.
	fn encode_image(
		&self,
		target_color_type: ColorType,
		format: ImageFormat,
	) -> Result<RustImageBuffer> {
		let image = self.data.as_ref().ok_or("image is empty")?;

		let mut bytes = Vec::new();
		match (image.color(), target_color_type) {
			(color, ColorType::Rgb8) if color != ColorType::Rgb8 => image
				.to_rgb8()
				.write_to(&mut Cursor::new(&mut bytes), format)?,
			(color, ColorType::Rgba8) if color != ColorType::Rgba8 => image
				.to_rgba8()
				.write_to(&mut Cursor::new(&mut bytes), format)?,
			_ => image.write_to(&mut Cursor::new(&mut bytes), format)?,
		};
		Ok(RustImageBuffer(bytes))
	}

	fn to_jpeg(&self) -> Result<RustImageBuffer> {
		// JPEG 需要 RGB 格式（不支持 alpha 通道）
		self.encode_image(ColorType::Rgb8, ImageFormat::Jpeg)
	}

	fn to_png(&self) -> Result<RustImageBuffer> {
		// PNG 使用 RGBA 格式以支持透明度
		self.encode_image(ColorType::Rgba8, ImageFormat::Png)
	}

	#[cfg(target_os = "windows")]
	fn to_bitmap(&self) -> Result<RustImageBuffer> {
		// BMP 使用 RGBA 格式
		self.encode_image(ColorType::Rgba8, ImageFormat::Bmp)
	}
}

#[cfg(feature = "image")]
impl RustImageBuffer {
	pub fn get_bytes(&self) -> &[u8] {
		&self.0
	}

	pub fn save_to_path(&self, path: &str) -> Result<()> {
		std::fs::write(path, &self.0)?;
		Ok(())
	}
}
