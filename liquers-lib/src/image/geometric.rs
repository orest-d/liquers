use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::try_to_image;
use std::sync::Arc;
use image::{DynamicImage, GenericImageView, imageops::FilterType};

/// Parse filter type from string.
fn parse_filter_type(method: &str) -> Result<FilterType, Error> {
    match method {
        "nearest" => Ok(FilterType::Nearest),
        "triangle" => Ok(FilterType::Triangle),
        "catmullrom" => Ok(FilterType::CatmullRom),
        "gaussian" => Ok(FilterType::Gaussian),
        "lanczos3" => Ok(FilterType::Lanczos3),
        _ => Err(Error::general_error(format!(
            "Invalid resize method '{}'. Supported: nearest, triangle, catmullrom, gaussian, lanczos3",
            method
        ))),
    }
}

/// Resize image to exact dimensions in pixels.
pub fn resize(
    state: &State<Value>,
    width: u32,
    height: u32,
    method: String,
) -> Result<Value, Error> {
    if width == 0 || height == 0 {
        return Err(Error::general_error(
            "Image dimensions must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let filter = parse_filter_type(&method)?;
    let result = Arc::as_ref(&img).resize_exact(width, height, filter);

    Ok(Value::from_image(Arc::new(result)))
}

/// Resize image by percentage (uniform scaling).
pub fn resize_by(state: &State<Value>, percent: f32, method: String) -> Result<Value, Error> {
    if percent <= 0.0 {
        return Err(Error::general_error(
            "Resize percentage must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let filter = parse_filter_type(&method)?;

    let (width, height) = Arc::as_ref(&img).dimensions();
    let scale = percent / 100.0;
    let new_width = ((width as f32 * scale).round() as u32).max(1);
    let new_height = ((height as f32 * scale).round() as u32).max(1);

    let result = Arc::as_ref(&img).resize_exact(new_width, new_height, filter);

    Ok(Value::from_image(Arc::new(result)))
}

/// Resize image preserving aspect ratio (thumbnail).
/// Ensures the image fits within the specified dimensions.
pub fn thumb(
    state: &State<Value>,
    max_width: u32,
    max_height: u32,
    method: String,
) -> Result<Value, Error> {
    if max_width == 0 || max_height == 0 {
        return Err(Error::general_error(
            "Thumbnail dimensions must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let filter = parse_filter_type(&method)?;
    let result = Arc::as_ref(&img).thumbnail(max_width, max_height);

    Ok(Value::from_image(Arc::new(result)))
}

/// Crop image to rectangle (x, y, width, height).
pub fn crop(
    state: &State<Value>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Value, Error> {
    if width == 0 || height == 0 {
        return Err(Error::general_error("Crop dimensions must be greater than 0".to_string()));
    }

    let img = try_to_image(state)?;
    let (img_width, img_height) = Arc::as_ref(&img).dimensions();

    // Validate crop bounds
    if x + width > img_width || y + height > img_height {
        return Err(Error::general_error(format!(
            "Crop region ({},{},{},{}) exceeds image bounds ({}x{})",
            x, y, width, height, img_width, img_height
        )));
    }

    // crop() requires a mutable DynamicImage, so we clone it
    let mut img_copy = Arc::try_unwrap(img).unwrap_or_else(|arc| (*arc).clone());
    let result = img_copy.crop(x, y, width, height);

    Ok(Value::from_image(Arc::new(result)))
}

/// Rotate image by arbitrary angle in degrees (clockwise is positive).
/// Uses bilinear interpolation by default.
pub fn rotate(state: &State<Value>, angle: f32) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    let dim = Arc::as_ref(&img).dimensions();
    let center = (dim.0 as f32 / 2.0, dim.1 as f32 / 2.0);
    // Note: imageproc rotation is counter-clockwise, so we negate the angle
    let result = imageproc::geometric_transformations::rotate(
        &Arc::as_ref(&img).to_rgba8(),
        center,
        (-angle).to_radians(),
        imageproc::geometric_transformations::Interpolation::Bilinear,
        image::Rgba([0, 0, 0, 0]),
    );

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(result))))
}

/// Rotate image 90 degrees clockwise.
pub fn rot90(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).rotate90();
    Ok(Value::from_image(Arc::new(result)))
}

/// Rotate image 180 degrees.
pub fn rot180(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).rotate180();
    Ok(Value::from_image(Arc::new(result)))
}

/// Rotate image 270 degrees clockwise (90 degrees counter-clockwise).
pub fn rot270(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).rotate270();
    Ok(Value::from_image(Arc::new(result)))
}

/// Flip image horizontally.
pub fn fliph(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).fliph();
    Ok(Value::from_image(Arc::new(result)))
}

/// Flip image vertically.
pub fn flipv(state: &State<Value>) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let result = Arc::as_ref(&img).flipv();
    Ok(Value::from_image(Arc::new(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::state::State;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_resize() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = resize(&state, 50, 50, "lanczos3".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 50));
    }

    #[test]
    fn test_resize_invalid_dimensions() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(resize(&state, 0, 50, "lanczos3".to_string()).is_err());
        assert!(resize(&state, 50, 0, "lanczos3".to_string()).is_err());
    }

    #[test]
    fn test_resize_by() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = resize_by(&state, 50.0, "lanczos3".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 50));
    }

    #[test]
    fn test_resize_by_invalid_percent() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(resize_by(&state, 0.0, "lanczos3".to_string()).is_err());
        assert!(resize_by(&state, -10.0, "lanczos3".to_string()).is_err());
    }

    #[test]
    fn test_thumb() {
        let img = create_test_image(200, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = thumb(&state, 100, 100, "lanczos3".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        let (w, h) = Arc::as_ref(&result_img).dimensions();
        // Should preserve aspect ratio: 2:1 becomes 100:50
        assert_eq!(w, 100);
        assert!(h <= 100);
    }

    #[test]
    fn test_crop() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = crop(&state, 10, 10, 50, 50).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 50));
    }

    #[test]
    fn test_crop_out_of_bounds() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        assert!(crop(&state, 90, 90, 20, 20).is_err());
        assert!(crop(&state, 0, 0, 200, 50).is_err());
    }

    #[test]
    fn test_rot90() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = rot90(&state).unwrap();

        let result_img = result.as_image().unwrap();
        // 90 degree rotation swaps width and height
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 100));
    }

    #[test]
    fn test_rot180() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = rot180(&state).unwrap();

        let result_img = result.as_image().unwrap();
        // 180 degree rotation preserves dimensions
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 50));
    }

    #[test]
    fn test_rot270() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = rot270(&state).unwrap();

        let result_img = result.as_image().unwrap();
        // 270 degree rotation swaps width and height
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (50, 100));
    }

    #[test]
    fn test_fliph() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = fliph(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 50));
    }

    #[test]
    fn test_flipv() {
        let img = create_test_image(100, 50);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = flipv(&state).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 50));
    }

    #[test]
    fn test_rotate() {
        let img = create_test_image(100, 100);
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = rotate(&state, 45.0).unwrap();

        let result_img = result.as_image().unwrap();
        // Rotated image will be larger to fit the rotated content
        let (w, h) = Arc::as_ref(&result_img).dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 100);
    }
}
