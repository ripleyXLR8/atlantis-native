//! The Czech CD (4 CDs, Bohemia Interactive, 2000; the discs read here are
//! the 2002 reissue *GAME4U Speciál 3*, Cenega Czech): the North American
//! CD's executable byte for byte (*Beyond Atlantis*, DreamCatcher,
//! 2000-04-05: 1.01's code relinked, no copy protection) and its installed
//! files but `loc\`: Czech texts in Windows-1250, a `Font.txt` that says
//! `[Language] CZE` (no branch of the program tests for it) with the
//! font GOG.com, the North American and the Polish CDs ship, which has
//! every Czech letter. On the discs: Czech voices, the Czech tracks of
//! films 19, 55 and 66 in their own place, Bohemia Interactive's logo
//! before the intro; the rest is the North American CD's.
//!
//! Needs `ATLANTIS2_DATA_CS` (the installed folder: `data1.hdr` +
//! `data1.cab` of the first disc extracted) and `ATLANTIS2_DISCS_CS` (a
//! folder holding its four CD images), only read; `ATLANTIS2_DATA_US` /
//! `ATLANTIS2_DISCS_US` (the North American CD), `ATLANTIS2_DATA_PL` /
//! `ATLANTIS2_DISCS_PL` (the Polish CD) and `ATLANTIS2_DATA` /
//! `ATLANTIS2_DISCS` (the French CD) for what the edition shares with
//! them; `ATLANTIS2_PNG_DIR`, when set, receives `cs-*.png`.
//!
//! The program is only read (its size, MD5 and data tables), never run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{parse_dial, FontTxt};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{bytes_at, identify, Release};
use atlantis2_data::files::{disc_number, find_exe};
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::engine::web::{load_model, SLOTS};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::subtitle::SubtitleFont;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use atlantis_data::disc_image::IMAGE_EXTENSIONS;
use atlantis_data::files::{FileSource, StdDir};
use atlantis_data::TextCodePage;
use md5::{Digest, Md5};

/// The North American CD's build: the Czech CD's program is that file.
const ID: &str = "atlantis2-cd-us-2199552";
const LABEL: &str = "Atlantis II, Czech CD (2000)";
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];

fn folders() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA_CS"),
        std::env::var_os("ATLANTIS2_DISCS_CS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA_CS / ATLANTIS2_DISCS_CS not set");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = folders()?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect("the Czech CD");
    assert_eq!(build.id, ID);
    Some(files)
}

/// An edition from its two variables, when they are set (optional here).
fn other(data: &str, discs: &str, what: &str) -> Option<A2Files> {
    let dir = std::env::var_os(data)?;
    let discs = std::env::var_os(discs)?;
    Some(
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])
            .expect(what)
            .0,
    )
}

fn us_cd() -> Option<A2Files> {
    other(
        "ATLANTIS2_DATA_US",
        "ATLANTIS2_DISCS_US",
        "the North American CD",
    )
}

fn polish_cd() -> Option<A2Files> {
    other("ATLANTIS2_DATA_PL", "ATLANTIS2_DISCS_PL", "the Polish CD")
}

fn french_cd() -> Option<A2Files> {
    other("ATLANTIS2_DATA", "ATLANTIS2_DISCS", "the French CD")
}

/// `s` in Windows-1250.
fn cp1250(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            (0..=255u8)
                .find(|&b| TextCodePage::Windows1250.char_of(b) == c)
                .unwrap_or_else(|| panic!("{c} is not in Windows-1250"))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn shot(screen: &[u16], w: usize, h: usize, name: &str) {
    if let Some(dir) = std::env::var_os("ATLANTIS2_PNG_DIR") {
        let png = atlantis2_engine::png::encode_rgb565(screen, w, h);
        std::fs::write(PathBuf::from(dir).join(name), png).unwrap();
    }
}

fn bytemuck_u16(s: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding and u8 no alignment.
    unsafe { std::slice::from_raw_parts(s.as_ptr().cast(), s.len() * 2) }
}

/// The files of a folder, by path relative to it.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.push(rel.replace('\\', "/"));
        }
    }
}

/// The engine at 59.94 passes a second, the clock running with them.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    fn new(e: Engine) -> Run {
        let (w, h) = e.screen_size();
        Run {
            e,
            pass: 0,
            screen: vec![0; w * h],
        }
    }

    fn step(&mut self) -> Vec<Event> {
        self.e.set_clock(self.pass as f64 * PASS);
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        std::mem::take(&mut self.e.events)
    }
}

fn boot(files: A2Files, data: &Path) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    e.set_data_dir(Some(data.to_path_buf()));
    e
}

/// A new game, its opening film ended, standing in Tibet.
fn in_game(files: A2Files, data: &Path) -> Run {
    let mut e = boot(files, data);
    e.start_new_game().unwrap();
    e.end_video();
    let mut r = Run::new(e);
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    r
}

/// Commands run in a new game, up to the panorama they lead to.
fn at(files: A2Files, data: &Path, commands: &[&[u8]]) -> Run {
    let mut r = in_game(files, data);
    for c in commands {
        r.e.run_command(c);
    }
    for _ in 0..200 {
        match r.e.loop_type {
            LoopType::Video => r.e.end_video(),
            LoopType::Dialog => r.e.end_dialog(),
            _ => {}
        }
        r.step();
        if r.e.loop_type == LoopType::Warp {
            break;
        }
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    r
}

/// The executable known by its MD5: the North American CD's file, so its
/// build, code and tables; the language Czech (told by `Menu.txt`), the
/// texts Windows-1250, the edition named after its own release; the
/// installed files the North American CD's but `loc\`, whose lines and
/// objects are that CD's, translated.
#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some((dir, _)) = folders() else { return };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(exe.len(), 2_199_552);
    assert_eq!(hex(&exe), "0f634c4620680dfea6782645c79dc84c");
    let b = identify(&exe).unwrap();
    assert_eq!(b.id, ID);
    assert_eq!(
        (b.release, b.gog, b.film_subtitles_blitted),
        (Release::V101, false, true)
    );
    assert_eq!(b.version, "Atlantis 2 - Version 3.18 - 8/10/1999");
    assert_eq!(
        bytes_at(&exe, 0x005e_31d8, b.version.len()),
        Some(b.version.as_bytes())
    );
    let table = bytes_at(&exe, b.place_sounds.names, 5780).unwrap();
    assert!(table[..16].contains(&0), "{:02x?}", &table[..16]);

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("atlantis2.exe"));
    assert_eq!(files.language(), Some(Language::Czech));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.text_code_page(), TextCodePage::Windows1250);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // `loc\Menu.txt`: every section read, in Czech.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert_eq!(raw.len(), 1207);
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    for line in ["Hlavní menu", "Hráč 1"] {
        let want = cp1250(line);
        assert!(raw.windows(want.len()).any(|w| w == want), "{line}");
    }
    // `Font.txt`: `[Language] CZE`, no `[CharacterSet]` (SBCS), 1.01's
    // heights.
    let font_raw = files.read(Folder::Loc, "Font.txt").unwrap();
    assert!(font_raw.windows(14).any(|w| w == b"[Language]\tCZE"));
    let font_txt = FontTxt::parse(&font_raw);
    assert_eq!(font_txt.mbcs, None);
    let heights: Vec<i32> = font_txt.fonts.iter().map(|f| f.height).collect();
    assert_eq!(heights, [12, 22, 20, 16]);

    if let Some((us_dir, us)) = std::env::var_os("ATLANTIS2_DATA_US").zip(us_cd()) {
        let us_dir = PathBuf::from(us_dir);
        // The program and every installed file but `loc\`'s texts: the
        // North American CD's, byte for byte.
        let us_exe = std::fs::read(us_dir.join("atlantis2.exe")).unwrap();
        assert_eq!(hex(&exe), hex(&us_exe));
        let mut names = Vec::new();
        walk(&dir, &dir, &mut names);
        assert_eq!(names.len(), 249);
        let us_install = StdDir::new(&us_dir);
        let mut same = 0;
        for name in &names {
            let ours = std::fs::read(dir.join(name)).unwrap();
            let theirs = us_install.read(name).unwrap_or_else(|| panic!("{name}"));
            if name.to_ascii_lowercase().starts_with("loc/")
                && !name.eq_ignore_ascii_case("loc/Atl_Font.ttf")
            {
                assert_ne!(hex(&ours), hex(&theirs), "{name}");
            } else {
                assert_eq!(hex(&ours), hex(&theirs), "{name}");
                same += 1;
            }
        }
        assert_eq!(same, 240);
        // `Font.txt`: the same fonts.
        let us_fonts = FontTxt::parse(&us.read(Folder::Loc, "Font.txt").unwrap());
        assert_eq!(font_txt.fonts, us_fonts.fonts);
        // The texts: the North American CD's lines in the same order;
        // the speakers' names too, but 20 of Ireland's (071_04 to
        // 076_06), which the Czech file gives to others. The speaker the
        // game uses is `scenar\comIR.txt`'s, the same file in both.
        for code in WORLDS {
            let name = format!("Dial{code}.txt");
            let rows = |f: &A2Files| -> Vec<(String, String)> {
                parse_dial(&f.read(Folder::Loc, &name).unwrap())
                    .into_iter()
                    .map(|d| (d.id, d.speaker))
                    .collect()
            };
            let (cs, en) = (rows(&files), rows(&us));
            let ids = |r: &[(String, String)]| r.iter().map(|(i, _)| i.clone()).collect::<Vec<_>>();
            assert_eq!(ids(&cs), ids(&en), "{name}");
            let differ: Vec<&str> = cs
                .iter()
                .zip(&en)
                .filter(|(a, b)| a.1 != b.1)
                .map(|(a, _)| a.0.as_str())
                .collect();
            if code == "IR" {
                assert_eq!(differ.len(), 20, "{differ:?}");
                assert!(differ.iter().all(|i| ("071_04"..="076_06").contains(i)));
            } else {
                assert!(differ.is_empty(), "{name}: {differ:?}");
            }
        }
        assert_eq!(
            files.read(Folder::Scenar, "comIR.txt"),
            us.read(Folder::Scenar, "comIR.txt")
        );
        let us_menu = MenuTxt::parse(&us.read(Folder::Loc, "Menu.txt").unwrap());
        assert_eq!(menu.sections.len(), us_menu.sections.len());
        for (a, e) in menu.sections.iter().zip(&us_menu.sections) {
            assert_eq!(a.len(), e.len());
        }
        let keys = |f: &A2Files| -> Vec<Vec<u8>> {
            ObjectNames::parse(&f.read(Folder::Loc, "Messages.txt").unwrap())
                .entries()
                .map(|(k, _)| k.to_vec())
                .collect()
        };
        assert_eq!(keys(&files), keys(&us));
    }
    if let Some(pl) = polish_cd() {
        // The font: the Polish CD's file too (GOG.com's).
        assert_eq!(
            files.read(Folder::Loc, "Atl_Font.ttf"),
            pl.read(Folder::Loc, "Atl_Font.ttf")
        );
    }
    // `Messages.txt`: every object name is a `defnames.con` object.
    let names = ObjectNames::parse(&files.read(Folder::Loc, "Messages.txt").unwrap());
    let dn = DefNames::parse(&files.read(Folder::Scenar, "defnames.con").unwrap()).unwrap();
    assert!(names.len() > 50, "{}", names.len());
    for (key, name) in names.entries() {
        assert!(dn.id(defnames::OBJETS, key).is_ok(), "{key:?} {name:?}");
    }
}

/// Every subtitle laid out within the screen, every letter of `loc\` has
/// a glyph in Windows-1250, and the Czech letters are among them.
#[test]
fn every_subtitle_and_every_letter() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1250);

    let (mut texts, mut used) = (0, BTreeSet::new());
    for code in WORLDS {
        let data = files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap();
        for d in parse_dial(&data) {
            let calls = font.layout(&d.text, 640, 480);
            if !d.text.is_empty() {
                assert!(!calls.is_empty(), "{code}.{}", d.id);
            }
            for c in &calls {
                let (w, _) = font.extent(&c.bytes);
                assert!(c.x >= 0 && c.x + w <= 640, "{code}.{}: {c:?}", d.id);
            }
            used.extend(d.text.iter().copied().filter(|&b| b >= 0x20));
            texts += 1;
        }
    }
    assert!(texts > 1000, "{texts}");
    let accented: String = used
        .iter()
        .filter(|&&b| b >= 0x80)
        .map(|&b| TextCodePage::Windows1250.char_of(b))
        .collect();
    for c in "áčďéěíňřšťúůýžČŘŠŽ".chars() {
        assert!(accented.contains(c), "{c} not in {accented}");
    }

    let mut all: BTreeSet<u8> = used;
    for name in ["Menu.txt", "Messages.txt", "Credits.txt"] {
        let data = files.read(Folder::Loc, name).unwrap();
        all.extend(data.iter().copied().filter(|&b| b >= 0x20));
    }
    let missing: Vec<u8> = all
        .iter()
        .copied()
        .filter(|&b| !font.has_glyph(b))
        .collect();
    assert!(missing.is_empty(), "no glyph for {missing:02x?}");
    // The letters a Western code page would show otherwise (0xe8 `è`, 0xf8
    // `ø`, 0xec `ì`, 0xf9 `ù`, 0xef `ï`) drawn as the Czech ones.
    let text = cp1250("Č Ř Ě Ů Ď č ř ě ů ď ť ň š ž");
    assert_eq!(TextCodePage::Windows1252.decode(&cp1250("čřěůď")), "èøìùï");
    let mut out = vec![0u16; 640 * 480];
    font.draw(&mut out, 640, 480, &font.layout(&text, 640, 480));
    assert!(out.iter().filter(|&&p| p == 0xffff).count() > 100);
    shot(&out, 640, 480, "cs-letters.png");
}

/// Films 19, 55 and 66: the discs' `apc\` tracks are the Czech ones
/// (neither the French, the Polish nor the North American CD's), and no
/// disc has a track in `apc\dial\`, so the bug fix of the films' language
/// finds nothing to change.
#[test]
fn the_films_speak_czech() {
    let Some(files) = game() else { return };
    let others: Vec<A2Files> = [french_cd(), polish_cd(), us_cd()]
        .into_iter()
        .flatten()
        .collect();
    for film in [19, 55, 66] {
        let name = format!("cine{film:03}.apc");
        let (track, _) = files.read_with_disc(Folder::Apc, &name).unwrap();
        assert!(atlantis_codecs::decode_apc(&track).is_ok(), "{name}");
        for disc in 1..=4 {
            assert_eq!(files.size_on_disc(disc, Folder::ApcDial, &name), None);
        }
        for other in &others {
            let (theirs, _) = other.read_with_disc(Folder::Apc, &name).unwrap();
            assert_ne!(hex(&track), hex(&theirs), "{name}");
        }
    }
}

/// The spider's models: the North American CD's files (textures named
/// without accents) read, every texture found, and the web drawn as on
/// the North American CD.
#[test]
fn the_spider_finds_its_textures() {
    let Some(files) = game() else { return };
    let us = us_cd();
    let mut materials = 0;
    for slot in SLOTS {
        let read = |p: &str| files.read(Folder::ThreeD, p);
        assert!(load_model(Fpu::Chop, slot, read).is_some(), "{slot:?}");
        let path = format!("pz_araig/{}", slot.file());
        let db = files.read(Folder::ThreeD, &path).unwrap();
        if let Some(us) = &us {
            assert_eq!(hex(&db), hex(&us.read(Folder::ThreeD, &path).unwrap()));
        }
        for t in Scene::parse(&db)
            .unwrap()
            .materials
            .iter()
            .filter_map(|m| m.texture().map(str::to_string))
        {
            assert!(t.is_ascii(), "{slot:?}: {t}");
            assert!(
                files
                    .read(Folder::ThreeD, &format!("pz_araig/{t}"))
                    .is_some(),
                "{slot:?}: {t}"
            );
            materials += 1;
        }
    }
    assert!(materials >= 7, "{materials}");

    let web = [&b"(monde=6)"[..], b"(changewam=toile.wam)", b"(vue=198)"];
    let picture = |files: A2Files, name: &str| {
        let data = scratch(name);
        let mut r = at(files, &data, &web);
        r.e.enter_at(199, 0.0, 0.0).unwrap();
        for _ in 0..60 {
            r.step();
        }
        let _ = std::fs::remove_dir_all(&data);
        r.screen
    };
    let cs = picture(game().unwrap(), "a2-cs-web");
    let colours: BTreeSet<u16> = cs.iter().copied().collect();
    assert!(colours.len() > 200, "{} colours", colours.len());
    shot(&cs, 640, 480, "cs-web.png");
    if let Some(us) = us {
        assert_eq!(
            hex(bytemuck_u16(&cs)),
            hex(bytemuck_u16(&picture(us, "a2-cs-web-us")))
        );
    }
}

/// The intro on disc 1: Bohemia Interactive's logo (90 frames, no track;
/// in the place of the North American CD's DreamCatcher logo), then the two
/// films with their tracks, then the player menu. The program plays
/// `intro%02d` as 1.01: `CZE` is none of the four languages its intro
/// branch knows (`dut`, `esp`, `ita`, `swe`).
#[test]
fn the_intro_plays_the_logo_and_the_two_films() {
    let Some(files) = game() else { return };
    assert!(files.read_current(Folder::Apc, "intro00.apc").is_none());
    let logo_file = files.read_current(Folder::Hnm, "intro00.hnm").unwrap();
    assert_eq!(&logo_file[..4], b"HNM6");
    if let Some(us) = us_cd() {
        assert_ne!(
            hex(&logo_file),
            hex(&us.read_current(Folder::Hnm, "intro00.hnm").unwrap())
        );
    }
    let data = scratch("a2-cs-intro");
    let mut e = boot(files, &data);
    e.open_player_menu();
    e.start_intro();
    let mut r = Run::new(e);
    assert_eq!(r.e.video_playing().map(|(n, _)| n), Some("intro00.hnm"));
    let mut journal = std::mem::take(&mut r.e.events);
    let mut logo = false;
    for _ in 0..60 * 60 * 5 {
        if r.e.loop_type == LoopType::Menu {
            break;
        }
        journal.extend(r.step());
        if !logo
            && r.e
                .video_playing()
                .is_some_and(|(n, k)| n == "intro00.hnm" && k == 60)
        {
            let colours: BTreeSet<u16> = r.screen.iter().copied().collect();
            assert!(colours.len() > 20, "the logo: {} colours", colours.len());
            shot(&r.screen, 640, 480, "cs-logo.png");
            logo = true;
        }
    }
    assert!(logo);
    assert_eq!(r.e.loop_type, LoopType::Menu);
    assert!(!r.e.intro_playing());
    let mut videos: Vec<(String, Option<u32>, u32)> = Vec::new();
    let mut sounds = Vec::new();
    for ev in &journal {
        match ev {
            Event::VideoFrame(_) => videos.last_mut().unwrap().2 += 1,
            Event::VideoStart { name, frames } => videos.push((name.clone(), *frames, 0)),
            Event::Sound(s) => sounds.push(s.clone()),
            _ => {}
        }
    }
    let names: Vec<&str> = videos.iter().map(|v| v.0.as_str()).collect();
    assert_eq!(
        names,
        ["intro00.hnm", "intro01.hnm", "intro02.hnm", "intro03.hnm"],
        "{videos:?}"
    );
    assert_eq!(videos[0].1, Some(90), "{videos:?}");
    assert_eq!(videos[3].1, None);
    assert_eq!(sounds, ["intro01.apc", "intro02.apc"]);
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
    let _ = std::fs::remove_dir_all(&data);
}

/// The Tibet line with the most Czech letters.
fn showcase(dial: &[u8]) -> (u32, String, Vec<u8>) {
    let d = parse_dial(dial)
        .into_iter()
        .max_by_key(|d| {
            let wide: BTreeSet<u8> = d.text.iter().copied().filter(|&b| b >= 0x80).collect();
            (wide.len(), d.text.len())
        })
        .unwrap();
    (d.numbers().unwrap().0, d.id, d.text)
}

/// A new game to a Tibet dialog: a Czech subtitle drawn, its voice
/// present (not the North American CD's, when set); the main menu in
/// Czech; a save and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let us = us_cd();
    let data = scratch("a2-cs-game");
    let mut r = in_game(files, &data);
    r.e.enter_view(7, (-164.394f32).to_radians(), 14.96f32.to_radians())
        .unwrap();
    for _ in 0..200 {
        match r.e.loop_type {
            LoopType::Video => r.e.end_video(),
            LoopType::Dialog => r.e.end_dialog(),
            _ => {}
        }
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert!(r.e.save_slot(3));

    let dial = r.e.files().read(Folder::Loc, "DialTI.txt").unwrap();
    let (n, id, text) = showcase(&dial);
    assert!(text.iter().any(|&b| b >= 0x80));
    r.e.start_dialog(n).unwrap();
    let (mut shown, mut other, mut voices) = (None, None, 0);
    for _ in 0..60 * 120 {
        let pass = r.pass;
        for ev in r.step() {
            if let Event::DialogLine(l) = ev {
                let line = r.e.dialog().unwrap().current_line().unwrap();
                let voice = r.e.files().read(Folder::ApcDial, &line.voice);
                assert!(voice.is_some(), "{l}: no voice");
                if let (Some(us), Some(voice)) = (&us, &voice) {
                    assert_ne!(
                        hex(voice),
                        hex(&us.read(Folder::ApcDial, &line.voice).unwrap()),
                        "{l}"
                    );
                    voices += 1;
                }
                if l == id {
                    shown = Some(pass);
                } else {
                    other = Some(pass);
                }
            }
        }
        if shown.is_none() && other.is_some_and(|p| pass == p + 30) {
            r.e.click(pass as f64 * PASS);
        }
        if shown.is_some_and(|p| pass == p + 30) {
            break;
        }
    }
    assert!(shown.is_some(), "{id} not shown");
    assert!(us.is_none() || voices > 0);
    let font = r.e.subtitle_font().unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1250);
    assert!(!font.layout(&text, 640, 480).is_empty());
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(white > 100, "{white} white pixels");
    eprintln!("{id}: {}", TextCodePage::Windows1250.decode(&text));
    shot(&r.screen, 640, 480, "cs-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "cs-main-menu.png");
    // A widget with a tooltip (Czech, from `Menu.txt`): the pointer on
    // it, its text drawn.
    let bare = r.screen.clone();
    let (cx, cy, tip) = {
        let page = r.e.menu().unwrap().page.as_ref().unwrap();
        let w = page
            .widgets
            .iter()
            .find(|w| w.enabled && w.visible && !w.text.is_empty())
            .expect("a tooltip");
        (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
            w.text.clone(),
        )
    };
    let menu_txt = r.e.files().read(Folder::Loc, "Menu.txt").unwrap();
    assert!(menu_txt.windows(tip.len()).any(|w| w == tip));
    let tip = TextCodePage::Windows1250.decode(&tip);
    r.e.pointer(cx, cy);
    for _ in 0..120 {
        r.step();
    }
    assert_ne!(
        hex(bytemuck_u16(&bare)),
        hex(bytemuck_u16(&r.screen)),
        "{tip}"
    );
    eprintln!("tooltip: {tip}");
    shot(&r.screen, 640, 480, "cs-main-menu-tooltip.png");
    r.e.escape();
    for _ in 0..10 {
        r.step();
    }
    r.e.load_slot(3).unwrap();
    for _ in 0..5 {
        r.step();
    }
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert!(data.join("save").is_dir());
    let _ = std::fs::remove_dir_all(&data);
}

/// A copy of a disc as the browser sees it (`atlantis_app`'s `PickedDir`):
/// files by lower-case path, no folders in a listing.
struct PickedCopy(Arc<dyn FileSource>);

impl FileSource for PickedCopy {
    fn size(&self, name: &str) -> Option<u64> {
        self.0.size(name)
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.0.read_at(name, offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.0.has_dir(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        let d = dir.trim_end_matches(['/', '\\']);
        self.0
            .list(dir)
            .into_iter()
            .filter(|n| !self.0.has_dir(&format!("{d}/{n}")))
            .collect()
    }
}

/// The browser's way in: the installed folder and the four raw `.bin`
/// images (Mode 2, 2352-byte sectors; the `.cue` sheets beside them are
/// not needed), as for the other CDs; every disc found, the intro, a
/// spider model and a Czech voice read through them, the intro started.
#[test]
fn the_browser_s_way_of_opening_the_edition() {
    let Some((dir, discs)) = folders() else {
        return;
    };
    let install = StdDir::new(&dir);
    let (exe_name, build, _) = find_exe(&install).unwrap();
    assert_eq!(build.id, ID);
    // The folder's images that are Atlantis II discs.
    let mut images: Vec<(u8, atlantis_data::disc_image::DiscImage)> = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&discs)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| {
                IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
            })
        })
        .collect();
    paths.sort();
    for p in paths {
        let image = atlantis_data::disc_image::DiscImage::open(&p).unwrap();
        let n = disc_number(&image);
        if n > 0 {
            images.push((n, image));
        }
    }
    let numbers: Vec<u8> = images.iter().map(|(n, _)| *n).collect();
    assert_eq!(numbers, [1, 2, 3, 4]);
    let copies: Vec<(String, Arc<dyn FileSource>)> = images
        .into_iter()
        .map(|(n, image)| {
            (
                format!("CD{n}/"),
                Arc::new(PickedCopy(Arc::new(image))) as Arc<dyn FileSource>,
            )
        })
        .collect();
    let mut files = A2Files::new(Arc::new(install), copies);
    files.set_exe(exe_name, build);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
    assert_eq!(files.label(), LABEL);
    assert!(files.read_current(Folder::Hnm, "intro00.hnm").is_some());
    assert!(files.read(Folder::ThreeD, "pz_araig/armar.db").is_some());
    let voice = files.read(Folder::ApcDial, "360_01.apc").unwrap();
    assert!(atlantis_codecs::decode_apc(&voice).is_ok());
    let data = scratch("a2-cs-browser");
    let mut e = boot(files, &data);
    e.open_player_menu();
    e.start_intro();
    let mut r = Run::new(e);
    assert_eq!(r.e.video_playing().map(|(n, _)| n), Some("intro00.hnm"));
    for _ in 0..30 {
        r.step();
    }
    let _ = std::fs::remove_dir_all(&data);
}
