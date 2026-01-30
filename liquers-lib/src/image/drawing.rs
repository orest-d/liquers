use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use super::util::{try_to_image, parse_color};
use std::sync::Arc;
use image::DynamicImage;
use imageproc::drawing::{
    draw_line_segment_mut, draw_hollow_rect_mut, draw_filled_rect_mut,
    draw_hollow_circle_mut, draw_filled_circle_mut,
};
use imageproc::rect::Rect;

/// Draw a line from (x1, y1) to (x2, y2) with specified color.
pub fn draw_line(
    state: &State<Value>,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color_str: String,
) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    let mut rgba_img = Arc::as_ref(&img).to_rgba8();
    draw_line_segment_mut(&mut rgba_img, (x1 as f32, y1 as f32), (x2 as f32, y2 as f32), color);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(rgba_img))))
}

/// Draw a rectangle outline.
pub fn draw_rect(
    state: &State<Value>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color_str: String,
) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    let mut rgba_img = Arc::as_ref(&img).to_rgba8();
    let rect = Rect::at(x, y).of_size(width, height);
    draw_hollow_rect_mut(&mut rgba_img, rect, color);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(rgba_img))))
}

/// Draw a filled rectangle.
pub fn draw_filled_rect(
    state: &State<Value>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color_str: String,
) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    let mut rgba_img = Arc::as_ref(&img).to_rgba8();
    let rect = Rect::at(x, y).of_size(width, height);
    draw_filled_rect_mut(&mut rgba_img, rect, color);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(rgba_img))))
}

/// Draw a circle outline.
pub fn draw_circle(
    state: &State<Value>,
    x: i32,
    y: i32,
    radius: i32,
    color_str: String,
) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    let mut rgba_img = Arc::as_ref(&img).to_rgba8();
    draw_hollow_circle_mut(&mut rgba_img, (x, y), radius, color);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(rgba_img))))
}

/// Draw a filled circle.
pub fn draw_filled_circle(
    state: &State<Value>,
    x: i32,
    y: i32,
    radius: i32,
    color_str: String,
) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    let mut rgba_img = Arc::as_ref(&img).to_rgba8();
    draw_filled_circle_mut(&mut rgba_img, (x, y), radius, color);

    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgba8(rgba_img))))
}

/// Draw text at the specified position.
/// Note: Currently not implemented - requires rusttype or ab_glyph font support.
pub fn draw_text(
    _state: &State<Value>,
    _x: i32,
    _y: i32,
    _text: String,
    _size: f32,
    _color_str: String,
) -> Result<Value, Error> {
    Err(Error::general_error(
        "draw_text is not yet implemented - requires font library support".to_string()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use liquers_core::state::State;

    fn create_test_image() -> DynamicImage {
        let img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_draw_line() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_line(&state, 10, 10, 50, 50, "red".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    #[test]
    fn test_draw_line_hex_color() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_line(&state, 10, 10, 50, 50, "0xFF0000".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    #[test]
    fn test_draw_rect() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_rect(&state, 10, 10, 30, 20, "blue".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    #[test]
    fn test_draw_filled_rect() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_filled_rect(&state, 10, 10, 30, 20, "green".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    #[test]
    fn test_draw_circle() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_circle(&state, 50, 50, 20, "orange".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    #[test]
    fn test_draw_filled_circle() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_filled_circle(&state, 50, 50, 20, "purple".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }

    // Note: draw_text test is commented out as it requires font file
    // Uncomment when font file is available
    /*
    #[test]
    fn test_draw_text() {
        let img = create_test_image();
        let state = State::new().with_data(Value::from_image(Arc::new(img)));
        let result = draw_text(&state, 10, 50, "Hello".to_string(), 20.0, "black".to_string()).unwrap();

        let result_img = result.as_image().unwrap();
        assert_eq!(Arc::as_ref(&result_img).dimensions(), (100, 100));
    }
    */
}
