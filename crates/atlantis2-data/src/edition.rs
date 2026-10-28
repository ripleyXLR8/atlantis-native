//! The language of a game folder, for the edition's name.
//!
//! The executable holds no language: every text, voice and font is in
//! `loc\` and `datas_a2\apc\dial\`, and GOG.com ships one `Game.exe` for
//! its English, French, German and Spanish installations (the same file
//! in each build's language-independent depot). So the language is told
//! by the data: the MD5 of `loc\Menu.txt`, which differs between every
//! known edition's languages. A file not listed here (another release,
//! an edited file) names no language, and the edition keeps the build's
//! own name.
//!
//! The GOG.com values are the per-language depots' of GOG's builds
//! "1.0" (2018-05-04), "1.0 w10" (2022-06-21) and "1.0 w10 hotfix"
//! (2022-07-13): the three builds ship the same `loc\` files.

use md5::{Digest, Md5};

use crate::exe::Build;

/// A language of the game's texts and voices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
}

impl Language {
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

/// The known `loc\Menu.txt` files: size, MD5, language.
pub const KNOWN_MENU_TXT: [(u64, [u8; 16], Language); 5] = [
    // The French CD's (1999), installed.
    (
        1371,
        [
            0x89, 0x32, 0xa3, 0x28, 0x10, 0x58, 0xd7, 0x5f, 0x7d, 0x13, 0xe3, 0x21, 0x60, 0x41,
            0x00, 0x02,
        ],
        Language::French,
    ),
    // GOG.com, French (asks for "le DVD").
    (
        1366,
        [
            0xd2, 0x41, 0x4c, 0x05, 0xfc, 0x85, 0x2a, 0xa5, 0x11, 0x8f, 0x8e, 0xc7, 0x10, 0x57,
            0xae, 0xbe,
        ],
        Language::French,
    ),
    // GOG.com, English.
    (
        1144,
        [
            0xc5, 0x8c, 0x88, 0xb8, 0x29, 0xfa, 0x72, 0xfd, 0x34, 0x57, 0x22, 0x82, 0x3c, 0x22,
            0x4e, 0xf6,
        ],
        Language::English,
    ),
    // GOG.com, German.
    (
        1184,
        [
            0x6e, 0xb3, 0x62, 0xc3, 0xe9, 0xca, 0x61, 0x97, 0x62, 0xb1, 0x77, 0x99, 0x57, 0xcd,
            0x52, 0xc4,
        ],
        Language::German,
    ),
    // GOG.com, Spanish.
    (
        1240,
        [
            0x67, 0xbb, 0x8b, 0xd3, 0xb2, 0xdc, 0x54, 0xec, 0x76, 0x67, 0x6c, 0x00, 0x01, 0xdd,
            0x1c, 0x7d,
        ],
        Language::Spanish,
    ),
];

/// The language of a `loc\Menu.txt`, `None` when it is no known file.
pub fn language_of_menu_txt(data: &[u8]) -> Option<Language> {
    let size = data.len() as u64;
    if !KNOWN_MENU_TXT.iter().any(|&(s, _, _)| s == size) {
        return None;
    }
    let md5: [u8; 16] = Md5::digest(data).into();
    KNOWN_MENU_TXT
        .iter()
        .find(|&&(s, m, _)| s == size && m == md5)
        .map(|&(_, _, l)| l)
}

/// The edition's name: the build's, with the language after the
/// release's name when the build's name does not say it already
/// ("Atlantis II, GOG.com, German (patch 1.01)"; the French CD's build
/// is French only).
pub fn edition_label(build: &Build, language: Option<Language>) -> String {
    let label = build.label;
    match (language, label.find(" (")) {
        (Some(l), Some(i)) if !label.contains(l.name()) => {
            format!("{}, {}{}", &label[..i], l.name(), &label[i..])
        }
        _ => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe::KNOWN_BUILDS;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn known_menus() {
        let md5s: Vec<String> = KNOWN_MENU_TXT.iter().map(|(_, m, _)| hex(m)).collect();
        assert_eq!(
            md5s,
            [
                "8932a3281058d75f7d13e32160410002",
                "d2414c05fc852aa5118f8ec71057aebe",
                "c58c88b829fa72fd345722823c224ef6",
                "6eb362c3e9ca619762b1779957cd52c4",
                "67bb8bd3b2dc54ec76676c0001dd1c7d",
            ]
        );
        // A file of a known size but other bytes names no language.
        assert_eq!(language_of_menu_txt(&[b' '; 1144]), None);
        assert_eq!(language_of_menu_txt(b"[MainMenu]\r\n"), None);
    }

    /// The French CD's and GOG's French `Menu.txt` on the real files.
    #[test]
    fn french_folders() {
        let (Some(cd), Some(gog)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_GOG"),
        ) else {
            crate::testing::missing("ATLANTIS2_DATA / ATLANTIS2_GOG not set");
            return;
        };
        for dir in [cd, gog] {
            let menu = std::path::Path::new(&dir).join("loc").join("Menu.txt");
            let data = std::fs::read(&menu).unwrap();
            assert_eq!(
                language_of_menu_txt(&data),
                Some(Language::French),
                "{}",
                menu.display()
            );
        }
    }

    #[test]
    fn labels() {
        let [cd, patch, gog] = &KNOWN_BUILDS;
        assert_eq!(
            edition_label(gog, Some(Language::German)),
            "Atlantis II, GOG.com, German (patch 1.01)"
        );
        assert_eq!(
            edition_label(gog, None),
            "Atlantis II, GOG.com (patch 1.01)"
        );
        assert_eq!(
            edition_label(patch, Some(Language::French)),
            "Atlantis II, patch 1.01, French (1999)"
        );
        assert_eq!(
            edition_label(cd, Some(Language::French)),
            "Atlantis II, French CD (1999)"
        );
    }
}
