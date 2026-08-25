//! Padding a rendered PNG up to a minimum byte size.
//!
//! TRMNL X firmware picks its grayscale rendering matrix from the **byte size**
//! of the downloaded image, not from anything in its content
//! (`trmnl-firmware/src/display.cpp`, v1.8.14):
//!
//! ```c
//! if (data_size > FASTEPD_LARGE_IMAGE_THRESHOLD) {   // 100 * 1024
//!     bbep.setCustomMatrix(u8_graytable_big, ...);   // 38-pass table
//! } else {
//!     bbep.setCustomMatrix(u8_graytable, ...);       // 9-pass table
//! }
//! ```
//!
//! The 38-pass table drives each grey level with four times as many pulses, so
//! it is the panel's best shot at landing mid-greys on their nominal level.
//! Byonk's PNGs are 3–60 KB, so an X has never once reached it.
//!
//! Padding buys that table without touching a single pixel: the filler goes
//! into an ancillary `tEXt` chunk, which every decoder skips. `tEXt` is stored
//! uncompressed, so its bytes count towards the file size in full — a `zTXt`
//! chunk would collapse and defeat the whole point.
//!
//! Sizing is safe by a wide margin: the X accepts up to `MAX_IMAGE_SIZE`
//! 750 000 bytes (`include/config.h`), against a 102 400-byte threshold.

/// Hard ceiling on the padded size, from the device's own `MAX_IMAGE_SIZE`
/// (`include/config.h`). An image past this is refused outright, so padding
/// beyond it turns a working screen into a blank one.
///
/// `min_png_bytes` reaches here as an unvalidated `u32` from `config.yaml`, so
/// this is also what stops a mistyped value asking for a multi-gigabyte
/// allocation on every single render.
pub const MAX_PNG_BYTES: usize = 750_000;

/// The 8-byte PNG file signature.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Keyword of the filler `tEXt` chunk. Must be 1–79 Latin-1 bytes.
const KEYWORD: &[u8] = b"Comment";

/// Filler byte. Any printable Latin-1 that is not NUL is legal in `tEXt`.
const FILLER: u8 = b'-';

/// Bytes a chunk costs beyond its filler: 4 length + 4 type + 4 CRC, plus the
/// keyword and its NUL separator.
const CHUNK_OVERHEAD: usize = 12 + KEYWORD.len() + 1;

/// Grow `png` to at least `min_bytes` by appending a filler `tEXt` chunk.
///
/// Returns the input untouched when it is already large enough, or when it does
/// not parse as a PNG — padding is an optimisation, never a reason to fail a
/// render that otherwise succeeded.
///
/// The decoded image is bit-for-bit identical either way.
pub fn pad_png_to_min_size(png: Vec<u8>, min_bytes: usize) -> Vec<u8> {
    let min_bytes = if min_bytes > MAX_PNG_BYTES {
        tracing::warn!(
            requested = min_bytes,
            capped_to = MAX_PNG_BYTES,
            "min_png_bytes is above the device's maximum image size; capping. \
             Lower it in config.yaml — a device refuses an image this large."
        );
        MAX_PNG_BYTES
    } else {
        min_bytes
    };
    if png.len() >= min_bytes {
        return png;
    }
    let Some(iend) = find_iend(&png) else {
        tracing::warn!(
            len = png.len(),
            "cannot pad image: no IEND chunk found, serving unpadded"
        );
        return png;
    };

    // Overshooting is fine and undershooting is not: the firmware compares with
    // `>`, so landing exactly on the threshold would still select the 9-pass
    // table. `max(1)` keeps the chunk well-formed when the shortfall is tiny.
    let filler_len = min_bytes
        .saturating_sub(png.len())
        .saturating_sub(CHUNK_OVERHEAD)
        .max(1);

    let mut data = Vec::with_capacity(KEYWORD.len() + 1 + filler_len);
    data.extend_from_slice(KEYWORD);
    data.push(0); // NUL separates keyword from text
    data.resize(data.len() + filler_len, FILLER);

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(b"tEXt");
    hasher.update(&data);
    let crc = hasher.finalize();

    let mut out = Vec::with_capacity(png.len() + 12 + data.len());
    out.extend_from_slice(&png[..iend]);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(b"tEXt");
    out.extend_from_slice(&data);
    out.extend_from_slice(&crc.to_be_bytes());
    out.extend_from_slice(&png[iend..]);
    out
}

/// Byte offset of the `IEND` chunk header, walking the chunk chain rather than
/// assuming it sits in the last 12 bytes (trailing garbage is legal in the wild).
fn find_iend(png: &[u8]) -> Option<usize> {
    if png.len() < 8 || png[..8] != SIGNATURE {
        return None;
    }
    let mut pos = 8;
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().ok()?) as usize;
        if &png[pos + 4..pos + 8] == b"IEND" {
            return Some(pos);
        }
        pos = pos.checked_add(12)?.checked_add(len)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but real 4x4 greyscale PNG.
    fn sample_png() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, 4, 4);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().expect("header");
            writer
                .write_image_data(&[
                    0, 40, 80, 120, 160, 200, 240, 255, 10, 20, 30, 40, 50, 60, 70, 80,
                ])
                .expect("data");
        }
        buf
    }

    fn decode(png_bytes: &[u8]) -> (png::OutputInfo, Vec<u8>) {
        let decoder = png::Decoder::new(png_bytes);
        let mut reader = decoder.read_info().expect("decodes");
        let mut out = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut out).expect("frame");
        out.truncate(info.buffer_size());
        (info, out)
    }

    #[test]
    fn padding_reaches_the_requested_size() {
        let padded = pad_png_to_min_size(sample_png(), 102_401);
        assert!(
            padded.len() > 102_400,
            "must clear the firmware threshold, got {}",
            padded.len()
        );
    }

    #[test]
    fn padding_is_capped_at_the_firmware_maximum() {
        // `min_png_bytes` is an unvalidated `u32` straight out of `config.yaml`.
        // A mistyped value asks for an allocation of that size on every render,
        // and anything past MAX_PNG_BYTES is worse than useless: the device
        // refuses the image outright.
        let padded = pad_png_to_min_size(sample_png(), 2_000_000);
        assert!(
            padded.len() <= MAX_PNG_BYTES,
            "padding must stop at the device's limit, got {}",
            padded.len()
        );
    }

    #[test]
    fn padding_does_not_change_a_single_pixel() {
        // The whole premise of the trick: same image, different byte count.
        let original = sample_png();
        let padded = pad_png_to_min_size(original.clone(), 102_401);
        let (info_a, pixels_a) = decode(&original);
        let (info_b, pixels_b) = decode(&padded);
        assert_eq!(info_a.width, info_b.width);
        assert_eq!(info_a.height, info_b.height);
        assert_eq!(info_a.color_type, info_b.color_type);
        assert_eq!(pixels_a, pixels_b, "padding must be pixel-transparent");
    }

    #[test]
    fn already_large_enough_is_returned_untouched() {
        let original = sample_png();
        let out = pad_png_to_min_size(original.clone(), original.len());
        assert_eq!(out, original, "no chunk when the file already qualifies");
    }

    #[test]
    fn non_png_input_is_returned_untouched() {
        // A render that succeeded must still be served if padding cannot apply.
        let junk = b"not a png at all".to_vec();
        assert_eq!(pad_png_to_min_size(junk.clone(), 4096), junk);
    }

    #[test]
    fn a_shortfall_smaller_than_the_chunk_overhead_still_overshoots() {
        let original = sample_png();
        let target = original.len() + 3;
        let padded = pad_png_to_min_size(original, target);
        assert!(padded.len() >= target);
        decode(&padded); // still a valid PNG
    }
}
