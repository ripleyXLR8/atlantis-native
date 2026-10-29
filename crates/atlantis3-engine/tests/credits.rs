//! The credits page (menu page 9, `i_credit`) against the original's code
//! under emulation (`tests/golden/credits.txt`): the element table, the
//! state after the start (atlantis3-gog-fr-3473408 @ 0x00457690), then
//! 230 frames half a second apart (31 text pages, the wrap from `PAGE30`
//! to `PAGE0`, a picture every second page, drawn one frame late) and 12
//! clicks (a page each), each frame's picture (the screen without its
//! texts: md5), sprites, shown elements and slide, and its text calls
//! (font, the page text's md5 and length, colour, the layout's margins
//! and alignments); then Escape: the configuration written, the quit.
//!
//! The same at 640 × 480 and 1024 × 768 (`credits_640.txt`,
//! `credits_1024.txt`: the click at the screen's centre, the text box's
//! margins (trunc(0.4 W), 0, 0, H − trunc(0.8 H))), the install's
//! `Atlantis3.cfg` given that `Resolution`.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};
use atlantis_data::files::{FileSource, StdDir};
use md5::{Digest, Md5};

const GOLDEN_640: &str = include_str!("golden/credits_640.txt");
const GOLDEN_800: &str = include_str!("golden/credits.txt");
const GOLDEN_1024: &str = include_str!("golden/credits_1024.txt");

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

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn md5_px(px: &[u16]) -> String {
    let b: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    md5(&b)
}

/// The clock: a double's little-endian bytes in hex.
fn clock(hex: &str) -> f64 {
    let mut b = [0u8; 8];
    for (i, v) in b.iter_mut().enumerate() {
        *v = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    f64::from_le_bytes(b)
}

/// The text calls of the frame just drawn, as the golden's `event` lines
/// write them: the measure (@ 0x0041e390, white) then the print
/// (@ 0x0041e2c0, the element's colour).
fn events(g: &Game) -> Vec<String> {
    let mut out = Vec::new();
    for p in &g.credits().unwrap().prints {
        let z = p.zone;
        let tail = |colour: &str| {
            format!(
                "font={} text=md5:{}/{} colour={colour} margins={},{},{},{} align={},{}",
                String::from_utf8_lossy(&p.font),
                md5(&p.text),
                p.text.len(),
                z.ml,
                z.mr,
                z.mt,
                z.mb,
                z.halign,
                z.valign
            )
        };
        let rgb = format!("{:02x}{:02x}{:02x}", p.rgb[0], p.rgb[1], p.rgb[2]);
        out.push(format!(
            "AtlFont_PrintChecked_0x0041e390 {}",
            tail("ffffff")
        ));
        out.push(format!("AtlFont_Print_0x0041e2c0 {}", tail(&rgb)));
    }
    out
}

/// The golden of the `w` × `h` screen replayed.
fn replay(golden: &str, w: usize, h: usize) {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let std_dir = StdDir::new(&dir);
    let mut cfg = std_dir.read("Datas/Atlantis3.cfg").unwrap_or_default();
    cfg.extend_from_slice(format!("\r\nResolution\t\t= {w} {h}\r\n").as_bytes());
    let files = A3Files::new(Arc::new(Resized { dir: std_dir, cfg }));
    let mut g = Game::new(files).expect("the game starts");
    assert_eq!((g.width(), g.height()), (w, h), "the golden's screen");
    g.set_pointer(w as i32 / 2, h as i32 / 2);
    // The start at the golden's clock.
    g.step(500.0);
    g.start_credits();
    let lines: Vec<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();
    let (mut elements, mut frames, mut prints, mut clicks) = (0, 0, 0, 0);
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            // `element K TYPE NOMREF NOMSPR x=X y=Y`
            "element" => {
                let k: usize = f[1].parse().unwrap();
                let it = &g.credits().unwrap().items[k];
                let s = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
                assert_eq!(s(&it.el.type_word), f[2], "{l}");
                assert_eq!(s(&it.el.nomref), f[3], "{l}");
                assert_eq!(
                    it.el.nomspr.as_deref().map(s).unwrap_or_default(),
                    f[4],
                    "{l}"
                );
                assert_eq!(format!("x={}", it.el.pos[0]), f[5], "{l}");
                assert_eq!(format!("y={}", it.el.pos[1]), f[6], "{l}");
                elements += 1;
            }
            // `start page=9 mode=6 slide=1 shown=...`
            "start" => {
                assert_eq!(g.mode(), Mode::Menu, "{l}");
                let c = g.credits().unwrap();
                assert_eq!(format!("slide={}", c.slide), f[3], "{l}");
                assert_eq!(format!("shown={}", c.shown_names().join(",")), f[4], "{l}");
            }
            // `frame CASE N T CLICK md5=... draws=... shown=... slide=...`
            "frame" => {
                let t = clock(f[3]);
                if f[4] == "1" {
                    g.click();
                    clicks += 1;
                }
                g.step(t);
                let c = g.credits().unwrap();
                assert_eq!(format!("md5={}", md5_px(&c.surface)), f[5], "{l}");
                assert_eq!(format!("draws={}", c.draws.join(",")), f[6], "{l}");
                assert_eq!(format!("shown={}", c.shown_names().join(",")), f[7], "{l}");
                assert_eq!(format!("slide={}", c.slide), f[8], "{l}");
                // Its `event` lines.
                let mut want = Vec::new();
                while i + 1 < lines.len() && lines[i + 1].starts_with("event ") {
                    i += 1;
                    let e: Vec<&str> = lines[i].splitn(4, ' ').collect();
                    if e[1] != "escape" {
                        want.push(e[3].to_string());
                    }
                }
                let got = events(&g);
                assert_eq!(got, want, "{l}: text calls");
                prints += got.len();
                frames += 1;
            }
            // Escape (@ 0x0042ac50 → @ 0x00457360): the configuration
            // written (@ 0x00408f80), the program's exit (@ 0x004dd9e0).
            "escape" => {
                assert!(g.credits_escape(), "{l}");
                assert!(g.quit, "{l}");
                assert!(g.credits().unwrap().config.is_some(), "{l}");
                let mut want = Vec::new();
                while i + 1 < lines.len() && lines[i + 1].starts_with("event escape ") {
                    i += 1;
                    want.push(&lines[i]["event escape ".len()..]);
                }
                assert_eq!(
                    want,
                    ["AtlConfig_MainWrite_0x00408f80", "App_Quit_0x004dd9e0 0"],
                    "{l}"
                );
            }
            _ => panic!("unknown line {l}"),
        }
        i += 1;
    }
    assert_eq!((elements, frames, prints, clicks), (41, 254, 508, 12));
}

#[test]
fn the_credits_as_the_original_at_640() {
    replay(GOLDEN_640, 640, 480);
}

#[test]
fn the_credits_as_the_original_at_800() {
    replay(GOLDEN_800, 800, 600);
}

#[test]
fn the_credits_as_the_original_at_1024() {
    replay(GOLDEN_1024, 1024, 768);
}
