//! A game directory: the executable's tables and the archives mounted.

use std::sync::Arc;

use crate::big::BigArchive;
use crate::exe::ExeTables;
use crate::files::FileSource;

#[derive(Debug)]
pub enum GameError {
    NoExecutable,
    UnknownExecutable(u64),
    NoArchives,
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameError::NoExecutable => write!(f, "atlantis.exe not found in the game directory"),
            GameError::UnknownExecutable(n) => {
                write!(f, "atlantis.exe ({n} bytes) is not a known build")
            }
            GameError::NoArchives => write!(f, "BIGCD1.BIG not found and no unpacked game files"),
        }
    }
}

impl std::error::Error for GameError {}

pub struct Game {
    pub exe: ExeTables,
    pub big: BigArchive,
    /// The GOG release: files unpacked next to stub archives.
    pub unpacked: bool,
}

impl Game {
    pub fn open(src: Arc<dyn FileSource>) -> Result<Game, GameError> {
        let exe_bytes = src.read("atlantis.exe").ok_or(GameError::NoExecutable)?;
        let exe = ExeTables::read(&exe_bytes)
            .ok_or(GameError::UnknownExecutable(exe_bytes.len() as u64))?;
        // The GOG release has a SCENAR directory beside its stubs.
        let unpacked = src.has_dir("SCENAR");
        let mut big = BigArchive::new(src);
        let mut any = false;
        for n in 1..=4 {
            any |= big.mount(&format!("BIGCD{n}.BIG"));
        }
        if !any && !unpacked {
            return Err(GameError::NoArchives);
        }
        big.set_loose(unpacked);
        Ok(Game { exe, big, unpacked })
    }
}
