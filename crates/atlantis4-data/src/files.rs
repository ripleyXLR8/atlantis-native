//! The installed game: a folder holding `Atlantis Evolution.exe` and the
//! data folders (`3D`, `2D`, `GUI`, `Sounds`, `Texts`, `Videos`, ...), every
//! file unpacked.
//!
//! Names are matched without regard to case, as on the original's
//! Windows. The scenes ("warps") are named `3D/<zone>/<NNNN>.wp`
//! (`Texts/Full/FirstWarpPath.lua`) or `<zone>\<NNNN>.wp` (an exit's
//! `linkedWarp`, to which the executable adds `3D\`, string
//! atlantis4-gog-en-2363910 @ 0x005b20a4); no `.wp` file exists: the name
//! stands for the scene's `.xml` and `.lua` beside it and its folder of
//! warp packages ([`WarpPath`]).

use std::path::Path;
use std::sync::{Arc, RwLock};

use atlantis_data::{FileSource, StdDir};

use crate::exe::{self, Build, ExeError};

/// The folder of the scenes.
pub const SCENES: &str = "3D";

/// The script every scene's script may call (global helpers).
pub const GAME_ENTER_SCRIPT: &str = "3D/OnGameEnter.lua";

/// The platform folder inside a scene's folder that holds its warp
/// packages. The executable has no `PC-MacOSX` string (it picks the folder
/// among version "definitions", a rule not read yet); every one of the 456
/// scene folders of the GOG.com install holds exactly this one.
pub const PLATFORM_DIR: &str = "PC-MacOSX";

/// An installed game.
#[derive(Clone)]
pub struct A4Files {
    src: Arc<dyn FileSource>,
    /// The language tag (shared by every clone): [`A4Files::choose_language`].
    lang: Arc<RwLock<String>>,
}

/// The languages `Application::create` tries, in order, when
/// `Texts/<language>.xml` is missing (the list @ 0x0057adde: string
/// pointers 0x005b98a4..0x005b98b8, then a null).
pub const FALLBACK_LANGUAGES: [&str; 6] = ["en", "fr", "de", "es", "it", "ru"];

/// Why a folder is not a playable install.
#[derive(Debug)]
pub enum OpenError {
    /// No `Atlantis Evolution.exe`.
    NoExe,
    /// No `3D` folder.
    NoScenes,
    /// The executable is not a known build.
    Exe(ExeError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NoExe => write!(f, "no {} in the folder", exe::EXE_NAME),
            OpenError::NoScenes => write!(f, "no 3D folder in the folder"),
            OpenError::Exe(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for OpenError {}

/// Whether `dir` looks like an *Atlantis Evolution* install (the
/// executable and `3D/OnGameEnter.lua`).
pub fn holds_atlantis4(dir: &Path) -> bool {
    let src = StdDir::new(dir);
    src.size(exe::EXE_NAME).is_some() && src.size(GAME_ENTER_SCRIPT).is_some()
}

impl A4Files {
    /// An install read from `src`, unchecked.
    ///
    /// Its language is the original's at start-up: `lang.ini`'s, checked
    /// against the texts ([`A4Files::choose_language`]).
    pub fn new(src: Arc<dyn FileSource>) -> A4Files {
        let files = A4Files {
            src,
            lang: Arc::new(RwLock::new(String::new())),
        };
        let ini = files.lang_ini();
        files.choose_language(ini.as_deref());
        files
    }

    /// The install at `dir`, with its executable identified.
    pub fn open_dir(dir: &Path) -> Result<(A4Files, &'static Build), OpenError> {
        let files = A4Files::new(Arc::new(StdDir::new(dir)));
        let build = files.build()?;
        Ok((files, build))
    }

    /// The build of the executable.
    pub fn build(&self) -> Result<&'static Build, OpenError> {
        if !self.src.has_dir(SCENES) {
            return Err(OpenError::NoScenes);
        }
        let exe = self.src.read(exe::EXE_NAME).ok_or(OpenError::NoExe)?;
        exe::identify(&exe).map_err(OpenError::Exe)
    }

    /// The source of the whole install.
    pub fn source(&self) -> &dyn FileSource {
        &*self.src
    }

    /// A whole file, by its name in the install (`/` or `\`, any case).
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.src.read(name)
    }

    /// Whether a file exists (what a read's success tells, without reading
    /// it: in the browser a whole music file is about 60 ms of reading).
    pub fn exists(&self, name: &str) -> bool {
        self.src.size(name).is_some()
    }

    /// The language code of `lang.ini` (`[lang]`, `lang=en`), lower case.
    ///
    /// The game's only source of its language: the core's constructor
    /// (@ 0x00550eb0 → 0x0056a000) reads it with
    /// `GetPrivateProfileStringA("lang", "lang", "", buf, 0x59,
    /// ".\\lang.ini")` and makes it the `language` tag. GOG.com ships
    /// `lang=en` and its installer rewrites the value for the language
    /// chosen in its client (`de`, `en`, `es`, `fr`, `it`); the executable
    /// reads neither the registry nor the system's locale.
    pub fn lang_ini(&self) -> Option<String> {
        parse_lang_ini(&self.src.read("lang.ini")?)
    }

    /// The language tag: the folder of the voices, movie sounds, texts and
    /// localised pictures (`de`, `en`, `es`, `fr`, `it`).
    pub fn language(&self) -> String {
        self.lang.read().map(|l| l.clone()).unwrap_or_default()
    }

    /// Sets the language tag to `wanted` (`None`: empty, as a missing
    /// `lang.ini` gives) as `Application::create` checks it
    /// (@ 0x0057aedf..0x0057b54a): while `Texts/<language>.xml` does not
    /// exist, the next of [`FALLBACK_LANGUAGES`] becomes the language; the
    /// list's end keeps the last one. Every clone sees the new tag.
    /// Returns it.
    pub fn choose_language(&self, wanted: Option<&str>) -> String {
        let mut lang = wanted.unwrap_or("").trim().to_ascii_lowercase();
        let mut next = FALLBACK_LANGUAGES.iter();
        while !self.exists(&crate::texts::texts_path(&lang)) {
            match next.next() {
                Some(l) => lang = l.to_string(),
                None => break,
            }
        }
        if let Ok(mut l) = self.lang.write() {
            l.clone_from(&lang);
        }
        lang
    }

    /// The languages the install has texts for, of [`FALLBACK_LANGUAGES`]
    /// (the five of GOG.com's install: `ru` has none).
    pub fn languages(&self) -> Vec<&'static str> {
        FALLBACK_LANGUAGES
            .iter()
            .copied()
            .filter(|l| self.exists(&crate::texts::texts_path(l)))
            .collect()
    }
}

/// The value of `lang=` in a `lang.ini` (any line, any case for the key),
/// trimmed and lower-cased.
pub fn parse_lang_ini(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("lang")
            .then(|| value.trim().to_ascii_lowercase())
    })
}

/// A scene's name and the files it stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarpPath {
    /// The zone folder under `3D` (`Bateau_CA`).
    pub zone: String,
    /// The scene's number as written (`0001`).
    pub node: String,
}

impl WarpPath {
    /// A `3D/<zone>/<node>.wp` or `<zone>\<node>.wp` name (either
    /// separator, the `3D` prefix optional, the `.wp` suffix optional).
    pub fn parse(name: &str) -> Option<WarpPath> {
        let name = name.replace('\\', "/");
        let name = name.trim_matches('/');
        let name = name.strip_suffix(".wp").unwrap_or(name);
        let mut parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() == 3 && parts[0].eq_ignore_ascii_case(SCENES) {
            parts.remove(0);
        }
        match parts.as_slice() {
            [zone, node] => Some(WarpPath {
                zone: (*zone).to_string(),
                node: (*node).to_string(),
            }),
            _ => None,
        }
    }

    /// `3D/<zone>/<node>`.
    pub fn stem(&self) -> String {
        format!("{SCENES}/{}/{}", self.zone, self.node)
    }

    /// The scene description, `3D/<zone>/<node>.xml`.
    pub fn xml(&self) -> String {
        format!("{}.xml", self.stem())
    }

    /// The scene script, `3D/<zone>/<node>.lua`.
    pub fn lua(&self) -> String {
        format!("{}.lua", self.stem())
    }

    /// The warp package `<name>.wpa` of the scene (`background`, an
    /// object's or an animation's name).
    pub fn package(&self, name: &str) -> String {
        format!("{}/{PLATFORM_DIR}/{name}.wpa", self.stem())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_spellings_of_a_scene_name_are_the_same_scene() {
        let a = WarpPath::parse("3D/Bateau_CA/0001.wp").unwrap();
        let b = WarpPath::parse("Bateau_CA\\0001.wp").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.xml(), "3D/Bateau_CA/0001.xml");
        assert_eq!(a.lua(), "3D/Bateau_CA/0001.lua");
        assert_eq!(
            a.package("background"),
            "3D/Bateau_CA/0001/PC-MacOSX/background.wpa"
        );
        assert_eq!(WarpPath::parse("0001.wp"), None);
    }

    #[test]
    fn lang_ini_gives_its_code() {
        assert_eq!(parse_lang_ini(b"[lang]\r\nlang=en").as_deref(), Some("en"));
        assert_eq!(parse_lang_ini(b"[lang]\r\n"), None);
        assert_eq!(
            parse_lang_ini(b"[lang]\r\nlang=fr\r\n").as_deref(),
            Some("fr")
        );
    }

    /// An install of the texts `names`, with `lang.ini` `ini`.
    fn install(names: &[&str], ini: Option<&str>) -> A4Files {
        let mut m = atlantis_data::MemFiles::default();
        for n in names {
            m.insert(n, b"<dialog />".to_vec());
        }
        if let Some(i) = ini {
            m.insert("lang.ini", format!("[lang]\r\nlang={i}").into_bytes());
        }
        A4Files::new(Arc::new(m))
    }

    #[test]
    fn the_language_falls_back_as_application_create() {
        let all = [
            "Texts/de.xml",
            "Texts/en.xml",
            "Texts/es.xml",
            "Texts/fr.xml",
            "Texts/it.xml",
        ];
        assert_eq!(install(&all, Some("fr")).language(), "fr");
        assert_eq!(install(&all, Some("IT")).language(), "it");
        // No `lang.ini`, an empty value, a language without texts: English.
        assert_eq!(install(&all, None).language(), "en");
        assert_eq!(install(&all, Some("")).language(), "en");
        assert_eq!(install(&all, Some("ru")).language(), "en");
        // Without English: the next of the list.
        assert_eq!(install(&["Texts/de.xml"], Some("ru")).language(), "de");
        // No texts at all: the list's end.
        assert_eq!(install(&[], Some("fr")).language(), "ru");
        let f = install(&all, Some("fr"));
        let g = f.clone();
        assert_eq!(f.choose_language(Some("de")), "de");
        assert_eq!(g.language(), "de", "clones share the tag");
        assert_eq!(f.languages(), ["en", "fr", "de", "es", "it"]);
    }
}
