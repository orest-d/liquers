use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;
use image::DynamicImage;

/// Apply Gaussian blur with specified sigma value.
pub fn blur(state: &State<Value>, sigma: f32) -> Result<Value, Error> {
    if sigma < 0.0 {
        return Err(Error::general_error("Blur sigma must be non-negative".to_string()));
    }

    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).blur(sigma);
    Ok(Value::from_image(Arc::new(result)))
}

/// Sharpen image.
/// Uses a 3x3 sharpening kernel.
pub fn sharpen(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    // Use imageproc's filter3x3 with a sharpening kernel
    // Standard sharpening kernel:
    // [ 0 -1  0]
    // [-1  5 -1]
    // [ 0 -1  0]
    let rgba_img = Arc::as_ref(&img).to_rgba8();
    let result = imageproc::filter::filter3x3(&rgba_img, &[
        0.0, -1.0, 0.0,
        -1.0, 5.0, -1.0,
        0.0, -1.0, 0.0,
    ]);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(result))))
}

/// Apply median filter for noise reduction.
/// Radius is in pixels.
pub fn median(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Median filter radius must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let rgba_img = Arc::as_ref(&img).to_rgba8();
    let result = imageproc::filter::median_filter(&rgba_img, radius, radius);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(result))))
}

/// Apply box/mean filter (blur by averaging).
/// Radius is in pixels.
pub fn boxfilt(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Box filter radius must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let gray_img = Arc::as_ref(&img).to_luma8();

    // box_filter returns Luma image
    let result = imageproc::filter::box_filter(&gray_img, radius, radius);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageLuma8(result))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::state::State;

    fn create_test_image() -> DynamicImage {
        // Create a simple test pattern
        let img = image::RgbaImage::from_pixel(20, 20, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_blur() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = blur(&state, 2.0).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_blur_zero_sigma() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Zero sigma should still work
        let result = blur(&state, 0.0).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_blur_negative_sigma() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(blur(&state, -1.0).is_err());
    }

    #[test]
    fn test_sharpen() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = sharpen(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_median() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = median(&state, 2).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_median_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(median(&state, 0).is_err());
    }

    #[test]
    fn test_boxfilt() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = boxfilt(&state, 3).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_boxfilt_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(boxfilt(&state, 0).is_err());
    }
}
