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
//!
//! The language is the installer's choice: the international DVD's
//! (NSIS, `LangDLL`) offers English, Spanish, Italian and German and
//! writes the chosen one's name (`ENGLISH`, ...) as `Language`; the
//! program has no language page of its own. Here a disc of several
//! languages gives a place per language ([`language_place`]: its
//! cabinet's path, `<root>/DATAS_<language>.cab`), each opened in that
//! language; the root itself opens in [`default_language`]'s.

use std::path::{Path, PathBuf};
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
pub const KNOWN_BUILDS: [Build; 3] = [
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
    // The international DVD's pressed program (TAGES-encrypted, as the
    // French one).
    Build {
        id: "atlantis5-dvd-int-3883008",
        label: "The Secrets of Atlantis (2006), international DVD",
        size: 3_883_008,
        // 2006-10-25 12:23:52 UTC.
        pe_timestamp: 0x453f_5758,
        md5: [
            0xfc, 0xb3, 0x57, 0xac, 0xdb, 0x6c, 0x16, 0xa7, 0x6f, 0x3f, 0x4f, 0x6c, 0xb5, 0xa6,
            0x25, 0xc5,
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
    /// A language's place ([`language_place`]) whose data hold the game
    /// but not that language.
    NotThisLanguage(String),
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
            OpenError::NotThisLanguage(l) => write!(f, "no {l} data here (DATAS_{l}.cab)"),
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
    /// The language chosen: opened from that language's place
    /// ([`language_place`]) or set ([`Atlantis5Source::choose_language`]).
    chosen: Option<String>,
    /// The program beside the data, when there is one.
    exe: Option<Arc<dyn FileSource>>,
    /// The disc's cabinets, when the data are read from them.
    cabs: Option<Arc<CabFiles>>,
}

impl Atlantis5Source {
    /// The data at `path`: a folder (the disc's root, an install, the
    /// extracted cabinets, `Datas` itself, or a folder holding the disc's
    /// image) or an image file.
    ///
    /// A language's place (`<root>/DATAS_<language>.cab`, [`language_place`])
    /// opens `<root>` in that language.
    pub fn open(path: &Path) -> Result<Atlantis5Source, OpenError> {
        if let Some((root, language)) = split_language_place(path) {
            match Self::open_root(&root) {
                Ok(mut s) => {
                    return if s.choose_language(&language) {
                        Ok(s)
                    } else {
                        Err(OpenError::NotThisLanguage(language))
                    };
                }
                Err(OpenError::NotTheGame) => {}
                Err(e) => return Err(e),
            }
        }
        Self::open_root(path)
    }

    /// [`Atlantis5Source::open`] of a path that is not a language's place.
    fn open_root(path: &Path) -> Result<Atlantis5Source, OpenError> {
        if path.is_file() {
            let img = DiscImage::open(path).ok_or(OpenError::NotTheGame)?;
            // Another disc's image: not the game (as for a folder), not
            // a cabinet that cannot be read.
            if img.size(DATA_CAB).is_none() {
                return Err(OpenError::NotTheGame);
            }
            return Self::from_disc(Arc::new(img), Form::DiscImage);
        }
        match Self::from_folder(Arc::new(StdDir::new(path))) {
            Err(OpenError::NotTheGame) => {}
            r => return r,
        }
        if let Some(set) = DiscSet::open_folder(path) {
            if set.size(DATA_CAB).is_some() {
                return Self::from_disc(Arc::new(set), Form::DiscImage);
            }
        }
        Err(OpenError::NotTheGame)
    }

    /// The disc's root at `path` when the data there are the disc's
    /// cabinets (a drive or a copy of its files, its image, or a folder
    /// holding the image): what [`Atlantis5Source::open`] reads them from.
    /// `None` for an install, the extracted cabinets, or no game.
    pub fn disc_root(path: &Path) -> Option<Arc<dyn FileSource>> {
        let path = &place_root(path);
        if path.is_file() {
            let img = DiscImage::open(path)?;
            return img.size(DATA_CAB).is_some().then(|| Arc::new(img) as _);
        }
        let dir = StdDir::new(path);
        if dir.size(DATA_CAB).is_some() {
            return Some(Arc::new(dir));
        }
        if dir.has_dir("datas/scenes") || dir.has_dir("scenes") {
            return None;
        }
        let set = DiscSet::open_folder(path)?;
        set.size(DATA_CAB).is_some().then(|| Arc::new(set) as _)
    }

    /// The data in folder `dir` (the disc's root or a copy of it, an
    /// install, the extracted cabinets, or `Datas` itself; not an image).
    pub fn from_folder(dir: Arc<dyn FileSource>) -> Result<Atlantis5Source, OpenError> {
        if dir.size(DATA_CAB).is_some() {
            return Self::from_disc(dir, Form::DiscFolder);
        }
        if dir.has_dir("datas/scenes") && dir.has_dir("datas/warps") {
            return Self::from_tree(dir.clone(), "datas/", Some(dir));
        }
        if dir.has_dir("scenes") && dir.has_dir("warps") && dir.has_dir("texts") {
            return Self::from_tree(dir, "", None);
        }
        Err(OpenError::NotTheGame)
    }

    /// The disc's files in `src` (its root): `DATAS.cab` and every
    /// `DATAS_<language>.cab` beside it.
    pub fn from_disc(src: Arc<dyn FileSource>, form: Form) -> Result<Atlantis5Source, OpenError> {
        Self::from_disc_indexed(src, form, None)
    }

    /// [`Atlantis5Source::from_disc`] with the cabinets' checkpoint
    /// indexes kept in `index` (see [`atlantis_data::cab::set_index_dir`],
    /// which gives the folder when `index` is `None`).
    pub fn from_disc_indexed(
        src: Arc<dyn FileSource>,
        form: Form,
        index: Option<PathBuf>,
    ) -> Result<Atlantis5Source, OpenError> {
        let open = |name: &str| {
            Cabinet::open(src.clone(), name).map(|mut c| {
                if index.is_some() {
                    c.use_index_dir(index.clone());
                }
                c
            })
        };
        let main = open(DATA_CAB).map_err(|e| OpenError::Cabinet(e.to_string()))?;
        let mut cabs = vec![main];
        for n in language_cabinets(&*src) {
            match open(&n) {
                Ok(c) => cabs.push(c),
                Err(e) => log::warn!("{n}: {e}"),
            }
        }
        let cabs = Arc::new(CabFiles::new(cabs));
        let files: Arc<dyn FileSource> = cabs.clone();
        let exe = src.size(EXE_NAME).is_some().then_some(src);
        let mut s = Self::new(files, "datas/", form, exe)?;
        s.cabs = Some(cabs);
        Ok(s)
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
            chosen: None,
            exe,
            cabs: None,
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

    /// The language chosen, if any ([`Atlantis5Source::language`] gives
    /// the one to play).
    pub fn chosen_language(&self) -> Option<&str> {
        self.chosen.as_deref()
    }

    /// Chooses `language` (a folder name, any case) when the data have
    /// it; false otherwise (the choice unchanged).
    pub fn choose_language(&mut self, language: &str) -> bool {
        let Some(l) = self
            .languages
            .iter()
            .find(|l| l.eq_ignore_ascii_case(language))
        else {
            return false;
        };
        self.chosen = Some(l.clone());
        true
    }

    /// The language to play: the one chosen, else [`default_language`]'s
    /// with the system's language `system` (an ISO 639-1 code: `de`, ...).
    pub fn language(&self, system: Option<&str>) -> String {
        self.chosen
            .clone()
            .unwrap_or_else(|| default_language(&self.languages, system))
    }

    /// The program beside the data, identified; `None` without one.
    pub fn build(&self) -> Option<Result<&'static Build, ExeError>> {
        let exe = self.exe.as_ref()?.read(EXE_NAME)?;
        Some(identify(&exe))
    }

    /// From the disc's cabinets: decodes the cabinet's folder holding
    /// `name` toward it, at most about `budget` bytes further
    /// ([`CabFiles::warm`]); true once it is reached, at once for the
    /// unpacked tree or a name not in the cabinets. The web build reads
    /// its start's films so, a step per frame.
    pub fn warm(&self, name: &str, budget: u64) -> bool {
        self.warm_progress(name, budget)
            .is_none_or(|(at, to)| at >= to)
    }

    /// [`Atlantis5Source::warm`] with how far: (the cabinet's folder
    /// decoded up to, the file's offset in it); `None` when nothing is to
    /// be decoded.
    pub fn warm_progress(&self, name: &str, budget: u64) -> Option<(u64, u64)> {
        self.cabs.as_ref()?.warm_progress(&self.map(name), budget)
    }

    /// From the disc's cabinets: decodes about `budget` bytes more of
    /// a folder not yet decoded to its end ([`CabFiles::warm_next`]);
    /// false once none is left (at once for the unpacked tree).
    pub fn warm_next(&self, budget: u64) -> bool {
        self.cabs.as_ref().is_some_and(|c| c.warm_next(budget))
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

/// The languages' cabinets in the disc's root `src` (`DATAS_<language>.cab`,
/// any case), in name order.
pub fn language_cabinets(src: &dyn FileSource) -> Vec<String> {
    let mut names: Vec<String> = src
        .list("")
        .into_iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("datas_") && l.ends_with(".cab")
        })
        .collect();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names
}

/// The disc's files the game needs, in its root `src`: `DATAS.cab`, the
/// languages' cabinets and, when the disc has it, the program (which says
/// the edition, [`Atlantis5Source::build`]). Copied to a folder, they open
/// as the disc's files ([`Form::DiscFolder`]).
pub fn disc_files(src: &dyn FileSource) -> Vec<String> {
    let mut names = vec![DATA_CAB.to_string()];
    names.extend(language_cabinets(src));
    if src.size(EXE_NAME).is_some() {
        names.push(EXE_NAME.to_string());
    }
    names
}

/// The installer's languages (the international DVD's `LangDLL` list,
/// and the French DVD's one): the data's folder name, the ISO 639-1 code
/// of its Windows language (`LANG_ENGLISH` 1033, ...), its name for people.
pub const LANGUAGES: [(&str, &str, &str); 5] = [
    ("english", "en", "English"),
    ("spanish", "es", "Spanish"),
    ("italian", "it", "Italian"),
    ("german", "de", "German"),
    ("french", "fr", "French"),
];

/// The name for people of the data's language folder `language`
/// (`german` → `German`); an unknown folder's name capitalised.
pub fn language_name(language: &str) -> String {
    if let Some((_, _, name)) = LANGUAGES
        .iter()
        .find(|(f, _, _)| f.eq_ignore_ascii_case(language))
    {
        return (*name).to_string();
    }
    let mut out = String::new();
    let mut chars = language.chars();
    if let Some(c) = chars.next() {
        out.extend(c.to_uppercase());
        out.extend(chars);
    }
    out
}

/// `languages` in the installer's order ([`LANGUAGES`]: English, Spanish,
/// Italian, German, French), any other after them.
pub fn in_installer_order(languages: &[String]) -> Vec<String> {
    let mut out = languages.to_vec();
    out.sort_by_key(|l| {
        LANGUAGES
            .iter()
            .position(|(f, _, _)| f.eq_ignore_ascii_case(l))
            .unwrap_or(LANGUAGES.len())
    });
    out
}

/// The data's language folder of the ISO 639-1 code `code` (`de`, `de-AT`,
/// `de_DE.UTF-8`: the part before `-`, `_` or `.`), if the game has one.
pub fn language_of_code(code: &str) -> Option<&'static str> {
    let primary = code.split(['-', '_', '.']).next()?;
    LANGUAGES
        .iter()
        .find(|(_, c, _)| c.eq_ignore_ascii_case(primary))
        .map(|(f, _, _)| *f)
}

/// The language played when none was chosen: the only one; else the
/// system's (`system`, an ISO 639-1 code) when the data have it; else
/// English; else the first. The original's installer asks, the system's
/// language preselected (NSIS sets `$LANGUAGE` from it); with another
/// system language it preselects its first table's, German [INFERRED:
/// NSIS 2's rule, not run]. Native takes English then, the port's own
/// choice.
pub fn default_language(languages: &[String], system: Option<&str>) -> String {
    if let [one] = languages {
        return one.clone();
    }
    let has = |l: &str| {
        languages
            .iter()
            .find(|x| x.eq_ignore_ascii_case(l))
            .cloned()
    };
    system
        .and_then(language_of_code)
        .and_then(has)
        .or_else(|| has("english"))
        .or_else(|| languages.first().cloned())
        .unwrap_or_else(|| "english".to_string())
}

/// `path` as a language's place: (the root, the language) when its name is
/// `DATAS_<language>.cab` (any case).
pub fn split_language_place(path: &Path) -> Option<(PathBuf, String)> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let language = lower.strip_prefix("datas_")?.strip_suffix(".cab")?;
    if language.is_empty() {
        return None;
    }
    Some((path.parent()?.to_path_buf(), language.to_string()))
}

/// The root of `path`: the folder or image a language's place is in, else
/// `path` itself.
pub fn place_root(path: &Path) -> PathBuf {
    match split_language_place(path) {
        Some((root, _)) if place_language(path).is_some() => root,
        _ => path.to_path_buf(),
    }
}

/// The language of `path` when it is a language's place (not a folder).
pub fn place_language(path: &Path) -> Option<String> {
    split_language_place(path)
        .filter(|_| !path.is_dir())
        .map(|(_, l)| l)
}

/// The place of `language` of the data at `root` (a disc's root, its image,
/// an install): `<root>/DATAS_<language>.cab`, the language's cabinet's own
/// name when `root` is the disc's files. Opened, it is the game in that
/// language. On the disc it is the cabinet itself; for an image or an
/// install it names no file, as the cabinet's place does not.
pub fn language_place(root: &Path, language: &str) -> PathBuf {
    let own = StdDir::new(root);
    let name = language_cabinets(&own)
        .into_iter()
        .find(|n| {
            split_language_place(Path::new(n))
                .is_some_and(|(_, l)| l.eq_ignore_ascii_case(language))
        })
        .unwrap_or_else(|| format!("DATAS_{language}.cab"));
    root.join(name)
}

/// The places of the languages of the data at `path`, one per language
/// ([`language_place`]), when there are two or more and `path` is not
/// one of them; empty otherwise (a single language, a language's place,
/// not the game).
pub fn language_places(path: &Path) -> Vec<PathBuf> {
    if place_language(path).is_some() {
        return Vec::new();
    }
    match Atlantis5Source::open_root(path) {
        Ok(s) if s.languages().len() > 1 => in_installer_order(s.languages())
            .iter()
            .map(|l| language_place(path, l))
            .collect(),
        _ => Vec::new(),
    }
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
/// launcher's folder check): its cabinets, an image of them or its
/// `Datas` tree, even when they cannot be read (`DATAS.cab` cut short, no
/// language's data): such a folder is the game's, and the player is told
/// what is wrong with it rather than that it is not the game.
pub fn holds_atlantis5(dir: &Path) -> bool {
    !matches!(Atlantis5Source::open(dir), Err(OpenError::NotTheGame))
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
            "fcb357acdb6c16a76f3f4f6cb5a625c5"
        );
        assert_eq!(
            hex(&KNOWN_BUILDS[2].md5),
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
    fn languages_and_their_choice() {
        let langs = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let int = langs(&["english", "german", "italian", "spanish"]);
        assert_eq!(default_language(&int, Some("de-DE")), "german");
        assert_eq!(default_language(&int, Some("it_IT.UTF-8")), "italian");
        assert_eq!(default_language(&int, Some("fr")), "english");
        assert_eq!(default_language(&int, None), "english");
        assert_eq!(default_language(&langs(&["french"]), Some("de")), "french");
        assert_eq!(
            default_language(&langs(&["german", "spanish"]), Some("en")),
            "german"
        );
        assert_eq!(language_of_code("ES"), Some("spanish"));
        assert_eq!(language_of_code("pt-BR"), None);
        assert_eq!(language_name("german"), "German");
        assert_eq!(language_name("dutch"), "Dutch");

        let mut m = MemFiles::new();
        m.insert("Datas/Scenes/zeppelin.wam", vec![1]);
        for l in ["english", "german"] {
            m.insert(
                &format!("Datas/Texts/Menu/{l}/string_resources.xml"),
                vec![3],
            );
        }
        let mut s = Atlantis5Source::from_tree(Arc::new(m), "datas/", None).unwrap();
        assert_eq!(s.languages(), ["english", "german"]);
        assert_eq!(s.chosen_language(), None);
        assert_eq!(s.language(Some("de")), "german");
        assert!(!s.choose_language("french"));
        assert!(s.choose_language("ENGLISH"));
        assert_eq!(s.language(Some("de")), "english");
    }

    #[test]
    fn language_places() {
        assert_eq!(
            split_language_place(Path::new("/dvd/datas_german.cab")),
            Some((PathBuf::from("/dvd"), "german".to_string()))
        );
        assert_eq!(
            split_language_place(Path::new("/x/soa.iso/DATAS_English.CAB")),
            Some((PathBuf::from("/x/soa.iso"), "english".to_string()))
        );
        assert_eq!(split_language_place(Path::new("/dvd/datas.cab")), None);
        assert_eq!(split_language_place(Path::new("/dvd/datas_.cab")), None);
        assert_eq!(
            place_root(Path::new("/nowhere/datas_spanish.cab")),
            PathBuf::from("/nowhere")
        );
        assert_eq!(
            place_root(Path::new("/nowhere/datas")),
            PathBuf::from("/nowhere/datas")
        );
        // An install with two languages: a place per language, each
        // opened in its language; one language: none.
        let base = std::env::var_os("ATLANTIS_NATIVE_DATA")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let root = base.join(format!("an-a5-places-{}", std::process::id()));
        for l in ["german", "english"] {
            let d = root.join(format!("Datas/Texts/Menu/{l}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("string_resources.xml"), b"x").unwrap();
        }
        std::fs::create_dir_all(root.join("Datas/Scenes")).unwrap();
        std::fs::create_dir_all(root.join("Datas/Warps")).unwrap();
        let places = super::language_places(&root);
        assert_eq!(
            places,
            [
                root.join("DATAS_english.cab"),
                root.join("DATAS_german.cab")
            ]
        );
        let s = Atlantis5Source::open(&places[1]).unwrap();
        assert_eq!(s.chosen_language(), Some("german"));
        assert!(super::language_places(&places[1]).is_empty());
        assert!(matches!(
            Atlantis5Source::open(&root.join("datas_italian.cab")),
            Err(OpenError::NotThisLanguage(l)) if l == "italian"
        ));
        assert!(holds_atlantis5(&places[0]));
        std::fs::remove_dir_all(root.join("Datas/Texts/Menu/english")).unwrap();
        assert!(super::language_places(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
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

    /// An ISO 9660 image whose root holds only `README.TXT`.
    fn other_disc() -> Vec<u8> {
        let mut s = vec![0u8; 2048 * 21];
        let rec = |name: &[u8], lba: u32, size: u32, dir: bool| -> Vec<u8> {
            let mut r = vec![0u8; 33 + name.len() + (name.len() + 1) % 2];
            r[0] = u8::try_from(r.len()).unwrap();
            r[2..6].copy_from_slice(&lba.to_le_bytes());
            r[10..14].copy_from_slice(&size.to_le_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = u8::try_from(name.len()).unwrap();
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        let at = |n: usize| n * 2048;
        s[at(16)] = 1;
        s[at(16) + 1..at(16) + 6].copy_from_slice(b"CD001");
        let root = rec(&[0], 18, 2048, true);
        s[at(16) + 156..at(16) + 156 + root.len()].copy_from_slice(&root);
        s[at(17)] = 255;
        s[at(17) + 1..at(17) + 6].copy_from_slice(b"CD001");
        let mut dir = Vec::new();
        dir.extend(rec(&[0], 18, 2048, true));
        dir.extend(rec(&[1], 18, 2048, true));
        dir.extend(rec(b"README.TXT;1", 20, 5, false));
        s[at(18)..at(18) + dir.len()].copy_from_slice(&dir);
        s[at(20)..at(20) + 5].copy_from_slice(b"hello");
        s
    }

    #[test]
    fn another_disc_s_image_is_not_the_game() {
        let base = std::env::var_os("ATLANTIS_NATIVE_DATA")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let p = base.join(format!("an-a5-other-disc-{}.iso", std::process::id()));
        std::fs::write(&p, other_disc()).unwrap();
        assert!(DiscImage::open(&p).is_some(), "an image");
        assert!(matches!(
            Atlantis5Source::open(&p),
            Err(OpenError::NotTheGame)
        ));
        let _ = std::fs::remove_file(&p);
    }
}
