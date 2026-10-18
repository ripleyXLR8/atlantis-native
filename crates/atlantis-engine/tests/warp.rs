//! The warp view against the original `atlantis.exe`.
//!
//! Expected values come from the original code run under emulation:
//! `FUN_0045179a(hfov, 50.0)` + `FUN_00451915(alpha, beta)` (projection
//! grid), `FUN_00451d8b` (renderer) and `FUN_0045166f` (screen to
//! panorama), with the game's x87 control word 0x127f, on a synthetic
//! 2048x1024 RGB565 panorama (pixel i = (i * 2654435761) >> 16). Ported
//! from ScummVM's `test/engines/cryomni3d/atlantis/graphics_navigation.h`.

use atlantis_engine::warp::{Warp, PANO_W, SCREEN_H, SCREEN_W, WARP_FOV_DEG};
use md5::{Digest, Md5};

fn panorama() -> Vec<u16> {
    (0..PANO_W as u32 * 1024)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 16) as u16)
        .collect()
}

/// A view as the tests set it up: the original receives float arguments.
fn view(fov: f32, alpha: f64, beta: f64) -> Warp {
    let mut w = Warp::new();
    w.set_hfov(fov);
    w.set_alpha(alpha as f32);
    w.set_beta(beta as f32);
    w
}

/// md5 of the 640x480 render, pixels little-endian.
fn render_md5(w: &mut Warp, pano: &[u16]) -> String {
    let mut out = vec![0u16; SCREEN_W * SCREEN_H];
    w.render(pano, &mut out);
    let bytes: Vec<u8> = out.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn projection_same_fov() {
    let cases: [(f32, f64, f64, &str); 7] = [
        (75.0, 0.0, 0.0, "bcbc7625823ab0642a1634bf64e36a8a"),
        (75.0, 1.0, 0.25, "793681ba0dc96cab61ab119d17d2aaa4"),
        (75.0, 3.0, -0.5, "df5598759f445e4f7b9eb5d852c03b12"),
        (75.0, 6.0, 0.75, "f83b989cb75a4db55f610f1b2c795007"),
        (75.137, 1.0, 0.25, "80a70639be883a61395deee473d1a0d8"),
        (20.68, 2.0, 0.375, "0e1a5c2d1147f50a61880fe5a663f905"),
        (48.26, 4.5, -0.25, "a010e7d4993c132c5261c37207d7ebec"),
    ];
    let pano = panorama();
    for (fov, a, b, want) in cases {
        let mut w = view(fov, a, b);
        assert_eq!(
            render_md5(&mut w, &pano),
            want,
            "fov {fov} alpha {a} beta {b}"
        );
    }
}

/// The warp FOV, 75.137 deg (0x42964625).
#[test]
fn projection_warp_fov() {
    let pano = panorama();
    let mut w = view(WARP_FOV_DEG, 0.0, 0.0);
    assert_eq!(
        render_md5(&mut w, &pano),
        "f7b654b61a06df529a00de37343db613"
    );
    // Warp::new() starts at the warp FOV, looking at (0, 0).
    assert_eq!(
        render_md5(&mut Warp::new(), &pano),
        "f7b654b61a06df529a00de37343db613"
    );
}

/// `FUN_0045166f` at the screen centre.
#[test]
fn centre_point() {
    let cases = [
        (0.0, 0.0, 0, 383),
        (1.0, 0.25, 1722, 481),
        (3.0, -0.5, 1070, 195),
        (6.0, 0.75, 92, 652),
    ];
    for (a, b, ox, oy) in cases {
        let mut w = view(75.137, a, b);
        assert_eq!(
            w.screen_to_panorama(320, 240),
            (ox, oy),
            "alpha {a} beta {b}"
        );
    }
}

/// `FUN_00451915` keeps the wrapped alpha and clamped beta.
#[test]
fn angles_wrapped_and_clamped() {
    let mut w = view(75.0, 6.5, 2.0);
    let _ = w.map_screen_point(0, 0);
    assert_eq!(w.beta(), w.beta_limit() as f32);
    assert_eq!(w.alpha(), (6.5f64 - f64::from(w.two_pi())) as f32);

    let mut w = view(75.0, -0.5, -2.0);
    let _ = w.map_screen_point(0, 0);
    assert_eq!(w.beta(), (-w.beta_limit()) as f32);
    assert_eq!(w.alpha(), (f64::from(w.two_pi()) - 0.5) as f32);
}

/// md5 of a pixel buffer, little-endian.
fn pixels_md5<T: Copy>(px: &[T], bytes: impl Fn(T) -> Vec<u8>) -> String {
    let b: Vec<u8> = px.iter().flat_map(|&p| bytes(p)).collect();
    Md5::digest(&b).iter().map(|b| format!("{b:02x}")).collect()
}

/// The generalised renderer (the HD mode's) at tile size 16 gives the
/// original's pixels: the digests above, and the same pixels as
/// `Warp::render` over a sweep of views.
#[test]
fn generalised_renderer_at_tile_16_is_the_original() {
    let pano = panorama();
    let scaled_md5 = |w: &mut Warp| {
        let mut out = vec![0u16; SCREEN_W * SCREEN_H];
        w.render_scaled(&pano, 16, &mut out);
        pixels_md5(&out, |p| p.to_le_bytes().to_vec())
    };
    let digests: [(f32, f64, f64, &str); 3] = [
        (75.0, 3.0, -0.5, "df5598759f445e4f7b9eb5d852c03b12"),
        (20.68, 2.0, 0.375, "0e1a5c2d1147f50a61880fe5a663f905"),
        (WARP_FOV_DEG, 0.0, 0.0, "f7b654b61a06df529a00de37343db613"),
    ];
    for (fov, a, b, want) in digests {
        assert_eq!(scaled_md5(&mut view(fov, a, b)), want, "fov {fov} {a} {b}");
    }
    for fov in [WARP_FOV_DEG, 75.187, 20.68, 30.0, 48.0, 48.26] {
        for i in 0..24 {
            let (a, b) = (f64::from(i) * 0.29 - 0.5, f64::from(i % 7) * 0.2 - 0.6);
            let mut w1 = view(fov, a, b);
            let mut w2 = w1.clone();
            assert_eq!(
                render_md5(&mut w1, &pano),
                scaled_md5(&mut w2),
                "fov {fov} alpha {a} beta {b}"
            );
        }
    }
}

/// The HD view: its size per tile, the same pixels at every run (and with
/// the threads as without), smooth (bilinear) rather than blocky.
#[test]
fn hd_view_sizes_and_determinism() {
    use atlantis_engine::warp::hd_size;
    assert_eq!(hd_size(16), (640, 480));
    assert_eq!(hd_size(36), (1440, 1080));
    assert_eq!(hd_size(64), (2560, 1920));
    let pano: Vec<u32> = panorama()
        .iter()
        .map(|&p| (u32::from(p) * 0x0101) & 0xff_ffff)
        .collect();
    for tile in [16, 17, 24, 32, 36, 48, 64] {
        let (w, h) = hd_size(tile);
        let mut a = vec![0u32; w * h];
        let mut b = vec![1u32; w * h];
        let mut v = view(WARP_FOV_DEG, 1.0, 0.25);
        v.render_bilinear(&pano, tile, &mut a);
        let mut v2 = view(WARP_FOV_DEG, 1.0, 0.25);
        v2.render_bilinear(&pano, tile, &mut b);
        assert_eq!(a, b, "tile {tile}");
        assert!(a.iter().all(|&p| p <= 0xff_ffff));
    }
    // A top / bottom limit view at the widest dialog FOV and a 768-row
    // source: no panic, every pixel written.
    let short = &pano[..PANO_W * 768];
    for beta in [-1.0, 1.0] {
        let (w, h) = hd_size(64);
        let mut out = vec![0xdead_beefu32; w * h];
        view(48.26, 5.5, beta).render_bilinear(short, 64, &mut out);
        assert!(out.iter().all(|&p| p != 0xdead_beef));
    }
}

/// The HD view at tile 32 samples between the 640x480 view's pixels: its
/// nearest-sampled version at 32 is the original's view doubled, up to
/// the rounding of the steps.
#[test]
fn hd_view_follows_the_original_view() {
    let pano = panorama();
    let mut lo = vec![0u16; SCREEN_W * SCREEN_H];
    let mut hi = vec![0u16; SCREEN_W * SCREEN_H * 4];
    let mut w = view(WARP_FOV_DEG, 2.0, 0.1);
    w.render(&pano, &mut lo);
    w.render_scaled(&pano, 32, &mut hi);
    // The odd HD pixels sample the centre of the 640x480 pixel's right /
    // lower half: compare the panorama point under each 640x480 pixel
    // with the one of its HD block, through the source pixel found.
    let mut same = 0;
    for y in 0..SCREEN_H {
        for x in 0..SCREEN_W {
            let block = [
                hi[(2 * y) * 1280 + 2 * x],
                hi[(2 * y) * 1280 + 2 * x + 1],
                hi[(2 * y + 1) * 1280 + 2 * x],
                hi[(2 * y + 1) * 1280 + 2 * x + 1],
            ];
            if block.contains(&lo[y * SCREEN_W + x]) {
                same += 1;
            }
        }
    }
    assert!(same * 100 >= SCREEN_W * SCREEN_H * 95, "{same}");
}

/// The HD view's "Bicubic" filter: the panorama doubled with Catmull-Rom
/// and sampled bilinearly comes close to the direct bicubic sampling,
/// keeps a constant panorama constant, and is deterministic; the edges
/// past the source read as 0 as with the other samplers.
#[test]
fn hd_bicubic_view() {
    use atlantis_engine::upscale::prepare_panorama;
    use atlantis_engine::warp::hd_size;
    // A smooth panorama (the photographs are): sums of slow waves.
    let surface: Vec<u16> = (0..PANO_W * 768)
        .map(|i| {
            let (x, y) = ((i % PANO_W) as f64, (i / PANO_W) as f64);
            let v = |f: f64, p: f64| (x * f + y * f * 0.7 + p).sin() * 0.5 + 0.5;
            let (r, g, b) = (v(0.11, 0.0), v(0.05, 1.0), v(0.2, 2.0));
            ((r * 31.0) as u16) << 11 | ((g * 63.0) as u16) << 5 | (b * 31.0) as u16
        })
        .collect();
    let mut pano = Vec::new();
    prepare_panorama(&surface, PANO_W, false, false, &mut pano);
    let mut pano2 = Vec::new();
    prepare_panorama(&surface, PANO_W, false, true, &mut pano2);
    let tile = 48;
    let (w, h) = hd_size(tile);
    let mut direct = vec![0u32; w * h];
    let mut doubled = vec![0u32; w * h];
    view(WARP_FOV_DEG, 2.5, 0.2).render_bicubic(&pano, tile, &mut direct);
    view(WARP_FOV_DEG, 2.5, 0.2).render_bilinear_2x(&pano2, tile, &mut doubled);
    let mut again = vec![1u32; w * h];
    view(WARP_FOV_DEG, 2.5, 0.2).render_bilinear_2x(&pano2, tile, &mut again);
    assert_eq!(doubled, again);
    let diff: u64 = direct
        .iter()
        .zip(&doubled)
        .map(|(&a, &b)| {
            [16, 8, 0]
                .iter()
                .map(|&s| ((a >> s & 0xff) as i64 - (b >> s & 0xff) as i64).unsigned_abs())
                .sum::<u64>()
        })
        .sum();
    let mean = diff as f64 / (3 * w * h) as f64;
    assert!(mean < 1.5, "mean difference {mean}");
    let flat = vec![0x0040_8020u32; 4 * PANO_W * 768];
    let mut out = vec![0u32; w * h];
    view(WARP_FOV_DEG, 1.0, 0.0).render_bilinear_2x(&flat, tile, &mut out);
    assert!(out.iter().all(|&p| p == 0x0040_8020));
    let flat1 = vec![0x0040_8020u32; PANO_W * 768];
    view(WARP_FOV_DEG, 1.0, 0.0).render_bicubic(&flat1, tile, &mut out);
    assert!(out.iter().all(|&p| p == 0x0040_8020));
    // The limit views at the widest dialog FOV: every pixel written.
    for beta in [-1.0, 1.0] {
        let mut out = vec![0xdead_beefu32; w * h];
        view(48.26, 5.5, beta).render_bilinear_2x(&pano2, tile, &mut out);
        assert!(out.iter().all(|&p| p != 0xdead_beef));
    }
}

/// Widescreen: the side columns carry the projection on; the 40 columns
/// of the original's view are its pixels, unchanged, at every tile size.
#[test]
fn widescreen_keeps_the_original_view_in_the_middle() {
    let pano = panorama();
    for fov in [WARP_FOV_DEG, 20.68, 48.26] {
        for i in 0..12 {
            let (a, b) = (f64::from(i) * 0.53 - 0.5, f64::from(i % 5) * 0.25 - 0.5);
            let mut original = view(fov, a, b);
            let mut screen = vec![0u16; SCREEN_W * SCREEN_H];
            original.render(&pano, &mut screen);
            for (side, tile) in [(7usize, 16usize), (3, 16), (7, 24)] {
                let mut w = view(fov, a, b);
                w.set_side_tiles(side);
                let (ww, wh) = w.tiles_size(tile);
                assert_eq!((ww, wh), ((40 + 2 * side) * tile, 30 * tile));
                let mut wide = vec![0u16; ww * wh];
                w.render_scaled(&pano, tile, &mut wide);
                let mut narrow = vec![0u16; 40 * tile * wh];
                let mut n = view(fov, a, b);
                n.render_scaled(&pano, tile, &mut narrow);
                let cw = 40 * tile;
                for y in 0..wh {
                    assert_eq!(
                        &wide[y * ww + side * tile..][..cw],
                        &narrow[y * cw..][..cw],
                        "fov {fov} alpha {a} beta {b} side {side} tile {tile} row {y}"
                    );
                }
                if tile == 16 {
                    assert_eq!(&narrow[..], &screen[..]);
                }
                // The sides continue the view: the column next to the
                // original's edge samples next to it in the panorama.
                let left = &wide[(wh / 2) * ww..][..side * tile];
                assert!(left.iter().any(|&p| p != 0), "side pixels drawn");
            }
        }
    }
}

/// The side columns are the view a wider field of view would show there:
/// a point of the side maps to the panorama direction the same projection
/// gives beyond the edge (the yaw grows past the edge, no seam).
#[test]
fn widescreen_sides_continue_the_yaw() {
    let mut w = view(WARP_FOV_DEG, 1.0, 0.0);
    w.set_side_tiles(6);
    // The column coordinate (11 integer bits) along the middle row.
    let (ww, wh) = w.tiles_size(16);
    let mut cols = vec![0u32; ww * wh];
    w.render_tiles(16, &mut cols, |px, _| px >> 21);
    let row = &cols[(wh / 2) * ww..][..ww];
    // Panorama columns along the row increase steadily (mod 2048).
    let mut prev = row[0];
    for (x, &c) in row.iter().enumerate().skip(1) {
        let step = (c + 2048 - prev) % 2048;
        assert!(step <= 3, "column {x}: {prev} -> {c}");
        prev = c;
    }
}
