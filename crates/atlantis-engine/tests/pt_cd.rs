//! The Brazilian Portuguese CD (MSD Multimídia, 1997, *Atlantis: A Lenda
//! Perdida*): its own build, atlantis-cd-pt-692736 (the Spanish October
//! 1997 program with the Chinese builds' 114-byte `players.atl`), over the
//! August 1997 archives with Portuguese voices, texts and fonts. Its
//! subtitles (the 1998 program's breaker `FUN_0042eae0`, GDI rows) against
//! the rows of its emulation on every dialog line, its Windows-1252 texts
//! and the subtitle font's glyphs for their letters, its tables and menu
//! fonts (a glyph for each byte up to 0xff, the program's own width
//! tables), its edition and fixes, from the start-up a new game to its
//! first dialog line (its rows on the screen) with a name typed in
//! Portuguese letters, a save and its load from the player screen and the
//! LOAD list, every chapter the scripts reach, chained as the game's
//! chapter changes are, and every chapter resumed from checkpoints the
//! original wrote. The expected values are numbers and digests only.
//!
//! Needs the folder of the CD's images (four `.iso`), or a folder with its
//! `atlantis.exe` and `BIGCD1.BIG` … `BIGCD4.BIG`: set `ATLANTIS_DATA_PTBR`;
//! without it the tests are skipped. A folder with `.iso` files is read
//! through them only. `ATLANTIS_SAVES_PTBR`, a folder of the
//! original's checkpoints (`pa<N>.gam`), for `every_chapter_resumed`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_codecs::HnmVideo;
use atlantis_data::disc_image::{DiscImage, DiscSet};
use atlantis_data::exe::{PT_CD, PT_FONTMAX_WIDTHS, PT_FONTMIN_ADVANCE};
use atlantis_data::saves::{Checkpoint, Players, PLAYERS_SIZE_ZH};
use atlantis_data::sprite::{limits, read_spr_file};
use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::con_script::script::{latin1_bytes, ConItem};
use atlantis_engine::con_script::{con_track_letter, ConScript};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::scripts::Scripts;
use atlantis_engine::subtitles::{split_subtitle, FontFace, SubtitleFont};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::{
    menu_number, walk_menu_text, FontMaxWidths, MenuRules, MenuWalk,
};
use atlantis_engine::{
    BugFix, BugFixes, Engine, Event, FixData, MemStore, Options, SaveStore, Start,
};
use md5::{Digest, Md5};

const VAR: &str = "ATLANTIS_DATA_PTBR";
const BUILD: &str = "atlantis-cd-pt-692736";

/// The golden of the breaker: atlantis-cd-pt-692736's FUN_0042eae0 (the
/// 1998 program's FUN_0042f108) run unmodified under emulation on every
/// dialog line (md5s of the rows, their counts).
const GOLDEN: &str = include_str!("golden/video_subtitles_pt692736.txt");

/// The `.iso` images of `dir`, in name order; `None` without any.
fn iso_set(dir: &Path) -> Option<DiscSet> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("the images' folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return None;
    }
    let images: Vec<DiscImage> = paths.iter().filter_map(|p| DiscImage::open(p)).collect();
    assert_eq!(images.len(), 4, "four disc images in {}", dir.display());
    Some(DiscSet { images })
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: {test} skipped");
        return None;
    };
    let dir: &Path = dir.as_ref();
    let g = match iso_set(dir) {
        Some(set) => Game::open(Arc::new(set)).expect("the game from its images"),
        None => Game::open(Arc::new(StdDir::new(dir))).expect("the game from its folder"),
    };
    assert_eq!(g.exe.build.name, BUILD);
    Some(g)
}

/// The golden's "key value" lines (the key everything before the last
/// space).
fn golden() -> BTreeMap<String, String> {
    GOLDEN
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.rsplit_once(' '))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The dialog lines' texts of a CON file, as the breaker gets them: the
/// text after "] " up to the CR.
fn dialog_texts(con: &[u8]) -> Vec<&[u8]> {
    con.split(|&c| c == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| l.first() == Some(&b'['))
        .filter_map(|l| {
            let close = l.iter().position(|&c| c == b']')?;
            let rest = &l[close + 1..];
            (!rest.iter().all(u8::is_ascii_whitespace))
                .then(|| rest.strip_prefix(b" ").unwrap_or(rest))
        })
        .collect()
}

/// The CON files (first copies over the discs), in name order.
fn con_files(g: &Game) -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|n| {
            let data = g.big.read(&n).unwrap();
            (n, data)
        })
        .collect()
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every dialog line through the port's breaker: each CON file's rows
/// (each line's rows joined by NUL, the lines by LF) against the
/// original's, emulated, and the statistics.
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    assert!(!g.exe.build.bitmap_subtitles);
    let want = golden();
    let mut total = Md5::new();
    let (mut lines, mut hist, mut longest, mut high) = (0, [0usize; 9], 0, 0);
    for (name, con) in con_files(&g) {
        let mut h = Md5::new();
        for t in dialog_texts(&con) {
            let rows = split_subtitle(t);
            lines += 1;
            hist[rows.len().min(8)] += 1;
            high += t.iter().filter(|&&c| c >= 0x80).count();
            for r in &rows {
                longest = longest.max(r.len());
            }
            h.update(rows.join(&0u8));
            h.update(b"\n");
        }
        let d: [u8; 16] = h.finalize().into();
        assert_eq!(Some(&hex(&d)), want.get(&name), "{name}");
        total.update(d);
    }
    assert_eq!(Some(&hex(&total.finalize())), want.get("rows md5"));
    assert_eq!(lines, 2541);
    assert_eq!(hist, [0, 2244, 275, 16, 6, 0, 0, 0, 0]);
    assert_eq!((longest, high), (100, 4063));
}

/// The texts are Windows-1252, as a Brazilian Windows reads them: every
/// byte of the dialog lines has a character there, the Portuguese letters
/// among them, and the subtitle font has a glyph for each.
#[test]
fn portuguese_letters_in_the_subtitles() {
    let Some(g) = game("portuguese_letters_in_the_subtitles") else {
        return;
    };
    let cp = g.exe.build.text_code_page;
    assert_eq!(cp, TextCodePage::Windows1252);
    let mut chars = BTreeSet::new();
    for (_, con) in con_files(&g) {
        for t in dialog_texts(&con) {
            let s = cp.decode(t);
            assert!(!s.contains('\0'), "{s}");
            chars.extend(s.chars().filter(|c| !c.is_ascii()));
        }
    }
    let got: String = chars.iter().collect();
    assert_eq!(got, "ÁÃÉÊÓàáãçéêíóôõú");
    let face = FontFace::bundled(cp);
    for c in &chars {
        assert!(face.has_char(*c), "no glyph for {c}");
    }
    let font = SubtitleFont::with_code_page(cp);
    let mut px = vec![0u16; 64 * 20];
    font.draw(&mut px, 64, 20, &[0xe3], 2, 2, 60, 0xffff);
    assert!(px.contains(&0xffff), "ã drawn");
}

/// The program's tables: the 1998 program's music, sound, inventory and
/// guest tables at its own addresses, the row `Novo Jogador` (no '#', also
/// the label), its own credits widths, and the menu width tables the port
/// holds (`PT_FONTMAX_WIDTHS`, `PT_FONTMIN_ADVANCE`) equal to the bytes at
/// 0x004865df and 0x004866bf (file offsets 0x733df and 0x734bf: DGROUP at
/// 0x00480000, raw 0x6ce00); FONTMAX's widths are its font's frame widths
/// but the comma's (2, its frame 8 pixels wide).
#[test]
fn tables_of_this_build() {
    let Some(g) = game("tables_of_this_build") else {
        return;
    };
    let exe = &g.exe;
    assert_eq!(exe.new_player_marker, b"Novo Jogador");
    assert_eq!(exe.new_player_label, b"Novo Jogador");
    // The music and NewSound names: the reference's (the golden of
    // atlantis-data's tests, the names joined in capitals).
    let golden = include_str!("../../atlantis-data/tests/golden/scripts_music.txt");
    let value = |key: &str| {
        golden
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
            .unwrap()
            .to_string()
    };
    let digest = |names: &[String]| {
        let joined: Vec<String> = names.iter().map(|n| n.to_ascii_uppercase()).collect();
        hex(&Md5::digest(joined.join("\n").as_bytes()))
    };
    assert_eq!(exe.music_names.len().to_string(), value("music_count"));
    assert_eq!(digest(&exe.music_names), value("music_names"));
    assert_eq!(
        exe.newsound_names.len().to_string(),
        value("newsound_count")
    );
    assert_eq!(digest(&exe.newsound_names), value("newsound_names"));
    assert_eq!(exe.inventory_object_ids.len(), 51);
    assert_eq!(exe.guest_sprite_ids.len(), 9);
    let bytes = g.big.source().read("ATLANTIS.EXE").unwrap();
    assert_eq!(bytes.len(), 692_736);
    assert_eq!(bytes[0x733df..0x733df + 224], PT_FONTMAX_WIDTHS);
    assert_eq!(bytes[0x734bf..0x734bf + 191], PT_FONTMIN_ADVANCE);
    assert_eq!(bytes[0x7357e..0x7357e + 256], exe.credit_widths);
    assert_eq!(PT_CD.credit_widths_va, 0x0048_677e);
    let (w, h) = limits::DEFAULT;
    let frames = |n: &str| {
        let d = g.big.read(&format!("SPRITE\\2D\\{n}.SPR")).unwrap();
        read_spr_file(&d, w, h).unwrap()
    };
    let fontmax = frames("FONTMAX");
    assert_eq!(fontmax.len(), 224);
    let mut widths: Vec<u8> = fontmax.iter().map(|s| s.w as u8).collect();
    assert_eq!(widths[usize::from(b',' - 0x20)], 8);
    widths[usize::from(b',' - 0x20)] = 2;
    assert_eq!(widths, PT_FONTMAX_WIDTHS);
    for n in ["FONTMAX2", "FONTMIN", "FONTMIN2"] {
        assert_eq!(frames(n).len(), 224, "{n}");
    }
    for n in ["CREDBLAN", "CREDBLEU"] {
        assert_eq!(frames(n).len(), 223, "{n}");
    }
}

/// The menus: the letters of the texts the menus, the LOAD list and the
/// credits draw (Windows-1252 bytes, the glyph indices of the fonts) all
/// within the fonts' 224 glyphs, and the walk drawing and measuring them
/// with the program's own FONTMAX widths (the options menu's title).
#[test]
fn portuguese_menus() {
    let Some(g) = game("portuguese_menus") else {
        return;
    };
    let mut high = BTreeSet::new();
    let names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.starts_with("SPRLIST\\"))
        .filter(|n| {
            n.ends_with("MENU.TXT")
                || n.ends_with("WAIT.TXT")
                || n.contains("ERROR")
                || n.ends_with("EPI.TXT")
                || n.ends_with("CREDITS.TXT")
        })
        .map(str::to_string)
        .collect();
    assert_eq!(names.len(), 17);
    for n in &names {
        high.extend(g.big.read(n).unwrap().into_iter().filter(|&b| b >= 0x80));
    }
    let letters: String = high
        .iter()
        .map(|&b| TextCodePage::Windows1252.char_of(b))
        .collect();
    assert_eq!(letters, "ÇÊÍÓÕàáãäçéêíîóõú");
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::NONE);
    assert_eq!(rules.fontmax, FontMaxWidths::of_build(&PT_CD));
    assert!(!rules.sharp_s_as_ss);
    let text = g.big.read("SPRLIST\\OPTMENU.TXT").unwrap();
    let first = text.split(|&c| c == b'\r').next().unwrap();
    assert_eq!(first, b"<>,50>MENU DE OP\xc7\xd5ES");
    let mut walk = MenuWalk::default();
    walk_menu_text(
        &text,
        menu_number("OPTMENU.TXT").unwrap() as i32,
        -1,
        0,
        0,
        rules,
        &mut walk,
    );
    let title: i32 = b"OP\xc7\xd5ES"
        .iter()
        .map(|&c| i32::from(PT_FONTMAX_WIDTHS[usize::from(c - 0x20)]))
        .sum();
    // "MENU DE OPÇÕES": the pen moves by the build's widths over `Ç` and
    // `Õ` (glyphs 0xa7 and 0xb5); the title record is as wide as the
    // build's measure of the line.
    let glyphs: Vec<(i32, i16)> = walk
        .ops
        .iter()
        .filter(|o| !o.sprite && o.line == 0)
        .map(|o| (o.index, o.x))
        .collect();
    let at = glyphs.iter().position(|g| g.0 == 0xa7).unwrap();
    assert_eq!(glyphs[at + 1].0, 0xb5);
    assert_eq!(i32::from(glyphs[at + 1].1 - glyphs[at].1), 21);
    assert_eq!(i32::from(glyphs[at + 2].1 - glyphs[at + 1].1), 28);
    assert_eq!(title, 135);
    assert_eq!(walk.records[0].w, 275);
    assert_eq!(rules.fontmax.measure(b"MENU DE OP\xc7\xd5ES"), 275);
    assert_ne!(
        FontMaxWidths::default().measure(b"MENU DE OP\xc7\xd5ES"),
        275
    );
}

/// The release as shown, and the fixes the options list for it: the
/// every-build fixes and the 1998 program's hexagon; not the German `ß`
/// one (0xe1 is `á` here, a glyph of its fonts), nor the disc revisions
/// (the fonts and credits are the same on every disc).
#[test]
fn edition_and_fixes_of_this_cd() {
    let Some(g) = game("edition_and_fixes_of_this_cd") else {
        return;
    };
    assert_eq!(
        g.edition().to_string(),
        "CD, Brazilian Portuguese (MSD, December 1997 build)"
    );
    let b = g.exe.build;
    assert!(b.players_1997 && b.subtitle_click_cycle && !b.bitmap_subtitles);
    let epi = g.big.read("SPRLIST\\EPI.TXT").unwrap();
    assert!(epi.contains(&atlantis_engine::ui::layout::SHARP_S));
    let data = FixData::of(&g);
    assert!(!data.sharp_s && !data.disc_revisions);
    let own: Vec<BugFix> = BugFix::of_game(&g)
        .into_iter()
        .filter(|f| !f.of_every_build())
        .collect();
    assert_eq!(own, [BugFix::HexagonPieces]);
}

/// A damaged copy of the discs: these films and the puzzle sprite bank,
/// which a known copy of the discs holds with unreadable sectors read as
/// zeros, decode without a panic (a film stops at its first frame that
/// does not decode; the bank reads).
#[test]
fn damaged_members_of_the_images_play() {
    let Some(g) = game("damaged_members_of_the_images_play") else {
        return;
    };
    for name in [
        "UBB_VUE\\FO028027.HNM",
        "UBB_VUE\\FO028029.HNM",
        "UBB_VUE\\FO028034.HNM",
        "UBB_VUE\\FO028035.HNM",
        "UBB_VUE\\FO029028.HNM",
        "UBB_VUE\\FO029030.HNM",
        "CYCLO\\FOR16029.HNM",
        "CYCLO\\PEC16135.HNM",
    ] {
        let data = g.big.read(name).unwrap();
        let Ok(mut v) = HnmVideo::new(&data[..]) else {
            continue;
        };
        let count = v.frame_count();
        let mut n = 0;
        while let Some(f) = v.next_frame() {
            if f.is_err() {
                break;
            }
            n += 1;
        }
        eprintln!("{name}: {n} of {count} frames");
    }
    let (w, h) = limits::DEFAULT;
    for disc in g.big.discs_of("PUZZLES\\PZ1L.SPR") {
        g.big.set_disc_copy(Some(disc));
        let d = g.big.read("PUZZLES\\PZ1L.SPR").unwrap();
        g.big.set_disc_copy(None);
        let n = read_spr_file(&d, w, h).map_or(0, |s| s.len());
        eprintln!("PZ1L.SPR disc {disc}: {n} sprites");
    }
}

struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
    alive: bool,
}

impl Headless {
    fn new(g: Game, opts: Options) -> Headless {
        let sys = Sys::new(Mixer::new());
        Headless {
            runner: Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts)),
            sys,
            now: 0,
            sound: vec![0.0; 2 * (22050 * 16 / 1000)],
            alive: true,
        }
    }

    fn step(&mut self) -> bool {
        self.alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        self.alive
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

    fn until(&mut self, n: usize, mut f: impl FnMut(&Headless) -> bool) -> bool {
        for _ in 0..n {
            if f(self) || !self.step() {
                return f(self);
            }
        }
        false
    }
}

fn menus(t: &Rc<RefCell<Vec<Event>>>, m: &str) -> usize {
    t.borrow()
        .iter()
        .filter(|e| matches!(e, Event::Menu(n) if n == m))
        .count()
}

/// From the start-up to the player screen: clicks through the films.
fn to_player_screen(h: &mut Headless, trace: &Rc<RefCell<Vec<Event>>>) -> bool {
    for _ in 0..100 {
        h.steps(30);
        if menus(trace, "SELEMENU.TXT") > 0 {
            return true;
        }
        h.click(320, 240);
    }
    false
}

/// The text of the dialog line `letter` of section `section` in the CON
/// of `chapter`.
fn line_text(g: &Game, chapter: u32, section: i32, letter: char) -> Option<Vec<u8>> {
    let con = g.big.read(&format!("SCENAR\\CHAPI{chapter:03}.CON"))?;
    let script = ConScript::parse(&con);
    script
        .sections()
        .iter()
        .filter(|s| s.id == section)
        .find_map(|s| {
            s.items
                .iter()
                .filter_map(|i| match i {
                    ConItem::Line(l) => Some(l),
                    ConItem::Set { .. } => None,
                })
                .enumerate()
                .find(|(k, _)| con_track_letter(*k) == letter)
                .map(|(_, l)| latin1_bytes(&l.text))
        })
}

/// The number of 14-pixel row bands above y 0x1ce, from x 5, holding the
/// subtitles' white (0xffff) over at least `min` pixels, counted up from
/// the bottom row until a band without.
fn white_rows(screen: &[u16], min: usize) -> usize {
    let mut n = 0;
    loop {
        let y0 = 0x1ce - (n as i32 + 2) * 14;
        if y0 < 0 {
            return n;
        }
        let white = (y0..y0 + 14)
            .flat_map(|y| (5..635).map(move |x| (x, y)))
            .filter(|&(x, y)| screen[y as usize * 640 + x as usize] == 0xffff)
            .count();
        if white < min {
            return n;
        }
        n += 1;
    }
}

/// From the start-up: the logos, the player screen, a new player, the
/// first chapter, the clicks through the opening to the first dialog line
/// with a subtitle: CHAPI001's first line, its rows as the original's
/// breaker cuts them, that many rows of white on the screen; then the line
/// played with that many rows.
#[test]
fn new_game_to_the_first_dialog() {
    let Some(g) = game("new_game_to_the_first_dialog") else {
        return;
    };
    let Some(probe) = game("new_game_to_the_first_dialog") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        subtitles: true,
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    h.click(200, 259);
    // "João": `ã` (0xe3 in Windows-1252) is past the name entry's
    // 0x20..0xa7, so the program takes "Joo".
    for c in "João".chars() {
        h.key(Key::Char(c));
    }
    h.steps(2);
    h.key(Key::Enter);
    assert!(
        h.until(2000, |_| trace.borrow().contains(&Event::ChapterLoaded(1))),
        "chapter 1 not loaded"
    );
    // The player row as typed but `ã`, in a players.atl of 114 bytes.
    let file = store.read(PLAYERS_FILE).expect("players.atl");
    assert_eq!(file.len(), PLAYERS_SIZE_ZH);
    let marker = probe.exe.new_player_marker.clone();
    let players = Players::read_rows(&file, &marker, false).expect("the players");
    assert!(
        (0..players.player_count()).any(|i| players.name(i) == b"Joo"),
        "{:?}",
        (0..players.player_count())
            .map(|i| players.name(i).to_vec())
            .collect::<Vec<_>>()
    );
    let mut found = None;
    for _ in 0..400 {
        let seen = trace.borrow().len();
        h.steps(20);
        let hit = trace.borrow()[seen..].iter().find_map(|e| match e {
            Event::Line {
                section, letter, ..
            } => Some((*section, *letter)),
            _ => None,
        });
        if let Some((section, letter)) = hit {
            if let Some(text) = line_text(&probe, 1, section, letter) {
                let trimmed = text.trim_ascii_end().to_vec();
                if !trimmed.is_empty() {
                    found = Some((section, letter, trimmed));
                    break;
                }
            }
        }
        if !h.sys.mouse_locked() {
            h.click(320, 240);
        }
    }
    let (section, letter, text) = found.expect("no dialog line");
    let con = probe.big.read("SCENAR\\CHAPI001.CON").unwrap();
    let raw = dialog_texts(&con)[0];
    assert_eq!(raw.trim_ascii_end(), &text[..], "not CHAPI001's first line");
    let rows = split_subtitle(raw);
    let lens: Vec<String> = rows.iter().map(|r| r.len().to_string()).collect();
    assert_eq!(
        Some(&lens.join(",")),
        golden().get("first line SCENAR\\CHAPI001.CON")
    );
    let mut drawn = 0;
    for _ in 0..200 {
        h.step();
        let n = white_rows(&h.sys.screen.borrow(), 15);
        if n >= rows.len() {
            drawn = n;
            break;
        }
    }
    assert_eq!(drawn, rows.len(), "the first line's rows on the screen");
    assert!(
        h.until(3000, |_| trace.borrow().iter().any(|e| matches!(
            e,
            Event::LinePlayed { section: s, letter: l, subtitles, .. }
                if *s == section && *l == letter && *subtitles == rows.len()
        ))),
        "the line's subtitles"
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}

/// The chapters of `g` the scripts reach from chapter 1, breadth first
/// along their `Chapter=` commands (the chapters with a CON file).
fn chapter_chain(g: &Game) -> Vec<u32> {
    let mut links: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (name, con) in con_files(g) {
        let Some(n) = name
            .strip_prefix("SCENAR\\CHAPI")
            .and_then(|r| r.strip_suffix(".CON"))
            .and_then(|r| r.parse::<u32>().ok())
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&con).to_ascii_lowercase();
        let mut to = Vec::new();
        for (at, _) in text.match_indices("chapter") {
            let rest = text[at + 7..].trim_start();
            if let Some(v) = rest.strip_prefix('=') {
                let digits: String = v
                    .trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if let Ok(d) = digits.parse() {
                    to.push(d);
                }
            }
        }
        links.insert(n, to);
    }
    let mut order = Vec::new();
    let mut seen = BTreeSet::from([1]);
    let mut queue = VecDeque::from([1]);
    while let Some(c) = queue.pop_front() {
        order.push(c);
        for &d in &links[&c] {
            if links.contains_key(&d) && seen.insert(d) {
                queue.push_back(d);
            }
        }
    }
    assert_eq!(seen.len(), links.len(), "every chapter reached");
    order
}

/// The whole story's chapters chained: from a new game, each chapter the
/// scripts reach entered in turn as a `Chapter=` command enters it (the
/// checkpoint written, then players.atl, the chapter's CON loaded and its
/// /INIT run), each checkpoint parsed back with its chapter.
#[test]
fn every_chapter_chained() {
    let Some(g) = game("every_chapter_chained") else {
        return;
    };
    let chain = chapter_chain(&g);
    assert_eq!(chain.len(), 75);
    let store = Rc::new(MemStore::new());
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let st = store.clone();
    let steps = chain.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = st;
        let marker = e.game.exe.new_player_marker.clone();
        e.players = Players::new(&marker);
        e.players.layout_1997 = e.game.exe.build.players_1997;
        e.players.sixth_row = Some([0; 17]);
        e.player = e.players.create(b"Ana").expect("a row");
        let mut scripts = Scripts::default();
        e.load_checkpoint(&mut scripts, false).await;
        for c in steps.into_iter().skip(1) {
            scripts
                .exec_set(&mut e, None, &format!("chapter={c}"))
                .await;
            assert_eq!(e.chapter, c);
        }
    });
    let mut t = 0;
    while r.step(t) {
        t += 16;
        assert!(t < 16 * 2_000_000, "the chapter changes did not end");
    }
    for &c in chain.iter().skip(1) {
        let name = format!("pa{c}.gam");
        let cp = store.read(&name).unwrap_or_else(|| panic!("{name}"));
        let cp = Checkpoint::parse(&cp).expect("a checkpoint");
        assert_eq!(u32::from(cp.chapter), c, "{name}");
    }
    let file = store.read(PLAYERS_FILE).unwrap();
    assert_eq!(file.len(), PLAYERS_SIZE_ZH);
}

/// Player 0's resume chapter in `store`'s players.atl set to `chapter`,
/// then from the start-up the player's row chosen on the player screen:
/// its checkpoint loaded, the films and lines that follow clicked through
/// to a place, 400 frames played there. Returns the chapters loaded (the
/// story may move on to the next one while the lines are clicked through).
fn resume(g: Game, store: &Rc<MemStore>, chapter: u32) -> Vec<u32> {
    let marker = g.exe.new_player_marker.clone();
    let mut players =
        Players::read_rows(&store.read(PLAYERS_FILE).unwrap(), &marker, false).unwrap();
    players.layout_1997 = true;
    players.chapters[0] = chapter as u8;
    store.write(PLAYERS_FILE, &players.to_bytes());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        ..Options::default()
    };
    let mut h = Headless::new(g, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    h.click(200, 259);
    h.click(200, 259);
    let want = format!("pa{chapter}.gam");
    assert!(
        h.until(3000, |_| trace
            .borrow()
            .iter()
            .any(|e| matches!(e, Event::CheckpointLoaded(n) if *n == want))),
        "{want} not loaded"
    );
    // In a place (the warp view holds the mouse), clicking through the
    // films and lines that play first.
    let mut in_place = false;
    for _ in 0..400 {
        h.steps(30);
        if h.sys.mouse_locked() {
            in_place = true;
            break;
        }
        h.click(320, 240);
    }
    assert!(in_place, "chapter {chapter}: no place after the load");
    h.steps(400);
    assert!(h.alive, "chapter {chapter}: the game ended");
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
    let loaded: Vec<u32> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::ChapterLoaded(c) => Some(*c),
            _ => None,
        })
        .collect();
    loaded
}

/// The whole story from checkpoints the original wrote: every
/// `pa<N>.gam` of a folder of saves (`ATLANTIS_SAVES_PTBR`, e.g. a save
/// set with one checkpoint per chapter), in chapter order, resumed from
/// the player screen as a returning player resumes it: its checkpoint
/// loaded, the films and lines that follow clicked through to a place,
/// 400 frames played there.
#[test]
fn every_chapter_resumed() {
    let Some(dir) = std::env::var_os("ATLANTIS_SAVES_PTBR") else {
        eprintln!("ATLANTIS_SAVES_PTBR not set: every_chapter_resumed skipped");
        return;
    };
    let Some(g) = game("every_chapter_resumed") else {
        return;
    };
    let mut chapters: Vec<(u32, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            let n = name
                .strip_prefix("pa")?
                .strip_suffix(".gam")?
                .parse()
                .ok()?;
            Some((n, std::fs::read(e.path()).ok()?))
        })
        .collect();
    chapters.sort();
    assert!(!chapters.is_empty(), "no pa<N>.gam in {dir:?}");
    let reached: BTreeSet<u32> = chapter_chain(&g).into_iter().collect();
    let marker = g.exe.new_player_marker.clone();
    for (c, cp) in &chapters {
        assert!(reached.contains(c), "chapter {c} has no CON");
        assert_eq!(u32::from(Checkpoint::parse(cp).unwrap().chapter), *c);
        let store = Rc::new(MemStore::new());
        let mut players = Players::new(&marker);
        players.layout_1997 = true;
        players.sixth_row = Some([0; 17]);
        players.create(b"Ana").unwrap();
        store.write(PLAYERS_FILE, &players.to_bytes());
        store.write(&format!("pa{c}.gam"), cp);
        let start = std::time::Instant::now();
        let g = game("every_chapter_resumed").unwrap();
        let got = resume(g, &store, *c);
        eprintln!(
            "chapter {c}: resumed (chapters loaded {got:?}) in {:.1} s",
            start.elapsed().as_secs_f32()
        );
        assert_eq!(got.first(), Some(c));
    }
    eprintln!("{} chapters resumed", chapters.len());
}

/// A checkpoint of chapter 2 for the player "Ana" in `store`, as the
/// chapter change writes it (`Chapter=2`: the checkpoint, then
/// players.atl).
fn checkpoint_of_chapter_2(g: Game, store: Rc<MemStore>) {
    let sys = Sys::new(Mixer::new());
    let s2 = sys.clone();
    let mut r = Runner::new(sys.clone(), async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.saves = store;
        let marker = e.game.exe.new_player_marker.clone();
        e.players = Players::new(&marker);
        e.players.layout_1997 = e.game.exe.build.players_1997;
        e.players.sixth_row = Some([0; 17]);
        e.player = e.players.create(b"Ana").expect("a row");
        let mut scripts = Scripts::default();
        e.load_checkpoint(&mut scripts, false).await;
        scripts.exec_set(&mut e, None, "chapter=2").await;
    });
    let mut t = 0;
    while r.step(t) {
        t += 16;
        assert!(t < 16 * 200_000, "the chapter change did not end");
    }
}

/// The centre of record `n` of a menu as the build walks it.
fn record_centre(g: &Game, file: &str, n: usize) -> (i32, i32) {
    let text = g.big.read(&format!("SPRLIST\\{file}")).unwrap();
    let mut walk = MenuWalk::default();
    let rules = MenuRules::of_build(g.exe.build, &BugFixes::NONE);
    let number = menu_number(file).unwrap() as i32;
    walk_menu_text(&text, number, -1, 0, 0, rules, &mut walk);
    let r = walk.records[n];
    (
        i32::from(r.x) + i32::from(r.w) / 2,
        i32::from(r.y) + i32::from(r.h) / 2,
    )
}

/// A save and its load: the checkpoint of a chapter change (players.atl of
/// 114 bytes), the player chosen on the player screen (its checkpoint
/// loaded), then the in-game menu's LOAD list with that checkpoint and its
/// load.
#[test]
fn save_and_load() {
    let Some(g) = game("save_and_load") else {
        return;
    };
    let Some(g2) = game("save_and_load") else {
        return;
    };
    let Some(probe) = game("save_and_load") else {
        return;
    };
    let store = Rc::new(MemStore::new());
    checkpoint_of_chapter_2(g, store.clone());
    assert!(store.read("pa2.gam").is_some(), "{:?}", store.list());
    assert_eq!(
        store.read(PLAYERS_FILE).map(|p| p.len()),
        Some(PLAYERS_SIZE_ZH)
    );
    let trace = Rc::new(RefCell::new(Vec::new()));
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        start: Start::Boot,
        ..Options::default()
    };
    let mut h = Headless::new(g2, opts);
    assert!(to_player_screen(&mut h, &trace), "no player screen");
    h.steps(10);
    let loaded = |t: &Rc<RefCell<Vec<Event>>>| {
        t.borrow()
            .iter()
            .filter(|e| matches!(e, Event::CheckpointLoaded(n) if n == "pa2.gam"))
            .count()
    };
    // The player's row, a double click: its checkpoint.
    h.click(200, 259);
    h.click(200, 259);
    assert!(
        h.until(2000, |_| loaded(&trace) > 0),
        "pa2.gam not loaded from the player screen"
    );
    // Escape in the place: the in-game menu; LOAD: the list with the
    // checkpoint; a double click on its row loads it again.
    let mut menu = false;
    for _ in 0..400 {
        h.steps(30);
        if h.sys.mouse_locked() {
            h.key(Key::Escape);
            if h.until(10, |_| menus(&trace, "MAINMENU.TXT") > 0) {
                menu = true;
                break;
            }
        } else {
            h.click(320, 240);
        }
    }
    assert!(menu, "the in-game menu did not open");
    let load = record_centre(&probe, "MAINMENU.TXT", 0);
    h.click(load.0, load.1);
    assert!(
        h.until(30, |_| menus(&trace, "LOADMENU.TXT") > 0),
        "no LOAD list"
    );
    h.sys.push_input(Input::MouseMove { x: 639, y: 0 });
    h.steps(10);
    let row = (200, 0x6e + 11);
    h.click(row.0, row.1);
    h.click(row.0, row.1);
    assert!(
        h.until(2000, |_| loaded(&trace) > 1),
        "pa2.gam not loaded from the LOAD list"
    );
    h.sys.push_input(Input::Quit);
    assert!(h.until(600, |hh| !hh.alive), "the game did not end");
}
