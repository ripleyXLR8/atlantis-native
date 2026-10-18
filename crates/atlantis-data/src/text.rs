//! The code page of the game's texts (CON scripts, EPI.TXT, CREDITS.TXT).
//!
//! The original never converts these bytes: its GDI subtitles draw them in
//! the ANSI code page, its bitmap and sprite fonts take the byte as the
//! glyph index. A port that shows them through Unicode (an outline font,
//! logs, tests) must decode them in the code page of the release's
//! archives.

/// How a release's text bytes read as letters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextCodePage {
    /// Windows-1252: the 2001, GOG.com and English 1997 releases, and the
    /// French August 1997 one, whose archives re-encode the accented
    /// letters for it (`0xe9` é).
    #[default]
    Windows1252,
    /// The DOS code page 850 (the same letters as 437 at the bytes the
    /// texts use: `0x82` é, `0x85` à, `0x8a` è, `0x87` ç...): the French
    /// May 1997 release, whose subtitles meet the code page 437 glyphs of
    /// `DIALOG\FONTS.BIN` byte for byte (atlantis-cd-fr-688640 FUN_0042ec80
    /// with FUN_00410800: glyph = byte).
    Dos850,
}

/// Windows-1252 for 0x80..0x9f (0 where the code page has no character);
/// the other bytes are Latin-1.
const CP1252_80: [u16; 32] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
];

/// Code page 850 for 0x80..0xff.
const CP850_80: [u16; 128] = [
    0x00c7, 0x00fc, 0x00e9, 0x00e2, 0x00e4, 0x00e0, 0x00e5, 0x00e7, 0x00ea, 0x00eb, 0x00e8, 0x00ef,
    0x00ee, 0x00ec, 0x00c4, 0x00c5, 0x00c9, 0x00e6, 0x00c6, 0x00f4, 0x00f6, 0x00f2, 0x00fb, 0x00f9,
    0x00ff, 0x00d6, 0x00dc, 0x00f8, 0x00a3, 0x00d8, 0x00d7, 0x0192, 0x00e1, 0x00ed, 0x00f3, 0x00fa,
    0x00f1, 0x00d1, 0x00aa, 0x00ba, 0x00bf, 0x00ae, 0x00ac, 0x00bd, 0x00bc, 0x00a1, 0x00ab, 0x00bb,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x00c1, 0x00c2, 0x00c0, 0x00a9, 0x2563, 0x2551, 0x2557,
    0x255d, 0x00a2, 0x00a5, 0x2510, 0x2514, 0x2534, 0x252c, 0x251c, 0x2500, 0x253c, 0x00e3, 0x00c3,
    0x255a, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256c, 0x00a4, 0x00f0, 0x00d0, 0x00ca, 0x00cb,
    0x00c8, 0x0131, 0x00cd, 0x00ce, 0x00cf, 0x2518, 0x250c, 0x2588, 0x2584, 0x00a6, 0x00cc, 0x2580,
    0x00d3, 0x00df, 0x00d4, 0x00d2, 0x00f5, 0x00d5, 0x00b5, 0x00fe, 0x00de, 0x00da, 0x00db, 0x00d9,
    0x00fd, 0x00dd, 0x00af, 0x00b4, 0x00ad, 0x00b1, 0x2017, 0x00be, 0x00b6, 0x00a7, 0x00f7, 0x00b8,
    0x00b0, 0x00a8, 0x00b7, 0x00b9, 0x00b3, 0x00b2, 0x25a0, 0x00a0,
];

impl TextCodePage {
    /// The character of one byte (`'\0'` where the code page has none).
    pub fn char_of(self, b: u8) -> char {
        let u = match (self, b) {
            (_, 0..=0x7f) => u32::from(b),
            (TextCodePage::Windows1252, 0x80..=0x9f) => u32::from(CP1252_80[usize::from(b - 0x80)]),
            (TextCodePage::Windows1252, _) => u32::from(b),
            (TextCodePage::Dos850, _) => u32::from(CP850_80[usize::from(b - 0x80)]),
        };
        char::from_u32(u).unwrap_or('\0')
    }

    /// The text of `bytes`.
    pub fn decode(self, bytes: &[u8]) -> String {
        bytes.iter().map(|&b| self.char_of(b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::TextCodePage::{Dos850, Windows1252};

    #[test]
    fn french_letters() {
        // The DOS bytes of the May 1997 French texts.
        assert_eq!(
            Dos850.decode(&[0x82, 0x83, 0x85, 0x87, 0x88, 0x8a, 0x8b, 0x8c, 0x93, 0x96, 0x97]),
            "éâàçêèïîôûù"
        );
        // The same bytes as Windows-1252 are punctuation.
        assert_eq!(Windows1252.decode(&[0x82, 0x85, 0x8a]), "‚…Š");
        // The August re-encoding.
        assert_eq!(Windows1252.decode(&[0xe9, 0xe0, 0xe8, 0xe7]), "éàèç");
        assert_eq!(Dos850.decode(b"Seth"), "Seth");
        assert_eq!(Windows1252.char_of(0x81), '\0');
    }
}
