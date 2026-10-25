//! The 800 x 600 mode (`atlantis2_engine::display`): the blit of the
//! 640 x 480 surface against the original's routine run under emulation
//! (`golden/blit800.txt`), then, with the game, the modes of the loop
//! types: the panorama drawn at 800 x 600, the menus, the videos and the
//! dialogs on the 640 x 480 surface enlarged, the switch from the options
//! page.
//!
//! The game parts need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::anim::MsvcRand;
use atlantis2_engine::display::{blit_640_to_800, NATIVE_DIALOGS};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::{MenuHost, PageKind};
use atlantis2_engine::{Camera, Engine, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

fn md5_hex(px: &[u16]) -> String {
    let mut h = Md5::new();
    for p in px {
        h.update(p.to_le_bytes());
    }
    format!("{:x}", h.finalize())
}

/// The golden's inputs (`blit800.txt`'s header).
fn input(name: &str) -> Vec<u16> {
    let (w, h) = (640usize, 480usize);
    match name {
        "random" => {
            let mut x: u32 = 0x2545_f491;
            (0..w * h)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u16
                })
                .collect()
        }
        "ramp" => (0..h)
            .flat_map(|y| (0..w).map(move |x| ((x * 37 + y * 101) & 0xffff) as u16))
            .collect(),
        "white" => vec![0xffff; w * h],
        "bars" => {
            let rows = [0xf800u16, 0x07e0, 0x001f];
            (0..h)
                .flat_map(|y| {
                    (0..w).map(move |x| {
                        if x % 4 < 2 {
                            if x % 2 == 0 {
                                0
                            } else {
                                rows[(y / 3) % 3]
                            }
                        } else {
                            0xffff
                        }
                    })
                })
                .collect()
        }
        _ => panic!("{name}"),
    }
}

#[test]
fn the_blit_is_the_originals() {
    let text = include_str!("golden/blit800.txt");
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let (head, rows) = line.split_once(" | ").unwrap();
        let f: Vec<&str> = head.split_whitespace().collect();
        // The port's screen is RGB565: FUN_00401202.
        if f[1] != "565" {
            continue;
        }
        let src = input(f[0]);
        let mut dst = vec![0u16; 800 * 600];
        blit_640_to_800(&src, &mut dst, 800);
        for (y, row) in rows.split(" / ").enumerate() {
            let want: Vec<u16> = row
                .split_whitespace()
                .map(|p| u16::from_str_radix(p, 16).unwrap())
                .collect();
            assert_eq!(&dst[y * 800..y * 800 + 10], &want[..], "{} row {y}", f[0]);
        }
        assert_eq!(md5_hex(&dst), f[2], "{}", f[0]);
        n += 1;
    }
    assert_eq!(n, 4);
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

/// An empty scratch folder holding `atlant2.cfg` for a `width` x
/// `height` screen.
fn scratch(name: &str, width: i32, height: i32) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("a2-display-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let cfg = atlantis2_data::save::Cfg {
        width,
        height,
        ..Default::default()
    };
    std::fs::write(d.join("atlant2.cfg"), cfg.to_bytes()).unwrap();
    d
}

/// A new game at 59.94 passes a second, its film ended, the screen kept.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    fn new(files: A2Files, data: PathBuf) -> Run {
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.anim.rand = MsvcRand(1000);
        e.set_data_dir(Some(data));
        let (w, h) = e.screen_size();
        Run {
            e,
            pass: 0,
            screen: vec![0; w * h],
        }
    }

    fn step(&mut self) {
        self.e.set_clock(self.pass as f64 * PASS);
        let (w, h) = self.e.screen_size();
        self.screen.resize(w * h, 0);
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
    }

    fn wait(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    fn new_game(&mut self) {
        self.e.start_new_game().unwrap();
        // With a data folder of its own the first new game copies chapter
        // 0's music under the please-wait page (`engine/media.rs`).
        while self.e.copying() {
            self.step();
        }
        self.e.end_video();
        self.wait(30);
        assert_eq!(self.e.loop_type, LoopType::Warp);
    }
}

#[test]
fn the_panorama_at_800x600() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, scratch("panorama", 800, 600));
    assert_eq!(r.e.screen_size(), (800, 600));
    r.new_game();
    // Native: the cube at 800 x 600, 76° across, centre (400, 300).
    assert_eq!((r.e.width(), r.e.height()), (800, 600));
    let want = Camera::new(r.e.camera.fpu, 800, 600);
    assert_eq!((r.e.camera.cx, r.e.camera.cy), (400.0, 300.0));
    assert_eq!(r.e.camera.fx, want.fx);
    assert_eq!(r.screen.len(), 800 * 600);
    let png = atlantis2_engine::png::encode_rgb565(&r.screen, 800, 600);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a2-display-800.png");
    std::fs::write(&path, png).unwrap();
    eprintln!("{}", path.display());
    // The screen is the cube's to its edges (the snow below the
    // horizon): no 640 x 480 picture with a black border.
    let below = |s: &[u16], w: usize, y0: usize| s[y0 * w..].iter().filter(|&&p| p == 0).count();
    assert_eq!(below(&r.screen, 800, 480), 0);
    assert!(r.screen[480 * 800..]
        .iter()
        .any(|&p| p != r.screen[480 * 800]));
    // The same view at 640 x 480 sees the same centre direction.
    let (yaw, pitch) = r.e.angles_deg();
    let files = game().unwrap();
    let mut s = Run::new(files, scratch("panorama-640", 640, 480));
    s.new_game();
    let (yaw2, pitch2) = s.e.angles_deg();
    assert_eq!((yaw, pitch), (yaw2, pitch2));
    assert_eq!(s.screen.len(), 640 * 480);
    assert_eq!(
        s.e.centre_zone().map(|z| z.0),
        r.e.centre_zone().map(|z| z.0)
    );
}

#[test]
fn the_menus_are_the_640_pages_enlarged() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, scratch("menus", 800, 600));
    r.new_game();
    r.e.escape();
    r.wait(2);
    assert_eq!(r.e.loop_type, LoopType::Menu);
    // Not native: the page is drawn at 640 x 480 and blitted.
    assert_eq!((r.e.width(), r.e.height()), (640, 480));
    assert_eq!((r.e.camera.cx, r.e.camera.cy), (320.0, 240.0));
    // The same steps at 640 x 480: the page there, enlarged, is the
    // screen at 800 x 600.
    let mut s = Run::new(game().unwrap(), scratch("menus-640", 640, 480));
    s.new_game();
    s.e.escape();
    s.wait(2);
    let mut want = vec![0u16; 800 * 600];
    blit_640_to_800(&s.screen, &mut want, 800);
    assert!(r.screen == want, "the screen is the page enlarged");
    // Escape again: back to the panorama at 800 x 600.
    r.e.escape();
    r.wait(2);
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert_eq!((r.e.width(), r.e.height()), (800, 600));
    assert_eq!((r.e.camera.cx, r.e.camera.cy), (400.0, 300.0));
}

#[test]
fn the_options_page_switches_the_screen() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, scratch("switch", 640, 480));
    r.new_game();
    let at640 = r.screen.clone();
    assert_eq!(r.e.screen_size(), (640, 480));
    r.e.open_menu_page(PageKind::Options);
    r.wait(2);
    let mut s = r.e.settings();
    (s.width, s.height) = (800, 600);
    r.e.set_settings(s);
    r.wait(2);
    assert_eq!(r.e.screen_size(), (800, 600));
    assert_eq!(r.screen.len(), 800 * 600);
    assert_eq!(r.e.cfg().width, 800);
    r.e.escape();
    r.wait(3);
    if r.e.loop_type == LoopType::Menu {
        r.e.escape();
        r.wait(3);
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert_eq!((r.e.width(), r.e.height()), (800, 600));
    // And back: the 640 x 480 picture as before.
    let mut s = r.e.settings();
    (s.width, s.height) = (640, 480);
    r.e.set_settings(s);
    r.wait(2);
    assert_eq!(r.e.screen_size(), (640, 480));
    assert_eq!((r.e.camera.cx, r.e.camera.cy), (320.0, 240.0));
    assert_eq!(r.screen.len(), at640.len());
}

#[test]
fn the_dialogs_by_number() {
    let Some(files) = game() else { return };
    let mut r = Run::new(files, scratch("dialogs", 800, 600));
    r.new_game();
    // A Tibet dialog with heads: not native, 640 x 480 enlarged.
    r.e.start_dialog(361).unwrap();
    r.wait(4);
    assert_eq!(r.e.loop_type, LoopType::Dialog);
    assert_eq!((r.e.width(), r.e.height()), (640, 480));
    r.e.end_dialog();
    r.wait(2);
    assert_eq!((r.e.width(), r.e.height()), (800, 600));
    assert!(NATIVE_DIALOGS.contains(&44));
}
