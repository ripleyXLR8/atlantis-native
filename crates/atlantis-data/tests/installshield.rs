//! The InstallShield reader on *The Secrets of Atlantis*'s North-American
//! CDs (`ATLANTIS5_CD_US`: a folder holding a copy of each disc's files in
//! `CD1`, `CD2` and `CD3`; skipped when unset): the set's table, its
//! groups, its volumes across the discs, and files read back against the
//! MD5 their descriptors give.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis_data::installshield::{InstallShield, IsFiles, Layered, FILE_SPLIT, LINK_PREV};
use atlantis_data::{FileSource, StdDir};
use md5::{Digest, Md5};

fn discs() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_CD_US") {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            eprintln!("SKIPPED: ATLANTIS5_CD_US is not set");
            None
        }
    }
}

/// The three discs' folders as one.
fn set(d: &std::path::Path) -> Arc<InstallShield> {
    let parts: Vec<(Arc<dyn FileSource>, String)> = ["CD1", "CD2", "CD3"]
        .iter()
        .map(|c| {
            (
                Arc::new(StdDir::new(d.join(c))) as Arc<dyn FileSource>,
                String::new(),
            )
        })
        .collect();
    Arc::new(InstallShield::open(Arc::new(Layered::new(parts))).expect("the set"))
}

fn md5(data: &[u8]) -> [u8; 16] {
    Md5::digest(data).into()
}

#[test]
fn the_table_groups_and_volumes() {
    let Some(d) = discs() else { return };
    let cab = set(&d);
    assert_eq!(cab.major, 9);
    assert_eq!(cab.files().len(), 5177);
    let groups: Vec<(&str, u32, u32)> = cab
        .groups()
        .iter()
        .filter(|g| g.installs())
        .map(|g| (g.name.as_str(), g.first, g.last))
        .collect();
    assert_eq!(groups, [("Files", 5176, 5176), ("Files1", 33, 5175)]);
    assert_eq!(cab.installed_files().len(), 5137);
    // Volume 1 holds the setup's own files only.
    assert_eq!(cab.volumes_needed(), [2, 3, 4]);
    assert!(cab.missing_volumes().is_empty());
    // Two files run on into the next disc's volume.
    let split: Vec<(String, u16)> = cab
        .files()
        .iter()
        .filter(|f| f.flags & FILE_SPLIT != 0)
        .map(|f| (f.name.clone(), f.volume))
        .collect();
    assert_eq!(
        split,
        [
            (
                "(zeppelin)_(022)_(engine_manometer)_(z3).spw".to_string(),
                2
            ),
            ("ec0010.jp6".to_string(), 3)
        ]
    );
    let (_, _, h3) = cab.volume(3).expect("data3.cab");
    assert_eq!((h3.first_file, h3.last_file), (3829, 4085));
}

#[test]
fn files_read_back_with_their_md5() {
    let Some(d) = discs() else { return };
    let cab = set(&d);
    let files = IsFiles::new(cab.clone());
    assert_eq!(
        files.list(""),
        [
            "ReadMe - The Secrets of Atlantis.txt",
            "bass.dll",
            "binkw32.dll",
            "fmod.dll",
            "lua50.dll",
            "spr_p5.dll",
            "spr_p6.dll",
            "datas"
        ]
    );
    for name in [
        // Split from data2.cab (disc 1) into data3.cab (disc 2), and from
        // data3.cab into data4.cab (disc 3).
        "datas/sprites3d/(zeppelin)_(022)_(engine_manometer)_(z3).spw",
        "datas/warps/ec0010.jp6",
        // Links: the data of an earlier file, in another volume than the
        // descriptor's own.
        "datas/Sprites2D/Dialog/Subjects/proxy_subject.SPR",
        "datas/Sprites2D/Menu/Inventory/old/button_left.spr",
        "datas/Videos/Logo_TAC.bik",
        "datas/Sounds/Video/english/cine01.ogg",
        "datas/Texts/Menu/english/string_resources.xml",
        "datas/Scripts/utility scripts/zone.lua",
        "lua50.dll",
    ] {
        let i = files.file_index(name).expect(name);
        let f = &cab.files()[i];
        let data = files.read(name).expect(name);
        assert_eq!(data.len() as u64, f.size, "{name}");
        assert_eq!(md5(&data), f.md5, "{name}");
    }
    let link = files
        .file_index("datas/Sprites2D/Dialog/Subjects/proxy_subject.SPR")
        .unwrap();
    assert_eq!(cab.files()[link].link_flags & LINK_PREV, LINK_PREV);
    // A part deep in a film, then one before it.
    let film = "datas/videos/cine01.bik";
    let whole = files.read(film).expect(film);
    assert_eq!(whole.len(), 68_229_512);
    assert_eq!(
        md5(&whole),
        cab.files()[files.file_index(film).unwrap()].md5
    );
    assert_eq!(
        files.read_at(film, 50_000_000, 100_000).unwrap(),
        whole[50_000_000..50_100_000]
    );
    assert_eq!(files.read_at(film, 1000, 10).unwrap(), whole[1000..1010]);
    assert_eq!(files.read_at(film, 68_229_511, 2), None);
}

/// Every file the installation copies, read whole and checked against
/// its descriptor's MD5 (about 2 GB decoded; run with `--ignored`). The
/// damaged files are listed.
#[test]
#[ignore = "reads the whole set"]
fn every_installed_file_matches_its_md5() {
    let Some(d) = discs() else { return };
    let cab = set(&d);
    let mut bad = Vec::new();
    for i in cab.installed_files() {
        let f = &cab.files()[i];
        let ok = cab.read_file(i).is_some_and(|data| md5(&data) == f.md5);
        if !ok {
            bad.push(format!(
                "{} (volume {})",
                f.path(),
                cab.files()[cab.data_of(i)].volume
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} files do not match:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
