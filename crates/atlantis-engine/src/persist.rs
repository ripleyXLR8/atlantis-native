//! Where the player's files live: the original's checkpoints
//! (`players.atl`, `p<letter><chapter>.gam`) and the port's settings.
//!
//! The original writes them in its install folder; the port keeps them in
//! a per-user data folder ([`DirStore`], see [`user_data_dir`]); the web
//! front end keeps them in the browser's storage; [`MemStore`] keeps them
//! in memory (tests, headless runs).

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::enhancements::{BugFix, Enhancements};
use crate::hd::{HdOptions, PanoFilter, VideoSides};

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
pub use dir::{
    data_dir_and_source, per_user_data_dir, portable_bases, portable_data_dir, resolve_data_dir,
    set_data_dir_override, user_data_dir, DataDirSource, DirStore, PORTABLE_DATA, PORTABLE_MARKER,
};

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

    /// The folder named by `--data-dir` for this run, set once by the
    /// desktop program before anything is read or written.
    static DATA_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

    /// `--data-dir <folder>`: the data folder of this run, over every
    /// other rule ([`user_data_dir`]).
    pub fn set_data_dir_override(dir: Option<PathBuf>) {
        if let Ok(mut d) = DATA_DIR_OVERRIDE.write() {
            *d = dir;
        }
    }

    /// The file whose presence beside the executable turns the portable
    /// mode on.
    pub const PORTABLE_MARKER: &str = "portable.txt";
    /// The portable data folder's name, beside the executable (its
    /// presence turns the portable mode on too).
    pub const PORTABLE_DATA: &str = "data";

    /// Where the data folder came from ([`resolve_data_dir`]).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DataDirSource {
        /// `--data-dir <folder>`.
        CommandLine,
        /// The `ATLANTIS_NATIVE_DATA` environment variable.
        Environment,
        /// `portable.txt` or `data/` beside the executable.
        Portable,
        /// The system's per-user folder.
        PerUser,
    }

    /// The folders the portable mode looks in for its marker: the
    /// executable's own folder and, on macOS (`macos`), the folder holding
    /// the application bundle when the executable is
    /// `<X>.app/Contents/MacOS/<exe>` (first: that is the folder the
    /// player sees).
    pub fn portable_bases(exe: &Path, macos: bool) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Some(exe_dir) = exe.parent() else {
            return out;
        };
        if macos {
            let macos_dir = exe_dir;
            let contents = macos_dir.parent();
            let bundle = contents.and_then(Path::parent);
            let is_bundle = macos_dir.file_name().is_some_and(|n| n == "MacOS")
                && contents
                    .and_then(Path::file_name)
                    .is_some_and(|n| n == "Contents")
                && bundle
                    .and_then(Path::extension)
                    .is_some_and(|e| e.eq_ignore_ascii_case("app"));
            if let Some(outer) = bundle.and_then(Path::parent).filter(|_| is_bundle) {
                out.push(outer.to_path_buf());
            }
        }
        out.push(exe_dir.to_path_buf());
        out
    }

    /// The portable data folder (`<base>/data`) of the first of `bases`
    /// holding `portable.txt` or a `data` folder.
    pub fn portable_data_dir(bases: &[PathBuf]) -> Option<PathBuf> {
        bases
            .iter()
            .find(|b| b.join(PORTABLE_MARKER).is_file() || b.join(PORTABLE_DATA).is_dir())
            .map(|b| b.join(PORTABLE_DATA))
    }

    /// The data folder by the rules, from their inputs (no global state,
    /// for the tests): `--data-dir` (`cli`), else `ATLANTIS_NATIVE_DATA`
    /// (`env`, empty counts as unset), else the portable mode (`bases`,
    /// see [`portable_bases`]; none on the web or Android), else the
    /// system's per-user folder (`per_user`).
    pub fn resolve_data_dir(
        cli: Option<PathBuf>,
        env: Option<PathBuf>,
        bases: &[PathBuf],
        per_user: Option<PathBuf>,
    ) -> Option<(PathBuf, DataDirSource)> {
        if let Some(d) = cli.filter(|d| !d.as_os_str().is_empty()) {
            return Some((d, DataDirSource::CommandLine));
        }
        if let Some(d) = env.filter(|d| !d.as_os_str().is_empty()) {
            return Some((d, DataDirSource::Environment));
        }
        if let Some(d) = portable_data_dir(bases) {
            return Some((d, DataDirSource::Portable));
        }
        per_user.map(|d| (d, DataDirSource::PerUser))
    }

    /// The system's per-user data folder of the port:
    /// `$XDG_DATA_HOME/atlantis-native` (or `~/.local/share/atlantis-native`)
    /// on Linux and the other Unix systems, `~/Library/Application
    /// Support/atlantis-native` on macOS, `%APPDATA%\atlantis-native` on
    /// Windows.
    pub fn per_user_data_dir() -> Option<PathBuf> {
        let base = if cfg!(windows) {
            env_path("APPDATA")
        } else if cfg!(target_os = "macos") {
            env_path("HOME").map(|h| h.join("Library").join("Application Support"))
        } else {
            env_path("XDG_DATA_HOME")
                .or_else(|| env_path("HOME").map(|h| h.join(".local").join("share")))
        }?;
        Some(base.join("atlantis-native"))
    }

    fn env_path(k: &str) -> Option<PathBuf> {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// The data folder of the port and where it came from: `--data-dir`
    /// ([`set_data_dir_override`]), else the folder `ATLANTIS_NATIVE_DATA`
    /// names (tests and trial runs that must not touch the player's
    /// saves), else, in the portable mode (`portable.txt` or a `data`
    /// folder beside the executable, or beside the `.app` on macOS), the
    /// `data` folder there (made if missing), else
    /// [`per_user_data_dir`].
    pub fn data_dir_and_source() -> Option<(PathBuf, DataDirSource)> {
        let cli = DATA_DIR_OVERRIDE.read().ok().and_then(|d| d.clone());
        let bases = if cfg!(target_os = "android") {
            Vec::new()
        } else {
            std::env::current_exe()
                .ok()
                .map(|exe| portable_bases(&exe, cfg!(target_os = "macos")))
                .unwrap_or_default()
        };
        let found = resolve_data_dir(
            cli,
            env_path("ATLANTIS_NATIVE_DATA"),
            &bases,
            per_user_data_dir(),
        );
        if let Some((d, DataDirSource::Portable)) = &found {
            let _ = std::fs::create_dir_all(d);
        }
        found
    }

    /// The data folder of the port (see [`data_dir_and_source`]): the
    /// saves, settings, game library and log.
    pub fn user_data_dir() -> Option<PathBuf> {
        data_dir_and_source().map(|(d, _)| d)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A fresh folder of the test's own.
        fn scratch(name: &str) -> PathBuf {
            let d = std::env::temp_dir()
                .join(format!("atlantis-portable-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }

        fn p(s: &str) -> Option<PathBuf> {
            Some(PathBuf::from(s))
        }

        #[test]
        fn command_line_first() {
            let d = scratch("cli");
            std::fs::write(d.join(PORTABLE_MARKER), b"").unwrap();
            let r = resolve_data_dir(p("/cli"), p("/env"), &[d.clone()], p("/user"));
            assert_eq!(r, Some((PathBuf::from("/cli"), DataDirSource::CommandLine)));
        }

        #[test]
        fn environment_over_portable() {
            let d = scratch("env");
            std::fs::write(d.join(PORTABLE_MARKER), b"").unwrap();
            let r = resolve_data_dir(None, p("/env"), &[d.clone()], p("/user"));
            assert_eq!(r, Some((PathBuf::from("/env"), DataDirSource::Environment)));
            // Empty values count as unset.
            let r = resolve_data_dir(p(""), p(""), &[d.clone()], p("/user"));
            assert_eq!(r, Some((d.join("data"), DataDirSource::Portable)));
        }

        #[test]
        fn portable_by_marker_or_data_folder() {
            let d = scratch("marker");
            let user = Some((PathBuf::from("/user"), DataDirSource::PerUser));
            // No marker: nothing changes for the players of before.
            assert_eq!(resolve_data_dir(None, None, &[d.clone()], p("/user")), user);
            std::fs::write(d.join(PORTABLE_MARKER), b"").unwrap();
            assert_eq!(
                resolve_data_dir(None, None, &[d.clone()], p("/user")),
                Some((d.join("data"), DataDirSource::Portable))
            );
            let e = scratch("datafolder");
            std::fs::create_dir_all(e.join(PORTABLE_DATA)).unwrap();
            assert_eq!(
                resolve_data_dir(None, None, &[e.clone()], p("/user")),
                Some((e.join("data"), DataDirSource::Portable))
            );
            // A file named `data` is no marker.
            let f = scratch("datafile");
            std::fs::write(f.join(PORTABLE_DATA), b"").unwrap();
            assert_eq!(resolve_data_dir(None, None, &[f], p("/user")), user);
            assert_eq!(resolve_data_dir(None, None, &[], None), None);
        }

        #[test]
        fn mac_bundle_folder() {
            let exe = Path::new("/Games/Atlantis Native.app/Contents/MacOS/atlantis-native");
            assert_eq!(
                portable_bases(exe, true),
                vec![
                    PathBuf::from("/Games"),
                    PathBuf::from("/Games/Atlantis Native.app/Contents/MacOS"),
                ]
            );
            assert_eq!(
                portable_bases(exe, false),
                vec![PathBuf::from("/Games/Atlantis Native.app/Contents/MacOS")]
            );
            // Not in a bundle: the executable's folder alone.
            let exe = Path::new("/Games/atlantis/atlantis-native");
            assert_eq!(
                portable_bases(exe, true),
                vec![PathBuf::from("/Games/atlantis")]
            );
            // The bundle's folder wins over the executable's.
            let d = scratch("mac");
            let macos = d.join("A.app").join("Contents").join("MacOS");
            std::fs::create_dir_all(&macos).unwrap();
            std::fs::write(d.join(PORTABLE_MARKER), b"").unwrap();
            std::fs::write(macos.join(PORTABLE_MARKER), b"").unwrap();
            let bases = portable_bases(&macos.join("atlantis-native"), true);
            assert_eq!(portable_data_dir(&bases), Some(d.join("data")));
            std::fs::remove_file(d.join(PORTABLE_MARKER)).unwrap();
            assert_eq!(portable_data_dir(&bases), Some(macos.join("data")));
        }
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

/// The volumes at start-up, before the sound menu's OK or a checkpoint
/// writes them: master 0x7f, music, effects and voices 0x6e (the data
/// bytes at 0x00495e70 and 0x00495e72..0x00495e74; atlantis-cd-en-714240
/// sets the voice sample to 0x6e at start-up, @ 0x00430e82).
pub const DEFAULT_VOLUMES: [u8; 4] = [0x7f, 0x6e, 0x6e, 0x6e];

/// A volume 0..127 of the original for the mixer, which takes Miles'
/// 0..127 values (`audio::miles_gain`).
pub fn mixer_volume(x: u8) -> u8 {
    x.min(127)
}

/// The options of the original's menus, and the port's enhancements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// SUBTITLES (0x004960a8): off at a fresh start, as the original
    /// starts every run; then as the options menu leaves it.
    pub subtitles: bool,
    /// OMNI 3D MODE 2 (0x004960ac == 1): the pitch follows the mouse
    /// (y - prevY); MODE 1, the original's default, inverts it.
    pub omni3d_mode2: bool,
    /// Master, music, effects and voices volumes, 0..127 (0x00495e70,
    /// 0x00495e72..0x00495e74); a fresh start has the original's
    /// [`DEFAULT_VOLUMES`].
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
    /// The window opens in full screen (the options menu's "Start in full
    /// screen"; switching full screen during a game sets it).
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Off, as every start of the original (0x004960a8 is 0 in the
            // data; only the options' toggle @ 0x0041d736 and a
            // checkpoint load write it).
            subtitles: false,
            omni3d_mode2: false,
            volumes: DEFAULT_VOLUMES,
            enhancements: Enhancements::default(),
            music_on_disk: 0,
            graphics: Graphics::Original,
            hd: HdOptions::default(),
            fullscreen: false,
        }
    }
}

impl Settings {
    /// `key = value` lines; unknown keys and bad values keep the default.
    /// One key per bug fix ([`BugFix::key`]); `minor_bug_fixes`, which
    /// earlier versions wrote for all of them, sets every fix whose own key
    /// is absent.
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        // `minor_bug_fixes`, and the fixes whose own key was read.
        let mut all_fixes = None;
        let mut read = Vec::new();
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
            if let Some(f) = BugFix::ALL.into_iter().find(|f| f.key() == k) {
                match b {
                    Some(b) => {
                        s.enhancements.fixes.set(f, b);
                        read.push(f);
                    }
                    None => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
                }
                continue;
            }
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
                ("minor_bug_fixes", Some(b), _) => all_fixes = Some(b),
                ("restored_content", Some(b), _) => s.enhancements.restored_content = b,
                ("ui_ux", Some(b), _) => s.enhancements.ui_ux = b,
                ("arrow_keys", Some(b), _) => s.enhancements.arrow_keys = b,
                ("background_run", Some(b), _) => s.enhancements.background_run = b,
                ("hd_sharpen", Some(b), _) => s.hd.sharpen = b,
                ("hd_sharp_subtitles", Some(b), _) => s.hd.sharp_subtitles = b,
                ("hd_widescreen", Some(b), _) => s.hd.widescreen = b,
                ("fullscreen", Some(b), _) => s.fullscreen = b,
                ("hd_video_sides", _, _) if VideoSides::from_name(v).is_some() => {
                    s.hd.video_sides = VideoSides::from_name(v).unwrap_or_default();
                }
                _ => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
            }
        }
        if let Some(on) = all_fixes {
            for f in BugFix::ALL.into_iter().filter(|f| !read.contains(f)) {
                s.enhancements.fixes.set(f, on);
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
             # Bug fixes (off: as the original; options menu, F10, \"Choose the\n\
             # bug fixes...\"): the music loops seamlessly; the second\n\
             # cinematic's sound; the talking heads' overlay (the guards' coif,\n\
             # the pilot's scarf); no icons left as the inventory bar slides out; a\n\
             # picked-up object's icon in its slot; each menu its own click\n\
             # areas; OK on the fifth player; the snakes puzzle's false first\n\
             # win; and those of some builds: the hexagon puzzle's pieces (1997\n\
             # and Chinese builds), LOADMUSIK track by track (French May 1997),\n\
             # the sharp s (German), credits and fonts of disc 1 (Dutch, German\n\
             # May 1997), OK and Annulla clicks (Italian), the subtitle rows\n\
             # (Simplified Chinese), the SUBTITLES record (Chinese)\n\
             {}\n\
             # Content the original never shows, interface changes\n\
             restored_content = {}\n\
             ui_ux = {}\n\
             # The arrow keys turn the view (off: F5..F8 only, as the original)\n\
             arrow_keys = {}\n\
             # The game goes on in a window that is not the active one (off:\n\
             # it stops, as the original)\n\
             background_run = {}\n\
             # The music tracks LoadMusik copied (hexadecimal bit set)\n\
             music_on_disk = {:x}\n\
             # The port's graphics (options menu, F10): original or hd\n\
             graphics = {}\n\
             # HD mode: panorama filter (bicubic or bilinear), its sharpening,\n\
             # the interface art (smooth: xBR, or pixels), sharp subtitles,\n\
             # widescreen (the panorama fills the sides of a wide window) and\n\
             # the bars beside the videos (black, blur, zoom or panorama)\n\
             hd_panorama = {}\n\
             hd_sharpen = {}\n\
             hd_interface = {}\n\
             hd_sharp_subtitles = {}\n\
             hd_widescreen = {}\n\
             hd_video_sides = {}\n\
             # The window opens in full screen (options menu, F10; switching\n\
             # full screen during a game changes it too)\n\
             fullscreen = {}\n",
            b(self.subtitles),
            b(self.omni3d_mode2),
            self.volumes[0],
            self.volumes[1],
            self.volumes[2],
            self.volumes[3],
            BugFix::ALL
                .iter()
                .map(|&f| format!("{} = {}", f.key(), b(self.enhancements.fixes.get(f))))
                .collect::<Vec<_>>()
                .join("\n"),
            b(self.enhancements.restored_content),
            b(self.enhancements.ui_ux),
            b(self.enhancements.arrow_keys),
            b(self.enhancements.background_run),
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
            b(self.hd.widescreen),
            self.hd.video_sides.name(),
            b(self.fullscreen),
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
                widescreen: false,
                video_sides: VideoSides::Black,
            },
            fullscreen: true,
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert!(s.to_text().contains("\nfullscreen = on\n"));
        // Full screen: off by default, its key alone, a bad value ignored.
        assert!(!Settings::default().fullscreen);
        assert!(Settings::default()
            .to_text()
            .contains("\nfullscreen = off\n"));
        assert!(Settings::parse("fullscreen = yes").fullscreen);
        assert!(!Settings::parse("fullscreen = maybe").fullscreen);
        assert!(!Settings::parse("fullscreen = on\nfullscreen = 0").fullscreen);
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
                volumes: DEFAULT_VOLUMES,
                ..Settings::default()
            }
        );
        let m = MemStore::new();
        s.save(&m);
        assert_eq!(Settings::load(&m), Some(s));
        assert_eq!(m.list(), vec![SETTINGS_FILE.to_string()]);
    }

    /// One key per bug fix, each read and written alone; the earlier
    /// versions' `minor_bug_fixes` sets every fix without its own key.
    #[test]
    fn one_key_per_fix_and_the_old_key_for_the_others() {
        use crate::enhancements::BugFixes;
        for f in BugFix::ALL {
            for on in [false, true] {
                let mut s = Settings::default();
                s.enhancements.fixes = BugFixes::all(!on);
                s.enhancements.fixes.set(f, on);
                let text = s.to_text();
                assert!(text.contains(&format!(
                    "\n{} = {}\n",
                    f.key(),
                    if on { "on" } else { "off" }
                )));
                assert!(!text.contains("minor_bug_fixes"));
                assert_eq!(Settings::parse(&text), s);
            }
            // That key alone: that fix alone changed.
            let p = Settings::parse(&format!("{} = off", f.key()));
            assert_eq!(p.enhancements.fixes, BugFixes::all_but(f));
            let p = Settings::parse(&format!("minor_bug_fixes = off\n{} = 1", f.key()));
            assert_eq!(p.enhancements.fixes, BugFixes::only(f));
            let p = Settings::parse(&format!("{} = no\nminor_bug_fixes = on", f.key()));
            assert_eq!(p.enhancements.fixes, BugFixes::all_but(f));
        }
        // An earlier version's file: its one key for every fix.
        let p = Settings::parse("minor_bug_fixes = off\nrestored_content = on");
        assert_eq!(p.enhancements.fixes, BugFixes::NONE);
        assert!(p.enhancements.restored_content);
        assert_eq!(
            Settings::parse("minor_bug_fixes = on").enhancements.fixes,
            BugFixes::ALL
        );
        // A bad value keeps the default.
        let p = Settings::parse("fix_music_loop = maybe");
        assert_eq!(p.enhancements.fixes, BugFixes::ALL);
    }
}
