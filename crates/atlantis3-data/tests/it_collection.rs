//! The Italian *Atlantis Collection* DVD (2014) on the real files: the
//! folder its Inno Setup installer lays out (`ATLANTIS3_IT_COLLECTION`:
//! `Game.exe`, the collection's launcher `Atlantis3.exe`, `Datas\`).
//!
//! - `Game.exe` is GOG.com's 2015 / 2018 file (atlantis3-gog-2018-3473408):
//!   the 2003 program, its reader opening `Atlantis4.cfg`, the thumbnails'
//!   original row scan;
//! - the edition is named after the collection's launcher; the same
//!   `Game.exe` without that launcher (an older GOG.com install) keeps
//!   GOG.com's name;
//! - the registry values the installer writes (`Language` = `ita`,
//!   `Install` = `CD`) are found from the data alone (`loc\text\ita\`);
//! - the Italian texts, films and voices are where the game reads them.

use std::sync::Arc;

use atlantis3_data::install::{install_registry, Language};
use atlantis3_data::spr_encode::RowScan;
use atlantis3_data::testing::it_collection;
use atlantis3_data::{A3Files, Release};
use atlantis_data::{FileSource, StdDir};

/// The install without its launcher `Atlantis3.exe`.
struct NoLauncher(StdDir);

impl NoLauncher {
    fn hidden(name: &str) -> bool {
        name.eq_ignore_ascii_case("Atlantis3.exe")
    }
}

impl FileSource for NoLauncher {
    fn size(&self, name: &str) -> Option<u64> {
        if Self::hidden(name) {
            return None;
        }
        self.0.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if Self::hidden(name) {
            return None;
        }
        self.0.read_at(name, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.0.has_dir(name)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.0
            .list(dir)
            .into_iter()
            .filter(|n| !Self::hidden(n))
            .collect()
    }
}

#[test]
fn build_edition_and_registry() {
    let Some(dir) = it_collection() else {
        return;
    };
    let (files, b) = A3Files::open_dir(&dir).unwrap();
    assert_eq!(b.id, "atlantis3-gog-2018-3473408");
    assert_eq!(
        (b.release, b.exe_name, b.cfg_name, b.thumb_rows),
        (
            Release::Gog2003,
            "Game.exe",
            "Atlantis4.cfg",
            RowScan::Original
        )
    );
    assert!(files.is_collection());
    assert_eq!(
        files.edition_label().unwrap(),
        "Atlantis III, Atlantis Collection, Italian"
    );
    let (language, version) = install_registry(&files);
    assert_eq!((&language[..], &version[..]), (&b"ita"[..], &b"CD"[..]));
    assert_eq!(Language::from_code(&language), Some(Language::Italian));
    assert!(files.discs().is_none());
    // The reader's file is not in the install; the writer's is.
    assert!(files.read("Atlantis4.cfg").is_none());
    assert!(files.read("Atlantis3.cfg").is_some());

    let alone = A3Files::new(Arc::new(NoLauncher(StdDir::new(&dir))));
    assert!(!alone.is_collection());
    assert_eq!(
        alone.edition_label().unwrap(),
        "Atlantis III, GOG.com (2015 / 2018 build), Italian"
    );
}

/// `loc\text\ita\` (13 files), `loc\hnm\hnm640\ita\` (12 films),
/// `loc\voix\ita\` (606 voices, in their folders), `hnm\intro_ita.hnm`;
/// the dialog texts parse.
#[test]
fn italian_data() {
    let Some(dir) = it_collection() else {
        return;
    };
    let (files, _) = A3Files::open_dir(&dir).unwrap();
    assert_eq!(files.list("loc/text"), ["ita"]);
    // The files of a folder of `Datas\` and of the folders inside it.
    fn count(files: &A3Files, dir: &str) -> usize {
        files
            .list(dir)
            .iter()
            .map(|n| {
                let p = format!("{dir}/{n}");
                if files.size(&p).is_some() {
                    1
                } else {
                    count(files, &p)
                }
            })
            .sum()
    }
    assert_eq!(count(&files, "loc/text/ita"), 13);
    assert_eq!(count(&files, "loc/hnm/hnm640/ita"), 12);
    assert_eq!(count(&files, "loc/voix/ita"), 606);
    assert_eq!(files.size("hnm/intro_ita.hnm"), Some(3_848_264));
    let dial = files
        .read("loc/text/ita/Dial.txt")
        .expect("loc\\text\\ita\\Dial.txt");
    let parsed = atlantis3_data::dialog::parse_dial(&dial);
    assert_eq!(parsed.lines.len(), 619);
    // Windows-1252 accents (à è é ì ò ù), as the fonts' ANSI_CHARSET draws.
    assert!(dial
        .iter()
        .any(|&c| matches!(c, 0xe0 | 0xe8 | 0xe9 | 0xec | 0xf2 | 0xf9)));
    // The scarab case's bank under the French and Spanish name only.
    assert!(files.size("spr/Scarabee.spr").is_none());
    assert!(files
        .size(&format!(
            "spr/{}",
            atlantis3_data::Release::Cd2001.scarab_bank()
        ))
        .is_some());
}
