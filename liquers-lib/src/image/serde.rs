use std::io::{Seek, Write};

use crate::image::util::{format_to_image_format, format_to_mime_type, normalize_format};
use image::DynamicImage;
use liquers_core::error::{Error, ErrorType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageDataFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Ico,
    DataUrl,
    Auto,
}

fn serialization_error(message: impl Into<String>) -> Error {
    Error::new(ErrorType::SerializationError, message.into())
}

pub fn parse_image_data_format(data_format: &str) -> Result<ImageDataFormat, Error> {
    let format = data_format.trim().to_ascii_lowercase();
    match format.as_str() {
        "" | "png" => Ok(ImageDataFormat::Png),
        "jpg" | "jpeg" | "jpe" => Ok(ImageDataFormat::Jpeg),
        "webp" => Ok(ImageDataFormat::Webp),
        "gif" => Ok(ImageDataFormat::Gif),
        "bmp" => Ok(ImageDataFormat::Bmp),
        "tif" | "tiff" => Ok(ImageDataFormat::Tiff),
        "ico" => Ok(ImageDataFormat::Ico),
        "dataurl" => Ok(ImageDataFormat::DataUrl),
        "auto" | "image" => Ok(ImageDataFormat::Auto),
        _ => Err(serialization_error(format!(
            "Unsupported image data_format '{}'",
            data_format
        ))),
    }
}

pub fn deserialize_image_from_bytes(
    bytes: &[u8],
    data_format: &str,
) -> Result<DynamicImage, Error> {
    match parse_image_data_format(data_format)? {
        ImageDataFormat::Auto => image::load_from_memory(bytes).map_err(|e| {
            serialization_error(format!(
                "Failed to deserialize image from auto-detected bytes: {}",
                e
            ))
        }),
        ImageDataFormat::DataUrl => Err(serialization_error(
            "Image deserialization from dataurl is not implemented",
        )),
        format => {
            let fmt = match format {
                ImageDataFormat::Png => image::ImageFormat::Png,
                ImageDataFormat::Jpeg => image::ImageFormat::Jpeg,
                ImageDataFormat::Webp => image::ImageFormat::WebP,
                ImageDataFormat::Gif => image::ImageFormat::Gif,
                ImageDataFormat::Bmp => image::ImageFormat::Bmp,
                ImageDataFormat::Tiff => image::ImageFormat::Tiff,
                ImageDataFormat::Ico => image::ImageFormat::Ico,
                ImageDataFormat::DataUrl | ImageDataFormat::Auto => unreachable!(),
            };
            image::load_from_memory_with_format(bytes, fmt).map_err(|e| {
                serialization_error(format!(
                    "Failed to deserialize image as {}: {}",
                    data_format, e
                ))
            })
        }
    }
}

pub fn serialize_image_to_writer<W: Write + Seek>(
    img: &DynamicImage,
    data_format: &str,
    mut writer: W,
) -> Result<(), Error> {
    match parse_image_data_format(data_format)? {
        ImageDataFormat::DataUrl => {
            let mut encoded = Vec::new();
            img.write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageFormat::Png,
            )
            .map_err(|e| serialization_error(format!("Failed to encode image as PNG: {}", e)))?;
            let mime = "image/png";
            let base64_data =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encoded);
            let data_url = format!("data:{};base64,{}", mime, base64_data);
            writer
                .write_all(data_url.as_bytes())
                .map_err(|e| serialization_error(format!("Failed to write dataurl bytes: {}", e)))
        }
        ImageDataFormat::Auto => Err(serialization_error(
            "Image serialization requires explicit data_format (use png/jpeg/webp/...)",
        )),
        _ => {
            let normalized = normalize_format(data_format)?;
            let img_format = format_to_image_format(&normalized)?;
            img.write_to(&mut writer, img_format)
                .map_err(|e| serialization_error(format!("Failed to encode image: {}", e)))
        }
    }
}

pub fn serialize_image_to_bytes(img: &DynamicImage, data_format: &str) -> Result<Vec<u8>, Error> {
    if matches!(
        parse_image_data_format(data_format)?,
        ImageDataFormat::DataUrl
    ) {
        let normalized = normalize_format("png")?;
        let mut encoded = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut encoded),
            format_to_image_format(&normalized)?,
        )
        .map_err(|e| serialization_error(format!("Failed to encode image as PNG: {}", e)))?;
        let mime = format_to_mime_type(&normalized)?;
        let base64_data =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encoded);
        return Ok(format!("data:{};base64,{}", mime, base64_data).into_bytes());
    }

    let mut buffer = Vec::new();
    serialize_image_to_writer(img, data_format, std::io::Cursor::new(&mut buffer))?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_image() -> DynamicImage {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_parse_aliases() {
        assert_eq!(
            parse_image_data_format("jpg").unwrap(),
            ImageDataFormat::Jpeg
        );
        assert_eq!(
            parse_image_data_format("jpeg").unwrap(),
            ImageDataFormat::Jpeg
        );
        assert_eq!(
            parse_image_data_format("image").unwrap(),
            ImageDataFormat::Auto
        );
        assert_eq!(
            parse_image_data_format("png").unwrap(),
            ImageDataFormat::Png
        );
    }

    #[test]
    fn test_roundtrip_png() {
        let img = make_test_image();
        let bytes = serialize_image_to_bytes(&img, "png").unwrap();
        let decoded = deserialize_image_from_bytes(&bytes, "png").unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 4);
    }

    #[test]
    fn test_roundtrip_jpeg() {
        let img = make_test_image();
        let bytes = serialize_image_to_bytes(&img, "jpeg").unwrap();
        let decoded = deserialize_image_from_bytes(&bytes, "jpeg").unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 4);
    }

    #[test]
    fn test_dataurl_serialization() {
        let img = make_test_image();
        let bytes = serialize_image_to_bytes(&img, "dataurl").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_unsupported_format_error() {
        assert!(parse_image_data_format("foo").is_err());
    }
}
