use std::sync::Arc;

use image::DynamicImage;
use liquers_core::value::DefaultValueSerializer;
use liquers_lib::value::{ExtValueInterface, Value};

fn make_test_image() -> Arc<DynamicImage> {
    let img = image::RgbaImage::from_pixel(8, 6, image::Rgba([255, 0, 0, 255]));
    Arc::new(DynamicImage::ImageRgba8(img))
}

#[test]
fn image_value_png_roundtrip() {
    let image = make_test_image();
    let value = Value::from_image(image);
    let bytes = value.as_bytes("png").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "image", "png").unwrap();
    let decoded_img = decoded.as_image().unwrap();
    assert_eq!(decoded_img.width(), 8);
    assert_eq!(decoded_img.height(), 6);
}

#[test]
fn image_value_jpg_alias_roundtrip() {
    let image = make_test_image();
    let value = Value::from_image(image);
    let bytes = value.as_bytes("jpg").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "image", "jpg").unwrap();
    let decoded_img = decoded.as_image().unwrap();
    assert_eq!(decoded_img.width(), 8);
    assert_eq!(decoded_img.height(), 6);
}

#[test]
fn image_value_unsupported_format_returns_error() {
    let value = Value::from_image(make_test_image());
    assert!(value.as_bytes("unsupported").is_err());
}
