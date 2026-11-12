//! The warp's maths against numbers the original executable's own code
//! produced (`tests/golden/a4_warp_*.txt`, see their headers).

use atlantis4_engine::camera::{perspective, view_quat, Camera, FAR, FOV_HD, FOV_SD, NEAR};
use atlantis4_engine::cube::{block_triangles, BLOCKS};
use atlantis4_engine::frame_anim::FrameAnim;
use atlantis4_engine::pick::{ray_triangle, Hit};
use atlantis4_engine::view::View;

fn golden(name: &str) -> Vec<String> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

fn nums(s: &str) -> Vec<f64> {
    s.split_whitespace().map(|x| x.parse().unwrap()).collect()
}

/// The golden's numbers are printed with 9 significant digits: a float
/// reads back to itself.
fn f(x: f64) -> f32 {
    x as f32
}

fn fov(deg: f64) -> f32 {
    if deg == 45.0 {
        FOV_SD
    } else {
        FOV_HD
    }
}

#[test]
fn block_triangles_are_the_originals_to_the_bit() {
    let lines = golden("a4_warp_blocks.txt");
    assert_eq!(lines.len(), 6 * 16 * 16);
    for l in &lines {
        let v = nums(l);
        let t = block_triangles(v[0] as usize, v[1] as u32, v[2] as u32, BLOCKS);
        let got: Vec<f32> = t.iter().flatten().flatten().copied().collect();
        let want: Vec<f32> = v[3..].iter().map(|&x| f(x)).collect();
        assert_eq!(got, want, "{l}");
    }
}

#[test]
fn perspective_matrices_are_the_originals_to_the_bit() {
    let lines = golden("a4_warp_projection.txt");
    assert_eq!(lines.len(), 18);
    for l in &lines {
        let v = nums(l);
        let m = perspective(
            fov(v[0]),
            v[1] as f32,
            v[2] as f32,
            v[3] as u32,
            v[4] as u32,
        );
        let want: Vec<f32> = v[5..].iter().map(|&x| f(x)).collect();
        assert_eq!(m.to_vec(), want, "{l}");
        assert_eq!((v[1] as f32, v[2] as f32), (NEAR, FAR));
    }
}

#[test]
fn yaw_and_pitch_setters_are_the_originals_to_the_bit() {
    let mut view = View::default();
    let mut n = 0;
    for l in golden("a4_warp_angles.txt") {
        let (head, tail) = l.split_once("->").unwrap_or((&l, ""));
        let w: Vec<&str> = head.split_whitespace().collect();
        let st = |v: &View| {
            [
                v.yaw,
                v.pitch,
                v.yaw_plus,
                v.yaw_minus,
                v.pitch_plus,
                v.pitch_minus,
            ]
        };
        if w[0] == "start" {
            let s: Vec<f32> = w[1..].iter().map(|x| f(x.parse().unwrap())).collect();
            view = View {
                yaw: s[0],
                pitch: s[1],
                yaw_plus: s[2],
                yaw_minus: s[3],
                pitch_plus: s[4],
                pitch_minus: s[5],
            };
            continue;
        }
        let x = f(w[1].parse().unwrap());
        match w[0] {
            "yaw" => view.set_yaw(x),
            "pitch" => view.set_pitch(x),
            o => panic!("{o}"),
        }
        let want: Vec<f32> = nums(tail).into_iter().map(f).collect();
        assert_eq!(st(&view).to_vec(), want, "{l}");
        n += 1;
    }
    assert_eq!(n, 19);
}

fn close(got: f64, want: f64, rel: f64, abs: f64) -> bool {
    (got - want).abs() <= abs.max(rel * want.abs())
}

#[test]
fn view_quaternions_rays_and_projections_match_the_original() {
    let (mut q, mut r, mut p) = (0, 0, 0);
    for l in golden("a4_warp_view.txt") {
        let (head, tail) = l.split_once("->").unwrap();
        let w: Vec<&str> = head.split_whitespace().collect();
        let a: Vec<f64> = w[1..].iter().map(|x| x.parse().unwrap()).collect();
        let want = nums(tail);
        match w[0] {
            "q" => {
                let got = view_quat(f(a[0]), f(a[1]));
                for k in 0..4 {
                    assert!(
                        close(f64::from(got[k]), want[k], 1e-6, 1e-7),
                        "{l}: {got:?}"
                    );
                }
                q += 1;
            }
            "ray" => {
                let cam = Camera::new(fov(a[0]), a[1] as u32, a[2] as u32, f(a[3]), f(a[4]));
                let d = cam.ray(a[5] as i32, a[6] as i32);
                assert_eq!(&want[..3], &[0.0; 3], "{l}");
                for k in 0..3 {
                    assert!(
                        close(f64::from(d[k]), want[3 + k], 1e-6, 1e-6),
                        "{l}: {d:?}"
                    );
                }
                r += 1;
            }
            "proj" => {
                let cam = Camera::new(fov(a[0]), a[1] as u32, a[2] as u32, f(a[3]), f(a[4]));
                let s = cam.project([f(a[5]), f(a[6]), f(a[7])]);
                // Points almost in the camera's plane (w near 0) magnify
                // the rounding of the original's floats: their screen
                // point only has to be as far off screen.
                if want[2].abs() > 100.0 {
                    assert!(!s[2].is_finite() || s[2].abs() > 100.0, "{l}: {s:?}");
                    p += 1;
                    continue;
                }
                // Behind the camera (z > 1) the screen point magnifies the
                // floats' rounding more.
                let rel = if want[2] > 1.0 { 1e-4 } else { 1e-5 };
                for k in 0..3 {
                    assert!(close(f64::from(s[k]), want[k], rel, 1e-3), "{l}: {s:?}");
                }
                p += 1;
            }
            o => panic!("{o}"),
        }
    }
    assert_eq!((q, r, p), (11, 396, 396));
}

#[test]
fn clicks_hit_the_originals_blocks() {
    type Block = ((usize, u32, u32), [[[f32; 3]; 3]; 2]);
    let tris: Vec<Block> = (0..6)
        .flat_map(|fc| (0..16).flat_map(move |ox| (0..16).map(move |oy| (fc, ox, oy))))
        .map(|(fc, ox, oy)| ((fc, ox, oy), block_triangles(fc, ox, oy, BLOCKS)))
        .collect();
    let lines = golden("a4_warp_pick.txt");
    let (mut edges, mut exact_edges) = (0, 0);
    for l in &lines {
        let (head, tail) = l.split_once("->").unwrap();
        let a = nums(head.trim_start_matches("click"));
        let cam = Camera::new(fov(a[0]), a[1] as u32, a[2] as u32, f(a[3]), f(a[4]));
        let d = cam.ray(a[5] as i32, a[6] as i32);
        let mut got = Vec::new();
        for (b, t) in &tris {
            for (k, tri) in t.iter().enumerate() {
                if let Hit::Hit(r) = ray_triangle([0.0; 3], d, tri) {
                    got.push((b.0, b.1, b.2, k, r));
                }
            }
        }
        let want: Vec<Vec<f64>> = tail
            .split(';')
            .map(nums)
            .filter(|v| !v.is_empty())
            .collect();
        let ids = |v: &[(usize, u32, u32, usize, f32)]| -> Vec<(usize, u32, u32, usize)> {
            v.iter().map(|h| (h.0, h.1, h.2, h.3)).collect()
        };
        let want_ids: Vec<_> = want
            .iter()
            .map(|v| (v[0] as usize, v[1] as u32, v[2] as u32, v[3] as usize))
            .collect();
        if want_ids.len() == 1 {
            assert_eq!(ids(&got), want_ids, "{l}");
        } else {
            // Several hits: the ray is exactly on a block edge (the middle
            // row at pitch 0, the middle column of an even width), and
            // which neighbours the floats hit depends on the ray's last
            // bit, which this getRay (doubles, not the original's chain of
            // float stores through 0x004ebbe0 / 0x004eb520 / 0x004fb930 /
            // 0x0050de10) does not always reproduce. At least one of them
            // and no other; the count of exact ones is kept.
            assert!(!got.is_empty(), "{l}");
            assert!(ids(&got).iter().all(|h| want_ids.contains(h)), "{l}");
            edges += 1;
            exact_edges += usize::from(ids(&got) == want_ids);
        }
        for g in &got {
            let w = &want[want_ids
                .iter()
                .position(|h| *h == (g.0, g.1, g.2, g.3))
                .unwrap()];
            assert!(close(f64::from(g.4), w[4], 1e-5, 1e-4), "{l}");
        }
    }
    eprintln!("edge clicks exact: {exact_edges} of {edges}");
    assert_eq!((lines.len(), edges), (110, 8));
    assert!(exact_edges >= EXACT_EDGES, "{exact_edges}");
}

#[test]
fn frame_animation_is_the_originals() {
    let lines = golden("a4_warp_frameanim.txt");
    for l in &lines {
        let (head, tail) = l.split_once("->").unwrap();
        let a = nums(head);
        let mut fa = FrameAnim {
            playing: true,
            start_ms: a[6],
            loops: a[4] as i32,
            reverse: a[5] != 0.0,
            n: a[0] as u32,
            fps: a[1] as f32,
            first: a[2] as i64 as u32,
            count: a[3] as u32,
            frame: a[8] as i32,
        };
        let ev = fa.update(a[7]);
        let t: Vec<&str> = tail.split_whitespace().collect();
        assert_eq!(fa.frame, t[0].parse::<i32>().unwrap(), "{l}");
        let mut evs = String::new();
        if ev.stopped {
            evs.push_str("XS");
        }
        if ev.finished {
            evs.push('F');
        }
        if ev.changed {
            evs.push('C');
        }
        assert_eq!(evs, t[1], "{l}");
    }
    assert_eq!(lines.len(), 960);
}

/// Edge clicks whose hits are exactly the original's.
const EXACT_EDGES: usize = 7;
