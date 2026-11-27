//! The Traditional Chinese CD (*亞特蘭提斯Ⅲ 失落的帝國*, Interwise, 2002,
//! three CDs) on the real files (`ATLANTIS3_CD_ZH_TW`: the three ISO
//! images and `installed\`, the first disc's `data1.cab` extracted;
//! optional, the tests write `SKIPPED` without it):
//!
//! - the program `Atlantis3.exe` is the Portuguese CD's build
//!   atlantis3-cd-pt-3424256 ([`Release::CdPt2001`]); the registry values
//!   `gbr` and `CD`; the edition "Atlantis III, Taiwanese CD, Traditional
//!   Chinese (Interwise, 2002)", told by its data: `font\font.txt` puts
//!   all five fonts in `CHINESEBIG5_CHARSET`;
//! - every text of `loc\text\gbr\` reads as code page 950 (but
//!   `i_creditold.txt`, which nothing reads), `i_credit.txt` is ASCII;
//! - the three discs are found from the `.iso` images;
//! - every file of the install and of the discs is read with its size
//!   (atlantis3-re `discs/cd-zh-tw.files.tsv`);
//! - the disc test chooses the copy of the five Hoggar place animations
//!   that differ on disc 2, as on the French CD.

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::discs::{disc_candidates, CdSource};
use atlantis3_data::testing::cd_zh_tw;
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
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let files = open(&install, &discs);
    let b = files.build().unwrap();
    assert_eq!(b.id, "atlantis3-cd-pt-3424256");
    assert_eq!(
        (b.release, b.exe_name, b.cfg_name),
        (Release::CdPt2001, "Atlantis3.exe", "Atlantis3.cfg")
    );
    assert_eq!(
        files.edition_label().unwrap(),
        "Atlantis III, Taiwanese CD, Traditional Chinese (Interwise, 2002)"
    );
    let (language, version) = atlantis3_data::install::install_registry(&files);
    assert_eq!((&language[..], &version[..]), (&b"gbr"[..], &b"CD"[..]));
    assert_eq!(
        atlantis3_data::install::data_language(&files, &language),
        Some(atlantis3_data::install::Language::TraditionalChinese)
    );
    let d = files.discs().expect("the CD's discs");
    assert_eq!(d.disc_numbers(), [1, 2, 3]);
    assert_eq!(d.current(), 0);
    let mut kinds: Vec<u8> = disc_candidates(&discs)
        .into_iter()
        .map(|(_, s)| atlantis3_data::discs::disc_number(&*s))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, [1, 2, 3]);
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
/// the inventory atlantis3-re `discs/cd-zh-tw.files.tsv` lists (4121 files,
/// the lines `<install|cd1|cd2|cd3>\t<path in lower case>\t<size>` in
/// order hash to the MD5 below).
#[test]
fn every_file_of_the_install_and_the_discs() {
    let Some((install, discs)) = cd_zh_tw() else {
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
    assert_eq!(rows.len(), 4121);
    assert_eq!(
        format!("{:x}", md5::Md5::digest(text.as_bytes())),
        "1890dba5d4a2abf718077b95e8f4e003"
    );
}

/// The disc test's choice, as on the French CD: disc 2's copy of the
/// Hoggar's view-23 place animations after `ChangeCD=2`, disc 3's
/// (GOG.com's) after `ChangeCD=4`; the install's `cine010.hnm`, which
/// disc 2 holds too.
#[test]
fn the_disc_test_chooses_the_copy() {
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let src = CdSource::open(Arc::new(StdDir::new(&install)), &install, &[discs]);
    let spw = "Datas/spw/SPW512/hoggar/cta1021023.SPW";
    assert_eq!(src.change_cd(2), 2);
    assert_eq!(src.size(spw), Some(48_610));
    assert_eq!(src.change_cd(7), 2);
    assert_eq!(src.change_cd(4), 3);
    assert_eq!(src.size(spw), Some(48_602));
    for mask in [2, 4] {
        src.change_cd(mask);
        assert_eq!(
            src.size("Datas/hnm/HNM640/ldm/cine010.hnm"),
            Some(1_836_380)
        );
    }
    assert!(src.list("Datas/warp/WARP512/paleo").len() > 10);
    assert!(src
        .size(&format!("Datas/spr/{}", Release::CdPt2001.scarab_bank()))
        .is_some());
    // The texts are in `gbr`, its only language.
    assert_eq!(src.list("Datas/loc/text"), ["gbr"]);
}

/// `font.txt`: the five fonts in `CHINESEBIG5_CHARSET` (0x88), heights
/// 21, 19, 24, 60, 18; every text of `gbr` but `i_creditold.txt` valid
/// code page 950 without a user-defined pair, `i_credit.txt` ASCII.
#[test]
fn big5_fonts_and_texts() {
    let Some((install, discs)) = cd_zh_tw() else {
        return;
    };
    let files = open(&install, &discs);
    let ft = atlantis3_data::font_txt::FontTxt::parse(
        &files
            .source()
            .read(atlantis3_data::font_txt::FONT_TXT_PATH)
            .unwrap(),
    );
    let got: Vec<(String, i32, u8)> = ft
        .fonts
        .iter()
        .filter(|f| !f.id.is_empty())
        .map(|f| (f.id.clone(), f.height, f.charset("gbr")))
        .collect();
    assert_eq!(
        got,
        [
            ("AtlFont_Default".to_string(), 21, 0x88),
            ("AtlFont_Bulle".to_string(), 19, 0x88),
            ("AtlFont_Special".to_string(), 24, 0x88),
            ("AtlFont_Titre".to_string(), 60, 0x88),
            ("AtlFont_Credit".to_string(), 18, 0x88),
        ]
    );
    let names = files.list("loc/text/gbr");
    assert!(names.len() > 40, "{names:?}");
    for n in &names {
        let d = files.read(&format!("loc/text/gbr/{n}")).unwrap();
        let lower = n.to_ascii_lowercase();
        if lower == "i_credit.txt" {
            assert!(d.is_ascii(), "{n}");
        }
        if lower == "i_creditold.txt" {
            continue;
        }
        let mut i = 0;
        while i < d.len() {
            let b = d[i];
            if (0x81..=0xfe).contains(&b) {
                let t = *d.get(i + 1).unwrap_or(&0);
                assert!(
                    matches!(b, 0xa1..=0xf9) && matches!(t, 0x40..=0x7e | 0xa1..=0xfe),
                    "{n} @ {i}: {b:02x} {t:02x}"
                );
                assert!(
                    !matches!((b, t), (0xc6, 0xa1..) | (0xc7..=0xc8, _)),
                    "{n} @ {i}"
                );
                i += 2;
            } else {
                assert!(b < 0x80, "{n} @ {i}: {b:02x}");
                i += 1;
            }
        }
    }
}
