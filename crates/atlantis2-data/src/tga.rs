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

/// A 1-5-5-5 TGA pixel as the game's RGB565 texel.
fn texel(p: u16) -> u16 {
    let r = (p >> 10) & 0x1f;
    let g = (p >> 5) & 0x1f;
    let b = p & 0x1f;
    (r << 11) | (g << 6) | b
}

impl Tga {
    pub fn parse(data: &[u8]) -> Result<Tga, TgaError> {
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
        let px = |at: usize| -> Result<u16, TgaError> {
            let b = data.get(at..at + size).ok_or(TgaError::Truncated)?;
            Ok(if size == 2 {
                texel(u16::from_le_bytes([b[0], b[1]]))
            } else {
                let (bl, g, r) = (u16::from(b[0]), u16::from(b[1]), u16::from(b[2]));
                ((r >> 3) << 11) | ((g >> 2) << 5) | (bl >> 3)
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
        let mut pixels = vec![0u16; n];
        for y in 0..height {
            let src = if top_down { y } else { height - 1 - y };
            for x in 0..width {
                pixels[y * width + x] = file[src * width + x];
            }
        }
        Ok(Tga {
            width,
            height,
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
    }
}
