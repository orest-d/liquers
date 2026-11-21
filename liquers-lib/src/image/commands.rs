//! The `img` commands: loading, transforming, filtering and saving images, as one macro.
//!
//! # Main components
//!
//! - `register_image_commands!`: registers every `img` command with its label and `hint icon`,
//!   and presets on `img/blur` (`blur-1`, `blur-3`, `blur-8`) and `img/rotate` (`rotate-90`,
//!   `rotate-180`, `rotate-270`).
//! - [`register_commands`]: the same, on a [`DefaultEnvironment`] taken by value.
//!
//! # Where it is used
//!
//! - `crate::commands::register_all_commands_fn` (with `image-support` on); the query editor draws
//!   each command with its label and icon.
//!
//! # Related concepts
//!
//! - The command functions in the sibling modules of `crate::image`; images are
//!   `ExtValue::Image` values.
//!
//! # Examples
//!
//! ```
//! use liquers_core::context::Environment;
//! use liquers_lib::environment::DefaultEnvironment;
//! use liquers_lib::value::Value;
//!
//! let env = liquers_lib::image::commands::register_commands(DefaultEnvironment::<Value>::new()).unwrap();
//! let registry = env.get_command_metadata_registry();
//! let blur = registry.commands.iter().find(|m| m.namespace == "img" && m.name == "blur").unwrap();
//! assert_eq!(blur.presets.len(), 3);
//! ```

use crate::environment::CommandRegistryAccess;
use crate::environment::DefaultEnvironment;
use crate::value::Value;
use liquers_core::{context::Context, error::Error};

/// Register all image commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_image_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::image::commands::*;

        // I/O Operations
        register_command!($cr,
            fn from_bytes(state) -> result
            label: "From bytes"
            hint icon: "image"
            doc: "Load image from bytes with auto-detected format"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn from_format(
                state,
                format_str: String (
                    enum(type: string, others: true): ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico"]
                )
            ) -> result
            label: "From format"
            hint icon: "image"
            doc: "Load image from bytes with explicitly specified format"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn svg_to_image(state, width: u32, height: u32) -> result
            label: "SVG to image"
            hint icon: "file-svg"
            doc: "Render SVG to raster image with specified dimensions"
            ns: "img"

        version: auto
        )?;

        // Format Conversion
        register_command!($cr,
            fn to_png(state) -> result
            label: "To PNG"
            hint icon: "file-image"
            doc: "Convert image to PNG format (returns bytes)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn to_jpeg(state, quality: u8 = 85) -> result
            label: "To JPEG"
            hint icon: "file-image"
            doc: "Convert image to JPEG format with specified quality (1-100)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn to_dataurl(
                state,
                format_str: String = "png" (
                    enum(type: string, others: true): ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico"]
                )
            ) -> result
            label: "To data URL"
            hint icon: "link"
            doc: "Convert image to base64 data URL with specified format (png, jpeg, webp, etc.)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn color_format(
                state,
                format: String (
                    enum: ["rgb8", "rgba8", "luma8", "luma_alpha8", "rgb16", "rgba16"]
                )
            ) -> result
            label: "Color format"
            doc: "Convert image color format (rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16)"
            ns: "img"

        version: auto
        )?;

        // Geometric Transformations
        register_command!($cr,
            fn resize(
                state,
                width: u32,
                height: u32,
                method: String = "lanczos3" (
                    enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]
                )
            ) -> result
            label: "Resize"
            hint icon: "arrows-out"
            doc: "Resize image to exact dimensions in pixels"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn resize_by(
                state,
                percent: f32,
                method: String = "lanczos3" (
                    enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]
                )
            ) -> result
            label: "Resize by percentage"
            hint icon: "arrows-out"
            doc: "Resize image by percentage (uniform scaling)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn thumb(
                state,
                max_width: u32,
                max_height: u32,
                method: String = "lanczos3" (
                    enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]
                )
            ) -> result
            label: "Thumbnail"
            hint icon: "arrows-out"
            doc: "Resize image preserving aspect ratio (fits within max dimensions)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn crop(state, x: u32, y: u32, width: u32, height: u32) -> result
            label: "Crop"
            hint icon: "crop"
            doc: "Crop image to rectangle (x, y, width, height)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn rotate(state, angle: f32) -> result
            label: "Rotate"
            hint icon: "arrow-clockwise"
            preset: "rotate-90" (label: "90°")
            preset: "rotate-180" (label: "180°")
            preset: "rotate-270" (label: "270°")
            doc: "Rotate image by arbitrary angle in degrees (positive = clockwise)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn rot90(state) -> result
            label: "Rotate 90°"
            hint icon: "arrow-clockwise"
            doc: "Rotate image 90 degrees clockwise"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn rot180(state) -> result
            label: "Rotate 180°"
            hint icon: "arrow-clockwise"
            doc: "Rotate image 180 degrees"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn rot270(state) -> result
            label: "Rotate 270°"
            hint icon: "arrow-clockwise"
            doc: "Rotate image 270 degrees clockwise (90 degrees counter-clockwise)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn fliph(state) -> result
            label: "Flip horizontal"
            hint icon: "flip-horizontal"
            doc: "Flip image horizontally"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn flipv(state) -> result
            label: "Flip vertical"
            hint icon: "flip-vertical"
            doc: "Flip image vertically"
            ns: "img"

        version: auto
        )?;

        // Color Operations
        register_command!($cr,
            fn gray(state) -> result
            label: "Grayscale"
            hint icon: "drop-half"
            doc: "Convert image to grayscale"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn invert(state) -> result
            label: "Invert"
            hint icon: "circle-half"
            doc: "Invert image colors (negative)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn brighten(state, value: i32) -> result
            label: "Brighten"
            hint icon: "sun"
            doc: "Adjust brightness (positive = brighten, negative = darken)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn contrast(state, value: f32) -> result
            label: "Contrast"
            hint icon: "circle-half-tilt"
            doc: "Adjust contrast (positive = increase, negative = decrease)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn huerot(state, value: i32) -> result
            label: "Hue rotate"
            doc: "Rotate hue by specified degrees (-180 to 180)"
            ns: "img"

        version: auto
        )?;

        // Filtering Operations
        register_command!($cr,
            fn blur(state, sigma: f32) -> result
            label: "Blur"
            hint icon: "drop"
            preset: "blur-1" (label: "Light")
            preset: "blur-3" (label: "Medium")
            preset: "blur-8" (label: "Strong")
            doc: "Apply Gaussian blur with specified sigma value"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn sharpen(state) -> result
            label: "Sharpen"
            hint icon: "diamond"
            doc: "Sharpen image using unsharp mask"
            ns: "img"

        version: auto
        )?;

        // Image Information
        register_command!($cr,
            fn dims(state) -> result
            label: "Dimensions"
            hint icon: "ruler"
            doc: "Get image dimensions as 'WIDTHxHEIGHT' string"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn width(state) -> result
            label: "Width"
            hint icon: "ruler"
            doc: "Get image width in pixels"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn height(state) -> result
            label: "Height"
            hint icon: "ruler"
            doc: "Get image height in pixels"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn colortype(state) -> result
            label: "Color type"
            doc: "Get image color type (Rgb8, Rgba8, L8, etc.)"
            ns: "img"

        version: auto
        )?;

        // ==== Phase 2 Commands ====

        // Format Conversion (Phase 2)
        register_command!($cr,
            fn to_webp(state) -> result
            label: "To WebP"
            hint icon: "file-image"
            doc: "Convert image to WebP format (returns bytes)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn to_gif(state) -> result
            label: "To GIF"
            hint icon: "file-image"
            doc: "Convert image to GIF format (returns bytes)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn to_bmp(state) -> result
            label: "To BMP"
            hint icon: "file-image"
            doc: "Convert image to BMP format (returns bytes)"
            ns: "img"

        version: auto
        )?;

        // Color Operations (Phase 2)
        register_command!($cr,
            fn gamma(state, gamma_value: f32) -> result
            label: "Gamma correction"
            doc: "Apply gamma correction (gamma > 1 darkens, < 1 brightens)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn saturate(state, factor: f32) -> result
            label: "Saturate"
            doc: "Adjust color saturation (1.0 = no change, > 1 = more, < 1 = less)"
            ns: "img"

        version: auto
        )?;

        // Filtering Operations (Phase 2)
        register_command!($cr,
            fn median(state, radius: u32) -> result
            label: "Median filter"
            doc: "Apply median filter for noise reduction (radius in pixels)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn boxfilt(state, radius: u32) -> result
            label: "Box filter"
            doc: "Apply box/mean filter for blur (radius in pixels)"
            ns: "img"

        version: auto
        )?;

        // Morphological Operations (Phase 2)
        register_command!($cr,
            fn erode(state, radius: u32) -> result
            label: "Erode"
            doc: "Morphological erosion (shrink bright regions, radius in pixels)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn dilate(state, radius: u32) -> result
            label: "Dilate"
            doc: "Morphological dilation (expand bright regions, radius in pixels)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn opening(state, radius: u32) -> result
            label: "Opening"
            doc: "Morphological opening (remove noise, radius in pixels)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn closing(state, radius: u32) -> result
            label: "Closing"
            doc: "Morphological closing (fill holes, radius in pixels)"
            ns: "img"

        version: auto
        )?;

        // Drawing Operations (Phase 2)
        register_command!($cr,
            fn draw_line(state, x1: i32, y1: i32, x2: i32, y2: i32, color_str: String) -> result
            label: "Draw line"
            hint icon: "line-segment"
            doc: "Draw line from (x1,y1) to (x2,y2) with specified color"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn draw_rect(state, x: i32, y: i32, width: u32, height: u32, color_str: String) -> result
            label: "Draw rectangle"
            hint icon: "square"
            doc: "Draw rectangle outline at (x,y) with specified dimensions and color"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn draw_filled_rect(state, x: i32, y: i32, width: u32, height: u32, color_str: String) -> result
            label: "Draw filled rectangle"
            hint icon: "square"
            doc: "Draw filled rectangle at (x,y) with specified dimensions and color"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn draw_circle(state, x: i32, y: i32, radius: i32, color_str: String) -> result
            label: "Draw circle"
            hint icon: "circle"
            doc: "Draw circle outline at (x,y) with specified radius and color"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn draw_filled_circle(state, x: i32, y: i32, radius: i32, color_str: String) -> result
            label: "Draw filled circle"
            hint icon: "circle"
            doc: "Draw filled circle at (x,y) with specified radius and color"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn draw_text(state, x: i32, y: i32, text: String, size: f32, color_str: String) -> result
            label: "Draw text"
            hint icon: "text-t"
            doc: "Draw text at (x,y) with specified font size and color"
            ns: "img"

        version: auto
        )?;

        // Edge Detection (Phase 2)
        register_command!($cr,
            fn sobel(state) -> result
            label: "Sobel edge detection"
            hint icon: "graph"
            doc: "Apply Sobel edge detection (horizontal + vertical edges)"
            ns: "img"

        version: auto
        )?;

        register_command!($cr,
            fn canny(state, low_threshold: f32, high_threshold: f32) -> result
            label: "Canny edge detection"
            hint icon: "graph"
            doc: "Apply Canny edge detection with low and high thresholds"
            ns: "img"

        version: auto
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_image_commands!` macro.
pub fn register_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = DefaultEnvironment<Value>;
    register_image_commands!(cr)?;
    Ok(env)
}

// Re-export command functions for testing and direct use
// (includes generated `__VERSION_` symbols used by `version: auto`).
pub use super::color::*;
pub use super::drawing::*;
pub use super::edges::*;
pub use super::filtering::*;
pub use super::format::*;
pub use super::geometric::*;
pub use super::info::*;
pub use super::io::*;
pub use super::morphology::*;
