use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;
use image::DynamicImage;

/// Morphological erosion (shrink bright regions).
/// Radius is in pixels.
/// Note: Simplified implementation using distance transform.
pub fn erode(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Erosion radius must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let gray_img = Arc::as_ref(&img).to_luma8();

    // Manual erosion using minimum filter
    use image::{Luma, GrayImage};
    let (width, height) = gray_img.dimensions();
    let mut result = GrayImage::new(width, height);

    let r = radius as i32;
    for y in 0..height {
        for x in 0..width {
            let mut min_val = 255u8;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    min_val = min_val.min(gray_img.get_pixel(nx, ny)[0]);
                }
            }
            result.put_pixel(x, y, Luma([min_val]));
        }
    }

    Ok(Value::from_image(Arc::new(DynamicImage::ImageLuma8(result))))
}

/// Morphological dilation (expand bright regions).
/// Radius is in pixels.
pub fn dilate(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Dilation radius must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let gray_img = Arc::as_ref(&img).to_luma8();

    // Manual dilation using maximum filter
    use image::{Luma, GrayImage};
    let (width, height) = gray_img.dimensions();
    let mut result = GrayImage::new(width, height);

    let r = radius as i32;
    for y in 0..height {
        for x in 0..width {
            let mut max_val = 0u8;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                    max_val = max_val.max(gray_img.get_pixel(nx, ny)[0]);
                }
            }
            result.put_pixel(x, y, Luma([max_val]));
        }
    }

    Ok(Value::from_image(Arc::new(DynamicImage::ImageLuma8(result))))
}

/// Morphological opening (erode then dilate, remove noise).
/// Radius is in pixels.
pub fn opening(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Opening radius must be greater than 0".to_string(),
        ));
    }

    // Opening = erosion followed by dilation
    let eroded_state = erode(state, radius)?;
    let eroded_image_state = State::new().with_data(eroded_state);
    dilate(&eroded_image_state, radius)
}

/// Morphological closing (dilate then erode, fill holes).
/// Radius is in pixels.
pub fn closing(state: &State<Value>, radius: u32) -> Result<Value, Error> {
    if radius == 0 {
        return Err(Error::general_error(
            "Closing radius must be greater than 0".to_string(),
        ));
    }

    // Closing = dilation followed by erosion
    let dilated_state = dilate(state, radius)?;
    let dilated_image_state = State::new().with_data(dilated_state);
    erode(&dilated_image_state, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::state::State;

    fn create_test_image() -> DynamicImage {
        // Create a test pattern with some bright and dark regions
        let mut img = image::GrayImage::new(20, 20);
        // Fill with a pattern: bright square in the middle
        for y in 5..15 {
            for x in 5..15 {
                img.put_pixel(x, y, image::Luma([255]));
            }
        }
        DynamicImage::ImageLuma8(img)
    }

    #[test]
    fn test_erode() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = erode(&state, 2).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_erode_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(erode(&state, 0).is_err());
    }

    #[test]
    fn test_dilate() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = dilate(&state, 2).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_dilate_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(dilate(&state, 0).is_err());
    }

    #[test]
    fn test_opening() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = opening(&state, 2).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_opening_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(opening(&state, 0).is_err());
    }

    #[test]
    fn test_closing() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = closing(&state, 2).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (20, 20));
    }

    #[test]
    fn test_closing_invalid_radius() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(closing(&state, 0).is_err());
    }
}
