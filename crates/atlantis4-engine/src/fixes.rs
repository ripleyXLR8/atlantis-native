//! The port's bug fixes for *Atlantis Evolution* (the GOG.com program,
//! atlantis4-gog-en-2363910), each a switch of its own: on by default, all
//! off with `--original` (the original game, bug for bug). They are kept
//! in the port's `settings.ini` (the `atlantis4` folder of the port's data
//! folder, beside the language, renderer and full-screen keys; in the
//! browser, the page's storage), one `key = value` line each, as the other
//! games' fixes: `#` comments, unknown keys and bad values ignored with a
//! warning, the file written with the defaults when there is none.
//!
//! - `fix_skip_films` ([`Fixes::skip_films`]): the original has no key to
//!   skip a `PlayMovie` film. Its only skip is the movie's own button: a
//!   click anywhere shows `skipVideoButton` (top right), a click on it
//!   stops the video (@ 0x005220b0, `TeFrameAnim` stop @ 0x004f5ab0); no
//!   key callback is ever instantiated (atlantis4-re
//!   `functions/Game-playMovie-FUN_005232b0.md`, Keys; live 2026-11-12:
//!   Esc does nothing). With the fix, **Escape** or **Space** does what
//!   the skip button's validated click does: the video sprite's stop,
//!   whose "stopped" signal runs the movie's one end (@ 0x00524740: the
//!   layouts hidden, the `video` channel stopped, the music from its
//!   start, `OnMovieFinished(<path>)`), the same end a film that plays to
//!   its last frame runs ([`crate::Game::skip_movie`]). The click and the
//!   button are unchanged.

/// The settings file's name (`settings.ini`).
pub const SETTINGS_FILE: &str = "settings.ini";

/// A fix's switch in [`Fixes`].
type Field = fn(&mut Fixes) -> &mut bool;

/// The bug fixes, each on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixes {
    /// `fix_skip_films`: Escape or Space skips a `PlayMovie` film as the
    /// skip button does.
    pub skip_films: bool,
}

impl Default for Fixes {
    /// Every fix on.
    fn default() -> Fixes {
        Fixes { skip_films: true }
    }
}

impl Fixes {
    /// Every fix off: the original game (`--original`).
    pub fn original() -> Fixes {
        Fixes { skip_films: false }
    }

    /// The keys of `settings.ini`, with their fields.
    const KEYS: [(&'static str, Field); 1] = [("fix_skip_films", |f| &mut f.skip_films)];

    /// One `key = value` pair: `Some(true)` when it is a fix's key with an
    /// on / off value (then set), `Some(false)` for a fix's key with a bad
    /// value (the setting kept), `None` when `key` is no fix's.
    pub fn set(&mut self, key: &str, value: &str) -> Option<bool> {
        let k = key.trim().to_ascii_lowercase();
        let field = Fixes::KEYS.iter().find(|(name, _)| *name == k)?.1;
        let b = match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" => true,
            "0" | "false" | "off" | "no" => false,
            _ => return Some(false),
        };
        *field(self) = b;
        Some(true)
    }

    /// `key = value` lines over the defaults; other keys, unknown keys and
    /// bad values keep the default (with a warning).
    pub fn parse(text: &str) -> Fixes {
        let mut out = Fixes::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let ok = line
                .split_once('=')
                .and_then(|(k, v)| out.set(k, v))
                .unwrap_or(false);
            if !ok {
                log::warn!("{SETTINGS_FILE}: '{line}' not understood");
            }
        }
        out
    }

    /// The fixes' lines: a comment, then `key = 0|1` for each.
    pub fn lines(&self) -> String {
        let mut s = String::from(
            "# Bug fixes (1 = on, 0 = the original's behaviour; --original turns them all off)\n\
             # fix_skip_films: Escape or Space skips a film, as its skip button does\n",
        );
        let mut f = *self;
        for (key, field) in Fixes::KEYS {
            s.push_str(&format!("{key} = {}\n", u8::from(*field(&mut f))));
        }
        s
    }

    /// The file's text when it holds the fixes only (the browser's).
    pub fn to_text(&self) -> String {
        format!(
            "# Atlantis Native: Atlantis Evolution's bug fixes\n{}",
            self.lines()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        assert_eq!(Fixes::parse(&Fixes::default().to_text()), Fixes::default());
        assert_eq!(
            Fixes::parse(&Fixes::original().to_text()),
            Fixes::original()
        );
        assert!(Fixes::default().skip_films);
        assert!(!Fixes::original().skip_films);
        assert!(!Fixes::parse("FIX_SKIP_FILMS = off\nother = 1\n").skip_films);
        assert!(Fixes::parse("fix_skip_films = maybe\n").skip_films);
        let mut f = Fixes::default();
        assert_eq!(f.set("fix_skip_films", " no "), Some(true));
        assert!(!f.skip_films);
        assert_eq!(f.set("fix_skip_films", "?"), Some(false));
        assert_eq!(f.set("language", "fr"), None);
    }
}
