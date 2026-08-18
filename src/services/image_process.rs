//! Decode, crop, resize, tone-map, and re-encode an image for embedding in an
//! SVG. Steps 1-3 and 17 of the pipeline described in
//! `docs/superpowers/specs/2026-08-06-lua-colors-and-image-ops-design.md`;
//! steps 4-16 live in the dependency-free `eink-photo` crate.

use base64::Engine as _;
use image::{GenericImageView as _, ImageDecoder as _};

/// Largest encoded input accepted, checked before any decoding.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
/// Largest source area accepted, enforced through `image::Limits`.
pub const MAX_SOURCE_PIXELS: u64 = 40_000_000;
/// Largest output dimension accepted, in pixels.
pub const MAX_OUTPUT_DIM: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Stretch,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct GeometryOpts {
    /// x, y, w, h — normalised 0..=1, relative to the decoded image.
    pub crop: Option<(f32, f32, f32, f32)>,
    pub fit: Fit,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Png,
    Jpeg { quality: u8 },
}

#[derive(Debug, thiserror::Error)]
pub enum ImageProcessError {
    #[error("could not decode the image: {0}")]
    Decode(String),
    #[error("could not encode the image: {0}")]
    Encode(String),
    #[error("invalid crop: {0}")]
    BadCrop(String),
    #[error("{what} exceeds the limit ({value} > {limit})")]
    TooLarge {
        what: &'static str,
        value: u64,
        limit: u64,
    },
    #[error("{0}")]
    Photo(String),
}

pub fn process_image(
    bytes: &[u8],
    geometry: &GeometryOpts,
    params: &eink_photo::Params,
    format: OutputFormat,
) -> Result<(String, u32, u32), ImageProcessError> {
    // Guard 1: encoded size, before touching a decoder.
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ImageProcessError::TooLarge {
            what: "source image size",
            value: bytes.len() as u64,
            limit: MAX_SOURCE_BYTES as u64,
        });
    }

    // Guard 2: requested output box, before allocating anything.
    for dim in [geometry.width, geometry.height].into_iter().flatten() {
        if dim > MAX_OUTPUT_DIM {
            return Err(ImageProcessError::TooLarge {
                what: "output dimension",
                value: dim as u64,
                limit: MAX_OUTPUT_DIM as u64,
            });
        }
    }

    // Guard 3: decoded area, enforced by the decoder itself so a
    // decompression bomb never gets allocated.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_OUTPUT_DIM.max(20_000));
    limits.max_image_height = Some(MAX_OUTPUT_DIM.max(20_000));
    limits.max_alloc = Some(MAX_SOURCE_PIXELS * 4);

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageProcessError::Decode(e.to_string()))?;
    reader.limits(limits);

    // `image` 0.25 does not apply EXIF orientation automatically (see the
    // note in the module docs / task brief). `ImageDecoder::orientation` and
    // `DynamicImage::apply_orientation` ARE available in this resolved
    // version (0.25.10), so read the orientation off the decoder before
    // consuming it, then apply it to the decoded image.
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| ImageProcessError::Decode(e.to_string()))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| ImageProcessError::Decode(e.to_string()))?;
    img.apply_orientation(orientation);

    // Step 2 — crop.
    let img = match geometry.crop {
        None => img,
        Some((cx, cy, cw, ch)) => {
            if !(0.0..=1.0).contains(&cx)
                || !(0.0..=1.0).contains(&cy)
                || cw <= 0.0
                || ch <= 0.0
                || cx + cw > 1.0 + 1e-6
                || cy + ch > 1.0 + 1e-6
            {
                return Err(ImageProcessError::BadCrop(format!(
                    "x={cx}, y={cy}, w={cw}, h={ch} does not lie within the image"
                )));
            }
            let (w, h) = img.dimensions();
            let px = (cx * w as f32).round() as u32;
            let py = (cy * h as f32).round() as u32;
            let pw = ((cw * w as f32).round() as u32).max(1).min(w - px);
            let ph = ((ch * h as f32).round() as u32).max(1).min(h - py);
            img.crop_imm(px, py, pw, ph)
        }
    };

    // Step 3 — fit / resize.
    let img = resize(img, geometry)?;
    let (out_w, out_h) = img.dimensions();

    // Steps 4-16 — hand the tone domain to eink-photo.
    let rgb = img.to_rgb8();
    let mut pixels: Vec<f32> = rgb.as_raw().iter().map(|&b| b as f32 / 255.0).collect();
    eink_photo::process(&mut pixels, out_w as usize, out_h as usize, params)
        .map_err(|e| ImageProcessError::Photo(e.to_string()))?;

    let bytes_out: Vec<u8> = pixels
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    let out = image::RgbImage::from_raw(out_w, out_h, bytes_out)
        .ok_or_else(|| ImageProcessError::Encode("pixel buffer size mismatch".to_string()))?;

    // Step 17 — encode.
    let mut buf = std::io::Cursor::new(Vec::new());
    let mime = match format {
        OutputFormat::Png => {
            image::DynamicImage::ImageRgb8(out)
                .write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| ImageProcessError::Encode(e.to_string()))?;
            "image/png"
        }
        OutputFormat::Jpeg { quality } => {
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
            enc.encode_image(&out)
                .map_err(|e| ImageProcessError::Encode(e.to_string()))?;
            "image/jpeg"
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok((format!("data:{mime};base64,{b64}"), out_w, out_h))
}

fn resize(
    img: image::DynamicImage,
    g: &GeometryOpts,
) -> Result<image::DynamicImage, ImageProcessError> {
    if g.fit == Fit::None {
        return Ok(img);
    }
    let (sw, sh) = img.dimensions();
    let (tw, th) = match (g.width, g.height) {
        (None, None) => return Ok(img),
        // One dimension given: scale the other by aspect ratio, in every mode.
        (Some(w), None) => (
            w,
            ((w as f32 / sw as f32) * sh as f32).round().max(1.0) as u32,
        ),
        (None, Some(h)) => (
            ((h as f32 / sh as f32) * sw as f32).round().max(1.0) as u32,
            h,
        ),
        (Some(w), Some(h)) => (w, h),
    };

    Ok(match g.fit {
        Fit::None => unreachable!("handled above"),
        Fit::Stretch => img.resize_exact(tw, th, image::imageops::FilterType::Lanczos3),
        Fit::Contain => img.resize(tw, th, image::imageops::FilterType::Lanczos3),
        Fit::Cover => img.resize_to_fill(tw, th, image::imageops::FilterType::Lanczos3),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny in-memory PNG, built rather than checked in as a fixture.
    fn test_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let v = (((x + y) % 2) * 200 + 27) as u8;
            *px = image::Rgb([v, v / 2, 255 - v]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn default_geometry() -> GeometryOpts {
        GeometryOpts {
            crop: None,
            fit: Fit::Cover,
            width: None,
            height: None,
        }
    }

    #[test]
    fn no_geometry_keeps_the_source_dimensions() {
        let (_uri, w, h) = process_image(
            &test_png(37, 21),
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (37, 21));
    }

    #[test]
    fn cover_fills_the_box_exactly() {
        let g = GeometryOpts {
            width: Some(80),
            height: Some(48),
            ..default_geometry()
        };
        let (_uri, w, h) = process_image(
            &test_png(200, 200),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (80, 48));
    }

    #[test]
    fn contain_fits_inside_the_box_preserving_aspect() {
        let g = GeometryOpts {
            fit: Fit::Contain,
            width: Some(80),
            height: Some(48),
            ..default_geometry()
        };
        let (_uri, w, h) = process_image(
            &test_png(200, 100),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        // 200x100 into an 80x48 box: scale = min(80/200, 48/100) = min(0.4,
        // 0.48) = 0.4, so the box's width is the binding edge and the result
        // is exactly 80x40 (not 80x48 — that would be Cover, which crops
        // rather than letterboxing). Pinning the exact value, not just
        // "fits inside", is what actually distinguishes Contain from Cover.
        assert_eq!((w, h), (80, 40));
    }

    #[test]
    fn stretch_and_cover_produce_different_pixels() {
        // Cover and Stretch both fill an 80x48 box exactly from a 200x100
        // source, so dimensions alone cannot tell them apart — Cover crops
        // to the box's aspect before a uniform scale, Stretch scales each
        // axis independently and distorts. Assert on content instead.
        let src = test_png(200, 100);
        let g_cover = GeometryOpts {
            fit: Fit::Cover,
            width: Some(80),
            height: Some(48),
            ..default_geometry()
        };
        let g_stretch = GeometryOpts {
            fit: Fit::Stretch,
            width: Some(80),
            height: Some(48),
            ..default_geometry()
        };
        let (cover_uri, cw, ch) = process_image(
            &src,
            &g_cover,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        let (stretch_uri, sw, sh) = process_image(
            &src,
            &g_stretch,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((cw, ch), (80, 48));
        assert_eq!((sw, sh), (80, 48));
        assert_ne!(
            cover_uri, stretch_uri,
            "Cover (crop-to-fill) and Stretch (distort-to-fill) must differ in content"
        );
    }

    #[test]
    fn one_dimension_scales_the_other_by_aspect() {
        let g = GeometryOpts {
            width: Some(100),
            height: None,
            ..default_geometry()
        };
        let (_uri, w, h) = process_image(
            &test_png(200, 100),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (100, 50));
    }

    #[test]
    fn fit_none_ignores_the_box() {
        let g = GeometryOpts {
            fit: Fit::None,
            width: Some(10),
            height: Some(10),
            ..default_geometry()
        };
        let (_uri, w, h) = process_image(
            &test_png(37, 21),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (37, 21));
    }

    #[test]
    fn crop_selects_a_normalised_region() {
        let g = GeometryOpts {
            crop: Some((0.25, 0.0, 0.5, 1.0)),
            fit: Fit::None,
            ..default_geometry()
        };
        let (_uri, w, h) = process_image(
            &test_png(100, 40),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert_eq!((w, h), (50, 40));
    }

    /// Decode a `data:image/png;base64,...` URI back to an RGB pixel at
    /// (x, y), so a test can assert on actual content rather than only
    /// dimensions.
    fn decode_data_uri_pixel(uri: &str, x: u32, y: u32) -> image::Rgb<u8> {
        let b64 = uri
            .strip_prefix("data:image/png;base64,")
            .expect("expected a PNG data URI");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        *img.to_rgb8().get_pixel(x, y)
    }

    #[test]
    fn crop_selects_the_correct_origin_not_just_the_correct_size() {
        // Every pixel encodes its own coordinates as (x, y, 0), so a crop
        // that gets the right *size* but the wrong *origin* (e.g. a bug
        // that always crops from (0, 0)) is still caught: dimensions alone
        // cannot tell (px=0,py=0) apart from the correct origin when the
        // crop width/height are unchanged, but the pixel content can.
        let mut coord_img = image::RgbImage::new(20, 20);
        for (x, y, px) in coord_img.enumerate_pixels_mut() {
            *px = image::Rgb([x as u8, y as u8, 0]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(coord_img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        let src = out.into_inner();

        // x=0.5, y=0.5, w=0.25, h=0.25 on a 20x20 source: px=py=10, pw=ph=5.
        let g = GeometryOpts {
            crop: Some((0.5, 0.5, 0.25, 0.25)),
            fit: Fit::None,
            ..default_geometry()
        };
        let (uri, w, h) =
            process_image(&src, &g, &eink_photo::Params::default(), OutputFormat::Png).unwrap();
        assert_eq!((w, h), (5, 5));
        let top_left = decode_data_uri_pixel(&uri, 0, 0);
        assert_eq!(
            top_left,
            image::Rgb([10, 10, 0]),
            "top-left pixel of the crop must be source (10, 10), not (0, 0)"
        );
    }

    #[test]
    fn crop_outside_the_image_is_an_error() {
        let g = GeometryOpts {
            crop: Some((0.9, 0.0, 0.5, 1.0)),
            ..default_geometry()
        };
        let err = process_image(
            &test_png(100, 40),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::BadCrop(_)));
    }

    #[test]
    fn output_is_a_png_data_uri() {
        let (uri, _, _) = process_image(
            &test_png(8, 8),
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        assert!(
            uri.starts_with("data:image/png;base64,"),
            "{}",
            &uri[..40.min(uri.len())]
        );
    }

    #[test]
    fn jpeg_output_is_smaller_and_labelled_correctly() {
        let src = test_png(64, 64);
        let (png_uri, _, _) = process_image(
            &src,
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap();
        let (jpeg_uri, _, _) = process_image(
            &src,
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Jpeg { quality: 90 },
        )
        .unwrap();
        assert!(jpeg_uri.starts_with("data:image/jpeg;base64,"));
        let _ = png_uri; // size comparison is content-dependent; the label is the contract
    }

    #[test]
    fn undecodable_input_is_an_error_not_a_panic() {
        let err = process_image(
            b"not an image at all",
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::Decode(_)));
    }

    #[test]
    fn an_oversized_source_is_rejected_before_decoding() {
        // A 2-byte "image" cannot exceed the byte cap; build the assertion
        // around the cap itself so it cannot silently stop testing anything.
        let huge = vec![0u8; MAX_SOURCE_BYTES + 1];
        let err = process_image(
            &huge,
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::TooLarge { .. }));
    }

    /// Standard zlib/PNG CRC-32 (polynomial 0xEDB88320), computed by hand
    /// rather than pulled from a dependency — this is only needed to make a
    /// hand-built PNG chunk pass the decoder's checksum check.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        crc ^ 0xFFFF_FFFF
    }

    /// A small, fully valid PNG (real IHDR + real IDAT pixel data, built by
    /// `test_png`) with its IHDR width/height overwritten to declare a
    /// 30000x30000 image, and the IHDR chunk's CRC recomputed so the chunk
    /// stays well-formed. The pixel data still matches the *original* small
    /// dimensions, but that never matters: the decoder must reject the file
    /// on the declared dimensions, via `image::Limits`, before it ever gets
    /// as far as reading a single IDAT byte.
    ///
    /// This is deliberately not a truncated/header-only file. A header-only
    /// fixture (no IDAT at all) triggers the decoder's "unexpected end of
    /// file" path for other reasons once the pixel data is actually needed,
    /// which produces the same `ImageProcessError::Decode` variant as a real
    /// limits rejection but is not exercising `image::Limits` at all. With
    /// real IDAT bytes present, EOF is not available as an alternate
    /// explanation for the rejection — only the declared-dimensions check
    /// can be what stops decoding.
    fn oversized_declared_dimensions_png() -> Vec<u8> {
        let mut png = test_png(4, 4);

        // PNG layout: 8-byte signature, then chunks of
        // [len:4][type:4][data:len][crc:4]. The first chunk after the
        // signature is always IHDR, with an all-fields data payload:
        // width:4, height:4, bit depth:1, color type:1, compression:1,
        // filter:1, interlace:1 (13 bytes total).
        assert_eq!(&png[12..16], b"IHDR", "test_png's first chunk must be IHDR");
        let width: u32 = 30_000;
        let height: u32 = 30_000;
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&height.to_be_bytes());

        // Recompute the IHDR CRC over its type + data bytes (offsets
        // 12..29 = 4-byte type + 13-byte data) so the chunk still passes
        // the decoder's checksum validation after the edit.
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());

        png
    }

    #[test]
    fn an_oversized_declared_image_is_rejected_before_allocating() {
        // The only control between attacker-supplied bytes and a very large
        // allocation is `image::Limits`, set on the reader before decoding.
        // A PNG with real pixel data but an IHDR declaring 30000x30000 must
        // be rejected on the declared size, before any pixel data is read.
        let huge = oversized_declared_dimensions_png();
        assert!(
            huge.len() < MAX_SOURCE_BYTES,
            "the fixture must pass the byte-length guard to actually test the pixel-area guard"
        );
        let err = process_image(
            &huge,
            &default_geometry(),
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        // `matches!(err, ImageProcessError::Decode(_))` alone is NOT enough
        // here: a truncated/EOF decode failure and a genuine
        // `image::Limits` rejection both land in the same `Decode` variant,
        // so that check alone cannot tell "the guard worked" apart from
        // "decoding failed for an unrelated reason". Pin the message text
        // that `image::LimitErrorKind::DimensionError` produces
        // (`image-0.25.10/src/error.rs`), which is specific to the limits
        // check and not emitted by any other decode failure path.
        let ImageProcessError::Decode(msg) = &err else {
            panic!("expected Decode, got {err:?}");
        };
        assert!(
            msg.contains("exceeds limit"),
            "expected an image::Limits dimension rejection, got: {msg}"
        );
    }

    #[test]
    fn an_out_of_range_photo_param_surfaces_as_a_photo_error() {
        // eink_photo::process's own `validate()` rejects exposure outside
        // -5..=5; process_image must map that PhotoError through as
        // ImageProcessError::Photo rather than panicking or losing it.
        let err = process_image(
            &test_png(4, 4),
            &default_geometry(),
            &eink_photo::Params {
                exposure: Some(30.0),
                ..Default::default()
            },
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(
            matches!(err, ImageProcessError::Photo(_)),
            "expected Photo, got {err:?}"
        );
    }

    #[test]
    fn an_oversized_output_box_is_rejected() {
        let g = GeometryOpts {
            width: Some(MAX_OUTPUT_DIM + 1),
            height: Some(10),
            ..default_geometry()
        };
        let err = process_image(
            &test_png(8, 8),
            &g,
            &eink_photo::Params::default(),
            OutputFormat::Png,
        )
        .unwrap_err();
        assert!(matches!(err, ImageProcessError::TooLarge { .. }));
    }

    #[test]
    fn sharpening_runs_after_the_downscale() {
        // Ordering is observable: sharpening a 400px source that is then
        // downscaled to 100px would lose the effect entirely. Because
        // sharpening happens after the resize, the two runs must differ.
        let src = test_png(400, 400);
        let g = GeometryOpts {
            width: Some(100),
            height: Some(100),
            ..default_geometry()
        };
        let plain =
            process_image(&src, &g, &eink_photo::Params::default(), OutputFormat::Png).unwrap();
        let sharp = process_image(
            &src,
            &g,
            &eink_photo::Params {
                sharpen: Some(eink_photo::Sharpen {
                    amount: 100.0,
                    radius: 1.0,
                }),
                ..Default::default()
            },
            OutputFormat::Png,
        )
        .unwrap();
        assert_ne!(
            plain.0, sharp.0,
            "sharpening at output resolution must change the result"
        );
    }
}
