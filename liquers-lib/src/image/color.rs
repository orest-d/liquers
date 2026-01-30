use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;

/// Convert image to grayscale.
pub fn gray(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).grayscale();
    Ok(Value::from_image(Arc::new(result)))
}

/// Invert image colors (negative).
pub fn invert(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let mut result = Arc::as_ref(&img).clone();
    result.invert();
    Ok(Value::from_image(Arc::new(result)))
}

/// Adjust image brightness.
/// Positive values brighten, negative values darken.
/// Note: The framework decodes ~10 to -10, so the function receives the actual i32 value.
pub fn brighten(state: &State<Value>, value: i32) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).brighten(value);
    Ok(Value::from_image(Arc::new(result)))
}

/// Adjust image contrast.
/// Positive values increase contrast, negative values decrease contrast.
pub fn contrast(state: &State<Value>, value: f32) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).adjust_contrast(value);
    Ok(Value::from_image(Arc::new(result)))
}

/// Rotate hue by specified degrees.
/// Value should be in range -180 to 180.
pub fn huerot(state: &State<Value>, value: i32) -> Result<Value, Error> {
    if value < -180 || value > 180 {
        return Err(Error::general_error(
            "Hue rotation value must be between -180 and 180 degrees".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).huerotate(value);
    Ok(Value::from_image(Arc::new(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView};
    use liquers_core::state::State;

    fn create_test_image() -> DynamicImage {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_gray() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = gray(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));
        // Grayscale should have L8 or LA8 color type
        let color = Arc::as_ref(&result_img).color();
        assert!(
            color == image::ColorType::L8 || color == image::ColorType::La8,
            "Expected grayscale color type, got {:?}",
            color
        );
    }

    #[test]
    fn test_invert() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = invert(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Check that the color is inverted
        let pixel = Arc::as_ref(&result_img).get_pixel(0, 0);
        // Red [255, 0, 0, 255] should become cyan [0, 255, 255, 255]
        assert_eq!(pixel[0], 0);
        assert_eq!(pixel[1], 255);
        assert_eq!(pixel[2], 255);
    }

    #[test]
    fn test_brighten() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Brighten
        let result = brighten(&state, 50).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Darken (negative value)
        let result = brighten(&state, -50).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));
    }

    #[test]
    fn test_contrast() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Increase contrast
        let result = contrast(&state, 50.0).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Decrease contrast
        let result = contrast(&state, -50.0).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));
    }

    #[test]
    fn test_huerot() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Rotate hue
        let result = huerot(&state, 120).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Negative rotation
        let result = huerot(&state, -90).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));
    }

    #[test]
    fn test_huerot_invalid_range() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(huerot(&state, 181).is_err());
        assert!(huerot(&state, -181).is_err());
    }
}
