//! The in-game menu's pages against the original under emulation
//! (`tests/golden/menu_pages.txt`): a walk through pages 1 → 4 → 1 → 5 → 6
//! → 5 → 1 → 8 → 1 → 2 → back to the game → 1 → the start page, at 800,
//! 640 and 1024 pixels wide, one session per size, with eight saves of
//! player 1 (their thumbnails made by the encoder from the harness's
//! pictures). Checked: each page's element table, its hover and click maps
//! (10-pixel grid), and per frame the page surface (md5), the sprites drawn,
//! the page and sub-state, the load list's scroll and count, the volumes
//! and options, the tooltip box, the elements' frame / hover / display
//! state, the calls the original's harness logs (`(PlaySound = 500)`, the
//! `Atlantis3.cfg` writes, the save files' existence tests, the loader, the
//! writer, back to the game, the display re-creation, the start page's
//! mode) and the text calls (tooltips, the load page's dates). Every
//! frame's page surface is compared, also after the options' `BT_BACK`
//! re-created it in 32 bits (drawn by the x8r8g8b8 blitters, truncated to
//! RGB565 by the copy to the screen).
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::collections::HashMap;

use atlantis3_data::cfg::Config;
use atlantis3_data::spr::Depth;
use atlantis3_data::spr_encode::{darken_black, ThumbEncoder};
use atlantis3_data::A3Files;
use atlantis3_engine::menu::{Env, Event, Host, Menus, TextCall};
use atlantis3_engine::text::Texts;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/menu_pages.txt");

fn hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn md5_16(px: &[u16]) -> String {
    hex(&px.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())
}

/// `a3_save.py thumb_image` (x8r8g8b8).
fn thumb_image(w: usize, h: usize, kind: &str) -> Vec<u32> {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = match kind {
                "grad" => {
                    ((x * 255 / (w - 1).max(1)) << 16) as u32
                        | ((y * 255 / (h - 1).max(1)) << 8) as u32
                        | (((x + y) * 3) & 0xff) as u32
                }
                "bars" => {
                    if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                        0
                    } else if (10..20).contains(&x) && (10..20).contains(&y) {
                        0x050307
                    } else {
                        [
                            0xFFFFFF, 0xFFFF00, 0x00FFFF, 0x00FF00, 0xFF00FF, 0xFF0000, 0x0000FF,
                            0x101010,
                        ][x * 8 / w]
                    }
                }
                _ => {
                    ((x as u64)
                        .wrapping_mul(1_103_515_245)
                        .wrapping_add((y as u64).wrapping_mul(12_345))
                        .wrapping_add(0x9E37_79B9))
                    .wrapping_mul(2_654_435_761) as u32
                        & 0xFF_FFFF
                }
            };
            px.push(c);
        }
    }
    px
}

/// The harness's saves (`a3_menus.make_saves`): `game1_<k>.sav` with the
/// writer's first lines, and its thumbnail for the screen width, made in
/// order by one encoder (its palette history carried from save to save).
/// The encoder reads past the pixels (see `spr_encode`): in the harness
/// the pixels (16-byte aligned) are followed by the alignment's zero bytes
/// and then by the encoder's own output buffer, already holding the flags
/// and the palette when the rows are scanned.
fn make_saves(width: usize, n: i32) -> HashMap<String, Vec<u8>> {
    let (tw, th) = match width {
        640 => (52, 40),
        800 => (65, 50),
        _ => (85, 65),
    };
    let mut enc = ThumbEncoder::new();
    let mut out = HashMap::new();
    for k in 1..=n {
        let name = format!("game1_{k}.sav");
        let text = format!(
            "CurrentCD = 1\nDateAndTime = 10/{k:02}/01 12:{k:02}:56\nWam = hoggar.wam\nMonde = 1\n\
             Vue = 5\nChapitre = 2\nNBFlag = 0\nNBObjet = 0\n"
        );
        out.insert(name.clone(), text.into_bytes());
        let mut px = thumb_image(tw, th, ["grad", "bars", "noise"][(k as usize - 1) % 3]);
        darken_black(&mut px);
        let first = enc.clone().encode(&px, tw, th, tw, &[]);
        let pad = (16 - tw * th * 4 % 16) % 16;
        let mut tail = vec![0u8; pad];
        tail.extend_from_slice(&first[..first.len().min(4 + 0x400)]);
        let mut bank = b"SPRCRYO1".to_vec();
        bank.extend(enc.encode(&px, tw, th, tw, &tail));
        out.insert(format!("thumb/{name}"), bank);
    }
    out
}

struct Saves(HashMap<String, Vec<u8>>);

impl Host for Saves {
    fn exists(&mut self, name: &str) -> bool {
        self.0.contains_key(name)
    }
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.0.get(name).cloned()
    }
    fn thumbnail(&mut self, _: usize, name: &str) -> Option<Vec<u8>> {
        self.0.get(&format!("thumb/{name}")).cloned()
    }
}

/// The harness's logged calls the port reproduces, as it writes them.
fn events(ev: &[Event]) -> Vec<String> {
    let mut out = Vec::new();
    for e in ev {
        match e {
            Event::Command(c) => {
                out.push(format!("RunOneCommand '{}'", String::from_utf8_lossy(c)))
            }
            Event::WriteCfg => out.push("cfg_write_0x00408f80".into()),
            Event::Exists(n, e) => out.push(format!("exists 'save\\\\{n}' -> {}", u8::from(*e))),
            Event::Load(n) => out.push(format!("loader_0x0045b750 'save\\\\{n}'")),
            Event::Save(n) => out.push(format!("save_writer_0x00459ee0 'save\\\\{n}'")),
            Event::Resume => out.push("back_to_game_0x00451280".into()),
            Event::DisplayChange(w, h) => out.push(format!("display_mode_0x0040fa60 {w} {h}")),
            Event::StartPage => out.push("set_mode_0x004d6b00 6".into()),
            _ => {}
        }
    }
    out
}

/// A golden event line in the same form, or none for the calls the port
/// does not log.
fn golden_event(e: &str) -> Option<String> {
    let k = e.split(' ').next().unwrap();
    match k {
        "RunOneCommand"
        | "exists"
        | "loader_0x0045b750"
        | "save_writer_0x00459ee0"
        | "back_to_game_0x00451280"
        | "display_mode_0x0040fa60"
        | "set_mode_0x004d6b00" => Some(e.to_string()),
        "cfg_write_0x00408f80" => Some(k.to_string()),
        _ => None,
    }
}

fn text_line(t: &TextCall) -> String {
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

fn golden_text(f: &[&str]) -> String {
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
    saves: Saves,
    screen: Vec<u16>,
}

impl Session<'_> {
    /// The golden's globals after a frame.
    fn globals(&self) -> String {
        let (c, m) = (&self.config, &self.menus);
        format!(
            "page={} sub={} mode=6 loadscroll={} nsaves={} vol_master={} vol_music={} vol_sfx={} \
             vol_voice={} subtitles={} smooth={} viseur={} invert_y={} warpres={} res_pending={} \
             depth_pending={} player=1",
            m.page,
            m.sub,
            m.scroll,
            m.count,
            c.global_volume,
            c.music_volume,
            c.sfx_volume,
            c.voices_volume,
            u8::from(c.subtitle_on),
            u8::from(c.smooth),
            u8::from(c.cursor),
            u8::from(c.invert_mouse_y),
            c.warp_res,
            m.res_pending,
            u8::from(m.depth_pending),
        )
    }

    fn state(&self, page: u8) -> String {
        self.menus
            .get(page)
            .unwrap()
            .items
            .iter()
            .map(|it| {
                format!(
                    "{}:f{}:h{}:d{}",
                    String::from_utf8_lossy(&it.el.nomref),
                    it.frame,
                    u8::from(it.hovered),
                    u8::from(it.el.displayed)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What a frame left to compare with the lines that follow it.
#[derive(Default)]
struct Pending {
    label: String,
    events: Vec<String>,
    texts: Vec<String>,
    want_events: Vec<String>,
    want_texts: Vec<String>,
}

impl Pending {
    fn check(&mut self, counts: &mut [usize; 2]) {
        assert_eq!(self.events, self.want_events, "{}: calls", self.label);
        assert_eq!(self.texts, self.want_texts, "{}: text calls", self.label);
        counts[0] += self.want_events.len();
        counts[1] += self.want_texts.len();
        *self = Pending::default();
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

#[test]
fn menu_pages_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let texts = Texts::load(&files, "fra");
    let mut s: Option<Session> = None;
    let mut pending = Pending::default();
    let mut page_of_frame = 0u8;
    let mut n = HashMap::<&str, usize>::new();
    let mut counts = [0usize; 2];
    let (mut recreations, mut md5s) = (0usize, 0usize);
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "event" | "text" | "state" if !pending.label.is_empty() => {
                let here = format!("{} {}", f[1], f[2]);
                assert_eq!(here, pending.label, "{l}: line of another frame");
                let s = s.as_ref().unwrap();
                match f[0] {
                    "event" => {
                        if let Some(e) = golden_event(&f[3..].join(" ")) {
                            pending.want_events.push(e);
                        }
                    }
                    "text" => pending.want_texts.push(golden_text(&f[3..])),
                    _ => assert_eq!(s.state(page_of_frame), f[3..].join(" "), "{l}"),
                }
                *n.entry(f[0]).or_default() += 1;
                continue;
            }
            _ => {}
        }
        if !pending.label.is_empty() {
            pending.check(&mut counts);
        }
        match f[0] {
            "screen" => {
                let (w, h): (usize, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                // The harness leaves `Version` empty.
                let config = Config {
                    resolution: (w as i32, h as i32),
                    langue: b"fra".to_vec(),
                    sous_titre: b"fra".to_vec(),
                    version: Vec::new(),
                    ..Config::default()
                };
                let mut menus = Menus::default();
                menus.load(&files, w, h, &config);
                let saves = make_saves(w, 8);
                s = Some(Session {
                    files: &files,
                    texts: &texts,
                    config,
                    menus,
                    saves: Saves(saves),
                    screen: vec![0; w * h],
                });
            }
            "set" => {
                let p: u8 = f[2].trim_start_matches("page=").parse().unwrap();
                s.as_mut().unwrap().menus.open(p);
            }
            "element" => {
                let s = s.as_ref().unwrap();
                let (p, k): (u8, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let it = &s.menus.get(p).unwrap().items[k];
                let flag = |b: bool, c: char| if b { c } else { '-' };
                let el = &it.el;
                let got = format!(
                    "{} {} {} x={} y={} frames={} flags={}{}{}{}{}{}{} fps={:08x} bulle={}",
                    String::from_utf8_lossy(&el.type_word),
                    String::from_utf8_lossy(&el.nomref),
                    el.nomspr
                        .as_ref()
                        .map_or("-".into(), |s| String::from_utf8_lossy(s).into_owned()),
                    el.pos[0],
                    el.pos[1],
                    it.bank.as_ref().map_or(0, |b| b.len()),
                    flag(el.displayed, 'd'),
                    flag(el.selectable, 's'),
                    flag(el.clickable, 'c'),
                    flag(el.bulle, 'b'),
                    flag(el.looping, 'l'),
                    flag(el.permanent, 'p'),
                    flag(el.text, 't'),
                    el.frame_time.to_bits(),
                    el.tooltip
                        .as_deref()
                        .filter(|t| !t.is_empty())
                        .map_or("-".into(), hex),
                );
                assert_eq!(got, f[3..12].join(" "), "{l}");
                *n.entry("element").or_default() += 1;
            }
            "hover" | "clickmap" => {
                let s = s.as_ref().unwrap();
                let (p, y): (u8, i32) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let page = s.menus.get(p).unwrap();
                for (i, want) in f[3..].iter().enumerate() {
                    let pt = (10 * i as i32, y);
                    let k = if f[0] == "hover" {
                        page.hit(pt)
                    } else {
                        page.click_hit(pt)
                    };
                    let got = k.map_or("-".to_string(), |k| {
                        String::from_utf8_lossy(&page.items[k].el.nomref).into_owned()
                    });
                    assert_eq!(&got, want, "{} at {pt:?} (page {p})", f[0]);
                }
                *n.entry(f[0]).or_default() += 1;
            }
            "frame" => {
                let s = s.as_mut().unwrap();
                let t = hexfloat(f[3]);
                let (x, y): (i32, i32) = (f[4].parse().unwrap(), f[5].parse().unwrap());
                let (released, held) = (f[6] == "c1", f[7] == "h1");
                page_of_frame = s.menus.page;
                s.menus.harness_input(released, held);
                let mut env = Env {
                    files: s.files,
                    config: &mut s.config,
                    texts: Some(s.texts),
                    host: &mut s.saves,
                    player: 1,
                    now: t,
                    pointer: (x, y),
                };
                let ev = s.menus.frame(&mut env, &mut s.screen);
                let (p0, p1) = f[8].trim_start_matches('p').split_once("->").unwrap();
                assert_eq!(page_of_frame.to_string(), p0, "{l}");
                assert_eq!(s.menus.page.to_string(), p1, "{l}");
                // Every frame's page surface as the harness's RGB565 screen
                // gets it (after the options re-created it in 32 bits: the
                // x8r8g8b8 page truncated by the copy).
                assert_eq!(format!("md5={}", md5_16(&s.menus.surface)), f[9], "{l}");
                md5s += 1;
                if ev.iter().any(|e| matches!(e, Event::DisplayChange(..))) {
                    // The walk's last depth click is `BT_32BITS`.
                    assert_eq!(s.menus.depth, Depth::Xrgb8888, "{l}");
                    recreations += 1;
                }
                let draws: Vec<String> = s
                    .menus
                    .draws
                    .iter()
                    .map(|d| {
                        format!(
                            "{}#{}@{}",
                            d.bank,
                            d.frame,
                            d.pos.map_or("def".into(), |(x, y)| format!("{x},{y}"))
                        )
                    })
                    .collect();
                assert_eq!(format!("draws={}", draws.join(",")), f[10], "{l}");
                assert_eq!(s.globals(), f[11..28].join(" "), "{l}");
                let b = s.menus.tip;
                let tip = b
                    .shown
                    .then(|| format!("tooltip_box={},{},{},{}", b.x0, b.y0, b.x1, b.y1));
                assert_eq!(tip.as_deref(), f.get(28).copied(), "{l}");
                pending.label = format!("{} {}", f[1], f[2]);
                pending.events = events(&ev);
                pending.texts = s.menus.texts.iter().map(text_line).collect();
                *n.entry("frame").or_default() += 1;
            }
            _ => {}
        }
    }
    if !pending.label.is_empty() {
        pending.check(&mut counts);
    }
    // 3 sizes: 3381 frames, 243 elements, 1295 rows of each map, 784
    // state lines.
    assert_eq!(
        (
            n["frame"],
            n["element"],
            n["hover"],
            n["clickmap"],
            n["state"]
        ),
        (3381, 243, 1295, 1295, 784)
    );
    assert!(counts[0] > 1500 && counts[1] > 0, "{counts:?}");
    assert_eq!(md5s, n["frame"], "every surface compared");
    assert_eq!(recreations, 3, "one per size");
}
