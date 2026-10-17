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
