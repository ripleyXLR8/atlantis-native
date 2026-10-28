//! Cryo's DVD-ROM of 1998: one double-sided disc for eight languages. Each
//! side holds a folder `FACE_A` (side A) or `FACE_B` (side B) with the
//! films, panoramas, talking heads, puzzles and ambient lists every
//! language shares (`CYCLO`, `DIALOG`, `PUZZLES`, `UBB_VUE`, `WAM`) and one
//! folder per language with its program, scripts, sprites, pictures and
//! voices. The program runs from its language's folder: it reads the shared
//! folders at the root of the disc (`\face_a\ubb_vue\`, ...: seven strings
//! of atlantis-dvd-en-695296's data, 0x004abbfc..0x004ae404) and the others
//! relative to its folder (`scenar\`, `wav\`, `sprite\`, `sprlist\`,
//! `images\`). [`DvdFiles`] serves one language's files that way, from a
//! copy of a side, the drive, or the images of the sides.

use std::sync::Arc;

use crate::files::FileSource;

/// The folders of a side that every language shares.
pub const SHARED_FOLDERS: [&str; 5] = ["CYCLO", "DIALOG", "PUZZLES", "UBB_VUE", "WAM"];

/// One language of the DVD: its side, its folder, its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DvdLanguage {
    /// 'A' or 'B'.
    pub side: char,
    /// Its folder in `FACE_<side>` (AUTORUN.EXE's table: `FACE_A/ALL`,
    /// `FACE_A/ENG`, `FACE_A/FRA`, `FACE_A/HOL`, `FACE_B/ESP`, `FACE_B/ITA`,
    /// `FACE_B/SUE`, `FACE_B/JAP`).
    pub folder: &'static str,
    /// The language, for people.
    pub language: &'static str,
}

/// The eight languages, in AUTORUN.EXE's order.
pub const LANGUAGES: [DvdLanguage; 8] = [
    DvdLanguage {
        side: 'A',
        folder: "ALL",
        language: "German",
    },
    DvdLanguage {
        side: 'A',
        folder: "ENG",
        language: "English",
    },
    DvdLanguage {
        side: 'A',
        folder: "FRA",
        language: "French",
    },
    DvdLanguage {
        side: 'A',
        folder: "HOL",
        language: "Dutch",
    },
    DvdLanguage {
        side: 'B',
        folder: "ESP",
        language: "Spanish",
    },
    DvdLanguage {
        side: 'B',
        folder: "ITA",
        language: "Italian",
    },
    DvdLanguage {
        side: 'B',
        folder: "SUE",
        language: "Swedish",
    },
    DvdLanguage {
        side: 'B',
        folder: "JAP",
        language: "Japanese",
    },
];

impl DvdLanguage {
    /// The side's folder: `FACE_A` or `FACE_B`.
    pub fn face(&self) -> &'static str {
        if self.side == 'A' {
            "FACE_A"
        } else {
            "FACE_B"
        }
    }

    /// The language's folder below the disc's root, `FACE_A/ENG`.
    pub fn path(&self) -> String {
        format!("{}/{}", self.face(), self.folder)
    }

    /// The language of side folder `face` (`FACE_A`, any case) and
    /// language folder `folder` (`ENG`, any case).
    pub fn of(face: &str, folder: &str) -> Option<&'static DvdLanguage> {
        LANGUAGES
            .iter()
            .find(|l| l.face().eq_ignore_ascii_case(face) && l.folder.eq_ignore_ascii_case(folder))
    }
}

/// Whether `src` holds side `side`'s shared folders.
pub fn has_side(src: &dyn FileSource, side: char) -> bool {
    let face = if side == 'A' { "FACE_A" } else { "FACE_B" };
    SHARED_FOLDERS
        .iter()
        .all(|f| src.has_dir(&format!("{face}/{f}")))
}

/// The languages `src` (a side's root, the drive, or the images of the
/// sides) holds: those whose folder holds `atlantis.exe`, on a side whose
/// shared folders are there; in AUTORUN.EXE's order.
pub fn languages_in(src: &dyn FileSource) -> Vec<&'static DvdLanguage> {
    let sides: Vec<char> = ['A', 'B']
        .into_iter()
        .filter(|&s| has_side(src, s))
        .collect();
    LANGUAGES
        .iter()
        .filter(|l| {
            sides.contains(&l.side) && src.size(&format!("{}/ATLANTIS.EXE", l.path())).is_some()
        })
        .collect()
}

/// The DVD's languages among relative paths (lower case, `/` separated,
/// the picked folder's name first: the web build's picked files): each
/// language whose program is there with its side's shared folders, and
/// the path before its `face_x/` (the shortest one).
pub fn languages_in_paths<'a>(
    paths: impl Iterator<Item = &'a str> + Clone,
) -> Vec<(&'static DvdLanguage, String)> {
    let mut out = Vec::new();
    for l in &LANGUAGES {
        let tail = format!("{}/atlantis.exe", l.path().to_ascii_lowercase());
        let root = paths
            .clone()
            .filter_map(|k| {
                let root = k.strip_suffix(&tail)?;
                (root.is_empty() || root.ends_with('/')).then(|| root.to_string())
            })
            .min_by_key(String::len);
        let Some(root) = root else { continue };
        let face = format!("{root}{}/", l.face().to_ascii_lowercase());
        let shared = SHARED_FOLDERS.iter().all(|f| {
            let dir = format!("{face}{}/", f.to_ascii_lowercase());
            paths.clone().any(|k| k.starts_with(&dir))
        });
        if shared {
            out.push((l, root));
        }
    }
    out
}

/// The name a relative path (lower case, as [`languages_in_paths`] takes
/// them) has for language `l` of the DVD whose root is `root`: below the
/// language's folder, or in a shared folder of its side; `None` for the
/// other files.
pub fn path_for_language(path: &str, l: &DvdLanguage, root: &str) -> Option<String> {
    let face = format!("{root}{}/", l.face().to_ascii_lowercase());
    let own = format!("{face}{}/", l.folder.to_ascii_lowercase());
    if let Some(rel) = path.strip_prefix(&own) {
        return Some(rel.to_string());
    }
    let rel = path.strip_prefix(&face)?;
    let first = rel.split('/').next()?;
    SHARED_FOLDERS
        .iter()
        .any(|s| s.eq_ignore_ascii_case(first))
        .then(|| rel.to_string())
}

/// The files of one language of the DVD, as its program reads them: a name
/// in one of the [`SHARED_FOLDERS`] from the side's folder, any other from
/// the language's folder.
pub struct DvdFiles {
    src: Arc<dyn FileSource>,
    language: &'static DvdLanguage,
}

impl DvdFiles {
    /// The language `language` of the disc whose root `src` serves; `None`
    /// when `src` lacks its side's shared folders or its program.
    pub fn new(src: Arc<dyn FileSource>, language: &'static DvdLanguage) -> Option<DvdFiles> {
        (has_side(src.as_ref(), language.side)
            && src
                .size(&format!("{}/ATLANTIS.EXE", language.path()))
                .is_some())
        .then_some(DvdFiles { src, language })
    }

    pub fn language(&self) -> &'static DvdLanguage {
        self.language
    }

    /// The name below the disc's root.
    fn map(&self, name: &str) -> String {
        let n = name.replace('\\', "/");
        let n = n.trim_matches('/');
        let first = n.split('/').next().unwrap_or("");
        if SHARED_FOLDERS.iter().any(|f| f.eq_ignore_ascii_case(first)) {
            format!("{}/{n}", self.language.face())
        } else if n.is_empty() {
            self.language.path()
        } else {
            format!("{}/{n}", self.language.path())
        }
    }
}

impl FileSource for DvdFiles {
    fn size(&self, name: &str) -> Option<u64> {
        self.src.size(&self.map(name))
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.src.read_at(&self.map(name), offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.src.has_dir(&self.map(name))
    }

    /// The player's file in the language's folder (a copy of the side), or
    /// beside the disc's files or images.
    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.src
            .read_beside(&self.map(name))
            .or_else(|| self.src.read_beside(name))
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.src.list(&self.map(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    fn side(files: &mut MemFiles, face: &str, langs: &[&str]) {
        for f in SHARED_FOLDERS {
            files.insert(&format!("{face}/{f}/x.bin"), f.as_bytes().to_vec());
        }
        files.insert(&format!("{face}/UBB_VUE/LOGO15N.HNM"), b"logo".to_vec());
        for l in langs {
            files.insert(&format!("{face}/{l}/ATLANTIS.EXE"), l.as_bytes().to_vec());
            files.insert(&format!("{face}/{l}/SCENAR/CHAPI001.CON"), b"con".to_vec());
        }
    }

    #[test]
    fn languages_of_one_or_both_sides() {
        let mut a = MemFiles::new();
        side(&mut a, "FACE_A", &["ALL", "ENG", "FRA", "HOL"]);
        let names = |src: &dyn FileSource| -> Vec<&str> {
            languages_in(src).iter().map(|l| l.folder).collect()
        };
        assert_eq!(names(&a), ["ALL", "ENG", "FRA", "HOL"]);
        side(&mut a, "FACE_B", &["ESP", "JAP"]);
        assert_eq!(names(&a), ["ALL", "ENG", "FRA", "HOL", "ESP", "JAP"]);
        // A language folder without its side's shared folders: not listed.
        let mut lone = MemFiles::new();
        lone.insert("FACE_B/SUE/ATLANTIS.EXE", vec![1]);
        assert!(languages_in(&lone).is_empty());
        assert_eq!(
            DvdLanguage::of("face_b", "sue").unwrap().language,
            "Swedish"
        );
        assert_eq!(DvdLanguage::of("FACE_A", "SUE"), None);
    }

    /// The web build's picked files: the drive (or a side's copy) picked,
    /// or the folder above it.
    #[test]
    fn languages_among_picked_paths() {
        let mut paths = vec![
            "side a/face_a/eng/atlantis.exe".to_string(),
            "side a/face_a/eng/scenar/chapi001.con".to_string(),
            "side a/face_a/fra/atlantis.exe".to_string(),
            "side a/face_a/hol/sprite/2d/fontmin.spr".to_string(),
            "side a/autorun.exe".to_string(),
        ];
        for f in SHARED_FOLDERS {
            paths.push(format!("side a/face_a/{}/x", f.to_ascii_lowercase()));
        }
        let found = languages_in_paths(paths.iter().map(String::as_str));
        let names: Vec<(&str, &str)> = found.iter().map(|(l, r)| (l.folder, r.as_str())).collect();
        assert_eq!(names, [("ENG", "side a/"), ("FRA", "side a/")]);
        let (l, root) = &found[0];
        let mapped: Vec<Option<String>> = paths
            .iter()
            .map(|p| path_for_language(p, l, root))
            .collect();
        assert_eq!(mapped[0].as_deref(), Some("atlantis.exe"));
        assert_eq!(mapped[1].as_deref(), Some("scenar/chapi001.con"));
        assert_eq!(mapped[2], None, "another language's");
        assert_eq!(mapped[3], None);
        assert_eq!(mapped[4], None);
        assert_eq!(mapped[5].as_deref(), Some("cyclo/x"));
        // The shared folders missing: none.
        assert!(languages_in_paths(paths[..5].iter().map(String::as_str)).is_empty());
    }

    #[test]
    fn shared_folders_from_the_side_the_rest_from_the_language() {
        let mut m = MemFiles::new();
        side(&mut m, "FACE_A", &["ENG", "FRA"]);
        let src: Arc<dyn FileSource> = Arc::new(m);
        let fr = DvdFiles::new(src.clone(), DvdLanguage::of("FACE_A", "FRA").unwrap()).unwrap();
        assert_eq!(fr.read("atlantis.exe").as_deref(), Some(&b"FRA"[..]));
        assert_eq!(
            fr.read("UBB_VUE\\LOGO15N.HNM").as_deref(),
            Some(&b"logo"[..])
        );
        assert_eq!(
            fr.read("ubb_vue/logo15n.hnm").as_deref(),
            Some(&b"logo"[..])
        );
        assert_eq!(
            fr.read("scenar\\chapi001.con").as_deref(),
            Some(&b"con"[..])
        );
        assert!(fr.has_dir("SCENAR") && fr.has_dir("CYCLO") && !fr.has_dir("IMAGES"));
        assert_eq!(fr.list("SCENAR"), ["chapi001.con"]);
        // A side not there.
        assert!(DvdFiles::new(src, DvdLanguage::of("FACE_B", "ESP").unwrap()).is_none());
    }
}
