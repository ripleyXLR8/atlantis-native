// Ported from ScummVM's `graphics/yuv_to_rgb.cpp` (`YUVToRGBLookup`,
// `convertYUV420ToRGB`, `convertYUVA420ToRGBA`) (ScummVM - Graphic
// Adventure Engine, Copyright (C) the ScummVM team, GNU General Public
// License, version 3 or later). ScummVM's code derives from SDL's YUV
// overlay code and mpeg_play: Copyright (c) 1995 The Regents of the
// University of California, Copyright (c) 1995 Erik Corry, portions
// Copyright (c) 1995 Brown University, all under the permission notice
// that ScummVM's file reproduces.

//! ScummVM's 4:2:0 Y'CbCr → RGB conversion as its Bink decoder calls it
//! (`kScaleITU`: studio-range luma, 16..235 stretched to 0..255).
//!
//! ScummVM's lookup tables, built once per output format: four chroma
//! tables of `int16` (the BT.601 factors 0.419/0.299, 0.299/0.419,
//! 0.114/0.331 and 0.587/0.331 applied to `Cr` or `Cb` − 128, truncated
//! toward zero) and a clip table indexed by luma plus chroma term; each
//! 2 × 2 block of luma shares the chroma sample at its top-left.
//!
//! [UNKNOWN] how RAD's `binkw32.dll` (which the 2004 program links)
//! converts its planes to RGB; this is ScummVM's conversion, not the
//! original's.

use std::sync::OnceLock;

/// An output format of the conversion: the bits each channel drops and
/// where it goes (`Graphics::PixelFormat`).
#[derive(Clone, Copy)]
struct Format {
    r_loss: u32,
    g_loss: u32,
    b_loss: u32,
    r_shift: u32,
    g_shift: u32,
    b_shift: u32,
}

/// x8r8g8b8 (and a8r8g8b8: the same tables, alpha at bit 24).
const XRGB8888: Format = Format {
    r_loss: 0,
    g_loss: 0,
    b_loss: 0,
    r_shift: 16,
    g_shift: 8,
    b_shift: 0,
};

/// RGB565.
const RGB565: Format = Format {
    r_loss: 3,
    g_loss: 2,
    b_loss: 3,
    r_shift: 11,
    g_shift: 5,
    b_shift: 0,
};

/// `YUVToRGBLookup` for one format at `kScaleITU`.
pub(super) struct Lookup {
    format: Format,
    /// `Cr_r`, `Cr_g`, `Cb_g`, `Cb_b`, 256 entries each; the offsets of
    /// the channels' clip tables (+ 256) are folded in, as in ScummVM.
    color_tab: [i16; 4 * 256],
    clip_table: [u8; 3 * 768],
}

impl Lookup {
    fn new(format: Format) -> Lookup {
        let mut clip_table = [0u8; 3 * 768];
        let r_offset = 0usize;
        let g_offset = if format.g_loss == format.r_loss {
            r_offset
        } else {
            r_offset + 768
        };
        let b_offset = if format.b_loss == format.g_loss {
            g_offset
        } else if format.b_loss == format.r_loss {
            r_offset
        } else {
            g_offset + 768
        };

        // kScaleITU: entries 16..235 of the rgb-to-pixel tables, then the
        // rest spread out so that no index needs a check.
        for (offset, loss) in [
            (r_offset, format.r_loss),
            (g_offset, format.g_loss),
            (b_offset, format.b_loss),
        ] {
            let t = &mut clip_table[offset..offset + 768];
            for i in 16..236 {
                let scaled = (i - 16) * 255 / 219;
                t[i + 256] = (scaled >> loss) as u8;
            }
            let low = t[256 + 16];
            t[..256 + 16].fill(low);
            let high = t[256 + 236 - 1];
            t[256 + 236..].fill(high);
        }

        let mut color_tab = [0i16; 4 * 256];
        for i in 0..256 {
            let cr = (i as i16 - 128) as f64;
            let cb = cr;
            color_tab[i] = ((0.419 / 0.299) * cr) as i16 + (r_offset + 256) as i16;
            color_tab[256 + i] = (-(0.299 / 0.419) * cr) as i16 + (g_offset + 256) as i16;
            color_tab[512 + i] = (-(0.114 / 0.331) * cb) as i16;
            color_tab[768 + i] = ((0.587 / 0.331) * cb) as i16 + (b_offset + 256) as i16;
        }

        Lookup {
            format,
            color_tab,
            clip_table,
        }
    }

    /// The lookup of x8r8g8b8 and a8r8g8b8.
    pub(super) fn xrgb8888() -> &'static Lookup {
        static L: OnceLock<Lookup> = OnceLock::new();
        L.get_or_init(|| Lookup::new(XRGB8888))
    }

    /// The lookup of RGB565.
    pub(super) fn rgb565() -> &'static Lookup {
        static L: OnceLock<Lookup> = OnceLock::new();
        L.get_or_init(|| Lookup::new(RGB565))
    }

    /// One pixel (`PUT_PIXEL`) without the alpha bits.
    #[inline]
    fn pixel(&self, y: u8, cr_r: i16, crb_g: i16, cb_b: i16) -> u32 {
        let l = &self.clip_table[usize::from(y)..];
        let f = &self.format;
        (u32::from(l[cr_r as usize]) << f.r_shift)
            | (u32::from(l[crb_g as usize]) << f.g_shift)
            | (u32::from(l[cb_b as usize]) << f.b_shift)
    }
}

/// The planes and geometry a conversion reads.
pub(super) struct Planes<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub a: Option<&'a [u8]>,
    pub y_pitch: usize,
    pub uv_pitch: usize,
    /// The surface's even size (`_surfaceWidth`, `_surfaceHeight`).
    pub even_width: usize,
    pub even_height: usize,
    /// The video's size: the pixels written.
    pub width: usize,
    pub height: usize,
}

/// `convertYUV420ToRGB` (alpha `None`) or `convertYUVA420ToRGBA`: calls
/// `put(x, y, rgb, alpha)` for each pixel of the video's size, `rgb` in
/// `lookup`'s format without alpha bits.
pub(super) fn convert420(lookup: &Lookup, p: &Planes<'_>, mut put: impl FnMut(usize, u32, u8)) {
    let cr_r_tab = &lookup.color_tab[0..256];
    let cr_g_tab = &lookup.color_tab[256..512];
    let cb_g_tab = &lookup.color_tab[512..768];
    let cb_b_tab = &lookup.color_tab[768..1024];
    for h in 0..p.even_height / 2 {
        for w in 0..p.even_width / 2 {
            let u = usize::from(p.u[h * p.uv_pitch + w]);
            let v = usize::from(p.v[h * p.uv_pitch + w]);
            let cr_r = cr_r_tab[v];
            let crb_g = cr_g_tab[v].wrapping_add(cb_g_tab[u]);
            let cb_b = cb_b_tab[u];
            for dy in 0..2 {
                let row = 2 * h + dy;
                if row >= p.height {
                    continue;
                }
                for dx in 0..2 {
                    let col = 2 * w + dx;
                    if col >= p.width {
                        continue;
                    }
                    let src = row * p.y_pitch + col;
                    let rgb = lookup.pixel(p.y[src], cr_r, crb_g, cb_b);
                    let alpha = p.a.map_or(0xFF, |a| a[src]);
                    put(row * p.width + col, rgb, alpha);
                }
            }
        }
    }
}
