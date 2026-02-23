# IMAGE-SERIALIZATION-FEATURE-GAPS

Status: Draft

## Summary
Standardize image serialization/deserialization in `liquers_lib::image` with reusable utility functions, explicit format mapping, and integration into `DefaultValueSerializer` for `ExtValue::Image`.

## Scope
1. Add utility functions for image serialization/deserialization in `liquers_lib::image`.
2. Utilities should be format-driven (`data_format` string) and writer/reader based.
3. Integrate these utilities into `DefaultValueSerializer` for image extended values.
4. Align image command handlers (`from_bytes`, `from_format`, `to_*`) to shared utilities.
5. Define canonical format names and aliases (`jpg`/`jpeg`, etc.).

## Primary Findings (Current Liquers, Rust image crate)

### 1. Serializer gap in `ExtValue`
1. `ExtValue::Image` exists and is used by image commands.
2. `ExtValue::default_extension/default_filename/default_media_type` already report image defaults (`png`, `image.png`, `image/png`).
3. `DefaultValueSerializer for ExtValue` currently implements only `PolarsDataFrame` serialization/deserialization.
4. Result: generic store/cache serialization for `type_identifier == "image"` is missing.

### 2. Image format support is spread across commands
1. `image/io.rs` supports loading from bytes (`from_bytes`) and explicit format (`from_format`).
2. `image/format.rs` supports writing to: `png`, `jpeg`, `webp`, `gif`, `bmp`, and `dataurl`.
3. Parsing and normalization helpers exist in `image/util.rs` (`normalize_format`, `format_to_image_format`, `format_to_mime_type`).
4. Serialization logic is duplicated across command implementations (manual `write_to` patterns).

### 3. Reader/writer shape and async-forward compatibility
1. `image` crate encode/decode APIs are synchronous (`Read`/`Write` and in-memory bytes).
2. Current Liquers serialization APIs are byte-vector based (`as_bytes`, `deserialize_from_bytes`).
3. Future async writer support (for stores/streaming) will require adapters, but a writer-based utility API now keeps migration straightforward.

### 4. Data URL is a transport representation, not canonical binary format
1. `dataurl` output is supported by command API and useful for UI/web embedding.
2. `dataurl` is text-wrapped binary and less suitable as canonical persisted format.
3. For serializer defaults, binary formats (PNG/JPEG/WebP/...) should be primary; `dataurl` can remain optional.

## Format Matrix (Image-oriented)

| Format Name | Extension | `data_format` strings (proposed) | Serialize | Deserialize | Backend | Notes |
|---|---|---|---|---|---|---|
| PNG | `png` | `png` | Yes | Yes | `image::ImageFormat::Png` | canonical default |
| JPEG | `jpg`,`jpeg`,`jpe` | `jpeg`, `jpg` | Yes | Yes | `image::ImageFormat::Jpeg` | aliases normalize to `jpeg` |
| WebP | `webp` | `webp` | Yes | Yes | `image::ImageFormat::WebP` | |
| GIF | `gif` | `gif` | Yes | Yes | `image::ImageFormat::Gif` | single-frame in current flow |
| BMP | `bmp` | `bmp` | Yes | Yes | `image::ImageFormat::Bmp` | |
| TIFF | `tif`,`tiff` | `tiff`, `tif` | Yes | Yes | `image::ImageFormat::Tiff` | |
| ICO | `ico` | `ico` | Yes | Yes | `image::ImageFormat::Ico` | |
| Data URL | n/a | `dataurl` | Yes | Optional | base64 + mime | text transport, non-canonical |
| Auto-detect | n/a | `image`, `auto` | n/a | Yes | `load_from_memory` | deserialize convenience |

## Minimum Required Support (this feature request)
1. Deserialization: `png`, `jpeg`/`jpg`, `webp`, `gif`, `bmp`, `tiff`, `ico`, and auto-detect (`image`/`auto`).
2. Serialization: `png`, `jpeg`/`jpg`, `webp`, `gif`, `bmp`, `tiff`, `ico`.
3. Optional serialization format: `dataurl`.

## Proposed API Design

### 1. New utility module
Create `liquers_lib::image::serde` with:

1. `parse_image_data_format(data_format: &str) -> Result<ImageDataFormat, Error>`
2. `deserialize_image_from_bytes(bytes: &[u8], data_format: &str) -> Result<image::DynamicImage, Error>`
3. `serialize_image_to_writer<W: Write>(img: &image::DynamicImage, data_format: &str, writer: W) -> Result<(), Error>`
4. `serialize_image_to_bytes(img: &image::DynamicImage, data_format: &str) -> Result<Vec<u8>, Error>`

### 2. Format enum
Define:
`enum ImageDataFormat { Png, Jpeg, Webp, Gif, Bmp, Tiff, Ico, DataUrl, Auto }`

Rules:
1. `jpg`/`jpeg`/`jpe` normalize to `Jpeg`.
2. `image` and `auto` map to `Auto` for deserialization only.
3. Unknown format returns `ErrorType::SerializationError`.

### 3. Serialization behavior
1. Binary formats write encoded bytes directly.
2. `Jpeg` may support quality options in command APIs; serializer uses fixed default quality unless extended format syntax is introduced.
3. `DataUrl` (if supported) returns UTF-8 bytes of `data:<mime>;base64,<payload>`.

### 4. Deserialization behavior
1. `Auto` uses `image::load_from_memory`.
2. Explicit formats use `image::load_from_memory_with_format`.
3. `dataurl` deserialization can be deferred unless there is a strong requirement.

## Integration Into `DefaultValueSerializer`

Target: `liquers-lib/src/value/mod.rs` (`impl DefaultValueSerializer for ExtValue`).

Plan:
1. For `ExtValue::Image` in `as_bytes(format)`:
   1. call `serialize_image_to_bytes`.
2. For `deserialize_from_bytes(...)` when `type_identifier == "image"`:
   1. call `deserialize_image_from_bytes`.
   2. return `ExtValue::from_image`.
3. Keep existing Polars and non-image branches unchanged.

## Complete Implementation Plan

### Phase 1: Image serde utilities
1. Add `image::serde` module with `ImageDataFormat` and parser.
2. Implement binary format serialize/deserialize helpers.
3. Add tests:
   1. alias parsing (`jpg`/`jpeg`),
   2. roundtrip for `png`, `jpeg`, `webp`,
   3. unsupported format errors.

### Phase 2: Serializer integration
1. Wire `ExtValue::Image` into `DefaultValueSerializer`.
2. Add tests for:
   1. `as_bytes("png")` + `deserialize_from_bytes(..., "image", "png")`,
   2. `jpeg` alias behavior,
   3. unsupported format handling.

### Phase 3: Command alignment
1. Refactor `image/io.rs` and `image/format.rs` to reuse shared `image::serde` helpers.
2. Remove duplicated encode/decode logic.
3. Ensure command metadata docs use canonical `data_format` names.

### Phase 4: Optional dataurl extensions
1. Decide if `dataurl` must be part of `DefaultValueSerializer` or command-only.
2. If included, add robust parse/validate path for deserialization.

## Open Decisions / Ambiguities Requiring Confirmation
1. Should `dataurl` be part of core serializer contract or remain command-level only?
2. Should JPEG quality be configurable via serializer format syntax (e.g. `jpeg:q85`)?
3. Is GIF animation support needed in serializer, or is single-frame sufficient?
4. Should SVG rasterization (`svg_to_image`) be represented as an image deserializer format (e.g. `svg`) or remain command-level?

## Suggested Acceptance Criteria
1. Shared `image::serde` module exists and is used by image serializer + image commands.
2. `ExtValue::Image` can roundtrip through `DefaultValueSerializer`.
3. Canonical image data format mapping is test-covered.
4. No duplicated binary encode/decode paths remain in command handlers.

## References (Primary)
1. `liquers-lib/src/image/io.rs`
2. `liquers-lib/src/image/format.rs`
3. `liquers-lib/src/image/util.rs`
4. `liquers-lib/src/value/mod.rs`
5. Rust `image` crate encode/decode APIs (`DynamicImage::write_to`, `load_from_memory`, `load_from_memory_with_format`)
