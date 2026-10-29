//! The code page of the game's texts (CON scripts, EPI.TXT, CREDITS.TXT).
//!
//! The original never converts these bytes: its GDI subtitles draw them in
//! the ANSI code page, its bitmap and sprite fonts take the byte as the
//! glyph index. A port that shows them through Unicode (an outline font,
//! logs, tests) must decode them in the code page of the release's
//! archives. The Chinese and Japanese releases' texts are double-byte
//! (Big5, GBK, Shift-JIS): their characters are decoded by `encoding_rs`
//! (MIT / Apache-2.0).

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
    /// Windows-1250 (Central European): the Czech CD's texts (Bohemia
    /// Interactive, 2000: the US August 1997 archives with Czech CON,
    /// SPRLIST and font files; `0xe8` č, `0xec` ě, `0xf8` ř, `0x9a` š,
    /// `0x9e` ž, `0xf9` ů, `0x9d` ť, `0xef` ď, `0xf2` ň). Its program is
    /// Cryo's 1.01 patch, which draws the subtitles with TextOutA in Arial
    /// asked in DEFAULT_CHARSET (FUN_0041ae20): on the Czech Windows the
    /// release was sold for, read in code page 1250.
    Windows1250,
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
    /// Code page 932, Shift-JIS: the Japanese DVD's `CHAPJ` texts
    /// (atlantis-dvd-ja-696832 passes them to TextOutA in the font
    /// "ＭＳ Ｐゴシック", read in the system's ANSI code page: FUN_0042f86c;
    /// lead bytes 0x81..0x9f and 0xe0..0xfc, the bytes 0xa1..0xdf
    /// half-width katakana). Decoded as the WHATWG Shift_JIS of
    /// `encoding_rs`, which is code page 932 at every pair of the shipped
    /// texts.
    Windows932,
}

/// Windows-1252 for 0x80..0x9f (0 where the code page has no character);
/// the other bytes are Latin-1.
const CP1252_80: [u16; 32] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
];

/// Windows-1250 for 0x80..0xff (0 where the code page has no character).
const CP1250_80: [u16; 128] = [
    0x20ac, 0, 0x201a, 0, 0x201e, 0x2026, 0x2020, 0x2021, 0, 0x2030, 0x0160, 0x2039, 0x015a,
    0x0164, 0x017d, 0x0179, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0, 0x2122,
    0x0161, 0x203a, 0x015b, 0x0165, 0x017e, 0x017a, 0x00a0, 0x02c7, 0x02d8, 0x0141, 0x00a4, 0x0104,
    0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x015e, 0x00ab, 0x00ac, 0x00ad, 0x00ae, 0x017b, 0x00b0, 0x00b1,
    0x02db, 0x0142, 0x00b4, 0x00b5, 0x00b6, 0x00b7, 0x00b8, 0x0105, 0x015f, 0x00bb, 0x013d, 0x02dd,
    0x013e, 0x017c, 0x0154, 0x00c1, 0x00c2, 0x0102, 0x00c4, 0x0139, 0x0106, 0x00c7, 0x010c, 0x00c9,
    0x0118, 0x00cb, 0x011a, 0x00cd, 0x00ce, 0x010e, 0x0110, 0x0143, 0x0147, 0x00d3, 0x00d4, 0x0150,
    0x00d6, 0x00d7, 0x0158, 0x016e, 0x00da, 0x0170, 0x00dc, 0x00dd, 0x0162, 0x00df, 0x0155, 0x00e1,
    0x00e2, 0x0103, 0x00e4, 0x013a, 0x0107, 0x00e7, 0x010d, 0x00e9, 0x0119, 0x00eb, 0x011b, 0x00ed,
    0x00ee, 0x010f, 0x0111, 0x0144, 0x0148, 0x00f3, 0x00f4, 0x0151, 0x00f6, 0x00f7, 0x0159, 0x016f,
    0x00fa, 0x0171, 0x00fc, 0x00fd, 0x0163, 0x02d9,
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
            TextCodePage::Windows932 => Some(encoding_rs::SHIFT_JIS),
            TextCodePage::Windows1252 | TextCodePage::Windows1250 | TextCodePage::Dos850 => None,
        }
    }

    /// Whether the code page has two-byte characters (a lead byte, then a
    /// trail byte).
    pub fn is_double_byte(self) -> bool {
        self.encoding().is_some()
    }

    /// Whether `b` starts a two-byte character: 0x81..0xfe in code pages
    /// 950 and 936, 0x81..0x9f and 0xe0..0xfc in 932; never in the
    /// single-byte code pages.
    pub fn is_lead_byte(self, b: u8) -> bool {
        match self {
            TextCodePage::Windows932 => matches!(b, 0x81..=0x9f | 0xe0..=0xfc),
            _ => self.is_double_byte() && (0x81..=0xfe).contains(&b),
        }
    }

    /// The character of one byte (`'\0'` where the code page has none; a
    /// lead byte of a double-byte code page alone has none).
    pub fn char_of(self, b: u8) -> char {
        let u = match (self, b) {
            (_, 0..=0x7f) => u32::from(b),
            (TextCodePage::Windows1252, 0x80..=0x9f) => u32::from(CP1252_80[usize::from(b - 0x80)]),
            (TextCodePage::Windows1252, _) => u32::from(b),
            (TextCodePage::Windows1250, _) => u32::from(CP1250_80[usize::from(b - 0x80)]),
            (TextCodePage::Dos850, _) => u32::from(CP850_80[usize::from(b - 0x80)]),
            (TextCodePage::Windows950 | TextCodePage::Windows936 | TextCodePage::Windows932, _) => {
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
    use super::TextCodePage::{
        Dos850, Windows1250, Windows1252, Windows932, Windows936, Windows950,
    };

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

    /// The Czech CD's letters (CHAPI001: "Jeho Výsost", "chlapče",
    /// "stráží", "dovnitř", "Saa'atův", "nejvyšší"), its capitals, and
    /// every byte as the code page reads it.
    #[test]
    fn czech_letters() {
        assert_eq!(
            Windows1250.decode(&[
                0xe1, 0xe8, 0xef, 0xe9, 0xec, 0xed, 0xf2, 0xf3, 0xf8, 0x9a, 0x9d, 0xfa, 0xf9, 0xfd,
                0x9e
            ]),
            "áčďéěíňóřšťúůýž"
        );
        assert_eq!(
            Windows1250.decode(&[
                0xc1, 0xc8, 0xcf, 0xc9, 0xcc, 0xcd, 0xd2, 0xd3, 0xd8, 0x8a, 0x8d, 0xda, 0xd9, 0xdd,
                0x8e
            ]),
            "ÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ"
        );
        assert_eq!(Windows1250.decode(b"Seth"), "Seth");
        // The bytes Windows-1250 leaves undefined.
        for b in [0x81, 0x83, 0x88, 0x90, 0x98] {
            assert_eq!(Windows1250.char_of(b), '\0');
        }
        // Every other byte is the code page's (encoding_rs's windows-1250
        // maps the undefined ones to C1 controls).
        for b in 0x80..=0xffu8 {
            let one = [b];
            let (want, _, _) = encoding_rs::WINDOWS_1250.decode(&one);
            let c = Windows1250.char_of(b);
            if c != '\0' {
                assert_eq!(want.chars().next(), Some(c), "{b:#x}");
            }
        }
        // The same bytes read as Windows-1252, as a Western Windows would.
        assert_eq!(Windows1252.decode(&[0xe8, 0xec, 0xf8, 0xf9]), "èìøù");
        assert!(!Windows1250.is_double_byte());
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

    /// The Japanese DVD's texts: kana and kanji, the ideographic full
    /// stop, half-width katakana (one byte), its lead bytes.
    #[test]
    fn japanese_characters() {
        // ようこそセス。 (CHAPJ003's first words)
        assert_eq!(
            Windows932.decode(b"\x82\xe6\x82\xa4\x82\xb1\x82\xbb\x83\x5a\x83\x58\x81\x42"),
            "ようこそセス。"
        );
        assert_eq!(
            Windows932.decode(b"[Seth] \x93\xfa\x96\x7b!"),
            "[Seth] 日本!"
        );
        // The face name of FUN_0041aec0 (0x004ac2d8).
        assert_eq!(
            Windows932.decode(b"\x82\x6c\x82\x72\x20\x82\x6f\x83\x53\x83\x56\x83\x62\x83\x4e"),
            "ＭＳ Ｐゴシック"
        );
        assert_eq!(Windows932.char_of(0xb1), 'ｱ');
        assert_eq!(Windows932.char_of(0x82), '\0');
        assert!(Windows932.is_lead_byte(0x82) && Windows932.is_lead_byte(0xe0));
        assert!(!Windows932.is_lead_byte(0xb1) && !Windows932.is_lead_byte(0xa0));
        assert!(!Windows932.is_lead_byte(0xfd) && Windows932.is_double_byte());
        // A trail byte is never an ASCII space (the Japanese breaker cuts
        // at each 0x20 without tracking the lead bytes).
        assert_eq!(Windows932.decode(&[0x81, 0x40]), "\u{3000}");
    }
}
