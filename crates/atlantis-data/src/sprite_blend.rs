//! The blend rules of the sprite blitters, on RGB565 pixels.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/sprite_blend.h, GPLv3, the ScummVM team).
//!
//! The original anti-aliases sprite edges by blending each edge pixel with
//! the framebuffer: an edge pixel carries a 5-bit coverage `factor` (0..31)
//! and a colour already premultiplied by factor/32 in the sprite data; the
//! destination adds the remaining (32 - factor)/32 of itself, per channel.
//! The original precomputes per-channel tables (FUN_0042b930 for SPR,
//! FUN_00452ac8 for SPW); each entry is (channel * (32 - factor)) / 32, so a
//! `>> 5` gives every table lookup bit for bit.

/// Marks an opaque pixel in [`crate::sprite::SprSprite::blend`].
pub const SPR_NO_BLEND: u8 = 0xff;
/// Marks a transparent pixel in [`crate::sprite::SprSprite::blend`]. SPR
/// factors are 0..31, so every RGB565 colour stays usable (the original
/// draws by run type, not by colour).
pub const SPR_TRANSPARENT: u8 = 0xfe;

/// An SPR edge pixel over `dst` (tables of FUN_0042b930, blitter
/// FUN_0041047f). The 6-bit green channel's *2/64 reduces to *1/32.
pub fn blend_spr_pixel565(src: u16, dst: u16, factor: u8) -> u16 {
    let inv = 32 - u32::from(factor & 0x1f);
    let d = u32::from(dst);
    let b = ((d & 0x1f) * inv) >> 5;
    let g = (((d >> 5) & 0x3f) * inv) >> 5;
    let r = (((d >> 11) & 0x1f) * inv) >> 5;
    src.wrapping_add(((r << 11) | (g << 5) | b) as u16)
}

/// An SPW / SPF edge pixel over `dst` (FUN_00452bde / FUN_00452d39, tables
/// of FUN_00452ac8): as [`blend_spr_pixel565`], but the green table is one
/// bit coarser (>> 6, placed with << 6), so green is effectively 5-bit.
pub fn blend_spw_pixel565(src: u16, dst: u16, factor: u8) -> u16 {
    let inv = 32 - u32::from(factor & 0x1f);
    let d = u32::from(dst);
    let b = ((d & 0x1f) * inv) >> 5;
    let g = (((d >> 5) & 0x3f) * inv) >> 6;
    let r = (((d >> 11) & 0x1f) * inv) >> 5;
    src.wrapping_add(((r << 11) | (g << 6) | b) as u16)
}

/// The UBB dialog compositor's blend: FUN_004508e0(shade, fg, dst), the
/// indirect call through the pointer at 0x00497c8c from FUN_00450b08, with
/// the table FUN_00450d90 builds (T[a][c] = (31 - a) * c). `a` is the
/// largest of `shade` and fg's 5-bit blue, green and red; each channel is
/// (fg * 32 + T[a][dst]) >> 5, green from its top 5 bits and >> 4 into the
/// 6-bit field. The three parts are added, so a channel can carry into the
/// next one, as in the original.
pub fn blend_ubb_pixel565(fg: u16, dst: u16, shade: u32) -> u16 {
    let fg = u32::from(fg);
    let dst = u32::from(dst);
    let (fb, fgn, fr) = (fg & 0x1f, ((fg >> 5) & 0x3f) >> 1, (fg >> 11) & 0x1f);
    let (db, dgn, dr) = (dst & 0x1f, ((dst >> 5) & 0x3f) >> 1, (dst >> 11) & 0x1f);
    let a = shade.max(fb).max(fgn).max(fr);
    let k = 31u32.wrapping_sub(a);
    let g = ((u32::from(k.wrapping_mul(dgn) as u16) + (fgn << 5)) >> 4) << 5;
    let b = ((fb << 5) + u32::from(k.wrapping_mul(db) as u16)) >> 5;
    let r = ((u32::from(k.wrapping_mul(dr) as u16) + (fr << 5)) >> 5) << 11;
    (r + g + b) as u16
}

/// FUN_00450b08: a dialog frame's opcode stream over `dst` (`dst.len()`
/// pixels, `src` the frame's palette indices, `pal` their RGB565 colours).
/// 0: skip a 24-bit run (0 ends the stream); 1: a run of palette colours
/// with one shade byte (>> 3), copied as is when it is 0x1f (FUN_00450ad3),
/// blended otherwise; 2: a run with one shade byte per pixel, always
/// blended (the original calls the blend even at 0x1f). An unknown opcode
/// ends the stream (0x00450c82).
pub fn compose_ubb_ia(
    dst: &mut [u16],
    src: &[u8],
    pal: &[u16; 256],
    opcodes: &[u8],
    blend: impl Fn(u16, u16, u32) -> u16,
) {
    let frame_n = dst.len();
    let mut op = 0usize;
    let mut pos = 0usize;
    let src_at = |i: usize| src.get(i).copied().unwrap_or(0);
    while op + 2 <= opcodes.len() && pos < frame_n {
        let kind = opcodes[op];
        let run_hi = usize::from(opcodes[op + 1]);
        match kind {
            0 => {
                if op + 4 > opcodes.len() {
                    break;
                }
                let run = usize::from(opcodes[op + 1])
                    | usize::from(opcodes[op + 2]) << 8
                    | usize::from(opcodes[op + 3]) << 16;
                if run == 0 {
                    break;
                }
                pos += run;
                op += 4;
            }
            1 => {
                if op + 3 > opcodes.len() {
                    break;
                }
                let shade = u32::from(opcodes[op + 2] >> 3);
                for i in 0..run_hi {
                    if pos + i >= frame_n {
                        break;
                    }
                    let c = pal[usize::from(src_at(pos + i))];
                    dst[pos + i] = if shade == 0x1f {
                        c
                    } else {
                        blend(c, dst[pos + i], shade)
                    };
                }
                pos += run_hi;
                op += 3;
            }
            2 => {
                if op + 2 + run_hi > opcodes.len() {
                    break;
                }
                for i in 0..run_hi {
                    if pos + i >= frame_n {
                        break;
                    }
                    let c = pal[usize::from(src_at(pos + i))];
                    let shade = u32::from(opcodes[op + 2 + i] >> 3);
                    dst[pos + i] = blend(c, dst[pos + i], shade);
                }
                pos += run_hi;
                op += 2 + run_hi;
            }
            _ => break,
        }
    }
}

/// The pixels [`compose_ubb_ia`] draws with `opcodes` over a frame of `n`
/// pixels: 1 where a run draws (opaque or blended), 0 where it skips.
pub fn ubb_ia_coverage(opcodes: &[u8], n: usize) -> Vec<u8> {
    let mut cover = vec![0u8; n];
    let (mut op, mut pos) = (0usize, 0usize);
    while op + 2 <= opcodes.len() && pos < n {
        let run_hi = usize::from(opcodes[op + 1]);
        let (run, next, draws) = match opcodes[op] {
            0 if op + 4 <= opcodes.len() => {
                let run =
                    run_hi | usize::from(opcodes[op + 2]) << 8 | usize::from(opcodes[op + 3]) << 16;
                if run == 0 {
                    break;
                }
                (run, op + 4, false)
            }
            1 if op + 3 <= opcodes.len() => (run_hi, op + 3, true),
            2 if op + 2 + run_hi <= opcodes.len() => (run_hi, op + 2 + run_hi, true),
            _ => break,
        };
        if draws {
            for c in cover.iter_mut().skip(pos).take(run) {
                *c = 1;
            }
        }
        pos += run;
        op = next;
    }
    cover
}

/// The menu backdrop dim, FUN_004108e0 (called by FUN_0041cc28 before the
/// in-game menu and the disc prompt): p - ((p & mask) >> 1), mask 0xf79e in
/// 5-6-5 (key 0x235), 0xfbde in 5-5-5 (key 0x613). Red and blue become
/// c - c / 2; the 5-6-5 mask also clears green's bit 1, so green becomes
/// g - ((g & 0x3c) >> 1).
pub fn dim_menu_pixel(p: u16, rgb565: bool) -> u16 {
    let mask = if rgb565 { 0xf79e } else { 0xfbde };
    p - ((p & mask) >> 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md5_le(v: &[u16]) -> String {
        use md5::{Digest, Md5};
        let bytes: Vec<u8> = v.iter().flat_map(|p| p.to_le_bytes()).collect();
        Md5::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    // FUN_0042b930 / FUN_00452ac8 tables applied by the original blitters
    // over every (factor 0..31, destination 0..65535) pair, for two source
    // colours (emulated digests).
    #[test]
    fn ubb_coverage_follows_the_runs() {
        // skip 3, draw 2 opaque, skip 1, draw 2 blended
        let ops = [0, 3, 0, 0, 1, 2, 0xff, 0, 1, 0, 0, 2, 2, 0x80, 0x40];
        assert_eq!(
            ubb_ia_coverage(&ops, 10),
            vec![0, 0, 0, 1, 1, 0, 1, 1, 0, 0]
        );
        let pal = [0x1234u16; 256];
        let mut dst = vec![0u16; 10];
        compose_ubb_ia(&mut dst, &[0; 10], &pal, &ops, |c, _, _| c);
        let drawn: Vec<u8> = dst.iter().map(|&p| u8::from(p != 0)).collect();
        assert_eq!(drawn, ubb_ia_coverage(&ops, 10));
    }

    #[test]
    fn spr_blend_exhaustive() {
        for (src, want) in [
            (0x0000u16, "45daab81bcefdc11f968460e2ee4f660"),
            (0x8410, "0fab5bc750835a035abaa94483f3b040"),
        ] {
            let out: Vec<u16> = (0..32u32 * 65536)
                .map(|g| blend_spr_pixel565(src, g as u16, (g >> 16) as u8))
                .collect();
            assert_eq!(md5_le(&out), want);
        }
    }

    #[test]
    fn spw_blend_exhaustive() {
        for (src, want) in [
            (0x0000u16, "7e6dda789be67392f8e535076afe972a"),
            (0x8410, "c5ca5e0245bc7cd9a00458b2a04041a0"),
        ] {
            let out: Vec<u16> = (0..32u32 * 65536)
                .map(|g| blend_spw_pixel565(src, g as u16, (g >> 16) as u8))
                .collect();
            assert_eq!(md5_le(&out), want);
        }
    }

    // FUN_004108e0: checked on every pixel against the per-channel reading
    // of its asm (no carry crosses a channel).
    #[test]
    fn menu_dim_exhaustive() {
        for p in 0..=0xffffu32 {
            let (r, g, b) = (p >> 11, (p >> 5) & 0x3f, p & 0x1f);
            let want565 =
                (((r - (r >> 1)) << 11) | ((g - ((g & 0x3c) >> 1)) << 5) | (b - (b >> 1))) as u16;
            assert_eq!(dim_menu_pixel(p as u16, true), want565);
            if p & 0x8000 != 0 {
                continue;
            }
            let (r5, g5) = (p >> 10, (p >> 5) & 0x1f);
            let want555 =
                (((r5 - (r5 >> 1)) << 10) | ((g5 - (g5 >> 1)) << 5) | (b - (b >> 1))) as u16;
            assert_eq!(dim_menu_pixel(p as u16, false), want555);
        }
        assert_eq!(dim_menu_pixel(0xffff, true), 0x8430);
    }
}
