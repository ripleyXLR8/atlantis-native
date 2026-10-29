//! The game's files: the install folder and the four CDs.
//!
//! The original reads loose files only (no archive), through thin wrappers
//! over the Win32 file API (`AtlFile_Open` @ 0x00403aa0). Every path is
//! built as `root + folder + name` (@ 0x005d9d68, `%s%s%s`) with one of two
//! roots:
//!
//! - the install folder, the executable's own (@ 0x0040de50): places,
//!   scripts, texts, sprite banks, music marks;
//! - the root of the current CD (@ 0x0040dde0): everything under
//!   `datas_a2\` (panoramas, videos, sounds, animations, 3D data).
//!
//! Windows matches names without regard to case, and the discs mix cases
//! (`.SPW` / `.spw`, `3d/Dial` / `3d/dial`): every source here folds case.
//!
//! The four discs share 260 paths, 254 of them byte for byte; six differ
//! between discs, so a file under `datas_a2\` is read from the disc the
//! original would read: the current one, chosen by the `MONDE` rule
//! ([`A2Files::set_monde`]) as on a single drive, the disc asked for
//! inserted when asked. A port with every disc at hand needs no disc
//! prompt; a file missing from the current disc is read from another disc
//! that has it (the original would fail to open it), and logged.
//! [`A2Files::read_current`] reads as the original does, for the files
//! whose absence the game shows: the sounds (film 44's track is on disc 2,
//! the film on disc 4, so the original plays it silent).
//!
//! GOG.com's edition installs every disc in one `DATAS_A2\` folder of the
//! install folder, with the four discs' markers: a folder (or image) that
//! holds all four counts as each of the discs 1 to 4, so no disc is ever
//! asked for (GOG's own executable is patched to use its install folder as
//! the CD and to leave the CD page at once). Its program is `Game.exe`
//! (its `atlantis2.exe` is a launcher), see [`find_exe`]. Ten of the
//! discs' file names carry accents (`TR-D-tête araignée.tga`, which the
//! spider's and the canoe's `.db` files name); GOG has them only without
//! accents and under 8.3 names: a name not found is looked for with its
//! accents folded, then as its ISO 9660 level-1 name ([`alias_of`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use atlantis_data::disc_image::{DiscImage, IMAGE_EXTENSIONS};
use atlantis_data::{FileSource, StdDir};

use crate::edition::{edition_label, language_of_menu_txt, Language};
use crate::exe::{identify, Build, ExeError, EXE_NAMES, KNOWN_BUILDS, LAUNCHER_SIZE};

/// Which root a folder is under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Root {
    /// The install folder.
    Install,
    /// The current CD.
    Cd,
}

/// The folders of the original's path builders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    /// `wam\` (install, @ 0x00403770): places.
    Wam,
    /// `datas_a2\warp\` (CD, @ 0x004037a0): view panoramas.
    Warp,
    /// `datas_a2\apc\` (CD, @ 0x004037d0): sounds.
    Apc,
    /// `music\` (install, @ 0x00403800): music marks (`.spp`).
    Music,
    /// `datas_a2\apc\music\` (CD, @ 0x00403830): music streams.
    ApcMusic,
    /// `datas_a2\apc\dial\` (CD, @ 0x00403860): dialog voices.
    ApcDial,
    /// `datas_a2\spw\` (CD, @ 0x00403890): place animations.
    Spw,
    /// `datas_a2\hnm\` (CD, @ 0x004038c0): cinematics, transitions, dialog
    /// videos.
    Hnm,
    /// `datas_a2\3d\` (CD, @ 0x004038f0): puzzle 3D data.
    ThreeD,
    /// `datas_a2\3d\dial\` (CD, @ 0x00403930): talking heads.
    ThreeDDial,
    /// `spr\` (install, @ 0x00403980): sprite banks.
    Spr,
    /// `datas_a2\spf\` (CD, @ 0x004039b0): transition sprites and the disc
    /// markers.
    Spf,
    /// `scenar\` (install, @ 0x004039e0): scripts.
    Scenar,
    /// `spr\menu\` (install, @ 0x00403a10): menu pictures.
    SprMenu,
    /// `loc\` (install, @ 0x00403a70): texts and the font.
    Loc,
}

impl Folder {
    /// The root and the folder (with `/` separators and a trailing one).
    pub fn location(self) -> (Root, &'static str) {
        use Folder::*;
        match self {
            Wam => (Root::Install, "wam/"),
            Warp => (Root::Cd, "datas_a2/warp/"),
            Apc => (Root::Cd, "datas_a2/apc/"),
            Music => (Root::Install, "music/"),
            ApcMusic => (Root::Cd, "datas_a2/apc/music/"),
            ApcDial => (Root::Cd, "datas_a2/apc/dial/"),
            Spw => (Root::Cd, "datas_a2/spw/"),
            Hnm => (Root::Cd, "datas_a2/hnm/"),
            ThreeD => (Root::Cd, "datas_a2/3d/"),
            ThreeDDial => (Root::Cd, "datas_a2/3d/dial/"),
            Spr => (Root::Install, "spr/"),
            Spf => (Root::Cd, "datas_a2/spf/"),
            Scenar => (Root::Install, "scenar/"),
            SprMenu => (Root::Install, "spr/menu/"),
            Loc => (Root::Install, "loc/"),
        }
    }

    /// The path of `name` in this folder, relative to its root.
    pub fn path(self, name: &str) -> String {
        format!("{}{}", self.location().1, name.replace('\\', "/"))
    }
}

/// The disc markers tested by @ 0x0040e1e0, in order: the last one found
/// (non-empty) gives the disc's number. Each is on its own disc only.
pub const DISC_MARKERS: [&str; 4] = [
    "datas_a2/spf/sphex.spr",
    "datas_a2/spf/cerceris.spr",
    "datas_a2/spf/bousier.spr",
    "datas_a2/spf/mante.spr",
];

/// The number (1..4) of the Atlantis II disc `src` holds, 0 when it is
/// none (@ 0x0040e1e0).
pub fn disc_number(src: &dyn FileSource) -> u8 {
    let mut n = 0;
    for (i, m) in DISC_MARKERS.iter().enumerate() {
        if src.size(m).is_some_and(|s| s > 0) {
            n = i as u8 + 1;
        }
    }
    n
}

/// The discs `src` stands for: every disc when it holds all four markers
/// (GOG's installation, the whole game in one folder), else the one of
/// [`disc_number`] (none when it is no disc). The original's own rule
/// (the last marker wins) would make GOG's folder disc 4 alone; GOG's
/// executable is patched instead to take its install folder for every
/// disc.
pub fn disc_numbers_of(src: &dyn FileSource) -> Vec<u8> {
    if DISC_MARKERS
        .iter()
        .all(|m| src.size(m).is_some_and(|s| s > 0))
    {
        return vec![1, 2, 3, 4];
    }
    match disc_number(src) {
        0 => Vec::new(),
        n => vec![n],
    }
}

/// `c` without its accent (Latin-1 letters), lower case.
fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        c => c.to_ascii_lowercase(),
    }
}

/// A file name with its accents folded, lower case
/// (`TR-D-tête araignée.tga` → `tr-d-tete araignee.tga`).
pub fn fold_accents(name: &str) -> String {
    name.chars().map(fold_char).collect()
}

/// A name's ISO 9660 level-1 form, as the mastering of GOG's data made
/// its 8.3 copies: every character but a letter or a digit becomes `_`
/// (an accented letter too), the name is cut to 8 characters and the
/// extension to 3, upper case (`tr-b-tete araignée.tga` →
/// `TR_B_TET.TGA`). Names that collide are numbered by the mastering
/// (`TR_B_TO1.TGA`), which this does not guess.
pub fn iso_level1_name(name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) => (&name[..i], Some(&name[i + 1..])),
        None => (name, None),
    };
    let map = |s: &str, n: usize| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .take(n)
            .collect()
    };
    match ext {
        Some(e) => format!("{}.{}", map(stem, 8), map(e, 3)),
        None => map(stem, 8),
    }
}

/// The name in `entries` that stands for `name` when no entry matches it
/// without regard to case: the same name with its accents folded, else
/// its ISO 9660 level-1 form; `None` when neither is there.
pub fn alias_of<'a>(name: &str, entries: &'a [String]) -> Option<&'a str> {
    let folded = fold_accents(name);
    if let Some(e) = entries.iter().find(|e| fold_accents(e) == folded) {
        return Some(e);
    }
    let iso = iso_level1_name(name);
    entries
        .iter()
        .find(|e| e.eq_ignore_ascii_case(&iso))
        .map(String::as_str)
}

/// The executable of an install folder: `atlantis2.exe` (the CD's, or the
/// 1.01 patch's) or GOG's `Game.exe`, the first that is a known build. GOG's
/// `atlantis2.exe` launcher is passed over; an executable of an unknown
/// build is reported (the first one met, the launcher aside).
pub fn find_exe(
    install: &dyn FileSource,
) -> Result<(&'static str, &'static Build, Vec<u8>), OpenError> {
    let mut unknown = None;
    for name in EXE_NAMES {
        let Some(data) = install.read(name) else {
            continue;
        };
        match identify(&data) {
            Ok(build) => return Ok((name, build, data)),
            Err(error) => {
                let launcher = name == EXE_NAMES[0] && data.len() as u64 == LAUNCHER_SIZE;
                if launcher {
                    log::debug!("{name}: GOG's launcher, not the game");
                } else if unknown.is_none() {
                    unknown = Some(OpenError::Exe { name, error });
                }
            }
        }
    }
    Err(unknown.unwrap_or(OpenError::NoExecutable))
}

/// One disc: a folder copy or an image.
struct Disc {
    number: u8,
    source: Arc<dyn FileSource>,
    origin: String,
}

/// The game's files: the install folder and the discs found.
pub struct A2Files {
    install: Arc<dyn FileSource>,
    /// One per disc number, in number order.
    discs: Vec<Disc>,
    /// The current disc (@ 0x00656a88), 0 when there is none.
    current: u8,
    /// The folders the discs were looked for in ([`A2Files::open_dir`]),
    /// looked in again by [`A2Files::refresh_discs`].
    roots: Vec<PathBuf>,
    /// The executable's name in the install folder and its build, when
    /// opened by [`A2Files::open_dir`].
    exe: Option<(&'static str, &'static Build)>,
    /// The names found by [`alias_of`], by source (0 the install folder,
    /// else the disc) and path: `None` when there is none.
    aliases: Mutex<HashMap<(u8, String), Option<String>>>,
}

/// Why a game folder does not open.
#[derive(Debug)]
pub enum OpenError {
    /// No `atlantis2.exe` (or GOG's `Game.exe`) in the folder.
    NoExecutable,
    /// An executable that is no known build: its name and why.
    Exe { name: &'static str, error: ExeError },
    /// No Atlantis II disc (image or copy) was found.
    NoDisc,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NoExecutable => write!(
                f,
                "atlantis2.exe (or GOG's Game.exe) not found in the game folder"
            ),
            OpenError::Exe { name, error } => write!(f, "{name}: {error}"),
            OpenError::NoDisc => write!(
                f,
                "no Atlantis II disc found: put the CD images (or copies of the CDs) in the game folder or a folder inside it"
            ),
        }
    }
}

impl std::error::Error for OpenError {}

impl A2Files {
    /// The files of an install folder and of the disc candidates (folder
    /// copies or images, each with a name for the log). Candidates that
    /// are no Atlantis II disc are dropped; of two copies of one disc the
    /// first is kept. The current disc starts as the lowest-numbered one:
    /// the original starts on the first drive holding any disc
    /// (@ 0x0040e0e0).
    pub fn new(
        install: Arc<dyn FileSource>,
        candidates: impl IntoIterator<Item = (String, Arc<dyn FileSource>)>,
    ) -> A2Files {
        let mut discs: Vec<Disc> = Vec::new();
        for (origin, source) in candidates {
            add_discs(&mut discs, origin, source);
        }
        discs.sort_by_key(|d| d.number);
        let current = discs.first().map_or(0, |d| d.number);
        A2Files {
            install,
            discs,
            current,
            roots: Vec::new(),
            exe: None,
            aliases: Mutex::new(HashMap::new()),
        }
    }

    /// Opens a game folder: the executable ([`find_exe`]) and the install
    /// files in `dir`, the discs in `dir`, in the folders inside it and in
    /// `disc_dirs` (see [`disc_candidates`]).
    pub fn open_dir(
        dir: &Path,
        disc_dirs: &[PathBuf],
    ) -> Result<(A2Files, &'static Build), OpenError> {
        let install: Arc<dyn FileSource> = Arc::new(StdDir::new(dir));
        let (exe_name, build, _) = find_exe(install.as_ref())?;
        let roots: Vec<PathBuf> = std::iter::once(dir.to_path_buf())
            .chain(disc_dirs.iter().cloned())
            .collect();
        let candidates = roots.iter().flat_map(|d| disc_candidates(d));
        let mut files = A2Files::new(install, candidates);
        if files.discs.is_empty() {
            return Err(OpenError::NoDisc);
        }
        files.roots = roots;
        files.exe = Some((exe_name, build));
        Ok((files, build))
    }

    /// The build of the executable ([`A2Files::open_dir`]); the reference
    /// (the French CD's) for files put together otherwise (tests).
    pub fn build(&self) -> &'static Build {
        self.exe.map_or(&KNOWN_BUILDS[0], |(_, b)| b)
    }

    /// The language of the texts and voices installed, told by
    /// `loc\Menu.txt` ([`crate::edition`]); `None` when that file is no
    /// known one.
    pub fn language(&self) -> Option<Language> {
        self.read(Folder::Loc, "Menu.txt")
            .and_then(|d| language_of_menu_txt(&d))
    }

    /// The edition's name: the build's and the language
    /// ([`edition_label`]).
    pub fn label(&self) -> String {
        edition_label(self.build(), self.language())
    }

    /// The executable's file name in the install folder (`atlantis2.exe`,
    /// GOG's `Game.exe`), when opened by [`A2Files::open_dir`].
    pub fn exe_name(&self) -> Option<&'static str> {
        self.exe.map(|(n, _)| n)
    }

    /// Sets the executable's name and build (files not opened by
    /// [`A2Files::open_dir`]: tests).
    pub fn set_exe(&mut self, name: &'static str, build: &'static Build) {
        self.exe = Some((name, build));
    }

    /// The executable's bytes (the tables the port reads from it), `None`
    /// when there is none.
    pub fn read_exe(&self) -> Option<Vec<u8>> {
        let name = self.exe.map_or(EXE_NAMES[0], |(n, _)| n);
        self.install.read(name)
    }

    /// The folders of [`A2Files::open_dir`] looked in again for discs
    /// that were not there (a disc image added while the game asks for
    /// it, the CD page's poll FUN_0040e090). The discs already found are
    /// kept. True when a disc was added.
    pub fn refresh_discs(&mut self) -> bool {
        let mut added = false;
        for root in self.roots.clone() {
            for (origin, source) in disc_candidates(&root) {
                let before = self.discs.len();
                add_discs(&mut self.discs, origin, source);
                added |= self.discs.len() > before;
            }
        }
        self.discs.sort_by_key(|d| d.number);
        added
    }

    /// The install folder.
    pub fn install(&self) -> &dyn FileSource {
        self.install.as_ref()
    }

    /// The numbers of the discs found, in order.
    pub fn disc_numbers(&self) -> Vec<u8> {
        self.discs.iter().map(|d| d.number).collect()
    }

    /// Where disc `n` was found (an image or a folder), for the log.
    pub fn disc_origin(&self, n: u8) -> Option<&str> {
        self.discs
            .iter()
            .find(|d| d.number == n)
            .map(|d| d.origin.as_str())
    }

    /// The current disc, 0 when none was found.
    pub fn current_disc(&self) -> u8 {
        self.current
    }

    /// `/set(monde=N)` (@ 0x00484df0): the disc of world N becomes the
    /// current one. 1..4 (Ireland, Yucatán, China, Shamballa) are discs
    /// 1..4; 6 (the Yucatán texts of the last disc) is disc 4; any other
    /// value leaves the disc unchanged. 5 (Tibet) takes the disc of the
    /// first drive holding one (@ 0x0040e0e0), whatever its number: with
    /// one drive, the disc already in it, so the current disc stays (after
    /// Shamballa, film 62's track is read from disc 4). Returns the disc
    /// wanted, which stays the current one even when it was not found (its
    /// files then come from the others).
    pub fn set_monde(&mut self, monde: i32) -> u8 {
        let wanted = match monde {
            1..=4 => monde as u8,
            6 => 4,
            _ => self.current,
        };
        if wanted != self.current {
            log::info!("MONDE {monde}: disc {wanted}");
        }
        self.current = wanted;
        wanted
    }

    /// `CHANGECD` (FUN_0040dff0 with the value − 1, atlantis2-cd-fr-2199552
    /// @ 0x0048275c): disc `n` becomes the wanted, current one, found or
    /// not (its files then come from the others), as [`A2Files::set_monde`]
    /// does for a world.
    pub fn set_disc(&mut self, n: u8) {
        if n != self.current {
            log::info!("CHANGECD: disc {n}");
        }
        self.current = n;
    }

    /// The path under which `src` (the install folder, 0, or disc `key`)
    /// holds `path`: itself, or the name [`alias_of`] finds in its folder
    /// (looked for once per path); `None` when it holds neither.
    fn present(&self, key: u8, src: &dyn FileSource, path: &str) -> Option<String> {
        if src.size(path).is_some() {
            return Some(path.to_string());
        }
        let mut cache = self.aliases.lock().unwrap();
        cache
            .entry((key, path.to_string()))
            .or_insert_with(|| {
                let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
                let entries = src.list(dir);
                let alias = alias_of(name, &entries)?;
                let full = if dir.is_empty() {
                    alias.to_string()
                } else {
                    format!("{dir}/{alias}")
                };
                src.size(&full)?;
                log::info!("{path}: read as {full}");
                Some(full)
            })
            .clone()
    }

    /// The source and path of a file: install files from the install
    /// folder; CD files from the current disc, else from the first other
    /// disc holding it.
    fn locate(&self, folder: Folder, name: &str) -> Option<(&dyn FileSource, String, u8)> {
        let path = folder.path(name);
        match folder.location().0 {
            Root::Install => {
                let p = self
                    .present(0, self.install.as_ref(), &path)
                    .unwrap_or(path);
                Some((self.install.as_ref(), p, 0))
            }
            Root::Cd => {
                let current = self.discs.iter().find(|d| d.number == self.current);
                if let Some(d) = current {
                    if let Some(p) = self.present(d.number, d.source.as_ref(), &path) {
                        return Some((d.source.as_ref(), p, d.number));
                    }
                }
                let (other, p) = self.discs.iter().find_map(|d| {
                    if d.number == self.current {
                        return None;
                    }
                    self.present(d.number, d.source.as_ref(), &path)
                        .map(|p| (d, p))
                })?;
                log::debug!(
                    "{path}: not on disc {}, read from disc {}",
                    self.current,
                    other.number
                );
                Some((other.source.as_ref(), p, other.number))
            }
        }
    }

    /// The size of a file, `None` when no source has it.
    pub fn size(&self, folder: Folder, name: &str) -> Option<u64> {
        let (src, path, _) = self.locate(folder, name)?;
        src.size(&path)
    }

    /// A whole file.
    pub fn read(&self, folder: Folder, name: &str) -> Option<Vec<u8>> {
        self.read_with_disc(folder, name).map(|(d, _)| d)
    }

    /// A whole file and the disc it came from (0 for the install folder).
    pub fn read_with_disc(&self, folder: Folder, name: &str) -> Option<(Vec<u8>, u8)> {
        let (src, path, disc) = self.locate(folder, name)?;
        src.read(&path).map(|d| (d, disc))
    }

    /// The size of a CD file on disc `n` only.
    pub fn size_on_disc(&self, n: u8, folder: Folder, name: &str) -> Option<u64> {
        let disc = self.discs.iter().find(|d| d.number == n)?;
        let p = self.present(n, disc.source.as_ref(), &folder.path(name))?;
        disc.source.size(&p)
    }

    /// A file as the original opens it: a CD file from the current disc
    /// only, `None` when that disc does not have it even if another one
    /// does (the original builds the path on the root of the CD in the
    /// drive). When the current disc was not found the others are read,
    /// as [`A2Files::read`] does (the original would ask for the disc).
    pub fn read_current(&self, folder: Folder, name: &str) -> Option<Vec<u8>> {
        if folder.location().0 == Root::Cd {
            if let Some(d) = self.discs.iter().find(|d| d.number == self.current) {
                let p = self.present(d.number, d.source.as_ref(), &folder.path(name))?;
                return d.source.read(&p);
            }
        }
        self.read(folder, name)
    }

    /// A CD file from disc `n` only.
    pub fn read_from_disc(&self, n: u8, folder: Folder, name: &str) -> Option<Vec<u8>> {
        let disc = self.discs.iter().find(|d| d.number == n)?;
        let p = self.present(n, disc.source.as_ref(), &folder.path(name))?;
        disc.source.read(&p)
    }
}

/// The discs of a candidate added to `discs` (see [`disc_numbers_of`]),
/// those already found left as they are.
fn add_discs(discs: &mut Vec<Disc>, origin: String, source: Arc<dyn FileSource>) {
    let numbers = disc_numbers_of(source.as_ref());
    if numbers.is_empty() {
        log::debug!("{origin}: not an Atlantis II disc");
        return;
    }
    if numbers.len() > 1 {
        log::info!("{origin}: every disc's marker, the whole game (discs {numbers:?})");
    }
    for number in numbers {
        if discs.iter().any(|d| d.number == number) {
            log::debug!("{origin}: disc {number} again, not used");
            continue;
        }
        log::info!("disc {number}: {origin}");
        discs.push(Disc {
            number,
            source: source.clone(),
            origin: origin.clone(),
        });
    }
}

/// Whether `dir` holds Atlantis II: an `atlantis2.exe` (any case; GOG's
/// folder holds its launcher under that name), or GOG's `Game.exe` beside
/// the game's `wam` and `scenar` folders.
pub fn holds_atlantis2(dir: &Path) -> bool {
    let d = StdDir::new(dir);
    d.size(EXE_NAMES[0]).is_some()
        || (d.size(EXE_NAMES[1]).is_some() && d.has_dir("wam") && d.has_dir("scenar"))
}

/// The disc candidates of a folder: the folder itself when it holds a
/// `datas_a2` folder (a copy of a CD), the CD images in it (`.iso`,
/// `.bin`, `.img`, `.mdf`, `.nrg`), and the same in each folder directly
/// inside it; in name order.
pub fn disc_candidates(dir: &Path) -> Vec<(String, Arc<dyn FileSource>)> {
    fn one_level(
        dir: &Path,
        out: &mut Vec<(String, Arc<dyn FileSource>)>,
        subdirs: &mut Vec<PathBuf>,
    ) {
        let copy = StdDir::new(dir);
        if copy.has_dir("datas_a2") {
            out.push((dir.display().to_string(), Arc::new(copy)));
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                subdirs.push(p);
            } else if p.extension().is_some_and(|e| {
                IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
            }) {
                if let Some(img) = DiscImage::open(&p) {
                    out.push((p.display().to_string(), Arc::new(img)));
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut subdirs = Vec::new();
    one_level(dir, &mut out, &mut subdirs);
    for s in subdirs {
        // `datas_a2` itself is a disc's content, not a disc.
        if s.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("datas_a2"))
        {
            continue;
        }
        one_level(&s, &mut out, &mut Vec::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::MemFiles;

    fn disc(n: usize, extra: &[(&str, &[u8])]) -> Arc<dyn FileSource> {
        let mut m = MemFiles::new();
        m.insert(DISC_MARKERS[n - 1], b"spr".to_vec());
        for (name, data) in extra {
            m.insert(name, data.to_vec());
        }
        Arc::new(m)
    }

    #[test]
    fn markers_and_the_monde_rule() {
        let mut install = MemFiles::new();
        install.insert("wam/tibet.wam", b"wam".to_vec());
        let files = A2Files::new(
            Arc::new(install),
            vec![
                (
                    "x".to_string(),
                    Arc::new(MemFiles::new()) as Arc<dyn FileSource>,
                ),
                (
                    "d3".to_string(),
                    disc(3, &[("datas_a2/hnm/a.hnm", b"three")]),
                ),
                ("d2".to_string(), disc(2, &[("datas_a2/hnm/a.hnm", b"two")])),
                ("d2 again".to_string(), disc(2, &[])),
                (
                    "d4".to_string(),
                    disc(4, &[("DATAS_A2/Warp/b.HNM", b"four")]),
                ),
            ],
        );
        let mut files = files;
        assert_eq!(files.disc_numbers(), vec![2, 3, 4]);
        assert_eq!(files.disc_origin(2), Some("d2"));
        // Start: the lowest disc found.
        assert_eq!(files.current_disc(), 2);
        assert_eq!(
            files.read(Folder::Wam, "TIBET.WAM").as_deref(),
            Some(&b"wam"[..])
        );
        assert_eq!(
            files.read(Folder::Hnm, "A.hnm").as_deref(),
            Some(&b"two"[..])
        );
        assert_eq!(files.set_monde(3), 3);
        assert_eq!(
            files.read_with_disc(Folder::Hnm, "a.hnm"),
            Some((b"three".to_vec(), 3))
        );
        // Not on the current disc: from another one.
        assert_eq!(
            files.read_with_disc(Folder::Warp, "b.hnm"),
            Some((b"four".to_vec(), 4))
        );
        // Tibet: the disc in the drive; 6: disc 4; others: unchanged.
        assert_eq!(files.set_monde(5), 3);
        assert_eq!(files.set_monde(6), 4);
        assert_eq!(files.set_monde(5), 4);
        assert_eq!(files.set_monde(0), 4);
        assert_eq!(files.set_monde(7), 4);
        // A disc that is not there stays wanted; files come from the others.
        assert_eq!(files.set_monde(1), 1);
        assert_eq!(
            files.read_with_disc(Folder::Hnm, "a.hnm"),
            Some((b"two".to_vec(), 2))
        );
        assert_eq!(
            files.read_from_disc(3, Folder::Hnm, "a.hnm").as_deref(),
            Some(&b"three"[..])
        );
        assert_eq!(files.size(Folder::Spw, "none.spw"), None);
    }

    #[test]
    fn last_marker_wins() {
        let mut m = MemFiles::new();
        m.insert(DISC_MARKERS[0], b"x".to_vec());
        m.insert(DISC_MARKERS[2], b"x".to_vec());
        // An empty marker does not count.
        m.insert(DISC_MARKERS[3], Vec::new());
        assert_eq!(disc_number(&m), 3);
        assert_eq!(disc_number(&MemFiles::new()), 0);
    }

    #[test]
    fn every_marker_is_every_disc() {
        let mut gog = MemFiles::new();
        for m in DISC_MARKERS {
            gog.insert(&m.to_ascii_uppercase(), b"spr".to_vec());
        }
        gog.insert("DATAS_A2/APC/MUSIC/MUZIK01.APC", b"music".to_vec());
        assert_eq!(disc_numbers_of(&gog), vec![1, 2, 3, 4]);
        let mut files = A2Files::new(
            Arc::new(MemFiles::new()),
            vec![("gog".to_string(), Arc::new(gog) as Arc<dyn FileSource>)],
        );
        assert_eq!(files.disc_numbers(), vec![1, 2, 3, 4]);
        assert_eq!(files.current_disc(), 1);
        for monde in [1, 2, 3, 4, 6] {
            let disc = files.set_monde(monde);
            assert_eq!(
                files.size_on_disc(disc, Folder::ApcMusic, "muzik01.apc"),
                Some(5)
            );
            assert!(files
                .read_current(Folder::ApcMusic, "Muzik01.apc")
                .is_some());
        }
        // Three markers: the last one wins, as on a disc.
        let mut three = MemFiles::new();
        for m in &DISC_MARKERS[..3] {
            three.insert(m, b"x".to_vec());
        }
        assert_eq!(disc_numbers_of(&three), vec![3]);
    }

    #[test]
    fn names_with_accents() {
        assert_eq!(
            fold_accents("TR-D-t\u{ea}te araign\u{e9}e.tga"),
            "tr-d-tete araignee.tga"
        );
        assert_eq!(
            iso_level1_name("tr-b-tete araign\u{e9}e.tga"),
            "TR_B_TET.TGA"
        );
        assert_eq!(
            iso_level1_name("TR-D-t\u{ea}te araign\u{e9}e.tga"),
            "TR_D_T_T.TGA"
        );
        assert_eq!(iso_level1_name("1aa1509509.SPW"), "1AA15095.SPW");
        let mut d = MemFiles::new();
        for m in DISC_MARKERS {
            d.insert(m, b"x".to_vec());
        }
        d.insert(
            "datas_a2/3d/pz_araig/TR-D-tete araignee.tga",
            b"web".to_vec(),
        );
        d.insert("datas_a2/3d/pz_araig/TR_B_TET.TGA", b"iso".to_vec());
        let files = A2Files::new(
            Arc::new(MemFiles::new()),
            vec![("gog".to_string(), Arc::new(d) as Arc<dyn FileSource>)],
        );
        assert_eq!(
            files
                .read(Folder::ThreeD, "pz_araig/TR-D-t\u{ea}te araign\u{e9}e.tga")
                .as_deref(),
            Some(&b"web"[..])
        );
        assert_eq!(
            files
                .read(Folder::ThreeD, "pz_araig/tr-b-tete Araign\u{e9}e.TGA")
                .as_deref(),
            Some(&b"iso"[..])
        );
        // Asked again: from the cache.
        assert_eq!(
            files.size(Folder::ThreeD, "pz_araig/TR-D-t\u{ea}te araign\u{e9}e.tga"),
            Some(3)
        );
        assert_eq!(files.size(Folder::ThreeD, "pz_araig/none.tga"), None);
    }

    #[test]
    fn the_executable_under_its_names() {
        // GOG: the launcher under atlantis2.exe, the game as Game.exe.
        let mut gog = MemFiles::new();
        let mut launcher = vec![0u8; LAUNCHER_SIZE as usize];
        launcher[..2].copy_from_slice(b"MZ");
        launcher[0x3c] = 0x80;
        launcher[0x80..0x84].copy_from_slice(b"PE\0\0");
        launcher[0x88..0x8c].copy_from_slice(&0x4c19_e4d8u32.to_le_bytes());
        gog.insert("atlantis2.exe", launcher.clone());
        assert!(matches!(find_exe(&gog), Err(OpenError::NoExecutable)));
        gog.insert("Game.exe", b"MZ".to_vec());
        match find_exe(&gog) {
            Err(OpenError::Exe { name, error }) => {
                assert_eq!((name, error), ("Game.exe", ExeError::NotPe));
            }
            other => panic!("{:?}", other.map(|(n, b, _)| (n, b.id))),
        }
        // Another program named atlantis2.exe is reported.
        let mut odd = MemFiles::new();
        odd.insert("atlantis2.exe", b"MZ".to_vec());
        assert!(matches!(
            find_exe(&odd),
            Err(OpenError::Exe {
                name: "atlantis2.exe",
                ..
            })
        ));
    }

    #[test]
    fn folder_paths() {
        assert_eq!(Folder::Warp.path("ti0002.hnm"), "datas_a2/warp/ti0002.hnm");
        assert_eq!(
            Folder::ThreeDDial.path("x\\y.db"),
            "datas_a2/3d/dial/x/y.db"
        );
        assert_eq!(Folder::SprMenu.location(), (Root::Install, "spr/menu/"));
    }
}
