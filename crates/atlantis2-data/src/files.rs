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
//! ([`A2Files::set_monde`]). A port with every disc at hand needs no disc
//! prompt; a file missing from the current disc is read from another disc
//! that has it (the original would fail to open it), and logged.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis_data::disc_image::{DiscImage, IMAGE_EXTENSIONS};
use atlantis_data::{FileSource, StdDir};

use crate::exe::{identify, Build, ExeError};

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
}

/// Why a game folder does not open.
#[derive(Debug)]
pub enum OpenError {
    /// No `atlantis2.exe` in the folder.
    NoExecutable,
    /// An `atlantis2.exe` that is no known build.
    Exe(ExeError),
    /// No Atlantis II disc (image or copy) was found.
    NoDisc,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NoExecutable => write!(f, "atlantis2.exe not found in the game folder"),
            OpenError::Exe(e) => write!(f, "{e}"),
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
            let number = disc_number(source.as_ref());
            if number == 0 {
                log::debug!("{origin}: not an Atlantis II disc");
                continue;
            }
            if discs.iter().any(|d| d.number == number) {
                log::debug!("{origin}: disc {number} again, not used");
                continue;
            }
            log::info!("disc {number}: {origin}");
            discs.push(Disc {
                number,
                source,
                origin,
            });
        }
        discs.sort_by_key(|d| d.number);
        let current = discs.first().map_or(0, |d| d.number);
        A2Files {
            install,
            discs,
            current,
            roots: Vec::new(),
        }
    }

    /// Opens a game folder: `atlantis2.exe` and the install files in `dir`,
    /// the discs in `dir`, in the folders inside it and in `disc_dirs`
    /// (see [`disc_candidates`]).
    pub fn open_dir(
        dir: &Path,
        disc_dirs: &[PathBuf],
    ) -> Result<(A2Files, &'static Build), OpenError> {
        let install: Arc<dyn FileSource> = Arc::new(StdDir::new(dir));
        let exe = install
            .read("atlantis2.exe")
            .ok_or(OpenError::NoExecutable)?;
        let build = identify(&exe).map_err(OpenError::Exe)?;
        let roots: Vec<PathBuf> = std::iter::once(dir.to_path_buf())
            .chain(disc_dirs.iter().cloned())
            .collect();
        let candidates = roots.iter().flat_map(|d| disc_candidates(d));
        let mut files = A2Files::new(install, candidates);
        if files.discs.is_empty() {
            return Err(OpenError::NoDisc);
        }
        files.roots = roots;
        Ok((files, build))
    }

    /// The folders of [`A2Files::open_dir`] looked in again for discs
    /// that were not there (a disc image added while the game asks for
    /// it, the CD page's poll FUN_0040e090). The discs already found are
    /// kept. True when a disc was added.
    pub fn refresh_discs(&mut self) -> bool {
        let mut added = false;
        for root in self.roots.clone() {
            for (origin, source) in disc_candidates(&root) {
                let number = disc_number(source.as_ref());
                if number == 0 || self.discs.iter().any(|d| d.number == number) {
                    continue;
                }
                log::info!("disc {number}: {origin}");
                self.discs.push(Disc {
                    number,
                    source,
                    origin,
                });
                added = true;
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
    /// 1..4; 5 (Tibet) is whichever disc is in the drive, here the
    /// lowest-numbered one found (the first drive holding a disc,
    /// @ 0x0040e0e0); 6 (the Yucatán texts of the last disc) is disc 4;
    /// any other value leaves the disc unchanged. Returns the disc wanted,
    /// which stays the current one even when it was not found (its files
    /// then come from the others).
    pub fn set_monde(&mut self, monde: i32) -> u8 {
        let wanted = match monde {
            1..=4 => monde as u8,
            5 => self.discs.first().map_or(self.current, |d| d.number),
            6 => 4,
            _ => self.current,
        };
        if wanted != self.current {
            log::info!("MONDE {monde}: disc {wanted}");
        }
        self.current = wanted;
        wanted
    }

    /// The source and path of a file: install files from the install
    /// folder; CD files from the current disc, else from the first other
    /// disc holding it.
    fn locate(&self, folder: Folder, name: &str) -> Option<(&dyn FileSource, String, u8)> {
        let path = folder.path(name);
        match folder.location().0 {
            Root::Install => Some((self.install.as_ref(), path, 0)),
            Root::Cd => {
                let current = self.discs.iter().find(|d| d.number == self.current);
                if let Some(d) = current {
                    if d.source.size(&path).is_some() {
                        return Some((d.source.as_ref(), path, d.number));
                    }
                }
                let other = self
                    .discs
                    .iter()
                    .find(|d| d.number != self.current && d.source.size(&path).is_some())?;
                log::debug!(
                    "{path}: not on disc {}, read from disc {}",
                    self.current,
                    other.number
                );
                Some((other.source.as_ref(), path, other.number))
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
        disc.source.size(&folder.path(name))
    }

    /// A CD file from disc `n` only.
    pub fn read_from_disc(&self, n: u8, folder: Folder, name: &str) -> Option<Vec<u8>> {
        let disc = self.discs.iter().find(|d| d.number == n)?;
        disc.source.read(&folder.path(name))
    }
}

/// Whether `dir` holds an `atlantis2.exe` (any case).
pub fn holds_atlantis2(dir: &Path) -> bool {
    StdDir::new(dir).size("atlantis2.exe").is_some()
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
        // Tibet: the first disc; 6: disc 4; others: unchanged.
        assert_eq!(files.set_monde(5), 2);
        assert_eq!(files.set_monde(6), 4);
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
    fn folder_paths() {
        assert_eq!(Folder::Warp.path("ti0002.hnm"), "datas_a2/warp/ti0002.hnm");
        assert_eq!(
            Folder::ThreeDDial.path("x\\y.db"),
            "datas_a2/3d/dial/x/y.db"
        );
        assert_eq!(Folder::SprMenu.location(), (Root::Install, "spr/menu/"));
    }
}
