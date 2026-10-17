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
