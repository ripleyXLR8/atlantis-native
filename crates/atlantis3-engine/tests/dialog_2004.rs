//! *Atlantis Evolution*'s 2004 dialogs (`ATLANTIS4_CD`): the cabin's
//! `DIAL=4,off` (chapi001.con /003, the open trunk clicked) and
//! `DIAL=5,off` (/007) are voice-overs of the narrator: the voice
//! `loc\voix\gbr\AT0004 01.ogg`, the panorama, the line's text, the end at
//! the voice's end or a click, the section going on; `DIAL=16` (a `HERO`
//! line, the player perso `Hewitt`) shows a head film of `binkdial\`
//! letterboxed in the 800 × 600 surface.

use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};
use atlantis3_engine::px::Surface;

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    Some(Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game"))
}

/// The cabin's view 1 after the intro, at clock `t`.
fn cabin() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, 0.01))
}

fn object(g: &Game, name: &str) -> i32 {
    g.names()
        .id(defnames::OBJETS, name.as_bytes())
        .unwrap_or_else(|_| panic!("{name}")) as i32
}

/// Steps 25 frames a second until `done` or `secs` passed.
fn run_until(
    g: &mut Game,
    t: &mut f64,
    secs: f64,
    mut done: impl FnMut(&mut Game) -> bool,
) -> bool {
    let end = *t + secs;
    while *t < end {
        *t += 0.04;
        g.step(*t);
        if done(g) {
            return true;
        }
    }
    false
}

#[test]
fn the_trunk_says_dialog_4() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let lang = g.startup.voice_language();
    // /003: `con(clicobjet=MALLE_OUVERTE)&(flag3=1)` — the open trunk
    // clicked (the 2004 objects' clicks are the place animations' part:
    // the click's variables are set here as the click sets them).
    let id = object(&g, "MALLE_OUVERTE");
    let s = g.script_mut().unwrap();
    s.flags[3] = 1;
    assert!(s.set_var("CLICOBJET", id));
    // The click's scan (`CtrlScenarIN`): /003 runs to its `DIAL` and waits.
    g.ctrl_scenar_in();
    assert_eq!(g.dialog_2004(), Some((4, 1, true, false)));
    assert_eq!(g.mode(), Mode::Dialog);
    let m = g.dialog_2004_media().unwrap();
    assert_eq!(m.voice, format!("loc/voix/{lang}/AT000401.ogg"));
    assert!(m.voice_playing, "the voice plays");
    assert_eq!(m.head, None, "a voice-over has no head");
    assert_eq!(
        m.text,
        b"Polishing camera lenses was out of the question.".to_vec()
    );
    // The voice ends the dialog; the section goes on (/003's next lines:
    // the trunk closes, flag3 = 2).
    assert!(
        run_until(&mut g, &mut t, 20.0, |g| g.dialog_2004().is_none()),
        "the dialog did not end"
    );
    assert_eq!(g.mode(), Mode::Warp);
    t += 0.04;
    g.step(t);
    assert_eq!(
        g.script().unwrap().flags[3],
        2,
        "/003 went on after the dialog"
    );
}

#[test]
fn a_click_ends_dialog_5() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let lang = g.startup.voice_language();
    g.startup.config.subtitle_on = true;
    g.run_command("(DIAL=5,off)");
    assert_eq!(g.dialog_2004(), Some((5, 1, true, false)));
    let m = g.dialog_2004_media().unwrap();
    assert_eq!(m.voice, format!("loc/voix/{lang}/AT000501.ogg"));
    assert!(m.voice_playing);
    assert!(m
        .text
        .starts_with(b"I didn't really need to pull the trunk"));
    t += 0.04;
    g.step(t);
    assert_eq!(g.mode(), Mode::Dialog);
    // The text is drawn at the flip: the screen's bottom has white pixels.
    let (w, h) = (g.width(), g.height());
    let white = |g: &Game| match g.surface() {
        Surface::Rgb565(p) => p[h * 3 / 4 * w..].iter().filter(|&&c| c == 0xffff).count(),
        Surface::Xrgb8888(p) => p[h * 3 / 4 * w..]
            .iter()
            .filter(|&&c| c & 0x00ff_ffff == 0x00ff_ffff)
            .count(),
    };
    if let Ok(dir) = std::env::var("DIALOG_2004_PNG") {
        write_ppm(&g, &std::path::Path::new(&dir).join("dialog5.ppm"));
    }
    assert!(white(&g) > 100, "the subtitle is drawn");
    // A click: past the last line, the dialog closes.
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(g.dialog_2004(), None);
    assert_eq!(g.mode(), Mode::Warp);
}

#[test]
fn a_hero_line_shows_his_head() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    assert_eq!(g.player_2004(), "Hewitt");
    g.run_command("(DIAL=16)");
    assert_eq!(g.dialog_2004(), Some((16, 1, false, false)));
    assert_eq!(g.mode(), Mode::Dialog);
    let m = g.dialog_2004_media().unwrap();
    assert_eq!(m.text, b"Aw shoot! Ouch!".to_vec());
    let (head, first) = m.head.expect("Hewitt's head film");
    assert!(
        [
            "binkdial/hh0000.bik",
            "binkdial/hh0001.bik",
            "binkdial/hh0002.bik"
        ]
        .contains(&head.as_str()),
        "{head}"
    );
    // The head plays at 15 frames a second.
    t += 0.5;
    g.step(t);
    let (_, later) = g.dialog_2004_media().unwrap().head.unwrap();
    assert_ne!(later, first);
    // Letterboxed: the top rows black, the head's rows not all black.
    let (w, h) = (g.width(), g.height());
    let row_black = |g: &Game, y: usize| match g.surface() {
        Surface::Rgb565(p) => p[y * w..(y + 1) * w].iter().all(|&c| c == 0),
        Surface::Xrgb8888(p) => p[y * w..(y + 1) * w].iter().all(|&c| c & 0xffffff == 0),
    };
    assert!(row_black(&g, 2));
    assert!(
        (h / 4..h / 2).any(|y| !row_black(&g, y)),
        "the head is drawn"
    );
    if let Ok(dir) = std::env::var("DIALOG_2004_PNG") {
        let path = std::path::Path::new(&dir).join("dialog16.ppm");
        write_ppm(&g, &path);
    }
    // Clicks end it.
    for _ in 0..3 {
        g.click();
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.dialog_2004(), None);
    assert_eq!(g.mode(), Mode::Warp);
}

/// The screen as a PPM (tests' screenshots).
fn write_ppm(g: &Game, path: &std::path::Path) {
    let (w, h) = (g.width(), g.height());
    let mut out = format!("P6 {w} {h} 255\n").into_bytes();
    let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8];
    match g.surface() {
        Surface::Rgb565(p) => {
            for &c in p {
                let c = u32::from(c);
                out.extend([
                    ((c >> 11) << 3) as u8,
                    (((c >> 5) & 63) << 2) as u8,
                    ((c & 31) << 3) as u8,
                ]);
            }
        }
        Surface::Xrgb8888(p) => {
            for &c in p {
                out.extend(rgb(c));
            }
        }
    }
    std::fs::write(path, out).unwrap();
}

/// Every `DIAL=` of the CD's scripts names dialogs of `dial.txt` whose
/// speakers load, and each of their lines has its voice; `PLAYERNAME` is a
/// variable and every name it is set to is a player perso.
#[test]
fn every_dial_of_the_scripts() {
    use atlantis3_data::evolution2004_dialog::{
        is_player, parse_com_2004, parse_info_2004, voice_name, DialOptions, Dialogs2004,
    };
    let Some(g) = game() else { return };
    assert!(g.script().unwrap().var("PLAYERNAME").is_some());
    let dir = std::path::PathBuf::from(std::env::var_os("ATLANTIS4_CD").unwrap());
    let scenar = dir.join("cd0/datas/Scenar");
    let read = |p: &std::path::Path| std::fs::read(p).unwrap();
    let info = parse_info_2004(&read(&scenar.join("info.txt")));
    let dial = atlantis3_data::dialog::parse_dial_2004(&read(
        &dir.join("cd0/datas/loc/text/gbr/Dial.txt"),
    ));
    let com = parse_com_2004(&read(&scenar.join("com.txt")));
    let d = Dialogs2004::build(info, &dial, &com);
    assert_eq!(d.issues, vec![], "every dial.txt and com.txt line loads");
    let voices: std::collections::HashSet<String> =
        std::fs::read_dir(dir.join("cd0/datas/loc/voix/gbr"))
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
            })
            .collect();
    let (mut dials, mut lines, mut missing) = (0, 0, Vec::new());
    let mut names = std::collections::BTreeSet::new();
    for e in std::fs::read_dir(&scenar).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| !x.eq_ignore_ascii_case("con")) {
            continue;
        }
        let text = read(&p);
        for line in text.split(|&c| c == b'\n') {
            let l = line.to_ascii_lowercase();
            if let Some(i) = l.windows(11).position(|w| w == b"playername=") {
                let name: Vec<u8> = line[i + 11..]
                    .iter()
                    .copied()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                names.insert(String::from_utf8(name).unwrap());
            }
            let Some(i) = l.windows(5).position(|w| w == b"dial=") else {
                continue;
            };
            let o = DialOptions::parse(&line[i + 5..]);
            assert!(!o.numbers.is_empty(), "{}", String::from_utf8_lossy(line));
            for n in o.numbers {
                dials += 1;
                assert!(d.count(n) > 0, "dialog {n} of {p:?} is not in dial.txt");
                for l in 1..=d.count(n) {
                    lines += 1;
                    assert!(d.entry(n, l).is_some(), "dialog {n} line {l}");
                    if !voices.contains(&voice_name(n, l).to_ascii_lowercase()) {
                        missing.push((n, l));
                    }
                }
            }
        }
    }
    assert!(dials > 500, "{dials}");
    assert!(
        missing.is_empty(),
        "{} of {lines} voices missing: {missing:?}",
        missing.len()
    );
    for n in &names {
        assert!(is_player(n), "PLAYERNAME={n}");
    }
}

/// `DIAL=161` (chapi012 /001, the lizard clicked at `grottex.wam`): line 2's
/// speaker `Batracien` has `[Spw]`, `[IdText] LEZARD`, `[Neutre] 3`
/// (info.txt): no head film; FUN_0046d3f0 shows the `LEZARD` slot playing
/// animation 3 (the others of `LEZARD` hidden) and `Perform` draws the
/// panorama in rows 76 .. h − 77 of the back buffer, black above and
/// below (FUN_00444870 with 0x4c).
#[test]
fn a_spw_speaker_shows_its_place_animation() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    for c in ["(changecd=2)", "(changewam=grottex.wam)", "(chapter=12)"] {
        g.run_command(c);
    }
    let lezard = g
        .names()
        .id(defnames::PERSONNAGES, b"LEZARD")
        .expect("LEZARD") as i32;
    // The view with the lizard's slot of animation 3.
    let mut view = None;
    for v in 1..40 {
        if g.wam().view(v).is_none() {
            continue;
        }
        g.run_command(&format!("(vue={v})"));
        t += 0.04;
        g.step(t);
        if g.mode() != Mode::Warp {
            continue;
        }
        if g.slots()
            .slots
            .iter()
            .any(|s| s.kind == 1 && s.id == lezard && s.state.anim == 3)
        {
            view = Some(v);
            break;
        }
    }
    let Some(view) = view else {
        panic!("no grottex view with LEZARD's animation 3");
    };
    g.run_command("(DIAL=161)");
    assert_eq!(g.dialog_2004(), Some((161, 1, false, false)));
    assert!(!g.dialog_2004_spw(), "line 1 is HERO's");
    g.click();
    t += 0.04;
    g.step(t);
    assert_eq!(g.dialog_2004().map(|d| d.1), Some(2), "view {view}");
    assert!(g.dialog_2004_spw());
    assert!(g.dialog_2004_media().unwrap().head.is_none());
    let shown: Vec<u32> = g
        .slots()
        .slots
        .iter()
        .filter(|s| s.kind == 1 && s.id == lezard && s.shown)
        .map(|s| s.state.anim)
        .collect();
    assert_eq!(shown, vec![3]);
    t += 0.04;
    g.step(t);
    let (w, h) = (g.width(), g.height());
    let black = |g: &Game, y: usize| match g.surface() {
        Surface::Rgb565(p) => p[y * w..(y + 1) * w].iter().all(|&c| c == 0),
        Surface::Xrgb8888(p) => p[y * w..(y + 1) * w].iter().all(|&c| c & 0xff_ffff == 0),
    };
    assert!(black(&g, 0) && black(&g, 75) && black(&g, h - 76));
    assert!(!black(&g, h / 2));
}
