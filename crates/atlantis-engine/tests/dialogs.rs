//! The dialogs against the original `atlantis.exe` (2001 English build):
//! the INITDIAL table of its parser `FUN_0042ea18`, the talking head of
//! every dialog line (`FUN_0042844c`, `FUN_0042ee88`), the SPF overlays,
//! the speaker lookup `FUN_00428f58`, the subject menu (`FUN_004261a0`,
//! `FUN_004211f0`) and the game-over screen `FUN_00426590`; then the
//! dialogs played headless: the first lines of a new game and a
//! conversation through the subject menu.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers, file names and
//! digests only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`video_dialog.h`, `video_subject_menu.h`, `scripts_gameover.h`).
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. Run them with `--release`. With
//! `ATLANTIS_DUMP_DIR` set, the headless tests write the frames they check
//! there (PPM).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::sprite::{read_spr_file, spw_frame_count};
use atlantis_data::sprite_blend::SPR_NO_BLEND;
use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::con::script::latin1_bytes;
use atlantis_engine::con::{con_game_over_narrator, con_track_letter, ConItem, ConScript};
use atlantis_engine::dialog::game_over::{
    game_over_fades_new_sound, game_over_image_name, game_over_subtitle_drawn,
    game_over_voice_name, game_over_wait_ends,
};
use atlantis_engine::dialog::rules::{head_open_fails, DialInit};
use atlantis_engine::dialog::subject_menu::{menu_draws, menu_hit, menu_subject};
use atlantis_engine::hooks::{DirectLoader, ScriptHooks};
use atlantis_engine::scripts::resolve_speaker_perso;
use atlantis_engine::subtitles::{split_subtitle, subtitle_line_y, SUBTITLE_X};
use atlantis_engine::sys::{Input, MouseButton, Runner, Sys};
use atlantis_engine::video_skip::VideoSkipState;
use atlantis_engine::{Dialogs, Engine, Event, Options, Scripts, Start};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
}

fn golden_map(text: &str) -> HashMap<&str, &str> {
    lines(text).filter_map(|l| l.rsplit_once(' ')).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

fn read(g: &Game, name: &str) -> Vec<u8> {
    g.big.read(name).unwrap_or_else(|| panic!("{name} missing"))
}

fn con_files(g: &Game) -> Vec<String> {
    let mut v: Vec<String> = g
        .big
        .names()
        .filter(|n| n.starts_with("SCENAR\\CHAPI") && n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

fn dial_init(g: &Game) -> DialInit {
    DialInit::parse(&read(g, "DIALOG\\INITDIAL.TXT"))
}

/// Writes an RGB565 640x480 frame as a PPM when `ATLANTIS_DUMP_DIR` is set.
fn dump(name: &str, frame: &[u16]) {
    let Some(dir) = std::env::var_os("ATLANTIS_DUMP_DIR") else {
        return;
    };
    let mut out = b"P6\n640 480\n255\n".to_vec();
    for &p in frame {
        let (r, g, b) = ((p >> 11) & 0x1f, (p >> 5) & 0x3f, p & 0x1f);
        out.extend([
            (r << 3 | r >> 2) as u8,
            (g << 2 | g >> 4) as u8,
            (b << 3 | b >> 2) as u8,
        ]);
    }
    let path = std::path::Path::new(&dir).join(format!("{name}.ppm"));
    std::fs::write(&path, out).expect("cannot write the dump");
    eprintln!("wrote {}", path.display());
}

/// The parser's table: 67 rows, the player character 512, every field as
/// the original stores it ("none" and the extension appended there).
#[test]
fn initdial_matches_original_parser() {
    let Some(g) = game("initdial_matches_original_parser") else {
        return;
    };
    let golden = golden_map(include_str!("golden/video_dialog_initdial.txt"));
    let t = dial_init(&g);
    assert_eq!(t.rows.len(), 67);
    assert_eq!(t.hero, 512);
    let norm = |s: &str, ext: &str| {
        let r = if s.is_empty() { "none" } else { s };
        format!("{}{ext}", r.to_ascii_lowercase())
    };
    let (mut compared, mut diffs) = (0, 0);
    for (id, d) in &t.rows {
        let fields = [
            ("s3d", norm(&d.s3d, ".s3d")),
            ("3dc", norm(&d.anim_3dc, ".3dc")),
            ("3da", norm(&d.anim_3da, ".3da")),
            ("3dam", norm(&d.anim_m, ".3da")),
            ("ubb2", norm(&d.cam_ubb[0], ".ubb")),
            ("ubb3", norm(&d.cam_ubb[1], ".ubb")),
            ("ubb4", norm(&d.cam_ubb[2], ".ubb")),
            ("ubb5", norm(&d.cam_ubb[3], ".ubb")),
            ("spf2", norm(&d.cam_spf[0], ".spf")),
            ("spf3", norm(&d.cam_spf[1], ".spf")),
            ("spf4", norm(&d.cam_spf[2], ".spf")),
            ("spf5", norm(&d.cam_spf[3], ".spf")),
        ];
        for (k, v) in fields {
            let key = format!("{id}.{k}");
            match golden.get(key.as_str()) {
                Some(&want) => {
                    compared += 1;
                    if want != v {
                        diffs += 1;
                        eprintln!("{key}: original {want}, port {v}");
                    }
                }
                None => {
                    diffs += 1;
                    eprintln!("not stored by the original: {key}");
                }
            }
        }
    }
    assert_eq!(compared, 67 * 12);
    assert_eq!(diffs, 0);
}

/// Every talking-head line of the game opens its UBB except the 7 whose
/// INITDIAL row has no record, whose head open fails.
#[test]
fn head_open_fails_match_original() {
    let Some(g) = game("head_open_fails_match_original") else {
        return;
    };
    let t = dial_init(&g);
    let (mut heads, mut fails, mut bad) = (0, 0, 0);
    for l in lines(include_str!("golden/video_dialog_heads.txt")) {
        let f: Vec<&str> = l.split(' ').collect();
        let row: i32 = f[3].parse().unwrap();
        let fails_here = head_open_fails(false, t.rows.contains_key(&row));
        match f[2] {
            "head" => {
                heads += 1;
                bad += usize::from(fails_here);
            }
            "fails" => {
                fails += 1;
                bad += usize::from(!fails_here);
            }
            _ => panic!("bad golden line {l}"),
        }
    }
    assert_eq!((heads, fails, bad), (2048, 7, 0));
    assert!(!head_open_fails(true, false), "voice-only: no UBB opened");
}

fn u32_at(g: &Game, name: &str, off: usize) -> Option<u32> {
    let d = g.big.read(name)?;
    Some(u32::from_le_bytes(d.get(off..off + 4)?.try_into().ok()?))
}

/// The SPF overlays: 128 cameras, 48 (UBB, SPF) pairs, each SPF with the
/// frame count of its UBB (31), so the port's modulo and the original's
/// reset at the UBB rewind draw the same frame.
#[test]
fn dialog_spf_frames_match_ubb() {
    let Some(g) = game("dialog_spf_frames_match_ubb") else {
        return;
    };
    let t = dial_init(&g);
    let mut pairs = BTreeSet::new();
    let mut cams = 0;
    for d in t.rows.values() {
        for c in 0..4 {
            if d.cam_spf[c].is_empty() {
                continue;
            }
            cams += 1;
            pairs.insert((
                format!("{}.ubb", d.cam_ubb[c].to_ascii_lowercase()),
                format!("{}.spf", d.cam_spf[c].to_ascii_lowercase()),
            ));
        }
    }
    assert_eq!(cams, 128);
    assert_eq!(pairs.len(), 48);
    let mut seen = BTreeSet::new();
    for l in lines(include_str!("golden/video_dialog_spf_frames.txt")) {
        let f: Vec<&str> = l.split(' ').collect();
        assert_eq!((f[0], f[3], f[5]), ("P", "ubb", "spf"), "{l}");
        let (ubb, spf) = (f[1].to_string(), f[2].to_string());
        let ubb_frames = u32_at(
            &g,
            &format!("DIALOG\\PERSO\\{}", ubb.to_ascii_uppercase()),
            0x10,
        );
        let spf_data = read(&g, &format!("DIALOG\\PERSO\\{}", spf.to_ascii_uppercase()));
        assert_eq!(ubb_frames, Some(f[4].parse().unwrap()), "{ubb}");
        assert_eq!(
            spw_frame_count(&spf_data),
            f[6].parse::<u32>().unwrap(),
            "{spf}"
        );
        assert_eq!(ubb_frames, Some(spw_frame_count(&spf_data)), "{ubb} {spf}");
        seen.insert((ubb, spf));
    }
    assert_eq!(seen, pairs);
}

/// Every speaker token of the CON files resolves to the ClicPerso the
/// original's chain writes (golden keyed by the token's MD5).
#[test]
fn speakers_match_original() {
    let Some(g) = game("speakers_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/video_dialog_speakers.txt"));
    let mut tokens = BTreeSet::new();
    for f in con_files(&g) {
        let script = ConScript::parse(&read(&g, &f));
        for sec in script.sections() {
            for it in &sec.items {
                if let ConItem::Line(l) = it {
                    tokens.insert(l.speaker.clone());
                }
            }
        }
    }
    let mut bad = 0;
    for t in &tokens {
        let key = md5_hex(&latin1_bytes(t));
        match golden.get(key.as_str()) {
            Some(v) if v.parse::<i32>().unwrap() == resolve_speaker_perso(t) => {}
            other => {
                bad += 1;
                eprintln!(
                    "speaker {key}: original {other:?}, port {}",
                    resolve_speaker_perso(t)
                );
            }
        }
    }
    assert_eq!(tokens.len(), golden.len());
    assert_eq!(bad, 0);
}

/// No dialog line of the game has a run of 100 bytes without a space (the
/// original faults on one, the port cuts it): the longest is 16.
#[test]
fn subtitles_have_spaces() {
    let Some(g) = game("subtitles_have_spaces") else {
        return;
    };
    let (mut longest, mut count) = (0, 0);
    for f in con_files(&g) {
        let script = ConScript::parse(&read(&g, &f));
        for sec in script.sections() {
            for it in &sec.items {
                if let ConItem::Line(l) = it {
                    count += 1;
                    let b = latin1_bytes(&l.text);
                    let run = b.split(|&c| c == b' ').map(<[u8]>::len).max().unwrap_or(0);
                    longest = longest.max(run);
                    assert!(split_subtitle(&b).iter().all(|s| s.len() <= 100));
                }
            }
        }
    }
    assert!(count >= 2540, "{count} dialog lines");
    assert_eq!(longest, 16);
}

/// The subject menu for 1..16 subjects: the draws, the grid and the click
/// map of the original.
#[test]
fn subject_menu_matches_original() {
    let mut out = Vec::new();
    for n in 1..=16 {
        let ids: Vec<i32> = (1..=n).collect();
        let (d, gx, gy) = menu_draws(&ids);
        let mut dl = format!("n {n} draws");
        for m in &d {
            dl += &format!(" {},{},{}", m.x, m.y, m.frame);
        }
        out.push(dl);
        out.push(format!(
            "n {n} grid x0 {gx} y0 {gy} cols {} rows {}",
            n / 5 + 1,
            n.min(4)
        ));
        let mut listing = Vec::new();
        let mut seen = BTreeSet::new();
        for y in (0..480).step_by(5) {
            if y != 0 {
                listing.push(b'\n');
            }
            for x in (320..640).step_by(5) {
                let idx = menu_hit(n, gx, gy, x, y);
                let c = if idx == -1 {
                    b'.'
                } else {
                    let s = menu_subject(&ids, idx);
                    if !(0..36).contains(&s) {
                        b'?'
                    } else {
                        b"0123456789abcdefghijklmnopqrstuvwxyz"[s as usize]
                    }
                };
                listing.push(c);
                seen.insert(c);
            }
        }
        let syms: String = seen.iter().map(|&c| char::from(c)).collect();
        out.push(format!(
            "n {n} clickmap md5 {} symbols {syms}",
            md5_hex(&listing)
        ));
    }
    let want: Vec<&str> = lines(include_str!("golden/video_subject_menu.txt")).collect();
    assert_eq!(out, want);
}

/// The game-over screen at each of the 144 game overs: image, voice,
/// subtitle, NewSound fade and the wait (its poll count for 7 inputs with
/// and without the voice).
#[test]
fn game_over_matches_original() {
    let Some(g) = game("game_over_matches_original") else {
        return;
    };
    struct Capture {
        section: i32,
        n: i32,
        letter: char,
        text: Option<String>,
    }
    let mut scripts: HashMap<String, ConScript> = HashMap::new();
    let mut capture = |con: &str, noff: usize| -> Option<Capture> {
        let script = scripts
            .entry(con.to_string())
            .or_insert_with(|| ConScript::parse(&read(&g, &format!("SCENAR\\{con}.CON"))));
        let text = script.text();
        for sec in script.sections() {
            let mut line_idx = 0;
            for (k, it) in sec.items.iter().enumerate() {
                let ConItem::Set { cmd, offset } = it else {
                    line_idx += 1;
                    continue;
                };
                let n = cmd
                    .get(..9)
                    .filter(|p| p.eq_ignore_ascii_case("gameover="))
                    .map_or(0, |_| {
                        atlantis_engine::con::script::ConText(&cmd.as_bytes()[9..]).atoi(0)
                    });
                if n == 0 {
                    continue;
                }
                // The executor passes the byte after the /set line's LF.
                let mut p = *offset as usize;
                while p < text.len() && text[p] != b'\n' {
                    p += 1;
                }
                if p + 1 != noff {
                    continue;
                }
                return Some(Capture {
                    section: sec.id,
                    n,
                    letter: con_track_letter(line_idx),
                    text: con_game_over_narrator(&sec.items, k).map(|l| l.text.clone()),
                });
            }
        }
        None
    };
    let (mut gos, mut waits, mut texts, mut bad) = (0, 0, 0, 0);
    let mut caps: HashMap<String, Capture> = HashMap::new();
    for l in lines(include_str!("golden/scripts_gameover.txt")) {
        let f: Vec<&str> = l.split(' ').collect();
        let con = f[1];
        let noff: usize = f[2]
            .trim_start_matches('@')
            .trim_end_matches(':')
            .parse()
            .unwrap();
        let key = format!("{con} {noff}");
        match f[0] {
            "G" => {
                gos += 1;
                // G <con> @<off> sec <s> n <n>: image <img> apc <apc> sub
                // a0c0=.. a0c1=.. a0c2=.. a1c0=.. a1c1=.. a1c2=.. newsound5
                // fade <f5> ... newsound0 fade <f0> ...
                let num = |k: &str| f[f.iter().position(|&x| x == k).unwrap() + 1];
                let Some(c) = capture(con, noff) else {
                    bad += 1;
                    eprintln!("game over {key}: not found by the port");
                    continue;
                };
                let img = format!("IMAGES\\{}", game_over_image_name(c.n));
                let voice = format!("WAV\\{}", game_over_voice_name(c.section, c.letter));
                let mut ok = c.n == num("n").trim_end_matches(':').parse::<i32>().unwrap()
                    && c.section == num("sec").parse::<i32>().unwrap()
                    && c.text.is_some()
                    && img.eq_ignore_ascii_case(num("image"))
                    && voice.eq_ignore_ascii_case(num("apc"));
                for a in 0..2 {
                    for k in 0..3 {
                        let tag = format!("a{a}c{k}=");
                        let v = f.iter().find_map(|x| x.strip_prefix(tag.as_str())).unwrap();
                        ok &= game_over_subtitle_drawn(a != 0) == (v != "0");
                    }
                }
                let f5 = f[f.iter().position(|&x| x == "newsound5").unwrap() + 2];
                let f0 = f[f.iter().position(|&x| x == "newsound0").unwrap() + 2];
                ok &= game_over_fades_new_sound(5) == (f5 == "2");
                ok &= game_over_fades_new_sound(0) == (f0 == "2");
                if !ok {
                    bad += 1;
                    eprintln!(
                        "game over differs: {l} (port {img} {voice} n {} sec {})",
                        c.n, c.section
                    );
                }
                caps.insert(key, c);
            }
            "T" => {
                let Some(c) = caps.get(&key) else { continue };
                texts += 1;
                let lines = split_subtitle(&latin1_bytes(c.text.as_deref().unwrap_or("")));
                let mut want = Vec::new();
                for (i, s) in lines.iter().enumerate() {
                    let y = subtitle_line_y(i, lines.len());
                    if i > 0 {
                        want.extend_from_slice(b" | ");
                    }
                    want.extend(format!("{},{},000000,\"", SUBTITLE_X + 1, y + 1).bytes());
                    want.extend_from_slice(s);
                    want.extend(format!("\" | {SUBTITLE_X},{y},ffffff,\"").bytes());
                    want.extend_from_slice(s);
                    want.push(b'"');
                }
                if md5_hex(&want) != f[4] {
                    bad += 1;
                    eprintln!("game over text differs: {key}");
                }
            }
            "W" => {
                waits += 1;
                // W <con> @<off> sound|nosound <input>: polls <p> ...
                let with_voice = f[3] == "sound";
                let input = f[4].trim_end_matches(':');
                let polls: u32 = f[6].parse().unwrap();
                // The harness: presses at the 2nd poll; the voice reads
                // "playing" 6 times then "done"; the clock +250 ms per read.
                let mut s = VideoSkipState {
                    section_play: true,
                    object_in_hand: input == "escape+objet",
                    ..VideoSkipState::default()
                };
                let (mut poll, mut pass) = (0u32, 0u64);
                loop {
                    pass += 1;
                    let now = poll + 1 == 2;
                    let left = now && input == "lbutton";
                    let right = now && input == "rbutton";
                    let esc = now && (input == "escape" || input == "escape+objet");
                    let before = s;
                    let ends = game_over_wait_ends(
                        &mut s,
                        with_voice,
                        pass > 6,
                        250 * pass,
                        left,
                        right,
                        esc,
                    );
                    // Without the voice the poll happens only on a pass that
                    // goes on; with it, on every pass.
                    if with_voice || !ends {
                        poll += 1;
                    } else {
                        s = before;
                    }
                    if ends || pass > 100 {
                        break;
                    }
                }
                if poll != polls {
                    bad += 1;
                    eprintln!("game over wait differs: {l} (port {poll} polls)");
                }
            }
            _ => panic!("bad golden line {l}"),
        }
    }
    assert_eq!((gos, texts, waits), (144, 144, 144 * 14));
    assert_eq!(bad, 0);
}

/// A headless front end: the runner stepped every 16 ms, the sound mixed
/// and dropped, so that the voices end as they would.
struct Headless {
    sys: Rc<Sys>,
    runner: Runner,
    now: u64,
    sound: Vec<f32>,
}

impl Headless {
    fn new(sys: Rc<Sys>, runner: Runner) -> Headless {
        Headless {
            sys,
            runner,
            now: 0,
            sound: vec![0.0; 2 * (22050 * 16 / 1000)],
        }
    }

    fn step(&mut self) -> bool {
        let alive = self.runner.step(self.now);
        self.sys.mixer.fill(&mut self.sound, 22050, 2);
        self.now += 16;
        alive
    }

    fn click(&mut self, x: i32, y: i32) {
        self.sys.push_input(Input::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        self.step();
        self.sys.push_input(Input::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
    }
}

/// The subtitle's white pixels in the rows of a one-line subtitle, and its
/// black shadow pixels one row and column further.
fn subtitle_pixels(frame: &[u16]) -> (usize, usize) {
    let (mut white, mut shadow) = (0, 0);
    for y in 434..434 + 14 {
        for x in 5..636 {
            let p = frame[y * 640 + x];
            if p == 0xffff {
                white += 1;
                if frame[(y + 1) * 640 + x + 1] == 0 {
                    shadow += 1;
                }
            }
        }
    }
    (white, shadow)
}

/// A new game, headless: after the intro cinematics the first line plays
/// with the pilot's talking head (its UBB over the panorama, the 3D mouth
/// over it), its voice and its subtitle, then the game goes on to place 1.
#[test]
fn new_game_plays_the_first_dialog_line() {
    let Some(g) = game("new_game_plays_the_first_dialog_line") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let opts = Options {
        trace: Some(trace.clone()),
        start: Start::NewGame,
        ..Options::default()
    };
    let runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut h = Headless::new(sys.clone(), runner);
    let mut digests = BTreeSet::new();
    let mut best: Option<(usize, Vec<u16>)> = None;
    let mut voices = 0;
    let mut frames = 0;
    while frames < 40_000 {
        assert!(h.step(), "the engine ended");
        frames += 1;
        let t = trace.borrow();
        let in_line = matches!(t.last(), Some(Event::Voice(_)));
        if in_line {
            let screen = sys.screen.borrow();
            voices = voices.max(sys.mixer.voice_count());
            digests.insert(md5_hex(
                &screen
                    .iter()
                    .flat_map(|p| p.to_le_bytes())
                    .collect::<Vec<_>>(),
            ));
            let (white, _) = subtitle_pixels(&screen);
            if best.as_ref().is_none_or(|(w, _)| white > *w) {
                best = Some((white, screen.clone()));
            }
        }
        if t.iter().any(|e| matches!(e, Event::Music(1))) {
            break;
        }
    }
    let ev = trace.borrow();
    let played: Vec<&Event> = ev
        .iter()
        .filter(|e| matches!(e, Event::LinePlayed { .. }))
        .collect();
    let Some(Event::LinePlayed {
        section,
        letter,
        video,
        frames,
        voice,
        subtitles,
    }) = played.first()
    else {
        panic!("no dialog line played: {ev:#?}");
    };
    eprintln!("{:?}", played[0]);
    assert_eq!((*section, *letter), (1, 'A'));
    let video = video.as_deref().expect("a talking head");
    assert!(video.starts_with("DIALOG\\PERSO\\BADAN1C"), "{video}");
    assert!(*frames > 20, "{frames} UBB frames");
    assert!(*voice);
    assert_eq!(*subtitles, 1);
    assert!(ev.contains(&Event::Voice("WAV\\ATA0001A.APC".into())));
    // The talking head moves, the voice plays, the subtitle is drawn in
    // white over its shadow.
    assert!(digests.len() > 20, "{} distinct frames", digests.len());
    assert!(voices >= 1);
    let (white, frame) = best.expect("no frame during the line");
    let (w, shadow) = subtitle_pixels(&frame);
    assert_eq!(w, white);
    assert!(white > 200, "{white} subtitle pixels");
    assert!(shadow > white / 2, "{shadow} shadowed of {white}");
    // The 3D head fills the pilot's mouth, which the UBB leaves black
    // (about 2500 near-black pixels of this box without the head).
    let fr = &frame;
    let dark = (190..250)
        .flat_map(|y| (80..140).map(move |x| fr[y * 640 + x]))
        .filter(|&p| {
            let (r, g, b) = ((p >> 11) & 0x1f, (p >> 5) & 0x3f, p & 0x1f);
            u32::from(r << 3 | r >> 2) + u32::from(g << 2 | g >> 4) + u32::from(b << 3 | b >> 2)
                < 60
        })
        .count();
    assert!(dark < 400, "{dark} near-black pixels where the mouth is");
    dump("dialog_first_line", &frame);
    // Then place 1.
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::PlaceEntered { place: 1, .. })));
}

/// A conversation: the guard (perso 600) of chapter 1 has four subjects;
/// the subject menu shows them over the panorama, a click on the second
/// one (subject 5) plays the guard's /con section on that subject, its
/// lines with their talking heads, voices and subtitles.
#[test]
fn guard_conversation_through_the_subject_menu() {
    let Some(g) = game("guard_conversation_through_the_subject_menu") else {
        return;
    };
    let sujets = read_spr_file(&read(&g, "SPRITE\\2D\\SUJETS.SPR"), 640, 480).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let done = Rc::new(RefCell::new(false));
    let (s2, t2, d2) = (sys.clone(), trace.clone(), done.clone());
    let task = async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.trace = Some(t2);
        let mut scripts = Scripts::with_dialog(Box::new(Dialogs::new()));
        e.new_game(&mut scripts, false).await;
        assert_eq!(scripts.subjects.subjects(600), &[12, 5, 1, 2]);
        scripts.npc_click(&mut e, 600, 0).await;
        *d2.borrow_mut() = true;
    };
    let runner = Runner::new(sys.clone(), task);
    let mut h = Headless::new(sys.clone(), runner);
    let menu_shown = |t: &RefCell<Vec<Event>>| {
        t.borrow()
            .iter()
            .any(|e| matches!(e, Event::SubjectMenu { .. }))
    };
    // Click the second cell until the menu takes it.
    let (cx, cy) = (475, 205);
    let mut menu_frame = Vec::new();
    for i in 0..2000 {
        if menu_shown(&trace) || *done.borrow() {
            break;
        }
        if i % 10 == 9 {
            menu_frame = sys.screen.borrow().clone();
            h.click(cx, cy);
        }
        assert!(h.step(), "the task ended");
    }
    assert!(menu_shown(&trace), "no subject menu");
    dump("dialog_subject_menu", &menu_frame);
    // The first cell (subject 12), away from the pointer: its border
    // (SUJETS.SPR frame 0) at (450, 130) and its icon (frame 12) at its
    // centre, pixel for pixel where they are opaque.
    let (mut opaque, mut same) = (0, 0);
    for (spr, (sx, sy)) in [(&sujets[0], (450, 130)), (&sujets[12], (475, 155))] {
        let w = usize::from(spr.w);
        for r in 0..usize::from(spr.h) {
            for c in 0..w {
                if spr.blend[r * w + c] != SPR_NO_BLEND {
                    continue;
                }
                let x = sx - i32::from(spr.xoff) + c as i32;
                let y = sy - i32::from(spr.yoff) + r as i32;
                opaque += 1;
                if menu_frame[y as usize * 640 + x as usize] == spr.pixels[r * w + c] {
                    same += 1;
                }
            }
        }
    }
    assert!(opaque > 100, "{opaque} opaque pixels");
    assert_eq!(same, opaque, "the first cell");
    // The lines play to their end (the voices end as the mixer pulls them).
    let mut n = 0;
    while !*done.borrow() {
        assert!(h.step() || *done.borrow(), "the task ended early");
        n += 1;
        assert!(n < 100_000, "the conversation does not end");
    }
    let ev = trace.borrow();
    eprintln!("{ev:#?}");
    let menu = ev
        .iter()
        .find_map(|e| match e {
            Event::SubjectMenu {
                perso,
                subjects,
                chosen,
            } => Some((*perso, subjects.clone(), *chosen)),
            _ => None,
        })
        .unwrap();
    assert_eq!(menu, (600, vec![12, 5, 1, 2], Some(5)));
    let played: Vec<(i32, char, bool)> = ev
        .iter()
        .filter_map(|e| match e {
            Event::LinePlayed {
                section,
                letter,
                voice,
                ..
            } => Some((*section, *letter, *voice)),
            _ => None,
        })
        .collect();
    assert!(!played.is_empty(), "no line played");
    assert!(played.iter().all(|&(_, _, v)| v), "{played:?}");
    let sec = played[0].0;
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::Section { id, kind: "con", .. } if *id == sec)));
}

type Task = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;

/// Runs `f` on an engine with a new game started (chapter 1's /INIT run,
/// place 450 shown), headless, until it returns; `on_step` sees each frame.
/// Returns the events.
fn with_new_game(
    g: Game,
    enhancements: atlantis_engine::Enhancements,
    f: impl FnOnce(Engine, Scripts) -> Task + 'static,
    mut on_step: impl FnMut(&mut Headless, &[Event]),
) -> Vec<Event> {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let done = Rc::new(RefCell::new(false));
    let (s2, t2, d2) = (sys.clone(), trace.clone(), done.clone());
    let task = async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.trace = Some(t2);
        e.enhancements = enhancements;
        let mut scripts = Scripts::with_dialog(Box::new(Dialogs::new()));
        e.new_game(&mut scripts, false).await;
        f(e, scripts).await;
        *d2.borrow_mut() = true;
    };
    let runner = Runner::new(sys.clone(), task);
    let mut h = Headless::new(sys, runner);
    let mut n = 0;
    while !*done.borrow() {
        h.step();
        on_step(&mut h, &trace.borrow());
        n += 1;
        assert!(n < 100_000, "the task does not end");
    }
    let ev = trace.borrow().clone();
    ev
}

/// The first line of a section of the loaded CON, as the walk hands it.
fn first_line(s: &Scripts, section: i32) -> atlantis_engine::con::ConLine {
    let sec = s
        .script
        .sections()
        .iter()
        .find(|x| x.id == section)
        .unwrap();
    sec.items
        .iter()
        .find_map(|it| match it {
            ConItem::Line(l) => Some(l.clone()),
            _ => None,
        })
        .unwrap()
}

fn request(
    section: i32,
    line: atlantis_engine::con::ConLine,
    perso: i32,
) -> atlantis_engine::scripts::LineRequest {
    atlantis_engine::scripts::LineRequest {
        section,
        letter: 'A',
        line,
        perso,
        interactive: false,
        inc_perso: 0,
    }
}

fn line_played(ev: &[Event]) -> Vec<(Option<String>, u32, bool, usize)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::LinePlayed {
                video,
                frames,
                voice,
                subtitles,
                ..
            } => Some((video.clone(), *frames, *voice, *subtitles)),
            _ => None,
        })
        .collect()
}

/// An `off` line (CHAPI001 /2 A, Seth): voice-only (`FUN_00430258`), the
/// view at the kept angles and the subtitle, no video.
#[test]
fn off_line_plays_voice_only() {
    use atlantis_engine::DialogPresenter;
    let Some(g) = game("off_line_plays_voice_only") else {
        return;
    };
    let mut best = 0;
    let ev = with_new_game(
        g,
        Default::default(),
        |mut e, s| {
            Box::pin(async move {
                let line = first_line(&s, 2);
                assert_eq!(
                    (line.speaker.as_str(), line.params.as_str()),
                    ("Seth", "off")
                );
                let perso = resolve_speaker_perso(&line.speaker);
                let mut d = Dialogs::new();
                let out = d.play_line(&mut e, &request(2, line, perso)).await;
                assert!(!out.escape_skip);
            })
        },
        |h, ev| {
            if matches!(ev.last(), Some(Event::Voice(_))) {
                best = best.max(subtitle_pixels(&h.sys.screen.borrow()).0);
            }
        },
    );
    assert_eq!(line_played(&ev), vec![(None, 0, true, 1)]);
    assert!(ev.contains(&Event::Voice("WAV\\ATA0002A.APC".into())));
    assert!(best > 100, "{best} subtitle pixels");
}

/// An `off0` line (`FUN_004305e8`): the subtitle over the screen as it
/// stands, nothing else drawn.
#[test]
fn off0_line_keeps_the_screen() {
    use atlantis_engine::DialogPresenter;
    let Some(g) = game("off0_line_keeps_the_screen") else {
        return;
    };
    let (mut frames, mut kept) = (0, true);
    let ev = with_new_game(
        g,
        Default::default(),
        |mut e, s| {
            Box::pin(async move {
                let mut line = first_line(&s, 2);
                line.params = "off0".into();
                e.sys.screen.borrow_mut().fill(0x1234);
                let mut d = Dialogs::new();
                d.play_line(&mut e, &request(2, line, 514)).await;
            })
        },
        |h, ev| {
            if matches!(ev.last(), Some(Event::Voice(_))) {
                frames += 1;
                let s = h.sys.screen.borrow();
                // Outside the subtitle's rows, the screen as it stood.
                kept &= s[..400 * 640].iter().all(|&p| p == 0x1234);
                kept &= s[460 * 640..].iter().all(|&p| p == 0x1234);
            }
        },
    );
    assert_eq!(line_played(&ev), vec![(None, 0, true, 1)]);
    assert!(frames > 10 && kept);
}

/// A talking head whose INITDIAL row has no record (CHAPI013 /180 A,
/// Servage, row 529): the error box, then nothing of the line; with the
/// UI/UX enhancements, no box.
#[test]
fn head_open_fails_shows_the_error_box() {
    use atlantis_engine::DialogPresenter;
    for ui_ux in [false, true] {
        let Some(g) = game("head_open_fails_shows_the_error_box") else {
            return;
        };
        assert!(g.big.contains("WAV\\ATA0180A.SYC"));
        let mut boxed = 0;
        let mut steps = 0;
        let enh = atlantis_engine::Enhancements {
            ui_ux,
            ..Default::default()
        };
        let ev = with_new_game(
            g,
            enh,
            |mut e, _| {
                Box::pin(async move {
                    let line = atlantis_engine::con::ConLine {
                        speaker: "Servage".into(),
                        text: "x".into(),
                        ..Default::default()
                    };
                    let mut d = Dialogs::new();
                    d.play_line(&mut e, &request(180, line, 529)).await;
                })
            },
            |h, ev| {
                if ev.iter().any(|e| matches!(e, Event::ChapterLoaded(_))) {
                    steps += 1;
                }
                let grey = h
                    .sys
                    .screen
                    .borrow()
                    .iter()
                    .filter(|&&p| p == 0xc618)
                    .count();
                if grey > 2000 {
                    boxed += 1;
                    if boxed == 20 {
                        h.sys
                            .push_input(Input::KeyDown(atlantis_engine::sys::Key::Enter));
                    }
                }
            },
        );
        assert_eq!(line_played(&ev), vec![(None, 0, false, 0)]);
        assert!(!ev.iter().any(|e| matches!(e, Event::Voice(_))));
        if ui_ux {
            assert_eq!(boxed, 0);
            assert!(steps < 5, "{steps} steps");
        } else {
            assert!(boxed >= 20, "{boxed} frames with the box");
        }
    }
}

/// Escape during the first line of the guard's section 3 (a talking head):
/// the line ends and the lines after it up to the next /set are skipped
/// (the original's walk: "esc 1 skipped 1").
#[test]
fn escape_skips_the_rest_of_the_section() {
    let Some(g) = game("escape_skips_the_rest_of_the_section") else {
        return;
    };
    let (mut clicked, mut escaped) = (0, false);
    let ev = with_new_game(
        g,
        Default::default(),
        |mut e, mut s| {
            Box::pin(async move {
                s.npc_click(&mut e, 600, 0).await;
            })
        },
        |h, ev| {
            let menu = ev.iter().any(|e| matches!(e, Event::SubjectMenu { .. }));
            if !menu {
                clicked += 1;
                if clicked % 10 == 0 {
                    h.click(475, 205);
                }
            } else if !escaped && ev.contains(&Event::Voice("WAV\\ATA0003A.APC".into())) {
                for _ in 0..10 {
                    h.step();
                }
                h.sys
                    .push_input(Input::KeyDown(atlantis_engine::sys::Key::Escape));
                escaped = true;
            }
        },
    );
    assert!(escaped);
    let played: Vec<char> = ev
        .iter()
        .filter_map(|e| match e {
            Event::LinePlayed {
                section: 3, letter, ..
            } => Some(*letter),
            _ => None,
        })
        .collect();
    assert_eq!(played, vec!['A']);
}

/// The game-over screen of CHAPI003's section 45 (GameOver=2): GOVER02.TGA
/// with the narrator's subtitle, the voice ATA0045C until it ends, then a
/// black screen.
#[test]
fn game_over_screen_plays_the_narrator() {
    use atlantis_engine::DialogPresenter;
    let Some(g) = game("game_over_screen_plays_the_narrator") else {
        return;
    };
    let (mut image, mut best) = (0, 0);
    let mut last = Vec::new();
    let ev = with_new_game(
        g,
        Default::default(),
        |mut e, _| {
            Box::pin(async move {
                let req = atlantis_engine::scripts::GameOverRequest {
                    section: 45,
                    n: 2,
                    narrator: Some(("Young Seth had failed.".into(), 'C')),
                };
                let mut d = Dialogs::new();
                d.game_over(&mut e, &req).await;
            })
        },
        |h, ev| {
            let s = h.sys.screen.borrow();
            if matches!(ev.last(), Some(Event::Voice(_))) {
                image += 1;
                best = best.max(subtitle_pixels(&s).0);
                if image == 10 {
                    dump("dialog_game_over", &s);
                }
            }
            last = s.clone();
        },
    );
    assert!(ev.contains(&Event::Voice("WAV\\ATA0045C.APC".into())));
    assert!(image > 30, "{image} frames of the image");
    assert!(best > 50, "{best} subtitle pixels");
    assert!(last.iter().all(|&p| p == 0), "the screen cleared");
}
