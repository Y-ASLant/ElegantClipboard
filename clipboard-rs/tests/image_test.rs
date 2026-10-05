#![cfg(feature = "image")]

use clipboard_rs::{
	common::{RustImage, RustImageData},
	Clipboard, ClipboardContext, ContentFormat,
};
use image::{DynamicImage, GrayAlphaImage, Rgba, RgbaImage};

const TEST_PNG: &[u8] = include_bytes!("test.png");

#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn test_image() {
	let ctx = ClipboardContext::new().unwrap();
	let original = RustImageData::from_bytes(TEST_PNG).unwrap();
	let original_pixels = original.to_rgba8().unwrap();
	ctx.set_image(original).unwrap();
	assert!(ctx.has(ContentFormat::Image));
	assert_eq!(
		ctx.get_image().unwrap().to_rgba8().unwrap(),
		original_pixels
	);
	ctx.clear().unwrap();
}

#[test]
fn png_round_trip_preserves_dimensions_and_pixels() {
	let original = RustImageData::from_bytes(TEST_PNG).unwrap();
	let encoded = original.to_png().unwrap();
	let decoded = RustImageData::from_bytes(encoded.get_bytes()).unwrap();
	assert_eq!(decoded.get_size(), original.get_size());
	assert_eq!(decoded.to_rgba8().unwrap(), original.to_rgba8().unwrap());
}

#[test]
fn jpeg_encoding_accepts_grayscale_alpha() {
	let image = RustImageData::from_dynamic_image(DynamicImage::ImageLumaA8(
		GrayAlphaImage::from_pixel(2, 2, image::LumaA([127, 64])),
	));
	let jpeg = image.to_jpeg().unwrap();
	let decoded = RustImageData::from_bytes(jpeg.get_bytes()).unwrap();
	assert_eq!(decoded.get_size(), (2, 2));
}

#[test]
fn png_encoding_preserves_alpha() {
	let pixels = RgbaImage::from_pixel(2, 2, Rgba([32, 64, 128, 16]));
	let image = RustImageData::from_dynamic_image(DynamicImage::ImageRgba8(pixels.clone()));
	let png = image.to_png().unwrap();
	assert_eq!(
		RustImageData::from_bytes(png.get_bytes())
			.unwrap()
			.to_rgba8()
			.unwrap(),
		pixels
	);
}

#[test]
fn empty_images_and_invalid_bytes_return_errors() {
	let image = RustImageData::empty();
	assert!(image.is_empty());
	assert!(image.to_png().is_err());
	assert!(image.to_jpeg().is_err());
	assert!(image.thumbnail(10, 10).is_err());
	assert!(RustImageData::from_bytes(b"not an image").is_err());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires exclusive access to the system clipboard; run with --ignored --test-threads=1"]
fn dib_round_trip_and_mixed_formats_preserve_pixels_and_payloads() {
	let ctx = ClipboardContext::new().unwrap();
	let pixels = RgbaImage::from_pixel(1, 1, Rgba([32, 64, 128, 16]));
	let original = RustImageData::from_dynamic_image(DynamicImage::ImageRgba8(pixels.clone()));
	let bitmap = original.to_bitmap().unwrap();
	let dib = &bitmap.get_bytes()[14..];
	ctx.set_image_with_dib(original, Some(dib)).unwrap();
	assert!(clipboard_win::is_format_avail(
		clipboard_win::formats::CF_BITMAP
	));
	let (image, raw_dib) = ctx.get_image_dib().unwrap();
	assert_eq!(image.to_rgba8().unwrap(), pixels);
	assert_eq!(raw_dib.as_deref(), Some(dib));

	let text = "mixed formats 中文 😀";
	ctx.set(vec![
		clipboard_rs::ClipboardContent::Text(text.to_string()),
		clipboard_rs::ClipboardContent::Image(RustImageData::from_dynamic_image(
			DynamicImage::ImageRgba8(pixels.clone()),
		)),
		clipboard_rs::ClipboardContent::Rtf("{\\rtf1 mixed formats}".to_string()),
	])
	.unwrap();
	let contents = ctx
		.get(&[
			ContentFormat::Text,
			ContentFormat::Image,
			ContentFormat::Rtf,
		])
		.unwrap();
	assert_eq!(contents.len(), 3);
	assert_eq!(ctx.get_text().unwrap(), text);
	assert_eq!(ctx.get_image().unwrap().to_rgba8().unwrap(), pixels);
	assert_eq!(ctx.get_rich_text().unwrap(), "{\\rtf1 mixed formats}");
	ctx.clear().unwrap();
}
