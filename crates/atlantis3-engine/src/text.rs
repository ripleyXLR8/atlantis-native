//! Text on screen: the dialog subtitles, the cinematics' captions and cue
//! lines, the `PLAYVOICE` lines — every one drawn by the flip
//! (@ 0x0040fdf0) with GDI on the back buffer through the linked *Cryo
//! Text Library 2.57*: a layout (four margins and two alignments,
//! `AtlFont_SetLayout` @ 0x0041e190) and a string (`AtlFont_Print`
//! @ 0x0041e2c0) broken into lines inside the margins (@ 0x006b4a20, with
//! a French rule: no break before `?`, `!` or `:`), stacked from the
//! bottom, each line drawn with `ExtTextOutA` and an advance per byte from
//! `GetCharABCWidthsA`.
//!
//! The fonts are `font\font.txt`'s GDI `Arial` (`font\Atlantis.ttf` is
//! Arial 2.76, registered by the game) at heights `ftol(height × f)`, f
//! the float 0.9 / 1.0 / 1.1 for 640 / 800 / 1024 pixels wide. The widths
//! GDI gives for them (`tmHeight`, `tmAscent`, `tmAveCharWidth`, the ABC
//! widths of the 256 bytes) are the table measured with GDI on the game's
//! font (`data/arial_gdi.txt`); the glyphs are drawn here from the game's
//! font file, antialiased (`ANTIALIASED_QUALITY`; GDI's own grey levels
//! are not reproduced).
//!
//! The character set is `font.txt`'s ([`atlantis3_data::font_txt`]):
//! `ANSI_CHARSET` (French, Spanish) draws Windows-1252;
//! `EASTEUROPE_CHARSET` (German) gives GDI the font's Central European
//! face, so the bytes are Windows-1250 and the widths that table's
//! (`data/arial_gdi_ee.txt`; the German texts differ only in the
//! credits' `è`, drawn `č`). English asks for `SHIFTJIS_CHARSET`: a
//! Windows with a Japanese font then answers with that font (MS PGothic on
//! Windows 11) and the text library's double-byte renderer; the port
//! draws English as a Windows without one, in Arial and Windows-1252
//! (its texts are ASCII but for one `’`).
//!
//! The Traditional Chinese CD (Interwise) asks for `Arial` in
//! `CHINESEBIG5_CHARSET` for all five fonts: GDI answers on a Traditional
//! Chinese Windows with its Big5 face (PMingLiU, inferred), whose
//! `tmCharSet` (0x88) makes the text library take its double-byte
//! renderer and its Big5 line break (atlantis3-cd-pt-3424256
//! @ 0x006bfb30, @ 0x006c0dc0): widths per character from
//! `GetCharacterPlacementA`, the layout loop counting characters with the
//! C runtime's `_mbsinc` / `_mbsdec` in code page 950 (@ 0x006bd930, the
//! twin of 0x006b4a20), a line ending only where the kinsoku rule of
//! [`big5_break`] says. The port measures such a font with [`Dbcs`]: the
//! face is the player's PMingLiU (`mingliu.ttc` beside the game's files),
//! else the bundled Noto Serif TC, and the advances are **derived from
//! the face's tables** (`hmtx` at the cell's pixels per em, rounded), not
//! measured with GDI.

use crate::px::ScreenPixel;
use atlantis_data::TextCodePage;
use atlantis_engine::subtitles::{FaceSource, FontFace};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// GDI's measurements of `Arial` at one requested height.
#[derive(Clone, Debug)]
pub struct GdiFont {
    pub height: i32,
    /// `tmHeight`, the line step.
    pub tm_height: i32,
    pub ascent: i32,
    pub ave_char_width: i32,
    /// `tmCharSet`: 0 ANSI, 238 Central European.
    pub charset: u8,
    /// A, B, C of each byte (single-byte fonts).
    pub abc: Vec<(i32, i32, i32)>,
    /// A double-byte font (`CHINESEBIG5_CHARSET`): its face and code
    /// page; its widths are per character, [`GdiFont::dx`].
    pub dbcs: Option<Arc<Dbcs>>,
}

/// `CHINESEBIG5_CHARSET`: the Traditional Chinese CD's fonts.
pub const CHARSET_BIG5: u8 = 0x88;

/// A double-byte font's face and measures: the face GDI would give (the
/// player's PMingLiU, else the port's Noto Serif TC), at `ppem` pixels per
/// em, its texts read in `code_page` (950: a byte 0x81..0xfe and the byte
/// after it are one character, as the C runtime's `_mbsinc` reads them
/// after `_setmbcp(_MB_CP_ANSI)` @ 0x006d04de on a Traditional Chinese
/// Windows).
#[derive(Clone, Debug)]
pub struct Dbcs {
    pub code_page: TextCodePage,
    pub face: FontFace,
    pub ppem: f32,
    /// `tmBreakChar`: the bytes skipped after a break (0x006bdbea); the
    /// space [INFERRED: PMingLiU's, not measured].
    pub break_char: u8,
}

impl Dbcs {
    /// The character of `ch` (a byte or a pair): `'\0'` where the code
    /// page has none, and for code page 950's user-defined pairs.
    pub fn char_of(&self, ch: &[u8]) -> char {
        if let [lead, trail] = *ch {
            let user_defined = matches!(
                (lead, trail),
                (0x81..=0xa0, _) | (0xc6, 0xa1..) | (0xc7..=0xc8, _) | (0xfa..=0xfe, _)
            );
            if self.code_page == TextCodePage::Windows950 && user_defined {
                return '\0';
            }
        }
        self.code_page.decode(ch).chars().next().unwrap_or('\0')
    }

    /// The advance of each character of `s` ([`chars`]), in pixels: the
    /// face's `hmtx` advance at `ppem`, rounded (derived from the font's
    /// tables; GDI's `GetCharacterPlacementA` was not measured). A
    /// character without a glyph takes the missing glyph's.
    #[allow(clippy::cast_possible_truncation)]
    pub fn advances(&self, s: &[u8]) -> Vec<i32> {
        let (data, index) = self.face.file();
        let face = ttf_parser::Face::parse(data, index).ok();
        let upem = face
            .as_ref()
            .map_or(1000.0, |f| f32::from(f.units_per_em()));
        chars(s, self.code_page)
            .windows(2)
            .map(|w| {
                let c = self.char_of(&s[w[0]..w[1]]);
                let adv = face.as_ref().map_or(0, |f| {
                    let g = if c == '\0' {
                        ttf_parser::GlyphId(0)
                    } else {
                        f.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0))
                    };
                    f.glyph_hor_advance(g).unwrap_or(0)
                });
                (f32::from(adv) * self.ppem / upem).round() as i32
            })
            .collect()
    }
}

/// The starts of the characters of `s` in `code_page`, then `s.len()`: a
/// lead byte and the byte after it are one character (a lead byte at the
/// end is one alone); in a single-byte code page every byte is one.
pub fn chars(s: &[u8], code_page: TextCodePage) -> Vec<usize> {
    let mut out = Vec::with_capacity(s.len() + 1);
    let mut i = 0;
    while i < s.len() {
        out.push(i);
        i += if code_page.is_lead_byte(s[i]) && i + 1 < s.len() {
            2
        } else {
            1
        };
    }
    out.push(s.len());
    out
}

const TABLE: &str = include_str!("data/arial_gdi.txt");
/// The same measures with `EASTEUROPE_CHARSET`.
const TABLE_EE: &str = include_str!("data/arial_gdi_ee.txt");

/// `EASTEUROPE_CHARSET`.
pub const CHARSET_EE: u8 = 0xee;

fn table() -> &'static HashMap<i32, GdiFont> {
    static T: OnceLock<HashMap<i32, GdiFont>> = OnceLock::new();
    T.get_or_init(|| parse_table(TABLE))
}

fn table_ee() -> &'static HashMap<i32, GdiFont> {
    static T: OnceLock<HashMap<i32, GdiFont>> = OnceLock::new();
    T.get_or_init(|| parse_table(TABLE_EE))
}

fn parse_table(text: &str) -> HashMap<i32, GdiFont> {
    let mut out = HashMap::new();
    let mut cur: Option<GdiFont> = None;
    for l in text.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first() {
            Some(&"FONT") => {
                if let Some(f) = cur.take() {
                    out.insert(f.height, f);
                }
                let tm = w.iter().position(|x| *x == "tm").unwrap_or(0);
                let n = |i: usize| w.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                let cs = w
                    .iter()
                    .position(|x| *x == "charset")
                    .map_or(0, |i| n(i + 1));
                cur = Some(GdiFont {
                    height: n(2),
                    tm_height: n(tm + 1),
                    ascent: n(tm + 2),
                    ave_char_width: n(tm + 6),
                    charset: u8::try_from(cs).unwrap_or(0),
                    abc: vec![(0, 0, 0); 256],
                    dbcs: None,
                });
            }
            Some(&"ABC") => {
                if let Some(f) = cur.as_mut() {
                    let n = |i: usize| w.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
                    let b: usize = n(1) as usize;
                    if b < 256 {
                        f.abc[b] = (n(2), n(3), n(4));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(f) = cur.take() {
        out.insert(f.height, f);
    }
    out
}

impl GdiFont {
    /// GDI's `Arial` at a requested height, when measured.
    pub fn arial(height: i32) -> Option<&'static GdiFont> {
        table().get(&height)
    }

    /// GDI's `Arial` at a requested height in a `font.txt` character set:
    /// the Central European measures for `EASTEUROPE_CHARSET`, else the
    /// ANSI ones (see the module's note for `SHIFTJIS_CHARSET`).
    pub fn arial_charset(height: i32, charset: u8) -> Option<&'static GdiFont> {
        if charset == CHARSET_EE {
            table_ee().get(&height)
        } else {
            Self::arial(height)
        }
    }

    /// A double-byte font for a request of `height` (a cell height, as
    /// `font.txt`'s) in `CHINESEBIG5_CHARSET`, drawn in `face`. The
    /// player's face is sized as GDI sizes a positive height: pixels per
    /// em = MulDiv(unitsPerEm, height, usWinAscent + usWinDescent), its
    /// ascent the Windows ascent at that size; the port's Noto Serif TC
    /// stands in for PMingLiU, whose cell is its em [INFERRED], at
    /// `height` pixels per em with its ideographs' ascent
    /// ([`FontFace::cjk_ascent`]). `tmHeight` is `height` [INFERRED];
    /// `tmAveCharWidth` the face's `xAvgCharWidth` (the layout's estimate
    /// only, which does not change the lines).
    pub fn big5(face: FontFace, height: i32) -> GdiFont {
        let h = height.max(1);
        let (data, index) = face.file();
        let parsed = ttf_parser::Face::parse(data, index).ok();
        let upem = parsed
            .as_ref()
            .map_or(1000, |f| i64::from(f.units_per_em()));
        let os2 = parsed.as_ref().and_then(|f| f.tables().os2);
        #[allow(clippy::cast_precision_loss)]
        let ppem = match (face.source(), os2) {
            (FaceSource::Player, Some(o))
                if i64::from(o.windows_ascender()) + i64::from(o.windows_descender().abs()) > 0 =>
            {
                let cell = i64::from(o.windows_ascender()) + i64::from(o.windows_descender().abs());
                ((upem * i64::from(h) + cell / 2) / cell).max(1) as f32
            }
            _ => h as f32,
        };
        #[allow(clippy::cast_possible_truncation)]
        let ascent = face
            .cjk_ascent(ppem)
            .unwrap_or((ppem * 0.88).round() as i32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // OS/2 xAvgCharWidth, an int16 at byte 2.
        let ave = parsed
            .as_ref()
            .and_then(|f| f.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2")))
            .and_then(|t| t.get(2..4))
            .map_or(h / 2, |r| {
                (f32::from(i16::from_be_bytes([r[0], r[1]])) * ppem / upem as f32).round() as i32
            });
        GdiFont {
            height,
            tm_height: h,
            ascent,
            ave_char_width: ave.max(1),
            charset: CHARSET_BIG5,
            abc: vec![(0, 0, 0); 256],
            dbcs: Some(Arc::new(Dbcs {
                code_page: TextCodePage::Windows950,
                face,
                ppem,
                break_char: b' ',
            })),
        }
    }

    /// [`GdiFont::big5`] at `height` in `face`, made once per face and
    /// height for the program's life (the fonts are handed out as
    /// `&'static`, as the measured tables are).
    pub fn big5_cached(face: &FontFace, height: i32) -> &'static GdiFont {
        type Cache = Mutex<Vec<(FaceSource, usize, u32, i32, &'static GdiFont)>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let (data, index) = face.file();
        let key = (face.source(), data.len(), index, height);
        let mut c = CACHE
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&(.., f)) = c.iter().find(|e| (e.0, e.1, e.2, e.3) == key) {
            return f;
        }
        let f: &'static GdiFont = Box::leak(Box::new(GdiFont::big5(face.clone(), height)));
        c.push((key.0, key.1, key.2, key.3, f));
        f
    }

    /// The code page the font's bytes are read in.
    pub fn code_page(&self) -> TextCodePage {
        match &self.dbcs {
            Some(d) => d.code_page,
            None if self.charset == CHARSET_EE => TextCodePage::Windows1250,
            None => TextCodePage::Windows1252,
        }
    }

    /// The advances `ExtTextOutA` is given (@ 0x006b6880): A + B + C per
    /// byte, the last byte's negative C and the first byte's positive A
    /// taken off. A double-byte font's are per character, without
    /// trims ([`Dbcs::advances`]: `GetCharacterPlacementA`'s `lpDx`,
    /// @ 0x006bfd30).
    pub fn dx(&self, s: &[u8]) -> Vec<i32> {
        if let Some(d) = &self.dbcs {
            return d.advances(s);
        }
        let n = s.len();
        s.iter()
            .enumerate()
            .map(|(i, &c)| {
                let (a, b, cc) = self.abc[usize::from(c)];
                let mut d = a + b + cc;
                if i + 1 == n && cc < 0 {
                    d -= cc;
                }
                if i == 0 && a > 0 {
                    d -= a;
                }
                d
            })
            .collect()
    }

    pub fn width(&self, s: &[u8]) -> i32 {
        self.dx(s).iter().sum()
    }
}

/// The text ids of `font.txt` the code asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontId {
    Default,
    Bulle,
    Special,
    Titre,
    Credit,
}

impl FontId {
    pub fn name(self) -> &'static str {
        match self {
            FontId::Default => "AtlFont_Default",
            FontId::Bulle => "AtlFont_Bulle",
            FontId::Special => "AtlFont_Special",
            FontId::Titre => "AtlFont_Titre",
            FontId::Credit => "AtlFont_Credit",
        }
    }
}

/// A layout (`AtlFont_SetLayout` @ 0x0041e190): the margins and the
/// alignments (horizontal 0 left, 1 right, 2 centre; vertical 0 top, 1
/// bottom, 2 middle). *Atlantis Evolution*'s 2004 program has a fourth
/// horizontal code, [`HALIGN_2004`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub ml: i32,
    pub mr: i32,
    pub mt: i32,
    pub mb: i32,
    pub halign: i32,
    pub valign: i32,
}

/// The 2004 program's horizontal code 4 (atlantis4-cd-en-3022848
/// FUN_00420b00, the `DrawTextA` format of the layout 0x006e73d8): with a
/// font whose `tmCharSet` is `HEBREW_CHARSET` (0xb1) or `ARABIC_CHARSET`
/// (0xb2), `DT_RIGHT | DT_RTLREADING` (0x20002); otherwise no flag, left
/// (`DT_LEFT`). Its callers: the dialog line (FUN_0046f630), the
/// inventory's hover label (FUN_0042bed0) and the load page's dates
/// (FUN_00459af0). The right-to-left reading order itself is not ported
/// (no CD font asks for those character sets: `font.txt` gives
/// `ANSI_CHARSET`).
pub const HALIGN_2004: i32 = 4;

impl Zone {
    pub const fn new(ml: i32, mr: i32, mt: i32, mb: i32, halign: i32, valign: i32) -> Zone {
        Zone {
            ml,
            mr,
            mt,
            mb,
            halign,
            valign,
        }
    }
}

/// The dialog line's layouts (`CDialogue` draw @ 0x0046a0e0): the black
/// shadow, then the white text.
pub const DIALOG_SHADOW: Zone = Zone::new(9, 11, 21, 19, 0, 1);
pub const DIALOG_TEXT: Zone = Zone::new(10, 10, 20, 20, 0, 1);
/// A caption (`CineNNN.txt`, @ 0x0040de60, flag 1): centred, no shadow.
pub const CAPTION: Zone = Zone::new(10, 10, 35, 25, 2, 1);
/// A cue line of `hnm\cineNNN.txt` (flag 0) and the voice slot k (bottom
/// margin + 15 k): shadow, then text.
pub const CUE_SHADOW: Zone = Zone::new(9, 11, 36, 4, 0, 1);
pub const CUE_TEXT: Zone = Zone::new(10, 10, 35, 5, 0, 1);

/// One line drawn: `ExtTextOutA(x, y, bytes, dx)`, (x, y) the top-left of
/// the text cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub x: i32,
    pub y: i32,
    pub bytes: Vec<u8>,
    pub dx: Vec<i32>,
}

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Where to break a paragraph overflowing at `e` (@ 0x006b7920): the
/// start of the last run of blanks before it, and that run's length;
/// the run just before a `?`, `!` or `:` (the byte at the overflow, past
/// blanks) is not a break (@ 0x006b78d0). (0, 0) without a break.
fn break_before(p: &[u8], e: usize) -> (usize, usize) {
    if e == 0 {
        return (0, 0);
    }
    let mut i = e - 1;
    let mut j = i;
    while j < p.len() && is_blank(p[j]) {
        j += 1;
    }
    let mut fr = j < p.len() && matches!(p[j], b'?' | b'!' | b':');
    loop {
        if is_blank(p[i]) {
            let top = i;
            while i > 0 && is_blank(p[i - 1]) {
                i -= 1;
            }
            if !fr {
                return (i, top - i + 1);
            }
            fr = false;
        }
        if i == 0 {
            return (0, 0);
        }
        i -= 1;
    }
}

/// The Big5 line break (atlantis3-cd-pt-3424256 @ 0x006c0dc0), its lists
/// of strings compared as prefixes (0x006c0910): A, after which a line
/// may end even past the zone (`! ) , . : ; ? ] }`); B, which may not end
/// a line (the opening marks); C, which may not start one (A and the
/// closing marks and the punctuation A141..A15C).
pub const BIG5_LIST_A: [&[u8]; 9] = [b"!", b")", b",", b".", b":", b";", b"?", b"]", b"}"];
pub const BIG5_LIST_B: [&[u8]; 26] = [
    b"(",
    b"[",
    b"{",
    b"\xa1\x5d",
    b"\xa1\x5f",
    b"\xa1\x61",
    b"\xa1\x63",
    b"\xa1\x65",
    b"\xa1\x67",
    b"\xa1\x69",
    b"\xa1\x6b",
    b"\xa1\x6d",
    b"\xa1\x6f",
    b"\xa1\x71",
    b"\xa1\x73",
    b"\xa1\x75",
    b"\xa1\x77",
    b"\xa1\x79",
    b"\xa1\x7b",
    b"\xa1\x7d",
    b"\xa1\xa1",
    b"\xa1\xa3",
    b"\xa1\xa5",
    b"\xa1\xa7",
    b"\xa1\xa9",
    b"\xa1\xab",
];

/// List C: list A, A141..A15C, then the closing marks.
fn big5_list_c(s: &[u8]) -> bool {
    const CLOSING: [u8; 23] = [
        0x5e, 0x60, 0x62, 0x64, 0x66, 0x68, 0x6a, 0x6c, 0x6e, 0x70, 0x72, 0x74, 0x76, 0x78, 0x7a,
        0x7c, 0x7e, 0xa2, 0xa4, 0xa6, 0xa8, 0xaa, 0xac,
    ];
    begins_with(s, &BIG5_LIST_A)
        || matches!(s, [0xa1, t, ..] if (0x41..=0x5c).contains(t) || CLOSING.contains(t))
}

fn begins_with(s: &[u8], list: &[&[u8]]) -> bool {
    list.iter().any(|e| s.starts_with(e))
}

/// Where the next line starts when a paragraph overflows at the
/// character `e` (its index in `st`, the paragraph's character starts;
/// the character `e - 1` is the first that does not fit), as the Big5
/// finder (@ 0x006c0dc0) answers: after that character when it begins
/// with list A; else back over the list B characters before it, then
/// back over list C characters. `None` where the original's walk would
/// leave the paragraph (`_mbsdec` at its start: a NULL the finder then
/// reads [INFERRED: a crash]; no shipped text gets there).
pub fn big5_break(p: &[u8], st: &[usize], e: usize) -> Option<usize> {
    let at = |k: usize| &p[st[k]..];
    let mut q = e.checked_sub(1)?;
    if begins_with(at(q), &BIG5_LIST_A) {
        return Some(q + 1);
    }
    while q > 0 && begins_with(at(q - 1), &BIG5_LIST_B) {
        q -= 1;
    }
    if q == 0 {
        return None;
    }
    while big5_list_c(at(q)) {
        q = q.checked_sub(1)?;
    }
    Some(q)
}

/// One paragraph's lines with a double-byte font (@ 0x006bd930 with the
/// Big5 finder): the loop of [`layout`] counting characters, widths summed
/// per character, a line ending only at [`big5_break`]'s answer, the next
/// one starting past the bytes equal to `tmBreakChar`.
fn dbcs_lines(font: &GdiFont, d: &Dbcs, p: &[u8], avail: i32, limit: i32, ml: i32) -> Vec<Vec<u8>> {
    let adv = d.advances(p);
    big5_lines(p, &adv, font.ave_char_width, avail, limit, ml, d.break_char)
        .into_iter()
        .map(|(s, e)| p[s..e].to_vec())
        .collect()
}

/// The lines (byte ranges) of paragraph `p` in code page 950, its
/// characters' advances `adv`, as the layout loop @ 0x006bd930 makes them
/// with the Big5 finder: the estimate `avail / ave` characters, shrunk or
/// extended one character at a time to the first end that overflows
/// (`ml + width > limit`), the line ended at [`big5_break`], the next
/// one started past the bytes equal to `break_char`; a paragraph's last
/// line whatever it holds (an empty one after a break that leaves only
/// such bytes).
#[allow(clippy::too_many_arguments)]
pub fn big5_lines(
    p: &[u8],
    adv: &[i32],
    ave: i32,
    avail: i32,
    limit: i32,
    ml: i32,
    break_char: u8,
) -> Vec<(usize, usize)> {
    big5_lines_cut(p, adv, ave, avail, limit, ml, break_char).0
}

/// [`big5_lines`] and how many lines the port had to cut where the
/// original never returns (no Big5 break above the line's start).
#[allow(clippy::too_many_arguments)]
pub fn big5_lines_cut(
    p: &[u8],
    adv: &[i32],
    ave: i32,
    avail: i32,
    limit: i32,
    ml: i32,
    break_char: u8,
) -> (Vec<(usize, usize)>, usize) {
    let mut cuts = 0;
    let st = chars(p, TextCodePage::Windows950);
    let n = st.len() - 1;
    let mut pw = Vec::with_capacity(n + 1);
    pw.push(0);
    for k in 0..n {
        pw.push(pw[k] + adv.get(k).copied().unwrap_or(0));
    }
    let mut out = Vec::new();
    let mut s = 0usize;
    loop {
        let over = |e: usize| ml + pw[e] - pw[s] > limit;
        let k = if ave > 0 {
            (avail / ave).max(0) as usize
        } else {
            0
        };
        let mut e = s + k.min(n - s);
        let overflow;
        if over(e) {
            while e > s && over(e) {
                e -= 1;
            }
            e += 1;
            overflow = true;
        } else {
            loop {
                if e == n {
                    overflow = false;
                    break;
                }
                e += 1;
                if over(e) {
                    overflow = true;
                    break;
                }
            }
        }
        if !overflow {
            out.push((st[s], st[e]));
            return (out, cuts);
        }
        match big5_break(p, &st, e).filter(|&b| b > s) {
            Some(b) => {
                out.push((st[s], st[b]));
                s = b;
            }
            None => {
                // The original never returns (or reads address 0); no
                // shipped text gets here: the port cuts before the
                // character that overflows.
                let cut = (e - 1).max(s + 1);
                out.push((st[s], st[cut]));
                s = cut;
                cuts += 1;
            }
        }
        while s < n && p[st[s]] == break_char {
            s += 1;
        }
    }
}

/// The lines of `text` (Windows-1252 bytes; Big5 with a double-byte font)
/// for a `w` x `h` screen (@ 0x006b4450, @ 0x006b51b0, @ 0x006b5370).
pub fn layout(font: &GdiFont, text: &[u8], w: i32, h: i32, z: Zone) -> Vec<Line> {
    let avail = (w - 1) - z.mr - z.ml;
    let limit = w - 1 - z.mr;
    let over = |s: &[u8]| z.ml + font.width(s) > limit;
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for p in text.split(|&c| c == b'\n') {
        if let Some(d) = &font.dbcs {
            lines.extend(dbcs_lines(font, d, p, avail, limit, z.ml));
            continue;
        }
        let mut s = 0usize;
        loop {
            let chars = if font.ave_char_width > 0 {
                (avail / font.ave_char_width).max(0) as usize
            } else {
                0
            };
            let mut e = s + chars.min(p.len() - s);
            let overflow;
            if over(&p[s..e]) {
                loop {
                    e -= 1;
                    if !over(&p[s..e]) {
                        break;
                    }
                }
                e += 1;
                overflow = true;
            } else {
                loop {
                    if e == p.len() {
                        overflow = false;
                        break;
                    }
                    e += 1;
                    if over(&p[s..e]) {
                        overflow = true;
                        break;
                    }
                }
            }
            if !overflow {
                lines.push(p[s..e].to_vec());
                break;
            }
            let (b, n) = break_before(p, e);
            if b <= s {
                // The original never returns (a word wider than the zone;
                // no shipped text has one): the port cuts the word.
                let cut = (e - 1).max(s + 1);
                lines.push(p[s..cut].to_vec());
                s = cut;
                continue;
            }
            lines.push(p[s..b].to_vec());
            s = b + n;
            while s < p.len() && p[s] == b' ' {
                s += 1;
            }
        }
    }
    let th = font.tm_height;
    let n = lines.len() as i32;
    let zone = (h - 1) - z.mb - z.mt;
    let y0 = match z.valign {
        1 => zone - n * th,
        2 => (zone - n * th) / 2,
        _ => 0,
    };
    lines
        .into_iter()
        .enumerate()
        .filter_map(|(j, bytes)| {
            let y = z.mt + y0 + j as i32 * th;
            if y >= (h - 1) - z.mb {
                return None;
            }
            let dx = font.dx(&bytes);
            let wd: i32 = dx.iter().sum();
            let x = match z.halign {
                1 => z.ml + avail - wd,
                HALIGN_2004 if matches!(font.charset, 0xb1 | 0xb2) => z.ml + avail - wd,
                2 => z.ml + (avail - wd) / 2,
                _ => z.ml,
            };
            Some(Line { x, y, bytes, dx })
        })
        .collect()
}

/// Draws lines of a font into a screen (RGB565 or x8r8g8b8): each glyph at its pen
/// position, the baseline `tmAscent` below the line's y, its coverage
/// blended in `rgb` (0xRRGGBB).
pub struct Glyphs {
    font: ab_glyph::FontVec,
}

impl Glyphs {
    /// The game's `font\Atlantis.ttf` (Arial).
    pub fn new(ttf: Vec<u8>) -> Option<Glyphs> {
        ab_glyph::FontVec::try_from_vec(ttf)
            .ok()
            .map(|font| Glyphs { font })
    }

    pub fn draw<P: ScreenPixel>(
        &self,
        gdi: &GdiFont,
        lines: &[Line],
        rgb: u32,
        out: &mut [P],
        w: usize,
        h: usize,
    ) {
        self.draw_in(gdi, lines, rgb, out, (0, 0, w, h), w, h);
    }

    /// The pixels the glyphs of `lines` may cover: (x0, y0, x1, y1), x1
    /// and y1 past the last; `None` when no glyph has an outline.
    pub fn bounds(&self, gdi: &GdiFont, lines: &[Line]) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        self.each_glyph(gdi, lines, |og| {
            let bb = og.px_bounds();
            #[allow(clippy::cast_possible_truncation)]
            let g = (
                bb.min.x as i32,
                bb.min.y as i32,
                bb.max.x.ceil() as i32 + 1,
                bb.max.y.ceil() as i32 + 1,
            );
            b = Some(match b {
                None => g,
                Some(a) => (a.0.min(g.0), a.1.min(g.1), a.2.max(g.2), a.3.max(g.3)),
            });
        });
        b
    }

    /// Each glyph of `lines` with an outline, placed as [`Glyphs::draw`]
    /// places it.
    fn each_glyph(
        &self,
        gdi: &GdiFont,
        lines: &[Line],
        mut f: impl FnMut(ab_glyph::OutlinedGlyph),
    ) {
        use ab_glyph::{Font, ScaleFont};
        if let Some(d) = &gdi.dbcs {
            let (data, index) = d.face.file();
            let Ok(font) = ab_glyph::FontRef::try_from_slice_and_index(data, index) else {
                return;
            };
            // ab_glyph scales by the line height (ascent − descent).
            let upem = font.units_per_em().unwrap_or(1000.0);
            let scale = ab_glyph::PxScale::from(d.ppem * font.height_unscaled() / upem);
            for l in lines {
                let mut pen = l.x as f32;
                let base = (l.y + gdi.ascent) as f32;
                let st = chars(&l.bytes, d.code_page);
                for (w, &adv) in st.windows(2).zip(&l.dx) {
                    let c = d.char_of(&l.bytes[w[0]..w[1]]);
                    if c > ' ' {
                        let id = font.glyph_id(c);
                        let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, base));
                        if let Some(og) = font.outline_glyph(glyph) {
                            f(og);
                        }
                    }
                    pen += adv as f32;
                }
            }
            return;
        }
        let scale = ab_glyph::PxScale::from(gdi.tm_height as f32);
        let sf = self.font.as_scaled(scale);
        for l in lines {
            let mut pen = l.x as f32;
            let base = (l.y + gdi.ascent) as f32;
            for (&c, &d) in l.bytes.iter().zip(&l.dx) {
                let ch = if gdi.charset == CHARSET_EE {
                    cp1250(c)
                } else {
                    cp1252(c)
                };
                let id = sf.glyph_id(ch);
                let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, base));
                if let Some(og) = self.font.outline_glyph(glyph) {
                    f(og);
                }
                pen += d as f32;
            }
        }
    }

    /// [`Glyphs::draw`] on a `w` × `h` screen of which `out` holds only
    /// the rectangle `rect` (x, y, width, height; row by row): the
    /// glyphs placed and clipped on the screen, the pixels outside the
    /// rectangle left out.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_in<P: ScreenPixel>(
        &self,
        gdi: &GdiFont,
        lines: &[Line],
        rgb: u32,
        out: &mut [P],
        rect: (i32, i32, usize, usize),
        w: usize,
        h: usize,
    ) {
        let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
        let (rx, ry, rw, rh) = rect;
        self.each_glyph(gdi, lines, |og| {
            let bb = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let x = bb.min.x as i32 + gx as i32;
                let y = bb.min.y as i32 + gy as i32;
                if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || cov <= 0.0 {
                    return;
                }
                let (sx, sy) = (x - rx, y - ry);
                if sx < 0 || sy < 0 || sx >= rw as i32 || sy >= rh as i32 {
                    return;
                }
                let px = &mut out[sy as usize * rw + sx as usize];
                let a = cov.min(1.0);
                let [dr, dg, db] = px.rgb8();
                let mix = |s: u32, d: u8| (s as f32 * a + f32::from(d) * (1.0 - a)) as u8;
                *px = P::from_rgb8(mix(r, dr), mix(g, dg), mix(b, db));
            });
        });
    }
}

/// A Windows-1252 byte as a character.
pub fn cp1252(c: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039,
        0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
        0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
    ];
    if (0x80..0xa0).contains(&c) {
        char::from_u32(u32::from(HIGH[usize::from(c - 0x80)])).unwrap_or('?')
    } else {
        char::from(c)
    }
}

/// A Windows-1250 (Central European) byte as a character.
pub fn cp1250(c: u8) -> char {
    const HIGH: [u16; 128] = [
        0x20ac, 0x0081, 0x201a, 0x0083, 0x201e, 0x2026, 0x2020, 0x2021, 0x0088, 0x2030, 0x0160,
        0x2039, 0x015a, 0x0164, 0x017d, 0x0179, 0x0090, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022,
        0x2013, 0x2014, 0x0098, 0x2122, 0x0161, 0x203a, 0x015b, 0x0165, 0x017e, 0x017a, 0x00a0,
        0x02c7, 0x02d8, 0x0141, 0x00a4, 0x0104, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x015e, 0x00ab,
        0x00ac, 0x00ad, 0x00ae, 0x017b, 0x00b0, 0x00b1, 0x02db, 0x0142, 0x00b4, 0x00b5, 0x00b6,
        0x00b7, 0x00b8, 0x0105, 0x015f, 0x00bb, 0x013d, 0x02dd, 0x013e, 0x017c, 0x0154, 0x00c1,
        0x00c2, 0x0102, 0x00c4, 0x0139, 0x0106, 0x00c7, 0x010c, 0x00c9, 0x0118, 0x00cb, 0x011a,
        0x00cd, 0x00ce, 0x010e, 0x0110, 0x0143, 0x0147, 0x00d3, 0x00d4, 0x0150, 0x00d6, 0x00d7,
        0x0158, 0x016e, 0x00da, 0x0170, 0x00dc, 0x00dd, 0x0162, 0x00df, 0x0155, 0x00e1, 0x00e2,
        0x0103, 0x00e4, 0x013a, 0x0107, 0x00e7, 0x010d, 0x00e9, 0x0119, 0x00eb, 0x011b, 0x00ed,
        0x00ee, 0x010f, 0x0111, 0x0144, 0x0148, 0x00f3, 0x00f4, 0x0151, 0x00f6, 0x00f7, 0x0159,
        0x016f, 0x00fa, 0x0171, 0x00fc, 0x00fd, 0x0163, 0x02d9,
    ];
    if c < 0x80 {
        char::from(c)
    } else {
        char::from_u32(u32::from(HIGH[usize::from(c - 0x80)])).unwrap_or('?')
    }
}

/// A character as a Windows-1252 byte (`?` when it has none).
pub fn to_cp1252(ch: char) -> u8 {
    let u = ch as u32;
    if u < 0x80 || (0xa0..0x100).contains(&u) {
        return u as u8;
    }
    (0x80u8..0xa0).find(|&b| cp1252(b) == ch).unwrap_or(b'?')
}

/// The game's text resources: the fonts of `font\font.txt`, the glyphs of
/// `font\Atlantis.ttf` and the dialog texts of `loc\text\<lang>\Dial.txt`
/// (the cue lines' and the dialogs' subtitles).
pub struct Texts {
    glyphs: Option<Glyphs>,
    /// `SousTitre`, which picks the charset of `ANSI_CHARSET` fonts.
    language: String,
    /// Whether the program's charset chain has the `tch` case (the 2004
    /// program's 2004-10-07 rebuild,
    /// `atlantis3_data::evolution2004::Build::tch_east_europe`).
    tch_east_europe: bool,
    fonts: atlantis3_data::font_txt::FontTxt,
    dial: HashMap<(i32, i32), Vec<u8>>,
    /// The face of the `CHINESEBIG5_CHARSET` fonts ([`Texts::big5_face`]),
    /// when `font.txt` asks for one.
    big5: Option<FontFace>,
}

/// Every font id `font.txt` gives.
pub const FONT_IDS: [FontId; 5] = [
    FontId::Default,
    FontId::Bulle,
    FontId::Special,
    FontId::Titre,
    FontId::Credit,
];

impl Texts {
    pub fn load(files: &atlantis3_data::A3Files, language: &str) -> Texts {
        let strip = |p: &str| p.trim_start_matches("Datas/").to_string();
        // The 2004 program's font and its `font.txt` are in the text
        // language's folder: FUN_0041f270 builds `loc\text\<SousTitre>\`
        // + `Atlantis` (0x006add08) through FUN_00419880, then
        // `%s%s.ttf` (0x006adcfc, @ 0x0041f32a) with the current
        // directory, and `loc\text\<SousTitre>\font.txt` (0x006adcf0,
        // @ 0x0041f3f0).
        let (ttf, font_txt) = if files.is_evolution_2004() {
            (
                format!("loc/text/{language}/Atlantis.ttf"),
                format!("loc/text/{language}/font.txt"),
            )
        } else {
            (
                strip(atlantis3_data::font_txt::TTF_PATH),
                strip(atlantis3_data::font_txt::FONT_TXT_PATH),
            )
        };
        let glyphs = files.read(&ttf).and_then(Glyphs::new);
        if glyphs.is_none() {
            log::warn!("font\\Atlantis.ttf missing: no text drawn");
        }
        let fonts = files
            .read(&font_txt)
            .map(|d| {
                if files.is_evolution_2004() {
                    atlantis3_data::font_txt::FontTxt::parse_2004(&d)
                } else {
                    atlantis3_data::font_txt::FontTxt::parse(&d)
                }
            })
            .unwrap_or_default();
        let dial = files
            .read(&strip(&atlantis3_data::dialog::dial_path(language)))
            .map(|d| {
                if files.is_evolution_2004() {
                    atlantis3_data::dialog::parse_dial_2004(&d)
                } else {
                    atlantis3_data::dialog::parse_dial(&d)
                }
                .lines
                .into_iter()
                .map(|l| ((l.dialog, l.line), l.text))
                .collect()
            })
            .unwrap_or_default();
        let tch_east_europe = files
            .evolution_2004_source()
            .and_then(|s| s.build())
            .is_some_and(|b| b.tch_east_europe);
        let big5 = FONT_IDS
            .iter()
            .any(|id| {
                fonts
                    .font(id.name())
                    .is_some_and(|d| d.charset_in(language, tch_east_europe) == CHARSET_BIG5)
            })
            .then(|| Texts::big5_face(files));
        Texts {
            glyphs,
            language: language.to_string(),
            tch_east_europe,
            fonts,
            dial,
            big5,
        }
    }

    /// The face a Traditional Chinese Windows gives `Arial` in
    /// `CHINESEBIG5_CHARSET` with `VARIABLE_PITCH` [INFERRED: PMingLiU, not
    /// measured]: the player's `mingliu.ttc` beside the game's files (its
    /// face PMingLiU), else the port's Noto Serif TC (in the web build,
    /// the one its page fetched).
    pub fn big5_face(files: &atlantis3_data::A3Files) -> FontFace {
        use atlantis_engine::subtitles::player_font;
        let code_page = TextCodePage::Windows950;
        if let Some((name, _)) = player_font(code_page) {
            if let Some(data) = files.source().read(name) {
                match FontFace::player_family(data, "PMingLiU") {
                    Some(face) => {
                        log::info!("text font: the player's {name} (PMingLiU)");
                        return face;
                    }
                    None => log::warn!("{name}: not a font the port can use"),
                }
            }
        }
        FontFace::bundled(code_page)
    }

    /// The face of the Big5 fonts, when `font.txt` has any.
    pub fn big5(&self) -> Option<&FontFace> {
        self.big5.as_ref()
    }

    /// The text of line `line` of dialog `dialog` (`GetTextFromDial`).
    pub fn dial(&self, dialog: i32, line: i32) -> Option<&[u8]> {
        self.dial.get(&(dialog, line)).map(Vec::as_slice)
    }

    /// GDI's measurements of font `id` at a screen width.
    pub fn gdi(&self, id: FontId, width: usize) -> Option<&'static GdiFont> {
        let def = self.fonts.font(id.name())?;
        let h = def.scaled_height(width as u32)?;
        let charset = def.charset_in(&self.language, self.tch_east_europe);
        if charset == CHARSET_BIG5 {
            return self.big5.as_ref().map(|f| GdiFont::big5_cached(f, h));
        }
        GdiFont::arial_charset(h, charset)
    }

    /// `AtlFont_SetLayout` + `AtlFont_Print` (@ 0x0041e190, @ 0x0041e2c0).
    #[allow(clippy::too_many_arguments)]
    pub fn print<P: ScreenPixel>(
        &self,
        out: &mut [P],
        w: usize,
        h: usize,
        id: FontId,
        text: &[u8],
        zone: Zone,
        rgb: u32,
    ) {
        let (Some(g), Some(gdi)) = (&self.glyphs, self.gdi(id, w)) else {
            return;
        };
        let lines = layout(gdi, text, w as i32, h as i32, zone);
        g.draw(gdi, &lines, rgb, out, w, h);
    }

    /// A text with its black shadow at (−1, +1): the dialog lines, the cue
    /// lines and the voice slots.
    #[allow(clippy::too_many_arguments)]
    pub fn print_shadowed<P: ScreenPixel>(
        &self,
        out: &mut [P],
        w: usize,
        h: usize,
        text: &[u8],
        shadow: Zone,
        zone: Zone,
        rgb: u32,
    ) {
        self.print(out, w, h, FontId::Default, text, shadow, 0);
        self.print(out, w, h, FontId::Default, text, zone, rgb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The synthetic cases of atlantis3-re `tools/emu/a3_text_model_dbcs.py
    /// --cases` (every pair 18 wide, every byte 9; W 200, margins 10, 10):
    /// the same lines. `a440` an ideograph, `a141` a comma (list C), `a175`
    /// an opening bracket (list B), `a176` its closing one (list C).
    #[test]
    fn big5_lines_as_the_reference_model() {
        let hex = |h: &str| -> Vec<u8> {
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect()
        };
        let (cjk, comma, open, close) = ("a440", "a141", "a175", "a176");
        let r = |n: usize| cjk.repeat(n);
        let cases: Vec<(String, &str)> = vec![
            (r(5), "0-10"),
            (r(30), "0-18 18-36 36-54 54-60"),
            (r(9) + comma + &r(5), "0-16 16-30"),
            (r(8) + open + &r(5), "0-16 16-28"),
            (r(9) + close + &r(5), "0-16 16-30"),
            (r(9) + "2e" + &r(5), "0-19 19-29"),
            (r(4) + "2020" + &r(8), "0-18 18-26"),
            ("4142".to_string() + &r(3) + "3132" + &r(12), "0-18 18-34"),
            (r(8) + comma + close + &r(4), "0-14 14-28"),
        ];
        for (h, want) in cases {
            let p = hex(&h);
            let st = chars(&p, TextCodePage::Windows950);
            let adv: Vec<i32> = st
                .windows(2)
                .map(|w| if w[1] - w[0] == 2 { 18 } else { 9 })
                .collect();
            let got: Vec<String> = big5_lines(&p, &adv, 9, 179, 189, 10, b' ')
                .iter()
                .map(|(s, e)| format!("{s}-{e}"))
                .collect();
            assert_eq!(got.join(" "), want, "{h}");
        }
    }

    /// The finder: list A hangs past the zone, list B moves to the next
    /// line, list C pulls the character before it; an overflow on the
    /// paragraph's first character has no answer.
    #[test]
    fn big5_finder_lists() {
        let p = b"\xa4\x40\xa4\x40\xa1\x41\xa4\x40";
        let st = chars(p, TextCodePage::Windows950);
        assert_eq!(st, [0, 2, 4, 6, 8]);
        assert_eq!(big5_break(p, &st, 3), Some(1));
        assert_eq!(big5_break(p, &st, 2), Some(1));
        assert_eq!(big5_break(p, &st, 1), None);
        let p = b"\xa4\x40!\xa4\x40";
        let st = chars(p, TextCodePage::Windows950);
        assert_eq!(big5_break(p, &st, 2), Some(2));
        let p = b"\xa4\x40\xa4\x40\xa1\x75\xa4\x40";
        let st = chars(p, TextCodePage::Windows950);
        assert_eq!(big5_break(p, &st, 4), Some(2));
    }

    /// The Central European table: its own charset and, on the bytes
    /// whose characters differ, its own widths (`è` against `č`).
    #[test]
    fn central_european_widths_and_characters() {
        for h in [16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 53, 60, 66] {
            let ansi = GdiFont::arial_charset(h, 0).unwrap();
            let ee = GdiFont::arial_charset(h, CHARSET_EE).unwrap();
            assert_eq!((ansi.charset, ee.charset), (0, CHARSET_EE));
            assert_eq!(ansi.tm_height, ee.tm_height);
            // ASCII and the German letters measure the same.
            for c in (0x20..0x7f).chain([0xc4, 0xd6, 0xdc, 0xdf, 0xe4, 0xf6, 0xfc]) {
                assert_eq!(ansi.abc[c], ee.abc[c], "{h} {c:#x}");
            }
        }
        let (a, e) = (
            GdiFont::arial_charset(21, 0).unwrap(),
            GdiFont::arial_charset(21, CHARSET_EE).unwrap(),
        );
        assert_ne!(a.width(b"\xe8"), e.width(b"\xe8"));
        assert_eq!((cp1252(0xe8), cp1250(0xe8)), ('è', 'č'));
        for c in 0..=255u8 {
            if c < 0x80 || matches!(c, 0xc4 | 0xd6 | 0xdc | 0xdf | 0xe4 | 0xf6 | 0xfc | 0xe9) {
                assert_eq!(cp1250(c), cp1252(c), "{c:#x}");
            }
        }
        // SHIFTJIS_CHARSET (the English font.txt) measures as ANSI here.
        assert_eq!(GdiFont::arial_charset(21, 0x80).unwrap().charset, 0);
    }
}
