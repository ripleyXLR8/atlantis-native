//! The Spanish CD (4 CDs, 1999, distributed in Spain by Friendware): the
//! Polish CD's executable inside a copy protection wrapper (the same code,
//! 1.01's behaviours, data tables at the same addresses), Spanish texts in
//! Windows-1252 with the Polish CD's `Font.txt` (1.01's heights) and the
//! French CD's font, Spanish voices, the Spanish tracks of films 19, 55
//! and 66 in their own place, a Spanish logo before the intro and the
//! spider's textures renamed without accents, as on the Polish CD. The
//! rest of the data is the French CD's.
//!
//! Needs `ATLANTIS2_DATA_ES` (the installed folder: `data1.cab` of the
//! first disc extracted) and `ATLANTIS2_DISCS_ES` (a folder holding its
//! four CD images), only read; `ATLANTIS2_DATA_PL` / `ATLANTIS2_DISCS_PL`
//! (the Polish CD) and `ATLANTIS2_DATA` / `ATLANTIS2_DISCS` (the French
//! CD) for what the edition shares with them; `ATLANTIS2_GOG_ES` (GOG.com's
//! Spanish) for the texts, voices and film tracks; `ATLANTIS2_PNG_DIR`,
//! when set, receives `es-*.png`.
//!
//! The program is only read (its size, MD5 and data tables), never run.

use std::collections::BTreeSet;
use std::path::PathBuf;
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

const ID: &str = "atlantis2-cd-es-2415963";
const LABEL: &str = "Atlantis II, Spanish CD (1999)";
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];

fn folders() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA_ES"),
        std::env::var_os("ATLANTIS2_DISCS_ES"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA_ES / ATLANTIS2_DISCS_ES not set");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = folders()?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect("the Spanish CD");
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

/// The Polish CD, when its variables are set (optional here).
fn polish_cd() -> Option<A2Files> {
    let dir = std::env::var_os("ATLANTIS2_DATA_PL")?;
    let discs = std::env::var_os("ATLANTIS2_DISCS_PL")?;
    Some(
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])
            .expect("the Polish CD")
            .0,
    )
}

/// GOG.com's Spanish, when its variable is set (optional here).
fn gog_spanish() -> Option<A2Files> {
    std::env::var_os("ATLANTIS2_GOG_ES").map(|d| {
        A2Files::open_dir(&PathBuf::from(d), &[])
            .expect("GOG Spanish")
            .0
    })
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

/// The executable known by its MD5 alone (its PE timestamp is the Polish
/// CD's), the Polish CD's code (1.01's behaviours) and data tables; the
/// language Spanish, the texts Windows-1252; `loc\` read, and the same as
/// GOG.com's Spanish but for `Menu.txt`'s line that asks for the CD and
/// `Font.txt`'s `[Language]`.
#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some((dir, _)) = folders() else { return };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(exe.len(), 2_415_963);
    assert_eq!(hex(&exe), "881e461cc4d0f765cb2942bd826aba8d");
    let b = identify(&exe).unwrap();
    assert_eq!(b.id, ID);
    assert_eq!(
        (b.release, b.gog, b.film_subtitles_blitted),
        (Release::V101, false, false)
    );
    assert_eq!(b.version, "Atlantis 2 - Version 3.15 - 8/10/1999");
    assert_eq!(
        bytes_at(&exe, 0x005e_21c8, b.version.len()),
        Some(b.version.as_bytes())
    );
    let table = bytes_at(&exe, b.place_sounds.names, 5780).unwrap();
    assert!(table[..16].contains(&0), "{:02x?}", &table[..16]);
    if let Some(pl) = polish_cd() {
        // `.rdata` and `.data` byte for byte the Polish CD's, at the same
        // addresses: every table the port reads.
        let pl_exe = pl.read_exe().unwrap();
        assert_eq!(pl.build().place_sounds, b.place_sounds);
        for (va, len) in [(0x005c_5000, 0x1_0000), (0x005d_5000, 0x3_f000)] {
            assert_eq!(
                hex(bytes_at(&exe, va, len).unwrap()),
                hex(bytes_at(&pl_exe, va, len).unwrap()),
                "{va:#x}"
            );
        }
    }

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("atlantis2.exe"));
    assert_eq!(files.language(), Some(Language::Spanish));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.text_code_page(), TextCodePage::Windows1252);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // `loc\Menu.txt`: every section read.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert_eq!(raw.len(), 1239);
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    for line in ["Menú principal", "Cargar juego"] {
        let want = cp1252(line);
        assert!(raw.windows(want.len()).any(|w| w == want), "{line}");
    }
    // `Font.txt`: no `[CharacterSet]` (SBCS), 1.01's heights, the Polish
    // CD's file; the font file the French CD's.
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    let heights: Vec<i32> = font_txt.fonts.iter().map(|f| f.height).collect();
    assert_eq!(heights, [12, 22, 20, 16]);
    if let Some(pl) = polish_cd() {
        assert_eq!(
            files.read(Folder::Loc, "Font.txt"),
            pl.read(Folder::Loc, "Font.txt")
        );
    }
    if let Some(fr) = french_cd() {
        assert_eq!(
            files.read(Folder::Loc, "Atl_Font.ttf"),
            fr.read(Folder::Loc, "Atl_Font.ttf")
        );
    }
    // The texts: GOG.com's Spanish.
    if let Some(gog) = gog_spanish() {
        for code in WORLDS {
            let name = format!("Dial{code}.txt");
            assert_eq!(
                files.read(Folder::Loc, &name),
                gog.read(Folder::Loc, &name),
                "{name}"
            );
        }
        for name in ["Messages.txt", "Credits.txt"] {
            assert_eq!(
                files.read(Folder::Loc, name),
                gog.read(Folder::Loc, name),
                "{name}"
            );
        }
        let gog_menu = gog.read(Folder::Loc, "Menu.txt").unwrap();
        let lines =
            |d: &[u8]| -> Vec<Vec<u8>> { d.split(|&b| b == b'\n').map(<[u8]>::to_vec).collect() };
        let (a, g) = (lines(&raw), lines(&gog_menu));
        assert_eq!(a.len(), g.len());
        let differ = a.iter().zip(&g).filter(|(x, y)| x != y).count();
        assert_eq!(differ, 1);
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
/// a glyph, and the Spanish letters are among them.
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
    for c in "áéíóúñ¿¡".chars() {
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
    let text = cp1252("¿Mañana? ¡Él está aquí!");
    let mut out = vec![0u16; 640 * 480];
    font.draw(&mut out, 640, 480, &font.layout(&text, 640, 480));
    assert!(out.iter().filter(|&&p| p == 0xffff).count() > 100);
    shot(&out, 640, 480, "es-letters.png");
}

/// Films 19, 55 and 66: the discs' `apc\` tracks are the Spanish ones
/// (GOG.com's Spanish keeps them in `apc\dial\`), and no disc has a track
/// in `apc\dial\`, so the bug fix of the films' language finds nothing
/// to change.
#[test]
fn the_films_speak_spanish() {
    let Some(files) = game() else { return };
    let gog = gog_spanish();
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

/// The spider's models: the files of CD4 rewritten to the textures'
/// names without accents (the Polish CD's files) read, every texture
/// found, and the web drawn as on the French CD.
#[test]
fn the_spider_finds_its_textures() {
    let Some(files) = game() else { return };
    let pl = polish_cd();
    let mut materials = 0;
    for slot in SLOTS {
        let read = |p: &str| files.read(Folder::ThreeD, p);
        assert!(load_model(Fpu::Chop, slot, read).is_some(), "{slot:?}");
        let path = format!("pz_araig/{}", slot.file());
        let db = files.read(Folder::ThreeD, &path).unwrap();
        if let Some(pl) = &pl {
            assert_eq!(hex(&db), hex(&pl.read(Folder::ThreeD, &path).unwrap()));
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
    let es = picture(game().unwrap(), "a2-es-web");
    let colours: BTreeSet<u16> = es.iter().copied().collect();
    assert!(colours.len() > 200, "{} colours", colours.len());
    shot(&es, 640, 480, "es-web.png");
    if let Some(fr) = french_cd() {
        assert_eq!(
            hex(bytemuck_u16(&es)),
            hex(bytemuck_u16(&picture(fr, "a2-es-web-fr")))
        );
    }
}

/// The intro on disc 1: the Spanish logo (GOG.com's `INTROESP.HNM`) with
/// no track, then the two films with their tracks, then the player menu.
#[test]
fn the_intro_plays_the_logo_and_the_two_films() {
    let Some(files) = game() else { return };
    assert!(files.read_current(Folder::Apc, "intro00.apc").is_none());
    if let Some(gog) = gog_spanish() {
        assert_eq!(
            hex(&files.read_current(Folder::Hnm, "intro00.hnm").unwrap()),
            hex(&gog.read(Folder::Hnm, "introesp.hnm").unwrap())
        );
    }
    let data = scratch("a2-es-intro");
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
                .is_some_and(|(n, k)| n == "intro00.hnm" && k == 45)
        {
            let colours: BTreeSet<u16> = r.screen.iter().copied().collect();
            assert!(colours.len() > 20, "the logo: {} colours", colours.len());
            shot(&r.screen, 640, 480, "es-logo.png");
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

/// The Tibet line with the most Spanish letters.
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

/// A new game to a Tibet dialog: a Spanish subtitle drawn, its voice
/// present (GOG.com's Spanish one, when set); the main menu in Spanish; a
/// save and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let gog = gog_spanish();
    let data = scratch("a2-es-game");
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
    shot(&r.screen, 640, 480, "es-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "es-main-menu.png");
    // A widget with a tooltip (Spanish, from `Menu.txt`): the pointer on
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
    shot(&r.screen, 640, 480, "es-main-menu-tooltip.png");
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

/// The browser's way in: the installed folder and the four images
/// (Alcohol 120% `.mdf`), as for the French CD; every disc found, the
/// intro, a spider model and a Spanish voice read through them, the intro
/// started.
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
    let data = scratch("a2-es-browser");
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
