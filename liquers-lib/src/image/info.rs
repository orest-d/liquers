use image::GenericImageView;
use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;

/// Get image dimensions as "WIDTHxHEIGHT" string.
pub fn dims(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let (width, height) = Arc::as_ref(&img).dimensions();
    Ok(Value::from(format!("{}x{}", width, height)))
}

/// Get image width.
pub fn width(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let (width, _) = Arc::as_ref(&img).dimensions();
    Ok(Value::from(width as i64))
}

/// Get image height.
pub fn height(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let (_, height) = Arc::as_ref(&img).dimensions();
    Ok(Value::from(height as i64))
}

/// Get image color type as string.
pub fn colortype(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = Arc::as_ref(&img).color();

    let color_str = match color {
        image::ColorType::L8 => "L8",
        image::ColorType::La8 => "La8",
        image::ColorType::Rgb8 => "Rgb8",
        image::ColorType::Rgba8 => "Rgba8",
        image::ColorType::L16 => "L16",
        image::ColorType::La16 => "La16",
        image::ColorType::Rgb16 => "Rgb16",
        image::ColorType::Rgba16 => "Rgba16",
        image::ColorType::Rgb32F => "Rgb32F",
        image::ColorType::Rgba32F => "Rgba32F",
        _ => "Unknown",
    };

    Ok(Value::from(color_str.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{state::State, value::ValueInterface};
    use std::sync::Arc;
    use image::DynamicImage;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_dims() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = dims(&state).unwrap();

        assert_eq!(result.try_into_string().unwrap(), "100x50");
    }

    #[test]
    fn test_width() {
        let img = create_test_image(123, 456);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = width(&state).unwrap();

        assert_eq!(result.try_into_i64().unwrap(), 123);
    }

    #[test]
    fn test_height() {
        let img = create_test_image(123, 456);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = height(&state).unwrap();

        assert_eq!(result.try_into_i64().unwrap(), 456);
    }

    #[test]
    fn test_colortype() {
        let img = create_test_image(10, 10);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = colortype(&state).unwrap();

        assert_eq!(result.try_into_string().unwrap(), "Rgba8");
    }

    #[test]
    fn test_colortype_rgb8() {
        let rgb_img = image::RgbImage::from_pixel(10, 10, image::Rgb([255, 0, 0]));
        let img = DynamicImage::ImageRgb8(rgb_img);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = colortype(&state).unwrap();

        assert_eq!(result.try_into_string().unwrap(), "Rgb8");
    }

    #[test]
    fn test_colortype_luma8() {
        let luma_img = image::GrayImage::from_pixel(10, 10, image::Luma([128]));
        let img = DynamicImage::ImageLuma8(luma_img);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = colortype(&state).unwrap();

        assert_eq!(result.try_into_string().unwrap(), "L8");
    }
}
