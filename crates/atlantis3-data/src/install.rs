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
//!
//! The Italian *Atlantis Collection* DVD (2014) installs the game with an
//! Inno Setup installer that writes `Install` = `CD` and `Language` =
//! `ita` (atlantis3-re `discs/it-collection.md`) and leaves no file of
//! them: its `Datas\loc\text\` holds `ita` alone, which gives the same
//! values.
//!
//! The North American CD (*Beyond Atlantis II*, DreamCatcher, 2001)
//! writes them under `HKLM\Software\DreamCatcher\BEYOND ATLANTIS 2`,
//! the key its program reads (atlantis3-cd-us-3366912 @ 0x004d5950):
//! `Language` = `usa` and `Install` = `CD` (its `setup.ins`); its
//! `Datas\loc\text\` holds `usa` alone, which gives the same values.
//!
//! The Portuguese CD (*O Novo Mundo*, Playgames, 2002) has Cryo's
//! installer of the French CD with a list of 15 language codes
//! (`setup.ins`); its `Datas\loc\text\` holds `por` alone. The Swedish
//! CD (*Den nya världen*, Toptronics, 2002) has the same installer, and
//! `swe`. The Czech CD (GAME4U Speciál 10, 2003) lists one code, `cze`,
//! the only folder of its `Datas\loc\text\`.
//!
//! The Traditional Chinese CD (亞特蘭提斯Ⅲ 失落的帝國, Interwise, 2002)
//! installs the same program with the same installer; its texts are in
//! `gbr`, in Big5, and its voices English: the registry value stays
//! `gbr`, and the edition's language is told by its data, its
//! `font\font.txt` asking for `CHINESEBIG5_CHARSET` ([`data_language`]).

use crate::exe::Build;
use crate::A3Files;

/// A language of the game's texts and voices, as the registry names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
    Italian,
    Portuguese,
    Swedish,
    Czech,
    /// The Traditional Chinese CD's: `gbr` with Big5 texts
    /// ([`data_language`]); never a registry code of its own.
    TraditionalChinese,
}

impl Language {
    /// The language of a registry `Language` value (`gbr`, `fra`, `ger`,
    /// `esp`, `ita`; the folder names of `loc\text\`, any case). The
    /// North American CD's `usa` is English too: its own texts and
    /// voices, the program's `usa` branches (the intro `intro_usa.hnm`).
    pub fn from_code(code: &[u8]) -> Option<Language> {
        match code.to_ascii_lowercase().as_slice() {
            b"gbr" | b"usa" => Some(Language::English),
            b"fra" => Some(Language::French),
            b"ger" => Some(Language::German),
            b"esp" => Some(Language::Spanish),
            b"ita" => Some(Language::Italian),
            b"por" => Some(Language::Portuguese),
            b"swe" => Some(Language::Swedish),
            b"cze" => Some(Language::Czech),
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
            Language::Italian => "Italian",
            Language::Portuguese => "Portuguese",
            Language::Swedish => "Swedish",
            Language::Czech => "Czech",
            Language::TraditionalChinese => "Traditional Chinese",
        }
    }
}

/// Editions that share a build and differ by their data's language: the
/// build, the install's language, the edition's whole name. The
/// Portuguese CD's `Atlantis3.exe` (atlantis3-cd-pt-3424256) is also the
/// Swedish CD's and the Traditional Chinese CD's (atlantis3-re
/// `discs/cd-sv.md`, `discs/cd-zh-tw.md`); each install's
/// `Datas\loc\text\` holds one language folder, `por` on the Portuguese
/// CD alone (`swe`, and `gbr` with Big5 texts, on the others), so the
/// language names the edition.
pub const LANGUAGE_EDITIONS: &[(&str, Language, &str)] = &[
    (
        "atlantis3-cd-pt-3424256",
        Language::Portuguese,
        "Atlantis III, Portuguese CD, Portuguese (Playgames, 2002)",
    ),
    (
        "atlantis3-cd-pt-3424256",
        Language::Swedish,
        "Atlantis III, Swedish CD, Swedish (Toptronics, 2002)",
    ),
    (
        "atlantis3-cd-pt-3424256",
        Language::TraditionalChinese,
        "Atlantis III, Taiwanese CD, Traditional Chinese (Interwise, 2002)",
    ),
];

/// Whether the install's `Datas\font\font.txt` puts a font in
/// `CHINESEBIG5_CHARSET` (the Traditional Chinese CD's, whose texts are
/// Big5 and drawn in the system's Big5 face).
pub fn has_big5_fonts(src: &dyn atlantis_data::FileSource) -> bool {
    src.read(crate::font_txt::FONT_TXT_PATH).is_some_and(|d| {
        crate::font_txt::FontTxt::parse(&d)
            .fonts
            .iter()
            .any(|f| f.charset_name == "CHINESEBIG5_CHARSET")
    })
}

/// The language of the install's texts: the registry value's
/// ([`Language::from_code`]), but Traditional Chinese for an English code
/// whose `font\font.txt` puts a font in `CHINESEBIG5_CHARSET` (the
/// Traditional Chinese CD's: all five, its `gbr` texts Big5; atlantis3-re
/// `discs/cd-zh-tw.md`). No other edition's `font.txt` asks for that
/// character set.
pub fn data_language(files: &A3Files, code: &[u8]) -> Option<Language> {
    let language = Language::from_code(code)?;
    if language != Language::English {
        return Some(language);
    }
    Some(if has_big5_fonts(files.source()) {
        Language::TraditionalChinese
    } else {
        language
    })
}

/// The edition's name: an edition of [`LANGUAGE_EDITIONS`], else the
/// build's name with the language ("Atlantis III, GOG.com, German",
/// [`named_label`]); an unknown language leaves the build's name alone.
pub fn edition_label(build: &Build, language: Option<Language>) -> String {
    if let Some(&(_, _, name)) = LANGUAGE_EDITIONS
        .iter()
        .find(|&&(id, l, _)| id == build.id && Some(l) == language)
    {
        return name.to_string();
    }
    named_label(build.label, language)
}

/// The edition `name` with the language, as [`edition_label`]: after
/// the name, or for a retail CD named with its publisher and year
/// ("Atlantis III, CD (DreamCatcher, 2001)") before them, as
/// `atlantis2_data::edition::edition_label` does ("Atlantis III, CD,
/// English (DreamCatcher, 2001)").
pub fn named_label(name: &str, language: Option<Language>) -> String {
    match (language, name.find("CD (")) {
        (Some(l), Some(i)) => format!("{}, {}{}", &name[..i + 2], l.name(), &name[i + 2..]),
        (Some(l), None) => format!("{name}, {}", l.name()),
        (None, _) => name.to_string(),
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
        assert_eq!(Language::from_code(b"ITA"), Some(Language::Italian));
        assert_eq!(Language::from_code(b"USA"), Some(Language::English));
        assert_eq!(Language::from_code(b"por"), Some(Language::Portuguese));
        assert_eq!(Language::from_code(b"SWE"), Some(Language::Swedish));
        assert_eq!(Language::from_code(b"cze"), Some(Language::Czech));
        assert_eq!(Language::from_code(b"rus"), None);
        assert_eq!(edition_label(b, None), "Atlantis III, GOG.com");
        assert_eq!(
            named_label("Atlantis III, Atlantis Collection", Some(Language::Italian)),
            "Atlantis III, Atlantis Collection, Italian"
        );
        assert_eq!(
            named_label(
                "Atlantis III, CD (DreamCatcher, 2001)",
                Some(Language::English)
            ),
            "Atlantis III, CD, English (DreamCatcher, 2001)"
        );
        let pt = KNOWN_BUILDS
            .iter()
            .find(|b| b.id == "atlantis3-cd-pt-3424256")
            .unwrap();
        assert_eq!(
            edition_label(pt, Some(Language::Portuguese)),
            "Atlantis III, Portuguese CD, Portuguese (Playgames, 2002)"
        );
        assert_eq!(
            edition_label(pt, Some(Language::Swedish)),
            "Atlantis III, Swedish CD, Swedish (Toptronics, 2002)"
        );
        assert_eq!(
            edition_label(pt, Some(Language::TraditionalChinese)),
            "Atlantis III, Taiwanese CD, Traditional Chinese (Interwise, 2002)"
        );
        assert_eq!(
            edition_label(pt, Some(Language::English)),
            "Atlantis III, CD, English (October 2001 build)"
        );
        assert_eq!(
            named_label("Atlantis III, CD", Some(Language::French)),
            "Atlantis III, CD, French"
        );
        assert_eq!(
            named_label(
                "Atlantis III, GOG.com (2015 / 2018 build)",
                Some(Language::Italian)
            ),
            "Atlantis III, GOG.com (2015 / 2018 build), Italian"
        );
    }
}
