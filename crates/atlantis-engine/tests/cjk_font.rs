//! The Chinese CDs' subtitles (atlantis-cd-zh-tw-696832,
//! atlantis-cd-zh-cn-692736): every character of their chapter scripts
//! in the bundled CJK font, and the first dialog line of a new game drawn
//! in it, in rows 18 pixels apart over the black shadow, recorded for the
//! HD mode's sharp text with the same face.
//!
//! Needs the folders of the discs' images: set `ATLANTIS_ISO_ZH_TW` (the
//! Traditional CD's ISO images) and `ATLANTIS_BIN_ZH_CN` (the Simplified
//! CD's raw images); without them the tests are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::{Game, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::subtitles::{
    split_subtitle_cjk, subtitle_row_y, CjkSplit, FaceSource, FontFace, SubtitleFont, SUBTITLE_COLOR, SUBTITLE_SHADOW, SUBTITLE_X,
};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::{Event, MemStore, Options, Start};

fn game(var: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(var) else {
        eprintln!("{var} not set: skipped");
        return None;
    };
    let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
    Some(Game::open(Arc::new(set)).expect("the game from its images"))
}

/// The game of `var`'s images with a player's font file `name` beside
/// them (links to the images in a folder of the test's own, the file a
/// copy of the port's Noto Serif SC).
fn game_with_font(var: &str, name: &str) -> Option<(Game, std::path::PathBuf)> {
    let src = std::path::PathBuf::from(std::env::var_os(var)?);
    let dir = std::env::temp_dir().join(format!("atlantis-cjk-font-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        let (from, to) = (e.path(), dir.join(e.file_name()));
        #[cfg(unix)]
        std::os::unix::fs::symlink(from, to).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(from, to).ok()?;
    }
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/fonts/NotoSerifSC-Subset.otf"
    );
    std::fs::copy(font, dir.join(name)).unwrap();
    let set = DiscSet::open_folder(&dir).expect("CD images");
    Some((
        Game::open(Arc::new(set)).expect("the game from its images"),
        dir,
    ))
}

/// Every character the scripts' bytes read as (their pairs from the start
/// of the file, and those of every subtitle row the build's breaker cuts
/// from a dialog line, with and without the minor bug fixes: a row can
/// start on a character's second byte) has a glyph in the bundled font,
/// but those of the code page's user-defined area (private use), which no
/// font has.
fn scripts_in_bundled_font(var: &str, cp: TextCodePage) {
    let Some(g) = game(var) else { return };
    assert_eq!(g.exe.build.text_code_page, cp);
    let face = FontFace::bundled(cp);
    let names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.starts_with("SCENAR\\") && n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    assert!(names.len() > 70, "{} scripts", names.len());
    let mut chars = std::collections::BTreeSet::new();
    let splits: Vec<CjkSplit> = [false, true]
        .into_iter()
        .map(|fixes| CjkSplit::of_build(g.exe.build, fixes).unwrap())
        .collect();
    for n in &names {
        let data = g.big.read(n).unwrap();
        chars.extend(cp.decode(&data).chars());
        for line in data.split(|&c| c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let Some(close) = line.iter().position(|&c| c == b']') else {
                continue;
            };
            if line.first() != Some(&b'[') {
                continue;
            }
            let text = &line[close + 1..];
            let text = text.strip_prefix(b" ").unwrap_or(text);
            for &split in &splits {
                for row in split_subtitle_cjk(text, split) {
                    chars.extend(cp.decode(&row).chars());
                }
            }
        }
    }
    let missing: Vec<char> = chars
        .iter()
        .copied()
        .filter(|&c| c >= ' ' && !('\u{e000}'..='\u{f8ff}').contains(&c))
        .filter(|&c| !face.has_char(c))
        .collect();
    assert!(missing.is_empty(), "{var}: no glyph for {missing:?}");
    let cjk = chars.iter().filter(|&&c| c >= '\u{3000}').count();
    assert!(cjk > 1400, "{cjk} characters");
}

#[test]
fn traditional_scripts_in_bundled_font() {
    scripts_in_bundled_font("ATLANTIS_ISO_ZH_TW", TextCodePage::Windows950);
}

#[test]
fn simplified_scripts_in_bundled_font() {
    scripts_in_bundled_font("ATLANTIS_BIN_ZH_CN", TextCodePage::Windows936);
}

struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
}

impl Headless {
    fn step(&mut self) -> bool {
        let alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        alive
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            if !self.step() {
                break;
            }
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        let button = MouseButton::Left;
        self.sys.push_input(Input::MouseMove { x, y });
        self.sys.push_input(Input::MouseDown { button, x, y });
        self.step();
        self.sys.push_input(Input::MouseUp { button, x, y });
        self.steps(3);
    }

    fn key(&mut self, k: Key) {
        self.sys.push_input(Input::KeyDown(k));
        self.sys.push_input(Input::KeyUp(k));
        self.steps(2);
    }
}

/// A new game up to its first subtitle (subtitles on, the HD mode's text
/// recorded): the last row at y = 0x1ce - 2 * 18 in the build's font, the
/// shadow at (+1, +1), the white and black pixels of the frame those of
/// the rows drawn by the font.
fn first_subtitle(var: &str, build: &str, cp: TextCodePage, player_font: Option<&str>) {
    let mut planted = None;
    let g = match player_font {
        Some(name) => game_with_font(var, name).map(|(g, dir)| {
            planted = Some(dir);
            g
        }),
        None => game(var),
    };
    let Some(g) = g else { return };
    assert_eq!(g.exe.build.name, build);
    assert_eq!(g.exe.build.subtitle_row_step, 18);
    let sys = Sys::new(Mixer::new());
    sys.set_hd_tile(Some(32));
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: Rc::new(MemStore::new()),
        start: Start::Boot,
        subtitles: true,
        ..Options::default()
    };
    let mut h = Headless {
        runner: Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts)),
        sys: sys.clone(),
        now: 0,
        sound: vec![0.0; 2 * (22050 * 16 / 1000)],
    };
    let player_screen = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .any(|e| matches!(e, Event::Menu(n) if n == "SELEMENU.TXT"))
    };
    for _ in 0..100 {
        h.steps(30);
        if player_screen(&trace) {
            break;
        }
        h.click(320, 240);
    }
    assert!(player_screen(&trace), "{build}: no player screen");
    h.steps(10);
    h.click(200, 259);
    for c in "Tester".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    // The first line's subtitle, recorded for the HD mode.
    let mut items = Vec::new();
    for _ in 0..20000 {
        if !h.step() {
            break;
        }
        let t = h.sys.hd_text.borrow();
        if !t.items.is_empty() {
            items = t.items.clone();
            break;
        }
    }
    assert!(!items.is_empty(), "{build}: no subtitle");
    let text = h.sys.hd_text.borrow().clone();
    let n = items.len() / 2;
    let face = items[0].face.clone().expect("the subtitle's face");
    if planted.is_some() {
        // The player's file, its face drawn at GDI's ascent.
        assert_eq!(face.source(), FaceSource::Player, "{build}");
        assert_ne!(face, FontFace::bundled(cp));
    } else {
        assert_eq!(face, FontFace::bundled(cp), "{build}: the bundled face");
    }
    for (k, it) in items.iter().enumerate() {
        let (i, shadow) = (k / 2, k % 2 == 0);
        let y = subtitle_row_y(i, n, 18);
        let (o, color) = if shadow {
            (1, SUBTITLE_SHADOW)
        } else {
            (0, SUBTITLE_COLOR)
        };
        assert_eq!(
            (it.x, it.y, it.color),
            (SUBTITLE_X + o, y + o, color),
            "{build}: item {k} of {}",
            items.len()
        );
        assert_eq!(it.code_page, cp);
        assert_eq!(it.face.as_ref(), Some(&face), "{build}");
    }
    assert_eq!(items.last().unwrap().y, 0x1ce - 2 * 18);
    // The frame: the rows drawn by the font over what was under them.
    let font = SubtitleFont::with_face(cp, face.clone()).with_row_step(18);
    let mut want = text.under.clone();
    let lines: Vec<Vec<u8>> = items.iter().step_by(2).map(|it| it.text.clone()).collect();
    font.draw_subtitles(&mut want, 640, 480, &lines, 0);
    assert!(want == text.after, "{build}: the frame's rows");
    let changed: Vec<usize> = (0..want.len())
        .filter(|&p| text.after[p] != text.under[p])
        .map(|p| p / 640)
        .collect();
    assert!(changed.len() > 100, "{build}: {} pixels", changed.len());
    let top = subtitle_row_y(0, n, 18) as usize;
    assert!(changed.iter().all(|&y| (top..0x1ce - 18 + 17).contains(&y)));
    eprintln!("{build}: {n} rows, {} pixels", changed.len());
    if let Some(dir) = planted {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn traditional_first_subtitle() {
    first_subtitle(
        "ATLANTIS_ISO_ZH_TW",
        "atlantis-cd-zh-tw-696832",
        TextCodePage::Windows950,
        None,
    );
}

#[test]
fn simplified_first_subtitle() {
    first_subtitle(
        "ATLANTIS_BIN_ZH_CN",
        "atlantis-cd-zh-cn-692736",
        TextCodePage::Windows936,
        None,
    );
}

/// The player's own font beside the images is drawn with: `mingliu.ttc`
/// for the Traditional CD, `simsun.ttc` for the Simplified one.
#[test]
fn players_font_beside_the_images() {
    first_subtitle(
        "ATLANTIS_ISO_ZH_TW",
        "atlantis-cd-zh-tw-696832",
        TextCodePage::Windows950,
        Some("MINGLIU.TTC"),
    );
    first_subtitle(
        "ATLANTIS_BIN_ZH_CN",
        "atlantis-cd-zh-cn-692736",
        TextCodePage::Windows936,
        Some("simsun.ttc"),
    );
}
