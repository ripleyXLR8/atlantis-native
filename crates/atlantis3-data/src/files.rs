//! The installed game: a folder holding `Game.exe` (or the retail CD's
//! `Atlantis3.exe`) and `Datas\`, whose file names the game writes in any
//! case (`Datas\spr\mouse.SPR`).
//!
//! The GOG.com release ships every file unpacked in its folder. The
//! executable also knows three CDs (`Atlantis3.cd1` .. `cd3` markers,
//! @ 0x00718640) and a DVD layout (`Atlantis512.dvd`, `Atlantis1024.dvd`,
//! @ 0x0071b76c): the French retail CD (2001) installs part of `Datas\`
//! and reads the rest from its three discs ([`crate::discs`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlantis_data::{FileSource, StdDir};

use crate::exe::{self, Build, ExeError};

/// The folder of the game's data inside the install.
pub const DATAS: &str = "Datas";

/// An installed game.
#[derive(Clone)]
pub struct A3Files {
    src: Arc<dyn FileSource>,
    /// *Atlantis Evolution*'s 2004 program's data: the source that routes
    /// the names to the discs.
    evolution_2004: Option<Arc<crate::evolution2004::Evolution2004Source>>,
    /// The retail CD's install and discs: the source that follows the
    /// disc tests.
    cd: Option<Arc<crate::discs::CdSource>>,
}

/// Why a folder is not a playable install.
#[derive(Debug)]
pub enum OpenError {
    /// No `Game.exe` (nor the CD's `Atlantis3.exe`).
    NoExe,
    /// No `Datas\` folder.
    NoDatas,
    /// `Game.exe` is not a known build.
    Exe(ExeError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NoExe => write!(f, "no Game.exe or Atlantis3.exe in the folder"),
            OpenError::NoDatas => write!(f, "no Datas folder in the folder"),
            OpenError::Exe(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for OpenError {}

/// Whether `dir` looks like an *Atlantis III* install (`Game.exe` or
/// `Atlantis3.exe`, and `Datas\scenar\defnames.con`).
pub fn holds_atlantis3(dir: &Path) -> bool {
    let src = StdDir::new(dir);
    exe::EXE_NAMES.iter().any(|n| src.size(n).is_some())
        && src.size("Datas/scenar/defnames.con").is_some()
}

impl A3Files {
    /// An install read from `src`, unchecked.
    pub fn new(src: Arc<dyn FileSource>) -> A3Files {
        A3Files {
            src,
            evolution_2004: None,
            cd: None,
        }
    }

    /// The retail CD's install and discs, read through `src`.
    pub fn cd(src: Arc<crate::discs::CdSource>) -> A3Files {
        A3Files {
            src: src.clone(),
            evolution_2004: None,
            cd: Some(src),
        }
    }

    /// The install `install` (the folder `dir`): GOG.com's as it is; the
    /// retail CD's with its discs, looked for in `dir`, the folders inside
    /// it and `disc_dirs` ([`crate::discs::CdSource::open`]).
    pub fn with_discs(install: Arc<dyn FileSource>, dir: &Path, disc_dirs: &[PathBuf]) -> A3Files {
        let files = A3Files::new(install.clone());
        match files.build() {
            Ok(b) if b.release.disc_test() => A3Files::cd(Arc::new(crate::discs::CdSource::open(
                install, dir, disc_dirs,
            ))),
            _ => files,
        }
    }

    /// The install `install`: GOG.com's as it is; the retail CD's with the
    /// disc candidates `candidates()` (copies or images, each with a name
    /// for the log, [`crate::discs::CdSource::new`]), asked for only for
    /// the CD's program. The web page's way: it has the picked folders,
    /// not paths.
    pub fn with_disc_sources(
        install: Arc<dyn FileSource>,
        candidates: impl FnOnce() -> Vec<(String, Arc<dyn FileSource>)>,
    ) -> A3Files {
        let files = A3Files::new(install.clone());
        match files.build() {
            Ok(b) if b.release.disc_test() => {
                A3Files::cd(Arc::new(crate::discs::CdSource::new(install, candidates())))
            }
            _ => files,
        }
    }

    /// The retail CD's discs, when the install is the CD's.
    pub fn discs(&self) -> Option<&crate::discs::CdSource> {
        self.cd.as_deref()
    }

    /// `Menu_TestChangeCD(mask)` of the CD's program
    /// ([`crate::discs::CdSource::change_cd`]); nothing for another
    /// install.
    pub fn change_cd(&self, mask: i32) {
        if let Some(c) = &self.cd {
            c.change_cd(mask);
        }
    }

    /// *Atlantis Evolution*'s 2004 data, read through `src`.
    pub fn evolution_2004(src: Arc<crate::evolution2004::Evolution2004Source>) -> A3Files {
        A3Files {
            src: src.clone(),
            evolution_2004: Some(src),
            cd: None,
        }
    }

    /// Whether the data are *Atlantis Evolution*'s 2004 program's.
    pub fn is_evolution_2004(&self) -> bool {
        self.evolution_2004.is_some()
    }

    /// The 2004 program's current disc (`cd<n>\`, 0x0070d1a0) := `n`
    /// (`Menu_TestChangeCD` @ 0x0045d580, an `FLA` install); nothing for
    /// *Atlantis III*.
    pub fn set_disc(&self, n: u32) {
        if let Some(e) = &self.evolution_2004 {
            e.set_disc(n);
        }
    }

    /// The install at `dir` (with the CD's discs in it or in a folder
    /// inside it), with its executable identified.
    pub fn open_dir(dir: &Path) -> Result<(A3Files, &'static Build), OpenError> {
        let files = A3Files::with_discs(Arc::new(StdDir::new(dir)), dir, &[]);
        let build = files.build()?;
        Ok((files, build))
    }

    /// The build of the executable: `Game.exe`, else `Atlantis3.exe`
    /// ([`exe::EXE_NAMES`]); GOG.com's launcher `Atlantis3.exe`
    /// ([`exe::LAUNCHER_SIZE`]) is not looked at.
    pub fn build(&self) -> Result<&'static Build, OpenError> {
        if !self.src.has_dir(DATAS) {
            return Err(OpenError::NoDatas);
        }
        let mut err = None;
        for name in exe::EXE_NAMES {
            match self.src.size(name) {
                None | Some(exe::LAUNCHER_SIZE) => continue,
                Some(_) => {}
            }
            let Some(exe) = self.src.read(name) else {
                continue;
            };
            match exe::identify(&exe) {
                Ok(b) => return Ok(b),
                Err(e) => err = err.or(Some(e)),
            }
        }
        Err(err.map_or(OpenError::NoExe, OpenError::Exe))
    }

    /// The edition's name with the install's language ("Atlantis III,
    /// GOG.com, English"), when `Game.exe` is a known build.
    pub fn edition_label(&self) -> Result<String, OpenError> {
        let build = self.build()?;
        let (language, _) = crate::install::install_registry(self);
        Ok(crate::install::edition_label(
            build,
            crate::install::Language::from_code(&language),
        ))
    }

    /// The source of the whole install.
    pub fn source(&self) -> &dyn FileSource {
        &*self.src
    }

    /// The source, shared: for a file read by parts (the web build's
    /// cinematics), by its [`A3Files::datas_path`].
    pub fn shared_source(&self) -> Arc<dyn FileSource> {
        self.src.clone()
    }

    /// The path in the source of a file of `Datas\`.
    pub fn datas_path(&self, name: &str) -> String {
        format!("{DATAS}/{name}")
    }

    /// A file of `Datas\` (`name` with `/` or `\`, any case).
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.src.read(&format!("{DATAS}/{name}"))
    }

    /// The size of a file of `Datas\`.
    pub fn size(&self, name: &str) -> Option<u64> {
        self.src.size(&format!("{DATAS}/{name}"))
    }

    /// Whether folder `dir` of `Datas\` exists (any case).
    pub fn has_dir(&self, dir: &str) -> bool {
        self.src.has_dir(&format!("{DATAS}/{dir}"))
    }

    /// The names of the entries of folder `dir` of `Datas\`.
    pub fn list(&self, dir: &str) -> Vec<String> {
        self.src.list(&format!("{DATAS}/{dir}"))
    }
}
