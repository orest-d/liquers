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

/// Apply gamma correction.
/// Gamma > 1.0 darkens, gamma < 1.0 brightens.
pub fn gamma(state: &State<Value>, gamma_value: f32) -> Result<Value, Error> {
    if gamma_value <= 0.0 {
        return Err(Error::general_error(
            "Gamma value must be greater than 0".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let rgba_img = Arc::as_ref(&img).to_rgba8();

    // Manual gamma correction: out = in^(1/gamma)
    use image::{Rgba, RgbaImage};
    let mut result = RgbaImage::new(rgba_img.width(), rgba_img.height());

    let gamma_inv = 1.0 / gamma_value;

    for (x, y, pixel) in rgba_img.enumerate_pixels() {
        let r = ((pixel[0] as f32 / 255.0).powf(gamma_inv) * 255.0) as u8;
        let g = ((pixel[1] as f32 / 255.0).powf(gamma_inv) * 255.0) as u8;
        let b = ((pixel[2] as f32 / 255.0).powf(gamma_inv) * 255.0) as u8;
        let a = pixel[3];

        result.put_pixel(x, y, Rgba([r, g, b, a]));
    }

    Ok(Value::from_image(Arc::new(image::DynamicImage::ImageRgba8(result))))
}

/// Adjust color saturation.
/// Factor of 1.0 = no change, > 1.0 = more saturated, < 1.0 = less saturated.
pub fn saturate(state: &State<Value>, factor: f32) -> Result<Value, Error> {
    if factor < 0.0 {
        return Err(Error::general_error(
            "Saturation factor must be non-negative".to_string(),
        ));
    }

    let img = try_to_image(state)?;
    let rgba_img = Arc::as_ref(&img).to_rgba8();

    // Convert to HSV, adjust saturation, convert back
    use image::{Rgba, RgbaImage};

    let mut result = RgbaImage::new(rgba_img.width(), rgba_img.height());

    for (x, y, pixel) in rgba_img.enumerate_pixels() {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;
        let a = pixel[3];

        // Convert RGB to HSV
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let s = if max == 0.0 { 0.0 } else { delta / max };
        let v = max;

        // Adjust saturation
        let s_adjusted = (s * factor).min(1.0);

        // Convert back to RGB
        let c = v * s_adjusted;
        let x_val = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r_out, g_out, b_out) = if h < 60.0 {
            (c, x_val, 0.0)
        } else if h < 120.0 {
            (x_val, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x_val)
        } else if h < 240.0 {
            (0.0, x_val, c)
        } else if h < 300.0 {
            (x_val, 0.0, c)
        } else {
            (c, 0.0, x_val)
        };

        let r_final = ((r_out + m) * 255.0) as u8;
        let g_final = ((g_out + m) * 255.0) as u8;
        let b_final = ((b_out + m) * 255.0) as u8;

        result.put_pixel(x, y, Rgba([r_final, g_final, b_final, a]));
    }

    Ok(Value::from_image(Arc::new(image::DynamicImage::ImageRgba8(result))))
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

    #[test]
    fn test_gamma() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Darken (gamma > 1)
        let result = gamma(&state, 2.0).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Brighten (gamma < 1)
        let result = gamma(&state, 0.5).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Invalid gamma (zero or negative)
        assert!(gamma(&state, 0.0).is_err());
        assert!(gamma(&state, -0.5).is_err());
    }

    #[test]
    fn test_saturate() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));

        // Increase saturation
        let result = saturate(&state, 1.5).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Decrease saturation
        let result = saturate(&state, 0.5).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // No change
        let result = saturate(&state, 1.0).unwrap();
        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (10, 10));

        // Invalid saturation (negative)
        assert!(saturate(&state, -0.5).is_err());
    }
}
