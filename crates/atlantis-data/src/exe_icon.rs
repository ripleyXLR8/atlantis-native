//! The icon of the player's `atlantis.exe` (its RT_ICON resource), read at
//! run time for the window: the port distributes no game data, the icon
//! comes from the player's own copy as the executable's tables do.
//!
//! The shipped builds hold one icon, 32x32 with 16 colours (a DIB:
//! BITMAPINFOHEADER, palette, XOR rows bottom-up, then the 1-bit AND mask).

/// An image, RGBA8, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// The file offset of a relative virtual address, through the section table.
fn rva_to_offset(exe: &[u8], sections: usize, nsec: usize, rva: u32) -> Option<usize> {
    for i in 0..nsec {
        let s = sections + i * 40;
        let vsize = u32_at(exe, s + 8)?;
        let va = u32_at(exe, s + 12)?;
        let raw_size = u32_at(exe, s + 16)?;
        let raw = u32_at(exe, s + 20)?;
        if rva >= va && rva < va + vsize.max(raw_size) {
            return Some((raw + (rva - va)) as usize);
        }
    }
    None
}

/// The data (offset, size) of the first resource of type `kind`.
fn first_resource(exe: &[u8], kind: u32) -> Option<(usize, usize)> {
    let pe = u32_at(exe, 0x3c)? as usize;
    if exe.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let nsec = u16_at(exe, pe + 6)? as usize;
    let opt_size = u16_at(exe, pe + 20)? as usize;
    let opt = pe + 24;
    // PE32: the data directories start at +96; resources are entry 2.
    if u16_at(exe, opt)? != 0x10b {
        return None;
    }
    let res_rva = u32_at(exe, opt + 96 + 2 * 8)?;
    if res_rva == 0 {
        return None;
    }
    let sections = opt + opt_size;
    let root = rva_to_offset(exe, sections, nsec, res_rva)?;
    // A directory: 16-byte header, then 8-byte entries (id, offset); the
    // high bit of the offset marks a subdirectory.
    let entries = |dir: usize| -> Option<Vec<(u32, u32)>> {
        let n = u16_at(exe, dir + 12)? as usize + u16_at(exe, dir + 14)? as usize;
        (0..n)
            .map(|i| {
                Some((
                    u32_at(exe, dir + 16 + 8 * i)?,
                    u32_at(exe, dir + 20 + 8 * i)?,
                ))
            })
            .collect()
    };
    let (_, t) = entries(root)?.into_iter().find(|&(id, _)| id == kind)?;
    let names = root + (t & 0x7fff_ffff) as usize;
    let (_, n) = *entries(names)?.first()?;
    let langs = root + (n & 0x7fff_ffff) as usize;
    let (_, l) = *entries(langs)?.first()?;
    if l & 0x8000_0000 != 0 {
        return None;
    }
    let data = root + l as usize;
    let rva = u32_at(exe, data)?;
    let size = u32_at(exe, data + 4)? as usize;
    Some((rva_to_offset(exe, sections, nsec, rva)?, size))
}

/// The executable's first icon, decoded (1, 4, 8, 24 or 32 bits per pixel;
/// `None` for anything else, e.g. a PNG icon, or no icon).
pub fn exe_icon(exe: &[u8]) -> Option<Icon> {
    const RT_ICON: u32 = 3;
    let (off, size) = first_resource(exe, RT_ICON)?;
    let d = exe.get(off..off + size)?;
    let hsize = u32_at(d, 0)? as usize;
    let w = u32_at(d, 4)? as i32;
    let h2 = u32_at(d, 8)? as i32;
    let bpp = u16_at(d, 14)? as usize;
    if hsize < 40 || w <= 0 || h2 <= 0 || w > 256 {
        return None;
    }
    let (w, h) = (w as usize, (h2 / 2) as usize);
    let colours = match bpp {
        1 | 4 | 8 => match u32_at(d, 32)? {
            0 => 1usize << bpp,
            n => n as usize,
        },
        24 | 32 => 0,
        _ => return None,
    };
    let palette = hsize;
    let xor = palette + 4 * colours;
    let xor_stride = (w * bpp).div_ceil(32) * 4;
    let and = xor + xor_stride * h;
    let and_stride = w.div_ceil(32) * 4;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        // Rows are stored bottom-up.
        let row = h - 1 - y;
        for x in 0..w {
            let (b, g, r, mut a) = match bpp {
                24 | 32 => {
                    let p = xor + row * xor_stride + x * bpp / 8;
                    let px = d.get(p..p + bpp / 8)?;
                    (px[0], px[1], px[2], if bpp == 32 { px[3] } else { 255 })
                }
                _ => {
                    let bit = x * bpp;
                    let byte = *d.get(xor + row * xor_stride + bit / 8)?;
                    let idx = (byte >> (8 - bpp - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    let c = palette + 4 * usize::from(idx);
                    let px = d.get(c..c + 3)?;
                    (px[0], px[1], px[2], 255)
                }
            };
            // The AND mask: 1 = transparent (32-bit icons carry alpha).
            if bpp != 32 {
                let m = *d.get(and + row * and_stride + x / 8)?;
                if m & (0x80 >> (x % 8)) != 0 {
                    a = 0;
                }
            }
            let o = (y * w + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    Some(Icon {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_an_exe() {
        assert_eq!(exe_icon(b"not an executable"), None);
        assert_eq!(exe_icon(&[]), None);
    }
}
