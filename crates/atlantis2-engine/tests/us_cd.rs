//! The North American CD, *Beyond Atlantis* (DreamCatcher, 2000): its own
//! executable (1.01's code relinked), GOG.com's English texts and voices
//! but for a line of `Menu.txt` and a line of the credits, a logo before
//! the films of the intro, a `chine.wam` without its snoring ambient and
//! spider models whose materials and textures have no accents.
//!
//! Needs `ATLANTIS2_DATA_US` (the installed folder) and
//! `ATLANTIS2_DISCS_US` (a folder of its four CD images or copies), only
//! read; `ATLANTIS2_GOG_EN` for the pictures compared with GOG's English;
//! `ATLANTIS2_PNG_DIR`, when set, receives `us-*.png`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{parse_dial, FontTxt};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{bytes_at, Release};
use atlantis2_data::files::find_exe;
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::spr::SprBank;
use atlantis2_data::wam::Wam;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::engine::web::{load_model, SLOTS};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::sound::VOICE_EFFECTS;
use atlantis2_engine::subtitle::{SubtitleFont, TextOut};
use atlantis2_engine::{BugFixes, Engine, Enhancements, Event, Fpu, LoopType, Variant};
use atlantis_data::files::{FileSource, StdDir};
use md5::{Digest, Md5};

const ID: &str = "atlantis2-cd-us-2199552";
const GOLDEN: &str = include_str!("golden/subtitles_101_gog_en.txt");
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];

fn folders() -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA_US"),
        std::env::var_os("ATLANTIS2_DISCS_US"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA_US / ATLANTIS2_DISCS_US not set");
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn game() -> Option<A2Files> {
    let (dir, discs) = folders()?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect("the US CD");
    assert_eq!(build.id, ID);
    Some(files)
}

fn gog_en() -> Option<A2Files> {
    let Some(dir) = std::env::var_os("ATLANTIS2_GOG_EN") else {
        atlantis2_data::testing::missing("ATLANTIS2_GOG_EN not set");
        return None;
    };
    Some(A2Files::open_dir(&PathBuf::from(dir), &[]).expect("GOG").0)
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

    fn until(&mut self, max: u64, until: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..max {
            let evs = self.step();
            let hit = evs.iter().any(&until);
            all.extend(evs);
            if hit {
                return all;
            }
        }
        panic!("not reached in {max} passes: {:?}", self.e.loop_type);
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

#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some(files) = game() else { return };
    let b = files.build();
    assert_eq!((b.id, b.release, b.gog), (ID, Release::V101, false));
    assert!(b.film_subtitles_blitted);
    assert_eq!(files.language(), Some(Language::English));
    assert_eq!(
        files.label(),
        "Atlantis II, CD, English (DreamCatcher, 2000)"
    );
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // The version string, and the listener table where the build says,
    // the same 5780 bytes as 1.01's.
    let exe = files.read_exe().unwrap();
    assert!(exe
        .windows(b.version.len())
        .any(|w| w == b.version.as_bytes()));
    let table = bytes_at(&exe, b.place_sounds.names, 5780).unwrap();
    let first = &table[..16];
    assert!(first.contains(&0), "{first:02x?}");
    if let Some(gog) = gog_en() {
        let gog_exe = gog.read_exe().unwrap();
        let t = gog.build().place_sounds;
        assert_eq!(hex(table), hex(bytes_at(&gog_exe, t.names, 5780).unwrap()));
    }

    // Menu.txt: every line read, one line against GOG's.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    let lines: Vec<&[u8]> = raw.split(|&b| b == b'\n').collect();
    assert!(
        lines[71].starts_with(b"Please insert any Atlantis CD"),
        "{:?}",
        String::from_utf8_lossy(lines[71])
    );
    // Font.txt's `[Language]` is not a font; the metrics of the 1.01
    // golden.
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    assert_eq!(font_txt.fonts.len(), 4);
    // The disc-request sprites, the French CD's.
    for n in 1..=4 {
        let name = format!("Menu\\b_cd{n}.SPR");
        let bank = SprBank::parse(&files.read(Folder::Spr, &name).unwrap()).unwrap();
        assert!(!bank.is_empty(), "{name}");
    }
}

/// A call as the goldens write it (after the id and the index).
fn call_row(text: &[u8], c: &TextOut) -> String {
    let off = if c.bytes.is_empty() {
        0
    } else {
        text.windows(c.bytes.len())
            .position(|w| w == c.bytes.as_slice())
            .map_or(-1, |p| p as i64)
    };
    format!(
        "{} {} {:06x} {} {} {off}",
        c.x,
        c.y,
        c.colour,
        c.bytes.len(),
        hex(&c.bytes)
    )
}

/// Every subtitle as the executable lays it out: under emulation, the
/// build's output on these texts is the patch's, GOG's English golden.
#[test]
fn every_subtitle_as_the_executable_lays_it_out() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.line_step(), 22);
    let mut want: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split_whitespace().filter(|&f| f != "|").collect();
        match f.first() {
            Some(&"text") => {
                want.entry(f[1].to_string()).or_default();
            }
            Some(&"call") => want
                .entry(f[1].to_string())
                .or_default()
                .push(f[3..].join(" ")),
            _ => {}
        }
    }
    let (mut texts, mut bad, mut used) = (0, Vec::new(), BTreeSet::new());
    for code in WORLDS {
        let data = files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap();
        for d in parse_dial(&data) {
            let got: Vec<String> = font
                .layout(&d.text, 640, 480)
                .iter()
                .map(|c| call_row(&d.text, c))
                .collect();
            let key = format!("{code}.{}", d.id);
            if want.get(&key) != Some(&got) {
                bad.push(key);
            }
            used.extend(d.text.iter().copied().filter(|&b| b >= 0x20));
            texts += 1;
        }
    }
    assert_eq!(want.len(), texts + 7);
    assert!(
        bad.is_empty(),
        "{} differ: {:?}",
        bad.len(),
        &bad[..5.min(bad.len())]
    );
    let missing: Vec<u8> = used.into_iter().filter(|&b| !font.has_glyph(b)).collect();
    assert!(missing.is_empty(), "no glyph for {missing:02x?}");
}

/// The intro's three films on disc 1: the silent DreamCatcher logo, then
/// the two films with their tracks, then the player menu. `intro00.apc`
/// does not exist: voice 2 stays silent for the logo, nothing else.
#[test]
fn the_intro_plays_the_logo_and_the_two_films() {
    let Some(files) = game() else { return };
    assert!(files.read_current(Folder::Apc, "intro00.apc").is_none());
    let data = scratch("a2-us-intro");
    let mut e = boot(files, &data);
    e.open_player_menu();
    e.start_intro();
    let mut r = Run::new(e);
    assert_eq!(r.e.video_playing().map(|(n, _)| n), Some("intro00.hnm"));
    assert!(r.e.intro_playing());
    let mut journal = std::mem::take(&mut r.e.events);
    let mut logo = false;
    let mut silent_logo = true;
    for _ in 0..60 * 60 * 5 {
        if r.e.loop_type == LoopType::Menu {
            break;
        }
        journal.extend(r.step());
        if r.e.video_playing().is_some_and(|(n, _)| n == "intro00.hnm") {
            silent_logo &= !r.e.voices().is_playing(VOICE_EFFECTS);
            if !logo && r.e.video_playing().is_some_and(|(_, k)| k == 30) {
                let colours: BTreeSet<u16> = r.screen.iter().copied().collect();
                assert!(colours.len() > 50, "the logo: {} colours", colours.len());
                shot(&r.screen, 640, 480, "us-dreamcatcher-logo.png");
                logo = true;
            }
        }
    }
    assert!(logo && silent_logo);
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
    // The logo, the two films, the end of the chain (`intro03` on no disc).
    assert_eq!(
        videos,
        [
            ("intro00.hnm".to_string(), Some(61), 61),
            ("intro01.hnm".to_string(), Some(327), 327),
            ("intro02.hnm".to_string(), Some(210), 210),
            ("intro03.hnm".to_string(), None, 0),
        ]
    );
    assert_eq!(sounds, ["intro01.apc", "intro02.apc"]);
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
    let _ = std::fs::remove_dir_all(&data);
}

/// A new game to the shaman's first dialog: an English subtitle on the
/// screen; the main menu; a save and a load kept in the data folder.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let data = scratch("a2-us-game");
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
    r.e.start_dialog(360).unwrap();
    r.until(600, |e| matches!(e, Event::DialogLine(_)));
    for _ in 0..60 {
        r.step();
    }
    let line = r.e.dialog().unwrap().current_line().unwrap().clone();
    assert!(!line.text.is_empty());
    let calls = r.e.subtitle_font().unwrap().layout(&line.text, 640, 480);
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(!calls.is_empty() && white > 100, "{white} white pixels");
    shot(&r.screen, 640, 480, "us-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "us-main-menu.png");
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

/// A film's dialog at 800 x 600: this build draws the subtitles before the
/// enlargement, so they show with the bug fix off too (the French CD's and
/// 1.01's do not: `tests/display.rs`).
#[test]
fn a_film_s_subtitles_show_at_800_x_600_without_the_fix() {
    let Some(_) = game() else { return };
    let frames = |fixes: BugFixes, command: &[u8], name: &str| {
        let data = scratch(name);
        let mut r = in_game(game().unwrap(), &data);
        r.e.enhancements = Enhancements {
            fixes,
            ..Enhancements::default()
        };
        r.e.set_screen_800(true);
        r.e.run_command(b"(monde=1)");
        r.e.run_command(command);
        let mut out = Vec::new();
        r.screen = vec![0; 800 * 600];
        for _ in 0..60 * 40 {
            r.step();
            if r.e.loop_type != LoopType::Video {
                break;
            }
            out.push(hex(bytemuck_u16(&r.screen)));
        }
        let _ = std::fs::remove_dir_all(&data);
        out
    };
    let bare = frames(BugFixes::ALL, b"(cinema=19)", "a2-us-800a");
    let with = frames(BugFixes::NONE, b"(cinema=19,71)", "a2-us-800b");
    let fixed = frames(BugFixes::ALL, b"(cinema=19,71)", "a2-us-800c");
    assert_eq!(with, fixed);
    let n = bare.len().min(with.len());
    let shown = (0..n).filter(|&i| bare[i] != with[i]).count();
    assert!(shown > 15, "{shown} of {n} frames with a subtitle");
}

fn bytemuck_u16(s: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding and u8 no alignment.
    unsafe { std::slice::from_raw_parts(s.as_ptr().cast(), s.len() * 2) }
}

/// The spider's models: the seven files with accents stripped from their
/// materials read, every texture found without an accent lookup, and the
/// web drawn as on GOG's English.
#[test]
fn the_spider_finds_its_textures_and_draws_as_gog_s() {
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
    let us = picture(game().unwrap(), "a2-us-web");
    let colours: BTreeSet<u16> = us.iter().copied().collect();
    assert!(colours.len() > 200, "{} colours", colours.len());
    if let Some(gog) = gog_en() {
        assert_eq!(
            hex(bytemuck_u16(&us)),
            hex(bytemuck_u16(&picture(gog, "a2-us-web-g")))
        );
    }
}

/// `chine.wam` without the ambient `ronfle`: the file reads, no other
/// sound or view of it changes, the three views play, no script names it.
#[test]
fn china_without_its_snoring_ambient() {
    let Some(files) = game() else { return };
    let us = Wam::parse(&files.read(Folder::Wam, "chine.wam").unwrap()).unwrap();
    assert_eq!(us.skipped, 0);
    assert!(!us
        .ambient_objects
        .iter()
        .any(|a| a.name.eq_ignore_ascii_case("ronfle")));
    if let Some(gog) = gog_en() {
        let g = Wam::parse(&gog.read(Folder::Wam, "chine.wam").unwrap()).unwrap();
        let snore: Vec<u32> = g
            .ambient_objects
            .iter()
            .filter(|a| a.name.eq_ignore_ascii_case("ronfle"))
            .map(|a| a.id)
            .collect();
        assert_eq!(snore, [12]);
        assert_eq!(g.ambient_objects.len(), us.ambient_objects.len() + 1);
        let mut changed = Vec::new();
        for (p, q) in g.places.iter().zip(&us.places) {
            for (v, w) in p.views.iter().zip(&q.views) {
                let rest: Vec<_> = v.sounds.iter().filter(|s| s.ambient_id != 12).collect();
                assert_eq!(rest, w.sounds.iter().collect::<Vec<_>>(), "{}", v.name);
                assert_eq!((&v.zones, &v.destinations), (&w.zones, &w.destinations));
                if v.sounds.len() != w.sounds.len() {
                    changed.push(v.name.trim_end_matches(".hnm").to_ascii_lowercase());
                }
            }
        }
        assert_eq!(changed, ["ch0012", "ch0013", "ch0020"]);
    }
    let ids: BTreeSet<u32> = us.ambient_objects.iter().map(|a| a.id).collect();
    for p in &us.places {
        for v in &p.views {
            for s in &v.sounds {
                assert!(ids.contains(&s.ambient_id), "{} {}", v.name, s.ambient_id);
            }
        }
    }
    let data = scratch("a2-us-china");
    let mut r = at(
        files,
        &data,
        &[b"(monde=3)", b"(changewam=chine.wam)", b"(vue=12)"],
    );
    for view in [12, 13, 20] {
        r.e.enter_at(view, 0.0, 0.0).unwrap();
        for _ in 0..60 {
            r.step();
        }
        assert!(!r
            .e
            .sound_table()
            .iter()
            .any(|(_, n)| n.to_ascii_lowercase().contains("ronfle")));
    }
    let _ = std::fs::remove_dir_all(&data);
}

/// A copy of a disc as the browser sees it ([`atlantis_app`]'s `PickedDir`):
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
/// discs (it reads files, not images), as for the French CD; the executable
/// known, every disc found, the intro's logo, a spider texture and a voice
/// read through them.
#[test]
fn the_browser_s_way_of_opening_the_edition() {
    let Some((dir, discs)) = folders() else {
        return;
    };
    let install = StdDir::new(&dir);
    let (exe_name, build, _) = find_exe(&install).unwrap();
    assert_eq!(build.id, ID);
    let copies: Vec<(String, Arc<dyn FileSource>)> = (1..=4)
        .map(|n| {
            let image =
                atlantis_data::disc_image::DiscImage::open(&discs.join(format!("cd{n}.iso")))
                    .unwrap();
            (
                format!("CD{n}/"),
                Arc::new(PickedCopy(Arc::new(image))) as Arc<dyn FileSource>,
            )
        })
        .collect();
    let mut files = A2Files::new(Arc::new(install), copies);
    files.set_exe(exe_name, build);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
    assert_eq!(
        files.label(),
        "Atlantis II, CD, English (DreamCatcher, 2000)"
    );
    assert!(files.read_current(Folder::Hnm, "intro00.hnm").is_some());
    assert!(files.read(Folder::ThreeD, "pz_araig/armar.db").is_some());
    assert!(files.read(Folder::ApcDial, "360_01.apc").is_some());
    let data = scratch("a2-us-browser");
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
