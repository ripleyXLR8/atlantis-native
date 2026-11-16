//! The French retail CD (three CDs, 2001) on the real files: its
//! installed folder (`ATLANTIS3_CD_FR`, `Atlantis3.exe` and the 890 files
//! the installer copies) and its discs (`ATLANTIS3_DISCS_FR`: the three CD
//! images, or copies of the CDs).
//!
//! - the program is the known CD build, the edition "Atlantis III, CD,
//!   French";
//! - the three discs are found from the images and from copies;
//! - every file of GOG.com's French `Datas\` (`ATLANTIS3_DATA`) is read
//!   from the install and the discs with its size (atlantis3-re
//!   `discs/cd-fr.md`: the same 3317 files);
//! - the disc test chooses the copy of the five Hoggar place animations
//!   that differ on disc 2; the install's `cine010.hnm` wins over disc
//!   2's.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis3_data::discs::{disc_candidates, CdSource};
use atlantis3_data::testing::{cd_fr, install_dir};
use atlantis3_data::{A3Files, Release};
use atlantis_data::{FileSource, StdDir};

fn open(install: &Path, discs: &Path) -> A3Files {
    A3Files::with_discs(
        Arc::new(StdDir::new(install)),
        install,
        &[discs.to_path_buf()],
    )
}

#[test]
fn build_edition_and_discs() {
    let Some((install, discs)) = cd_fr() else {
        return;
    };
    let files = open(&install, &discs);
    let b = files.build().unwrap();
    assert_eq!(b.id, "atlantis3-cd-fr-3379200");
    assert_eq!((b.release, b.exe_name), (Release::Cd2001, "Atlantis3.exe"));
    assert_eq!(files.edition_label().unwrap(), "Atlantis III, CD, French");
    let (language, version) = atlantis3_data::install::install_registry(&files);
    assert_eq!((&language[..], &version[..]), (&b"fra"[..], &b"CD"[..]));
    let d = files.discs().expect("the CD's discs");
    assert_eq!(d.disc_numbers(), [1, 2, 3]);
    assert_eq!(d.current(), 0);
    // Without discs: the install alone, no disc.
    let alone = A3Files::with_discs(Arc::new(StdDir::new(&install)), &install, &[]);
    if disc_candidates(&install).is_empty() {
        assert_eq!(alone.discs().map(|d| d.disc_numbers().len()), Some(0));
    }
}

/// The images and the copies give the same discs (the copies when
/// `ATLANTIS3_DISCS_FR`'s parent holds `cd1` .. `cd3`, as the extracted
/// layout of atlantis3-re `discs/cd-fr.md` does).
#[test]
fn images_and_copies() {
    let Some((_, discs)) = cd_fr() else {
        return;
    };
    let mut kinds: Vec<(String, u8)> = disc_candidates(&discs)
        .into_iter()
        .map(|(o, s)| (o, atlantis3_data::discs::disc_number(&*s)))
        .collect();
    kinds.sort_by_key(|k| k.1);
    assert_eq!(kinds.iter().map(|k| k.1).collect::<Vec<_>>(), [1, 2, 3]);
    let copies: Vec<PathBuf> = (1..=3)
        .map(|n| discs.parent().unwrap().join(format!("cd{n}")))
        .filter(|p| p.is_dir())
        .collect();
    for c in copies {
        let n = atlantis3_data::discs::disc_number(&StdDir::new(&c));
        assert!((1..=3).contains(&n), "{}", c.display());
    }
}

/// Every file of GOG.com's French `Datas\` is there, with its size.
#[test]
fn every_gog_file_from_the_install_and_the_discs() {
    let Some((install, discs)) = cd_fr() else {
        return;
    };
    let Some(gog) = install_dir() else {
        return;
    };
    let files = open(&install, &discs);
    let src = files.source();
    let mut n = 0;
    let mut stack = vec![gog.join("Datas")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(&gog)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let size = e.metadata().unwrap().len();
            assert_eq!(src.size(&rel), Some(size), "{rel}");
            n += 1;
        }
    }
    assert_eq!(n, 3317);
}

/// The disc test's choice: disc 2's copy of the Hoggar's view-23 place
/// animations after `ChangeCD=2`, disc 3's (GOG.com's) after
/// `ChangeCD=4`; `ChangeCD=7` keeps the disc; the install's
/// `cine010.hnm` whatever the disc.
#[test]
fn the_disc_test_chooses_the_copy() {
    let Some((install, discs)) = cd_fr() else {
        return;
    };
    let src = CdSource::open(Arc::new(StdDir::new(&install)), &install, &[discs]);
    let spw = "Datas/spw/SPW512/hoggar/cta1021023.SPW";
    assert_eq!(src.change_cd(2), 2);
    assert_eq!(src.size(spw), Some(48_610));
    assert_eq!(src.change_cd(7), 2);
    assert_eq!(src.size(spw), Some(48_610));
    assert_eq!(src.change_cd(4), 3);
    assert_eq!(src.size(spw), Some(48_602));
    if let Some(gog) = install_dir() {
        let g = StdDir::new(&gog);
        assert_eq!(src.read(spw), g.read(spw));
    }
    for mask in [2, 4] {
        src.change_cd(mask);
        assert_eq!(
            src.size("Datas/hnm/HNM640/ldm/cine010.hnm"),
            Some(1_836_380)
        );
    }
    // The Palaeolithic (disc 1 only) from disc 3, as the port allows.
    assert!(src.has_dir("Datas/warp/WARP512/paleo"));
    assert!(src.list("Datas/warp/WARP512/paleo").len() > 10);
    // The scarab case's bank, in the install under the CD program's name.
    assert!(src
        .size(&format!("Datas/spr/{}", Release::Cd2001.scarab_bank()))
        .is_some());
}
