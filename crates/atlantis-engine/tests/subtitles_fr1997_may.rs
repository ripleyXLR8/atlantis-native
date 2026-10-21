//! The subtitles of the French retail CD's May 1997 build
//! (atlantis-cd-fr-688640), against its own code run unmodified under
//! emulation: its breaker `FUN_0042eab4` and drawer `FUN_0042ec80` are the
//! English 1997 build's (`DIALOG\FONTS.BIN`, glyph = byte). Its CON texts
//! are in the DOS code page, whose accented letters the font's code page
//! 437 glyphs show as they are. The expected values are numbers and digests
//! only.
//!
//! Needs the game folder of that build: set `ATLANTIS_DATA_FR1997_MAY`;
//! without it the tests are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir, TextCodePage};
use atlantis_engine::audio::Mixer;
use atlantis_engine::con_script::script::latin1_bytes;
use atlantis_engine::con_script::{ConItem, ConScript};
use atlantis_engine::subtitles::{bitmap_line_y, split_subtitle_1997, BitmapFont};
use atlantis_engine::sys::{Runner, Sys};
use atlantis_engine::{Event, Options, Start};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_FR1997_MAY") else {
        eprintln!("ATLANTIS_DATA_FR1997_MAY not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

/// The dialog texts of the CON files, in file-name order.
fn dialog_texts(g: &Game) -> Vec<Vec<u8>> {
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
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

/// Every dialog line through the breaker, each as the first line after
/// start-up (a zero tail after its NUL): the emulation's statistics (rows
/// per line, the longest row, the rows wider than the screen).
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    assert!(g.exe.build.bitmap_subtitles);
    let texts = dialog_texts(&g);
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
    assert_eq!(texts.len(), 2540);
    assert_eq!(hist, [0, 1852, 589, 76, 18, 4, 1, 0]);
    assert_eq!((longest, over79, over84), (81, 8, 0));
}

/// The texts' bytes above 0x7f, and the letters code page 850 reads in
/// them (the build's font shows these letters at those bytes).
#[test]
fn dos_code_page_letters() {
    let Some(g) = game("dos_code_page_letters") else {
        return;
    };
    assert_eq!(g.exe.build.text_code_page, TextCodePage::Dos850);
    let texts = dialog_texts(&g);
    let mut count = [0usize; 256];
    let mut with_high = 0;
    for t in &texts {
        with_high += usize::from(t.iter().any(|&c| c >= 0x80));
        for &c in t {
            count[usize::from(c)] += 1;
        }
    }
    assert_eq!(with_high, 1592);
    let want = [
        (0x82, 1697, 'é'),
        (0x83, 22, 'â'),
        (0x85, 529, 'à'),
        (0x87, 158, 'ç'),
        (0x88, 320, 'ê'),
        (0x8a, 233, 'è'),
        (0x8b, 5, 'ï'),
        (0x8c, 78, 'î'),
        (0x93, 71, 'ô'),
        (0x96, 45, 'û'),
        (0x97, 142, 'ù'),
    ];
    let high: Vec<usize> = (0x80..0x100).filter(|&c| count[c] > 0).collect();
    assert_eq!(high, want.map(|w| w.0).to_vec());
    for (b, n, letter) in want {
        assert_eq!(count[b], n, "{b:#x}");
        assert_eq!(TextCodePage::Dos850.char_of(b as u8), letter);
    }
}

/// The subtitle the emulation rendered: the first dialog line with four
/// accented letters and over 80 bytes, its rows drawn over a mid grey
/// screen; pixel rows 400..479, RGB565.
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
        .find(|t| t.iter().filter(|&&c| c >= 0x80).count() >= 4 && t.len() > 80)
        .unwrap();
    // 3 rows as the first line after start-up: the second window's last
    // word runs into the zero tail, the backward cut.
    let rows = split_subtitle_1997(&text);
    assert_eq!(rows.len(), 3);
    let mut screen = vec![0x8410u16; 640 * 480];
    font.draw_subtitles(&mut screen, 640, &rows, 0);
    let crop: Vec<u8> = screen[400 * 640..]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    assert_eq!(md5_hex(&crop), "77d722e2c8e31f315ddc366b53b73a2d");
    let count = |c: u16| screen.iter().filter(|&&p| p == c).count();
    assert_eq!((count(0xffff), count(0)), (2427, 1428));
}

/// The menu font advances of this build are the ones the port carries:
/// FONTMAX at 0x004865eb, FONTMIN at 0x00486674 (file offsets 0x72beb and
/// 0x72c74).
#[test]
fn menu_font_advances() {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_FR1997_MAY") else {
        eprintln!("ATLANTIS_DATA_FR1997_MAY not set: menu_font_advances skipped");
        return;
    };
    let src = StdDir::new(dir);
    let exe = atlantis_data::FileSource::read(&src, "atlantis.exe").unwrap();
    use atlantis_engine::ui::layout::{FONTMAX_WIDTHS, FONTMIN_ADVANCE};
    assert_eq!(exe[0x72beb..0x72beb + FONTMAX_WIDTHS.len()], FONTMAX_WIDTHS);
    assert_eq!(
        exe[0x72c74..0x72c74 + FONTMIN_ADVANCE.len()],
        FONTMIN_ADVANCE
    );
}

/// A new game, headless: the first dialog line's subtitle is drawn with
/// the bitmap font, white over its black shadow, at the 1997 layout.
#[test]
fn new_game_subtitle_in_the_bitmap_font() {
    let Some(g) = game("new_game_subtitle_in_the_bitmap_font") else {
        return;
    };
    let font = BitmapFont::new(&g.big.read("DIALOG\\FONTS.BIN").unwrap()).unwrap();
    let script = ConScript::parse(&g.big.read("SCENAR\\CHAPI001.CON").unwrap());
    let text = script
        .sections()
        .iter()
        .find(|s| s.id == 1)
        .and_then(|s| {
            s.items.iter().find_map(|it| match it {
                ConItem::Line(l) => Some(latin1_bytes(&l.text)),
                _ => None,
            })
        })
        .unwrap();
    let rows = split_subtitle_1997(&text);
    // The pixels the font sets for the white text and the shadow.
    let mut white = vec![0u16; 640 * 480];
    let mut shadow = vec![0u16; 640 * 480];
    for (i, r) in rows.iter().enumerate() {
        let y = bitmap_line_y(i, rows.len());
        font.draw(&mut white, 640, r, 5, y, 1);
        font.draw(&mut shadow, 640, r, 6, y + 1, 1);
    }
    let want_white = white.iter().filter(|&&p| p == 1).count();
    assert!(want_white > 100);

    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let opts = Options {
        trace: Some(trace.clone()),
        start: Start::NewGame,
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut sound = vec![0.0; 2 * (22050 * 16 / 1000)];
    let mut matched = None;
    for frame in 0..40_000u64 {
        assert!(runner.step(frame * 16), "the engine ended");
        sys.mixer.fill(&mut sound, 22050, 2);
        let t = trace.borrow();
        if matches!(t.last(), Some(Event::Voice(_))) {
            let screen = sys.screen.borrow();
            let text_ok = (0..640 * 480).all(|i| white[i] == 0 || screen[i] == 0xffff);
            let shadow_ok =
                (0..640 * 480).all(|i| shadow[i] == 0 || white[i] == 1 || screen[i] == 0);
            if text_ok && shadow_ok {
                matched = Some(screen.clone());
                break;
            }
        }
        if t.iter().any(|e| matches!(e, Event::Music(1))) {
            break;
        }
    }
    assert!(matched.is_some(), "no frame with the bitmap subtitle");
}
