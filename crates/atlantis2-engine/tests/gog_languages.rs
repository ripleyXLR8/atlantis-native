//! GOG.com's four languages and its three builds.
//!
//! GOG ships one `Game.exe` for its English, French, German and Spanish
//! installations; the languages differ in `loc\` (texts, `Font.txt`), the
//! voices of `DATAS_A2\APC\DIAL\`, the manual and the splash pictures.
//! For each installation: the edition's name and language, every `loc`
//! file read as the original reads it, every subtitle laid out as the
//! 1.01 executable lays it out (`tests/golden/subtitles_101_gog*.txt`,
//! one per language, from the executable's own subtitle function under
//! emulation), every letter of the texts drawn with a glyph of the font,
//! every voice decoded, the film tracks, and a new game played to the
//! shaman's first dialog with a subtitle drawn.
//!
//! GOG's older builds ("1.0" of 2018, "1.0 w10" of June 2022) have their
//! own `Game.exe`: their bytes are checked against the current one's and
//! the game boots on each.
//!
//! Needs `ATLANTIS2_GOG` (French), `ATLANTIS2_GOG_EN`, `ATLANTIS2_GOG_DE`,
//! `ATLANTIS2_GOG_ES` (installation folders, only read) and
//! `ATLANTIS2_GOG_OLD` (a folder holding `1.0/Atlantis 2/Game.exe` and
//! `1.0-w10/Atlantis 2/Game.exe`); `ATLANTIS2_PNG_DIR`, when set, receives
//! a screenshot of a subtitle per language.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use atlantis2_data::dialog::{parse_dial, FontTxt, World};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::Release;
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::dialog::DialogScript;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::subtitle::{SubtitleFont, TextOut};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

/// The installations: variable, language, golden.
const LANGUAGES: [(&str, Language, &str); 4] = [
    (
        "ATLANTIS2_GOG",
        Language::French,
        include_str!("golden/subtitles_101_gog.txt"),
    ),
    (
        "ATLANTIS2_GOG_EN",
        Language::English,
        include_str!("golden/subtitles_101_gog_en.txt"),
    ),
    (
        "ATLANTIS2_GOG_DE",
        Language::German,
        include_str!("golden/subtitles_101_gog_de.txt"),
    ),
    (
        "ATLANTIS2_GOG_ES",
        Language::Spanish,
        include_str!("golden/subtitles_101_gog_es.txt"),
    ),
];

/// The worlds' codes and `MONDE` numbers.
const WORLDS: [(&str, i32); 5] = [("CH", 3), ("IR", 1), ("SH", 4), ("TI", 5), ("YU", 2)];

/// Each installation's folder, or `None` (after [`missing`]) when one
/// variable is not set.
///
/// [`missing`]: atlantis2_data::testing::missing
fn installations() -> Option<Vec<(PathBuf, Language, &'static str)>> {
    let mut out = Vec::new();
    for (var, language, golden) in LANGUAGES {
        let Some(dir) = std::env::var_os(var) else {
            atlantis2_data::testing::missing(&format!("{var} not set"));
            return None;
        };
        out.push((PathBuf::from(dir), language, golden));
    }
    Some(out)
}

fn open(dir: &Path) -> A2Files {
    A2Files::open_dir(dir, &[]).expect("GOG folder").0
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

#[test]
fn each_language_is_named_and_its_loc_files_read() {
    let Some(all) = installations() else { return };
    let mut ids: Vec<BTreeMap<String, BTreeSet<String>>> = Vec::new();
    let mut credits_pages = Vec::new();
    for (dir, language, _) in &all {
        let files = open(dir);
        let build = files.build();
        assert_eq!(
            (build.id, build.release, build.gog),
            ("atlantis2-gog-fr-2183168", Release::V101, true),
            "{language:?}"
        );
        assert_eq!(files.language(), Some(*language));
        assert_eq!(
            files.label(),
            format!("Atlantis II, GOG.com, {} (patch 1.01)", language.name())
        );
        // Menu.txt: every section with its lines, none padded.
        let menu = MenuTxt::parse(&files.read(Folder::Loc, "Menu.txt").unwrap());
        assert!(menu.padded.is_empty(), "{language:?}: {:?}", menu.padded);
        // Font.txt: the same fonts and breaks as the French file, 1.01's
        // single-byte mode.
        let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
        assert_eq!(font_txt.mbcs, None);
        assert_eq!(font_txt.breaks, [0x20]);
        let heights: Vec<(String, i32)> = font_txt
            .fonts
            .iter()
            .map(|f| (f.id.clone(), f.height))
            .collect();
        assert_eq!(
            heights,
            [
                ("AtlFT_Default".to_string(), 12),
                ("AtlFT_Dialog".to_string(), 22),
                ("AtlFT_Big_Default".to_string(), 20),
                ("AtlFT_POPUP".to_string(), 16),
            ],
            "{language:?}"
        );
        // Messages.txt: every object name is a `defnames.con` object (the
        // original stops the game at one that is not, FUN_004106e0).
        let names = ObjectNames::parse(&files.read(Folder::Loc, "Messages.txt").unwrap());
        let dn = DefNames::parse(&files.read(Folder::Scenar, "defnames.con").unwrap()).unwrap();
        assert!(names.len() > 50, "{language:?}: {}", names.len());
        for (key, name) in names.entries() {
            assert!(
                dn.id(defnames::OBJETS, key).is_ok(),
                "{language:?}: {}",
                String::from_utf8_lossy(key)
            );
            assert!(!name.is_empty());
        }
        // The names as the bag bar shows them (`AtlFT_POPUP`, one
        // `TextOutA` centred on the cell, kept on the screen): the widest.
        let popup = SubtitleFont::load(&files, &font_txt, "AtlFT_POPUP").unwrap();
        let widest = names
            .entries()
            .map(|(_, n)| popup.extent(n).0)
            .max()
            .unwrap();
        assert!(widest < 640, "{language:?}: {widest}");
        // Credits.txt: every page closed.
        let credits = files.read(Folder::Loc, "Credits.txt").unwrap();
        let count = |tag: &[u8]| {
            credits
                .split(|&b| b == b'\n')
                .filter(|l| l.strip_suffix(b"\r").unwrap_or(l) == tag)
                .count()
        };
        assert_eq!(count(b"[BeginPage]"), count(b"[EndPage]"), "{language:?}");
        let pages = atlantis2_engine::credits::parse(&credits);
        credits_pages.push(pages.len());
        // The five worlds' dialog files.
        let mut per_world = BTreeMap::new();
        for (code, _) in WORLDS {
            let d = parse_dial(&files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap());
            assert!(!d.is_empty());
            per_world.insert(
                code.to_string(),
                d.into_iter().map(|l| l.id).collect::<BTreeSet<_>>(),
            );
        }
        ids.push(per_world);
        eprintln!(
            "{}: {} object names (the widest {widest} pixels), {} credit pages, {} dialog lines",
            language.name(),
            names.len(),
            pages.len(),
            ids.last()
                .unwrap()
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
        );
    }
    // The credits: 21 pages in French, 22 in the others (each page over
    // picture i mod 16 + 1, any count). The same line identifiers in every
    // language.
    assert_eq!(credits_pages, [21, 22, 22, 22]);
    for (k, other) in ids.iter().enumerate().skip(1) {
        assert_eq!(other, &ids[0], "{:?}", all[k].1);
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

/// The calls of every text of a golden, by key (`TI.360_01`).
fn golden_calls(golden: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in golden.lines() {
        let f: Vec<&str> = line.split_whitespace().filter(|&f| f != "|").collect();
        match f.first() {
            Some(&"text") => {
                out.entry(f[1].to_string()).or_default();
            }
            Some(&"call") => out
                .entry(f[1].to_string())
                .or_default()
                .push(f[3..].join(" ")),
            _ => {}
        }
    }
    out
}

/// Every subtitle of each language as 1.01 lays it out, and every byte of
/// the texts a glyph of the font.
#[test]
fn each_language_s_subtitles_match_the_original() {
    let Some(all) = installations() else { return };
    for (dir, language, golden) in &all {
        let files = open(dir);
        let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
        let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
        assert_eq!(font.line_step(), 22);
        let want = golden_calls(golden);
        let mut used = BTreeSet::new();
        let (mut texts, mut rows, mut bad) = (0, 0, Vec::new());
        for (code, _) in WORLDS {
            let data = files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap();
            for d in parse_dial(&data) {
                let key = format!("{code}.{}", d.id);
                let got: Vec<String> = font
                    .layout(&d.text, 640, 480)
                    .iter()
                    .map(|c| call_row(&d.text, c))
                    .collect();
                rows += got.len() / 2;
                if want.get(&key) != Some(&got) {
                    bad.push(key);
                }
                used.extend(d.text.iter().copied().filter(|&b| b >= 0x20));
                texts += 1;
            }
        }
        // The golden also holds 7 synthetic texts (tests/dialogs.rs).
        assert_eq!(want.len(), texts + 7, "{language:?}");
        assert!(
            bad.is_empty(),
            "{language:?}: {} lines differ, first {:?}",
            bad.len(),
            &bad[..bad.len().min(5)]
        );
        let missing: Vec<u8> = used
            .iter()
            .copied()
            .filter(|&b| !font.has_glyph(b))
            .collect();
        assert!(
            missing.is_empty(),
            "{language:?}: no glyph for {missing:02x?}"
        );
        // Each letter beyond ASCII drawn alone: white pixels, and not the
        // pixels of its base letter (the accent is there).
        let white = |text: &[u8]| {
            let mut out = vec![0u16; 640 * 480];
            font.draw(&mut out, 640, 480, &font.layout(text, 640, 480));
            out
        };
        for &b in used.iter().filter(|&&b| b >= 0x80) {
            let px = white(&[b]);
            assert!(px.iter().filter(|&&p| p == 0xffff).count() > 3, "{b:02x}");
            if let Some(base) = unicode_base(atlantis_data::TextCodePage::Windows1252.char_of(b)) {
                assert_ne!(px, white(&[base]), "{b:02x} drawn as {}", base as char);
            }
        }
        // The line of the five worlds with the most letters beyond ASCII,
        // drawn on black.
        if let Some(out) = std::env::var_os("ATLANTIS2_PNG_DIR") {
            let mut best: (usize, Vec<u8>) = (0, Vec::new());
            for (code, _) in WORLDS {
                let data = files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap();
                for d in parse_dial(&data) {
                    let wide: BTreeSet<u8> =
                        d.text.iter().copied().filter(|&b| b >= 0x80).collect();
                    if wide.len() > best.0 {
                        best = (wide.len(), d.text);
                    }
                }
            }
            let png = atlantis2_engine::png::encode_rgb565(&white(&best.1), 640, 480);
            let name = format!("gog-{}-letters.png", language.name().to_lowercase());
            std::fs::write(PathBuf::from(out).join(name), png).unwrap();
        }
        let accented: String = used
            .iter()
            .filter(|&&b| b >= 0x80)
            .map(|&b| atlantis_data::TextCodePage::Windows1252.char_of(b))
            .collect();
        eprintln!(
            "{}: {texts} lines, {rows} rows, as 1.01; letters beyond ASCII: {accented}",
            language.name()
        );
    }
}

/// The ASCII letter under an accented Latin-1 one (`é` → `e`), `None`
/// for the others (`ß`, `¿`, `œ`).
fn unicode_base(c: char) -> Option<u8> {
    const FROM: &str = "ÀÁÂÄÇÈÉÊËÌÍÎÏÑÒÓÔÖÙÚÛÜàáâäçèéêëìíîïñòóôöùúûü";
    const TO: &[u8] = b"AAAACEEEEIIIINOOOOUUUUaaaaceeeeiiiinoooouuuu";
    FROM.chars().position(|f| f == c).map(|i| TO[i])
}

/// Every dialog line's voice, in each language: present where the French
/// one is, and decoded.
#[test]
fn each_language_s_voices_decode() {
    let Some(all) = installations() else { return };
    let mut present: Vec<BTreeSet<String>> = Vec::new();
    for (dir, language, _) in &all {
        let files = open(dir);
        let (mut lines, mut decoded, mut seconds) = (0, BTreeSet::new(), 0.0);
        for (code, monde) in WORLDS {
            let world = World::of_monde(monde).unwrap();
            assert_eq!(world.code, code);
            let script = DialogScript::load(&files, world, Fpu::Double).unwrap();
            for l in &script.com {
                lines += 1;
                let voice_name = format!("{}.apc", l.id);
                let Some(apc) = files.read(Folder::ApcDial, &voice_name) else {
                    continue;
                };
                let voice = atlantis_codecs::decode_apc(&apc)
                    .unwrap_or_else(|e| panic!("{language:?} {voice_name}: {e}"));
                assert!(!voice.samples.is_empty(), "{language:?} {voice_name}");
                seconds += voice.samples.len() as f64
                    / f64::from(voice.sample_rate)
                    / f64::from(voice.channels);
                decoded.insert(voice_name);
            }
        }
        eprintln!(
            "{}: {lines} dialog lines, {} voices decoded, {seconds:.0} s",
            language.name(),
            decoded.len()
        );
        present.push(decoded);
    }
    for (k, p) in present.iter().enumerate().skip(1) {
        assert_eq!(p, &present[0], "{:?}", all[k].1);
    }
}

/// Films 19, 55 and 66: the track is read from `datas_a2\apc\`
/// (`atlantis2-gog-fr-2183168` @ 0x00481d57 `cine%03d.apc` →
/// 0x0047a3b0 → 0x0047a5c0 → the folder builder 0x00402760,
/// `datas_a2\apc\`), where GOG put the English tracks for every language;
/// each language's own track is in `apc\dial\`, which the film does not
/// read.
#[test]
fn the_film_tracks_are_the_english_ones_in_every_language() {
    let Some(all) = installations() else { return };
    let english = open(&all[1].0);
    for (dir, language, _) in &all {
        let files = open(dir);
        for film in [19, 55, 66] {
            let name = format!("cine{film:03}.apc");
            let played = files.read(Folder::Apc, &name).unwrap();
            let english_own = english.read(Folder::ApcDial, &name).unwrap();
            let own = files.read(Folder::ApcDial, &name).unwrap();
            assert_eq!(hex(&played), hex(&english_own), "{language:?} {name}");
            assert_eq!(
                own == played,
                *language == Language::English,
                "{language:?} {name}"
            );
        }
        // The engine loads that file for the film.
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.run_command(b"(monde=3)");
        e.run_command(b"(cinema=19)");
        assert!(
            e.sound_table().iter().any(|(_, n)| n == "cine019.apc"),
            "{language:?}: {:?}",
            e.sound_table()
        );
    }
}

/// The engine at 59.94 passes a second, the clock running with them.
struct Run {
    e: Engine,
    pass: u64,
}

impl Run {
    fn step(&mut self) -> Vec<Event> {
        self.e.set_clock(self.pass as f64 * PASS);
        self.e.pass(None);
        self.pass += 1;
        std::mem::take(&mut self.e.events)
    }

    fn wait(&mut self, n: u64) -> Vec<Event> {
        (0..n).flat_map(|_| self.step()).collect()
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

    fn click(&mut self) {
        self.e.click(self.pass as f64 * PASS);
    }

    /// Zone `zone` of the view looked at and clicked, the transition
    /// video clicked away, up to the arrival in `to`.
    fn walk(&mut self, zone: usize, to: u32) -> Vec<Event> {
        self.wait(15);
        let view = self.e.place().1;
        let (yaw, pitch) = self.e.zone_aim(view, zone).unwrap();
        self.e.camera.enter(yaw, pitch);
        self.step();
        self.click();
        let mut all = Vec::new();
        for _ in 0..600 {
            let evs = self.step();
            if evs.iter().any(|e| {
                matches!(
                    e,
                    Event::VideoStart {
                        frames: Some(_),
                        ..
                    }
                )
            }) {
                all.extend(self.wait(15));
                self.click();
            }
            let arrived = evs.iter().any(|e| matches!(e, Event::ArrivalScan(_)));
            all.extend(evs);
            if arrived {
                assert_eq!(self.e.place().1, to);
                return all;
            }
        }
        panic!("no arrival in view {to}");
    }
}

/// The Tibet line to show: the one with the most letters beyond ASCII
/// (ß, ñ, ¿, ...), the longest of them.
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

/// A new game in each language to the shaman's first dialog (360), its
/// subtitles as 1.01 lays them out; then a Tibet line with letters beyond
/// ASCII drawn on the screen.
#[test]
fn a_new_game_to_the_first_dialog_in_each_language() {
    let Some(all) = installations() else { return };
    for (dir, language, golden) in &all {
        let want = golden_calls(golden);
        let files = open(dir);
        let data = scratch(&format!("a2-gog-{}", language.name()));
        let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        e.rand = Rand::new(1000);
        e.trace = true;
        e.set_data_dir(Some(data.clone()));
        e.start_new_game().unwrap();
        let mut r = Run { e, pass: 0 };
        r.until(600, |e| matches!(e, Event::VideoFrame(3)));
        r.click();
        r.until(600, |e| *e == Event::VideoEnd);
        r.until(600, |e| matches!(e, Event::Music { .. }));
        assert_eq!(r.e.loop_type, LoopType::Warp);
        r.wait(15);
        r.click();
        r.until(2, |e| matches!(e, Event::ClickScan(_)));
        for (zone, to) in [(0, 3), (1, 4), (0, 5), (1, 6)] {
            r.walk(zone, to);
        }
        let evs = r.walk(0, 7);
        assert!(evs.contains(&Event::DialogStart {
            number: 360,
            started: true
        }));
        let mut lines = Vec::new();
        for _ in 0..60 * 60 {
            let step = r.step();
            for ev in &step {
                if let Event::DialogLine(id) = ev {
                    let line = r.e.dialog().unwrap().current_line().unwrap();
                    assert!(
                        r.e.files().read(Folder::ApcDial, &line.voice).is_some(),
                        "{language:?} {id}: no voice"
                    );
                    let calls = r.e.subtitle_font().unwrap().layout(&line.text, 640, 480);
                    let got: Vec<String> = calls.iter().map(|c| call_row(&line.text, c)).collect();
                    assert_eq!(Some(&got), want.get(&format!("TI.{id}")), "{id}");
                    lines.push(id.clone());
                }
            }
            if step.contains(&Event::DialogEnd) {
                break;
            }
        }
        assert!(lines.len() >= 2, "{language:?}: {lines:?}");
        assert_eq!(r.e.loop_type, LoopType::Warp);

        // The showcase line, its dialog played, the lines before it
        // clicked away, the line drawn.
        let dial = r.e.files().read(Folder::Loc, "DialTI.txt").unwrap();
        let (n, id, text) = showcase(&dial);
        r.e.start_dialog(n).unwrap();
        let mut screen = vec![0u16; 640 * 480];
        let (mut shown, mut other) = (None, None);
        for _ in 0..60 * 120 {
            r.e.set_clock(r.pass as f64 * PASS);
            r.e.pass(Some(&mut screen));
            r.pass += 1;
            for ev in std::mem::take(&mut r.e.events) {
                if let Event::DialogLine(l) = ev {
                    if l == id {
                        shown = Some(r.pass);
                    } else {
                        other = Some(r.pass);
                    }
                }
            }
            if shown.is_none() && other.is_some_and(|p| r.pass == p + 30) {
                r.click();
            }
            if shown.is_some_and(|p| r.pass == p + 30) {
                break;
            }
        }
        assert!(shown.is_some(), "{language:?}: {id} not shown");
        let calls = r.e.subtitle_font().unwrap().layout(&text, 640, 480);
        let got: Vec<String> = calls.iter().map(|c| call_row(&text, c)).collect();
        assert_eq!(Some(&got), want.get(&format!("TI.{id}")));
        for c in calls.iter().filter(|c| c.colour == 0xffffff) {
            let white = (c.y..c.y + 21)
                .flat_map(|y| (c.x..630).map(move |x| (x, y)))
                .filter(|&(x, y)| screen[y as usize * 640 + x as usize] == 0xffff)
                .count();
            assert!(white > 20, "{language:?} row at {}: {white}", c.y);
        }
        let wide: String = text
            .iter()
            .filter(|&&b| b >= 0x80)
            .map(|&b| atlantis_data::TextCodePage::Windows1252.char_of(b))
            .collect();
        eprintln!(
            "{}: dialog 360 {} lines; {id} drawn in {} rows ({wide})",
            language.name(),
            lines.len(),
            calls.len() / 2
        );
        if let Some(out) = std::env::var_os("ATLANTIS2_PNG_DIR") {
            let png = atlantis2_engine::png::encode_rgb565(&screen, 640, 480);
            let name = format!("gog-{}-subtitle.png", language.name().to_lowercase());
            std::fs::write(PathBuf::from(out).join(name), png).unwrap();
        }
        drop(r);
        let _ = std::fs::remove_dir_all(&data);
    }
}

/// The older builds' `Game.exe`: the bytes that differ from the current
/// one are GOG's patches (file offsets), and the game opens and boots on
/// each with the English data.
#[cfg(unix)]
#[test]
fn gogs_older_builds() {
    let (Some(old), Some(en)) = (
        std::env::var_os("ATLANTIS2_GOG_OLD").map(PathBuf::from),
        std::env::var_os("ATLANTIS2_GOG_EN").map(PathBuf::from),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_GOG_OLD / ATLANTIS2_GOG_EN not set");
        return;
    };
    let current = std::fs::read(en.join("Game.exe")).unwrap();
    let runs = |a: &[u8], b: &[u8]| -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for i in (0..a.len()).filter(|&i| a[i] != b[i]) {
            match out.last_mut() {
                Some((_, end)) if *end == i => *end = i + 1,
                _ => out.push((i, i + 1)),
            }
        }
        out
    };
    let w10 = std::fs::read(old.join("1.0-w10/Atlantis 2/Game.exe")).unwrap();
    // "1.0 w10" keeps the two `SendMessageA(HWND_BROADCAST,
    // WM_FONTCHANGE)` of 1.01 (@ 0x0046ee34, 0x0046f066) that the hotfix
    // turns into `nop`s; nothing else differs.
    let r = runs(&w10, &current);
    assert_eq!(r, [(0x6ee34, 0x6ee43), (0x6f066, 0x6f077)]);
    assert!(current[0x6ee34..0x6ee43].iter().all(|&b| b == 0x90));
    assert!(current[0x6f066..0x6f077].iter().all(|&b| b == 0x90));
    let y2018 = std::fs::read(old.join("1.0/Atlantis 2/Game.exe")).unwrap();
    if let Some(patch) = std::env::var_os("ATLANTIS2_PATCH101") {
        // "1.0" is 1.01 with five patches, each one of the current's:
        // fixed drives as the CD (@ 0x004054c7) with an empty root
        // (@ 0x004054cf), no free-space test (@ 0x00479819, 0x00479b9f),
        // the CD page closing at once (@ 0x00488e67).
        let base = std::fs::read(patch).unwrap();
        let r = runs(&y2018, &base);
        let at: Vec<usize> = r.iter().map(|&(s, _)| s).collect();
        assert_eq!(at, [0x54c7, 0x54cf, 0x79819, 0x79b9f, 0x88e67]);
        for (s, e) in r {
            assert_eq!(y2018[s..e], current[s..e], "{s:#x}");
        }
    } else {
        atlantis2_data::testing::missing("ATLANTIS2_PATCH101 not set");
    }
    for (build, exe, id) in [
        ("1.0-w10", &w10, "atlantis2-gog-w10-2183168"),
        ("1.0", &y2018, "atlantis2-gog-2018-2183168"),
    ] {
        let install = scratch(&format!("a2-gog-{build}"));
        for entry in std::fs::read_dir(&en).unwrap().flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().eq_ignore_ascii_case("Game.exe") {
                std::os::unix::fs::symlink(entry.path(), install.join(&name)).unwrap();
            }
        }
        std::fs::write(install.join("Game.exe"), exe).unwrap();
        let (files, b) = A2Files::open_dir(&install, &[]).unwrap();
        assert_eq!((b.id, b.release, b.gog), (id, Release::V101, true));
        assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
        assert!(files.label().contains("English"), "{}", files.label());
        let e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
        assert_eq!(e.files().build().id, id);
        drop(e);
        let _ = std::fs::remove_dir_all(&install);
    }
}
