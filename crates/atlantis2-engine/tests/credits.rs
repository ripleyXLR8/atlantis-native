//! The credits against the original (`tests/golden/credits.txt`): the
//! original's credits page built and run frame by frame under emulation
//! (its builder, its draw and its action of the frame), the clock given in
//! milliseconds. Compared here: the blend table's formula, the text
//! layout (every `GetTextExtentPoint32A` width and `TextOutA` call of
//! every page: position, colour, bytes by md5), the pictures as loaded
//! into the page surfaces, every frame of the 0x027f run (page, fade
//! weight, flags, action, the md5 and samples of the 640 x 480 RGB565
//! screen; the original's text pixels are GDI's and are not in its frames,
//! so the port's frames here are drawn without texts), the fade weights of
//! the 0x007f and 0x0e7f runs millisecond by millisecond, and the blend on
//! synthetic pictures.
//!
//! The text, picture and frame tests need the game: `ATLANTIS2_DATA` and
//! `ATLANTIS2_DISCS` (see `tests/warp.rs`); without them they are skipped.

use std::path::PathBuf;

use atlantis2_data::dialog::FontTxt;
use atlantis2_data::tga::Bgr24;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::credits::{self, Credits, HEIGHT, WIDTH};
use atlantis2_engine::subtitle::SubtitleFont;
use atlantis2_engine::Fpu;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/credits.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn md5(b: &[u8]) -> String {
    format!("{:x}", Md5::digest(b))
}

fn rows(kind: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    GOLDEN
        .lines()
        .filter(move |l| l.split(' ').next() == Some(kind))
        .map(|l| l.split(' ').skip(1).collect())
}

fn cw(s: &str) -> Fpu {
    match s {
        "027f" => Fpu::Double,
        "007f" => Fpu::Single,
        "0e7f" => Fpu::Chop,
        _ => panic!("control word {s}"),
    }
}

fn screen_bytes(px: &[u16]) -> Vec<u8> {
    px.iter().flat_map(|p| p.to_le_bytes()).collect()
}

/// The golden's samples: `x,y=pixel` at the points of the harness.
fn samples(px: &[u16]) -> String {
    [
        (0, 0),
        (100, 100),
        (320, 240),
        (522, 240),
        (600, 400),
        (639, 479),
    ]
    .iter()
    .map(|&(x, y)| format!("{x},{y}={:04x}", px[y * WIDTH + x]))
    .collect::<Vec<_>>()
    .join(" ")
}

fn popup(files: &A2Files) -> SubtitleFont {
    let f = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").expect("Font.txt"));
    let file = f.font("AtlFT_POPUP").expect("AtlFT_POPUP").file.clone();
    SubtitleFont::named(
        files.read(Folder::Loc, &file).expect("ttf"),
        &f,
        "AtlFT_POPUP",
    )
    .expect("font")
}

fn pictures(files: &A2Files) -> Vec<Vec<u8>> {
    (0..credits::PICTURES)
        .map(|i| {
            let d = files
                .read(Folder::SprMenu, &credits::picture_name(i))
                .expect("picture");
            Bgr24::parse(&d).expect("tga").bytes
        })
        .collect()
}

fn pages(files: &A2Files) -> Vec<Vec<Vec<u8>>> {
    credits::parse(&files.read(Folder::Loc, "Credits.txt").expect("Credits.txt"))
}

#[test]
fn the_blend_table_is_exact() {
    // T[i][j] = ftol(i · j · (1/256)), read back from the original's
    // table under each control word.
    let t: Vec<u8> = (0..256u32)
        .flat_map(|i| (0..256u32).map(move |j| ((i * j) >> 8) as u8))
        .collect();
    let mut n = 0;
    for r in rows("table") {
        assert_eq!(r[2], md5(&t), "table {}", r[0]);
        assert_eq!(r[4], "yes");
        n += 1;
    }
    assert_eq!(n, 3);
}

#[test]
fn the_texts_are_laid_out_as_the_original() {
    let Some(files) = game() else { return };
    let font = popup(&files);
    let pages = pages(&files);
    let p = rows("pages").next().unwrap();
    assert_eq!(pages.len(), p[0].parse::<usize>().unwrap());
    assert_eq!(credits::PICTURES, p[2].parse::<usize>().unwrap());
    // Every width measured, in the original's order.
    let measured: Vec<Vec<u8>> = pages
        .iter()
        .flatten()
        .filter(|l| l.first() == Some(&b'#'))
        .map(|l| {
            let mut l = l.clone();
            l[0] = b' ';
            l
        })
        .collect();
    let golden: Vec<_> = rows("extent").collect();
    assert_eq!(measured.len(), golden.len());
    for (b, r) in measured.iter().zip(&golden) {
        assert_eq!(
            (
                b.len().to_string(),
                md5(b),
                "13".to_string(),
                font.extent(b).0.to_string()
            ),
            (r[0].into(), r[1].into(), r[2].into(), r[3].into())
        );
    }
    // Every TextOutA call.
    let calls = credits::layout(&pages, |b| font.extent(b).0, font.line_step(), 640);
    let mut seen = vec![0usize; calls.len()];
    let mut n = 0;
    for r in rows("text") {
        let (page, k): (usize, usize) = (r[0].parse().unwrap(), r[1].parse().unwrap());
        let c = &calls[page][k];
        assert_eq!(
            (
                c.x.to_string(),
                c.y.to_string(),
                format!("{:06x}", c.colour),
                c.bytes.len().to_string(),
                md5(&c.bytes)
            ),
            (
                r[2].into(),
                r[3].into(),
                r[4].into(),
                r[5].into(),
                r[6].into()
            ),
            "page {page} call {k}"
        );
        assert_eq!(r[7], "pts=10");
        seen[page] += 1;
        n += 1;
    }
    for (p, c) in calls.iter().enumerate() {
        assert_eq!(seen[p], c.len(), "page {p}: calls not all in the golden");
    }
    assert_eq!(n, calls.iter().map(Vec::len).sum::<usize>());
}

#[test]
fn the_pictures_load_as_the_original() {
    let Some(files) = game() else { return };
    let pics = pictures(&files);
    let mut n = 0;
    for r in rows("load") {
        let pic: usize = r[2].parse().unwrap();
        assert_eq!(md5(&pics[pic]), r[3], "picture {pic}");
        n += 1;
    }
    // Page 0 at the build, then pages 1..20 at their reloads.
    assert_eq!(n, 21);
}

/// The harness's frame schedule: each cycle starts 7016 ms after the
/// previous one; `sweep` adds every millisecond from 4990 to 7010.
fn offsets(sweep: bool) -> Vec<u32> {
    let mut o = vec![0, 4999, 5000, 5400, 6000, 6200, 6999, 7000];
    if sweep {
        o.extend(4990..=7010);
        o.sort_unstable();
        o.dedup();
    }
    o
}

fn run(label: &str, pics: Vec<Vec<u8>>, npages: usize) {
    let fpu = cw(label);
    let mut c = Credits::new(pics, &vec![Vec::new(); npages], None);
    let full = label == "027f";
    let mut frames = rows("frame").filter(|r| r[0] == label);
    let mut sweeps = rows("sweep").filter(|r| r[0] == label);
    let mut screen = vec![0u16; WIDTH * HEIGHT];
    let (mut start, mut cycle, mut nframes) = (0u32, 0, 0);
    let fixed = offsets(false);
    loop {
        let mut seq = Vec::new();
        let mut act = 0;
        for off in offsets(!full) {
            let ms = start + off;
            // The harness's counter is the port's: milliseconds over the
            // frequency 1000.0, the quotient taken by the update.
            c.update(fpu, u64::from(ms));
            act = if c.done { credits::END } else { 0 };
            seq.push(c.alpha);
            if fixed.contains(&off) {
                let r = frames.next().expect("frame row");
                let mut got = format!(
                    "{ms} {} {} {} {} {act:#x}",
                    c.page,
                    c.alpha,
                    u8::from(c.next),
                    u8::from(c.done)
                );
                if full {
                    c.draw(&mut screen);
                    got += &format!(" {} {}", md5(&screen_bytes(&screen)), samples(&screen));
                }
                assert_eq!(got, r[1..].join(" "), "control word {label}");
                nframes += 1;
            }
            if act != 0 {
                break;
            }
        }
        if !full {
            let r = sweeps.next().expect("sweep row");
            assert_eq!(
                (r[1], r[2]),
                (cycle.to_string().as_str(), start.to_string().as_str())
            );
            // Offsets 4990..7010 that ran (the last cycle stops at the end).
            let mut sw = Vec::new();
            for (off, a) in offsets(true).iter().zip(&seq) {
                if (4990..=7010).contains(off) {
                    sw.push(*a);
                }
            }
            assert_eq!(md5(&sw), r[3], "sweep {label} cycle {cycle}");
        }
        if act != 0 {
            break;
        }
        start += 7016;
        cycle += 1;
    }
    assert!(frames.next().is_none() && sweeps.next().is_none());
    assert!(nframes > 100);
}

#[test]
fn the_frames_are_the_originals() {
    let Some(files) = game() else { return };
    let n = pages(&files).len();
    run("027f", pictures(&files), n);
}

#[test]
fn the_fade_follows_the_control_word() {
    let Some(files) = game() else { return };
    let n = pages(&files).len();
    let pics = pictures(&files);
    run("007f", pics.clone(), n);
    run("0e7f", pics, n);
}

#[test]
fn the_blend_on_synthetic_pictures() {
    // credits.synthetic of the harness: old (x, y, x + y), new (255 − y,
    // 3x, x ^ y) as B, G, R bytes.
    let (mut old, mut new) = (Vec::new(), Vec::new());
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            old.extend([x as u8, y as u8, (x + y) as u8]);
            new.extend([(255 - (y & 255)) as u8, (3 * x) as u8, (x ^ y) as u8]);
        }
    }
    let mut screen = vec![0u16; WIDTH * HEIGHT];
    let mut n = 0;
    for r in rows("blend") {
        credits::blend(&old, &new, r[0].parse().unwrap(), &mut screen);
        assert_eq!(
            format!("{} {}", md5(&screen_bytes(&screen)), samples(&screen)),
            r[1..].join(" "),
            "alpha {}",
            r[0]
        );
        n += 1;
    }
    assert_eq!(n, 8);
}

/// A 640 x 480 24-bit TGA of the harness's pattern (type 2 or 10, rows
/// top-down or bottom-up), as `credits.py tga24` writes it.
fn tga24(kind: u8, top_down: bool) -> Vec<u8> {
    let (w, h) = (WIDTH as u16, HEIGHT as u16);
    let mut f = vec![0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    f.extend(w.to_le_bytes());
    f.extend(h.to_le_bytes());
    f.extend([24, if top_down { 0x20 } else { 0 }]);
    let row = |y: usize| -> Vec<[u8; 3]> {
        (0..WIDTH)
            .map(|x| [x as u8, y as u8, (x + y) as u8])
            .collect()
    };
    let ys: Vec<usize> = if top_down {
        (0..HEIGHT).collect()
    } else {
        (0..HEIGHT).rev().collect()
    };
    for y in ys {
        let px = row(y);
        if kind == 2 {
            f.extend(px.concat());
            continue;
        }
        // No two neighbours are equal: raw packets of up to 128 pixels.
        for chunk in px.chunks(128) {
            f.push(chunk.len() as u8 - 1);
            f.extend(chunk.concat());
        }
    }
    f
}

/// A 24-bit picture goes into a page surface unchanged, as the original
/// copies it (`credits_tga24.txt`).
#[test]
fn a_24_bit_picture_is_copied_as_it_is() {
    let golden = include_str!("golden/credits_tga24.txt");
    let mut n = 0;
    for r in golden.lines().filter(|l| l.starts_with("tga24 ")) {
        let r: Vec<&str> = r.split_whitespace().collect();
        let file = tga24(r[1].parse().unwrap(), r[2] == "1");
        assert_eq!(md5(&file), r[3], "the harness's file");
        assert_eq!(
            md5(&Bgr24::parse(&file).unwrap().bytes),
            r[4],
            "type {} top-down {}",
            r[1],
            r[2]
        );
        assert_eq!(r[5], "yes");
        n += 1;
    }
    assert_eq!(n, 4);
}
