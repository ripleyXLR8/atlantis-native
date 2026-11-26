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
//! The Polish CD's (1999) is Polish's only known one. The German CD's
//! (1999) differs from GOG's German by one line (it asks for the CD), and
//! so does the Spanish CD's (1999) from GOG's Spanish. The Swedish CD's
//! (1999) is Swedish's only known one: no GOG.com depot of Atlantis II
//! holds Swedish. The Czech CD's (2000, reissued in 2002 as *GAME4U
//! Speciál 3*) is Czech's only known one. The Traditional Chinese CD's
//! (Interwise, 2000) is Traditional Chinese's only known one; the English
//! disc 1 sold with it installs GOG's English but for the line that asks
//! for the CD ("any Atlantis 2 CD").
//!
//! The Czech CD ships the North American CD's executable byte for byte
//! (atlantis2-re `binaries/atlantis2-cd-us-2199552.md`), so its build is
//! that one; the language tells the edition apart, and [`edition_label`]
//! names it after its own release ([`LANGUAGE_EDITIONS`]). So do the two
//! disc 1s of the Interwise release: the Traditional Chinese one installs
//! the 1.01 patch's executable byte for byte, the English one the Polish
//! CD's (atlantis2-re `binaries/atlantis2-cd-zh-tw-2183168.md`).
//!
//! The language also tells the code page the texts are read in
//! ([`Language::text_code_page`]): the original passes the bytes of
//! `loc\*.txt` to `TextOutA` with fonts made in `DEFAULT_CHARSET`
//! (`CreateFontA` @ 0x00404cc8; atlantis2-cd-pl-2183168 @ 0x0046f2a1), so
//! they are read in the system's ANSI code page, the one each edition was
//! made for: Windows-1252 for the Western ones, Windows-1250 for the
//! Polish and Czech CDs (atlantis2-re `binaries/atlantis2-cd-pl-2183168.md`,
//! "Text code page"; the Czech CD's program makes its fonts the same way,
//! `CreateFontA` with `DEFAULT_CHARSET`), code page 950 (Big5) for the
//! Traditional Chinese CD, whose `Font.txt` says `[CharacterSet] MBCS`.

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
    Swedish,
    Czech,
    TraditionalChinese,
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
            Language::Swedish => "Swedish",
            Language::Czech => "Czech",
            Language::TraditionalChinese => "Traditional Chinese",
        }
    }

    /// The InstallShield language id an installer made for it writes in
    /// its disc's `setup.lid` (`Default=`): the Windows primary language
    /// (`0009` on the English disc 1 of the Interwise release, `0005` on
    /// the Czech CD), the whole language id for Traditional Chinese
    /// (`0404`, its Chinese disc 1).
    pub fn installer_id(self) -> u16 {
        match self {
            Language::English => 0x0009,
            Language::French => 0x000c,
            Language::German => 0x0007,
            Language::Spanish => 0x000a,
            Language::Italian => 0x0010,
            Language::Polish => 0x0015,
            Language::Swedish => 0x001d,
            Language::Czech => 0x0005,
            Language::TraditionalChinese => 0x0404,
        }
    }

    /// The code page its texts are read in: the system code page of the
    /// Windows the edition was sold for. Windows-1250 for Polish (the
    /// Polish CD's `loc\*.txt` use 0xb9 `ą`, 0xb3 `ł`, 0xbf `ż`, 0x9c `ś`,
    /// 0xe6 `ć`, 0xf1 `ń`, 0xea `ę`, 0x9f `ź`...), Windows-1252 for the
    /// others (the Swedish CD's use 0xe5 `å`, 0xe4 `ä`, 0xf6 `ö`, 0xc5 `Å`,
    /// 0xc4 `Ä`, 0xd6 `Ö`). Windows-1250 for Czech too (the Czech CD's use
    /// 0x9a `š`, 0x9e `ž`, 0x9d `ť`, 0xe8 `č`, 0xec `ě`, 0xf8 `ř`, 0xf9 `ů`,
    /// 0xef `ď`, 0xf2 `ň`...; its manual is `charset=windows-1250`).
    /// Code page 950 (Big5) for Traditional Chinese: the Traditional Chinese CD's
    /// texts are Big5 pairs (lead bytes 0xa1..0xf9; its `Font.txt` breaks
    /// lines at A1 41 `，`, A1 43 `。`, A1 48 `？`, A1 49 `！`), drawn on a
    /// Traditional Chinese Windows.
    pub fn text_code_page(self) -> TextCodePage {
        match self {
            Language::Polish | Language::Czech => TextCodePage::Windows1250,
            Language::TraditionalChinese => TextCodePage::Windows950,
            Language::English
            | Language::French
            | Language::German
            | Language::Spanish
            | Language::Italian
            | Language::Swedish => TextCodePage::Windows1252,
        }
    }
}

/// The code page of the texts of a folder whose language is `language`:
/// the language's, Windows-1252 when it is not known (every edition but
/// the Polish, Czech and Traditional Chinese CDs).
pub fn text_code_page(language: Option<Language>) -> TextCodePage {
    language.map_or(TextCodePage::Windows1252, Language::text_code_page)
}

/// The known `loc\Menu.txt` files: size, MD5, language.
pub const KNOWN_MENU_TXT: [(u64, [u8; 16], Language); 14] = [
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
    // The German CD (1999), installed (`loc\menu.txt`): GOG's German but
    // for the line that asks for the CD.
    (
        1183,
        [
            0xdb, 0x4a, 0x62, 0x1e, 0xe8, 0xbe, 0xe5, 0xc2, 0xc1, 0x55, 0xcf, 0x43, 0xbf, 0xdc,
            0x44, 0x2a,
        ],
        Language::German,
    ),
    // The Spanish CD (1999), installed (`loc\Menu.txt`): GOG's Spanish but
    // for the line that asks for the CD.
    (
        1239,
        [
            0x06, 0x09, 0xb0, 0xb5, 0x99, 0x04, 0xd1, 0xea, 0x7e, 0x00, 0x1f, 0xbc, 0x19, 0x86,
            0x03, 0x6b,
        ],
        Language::Spanish,
    ),
    // The Swedish CD (1999), installed (`loc\Menu.txt`).
    (
        1043,
        [
            0x04, 0x12, 0x55, 0x98, 0x0d, 0x18, 0xb1, 0xad, 0x86, 0x69, 0x11, 0xd5, 0x75, 0x30,
            0x1f, 0x26,
        ],
        Language::Swedish,
    ),
    // The Czech CD (2000; *GAME4U Speciál 3*, 2002), installed
    // (`loc\Menu.txt`, Windows-1250).
    (
        1207,
        [
            0xec, 0x3a, 0xf0, 0x76, 0x2a, 0x15, 0xb5, 0x7a, 0x95, 0xde, 0xad, 0xcf, 0xce, 0xb0,
            0xbc, 0x87,
        ],
        Language::Czech,
    ),
    // The Traditional Chinese CD (Interwise, 2000), installed from its
    // Chinese disc 1 (`loc\Menu.txt`, Big5).
    (
        928,
        [
            0x6e, 0x3f, 0xdc, 0x9a, 0x71, 0xde, 0x77, 0xe6, 0x77, 0xf3, 0x58, 0x76, 0x34, 0x38,
            0x3d, 0x6a,
        ],
        Language::TraditionalChinese,
    ),
    // The same release's English disc 1, installed (`loc\Menu.txt`): GOG's
    // English but for line 72 (asks for "any Atlantis 2 CD").
    (
        1143,
        [
            0x7c, 0x62, 0x42, 0xd7, 0x95, 0xdb, 0xab, 0x5d, 0x3d, 0x67, 0xde, 0x81, 0xc4, 0x92,
            0x09, 0x6a,
        ],
        Language::English,
    ),
];

/// Editions whose build is another edition's: (the build's id, the
/// language, the edition's name). The Czech CD's `atlantis2.exe` is the
/// North American CD's file (MD5 `0f634c46…`, linked 2000-04-05): its
/// Czech `loc\` makes it the Czech CD, named after its own release and
/// not DreamCatcher's. The Interwise release (Taiwan, 2000) has two disc 1s, each
/// with its installer, and discs 2 to 4 shared (the North American CD's
/// files): the Traditional Chinese one installs the 1.01 patch's
/// `atlantis2.exe` (MD5 `255310e7…`) with Big5 texts, the English one the
/// Polish CD's (MD5 `f2b6f7bf…`) with English texts.
pub const LANGUAGE_EDITIONS: [(&str, Language, &str); 3] = [
    (
        "atlantis2-cd-us-2199552",
        Language::Czech,
        "Atlantis II, Czech CD (Bohemia Interactive, 2000)",
    ),
    (
        "atlantis2-patch101-2183168",
        Language::TraditionalChinese,
        "Atlantis II, Traditional Chinese CD (Interwise, 2000)",
    ),
    (
        "atlantis2-cd-pl-2183168",
        Language::English,
        "Atlantis II, English CD (Interwise, 2000)",
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
/// is French only); an edition that shares another's build by its own
/// name ([`LANGUAGE_EDITIONS`]: "Atlantis II, Czech CD (Bohemia Interactive, 2000)").
pub fn edition_label(build: &Build, language: Option<Language>) -> String {
    if let Some(&(_, _, name)) = LANGUAGE_EDITIONS
        .iter()
        .find(|&&(id, l, _)| id == build.id && Some(l) == language)
    {
        return name.to_string();
    }
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
                "db4a621ee8bee5c2c155cf43bfdc442a",
                "0609b0b59904d1ea7e001fbc1986036b",
                "041255980d18b1ad866911d575301f26",
                "ec3af0762a15b57a95deadcfceb0bc87",
                "6e3fdc9a71de77e677f3587634383d6a",
                "7c6242d795dbab5d3d67de81c492096a",
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
        let [cd, patch, gog, w10, _, us, collection, pl, de, es, sv] = &KNOWN_BUILDS;
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
        // The Czech CD: the North American CD's build, its own name.
        assert_eq!(
            edition_label(us, Some(Language::Czech)),
            "Atlantis II, Czech CD (Bohemia Interactive, 2000)"
        );
        assert_eq!(
            edition_label(us, None),
            "Atlantis II, CD (DreamCatcher, 2000)"
        );
        // Czech with another build: the build's name and the language.
        assert_eq!(
            edition_label(patch, Some(Language::Czech)),
            "Atlantis II, patch 1.01, Czech (1999)"
        );
        assert_eq!(
            edition_label(collection, Some(Language::Italian)),
            "Atlantis II, Italian Collection (patch 1.01)"
        );
        assert_eq!(
            edition_label(pl, Some(Language::Polish)),
            "Atlantis II, Polish CD (1999)"
        );
        assert_eq!(
            edition_label(de, Some(Language::German)),
            "Atlantis II, German CD (1999)"
        );
        assert_eq!(
            edition_label(es, Some(Language::Spanish)),
            "Atlantis II, Spanish CD (1999)"
        );
        assert_eq!(
            edition_label(sv, Some(Language::Swedish)),
            "Atlantis II, Swedish CD (1999)"
        );
        // The Interwise release's two disc 1s: the 1.01 patch's build in
        // Traditional Chinese, the Polish CD's in English.
        assert_eq!(
            edition_label(patch, Some(Language::TraditionalChinese)),
            "Atlantis II, Traditional Chinese CD (Interwise, 2000)"
        );
        assert_eq!(
            edition_label(pl, Some(Language::English)),
            "Atlantis II, English CD (Interwise, 2000)"
        );
        assert_eq!(
            edition_label(patch, Some(Language::English)),
            "Atlantis II, patch 1.01, English (1999)"
        );
        assert_eq!(edition_label(pl, None), "Atlantis II, Polish CD (1999)");
    }

    #[test]
    fn code_pages() {
        for l in [Language::Polish, Language::Czech] {
            assert_eq!(text_code_page(Some(l)), TextCodePage::Windows1250);
        }
        for l in [
            Language::English,
            Language::French,
            Language::German,
            Language::Spanish,
            Language::Italian,
            Language::Swedish,
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
        // The Swedish letters at their Windows-1252 bytes.
        let sv = TextCodePage::Windows1252.decode(&[0xe5, 0xe4, 0xf6, 0xc5, 0xc4, 0xd6]);
        assert_eq!(sv, "åäöÅÄÖ");
        // The Czech letters at their Windows-1250 bytes.
        let cs = TextCodePage::Windows1250.decode(&[
            0xe1, 0xe8, 0xef, 0xe9, 0xec, 0xed, 0xf2, 0xf3, 0xf8, 0x9a, 0x9d, 0xfa, 0xf9, 0xfd,
            0x9e, 0xc8, 0xd8, 0x8a, 0x8e,
        ]);
        assert_eq!(cs, "áčďéěíňóřšťúůýžČŘŠŽ");
        // Traditional Chinese: code page 950, the line breaks of its
        // `Font.txt` (A1 41, A1 43, A1 48, A1 49).
        let zh = text_code_page(Some(Language::TraditionalChinese));
        assert_eq!(zh, TextCodePage::Windows950);
        assert_eq!(
            zh.decode(&[0xa1, 0x41, 0xa1, 0x43, 0xa1, 0x48, 0xa1, 0x49]),
            "，。？！"
        );
    }
}
