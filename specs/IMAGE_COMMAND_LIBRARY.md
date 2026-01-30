# Image Command Library Specification

## Table of Contents

1. [Overview](#overview)
2. [Query Format](#query-format)
3. [Module Structure](#module-structure)
4. [Command Reference](#command-reference)
   - [I/O Operations](#io-operations)
   - [Format Conversion](#format-conversion)
   - [Geometric Transformations](#geometric-transformations)
   - [Color Operations](#color-operations)
   - [Filtering Operations](#filtering-operations)
   - [Morphological Operations](#morphological-operations)
   - [Image Information](#image-information)
   - [Drawing Operations](#drawing-operations-phase-2)
   - [Edge Detection](#edge-detection-phase-2)
5. [Complete Command Table](#complete-command-table)
6. [Implementation Notes](#implementation-notes)
7. [Phase 1 Implementation (MVP)](#phase-1-implementation-mvp)
8. [Recommended Implementation Order](#recommended-implementation-order)
9. [Related Documentation](#related-documentation)

## Overview

This specification defines a command library wrapping image processing functionality for Liquers using the `image` and `imageproc` crates. The library enables users to perform common image transformations, filtering, format conversions, and manipulations.

Commands are registered in the **`img` namespace** and follow Liquers naming conventions (alphanumeric + underscore, starting with letter).

**Module**: `liquers-lib::image`
**Namespace**: `img` (default realm)
**Value Type**: `ExtValue::Image` (Arc-wrapped `image::DynamicImage`)
**Replaces**: `RasterImage` (to be removed)
**Feature Flag**: `image-support` (controls entire image module and `imageproc` dependency)

## Command Naming Conventions

All command names follow Liquers conventions:
- **Alphanumeric + underscore only**, starting with a letter
- **No hyphens in command names** (hyphens separate arguments)
- **Lowercase preferred** for consistency
- **Shorter is better** - queries are typically oneliners; concise names save space
- **But must remain understandable** - don't over-abbreviate

**Naming philosophy**:
- Prefer `rot90` over `rotate90` (clear abbreviation)
- Prefer `dims` over `dimensions` (space-saving)
- Prefer `gray` over `grayscale` (common shorthand)
- Prefer `thumb` over `thumbnail` (common abbreviation)
- Keep `brighten` over `bright` (verb clarity, avoids ambiguity)
- Keep `sharpen` over `sharp` (verb clarity, avoids ambiguity)
- Keep `resize` not `rsz` (too cryptic)
- Keep `invert` not `inv` (short enough already)

**Method/format arguments** use enum-style strings passed as regular arguments:
- Resize method: `resize-800-600-lanczos3` (not `resize_lanczos3-800-600`)
- Color format: `color_format-rgba8` (not `to_rgba8` as separate command)
- Blur type: `blur-gaussian-2.5` (not `gaussian_blur-2.5`)
- Rotation method: `rotate-45-bilinear` (not `rotate_bilinear-45`)

### Suggested Command Names (All Commands)

**I/O Operations**:
- `from_bytes` - Load image from bytes (auto-detect format)
- `from_format` - Load image with explicit format
- `svg_to_image` - Render SVG to raster image

**Format Conversion**:
- `to_png` - Convert to PNG format
- `to_jpeg` - Convert to JPEG format
- `to_webp` - Convert to WebP format
- `to_gif` - Convert to GIF format
- `to_bmp` - Convert to BMP format
- `to_dataurl` - Convert to base64 data URL

**Color Format Conversion** (single command with enum):
- `color_format` - Convert color format (arguments: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16, rgb32f, rgba32f)

**Geometric Transformations**:
- `resize` - Resize to exact dimensions in pixels
- `resize_by` - Resize by percentage (uniform scaling)
- `thumb` - Resize preserving aspect ratio (shorter than `thumbnail`)
- `crop` - Crop to rectangle
- `rotate` - Rotate by arbitrary angle
- `rot90` - Rotate 90° clockwise (shorter than `rotate90`)
- `rot180` - Rotate 180° (shorter than `rotate180`)
- `rot270` - Rotate 270° clockwise (shorter than `rotate270`)
- `fliph` - Flip horizontally
- `flipv` - Flip vertically

**Color Operations**:
- `gray` - Convert to grayscale (shorter than `grayscale`)
- `invert` - Invert colors (negative)
- `brighten` - Adjust brightness (verb clarity)
- `contrast` - Adjust contrast
- `huerot` - Rotate hue (shorter than `huerotate`)
- `gamma` - Apply gamma correction (shorter than `adjust_gamma`)
- `saturate` - Adjust color saturation (shorter than `adjust_saturation`)

**Filtering**:
- `blur` - Apply blur (Gaussian by default)
- `sharpen` - Sharpen image (verb clarity)
- `median` - Apply median filter
- `boxfilt` - Apply box/mean filter (shorter than `box_filter`)

**Morphological Operations**:
- `erode` - Morphological erosion
- `dilate` - Morphological dilation
- `opening` - Morphological opening
- `closing` - Morphological closing

**Image Information**:
- `dims` - Get width and height (shorter than `dimensions`)
- `width` - Get width only
- `height` - Get height only
- `colortype` - Get color type (shorter than `color_type`)

**Drawing Operations** (Phase 2):
- `draw_line` - Draw line
- `draw_rect` - Draw rectangle outline
- `draw_filled_rect` - Draw filled rectangle
- `draw_circle` - Draw circle outline
- `draw_filled_circle` - Draw filled circle
- `draw_text` - Draw text

**Edge Detection** (Phase 2):
- `sobel` - Sobel edge detection
- `canny` - Canny edge detection

### Naming Philosophy Examples

**Shortened (space-saving, still clear)**:
```
rot90           ✓ (vs rotate90 - saves 3 chars, meaning obvious)
thumb           ✓ (vs thumbnail - saves 4 chars, common abbreviation)
gray            ✓ (vs grayscale - saves 5 chars, widely understood)
dims            ✓ (vs dimensions - saves 5 chars, common abbreviation)
huerot          ✓ (vs huerotate - saves 3 chars, clear meaning)
colortype       ✓ (vs color_type - saves 1 char, no underscore needed)
boxfilt         ✓ (vs box_filter - saves 4 chars, clear in context)
```

**Keep full name (clarity, verb forms, or already short)**:
```
brighten        ✓ (verb form clearer than "bright", avoid noun/adj confusion)
sharpen         ✓ (verb form clearer than "sharp", avoid adj confusion)
resize          ✓ (rsz would be too cryptic)
invert          ✓ (already short at 6 chars)
rotate          ✓ (arbitrary angle, distinct from rot90/180/270)
blur            ✓ (already short at 4 chars)
crop            ✓ (already short at 4 chars)
contrast        ✓ (clear and concise at 8 chars)
fliph/flipv     ✓ (already minimal)
```

**Rejected shortenings (too cryptic or ambiguous)**:
```
rsz             ✗ (resize is clearer)
inv             ✗ (could mean inverse, invert, or invalid)
rot             ✗ (ambiguous - which angle?)
bright          ✗ (brighten is clearer as verb, "bright" could be adj/noun)
sharp           ✗ (sharpen is clearer as verb, "sharp" could be adj)
br              ✗ (brighten shortened too much)
cnt             ✗ (contrast shortened too much)
```

## Query Format

Queries use the format:
```
-R/data/photo.jpg/-/ns-img/<command>-<arg1>-<arg2>/<command>-<arg1>...
```

Where:
- `-R/data/photo.jpg` is the resource (file path)
- `/-/` separates the resource from operations
- `ns-img` selects the image namespace (applies to all subsequent commands)
- Operations are chained with `/` (not `/-/`)
- Arguments within a command are separated by `-`
- **Negative numbers**: Use tilde prefix (e.g., `~10` means `-10`) since dash is the argument separator

**Important syntax rules**:
- **Dash (`-`)** separates arguments: `resize-800-600-lanczos3`
- **Tilde (`~`)** before digit means negative: `brighten-~10` (darken by 10)
- **Namespace persists**: Once `ns-img` is set, all following commands use that namespace

Examples:
```
# Simple resize and convert (with namespace)
-R/photos/portrait.jpg/-/ns-img/resize-800-600-lanczos3/to_png

# Complex image processing pipeline
-R/photos/landscape.jpg/-/ns-img/resize-1920-1080/rotate-5/brighten-10/contrast-5/blur-gaussian-1.5/to_jpeg-90

# SVG to raster with effects
-R/graphics/logo.svg/-/ns-img/svg_to_image-512-512/color_format-rgba8/invert/to_png

# Thumbnail generation
-R/photos/product.jpg/-/ns-img/thumb-300-300/to_jpeg-85/to_dataurl

# Grayscale conversion and enhancement
-R/scans/document.jpg/-/ns-img/color_format-luma8/contrast-20/sharpen-1.0/to_png

# Quick rotate and flip
-R/photos/rotated.jpg/-/ns-img/rot90/fliph/to_png

# Darkening with negative value (tilde syntax)
-R/photos/bright.jpg/-/ns-img/brighten-~20/to_png

# Color adjustment chain
-R/photos/sunset.jpg/-/ns-img/huerot-15/brighten-5/saturate-1.2/to_jpeg-95

# Resize by percentage
-R/photos/large.jpg/-/ns-img/resize_by-50/to_png

# Create thumbnail (50% size) with quality
-R/photos/original.jpg/-/ns-img/resize_by-50-lanczos3/sharpen-1.0/to_jpeg-85
```

## Module Structure

```
liquers-lib/src/image/
├── mod.rs              # Module declaration and registration
├── io.rs               # Input/output operations (from_*, to_*)
├── geometric.rs        # Resize, rotate, flip, crop
├── color.rs            # Color space conversions, adjustments
├── filtering.rs        # Blur, sharpen, median, bilateral
├── morphology.rs       # Erosion, dilation, opening, closing
├── drawing.rs          # Lines, shapes, text (Phase 2)
├── edges.rs            # Edge detection (Phase 2)
├── util.rs             # Helper functions (try_to_image, etc.)
└── raster_image.rs     # DEPRECATED: to be removed
```

## Command Reference

### I/O Operations

| Command | Arguments | Description | image Equivalent | Phase |
|---------|-----------|-------------|------------------|-------|
| `from_bytes` | *(none)* | Load image from byte data (auto-detect format) | `image::load_from_memory()` | 1 |
| `from_format` | `format` | Load image from bytes with explicit format | `image::load_from_memory_with_format()` | 1 |
| `svg_to_image` | `width-height` | Render SVG to raster image | `resvg::render()` | 1 |

**Format Detection**: Uses `state.metadata.get_data_format()` to determine format. If format is `"binary"`, `"bin"`, `"b"`, or missing, attempts auto-detection.

**Supported Formats**: PNG, JPEG, WebP, GIF, BMP, TIFF, ICO

**Examples**:
- `-R/photos/image.jpg/-/from_bytes` (auto-detect from metadata)
- `-R/data/unknown.bin/-/from_format-png` (explicit format)
- `-R/graphics/logo.svg/-/svg_to_image-512-512` (SVG rendering)

### Format Conversion

| Command | Arguments | Description | image Equivalent | Phase |
|---------|-----------|-------------|------------------|-------|
| `to_png` | *(none)* | Convert to PNG format (lossless) | `DynamicImage::write_to(&mut buf, ImageFormat::Png)` | 1 |
| `to_jpeg` | `[quality]` | Convert to JPEG (quality: 1-100, default 85) | `write_to(..., ImageFormat::Jpeg)` | 1 |
| `to_webp` | *(none)* | Convert to WebP format | `write_to(..., ImageFormat::WebP)` | 2 |
| `to_gif` | *(none)* | Convert to GIF format | `write_to(..., ImageFormat::Gif)` | 2 |
| `to_bmp` | *(none)* | Convert to BMP format | `write_to(..., ImageFormat::Bmp)` | 2 |
| `to_dataurl` | `[format]` | Convert to data URL (base64-encoded, format: png, jpg/jpeg, webp; default: png) | Custom implementation | 1 |

**Color Format Conversion** (single command with format argument):

| Command | Arguments | Description | image Equivalent | Phase |
|---------|-----------|-------------|------------------|-------|
| `color_format` | `format` | Convert to specified color format (format: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16, rgb32f, rgba32f) | `DynamicImage::to_*()` methods | 1 |

**Supported formats** for `color_format` command:
- `rgb8` - 8-bit RGB (3 channels, no alpha)
- `rgba8` - 8-bit RGBA (4 channels with alpha)
- `luma8` - 8-bit grayscale (1 channel)
- `luma_alpha8` - 8-bit grayscale + alpha (2 channels)
- `rgb16` - 16-bit RGB (high precision, no alpha) [Phase 2]
- `rgba16` - 16-bit RGBA (high precision with alpha) [Phase 2]
- `rgb32f` - 32-bit float RGB (HDR support) [Phase 2]
- `rgba32f` - 32-bit float RGBA (HDR support) [Phase 2]

**Examples**:
- `to_jpeg-95` (JPEG with quality 95)
- `to_dataurl` (PNG data URL, default format)
- `to_dataurl-png` (PNG data URL, explicit)
- `to_dataurl-jpeg` (JPEG data URL, uses file extension)
- `to_dataurl-jpg` (same as jpeg, variation supported)
- `color_format-rgba8` (convert to 8-bit RGBA)
- `color_format-luma8` (convert to grayscale)
- `color_format-rgb16` (convert to 16-bit RGB, Phase 2)

### Geometric Transformations

| Command | Arguments | Description | image/imageproc Equivalent | Phase |
|---------|-----------|-------------|----------------------------|-------|
| `resize` | `width-height-[method]` | Resize to exact dimensions in pixels (method: nearest, triangle, catmullrom, gaussian, lanczos3; default: lanczos3) | `DynamicImage::resize_exact()` with filter | 1 |
| `resize_by` | `percent-[method]` | Resize by percentage (uniform scaling, e.g., 50 = half size, 200 = double; method: same as resize; default: lanczos3) | Calculate dimensions, then `DynamicImage::resize_exact()` | 1 |
| `thumb` | `width-height` | Resize preserving aspect ratio (fit within bounds) | `DynamicImage::thumbnail()` | 1 |
| `crop` | `x-y-width-height` | Crop to rectangle | `DynamicImage::crop()` | 1 |
| `rot90` | *(none)* | Rotate 90° clockwise | `DynamicImage::rotate90()` | 1 |
| `rot180` | *(none)* | Rotate 180° | `DynamicImage::rotate180()` | 1 |
| `rot270` | *(none)* | Rotate 270° clockwise (90° counter-clockwise) | `DynamicImage::rotate270()` | 1 |
| `fliph` | *(none)* | Flip horizontally | `DynamicImage::fliph()` | 1 |
| `flipv` | *(none)* | Flip vertically | `DynamicImage::flipv()` | 1 |
| `rotate` | `angle-[method]` | Rotate by arbitrary angle in degrees (method: nearest, bilinear; default: bilinear) | `imageproc::geometric_transformations::rotate_about_center()` | 1 |
| `affine` | `a-b-c-d-e-f` | Apply affine transform matrix | `imageproc::geometric_transformations::affine()` | Not supported |

**Method Arguments** (for `resize`):
- `nearest` - Nearest neighbor (fast, blocky)
- `triangle` - Linear/bilinear interpolation
- `catmullrom` - Catmull-Rom spline
- `gaussian` - Gaussian filter
- `lanczos3` - Lanczos with window 3 (high quality, default)

**Examples**:
- `resize-800-600` (resize to 800x600 pixels, Lanczos3 by default)
- `resize-800-600-nearest` (resize to 800x600, fast nearest neighbor)
- `resize-1920-1080-lanczos3` (resize to 1920x1080, explicit high quality)
- `resize_by-50` (resize to 50% of original size, uniform scaling)
- `resize_by-200` (resize to 200% = double size)
- `resize_by-75-nearest` (resize to 75%, fast nearest neighbor)
- `thumb-800-600` (preserve aspect ratio, fit within 800x600 bounds)
- `crop-100-100-400-300` (x=100, y=100, w=400, h=300)
- `rot90` (90° clockwise, fast)
- `rot180` (180° rotation)
- `rot270` (270° clockwise = 90° counter-clockwise)
- `rotate-45` (rotate 45° with bilinear interpolation)
- `rotate-45-bilinear` (explicit method)
- `rotate-30-nearest` (rotate 30° with nearest neighbor, faster but lower quality)
- `rotate-~15` (rotate -15° = 15° counter-clockwise, tilde for negative)
- `fliph` (flip horizontally)
- `flipv` (flip vertically)

### Color Operations

| Command | Arguments | Description | image/imageproc Equivalent | Phase |
|---------|-----------|-------------|----------------------------|-------|
| `gray` | *(none)* | Convert to grayscale (preserves color type as luma) | `DynamicImage::grayscale()` | 1 |
| `invert` | *(none)* | Invert colors (negative) | `DynamicImage::invert()` | 1 |
| `brighten` | `value` | Adjust brightness (positive=brighter, negative=darker) | `DynamicImage::brighten()` | 1 |
| `contrast` | `value` | Adjust contrast (positive=more, negative=less) | `imageproc::contrast::contrast()` | 1 |
| `huerot` | `degrees` | Rotate hue by degrees (0-360) | `DynamicImage::huerotate()` | 1 |
| `gamma` | `value` | Gamma correction (gamma > 1 darkens, < 1 brightens) | Custom or imageproc | 2 |
| `saturate` | `factor` | Adjust saturation (1.0=no change, >1=more saturated, <1=less) | Custom or imageproc | 2 |

**Examples**:
- `gray` (convert to grayscale)
- `brighten-20` (brighten by 20)
- `brighten-~10` (darken by 10, tilde indicates negative)
- `contrast-10` (increase contrast)
- `huerot-180` (shift hue by 180°)
- `invert` (negative image)

### Filtering Operations

| Command | Arguments | Description | imageproc Equivalent | Phase |
|---------|-----------|-------------|---------------------|-------|
| `blur` | `method-sigma` | Gaussian blur with sigma (method: gaussian; future: box, median, bilateral) | `imageproc::filter::gaussian_blur_f32()` | 1 |
| `sharpen` | `amount` | Sharpen image (amount: sigma for unsharp mask) | `imageproc::filter::sharpen()` or custom | 1 |
| `median` | `radius` | Median filter (noise reduction, radius in pixels) | `imageproc::filter::median_filter()` | 2 |
| `boxfilt` | `radius` | Box/mean filter (blur by averaging, radius in pixels) | Custom box filter | 2 |
| `bilateral` | `sigma_color-sigma_space` | Bilateral filter (edge-preserving blur) | `imageproc::filter::bilateral_filter()` if available | Not supported |

**Method Arguments** (for `blur`):
- `gaussian` - Gaussian blur (default, smooth)
- Future: `box`, `median`, `bilateral`

**Examples**:
- `blur-gaussian-2.5` (Gaussian blur with sigma=2.5)
- `sharpen-1.5` (sharpen with sigma=1.5)
- `median-3` (Phase 2: median filter with radius 3)
- `boxfilt-5` (Phase 2: box filter with radius 5)

### Morphological Operations

| Command | Arguments | Description | imageproc Equivalent | Phase |
|---------|-----------|-------------|---------------------|-------|
| `erode` | `radius` | Morphological erosion (shrink bright regions) | `imageproc::morphology::erode()` | 2 |
| `dilate` | `radius` | Morphological dilation (expand bright regions) | `imageproc::morphology::dilate()` | 2 |
| `opening` | `radius` | Morphological opening (erode then dilate, remove noise) | Custom (erode + dilate) | 2 |
| `closing` | `radius` | Morphological closing (dilate then erode, fill holes) | Custom (dilate + erode) | 2 |

**Examples**:
- `erode-3` (erode with radius 3)
- `dilate-5` (dilate with radius 5)
- `opening-2` (remove small bright spots)
- `closing-2` (fill small dark holes)

### Image Information

| Command | Arguments | Description | image Equivalent | Phase |
|---------|-----------|-------------|------------------|-------|
| `dims` | *(none)* | Get image dimensions as dictionary `{width: u32, height: u32}` | `DynamicImage::dimensions()` | 1 |
| `width` | *(none)* | Get image width | `DynamicImage::width()` | 1 |
| `height` | *(none)* | Get image height | `DynamicImage::height()` | 1 |
| `colortype` | *(none)* | Get color type (e.g., "Rgb8", "Rgba8", "Luma8") | `DynamicImage::color()` | 1 |

**Examples**:
- `dims` returns `{width: 1920, height: 1080}`
- `width` returns `1920`
- `height` returns `1080`
- `colortype` returns `"Rgba8"`

### Drawing Operations (Phase 2)

| Command | Arguments | Description | imageproc Equivalent | Phase |
|---------|-----------|-------------|---------------------|-------|
| `draw_line` | `x1-y1-x2-y2-color` | Draw line from (x1,y1) to (x2,y2) | `imageproc::drawing::draw_line_segment_mut()` | 2 |
| `draw_rect` | `x-y-width-height-color` | Draw rectangle outline | `imageproc::drawing::draw_hollow_rect_mut()` | 2 |
| `draw_filled_rect` | `x-y-width-height-color` | Draw filled rectangle | `imageproc::drawing::draw_filled_rect_mut()` | 2 |
| `draw_circle` | `x-y-radius-color` | Draw circle outline | `imageproc::drawing::draw_hollow_circle_mut()` | 2 |
| `draw_filled_circle` | `x-y-radius-color` | Draw filled circle | `imageproc::drawing::draw_filled_circle_mut()` | 2 |
| `draw_text` | `x-y-text-size-color-[font]` | Draw text at position | `imageproc::drawing::draw_text_mut()` | 2 |

**Color Format**:
Colors can be specified as:
1. **Named colors** (case-insensitive): `red`, `blue`, `green`, `black`, `white`, `yellow`, `cyan`, `magenta`, `gray`/`grey`, `orange`, `purple`, `brown`, `pink`, `lime`, `navy`, `teal`, `olive`, `maroon`, `silver`
2. **Hex colors with 0x prefix**:
   - 6 hex digits (RRGGBB, opaque): `0xFF0000` for red
   - 8 hex digits (RRGGBBAA, with alpha): `0xFF000080` for semi-transparent red
   - Prefix `0x` disambiguates from potential numeric arguments

**Note**: Hash prefix (`#`) is not used because it would require escaping in query syntax.

**Examples**:
- `draw_line-10-10-100-100-red` (red line using named color)
- `draw_line-10-10-100-100-0xFF0000` (red line using hex)
- `draw_filled_rect-50-50-200-100-0x0000FF80` (semi-transparent blue rectangle)
- `draw_circle-100-100-50-orange` (orange circle outline)

### Edge Detection (Phase 2)

| Command | Arguments | Description | imageproc Equivalent | Phase |
|---------|-----------|-------------|---------------------|-------|
| `sobel` | *(none)* | Sobel edge detection (horizontal + vertical) | `imageproc::edges::sobel_gradients()` | 2 |
| `canny` | `low_threshold-high_threshold` | Canny edge detection | `imageproc::edges::canny()` | 2 |
| `scharr` | *(none)* | Scharr edge detection (more accurate than Sobel) | Custom Scharr filter | Not supported |

**Examples**:
- `sobel` (detect all edges)
- `canny-50-150` (Canny with low=50, high=150)

## Complete Command Table

| Category | Command | Arguments | Description | Phase | Priority |
|----------|---------|-----------|-------------|-------|----------|
| **I/O** | `from_bytes` | - | Load image from bytes (auto-detect format) | 1 | High |
| **I/O** | `from_format` | `format` | Load image with explicit format | 1 | High |
| **I/O** | `svg_to_image` | `width-height` | Render SVG to raster | 1 | Medium |
| **Format** | `to_png` | - | Convert to PNG | 1 | High |
| **Format** | `to_jpeg` | `[quality]` | Convert to JPEG | 1 | High |
| **Format** | `to_webp` | - | Convert to WebP | 2 | Low |
| **Format** | `to_gif` | - | Convert to GIF | 2 | Low |
| **Format** | `to_bmp` | - | Convert to BMP | 2 | Low |
| **Format** | `to_dataurl` | `[format]` | Convert to data URL (base64) | 1 | Medium |
| **Color Format** | `color_format` | `format` | Convert color format (rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16, rgb32f, rgba32f) | 1 | High |
| **Geometric** | `resize` | `width-height-[method]` | Resize to exact dimensions | 1 | High |
| **Geometric** | `resize_by` | `percent-[method]` | Resize by percentage (uniform scaling) | 1 | High |
| **Geometric** | `thumb` | `width-height` | Resize preserving aspect ratio | 1 | High |
| **Geometric** | `crop` | `x-y-width-height` | Crop to rectangle | 1 | High |
| **Geometric** | `rot90` | - | Rotate 90° CW | 1 | High |
| **Geometric** | `rot180` | - | Rotate 180° | 1 | Medium |
| **Geometric** | `rot270` | - | Rotate 270° CW | 1 | Medium |
| **Geometric** | `fliph` | - | Flip horizontal | 1 | Medium |
| **Geometric** | `flipv` | - | Flip vertical | 1 | Medium |
| **Geometric** | `rotate` | `angle-[method]` | Rotate arbitrary angle | 1 | High |
| **Geometric** | `affine` | `a-b-c-d-e-f` | Affine transform | Not supported | - |
| **Color** | `gray` | - | Convert to grayscale | 1 | High |
| **Color** | `invert` | - | Invert colors | 1 | Medium |
| **Color** | `brighten` | `value` | Adjust brightness | 1 | High |
| **Color** | `contrast` | `value` | Adjust contrast | 1 | High |
| **Color** | `huerot` | `degrees` | Rotate hue | 1 | Medium |
| **Color** | `gamma` | `value` | Gamma correction | 2 | Low |
| **Color** | `saturate` | `factor` | Adjust saturation | 2 | Low |
| **Filter** | `blur` | `method-sigma` | Gaussian blur | 1 | High |
| **Filter** | `sharpen` | `amount` | Sharpen image | 1 | High |
| **Filter** | `median` | `radius` | Median filter | 2 | Medium |
| **Filter** | `boxfilt` | `radius` | Box/mean filter | 2 | Low |
| **Filter** | `bilateral` | `sigma_color-sigma_space` | Bilateral filter | Not supported | - |
| **Morphology** | `erode` | `radius` | Morphological erosion | 2 | Low |
| **Morphology** | `dilate` | `radius` | Morphological dilation | 2 | Low |
| **Morphology** | `opening` | `radius` | Morphological opening | 2 | Low |
| **Morphology** | `closing` | `radius` | Morphological closing | 2 | Low |
| **Info** | `dims` | - | Get width and height | 1 | High |
| **Info** | `width` | - | Get width | 1 | Medium |
| **Info** | `height` | - | Get height | 1 | Medium |
| **Info** | `colortype` | - | Get color type | 1 | Medium |
| **Drawing** | `draw_line` | `x1-y1-x2-y2-color` | Draw line | 2 | Low |
| **Drawing** | `draw_rect` | `x-y-width-height-color` | Draw rectangle | 2 | Low |
| **Drawing** | `draw_filled_rect` | `x-y-width-height-color` | Draw filled rectangle | 2 | Low |
| **Drawing** | `draw_circle` | `x-y-radius-color` | Draw circle | 2 | Low |
| **Drawing** | `draw_filled_circle` | `x-y-radius-color` | Draw filled circle | 2 | Low |
| **Drawing** | `draw_text` | `x-y-text-size-color-[font]` | Draw text | 2 | Low |
| **Edges** | `sobel` | - | Sobel edge detection | 2 | Low |
| **Edges** | `canny` | `low-high` | Canny edge detection | 2 | Low |
| **Edges** | `scharr` | - | Scharr edge detection | Not supported | - |

## Implementation Notes

### General Principles

1. **State Parameter**: Commands receive `state: &State<V>`
   - Extract image: Use `try_to_image(&state)?` utility function
   - Return image: `V::from_image(dynamic_image)`
   - Handle Arc wrapper transparently

2. **Image Extraction Utility** (`util.rs`):

   All image commands should use the `try_to_image` utility function to convert State into a DynamicImage. This function:

   **Signature**:
   ```rust
   pub fn try_to_image<V: ValueInterface + ExtValueInterface>(
       state: &State<V>
   ) -> Result<Arc<image::DynamicImage>, Error>
   ```

   **Logic**:
   1. **Direct conversion**: First try `state.data.as_image()`
      - If successful, return the DynamicImage immediately
      - This handles the case where state already contains an Image

   2. **Deserialization from bytes**:
      - If direct conversion fails, check if value is Bytes
      - Inspect `state.metadata.get_data_format()` to determine format
      - Attempt deserialization based on format:

   **Supported formats**:
   - `"png"`: Parse as PNG
   - `"jpeg"`, `"jpg"`: Parse as JPEG
   - `"webp"`: Parse as WebP
   - `"gif"`: Parse as GIF
   - `"bmp"`: Parse as BMP
   - `"tiff"`, `"tif"`: Parse as TIFF
   - `"ico"`: Parse as ICO
   - `"binary"`, `"bin"`, `"b"`, or missing: Auto-detect format using `image::load_from_memory()`

   **Error handling**:
   - If value is not Image or Bytes: `"Cannot convert {type} to Image. Expected Image or Bytes."`
   - If format is unknown: `"Unsupported image format '{format}'. Supported: png, jpeg, webp, gif, bmp, tiff, ico"`
   - If deserialization fails: `"Failed to load image from {format}: {error}"`

   **Example implementation skeleton**:
   ```rust
   use image::DynamicImage;
   use std::sync::Arc;
   use liquers_core::{state::State, error::Error, value::ValueInterface};
   use crate::value::ExtValueInterface;
   use crate::image::util::{normalize_format, format_to_image_format};

   pub fn try_to_image<V: ValueInterface + ExtValueInterface>(
       state: &State<V>
   ) -> Result<Arc<DynamicImage>, Error> {
       // Try direct conversion first
       if let Ok(img) = state.data.as_image() {
           return Ok(img);
       }

       // Try to get bytes
       let bytes = state.data.as_bytes()
           .map_err(|_| Error::general_error(
               format!("Cannot convert {} to Image. Expected Image or Bytes.",
                   state.data.identifier())
           ))?;

       // Get format from metadata
       let format_str = state.metadata.get_data_format();

       // Load image based on format (with normalization)
       let img = match format_str.trim().to_lowercase().as_str() {
           "binary" | "bin" | "b" | "" => {
               // Auto-detect format
               image::load_from_memory(&bytes)
           },
           other => {
               // Normalize format (handles jpg/jpeg/JPG, case-insensitive)
               let normalized = normalize_format(other)?;
               let img_format = format_to_image_format(&normalized)?;
               image::load_from_memory_with_format(&bytes, img_format)
           }
       };

       img.map(Arc::new)
           .map_err(|e| Error::general_error(
               format!("Failed to load image from '{}': {}", format_str, e)
           ))
   }
   ```

   **Format handling improvements**:
   - Uses `normalize_format()` for consistent format handling
   - Case-insensitive: `PNG`, `png`, `Png` all work
   - Variations: `jpg`, `jpeg`, `JPG` all map to JPEG
   - Clear error messages listing supported formats
   - Auto-detect for `binary`, `bin`, `b`, or empty string

   **Usage in commands**:
   ```rust
   use crate::image::util::try_to_image;
   use liquers_core::{state::State, error::Error};
   use crate::value::Value;

   // Example 1: Simple transformation (gray)
   fn gray(state: &State<Value>) -> Result<Value, Error> {
       let img = try_to_image(state)?;
       let result = img.grayscale();
       Ok(Value::from_image(result))
   }

   // Example 1a: Command with numeric parameter (brighten)
   // Note: Framework decodes ~10 to -10, so command receives actual i32 value
   fn brighten(state: &State<Value>, value: i32) -> Result<Value, Error> {
       let img = try_to_image(state)?;
       let result = img.brighten(value);
       Ok(Value::from_image(result))
   }

   // Example 1b: Color format conversion (color_format)
   fn color_format(state: &State<Value>, format: String) -> Result<Value, Error> {
       let img = try_to_image(state)?;

       let result = match format.as_str() {
           "rgb8" => img.to_rgb8().into(),
           "rgba8" => img.to_rgba8().into(),
           "luma8" => img.to_luma8().into(),
           "luma_alpha8" => img.to_luma_alpha8().into(),
           "rgb16" => img.to_rgb16().into(),
           "rgba16" => img.to_rgba16().into(),
           // Future: rgb32f, rgba32f if supported
           _ => return Err(Error::general_error(
               format!("Invalid color format '{}'. Use: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16", format)
           )),
       };

       Ok(Value::from_image(result))
   }

   // Example 2: Parameterized operation (resize)
   fn resize(state: &State<Value>, width: u32, height: u32, method: String) -> Result<Value, Error> {
       let img = try_to_image(state)?;

       let filter = match method.as_str() {
           "nearest" => image::imageops::FilterType::Nearest,
           "triangle" => image::imageops::FilterType::Triangle,
           "catmullrom" => image::imageops::FilterType::CatmullRom,
           "gaussian" => image::imageops::FilterType::Gaussian,
           "lanczos3" => image::imageops::FilterType::Lanczos3,
           _ => return Err(Error::general_error(
               format!("Invalid resize method '{}'. Use: nearest, triangle, catmullrom, gaussian, lanczos3", method)
           )),
       };

       if width == 0 || height == 0 {
           return Err(Error::general_error(
               "Image dimensions must be greater than 0"
           ));
       }

       let result = img.resize_exact(width, height, filter);
       Ok(Value::from_image(result))
   }

   // Example 2b: Resize by percentage (resize_by)
   fn resize_by(state: &State<Value>, percent: f32, method: String) -> Result<Value, Error> {
       let img = try_to_image(state)?;

       // Validate percentage
       if percent <= 0.0 {
           return Err(Error::general_error(
               "Resize percentage must be greater than 0"
           ));
       }

       // Parse filter method (same as resize)
       let filter = match method.as_str() {
           "nearest" => image::imageops::FilterType::Nearest,
           "triangle" => image::imageops::FilterType::Triangle,
           "catmullrom" => image::imageops::FilterType::CatmullRom,
           "gaussian" => image::imageops::FilterType::Gaussian,
           "lanczos3" => image::imageops::FilterType::Lanczos3,
           _ => return Err(Error::general_error(
               format!("Invalid resize method '{}'. Use: nearest, triangle, catmullrom, gaussian, lanczos3", method)
           )),
       };

       // Calculate new dimensions
       let (width, height) = img.dimensions();
       let scale = percent / 100.0;
       let new_width = ((width as f32 * scale).round() as u32).max(1);
       let new_height = ((height as f32 * scale).round() as u32).max(1);

       let result = img.resize_exact(new_width, new_height, filter);
       Ok(Value::from_image(result))
   }

   // Example 3: Format conversion (to_jpeg)
   fn to_jpeg(state: &State<Value>, quality: u8) -> Result<Value, Error> {
       let img = try_to_image(state)?;

       if quality == 0 || quality > 100 {
           return Err(Error::general_error(
               "JPEG quality must be between 1 and 100"
           ));
       }

       let mut buf = Vec::new();
       let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
       encoder.encode_image(&img)
           .map_err(|e| Error::general_error(format!("Failed to encode JPEG: {}", e)))?;

       Ok(Value::from_bytes(buf))
   }

   // Example 4: Arbitrary rotation with method parameter (rotate)
   fn rotate(state: &State<Value>, angle: f32, method: String) -> Result<Value, Error> {
       use imageproc::geometric_transformations::{rotate_about_center, Interpolation};

       let img = try_to_image(state)?;

       // Parse interpolation method
       let interpolation = match method.as_str() {
           "nearest" => Interpolation::Nearest,
           "bilinear" => Interpolation::Bilinear,
           _ => return Err(Error::general_error(
               format!("Invalid rotation method '{}'. Use: nearest, bilinear", method)
           )),
       };

       // Convert angle to radians (imageproc expects radians)
       let angle_radians = angle.to_radians();

       // Rotate - need to handle different color types
       let result = match img.as_ref() {
           DynamicImage::ImageRgb8(rgb) => {
               let rotated = rotate_about_center(rgb, angle_radians, interpolation, image::Rgb([0, 0, 0]));
               DynamicImage::ImageRgb8(rotated)
           }
           DynamicImage::ImageRgba8(rgba) => {
               let rotated = rotate_about_center(rgba, angle_radians, interpolation, image::Rgba([0, 0, 0, 0]));
               DynamicImage::ImageRgba8(rotated)
           }
           DynamicImage::ImageLuma8(luma) => {
               let rotated = rotate_about_center(luma, angle_radians, interpolation, image::Luma([0]));
               DynamicImage::ImageLuma8(rotated)
           }
           DynamicImage::ImageLumaA8(luma_a) => {
               let rotated = rotate_about_center(luma_a, angle_radians, interpolation, image::LumaA([0, 0]));
               DynamicImage::ImageLumaA8(rotated)
           }
           // Handle other color types by converting to RGBA8
           _ => {
               let rgba = img.to_rgba8();
               let rotated = rotate_about_center(&rgba, angle_radians, interpolation, image::Rgba([0, 0, 0, 0]));
               DynamicImage::ImageRgba8(rotated)
           }
       };

       Ok(Value::from_image(result))
   }
   ```

3. **Parameter Parsing**:
   - Framework handles argument parsing
   - Use `EnumArgumentType` for method/format selection:
     - Resize method: `resize-800-600-lanczos3` (enum: nearest, triangle, catmullrom, gaussian, lanczos3)
     - Color format: `color_format-rgba8` (enum: rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16, rgb32f, rgba32f)
     - Blur method: `blur-gaussian-2.5` (enum: gaussian; future: box, median)
     - Rotation method: `rotate-45-bilinear` (enum: nearest, bilinear)
   - Numeric arguments: u32 for dimensions, f32 for sigma/gamma/angle, i32 for brightness
   - Default values: specify with `register_command!` DSL

**Example command registration with EnumArgumentType**:
```rust
use liquers_macro::register_command;

// Register color_format command
register_command!(cr,
    fn color_format(state, format: String) -> result
    label: "Convert color format"
    doc: "Convert image to specified color format (rgb8, rgba8, luma8, luma_alpha8, rgb16, rgba16)"
)?;

// Register resize with method enum and default
register_command!(cr,
    fn resize(state, width: u32, height: u32, method: String = "lanczos3") -> result
    label: "Resize image"
    doc: "Resize to exact dimensions with interpolation method"
    namespace: "img"
)?;

// Register resize_by with percentage and method
register_command!(cr,
    fn resize_by(state, percent: f32, method: String = "lanczos3") -> result
    label: "Resize by percentage"
    doc: "Resize image by percentage (uniform scaling, e.g., 50 = half size, 200 = double)"
    namespace: "img"
)?;

// Register rotate with method enum and default
register_command!(cr,
    fn rotate(state, angle: f32, method: String = "bilinear") -> result
    label: "Rotate by angle"
    doc: "Rotate image by arbitrary angle in degrees"
)?;
```

4. **Negative Number Handling**:
   - Query syntax uses tilde (`~`) before digits to indicate negative numbers
   - Example: `brighten-~10` in query becomes `brighten(-10)` in Rust
   - Framework handles decoding: `~42` → `-42`
   - Commands receive decoded negative values directly (no special handling needed)
   - Works for i32, f32 parameters: brightness, contrast, rotation angles, etc.

5. **Error Handling**:
   - Use `Error::general_error()` for operation failures
   - Provide clear, specific error messages
   - Never unwrap - always use `?` or `map_err()`
   - Validate inputs (dimensions > 0, quality 1-100, etc.)

5. **Type Conversion**:
   - Preserve color type by default (DynamicImage handles this)
   - Explicit conversion commands: `to_rgb8`, `to_rgba8`, `to_luma8`, etc.
   - Arc<DynamicImage> for efficient cloning

6. **Format Detection**:
   - Explicit format from metadata preferred
   - Auto-detect only for "binary", "bin", "b", or missing format
   - Error on unsupported formats with clear message

7. **Namespace Handling**:
   - All image commands registered in `img` namespace
   - Users select namespace with `ns-img` instruction in query
   - Namespace persists for all subsequent commands in the query
   - Example: `-R/photo.jpg/-/ns-img/resize-800-600/gray/to_png`
   - Command registration: `register_command!(cr, fn resize(...) namespace: "img")?`

8. **Format Handling**:
   - **Use file extensions**, not MIME types (e.g., `png`, `jpeg`, not `image/png`)
   - **Avoid special characters**: `/` and `-` have special meaning in query syntax
   - **Case-insensitive**: `PNG`, `png`, `Png` all work (normalized to lowercase)
   - **Support variations**: `jpg`, `jpeg`, `JPG`, `JPEG` all map to `jpeg`
   - **Normalize early**: Use `normalize_format()` utility in all commands
   - **Consistent mapping**: Use `format_to_image_format()` and `format_to_mime_type()` helpers
   - **Examples**:
     - ✅ `to_dataurl-png` (file extension)
     - ✅ `to_dataurl-jpg` (variation of jpeg)
     - ✅ `to_dataurl-JPEG` (case variation)
     - ❌ `to_dataurl-image/png` (MIME type with `/` - breaks query syntax)
     - ❌ `to_dataurl-image-png` (MIME type with `-` - ambiguous)

### SVG Rendering

SVG rendering uses the same approach as the deprecated `RasterImage`:

```rust
use resvg::usvg::{Options, Tree};
use tiny_skia::Pixmap;
use image::{DynamicImage, RgbaImage};

fn svg_to_image(state: &State<Value>, width: u32, height: u32) -> Result<Value, Error> {
    // Get SVG bytes from state
    let svg_bytes = state.data.as_bytes()
        .map_err(|_| Error::general_error("SVG rendering requires byte data"))?;

    // Parse SVG
    let opt = Options::default();
    let rtree = Tree::from_data(&svg_bytes, &opt)
        .map_err(|e| Error::general_error(format!("Failed to parse SVG: {:?}", e)))?;

    // Render to pixmap
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| Error::general_error("Failed to create pixmap"))?;
    resvg::render(&rtree, usvg::Transform::default(), &mut pixmap.as_mut());

    // Convert to DynamicImage
    let mut img_buf = RgbaImage::new(width, height);
    for (i, pixel) in pixmap.pixels().iter().enumerate() {
        let x = (i % width as usize) as u32;
        let y = (i / width as usize) as u32;
        img_buf.put_pixel(x, y, image::Rgba([
            pixel.red(),
            pixel.green(),
            pixel.blue(),
            pixel.alpha(),
        ]));
    }

    Ok(Value::from_image(DynamicImage::ImageRgba8(img_buf)))
}
```

### Format Normalization Utility

Image commands should normalize format strings to handle variations and case-insensitivity:

```rust
/// Normalize image format string to canonical form.
/// Case-insensitive, supports common variations.
/// Returns normalized format (lowercase) or error if unsupported.
pub fn normalize_format(format: &str) -> Result<String, Error> {
    let normalized = match format.trim().to_lowercase().as_str() {
        // PNG variants
        "png" => "png",

        // JPEG variants (jpg, jpeg, JPG, JPEG all map to "jpeg")
        "jpg" | "jpeg" | "jpe" => "jpeg",

        // WebP variants
        "webp" => "webp",

        // GIF variants
        "gif" => "gif",

        // BMP variants
        "bmp" | "dib" => "bmp",

        // TIFF variants
        "tiff" | "tif" => "tiff",

        // ICO variants
        "ico" => "ico",

        // Empty string defaults to PNG
        "" => "png",

        _ => return Err(Error::general_error(
            format!("Unsupported image format '{}'. Use: png, jpg/jpeg, webp, gif, bmp, tiff/tif, ico", format)
        )),
    };

    Ok(normalized.to_string())
}

/// Map normalized format to image::ImageFormat enum.
pub fn format_to_image_format(format: &str) -> Result<image::ImageFormat, Error> {
    match format {
        "png" => Ok(image::ImageFormat::Png),
        "jpeg" => Ok(image::ImageFormat::Jpeg),
        "webp" => Ok(image::ImageFormat::WebP),
        "gif" => Ok(image::ImageFormat::Gif),
        "bmp" => Ok(image::ImageFormat::Bmp),
        "tiff" => Ok(image::ImageFormat::Tiff),
        "ico" => Ok(image::ImageFormat::Ico),
        _ => Err(Error::general_error(format!("Unknown format: {}", format))),
    }
}

/// Map normalized format to MIME type.
pub fn format_to_mime_type(format: &str) -> Result<&'static str, Error> {
    match format {
        "png" => Ok("image/png"),
        "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        "bmp" => Ok("image/bmp"),
        "tiff" => Ok("image/tiff"),
        "ico" => Ok("image/x-icon"),
        _ => Err(Error::general_error(format!("Unknown format: {}", format))),
    }
}
```

### Data URL Conversion

The `to_dataurl` command converts an image to a base64-encoded data URL using file extensions:

```rust
use base64::{Engine as _, engine::general_purpose};
use crate::image::util::{normalize_format, format_to_image_format, format_to_mime_type};

fn to_dataurl(state: &State<Value>, format_str: String) -> Result<Value, Error> {
    let img = try_to_image(state)?;

    // Normalize format (handles jpg/jpeg/JPG variations, case-insensitive)
    let normalized_format = normalize_format(&format_str)?;

    // Map to image format and MIME type
    let img_format = format_to_image_format(&normalized_format)?;
    let mime_type = format_to_mime_type(&normalized_format)?;

    // Encode to bytes
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), img_format)
        .map_err(|e| Error::general_error(format!("Failed to encode image: {}", e)))?;

    // Base64 encode
    let b64 = general_purpose::STANDARD.encode(&buf);
    let data_url = format!("data:{};base64,{}", mime_type, b64);

    Ok(Value::from_string(data_url))
}
```

**Examples**:
```rust
// All these work (case-insensitive, variation support):
to_dataurl("")        // → data:image/png;base64,... (default)
to_dataurl("png")     // → data:image/png;base64,...
to_dataurl("PNG")     // → data:image/png;base64,... (normalized)
to_dataurl("jpg")     // → data:image/jpeg;base64,...
to_dataurl("jpeg")    // → data:image/jpeg;base64,... (same as jpg)
to_dataurl("JPEG")    // → data:image/jpeg;base64,... (normalized)
to_dataurl("webp")    // → data:image/webp;base64,...
```

### Serialization (DefaultValueSerializer)

Implement `DefaultValueSerializer` for `ExtValue::Image` to support automatic serialization with format normalization:

```rust
use crate::image::util::{normalize_format, format_to_image_format};

impl DefaultValueSerializer for ExtValue {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match self {
            ExtValue::Image { value } => {
                // Normalize format (handles jpg/jpeg/JPG, case-insensitive)
                let normalized = normalize_format(format)
                    .map_err(|e| Error::new(ErrorType::SerializationError, e.to_string()))?;

                let img_format = format_to_image_format(&normalized)
                    .map_err(|e| Error::new(ErrorType::SerializationError, e.to_string()))?;

                let mut buf = Vec::new();
                value.write_to(&mut std::io::Cursor::new(&mut buf), img_format)
                    .map_err(|e| Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize image: {}", e),
                    ))?;
                Ok(buf)
            }
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format {}", format),
            )),
        }
    }

    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        if type_identifier != "image" {
            return Err(Error::new(
                ErrorType::SerializationError,
                format!("Expected 'image' type, got '{}'", type_identifier),
            ));
        }

        // Normalize format (supports jpg/jpeg/JPG variations, case-insensitive)
        // Special handling for auto-detect formats
        let img_format = match fmt.trim().to_lowercase().as_str() {
            "binary" | "bin" | "b" | "" => None, // Auto-detect
            other => {
                let normalized = normalize_format(other)
                    .map_err(|e| Error::new(ErrorType::SerializationError, e.to_string()))?;
                Some(format_to_image_format(&normalized)
                    .map_err(|e| Error::new(ErrorType::SerializationError, e.to_string()))?)
            }
        };

        let img = if let Some(format) = img_format {
            image::load_from_memory_with_format(b, format)
        } else {
            image::load_from_memory(b)
        }
        .map_err(|e| Error::new(
            ErrorType::SerializationError,
            format!("Failed to deserialize image: {}", e),
        ))?;

        Ok(ExtValue::Image { value: Arc::new(img) })
    }
}
```

**Format Normalization Benefits**:
- **Case-insensitive**: `PNG`, `png`, `Png` all work
- **Variations supported**: `jpg`, `jpeg`, `JPG`, `JPEG` → all map to `jpeg`
- **Consistent**: Same normalization logic across all image operations
- **Clear errors**: Helpful message listing supported formats

### ExtValueInterface Extension

Add image methods to `ExtValueInterface` in `liquers-lib/src/value/mod.rs`:

```rust
pub trait ExtValueInterface {
    fn from_image(image: image::DynamicImage) -> Self;
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error>;
    // ... existing methods ...
}

impl ExtValueInterface for ExtValue {
    fn from_image(image: image::DynamicImage) -> Self {
        ExtValue::Image { value: Arc::new(image) }
    }

    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error> {
        match self {
            ExtValue::Image { value } => Ok(value.clone()),
            _ => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "Image"
            )),
        }
    }
    // ... existing implementations ...
}

impl ExtValueInterface for Value {
    fn from_image(image: image::DynamicImage) -> Self {
        Value::Extended(ExtValue::from_image(image))
    }

    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error> {
        match self {
            Value::Extended(ext) => ext.as_image(),
            _ => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "Image"
            )),
        }
    }
    // ... existing implementations ...
}
```

### Color Parsing Utility

Drawing operations (Phase 2) require color parsing. Implement a `parse_color` utility in `util.rs` that returns native `image::Rgba<u8>`:

```rust
use image::Rgba;

/// Parse a color string into image::Rgba<u8>.
/// Supports named colors and hex format with 0x prefix.
pub fn parse_color(s: &str) -> Result<Rgba<u8>, Error> {
    let s = s.trim().to_lowercase();

    // Try named colors first
    match s.as_str() {
        "black"   => Ok(Rgba([0, 0, 0, 255])),
        "white"   => Ok(Rgba([255, 255, 255, 255])),
        "red"     => Ok(Rgba([255, 0, 0, 255])),
        "green"   => Ok(Rgba([0, 255, 0, 255])),
        "blue"    => Ok(Rgba([0, 0, 255, 255])),
        "yellow"  => Ok(Rgba([255, 255, 0, 255])),
        "cyan"    => Ok(Rgba([0, 255, 255, 255])),
        "magenta" => Ok(Rgba([255, 0, 255, 255])),
        "gray" | "grey" => Ok(Rgba([128, 128, 128, 255])),
        "orange"  => Ok(Rgba([255, 165, 0, 255])),
        "purple"  => Ok(Rgba([128, 0, 128, 255])),
        "brown"   => Ok(Rgba([165, 42, 42, 255])),
        "pink"    => Ok(Rgba([255, 192, 203, 255])),
        "lime"    => Ok(Rgba([0, 255, 0, 255])),
        "navy"    => Ok(Rgba([0, 0, 128, 255])),
        "teal"    => Ok(Rgba([0, 128, 128, 255])),
        "olive"   => Ok(Rgba([128, 128, 0, 255])),
        "maroon"  => Ok(Rgba([128, 0, 0, 255])),
        "silver"  => Ok(Rgba([192, 192, 192, 255])),
        _ => {
            // Try hex with 0x prefix
            if let Some(hex) = s.strip_prefix("0x") {
                match hex.len() {
                    6 => {
                        // RRGGBB (opaque)
                        let r = u8::from_str_radix(&hex[0..2], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad R component", s)
                            ))?;
                        let g = u8::from_str_radix(&hex[2..4], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad G component", s)
                            ))?;
                        let b = u8::from_str_radix(&hex[4..6], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad B component", s)
                            ))?;
                        Ok(Rgba([r, g, b, 255]))
                    }
                    8 => {
                        // RRGGBBAA (with alpha)
                        let r = u8::from_str_radix(&hex[0..2], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad R component", s)
                            ))?;
                        let g = u8::from_str_radix(&hex[2..4], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad G component", s)
                            ))?;
                        let b = u8::from_str_radix(&hex[4..6], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad B component", s)
                            ))?;
                        let a = u8::from_str_radix(&hex[6..8], 16)
                            .map_err(|_| Error::general_error(
                                format!("Invalid hex color '{}': bad A component", s)
                            ))?;
                        Ok(Rgba([r, g, b, a]))
                    }
                    _ => {
                        Err(Error::general_error(
                            format!("Invalid hex color '{}': expected 6 or 8 hex digits after 0x", s)
                        ))
                    }
                }
            } else {
                Err(Error::general_error(
                    format!("Unknown color '{}'. Use named color or hex with 0x prefix (e.g., 0xFF0000)", s)
                ))
            }
        }
    }
}
```

**Usage Example**:
```rust
use imageproc::drawing::draw_line_segment_mut;

fn draw_line(state: &State<Value>, x1: i32, y1: i32, x2: i32, y2: i32, color_str: String) -> Result<Value, Error> {
    let img = try_to_image(state)?;
    let color = parse_color(&color_str)?;

    // Convert DynamicImage to mutable buffer (example with Rgba8)
    let mut rgba_img = img.to_rgba8();

    // Draw line using imageproc with native Rgba<u8> color
    draw_line_segment_mut(
        &mut rgba_img,
        (x1 as f32, y1 as f32),
        (x2 as f32, y2 as f32),
        color
    );

    Ok(Value::from_image(DynamicImage::ImageRgba8(rgba_img)))
}
```

**Note**: Using `image::Rgba<u8>` directly is more efficient and idiomatic than converting from float tuples. The `imageproc` crate expects `image::Rgba<u8>` for drawing operations.

### egui Rendering Support

The new `Image` type (DynamicImage) should support rendering in egui, similar to the deprecated `RasterImage`. Add a `show()` method or implement a trait:

```rust
use egui;
use image::DynamicImage;

/// Extension trait for rendering DynamicImage in egui
pub trait DynamicImageEguiExt {
    fn show(&self, ui: &mut egui::Ui, id: egui::Id, zoom: f32);
}

impl DynamicImageEguiExt for DynamicImage {
    fn show(&self, ui: &mut egui::Ui, id: egui::Id, zoom: f32) {
        // Convert DynamicImage to RGBA8
        let rgba8 = self.to_rgba8();
        let (width, height) = rgba8.dimensions();

        // Convert to egui::ColorImage
        let pixels = rgba8.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &pixels,
        );

        // Load as texture
        let texture = ui.ctx().load_texture(
            format!("dynamic_image_{:?}", id),
            color_image,
            Default::default(),
        );

        // Display with zoom
        let size = egui::Vec2::new(width as f32 * zoom, height as f32 * zoom);
        let image_widget = egui::Image::from_texture(&texture).fit_to_exact_size(size);
        ui.add(image_widget);
    }
}
```

**Usage**:
```rust
use crate::image::util::DynamicImageEguiExt;

// In a widget or UI command:
let img: Arc<DynamicImage> = ...;
img.show(ui, egui::Id::new("my_image"), 1.0);
```

**Integration with Widget System**:
The `ExtValue::Image` should be renderable in the egui widget system. Update widget handling to support Image values alongside existing types (PolarsDataFrame, UiCommand, etc.).

### Edge Cases & Error Handling

**Invalid Dimensions**:
- `resize-0-100` or `resize-800-0`
- Error: `"Image dimensions must be greater than 0"`

**Out of Bounds Crop**:
- `crop-1000-1000-500-500` on 800x600 image
- Error: `"Crop region (x=1000, y=1000, w=500, h=500) exceeds image bounds (800x600)"`

**Invalid Quality**:
- `to_jpeg-200` (quality > 100)
- Error: `"JPEG quality must be between 1 and 100"`

**Invalid Method**:
- `resize-800-600-invalid_method`
- Error: `"Invalid resize method 'invalid_method'. Use: nearest, triangle, catmullrom, gaussian, lanczos3"`

**Format Not Supported**:
- State has `data_format: "pdf"`
- Error: `"Unsupported image format 'pdf'. Supported: png, jpeg, webp, gif, bmp, tiff, ico"`

**SVG Parse Error**:
- Malformed SVG data
- Error: `"Failed to parse SVG: <specific error from usvg>"`

**Negative Brightness** (tilde syntax):
- `brighten-~10` (darken by 10, tilde indicates negative)
- Tilde before digit decodes to `-`: `~42` → `-42`
- Required because dash is argument separator

**Invalid Color** (Phase 2 drawing commands):
- `draw_line-10-10-100-100-invalidcolor`
- Error: `"Unknown color 'invalidcolor'. Use named color or hex with 0x prefix (e.g., 0xFF0000)"`

**Invalid Hex Color**:
- `draw_rect-0-0-100-100-0xGGHHII` (invalid hex digits)
- Error: `"Invalid hex color '0xGGHHII': bad R component"`
- `draw_rect-0-0-100-100-0xFFFF` (wrong length)
- Error: `"Invalid hex color '0xFFFF': expected 6 or 8 hex digits after 0x"`

**Missing 0x Prefix**:
- `draw_circle-100-100-50-FF0000` (hex without prefix - ambiguous)
- Error: `"Unknown color 'ff0000'. Use named color or hex with 0x prefix (e.g., 0xFF0000)"`

**Rotation Angle**:
- `rotate-~45` (negative angle, counter-clockwise)
- Works correctly (tilde indicates negative: `~45` → `-45`)

## Phase 1 Implementation (MVP)

Essential commands covering 80% of use cases:

**I/O** (3):
- `from_bytes`, `from_format`, `svg_to_image`

**Format Conversion** (3):
- `to_png`, `to_jpeg`, `to_dataurl`

**Color Format** (1):
- `color_format` (single command with format enum: rgb8, rgba8, luma8, luma_alpha8)

**Geometric** (10):
- `resize`, `resize_by`, `thumb`, `crop`, `rotate`, `rot90`, `rot180`, `rot270`, `fliph`, `flipv`

**Color Operations** (5):
- `gray`, `invert`, `brighten`, `contrast`, `huerot`

**Filtering** (2):
- `blur`, `sharpen`

**Info** (4):
- `dims`, `width`, `height`, `colortype`

**Total MVP**: 28 commands

## Recommended Implementation Order

### Step 1: Foundation & Utilities (2 days)
1. **Feature flag setup**: Add `image-support` feature to `Cargo.toml` that controls image module and `imageproc` dependency
2. **Module setup**: Update `liquers-lib/src/image/mod.rs` and create submodules (all behind feature flag)
3. **Core utility** (`util.rs`):
   - `try_to_image<V>(state: &State<V>) -> Result<Arc<DynamicImage>, Error>` - Extract image from state
   - `normalize_format(format: &str) -> Result<String, Error>` - Normalize format strings (case-insensitive, handles jpg/jpeg variations)
   - `format_to_image_format(format: &str) -> Result<image::ImageFormat, Error>` - Map to image crate enum
   - `format_to_mime_type(format: &str) -> Result<&'static str, Error>` - Map to MIME type for data URLs
   - `parse_color(s: &str) -> Result<image::Rgba<u8>, Error>` (for Phase 2, can implement now)
4. **ExtValue updates**:
   - Replace `RasterImage` with `Arc<DynamicImage>` in `ExtValue::Image`
   - Update `ExtValueInterface` trait (add `from_image`, `as_image`)
   - Implement `DefaultValueSerializer` for Image
5. **egui integration**:
   - Add `DynamicImageEguiExt` trait with `show()` method
   - Update widget system to render Image values
6. **Test utilities**: Sample image generators

### Step 2: I/O Operations (1 day)
5. **`from_bytes`** - Auto-detect format
6. **`from_format`** - Explicit format loading
7. **`to_png`** - Basic output
8. **Test**: Load PNG/JPEG, verify DynamicImage structure

### Step 3: Format Conversion (1 day)
9. **`to_jpeg`** - With quality parameter
10. **`to_dataurl`** - Base64 encoding
11. **`color_format`** - Single command with enum argument (replaces separate to_rgb8, to_rgba8, etc.)
12. **Test**: Convert formats, verify output

### Step 4: Info Commands (0.5 days)
13. **`dims`**, **`width`**, **`height`**, **`colortype`**
14. **Test**: Verify metadata extraction

### Step 5: Geometric Transformations (2-3 days)
15. **`resize`** with filter type parameter (EnumArgumentType for method)
16. **`resize_by`** - Percentage-based scaling (calculate dimensions, then resize)
17. **`thumb`** - Aspect ratio preservation
18. **`crop`** - With bounds checking
19. **Rotation (fixed angles)**: `rot90`, `rot180`, `rot270`
20. **Rotation (arbitrary angle)**: `rotate` with angle and method parameter (uses imageproc)
21. **Flipping**: `fliph`, `flipv`
22. **Test**: Chain transformations, verify dimensions, test rotation methods, test percentage scaling

### Step 6: Color Operations (1 day)
23. **`gray`**, **`invert`**
24. **`brighten`**, **`contrast`**
25. **`huerot`**
26. **Test**: Color transformations on various formats, test tilde syntax for negative brightness

### Step 7: Filtering (1-2 days)
27. **`blur`** - Gaussian blur with sigma (uses imageproc)
28. **`sharpen`** - Unsharp mask or imageproc sharpen
29. **Test**: Filter effects on test images

### Step 8: SVG Rendering (1 day)
30. **`svg_to_image`** - Resvg integration
31. **Test**: Render SVG to various sizes

### Step 9: Integration Testing & Refinement (1 day)
32. **Complex pipelines**: Combine load, transform, filter, save
33. **Error handling**: Test edge cases
34. **Documentation**: Verify examples
35. **Remove RasterImage**: Clean up deprecated code from `liquers-lib/src/image/raster_image.rs`

### Total Estimated Effort: 11-13 days for MVP

**Dependencies** (in `liquers-lib/Cargo.toml`):
```toml
[features]
default = []
image-support = ["dep:image", "dep:imageproc", "dep:resvg", "dep:usvg", "dep:tiny-skia", "dep:base64"]

[dependencies]
# Image support (optional, controlled by feature flag)
image = { version = "0.25", features = ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico"], optional = true }
imageproc = { version = "0.26", optional = true }
resvg = { version = "0.43", optional = true }
usvg = { version = "0.43", optional = true }
tiny-skia = { version = "0.11", optional = true }
base64 = { version = "0.22", optional = true }
```

**Note**: The entire `liquers-lib/src/image/` module should be conditionally compiled:
```rust
#[cfg(feature = "image-support")]
pub mod image;
```

## Related Documentation

- [image crate docs](https://docs.rs/image/0.25.9/image/)
- [imageproc crate docs](https://docs.rs/imageproc/0.26.0/imageproc/)
- [resvg docs](https://docs.rs/resvg/latest/resvg/)
- `specs/COMMAND_REGISTRATION_GUIDE.md` - How to register commands
- `specs/POLARS_COMMAND_LIBRARY.md` - Analogous command library pattern
- `CLAUDE.md` - Architecture and module organization
- `PROJECT_OVERVIEW.md` - Liquers query language design
