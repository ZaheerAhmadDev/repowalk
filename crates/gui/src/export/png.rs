//! Whole-window PNG export. OWNER: RSK-23 (spike).
//!
//! Spike result (eframe/egui 0.36): `ViewportCommand::Screenshot(user_data)` is
//! delivered as `Event::Screenshot { image: Arc<ColorImage>, user_data, .. }` on a
//! later frame's input. `request_screenshot` sends the tagged command; `App`
//! matches the tag and calls [`save_png`].
//!
//! The pinned stack has no image-encoding crate (zero new deps rule), so
//! `save_png` emits a valid PNG with *stored* (uncompressed) deflate blocks —
//! hand-rolled CRC32 + Adler32, fully deterministic.

use eframe::egui;

/// Ask the window manager for a screenshot; the reply arrives as
/// `Event::Screenshot` carrying the same `tag` in `user_data`.
pub fn request_screenshot(ctx: &egui::Context, tag: u64) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(tag)));
}

/// Encode an RGBA8 `ColorImage` as PNG (8-bit, no interlace, stored blocks).
pub fn save_png(image: &egui::ColorImage, path: &std::path::Path) -> anyhow::Result<()> {
    let bytes = encode_png(image);
    std::fs::write(path, bytes)?;
    Ok(())
}

/// PNG byte stream for `image` (RGBA8, straight alpha, filter None).
pub fn encode_png(image: &egui::ColorImage) -> Vec<u8> {
    let w = image.width() as u32;
    let h = image.height() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR: width, height, bit depth 8, color type 6 (RGBA), deflate, filter 0, interlace 0.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(&mut out, b"IHDR", &ihdr);

    // IDAT: one zlib stream of stored deflate blocks over filtered scanlines.
    let mut raw = Vec::with_capacity(h as usize * (w as usize * 4 + 1));
    for y in 0..h as usize {
        raw.push(0u8); // filter type: None
        for x in 0..w as usize {
            let px = image.pixels[y * w as usize + x];
            raw.extend_from_slice(&px.to_array());
        }
    }
    push_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    push_chunk(&mut out, b"IEND", &[]);
    out
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// zlib stream wrapping `raw` in stored (BTYPE=00) deflate blocks, max 65535 bytes each.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut z = Vec::new();
    z.push(0x78); // CMF: deflate, 32K window
    z.push(0x01); // FLG: check bits for (0x7801), no dict, fastest
    let chunks: Box<dyn Iterator<Item = &[u8]>> = if raw.is_empty() {
        Box::new(std::iter::once(&[][..]))
    } else {
        Box::new(raw.chunks(65535))
    };
    let mut it = chunks.peekable();
    while let Some(chunk) = it.next() {
        let last = it.peek().is_none();
        z.push(if last { 1 } else { 0 }); // BFINAL + BTYPE=00
        let len = chunk.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    z.extend_from_slice(&adler32(raw).to_be_bytes());
    z
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_signature_and_chunk_layout() {
        let img = egui::ColorImage::from_rgba_unmultiplied([2, 1], &[1, 2, 3, 255, 4, 5, 6, 128]);
        let png = encode_png(&img);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR chunk: len 13, then "IHDR"
        assert_eq!(&png[8..12], &13u32.to_be_bytes());
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(
            &png[16..24],
            &[2u32.to_be_bytes(), 1u32.to_be_bytes()].concat()
        );
        assert_eq!(png[24], 8, "bit depth");
        assert_eq!(png[25], 6, "RGBA color type");
        // IEND chunk (len 0 + "IEND" + CRC) closes the stream
        assert_eq!(&png[png.len() - 12..png.len() - 8], &0u32.to_be_bytes());
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler32_known_vector() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn zlib_stream_roundtrip_structure() {
        let z = zlib_stored(&[1, 2, 3]);
        assert_eq!(z[0], 0x78);
        // stored block header: BFINAL=1 (last), LEN=3, NLEN=!3
        assert_eq!(z[2], 1);
        assert_eq!(&z[3..5], &3u16.to_le_bytes());
        assert_eq!(&z[5..7], &(!3u16).to_le_bytes());
        assert_eq!(&z[7..10], &[1, 2, 3]);
    }
}
