use anyhow::{Result, anyhow};
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use png::{ColorType, Decoder, Transformations};
use std::{fs::File, io::BufReader, path::Path, sync::Arc};

/// Full RGBA decode above this size is the list-scroll spike. Stream instead.
const STREAM_BYTES: u64 = 4 * 1024 * 1024;

/// `Ok(None)` asks the caller to use the generic decoder (small, interlaced, or odd layout).
pub(super) fn decode(path: &Path, max: (u32, u32)) -> Result<Option<Arc<RenderImage>>> {
    let mut decoder = Decoder::new(BufReader::new(File::open(path)?));
    decoder.set_transformations(Transformations::normalize_to_color8() | Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|error| anyhow!(error))?;
    if reader.info().interlaced {
        return Ok(None);
    }
    let (color, _) = reader.output_color_type();
    if color != ColorType::Rgba {
        return Ok(None);
    }
    let width = reader.info().width;
    let height = reader.info().height;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) * 4 <= STREAM_BYTES {
        return Ok(None);
    }
    let (out_w, out_h) = fit(width, height, max.0.max(1), max.1.max(1));
    let mut pixels = downsample(&mut reader, width, height, out_w, out_h)?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)]))))
}

fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let ratio = f64::min(
        f64::from(max_w) / f64::from(width),
        f64::from(max_h) / f64::from(height),
    );
    let out_w = (f64::from(width) * ratio).round().max(1.0) as u32;
    let out_h = (f64::from(height) * ratio).round().max(1.0) as u32;
    (out_w.min(max_w), out_h.min(max_h))
}

fn downsample(
    reader: &mut png::Reader<BufReader<File>>,
    width: u32,
    height: u32,
    out_w: u32,
    out_h: u32,
) -> Result<RgbaImage> {
    let x_ratio = width as f32 / out_w as f32;
    let y_ratio = height as f32 / out_h as f32;
    let mut image = RgbaImage::new(out_w, out_h);
    let mut src_y = 0u32;
    for out_y in 0..out_h {
        let bottom = (out_y as f32 * y_ratio).ceil() as u32;
        let top = (((out_y as f32 * y_ratio) + y_ratio).ceil() as u32).clamp(bottom + 1, height);
        let mut sums = vec![[0u64; 4]; out_w as usize];
        let mut rows = 0u64;
        while src_y < top {
            let Some(row) = reader.next_row().map_err(|error| anyhow!(error))? else {
                anyhow::bail!("PNG ended before row {src_y}");
            };
            if src_y >= bottom {
                add_row(row.data(), width, x_ratio, &mut sums);
                rows += 1;
            }
            src_y += 1;
        }
        let rows = rows.max(1);
        for (out_x, sum) in sums.iter().enumerate() {
            let samples = rows * x_span(out_x as u32, width, x_ratio).2;
            let pixel = image.get_pixel_mut(out_x as u32, out_y);
            for (channel, total) in sum.iter().enumerate() {
                pixel.0[channel] = ((total + samples / 2) / samples) as u8;
            }
        }
    }
    Ok(image)
}

fn add_row(row: &[u8], width: u32, x_ratio: f32, sums: &mut [[u64; 4]]) {
    for (out_x, sum) in sums.iter_mut().enumerate() {
        let (left, right, _) = x_span(out_x as u32, width, x_ratio);
        for x in left..right {
            let pixel = &row[x as usize * 4..x as usize * 4 + 4];
            for (channel, byte) in pixel.iter().enumerate() {
                sum[channel] += u64::from(*byte);
            }
        }
    }
}

fn x_span(out_x: u32, width: u32, x_ratio: f32) -> (u32, u32, u64) {
    let left = (out_x as f32 * x_ratio).ceil() as u32;
    let right = (((out_x as f32 * x_ratio) + x_ratio).ceil() as u32).clamp(left + 1, width);
    (left, right, u64::from(right - left))
}
