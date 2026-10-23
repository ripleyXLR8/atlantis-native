//! The HD mode (`atlantis2_engine::hd`): the panorama at more pixels with
//! the original's projection, the original's frames unchanged.
//!
//! - the centre ray and sample rays of the HD view are the original's:
//!   the camera's ray of the screen centre (`FUN_0045adf0`, the zone and
//!   sprite picks), and for any screen point the cube point whose
//!   projection by the original's formulas (`FUN_0045c030`: the matrix,
//!   sx = fx·x/z + cx) is that point;
//! - drawn with the original's nearest texel at the original's pixel
//!   centres, the HD view gives `warpPII`'s pixels but for its one-texel
//!   truncations (synthetic faces whose pixels name their texel);
//! - a window pixel is the 640x480 pixel the composition shows there, so
//!   a click lands where it would in the original;
//! - with the game (`ATLANTIS2_DATA`, `ATLANTIS2_DISCS`; skipped
//!   without): the 640x480 frames are the same with the HD mode on or off,
//!   in Tibet, Ireland and China, the layer's base is the cube the frame
//!   shows, and the zone under the centre is the same.

use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::camera::deg_to_rad;
use atlantis2_engine::hd::{
    cube_point, picture_rect, render_view, side_tiles, tile_for_output, window_to_screen,
    HdProjection, SCREEN_H, SCREEN_W,
};
use atlantis2_engine::{Camera, Engine, Enhancements, Fpu, LoopType, Renderer, Variant};
use atlantis_engine::hd::{HdCompositor, HdFrame, HdOptions};
use atlantis_engine::upscale::Axis;

/// A camera entered at (yaw, pitch) degrees, under 0x0e7f (a view after
/// the first).
fn camera(yaw: i32, pitch: i32) -> Camera {
    let mut c = Camera::new(Fpu::Double, 640, 480);
    c.fpu = Fpu::Chop;
    c.enter(deg_to_rad(Fpu::Chop, yaw), deg_to_rad(Fpu::Chop, pitch));
    c.set_fov(atlantis2_engine::camera::WARP_FOV, 640, 480);
    c
}

/// The angles the tests look at: the original's golden grid's pitches and
/// every 15° of yaw, the face edges and corners included.
fn angles() -> Vec<(i32, i32)> {
    let mut a = Vec::new();
    for yaw in (-180..180).step_by(15) {
        for pitch in [-70, -45, -20, 0, 20, 45, 70] {
            a.push((yaw, pitch));
        }
    }
    a
}

fn norm(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / n)
}

/// The world point's camera coordinates as `FUN_0045c6d0` turns the
/// cube's corners, and its screen point as `FUN_0045c030` projects them.
fn project(cam: &Camera, p: [f64; 3]) -> (f64, f64) {
    let m = cam.matrix().map(f64::from);
    let [x, y, z] = p;
    let cx = (z * m[6] + y * m[3]) + x * m[0];
    let cy = (x * m[1] + z * m[7]) + y * m[4];
    let cz = (x * m[2] + z * m[8]) + y * m[5];
    let [fx, fy, ox, oy] = cam.projection().map(f64::from);
    (fx * cx / cz + ox, fy * cy / cz + oy)
}

/// The point of the cube (half-size 10) at face `face`, texel (u, v):
/// the inverse of the cube's face table (`warp.rs`, `INDICES`; the
/// corners' texture coordinates of `FUN_0045c790`).
fn face_point(face: usize, u: f64, v: f64) -> [f64; 3] {
    let s = |t: f64| ((t - 1.0) / 510.0) * 2.0 - 1.0;
    let (a, b) = (s(u) * 10.0, s(v) * 10.0);
    match face {
        0 => [a, b, 10.0],
        1 => [-a, b, -10.0],
        2 => [a, -10.0, b],
        3 => [a, 10.0, -b],
        4 => [-10.0, b, a],
        5 => [10.0, b, -a],
        _ => unreachable!(),
    }
}

/// The screen centre: the HD view's ray there is the camera's
/// (`FUN_0045adf0`, the ray the zone and sprite picks use), and the
/// window's centre is the screen's (cx, cy) at every HD size, sides or
/// not.
#[test]
fn the_centre_ray_is_the_original_s() {
    for (yaw, pitch) in angles() {
        let cam = camera(yaw, pitch);
        let ray = cam.ray(cam.cx, cam.cy).map(f64::from);
        for (tile, side_tiles) in [(16, 0), (36, 0), (36, 7), (48, 16), (64, 3)] {
            let side = side_tiles * tile;
            let p = HdProjection::new(&cam.matrix(), cam.projection(), tile, side);
            // The window's centre: the edge between its two middle columns
            // and rows.
            let (w, h) = (40 * tile + 2 * side, 30 * tile);
            let (sx, sy) = p.screen_point(w as f64 / 2.0 - 0.5, h as f64 / 2.0 - 0.5);
            assert_eq!((sx, sy), (320.0, 240.0));
            let d = p.direction(sx, sy);
            let (a, b) = (norm(d), norm(ray));
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-6, "y{yaw} p{pitch}: {a:?} {b:?}");
            }
            let (fa, ua, va) = cube_point(d).unwrap();
            let (fb, ub, vb) = cube_point(ray).unwrap();
            assert_eq!(fa, fb);
            assert!((ua - ub).abs() < 1e-3 && (va - vb).abs() < 1e-3);
        }
    }
}

/// Sample rays: the cube point of the HD view's ray through a screen
/// point projects, by the original's formulas, back to that point; past
/// the 640 columns too (widescreen).
#[test]
fn sample_rays_follow_the_original_projection() {
    let mut n = 0;
    for (yaw, pitch) in angles() {
        let cam = camera(yaw, pitch);
        let p = HdProjection::new(&cam.matrix(), cam.projection(), 36, 7 * 36);
        for sx in (-150..=790).step_by(37) {
            for sy in (0..=480).step_by(29) {
                let (sx, sy) = (sx as f64 + 0.25, sy as f64 + 0.5);
                let (face, u, v) = cube_point(p.direction(sx, sy)).unwrap();
                assert!((1.0..=511.0).contains(&u) && (1.0..=511.0).contains(&v));
                let (px, py) = project(&cam, face_point(face, u, v));
                assert!(
                    (px - sx).abs() < 1e-3 && (py - sy).abs() < 1e-3,
                    "y{yaw} p{pitch} ({sx}, {sy}) -> face {face} ({u}, {v}) -> ({px}, {py})"
                );
                n += 1;
            }
        }
    }
    assert!(n > 70_000, "{n}");
}

/// The faces' texel (u, v) = ((v & 0x7f) << 9) | u, as the original's
/// `uv` goldens: each pixel tells the texel it sampled.
fn uv_faces() -> [Vec<u16>; 6] {
    let face: Vec<u16> = (0..512 * 512)
        .map(|i| ((((i / 512) & 0x7f) << 9) | (i % 512)) as u16)
        .collect();
    std::array::from_fn(|_| face.clone())
}

/// The HD view drawn with the original's nearest texel at `tile`: each
/// pixel the face and texel, `face << 20 | v << 10 | u`.
fn nearest_view(cam: &Camera, tile: usize) -> Vec<u32> {
    let p = HdProjection::new(&cam.matrix(), cam.projection(), tile, 0);
    let (w, h) = (40 * tile, 30 * tile);
    let mut out = vec![0u32; w * h];
    render_view(&p, w, h, &mut out, |f, u, v| {
        let t = |c: f64| (c as u32).min(511);
        (f as u32) << 20 | t(v) << 10 | t(u)
    });
    out
}

/// At the original's pixel centres (tile 16; tile 48, every third pixel
/// from the second), the HD view with the original's nearest texel is
/// `warpPII`'s frame but where `warpPII`'s approximations move it: its
/// spans interpolate affinely between divisions 32 pixels apart, and step
/// in truncated 16-bit words (most often to the texel before), and at a
/// face's edge its polygon edge may give the pixel to the next face.
/// Nowhere more than two texels away, more than one only in a few pixels
/// in a million.
#[test]
fn nearest_texels_are_warp_pii_s() {
    let faces = uv_faces();
    let (mut same, mut total, mut edge, mut worst) = (0usize, 0usize, 0usize, 0.0f64);
    let mut hist = [0usize; 10];
    let mut lower = 0usize;
    for (yaw, pitch) in angles() {
        let cam = camera(yaw, pitch);
        let mut r = Renderer::new(Variant::PII, cam.fpu, 640, 480);
        let mut pii = vec![0u16; 640 * 480];
        r.render(&faces, &cam.matrix(), cam.projection(), &mut pii);
        for (tile, k, o) in [(16, 1, 0), (48, 3, 1)] {
            let hd = nearest_view(&cam, tile);
            let mut s = 0;
            for y in 0..480 {
                for x in 0..640 {
                    let a = pii[y * 640 + x];
                    let b = hd[(k * y + o) * 40 * tile + k * x + o];
                    let (ua, va) = (i32::from(a & 0x1ff), i32::from(a >> 9));
                    let (ub, vb) = ((b & 0x3ff) as i32, ((b >> 10) & 0x3ff) as i32);
                    if (ua, va) == (ub, vb & 0x7f) {
                        s += 1;
                        continue;
                    }
                    // On a face's edge texel (1 or 510), the original's
                    // polygon edge may give the pixel to the next face.
                    if [ub, vb].iter().any(|&c| c <= 1 || c >= 510) {
                        edge += 1;
                        continue;
                    }
                    let dv = (va - vb).rem_euclid(128).min((vb - va).rem_euclid(128));
                    let d = (ua - ub).abs().max(dv) as usize;
                    hist[d.min(9)] += 1;
                    lower += usize::from(ua < ub || (va - vb).rem_euclid(128) == 127);
                }
            }
            same += s;
            total += 640 * 480;
            worst = worst.max(1.0 - s as f64 / (640.0 * 480.0));
        }
    }
    let share = same as f64 / total as f64;
    eprintln!(
        "HD nearest vs warpPII: {:.2} % of the pixels the same, at worst {:.2} % differ; \
         {edge} on a face's edge; texels away {hist:?}, {lower} lower",
        share * 100.0,
        worst * 100.0
    );
    // warpPII: 82.75 % the same, 17.2 % one texel away (83 % of them
    // lower), 740 pixels of 103 million two texels away, a few on a face's
    // edge; warpC (the affine spans without the 16-bit steps): 87 %.
    assert!(share > 0.8, "{share}");
    assert!(hist[2] * 10_000 < total, "{hist:?}");
    assert!(hist[3..].iter().all(|&n| n == 0), "{hist:?}");
    assert!(edge * 1000 < total, "{edge}");
}

/// Every window pixel of the picture shows the 640x480 pixel
/// `window_to_screen` gives: a click there is a click on that pixel, at
/// every window size, with or without widescreen sides (which show no
/// 640x480 pixel).
#[test]
fn clicks_land_on_the_pixel_shown() {
    // A frame whose every pixel is different (no layer): the composition
    // as pixels.
    let screen: Vec<u16> = (0..SCREEN_W * SCREEN_H)
        .map(|i| ((i % SCREEN_W) as u16) | (((i / SCREEN_W) as u16 & 0x3f) << 10))
        .collect();
    let opts = HdOptions {
        smooth_interface: false,
        sharp_subtitles: false,
        ..HdOptions::default()
    };
    let lut = atlantis2_engine::hd::rgb565_to_888();
    for (w, h) in [
        (1920, 1080),
        (1280, 960),
        (2560, 1440),
        (1366, 768),
        (1024, 768),
        (800, 1000),
    ] {
        let rect = picture_rect(w, h);
        let (ox, oy, dw, dh) = rect;
        let (ax, ay) = (Axis::new(SCREEN_W, dw), Axis::new(SCREEN_H, dh));
        let mut out = vec![0u32; w * h];
        let mut c = HdCompositor::new();
        let f = HdFrame {
            screen: &screen,
            layer: None,
            texts: &[],
            photo: false,
            video_pano: None,
        };
        c.compose(f, opts, &mut out, w, ox, oy, dw, dh);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = window_to_screen(x as i64, y as i64, rect);
                let inside = (ox..ox + dw).contains(&x) && (oy..oy + dh).contains(&y);
                assert_eq!(
                    inside,
                    (0..640).contains(&sx) && (0..480).contains(&sy),
                    "{w}x{h} ({x}, {y})"
                );
                if inside {
                    assert_eq!(
                        (sx as usize, sy as usize),
                        (ax.nearest[x - ox], ay.nearest[y - oy])
                    );
                    let p = screen[sy as usize * SCREEN_W + sx as usize];
                    assert_eq!(out[y * w + x], lut[usize::from(p)], "{w}x{h} ({x}, {y})");
                }
            }
        }
        // The HD size covers the picture; widescreen fills the bars.
        let tile = tile_for_output(w, h, 64);
        if dw > 640 {
            assert!(tile.unwrap() * 40 >= dw.min(64 * 40), "{w}x{h}");
        }
        let side = side_tiles(ox, dw);
        if ox > 0 && side < 16 {
            assert!(side * dw / 40 >= ox, "{w}x{h}: {side} tile columns");
        }
    }
}

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// A game at a place's view: Tibet (a new game's first view), or the
/// commands of another world's section, the films and dialogs ended.
fn at(files: A2Files, commands: &[&[u8]], hd: bool) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements = Enhancements {
        hd,
        ..Enhancements::default()
    };
    e.start_new_game().unwrap();
    e.end_video();
    for c in commands {
        e.run_command(c);
    }
    for _ in 0..200 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp {
            break;
        }
    }
    e
}

/// The places of the three worlds the port plays.
const PLACES: [(&str, &[&[u8]]); 3] = [
    ("Tibet", &[]),
    (
        "Ireland",
        &[
            b"(monde=1)",
            b"(changewam=bigile1.wam)",
            b"(vue=5)",
            b"(chapter=2)",
        ],
    ),
    (
        "China",
        &[b"(monde=3)", b"(changewam=chine.wam)", b"(vue=10)"],
    ),
];

/// With the HD mode on, the 640x480 frames are those of the HD mode off,
/// pass after pass while the view turns and the place animations play;
/// the layer stands for the cube under them; the zone under the centre is
/// the same.
#[test]
fn hd_keeps_the_original_frames() {
    if game().is_none() {
        return;
    }
    for (name, commands) in PLACES {
        let mut off = at(game().unwrap(), commands, false);
        let mut on = at(game().unwrap(), commands, true);
        on.set_hd_output(Some(20), 4);
        off.set_hd_output(Some(20), 4);
        let (mut a, mut b) = (vec![0u16; 640 * 480], vec![0u16; 640 * 480]);
        let mut layers = 0;
        for i in 0..90 {
            for e in [&mut off, &mut on] {
                e.set_clock(i as f64 / 60.0);
                e.mouse_motion(
                    if i < 45 { 23 } else { -11 },
                    if i % 30 < 15 { 9 } else { -9 },
                );
            }
            off.pass(Some(&mut a));
            on.pass(Some(&mut b));
            assert!(a == b, "{name}: frame {i} differs with the HD mode");
            assert!(off.hd_layer().is_none());
            assert_eq!(
                off.centre_zone().map(|z| z.0),
                on.centre_zone().map(|z| z.0),
                "{name}: frame {i}"
            );
            if let Some(l) = on.hd_layer() {
                layers += 1;
                assert_eq!((l.width, l.height, l.side), (40 * 20 + 2 * 80, 30 * 20, 80));
                // The cube under the cursor and the centre marker: the
                // frame differs from the base only around the centre.
                let differ: Vec<usize> = (0..640 * 480).filter(|&k| b[k] != l.base[k]).collect();
                for &k in &differ {
                    let (x, y) = (k % 640, k / 640);
                    assert!(
                        (x as i32 - 320).abs() < 80 && (y as i32 - 240).abs() < 80,
                        "{name}: frame {i}: ({x}, {y}) differs from the base"
                    );
                }
            }
        }
        assert!(layers >= 80, "{name}: {layers} frames with the layer");
    }
}

/// The HD view of a place: the cube's colours where the 640x480 frame
/// shows them, at the HD size: each original pixel centre's HD pixel is
/// within the colours of the texels around the original's.
#[test]
fn hd_view_shows_the_panorama() {
    if game().is_none() {
        return;
    }
    for (name, commands) in PLACES {
        let mut e = at(game().unwrap(), commands, true);
        e.enhancements.hd_options.sharpen = false;
        e.enhancements.hd_options.panorama = atlantis2_engine::hd::PanoFilter::Bilinear;
        e.set_hd_output(Some(48), 0);
        let mut out = vec![0u16; 640 * 480];
        e.pass(Some(&mut out));
        let l = e.hd_layer().expect("a layer");
        let lut = atlantis2_engine::hd::rgb565_to_888();
        let (mut close, mut n) = (0, 0);
        for y in (0..480).step_by(7) {
            for x in (0..640).step_by(7) {
                let o = lut[usize::from(l.base[y * 640 + x])];
                let h = l.pixels[(3 * y + 1) * l.width + 3 * x + 1];
                let d = (0..3)
                    .map(|k| ((o >> (8 * k)) & 0xff).abs_diff((h >> (8 * k)) & 0xff))
                    .max()
                    .unwrap();
                close += usize::from(d <= 48);
                n += 1;
            }
        }
        eprintln!("{name}: {close} of {n} sample pixels close to the original's");
        assert!(close * 10 >= n * 9, "{name}: {close} of {n}");
    }
}

/// The HD view's timings (`cargo test --release -p atlantis2-engine
/// --test hd -- --ignored --nocapture hd_timings`): the faces prepared
/// (a new view; a frame where nothing changed; a sprite's frame), the
/// view drawn at 1920x1080 (4:3 and widescreen) and 2560x1440.
#[test]
#[ignore]
fn hd_timings() {
    use atlantis2_engine::hd::{HdPanorama, PanoFilter};
    use std::time::Instant;
    let faces: [Vec<u16>; 6] = std::array::from_fn(|f| {
        (0..512 * 512)
            .map(|i: u32| (i.wrapping_mul(2_654_435_761).wrapping_add(f as u32) >> 16) as u16)
            .collect()
    });
    let n = 20;
    let time = |f: &mut dyn FnMut(usize)| {
        let t = Instant::now();
        for i in 0..n {
            f(i);
        }
        t.elapsed().as_secs_f64() * 1e3 / n as f64
    };
    for filter in [PanoFilter::Bilinear, PanoFilter::Bicubic] {
        let opts = HdOptions {
            panorama: filter,
            ..HdOptions::default()
        };
        let new_view = time(&mut |_| {
            HdPanorama::new().update(&faces, &opts);
        });
        let mut pano = HdPanorama::new();
        pano.update(&faces, &opts);
        let still = time(&mut |_| {
            pano.update(&faces, &opts);
        });
        let mut moving = faces.clone();
        let sprite = time(&mut |i| {
            for y in 100..200 {
                for x in 200..260 {
                    moving[2][y * 512 + x] = (i * 977 + x) as u16;
                }
            }
            pano.update(&moving, &opts);
        });
        eprintln!(
            "{filter:?}: faces prepared: new view {new_view:.2} ms, unchanged {still:.2} ms, \
             a sprite's frame {sprite:.2} ms"
        );
        for (w, h, wide) in [(1920, 1080, false), (1920, 1080, true), (2560, 1440, true)] {
            let (ox, _, dw, _) = picture_rect(w, h);
            let tile = tile_for_output(w, h, 64).unwrap();
            let side = if wide { side_tiles(ox, dw) * tile } else { 0 };
            let (lw, lh) = (40 * tile + 2 * side, 30 * tile);
            let mut out = vec![0u32; lw * lh];
            let view = time(&mut |i| {
                let cam = camera(i as i32 * 17 - 170, (i as i32 % 7) * 10 - 30);
                let p = HdProjection::new(&cam.matrix(), cam.projection(), tile, side);
                pano.render(&p, lw, lh, &mut out);
            });
            eprintln!(
                "  {w}x{h}{}: view {lw}x{lh} {view:.2} ms",
                if wide { " widescreen" } else { "" }
            );
        }
    }
}

/// The view is drawn through each pixel's own ray: its texels are those
/// of [`HdProjection::direction`] at the pixel's screen point, sides
/// included.
#[test]
fn the_view_is_drawn_through_its_rays() {
    for (yaw, pitch) in [(-90, 0), (17, -63), (-150, 41), (45, 70)] {
        let cam = camera(yaw, pitch);
        let (tile, side) = (20, 3 * 20);
        let p = HdProjection::new(&cam.matrix(), cam.projection(), tile, side);
        let (w, h) = (40 * tile + 2 * side, 30 * tile);
        let mut out = vec![0u32; w * h];
        let code = |f: usize, u: f64, v: f64| (f as u32) << 20 | (v as u32) << 10 | u as u32;
        render_view(&p, w, h, &mut out, code);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = p.screen_point(x as f64, y as f64);
                let (f, u, v) = cube_point(p.direction(sx, sy)).unwrap();
                assert_eq!(out[y * w + x], code(f, u, v), "y{yaw} p{pitch} ({x}, {y})");
            }
        }
    }
}
