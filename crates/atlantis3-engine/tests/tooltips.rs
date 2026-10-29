//! The menu pages' tooltips against the original under emulation
//! (`tests/golden/menu_tooltips.txt`): pages 0, 1, 2, 4, 5, 6, 8 at 800,
//! 640 and 1024 pixels wide, one session per size; per frame the page
//! surface (md5), the tooltip box and its flag, the box's fill and lines,
//! and the text calls (layouts, then each line's position, font, advances
//! and bytes by md5). The tooltip texts are checked by md5.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::cfg::Config;
use atlantis3_data::A3Files;
use atlantis3_engine::menu::{Env, Host, Menus, Shape, TextCall};
use atlantis3_engine::text::Texts;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/menu_tooltips.txt");

fn hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn md5_16(px: &[u16]) -> String {
    hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
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

/// The calls of a frame as the golden writes them.
pub fn shape_line(s: &Shape) -> String {
    match *s {
        Shape::Fill { rect, colour } => format!(
            "FILL rect={},{},{},{} colour={colour:08x}",
            rect[0], rect[1], rect[2], rect[3]
        ),
        Shape::Line { p0, p1, colour } => format!(
            "LINE p0={},{} p1={},{} colour={colour:08x}",
            p0.0, p0.1, p1.0, p1.1
        ),
    }
}

/// A text call as `LAYOUT mL mR mT mB h v` or `DRAW x y colour font n dx
/// md5`.
pub fn text_line(t: &TextCall) -> String {
    match t {
        TextCall::Layout(z) => format!(
            "LAYOUT {} {} {} {} {} {}",
            z.ml, z.mr, z.mt, z.mb, z.halign, z.valign
        ),
        TextCall::Draw { line, font, rgb } => format!(
            "DRAW {} {} {rgb:06x} {} {} {} {}",
            line.x,
            line.y,
            font.name(),
            line.bytes.len(),
            line.dx
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(","),
            hex(&line.bytes)
        ),
    }
}

/// A golden text line in the same form.
pub fn golden_text(l: &str) -> String {
    let f: Vec<&str> = l.split_whitespace().collect();
    if f[0] == "LAYOUT" {
        return f.join(" ");
    }
    let kv = |k: &str| {
        f.iter()
            .find_map(|t| t.strip_prefix(k).and_then(|t| t.strip_prefix('=')))
            .unwrap_or("")
    };
    format!(
        "DRAW {} {} {} {} {} {} {}",
        f[1],
        f[2],
        kv("colour"),
        kv("font"),
        kv("n"),
        kv("dx"),
        kv("md5")
    )
}

struct Session<'a> {
    files: &'a A3Files,
    texts: &'a Texts,
    config: Config,
    menus: Menus,
    screen: Vec<u16>,
    t: f64,
}

impl Session<'_> {
    fn frame(&mut self, x: i32, y: i32) {
        self.t += 1.0 / 32.0;
        let mut host = NoSaves;
        let mut env = Env {
            files: self.files,
            config: &mut self.config,
            texts: Some(self.texts),
            host: &mut host,
            player: 1,
            now: self.t,
            pointer: (x, y),
        };
        self.menus.frame(&mut env, &mut self.screen);
    }
}

#[test]
fn tooltips_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let texts = Texts::load(&files, "fra");
    let mut s: Option<Session> = None;
    let mut expect: Vec<String> = Vec::new();
    let mut got: Vec<String> = Vec::new();
    let mut label = String::new();
    let (mut frames, mut shown, mut calls) = (0, 0, 0);
    let flush =
        |label: &str, expect: &mut Vec<String>, got: &mut Vec<String>, calls: &mut usize| {
            assert_eq!(got, expect, "{label}: shapes and texts");
            *calls += expect.len();
            expect.clear();
            got.clear();
        };
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(rest) = l.strip_prefix("  ") {
            let f0 = rest.split(' ').next().unwrap();
            expect.push(if f0 == "FILL" || f0 == "LINE" {
                rest.to_string()
            } else {
                golden_text(rest)
            });
            continue;
        }
        flush(&label, &mut expect, &mut got, &mut calls);
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "screen" => {
                let (w, h): (usize, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let mut config = Config::default();
                config.resolution = (w as i32, h as i32);
                config.langue = b"fra".to_vec();
                config.sous_titre = b"fra".to_vec();
                // The harness leaves `Version` empty.
                config.version = Vec::new();
                let mut menus = Menus::default();
                menus.load(&files, w, h, &config);
                s = Some(Session {
                    files: &files,
                    texts: &texts,
                    config,
                    menus,
                    screen: vec![0; w * h],
                    t: 100.0,
                });
            }
            "page" => {
                let s = s.as_mut().unwrap();
                let p: u8 = f[1].parse().unwrap();
                s.menus.open(p);
                for _ in 0..60 {
                    s.frame(5, 5);
                    if p == 0 || s.menus.sub == 1 {
                        break;
                    }
                }
                s.frame(5, 5);
                s.frame(5, 5);
                assert_eq!(format!("sub={}", s.menus.sub), f[4], "{l}");
                let page = s.menus.get(p).unwrap();
                let bulles: Vec<String> = page
                    .items
                    .iter()
                    .filter(|it| it.el.bulle)
                    .map(|it| {
                        format!(
                            "{}:{}",
                            String::from_utf8_lossy(&it.el.nomref),
                            hex(it.el.tooltip.as_deref().unwrap_or_default())
                        )
                    })
                    .collect();
                assert_eq!(format!("bulles={}", bulles.join(",")), f[5], "{l}");
            }
            "frame" => {
                let s = s.as_mut().unwrap();
                let (x, y): (i32, i32) = (f[5].parse().unwrap(), f[6].parse().unwrap());
                s.frame(x, y);
                label = l.to_string();
                assert_eq!(format!("md5={}", md5_16(&s.menus.surface)), f[7], "{l}");
                let b = s.menus.tip;
                assert_eq!(
                    format!(
                        "box={},{},{},{} flag={}",
                        b.x0,
                        b.y0,
                        b.x1,
                        b.y1,
                        u8::from(b.shown)
                    ),
                    format!("{} {}", f[8], f[9]),
                    "{l}"
                );
                got.extend(s.menus.shapes.iter().map(shape_line));
                got.extend(s.menus.texts.iter().map(text_line));
                frames += 1;
                shown += usize::from(b.shown);
            }
            _ => {}
        }
    }
    flush(&label, &mut expect, &mut got, &mut calls);
    assert_eq!(frames, 5301);
    assert!(shown > 0 && calls > 0);
}
