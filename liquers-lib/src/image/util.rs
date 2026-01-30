use liquers_core::{error::Error, state::State};
use crate::value::{ExtValueInterface, Value};
use std::sync::Arc;
use image::{DynamicImage, ImageFormat, Rgba};

/// Extract Arc<DynamicImage> from state, ensuring it contains an image.
pub fn try_to_image(state: &State<Value>) -> Result<Arc<DynamicImage>, Error> {
    state.data.as_image()
}

/// Normalize format string to lowercase canonical name, handling common variations.
/// Returns normalized format or error if unsupported.
///
/// Examples:
/// - "png", "PNG", "Png" → "png"
/// - "jpg", "jpeg", "JPG", "JPEG", "jpe" → "jpeg"
/// - "" → "png" (default)
pub fn normalize_format(format: &str) -> Result<String, Error> {
    let normalized = match format.trim().to_lowercase().as_str() {
        "" | "png" => "png",
        "jpg" | "jpeg" | "jpe" => "jpeg",
        "webp" => "webp",
        "gif" => "gif",
        "bmp" => "bmp",
        "tiff" | "tif" => "tiff",
        "ico" => "ico",
        _ => {
            return Err(Error::general_error(format!(
                "Unsupported image format '{}'. Supported: png, jpg/jpeg, webp, gif, bmp, tiff/tif, ico",
                format
            )))
        }
    };
    Ok(normalized.to_string())
}

/// Map normalized format string to image::ImageFormat enum.
pub fn format_to_image_format(format: &str) -> Result<ImageFormat, Error> {
    match format {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::WebP),
        "gif" => Ok(ImageFormat::Gif),
        "bmp" => Ok(ImageFormat::Bmp),
        "tiff" => Ok(ImageFormat::Tiff),
        "ico" => Ok(ImageFormat::Ico),
        _ => Err(Error::general_error(format!(
            "Unknown format '{}' (use normalize_format first)",
            format
        ))),
    }
}

/// Map normalized format string to MIME type for data URLs.
pub fn format_to_mime_type(format: &str) -> Result<&'static str, Error> {
    match format {
        "png" => Ok("image/png"),
        "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        "bmp" => Ok("image/bmp"),
        "tiff" => Ok("image/tiff"),
        "ico" => Ok("image/x-icon"),
        _ => Err(Error::general_error(format!(
            "Unknown format '{}' (use normalize_format first)",
            format
        ))),
    }
}

/// Parse a color string (name or RRGGBB[AA] hex value, without #) into image::Rgba<u8>.
/// Supports common color names and hex values like "ff0000" or "ff000080".
///
/// Examples:
/// - "red" → Rgba([255, 0, 0, 255])
/// - "ff0000" → Rgba([255, 0, 0, 255])
/// - "ff000080" → Rgba([255, 0, 0, 128])
pub fn parse_color(s: &str) -> Result<Rgba<u8>, Error> {
    let s = s.trim().to_lowercase();

    // Named colors
    let (r, g, b, a) = match s.as_str() {
        "black" => (0, 0, 0, 255),
        "white" => (255, 255, 255, 255),
        "red" => (255, 0, 0, 255),
        "green" => (0, 255, 0, 255),
        "blue" => (0, 0, 255, 255),
        "yellow" => (255, 255, 0, 255),
        "cyan" => (0, 255, 255, 255),
        "magenta" => (255, 0, 255, 255),
        "gray" | "grey" => (128, 128, 128, 255),
        "orange" => (255, 165, 0, 255),
        "purple" => (128, 0, 128, 255),
        "brown" => (153, 102, 51, 255),
        "pink" => (255, 192, 203, 255),
        "lime" => (0, 255, 0, 255),
        "navy" => (0, 0, 128, 255),
        "teal" => (0, 128, 128, 255),
        "olive" => (128, 128, 0, 255),
        "maroon" => (128, 0, 0, 255),
        "silver" => (192, 192, 192, 255),
        _ => {
            // Try hex without #
            match s.len() {
                6 => {
                    // RRGGBB
                    let r = u8::from_str_radix(&s[0..2], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    let g = u8::from_str_radix(&s[2..4], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    let b = u8::from_str_radix(&s[4..6], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    (r, g, b, 255)
                }
                8 => {
                    // RRGGBBAA
                    let r = u8::from_str_radix(&s[0..2], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    let g = u8::from_str_radix(&s[2..4], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    let b = u8::from_str_radix(&s[4..6], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    let a = u8::from_str_radix(&s[6..8], 16).map_err(|_| {
                        Error::general_error(format!("Invalid hex color format: {}", s))
                    })?;
                    (r, g, b, a)
                }
                _ => {
                    return Err(Error::general_error(format!(
                        "Invalid color '{}'. Use named color or hex format (RRGGBB or RRGGBBAA)",
                        s
                    )))
                }
            }
        }
    };

    Ok(Rgba([r, g, b, a]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_format() {
        assert_eq!(normalize_format("png").unwrap(), "png");
        assert_eq!(normalize_format("PNG").unwrap(), "png");
        assert_eq!(normalize_format("jpg").unwrap(), "jpeg");
        assert_eq!(normalize_format("JPEG").unwrap(), "jpeg");
        assert_eq!(normalize_format("jpe").unwrap(), "jpeg");
        assert_eq!(normalize_format("").unwrap(), "png");
        assert_eq!(normalize_format("tiff").unwrap(), "tiff");
        assert_eq!(normalize_format("tif").unwrap(), "tiff");
        assert!(normalize_format("invalid").is_err());
    }

    #[test]
    fn test_format_to_image_format() {
        assert!(matches!(
            format_to_image_format("png").unwrap(),
            ImageFormat::Png
        ));
        assert!(matches!(
            format_to_image_format("jpeg").unwrap(),
            ImageFormat::Jpeg
        ));
    }

    #[test]
    fn test_format_to_mime_type() {
        assert_eq!(format_to_mime_type("png").unwrap(), "image/png");
        assert_eq!(format_to_mime_type("jpeg").unwrap(), "image/jpeg");
        assert_eq!(format_to_mime_type("webp").unwrap(), "image/webp");
    }

    #[test]
    fn test_parse_color() {
        // Named colors
        assert_eq!(parse_color("red").unwrap(), Rgba([255, 0, 0, 255]));
        assert_eq!(parse_color("blue").unwrap(), Rgba([0, 0, 255, 255]));
        assert_eq!(parse_color("white").unwrap(), Rgba([255, 255, 255, 255]));

        // Hex colors
        assert_eq!(parse_color("ff0000").unwrap(), Rgba([255, 0, 0, 255]));
        assert_eq!(parse_color("00ff00").unwrap(), Rgba([0, 255, 0, 255]));
        assert_eq!(parse_color("ff000080").unwrap(), Rgba([255, 0, 0, 128]));

        // Invalid colors
        assert!(parse_color("invalid").is_err());
        assert!(parse_color("gg0000").is_err());
        assert!(parse_color("ff00").is_err());
    }
}
