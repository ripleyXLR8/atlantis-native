//! The Dutch retail CD (atlantis-cd-nl-688128, the English May 1997
//! build's code): its subtitles (breaker `FUN_0042e990`, drawer
//! `FUN_0042eb5c`, `DIALOG\FONTS.BIN`, glyph = byte) against the statistics
//! and the picture of their emulation, its DOS code page letters, its menu
//! font advances, the R&P logo of its start-up, and disc 2's older copies
//! of CREDITS.TXT and the FONTMIN fonts, read while disc 2 is the current
//! disc. The expected values are numbers and digests only.
//!
//! Needs the game folder of that build: set `ATLANTIS_DATA_NL1997`;
//! without it the tests are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::enhancements::Enhancements;
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::subtitles::{split_subtitle_1997, BitmapFont};
use atlantis_engine::sys::{Input, Runner, Sys};
use atlantis_engine::{Engine, Event, Options, Start};
use md5::{Digest, Md5};

const VAR: &str = "ATLANTIS_DATA_NL1997";

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

/// The dialog texts of the CON files (first copies), in file-name order.
fn dialog_texts(g: &Game) -> Vec<Vec<u8>> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    let mut out = Vec::new();
    for n in &names {
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

/// The distinct dialog texts.
fn distinct_texts(g: &Game) -> Vec<Vec<u8>> {
    let mut texts = dialog_texts(g);
    texts.sort();
    texts.dedup();
    texts
}

/// Every dialog line through the breaker, each as the first line after
/// start-up (a zero tail after its NUL): the emulation's statistics over
/// the distinct texts (rows per text, the longest row, the rows wider than
/// the screen, none past the 0x55-byte row buffer).
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    assert!(g.exe.build.bitmap_subtitles);
    assert_eq!(dialog_texts(&g).len(), 2539);
    let texts = distinct_texts(&g);
    let (mut hist, mut longest, mut over79, mut over84) = ([0usize; 8], 0, 0, 0);
    for text in &texts {
        let rows = split_subtitle_1997(text);
        hist[rows.len().min(7)] += 1;
        for r in &rows {
            longest = longest.max(r.len());
            over79 += usize::from(r.len() > 79);
            over84 += usize::from(r.len() > 84);
        }
    }
    assert_eq!(texts.len(), 1945);
    assert_eq!(hist, [0, 1484, 379, 69, 9, 4, 0, 0]);
    assert_eq!((longest, over79, over84), (81, 8, 0));
}

/// The distinct texts' bytes above 0x7f, and the letters code page 850 reads in
/// them (FONTS.BIN's code page 437 glyphs show the same letters).
#[test]
fn dos_code_page_letters() {
    let Some(g) = game("dos_code_page_letters") else {
        return;
    };
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Dos850);
    let mut count = [0usize; 256];
    for t in distinct_texts(&g) {
        for c in t {
            count[usize::from(c)] += 1;
        }
    }
    let want = [
        (0x82, 64, 'é'),
        (0x89, 1, 'ë'),
        (0x8a, 19, 'è'),
        (0x8b, 1, 'ï'),
        (0xa3, 1, 'ú'),
    ];
    let high: Vec<usize> = (0x80..0x100).filter(|&c| count[c] > 0).collect();
    assert_eq!(high, want.map(|w| w.0).to_vec());
    for (b, n, letter) in want {
        assert_eq!(count[b], n, "{b:#x}");
        assert_eq!(TextCodePage::Dos850.char_of(b as u8), letter);
    }
}

/// The subtitle the emulation rendered ("Alléén ... één"), its row drawn
/// over a mid grey screen; pixel rows 400..479, RGB565.
#[test]
fn accented_subtitle_pixels_match_original() {
    let Some(g) = game("accented_subtitle_pixels_match_original") else {
        return;
    };
    let data = g.big.read("DIALOG\\FONTS.BIN").expect("FONTS.BIN");
    assert_eq!(md5_hex(&data), "4aec21ccbf3bd593dc01751adb834964");
    let font = BitmapFont::new(&data).unwrap();
    let text = dialog_texts(&g)
        .into_iter()
        .find(|t| t.windows(8).any(|w| w == b"All\x82\x82n s"))
        .unwrap();
    let rows = split_subtitle_1997(&text);
    assert_eq!(rows.len(), 1);
    let mut screen = vec![0x8410u16; 640 * 480];
    font.draw_subtitles(&mut screen, 640, &rows, 0);
    let crop: Vec<u8> = screen[400 * 640..]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    assert_eq!(md5_hex(&crop), "dffa815ef1f0451d140d4c3376776541");
    let count = |c: u16| screen.iter().filter(|&&p| p == c).count();
    assert_eq!((count(0xffff), count(0)), (1177, 685));
}

/// The menu font advances of this build are the ones the port carries:
/// FONTMAX at 0x004865ea, FONTMIN at 0x00486673 (file offsets 0x729ea and
/// 0x72a73).
#[test]
fn menu_font_advances() {
    let Some(dir) = std::env::var_os(VAR) else {
        eprintln!("{VAR} not set: menu_font_advances skipped");
        return;
    };
    let exe = atlantis_data::FileSource::read(&StdDir::new(dir), "atlantis.exe").unwrap();
    use atlantis_engine::ui::layout::{FONTMAX_WIDTHS, FONTMIN_ADVANCE};
    assert_eq!(exe[0x729ea..0x729ea + FONTMAX_WIDTHS.len()], FONTMAX_WIDTHS);
    assert_eq!(
        exe[0x72a73..0x72a73 + FONTMIN_ADVANCE.len()],
        FONTMIN_ADVANCE
    );
}

/// A headless start-up: LOGO15N, the R&P logo (60 frames, no sound of its
/// own), PREINTRO, then the player screen.
#[test]
fn startup_plays_the_rp_logo() {
    let Some(g) = game("startup_plays_the_rp_logo") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let opts = Options {
        trace: Some(trace.clone()),
        start: Start::Boot,
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut frame = 0u64;
    let mut logorp_ms = None;
    while runner.step(frame * 16) {
        frame += 1;
        let t = trace.borrow();
        if logorp_ms.is_none()
            && t.iter()
                .any(|e| matches!(e, Event::Video { name, .. } if name == "PREINTRO.HNM"))
        {
            logorp_ms = Some(frame * 16);
        }
        if t.contains(&Event::Menu("SELEMENU.TXT".into())) {
            sys.push_input(Input::Quit);
        }
        assert!(frame < 40_000, "no player screen");
    }
    let t = trace.borrow();
    let videos: Vec<(String, u32)> = t
        .iter()
        .filter_map(|e| match e {
            Event::Video { name, frames, .. } => Some((name.clone(), *frames)),
            _ => None,
        })
        .collect();
    assert_eq!(videos.len(), 3, "{videos:?}");
    assert_eq!(videos[0].0, "LOGO15N.HNM");
    assert_eq!(videos[1], ("LOGORP.HNM".to_string(), 60));
    assert_eq!(videos[2].0, "PREINTRO.HNM");
    // No NEWSOUND for the logo: only LOGO15N's and PREINTRO's sounds.
    let sounds: Vec<&String> = t
        .iter()
        .filter_map(|e| match e {
            Event::NewSound(_, apc) => Some(apc),
            _ => None,
        })
        .collect();
    assert!(sounds.iter().all(|s| !s.contains("LOGORP")), "{sounds:?}");
    let at = |n: &str| {
        t.iter()
            .position(|e| matches!(e, Event::Video { name, .. } if name == n))
    };
    let menu = t
        .iter()
        .position(|e| *e == Event::Menu("SELEMENU.TXT".into()))
        .unwrap();
    assert!(at("LOGORP.HNM") < at("PREINTRO.HNM") && at("PREINTRO.HNM") < Some(menu));
    assert!(logorp_ms.is_some());
}

/// Runs `f` on an engine of the Dutch game with its interface loaded.
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

/// While disc 2 is the current disc, CREDITS.TXT and the FONTMIN fonts are
/// disc 2's older copies (as the original reads them from the disc in the
/// drive); with the minor bug fixes, disc 1's whatever the disc.
#[test]
fn disc_2_copies_both_ways() {
    let Some(g) = game("disc_2_copies_both_ways") else {
        return;
    };
    assert!(g.exe.build.credits_read_per_run);
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    with_engine(g, async move |e: &mut Engine| {
        let j = usize::from(b'j' - 0x20);
        for (fixes, disc) in [(false, 1), (false, 2), (true, 2), (false, 3)] {
            let mut enh = Enhancements::original();
            enh.minor_bug_fixes = fixes;
            e.sys.set_enhancements(enh);
            e.disc = disc;
            let credits = e
                .read_from_current_disc("SPRLIST\\CREDITS.TXT")
                .await
                .unwrap();
            e.load_fontmin_from_current_disc().await;
            let from = e.ui.fontmin_from.clone().unwrap().0;
            let jy = e.ui.fontmin[j].yoff;
            o.borrow_mut()
                .push(format!("{fixes} {disc}: {} {from} {jy}", credits.len()));
        }
    });
    let out = out.borrow();
    let jy = |i: usize| out[i].rsplit(' ').next().unwrap().to_string();
    assert!(out[0].starts_with("false 1: 5034 BIGCD1.BIG"), "{out:?}");
    assert!(out[1].starts_with("false 2: 5031 BIGCD2.BIG"), "{out:?}");
    assert!(out[2].starts_with("true 2: 5034 BIGCD1.BIG"), "{out:?}");
    assert!(out[3].starts_with("false 3: 5034 BIGCD3.BIG"), "{out:?}");
    // The `j` hotspot: disc 2's font differs from the others'.
    assert_ne!(jy(1), jy(0));
    assert_eq!(jy(2), jy(0));
    assert_eq!(jy(3), jy(0));
}
