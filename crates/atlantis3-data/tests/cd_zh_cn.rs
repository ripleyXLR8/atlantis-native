//! The Simplified Chinese CD (*亚特兰蒂斯3*, 2002, three CDs) on the real
//! files (`ATLANTIS3_CD_ZH_CN`: the three `.bin` / `.cue` images and
//! `installed\`, the first disc's `GAMEDATA\` copied; optional, the tests
//! write `SKIPPED` without it):
//!
//! - the program `Atlantis3.exe` is a build of its own,
//!   atlantis3-cd-zh-cn-3682370, whose behaviours are the Portuguese CD's
//!   program's ([`Release::CdPt2001`]); the registry values `gbr` and
//!   `CD`; the edition "Atlantis III, Chinese CD, Simplified Chinese
//!   (2002)", told by the text hook's table `GSooAPI.res` beside it;
//! - the data are GOG.com English's: `font.txt` asks for `Arial` in
//!   `SHIFTJIS_CHARSET`, the texts of `loc\text\gbr\` are English;
//! - the table: its entries, every `Dial.txt` text but one found
//!   by the hook's search, the Chinese GB2312 only, `i_credit.txt`'s lines
//!   not in it;
//! - the three discs are found from the `.bin` images; every file of the
//!   install and of the discs is read with its size (atlantis3-re
//!   `discs/cd-zh-cn.files.tsv`).

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::discs::{disc_candidates, CdSource};
use atlantis3_data::gsoo::GsooTable;
use atlantis3_data::install::{data_language, has_gsoo_texts, Language};
use atlantis3_data::testing::cd_zh_cn;
use atlantis3_data::{A3Files, Release};
use atlantis_data::{FileSource, StdDir};
use md5::Digest;

fn open(install: &Path, discs: &Path) -> A3Files {
    A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    )
}

#[test]
fn build_edition_and_discs() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let files = open(&install, &discs);
    let b = files.build().unwrap();
    assert_eq!(b.id, "atlantis3-cd-zh-cn-3682370");
    assert_eq!(
        (b.release, b.exe_name, b.cfg_name),
        (Release::CdPt2001, "Atlantis3.exe", "Atlantis3.cfg")
    );
    assert_eq!(
        files.edition_label().unwrap(),
        "Atlantis III, Chinese CD, Simplified Chinese (2002)"
    );
    let (language, version) = atlantis3_data::install::install_registry(&files);
    assert_eq!((&language[..], &version[..]), (&b"gbr"[..], &b"CD"[..]));
    assert!(has_gsoo_texts(&files));
    assert_eq!(
        data_language(&files, &language),
        Some(Language::SimplifiedChinese)
    );
    let d = files.discs().expect("the CD's discs");
    assert_eq!(d.disc_numbers(), [1, 2, 3]);
    let mut kinds: Vec<u8> = disc_candidates(&discs)
        .into_iter()
        .map(|(_, s)| atlantis3_data::discs::disc_number(&*s))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, [1, 2, 3]);
    let src = CdSource::open(Arc::new(StdDir::new(&install)), &install, &[discs]);
    assert_eq!(src.list("Datas/loc/text"), ["gbr"]);
    assert!(src
        .size(&format!("Datas/spr/{}", Release::CdPt2001.scarab_bank()))
        .is_some());
}

/// Every file of a source, from its own listings: `(path in lower case,
/// size)`.
fn walk(src: &dyn FileSource, dir: &str, out: &mut Vec<(String, u64)>) {
    for n in src.list(dir) {
        let p = if dir.is_empty() {
            n
        } else {
            format!("{dir}/{n}")
        };
        match src.size(&p) {
            Some(size) => out.push((p.to_ascii_lowercase(), size)),
            None => walk(src, &p, out),
        }
    }
}

/// Every file of the install and of the three discs, read with its size:
/// the inventory atlantis3-re `discs/cd-zh-cn.files.tsv` lists, with the
/// install = disc 1's `GAMEDATA\` (4720 rows `<install|cd1|cd2|cd3>\t<path
/// in lower case>\t<size>`, sorted, hashing to the MD5 below).
#[test]
fn every_file_of_the_install_and_the_discs() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let mut rows = Vec::new();
    let mut files = Vec::new();
    walk(&StdDir::new(&install), "", &mut files);
    rows.extend(
        files
            .into_iter()
            .map(|(p, s)| ("install".to_string(), p, s)),
    );
    for (_, src) in disc_candidates(&discs) {
        let n = atlantis3_data::discs::disc_number(&*src);
        let mut files = Vec::new();
        walk(&*src, "", &mut files);
        rows.extend(files.into_iter().map(|(p, s)| (format!("cd{n}"), p, s)));
    }
    rows.sort();
    let text: String = rows
        .iter()
        .map(|(w, p, s)| format!("{w}\t{p}\t{s}\n"))
        .collect();
    assert_eq!(rows.len(), 4720);
    assert_eq!(
        format!("{:x}", md5::Md5::digest(text.as_bytes())),
        "9ac0b07a7af69b633caa37364f87e687"
    );
}

/// The program, the hook's files and the table, by their MD5 (atlantis3-re
/// `discs/cd-zh-cn.md`); `font.txt` GOG.com English's.
#[test]
fn the_program_and_the_hook() {
    let Some((install, _)) = cd_zh_cn() else {
        return;
    };
    let dir = StdDir::new(&install);
    let md5 = |n: &str| format!("{:x}", md5::Md5::digest(dir.read(n).unwrap()));
    assert_eq!(md5("Atlantis3.exe"), "854f9bc6ac48af9cb9e778ca49f9d460");
    assert_eq!(md5("GSooAPI.res"), "12956b26eef51e90763923303e235a51");
    assert_eq!(md5("GSooKernel.dll"), "b5cde20c2b7ab7c23490c6f4ae72cbfe");
    assert_eq!(md5("GSooAPIRes.dll"), "ed4e64ff9209a95002ef9b74e40fdaf4");
    let ft = atlantis3_data::font_txt::FontTxt::parse(
        &dir.read(atlantis3_data::font_txt::FONT_TXT_PATH).unwrap(),
    );
    assert!(ft
        .fonts
        .iter()
        .filter(|f| !f.id.is_empty())
        .all(|f| f.charset("gbr") == 0x80));
}

/// The table as the hook reads it: 729 entries with both lines (No.449's
/// English is `?` and a bare LF: its line ends at the CR LF), the
/// Chinese GB2312 pairs and ASCII only; every `Dial.txt` text but one
/// (a stage direction) found by `SearchString16`, the 529 lines of 16 bytes or
/// more tagged with their entry's number; the 64 names of `Objects.txt`
/// found; no line of `i_credit.txt` an entry.
#[test]
fn the_table_and_the_texts() {
    let Some((install, discs)) = cd_zh_cn() else {
        return;
    };
    let files = open(&install, &discs);
    let t = GsooTable::load(files.source()).unwrap();
    let entries = t.entries();
    assert_eq!(entries.len(), 729);
    for e in &entries {
        let mut i = 0;
        let c = e.chinese;
        while i < c.len() {
            if c[i] >= 0x80 {
                assert!(
                    (0xa1..=0xf7).contains(&c[i]) && (0xa1..=0xfe).contains(&c[i + 1]),
                    "No.{} @ {i}",
                    e.number
                );
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    let dial = atlantis3_data::dialog::parse_dial(&files.read("loc/text/gbr/Dial.txt").unwrap());
    let texts: Vec<&[u8]> = dial.lines.iter().map(|l| l.text.as_slice()).collect();
    let (mut found, mut tagged, mut missed) = (0, 0, Vec::new());
    for s in &texts {
        match t.search(s, s.len()) {
            Some(_) => found += 1,
            None => missed.push(String::from_utf8_lossy(s).into_owned()),
        }
        if let Some(tag) = t.tag(s) {
            assert!(GsooTable::is_tag(&tag));
            let n: u32 = std::str::from_utf8(&tag[8..]).unwrap().parse().unwrap();
            assert!(entries.iter().any(|e| e.number == n));
            tagged += 1;
        }
    }
    assert_eq!((texts.len(), found), (618, 617), "{missed:?}");
    assert_eq!(missed.len(), 1);
    // The others are shorter than 16 bytes: drawn through the line match.
    assert_eq!(tagged, 529);
    let objects = atlantis3_data::objects::ObjectNames::parse(
        &files.read("loc/text/gbr/Objects.txt").unwrap(),
    )
    .unwrap();
    let names: Vec<&[u8]> = objects.entries.iter().map(|(_, v)| v.as_slice()).collect();
    let found = names
        .iter()
        .filter(|n| t.search(n, n.len()).is_some())
        .count();
    assert_eq!((names.len(), found), (64, 64));
    let credits = files.read("loc/text/gbr/i_credit.txt").unwrap();
    let mut credit_hits = Vec::new();
    for l in credits.split(|&c| c == b'\n') {
        let l = l.strip_suffix(b"\r").unwrap_or(l);
        if l.len() > 1 && t.search(l, l.len()).is_some() {
            credit_hits.push(String::from_utf8_lossy(l).into_owned());
        }
    }
    assert!(credit_hits.is_empty(), "{credit_hits:?}");
}
