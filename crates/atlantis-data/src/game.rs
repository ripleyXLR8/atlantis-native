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
    /// The CD release's archives (one per disc) that are not in the
    /// folder: their numbers, 1..4.
    MissingArchives(Vec<u8>),
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameError::NoExecutable => write!(f, "atlantis.exe not found in the game directory"),
            GameError::UnknownExecutable(n) => {
                write!(f, "atlantis.exe ({n} bytes) is not a known build")
            }
            GameError::NoArchives => write!(f, "BIGCD1.BIG not found and no unpacked game files"),
            GameError::MissingArchives(n) => {
                let names: Vec<String> = n.iter().map(|n| format!("BIGCD{n}.BIG")).collect();
                write!(f, "{} not found", names.join(", "))
            }
        }
    }
}

impl std::error::Error for GameError {}

pub struct Game {
    pub exe: ExeTables,
    pub big: BigArchive,
    /// The GOG release: files unpacked next to stub archives.
    pub unpacked: bool,
    /// The executable's icon, for the window (from the player's copy).
    pub icon: Option<crate::exe_icon::Icon>,
}

impl Game {
    pub fn open(src: Arc<dyn FileSource>) -> Result<Game, GameError> {
        let exe_bytes = src.read("atlantis.exe").ok_or(GameError::NoExecutable)?;
        let icon = crate::exe_icon::exe_icon(&exe_bytes);
        let exe = ExeTables::read(&exe_bytes)
            .ok_or(GameError::UnknownExecutable(exe_bytes.len() as u64))?;
        // The GOG release has a SCENAR directory beside its stubs.
        let unpacked = src.has_dir("SCENAR");
        let mut big = BigArchive::new(src);
        let mut missing = Vec::new();
        for n in 1..=4u8 {
            if !big.mount(&format!("BIGCD{n}.BIG")) {
                missing.push(n);
            }
        }
        // The CD release needs its four archives (one per disc); the GOG
        // release reads its unpacked files.
        if !unpacked {
            if missing.len() == 4 {
                return Err(GameError::NoArchives);
            }
            if !missing.is_empty() {
                return Err(GameError::MissingArchives(missing));
            }
        }
        big.set_loose(unpacked);
        Ok(Game {
            exe,
            big,
            unpacked,
            icon,
        })
    }
}

/// Which release of the game a folder holds, for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edition {
    /// "CD", "CD, patch 1.01" or "GOG.com".
    pub release: &'static str,
    /// "English", "French", ... (`None` when not recognised).
    pub language: Option<&'static str>,
    /// The year the executable was built.
    pub build_year: u16,
    /// Its month, for the 1997 builds.
    pub build_month: Option<&'static str>,
}

impl std::fmt::Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.release)?;
        if let Some(l) = self.language {
            write!(f, ", {l}")?;
        }
        match self.build_month {
            Some(m) => write!(f, " ({m} {} build)", self.build_year),
            None => write!(f, " ({} build)", self.build_year),
        }
    }
}

impl Game {
    /// The release: the executable's build, and the language of the GOG
    /// build from its player-screen row (the five GOG languages share one
    /// executable; only this string differs). The 1997 builds' rows have no
    /// '#': "New Player" (English), "NOUVEAU JOUEUR" (French). The 1.01
    /// patch's executable is French ("#NOUVEAU JOUEUR") over whichever CD it
    /// was installed on: the language is its interface's.
    pub fn edition(&self) -> Edition {
        let name = self.exe.build.name;
        let marker = self.exe.new_player_marker.as_slice();
        let starts = |p: &[u8]| marker.len() >= p.len() && &marker[..p.len()] == p;
        let language = if starts(b"#NOUVEAU") || starts(b"NOUVEAU JOUEUR") {
            Some("French")
        } else if starts(b"#NEUE") {
            Some("German")
        } else if starts(b"#NUEVO") {
            Some("Spanish")
        } else if starts(b"#NUOVO") {
            Some("Italian")
        } else if starts(b"#NEW") || starts(b"#New") || starts(b"New Player") {
            Some("English")
        } else {
            None
        };
        Edition {
            release: if name.starts_with("atlantis-gog") {
                "GOG.com"
            } else if name.starts_with("atlantis-patch") {
                "CD, patch 1.01"
            } else {
                "CD"
            },
            language,
            build_year: self.exe.build.year,
            build_month: self.exe.build.month,
        }
    }
}
