//! The start-up's settings: `Atlantis3.cfg` over its defaults, then the
//! installation's language and medium, and the sizes of the screen and of
//! the warp view they give.
//!
//! The original's start-up (`Game.exe` @ 0x004d5e80) reads the file
//! (@ 0x00406ea0, `atlantis3_data::cfg`), then the registry key
//! `HKLM\SOFTWARE\CRYO\ATLANTIS 3`: `Install` becomes `Version` and
//! `Language` becomes `Langue` and `SousTitre`, whatever the file said
//! (@ 0x004d6148..0x004d642a). The port has no registry: it reads the values
//! the installer writes there ([`install_registry`]): GOG.com's
//! `goggame-<id>.script` (`Install` = `CD`; `Language` = `fra`, `gbr`,
//! `ger` or `esp` for its French, English, German and Spanish installs);
//! without that file the language is the only folder of
//! `Datas\loc\text\` (`ita` for the Italian *Atlantis Collection*, whose
//! installer writes `Language` = `ita`, `Install` = `CD`).

use atlantis3_data::cfg::Config;
use atlantis3_data::A3Files;

use crate::fixes::Fixes;

pub use atlantis3_data::install::install_registry;

/// The settings the game starts with.
#[derive(Clone, Debug)]
pub struct Startup {
    pub config: Config,
    /// The registry's `Language` (`fra`), kept apart (DAT_00789c40) for the
    /// file's writer.
    pub install_language: Vec<u8>,
    /// The registry's `LanguageST` of the 2004 program (`tch` for the
    /// Czech *Level* DVD), which `Atlantis4.cfg`'s writer compares
    /// `SousTitre` with; [`Startup::install_language`] for *Atlantis III*,
    /// whose registry has one language.
    pub install_language_st: Vec<u8>,
}

impl Startup {
    /// The settings of the install `files`: its `Atlantis3.cfg` (none: the
    /// defaults) and its registry values, every bug fix on but `fix_32bits`
    /// ([`Fixes::original_depth`]).
    pub fn read(files: &A3Files) -> Startup {
        Startup::read_with(files, Fixes::original_depth())
    }

    /// [`Startup::read`] with the bug fixes `fixes` ([`Fixes::apply`],
    /// after the registry values).
    pub fn read_with(files: &A3Files, fixes: Fixes) -> Startup {
        let evolution = files.is_evolution_2004();
        let mut config = if evolution {
            // *Atlantis Evolution*'s 2004 program: `Datas\Atlantis4.cfg`
            // over its own defaults (atlantis3_data::evolution2004_cfg).
            use atlantis3_data::evolution2004_cfg as cfg4;
            match files.read("Atlantis4.cfg") {
                Some(data) => cfg4::Config::parse(&data).to_a3(),
                None => {
                    log::warn!("Datas\\Atlantis4.cfg not found: the defaults");
                    cfg4::Config::default().to_a3()
                }
            }
        } else {
            // The reader's name: `Atlantis4.cfg` in GOG.com's 2015 / 2018
            // `Game.exe` (`fix_cfg_name` reads `Atlantis3.cfg`, the 2022
            // file's). A missing file leaves the defaults (@ 0x004095a0,
            // set first; 0x00406fc6 then returns).
            let name = match files.build() {
                Ok(b) if !fixes.cfg_name => b.cfg_name,
                _ => "Atlantis3.cfg",
            };
            match files.read(name) {
                Some(data) => Config::parse(&data),
                None => {
                    log::warn!("Datas\\{name} not found: the defaults");
                    Config::default()
                }
            }
        };
        let (mut language, mut install) = install_registry(files);
        let mut language_st = language.clone();
        if let Some(src) = files.evolution_2004_source() {
            // Its installers write `Install = FLA` (MSI `Registry` table,
            // atlantis4-re subsystems/cd2004-files.md; the Inno Setup
            // installers of the Italian collection and the Czech Level
            // DVD likewise), and two languages, `Language` for the voices
            // and `LanguageST` for the texts (`gbr` / `tch` on the Czech
            // DVD).
            install = b"FLA".to_vec();
            (language, language_st) = src.install_languages();
        }
        log::info!(
            "install: language {}, subtitles {}, medium {}",
            String::from_utf8_lossy(&language),
            String::from_utf8_lossy(&language_st),
            String::from_utf8_lossy(&install)
        );
        if evolution {
            config.apply_install_2004(&language, &language_st, &install);
        } else {
            config.apply_install(&language, &install);
        }
        // The fixes are *Atlantis III*'s (`fix_warpres_cd`: its GOG.com
        // installs' 512 folders); the 2004 program's WarpRes is its file's.
        if !evolution {
            fixes.apply(&mut config, files);
        }
        Startup {
            config,
            install_language: language,
            install_language_st: language_st,
        }
    }

    /// The screen's size (`Resolution`, 800 x 600 by default).
    pub fn screen(&self) -> (usize, usize) {
        let (w, h) = self.config.resolution;
        (w.max(1) as usize, h.max(1) as usize)
    }

    /// The warp view's size (640 x 480 with the 512-pixel faces; the 3D
    /// dialogs' surface).
    pub fn warp_view(&self) -> (usize, usize) {
        let (w, h) = self.config.warp_view();
        (w as usize, h as usize)
    }

    /// The texts' language folder (`SousTitre`, `fra`).
    pub fn text_language(&self) -> String {
        String::from_utf8_lossy(&self.config.sous_titre).into_owned()
    }

    /// The voices' language folder (`Langue`, `fra`).
    pub fn voice_language(&self) -> String {
        String::from_utf8_lossy(&self.config.langue).into_owned()
    }
}
