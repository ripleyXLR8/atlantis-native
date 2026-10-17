//! Where the player's files live: the original's checkpoints
//! (`players.atl`, `p<letter><chapter>.gam`) and the port's settings.
//!
//! The original writes them in its install folder; the port keeps them in
//! a per-user data folder ([`DirStore`], see [`user_data_dir`]); the web
//! front end keeps them in the browser's storage; [`MemStore`] keeps them
//! in memory (tests, headless runs).

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::enhancements::Enhancements;
use crate::hd::{HdOptions, PanoFilter};

/// A flat folder of named files (names compared without regard to case).
pub trait SaveStore {
    fn read(&self, name: &str) -> Option<Vec<u8>>;
    /// False when the file could not be written.
    fn write(&self, name: &str, data: &[u8]) -> bool;
    fn remove(&self, name: &str);
    /// The file names (lower case).
    fn list(&self) -> Vec<String>;
}

/// Files in memory.
#[derive(Debug, Default)]
pub struct MemStore(RefCell<BTreeMap<String, Vec<u8>>>);

impl MemStore {
    pub fn new() -> MemStore {
        MemStore::default()
    }
}

impl SaveStore for MemStore {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.0.borrow().get(&name.to_ascii_lowercase()).cloned()
    }

    fn write(&self, name: &str, data: &[u8]) -> bool {
        self.0
            .borrow_mut()
            .insert(name.to_ascii_lowercase(), data.to_vec());
        true
    }

    fn remove(&self, name: &str) {
        self.0.borrow_mut().remove(&name.to_ascii_lowercase());
    }

    fn list(&self) -> Vec<String> {
        self.0.borrow().keys().cloned().collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use dir::{user_data_dir, DirStore};

#[cfg(not(target_arch = "wasm32"))]
mod dir {
    use super::SaveStore;
    use std::path::{Path, PathBuf};

    /// Files of a folder on disk, created on the first write.
    #[derive(Clone, Debug)]
    pub struct DirStore {
        dir: PathBuf,
    }

    impl DirStore {
        pub fn new(dir: impl Into<PathBuf>) -> DirStore {
            DirStore { dir: dir.into() }
        }

        pub fn dir(&self) -> &Path {
            &self.dir
        }

        fn path(&self, name: &str) -> PathBuf {
            let lower = name.to_ascii_lowercase();
            if let Ok(rd) = std::fs::read_dir(&self.dir) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().to_ascii_lowercase() == lower {
                        return e.path();
                    }
                }
            }
            self.dir.join(lower)
        }
    }

    impl SaveStore for DirStore {
        fn read(&self, name: &str) -> Option<Vec<u8>> {
            std::fs::read(self.path(name)).ok()
        }

        fn write(&self, name: &str, data: &[u8]) -> bool {
            if let Err(e) = std::fs::create_dir_all(&self.dir) {
                log::warn!("{}: {e}", self.dir.display());
                return false;
            }
            let path = self.path(name);
            // Written whole, then renamed: a crash leaves the old file.
            let tmp = path.with_extension("tmp");
            match std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, &path)) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("{}: {e}", path.display());
                    false
                }
            }
        }

        fn remove(&self, name: &str) {
            let _ = std::fs::remove_file(self.path(name));
        }

        fn list(&self) -> Vec<String> {
            std::fs::read_dir(&self.dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_file())
                        .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// The per-user data folder of the port: `$XDG_DATA_HOME/atlantis-native`
    /// (or `~/.local/share/atlantis-native`) on Linux and the other Unix
    /// systems, `~/Library/Application Support/atlantis-native` on macOS,
    /// `%APPDATA%\atlantis-native` on Windows.
    pub fn user_data_dir() -> Option<PathBuf> {
        let env = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let base = if cfg!(windows) {
            env("APPDATA")
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|h| h.join("Library").join("Application Support"))
        } else {
            env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local").join("share")))
        }?;
        Some(base.join("atlantis-native"))
    }
}

/// The settings file in the store.
pub const SETTINGS_FILE: &str = "settings.ini";

/// The port's graphics mode (its options menu, F10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Graphics {
    /// The original's 640x480 frames, scaled to the window.
    #[default]
    Original,
    /// The panoramic view and the talking heads drawn at the window's
    /// resolution (see [`crate::hd`]).
    Hd,
}

impl Graphics {
    /// The value in `settings.ini`.
    pub fn name(self) -> &'static str {
        match self {
            Graphics::Original => "original",
            Graphics::Hd => "hd",
        }
    }

    pub fn from_name(s: &str) -> Option<Graphics> {
        match s.to_ascii_lowercase().as_str() {
            "original" => Some(Graphics::Original),
            "hd" => Some(Graphics::Hd),
            _ => None,
        }
    }
}

/// The options of the original's menus, and the port's enhancements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// SUBTITLES (0x004960a8).
    pub subtitles: bool,
    /// OMNI 3D MODE 2 (0x004960ac == 1): the pitch follows the mouse
    /// (y - prevY); MODE 1, the original's default, inverts it.
    pub omni3d_mode2: bool,
    /// Master, music, effects and voices volumes, 0..127 (0x00495e70,
    /// 0x00495e72..0x00495e74).
    pub volumes: [u8; 4],
    pub enhancements: Enhancements,
    /// The music tracks the original would find copied in its install
    /// folder (bit i = track index i, LoadMusik / DelMusik): like that
    /// folder, the set outlives the session.
    pub music_on_disk: u32,
    /// The port's graphics mode.
    pub graphics: Graphics,
    /// The HD mode's picture options.
    pub hd: HdOptions,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            subtitles: true,
            omni3d_mode2: false,
            volumes: [127; 4],
            enhancements: Enhancements::default(),
            music_on_disk: 0,
            graphics: Graphics::Original,
            hd: HdOptions::default(),
        }
    }
}

impl Settings {
    /// `key = value` lines; unknown keys and bad values keep the default.
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            let b = match v.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Some(true),
                "0" | "false" | "off" | "no" => Some(false),
                _ => None,
            };
            if k == "graphics" {
                match Graphics::from_name(v) {
                    Some(g) => s.graphics = g,
                    None => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
                }
                continue;
            }
            if k == "hd_panorama" {
                match PanoFilter::from_name(v) {
                    Some(f) => s.hd.panorama = f,
                    None => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
                }
                continue;
            }
            if k == "hd_interface" {
                match v.to_ascii_lowercase().as_str() {
                    "smooth" => s.hd.smooth_interface = true,
                    "pixels" => s.hd.smooth_interface = false,
                    _ => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
                }
                continue;
            }
            if k == "music_on_disk" {
                match u32::from_str_radix(v, 16) {
                    Ok(bits) => s.music_on_disk = bits,
                    Err(_) => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
                }
                continue;
            }
            let n = v.parse::<u8>().ok().map(|n| n.min(127));
            match (k.as_str(), b, n) {
                ("subtitles", Some(b), _) => s.subtitles = b,
                ("omni3d_mode2", Some(b), _) => s.omni3d_mode2 = b,
                ("volume_master", _, Some(n)) => s.volumes[0] = n,
                ("volume_music", _, Some(n)) => s.volumes[1] = n,
                ("volume_effects", _, Some(n)) => s.volumes[2] = n,
                ("volume_voices", _, Some(n)) => s.volumes[3] = n,
                ("minor_bug_fixes", Some(b), _) => s.enhancements.minor_bug_fixes = b,
                ("restored_content", Some(b), _) => s.enhancements.restored_content = b,
                ("ui_ux", Some(b), _) => s.enhancements.ui_ux = b,
                ("hd_sharpen", Some(b), _) => s.hd.sharpen = b,
                ("hd_sharp_subtitles", Some(b), _) => s.hd.sharp_subtitles = b,
                _ => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let b = |v: bool| if v { "on" } else { "off" };
        format!(
            "# Atlantis Native settings\n\
             # The options of the game's menus\n\
             subtitles = {}\n\
             omni3d_mode2 = {}\n\
             volume_master = {}\n\
             volume_music = {}\n\
             volume_effects = {}\n\
             volume_voices = {}\n\
             # Enhancements: where the port may differ from the original game\n\
             minor_bug_fixes = {}\n\
             restored_content = {}\n\
             ui_ux = {}\n\
             # The music tracks LoadMusik copied (hexadecimal bit set)\n\
             music_on_disk = {:x}\n\
             # The port's graphics (options menu, F10): original or hd\n\
             graphics = {}\n\
             # HD mode: panorama filter (bicubic or bilinear), its sharpening,\n\
             # the interface art (smooth: xBR, or pixels), sharp subtitles\n\
             hd_panorama = {}\n\
             hd_sharpen = {}\n\
             hd_interface = {}\n\
             hd_sharp_subtitles = {}\n",
            b(self.subtitles),
            b(self.omni3d_mode2),
            self.volumes[0],
            self.volumes[1],
            self.volumes[2],
            self.volumes[3],
            b(self.enhancements.minor_bug_fixes),
            b(self.enhancements.restored_content),
            b(self.enhancements.ui_ux),
            self.music_on_disk,
            self.graphics.name(),
            self.hd.panorama.name(),
            b(self.hd.sharpen),
            if self.hd.smooth_interface {
                "smooth"
            } else {
                "pixels"
            },
            b(self.hd.sharp_subtitles),
        )
    }

    pub fn load(store: &dyn SaveStore) -> Option<Settings> {
        let d = store.read(SETTINGS_FILE)?;
        Some(Settings::parse(&String::from_utf8_lossy(&d)))
    }

    pub fn save(&self, store: &dyn SaveStore) {
        store.write(SETTINGS_FILE, self.to_text().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            subtitles: false,
            omni3d_mode2: true,
            volumes: [1, 2, 3, 127],
            enhancements: Enhancements::original(),
            music_on_disk: 0x8001_00a5,
            graphics: Graphics::Hd,
            hd: HdOptions {
                panorama: PanoFilter::Bilinear,
                sharpen: false,
                smooth_interface: false,
                sharp_subtitles: false,
            },
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert_eq!(Settings::parse("music_on_disk = 1F").music_on_disk, 0x1f);
        assert_eq!(Settings::parse("graphics = HD").graphics, Graphics::Hd);
        assert_eq!(
            Settings::parse("graphics = 4k").graphics,
            Graphics::Original
        );
        assert_eq!(Settings::default().graphics, Graphics::Original);
        // The HD options: the better-looking ones by default; each key.
        let d = Settings::default().hd;
        assert_eq!(d.panorama, PanoFilter::Bicubic);
        assert!(d.sharpen && d.smooth_interface && d.sharp_subtitles);
        assert_eq!(
            Settings::parse(&Settings::default().to_text()),
            Settings::default()
        );
        let p = Settings::parse(
            "hd_panorama = Bilinear\nhd_sharpen = off\nhd_interface = pixels\nhd_sharp_subtitles = 0",
        );
        assert_eq!(p.hd, s.hd);
        assert_eq!(Settings::parse("hd_panorama = lanczos").hd, d);
        assert_eq!(Settings::parse("hd_interface = blurry").hd, d);
        assert_eq!(
            Settings::parse("volume_music = 300\nbad"),
            Settings {
                volumes: [127; 4],
                ..Settings::default()
            }
        );
        let m = MemStore::new();
        s.save(&m);
        assert_eq!(Settings::load(&m), Some(s));
        assert_eq!(m.list(), vec![SETTINGS_FILE.to_string()]);
    }
}
