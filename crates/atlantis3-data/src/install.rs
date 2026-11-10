//! The installation's registry values and its language, for the start-up
//! and for the edition's name.
//!
//! The original's start-up (`Game.exe` @ 0x004d5e80) reads the registry
//! key `HKLM\SOFTWARE\CRYO\ATLANTIS 3`: `Install` becomes `Version` and
//! `Language` becomes `Langue` and `SousTitre`, whatever `Atlantis3.cfg`
//! said (@ 0x004d6148..0x004d642a). The port has no registry: it reads the
//! values the installer writes there. GOG.com's installer lists them in its
//! `goggame-<id>.script` (`setRegistry` actions: `Install` = `CD`,
//! `Language` = `fra`, `gbr`, `ger` or `esp`); without that file the
//! language is the only folder of `Datas\loc\text\`.
//!
//! GOG.com sells the game in English, French, German and Spanish with one
//! `Game.exe` for all four (the same file in the build's
//! language-independent depot): the language is the data's, and that
//! registry value is what the game itself goes by.

use crate::exe::Build;
use crate::A3Files;

/// A language of the game's texts and voices, as the registry names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
}

impl Language {
    /// The language of a registry `Language` value (`gbr`, `fra`, `ger`,
    /// `esp`; the folder names of `loc\text\`, any case).
    pub fn from_code(code: &[u8]) -> Option<Language> {
        match code.to_ascii_lowercase().as_slice() {
            b"gbr" => Some(Language::English),
            b"fra" => Some(Language::French),
            b"ger" => Some(Language::German),
            b"esp" => Some(Language::Spanish),
            _ => None,
        }
    }

    /// Its name in English, as the editions are named.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "French",
            Language::German => "German",
            Language::Spanish => "Spanish",
        }
    }
}

/// The edition's name: the build's, then the language
/// ("Atlantis III, GOG.com, German"); an unknown language leaves the
/// build's name alone.
pub fn edition_label(build: &Build, language: Option<Language>) -> String {
    match language {
        Some(l) => format!("{}, {}", build.label, l.name()),
        None => build.label.to_string(),
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
    use crate::exe::KNOWN_BUILDS;

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

    #[test]
    fn languages_and_labels() {
        let b = &KNOWN_BUILDS[0];
        for (code, name) in [
            (&b"gbr"[..], "English"),
            (b"FRA", "French"),
            (b"ger", "German"),
            (b"esp", "Spanish"),
        ] {
            let l = Language::from_code(code);
            assert_eq!(l.map(Language::name), Some(name));
            assert_eq!(
                edition_label(b, l),
                format!("Atlantis III, GOG.com, {name}")
            );
        }
        assert_eq!(Language::from_code(b"ita"), None);
        assert_eq!(edition_label(b, None), "Atlantis III, GOG.com");
    }
}
