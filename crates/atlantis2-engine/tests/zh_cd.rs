//! The Interwise release (Taiwan, 4 CDs, 2000): two disc 1s, each with its own
//! installer, and discs 2 to 4 shared, the North American CD's files.
//!
//! - The Traditional Chinese disc 1 installs the 1.01 patch's executable
//!   byte for byte (version 3.18, no copy protection) and the Polish CD's
//!   files but `loc\`: Big5 texts with `[CharacterSet] MBCS`, line breaks
//!   at the space and at `，` `。` `？` `！`, and a Japanese font
//!   (`atl_font.ttf`, HG-Gothic-M) that has no Big5 character set. Its
//!   disc holds Interwise's logo before the intro.
//! - The English disc 1 installs the Polish CD's executable (1.01's code,
//!   3.15) and English texts: GOG's English but for the line that asks for
//!   the CD.
//!
//! The voices and the films' tracks are English on every disc, the North
//! American CD's files.
//!
//! Needs `ATLANTIS2_DATA_ZH` (the Chinese disc 1's `data1.cab`
//! extracted), `ATLANTIS2_DATA_ZH_EN` (the English disc 1's) and
//! `ATLANTIS2_DISCS_ZH` (a folder holding the five CD images), only read;
//! `ATLANTIS2_PATCH101` (the 1.01 patch's executable),
//! `ATLANTIS2_DATA_PL` / `ATLANTIS2_DISCS_PL` (the Polish CD) and
//! `ATLANTIS2_DATA_US` / `ATLANTIS2_DISCS_US` (the North American CD) for
//! what the release shares with them; `ATLANTIS2_PNG_DIR`, when set,
//! receives `zh-*.png`.
//!
//! The programs are only read (their size, MD5 and data tables), never
//! run.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ab_glyph::Font;
use atlantis2_data::dialog::{parse_dial, FontTxt};
use atlantis2_data::edition::Language;
use atlantis2_data::exe::{identify, Release};
use atlantis2_data::files::{disc_number, find_exe, installer_language};
use atlantis2_data::menu_txt::MenuTxt;
use atlantis2_data::messages::ObjectNames;
use atlantis2_data::{defnames, A2Files, DefNames, Folder};
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::subtitle::{layout, SubtitleFont};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};
use atlantis_data::disc_image::IMAGE_EXTENSIONS;
use atlantis_data::files::{FileSource, StdDir};
use atlantis_data::TextCodePage;
use atlantis_engine::subtitles::FontFace;
use md5::{Digest, Md5};

/// The 1.01 patch's build: the Chinese disc 1's program is that file.
const ID: &str = "atlantis2-patch101-2183168";
const LABEL: &str = "Atlantis II, Traditional Chinese CD (Interwise, 2000)";
/// The Polish CD's build: the English disc 1's program.
const ID_EN: &str = "atlantis2-cd-pl-2183168";
const LABEL_EN: &str = "Atlantis II, English CD (Interwise, 2000)";
const WORLDS: [&str; 5] = ["CH", "IR", "SH", "TI", "YU"];
const CP: TextCodePage = TextCodePage::Windows950;
/// The 1.01 executable's subtitle layout (@ 0x0045a2f0) on this CD's
/// `loc\` under emulation, with a model of MingLiU's widths (atlantis2-re
/// `tools/emu/subtitles_101_zh.py`).
const GOLDEN: &str = include_str!("golden/subtitles_101_zh.txt");

fn folders(data: &str) -> Option<(PathBuf, PathBuf)> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os(data),
        std::env::var_os("ATLANTIS2_DISCS_ZH"),
    ) else {
        atlantis2_data::testing::missing(&format!("{data} / ATLANTIS2_DISCS_ZH not set"));
        return None;
    };
    Some((PathBuf::from(dir), PathBuf::from(discs)))
}

fn open(data: &str, id: &str) -> Option<A2Files> {
    let (dir, discs) = folders(data)?;
    let (files, build) = A2Files::open_dir(&dir, &[discs]).expect(data);
    assert_eq!(build.id, id);
    Some(files)
}

fn game() -> Option<A2Files> {
    open("ATLANTIS2_DATA_ZH", ID)
}

fn english() -> Option<A2Files> {
    open("ATLANTIS2_DATA_ZH_EN", ID_EN)
}

/// An edition from its two variables, when they are set (optional here).
fn other(data: &str, discs: &str, what: &str) -> Option<A2Files> {
    let dir = std::env::var_os(data)?;
    let discs = std::env::var_os(discs)?;
    Some(
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])
            .expect(what)
            .0,
    )
}

fn us_cd() -> Option<A2Files> {
    other(
        "ATLANTIS2_DATA_US",
        "ATLANTIS2_DISCS_US",
        "the North American CD",
    )
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

fn shot(screen: &[u16], w: usize, h: usize, name: &str) {
    if let Some(dir) = std::env::var_os("ATLANTIS2_PNG_DIR") {
        let png = atlantis2_engine::png::encode_rgb565(screen, w, h);
        std::fs::write(PathBuf::from(dir).join(name), png).unwrap();
    }
}

fn bytemuck_u16(s: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding and u8 no alignment.
    unsafe { std::slice::from_raw_parts(s.as_ptr().cast(), s.len() * 2) }
}

/// The files of a folder, by path relative to it.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.push(rel.replace('\\', "/"));
        }
    }
}

/// The characters of Big5 bytes: (lead, trail) pairs and single bytes,
/// as GDI reads them in code page 950.
fn big5_chars(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let n = if CP.is_lead_byte(bytes[i]) && i + 1 < bytes.len() {
            2
        } else {
            1
        };
        out.push(&bytes[i..i + n]);
        i += n;
    }
    out
}

/// A golden `TextOutA` call: x, y, colour, MD5 of the bytes, their
/// offset in the text.
type Call<'a> = (i32, i32, u32, &'a str, i64);

/// A `u32` of a TrueType font's `OS/2` table at `offset`.
fn os2_u32(ttf: &[u8], offset: usize) -> u32 {
    let n = usize::from(u16::from_be_bytes([ttf[4], ttf[5]]));
    let rec = (0..n)
        .map(|i| &ttf[12 + 16 * i..28 + 16 * i])
        .find(|r| &r[..4] == b"OS/2")
        .unwrap();
    let at = u32::from_be_bytes(rec[8..12].try_into().unwrap()) as usize + offset;
    u32::from_be_bytes(ttf[at..at + 4].try_into().unwrap())
}

/// The engine at 59.94 passes a second, the clock running with them.
struct Run {
    e: Engine,
    pass: u64,
    screen: Vec<u16>,
}

impl Run {
    fn new(e: Engine) -> Run {
        let (w, h) = e.screen_size();
        Run {
            e,
            pass: 0,
            screen: vec![0; w * h],
        }
    }

    fn step(&mut self) -> Vec<Event> {
        self.e.set_clock(self.pass as f64 * PASS);
        self.e.pass(Some(&mut self.screen));
        self.pass += 1;
        std::mem::take(&mut self.e.events)
    }
}

fn boot(files: A2Files, data: &Path) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.trace = true;
    e.set_data_dir(Some(data.to_path_buf()));
    e
}

/// A new game, its opening film ended, standing in Tibet.
fn in_game(files: A2Files, data: &Path) -> Run {
    let mut e = boot(files, data);
    e.start_new_game().unwrap();
    e.end_video();
    let mut r = Run::new(e);
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    r
}

/// The Chinese disc 1's program known by its MD5: the 1.01 patch's file,
/// so its build; the language Traditional Chinese (told by `Menu.txt`),
/// the texts Big5, the edition named after its own release; the
/// installed files the Polish CD's but `loc\` and one script; the disc 1
/// taken the Chinese one, the English one left.
#[test]
fn the_edition_is_named_and_its_own_files_read() {
    let Some((dir, _)) = folders("ATLANTIS2_DATA_ZH") else {
        return;
    };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(exe.len(), 2_183_168);
    assert_eq!(hex(&exe), "255310e7d6bd7f823f680e2bd3be8b7f");
    let b = identify(&exe).unwrap();
    assert_eq!(b.id, ID);
    assert_eq!((b.release, b.gog), (Release::V101, false));
    assert_eq!(b.version, "Atlantis 2 - Version 3.18 - 8/10/1999");
    if let Some(patch) = std::env::var_os("ATLANTIS2_PATCH101") {
        assert_eq!(hex(&exe), hex(&std::fs::read(patch).unwrap()));
    }

    let files = game().unwrap();
    assert_eq!(files.exe_name(), Some("atlantis2.exe"));
    assert_eq!(files.language(), Some(Language::TraditionalChinese));
    assert_eq!(files.label(), LABEL);
    assert_eq!(files.text_code_page(), CP);
    assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
    let disc1 = files.disc_origin(1).unwrap();
    assert!(disc1.contains("XITOYCHA"), "{disc1}");

    // `loc\Menu.txt`: every section read, in Chinese.
    let raw = files.read(Folder::Loc, "Menu.txt").unwrap();
    assert_eq!(raw.len(), 928);
    let menu = MenuTxt::parse(&raw);
    assert!(menu.padded.is_empty(), "{:?}", menu.padded);
    assert!(CP.decode(&raw).contains("主選單"));
    // `Font.txt`: MBCS, the Big5 punctuation as break characters, the
    // Japanese font's face (no `[Language]`: 1.01 reads none).
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, Some(true));
    assert_eq!(font_txt.breaks, [0x20, 0xa148, 0xa141, 0xa143, 0xa149]);
    let faces: BTreeSet<&str> = font_txt.fonts.iter().map(|f| f.face.as_str()).collect();
    assert_eq!(faces, BTreeSet::from(["HG-Gothic-M Medium"]));
    let heights: Vec<i32> = font_txt.fonts.iter().map(|f| f.height).collect();
    assert_eq!(heights, [12, 18, 20, 12]);
    // That font: Japanese (code pages Latin 1 and JIS, not Big5), and
    // without 94 of the 1350 characters of the texts.
    let ttf = files.read(Folder::Loc, "atl_font.ttf").unwrap();
    assert_eq!(ttf.len(), 2_497_912);
    assert_eq!(os2_u32(&ttf, 78), 0x0002_0001);
    let shipped = ab_glyph::FontRef::try_from_slice(&ttf).unwrap();
    let mut used = BTreeSet::new();
    for name in [
        "DialCH.txt",
        "DialIR.txt",
        "DialSH.txt",
        "DialTI.txt",
        "DialYU.txt",
        "Menu.txt",
        "Messages.txt",
        "Credits.txt",
    ] {
        let data = files.read(Folder::Loc, name).unwrap();
        // (Not the pairs of code page 950's user-defined range, see
        // below.)
        for c in big5_chars(&data) {
            if c.len() == 2 && c[0] >= 0xa1 {
                used.extend(CP.decode(c).chars().filter(|&c| c != '\0'));
            }
        }
    }
    let lacking: String = used
        .iter()
        .filter(|&&c| shipped.glyph_id(c).0 == 0)
        .collect();
    assert_eq!((used.len(), lacking.chars().count()), (1350, 94));
    assert!(
        lacking.contains('你') && lacking.contains('說'),
        "{lacking}"
    );

    if let Some(pl_dir) = std::env::var_os("ATLANTIS2_DATA_PL") {
        // Every installed file but the program, `loc\` and one script:
        // the Polish CD's, byte for byte.
        let mut names = Vec::new();
        walk(&dir, &dir, &mut names);
        assert_eq!(names.len(), 248);
        let pl = StdDir::new(PathBuf::from(pl_dir));
        let mut differ: Vec<String> = Vec::new();
        for name in &names {
            let ours = std::fs::read(dir.join(name)).unwrap();
            let theirs = pl.read(name).unwrap_or_else(|| panic!("{name}"));
            if hex(&ours) != hex(&theirs) {
                differ.push(name.to_ascii_lowercase());
            }
        }
        differ.sort();
        let loc: Vec<&String> = differ.iter().filter(|n| n.starts_with("loc/")).collect();
        assert_eq!(loc.len(), 10, "{differ:?}");
        assert_eq!(
            differ
                .iter()
                .filter(|n| !n.starts_with("loc/"))
                .collect::<Vec<_>>(),
            ["atlantis2.exe", "scenar/chapi083.con"]
        );
    }
    if let Some(us) = us_cd() {
        // The texts: the North American CD's lines, in the same order;
        // its `scenar\chapi083.con` too; the same objects.
        for code in WORLDS {
            let name = format!("Dial{code}.txt");
            let ids = |f: &A2Files| -> Vec<String> {
                parse_dial(&f.read(Folder::Loc, &name).unwrap())
                    .into_iter()
                    .map(|d| d.id)
                    .collect()
            };
            assert_eq!(ids(&files), ids(&us), "{name}");
        }
        assert_eq!(
            files.read(Folder::Scenar, "chapi083.con"),
            us.read(Folder::Scenar, "chapi083.con")
        );
        let us_menu = MenuTxt::parse(&us.read(Folder::Loc, "Menu.txt").unwrap());
        assert_eq!(menu.sections.len(), us_menu.sections.len());
        for (a, e) in menu.sections.iter().zip(&us_menu.sections) {
            assert_eq!(a.len(), e.len());
        }
        let keys = |f: &A2Files| -> Vec<Vec<u8>> {
            ObjectNames::parse(&f.read(Folder::Loc, "Messages.txt").unwrap())
                .entries()
                .map(|(k, _)| k.to_vec())
                .collect()
        };
        assert_eq!(keys(&files), keys(&us));
    }
    // `Messages.txt`: every object name is a `defnames.con` object.
    let names = ObjectNames::parse(&files.read(Folder::Loc, "Messages.txt").unwrap());
    let dn = DefNames::parse(&files.read(Folder::Scenar, "defnames.con").unwrap()).unwrap();
    assert!(names.len() > 50, "{}", names.len());
    for (key, name) in names.entries() {
        assert!(dn.id(defnames::OBJETS, key).is_ok(), "{key:?} {name:?}");
    }
}

/// The English disc 1: the Polish CD's program with English texts, named
/// after its release; its own disc 1 taken although the Chinese one comes
/// first in the folder; the Chinese install's files but the program,
/// `loc\` and one script.
#[test]
fn the_english_disc_is_named_and_its_own_disc_taken() {
    let Some((dir, discs)) = folders("ATLANTIS2_DATA_ZH_EN") else {
        return;
    };
    let exe = std::fs::read(dir.join("atlantis2.exe")).unwrap();
    assert_eq!(hex(&exe), "f2b6f7bfa3c1952dceddfa1a662ebb84");
    assert_eq!(identify(&exe).unwrap().id, ID_EN);
    let files = english().unwrap();
    assert_eq!(files.language(), Some(Language::English));
    assert_eq!(files.label(), LABEL_EN);
    assert_eq!(files.text_code_page(), TextCodePage::Windows1252);
    let disc1 = files.disc_origin(1).unwrap();
    assert!(disc1.contains("INGLIZCHA"), "{disc1}");
    // The two disc 1s: their installers' languages.
    let mut lids = BTreeMap::new();
    for e in std::fs::read_dir(&discs).unwrap().flatten() {
        let p = e.path();
        let image = atlantis_data::disc_image::DiscImage::open(&p);
        if let Some(img) = image.filter(|i| disc_number(i) == 1) {
            lids.insert(installer_language(&img), p.file_name().unwrap().to_owned());
        }
    }
    assert_eq!(
        lids.keys().copied().collect::<Vec<_>>(),
        [Some(0x0009), Some(0x0404)]
    );
    // `Font.txt`: the Polish CD's (SBCS, 1.01's heights), the font the
    // French CD's.
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    assert_eq!(font_txt.mbcs, None);
    assert_eq!(font_txt.breaks, [0x20]);
    assert_eq!(
        hex(&files.read(Folder::Loc, "Atl_Font.ttf").unwrap()),
        "25c91ccae274b4862a9319b7ddae1d26"
    );
    if let Some(us) = us_cd() {
        // The dialogs: the North American CD's English files.
        for code in WORLDS {
            let name = format!("Dial{code}.txt");
            assert_eq!(
                files.read(Folder::Loc, &name),
                us.read(Folder::Loc, &name),
                "{name}"
            );
        }
    }
    if let Some(zh) = std::env::var_os("ATLANTIS2_DATA_ZH") {
        let zh = PathBuf::from(zh);
        let mut names = Vec::new();
        walk(&dir, &dir, &mut names);
        let same = names
            .iter()
            .filter(|n| {
                let theirs = StdDir::new(&zh).read(n);
                theirs.is_some_and(|t| hex(&t) == hex(&std::fs::read(dir.join(n)).unwrap()))
            })
            .count();
        assert_eq!((names.len(), same), (248, 236));
    }
}

/// Every subtitle laid out as the 1.01 executable lays it out (the
/// golden, with a model of the font's widths: a Big5 pair as wide as the
/// em, a byte half of it), within the screen with the port's font; every
/// character of `loc\` has a glyph in the port's Noto Serif TC, but the
/// four pairs of `Credits.txt` code page 950 leaves undefined.
#[test]
fn every_subtitle_and_every_character() {
    let Some(files) = game() else { return };
    let font_txt = FontTxt::parse(&files.read(Folder::Loc, "Font.txt").unwrap());
    let font = SubtitleFont::load(&files, &font_txt, "AtlFT_Dialog").unwrap();
    assert_eq!(font.code_page(), CP);
    // `Height` 18 as a cell height: 18 pixels per em (MingLiU's em is its
    // cell), Noto Serif TC's ideographs' top 0.88 em above the baseline,
    // lines 19 apart.
    assert_eq!(font.metrics(), (18, 16, 18));
    assert_eq!(font.line_step(), 19);
    let popup = SubtitleFont::load(&files, &font_txt, "AtlFT_POPUP").unwrap();
    assert_eq!(popup.metrics(), (12, 11, 12));
    // An ideograph is one character, as wide as the em; `，` too.
    assert_eq!(font.extent(&[0xa7, 0xda]).0, 18);
    assert_eq!(font.extent(&[0xa7, 0xda, 0xa1, 0x41]).0, 36);

    // The golden: the break table and the flag as the original reads
    // them, every world line's calls.
    assert!(GOLDEN.contains("\nmbcs 1\n"));
    assert!(GOLDEN.contains("\nbreaks 0020 a148 a141 a143 a149\n"));
    assert!(GOLDEN.contains("\nfont f2 HG-Gothic-M_Medium 18 | 18 14 18\n"));
    let model = |b: &[u8]| -> (i32, i32) {
        let w = big5_chars(b)
            .iter()
            .map(|c| if c.len() == 2 { 18 } else { 9 })
            .sum();
        (w, 18)
    };
    let mut texts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for code in WORLDS {
        let data = files.read(Folder::Loc, &format!("Dial{code}.txt")).unwrap();
        for d in parse_dial(&data) {
            texts.insert(format!("{code}.{}", d.id), d.text);
        }
    }
    let mut golden: BTreeMap<&str, Vec<Call>> = BTreeMap::new();
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first() {
            Some(&"text") if !f[1].starts_with("synthetic") => {
                let text = &texts[f[1]];
                assert_eq!(
                    (text.len().to_string(), hex(text)),
                    (f[2].into(), f[3].into())
                );
                golden.entry(f[1]).or_default();
            }
            Some(&"call") if !f[1].starts_with("synthetic") => {
                golden.entry(f[1]).or_default().push((
                    f[3].parse().unwrap(),
                    f[4].parse().unwrap(),
                    u32::from_str_radix(f[5], 16).unwrap(),
                    f[7],
                    f[8].parse().unwrap(),
                ));
            }
            _ => {}
        }
    }
    assert_eq!(golden.len(), texts.len());
    assert_eq!(texts.len(), 1214);
    let mut lines = BTreeMap::new();
    for (key, want) in &golden {
        let text = &texts[*key];
        let got: Vec<(i32, i32, u32, String, i64)> =
            layout(text, model, 19, &font_txt.breaks, true, 640, 480)
                .iter()
                .map(|c| {
                    let off = if c.bytes.is_empty() {
                        0
                    } else {
                        text.windows(c.bytes.len())
                            .position(|w| w == c.bytes.as_slice())
                            .map_or(-1, |p| p as i64)
                    };
                    (c.x, c.y, c.colour, hex(&c.bytes), off)
                })
                .collect();
        let want: Vec<(i32, i32, u32, String, i64)> = want
            .iter()
            .map(|&(x, y, c, m, o)| (x, y, c, m.to_string(), o))
            .collect();
        assert_eq!(got, want, "{key}");
        *lines.entry(want.len() / 2).or_insert(0) += 1;
        // Every row ends at a character's end.
        for c in layout(text, model, 19, &font_txt.breaks, true, 640, 480) {
            assert!(!CP.decode(&c.bytes).contains('\0'), "{key}");
        }
    }
    eprintln!("rows per line (the model's widths): {lines:?}");

    // The port's font: every row but its last character within the
    // width W − 20 (the row ends after the character that reached it,
    // when that one is a break: a row of 35 ideographs ending with `，`
    // takes 630 pixels from x = 10, one past the screen, as in the
    // original).
    let mut wide = 0;
    for (key, text) in &texts {
        for c in font.layout(text, 640, 480) {
            let chars = big5_chars(&c.bytes);
            let last = chars.last().map_or(0, |l| l.len());
            let (w, _) = font.extent(&c.bytes[..c.bytes.len() - last]);
            assert!(c.x >= 0 && w < 620, "{key}: {c:?}");
            if c.x + font.extent(&c.bytes).0 > 640 {
                wide += 1;
            }
        }
    }
    eprintln!("rows past the screen's edge by their last character: {wide}");
    // Every character of `loc\` in the port's Noto Serif TC.
    let face = FontFace::bundled(CP);
    let mut undefined = Vec::new();
    for name in [
        "DialCH.txt",
        "DialIR.txt",
        "DialSH.txt",
        "DialTI.txt",
        "DialYU.txt",
        "Menu.txt",
        "Messages.txt",
        "Credits.txt",
    ] {
        let data = files.read(Folder::Loc, name).unwrap();
        for c in big5_chars(&data) {
            if c[0] < 0x20 {
                continue;
            }
            if c.len() == 2 && c[0] < 0xa1 {
                undefined.push((name, c.to_vec()));
                continue;
            }
            let s = CP.decode(c);
            let ch = s.chars().next().unwrap();
            assert!(face.has_char(ch), "{name}: {c:02x?} {s}");
        }
    }
    // A Windows-1252 apostrophe (0x92) before a letter in the music's
    // credits: a pair of code page 950's user-defined range.
    assert_eq!(undefined.len(), 4, "{undefined:?}");
    assert!(undefined
        .iter()
        .all(|(n, c)| *n == "Credits.txt" && c[0] == 0x92));

    let text = b"\xa7\xda\xac\x4f\xa4\xf4\xb4\xb9\xb2\x79\xaa\xba\xa6\x75\xc5\x40\xaa\xcc\xa1\x43";
    assert_eq!(CP.decode(text), "我是水晶球的守護者。");
    let mut out = vec![0u16; 640 * 480];
    font.draw(&mut out, 640, 480, &font.layout(text, 640, 480));
    assert!(out.iter().filter(|&&p| p == 0xffff).count() > 300);
    shot(&out, 640, 480, "zh-characters.png");
}

/// Films 19, 55 and 66 and the voices: the North American CD's English
/// files on every disc, no film track in `apc\dial\`.
#[test]
fn the_voices_and_films_are_english() {
    let Some(files) = game() else { return };
    let Some(us) = us_cd() else {
        eprintln!("ATLANTIS2_DATA_US / ATLANTIS2_DISCS_US not set: nothing to compare");
        return;
    };
    for film in [19, 55, 66] {
        let name = format!("cine{film:03}.apc");
        let (track, _) = files.read_with_disc(Folder::Apc, &name).unwrap();
        let (theirs, _) = us.read_with_disc(Folder::Apc, &name).unwrap();
        assert_eq!(hex(&track), hex(&theirs), "{name}");
        for disc in 1..=4 {
            assert_eq!(files.size_on_disc(disc, Folder::ApcDial, &name), None);
        }
    }
    for voice in ["360_01.apc", "363_04.apc"] {
        let ours = files.read(Folder::ApcDial, voice).unwrap();
        assert_eq!(hex(&ours), hex(&us.read(Folder::ApcDial, voice).unwrap()));
    }
}

/// The intro on each disc 1: the Chinese one plays Interwise's logo (90
/// frames, no track) before the two films every European CD has; the
/// English one those two films alone, as `intro00` and `intro01`.
#[test]
fn the_intros_of_the_two_disc_1s() {
    let Some(files) = game() else { return };
    let Some(en) = english() else { return };
    assert!(files.read_current(Folder::Apc, "intro00.apc").is_none());
    let logo = files.read_current(Folder::Hnm, "intro00.hnm").unwrap();
    assert_eq!(&logo[..4], b"HNM6");
    assert_eq!(hex(&logo), "eb06f8eeb1d2316d8253622549471af6");
    for (a, b) in [("intro01", "intro00"), ("intro02", "intro01")] {
        for folder in [Folder::Hnm, Folder::Apc] {
            let ext = if folder == Folder::Hnm { "hnm" } else { "apc" };
            assert_eq!(
                files.read_current(folder, &format!("{a}.{ext}")),
                en.read_current(folder, &format!("{b}.{ext}")),
                "{a}.{ext}"
            );
        }
    }
    let intro = |files: A2Files, name: &str, png: &str| {
        let data = scratch(name);
        let mut e = boot(files, &data);
        e.open_player_menu();
        e.start_intro();
        let mut r = Run::new(e);
        let mut journal = std::mem::take(&mut r.e.events);
        let mut seen = false;
        for _ in 0..60 * 60 * 5 {
            if r.e.loop_type == LoopType::Menu {
                break;
            }
            journal.extend(r.step());
            if !seen
                && r.e
                    .video_playing()
                    .is_some_and(|(n, k)| n == "intro00.hnm" && k == 40)
            {
                let colours: BTreeSet<u16> = r.screen.iter().copied().collect();
                assert!(colours.len() > 20, "{name}: {} colours", colours.len());
                shot(&r.screen, 640, 480, png);
                seen = true;
            }
        }
        assert!(seen, "{name}");
        assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Player));
        let _ = std::fs::remove_dir_all(&data);
        let mut videos: Vec<(String, Option<u32>)> = Vec::new();
        let mut sounds = Vec::new();
        for ev in &journal {
            match ev {
                Event::VideoStart { name, frames } => videos.push((name.clone(), *frames)),
                Event::Sound(s) => sounds.push(s.clone()),
                _ => {}
            }
        }
        (videos, sounds)
    };
    let (videos, sounds) = intro(files, "a2-zh-intro", "zh-logo.png");
    let names: Vec<&str> = videos.iter().map(|v| v.0.as_str()).collect();
    assert_eq!(
        names,
        ["intro00.hnm", "intro01.hnm", "intro02.hnm", "intro03.hnm"],
        "{videos:?}"
    );
    assert_eq!((videos[0].1, videos[3].1), (Some(90), None));
    assert_eq!(sounds, ["intro01.apc", "intro02.apc"]);
    let (videos, sounds) = intro(en, "a2-zh-en-intro", "zh-en-intro.png");
    let names: Vec<&str> = videos.iter().map(|v| v.0.as_str()).collect();
    assert_eq!(
        names,
        ["intro00.hnm", "intro01.hnm", "intro02.hnm"],
        "{videos:?}"
    );
    assert_eq!(videos[2].1, None);
    assert_eq!(sounds, ["intro00.apc", "intro01.apc"]);
}

/// The Tibet line with the most characters.
fn showcase(dial: &[u8]) -> (u32, String, Vec<u8>) {
    let d = parse_dial(dial)
        .into_iter()
        .max_by_key(|d| d.text.len())
        .unwrap();
    (d.numbers().unwrap().0, d.id, d.text)
}

/// A new game to a Tibet dialog: a Chinese subtitle drawn on two or more
/// rows, its English voice; the main menu's tooltip in Chinese; a save
/// and a load.
#[test]
fn a_subtitle_the_main_menu_and_a_save() {
    let Some(files) = game() else { return };
    let data = scratch("a2-zh-game");
    let mut r = in_game(files, &data);
    r.e.enter_view(7, (-164.394f32).to_radians(), 14.96f32.to_radians())
        .unwrap();
    for _ in 0..200 {
        match r.e.loop_type {
            LoopType::Video => r.e.end_video(),
            LoopType::Dialog => r.e.end_dialog(),
            _ => {}
        }
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);
    assert!(r.e.save_slot(3));

    let dial = r.e.files().read(Folder::Loc, "DialTI.txt").unwrap();
    let (n, id, text) = showcase(&dial);
    r.e.start_dialog(n).unwrap();
    let (mut shown, mut other) = (None, None);
    for _ in 0..60 * 120 {
        let pass = r.pass;
        for ev in r.step() {
            if let Event::DialogLine(l) = ev {
                let line = r.e.dialog().unwrap().current_line().unwrap();
                assert!(r.e.files().read(Folder::ApcDial, &line.voice).is_some());
                if l == id {
                    shown = Some(pass);
                } else {
                    other = Some(pass);
                }
            }
        }
        if shown.is_none() && other.is_some_and(|p| pass == p + 30) {
            r.e.click(pass as f64 * PASS);
        }
        if shown.is_some_and(|p| pass == p + 30) {
            break;
        }
    }
    assert!(shown.is_some(), "{id} not shown");
    let font = r.e.subtitle_font().unwrap();
    assert_eq!(font.code_page(), CP);
    assert!(font.layout(&text, 640, 480).len() >= 4);
    let white = r.screen.iter().filter(|&&p| p == 0xffff).count();
    assert!(white > 300, "{white} white pixels");
    eprintln!("{id}: {}", CP.decode(&text));
    shot(&r.screen, 640, 480, "zh-subtitle.png");
    r.e.end_dialog();
    for _ in 0..30 {
        r.step();
    }
    assert_eq!(r.e.loop_type, LoopType::Warp);

    r.e.escape();
    r.step();
    r.step();
    assert_eq!(r.e.menu().and_then(|m| m.kind()), Some(PageKind::Main));
    let bare = r.screen.clone();
    let (cx, cy, tip) = {
        let page = r.e.menu().unwrap().page.as_ref().unwrap();
        let w = page
            .widgets
            .iter()
            .find(|w| w.enabled && w.visible && !w.text.is_empty())
            .expect("a tooltip");
        (
            (w.hit.left + w.hit.right) / 2,
            (w.hit.top + w.hit.bottom) / 2,
            w.text.clone(),
        )
    };
    let menu_txt = r.e.files().read(Folder::Loc, "Menu.txt").unwrap();
    assert!(menu_txt.windows(tip.len()).any(|w| w == tip));
    let tip = CP.decode(&tip);
    assert!(!tip.contains('\0') && !tip.is_ascii(), "{tip}");
    r.e.pointer(cx, cy);
    for _ in 0..120 {
        r.step();
    }
    assert_ne!(
        hex(bytemuck_u16(&bare)),
        hex(bytemuck_u16(&r.screen)),
        "{tip}"
    );
    eprintln!("tooltip: {tip}");
    shot(&r.screen, 640, 480, "zh-main-menu-tooltip.png");
    r.e.escape();
    for _ in 0..10 {
        r.step();
    }
    r.e.load_slot(3).unwrap();
    for _ in 0..5 {
        r.step();
    }
    assert_eq!(r.e.place(), ("tibet.wam", 7));
    assert!(data.join("save").is_dir());
    let _ = std::fs::remove_dir_all(&data);
}

/// A copy of a disc as the browser sees it (`atlantis_app`'s `PickedDir`):
/// files by lower-case path, no folders in a listing.
struct PickedCopy(Arc<dyn FileSource>);

impl FileSource for PickedCopy {
    fn size(&self, name: &str) -> Option<u64> {
        self.0.size(name)
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.0.read_at(name, offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.0.has_dir(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        let d = dir.trim_end_matches(['/', '\\']);
        self.0
            .list(dir)
            .into_iter()
            .filter(|n| !self.0.has_dir(&format!("{d}/{n}")))
            .collect()
    }
}

/// The browser's way in: each installed folder and the five images
/// (both disc 1s), as for the other CDs; each half takes its own disc 1,
/// the intro started.
#[test]
fn the_browser_s_way_of_opening_the_edition() {
    for (data, id, label, disc1) in [
        ("ATLANTIS2_DATA_ZH", ID, LABEL, 0x0404),
        ("ATLANTIS2_DATA_ZH_EN", ID_EN, LABEL_EN, 0x0009),
    ] {
        let Some((dir, discs)) = folders(data) else {
            return;
        };
        let install = StdDir::new(&dir);
        let (exe_name, build, _) = find_exe(&install).unwrap();
        assert_eq!(build.id, id);
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&discs)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|e| {
                    IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
                })
            })
            .collect();
        paths.sort();
        let mut copies: Vec<(String, Arc<dyn FileSource>)> = Vec::new();
        for p in paths {
            let image = atlantis_data::disc_image::DiscImage::open(&p).unwrap();
            let n = disc_number(&image);
            if n > 0 {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                copies.push((name, Arc::new(PickedCopy(Arc::new(image)))));
            }
        }
        assert_eq!(copies.len(), 5);
        let mut files = A2Files::new(Arc::new(install), copies.clone());
        files.set_exe(exe_name, build);
        assert_eq!(files.disc_numbers(), [1, 2, 3, 4]);
        assert_eq!(files.label(), label);
        let origin = files.disc_origin(1).unwrap();
        let (_, source) = copies.iter().find(|(n, _)| n == origin).unwrap();
        assert_eq!(installer_language(source.as_ref()), Some(disc1));
        assert!(files.read_current(Folder::Hnm, "intro00.hnm").is_some());
        let voice = files.read(Folder::ApcDial, "360_01.apc").unwrap();
        assert!(atlantis_codecs::decode_apc(&voice).is_ok());
        let data = scratch("a2-zh-browser");
        let mut e = boot(files, &data);
        e.open_player_menu();
        e.start_intro();
        let mut r = Run::new(e);
        assert_eq!(r.e.video_playing().map(|(n, _)| n), Some("intro00.hnm"));
        for _ in 0..30 {
            r.step();
        }
        let _ = std::fs::remove_dir_all(&data);
    }
}
