use image::{self, ImageEncoder};
use resvg::usvg::{Options, Tree};
use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;
use usvg::Transform;


/// A simple RGBA raster image with f32 channels per pixel.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RasterImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<(f32, f32, f32, f32)>, // (r, g, b, a) for each pixel
}

impl RasterImage {
    /// Load a RasterImage from a PNG file.
    pub fn from_png(path: &str) -> image::ImageResult<Self> {
        let img = image::open(path)?.to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                (
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                )
            })
            .collect();
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Load a RasterImage from PNG bytes.
    pub fn from_png_bytes(bytes: &[u8]) -> image::ImageResult<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                (
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                )
            })
            .collect();
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Load a RasterImage from an SVG file, rendered at the given size.
    pub fn from_svg(path: &str, width: u32, height: u32) -> Result<Self, String> {
        // Read SVG data
        let svg_data = std::fs::read(path).map_err(|e| e.to_string())?;
        let opt = Options::default();
        let rtree = Tree::from_data(&svg_data, &opt).map_err(|e| format!("{:?}", e))?;

        // Render SVG to pixmap
        let mut pixmap = Pixmap::new(width, height).ok_or("Failed to create pixmap")?;
        resvg::render(&rtree, Transform::default(), &mut pixmap.as_mut());

        // Convert pixmap to RasterImage
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for p in pixmap.pixels() {
            let r = p.red();
            let g = p.green();
            let b = p.blue();
            let a = p.alpha();
            pixels.push((
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ));
        }
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Render the image in egui at the given zoom factor.
    pub fn show(&self, ui: &mut egui::Ui, id: egui::Id, zoom: f32) {
        // Convert to egui::ColorImage
        let mut rgba_u8: Vec<u8> = Vec::with_capacity(self.width * self.height * 4);
        for &(r, g, b, a) in &self.pixels {
            rgba_u8.push((r.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((g.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((b.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((a.clamp(0.0, 1.0) * 255.0) as u8);
        }
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([self.width, self.height], &rgba_u8);

        let texture = ui.ctx().load_texture(
            format!("raster_image_{:?}", id),
            color_image,
            Default::default(),
        );

        let size = egui::Vec2::new(self.width as f32 * zoom, self.height as f32 * zoom);
        let im = egui::Image::from_texture(&texture).fit_to_exact_size(size);
        ui.add(im);
    }

    /// Encode the RasterImage as PNG and return the bytes.
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, image::ImageError> {
        use image::{codecs::png::PngEncoder, ColorType, Rgba, RgbaImage};
        let mut img = RgbaImage::new(self.width as u32, self.height as u32);
        for (i, &(r, g, b, a)) in self.pixels.iter().enumerate() {
            let x = (i % self.width) as u32;
            let y = (i / self.width) as u32;
            img.put_pixel(
                x,
                y,
                Rgba([
                    (r.clamp(0.0, 1.0) * 255.0) as u8,
                    (g.clamp(0.0, 1.0) * 255.0) as u8,
                    (b.clamp(0.0, 1.0) * 255.0) as u8,
                    (a.clamp(0.0, 1.0) * 255.0) as u8,
                ]),
            );
        }
        let mut buf = Vec::new();
        let encoder = PngEncoder::new(&mut buf);
        encoder.write_image(
            &img,
            self.width as u32,
            self.height as u32,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(buf)
    }

    /// Create a new empty RasterImage with the specified dimensions and fill color.
    /// The color can be any type that implements Into<(f32, f32, f32, f32)>.
    pub fn new_filled<T: Into<(f32, f32, f32, f32)>>(
        width: usize,
        height: usize,
        color: T,
    ) -> Self {
        let color = color.into();
        let pixels = vec![color; width * height];
        Self {
            width,
            height,
            pixels,
        }
    }
}

