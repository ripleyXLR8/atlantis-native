//! *The Secrets of Atlantis: The Sacred Legacy* (*Atlantis V*, Nobilis
//! 2006), the third program of *Atlantis III*'s engine: which build a disc
//! or folder holds, and where each of its data files is.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and is not read).
//!
//! The DVD holds the data in Microsoft cabinets: `DATAS.cab` (every
//! language's pictures, sprites, films, panoramas, music and scripts) and
//! one `DATAS_<language>.cab` per language (voices, texts, the font list);
//! every name in them is `.\Datas\...`. The installer unpacks them into the
//! install folder. The program reads every file under `./datas/`
//! (0x0066c75c, set @ 0x00429620 unless the developers' folder exists),
//! then a folder of its path table (@ 0x004314c0: `scenes/`, `warps/`,
//! `sprites3d/`, `texts/menu/`, ...), then, for the localised files, the
//! registry's `Language` value as a folder (read @ 0x0043dda0 from
//! `HKLM\Software\Nobilis\The secrets of Atlantis`; the texts' path is
//! the table's folder + the language + `/dialogs.xml` @ 0x0043d910).
//!
//! [`Atlantis5Source`] serves those names (`datas/...`, any case, `\` or
//! `/`) from what the player has: the disc's root (a drive or a copy),
//! the disc's image, or an installed or extracted folder.

use std::path::Path;
use std::sync::Arc;

use atlantis_data::cab::{CabFiles, Cabinet};
use atlantis_data::disc_image::{DiscImage, DiscSet};
use atlantis_data::{FileSource, StdDir};
use md5::{Digest, Md5};

use crate::exe::pe_timestamp;

/// The program's file name on the disc and in the install folder.
pub const EXE_NAME: &str = "The secrets of Atlantis.exe";

/// The shared cabinet; the languages' are `DATAS_<language>.cab`.
pub const DATA_CAB: &str = "DATAS.cab";

/// A known build of the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    /// `atlantis5-<medium>-<language>-<size>`.
    pub id: &'static str,
    /// The edition, for people.
    pub label: &'static str,
    pub size: u64,
    /// The PE header's `TimeDateStamp`.
    pub pe_timestamp: u32,
    pub md5: [u8; 16],
}

/// The builds this port knows. Detection does not depend on them (the data
/// decide); they name the edition.
pub const KNOWN_BUILDS: [Build; 2] = [
    // The French DVD's pressed program (TAGES-encrypted).
    Build {
        id: "atlantis5-dvd-fr-3883008",
        label: "The Secrets of Atlantis (2006), French DVD",
        size: 3_883_008,
        // 2006-09-29 14:09:46 UTC.
        pe_timestamp: 0x451d_292a,
        md5: [
            0xe5, 0xc4, 0x3e, 0xb0, 0x41, 0x1d, 0x17, 0x75, 0xe6, 0x24, 0x2f, 0xec, 0x6d, 0x24,
            0x1f, 0x6e,
        ],
    },
    // The international DVD's program as a modified no-disc build.
    Build {
        id: "atlantis5-dvd-int-3891200",
        label: "The Secrets of Atlantis (2006), international DVD",
        size: 3_891_200,
        // 2006-10-25 14:23:52 UTC.
        pe_timestamp: 0x453f_7378,
        md5: [
            0x3d, 0x0f, 0x55, 0xd3, 0x3d, 0x27, 0x1f, 0x6f, 0x5b, 0x12, 0x25, 0x0d, 0x83, 0x8a,
            0x2b, 0x83,
        ],
    },
];

/// Why a file is not a known build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExeError {
    NotPe,
    Unknown {
        size: u64,
        pe_timestamp: u32,
        md5: String,
    },
}

impl std::fmt::Display for ExeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExeError::NotPe => write!(f, "the game's program is not a Windows executable"),
            ExeError::Unknown {
                size,
                pe_timestamp,
                md5,
            } => write!(
                f,
                "the program is not a known build of The Secrets of Atlantis ({size} bytes, PE timestamp {pe_timestamp:#010x}, MD5 {md5})"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Which known build `exe` is (size, PE timestamp, MD5).
pub fn identify(exe: &[u8]) -> Result<&'static Build, ExeError> {
    let stamp = pe_timestamp(exe).ok_or(ExeError::NotPe)?;
    let size = exe.len() as u64;
    let md5: [u8; 16] = Md5::digest(exe).into();
    KNOWN_BUILDS
        .iter()
        .find(|b| b.size == size && b.pe_timestamp == stamp && b.md5 == md5)
        .ok_or(ExeError::Unknown {
            size,
            pe_timestamp: stamp,
            md5: hex(&md5),
        })
}

/// How the player's data came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The disc's files (a drive or a copy): the cabinets, read in place.
    DiscFolder,
    /// An image of the disc (or a folder holding one): its cabinets.
    DiscImage,
    /// The unpacked tree: an install folder (with `Datas\`) or the
    /// cabinets extracted.
    Tree,
}

/// Why a folder is not the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// Neither the cabinets, an image of them, nor a `Datas` tree.
    NotTheGame,
    /// `DATAS.cab` is there but cannot be read.
    Cabinet(String),
    /// No language's data (`DATAS_<language>.cab`, or
    /// `Datas\Texts\Menu\<language>\`).
    NoLanguage,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NotTheGame => write!(
                f,
                "no data of The Secrets of Atlantis here (DATAS.cab, a disc image, or a Datas folder)"
            ),
            OpenError::Cabinet(e) => write!(f, "{DATA_CAB}: {e}"),
            OpenError::NoLanguage => write!(f, "no language's data (DATAS_<language>.cab)"),
        }
    }
}

impl std::error::Error for OpenError {}

/// A name as the program builds it (`./datas/warps/x.jp6`), `/`-separated
/// with no `.` component.
fn clean(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// The game's data, served under the names the program builds
/// (`datas/...`), whatever form the player has them in.
pub struct Atlantis5Source {
    inner: Arc<dyn FileSource>,
    /// Prefix that replaces `datas/` in the inner source (`datas/` itself,
    /// or empty when the folder given is `Datas` itself).
    prefix: String,
    form: Form,
    languages: Vec<String>,
    /// The program beside the data, when there is one.
    exe: Option<Arc<dyn FileSource>>,
}

impl Atlantis5Source {
    /// The data at `path`: a folder (the disc's root, an install, the
    /// extracted cabinets, `Datas` itself, or a folder holding the disc's
    /// image) or an image file.
    pub fn open(path: &Path) -> Result<Atlantis5Source, OpenError> {
        if path.is_file() {
            let img = DiscImage::open(path).ok_or(OpenError::NotTheGame)?;
            return Self::from_disc(Arc::new(img), Form::DiscImage);
        }
        let dir: Arc<dyn FileSource> = Arc::new(StdDir::new(path));
        if dir.size(DATA_CAB).is_some() {
            return Self::from_disc(dir, Form::DiscFolder);
        }
        if dir.has_dir("datas/scenes") && dir.has_dir("datas/warps") {
            return Self::from_tree(dir.clone(), "datas/", Some(dir));
        }
        if dir.has_dir("scenes") && dir.has_dir("warps") && dir.has_dir("texts") {
            return Self::from_tree(dir, "", None);
        }
        if let Some(set) = DiscSet::open_folder(path) {
            if set.size(DATA_CAB).is_some() {
                return Self::from_disc(Arc::new(set), Form::DiscImage);
            }
        }
        Err(OpenError::NotTheGame)
    }

    /// The disc's files in `src` (its root): `DATAS.cab` and every
    /// `DATAS_<language>.cab` beside it.
    pub fn from_disc(src: Arc<dyn FileSource>, form: Form) -> Result<Atlantis5Source, OpenError> {
        let main =
            Cabinet::open(src.clone(), DATA_CAB).map_err(|e| OpenError::Cabinet(e.to_string()))?;
        let mut cabs = vec![main];
        let mut names: Vec<String> = src
            .list("")
            .into_iter()
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.starts_with("datas_") && l.ends_with(".cab")
            })
            .collect();
        names.sort_by_key(|n| n.to_ascii_lowercase());
        for n in names {
            match Cabinet::open(src.clone(), &n) {
                Ok(c) => cabs.push(c),
                Err(e) => log::warn!("{n}: {e}"),
            }
        }
        let files: Arc<dyn FileSource> = Arc::new(CabFiles::new(cabs));
        let exe = src.size(EXE_NAME).is_some().then_some(src);
        Self::new(files, "datas/", form, exe)
    }

    /// An unpacked tree: `inner`'s `prefix` holds `scenes/`, `warps/`, ...
    pub fn from_tree(
        inner: Arc<dyn FileSource>,
        prefix: &str,
        exe: Option<Arc<dyn FileSource>>,
    ) -> Result<Atlantis5Source, OpenError> {
        let exe = exe.filter(|s| s.size(EXE_NAME).is_some());
        Self::new(inner, prefix, Form::Tree, exe)
    }

    fn new(
        inner: Arc<dyn FileSource>,
        prefix: &str,
        form: Form,
        exe: Option<Arc<dyn FileSource>>,
    ) -> Result<Atlantis5Source, OpenError> {
        let mut s = Atlantis5Source {
            inner,
            prefix: prefix.to_string(),
            form,
            languages: Vec::new(),
            exe,
        };
        // The languages: the folders of `texts/menu/` (each language's
        // cabinet brings its `string_resources.xml` there).
        let mut langs: Vec<String> = s
            .list("datas/texts/menu")
            .into_iter()
            .filter(|l| {
                s.size(&format!("datas/texts/menu/{l}/string_resources.xml"))
                    .is_some()
            })
            .map(|l| l.to_ascii_lowercase())
            .collect();
        langs.sort();
        langs.dedup();
        if langs.is_empty() {
            return Err(OpenError::NoLanguage);
        }
        s.languages = langs;
        Ok(s)
    }

    pub fn form(&self) -> Form {
        self.form
    }

    /// The languages present (folder names, lower case: `french`, ...).
    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    /// The program beside the data, identified; `None` without one.
    pub fn build(&self) -> Option<Result<&'static Build, ExeError>> {
        let exe = self.exe.as_ref()?.read(EXE_NAME)?;
        Some(identify(&exe))
    }

    /// Where `name` is in the inner source.
    fn map(&self, name: &str) -> String {
        let n = clean(name);
        match n.get(..6) {
            Some(d) if d.eq_ignore_ascii_case("datas/") => format!("{}{}", self.prefix, &n[6..]),
            _ if n.eq_ignore_ascii_case("datas") => self.prefix.trim_end_matches('/').to_string(),
            _ => n,
        }
    }
}

impl FileSource for Atlantis5Source {
    fn size(&self, name: &str) -> Option<u64> {
        self.inner.size(&self.map(name))
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.inner.read_at(&self.map(name), offset, len)
    }
    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        self.inner.read_ahead(&self.map(name), offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.inner.has_dir(&self.map(name))
    }
    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.inner.read_beside(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        self.inner.list(&self.map(dir))
    }
}

/// The folders of the program's path table (@ 0x004314c0), each under
/// `./datas/`.
pub mod folder {
    pub const SCENES: &str = "datas/scenes/";
    pub const SCRIPTS: &str = "datas/scripts/";
    pub const SOUNDS: &str = "datas/sounds/";
    pub const MUSIC: &str = "datas/sounds/music/";
    pub const SOUNDS_DIALOG: &str = "datas/sounds/dialog/";
    pub const SOUNDS_VIDEO: &str = "datas/sounds/video/";
    pub const SPRITES3D: &str = "datas/sprites3d/";
    pub const SPRITES2D: &str = "datas/sprites2d/";
    pub const FONTS: &str = "datas/fonts/";
    pub const TEXTS_DIALOG: &str = "datas/texts/dialog/";
    pub const TEXTS_MENU: &str = "datas/texts/menu/";
    pub const TEXTS_VIDEO: &str = "datas/texts/video/";
    pub const VIDEOS: &str = "datas/videos/";
    pub const WARPS: &str = "datas/warps/";
    pub const SAVES: &str = "datas/saves/";
}

/// The three text tables of `language` (@ 0x0043d910: the folder, the
/// language, then the file name).
pub fn text_paths(language: &str) -> [String; 3] {
    [
        format!("{}{language}/dialogs.xml", folder::TEXTS_DIALOG),
        format!("{}{language}/string_resources.xml", folder::TEXTS_MENU),
        format!("{}{language}/kinematic_subtitles.xml", folder::TEXTS_VIDEO),
    ]
}

/// Whether `dir` looks like the game in any of its forms (for the
/// launcher's folder check).
pub fn holds_atlantis5(dir: &Path) -> bool {
    Atlantis5Source::open(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::files::MemFiles;

    #[test]
    fn builds() {
        assert_eq!(
            hex(&KNOWN_BUILDS[0].md5),
            "e5c43eb0411d1775e6242fec6d241f6e"
        );
        assert_eq!(
            hex(&KNOWN_BUILDS[1].md5),
            "3d0f55d33d271f6f5b12250d838a2b83"
        );
        assert_eq!(identify(b"MZ"), Err(ExeError::NotPe));
    }

    #[test]
    fn a_tree_and_its_names() {
        let mut m = MemFiles::new();
        m.insert("Datas/Scenes/zeppelin.wam", vec![1]);
        m.insert("Datas/Warps/ze0003.jp6", vec![2]);
        m.insert("Datas/Texts/Menu/french/string_resources.xml", vec![3]);
        m.insert("Datas/Texts/Menu/string_resources.xsd", vec![4]);
        let s = Atlantis5Source::from_tree(Arc::new(m), "datas/", None).unwrap();
        assert_eq!(s.languages(), ["french"]);
        assert_eq!(s.read("./datas/warps/ZE0003.JP6"), Some(vec![2]));
        assert_eq!(s.read(".\\Datas\\Scenes\\zeppelin.wam"), Some(vec![1]));
        assert!(s.has_dir("datas/texts/menu/french"));
        assert_eq!(
            text_paths("french")[1],
            "datas/texts/menu/french/string_resources.xml"
        );
    }

    #[test]
    fn datas_itself() {
        let mut m = MemFiles::new();
        m.insert("Scenes/zeppelin.wam", vec![1]);
        m.insert("Texts/Menu/german/string_resources.xml", vec![3]);
        let s = Atlantis5Source::from_tree(Arc::new(m), "", None).unwrap();
        assert_eq!(s.read("datas/scenes/zeppelin.wam"), Some(vec![1]));
        assert_eq!(s.languages(), ["german"]);
    }
}
