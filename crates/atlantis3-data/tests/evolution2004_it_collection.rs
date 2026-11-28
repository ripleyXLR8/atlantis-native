//! *Atlantis Evolution*'s Italian *Atlantis Collection* DVD (2014), installed
//! (`ATLANTIS4_IT_COLLECTION`: the folder its Inno Setup installer fills,
//! `Atlantis Evolution.exe` beside `cd0` .. `cd4`; skipped when unset):
//!
//! - its program is the 2004 program rebuilt on 2004-10-07
//!   (atlantis4-collection-it-3022848), named "Atlantis Evolution (2004),
//!   Atlantis Collection, Italian";
//! - the registry's `Language` (`ita`) is found from the data alone
//!   (`loc\text\ita\`), and every data file is where the `FLA` rule puts it;
//! - disc 2's world data are missing (only `cd2\datas\loc\bink\ita`), and
//!   with the English CD's installation (`ATLANTIS4_CD`) beside it, its
//!   `cd2` fills them: none of its files is in the collection's install,
//!   and the other discs' files outside `loc` are the collection's but for
//!   seven (atlantis4-re `discs/it-collection.md`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis3_data::evolution2004::{
    check_disc2, edition_label, holds_evolution_2004, identify_dir, missing_disc2,
    Evolution2004Source, COLLECTION_BUILD, DISC2_FOLDERS, SPR_DLLS,
};
use atlantis3_data::install::{install_registry, Language};
use atlantis3_data::A3Files;
use atlantis_data::{FileSource, StdDir};
use md5::{Digest, Md5};

fn install() -> Option<PathBuf> {
    atlantis3_data::testing::a4_it_collection()
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

/// The program, its edition, its language and its libraries.
#[test]
fn the_install_is_the_collections_program_in_italian() {
    let Some(dir) = install() else { return };
    assert!(holds_evolution_2004(&dir));
    let build = identify_dir(&dir)
        .expect("the program")
        .expect("a known build");
    assert_eq!(build.id, COLLECTION_BUILD);
    assert_eq!((build.size, build.pe_timestamp), (3_022_848, 0x4165_4813));
    assert_eq!(
        edition_label(build),
        "Atlantis Evolution (2004), Atlantis Collection, Italian"
    );
    // The CD's sprite libraries.
    for (name, size, stamp, md5) in SPR_DLLS {
        let d = std::fs::read(dir.join(name)).unwrap();
        let m: [u8; 16] = Md5::digest(&d).into();
        assert_eq!(
            (d.len() as u64, atlantis3_data::exe::pe_timestamp(&d), m),
            (size, Some(stamp), md5),
            "{name}"
        );
    }
    // The installer's `Language` = `ita`, from `loc\text\` alone.
    let src = Arc::new(Evolution2004Source::new(Arc::new(StdDir::new(&dir))));
    let files = A3Files::evolution_2004(src);
    let (language, _) = install_registry(&files);
    assert_eq!(language, b"ita");
    assert_eq!(Language::from_code(&language), Some(Language::Italian));
}

/// Every data file of the install, read through the source by its
/// `Datas\` name with its disc current; the localised folders are `ita`.
#[test]
fn every_data_file_is_where_the_rule_puts_it() {
    let Some(dir) = install() else { return };
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
    // 6490 files, less the program, its four libraries, `Microids.url`,
    // `atl3.ico` and `datas\atlantis4.cfg`.
    assert_eq!(n, 6490 - 8, "data files");
    assert!(dir.join("cd0/datas/loc/text/ita").is_dir());
    assert!(dir.join("cd0/datas/loc/voix/ita").is_dir());
    for disc in 1..=4 {
        assert!(dir.join(format!("cd{disc}/datas/loc/bink/ita")).is_dir());
    }
    assert!(!dir.join("cd0/datas/loc/text/gbr").exists());
}

/// Disc 2 holds the Italian films only: the eight folders of its world
/// data are missing, and the install is refused with what to do.
#[test]
fn disc2_world_data_are_missing() {
    let Some(dir) = install() else { return };
    let cd2 = files_under(&dir, "cd2");
    assert_eq!(cd2.len(), 22);
    assert!(cd2.iter().all(|p| p.starts_with("cd2/datas/loc/bink/ita/")));
    let src = StdDir::new(&dir);
    assert_eq!(missing_disc2(&src), DISC2_FOLDERS);
    let build = identify_dir(&dir).unwrap().unwrap();
    let e = check_disc2(&src, build).expect("refused");
    assert!(e.collection);
    assert!(e.to_string().contains("English 2004 CD"));
}

/// The English CD's installation's `cd2` over the collection's: what it
/// adds is exactly what is missing (1142 files in the eight folders, none
/// already in the collection's install; its films' names are the
/// collection's Italian films' names), and the install then opens.
#[test]
fn the_english_cds_disc2_fills_the_gap() {
    let Some(dir) = install() else { return };
    let Some(cd) = std::env::var_os("ATLANTIS4_CD").map(PathBuf::from) else {
        eprintln!("SKIPPED the_english_cds_disc2_fills_the_gap: ATLANTIS4_CD not set");
        return;
    };
    let ours: Vec<String> = files_under(&dir, "cd2")
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let theirs = files_under(&cd, "cd2");
    let (local, world): (Vec<&String>, Vec<&String>) = theirs
        .iter()
        .partition(|p| p.to_lowercase().starts_with("cd2/datas/loc/"));
    assert_eq!(world.len(), 1142);
    assert!(world.iter().all(|p| !ours.contains(&p.to_lowercase())));
    for p in &world {
        let folder = p.split('/').nth(2).unwrap().to_lowercase();
        assert!(DISC2_FOLDERS.contains(&folder.as_str()), "{p}");
    }
    let name = |p: &str| p.rsplit('/').next().unwrap().to_lowercase();
    let mut english: Vec<String> = local.iter().map(|p| name(p)).collect();
    let mut italian: Vec<String> = ours.iter().map(|p| name(p)).collect();
    english.sort();
    italian.sort();
    assert_eq!(english, italian);
    // The other discs: every file outside `loc` is in both, the same size
    // but for these seven (atlantis4-re `discs/it-collection.md`).
    let mut differ = Vec::new();
    for disc in ["cd0", "cd1", "cd3", "cd4"] {
        for p in files_under(&cd, disc) {
            if p.to_lowercase().contains("/loc/") {
                continue;
            }
            let ours = std::fs::metadata(dir.join(&p)).unwrap_or_else(|_| panic!("{p}"));
            if ours.len() != std::fs::metadata(cd.join(&p)).unwrap().len() {
                differ.push(p.to_lowercase());
            }
        }
    }
    differ.sort();
    assert_eq!(
        differ,
        [
            "cd0/datas/menu/desc/i_langue_full.atl",
            "cd0/datas/menu/spr_1024/32bits/lan_rus_.spr",
            "cd0/datas/menu/spr_1024/32bits/lan_tch_.spr",
            "cd0/datas/menu/spr_1024/32bits/st_rus_.spr",
            "cd0/datas/menu/spr_1024/32bits/st_tch_.spr",
            "cd0/datas/scenar/chapi014.con",
            "cd3/datas/bink/chel2bloc2.txt",
        ]
    );
    // The two installs seen as one: the English `cd2` world data under the
    // collection's; nothing is missing any more.
    let merged = Merged {
        it: StdDir::new(&dir),
        cd: StdDir::new(&cd),
    };
    assert!(missing_disc2(&merged).is_empty());
    let build = identify_dir(&dir).unwrap().unwrap();
    assert_eq!(check_disc2(&merged, build), None);
}

/// The collection's install with the English CD's `cd2\datas\<world
/// folder>` files, as a player who merged the two `cd2` folders has it.
struct Merged {
    it: StdDir,
    cd: StdDir,
}

impl Merged {
    fn from_cd(name: &str) -> bool {
        let n = name.replace('\\', "/").to_lowercase();
        DISC2_FOLDERS
            .iter()
            .any(|f| n == format!("cd2/datas/{f}") || n.starts_with(&format!("cd2/datas/{f}/")))
    }

    fn pick(&self, name: &str) -> &StdDir {
        if Self::from_cd(name) {
            &self.cd
        } else {
            &self.it
        }
    }
}

impl FileSource for Merged {
    fn size(&self, name: &str) -> Option<u64> {
        self.pick(name).size(name)
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.pick(name).read_at(name, offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.pick(name).has_dir(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        self.pick(dir).list(dir)
    }
}
