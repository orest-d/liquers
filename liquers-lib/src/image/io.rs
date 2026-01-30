use liquers_core::{error::Error, state::State, value::ValueInterface};
use crate::value::{ExtValueInterface, Value};
use std::sync::Arc;
use image::DynamicImage;
use super::util::{normalize_format, format_to_image_format};

/// Load image from bytes with auto-detected format.
/// Input state should contain binary data (bytes).
pub fn from_bytes(state: &State<Value>) -> Result<Value, Error> {
    let bytes = state.data.try_into_bytes()?;

    let img = image::load_from_memory(&bytes).map_err(|e| {
        Error::general_error(format!("Failed to load image from bytes: {}", e))
    })?;

    Ok(Value::from_image(Arc::new(img)))
}

/// Load image from bytes with explicitly specified format.
/// Input state should contain binary data (bytes).
pub fn from_format(state: &State<Value>, format_str: String) -> Result<Value, Error> {
    let bytes = state.data.try_into_bytes()?;

    let normalized_format = normalize_format(&format_str)?;
    let img_format = format_to_image_format(&normalized_format)?;

    let img = image::load_from_memory_with_format(&bytes, img_format).map_err(|e| {
        Error::general_error(format!(
            "Failed to load image from bytes as {}: {}",
            format_str, e
        ))
    })?;

    Ok(Value::from_image(Arc::new(img)))
}

/// Render SVG to raster image.
/// Input state should contain SVG content as string or bytes.
/// Width and height specify the output dimensions in pixels.
pub fn svg_to_image(state: &State<Value>, width: u32, height: u32) -> Result<Value, Error> {
    if width == 0 || height == 0 {
        return Err(Error::general_error(
            "SVG dimensions must be greater than 0".to_string(),
        ));
    }

    // Try to get SVG content as string or bytes
    let svg_data = if let Ok(text) = state.try_into_string() {
        text.into_bytes()
    } else if let Ok(bytes) = state.as_bytes() {
        bytes
    } else {
        return Err(Error::general_error(
            "svg_to_image expects string or binary input containing SVG data".to_string(),
        ));
    };

    // Parse SVG
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&svg_data, &options).map_err(|e| {
        Error::general_error(format!("Failed to parse SVG: {}", e))
    })?;

    // Render to pixmap
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        Error::general_error("Failed to create pixmap for SVG rendering".to_string())
    })?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // Convert pixmap to DynamicImage
    let img = image::RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .ok_or_else(|| Error::general_error("Failed to convert pixmap to image".to_string()))?;

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(img))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::{state::State, value::ValueInterface};

    #[test]
    fn test_from_bytes_png() {
        // Create a small 1x1 red PNG
        let mut png_data = Vec::new();
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png)
            .unwrap();

        let state = State::new().with_data(Value::from_bytes(png_data));
        let result = from_bytes(&state).unwrap();

        let img_arc = result.as_image().unwrap();
        let (w, h) = Arc::as_ref(&img_arc).dimensions();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_from_format_jpeg() {
        // Create a small 1x1 red JPEG
        let mut jpeg_data = Vec::new();
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg_data),
                image::ImageFormat::Jpeg,
            )
            .unwrap();

        let state = State::new().with_data(Value::from_bytes(jpeg_data));
        let result = from_format(&state, "jpeg".to_string()).unwrap();

        let img_arc = result.as_image().unwrap();
        let (w, h) = Arc::as_ref(&img_arc).dimensions();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_svg_to_image() {
        let svg_content = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <rect width="100" height="100" fill="red"/>
        </svg>"#;

        let state = State::new().with_data(Value::from(svg_content.to_string()));
        let result = svg_to_image(&state, 100, 100).unwrap();

        let img_arc = result.as_image().unwrap();
        let (w, h) = Arc::as_ref(&img_arc).dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 100);
    }

    #[test]
    fn test_svg_to_image_invalid_dimensions() {
        let svg_content = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>"#;
        let state = State::new().with_data(Value::from(svg_content.to_string()));

        assert!(svg_to_image(&state, 0, 100).is_err());
        assert!(svg_to_image(&state, 100, 0).is_err());
    }
}
