//! The Portuguese CD (*Atlantis III: O Novo Mundo*, Playgames, 2002,
//! three CDs) on the real files (`ATLANTIS3_CD_PT`: the three CloneCD
//! images under `Atlantis III - O Novo Mundo\CD1..CD3\` and `installed\`,
//! the first disc's `data1.cab` extracted; optional, the tests write
//! `SKIPPED` without it):
//!
//! - the program `Atlantis3.exe` is the known build
//!   atlantis3-cd-pt-3424256 ([`Release::CdPt2001`]), the edition
//!   "Atlantis III, Portuguese CD, Portuguese (Playgames, 2002)", the registry values
//!   `por` and `CD`;
//! - the three discs are found from the `.img` images;
//! - every file of the install and of the discs is read with its size
//!   (atlantis3-re `discs/cd-pt.files.tsv`);
//! - the disc test chooses the copy of the five Hoggar place animations
//!   that differ on disc 2, as on the French CD.

use std::path::Path;
use std::sync::Arc;

use atlantis3_data::discs::{disc_candidates, CdSource};
use atlantis3_data::testing::cd_pt;
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
    let Some((install, discs)) = cd_pt() else {
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
        "Atlantis III, Portuguese CD, Portuguese (Playgames, 2002)"
    );
    let (language, version) = atlantis3_data::install::install_registry(&files);
    assert_eq!((&language[..], &version[..]), (&b"por"[..], &b"CD"[..]));
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
/// the inventory atlantis3-re `discs/cd-pt.files.tsv` lists (4118 files,
/// the lines `<install|cd1|cd2|cd3>\t<path in lower case>\t<size>` in
/// order hash to the MD5 below).
#[test]
fn every_file_of_the_install_and_the_discs() {
    let Some((install, discs)) = cd_pt() else {
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
    assert_eq!(rows.len(), 4118);
    assert_eq!(
        format!("{:x}", md5::Md5::digest(text.as_bytes())),
        "2cd299e10816f57ccab4dde0e76ed63d"
    );
}

/// The disc test's choice, as on the French CD: disc 2's copy of the
/// Hoggar's view-23 place animations after `ChangeCD=2`, disc 3's
/// (GOG.com's) after `ChangeCD=4`; the install's `cine010.hnm`, which
/// disc 2 holds too.
#[test]
fn the_disc_test_chooses_the_copy() {
    let Some((install, discs)) = cd_pt() else {
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
    // The texts are in `por`, its only language.
    assert_eq!(src.list("Datas/loc/text"), ["por"]);
}
