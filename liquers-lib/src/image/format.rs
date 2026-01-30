use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::{try_to_image, normalize_format, format_to_image_format, format_to_mime_type};
use std::sync::Arc;
use image::DynamicImage;

/// Convert image to PNG format (returns PNG bytes).
pub fn to_png(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let mut buffer = Vec::new();
    Arc::as_ref(&img).write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Png,
    )
    .map_err(|e| Error::general_error(format!("Failed to encode PNG: {}", e)))?;

    Ok(Value::Base(crate::value::simple::SimpleValue::Bytes { value: buffer }))
}

/// Convert image to JPEG format with specified quality (1-100).
/// Returns JPEG bytes.
pub fn to_jpeg(state: &State<Value>, quality: u8) -> Result<Value, Error> {
    if quality == 0 || quality > 100 {
        return Err(Error::general_error(
            "JPEG quality must be between 1 and 100".to_string(),
        ));
    }

    let img = try_to_image(state)?;

    // Convert to RGB if needed (JPEG doesn't support alpha)
    let rgb_img = Arc::as_ref(&img).to_rgb8();

    let mut buffer = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::io::Cursor::new(&mut buffer),
        quality,
    );

    rgb_img
        .write_with_encoder(encoder)
        .map_err(|e| Error::general_error(format!("Failed to encode JPEG: {}", e)))?;

    Ok(Value::Base(crate::value::simple::SimpleValue::Bytes { value: buffer }))
}

/// Convert image to WebP format (returns WebP bytes).
pub fn to_webp(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let mut buffer = Vec::new();
    Arc::as_ref(&img).write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::WebP,
    )
    .map_err(|e| Error::general_error(format!("Failed to encode WebP: {}", e)))?;

    Ok(Value::Base(crate::value::simple::SimpleValue::Bytes { value: buffer }))
}

/// Convert image to GIF format (returns GIF bytes).
pub fn to_gif(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let mut buffer = Vec::new();
    Arc::as_ref(&img).write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Gif,
    )
    .map_err(|e| Error::general_error(format!("Failed to encode GIF: {}", e)))?;

    Ok(Value::Base(crate::value::simple::SimpleValue::Bytes { value: buffer }))
}

/// Convert image to BMP format (returns BMP bytes).
pub fn to_bmp(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let mut buffer = Vec::new();
    Arc::as_ref(&img).write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Bmp,
    )
    .map_err(|e| Error::general_error(format!("Failed to encode BMP: {}", e)))?;

    Ok(Value::Base(crate::value::simple::SimpleValue::Bytes { value: buffer }))
}

/// Convert image to base64 data URL with specified format.
/// Format should be a file extension like "png", "jpeg", "webp", etc.
pub fn to_dataurl(state: &State<Value>, format_str: String) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let normalized_format = normalize_format(&format_str)?;
    let img_format = format_to_image_format(&normalized_format)?;
    let mime_type = format_to_mime_type(&normalized_format)?;

    let mut buffer = Vec::new();
    Arc::as_ref(&img).write_to(&mut std::io::Cursor::new(&mut buffer), img_format)
        .map_err(|e| Error::general_error(format!("Failed to encode image: {}", e)))?;

    let base64_data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buffer);
    let data_url = format!("data:{};base64,{}", mime_type, base64_data);

    Ok(Value::from(data_url))
}

/// Convert image color format.
/// Supported formats: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16
pub fn color_format(state: &State<Value>, format: String) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let result: DynamicImage = match format.as_str() {
        "rgb8" => Arc::as_ref(&img).to_rgb8().into(),
        "rgba8" => Arc::as_ref(&img).to_rgba8().into(),
        "luma8" => Arc::as_ref(&img).to_luma8().into(),
        "luma_alpha8" => Arc::as_ref(&img).to_luma_alpha8().into(),
        "rgb16" => Arc::as_ref(&img).to_rgb16().into(),
        "rgba16" => Arc::as_ref(&img).to_rgba16().into(),
        _ => {
            return Err(Error::general_error(format!(
                "Invalid color format '{}'. Supported: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16",
                format
            )))
        }
    };

    Ok(Value::from_image(Arc::new(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{state::State, value::ValueInterface};

    fn create_test_image() -> DynamicImage {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_to_png() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_png(&state).unwrap();

        let bytes = result.try_into_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Verify PNG header
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_to_jpeg() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_jpeg(&state, 85).unwrap();

        // Verify it's bytes via State
        let bytes = result.try_into_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Verify JPEG header
        assert_eq!(&bytes[0..2], &[255, 216]); // JPEG SOI marker
    }

    #[test]
    fn test_to_jpeg_invalid_quality() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(to_jpeg(&state, 0).is_err());
        assert!(to_jpeg(&state, 101).is_err());
    }

    #[test]
    fn test_to_webp() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_webp(&state).unwrap();

        let bytes = result.try_into_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Verify WebP header (RIFF...WEBP)
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn test_to_gif() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_gif(&state).unwrap();

        let bytes = result.try_into_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Verify GIF header
        assert!(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"));
    }

    #[test]
    fn test_to_bmp() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_bmp(&state).unwrap();

        let bytes = result.try_into_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Verify BMP header
        assert_eq!(&bytes[0..2], b"BM");
    }

    #[test]
    fn test_to_dataurl() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = to_dataurl(&state, "png".to_string()).unwrap();

        let dataurl = result.try_into_string().unwrap();
        assert!(dataurl.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_color_format() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Test RGB8
        let result = color_format(&state, "rgb8".to_string()).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).color(), image::ColorType::Rgb8);

        // Test RGBA8
        let result = color_format(&state, "rgba8".to_string()).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).color(), image::ColorType::Rgba8);

        // Test Luma8
        let result = color_format(&state, "luma8".to_string()).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).color(), image::ColorType::L8);

        // Test invalid format
        assert!(color_format(&state, "invalid".to_string()).is_err());
    }
}
