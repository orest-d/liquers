use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;
use image::DynamicImage;

/// Apply Sobel edge detection.
/// Returns horizontal + vertical edges combined.
pub fn sobel(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let gray_img = Arc::as_ref(&img).to_luma8();

    // Sobel edge detection - compute horizontal and vertical gradients
    let horizontal = imageproc::gradients::horizontal_sobel(&gray_img);
    let vertical = imageproc::gradients::vertical_sobel(&gray_img);

    // Combine gradients: magnitude = sqrt(h^2 + v^2)
    use image::{Luma, GrayImage};
    let (width, height) = gray_img.dimensions();
    let mut result = GrayImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let h = horizontal.get_pixel(x, y)[0] as f32;
            let v = vertical.get_pixel(x, y)[0] as f32;
            let magnitude = ((h * h + v * v).sqrt() as u8).min(255);
            result.put_pixel(x, y, Luma([magnitude]));
        }
    }

    Ok(Value::from_image(Arc::new(DynamicImage::ImageLuma8(result))))
}

/// Apply Canny edge detection with low and high thresholds.
pub fn canny(state: &State<Value>, low_threshold: f32, high_threshold: f32) -> Result<Value, Error> {
    if low_threshold < 0.0 || high_threshold < 0.0 {
        return Err(Error::general_error(
            "Canny thresholds must be non-negative".to_string(),
        ));
    }

    if low_threshold >= high_threshold {
        return Err(Error::general_error(
            "Canny low threshold must be less than high threshold".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let gray_img = Arc::as_ref(&img).to_luma8();

    let result = imageproc::edges::canny(&gray_img, low_threshold, high_threshold);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageLuma8(result))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::state::State;

    fn create_test_image() -> DynamicImage {
        // Create a test pattern with edges
        let mut img = image::GrayImage::new(50, 50);
        // Create a simple edge pattern
        for y in 0..50 {
            for x in 0..50 {
                if x > 20 && x < 30 {
                    img.put_pixel(x, y, image::Luma([255]));
                } else {
                    img.put_pixel(x, y, image::Luma([0]));
                }
            }
        }
        DynamicImage::ImageLuma8(img)
    }

    #[test]
    fn test_sobel() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = sobel(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 50));
    }

    #[test]
    fn test_canny() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = canny(&state, 50.0, 150.0).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 50));
    }

    #[test]
    fn test_canny_invalid_thresholds() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Negative thresholds
        assert!(canny(&state, -10.0, 150.0).is_err());
        assert!(canny(&state, 50.0, -10.0).is_err());

        // Low >= high
        assert!(canny(&state, 150.0, 50.0).is_err());
        assert!(canny(&state, 100.0, 100.0).is_err());
    }
}
