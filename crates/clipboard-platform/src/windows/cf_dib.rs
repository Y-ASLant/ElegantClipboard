//! 24-bit BI_RGB `CF_DIB`, encoded before a paste or drag consumer asks for it.
//!
//! QQ reads `CF_DIB` on its UI thread. A 32-bit BI_RGB bitmap stores alpha in
//! the high byte, which that path treats as a corrupt bitmap and freezes on.
//! A deferred bitmap blocks the same thread until decoding finishes.
use anyhow::{Context, Result, bail};
use clipboard_core::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
use image::ImageDecoder;

pub(crate) fn from_png(png: &[u8]) -> Result<Vec<u8>> {
    if png.len() > MAX_IMAGE_BYTES {
        bail!("图片超过 50 MiB，无法生成位图");
    }
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(png))
        .context("无法读取 PNG")?;
    let (width, height) = decoder.dimensions();
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 || pixels > MAX_IMAGE_PIXELS {
        bail!("图片尺寸无效或超过 2500 万像素");
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .context("无法解码 PNG")?
        .to_rgba8();
    encode(&image)
}

pub(crate) fn encode(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let width = image.width();
    let height = image.height();
    let width_usize = usize::try_from(width).context("图片宽度超过位图限制")?;
    let height_usize = usize::try_from(height).context("图片高度超过位图限制")?;
    let stride = width_usize
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(3))
        .map(|bytes| bytes & !3)
        .context("图片行长度溢出")?;
    let image_bytes = stride
        .checked_mul(height_usize)
        .context("图片缓冲长度溢出")?;
    let image_size = u32::try_from(image_bytes).context("图片像素超过位图长度限制")?;
    let width_i32 = i32::try_from(width).context("图片宽度超过位图限制")?;
    let height_i32 = i32::try_from(height).context("图片高度超过位图限制")?;
    let mut dib = Vec::new();
    dib.try_reserve_exact(image_bytes.checked_add(40).context("位图长度溢出")?)
        .context("位图缓冲分配失败")?;
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&width_i32.to_le_bytes());
    dib.extend_from_slice(&height_i32.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&24u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&image_size.to_le_bytes());
    dib.extend_from_slice(&[0; 16]);
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel = image.get_pixel(x, y).0;
            dib.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        let padding = stride - width_usize * 3;
        dib.extend(std::iter::repeat_n(0, padding));
    }
    Ok(dib)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_four_bit_rows_are_bottom_up_bgr_and_dword_aligned() {
        let image = image::RgbaImage::from_raw(
            2,
            2,
            vec![
                0x11, 0x22, 0x33, 0xFF, 0x44, 0x55, 0x66, 0xFF, 0x77, 0x88, 0x99, 0xFF, 0xAA,
                0xBB, 0xCC, 0xFF,
            ],
        )
        .unwrap();
        let dib = encode(&image).unwrap();
        assert_eq!(&dib[..4], &40u32.to_le_bytes());
        assert_eq!(&dib[14..16], &24u16.to_le_bytes());
        assert_eq!(&dib[16..20], &0u32.to_le_bytes());
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(&dib[40..46], &[0x99, 0x88, 0x77, 0xCC, 0xBB, 0xAA]);
        assert_eq!(&dib[46..48], &[0, 0]);
        assert_eq!(&dib[48..54], &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44]);
        assert_eq!(&dib[54..56], &[0, 0]);
    }
}
