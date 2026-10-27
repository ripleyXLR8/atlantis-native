//! A game directory: the executable's tables and the archives mounted.

use std::sync::Arc;

use crate::big::BigArchive;
use crate::exe::ExeTables;
use crate::files::FileSource;
use crate::text::TextCodePage;

#[derive(Debug)]
pub enum GameError {
    NoExecutable,
    UnknownExecutable(u64),
    NoArchives,
    /// The CD release's archives (one per disc) that are not in the
    /// folder: their numbers, 1..4.
    MissingArchives(Vec<u8>),
    /// A language of the DVD whose side ('A' or 'B') is not the one given
    /// (the drive holds the other side, or only the other side's image is
    /// there).
    DvdOtherSide(char),
    /// A language of the DVD without its side's shared folders (`CYCLO`,
    /// `DIALOG`, `PUZZLES`, `UBB_VUE`, `WAM`): its folder copied alone.
    DvdSharedFolders,
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
            GameError::DvdSharedFolders => write!(
                f,
                "a language of the DVD without the side's CYCLO, DIALOG, PUZZLES, UBB_VUE and WAM"
            ),
            GameError::DvdOtherSide(side) => write!(
                f,
                "this version is on the other side of the DVD (side {side})"
            ),
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
        // The DVD's program reads the films and the panoramas from its
        // side's folder ([`crate::dvd::DvdFiles`]).
        if exe.build.dvd_side().is_some() && !src.has_dir("UBB_VUE") {
            return Err(GameError::DvdSharedFolders);
        }
        // The GOG release (and the DVD, the Perfect Edition) has a SCENAR
        // directory beside its stubs.
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
    /// "CD", "CD, patch 1.01", "GOG.com", "DVD" (Cryo's DVD-ROM of 1998)
    /// or "Perfect Edition" (Tgames' 2018 package of that DVD's English
    /// program).
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
    /// '#': "New Player" (English), "NOUVEAU JOUEUR" (French), "NEUER
    /// SPIELER" (German), "Nieuwe speler" (Dutch), "Ny spelare" (Swedish),
    /// "Jugador nuevo" (Spanish), "Nuovo Giocatore" (Italian).
    /// The 1.01 patch's executable is French ("#NOUVEAU JOUEUR") over
    /// whichever CD it was installed on: the language is its interface's.
    /// The Chinese builds keep the English row ("New Player", no '#'): their
    /// language is their texts' code page's. The 1998 DVD builds' rows have
    /// a '#' and the 1997 case: "#Nieuwe speler", "#Ny spelare",
    /// "#Jugador nuevo", "#Nuovo Giocatore".
    pub fn edition(&self) -> Edition {
        let name = self.exe.build.name;
        let marker = self.exe.new_player_marker.as_slice();
        // The row without its '#', in capitals ("#Nieuwe speler" of the
        // Dutch DVD, "NOUVEAU JOUEUR" of the French CD).
        let row = marker
            .strip_prefix(b"#")
            .unwrap_or(marker)
            .to_ascii_uppercase();
        let starts = |p: &[u8]| row.starts_with(p);
        let language = if self.exe.build.text_code_page == TextCodePage::Windows950 {
            Some("Traditional Chinese")
        } else if self.exe.build.text_code_page == TextCodePage::Windows936 {
            Some("Simplified Chinese")
        } else if starts(b"NOUVEAU") {
            Some("French")
        } else if starts(b"NEUE") {
            Some("German")
        } else if starts(b"NIEUWE") {
            Some("Dutch")
        } else if starts(b"NY SPELARE") {
            Some("Swedish")
        } else if starts(b"NUEVO") || starts(b"JUGADOR") {
            Some("Spanish")
        } else if starts(b"NUOVO") {
            Some("Italian")
        } else if starts(b"NEW") {
            Some("English")
        } else {
            None
        };
        Edition {
            release: if name.starts_with("atlantis-gog") {
                "GOG.com"
            } else if name.starts_with("atlantis-patch") {
                "CD, patch 1.01"
            } else if name.starts_with("atlantis-dvd") {
                "DVD"
            } else if name.starts_with("atlantis-pe") {
                "Perfect Edition"
            } else {
                "CD"
            },
            language,
            build_year: self.exe.build.year,
            build_month: self.exe.build.month,
        }
    }
}
