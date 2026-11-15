//! The Polish CD (4 CDs, 1999): its own executable (1.01's code under the
//! French CD's version string 3.15), Polish texts in Windows-1250 with
//! GOG.com's `Atl_Font.ttf`, dubbed voices, and spider textures renamed
//! without accents. The rest of the data is the French CD's.
//!
//! Needs `ATLANTIS2_DATA_PL` (the installed folder: `data1.cab` of the
//! first disc extracted) and `ATLANTIS2_DISCS_PL` (a folder of its four
//! CD images or copies), only read; `ATLANTIS2_DATA` / `ATLANTIS2_DISCS`
//! (the French CD) for the spider's picture compared;
//! `ATLANTIS2_PNG_DIR`, when set, receives `pl-*.png`.
//!
//! No golden of the original's layout is kept for these texts: the
//! goldens' widths were measured on a Western Windows (code page 1252),
//! which reads this edition's bytes as other letters
//! (atlantis2-re `binaries/atlantis2-cd-pl-2183168.md`, "Text code page").

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{parse_dial, FontTxt};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{bytes_at, identify, Release};
use atlantis2_data::files::find_exe;
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::engine::web::{load_model, SLOTS};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::subtitle::SubtitleFont;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use atlantis_data::files::{FileSource, StdDir};
use atlantis_data::TextCodePage;
use md5::{Digest, Md5};

const ID: &str = "atlantis2-cd-pl-2183168";
const LABEL: &str = "Atlantis II, Polish CD (1999)";
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];
/// The 18 Polish letters, lower then upper case.
const POLISH: &str = "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ";

fn folders() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA_PL"),
        std::env::var_os("ATLANTIS2_DISCS_PL"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA_PL / ATLANTIS2_DISCS_PL not set");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = folders()?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect("the Polish CD");
    assert_eq!(build.id, ID);
    Some(files)
}

fn french_cd() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    Some(
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])
            .expect("the French CD")
            .0,
    )
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

/// Commands run in a new game, up to the panorama they lead to.
fn at(files: A2Files, data: &std::path::Path, commands: &[&[u8]]) -> Run {
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

/// The executable known by its MD5, its build 1.01's code without GOG's
/// patches; the language Polish, the texts Windows-1250; `loc\` read.
#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some((dir, _)) = folders() else { return };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(hex(&exe), "f2b6f7bfa3c1952dceddfa1a662ebb84");
    let b = identify(&exe).unwrap();
    assert_eq!(b.id, ID);
    assert_eq!(
        (b.release, b.gog, b.film_subtitles_blitted),
        (Release::V101, false, false)
    );
    assert_eq!(b.version, "Atlantis 2 - Version 3.15 - 8/10/1999");
    assert!(exe
        .windows(b.version.len())
        .any(|w| w == b.version.as_bytes()));
    // The listener table at 1.01's address: the French CD's 5780 bytes.
    let table = bytes_at(&exe, b.place_sounds.names, 5780).unwrap();
    assert!(table[..16].contains(&0), "{:02x?}", &table[..16]);
    if let Some(fr) = french_cd() {
        let fr_exe = fr.read_exe().unwrap();
        let t = fr.build().place_sounds;
        assert_eq!(hex(table), hex(bytes_at(&fr_exe, t.names, 5780).unwrap()));
    }

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("atlantis2.exe"));
    assert_eq!(files.language(), Some(Language::Polish));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.text_code_page(), TextCodePage::Windows1250);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // `loc\MENU.TXT` (upper case on this CD): every section read.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert_eq!(raw.len(), 1106);
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    for line in [
        "Menu główne",
        "Wyjście z gry",
        "Opcje dźwięku",
        "Wczytaj grę",
    ] {
        let want = cp1250(line);
        assert!(raw.windows(want.len()).any(|w| w == want), "{line}");
    }
    // `Font.txt`: no `[CharacterSet]` (SBCS), GOG's heights.
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    assert_eq!(font_txt.breaks, [0x20]);
    let heights: Vec<i32> = font_txt.fonts.iter().map(|f| f.height).collect();
    assert_eq!(heights, [12, 22, 20, 16]);
    // GOG's font file.
    let ttf = files.read(Folder::Loc, "Atl_Font.ttf").unwrap();
    assert_eq!(hex(&ttf), "0623dfbee11d3d55eb6fc71f8dd6304b");
    // `Messages.txt`: every object name is a `defnames.con` object.
    let names = ObjectNames::parse(&files.read(Folder::Loc, "Messages.txt").unwrap());
    let dn = DefNames::parse(&files.read(Folder::Scenar, "defnames.con").unwrap()).unwrap();
    assert!(names.len() > 50, "{}", names.len());
    for (key, name) in names.entries() {
        assert!(dn.id(defnames::OBJETS, key).is_ok(), "{key:?} {name:?}");
    }
}

/// The ASCII letter under a Polish one.
fn base_of(c: char) -> Option<u8> {
    const FROM: &str = "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ";
    const TO: &[u8] = b"acelnoszzACELNOSZZ";
    FROM.chars().position(|f| f == c).map(|i| TO[i])
}

/// Every subtitle laid out within the screen; every letter of `loc\`
/// drawn: Windows-1250 bytes, the Polish letters in the dialogs, each a
/// glyph of its own, unlike its base letter and unlike the Windows-1252
/// reading of its byte.
#[test]
fn every_subtitle_and_every_letter() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1250);
    assert_eq!(font.line_step(), 22);
    let ttf = files.read(Folder::Loc, "Atl_Font.ttf").unwrap();
    let western = SubtitleFont::named_in(
        ttf,
        &font_txt,
        "AtlFT_Dialog",
        Release::V101,
        TextCodePage::Windows1252,
    )
    .unwrap();

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
    assert_eq!(accented, "ŚśźŻłążćęńó");

    let mut others = BTreeSet::new();
    for name in ["Menu.txt", "Messages.txt", "Credits.txt"] {
        let data = files.read(Folder::Loc, name).unwrap();
        others.extend(data.iter().copied().filter(|&b| b >= 0x80));
    }
    let px = |f: &SubtitleFont, text: &[u8]| {
        let mut out = vec![0u16; 640 * 480];
        f.draw(&mut out, 640, 480, &f.layout(text, 640, 480));
        out
    };
    let all: BTreeSet<u8> = used.iter().chain(&others).copied().collect();
    let missing: Vec<u8> = all
        .iter()
        .copied()
        .filter(|&b| !font.has_glyph(b))
        .collect();
    assert!(missing.is_empty(), "no glyph for {missing:02x?}");
    for &b in all.iter().filter(|&&b| b >= 0x80) {
        let drawn = px(&font, &[b]);
        assert!(
            drawn.iter().filter(|&&p| p == 0xffff).count() > 3,
            "{b:02x}"
        );
        let c = TextCodePage::Windows1250.char_of(b);
        if let Some(base) = base_of(c) {
            assert_ne!(drawn, px(&font, &[base]), "{c} drawn as {}", base as char);
        }
        if c != TextCodePage::Windows1252.char_of(b) {
            assert_ne!(drawn, px(&western, &[b]), "{c} drawn as in Windows-1252");
        }
    }
}

/// A Polish string with every Polish letter drawn by the dialog, the
/// popup (menus) and the big (messages) fonts: no missing-glyph box.
#[test]
fn a_polish_string_in_every_font() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let text = cp1250(&format!("Zażółć gęślą jaźń {POLISH}"));
    for id in [
        "AtlFT_Dialog",
        "AtlFT_POPUP",
        "AtlFT_Big_Default",
        "AtlFT_Default",
    ] {
        let font = SubtitleFont::load(&files, &font_txt, id).unwrap();
        assert_eq!(font.code_page(), TextCodePage::Windows1250, "{id}");
        for &b in &text {
            assert!(
                font.has_glyph(b),
                "{id}: {} ({b:02x})",
                TextCodePage::Windows1250.char_of(b)
            );
        }
        let mut out = vec![0u16; 640 * 480];
        font.draw(&mut out, 640, 480, &font.layout(&text, 640, 480));
        assert!(out.iter().filter(|&&p| p == 0xffff).count() > 100, "{id}");
        if id == "AtlFT_Dialog" {
            shot(&out, 640, 480, "pl-letters.png");
        }
    }
}

/// The spider's models: the files of CD4 rewritten to the textures'
/// names without accents read, every texture found, and the web drawn as
/// on the French CD (the same texture bytes under the accented names).
#[test]
fn the_spider_finds_its_textures() {
    let Some(files) = game() else { return };
    let mut materials = 0;
    for slot in SLOTS {
        let read = |p: &str| files.read(Folder::ThreeD, p);
        assert!(load_model(Fpu::Chop, slot, read).is_some(), "{slot:?}");
        let db = files
            .read(Folder::ThreeD, &format!("pz_araig/{}", slot.file()))
            .unwrap();
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
    let pl = picture(game().unwrap(), "a2-pl-web");
    let colours: BTreeSet<u16> = pl.iter().copied().collect();
    assert!(colours.len() > 200, "{} colours", colours.len());
    shot(&pl, 640, 480, "pl-web.png");
    if let Some(fr) = french_cd() {
        assert_eq!(
            hex(bytemuck_u16(&pl)),
            hex(bytemuck_u16(&picture(fr, "a2-pl-web-fr")))
        );
    }
}

/// The Tibet line with the most Polish letters.
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

/// A new game to a Tibet dialog: a Polish subtitle drawn, its voice
/// present; the main menu in Polish; a save and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let data = scratch("a2-pl-game");
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
    let (mut shown, mut other) = (None, None);
    for _ in 0..60 * 120 {
        let pass = r.pass;
        for ev in r.step() {
            if let Event::DialogLine(l) = ev {
                let line = r.e.dialog().unwrap().current_line().unwrap();
                assert!(
                    r.e.files().read(Folder::ApcDial, &line.voice).is_some(),
                    "{l}: no voice"
                );
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
    let font = r.e.subtitle_font().unwrap();
    assert_eq!(font.code_page(), TextCodePage::Windows1250);
    assert!(!font.layout(&text, 640, 480).is_empty());
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(white > 100, "{white} white pixels");
    shot(&r.screen, 640, 480, "pl-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "pl-main-menu.png");
    // A widget whose tooltip has a Polish letter: the pointer on it, its
    // text drawn in the popup font, Windows-1250.
    let bare = r.screen.clone();
    let (cx, cy, tip) = {
        let page = r.e.menu().unwrap().page.as_ref().unwrap();
        let w = page
            .widgets
            .iter()
            .find(|w| w.enabled && w.visible && w.text.iter().any(|&b| b >= 0x80))
            .expect("a tooltip with a Polish letter");
        (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
            w.text.clone(),
        )
    };
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
    shot(&r.screen, 640, 480, "pl-main-menu-tooltip.png");
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
/// discs, as for the French CD; every disc found, the intro, a spider
/// texture and a dubbed voice read through them.
#[test]
fn the_browser_s_way_of_opening_the_edition() {
    let Some((dir, discs)) = folders() else {
        return;
    };
    let install = StdDir::new(&dir);
    let (exe_name, build, _) = find_exe(&install).unwrap();
    assert_eq!(build.id, ID);
    let mut images: Vec<PathBuf> = std::fs::read_dir(&discs)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")))
        .collect();
    images.sort();
    assert_eq!(images.len(), 4, "{images:?}");
    let copies: Vec<(String, Arc<dyn FileSource>)> = images
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let image = atlantis_data::disc_image::DiscImage::open(p).unwrap();
            (
                format!("CD{}/", i + 1),
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
    let data = scratch("a2-pl-browser");
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
