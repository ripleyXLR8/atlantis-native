//! The dialogs against the original's own results:
//!
//! - `tests/golden/dialog_script_ti.txt`: every Tibet dialog initialised
//!   (the lines, their speakers, stands, cameras drawn with `rand`, video
//!   and voice files, texts) as `dlg_Dialog::Initialise` does it;
//! - `tests/golden/dialog_frames.txt`: the body video stream (its frame
//!   numbers and loop), and the dialog frame composed of the video through
//!   its mask and the head, frame by frame;
//! - `tests/golden/subtitles.txt`: the line breaks and positions of every
//!   subtitle of the five worlds;
//! - `tests/golden/music_fade_*.txt`, `sound_volume.txt`: the music fades
//!   (the dialogs duck the music to 16 %), the volume and pan tables and
//!   the default levels;
//! - a whole Tibet dialog played by the engine: lines, voices, video,
//!   panorama shots, music.
//!
//! The game-data tests need `ATLANTIS2_DATA` (the installed folder) and
//! `ATLANTIS2_DISCS` (a folder holding the four CD images or copies);
//! without them they are skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::dialog::{parse_dial, FontTxt, World};
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::dialog::{compose, load_head, Dialog, DialogScript};
use atlantis2_engine::hnm_stream::HnmStream;
use atlantis2_engine::raster3d::{HEIGHT, WIDTH};
use atlantis2_engine::sound::{
    attenuation_table, pan_table, volume_to_attenuation, MusicVolume, DEFAULT_FX_VOLUME,
    DEFAULT_MASTER, DEFAULT_MUSIC_VOLUME, DEFAULT_VOICE_VOLUME,
};
use atlantis2_engine::subtitle::{layout, SubtitleFont};
use atlantis2_engine::{Engine, Fpu, Variant};
use md5::{Digest, Md5};

const SCRIPT: &str = include_str!("golden/dialog_script_ti.txt");
const FRAMES: &str = include_str!("golden/dialog_frames.txt");
const SUBTITLES: &str = include_str!("golden/subtitles.txt");
const LIPSYNC: &str = include_str!("golden/lipsync_tibet.txt");
const VOLUME: &str = include_str!("golden/sound_volume.txt");
const FADES: [(&str, Fpu); 3] = [
    (include_str!("golden/music_fade_027f.txt"), Fpu::Double),
    (include_str!("golden/music_fade_007f.txt"), Fpu::Single),
    (include_str!("golden/music_fade_0e7f.txt"), Fpu::Chop),
];

fn hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn le(px: &[u16]) -> Vec<u8> {
    px.iter().flat_map(|p| p.to_le_bytes()).collect()
}

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA or ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// The fields of a golden line after its first word.
fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().filter(|&f| f != "|").collect()
}

/// Every Tibet dialog, in order, from one `srand(0)`: as the original
/// initialised them.
#[test]
fn tibet_dialogs_initialise_as_the_original() {
    let Some(files) = game() else { return };
    let script = DialogScript::load(&files, World::of_monde(5).unwrap(), Fpu::Double).unwrap();
    let mut rand = Rand::new(0);
    let mut dialogs = 0;
    let mut lines_checked = 0;
    let mut current: Option<Dialog> = None;
    for line in SCRIPT.lines().filter(|l| !l.starts_with('#')) {
        let f = fields(line);
        match f[0] {
            "dialog" => {
                let n: u32 = f[1].parse().unwrap();
                // The golden's word: 0x027f (and the C heads).
                let d = Dialog::initialise(n, &script, &files, &mut rand, Variant::C, Fpu::Double)
                    .unwrap();
                assert_eq!(f[3], "1", "{n} ok");
                assert_eq!(d.lines().len().to_string(), f[5], "{n} lines");
                assert_eq!(d.speakers().len().to_string(), f[7], "{n} speakers");
                current = Some(d);
                dialogs += 1;
            }
            "rep" => {
                let d = current.as_ref().unwrap();
                let (n, k): (u32, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let l = &d.lines()[k];
                let tag = format!("{n} line {k}");
                let num = |v: Option<usize>| v.map_or(-1, |x| x as i64).to_string();
                assert_eq!(l.perso.to_string(), f[3], "{tag} speaker");
                assert_eq!(num(l.stand), f[4], "{tag} stand");
                assert_eq!(num(l.camera), f[5], "{tag} camera");
                assert_eq!(l.mood.to_string(), f[6], "{tag} mood");
                let kind = match l.kind {
                    atlantis2_data::dialog::LineKind::Head => 0,
                    atlantis2_data::dialog::LineKind::VoiceOver => 1,
                    atlantis2_data::dialog::LineKind::VoiceOver0 => 2,
                    atlantis2_data::dialog::LineKind::Cinema => 3,
                };
                assert_eq!(kind.to_string(), f[7], "{tag} kind");
                assert_eq!(
                    (l.from.to_string(), l.to.to_string()),
                    (f[8].into(), f[9].into())
                );
                // The original's paths start with the CD's root and the
                // folder: `<root>datas_a2\hnm\dial/shs1c1.hnm`.
                let video = f[10].rsplit_once("datas_a2\\hnm\\").map_or("-", |(_, v)| v);
                assert_eq!(l.video.as_deref().unwrap_or("-"), video, "{tag} video");
                let voice = f[11].rsplit('\\').next().unwrap();
                assert_eq!(l.voice, voice, "{tag} voice");
                // The original's text holds the line's CR as a NUL.
                let mut text = l.text.clone();
                text.push(0);
                assert_eq!(
                    (text.len().to_string(), hex(&text)),
                    (f[12].into(), f[13].into()),
                    "{tag} text"
                );
                lines_checked += 1;
            }
            "perso" => {
                let d = current.as_ref().unwrap();
                let k: usize = f[2].parse().unwrap();
                let got: Vec<String> = d.speakers()[k]
                    .1
                    .iter()
                    .map(|(s, c)| format!("{s}:{c}"))
                    .collect();
                assert_eq!(got.join(" "), f[3..].join(" "), "{} speaker {k}", f[1]);
            }
            _ => {}
        }
    }
    assert_eq!(dialogs, 26);
    assert_eq!(lines_checked, 82);
}

/// The harness's background under the dialog frames.
fn background() -> Vec<u16> {
    let (w, h) = (WIDTH, HEIGHT);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = (x * 31 / (w - 1), y * 63 / (h - 1), (x + y) & 31);
            out.push(((r << 11) | (g << 5) | b) as u16);
        }
    }
    out
}

/// The body video stream and the dialog frame, tick by tick: frame
/// numbers 1..40 then 1 again, the video through its mask, the head over
/// it.
#[test]
fn dialog_frames_match_the_original() {
    let Some(files) = game() else { return };
    let bg = background();
    let mut cases: Vec<(String, String)> = Vec::new();
    for line in FRAMES.lines().filter(|l| l.starts_with("open ")) {
        let f = fields(line);
        cases.push((f[1].into(), f[2].into()));
        let _ = (&f[3], &f[4]);
    }
    assert_eq!(cases.len(), 4);
    let mut ticks = 0;
    for (db, video) in cases {
        let open = FRAMES
            .lines()
            .find(|l| l.starts_with(&format!("open {db} {video} ")))
            .map(fields)
            .unwrap();
        let mesh = db.strip_suffix("s1.db").unwrap();
        // The golden's heads: the C routines under 0x027f.
        let mut head = load_head(&files, mesh, 0, Variant::C, Fpu::Double).expect(&db);
        let data = files
            .read(Folder::Hnm, &format!("dial/{video}"))
            .expect(&video);
        let mut stream = HnmStream::open(&video, Arc::from(data)).unwrap();
        assert_eq!(
            stream.number().to_string(),
            open[3],
            "{video} after the open"
        );
        assert_eq!(
            u8::from(stream.has_alpha()).to_string(),
            open[4],
            "{video} AL"
        );
        let mut rand = Rand::new(0);
        for line in FRAMES
            .lines()
            .filter(|l| l.starts_with(&format!("tick {db} {video} ")))
        {
            let f = fields(line);
            let (cam, t): (usize, usize) = (f[3].parse().unwrap(), f[4].parse().unwrap());
            if t > 0 {
                stream.next_frame().unwrap();
            }
            let n = stream.number();
            assert_eq!(n.to_string(), f[5], "{video} tick {t} frame number");
            head.pose(n);
            head.mouth(1, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
            let mut px = bg.clone();
            let mut z = vec![0u32; WIDTH * HEIGHT];
            compose(
                &mut px,
                &mut z,
                WIDTH,
                Some(&stream),
                Some((&mut head, cam)),
                &mut rand,
            );
            let zb: Vec<u8> = z.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                (hex(&le(&px)), hex(&zb)),
                (f[6].to_string(), f[7].to_string()),
                "{video} camera {cam} tick {t}"
            );
            let mut plain = bg.clone();
            compose(&mut plain, &mut z, WIDTH, Some(&stream), None, &mut rand);
            assert_eq!(hex(&le(&plain)), f[8], "{video} tick {t} without the head");
            ticks += 1;
        }
    }
    assert_eq!(ticks, 44 + 3 + 3 + 3);
}

/// The synthetic texts of the golden (not game content).
fn synthetic() -> Vec<Vec<u8>> {
    let mut t = vec![b"Court.".to_vec()];
    let mut s = b"Un mot ".to_vec();
    s.extend(b"tr\xe8s".repeat(40));
    s.extend(b" fin.");
    t.push(s);
    t.push(
        b"Caf\xe9 au lait, th\xe9 au citron, \xe9t\xe9 \xe0 la plage, d\xe9j\xe0 vu, ".repeat(3),
    );
    t.push(b"Une phrase qui finit sur une lettre accentu\xe9e : caf\xe9".to_vec());
    t.push(b"x ".repeat(400));
    t.push(b"MMMMMMMMM ".repeat(100));
    t.push(Vec::new());
    t
}

/// Every subtitle of the five worlds (and synthetic texts): the breaks,
/// the lines, their positions and colours, with the font's own widths.
#[test]
fn subtitles_match_the_original() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let ttf = files.read(Folder::Loc, "Atl_Font.ttf").unwrap();
    let font = SubtitleFont::new(ttf, &font_txt).unwrap();
    let breaks: Vec<String> = font_txt.breaks.iter().map(|b| format!("{b:04x}")).collect();
    let want_breaks = SUBTITLES
        .lines()
        .find(|l| l.starts_with("breaks "))
        .unwrap();
    assert_eq!(fields(want_breaks)[1..].join(" "), breaks.join(" "));
    let mut texts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for w in ["CH", "IR", "SH", "TI", "YU"] {
        let data = files.read(Folder::Loc, &format!("Dial{w}.txt")).unwrap();
        for d in parse_dial(&data) {
            texts.insert(format!("{w}.{}", d.id), d.text);
        }
    }
    for (k, t) in synthetic().into_iter().enumerate() {
        texts.insert(format!("synthetic.{k}"), t);
    }
    let mut calls: BTreeMap<&str, Vec<Vec<&str>>> = BTreeMap::new();
    let mut count = 0;
    for line in SUBTITLES.lines() {
        let f = fields(line);
        match f.first() {
            Some(&"text") => {
                let text = &texts[f[1]];
                assert_eq!(
                    (text.len().to_string(), hex(text)),
                    (f[2].into(), f[3].into()),
                    "{}",
                    f[1]
                );
                calls.entry(f[1]).or_default();
                count += 1;
            }
            Some(&"call") => calls.entry(f[1]).or_default().push(f),
            _ => {}
        }
    }
    assert_eq!(count, 1221);
    let mut bad = Vec::new();
    for (key, want) in &calls {
        let text = &texts[*key];
        let got = layout(text, |b| font.extent(b), 12, &font_txt.breaks, 640, 480);
        let got: Vec<String> = got
            .iter()
            .map(|c| {
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
            })
            .collect();
        let want: Vec<String> = want.iter().map(|f| f[3..].join(" ")).collect();
        if got != want {
            bad.push(format!("{key}: got {got:?}, want {want:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} texts differ, first: {:#?}",
        bad.len(),
        &bad[..bad.len().min(3)]
    );
}

/// The volume and pan tables, the default levels and the music fades,
/// under the three control words. Every row is compared but the name and
/// handle of the `after` rows and the `idle_start` row: they are the music
/// player's (the file opened, a start while stopped), which Native does
/// not have yet.
#[test]
fn music_fades_and_volumes_match_the_original() {
    let att = attenuation_table();
    let pan = pan_table();
    let mut n = 0;
    for line in VOLUME.lines().filter(|l| !l.starts_with('#')) {
        let (key, v) = line.split_once(' ').unwrap();
        let v: i32 = v.parse().unwrap();
        if let Some(i) = key.strip_prefix("voltab.") {
            assert_eq!(att[i.parse::<usize>().unwrap()], v, "{key}");
        } else if let Some(i) = key.strip_prefix("pantab.") {
            assert_eq!(pan[i.parse::<usize>().unwrap()], v, "{key}");
        } else if let Some(rest) = key.strip_prefix("vol2att.m") {
            let (m, vol) = rest.split_once(".v").unwrap();
            assert_eq!(
                volume_to_attenuation(&att, m.parse().unwrap(), vol.parse().unwrap()),
                v,
                "{key}"
            );
        } else {
            let want = match key {
                "defaults.master" => DEFAULT_MASTER,
                "defaults.fx" => DEFAULT_FX_VOLUME,
                "defaults.music" => DEFAULT_MUSIC_VOLUME,
                "defaults.voice" => DEFAULT_VOICE_VOLUME,
                _ => panic!("{key}"),
            };
            assert_eq!(want, v, "{key}");
        }
        n += 1;
    }
    assert_eq!(n, 2 * 128 + 9 * 139 + 4);
    for (golden, fpu) in FADES {
        let mut m: Option<MusicVolume> = None;
        let mut rows = 0;
        let mut fatal = None;
        for line in golden.lines().filter(|l| !l.starts_with('#')) {
            let (key, rest) = line.split_once(' ').unwrap();
            let kv: BTreeMap<&str, &str> = rest
                .split_whitespace()
                .filter_map(|w| w.split_once('='))
                .collect();
            let tag = format!("{:04x} {key}", fpu.control_word());
            if key == "fatal" {
                fatal = Some(rest.to_string());
            } else if key.ends_with(".do") {
                let w: Vec<&str> = rest.split_whitespace().collect();
                match w[0] {
                    "init" => {
                        // The harness sets these three and keeps the
                        // other fade globals.
                        let g = |k: &str| kv[k].parse::<i32>().unwrap();
                        match m.as_mut() {
                            None => {
                                m = Some(MusicVolume::new(fpu, g("maxvol"), g("vol"), g("state")))
                            }
                            Some(m) => {
                                m.max = g("maxvol");
                                m.volume = g("vol");
                                m.state = g("state");
                            }
                        }
                    }
                    "setpct" => m.as_mut().unwrap().set_percent(w[1].parse().unwrap()),
                    "stop" => m.as_mut().unwrap().stop(),
                    "start" => {
                        let mark: i32 = w[1].parse().unwrap();
                        m.as_mut().unwrap().switch(if mark != 0 { 2 } else { 1 });
                    }
                    other => panic!("{tag}: {other}"),
                }
            } else if key.ends_with(".armed") {
                let m = m.as_ref().unwrap();
                assert_eq!(m.start_volume.to_string(), kv["startvol"], "{tag}");
                assert_eq!(m.fade_target.to_string(), kv["target"], "{tag}");
                assert_eq!(
                    format!("{:08x}", m.duration.to_bits()),
                    kv["duration"],
                    "{tag}"
                );
                assert_eq!(
                    format!("{:08x}", m.start_time.to_bits()),
                    kv["start_time"],
                    "{tag}"
                );
                rows += 1;
            } else if key.ends_with(".step") {
                let m = m.as_mut().unwrap();
                let clock = f64::from_bits(u64::from_str_radix(kv["clock"], 16).unwrap());
                let set = m.step(clock).map_or("-".to_string(), |a| a.to_string());
                let got = (m.volume, m.state, m.next, m.target, set);
                let want = (
                    kv["vol"].parse().unwrap(),
                    kv["state"].parse().unwrap(),
                    kv["next"].parse().unwrap(),
                    kv["target"].parse().unwrap(),
                    kv["set"].to_string(),
                );
                assert_eq!(got, want, "{tag}");
                rows += 1;
            } else if key.ends_with(".after") {
                // A music switch's end: the target is the music volume.
                assert_eq!(
                    m.as_ref().unwrap().target.to_string(),
                    kv["target"],
                    "{tag}"
                );
                rows += 1;
            } else if key.ends_with(".idle_pct") {
                // FUN_004770c0(20) while no music plays: refused, the
                // previous target kept.
                let m = m.as_mut().unwrap();
                m.set_percent(20);
                let got = (m.state, m.target);
                let want = (kv["state"].parse().unwrap(), kv["target"].parse().unwrap());
                assert_eq!(got, want, "{tag}");
                rows += 1;
            } else {
                assert!(key.ends_with(".idle_start"), "{tag}");
            }
        }
        assert!(rows > 400, "{rows}");
        assert_eq!(fatal.as_deref(), Some("0"));
    }
}

/// Tibet's first dialog (360) played by the engine at 60 frames a second
/// from the arrival in view 7: the three lines follow their voices, the
/// first voice is the one the lip-sync golden analysed, the video loops
/// frames 1..40, the hero's line is a reverse shot with its own field of
/// view, the music ducks to 16 % and comes back. Native has no music
/// player yet, so no music plays and the duck is refused, as the original
/// refuses it (the fade golden's `idle_pct`); the test then sets the
/// player's state to "playing" (4) at the default volume, as the fade
/// golden's `init` does, to see the duck.
#[test]
fn dialog_360_plays_end_to_end() {
    let Some(files) = game() else { return };
    let mut e = Engine::new_game(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    // The cut from view 6 (zone 0's aim).
    e.enter_view(7, (-164.394f32).to_radians(), 14.96f32.to_radians())
        .unwrap();
    e.start_dialog(360).unwrap();
    assert_eq!((e.music.state, e.music.volume), (0, 0));
    e.end_dialog();
    assert_eq!((e.music.state, e.music.volume), (0, 0));
    e.music.state = 4;
    e.music.volume = DEFAULT_MUSIC_VOLUME;
    e.start_dialog(360).unwrap();
    let want: BTreeMap<&str, &str> = LIPSYNC
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .collect();
    let mut screen = vec![0u16; 640 * 480];
    let mut seen: Vec<(String, f64)> = Vec::new();
    let mut numbers = Vec::new();
    let mut yaws = Vec::new();
    let mut min_music = 100;
    let mut frame = 0u64;
    while e.dialog().is_some() && frame < 60 * 30 {
        let t = frame as f64 / 60.0;
        e.set_clock(t);
        e.frame();
        e.render(&mut screen);
        min_music = min_music.min(e.music.volume);
        if let Some(d) = e.dialog() {
            // The heads follow the panorama's CPU: the Pentium II's
            // routines, under the 0x0e7f warpPII and the II set-up leave.
            assert_eq!(d.variant(), Variant::PII);
            assert_eq!((d.fpu(), e.fpu()), (Fpu::Chop, Fpu::Chop));
            if let Some(l) = d.current_line() {
                if seen.last().is_none_or(|(id, _)| *id != l.id) {
                    seen.push((l.id.clone(), t));
                    yaws.push((e.angles_deg().0, e.camera.fx));
                    let ls = d.lipsync().expect("voice analysed");
                    let k = |s: &str| want[format!("line.{}.{s}", l.id).as_str()];
                    assert_eq!(ls.blocks().to_string(), k("blocks"), "{}", l.id);
                    assert_eq!(ls.max_energy().to_string(), k("energy_max"), "{}", l.id);
                    let pcm: Vec<u8> = d
                        .voice()
                        .unwrap()
                        .0
                        .iter()
                        .flat_map(|s| s.to_le_bytes())
                        .collect();
                    assert_eq!(hex(&pcm), k("pcm_md5"), "{} voice", l.id);
                }
                if l.id == "360_01" {
                    numbers.push(d.stream().unwrap().number());
                }
            }
        }
        frame += 1;
    }
    let ids: Vec<&str> = seen.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(ids, ["360_01", "360_02", "360_03"]);
    // Each line lasts its voice, rounded up to the ticks: 360_01 has
    // 60193 samples (2.730 s).
    let first = seen[1].1 - seen[0].1;
    assert!((2.73..2.73 + 3.0 / 15.0).contains(&first), "{first}");
    assert!(numbers.contains(&40) && numbers.windows(2).any(|w| w == [40, 1]));
    assert!(!numbers.contains(&0));
    // The hero's line turns the view around; the shaman's turns it back.
    let turn = |a: f32, b: f32| ((a - b).rem_euclid(360.0) - 180.0).abs() < 0.01;
    assert!(
        turn(yaws[0].0, yaws[1].0) && turn(yaws[1].0, yaws[2].0),
        "{yaws:?}"
    );
    assert_eq!(min_music, 16);
    assert!(e.dialog().is_none());
    // Back up to 100 % in 84 / 127 s.
    for k in 0..60 {
        e.set_clock((frame + k) as f64 / 60.0);
    }
    assert_eq!(e.music.volume, 100);
}
