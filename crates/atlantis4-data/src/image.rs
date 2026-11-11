//! The still pictures: JPEG (the warp atlases, object sprites and `2D`
//! stills), PNG (interface art, font atlases) and TGA (two marker
//! pictures).
//!
//! The original decodes them with the libraries it links (libjpeg-turbo
//! 2012, file +0x1a4330; libpng 1.4.4, +0x1fa47c). Here:
//!
//! - **PNG** with the `png` crate, palette and low bit depths expanded to 8
//!   bits per channel. Lossless: the pixels equal libpng's.
//! - **JPEG** with [`crate::jpeg`], this crate's decoder. Every JPEG of
//!   the data is baseline with one scan, colour 4:2:0 (11,589) or 4:4:4
//!   (2, one with restart markers), or grey (345 alpha masks). The
//!   original decodes with libjpeg's defaults: `TeJpeg`'s decoder
//!   (atlantis4-gog-en-2363910 @ 0x004b1a60) does not change the
//!   decompression parameters libjpeg's `jpeg_read_header` sets
//!   (@ 0x00444c00: `dct_method` = 0, `JDCT_ISLOW`, @ 0x00444d93;
//!   `do_fancy_upsampling` = 1 @ 0x00444da5). [`crate::jpeg`] does the same
//!   integer IDCT, triangle upsampling and colour tables: its pixels equal
//!   libjpeg-turbo's on all 11,936 JPEGs. (The `jpeg-decoder` crate,
//!   used before, differed by up to 4 per channel on about 6 % of the
//!   bytes.)
//! - **TGA** here (types 2 and 10, 24 or 32 bits), as libpng has no part in
//!   it; the two files of the data are type 10, 32 bits, 32 × 32.

/// A decoded picture, rows top to bottom, 8 bits per channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// 1 (grey), 2 (grey + alpha), 3 (RGB) or 4 (RGBA).
    pub channels: u8,
    pub pixels: Vec<u8>,
}

/// Why a picture cannot be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageError(pub String);

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImageError {}

/// A sequential JPEG, decoded as the original's libjpeg does (see the
/// module's note).
pub fn decode_jpeg(data: &[u8]) -> Result<Image, ImageError> {
    let d = crate::jpeg::decode(data).map_err(|e| ImageError(e.0))?;
    Ok(Image {
        width: d.width,
        height: d.height,
        channels: d.channels,
        pixels: d.pixels,
    })
}

/// A PNG, expanded to 8 bits per channel.
pub fn decode_png(data: &[u8]) -> Result<Image, ImageError> {
    let err = |e: png::DecodingError| ImageError(format!("PNG: {e}"));
    let mut d = png::Decoder::new(std::io::Cursor::new(data));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = d.read_info().map_err(err)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| ImageError("PNG: too large".into()))?;
    let mut pixels = vec![0; size];
    let info = reader.next_frame(&mut pixels).map_err(err)?;
    pixels.truncate(info.buffer_size());
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(ImageError("PNG: palette not expanded".into())),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        channels,
        pixels,
    })
}

/// A Truevision TGA of type 2 (raw) or 10 (run-length), 24 or 32 bits, as
/// RGBA (24-bit pictures opaque).
pub fn decode_tga(data: &[u8]) -> Result<Image, ImageError> {
    let bad = |w: &str| ImageError(format!("TGA: {w}"));
    let h = data.get(..18).ok_or_else(|| bad("truncated header"))?;
    let (id_len, cmap_type, kind) = (usize::from(h[0]), h[1], h[2]);
    let width = u32::from(u16::from_le_bytes([h[12], h[13]]));
    let height = u32::from(u16::from_le_bytes([h[14], h[15]]));
    let (bpp, descriptor) = (h[16], h[17]);
    if cmap_type != 0 || !(kind == 2 || kind == 10) || !(bpp == 24 || bpp == 32) {
        return Err(bad("only true-colour types 2 and 10, 24 or 32 bits"));
    }
    let px = usize::from(bpp / 8);
    let count = width as usize * height as usize;
    let mut at = 18 + id_len;
    let mut src = Vec::with_capacity(count * px);
    while src.len() < count * px {
        if kind == 2 {
            let need = count * px - src.len();
            src.extend_from_slice(data.get(at..at + need).ok_or_else(|| bad("truncated"))?);
            at += need;
            continue;
        }
        let head = *data.get(at).ok_or_else(|| bad("truncated"))?;
        at += 1;
        let n = usize::from(head & 0x7f) + 1;
        if head & 0x80 != 0 {
            let p = data.get(at..at + px).ok_or_else(|| bad("truncated"))?;
            at += px;
            for _ in 0..n {
                src.extend_from_slice(p);
            }
        } else {
            src.extend_from_slice(data.get(at..at + n * px).ok_or_else(|| bad("truncated"))?);
            at += n * px;
        }
    }
    src.truncate(count * px);
    // Bit 5 of the descriptor: rows stored top first; else bottom first.
    let top_first = descriptor & 0x20 != 0;
    let mut pixels = vec![0u8; count * 4];
    for y in 0..height as usize {
        let sy = if top_first {
            y
        } else {
            height as usize - 1 - y
        };
        for x in 0..width as usize {
            let s = &src[(sy * width as usize + x) * px..][..px];
            let d = &mut pixels[(y * width as usize + x) * 4..][..4];
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = if px == 4 { s[3] } else { 255 };
        }
    }
    Ok(Image {
        width,
        height,
        channels: 4,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_length_tga_decodes_bottom_up() {
        // 2 x 2, 32 bits, type 10, rows bottom first: a run of two blue
        // pixels, then two raw pixels (red, green).
        let mut d = vec![0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 32, 8];
        d.extend([0x81, 255, 0, 0, 255]);
        d.extend([0x01, 0, 0, 255, 255, 0, 255, 0, 128]);
        let img = decode_tga(&d).unwrap();
        assert_eq!((img.width, img.height, img.channels), (2, 2, 4));
        assert_eq!(&img.pixels[..8], &[255, 0, 0, 255, 0, 255, 0, 128]);
        assert_eq!(&img.pixels[8..], &[0, 0, 255, 255, 0, 0, 255, 255]);
        assert!(decode_tga(&d[..20]).is_err());
    }

    #[test]
    fn broken_pictures_are_refused() {
        assert!(decode_png(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(decode_jpeg(b"\xff\xd8\xff").is_err());
    }
}
