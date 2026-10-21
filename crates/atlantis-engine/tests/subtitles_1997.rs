//! The subtitles of the English retail CD's 1997 build
//! (atlantis-cd-en-688128) against its own code: the line breaker
//! `FUN_0042e970` and the drawer `FUN_0042eb3c` with the 8x8 font
//! `DIALOG\FONTS.BIN`, run unmodified under emulation on synthetic texts
//! and on every dialog line of the CON files. The expected values are
//! numbers and digests only.
//!
//! The data tests need the game folder of that build: set
//! `ATLANTIS_DATA_1997`; without it they are skipped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
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
    let Some(dir) = std::env::var_os("ATLANTIS_DATA_1997") else {
        eprintln!("ATLANTIS_DATA_1997 not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog, then the lazy dog wakes up \
and chases the fox around the field.";

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
}

/// The row lengths of `FUN_0042e970` on the synthetic texts of the
/// emulation.
#[test]
fn line_breaks_match_original() {
    let cases: Vec<(Vec<u8>, Vec<usize>)> = vec![
        (b"Short line.".to_vec(), vec![12]),
        ([&[b'y'; 49][..], b" "].concat().repeat(2), vec![50, 51]),
        (vec![b'x'; 74], vec![75]),
        (vec![b'x'; 75], vec![76]),
        ([&[b'x'; 76][..], b" tail"].concat(), vec![77, 5]),
        ([&[b'x'; 60][..], b" "].concat().repeat(3), vec![61, 61, 62]),
        (["word"; 30].join(" ").into_bytes(), vec![75, 75]),
        (
            ["abcdefghi"; 25].join(" ").into_bytes(),
            vec![70, 70, 70, 40],
        ),
        (FOX.to_vec(), vec![76, 33]),
    ];
    for (text, want) in cases {
        let rows = split_subtitle_1997(&text);
        let lens: Vec<usize> = rows.iter().map(Vec::len).collect();
        assert_eq!(lens, want, "{}", String::from_utf8_lossy(&text));
        // Every row starts with a space; together they are the text.
        assert!(rows.iter().all(|r| r[0] == b' '));
        let joined = rows.concat();
        assert_eq!(&joined[1..joined.len().min(text.len() + 1)], &text[..]);
    }
    let fox = split_subtitle_1997(FOX);
    assert_eq!(
        fox[0],
        b" The quick brown fox jumps over the lazy dog, then the lazy dog wakes up and"
    );
    assert_eq!(fox[1], b" chases the fox around the field.");
}

/// Row i of n at y = 0x1ce - (n - i + 1) * 10.
#[test]
fn rows_are_10_pixels_apart() {
    assert_eq!(bitmap_line_y(0, 1), 0x1ce - 20);
    assert_eq!(bitmap_line_y(0, 2), 0x1ce - 30);
    assert_eq!(bitmap_line_y(1, 2), 0x1ce - 20);
}

/// FUN_00410800 has no clipping: a row wider than the screen goes on at
/// the left of the next pixel row (three shipped rows do).
#[test]
fn wide_rows_wrap() {
    let font = BitmapFont::new(&[1u8; 256 * 64]).unwrap();
    let mut dst = vec![0u16; 640 * 480];
    // 81 glyphs at x = 5: 5 + 648 = 653, 13 pixels past the edge.
    font.draw(&mut dst, 640, &[b'x'; 81], 5, 100, 0xffff);
    assert!(dst[100 * 640..100 * 640 + 5].iter().all(|&p| p == 0));
    assert!(dst[108 * 640 + 13..109 * 640].iter().all(|&p| p == 0));
    for row in 0..8 {
        let y = 100 + row;
        assert!(dst[y * 640 + 5..(y + 1) * 640].iter().all(|&p| p == 0xffff));
        // The next pixel row's first 13 pixels.
        assert!(dst[(y + 1) * 640..(y + 1) * 640 + 13]
            .iter()
            .all(|&p| p == 0xffff));
    }
}

/// The subtitle the emulation rendered (the two rows of the last sample
/// over a mid grey screen): pixel rows 400..479, RGB565.
#[test]
fn subtitle_pixels_match_original() {
    let Some(g) = game("subtitle_pixels_match_original") else {
        return;
    };
    let data = g.big.read("DIALOG\\FONTS.BIN").expect("FONTS.BIN");
    assert_eq!(md5_hex(&data), "4aec21ccbf3bd593dc01751adb834964");
    let font = BitmapFont::new(&data).unwrap();
    let mut screen = vec![0x8410u16; 640 * 480];
    font.draw_subtitles(&mut screen, 640, &split_subtitle_1997(FOX), 0);
    let crop: Vec<u8> = screen[400 * 640..]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    assert_eq!(md5_hex(&crop), "901aa85a3ffb08be0af6202cdcd8970c");
    let count = |c: u16| screen.iter().filter(|&&p| p == c).count();
    assert_eq!((count(0xffff), count(0)), (1811, 1036));
}

/// Every dialog line of the CON files through the breaker, each as the
/// first line after start-up (a zero tail after its NUL): the emulation's
/// statistics (rows per line, the longest row, the rows wider than the
/// screen).
#[test]
fn con_lines_match_original() {
    let Some(g) = game("con_lines_match_original") else {
        return;
    };
    let mut names: Vec<String> = g
        .big
        .names()
        .filter(|n| n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    let (mut lines, mut hist, mut longest, mut over79, mut over84) = (0, [0usize; 8], 0, 0, 0);
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
            let text = rest.strip_prefix(b" ").unwrap_or(rest);
            let rows = split_subtitle_1997(text);
            lines += 1;
            hist[rows.len().min(7)] += 1;
            for r in &rows {
                longest = longest.max(r.len());
                over79 += usize::from(r.len() > 79);
                over84 += usize::from(r.len() > 84);
            }
        }
    }
    assert_eq!(lines, 2540);
    assert_eq!(hist, [0, 1924, 528, 73, 11, 4, 0, 0]);
    assert_eq!((longest, over79, over84), (81, 2, 0));
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
    let frame = matched.expect("no frame with the bitmap subtitle");
    dump("dialog_first_line_1997", &frame);
}
