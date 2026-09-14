use image::{DynamicImage, ImageFormat, Rgba, RgbaImage, imageops::FilterType};
use std::{env, fs, io::Cursor, path::PathBuf};

const ICON_SIZE: u32 = 256;
const DRAW_SCALE: u32 = 2;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let icon = image::imageops::resize(&draw_icon(), ICON_SIZE, ICON_SIZE, FilterType::Lanczos3);
    fs::write(output.join("miaozip-icon.rgba"), icon.as_raw()).expect("write RGBA icon");
    icon.save(output.join("miaozip-icon.png"))
        .expect("write icon preview");

    let sizes = [16, 32, 48, 256];
    let images: Vec<Vec<u8>> = sizes
        .map(|size| {
            let resized = image::imageops::resize(&icon, size, size, FilterType::Lanczos3);
            let mut bytes = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(resized)
                .write_to(&mut bytes, ImageFormat::Png)
                .expect("encode ICO image");
            bytes.into_inner()
        })
        .into();
    let icon_path = output.join("miaozip.ico");
    fs::write(&icon_path, encode_ico(&sizes, &images)).expect("write Windows ICO");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon(icon_path.to_str().expect("Unicode icon path"))
            .set("ProductName", "妙压")
            .set("FileDescription", "妙压压缩文件管理器")
            .set("InternalName", "miaozip")
            .set("OriginalFilename", "miaozip.exe")
            .compile()
            .expect("embed Windows executable icon");
    }
}

fn encode_ico(sizes: &[u32], images: &[Vec<u8>]) -> Vec<u8> {
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, bytes) in sizes.iter().zip(images) {
        ico.extend_from_slice(&[if size == 256 { 0 } else { size as u8 }; 2]);
        ico.extend_from_slice(&0u16.to_le_bytes());
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += bytes.len() as u32;
    }
    for bytes in images {
        ico.extend_from_slice(bytes);
    }
    ico
}

struct Canvas(RgbaImage);

impl Canvas {
    fn new() -> Self {
        Self(RgbaImage::from_pixel(
            ICON_SIZE * DRAW_SCALE,
            ICON_SIZE * DRAW_SCALE,
            Rgba([0, 0, 0, 0]),
        ))
    }

    fn pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let destination = self.0.get_pixel_mut(x, y);
        let alpha = color[3] as f32 / 255.0;
        let old_alpha = destination[3] as f32 / 255.0;
        let final_alpha = alpha + old_alpha * (1.0 - alpha);
        if final_alpha == 0.0 {
            return;
        }
        for channel in 0..3 {
            destination[channel] = ((color[channel] as f32 * alpha
                + destination[channel] as f32 * old_alpha * (1.0 - alpha))
                / final_alpha)
                .round() as u8;
        }
        destination[3] = (final_alpha * 255.0).round() as u8;
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 4]) {
        for y in (y0 * DRAW_SCALE as f32) as u32..(y1 * DRAW_SCALE as f32) as u32 {
            for x in (x0 * DRAW_SCALE as f32) as u32..(x1 * DRAW_SCALE as f32) as u32 {
                self.pixel(x, y, color);
            }
        }
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: [u8; 4]) {
        let left = ((cx - rx) * DRAW_SCALE as f32).max(0.0) as u32;
        let top = ((cy - ry) * DRAW_SCALE as f32).max(0.0) as u32;
        let right = ((cx + rx) * DRAW_SCALE as f32) as u32;
        let bottom = ((cy + ry) * DRAW_SCALE as f32) as u32;
        for y in top..bottom {
            for x in left..right {
                let dx = (x as f32 / DRAW_SCALE as f32 - cx) / rx;
                let dy = (y as f32 / DRAW_SCALE as f32 - cy) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    self.pixel(x, y, color);
                }
            }
        }
    }

    fn polygon(&mut self, points: &[(f32, f32)], color: [u8; 4]) {
        let left = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let right = points.iter().map(|p| p.0).fold(0.0, f32::max);
        let top = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let bottom = points.iter().map(|p| p.1).fold(0.0, f32::max);
        for y in (top * DRAW_SCALE as f32) as u32..(bottom * DRAW_SCALE as f32) as u32 {
            for x in (left * DRAW_SCALE as f32) as u32..(right * DRAW_SCALE as f32) as u32 {
                let px = (x as f32 + 0.5) / DRAW_SCALE as f32;
                let py = (y as f32 + 0.5) / DRAW_SCALE as f32;
                let mut inside = false;
                let mut previous = points.len() - 1;
                for current in 0..points.len() {
                    let (ax, ay) = points[current];
                    let (bx, by) = points[previous];
                    if (ay > py) != (by > py) && px < (bx - ax) * (py - ay) / (by - ay) + ax {
                        inside = !inside;
                    }
                    previous = current;
                }
                if inside {
                    self.pixel(x, y, color);
                }
            }
        }
    }
}

fn draw_icon() -> RgbaImage {
    let mut canvas = Canvas::new();
    canvas.ellipse(128.0, 229.0, 105.0, 14.0, [0, 24, 36, 65]);

    // The familiar blue/green/orange archive stack is shared by the title,
    // executable, window and Explorer menu icons.
    canvas.polygon(
        &[(31.0, 55.0), (216.0, 55.0), (229.0, 109.0), (25.0, 109.0)],
        [27, 144, 191, 255],
    );
    canvas.polygon(
        &[(43.0, 42.0), (207.0, 42.0), (216.0, 56.0), (31.0, 56.0)],
        [71, 194, 226, 255],
    );
    canvas.polygon(
        &[(31.0, 56.0), (51.0, 56.0), (45.0, 109.0), (25.0, 109.0)],
        [13, 119, 175, 255],
    );
    canvas.polygon(
        &[(25.0, 108.0), (229.0, 108.0), (227.0, 163.0), (26.0, 163.0)],
        [52, 170, 73, 255],
    );
    canvas.rect(27.0, 111.0, 225.0, 117.0, [104, 202, 94, 255]);
    canvas.rect(26.0, 154.0, 227.0, 163.0, [32, 131, 54, 255]);
    canvas.polygon(
        &[(26.0, 163.0), (227.0, 163.0), (219.0, 220.0), (34.0, 220.0)],
        [219, 95, 48, 255],
    );
    canvas.rect(29.0, 165.0, 225.0, 171.0, [242, 132, 59, 255]);
    canvas.rect(34.0, 210.0, 219.0, 220.0, [164, 61, 37, 255]);

    canvas.polygon(
        &[(108.0, 43.0), (145.0, 43.0), (149.0, 220.0), (105.0, 220.0)],
        [163, 94, 50, 255],
    );
    canvas.rect(116.0, 43.0, 138.0, 220.0, [236, 183, 106, 255]);
    canvas.rect(118.0, 44.0, 123.0, 219.0, [255, 218, 147, 255]);
    canvas.rect(103.0, 132.0, 153.0, 175.0, [116, 68, 40, 255]);
    canvas.rect(110.0, 139.0, 147.0, 168.0, [245, 200, 120, 255]);
    canvas.rect(116.0, 145.0, 141.0, 162.0, [119, 73, 45, 255]);
    canvas.rect(112.0, 141.0, 116.0, 167.0, [255, 235, 165, 255]);

    canvas.0
}
