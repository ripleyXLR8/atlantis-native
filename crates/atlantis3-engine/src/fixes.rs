//! The port's bug fixes for Atlantis III, each a switch of its own: on by
//! default, all off with `--original` (the original game, bug for bug).
//! They are kept in `settings.ini` beside the saves and `Atlantis3.cfg`
//! (the `atlantis3` folder of the port's data folder), one `key = value`
//! line each, as the other games' fixes: `#` comments, unknown keys and
//! bad values ignored with a warning. Atlantis III has no options menu of
//! the port's: the file is the only way to change them, and the program
//! writes it with the defaults when there is none.
//!
//! - `fix_warpres_cd` ([`Fixes::warpres_cd`]): GOG.com's Spanish
//!   `Datas\Atlantis3.cfg` says `WarpRes = 1024`, and no GOG.com install
//!   ships the 1024 / 800 folders. The original reads the file at start-up
//!   (@ 0x004d5e9c → 0x00406ea0) and every path builder follows WarpRes
//!   (0x00789d34): the panorama faces' debug colours, no place animation,
//!   no film but the intro, until the player opens the options page, whose
//!   `Version = CD` branch sets WarpRes to 512 (@ 0x00456041). With the
//!   fix, that same rule runs at start-up, before any cube or path is
//!   built: `Version = CD` (every GOG.com install), or no
//!   `Datas\warp\Warp1024\` folder, gives WarpRes 512 and the 640 x 480
//!   warp view ([`Fixes::apply`]). The file is left as it is until the
//!   menus write it, as the original's next write does (with 512).

use atlantis3_data::cfg::Config;
use atlantis3_data::A3Files;

/// The settings file's name.
pub const SETTINGS_FILE: &str = "settings.ini";

/// A fix's switch in [`Fixes`].
type Field = fn(&mut Fixes) -> &mut bool;

/// The bug fixes, each on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixes {
    /// `fix_warpres_cd`: the options page's `Version = CD` rule (and a
    /// missing `warp\Warp1024\`) applied at start-up ([`Fixes::apply`]).
    pub warpres_cd: bool,
}

impl Default for Fixes {
    /// Every fix on.
    fn default() -> Fixes {
        Fixes { warpres_cd: true }
    }
}

impl Fixes {
    /// Every fix off: the original game (`--original`).
    pub fn original() -> Fixes {
        Fixes { warpres_cd: false }
    }

    /// The keys of `settings.ini`, with their fields.
    const KEYS: [(&'static str, Field); 1] = [("fix_warpres_cd", |f| &mut f.warpres_cd)];

    /// `key = value` lines over the defaults; unknown keys and bad values
    /// keep the default.
    pub fn parse(text: &str) -> Fixes {
        let mut out = Fixes::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let value = line.split_once('=').and_then(|(k, v)| {
                let k = k.trim().to_ascii_lowercase();
                let field = Fixes::KEYS.iter().find(|(key, _)| *key == k)?.1;
                let b = match v.trim().to_ascii_lowercase().as_str() {
                    "1" | "true" | "on" | "yes" => true,
                    "0" | "false" | "off" | "no" => false,
                    _ => return None,
                };
                Some((field, b))
            });
            match value {
                Some((field, b)) => *field(&mut out) = b,
                None => log::warn!("{SETTINGS_FILE}: '{line}' not understood"),
            }
        }
        out
    }

    /// The file's text: a comment, then one line per fix.
    pub fn to_text(&self) -> String {
        let mut s = String::from(
            "# Atlantis Native: Atlantis III's bug fixes (1 = on, 0 = the original's behaviour)\n",
        );
        let mut f = *self;
        for (key, field) in Fixes::KEYS {
            s.push_str(&format!("{key} = {}\n", u8::from(*field(&mut f))));
        }
        s
    }

    /// The start-up's settings with the fixes applied, before any cube or
    /// path is built. `fix_warpres_cd`: when `Version` is `CD` (the
    /// options page's rule, @ 0x00455f25..0x004560bf) or the install has
    /// no `warp\Warp1024\` folder (any case), WarpRes := 512 and the warp
    /// view := 640 x 480, what the original reaches at the first visit of
    /// its options page (and keeps from the next start, its menus having
    /// written `WarpRes = 512`).
    pub fn apply(&self, config: &mut Config, files: &A3Files) {
        if self.warpres_cd
            && config.warp_res != 512
            && (config.version == b"CD" || !files.has_dir("warp/Warp1024"))
        {
            log::info!(
                "fix_warpres_cd: WarpRes {} → 512 (Version {}, warp\\Warp1024 {})",
                config.warp_res,
                String::from_utf8_lossy(&config.version),
                if files.has_dir("warp/Warp1024") {
                    "present"
                } else {
                    "missing"
                }
            );
            config.warp_res = 512;
            config.view = Config::view_of(512);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::files::MemFiles;
    use std::sync::Arc;

    fn files(paths: &[&str]) -> A3Files {
        let mut m = MemFiles::default();
        for p in paths {
            m.insert(p, b"x".to_vec());
        }
        A3Files::new(Arc::new(m))
    }

    fn spanish(version: &[u8]) -> Config {
        let mut c = Config::parse(b"WarpRes\t\t=\t1024\r\nPixelFormat\t=\t32bits");
        c.apply_install(b"esp", version);
        c
    }

    #[test]
    fn keys_round_trip() {
        assert_eq!(Fixes::parse(&Fixes::default().to_text()), Fixes::default());
        assert_eq!(
            Fixes::parse(&Fixes::original().to_text()),
            Fixes::original()
        );
        assert!(!Fixes::parse("FIX_WARPRES_CD = off\nother = 1\n").warpres_cd);
        assert!(Fixes::parse("fix_warpres_cd = maybe\n").warpres_cd);
    }

    /// On: `Version = CD` (GOG.com) gives 512 and 640 x 480 at start-up,
    /// with or without the 1024 folder.
    #[test]
    fn on_cd_gives_512() {
        for have in [false, true] {
            let f = files(if have {
                &["Datas/warp/Warp1024/hoggar/ho0001.hnm"]
            } else {
                &["Datas/warp/Warp512/hoggar/ho0001.hnm"]
            });
            let mut c = spanish(b"CD");
            Fixes::default().apply(&mut c, &f);
            assert_eq!((c.warp_res, c.warp_view()), (512, (640, 480)));
        }
    }

    /// On, another medium: 1024 is kept when `warp\Warp1024\` exists
    /// (any case), else 512.
    #[test]
    fn on_dvd_follows_the_folder() {
        let mut c = spanish(b"DVD");
        Fixes::default().apply(&mut c, &files(&["Datas/WARP/warp1024/a.hnm"]));
        assert_eq!((c.warp_res, c.warp_view()), (1024, (800, 600)));
        let mut c = spanish(b"DVD");
        Fixes::default().apply(&mut c, &files(&["Datas/warp/Warp512/a.hnm"]));
        assert_eq!((c.warp_res, c.warp_view()), (512, (640, 480)));
    }

    /// Off: the reader's 1024 and 800 x 600 stay, as in the original.
    #[test]
    fn off_keeps_the_file() {
        let mut c = spanish(b"CD");
        Fixes::original().apply(&mut c, &files(&["Datas/warp/Warp512/a.hnm"]));
        assert_eq!((c.warp_res, c.warp_view()), (1024, (800, 600)));
    }
}
