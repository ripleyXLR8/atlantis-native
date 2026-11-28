//! The German CD (4 CDs, 1999; also in the compilation *Die Legende
//! Atlantis I & II*, 2000): the French CD's executable inside a copy
//! protection wrapper (the same code, data tables at the same addresses),
//! German texts in Windows-1252 with the French CD's `Font.txt` and font,
//! German voices, and the German tracks of films 19, 55 and 66 in their
//! own place. The rest of the data is the French CD's.
//!
//! Needs `ATLANTIS2_DATA_DE` (the installed folder: `data1.cab` of the
//! first disc extracted) and `ATLANTIS2_DISCS_DE` (a folder holding its
//! four CD images, other images beside them ignored), only read;
//! `ATLANTIS2_DATA` / `ATLANTIS2_DISCS` (the French CD) for what the
//! edition shares with it; `ATLANTIS2_GOG_DE` (GOG.com's German) for the
//! film tracks; `ATLANTIS2_PNG_DIR`, when set, receives `de-*.png`.
//!
//! The program is only read (its size, MD5 and data tables), never run.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::dialog::{parse_dial, FontTxt};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{bytes_at, identify, Release};
use atlantis2_data::files::{disc_number, find_exe};
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::subtitle::SubtitleFont;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use atlantis_data::files::{FileSource, StdDir};
use atlantis_data::TextCodePage;
use md5::{Digest, Md5};

const ID: &str = "atlantis2-cd-de-2432347";
const LABEL: &str = "Atlantis II, German CD (1999)";
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];

fn folders() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA_DE"),
        std::env::var_os("ATLANTIS2_DISCS_DE"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA_DE / ATLANTIS2_DISCS_DE not set");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = folders()?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect("the German CD");
    assert_eq!(build.id, ID);
    Some(files)
}

/// The French CD, when its variables are set (optional here).
fn french_cd() -> Option<A2Files> {
    let dir = std::env::var_os("ATLANTIS2_DATA")?;
    let discs = std::env::var_os("ATLANTIS2_DISCS")?;
    Some(
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])
            .expect("the French CD")
            .0,
    )
}

/// `s` in Windows-1252.
fn cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            (0..=255u8)
                .find(|&b| TextCodePage::Windows1252.char_of(b) == c)
                .unwrap_or_else(|| panic!("{c} is not in Windows-1252"))
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

fn boot(files: A2Files, data: &std::path::Path) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    e.set_data_dir(Some(data.to_path_buf()));
    e
}

/// A new game, its opening film ended, standing in Tibet.
fn in_game(files: A2Files, data: &std::path::Path) -> Run {
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

/// The executable known by its MD5 alone (its PE timestamp is the French
/// CD's), the French CD's code (1.00) and data tables; the language
/// German, the texts Windows-1252; `loc\` read.
#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some((dir, _)) = folders() else { return };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(exe.len(), 2_432_347);
    assert_eq!(hex(&exe), "8550d510492f7f992a15a623bbefd322");
    let b = identify(&exe).unwrap();
    assert_eq!(b.id, ID);
    assert_eq!(
        (b.release, b.gog, b.film_subtitles_blitted),
        (Release::V100, false, false)
    );
    assert_eq!(b.version, "Atlantis 2 - Version 3.15 - 8/10/1999");
    assert_eq!(
        bytes_at(&exe, 0x005e_3128, b.version.len()),
        Some(b.version.as_bytes())
    );
    let table = bytes_at(&exe, b.place_sounds.names, 5780).unwrap();
    assert!(table[..16].contains(&0), "{:02x?}", &table[..16]);
    if let Some(fr) = french_cd() {
        // `.rdata` and `.data` byte for byte the French CD's, at the same
        // addresses: every table the port reads.
        let fr_exe = fr.read_exe().unwrap();
        assert_eq!(fr.build().place_sounds, b.place_sounds);
        for (va, len) in [(0x005c_6000, 0x1_0000), (0x005d_6000, 0x4_1000)] {
            assert_eq!(
                hex(bytes_at(&exe, va, len).unwrap()),
                hex(bytes_at(&fr_exe, va, len).unwrap()),
                "{va:#x}"
            );
        }
    }

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("atlantis2.exe"));
    assert_eq!(files.language(), Some(Language::German));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.text_code_page(), TextCodePage::Windows1252);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // `loc\menu.txt` (lower case on this CD): every section read.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert_eq!(raw.len(), 1183);
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    for line in ["Hauptmenü", "Atlantis-2-CD"] {
        let want = cp1252(line);
        assert!(raw.windows(want.len()).any(|w| w == want), "{line}");
    }
    // `Font.txt` and the font: the French CD's (1.00's heights, no
    // `[CharacterSet]`).
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    if let Some(fr) = french_cd() {
        for name in ["Font.txt", "Atl_Font.ttf"] {
            assert_eq!(
                files.read(Folder::Loc, name),
                fr.read(Folder::Loc, name),
                "{name}"
            );
        }
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
/// a glyph, and the German letters are among them.
#[test]
fn every_subtitle_and_every_letter() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1252);

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
        .map(|&b| TextCodePage::Windows1252.char_of(b))
        .collect();
    for c in "äöüß".chars() {
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
    let text = cp1252("Grüße aus Äöü");
    let mut out = vec![0u16; 640 * 480];
    font.draw(&mut out, 640, 480, &font.layout(&text, 640, 480));
    assert!(out.iter().filter(|&&p| p == 0xffff).count() > 100);
    shot(&out, 640, 480, "de-letters.png");
}

/// Films 19, 55 and 66: the discs' `apc\` tracks are the German ones
/// (GOG.com's German keeps them in `apc\dial\`), and no disc has a track
/// in `apc\dial\`, so the bug fix of the films' language finds nothing
/// to change.
#[test]
fn the_films_speak_german() {
    let Some(files) = game() else { return };
    let gog = std::env::var_os("ATLANTIS2_GOG_DE").map(|d| {
        A2Files::open_dir(&PathBuf::from(d), &[])
            .expect("GOG German")
            .0
    });
    for film in [19, 55, 66] {
        let name = format!("cine{film:03}.apc");
        let (track, _) = files.read_with_disc(Folder::Apc, &name).unwrap();
        assert!(atlantis_codecs::decode_apc(&track).is_ok(), "{name}");
        for disc in 1..=4 {
            assert_eq!(files.size_on_disc(disc, Folder::ApcDial, &name), None);
        }
        if let Some(gog) = &gog {
            assert_eq!(
                hex(&track),
                hex(&gog.read(Folder::ApcDial, &name).unwrap()),
                "{name}"
            );
        }
        if let Some(fr) = french_cd() {
            let (fr_track, _) = fr.read_with_disc(Folder::Apc, &name).unwrap();
            assert_ne!(hex(&track), hex(&fr_track), "{name}");
        }
    }
}

/// The Tibet line with the most German letters.
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

/// A new game to a Tibet dialog: a German subtitle drawn, its voice
/// present (GOG.com's German one, when set); the main menu in German; a
/// save and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let gog = std::env::var_os("ATLANTIS2_GOG_DE").map(|d| {
        A2Files::open_dir(&PathBuf::from(d), &[])
            .expect("GOG German")
            .0
    });
    let data = scratch("a2-de-game");
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
                if let (Some(gog), Some(voice)) = (&gog, &voice) {
                    assert_eq!(
                        hex(voice),
                        hex(&gog.read(Folder::ApcDial, &line.voice).unwrap()),
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
    assert!(gog.is_none() || voices > 0);
    let font = r.e.subtitle_font().unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1252);
    assert!(!font.layout(&text, 640, 480).is_empty());
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(white > 100, "{white} white pixels");
    eprintln!("{id}: {}", TextCodePage::Windows1252.decode(&text));
    shot(&r.screen, 640, 480, "de-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "de-main-menu.png");
    // A widget with a tooltip (German, from `menu.txt`): the pointer on
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
    let tip = TextCodePage::Windows1252.decode(&tip);
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
    shot(&r.screen, 640, 480, "de-main-menu-tooltip.png");
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

/// The browser's way in: the installed folder and copies of the four
/// discs, as for the French CD; every disc found, the intro and a German
/// voice read through them, the intro started.
#[test]
fn the_browser_s_way_of_opening_the_edition() {
    let Some((dir, discs)) = folders() else {
        return;
    };
    let install = StdDir::new(&dir);
    let (exe_name, build, _) = find_exe(&install).unwrap();
    assert_eq!(build.id, ID);
    // The folder's ISO images that are Atlantis II discs.
    let mut images: Vec<(u8, atlantis_data::disc_image::DiscImage)> = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&discs)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")))
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
    let voice = files.read(Folder::ApcDial, "360_01.apc").unwrap();
    assert!(atlantis_codecs::decode_apc(&voice).is_ok());
    let data = scratch("a2-de-browser");
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
