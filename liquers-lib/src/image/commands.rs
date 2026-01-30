use liquers_core::{error::Error, context::Context};
use crate::environment::CommandRegistryAccess;
use crate::environment::DefaultEnvironment;
use crate::value::Value;
use liquers_macro::register_command;

/// Register all Phase 1 image commands.
pub fn register_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = DefaultEnvironment<Value>;

    // I/O Operations
    register_command!(cr,
        fn from_bytes(state) -> result
        label: "From bytes"
        doc: "Load image from bytes with auto-detected format"
        ns: "img"
    )?;

    register_command!(cr,
        fn from_format(state, format_str: String) -> result
        label: "From format"
        doc: "Load image from bytes with explicitly specified format"
        ns: "img"
    )?;

    register_command!(cr,
        fn svg_to_image(state, width: u32, height: u32) -> result
        label: "SVG to image"
        doc: "Render SVG to raster image with specified dimensions"
        ns: "img"
    )?;

    // Format Conversion
    register_command!(cr,
        fn to_png(state) -> result
        label: "To PNG"
        doc: "Convert image to PNG format (returns bytes)"
        ns: "img"
    )?;

    register_command!(cr,
        fn to_jpeg(state, quality: u8 = 85) -> result
        label: "To JPEG"
        doc: "Convert image to JPEG format with specified quality (1-100)"
        ns: "img"
    )?;

    register_command!(cr,
        fn to_dataurl(state, format_str: String = "png") -> result
        label: "To data URL"
        doc: "Convert image to base64 data URL with specified format (png, jpeg, webp, etc.)"
        ns: "img"
    )?;

    register_command!(cr,
        fn color_format(state, format: String) -> result
        label: "Color format"
        doc: "Convert image color format (rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16)"
        ns: "img"
    )?;

    // Geometric Transformations
    register_command!(cr,
        fn resize(state, width: u32, height: u32, method: String = "lanczos3") -> result
        label: "Resize"
        doc: "Resize image to exact dimensions in pixels"
        ns: "img"
    )?;

    register_command!(cr,
        fn resize_by(state, percent: f32, method: String = "lanczos3") -> result
        label: "Resize by percentage"
        doc: "Resize image by percentage (uniform scaling)"
        ns: "img"
    )?;

    register_command!(cr,
        fn thumb(state, max_width: u32, max_height: u32, method: String = "lanczos3") -> result
        label: "Thumbnail"
        doc: "Resize image preserving aspect ratio (fits within max dimensions)"
        ns: "img"
    )?;

    register_command!(cr,
        fn crop(state, x: u32, y: u32, width: u32, height: u32) -> result
        label: "Crop"
        doc: "Crop image to rectangle (x, y, width, height)"
        ns: "img"
    )?;

    register_command!(cr,
        fn rotate(state, angle: f32) -> result
        label: "Rotate"
        doc: "Rotate image by arbitrary angle in degrees (positive = clockwise)"
        ns: "img"
    )?;

    register_command!(cr,
        fn rot90(state) -> result
        label: "Rotate 90°"
        doc: "Rotate image 90 degrees clockwise"
        ns: "img"
    )?;

    register_command!(cr,
        fn rot180(state) -> result
        label: "Rotate 180°"
        doc: "Rotate image 180 degrees"
        ns: "img"
    )?;

    register_command!(cr,
        fn rot270(state) -> result
        label: "Rotate 270°"
        doc: "Rotate image 270 degrees clockwise (90 degrees counter-clockwise)"
        ns: "img"
    )?;

    register_command!(cr,
        fn fliph(state) -> result
        label: "Flip horizontal"
        doc: "Flip image horizontally"
        ns: "img"
    )?;

    register_command!(cr,
        fn flipv(state) -> result
        label: "Flip vertical"
        doc: "Flip image vertically"
        ns: "img"
    )?;

    // Color Operations
    register_command!(cr,
        fn gray(state) -> result
        label: "Grayscale"
        doc: "Convert image to grayscale"
        ns: "img"
    )?;

    register_command!(cr,
        fn invert(state) -> result
        label: "Invert"
        doc: "Invert image colors (negative)"
        ns: "img"
    )?;

    register_command!(cr,
        fn brighten(state, value: i32) -> result
        label: "Brighten"
        doc: "Adjust brightness (positive = brighten, negative = darken)"
        ns: "img"
    )?;

    register_command!(cr,
        fn contrast(state, value: f32) -> result
        label: "Contrast"
        doc: "Adjust contrast (positive = increase, negative = decrease)"
        ns: "img"
    )?;

    register_command!(cr,
        fn huerot(state, value: i32) -> result
        label: "Hue rotate"
        doc: "Rotate hue by specified degrees (-180 to 180)"
        ns: "img"
    )?;

    // Filtering Operations
    register_command!(cr,
        fn blur(state, sigma: f32) -> result
        label: "Blur"
        doc: "Apply Gaussian blur with specified sigma value"
        ns: "img"
    )?;

    register_command!(cr,
        fn sharpen(state) -> result
        label: "Sharpen"
        doc: "Sharpen image using unsharp mask"
        ns: "img"
    )?;

    // Image Information
    register_command!(cr,
        fn dims(state) -> result
        label: "Dimensions"
        doc: "Get image dimensions as 'WIDTHxHEIGHT' string"
        ns: "img"
    )?;

    register_command!(cr,
        fn width(state) -> result
        label: "Width"
        doc: "Get image width in pixels"
        ns: "img"
    )?;

    register_command!(cr,
        fn height(state) -> result
        label: "Height"
        doc: "Get image height in pixels"
        ns: "img"
    )?;

    register_command!(cr,
        fn colortype(state) -> result
        label: "Color type"
        doc: "Get image color type (Rgb8, Rgba8, L8, etc.)"
        ns: "img"
    )?;

    Ok(env)
}

// Re-export command functions for testing and direct use
pub use super::io::{from_bytes, from_format, svg_to_image};
pub use super::format::{to_png, to_jpeg, to_dataurl, color_format};
pub use super::geometric::{resize, resize_by, thumb, crop, rotate, rot90, rot180, rot270, fliph, flipv};
pub use super::color::{gray, invert, brighten, contrast, huerot};
pub use super::filtering::{blur, sharpen};
pub use super::info::{dims, width, height, colortype};
