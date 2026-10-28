//! The installed game: a folder holding `Game.exe` and `Datas\`, whose
//! file names the game writes in any case (`Datas\spr\mouse.SPR`).
//!
//! The GOG.com release ships every file unpacked in its folder; the
//! executable also knows three CDs (`Atlantis3.cd1` .. `cd3` markers,
//! @ 0x00718640) and a DVD layout (`Atlantis512.dvd`, `Atlantis1024.dvd`,
//! @ 0x0071b76c), which no edition seen yet uses.

use std::path::Path;
use std::sync::Arc;

use atlantis_data::{FileSource, StdDir};

use crate::exe::{self, Build, ExeError};

/// The folder of the game's data inside the install.
pub const DATAS: &str = "Datas";

/// An installed game.
#[derive(Clone)]
pub struct A3Files {
    src: Arc<dyn FileSource>,
}

/// Why a folder is not a playable install.
#[derive(Debug)]
pub enum OpenError {
    /// No `Game.exe`.
    NoExe,
    /// No `Datas\` folder.
    NoDatas,
    /// `Game.exe` is not a known build.
    Exe(ExeError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NoExe => write!(f, "no Game.exe in the folder"),
            OpenError::NoDatas => write!(f, "no Datas folder in the folder"),
            OpenError::Exe(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for OpenError {}

/// Whether `dir` looks like an *Atlantis III* install (`Game.exe` and
/// `Datas\scenar\defnames.con`).
pub fn holds_atlantis3(dir: &Path) -> bool {
    let src = StdDir::new(dir);
    src.size(exe::EXE_NAME).is_some() && src.size("Datas/scenar/defnames.con").is_some()
}

impl A3Files {
    /// An install read from `src`, unchecked.
    pub fn new(src: Arc<dyn FileSource>) -> A3Files {
        A3Files { src }
    }

    /// The install at `dir`, with its executable identified.
    pub fn open_dir(dir: &Path) -> Result<(A3Files, &'static Build), OpenError> {
        let files = A3Files::new(Arc::new(StdDir::new(dir)));
        let build = files.build()?;
        Ok((files, build))
    }

    /// The build of `Game.exe`.
    pub fn build(&self) -> Result<&'static Build, OpenError> {
        if !self.src.has_dir(DATAS) {
            return Err(OpenError::NoDatas);
        }
        let exe = self.src.read(exe::EXE_NAME).ok_or(OpenError::NoExe)?;
        exe::identify(&exe).map_err(OpenError::Exe)
    }

    /// The source of the whole install.
    pub fn source(&self) -> &dyn FileSource {
        &*self.src
    }

    /// A file of `Datas\` (`name` with `/` or `\`, any case).
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.src.read(&format!("{DATAS}/{name}"))
    }

    /// The size of a file of `Datas\`.
    pub fn size(&self, name: &str) -> Option<u64> {
        self.src.size(&format!("{DATAS}/{name}"))
    }

    /// The names of the entries of folder `dir` of `Datas\`.
    pub fn list(&self, dir: &str) -> Vec<String> {
        self.src.list(&format!("{DATAS}/{dir}"))
    }
}
