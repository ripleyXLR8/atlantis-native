//! The texts the flip draws over every frame (the subtitle line, the
//! three `PLAYVOICE` voice slots) against the original's own code under
//! emulation (`tests/golden/warp_subtitles.txt`): the star room's tips set
//! as its frame sets them, voices taken, ended, stopped, refused when the
//! three slots are busy, hidden and kept while the subtitle line shows,
//! kept while `SubTitleOn` is off, cleared; after every flip the prints
//! (layout, position, advances, colour, text by md5) and the slots' state,
//! at 640 × 480, 800 × 600 and 1024 × 768. Then the warp frame, the
//! inventory and Escape with them.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::text::{layout, FontId, GdiFont};
use atlantis3_engine::{Game, Mode};
use atlantis_data::files::{FileSource, StdDir};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/warp_subtitles.txt");

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
    if b.is_empty() {
        return "-".into();
    }
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// The game at `w` × `h` (`None` without the install).
fn game(w: usize, h: usize) -> Option<Game> {
    let dir = atlantis3_data::testing::install_dir()?;
    let std_dir = StdDir::new(&dir);
    let mut cfg = std_dir.read("Datas/Atlantis3.cfg").unwrap_or_default();
    cfg.extend_from_slice(format!("\r\nResolution\t\t= {w} {h}\r\n").as_bytes());
    let files = A3Files::new(Arc::new(Resized { dir: std_dir, cfg }));
    let g = Game::new(files).expect("the game starts");
    assert_eq!((g.width(), g.height()), (w, h), "the golden's screen");
    Some(g)
}

/// The font heights of `font.txt` at a screen width (`ftol(h × f)`).
fn height(font: FontId, w: usize) -> i32 {
    let base = match font {
        FontId::Default => 21.0f32,
        FontId::Special => 24.0,
        _ => panic!("{font:?}"),
    };
    let f = match w {
        640 => 0.9f32,
        1024 => 1.1f32,
        _ => 1.0f32,
    };
    (f64::from(base) * f64::from(f)) as i32
}

/// A flip: @ 0x0040de60's prints as the golden writes them, then the
/// slots and the subtitle line.
fn flip(g: &mut Game, w: usize, h: usize) -> (Vec<String>, Vec<String>, Option<String>) {
    let mut draws = Vec::new();
    for p in g.test_flip_plan() {
        let z = p.zone;
        draws.push(format!(
            "LAYOUT {} {} {} {} {} {}",
            z.ml, z.mr, z.mt, z.mb, z.halign, z.valign
        ));
        let gdi = GdiFont::arial(height(p.font, w)).expect("measured");
        for l in layout(gdi, &p.text, w as i32, h as i32, z) {
            let dx = if l.dx.is_empty() {
                "-".to_string()
            } else {
                l.dx.iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            draws.push(format!(
                "DRAW {} {} {:06x} {} {} {} {}",
                l.x,
                l.y,
                p.rgb,
                p.font.name(),
                l.bytes.len(),
                dx,
                md5(&l.bytes)
            ));
        }
    }
    let slots = g
        .test_voice_slots()
        .iter()
        .enumerate()
        .map(|(k, s)| {
            format!(
                "SLOT {k} {} {},{},{} {} {}",
                u8::from(s.busy),
                s.rgb[0],
                s.rgb[1],
                s.rgb[2],
                if s.name.is_empty() { "-" } else { &s.name },
                md5(&s.text)
            )
        })
        .collect();
    let sub = g
        .test_subtitle()
        .filter(|(t, ..)| !t.is_empty())
        .map(|(t, rgb, caption)| {
            format!(
                "SUB {} {},{},{} {}",
                u8::from(caption),
                rgb[0],
                rgb[1],
                rgb[2],
                md5(&t)
            )
        });
    (draws, slots, sub)
}

#[test]
fn the_flip_draws_the_subtitle_line_and_the_voice_slots_as_the_original() {
    let lines: Vec<&str> = GOLDEN.lines().filter(|l| !l.starts_with('#')).collect();
    let mut i = 0;
    let (mut flips, mut draws) = (0, 0);
    let mut g: Option<Game> = None;
    let (mut w, mut h) = (0, 0);
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "SCREEN" => {
                (w, h) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let Some(mut n) = game(w, h) else {
                    return;
                };
                n.test_text_op("world", &[3], "");
                n.test_text_op("subon", &[1], "");
                g = Some(n);
            }
            "STEP" => {
                let g = g.as_mut().unwrap();
                let args: Vec<i32> = f[3..].iter().filter_map(|v| v.parse().ok()).collect();
                let name = f.get(3).copied().unwrap_or("");
                if f[2] != "flip" {
                    if f[2] == "voice" {
                        // The voice's name as PLAYVOICE names it.
                        let want = g.test_voice_name(args[0], args[1]);
                        assert!(!want.is_empty(), "{l}");
                    }
                    g.test_text_op(f[2], &args, name);
                    continue;
                }
                // The flip's prints, the slots, the subtitle line.
                let mut want_draws = Vec::new();
                while i < lines.len()
                    && (lines[i].starts_with("LAYOUT") || lines[i].starts_with("DRAW"))
                {
                    want_draws.push(lines[i].to_string());
                    i += 1;
                }
                let want_slots: Vec<String> =
                    lines[i..i + 3].iter().map(|s| s.to_string()).collect();
                let want_sub = lines[i + 3];
                i += 4;
                let (got_draws, got_slots, got_sub) = flip(g, w, h);
                assert_eq!(got_draws, want_draws, "{w}x{h} {l}");
                assert_eq!(got_slots.len(), 3);
                for (got, want) in got_slots.iter().zip(&want_slots) {
                    assert!(
                        got.eq_ignore_ascii_case(want),
                        "{w}x{h} {l}: {got} / {want}"
                    );
                }
                // A cleared subtitle line keeps its colour and flag in the
                // original; the port forgets them (nothing draws them).
                let sw: Vec<&str> = want_sub.split(' ').collect();
                match got_sub {
                    Some(s) => assert_eq!(s, want_sub, "{w}x{h} {l}"),
                    None => assert_eq!(sw[3], "-", "{w}x{h} {l}"),
                }
                flips += 1;
                draws += want_draws.iter().filter(|d| d.starts_with("DRAW")).count();
            }
            _ => panic!("{l}"),
        }
    }
    if g.is_some() {
        let want = lines
            .iter()
            .filter(|l| l.starts_with("STEP") && l.ends_with(" flip"))
            .count();
        assert_eq!(flips, want, "flips");
        assert!(draws > 150, "{draws}");
    }
}

/// Pixels of the bottom band (where the texts go) that are not black.
fn lit(g: &Game) -> usize {
    let (w, h) = (g.width(), g.height());
    g.screen()[(h - 80) * w..]
        .iter()
        .filter(|&&p| p != 0)
        .count()
}

#[test]
fn the_warp_frame_flips_the_tip_and_the_voices_and_escape_clears_them() {
    let Some(mut g) = game(800, 600) else {
        return;
    };
    g.flat_warp = true;
    assert!(g.load_place("PMagi.wam"));
    g.enter_at(10, 0.0, 0.0);
    g.test_text_op("world", &[3], "");
    g.test_text_op("subon", &[1], "");
    let mut t = 100.0;
    let mut frame = |g: &mut Game| {
        t += 1.0 / 15.0;
        g.step(t);
    };
    frame(&mut g);
    assert_eq!(g.mode(), Mode::Warp);
    assert_eq!(lit(&g), 0, "no text");
    // The tip set as the star room's frame sets it: drawn by the frame's
    // flip.
    g.test_text_op("sub", &[216, 1, 200, 255, 255], "");
    frame(&mut g);
    assert!(lit(&g) > 200, "the tip over the warp");
    g.test_text_op("clear", &[], "");
    frame(&mut g);
    assert_eq!(lit(&g), 0, "cleared");
    // A voice: drawn while it plays, its slot freed by the flip after it
    // ends.
    g.test_text_op("voice", &[199, 1, 200, 255, 255], "");
    let name = g.test_voice_name(199, 1);
    frame(&mut g);
    assert!(g.test_voice_slots()[0].busy);
    assert!(lit(&g) > 200, "the voice's text over the warp");
    g.test_text_op("end", &[], &name);
    frame(&mut g);
    assert!(!g.test_voice_slots()[0].busy, "freed by the flip");
    assert_eq!(lit(&g), 0);
    // `SubTitleOn` off: nothing drawn, the slot kept after its voice ends.
    g.test_text_op("subon", &[0], "");
    g.test_text_op("voice", &[199, 1, 200, 255, 255], "");
    frame(&mut g);
    assert_eq!(lit(&g), 0);
    g.test_text_op("end", &[], &name);
    frame(&mut g);
    assert!(g.test_voice_slots()[0].busy, "kept with the subtitles off");
    g.test_text_op("subon", &[1], "");
    // Escape in the warp: the slots stopped and freed (@ 0x0042ad09).
    g.test_text_op("voice", &[199, 1, 200, 255, 255], "");
    assert!(g.test_voice_slots()[1].busy);
    g.escape();
    assert!(
        g.test_voice_slots().iter().all(|s| !s.busy),
        "cleared by Escape"
    );
}
