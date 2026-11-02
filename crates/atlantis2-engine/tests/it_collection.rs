//! The Italian *Atlantis Collection* DVD (2014): its installer's folder.
//!
//! GOG.com's layout, with its own `Game.exe` (1.01 with four of the five
//! patches of GOG's 2018 build, byte for byte), its own launcher under
//! `atlantis2.exe`, Italian texts and voices, three films of the intro and
//! its own tracks for films 19, 55 and 66.
//!
//! Needs `ATLANTIS2_IT_COLLECTION` (the installed folder, only read);
//! `ATLANTIS2_GOG_EN` and `ATLANTIS2_GOG_OLD` for what is compared with
//! GOG's; `ATLANTIS2_PNG_DIR`, when set, receives `it-*.png`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use atlantis2_data::dialog::{parse_dial, FontTxt, World};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{bytes_at, Release, LAUNCHER_SIZES};
use atlantis2_data::files::{find_exe, holds_atlantis2};
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::dialog::DialogScript;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::subtitle::{SubtitleFont, TextOut};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use atlantis_data::files::StdDir;
use atlantis_data::TextCodePage;
use md5::{Digest, Md5};

const ID: &str = "atlantis2-collection-it-2183168";
const LABEL: &str = "Atlantis II, Italian Collection (patch 1.01)";
const GOLDEN: &str = include_str!("golden/subtitles_101_it_collection.txt");
const WORLDS: [(&str, i32); 5] = [("CH", 3), ("IR", 1), ("SH", 4), ("TI", 5), ("YU", 2)];

fn folder() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("ATLANTIS2_IT_COLLECTION") else {
        atlantis2_data::testing::missing("ATLANTIS2_IT_COLLECTION not set");
        return None;
    };
    Some(PathBuf::from(dir))
}

fn game() -> Option<A2Files> {
    let dir = folder()?;
    let (files, build) = A2Files::open_dir(&dir, &[]).expect("the collection");
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

/// The folder holds the game as `Game.exe` and a launcher as
/// `atlantis2.exe`: the game is found and named, the launcher's `Settings.ini`
/// names `Game.exe`, the whole game is in the one folder.
#[test]
fn the_executable_behind_its_launcher_and_the_edition_s_name() {
    let Some(dir) = folder() else { return };
    assert!(holds_atlantis2(&dir));
    let install = StdDir::new(&dir);
    assert_eq!(
        atlantis_data::files::FileSource::size(&install, "atlantis2.exe"),
        Some(LAUNCHER_SIZES[1])
    );
    let (name, build, _) = find_exe(&install).unwrap();
    assert_eq!((name, build.id), ("Game.exe", ID));
    assert_eq!(
        (build.release, build.gog, build.film_subtitles_blitted),
        (Release::V101, true, false)
    );
    let settings = atlantis_data::files::FileSource::read(&install, "Launch/Settings.ini").unwrap();
    assert!(settings
        .windows(b"File0=\\Game.exe".len())
        .any(|w| w == b"File0=\\Game.exe"));

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("Game.exe"));
    assert_eq!(files.language(), Some(Language::Italian));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);

    // The version string, and the listener table at 1.01's address.
    let exe = files.read_exe().unwrap();
    assert!(exe
        .windows(build.version.len())
        .any(|w| w == build.version.as_bytes()));
    assert!(bytes_at(&exe, build.place_sounds.names, 5780).is_some());
}

/// One byte from the `Game.exe` of GOG's 2018 build: @ 0x004054c7 in `cmp
/// eax,5` (a CD drive), where GOG has 3 (a fixed drive).
#[test]
#[cfg(unix)]
fn one_byte_from_gog_s_2018_build() {
    let Some(dir) = folder() else { return };
    let Some(old) = std::env::var_os("ATLANTIS2_GOG_OLD") else {
        atlantis2_data::testing::missing("ATLANTIS2_GOG_OLD not set");
        return;
    };
    let ours = std::fs::read(dir.join("Game.exe")).unwrap();
    let gog = std::fs::read(PathBuf::from(old).join("1.0/Atlantis 2/Game.exe")).unwrap();
    assert_eq!(ours.len(), gog.len());
    let diff: Vec<(usize, u8, u8)> = (0..ours.len())
        .filter(|&i| ours[i] != gog[i])
        .map(|i| (i, ours[i], gog[i]))
        .collect();
    assert_eq!(diff, [(0x54c7, 5, 3)]);
}

/// `loc\`: every file read as the original reads it.
#[test]
fn its_loc_files_read() {
    let Some(files) = game() else { return };
    let menu = MenuTxt::parse(&files.read(Folder::Loc, "Menu.txt").unwrap());
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert!(raw
        .windows(b"Menu principale".len())
        .any(|w| w == b"Menu principale"));
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    assert_eq!(font_txt.breaks, [0x20]);
    let heights: Vec<i32> = font_txt.fonts.iter().map(|f| f.height).collect();
    assert_eq!(heights, [12, 22, 20, 16]);
    // Messages.txt: every object name is a `defnames.con` object.
    let names = ObjectNames::parse(&files.read(Folder::Loc, "Messages.txt").unwrap());
    let dn = DefNames::parse(&files.read(Folder::Scenar, "defnames.con").unwrap()).unwrap();
    assert!(names.len() > 50, "{}", names.len());
    for (key, name) in names.entries() {
        assert!(dn.id(defnames::OBJETS, key).is_ok(), "{key:?} {name:?}");
    }
}

/// A call as the golden writes it (after the id and the index).
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

fn golden_calls() -> BTreeMap<String, Vec<String>> {
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
    want
}

/// The ASCII letter under an accented one.
fn base_of(c: char) -> Option<u8> {
    const FROM: &str = "àèéëìòù";
    const TO: &[u8] = b"aeeeiou";
    FROM.chars().position(|f| f == c).map(|i| TO[i])
}

/// Every subtitle as the executable lays it out under emulation, every
/// letter drawn: Windows-1252 bytes, à è é ì ò ù in the dialogs, each drawn
/// unlike its base letter; the letters of the menus and object names too.
#[test]
fn every_subtitle_as_the_executable_lays_it_out_and_every_letter() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.line_step(), 22);
    let want = golden_calls();
    let (mut texts, mut bad, mut used) = (0, Vec::new(), BTreeSet::new());
    for (code, _) in WORLDS {
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
    // The golden also holds 7 synthetic texts (tests/dialogs.rs).
    assert_eq!(want.len(), texts + 7);
    assert!(
        bad.is_empty(),
        "{} differ: {:?}",
        bad.len(),
        &bad[..5.min(bad.len())]
    );
    let accented: String = used
        .iter()
        .filter(|&&b| b >= 0x80)
        .map(|&b| TextCodePage::Windows1252.char_of(b))
        .collect();
    assert_eq!(accented, "àèéìòù");
    let mut others = BTreeSet::new();
    for name in ["Menu.txt", "Messages.txt", "Credits.txt"] {
        let data = files.read(Folder::Loc, name).unwrap();
        others.extend(data.iter().copied().filter(|&b| b >= 0x80));
    }
    let px = |text: &[u8]| {
        let mut out = vec![0u16; 640 * 480];
        font.draw(&mut out, 640, 480, &font.layout(text, 640, 480));
        out
    };
    let missing: Vec<u8> = used
        .iter()
        .chain(&others)
        .copied()
        .filter(|&b| !font.has_glyph(b))
        .collect();
    assert!(missing.is_empty(), "no glyph for {missing:02x?}");
    for &b in used.iter().chain(&others).filter(|&&b| b >= 0x80) {
        let drawn = px(&[b]);
        assert!(
            drawn.iter().filter(|&&p| p == 0xffff).count() > 3,
            "{b:02x}"
        );
        if let Some(base) = base_of(TextCodePage::Windows1252.char_of(b)) {
            assert_ne!(drawn, px(&[base]), "{b:02x} drawn as {}", base as char);
        }
    }
}

/// Every dialog line's voice decoded, and the same set as GOG's.
#[test]
fn its_voices_decode() {
    let Some(files) = game() else { return };
    let voices = |files: &A2Files| {
        let mut decoded = BTreeSet::new();
        let mut seconds = 0.0;
        for (_, monde) in WORLDS {
            let world = World::of_monde(monde).unwrap();
            let script = DialogScript::load(files, world, Fpu::Double).unwrap();
            for l in &script.com {
                let name = format!("{}.apc", l.id);
                let Some(apc) = files.read(Folder::ApcDial, &name) else {
                    continue;
                };
                let v = atlantis_codecs::decode_apc(&apc).unwrap();
                assert!(!v.samples.is_empty(), "{name}");
                seconds +=
                    v.samples.len() as f64 / f64::from(v.sample_rate) / f64::from(v.channels);
                decoded.insert(name);
            }
        }
        (decoded, seconds)
    };
    let (ours, seconds) = voices(&files);
    eprintln!("{} voices decoded, {seconds:.0} s", ours.len());
    assert_eq!(ours.len(), 1199);
    if let Some(en) = gog_en() {
        assert_eq!(ours, voices(&en).0);
    }
}

/// The mean absolute difference of two tracks' first channel over their
/// common length, the first 60 s, against the first's mean level: near 0
/// for one recording, not for another voice.
fn apart(a: &[u8], b: &[u8]) -> f64 {
    let a = atlantis_codecs::decode_apc(a).unwrap();
    let b = atlantis_codecs::decode_apc(b).unwrap();
    let n = a.samples.len().min(b.samples.len()).min(44100 * 60);
    let level: f64 = a.samples[..n].iter().map(|&s| f64::from(s).abs()).sum();
    let diff: f64 = a.samples[..n]
        .iter()
        .zip(&b.samples[..n])
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).abs())
        .sum();
    diff / level
}

/// Films 19, 55 and 66: the folder the film reads (`datas_a2\apc\`) holds
/// neither GOG's English tracks nor the same bytes as `apc\dial\`; the
/// engine, with the bug fixes off, loads that file for the film.
#[test]
fn the_film_tracks_are_the_collection_s_own() {
    let Some(files) = game() else { return };
    let en = gog_en();
    for film in [19, 55, 66] {
        let name = format!("cine{film:03}.apc");
        let played = files.read(Folder::Apc, &name).unwrap();
        let own = files.read(Folder::ApcDial, &name).unwrap();
        assert_ne!(hex(&played), hex(&own), "{name}");
        let with_own = apart(&played, &own);
        let mut line = format!("{name}: against apc\\dial {with_own:.3}");
        if let Some(en) = &en {
            let english = en.read(Folder::Apc, &name).unwrap();
            assert_ne!(hex(&played), hex(&english), "{name}");
            let with_english = apart(&played, &english);
            line += &format!(", against GOG's English {with_english:.3}");
        }
        eprintln!("{line}");
    }
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements.fixes = atlantis2_engine::BugFixes::NONE;
    e.run_command(b"(monde=3)");
    e.run_command(b"(cinema=19)");
    assert!(
        e.sound_table().iter().any(|(_, n)| n == "cine019.apc"),
        "{:?}",
        e.sound_table()
    );
}

/// The intro: `intro02.hnm` is on this folder, the bytes of `intro01`, so
/// the chain (`intro%02d` up to the first file missing) shows that film
/// twice; GOG's stops after `intro01`.
#[test]
fn the_intro_plays_three_films() {
    let Some(files) = game() else { return };
    assert_eq!(
        hex(&files.read(Folder::Hnm, "intro01.hnm").unwrap()),
        hex(&files.read(Folder::Hnm, "intro02.hnm").unwrap())
    );
    let data = scratch("a2-it-intro");
    let mut e = boot(files, &data);
    e.open_player_menu();
    e.start_intro();
    let mut r = Run::new(e);
    let mut journal = std::mem::take(&mut r.e.events);
    for _ in 0..60 * 60 * 5 {
        if r.e.loop_type == LoopType::Menu {
            break;
        }
        journal.extend(r.step());
    }
    assert_eq!(r.e.loop_type, LoopType::Menu);
    let mut videos: Vec<(String, Option<u32>)> = Vec::new();
    let mut sounds = Vec::new();
    for ev in &journal {
        match ev {
            Event::VideoStart { name, frames } => videos.push((name.clone(), *frames)),
            Event::Sound(s) => sounds.push(s.clone()),
            _ => {}
        }
    }
    let names: Vec<&str> = videos.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["intro00.hnm", "intro01.hnm", "intro02.hnm", "intro03.hnm"]
    );
    assert_eq!(videos[3].1, None);
    assert_eq!(sounds, ["intro00.apc", "intro01.apc", "intro02.apc"]);
    let _ = std::fs::remove_dir_all(&data);
}

/// The music: `music\` holds one empty file, the streams are under
/// `datas_a2\apc\music\` as on GOG's; a missing copy counts as an empty
/// one, so nothing is taken as copied. Each music in the data decodes.
#[test]
fn the_music_is_read_from_its_folder() {
    let Some(dir) = folder() else { return };
    let files = game().unwrap();
    assert_eq!(
        std::fs::metadata(dir.join("music/Muzik39.apc"))
            .unwrap()
            .len(),
        0
    );
    let copies = std::fs::read_dir(dir.join("music")).unwrap().count();
    assert_eq!(copies, 1);
    let data = scratch("a2-it-music");
    let e = boot(game().unwrap(), &data);
    assert!(e.media.copies.is_empty(), "{:?}", e.media.copies);
    let _ = std::fs::remove_dir_all(&data);
    let names = files.install().list("datas_a2/apc/music");
    let streams: Vec<&String> = names
        .iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".apc"))
        .collect();
    assert!(streams.len() >= 20, "{}", streams.len());
    for n in streams {
        let d = files.read(Folder::ApcMusic, n).unwrap();
        assert!(atlantis_codecs::decode_apc(&d).is_ok(), "{n}");
    }
    if let Some(en) = gog_en() {
        for n in ["muzik39.apc", "muzik01.apc", "muzik01.spp"] {
            assert_eq!(
                hex(&files.read(Folder::ApcMusic, n).unwrap()),
                hex(&en.read(Folder::ApcMusic, n).unwrap()),
                "{n}"
            );
        }
    }
}

/// The Tibet line with the most accented letters.
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

/// A new game to the shaman's first dialog: an Italian subtitle with an
/// accent drawn, its voice present; the main menu; a save and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let want = golden_calls();
    let data = scratch("a2-it-game");
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
    let calls = r.e.subtitle_font().unwrap().layout(&text, 640, 480);
    let got: Vec<String> = calls.iter().map(|c| call_row(&text, c)).collect();
    assert_eq!(Some(&got), want.get(&format!("TI.{id}")));
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(white > 100, "{white} white pixels");
    shot(&r.screen, 640, 480, "it-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    shot(&r.screen, 640, 480, "it-main-menu.png");
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
