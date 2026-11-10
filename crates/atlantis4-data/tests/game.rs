//! Every file of the installed game read by this crate, with the counts the
//! original's readers give.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder.

use std::path::{Path, PathBuf};

use atlantis4_data::A4Files;

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// Every file under `dir`, as paths relative to `root` with `/`, sorted.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.push((rel.replace('\\', "/"), e.metadata().unwrap().len()));
        }
    }
}

#[test]
fn the_executable_is_known() {
    let Some(dir) = install() else { return };
    let (files, build) = A4Files::open_dir(&dir).unwrap();
    assert_eq!(build.id, "atlantis4-gog-en-2363910");
    assert!(atlantis4_data::files::holds_atlantis4(&dir));
    assert!(files.lang_ini().is_some());
}

#[test]
fn the_install_holds_every_file_of_the_inventory() {
    let Some(dir) = install() else { return };
    let mut all = Vec::new();
    walk(&dir, &dir, &mut all);
    let bytes: u64 = all.iter().map(|f| f.1).sum();
    assert_eq!((all.len(), bytes), (9_858, 1_932_367_911));
    let mut by_ext = std::collections::BTreeMap::<String, usize>::new();
    for (name, _) in &all {
        let base = name.rsplit('/').next().unwrap();
        let ext = base
            .rsplit_once('.')
            .map_or("", |x| x.1)
            .to_ascii_lowercase();
        *by_ext.entry(ext).or_default() += 1;
    }
    let want = [
        ("dll", 5),
        ("exe", 1),
        ("font", 10),
        ("hashdb", 1),
        ("info", 1),
        ("ini", 1),
        ("jpg", 102),
        ("lua", 504),
        ("ogg", 5_303),
        ("ogv", 446),
        ("png", 433),
        ("script", 1),
        ("tef", 6),
        ("tga", 2),
        ("txt", 2),
        ("wav", 11),
        ("wpa", 2_613),
        ("wpp", 1),
        ("xml", 415),
    ];
    let want: std::collections::BTreeMap<String, usize> =
        want.iter().map(|(k, n)| (k.to_string(), *n)).collect();
    assert_eq!(by_ext, want);
}
