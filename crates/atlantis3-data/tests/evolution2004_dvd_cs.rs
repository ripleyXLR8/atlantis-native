//! *Atlantis Evolution CZ*, the Czech magazine *Level* 153's DVD (CINEMAX,
//! 2007), installed (`ATLANTIS4_DVD_CS`: the folder its Inno Setup
//! installer fills, `Atlantis Evolution.exe` beside `cd0` .. `cd4`;
//! skipped when unset; atlantis4-re `discs/dvd-cs-level153.md`):
//!
//! - its program is the Italian collection's file
//!   (atlantis4-collection-it-3022848), the edition told by the install's
//!   Czech texts: atlantis4-dvd-cs-3022848, "Atlantis Evolution (2004),
//!   Level DVD (CINEMAX), Czech";
//! - the registry's two languages, `Language` = `gbr` (`loc\voix\`) and
//!   `LanguageST` = `tch` (`loc\text\`), are found from the data alone;
//! - its fonts, all `ANSI_CHARSET`, are Central European with `tch` in
//!   this build (and would be ANSI in the CD's), its texts Windows-1250;
//! - it has no `Datas\Atlantis4.cfg`, disc 2 is complete, and every data
//!   file is where the `FLA` rule puts it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis3_data::evolution2004::{
    check_disc2, edition_label, holds_evolution_2004, identify, identify_dir, install_languages,
    missing_disc2, Evolution2004Source, COLLECTION_BUILD, DVD_CS_BUILD, EXE_NAME, KNOWN_BUILDS,
    SPR_DLLS,
};
use atlantis3_data::font_txt::FontTxt;
use atlantis_data::{FileSource, StdDir};
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    atlantis3_data::testing::a4_dvd_cs()
}

/// Every file under `dir`, relative to `root`, with `/`.
fn files_under(root: &Path, dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.join(dir)];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// The program, its edition, its languages and its libraries.
#[test]
fn the_install_is_the_collections_program_in_czech() {
    let Some(dir) = install() else { return };
    assert!(holds_evolution_2004(&dir));
    let exe = std::fs::read(dir.join(EXE_NAME)).unwrap();
    // The file alone is the collection's program.
    assert_eq!(identify(&exe).unwrap().id, COLLECTION_BUILD);
    let build = identify_dir(&dir)
        .expect("the program")
        .expect("a known build");
    assert_eq!(build.id, DVD_CS_BUILD);
    assert_eq!((build.size, build.pe_timestamp), (3_022_848, 0x4165_4813));
    assert!(build.tch_east_europe);
    assert_eq!(
        edition_label(build),
        "Atlantis Evolution (2004), Level DVD (CINEMAX), Czech"
    );
    assert!(StdDir::new(&dir).size("HYPERMAX.url").is_some());
    for (name, size, stamp, md5) in SPR_DLLS {
        let d = std::fs::read(dir.join(name)).unwrap();
        let m: [u8; 16] = Md5::digest(&d).into();
        assert_eq!(
            (d.len() as u64, atlantis3_data::exe::pe_timestamp(&d), m),
            (size, Some(stamp), md5),
            "{name}"
        );
    }
    // The installer's `Language` = `gbr`, `LanguageST` = `tch`.
    let (language, language_st) = install_languages(&StdDir::new(&dir));
    assert_eq!(
        (&language[..], &language_st[..]),
        (&b"gbr"[..], &b"tch"[..])
    );
    // No `Datas\Atlantis4.cfg`: the program keeps its defaults.
    let src = Evolution2004Source::new(Arc::new(StdDir::new(&dir)));
    assert_eq!(src.size("Datas/Atlantis4.cfg"), None);
    assert_eq!(src.build().map(|b| b.id), Some(DVD_CS_BUILD));
    assert_eq!(src.install_languages(), (language, language_st));
}

/// The fonts are `ANSI_CHARSET`: `tch` makes them Central European in
/// this build only; the texts' letters are Windows-1250's.
#[test]
fn the_czech_texts_are_central_european() {
    let Some(dir) = install() else { return };
    let src = StdDir::new(&dir);
    let fonts = FontTxt::parse_2004(&src.read("cd0/datas/loc/text/tch/font.txt").unwrap());
    assert_eq!(fonts.fonts.len(), 5);
    let cd = &KNOWN_BUILDS[0];
    let cs = identify_dir(&dir).unwrap().unwrap();
    for f in &fonts.fonts {
        assert_eq!(f.charset_name, "ANSI_CHARSET", "{}", f.id);
        assert_eq!(f.charset_in("tch", cs.tch_east_europe), 0xee, "{}", f.id);
        assert_eq!(f.charset_in("tch", cd.tch_east_europe), 0, "{}", f.id);
    }
    // Windows-1250's `š` and `ž` (0x9a, 0x9e) occur, ISO 8859-2's (0xb9,
    // 0xbe) do not.
    let dial = src.read("cd0/datas/loc/text/tch/Dial.txt").unwrap();
    let count = |b: u8| dial.iter().filter(|&&c| c == b).count();
    assert!(count(0x9a) > 0 && count(0x9e) > 0);
    assert_eq!((count(0xb9), count(0xbe)), (0, 0));
}

/// Disc 2 is complete, and every data file of the install is read through
/// the source by its `Datas\` name with its disc current; the texts are
/// `tch`, the voices and films `gbr`.
#[test]
fn every_data_file_is_where_the_rule_puts_it() {
    let Some(dir) = install() else { return };
    assert!(missing_disc2(&StdDir::new(&dir)).is_empty());
    let build = identify_dir(&dir).unwrap().unwrap();
    assert_eq!(check_disc2(&StdDir::new(&dir), build), None);
    let src = Evolution2004Source::new(Arc::new(StdDir::new(&dir)));
    let mut n = 0;
    for disc in 0..=4u32 {
        let datas = format!("cd{disc}/datas");
        for rel in files_under(&dir, &datas) {
            if disc > 0 {
                src.set_disc(disc);
            }
            let name = format!("Datas/{}", &rel[datas.len() + 1..]);
            assert_eq!(
                src.size(&name),
                Some(std::fs::metadata(dir.join(&rel)).unwrap().len()),
                "{name} (disc {disc})"
            );
            n += 1;
        }
    }
    // 7628 files, less the program, its four libraries, `Readme.rtf`,
    // `atl3.ico`, `AdventureCompany.url` and `HYPERMAX.url`.
    assert_eq!(n, 7628 - 9, "data files");
    let local = |p: &str| StdDir::new(&dir).list(p);
    assert_eq!(local("cd0/datas/loc/text"), ["tch"]);
    assert_eq!(local("cd0/datas/loc/voix"), ["gbr"]);
    for disc in 1..=4 {
        assert_eq!(local(&format!("cd{disc}/datas/loc/bink")), ["gbr"]);
    }
}
