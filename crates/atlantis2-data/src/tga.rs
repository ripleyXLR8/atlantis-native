//! The dialog heads' texture maps (`datas_a2\3d\dial\*.tga`) and the
//! menus' backgrounds (`spr\menu\f_*.tga`): TGA true colour, 16 bits per
//! pixel (1-5-5-5) or 24 (B, G, R), run-length encoded (type 10) or not
//! (type 2). Both are loaded by the same code (the TGA reader @ 0x004c6860
//! and the surface copy @ 0x004b6cc0, called by `dlg_Perso::LoadTextures`
//! @ 0x00463a90 and `CMnMenu::LoadTga` @ 0x00472830).
//!
//! The copy into a surface of the screen's pixel format (@ 0x004b5770)
//! moves each channel's mask to the destination's, top bits aligned. On a
//! 16-bit RGB565 screen, the format the port draws in, a texel (r, g, b)
//! of 5 bits each becomes `(r << 11) | (g << 6) | b`: green's low bit is
//! 0 (checked on a head's map under emulation); a 24-bit pixel keeps the
//! top 5, 6 and 5 bits of its channels [inferred from the same rule]. The
//! rows are stored
//! top-down (a bottom-up file, descriptor bit 5 clear, is flipped).

use std::fmt;

/// A decoded map: `width` x `height` RGB565 texels, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tga {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
}

/// Why a map could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TgaError {
    /// Shorter than its header or its pixels.
    Truncated,
    /// A colour-mapped (palette) file: refused by the original (`la map %s
    /// est en format paletté`), or another kind not read here.
    Unsupported { image_type: u8, bits: u8 },
}

impl fmt::Display for TgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TgaError::Truncated => write!(f, "truncated TGA file"),
            TgaError::Unsupported { image_type, bits } => {
                write!(f, "unsupported TGA file (type {image_type}, {bits} bits)")
            }
        }
    }
}

impl std::error::Error for TgaError {}

/// A decoded file at 8 bits a channel, before the copy into a surface.
struct Decoded {
    width: usize,
    height: usize,
    bits: u8,
    /// B, G, R, top row first: a 1-5-5-5 pixel's channels moved to the top
    /// of their byte (`c << 3`, the copy's rule), a 24-bit pixel as stored.
    bgr: Vec<[u8; 3]>,
}

impl Decoded {
    fn parse(data: &[u8]) -> Result<Decoded, TgaError> {
        let h = data.get(..18).ok_or(TgaError::Truncated)?;
        let id_len = h[0] as usize;
        let cmap_type = h[1];
        let image_type = h[2];
        let cmap_len = u16::from_le_bytes([h[5], h[6]]) as usize;
        let cmap_bits = h[7] as usize;
        let width = u16::from_le_bytes([h[12], h[13]]) as usize;
        let height = u16::from_le_bytes([h[14], h[15]]) as usize;
        let bits = h[16];
        let top_down = h[17] & 0x20 != 0;
        if !(bits == 16 || bits == 24) || !(image_type == 2 || image_type == 10) {
            return Err(TgaError::Unsupported { image_type, bits });
        }
        let mut at = 18 + id_len;
        if cmap_type != 0 {
            at += cmap_len * cmap_bits.div_ceil(8);
        }
        let n = width * height;
        let mut file = Vec::with_capacity(n);
        let size = usize::from(bits / 8);
        let px = |at: usize| -> Result<[u8; 3], TgaError> {
            let b = data.get(at..at + size).ok_or(TgaError::Truncated)?;
            Ok(if size == 2 {
                let p = u16::from_le_bytes([b[0], b[1]]);
                let c = |s: u16| (((p >> s) & 0x1f) << 3) as u8;
                [c(0), c(5), c(10)]
            } else {
                [b[0], b[1], b[2]]
            })
        };
        if image_type == 2 {
            for i in 0..n {
                file.push(px(at + size * i)?);
            }
        } else {
            while file.len() < n {
                let head = *data.get(at).ok_or(TgaError::Truncated)?;
                at += 1;
                let count = (head & 0x7f) as usize + 1;
                if head & 0x80 != 0 {
                    let p = px(at)?;
                    at += size;
                    file.extend(std::iter::repeat_n(p, count));
                } else {
                    for _ in 0..count {
                        file.push(px(at)?);
                        at += size;
                    }
                }
            }
            file.truncate(n);
        }
        let mut bgr = vec![[0u8; 3]; n];
        for y in 0..height {
            let src = if top_down { y } else { height - 1 - y };
            bgr[y * width..(y + 1) * width].copy_from_slice(&file[src * width..(src + 1) * width]);
        }
        Ok(Decoded {
            width,
            height,
            bits,
            bgr,
        })
    }
}

/// A map copied into a 24-bit surface (the credits' page surfaces,
/// pixel format record 0x94 of FUN_004a3ab0: bytes B, G, R): a 1-5-5-5
/// file's channel `c` becomes `c << 3` (checked under emulation on the 16
/// credits pictures, `credits.txt`'s `load` rows); a 24-bit file's bytes
/// are copied as they are (checked under emulation on synthetic files of
/// both types and row orders, `credits_tga24.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bgr24 {
    pub width: usize,
    pub height: usize,
    /// B, G, R a pixel, top row first.
    pub bytes: Vec<u8>,
}

impl Bgr24 {
    pub fn parse(data: &[u8]) -> Result<Bgr24, TgaError> {
        let d = Decoded::parse(data)?;
        Ok(Bgr24 {
            width: d.width,
            height: d.height,
            bytes: d.bgr.concat(),
        })
    }
}

impl Tga {
    pub fn parse(data: &[u8]) -> Result<Tga, TgaError> {
        let d = Decoded::parse(data)?;
        let pixels = d
            .bgr
            .iter()
            .map(|&[b, g, r]| {
                let (b, g, r) = (u16::from(b), u16::from(g), u16::from(r));
                if d.bits == 16 {
                    // A 1-5-5-5 texel (r, g, b) as `(r << 11) | (g << 6) | b`.
                    ((r >> 3) << 11) | ((g >> 3) << 6) | (b >> 3)
                } else {
                    ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
                }
            })
            .collect();
        Ok(Tga {
            width: d.width,
            height: d.height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_bottom_up() {
        // 2 x 2, type 10, 16 bits, bottom-up: a run of two red pixels
        // (the bottom row), then two raw pixels, green and blue.
        let mut f = vec![0u8; 18];
        f[2] = 10;
        f[12] = 2;
        f[14] = 2;
        f[16] = 16;
        f.extend([0x81, 0x00, 0x7c]);
        f.extend([0x01, 0xe0, 0x03, 0x1f, 0x00]);
        let t = Tga::parse(&f).unwrap();
        assert_eq!((t.width, t.height), (2, 2));
        assert_eq!(t.pixels, vec![0x1f << 6, 0x1f, 0xf800, 0xf800]);
        f[2] = 1;
        assert!(Tga::parse(&f).is_err());
        // 1 x 1, type 2, 24 bits: B, G, R keep their top bits.
        let mut g = vec![0u8; 18];
        g[2] = 2;
        g[12] = 1;
        g[14] = 1;
        g[16] = 24;
        g.extend([0xff, 0x84, 0x0f]);
        assert_eq!(
            Tga::parse(&g).unwrap().pixels,
            vec![(1 << 11) | (0x21 << 5) | 0x1f]
        );
        assert_eq!(Tga::parse(&f[..10]), Err(TgaError::Truncated));
        // The same files in a 24-bit surface.
        assert_eq!(Bgr24::parse(&g).unwrap().bytes, vec![0xff, 0x84, 0x0f]);
        f[2] = 10;
        assert_eq!(
            Bgr24::parse(&f).unwrap().bytes,
            [[0, 0xf8, 0], [0xf8, 0, 0], [0, 0, 0xf8], [0, 0, 0xf8]].concat()
        );
    }
}
