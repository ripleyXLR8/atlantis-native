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
//! (2022-07-13): the three builds ship the same `loc\` files, checked
//! on the four languages' installations. The North American CD's differs
//! from GOG's English by one line. The Italian *Atlantis Collection*'s is
//! Italian's only known one: no GOG.com depot of Atlantis II holds Italian.
//! The Polish CD's (1999) is Polish's only known one.
//!
//! The language also tells the code page the texts are read in
//! ([`Language::text_code_page`]): the original passes the bytes of
//! `loc\*.txt` to `TextOutA` with fonts made in `DEFAULT_CHARSET`
//! (`CreateFontA` @ 0x00404cc8; atlantis2-cd-pl-2183168 @ 0x0046f2a1), so
//! they are read in the system's ANSI code page, the one each edition was
//! made for: Windows-1252 for the Western ones, Windows-1250 for the
//! Polish CD (atlantis2-re `binaries/atlantis2-cd-pl-2183168.md`, "Text
//! code page").

use atlantis_data::TextCodePage;
use md5::{Digest, Md5};

use crate::exe::Build;

/// A language of the game's texts and voices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
    Italian,
    Polish,
}

impl Language {
    /// Its name in English, as the editions are named.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "French",
            Language::German => "German",
            Language::Spanish => "Spanish",
            Language::Italian => "Italian",
            Language::Polish => "Polish",
        }
    }

    /// The code page its texts are read in: the system code page of the
    /// Windows the edition was sold for. Windows-1250 for Polish (the
    /// Polish CD's `loc\*.txt` use 0xb9 `ą`, 0xb3 `ł`, 0xbf `ż`, 0x9c `ś`,
    /// 0xe6 `ć`, 0xf1 `ń`, 0xea `ę`, 0x9f `ź`...), Windows-1252 for the
    /// others.
    pub fn text_code_page(self) -> TextCodePage {
        match self {
            Language::Polish => TextCodePage::Windows1250,
            Language::English
            | Language::French
            | Language::German
            | Language::Spanish
            | Language::Italian => TextCodePage::Windows1252,
        }
    }
}

/// The code page of the texts of a folder whose language is `language`:
/// the language's, Windows-1252 when it is not known (every edition but
/// the Polish CD).
pub fn text_code_page(language: Option<Language>) -> TextCodePage {
    language.map_or(TextCodePage::Windows1252, Language::text_code_page)
}

/// The known `loc\Menu.txt` files: size, MD5, language.
pub const KNOWN_MENU_TXT: [(u64, [u8; 16], Language); 8] = [
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
    // The North American CD (*Beyond Atlantis*, DreamCatcher, 2000): GOG's
    // English but for line 72 (asks for "any Atlantis CD").
    (
        1141,
        [
            0xbd, 0xb1, 0xf4, 0x7e, 0x47, 0x2f, 0x44, 0xa3, 0x61, 0x7a, 0xab, 0xaa, 0xe9, 0xed,
            0x60, 0xf7,
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
    // The Italian *Atlantis Collection* DVD (2014).
    (
        1169,
        [
            0x78, 0x5d, 0xd7, 0xa4, 0x66, 0x78, 0x51, 0x97, 0xb8, 0x14, 0x18, 0xb0, 0x8c, 0x69,
            0xbd, 0xef,
        ],
        Language::Italian,
    ),
    // The Polish CD (1999), installed (`loc\MENU.TXT`, Windows-1250).
    (
        1106,
        [
            0x12, 0xeb, 0x5e, 0x1c, 0x33, 0x79, 0xda, 0xc9, 0xe2, 0xcc, 0xb1, 0x61, 0x56, 0xa9,
            0x71, 0xc1,
        ],
        Language::Polish,
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
                "bdb1f47e472f44a3617aabaae9ed60f7",
                "6eb362c3e9ca619762b1779957cd52c4",
                "67bb8bd3b2dc54ec76676c0001dd1c7d",
                "785dd7a466785197b81418b08c69bdef",
                "12eb5e1c3379dac9e2ccb16156a971c1",
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
        let [cd, patch, gog, w10, _, us, collection, pl] = &KNOWN_BUILDS;
        assert_eq!(
            edition_label(gog, Some(Language::German)),
            "Atlantis II, GOG.com, German (patch 1.01)"
        );
        assert_eq!(
            edition_label(gog, None),
            "Atlantis II, GOG.com (patch 1.01)"
        );
        assert_eq!(
            edition_label(w10, Some(Language::English)),
            "Atlantis II, GOG.com, English (patch 1.01, June 2022 build)"
        );
        assert_eq!(
            edition_label(patch, Some(Language::French)),
            "Atlantis II, patch 1.01, French (1999)"
        );
        assert_eq!(
            edition_label(cd, Some(Language::French)),
            "Atlantis II, French CD (1999)"
        );
        assert_eq!(
            edition_label(us, Some(Language::English)),
            "Atlantis II, CD, English (DreamCatcher, 2000)"
        );
        assert_eq!(
            edition_label(collection, Some(Language::Italian)),
            "Atlantis II, Italian Collection (patch 1.01)"
        );
        assert_eq!(
            edition_label(pl, Some(Language::Polish)),
            "Atlantis II, Polish CD (1999)"
        );
    }

    #[test]
    fn code_pages() {
        assert_eq!(
            text_code_page(Some(Language::Polish)),
            TextCodePage::Windows1250
        );
        for l in [
            Language::English,
            Language::French,
            Language::German,
            Language::Spanish,
            Language::Italian,
        ] {
            assert_eq!(text_code_page(Some(l)), TextCodePage::Windows1252);
        }
        assert_eq!(text_code_page(None), TextCodePage::Windows1252);
        // The Polish letters at their Windows-1250 bytes.
        let pl = TextCodePage::Windows1250.decode(&[
            0xb9, 0xea, 0xb3, 0xbf, 0x9c, 0xe6, 0xf1, 0xf3, 0x9f, 0xa5, 0xca, 0xa3, 0xaf, 0x8c,
            0xc6, 0xd1, 0xd3, 0x8f,
        ]);
        assert_eq!(pl, "ąęłżśćńóźĄĘŁŻŚĆŃÓŹ");
    }
}
