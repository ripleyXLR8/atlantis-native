//! The options' screen size / colour depth switch against the original
//! under emulation (`tests/golden/display_switch.txt`): from 800 x 600, the
//! options page's `BT_RESOLUTION` (and `BT_32BITS`) then `BT_BACK`, whose
//! chain re-creates the display at once (0x0040fa60), the page surface at
//! the new size and format (black on the frame of the click), every page's
//! banks from `menu\spr_<W>\` of the new width, the warp camera and the
//! fonts; then page 1 drawn at the new size.
//!
//! Two tests: the menus alone (every frame's page and page surface md5 and
//! sprite calls, the re-creation's size and depth), and the game (the
//! screen, `Atlantis3.cfg`'s size and depth, the warp camera, the fonts,
//! the page 1 frames at the new size, the panorama drawn at the new size
//! after `BT_BACK` of page 1).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_engine::px::Faces;
use std::sync::Arc;

use atlantis3_data::cfg::{Config, PixelFormat};
use atlantis3_data::spr::Depth;
use atlantis3_data::A3Files;
use atlantis3_engine::menu::{Env, Event, Host, Menus};
use atlantis3_engine::text::{FontId, Texts};
use atlantis3_engine::{Game, Mode};
use atlantis_data::files::{FileSource, StdDir};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/display_switch.txt");

fn hex(b: &[u8]) -> String {
    format!("{:x}", Md5::digest(b))
}

fn md5_page(m: &Menus) -> String {
    match m.depth {
        Depth::Rgb565 => hex(&m
            .surface
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect::<Vec<u8>>()),
        Depth::Xrgb8888 => hex(&m
            .surface32
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect::<Vec<u8>>()),
    }
}

fn hexfloat(s: &str) -> f64 {
    let (m, e) = s.trim_start_matches("0x").split_once('p').unwrap();
    let (int, frac) = m.split_once('.').unwrap();
    let mut v = f64::from(u32::from_str_radix(int, 16).unwrap());
    for (i, c) in frac.chars().enumerate() {
        v += f64::from(c.to_digit(16).unwrap()) / 16f64.powi(i as i32 + 1);
    }
    v * 2f64.powi(e.parse().unwrap())
}

struct NoSaves;

impl Host for NoSaves {
    fn exists(&mut self, _: &str) -> bool {
        false
    }
    fn read(&mut self, _: &str) -> Option<Vec<u8>> {
        None
    }
    fn thumbnail(&mut self, _: usize, _: &str) -> Option<Vec<u8>> {
        None
    }
}

/// The draws as the golden writes them: bank#frame@pos (`def` for the
/// bank's default position; a fixed position's two numbers are split by
/// the golden's own comma).
fn port_draws(m: &Menus) -> String {
    m.draws
        .iter()
        .map(|d| {
            format!(
                "{}#{}@{}",
                d.bank,
                d.frame,
                d.pos.map_or("def".into(), |(x, y)| format!("{x},{y}"))
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The golden's draws with the pointer's call (a hex bank) taken out.
fn without_pointer(draws: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in draws.split(',') {
        if part.contains('#') {
            out.push(part.to_string());
        } else if let Some(last) = out.last_mut() {
            // The y of a fixed position.
            last.push(',');
            last.push_str(part);
        }
    }
    out.retain(|d| !d.starts_with("0x"));
    out.join(",")
}

/// A frame line: (label, t, x, y, released, held, page before, page after,
/// md5, draws).
struct Frame<'a> {
    label: String,
    t: f64,
    x: i32,
    y: i32,
    released: bool,
    held: bool,
    p0: u8,
    p1: u8,
    md5: &'a str,
    draws: String,
}

fn frame(l: &str) -> Frame<'_> {
    let f: Vec<&str> = l.split(' ').collect();
    let (p0, p1) = f[8].trim_start_matches('p').split_once("->").unwrap();
    Frame {
        label: format!("{} {}", f[1], f[2]),
        t: hexfloat(f[3]),
        x: f[4].parse().unwrap(),
        y: f[5].parse().unwrap(),
        released: f[6] == "c1",
        held: f[7] == "h1",
        p0: p0.parse().unwrap(),
        p1: p1.parse().unwrap(),
        md5: f[9].trim_start_matches("md5="),
        draws: without_pointer(f[10].trim_start_matches("draws=")),
    }
}

/// The golden's runs: (name, lines).
fn runs() -> Vec<(String, Vec<&'static str>)> {
    let mut out: Vec<(String, Vec<&str>)> = Vec::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(r) = l.strip_prefix("run ") {
            out.push((r.split(' ').next().unwrap().to_string(), Vec::new()));
        } else {
            out.last_mut().unwrap().1.push(l);
        }
    }
    out
}

fn field<'a>(l: &'a str, key: &str) -> &'a str {
    l.split(' ')
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("{l}: no {key}"))
}

fn size(s: &str) -> (usize, usize) {
    let (w, h) = s.split_once('x').unwrap();
    (w.parse().unwrap(), h.parse().unwrap())
}

#[test]
fn the_menus_switch_size_and_depth_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let texts = Texts::load(&files, "fra");
    let mut switched = 0;
    for (name, lines) in runs() {
        // The harness leaves `Version` empty.
        let mut config = Config {
            resolution: (800, 600),
            langue: b"fra".to_vec(),
            sous_titre: b"fra".to_vec(),
            version: Vec::new(),
            ..Config::default()
        };
        let mut menus = Menus::default();
        menus.load(&files, 800, 600, &config);
        let mut screen = atlantis3_engine::px::Surface::Rgb565(vec![0u16; 800 * 600]);
        let mut frames = 0;
        for l in &lines {
            if l.starts_with("set ") {
                menus.open(1);
            } else if l.starts_with("frame ") {
                let fr = frame(l);
                assert_eq!(menus.page, fr.p0, "{name} {}", fr.label);
                menus.harness_input(fr.released, fr.held);
                let mut host = NoSaves;
                let mut env = Env {
                    files: &files,
                    config: &mut config,
                    texts: Some(&texts),
                    host: &mut host,
                    player: 1,
                    now: fr.t,
                    pointer: (fr.x, fr.y),
                };
                let ev = menus.frame(&mut env, &mut screen);
                if let Some(&Event::DisplayChange(w, h)) =
                    ev.iter().find(|e| matches!(e, Event::DisplayChange(..)))
                {
                    // The game re-creates its screen (the game's test).
                    screen =
                        atlantis3_engine::px::Surface::Rgb565(vec![0; w as usize * h as usize]);
                    switched += 1;
                }
                assert_eq!(menus.page, fr.p1, "{name} {}", fr.label);
                assert_eq!(
                    md5_page(&menus),
                    fr.md5,
                    "{name} {}: the page surface",
                    fr.label
                );
                assert_eq!(port_draws(&menus), fr.draws, "{name} {}", fr.label);
                frames += 1;
            } else if let Some(e) = l
                .strip_prefix("event ")
                .and_then(|e| e.split_once("display_mode_0x0040fa60 "))
            {
                let v: Vec<usize> = e.1.split(' ').map(|x| x.parse().unwrap()).collect();
                assert_eq!((menus.width, menus.height), (v[0], v[1]), "{l}");
                let bits = match menus.depth {
                    Depth::Rgb565 => 16,
                    Depth::Xrgb8888 => 32,
                };
                assert_eq!(bits, v[2], "{l}");
            } else if l.starts_with("after ") {
                let (pw, ph) = size(field(l, "page_surface="));
                assert_eq!((menus.width, menus.height), (pw, ph), "{l}");
                assert_eq!(menus.surface.len(), pw * ph, "{l}");
            }
        }
        assert!(frames > 30, "{name}: {frames}");
    }
    assert_eq!(switched, 2);
}

/// The install with its `Atlantis3.cfg` followed by a `Resolution` line
/// (the last one read wins).
struct Resized {
    dir: StdDir,
    cfg: Vec<u8>,
}

impl Resized {
    fn is_cfg(name: &str) -> bool {
        name.replace('\\', "/")
            .eq_ignore_ascii_case("Datas/Atlantis3.cfg")
    }
}

impl FileSource for Resized {
    fn size(&self, name: &str) -> Option<u64> {
        if Resized::is_cfg(name) {
            return Some(self.cfg.len() as u64);
        }
        self.dir.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if Resized::is_cfg(name) {
            let start = usize::try_from(offset).ok()?;
            return self
                .cfg
                .get(start..start.checked_add(len)?)
                .map(<[u8]>::to_vec);
        }
        self.dir.read_at(name, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.dir.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.dir.list(dir)
    }
}

#[test]
fn the_game_switches_size_and_depth_at_once_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    for (name, lines) in runs() {
        let std_dir = StdDir::new(&dir);
        let mut cfg = std_dir.read("Datas/Atlantis3.cfg").unwrap_or_default();
        cfg.extend_from_slice(b"\r\nResolution\t\t= 800 600\r\nPixelFormat\t\t= 16bits\r\n");
        let files = A3Files::new(Arc::new(Resized { dir: std_dir, cfg }));
        let mut g = Game::new(files).expect("the game starts");
        assert_eq!((g.width(), g.height()), (800, 600));
        assert!(g.show_save_page());
        let after = lines.iter().find(|l| l.starts_with("after ")).unwrap();
        let (sw, sh) = size(field(after, "screen="));
        let bits: u32 = field(after, "bits=").parse().unwrap();
        let mut at_new_size = 0;
        for l in lines.iter().filter(|l| l.starts_with("frame ")) {
            let fr = frame(l);
            g.set_pointer(fr.x, fr.y);
            g.menus_mut().harness_input(fr.released, fr.held);
            g.step(fr.t);
            assert_eq!(
                g.menu_page().map(|p| p.0),
                Some(fr.p1),
                "{name} {}",
                fr.label
            );
            if fr.label.starts_with("option_click_BT_BACK 1") {
                // The frame of the click: the display re-created, the new
                // page surface black on it (the pointer and the texts
                // aside).
                assert_eq!((g.width(), g.height()), (sw, sh), "{name}");
                assert_eq!(g.surface().len(), sw * sh);
                assert_eq!(
                    g.surface().depth(),
                    if bits > 16 {
                        Depth::Xrgb8888
                    } else {
                        Depth::Rgb565
                    },
                    "{name}: the display's format"
                );
                let lit = g
                    .surface()
                    .to_rgb()
                    .iter()
                    .filter(|p| **p != [0; 3])
                    .count();
                assert!(lit < 2000, "{name}: {lit} lit pixels on the black frame");
                let c = &g.startup.config;
                assert_eq!(
                    c.resolution,
                    (sw as i32, sh as i32),
                    "{name}: the cfg's size"
                );
                assert_eq!(c.pixel_format.bits(), bits, "{name}: the cfg's depth");
                let px = if bits > 16 {
                    PixelFormat::X8R8G8B8
                } else {
                    PixelFormat::R5G6B5
                };
                assert_eq!(c.pixel_format, px);
                // The warp camera (fx, fy, cx, cy as the golden's floats).
                let cam: Vec<u32> = field(after, "camera=")
                    .split(',')
                    .map(|x| u32::from_str_radix(x, 16).unwrap())
                    .collect();
                let k = g.camera();
                assert_eq!(
                    [
                        k.fx.to_bits(),
                        k.fy.to_bits(),
                        k.cx.to_bits(),
                        k.cy.to_bits()
                    ],
                    [cam[0], cam[1], cam[2], cam[3]],
                    "{name}: the warp camera"
                );
            }
            if g.width() == sw && fr.p0 == 1 && fr.p1 == 1 {
                // Page 1 at the new size: the page surface as the
                // original's (16 bits; the 32-bit one is compared by the
                // menus' test).
                if bits == 16 {
                    assert_eq!(md5_page(g.menus()), fr.md5, "{name} {}", fr.label);
                }
                at_new_size += 1;
            }
        }
        assert!(at_new_size >= 6, "{name}: {at_new_size}");
        // The fonts made again for the new size.
        let want: Vec<(&str, i32)> = after
            .split("fonts=")
            .nth(1)
            .unwrap()
            .split(' ')
            .filter_map(|x| x.split_once(':'))
            .map(|(n, h)| (n, h.parse().unwrap()))
            .collect();
        assert_eq!(want.len(), 5, "{after}");
        let texts = Texts::load(&A3Files::open_dir(&dir).unwrap().0, "fra");
        for (n, h) in want {
            let id = [
                FontId::Default,
                FontId::Bulle,
                FontId::Special,
                FontId::Titre,
                FontId::Credit,
            ]
            .into_iter()
            .find(|f| f.name() == n)
            .unwrap();
            let m = texts.gdi(id, g.width()).unwrap();
            assert_eq!(m.height, h, "{name}: {n} at {}", g.width());
        }
        // The six faces re-allocated by the warp init: zero pages from the
        // heap (black), in the new format, while the menu shows.
        let faces = g.place_faces();
        assert_eq!(faces.depth(), g.depth(), "{name}");
        let black = match faces {
            Faces::Rgb565(f) => f
                .iter()
                .all(|x| x.len() == 512 * 512 && x.iter().all(|&p| p == 0)),
            Faces::Xrgb8888(f) => f
                .iter()
                .all(|x| x.len() == 512 * 512 && x.iter().all(|&p| p == 0)),
        };
        assert!(black, "{name}: the faces black after the switch");
        // Back to the game by page 1's `BT_BACK`: the view entered again
        // (@ 0x0043fd00, `tests/menu_back.rs`): the cube read again into
        // the faces, in the new format, so the panorama is drawn at once.
        g.menu_back();
        assert_eq!(g.mode(), Mode::Warp);
        assert_eq!(g.place_faces().depth(), g.depth(), "{name}");
        let refilled = match g.place_faces() {
            Faces::Rgb565(f) => f.iter().any(|x| x.iter().any(|&p| p != 0)),
            Faces::Xrgb8888(f) => f.iter().any(|x| x.iter().any(|&p| p != 0)),
        };
        assert!(refilled, "{name}: the faces read again");
        let t = 200.0;
        g.step(t);
        g.step(t + 0.1);
        assert_eq!(g.surface().len(), sw * sh);
        assert!(
            g.surface()
                .to_rgb()
                .iter()
                .filter(|p| **p != [0; 3])
                .count()
                > 100_000,
            "{name}: the panorama drawn after the menu"
        );
    }
}
