//! The code page of the game's texts (CON scripts, EPI.TXT, CREDITS.TXT).
//!
//! The original never converts these bytes: its GDI subtitles draw them in
//! the ANSI code page, its bitmap and sprite fonts take the byte as the
//! glyph index. A port that shows them through Unicode (an outline font,
//! logs, tests) must decode them in the code page of the release's
//! archives. The Chinese releases' texts are double-byte (Big5, GBK):
//! their characters are decoded by `encoding_rs` (MIT / Apache-2.0).

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
    /// Code page 950, Big5: the Traditional Chinese release's CON texts
    /// (atlantis-cd-zh-tw-696832 passes them to TextOutA, read in the
    /// system's ANSI code page: FUN_0042f798; lead bytes 0xa1..0xf4, every
    /// pair valid Big5). Decoded as the WHATWG Big5 of `encoding_rs`, which
    /// is code page 950 at every pair of the shipped texts.
    Windows950,
    /// Code page 936, GBK: the Simplified Chinese release's CON texts
    /// (atlantis-cd-zh-cn-692736, the same TextOutA path), GB2312 with a
    /// few GBK pairs (`85 85`, `97 97`, `A8 44`). Decoded as the WHATWG GBK
    /// of `encoding_rs` (0x80 alone is `€`, as in code page 936).
    Windows936,
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
    /// The `encoding_rs` decoder of a double-byte code page.
    fn encoding(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            TextCodePage::Windows950 => Some(encoding_rs::BIG5),
            TextCodePage::Windows936 => Some(encoding_rs::GBK),
            TextCodePage::Windows1252 | TextCodePage::Dos850 => None,
        }
    }

    /// Whether the code page has two-byte characters (a lead byte, then a
    /// trail byte).
    pub fn is_double_byte(self) -> bool {
        self.encoding().is_some()
    }

    /// Whether `b` starts a two-byte character: 0x81..0xfe in code pages
    /// 950 and 936; never in the single-byte code pages.
    pub fn is_lead_byte(self, b: u8) -> bool {
        self.is_double_byte() && (0x81..=0xfe).contains(&b)
    }

    /// The character of one byte (`'\0'` where the code page has none; a
    /// lead byte of a double-byte code page alone has none).
    pub fn char_of(self, b: u8) -> char {
        let u = match (self, b) {
            (_, 0..=0x7f) => u32::from(b),
            (TextCodePage::Windows1252, 0x80..=0x9f) => u32::from(CP1252_80[usize::from(b - 0x80)]),
            (TextCodePage::Windows1252, _) => u32::from(b),
            (TextCodePage::Dos850, _) => u32::from(CP850_80[usize::from(b - 0x80)]),
            (TextCodePage::Windows950 | TextCodePage::Windows936, _) => {
                return self.decode(&[b]).chars().next().unwrap_or('\0');
            }
        };
        char::from_u32(u).unwrap_or('\0')
    }

    /// The text of `bytes`; in a double-byte code page a byte pair is one
    /// character, and a byte or pair the code page has no character for is
    /// `'\0'`.
    pub fn decode(self, bytes: &[u8]) -> String {
        match self.encoding() {
            Some(enc) => enc
                .decode_without_bom_handling(bytes)
                .0
                .chars()
                .map(|c| {
                    if c == char::REPLACEMENT_CHARACTER {
                        '\0'
                    } else {
                        c
                    }
                })
                .collect(),
            None => bytes.iter().map(|&b| self.char_of(b)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextCodePage::{Dos850, Windows1252, Windows936, Windows950};

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

    /// The subtitle breakers' cut marks, as the executables compare them
    /// (atlantis-cd-zh-tw-696832 FUN_0042f424 @ 0x0042f510..0x0042f566,
    /// atlantis-cd-zh-cn-692736 FUN_0042f424 @ 0x0042f510..0x0042f568).
    #[test]
    fn chinese_punctuation() {
        assert_eq!(
            Windows950
                .decode(&[0xa1, 0x41, 0xa1, 0x43, 0xa1, 0x46, 0xa1, 0x48, 0xa1, 0x49, 0xa1, 0x4b]),
            "，。；？！…"
        );
        assert_eq!(
            Windows936.decode(&[0xa3, 0xac, 0xa1, 0xa3, 0xa3, 0xbf, 0xa3, 0xa1, 0xa1, 0xa1]),
            "，。？！\u{3000}"
        );
    }

    #[test]
    fn chinese_characters() {
        // 中文 in each code page, between ASCII.
        assert_eq!(
            Windows950.decode(b"[Seth] \xa4\xa4\xa4\xe5!"),
            "[Seth] 中文!"
        );
        assert_eq!(
            Windows936.decode(b"[Seth] \xd6\xd0\xce\xc4!"),
            "[Seth] 中文!"
        );
        // The GBK pairs outside GB2312 of the Simplified texts.
        assert_eq!(
            Windows936.decode(&[0x85, 0x85, 0x97, 0x97, 0xa8, 0x44]),
            "厖棗―"
        );
        // A lead byte without its trail byte, and a byte with no character.
        assert_eq!(Windows950.decode(&[b'a', 0xa4]), "a\0");
        assert_eq!(Windows950.decode(&[0xff, b'b']), "\0b");
        assert_eq!(Windows950.char_of(0xa4), '\0');
        assert_eq!(Windows950.char_of(b'S'), 'S');
        assert_eq!(Windows936.char_of(0x80), '€');
        assert!(Windows950.is_lead_byte(0xa4) && Windows936.is_lead_byte(0x85));
        assert!(!Windows950.is_lead_byte(b'a') && !Windows1252.is_lead_byte(0xa4));
        assert!(Windows936.is_double_byte() && !Dos850.is_double_byte());
        // The single-byte code pages are unchanged.
        assert_eq!(Windows1252.decode(&[0xa4, 0xa4]), "¤¤");
    }
}
