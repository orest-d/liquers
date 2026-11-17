---
id: IMAGE-CANNOT-BE-WRITTEN-IN-DISPLAY-FORMATS
kind: feature
title: An image cannot be read or written in the raw display formats that liquers-embedded devices show
status: draft
priority: P2
complexity: L
area: [lib/image, lib/value]
design:
created: 2026-11-17
github:
---

## Problem

liquers-embedded defines a family of raw image formats for microcontroller displays, **LQI**
(reference design `specs/design/display-images/` in `orest-d/liquers-embedded`, to become its
`specs/reference/DISPLAY_IMAGES.md`). These are pixels in the layout a display controller takes
directly, behind a 16-byte header (`LQI\x01`, format, compression, palette size, width, height,
stride, frames, flags), each format named by a three-character extension:

| Ext | Format | Ext | Format |
|---|---|---|---|
| `i1h` | 1 bpp row-major, 1 = light | `i8g` | 8-bit grey |
| `i1v` | 1 bpp in 8-row pages (SSD1306) | `i8p` | 8-bit palette |
| `i1t` | two 1 bpp planes (tri-colour e-paper) | `i8r` | RGB332 |
| `i2g` / `i2p` | 2-bit grey / palette | `i5b` | RGB565 big-endian (MIPI TFTs) |
| `i4g` / `i4p` | 4-bit grey / palette | `i5l` | RGB565 little-endian |
| | | `i6r` / `i3r` | RGB666 / RGB888 |

The pixel data may be **compressed** inside the header (`compression` byte): `1` run-length
(LVGL's scheme: a control byte, high bit = literal run, otherwise a repeat count) and `2` heatshrink
(LZSS, window 2^8, lookahead 2^4). A file may hold several frames (an animation or sprite sheet).

Upstream's `Image` type (`liquers-lib/src/value/mod.rs` `type_descriptions`, `image/serde.rs`)
knows none of these formats, and none of the **compressed conventional formats** a device reads
either: **QOI**, **TGA** (with its RLE) and binary **Netpbm** (`pbm`, `pgm`, `ppm`). Of the formats
a device reads, upstream lists only `bmp` (uncompressed; BMP's RLE4/RLE8 are read by the `image`
crate but not written). So a host running upstream cannot prepare an image in the form a device
shows unchanged.

## Impact

Every image for a device needs an external converter (a Python script per project, as in
`orest-d/rp2040badge`). The device-side design wants the host to do the heavy work (resizing,
dithering, palette reduction, rendering) and send a ready frame; without these formats the
Liquers way of asking for one, a query ending in the filename (`…/cat.i5b`), does not work
upstream.

## Expected behaviour

- `Image` gains the LQI extensions as data formats: serializing an image to `cat.i5b` writes the LQI
  header and RGB565 big-endian pixels, and reading LQI bytes yields an `Image`, with the header
  authoritative on read and the extension choosing the format on write. Writing with compression
  (run-length, heatshrink) is chosen by a command argument, since the extension names the pixel
  format only.
- `Image` also gains `qoi`, `tga` and `pbm`/`pgm`/`ppm` as data formats (the `image` crate already
  reads and writes them, behind its cargo features).
- `ns-img` gains what preparing a device frame needs and is not there yet: reducing to a palette
  (`i4p` for ACeP e-paper, `i8p`), monochrome with a threshold or dithering, and the page layout of
  `i1v`, so that `-R/pics/cat.png/-/ns-img/resize-240-240/cat.i5b` gives a file a GC9A01 shows as it
  is.
- The type identifier: the device uses `lqe.Image` until upstream reads the formats, then
  upstream's `Image`.

## Discovery

Raised by the maintainer on 2026-11-17 while designing liquers-embedded's display image formats
(project thread "Design display image formats"; draft PR `orest-d/liquers-embedded#18`), which asked
for an upstream issue covering the LQI formats and the compressed image formats.
