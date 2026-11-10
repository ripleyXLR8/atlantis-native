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
//! `Datas\loc\text\`.

use atlantis3_data::cfg::Config;
use atlantis3_data::A3Files;

pub use atlantis3_data::install::install_registry;

/// The settings the game starts with.
#[derive(Clone, Debug)]
pub struct Startup {
    pub config: Config,
    /// The registry's `Language` (`fra`), kept apart (DAT_00789c40) for the
    /// file's writer.
    pub install_language: Vec<u8>,
}

impl Startup {
    /// The settings of the install `files`: its `Atlantis3.cfg` (none: the
    /// defaults) and its registry values.
    pub fn read(files: &A3Files) -> Startup {
        let mut config = match files.read("Atlantis3.cfg") {
            Some(data) => Config::parse(&data),
            None => {
                log::warn!("Datas\\Atlantis3.cfg not found: the defaults");
                Config::default()
            }
        };
        let (language, install) = install_registry(files);
        log::info!(
            "install: language {}, medium {}",
            String::from_utf8_lossy(&language),
            String::from_utf8_lossy(&install)
        );
        config.apply_install(&language, &install);
        Startup {
            config,
            install_language: language,
        }
    }

    /// The screen's size (`Resolution`, 800 x 600 by default).
    pub fn screen(&self) -> (usize, usize) {
        let (w, h) = self.config.resolution;
        (w.max(1) as usize, h.max(1) as usize)
    }

    /// The warp view's size (640 x 480 with the 512-pixel faces).
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
