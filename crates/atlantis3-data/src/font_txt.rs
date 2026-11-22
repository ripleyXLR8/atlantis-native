//! `font\font.txt`: the GDI fonts of the texts, read by `AtlFont_MainInit`
//! (@ 0x0041d960) with the shared reader of [`crate::tokens`], one font a
//! line: `<id> <face> <height> <width> <weight> <italic> <charset>`.
//!
//! Unlike the other text files, every line is a font: there is no comment
//! (a `//` line would be read as a font called `//`). Before reading it the
//! game registers a TrueType file `Atlantis.ttf` (path formatted
//! @ 0x0041da8e from @ 0x00716140 and @ 0x0071614c; its folder, `font\`,
//! is `[INFERRED]` from the shipped file).
//! The fonts are kept in a map by id (@ 0x0041e174 → @ 0x0041e5b0), the
//! later line of an id replacing the earlier one `[INFERRED]` (the map
//! slot is assigned the new font). The code asks for `AtlFont_Default`,
//! `AtlFont_Bulle`, `AtlFont_Special`, `AtlFont_Titre` and
//! `AtlFont_Credit` (@ 0x007151d0, @ 0x007159cc, @ 0x007151e0,
//! @ 0x0071b624, @ 0x0071b634).

use crate::tokens::{atoi, latin1, lower, Reader};

/// `font\font.txt` (@ 0x0041db27).
pub const FONT_TXT_PATH: &str = "Datas/font/font.txt";

/// The TrueType file registered before the fonts are made (@ 0x0041daaa;
/// the folder `[INFERRED]`).
pub const TTF_PATH: &str = "Datas/font/Atlantis.ttf";

/// A line of `font.txt` (@ 0x0041dc25 .. 0x0041dd77).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontDef {
    /// The name the code asks for.
    pub id: String,
    /// The face name.
    pub face: String,
    /// `atoi`; scaled by [`FontDef::scaled_height`].
    pub height: i32,
    /// `atoi`, given to GDI as is (0: the face's own).
    pub width: i32,
    /// The weight's name, see [`FontDef::weight`].
    pub weight_name: String,
    /// The sixth word, lower-cased, equal to `true` (@ 0x00714884).
    pub italic: bool,
    /// The character set's name, see [`FontDef::charset`].
    pub charset_name: String,
    /// *Atlantis V*'s `Antialias` key: the word lower-cased equal to
    /// `true` (atlantis5-dvd-int-3891200 @ 0x004610e8..0x004611f8); on
    /// when the block has no such key (0x00460a88). The font is made with
    /// `lfQuality` 4 (`ANTIALIASED_QUALITY`) when on, 3
    /// (`NONANTIALIASED_QUALITY`) when off (@ 0x0045fa40).
    pub antialias: bool,
}

impl FontDef {
    /// The GDI weight of [`FontDef::weight_name`] (@ 0x0041cbb0): the
    /// `FW_*` names, case kept; 400 for any other word.
    pub fn weight(&self) -> i32 {
        match self.weight_name.as_str() {
            "FW_DONTCARE" => 0,
            "FW_THIN" => 100,
            "FW_EXTRALIGHT" | "FW_ULTRALIGHT" => 200,
            "FW_LIGHT" => 300,
            "FW_MEDIUM" => 500,
            "FW_SEMIBOLD" | "FW_DEMIBOLD" => 600,
            "FW_BOLD" => 700,
            "FW_EXTRABOLD" | "FW_ULTRABOLD" => 800,
            "FW_HEAVY" | "FW_BLACK" => 900,
            _ => 400,
        }
    }

    /// The GDI character set of [`FontDef::charset_name`] (@ 0x0041cbb0),
    /// `subtitle_language` being the `SousTitre` value. `ANSI_CHARSET` and
    /// unknown names do not give ANSI: they choose by the subtitle
    /// language, lower-cased (@ 0x0041cc14, which lower-cases the
    /// configuration's value itself): `gre` Greek, `chi` Big5, `isr`
    /// Hebrew, `rus` Russian, else ANSI (0).
    pub fn charset(&self, subtitle_language: &str) -> u8 {
        match self.charset_name.as_str() {
            "DEFAULT_CHARSET" => 1,
            "SYMBOL_CHARSET" => 2,
            "SHIFTJIS_CHARSET" => 0x80,
            "HANGEUL_CHARSET" => 0x81,
            "GB2312_CHARSET" => 0x86,
            "CHINESEBIG5_CHARSET" => 0x88,
            "OEM_CHARSET" => 0xff,
            "JOHAB_CHARSET" => 0x82,
            "HEBREW_CHARSET" => 0xb1,
            "ARABIC_CHARSET" => 0xb2,
            "GREEK_CHARSET" => 0xa1,
            "TURKISH_CHARSET" => 0xa2,
            "VIETNAMESE_CHARSET" => 0xa3,
            "THAI_CHARSET" => 0xde,
            "EASTEUROPE_CHARSET" => 0xee,
            "RUSSIAN_CHARSET" => 0xcc,
            "MAC_CHARSET" => 0x4d,
            "BALTIC_CHARSET" => 0xba,
            _ => match lower(subtitle_language.as_bytes()).as_slice() {
                b"gre" => 0xa1,
                b"chi" => 0x88,
                b"isr" => 0xb1,
                b"rus" => 0xcc,
                _ => 0,
            },
        }
    }

    /// The height given to GDI for a screen `screen_width` pixels wide
    /// (@ 0x0041e090): the height times 0.9 (640), 1.0 (800) or 1.1
    /// (1024), as floats, truncated toward zero. Other widths leave the
    /// original's factor uninitialised: `None`.
    pub fn scaled_height(&self, screen_width: u32) -> Option<i32> {
        let factor: f32 = match screen_width {
            640 => 0.9,
            800 => 1.0,
            1024 => 1.1,
            _ => return None,
        };
        Some((f64::from(self.height) * f64::from(factor)).trunc() as i32)
    }
}

/// `font.txt` as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontTxt {
    /// The lines in file order.
    pub fonts: Vec<FontDef>,
}

impl FontTxt {
    /// Reads the file: one font per line, missing words empty (so 0 for
    /// the numbers).
    pub fn parse(data: &[u8]) -> FontTxt {
        let mut file = Reader::new(data);
        let mut fonts = Vec::new();
        while !file.at_end() {
            let mut w = Reader::new(file.line());
            let id = latin1(w.word());
            let face = latin1(w.word());
            let height = atoi(w.word());
            let width = atoi(w.word());
            let weight_name = latin1(w.word());
            let italic = lower(w.word()) == b"true";
            let charset_name = latin1(w.word());
            fonts.push(FontDef {
                id,
                face,
                height,
                width,
                weight_name,
                italic,
                charset_name,
                antialias: false,
            });
        }
        FontTxt { fonts }
    }

    /// Reads *Atlantis Evolution*'s 2004 `loc\text\<SousTitre>\font.txt`
    /// (atlantis4-cd-en-3022848 FUN_0041f270 @ 0x0041f3f0..0x0042038a):
    /// lines read by the shared reader, a line starting with `'` skipped
    /// (@ 0x0041f484, @ 0x0041fd88); `[Font]` (0x006adc0c) opens a font
    /// whose lines, up to `[EndFont]` (0x006adc00), are `<key> <value>`:
    /// `NameID` (the id), `Name` (the TrueType file), `Type` (0x006adbc0,
    /// the face), `Height` and `Width` (`atoi`), `Weight` (default
    /// `FW_NORMAL`, 0x006ada3c), `Italic` (the word lower-cased equal to
    /// `true`, 0x006ac8f0), `Charset` (default `DEFAULT_CHARSET`,
    /// 0x006adb9c); then the font is made (FUN_0041e480 @ 0x0042038a).
    /// `[PartitionCode]`, `[PartitionMode]` and `[PartitionOptions]` (the
    /// line breaking) are not kept here.
    pub fn parse_2004(data: &[u8]) -> FontTxt {
        let mut file = Reader::new(data);
        let mut fonts = Vec::new();
        while !file.at_end() {
            let line = file.line();
            if line.first() == Some(&b'\'') || Reader::new(line).word() != b"[Font]" {
                continue;
            }
            // The block's defaults (@ 0x004608dc..0x00460a88): `FW_NORMAL`,
            // `DEFAULT_CHARSET`, and `Antialias` on (the byte +0x48 set to
            // 1 at 0x00460a88).
            let mut f = FontDef {
                weight_name: "FW_NORMAL".into(),
                charset_name: "DEFAULT_CHARSET".into(),
                antialias: true,
                ..FontDef::default()
            };
            while !file.at_end() {
                let line = file.line();
                if line.first() == Some(&b'\'') {
                    continue;
                }
                let mut w = Reader::new(line);
                match w.word() {
                    b"[EndFont]" => break,
                    b"NameID" => f.id = latin1(w.word()),
                    b"Type" => f.face = latin1(w.word()),
                    b"Height" => f.height = atoi(w.word()),
                    b"Width" => f.width = atoi(w.word()),
                    b"Weight" => f.weight_name = latin1(w.word()),
                    b"Italic" => f.italic = lower(w.word()) == b"true",
                    b"Charset" => f.charset_name = latin1(w.word()),
                    _ => {}
                }
            }
            fonts.push(f);
        }
        FontTxt { fonts }
    }

    /// Reads *Atlantis V*'s `datas/fonts/<language>/font.txt`
    /// (atlantis5-dvd-int-3891200 @ 0x004601f0, `CTextManager`): the 2004
    /// reader's blocks and keys ([`FontTxt::parse_2004`]: `'` lines skipped
    /// @ 0x004603fa and 0x00460b20, `[Font]` 0x0066f36c .. `[EndFont]`
    /// 0x0066f350, the same defaults `FW_NORMAL` 0x0066f274 and
    /// `DEFAULT_CHARSET` 0x0066f35c), with three differences:
    ///
    /// - a word starting with `"` runs to the next `"` (the word reader
    ///   @ 0x00455ea0: `Type "Courier New"`); its separators are the 2004
    ///   ones (@ 0x00456300);
    /// - the key `Antialias` (0x0066f2fc) sets [`FontDef::antialias`];
    /// - a font whose id is already made is not made again (log "La fonte
    ///   %s existe deja. Creation annulee", 0x0066f2b0): the first wins.
    pub fn parse_atlantis5(data: &[u8]) -> FontTxt {
        let mut file = Reader::new(data);
        let mut fonts: Vec<FontDef> = Vec::new();
        while !file.at_end() {
            let line = file.line();
            if line.first() == Some(&b'\'') || quoted_word(line).0 != b"[Font]" {
                continue;
            }
            // The block's defaults (@ 0x004608dc..0x00460a88): `FW_NORMAL`,
            // `DEFAULT_CHARSET`, and `Antialias` on (the byte +0x48 set to
            // 1 at 0x00460a88).
            let mut f = FontDef {
                weight_name: "FW_NORMAL".into(),
                charset_name: "DEFAULT_CHARSET".into(),
                antialias: true,
                ..FontDef::default()
            };
            while !file.at_end() {
                let line = file.line();
                if line.first() == Some(&b'\'') {
                    continue;
                }
                let (key, rest) = quoted_word(line);
                let value = quoted_word(rest).0;
                match key {
                    b"[EndFont]" => break,
                    b"NameID" => f.id = latin1(value),
                    b"Type" => f.face = latin1(value),
                    b"Height" => f.height = atoi(value),
                    b"Width" => f.width = atoi(value),
                    b"Weight" => f.weight_name = latin1(value),
                    b"Italic" => f.italic = lower(value) == b"true",
                    b"Charset" => f.charset_name = latin1(value),
                    b"Antialias" => f.antialias = lower(value) == b"true",
                    _ => {}
                }
            }
            if fonts.iter().any(|g| g.id == f.id) {
                log::debug!("font.txt: {} made already", f.id);
                continue;
            }
            fonts.push(f);
        }
        FontTxt { fonts }
    }

    /// The font `id`: the last line with that id.
    pub fn font(&self, id: &str) -> Option<&FontDef> {
        self.fonts.iter().rev().find(|f| f.id == id)
    }
}

/// The next word of `s` and what follows it (@ 0x00455ea0): separators
/// skipped, then a `"`-quoted run (without its quotes) or a run to the
/// next separator.
fn quoted_word(s: &[u8]) -> (&[u8], &[u8]) {
    let mut p = 0;
    while p < s.len() && crate::tokens::is_separator(s[p]) {
        p += 1;
    }
    if s.get(p) == Some(&b'"') {
        let start = p + 1;
        let end = s[start..]
            .iter()
            .position(|&c| c == b'"')
            .map_or(s.len(), |i| start + i);
        return (&s[start..end], s.get(end + 1..).unwrap_or(&[]));
    }
    let start = p;
    while p < s.len() && !crate::tokens::is_separator(s[p]) {
        p += 1;
    }
    (&s[start..p], &s[p..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_2004() {
        let t = FontTxt::parse_2004(
            b"' c\r\n[PartitionCode]\t0x20\r\n[Font]\r\nNameID\tAtlFont_Default \r\n\
Name\tAtlantis.ttf\r\nType\tArial\r\nHeight\t21\r\nWidth\t0\r\n' x\r\nItalic\tTRUE\r\n\
[EndFont]\r\n[Font]\r\nNameID F_B\r\nWeight FW_BOLD\r\nCharset ANSI_CHARSET\r\n[EndFont]\r\n",
        );
        assert_eq!(t.fonts.len(), 2);
        let f = t.font("AtlFont_Default").unwrap();
        assert_eq!((f.face.as_str(), f.height, f.width), ("Arial", 21, 0));
        assert!(f.italic);
        assert_eq!(f.weight_name, "FW_NORMAL");
        assert_eq!(f.charset_name, "DEFAULT_CHARSET");
        let b = t.font("F_B").unwrap();
        assert_eq!((b.weight(), b.charset_name.as_str()), (700, "ANSI_CHARSET"));
    }

    #[test]
    fn fonts_atlantis5() {
        let t = FontTxt::parse_atlantis5(
            b"[Font]\r\nNameID\tA \r\nType\t\"Courier New\"\r\nHeight\t16\r\nAntialias TRUE\r\n[EndFont]\r\n\
[Font]\r\nNameID A\r\nType arial\r\n[EndFont]\r\n[Font]\r\nNameID B\r\nAntialias false\r\n[EndFont]\r\n\
[Font]\r\nNameID C\r\n[EndFont]\r\n",
        );
        assert_eq!(t.fonts.len(), 3);
        assert!(!t.font("B").unwrap().antialias);
        assert!(t.font("C").unwrap().antialias);
        let a = &t.fonts[0];
        assert_eq!(
            (a.id.as_str(), a.face.as_str(), a.height),
            ("A", "Courier New", 16)
        );
        assert!(a.antialias);
    }

    #[test]
    fn fonts() {
        let t = FontTxt::parse(
            b"\r\nF_A Face 21 0\t\tFW_NORMAL false ANSI_CHARSET\r\nF_B Face 19 3 FW_BOLD TRUE GREEK_CHARSET\r\n\
F_C Other 60 0 heavy x\r\nF_A Again 10 0 FW_THIN false ANSI_CHARSET",
        );
        assert_eq!(t.fonts.len(), 4);
        let a = &t.fonts[0];
        assert_eq!(
            (a.id.as_str(), a.face.as_str(), a.height, a.width),
            ("F_A", "Face", 21, 0)
        );
        assert_eq!((a.weight(), a.italic), (400, false));
        assert_eq!((a.charset("fra"), a.charset("RUS")), (0, 0xcc));
        assert_eq!(a.scaled_height(640), Some(18));
        assert_eq!(a.scaled_height(800), Some(21));
        assert_eq!(a.scaled_height(1024), Some(23));
        assert_eq!(a.scaled_height(1280), None);
        let b = &t.fonts[1];
        assert_eq!((b.weight(), b.italic, b.charset("rus")), (700, true, 0xa1));
        let c = &t.fonts[2];
        assert_eq!((c.weight(), c.italic, c.charset("chi")), (400, false, 0x88));
        assert_eq!(c.charset_name, "");
        assert_eq!(t.font("F_A").unwrap().face, "Again");
        assert!(t.font("F_D").is_none());
    }
}
