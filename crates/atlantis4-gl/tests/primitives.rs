//! Each drawing primitive on the processor and on the card, compared
//! within [`atlantis4_gl::check`]'s tolerance; synthetic pictures, no game
//! data. Needs a headless OpenGL context (see `common`), else skips.

mod common;

use atlantis4_data::anim::TileRange;
use atlantis4_engine::camera::{Camera, FOV_HD};
use atlantis4_engine::canvas::{Canvas, CpuCache, CpuCanvas, Edge};
use atlantis4_engine::cube::FACE_PICTURE;
use atlantis4_engine::layout::Rect;
use atlantis4_engine::movie::Mode;
use atlantis4_engine::package::Picture;
use atlantis4_engine::render::Weights;
use atlantis4_gl::check::{compare, Diff};
use atlantis4_gl::Renderer;

const W: u32 = 640;
const H: u32 = 360;

/// A reproducible noise.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u8 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 56) as u8
    }
}

/// A photograph-like picture: smooth gradients, some noise, a hard
/// edge, alpha from opaque to clear when `alpha`.
fn picture(w: u32, h: u32, seed: u64, alpha: bool) -> Picture {
    let mut r = Lcg(seed);
    let mut rgba = Vec::with_capacity((4 * w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let n = r.next() / 8;
            let edge = if x > w / 2 { 120 } else { 0 };
            rgba.extend([
                ((x * 255 / w.max(1)) as u8).wrapping_add(n),
                ((y * 255 / h.max(1)) as u8).saturating_add(edge),
                (((x + y) * 3) as u8) ^ n,
                if alpha {
                    ((x * 255 / w.max(1)) as u8).max((y * 255 / h.max(1)) as u8)
                } else {
                    255
                },
            ]);
        }
    }
    Picture {
        width: w,
        height: h,
        rgba,
    }
}

fn faces() -> [Vec<u32>; 6] {
    std::array::from_fn(|f| {
        let p = picture(
            FACE_PICTURE as u32,
            FACE_PICTURE as u32,
            11 + f as u64,
            false,
        );
        p.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]))
            .collect()
    })
}

/// Draws the same scene on both canvases.
fn scene(c: &mut dyn Canvas, faces: &[Vec<u32>; 6], cam: &Camera, pics: &[Picture]) {
    c.background(cam, faces, 1);
    // A tile: a quad in front of the camera, half transparent picture.
    let range = TileRange {
        first_frame: 0,
        vertices: vec![
            [-120.0, -60.0, -400.0],
            [130.0, -70.0, -420.0],
            [140.0, 90.0, -380.0],
            [-110.0, 80.0, -390.0],
        ],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    c.mesh(cam, &range, &pics[0]);
    c.sprite(&pics[1], (0, 8, 24, 24), 40.25, 30.75);
    c.picture(
        &pics[2],
        Rect {
            left: 300.4,
            top: 20.2,
            width: 150.7,
            height: 90.3,
        },
        Some((320, 430, 30, 100)),
        [200, 230, 255, 180],
    );
    c.rotated(
        &pics[1],
        Rect {
            left: 500.0,
            top: 200.0,
            width: 64.0,
            height: 48.0,
        },
        30.0,
        [255; 4],
    );
    let col: Vec<[u8; 4]> = (0..pics[3].height).map(|y| [y as u8, 0, 90, 255]).collect();
    let row: Vec<[u8; 4]> = (0..pics[3].width).map(|x| [0, x as u8, 40, 255]).collect();
    c.stretched(
        &pics[3],
        Rect {
            left: 60.0,
            top: 220.0,
            width: 213.3,
            height: 120.0,
        },
        Mode::Opaque,
        Edge::Margin {
            col: &col,
            row: &row,
        },
        None,
    );
    c.fill(
        Rect {
            left: 600.0,
            top: 300.0,
            width: 30.0,
            height: 40.0,
        },
        0x0040_8020,
    );
}

fn run(gl: glow::Context) -> Vec<(String, Diff)> {
    let mut r = Renderer::new(gl).expect("renderer");
    eprintln!("GL: {}", r.info);
    let faces = faces();
    let pics = [
        picture(96, 64, 1, true),
        picture(24, 48, 2, true),
        picture(50, 30, 3, true),
        picture(160, 90, 4, false),
    ];
    let mut out = Vec::new();
    for (yaw, pitch) in [(0.0, 0.0), (33.3, 12.5), (-140.0, -38.0), (91.0, 70.0)] {
        let cam = Camera::new(FOV_HD, W, H, yaw, pitch);
        let mut cpu = vec![0u32; (W * H) as usize];
        let mut cache = CpuCache::default();
        scene(
            &mut CpuCanvas::new(&mut cpu, W, H, &mut cache, Weights::Exact),
            &faces,
            &cam,
            &pics,
        );
        let mut f = r.begin(W, H);
        scene(&mut f, &faces, &cam, &pics);
        f.finish();
        let gpu = r.read_pixels();
        let d = compare(&cpu, &gpu, W as usize);
        if std::env::var_os("A4_GL_DUMP").is_some() {
            for (i, (a, b)) in cpu.iter().zip(&gpu).enumerate() {
                let e = [16, 8, 0]
                    .map(|s| ((a >> s) & 255).abs_diff((b >> s) & 255))
                    .into_iter()
                    .max()
                    .unwrap();
                if e > 4 {
                    eprintln!(
                        "  over {},{} {a:06x} {b:06x}",
                        i % W as usize,
                        i / W as usize
                    );
                }
            }
        }
        out.push((format!("yaw {yaw} pitch {pitch}"), d));
    }
    out
}

#[test]
fn the_card_draws_each_primitive_as_the_processor() {
    let Some(mut h) = common::context() else {
        return;
    };
    eprintln!("device: {}", h.device);
    let gl = h.gl.take().expect("gl");
    for (name, d) in run(gl) {
        eprintln!("{name}: {d}");
        assert!(d.within(), "{name}: {d}");
    }
}
