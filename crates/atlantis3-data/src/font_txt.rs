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
            });
        }
        FontTxt { fonts }
    }

    /// The font `id`: the last line with that id.
    pub fn font(&self, id: &str) -> Option<&FontDef> {
        self.fonts.iter().rev().find(|f| f.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
