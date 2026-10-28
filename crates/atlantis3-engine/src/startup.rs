//! The start-up's settings: `Atlantis3.cfg` over its defaults, then the
//! installation's language and medium, and the sizes of the screen and of
//! the warp view they give.
//!
//! The original's start-up (`Game.exe` @ 0x004d5e80) reads the file
//! (@ 0x00406ea0, `atlantis3_data::cfg`), then the registry key
//! `HKLM\SOFTWARE\CRYO\ATLANTIS 3`: `Install` becomes `Version` and
//! `Language` becomes `Langue` and `SousTitre`, whatever the file said
//! (@ 0x004d6148..0x004d642a). The port has no registry: it reads the values
//! the installer writes there. GOG.com's installer lists them in its
//! `goggame-<id>.script` (`setRegistry` actions: `Install` = `CD`,
//! `Language` = `fra` for its French install); without that file the
//! language is the only folder of `Datas\loc\text\`.

use atlantis3_data::cfg::Config;
use atlantis3_data::A3Files;

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

/// The registry values `Language` and `Install` of the installation: from
/// GOG.com's `goggame-<id>.script` (the `setRegistry` action of the
/// install's language, read from `goggame-<id>.info`), else the folder of
/// `Datas\loc\text\` and `CD`.
pub fn install_registry(files: &A3Files) -> (Vec<u8>, Vec<u8>) {
    let src = files.source();
    let names = src.list("");
    let read = |suffix: &str| {
        names
            .iter()
            .find(|n| {
                let l = n.to_ascii_lowercase();
                l.starts_with("goggame-") && l.ends_with(suffix)
            })
            .and_then(|n| src.read(n))
            .map(|d| String::from_utf8_lossy(&d).into_owned())
    };
    let install_langs = read(".info")
        .map(|t| json_strings_after(&t, "\"languages\""))
        .unwrap_or_default();
    let mut language = None;
    let mut install = None;
    if let Some(script) = read(".script") {
        for action in script.split("\"setRegistry\"").skip(1) {
            let name = json_string_after(action, "\"valueName\"");
            let data = json_string_after(action, "\"valueData\"");
            let langs = json_strings_after(action, "\"languages\"");
            let ours = langs.iter().any(|l| l == "*" || install_langs.contains(l));
            match (name.as_deref(), data) {
                (Some("Language"), Some(d)) if ours || language.is_none() => language = Some(d),
                (Some("Install"), Some(d)) => install = Some(d),
                _ => {}
            }
        }
    }
    let language = language.map(String::into_bytes).unwrap_or_else(|| {
        let folders = files.list("loc/text");
        if folders.len() != 1 {
            log::warn!("no install language: loc\\text\\ holds {folders:?}");
        }
        folders.first().cloned().unwrap_or_default().into_bytes()
    });
    let install = install
        .map(String::into_bytes)
        .unwrap_or_else(|| b"CD".to_vec());
    (language, install)
}

/// The string value after `key` (`"key": "value"`).
fn json_string_after(text: &str, key: &str) -> Option<String> {
    let rest = &text[text.find(key)? + key.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// The strings of the array after `key` (`"key": ["a", "b"]`).
fn json_strings_after(text: &str, key: &str) -> Vec<String> {
    let Some(at) = text.find(key) else {
        return Vec::new();
    };
    let rest = &text[at + key.len()..];
    let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) else {
        return Vec::new();
    };
    if close < open {
        return Vec::new();
    }
    rest[open + 1..close]
        .split(',')
        .filter_map(|s| {
            let s = s.trim().strip_prefix('"')?;
            Some(s[..s.find('"')?].to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_helpers() {
        let t = r#"{"valueData": "fra", "valueName": "Language"}, "languages": [ "fr-FR", "*" ]"#;
        assert_eq!(
            json_string_after(t, "\"valueData\"").as_deref(),
            Some("fra")
        );
        assert_eq!(
            json_string_after(t, "\"valueName\"").as_deref(),
            Some("Language")
        );
        assert_eq!(json_strings_after(t, "\"languages\""), ["fr-FR", "*"]);
        assert!(json_strings_after(t, "\"nothing\"").is_empty());
    }
}
