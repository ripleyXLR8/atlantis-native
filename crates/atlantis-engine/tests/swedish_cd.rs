//! The Swedish retail CD (atlantis-cd-sv-688128, the English May 1997
//! build's code): its subtitles (breaker `FUN_0042ea7c`, which cuts every
//! row at the space back from the end of the 75-byte window; drawer
//! `FUN_0042ebfc`, `DIALOG\FONTS.BIN`, glyph = byte) against the statistics
//! and the picture of their emulation, its DOS code page letters, its name
//! entry (the Swedish letters stored as DOS bytes), its LOAD picker (the
//! clamp with the knob at y 0), and the members the original reads from
//! the disc in the drive, the same bytes on each Swedish disc. The expected
//! values are numbers and digests only.
//!
//! Needs the game folder of that build: set `ATLANTIS_DATA_SV1997`;
//! without it the tests are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::saves::Players;
use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::checkpoint::PLAYERS_FILE;
use atlantis_engine::enhancements::{BugFix, Enhancements};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::subtitles::{split_subtitle_1997, split_subtitle_1997_back, BitmapFont};
use atlantis_engine::sys::{Input, Key, MouseButton, Runner, Sys};
use atlantis_engine::ui::layout::LoadPickerScroll;
use atlantis_engine::{Engine, Event, MemStore, Options, SaveStore};
use md5::{Digest, Md5};

const VAR: &str = "ATLANTIS_DATA_SV1997";

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The CON files' names (first copies), in name order.
fn con_names(g: &Game) -> Vec<String> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

/// The dialog texts of the CON files, in file-name order.
fn dialog_texts(g: &Game) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for n in &con_names(g) {
        let data = g.big.read(n).unwrap();
        for line in data.split(|&c| c == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.first() != Some(&b'[') {
                continue;
            }
            let Some(close) = line.iter().position(|&c| c == b']') else {
                continue;
            };
            let rest = &line[close + 1..];
            if rest.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            out.push(rest.strip_prefix(b" ").unwrap_or(rest).to_vec());
        }
    }
    out
}

/// Rows per text (1..5), the longest row, the rows over 75 and over 79
/// bytes (x = 5 + 8 x length past the 640-pixel screen).
fn stats(texts: &[Vec<u8>], split: fn(&[u8]) -> Vec<Vec<u8>>) -> ([usize; 7], usize, usize, usize) {
    let (mut hist, mut longest, mut over75, mut over79) = ([0usize; 7], 0, 0, 0);
    for text in texts {
        let rows = split(text);
        hist[rows.len().min(6)] += 1;
        for r in &rows {
            longest = longest.max(r.len());
            over75 += usize::from(r.len() > 75);
            over79 += usize::from(r.len() > 79);
        }
    }
    (hist, longest, over75, over79)
}

/// The 2539 dialog lines through this build's breaker and through
/// en-688128's, each as the first line after start-up (a zero tail after
/// its NUL): the emulation's statistics of both, and how many lines they
/// break differently.
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    let b = g.exe.build;
    assert!(b.bitmap_subtitles && b.subtitle_cut_back_only);
    let texts = dialog_texts(&g);
    assert_eq!(texts.len(), 2539);
    assert_eq!(
        stats(&texts, split_subtitle_1997_back),
        ([0, 1920, 520, 85, 9, 5, 0], 75, 0, 0)
    );
    assert_eq!(
        stats(&texts, split_subtitle_1997),
        ([0, 1920, 525, 80, 10, 4, 0], 81, 255, 16)
    );
    let (mut differ, mut more, mut fewer) = (0, 0, 0);
    for t in &texts {
        let (sv, en) = (split_subtitle_1997_back(t), split_subtitle_1997(t));
        if sv != en {
            differ += 1;
            more += usize::from(sv.len() > en.len());
            fewer += usize::from(sv.len() < en.len());
        }
        assert_eq!(sv.concat().len(), t.len() + 1);
    }
    assert_eq!((differ, more, fewer), (242, 6, 0));
}

/// The CON files' bytes above 0x7f, and the letters code page 850 reads in
/// them (FONTS.BIN's code page 437 glyphs show the same letters).
#[test]
fn dos_code_page_letters() {
    let Some(g) = game("dos_code_page_letters") else {
        return;
    };
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Dos850);
    let mut count = [0usize; 256];
    for n in &con_names(&g) {
        for c in g.big.read(n).unwrap() {
            count[usize::from(c)] += 1;
        }
    }
    let want = [
        (0x82, 10, 'é'),
        (0x84, 2740, 'ä'),
        (0x86, 2020, 'å'),
        (0x8e, 54, 'Ä'),
        (0x8f, 27, 'Å'),
        (0x94, 1897, 'ö'),
        (0x99, 25, 'Ö'),
    ];
    let high: Vec<usize> = (0x80..0x100).filter(|&c| count[c] > 0).collect();
    assert_eq!(high, want.map(|w| w.0).to_vec());
    for (b, n, letter) in want {
        assert_eq!(count[b], n, "{b:#x}");
        assert_eq!(TextCodePage::Dos850.char_of(b as u8), letter);
    }
}

/// The subtitle the emulation rendered ("Nej. Hon är på en rundtur ...",
/// two rows) over a mid grey screen; pixel rows 400..479, RGB565.
#[test]
fn swedish_subtitle_pixels_match_original() {
    let Some(g) = game("swedish_subtitle_pixels_match_original") else {
        return;
    };
    let data = g.big.read("DIALOG\\FONTS.BIN").expect("FONTS.BIN");
    assert_eq!(md5_hex(&data), "4aec21ccbf3bd593dc01751adb834964");
    let font = BitmapFont::new(&data).unwrap();
    let text = dialog_texts(&g)
        .into_iter()
        .find(|t| t.starts_with(b"Nej. Hon \x84r p\x86 en rundtur"))
        .unwrap();
    let rows = split_subtitle_1997_back(&text);
    assert_eq!(rows.len(), 2);
    let mut screen = vec![0x8410u16; 640 * 480];
    font.draw_subtitles(&mut screen, 640, &rows, 0);
    let crop: Vec<u8> = screen[400 * 640..]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    assert_eq!(md5_hex(&crop), "f6ad8b5d127754b2c2138451f639e7a4");
    let count = |c: u16| screen.iter().filter(|&&p| p == c).count();
    assert_eq!((count(0xffff), count(0)), (2333, 1335));
}

/// The LOAD picker of this build: the knob starts at y 0, as en-688128's,
/// but a drag just under the bar's top puts it back at y 0x7b with offset
/// 0, as the reference's (FUN_0041c9b0 @ 0x0041cc32, 0x0041cd08..0x0041cd1e).
#[test]
fn load_picker_clamp_with_the_knob_at_0() {
    let Some(g) = game("load_picker_clamp_with_the_knob_at_0") else {
        return;
    };
    let b = g.exe.build;
    let mut sb = LoadPickerScroll::new(20, b.load_knob_starts_at_bar, b.load_picker_clamp);
    assert_eq!((sb.knob_y, sb.offset, sb.clamp), (0, 0, true));
    sb.frame(true, true, 0x80);
    assert_eq!((sb.knob_y, sb.offset), (0x7b, 0));
    sb.frame(true, true, 0x100);
    assert_eq!(
        (sb.knob_y, sb.offset),
        (0xf6, ((0xf6 - 0x7b) / sb.step) as u8)
    );
}

/// A headless start-up (no R&P logo) up to the player screen, a name typed with the
/// Swedish letters, Enter: players.atl holds them as the DOS code page
/// bytes, the other letters as typed (no upper-casing).
#[test]
fn name_entry_stores_swedish_letters_as_dos_bytes() {
    let Some(g) = game("name_entry_stores_swedish_letters_as_dos_bytes") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let store = Rc::new(MemStore::new());
    let sys = Sys::new(Mixer::new());
    let opts = Options {
        trace: Some(trace.clone()),
        saves: store.clone(),
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut now = 0u64;
    let mut step = |n: usize| {
        for _ in 0..n {
            runner.step(now);
            now += 16;
        }
    };
    let mut frames = 0;
    while !trace.borrow().contains(&Event::Menu("SELEMENU.TXT".into())) {
        step(1);
        frames += 1;
        assert!(frames < 40_000, "no player screen");
    }
    // The start-up: LOGO15N then PREINTRO, no R&P logo (the Dutch
    // build's WinMain calls are not in this build).
    let videos: Vec<String> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::Video { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(videos, ["LOGO15N.HNM", "PREINTRO.HNM"]);
    step(10);
    let (x, y) = (200, 259);
    sys.push_input(Input::MouseMove { x, y });
    sys.push_input(Input::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    step(1);
    sys.push_input(Input::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    step(3);
    for c in "Åsa Ärlig åäöÖé".chars() {
        let k = if c == ' ' { Key::Space } else { Key::Char(c) };
        sys.push_input(Input::KeyDown(k));
        sys.push_input(Input::KeyUp(k));
        step(2);
    }
    sys.push_input(Input::KeyDown(Key::Enter));
    sys.push_input(Input::KeyUp(Key::Enter));
    let mut frames = 0;
    while store.read(PLAYERS_FILE).is_none() {
        step(1);
        frames += 1;
        assert!(frames < 2000, "no players.atl");
    }
    sys.push_input(Input::Quit);
    step(5);
    let players = Players::read(&store.read(PLAYERS_FILE).unwrap(), b"Ny spelare").unwrap();
    // `é` (0xe9, past 0xa7) refused.
    assert_eq!(players.name(0), b"\x8fsa \x8erlig \x86\x84\x94\x99");
    assert!(players.is_new_row(1));
}

/// Runs `f` on an engine of the Swedish game with its interface loaded.
fn with_engine(g: Game, f: impl AsyncFnOnce(&mut Engine) + 'static) {
    let sys = Sys::new(Mixer::new());
    let done = Rc::new(RefCell::new(false));
    let (s2, d2) = (sys.clone(), done.clone());
    let mut runner = Runner::new(sys, async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.load_interface().await;
        f(&mut e).await;
        *d2.borrow_mut() = true;
    });
    let mut n = 0u64;
    while !*done.borrow() {
        runner.step(n * 16);
        n += 1;
        assert!(n < 10_000, "the task does not end");
    }
}

/// CREDITS.TXT and the FONTMIN fonts are read from the current disc (each
/// Swedish disc carries a copy), or disc 1's with the disc revision's fix
/// (alone): the same bytes either way.
#[test]
fn current_disc_copies_both_ways() {
    let Some(g) = game("current_disc_copies_both_ways") else {
        return;
    };
    assert!(g.exe.build.credits_read_per_run);
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(g, async move |e: &mut Engine| {
        let j = usize::from(b'j' - 0x20);
        for (fixes, disc) in [(false, 1), (false, 2), (true, 2), (false, 3), (false, 4)] {
            e.sys.set_enhancements(if fixes {
                Enhancements::only_fix(BugFix::DiscRevision)
            } else {
                Enhancements::original()
            });
            e.disc = disc;
            let credits = e
                .read_from_current_disc("SPRLIST\\CREDITS.TXT")
                .await
                .unwrap();
            e.load_fontmin_from_current_disc().await;
            let from = e.ui.fontmin_from.clone().unwrap().0;
            let f = &e.ui.fontmin[j];
            o.borrow_mut().push((
                format!("{fixes} {disc}: {from}"),
                md5_hex(&credits),
                (f.w, f.h, f.yoff, e.ui.fontmin.len()),
            ));
        }
    });
    let out = out.borrow();
    let froms: Vec<&str> = out.iter().map(|o| o.0.as_str()).collect();
    assert_eq!(
        froms,
        [
            "false 1: BIGCD1.BIG",
            "false 2: BIGCD2.BIG",
            "true 2: BIGCD1.BIG",
            "false 3: BIGCD3.BIG",
            "false 4: BIGCD4.BIG",
        ]
    );
    // CREDITS.TXT: 5095 bytes, md5 73ea91e8..., on every disc.
    assert!(out.iter().all(|o| o.1.starts_with("73ea91e8")), "{out:?}");
    assert!(out.iter().all(|o| o.2 == out[0].2), "{out:?}");
}
