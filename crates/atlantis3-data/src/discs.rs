//! The French retail CD's three discs (atlantis3-cd-fr-3379200, 2001).
//!
//! The CD's installer (`cd1\data1.cab`, InstallShield 5) copies the
//! program, `spr_p5.dll`, `spr_p6.dll` and 890 files of `Datas\` to the
//! hard disk (the intro, the menus, the scripts, the texts, the sounds,
//! the inter-dimension world) and writes `Install` = `CD` in the
//! registry; the rest of `Datas\` stays on the discs, each with its marker
//! file `Atlantis3.cd<n>` at the root (atlantis3-re `discs/cd-fr.md`):
//! disc 1 holds the Palaeolithic, disc 2 the land of the dead, disc 3
//! Baghdad and the inter-dimension's videos; all three a part of the
//! Hoggar.
//!
//! Every data file is opened by `AtlFile_Open` (atlantis3-cd-fr-3379200
//! @ 0x00418370, the same code in GOG's `Game.exe`): the install's
//! `Datas\` first, then `<disc root>datas\` + the same name, the disc root
//! being the drive the last disc test chose (@ 0x00764560, empty until
//! the first one). The disc test `Menu_TestChangeCD(mask)` (@ 0x004578d0)
//! runs on `CHANGECD=mask` (the scripts' 2, 4 and 7, the save loader's
//! `(changecd=<CurrentCD>)`): bit 0 asks for disc 1, bit 1 disc 2, bit 2
//! disc 3; of the discs found in the drives (@ 0x00442600, the markers
//! looked for on every CD drive) the lowest asked for becomes the current
//! one (@ 0x00457d5f..0x00457e00), else the disc request page loops until
//! one is inserted (`ChangeCDLoop` @ 0x004572d0).
//!
//! The port has every disc at hand. It plays as a player with one drive
//! who inserts the disc asked for: a mask that holds the current disc
//! keeps it, another mask takes its lowest disc; a mask of 0 (a save of
//! GOG's program, whose `CurrentCD` stays 0) keeps the current disc. A
//! file missing from the install and the current disc is read from
//! another disc that has it (the original would fail to open it: the
//! Palaeolithic, on disc 1 only, is entered from the inter-dimension
//! without a disc test), and logged. Of the 3317 files, six differ
//! between copies: `hnm\HNM640\ldm\cine010.hnm` (the install's wins) and
//! five `spw\SPW512\hoggar\*023.spw` of disc 2, read in the Hoggar after
//! `ChangeCD=2` (GOG.com ships discs 1 and 3's).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use atlantis_data::disc_image::{DiscImage, IMAGE_EXTENSIONS};
use atlantis_data::{FileSource, StdDir};

/// The discs' marker files (atlantis3-cd-fr-3379200 @ 0x00702530,
/// 0x00702520, 0x00702510), disc 1 to 3.
pub const DISC_MARKERS: [&str; 3] = ["Atlantis3.cd1", "Atlantis3.cd2", "Atlantis3.cd3"];

/// The number (1..3) of the disc `src` is (its root: a copy or an image),
/// 0 when it is none: its marker beside a `Datas` folder. A root with
/// several markers (GOG.com's Spanish install has all three) is no disc.
pub fn disc_number(src: &dyn FileSource) -> u8 {
    let found: Vec<u8> = (1..=3u8)
        .filter(|n| src.size(DISC_MARKERS[usize::from(*n) - 1]).is_some())
        .collect();
    match found.as_slice() {
        [n] if src.has_dir("Datas") => *n,
        _ => 0,
    }
}

/// The disc candidates of a folder: the copies (a folder holding a disc's
/// marker and `Datas\`) and the CD images (`.iso`, `.bin`, `.img`,
/// `.mdf`, `.nrg`) in it and in each folder directly inside it, in name
/// order, each with its path for the log. The folder itself is not one
/// (it is the install).
pub fn disc_candidates(dir: &Path) -> Vec<(String, Arc<dyn FileSource>)> {
    fn one(p: &Path, out: &mut Vec<(String, Arc<dyn FileSource>)>, subdirs: &mut Vec<PathBuf>) {
        if p.is_dir() {
            let copy = StdDir::new(p);
            if disc_number(&copy) != 0 {
                out.push((p.display().to_string(), Arc::new(copy)));
            } else {
                subdirs.push(p.to_path_buf());
            }
        } else if p.extension().is_some_and(|e| {
            IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
        }) {
            if let Some(img) = DiscImage::open(p) {
                if disc_number(&img) != 0 {
                    out.push((p.display().to_string(), Arc::new(img)));
                }
            }
        }
    }
    fn entries(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }
    let mut out = Vec::new();
    let mut subdirs = Vec::new();
    for p in entries(dir) {
        // `Datas` is the install's, not a disc.
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("datas"))
        {
            continue;
        }
        one(&p, &mut out, &mut subdirs);
    }
    for s in subdirs {
        for p in entries(&s) {
            one(&p, &mut out, &mut Vec::new());
        }
    }
    out
}

/// One disc found.
struct Disc {
    number: u8,
    source: Arc<dyn FileSource>,
    origin: String,
}

/// The install and the discs, read as the CD's program reads them.
pub struct CdSource {
    install: Arc<dyn FileSource>,
    discs: Vec<Disc>,
    /// The current disc (1..3), 0 before the first disc test.
    current: AtomicU8,
}

impl CdSource {
    /// The install `install` and the disc candidates (copies or images,
    /// each with a name for the log): those that are no disc are
    /// dropped, of two copies of a disc the first is kept.
    pub fn new(
        install: Arc<dyn FileSource>,
        candidates: impl IntoIterator<Item = (String, Arc<dyn FileSource>)>,
    ) -> CdSource {
        let mut discs: Vec<Disc> = Vec::new();
        for (origin, source) in candidates {
            let number = disc_number(&*source);
            if number == 0 {
                log::debug!("{origin}: no Atlantis III disc");
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
        CdSource {
            install,
            discs,
            current: AtomicU8::new(0),
        }
    }

    /// The install folder `dir` and the discs in it, in the folders
    /// inside it and in `disc_dirs` ([`disc_candidates`]).
    pub fn open(install: Arc<dyn FileSource>, dir: &Path, disc_dirs: &[PathBuf]) -> CdSource {
        let mut c = disc_candidates(dir);
        for d in disc_dirs {
            let copy = StdDir::new(d);
            if disc_number(&copy) != 0 {
                c.push((d.display().to_string(), Arc::new(copy)));
            } else {
                c.extend(disc_candidates(d));
            }
        }
        CdSource::new(install, c)
    }

    /// The numbers of the discs found.
    pub fn disc_numbers(&self) -> Vec<u8> {
        self.discs.iter().map(|d| d.number).collect()
    }

    /// Where disc `n` was found.
    pub fn disc_origin(&self, n: u8) -> Option<&str> {
        self.discs
            .iter()
            .find(|d| d.number == n)
            .map(|d| d.origin.as_str())
    }

    /// The current disc, 0 before the first disc test.
    pub fn current(&self) -> u8 {
        self.current.load(Ordering::Relaxed)
    }

    /// `Menu_TestChangeCD(mask)` (@ 0x004578d0) with one drive and the
    /// disc asked for inserted: the current disc when `mask` asks for it
    /// (or is 0), else the lowest disc `mask` asks for that was found.
    /// Returns the current disc.
    pub fn change_cd(&self, mask: i32) -> u8 {
        let cur = self.current();
        let asked = |n: u8| mask & (1 << (n - 1)) != 0;
        if mask == 0 || (cur != 0 && asked(cur)) {
            log::info!("Menu_TestChangeCD({mask}): disc {cur} stays");
            return cur;
        }
        match self.discs.iter().map(|d| d.number).find(|&n| asked(n)) {
            Some(n) => {
                log::info!("Menu_TestChangeCD({mask}): disc {n}");
                self.current.store(n, Ordering::Relaxed);
                n
            }
            None => {
                log::warn!("Menu_TestChangeCD({mask}): no such disc found; disc {cur} stays");
                cur
            }
        }
    }

    /// The disc that holds `name` (a path from the disc's root): the
    /// current one, else the lowest other one.
    fn disc_of(&self, name: &str) -> Option<&Disc> {
        let cur = self.current();
        if let Some(d) = self.discs.iter().find(|d| d.number == cur) {
            if d.source.size(name).is_some() {
                return Some(d);
            }
        }
        let d = self
            .discs
            .iter()
            .find(|d| d.number != cur && d.source.size(name).is_some())?;
        log::debug!("{name}: not on disc {cur}, read from disc {}", d.number);
        Some(d)
    }
}

/// Whether `name` is under `Datas\` (the only folder the discs serve).
fn in_datas(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n.get(..6).is_some_and(|d| d.eq_ignore_ascii_case("datas/"))
}

impl FileSource for CdSource {
    fn size(&self, name: &str) -> Option<u64> {
        self.install.size(name).or_else(|| {
            in_datas(name)
                .then(|| self.disc_of(name)?.source.size(name))
                .flatten()
        })
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if self.install.size(name).is_some() {
            return self.install.read_at(name, offset, len);
        }
        if !in_datas(name) {
            return None;
        }
        self.disc_of(name)?.source.read_at(name, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.install.has_dir(name)
            || (in_datas(name) && self.discs.iter().any(|d| d.source.has_dir(name)))
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let mut out = self.install.list(dir);
        if in_datas(&format!("{dir}/")) {
            for d in &self.discs {
                out.extend(d.source.list(dir));
            }
        }
        out.sort_by_key(|s| s.to_ascii_lowercase());
        out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        out
    }

    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.install.read_beside(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::MemFiles;

    fn disc(n: usize, files: &[(&str, &[u8])]) -> (String, Arc<dyn FileSource>) {
        let mut m = MemFiles::new();
        m.insert(DISC_MARKERS[n - 1], Vec::new());
        for (name, data) in files {
            m.insert(name, data.to_vec());
        }
        (format!("d{n}"), Arc::new(m))
    }

    #[test]
    fn markers_install_first_and_the_disc_test() {
        let mut install = MemFiles::new();
        install.insert("Datas/hnm/intro.hnm", b"intro".to_vec());
        install.insert("Datas/hnm/a.hnm", b"install".to_vec());
        let src = CdSource::new(
            Arc::new(install),
            vec![
                disc(
                    3,
                    &[("Datas/spw/x.spw", b"three"), ("Datas/bag/b.hnm", b"b")],
                ),
                disc(
                    2,
                    &[("Datas/spw/x.spw", b"two"), ("Datas/hnm/a.hnm", b"disc")],
                ),
                disc(2, &[]),
                disc(1, &[("Datas/paleo/p.spw", b"one")]),
                (
                    "none".into(),
                    Arc::new(MemFiles::new()) as Arc<dyn FileSource>,
                ),
            ],
        );
        assert_eq!(src.disc_numbers(), [1, 2, 3]);
        assert_eq!(src.disc_origin(2), Some("d2"));
        assert_eq!(src.current(), 0);
        // The install first, whatever the disc.
        assert_eq!(
            src.read("datas\\HNM\\a.hnm").as_deref(),
            Some(&b"install"[..])
        );
        // Before any test: the lowest disc that has it.
        assert_eq!(src.read("Datas/spw/x.spw").as_deref(), Some(&b"two"[..]));
        assert_eq!(src.change_cd(2), 2);
        assert_eq!(src.read("Datas/spw/x.spw").as_deref(), Some(&b"two"[..]));
        // 7 holds disc 2: it stays.
        assert_eq!(src.change_cd(7), 2);
        assert_eq!(src.change_cd(4), 3);
        assert_eq!(src.read("Datas/spw/x.spw").as_deref(), Some(&b"three"[..]));
        // A file of another disc only.
        assert_eq!(src.read("Datas/paleo/p.spw").as_deref(), Some(&b"one"[..]));
        assert_eq!(src.change_cd(0), 3);
        assert_eq!(src.change_cd(8), 3);
        // Outside Datas: the install only.
        assert_eq!(src.size("Atlantis3.cd1"), None);
        assert!(src.has_dir("Datas/bag") && !src.has_dir("bag"));
        assert_eq!(src.list("Datas/spw"), ["x.spw"]);
    }

    #[test]
    fn a_root_with_every_marker_is_no_disc() {
        let mut m = MemFiles::new();
        for mk in DISC_MARKERS {
            m.insert(mk, Vec::new());
        }
        m.insert("Datas/x", Vec::new());
        assert_eq!(disc_number(&m), 0);
        let (_, d) = disc(2, &[("Datas/x", b"")]);
        assert_eq!(disc_number(&*d), 2);
        let (_, d) = disc(2, &[]);
        assert_eq!(disc_number(&*d), 0, "no Datas");
    }
}
