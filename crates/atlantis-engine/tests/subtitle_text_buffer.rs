//! The subtitle text buffer of the bitmap-subtitle builds of 1997
//! (atlantis-cd-en-688128, fr-688640, nl-688128, sv-688128, de-693248)
//! against their own code: the start-up clear `FUN_00422bd0`, the section
//! executor's copy of every `[` line (`FUN_00427420` @
//! 0x00427707..0x00427722), the game-over screen's copy of its narrator
//! line (`FUN_004256a4` @ 0x00425a31..0x00425a50) and the row breaker
//! (`FUN_0042e970` and its copies, `FUN_0042ea7c` in the Swedish build),
//! run unmodified under emulation. The breaker's forward scan reads past
//! the text's NUL, into the zeros of the start-up clear or the tail of an
//! earlier, longer text, so the rows of a line depend on what was shown
//! before it. The expected values are numbers and file names only.
//!
//! The tests of the CON files need each build's game folder
//! (`ATLANTIS_DATA_1997`, `ATLANTIS_DATA_FR1997_MAY`,
//! `ATLANTIS_DATA_NL1997`, `ATLANTIS_DATA_SV1997`; `ATLANTIS_DATA_DE_MAY97`,
//! the folder of the German May CD's images); without it they are
//! skipped.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::{Game, StdDir};
use atlantis_engine::con_script::{
    con_line_buffer_text, con_narrator_buffer_text, ConItem, ConScript,
};
use atlantis_engine::subtitles::{split_bitmap_buffer, BitmapBreaker, SubtitleTextBuffer};

struct Build {
    golden: &'static str,
    var: &'static str,
    breaker: BitmapBreaker,
}

const BUILDS: [Build; 5] = [
    Build {
        golden: include_str!("golden/video_subtitle_buffer_en688128.txt"),
        var: "ATLANTIS_DATA_1997",
        breaker: BitmapBreaker::Nearest,
    },
    Build {
        golden: include_str!("golden/video_subtitle_buffer_fr688640.txt"),
        var: "ATLANTIS_DATA_FR1997_MAY",
        breaker: BitmapBreaker::Nearest,
    },
    Build {
        golden: include_str!("golden/video_subtitle_buffer_nl688128.txt"),
        var: "ATLANTIS_DATA_NL1997",
        breaker: BitmapBreaker::Nearest,
    },
    Build {
        golden: include_str!("golden/video_subtitle_buffer_sv688128.txt"),
        var: "ATLANTIS_DATA_SV1997",
        breaker: BitmapBreaker::Back,
    },
    Build {
        golden: include_str!("golden/video_subtitle_buffer_de693248.txt"),
        var: "ATLANTIS_DATA_DE_MAY97",
        breaker: BitmapBreaker::Nearest,
    },
];

const FOX: &str = "The quick brown fox jumps over the lazy dog, then the lazy dog wakes up \
and chases the fox around the field.";

/// The texts the emulation broke, in its order: the earlier synthetic
/// texts, then four whose last word crosses a window end.
fn samples() -> Vec<String> {
    let x = |n: usize| "x".repeat(n);
    let mut t = String::new();
    for _ in 0..15 {
        t += "abcdefghi ";
    }
    vec![
        "Short line.".to_string(),
        format!("{} ", "y".repeat(49)).repeat(2),
        x(74),
        x(75),
        format!("{} tail", x(76)),
        format!("{} ", x(60)).repeat(3),
        ["word"; 30].join(" "),
        ["abcdefghi"; 25].join(" "),
        FOX.to_string(),
        format!("{} {}", x(60), "y".repeat(16)),
        format!("{} {}", x(70), "y".repeat(5)),
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen"
            .to_string(),
        t + "abcdefghijklmnopqrst",
    ]
}

/// The emulation's history: the writes, in order, after the start-up
/// clear: 'e' the executor's copy of a `[` line, 'g' the game-over
/// screen's copy of a narrator line.
fn history(name: &str) -> Vec<(char, String)> {
    let x60y16 = format!("{} {}", "x".repeat(60), "y".repeat(16));
    let ex = |l: String| ('e', l);
    match name {
        "stale_space" => vec![
            ex(format!(
                "[Seth,cam2] {} {} qq",
                "q".repeat(40),
                "q".repeat(38)
            )),
            ex(format!("[Seth,cam2] {x60y16}")),
        ],
        "stale_nospace" => vec![
            ex(format!("[Seth,cam2] {} {}", "q".repeat(60), "q".repeat(30))),
            ex(format!("[Seth,cam2] {x60y16}")),
        ],
        "shorter_then_longer" => vec![
            ex(format!("[Seth] {} s", "r".repeat(70))),
            ex(format!("[Seth] {x60y16}")),
            ex(format!("[Seth] {} {}", "x".repeat(70), "y".repeat(5))),
        ],
        "game_over" => vec![
            (
                'g',
                format!(
                    "[Narratrice] {} {} end of the story",
                    "n".repeat(40),
                    "n".repeat(39)
                ),
            ),
            ex(format!("[Seth] {x60y16}")),
        ],
        "trailing_blanks" => vec![
            ex(format!("[Seth] {x60y16}   ")),
            ex(format!("[Seth] {x60y16}")),
        ],
        "no_blank_after_bracket" => vec![ex(format!("[Seth]{x60y16}"))],
        _ => Vec::new(),
    }
}

/// "[61,17]", the rows' lengths as the emulation prints them.
fn fmt(rows: &[Vec<u8>]) -> String {
    let lens: Vec<String> = rows.iter().map(|r| r.len().to_string()).collect();
    format!("[{}]", lens.join(","))
}

/// The first "[...]" of `s` from `from`.
fn bracket(s: &str, from: usize) -> &str {
    let Some(a) = s[from..].find('[').map(|a| a + from) else {
        return "";
    };
    s[a..].find(']').map_or("", |b| &s[a..=a + b])
}

/// A window without a space: the original loops or faults there, the
/// port's cut is its own.
fn undefined_rows(s: &str) -> bool {
    s.contains("nospace") || s.contains("no-return")
}

/// One write of the original's copy loops.
fn store(buf: &mut SubtitleTextBuffer, kind: char, line: &str) {
    let raw = line.as_bytes();
    buf.store(if kind == 'e' {
        con_line_buffer_text(raw)
    } else {
        con_narrator_buffer_text(raw)
    });
}

/// The buffer as the first line after start-up leaves it (zeros after the
/// NUL), in the four builds: the original's rows for the samples, except
/// the windows without a space (the original loops or faults: the port
/// cuts at the forward space or the end of the text, the Swedish breaker
/// at 75 bytes).
#[test]
fn fresh_buffer_matches_original() {
    let input = samples();
    for b in &BUILDS {
        let (mut k, mut undef, mut past) = (0, 0, 0);
        for l in b.golden.lines().filter(|l| l.starts_with("fresh len ")) {
            let len: usize = l[10..]
                .split(' ')
                .find(|t| !t.is_empty())
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(len, input[k].len(), "{l}");
            let rows = split_bitmap_buffer(
                &SubtitleTextBuffer::fresh(format!(" {}", input[k]).as_bytes()),
                b.breaker,
            );
            if undefined_rows(l) {
                undef += 1;
                // the text kept whole, or cut at 75 bytes by the Swedish
                // breaker
                assert_eq!(rows.concat().len(), len + 1, "{l}");
            } else {
                assert_eq!(fmt(&rows), bracket(l, l.find("rows ").unwrap()), "{l}");
            }
            past += usize::from(l.contains(" past"));
            k += 1;
        }
        assert_eq!(k, input.len());
        // x * 75 (and x * 76 + " tail" in the Swedish build); three samples
        // run their forward scan out of the buffer
        let want = if b.breaker == BitmapBreaker::Back {
            2
        } else {
            1
        };
        assert_eq!((undef, past), (want, 3));
    }
}

/// The buffer's history: the original's copies (executor, game-over
/// narrator) leave the tail of an earlier, longer text after the NUL, and
/// its spaces can stop the forward scan (stale_space, game_over: one row
/// where a first line after start-up has two). The port's buffer, filled
/// as the original fills it, gives the same rows after every write and
/// the same tail.
#[test]
fn buffer_history_matches_original() {
    for b in &BUILDS {
        let (mut seen, mut checked) = (0, 0);
        for l in b.golden.lines().filter(|l| l.starts_with("history ")) {
            let (head, rest) = l.split_once(':').unwrap();
            let steps = history(&head[8..]);
            assert!(!steps.is_empty(), "{l}");
            seen += 1;
            let mut buf = SubtitleTextBuffer::default();
            let tokens: Vec<&str> = rest.split(" ; ").map(str::trim).collect();
            for (i, (kind, line)) in steps.iter().enumerate() {
                store(&mut buf, *kind, line);
                let rows = split_bitmap_buffer(&buf, b.breaker);
                let token = tokens[i];
                assert!(
                    token.starts_with(if *kind == 'e' { "ex " } else { "go " }),
                    "{l}"
                );
                if undefined_rows(token) {
                    continue;
                }
                assert_eq!(fmt(&rows), bracket(token, 0), "{l}");
                checked += 1;
            }
            // the 24 bytes from the NUL on
            let tail = tokens.last().unwrap().strip_prefix("tail ").unwrap();
            let nul = buf.text().len();
            let hex: String = (0..24)
                .map(|i| format!("{:02x}", buf.at(nul + i)))
                .collect();
            assert_eq!(hex, tail, "{l}");
        }
        assert_eq!(seen, 6);
        assert!(checked >= 9);
    }
    // stale_space in en-688128: the tail of the first line stops the
    // second line's forward scan, one row of 78; as the first line after
    // start-up it is 61 | 17.
    let h = history("stale_space");
    let mut buf = SubtitleTextBuffer::default();
    store(&mut buf, 'e', &h[1].1);
    assert_eq!(
        fmt(&split_bitmap_buffer(&buf, BitmapBreaker::Nearest)),
        "[61,17]"
    );
    store(&mut buf, 'e', &h[0].1);
    store(&mut buf, 'e', &h[1].1);
    assert_eq!(
        fmt(&split_bitmap_buffer(&buf, BitmapBreaker::Nearest)),
        "[78]"
    );
}

/// The `[` lines of a release's CON files in the emulation's order: names
/// sorted, lines split at CR LF, a line kept from its '[' when it holds a
/// ']', up to its first CR; its file name and line number.
fn con_lines(g: &Game) -> Vec<(String, Vec<u8>)> {
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
        let Some(b) = g.big.read(n) else {
            continue;
        };
        let (mut i, mut no) = (0usize, 0);
        while i <= b.len() {
            let mut j = i;
            while j < b.len() && !(b[j] == b'\r' && j + 1 < b.len() && b[j + 1] == b'\n') {
                j += 1;
            }
            no += 1;
            let seg = &b[i..j];
            if seg.first() == Some(&b'[') && seg.contains(&b']') {
                let e = seg.iter().position(|&c| c == b'\r').unwrap_or(seg.len());
                out.push((format!("{n}:{no}"), seg[..e].to_vec()));
            }
            if j >= b.len() {
                break;
            }
            i = j + 2;
        }
    }
    out
}

/// A field of the golden's scan line.
fn field(scan: &str, key: &str) -> i64 {
    let k = format!("'{key}': ");
    let p = scan.find(&k).unwrap_or_else(|| panic!("no {key}")) + k.len();
    let end = scan[p..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(scan.len(), |e| p + e);
    scan[p..end].parse().unwrap()
}

/// Every `[` line of each release's CON files: as the first line after
/// start-up (the emulation's statistics, and the lines whose rows the CR
/// the earlier model put after the NUL changed), then all of them in file
/// order through one buffer (the lines whose rows the stale tail changes).
#[test]
fn con_walk_matches_original() {
    for b in &BUILDS {
        let Some(dir) = std::env::var_os(b.var) else {
            eprintln!("{} not set: skipped", b.var);
            continue;
        };
        // A game folder, else a folder of the CD's images.
        let g = Game::open(Arc::new(StdDir::new(&dir)))
            .ok()
            .or_else(|| Game::open(Arc::new(DiscSet::open_folder(dir.as_ref())?)).ok())
            .expect("cannot open the game directory");
        assert!(g.exe.build.bitmap_subtitles);
        let scan = b
            .golden
            .lines()
            .find(|l| l.starts_with("# scan of every [ line"))
            .unwrap();
        let mut crdiff = HashMap::new();
        let mut walk_want = Vec::new();
        let (mut crdiff_count, mut walk_count) = (-1, -1);
        for l in b.golden.lines() {
            if let Some(n) = l.strip_prefix("# lines whose fresh rows differ from the CR model: ") {
                crdiff_count = n.parse().unwrap();
            } else if let Some(n) = l.strip_prefix("# file-order walk after one start-up clear: ") {
                walk_count = n.split(' ').next().unwrap().parse().unwrap();
            } else if let Some(r) = l.strip_prefix("crdiff ") {
                let (at, _) = r.split_once(' ').unwrap();
                crdiff.insert(
                    at.to_string(),
                    bracket(l, l.find(" fresh ").unwrap()).to_string(),
                );
            } else if let Some(r) = l.strip_prefix("walk ") {
                let (at, _) = r.split_once(' ').unwrap();
                let rows = bracket(l, 5 + at.len());
                let fresh = bracket(l, l.find(" fresh ").unwrap());
                walk_want.push(format!("{at} {rows} {fresh}"));
            }
        }
        assert_eq!(crdiff.len() as i64, crdiff_count, "{}", b.var);
        assert_eq!(walk_want.len() as i64, walk_count, "{}", b.var);
        let refs = con_lines(&g);
        let (mut max_rows, mut max_len, mut over79, mut over84, mut max_text) = (0, 0, 0, 0, 0);
        let mut hist = [0i64; 9];
        let mut fresh = Vec::new();
        let mut cr_checked = 0;
        for (at, raw) in &refs {
            let mut buf = SubtitleTextBuffer::default();
            buf.store(con_line_buffer_text(raw));
            let rows = split_bitmap_buffer(&buf, b.breaker);
            fresh.push(fmt(&rows));
            max_rows = max_rows.max(rows.len());
            max_text = max_text.max(buf.text().len());
            hist[rows.len().min(8)] += 1;
            for r in &rows {
                max_len = max_len.max(r.len());
                over79 += i64::from(r.len() > 79);
                over84 += i64::from(r.len() > 84);
            }
            if let Some(want) = crdiff.get(at) {
                assert_eq!(fresh.last().unwrap(), want, "{at}");
                cr_checked += 1;
            }
        }
        assert_eq!(refs.len() as i64, field(scan, "lines"), "{}", b.var);
        assert_eq!(max_rows as i64, field(scan, "max_rows"));
        assert_eq!(max_len as i64, field(scan, "max_row_len"));
        assert_eq!(over79, field(scan, "rows_over_79"));
        assert_eq!(over84, field(scan, "rows_over_84"));
        assert_eq!(max_text as i64, field(scan, "max_text"));
        assert_eq!(field(scan, "dependent"), 0);
        let hist_got: Vec<String> = (1..=max_rows.min(8))
            .map(|n| format!("{n}: {}", hist[n]))
            .collect();
        assert!(
            scan.contains(&format!("'rows_hist': {{{}}}", hist_got.join(", "))),
            "{}: {hist_got:?}",
            b.var
        );
        assert_eq!(cr_checked, crdiff.len());
        // The walk: one buffer, every line in file order.
        let mut buf = SubtitleTextBuffer::default();
        let mut walk_got = Vec::new();
        for (i, (at, raw)) in refs.iter().enumerate() {
            buf.store(con_line_buffer_text(raw));
            let rows = fmt(&split_bitmap_buffer(&buf, b.breaker));
            if rows != fresh[i] {
                walk_got.push(format!("{at} {rows} {}", fresh[i]));
            }
        }
        assert_eq!(walk_got, walk_want, "{}", b.var);
    }
}

/// `ConLine::raw` keeps the line up to its CR with its final blanks (the
/// parsed text drops them); the executor's copy starts after the first
/// ']', the game-over screen's at the line's 13th byte.
#[test]
fn con_line_raw() {
    let text = b"/1\r\n/con(ClicPerso=600)\r\n[Seth,cam2] A b  \r\n[Seth]x]y\r\n/000\r\n";
    let script = ConScript::parse(text);
    let sec = script.sections().iter().find(|s| s.id == 1).unwrap();
    let lines: Vec<_> = sec
        .items
        .iter()
        .filter_map(|it| match it {
            ConItem::Line(l) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "A b");
    assert_eq!(lines[0].raw, "[Seth,cam2] A b  ");
    assert_eq!(con_line_buffer_text(lines[0].raw.as_bytes()), b" A b  ");
    assert_eq!(con_line_buffer_text(lines[1].raw.as_bytes()), b"x]y");
    assert_eq!(
        con_narrator_buffer_text(b"[Narratrice] Seth est mort."),
        b" Seth est mort."
    );
    // A ']' right before the CR: the original copies on past the line
    // (none shipped); the port copies nothing.
    assert_eq!(con_line_buffer_text(b"[Seth]"), b"");
    // The copy keeps the bytes after the NUL.
    let mut buf = SubtitleTextBuffer::default();
    buf.store(b" long text");
    buf.store(b" ab");
    assert_eq!(buf.text(), b" ab");
    assert_eq!(
        (0..8).map(|i| buf.at(3 + i)).collect::<Vec<_>>(),
        [0, b'g', b' ', b't', b'e', b'x', b't', 0]
    );
}
