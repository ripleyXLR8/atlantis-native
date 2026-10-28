//! The dialog subtitles: the line breaks of `FUN_0042ffe8`, the layout of
//! `FUN_00430178` and the font; for the 1997 build (atlantis-cd-en-688128),
//! its own: the 8x8 bitmap font `DIALOG\FONTS.BIN`, the breaks of
//! `FUN_0042e970` and the layout of `FUN_0042eb3c` ([`BitmapFont`]); for
//! the Chinese builds, the breaks of their `FUN_0042f424` ([`cjk`]).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (`subtitle_layout.cpp`, `engine.cpp` `subtitleFont`, the subtitle draw of
//! `dialog.cpp`; GPLv3, the ScummVM team).
//!
//! The original draws the subtitles with the system's Arial
//! (`FUN_0041ae20`: CreateFontA with a cell height of 16, which Windows 98
//! renders at 13 pixels per em, without smoothing). The port draws them
//! with Liberation Sans, which has the metrics of Arial, at 13 pixels per
//! em, monochrome, as the ScummVM port does when the player has no
//! arial.ttf. The font is bundled (`assets/fonts`, SIL Open Font Licence
//! 1.1) and rasterised by `ab_glyph`, so every target, the web included,
//! draws the same pixels.
//!
//! The Chinese builds ask GDI for a face no Windows has, which a Chinese
//! Windows answers with its MingLiU or SimSun ([`CJK_SUBTITLE_EM_PX`]): the
//! port draws with the player's own `mingliu.ttc` / `simsun.ttc` when it
//! is beside the game's files, else with Noto Serif TC / SC (the same Ming
//! / Song style; bundled, cut to the characters of the releases' scripts,
//! SIL Open Font Licence 1.1), at 16 pixels per em, monochrome, the bytes
//! read as Big5 / GBK. The web build leaves those two out of its program:
//! its page fetches the one a Chinese release needs, beside the page, and
//! hands it over ([`provide_cjk_font`]).
//!
//! The Japanese DVD build asks for MS PGothic ("ＭＳ Ｐゴシック") at the
//! same cell height: the port draws with the player's `msgothic.ttc` (its
//! face MS PGothic) beside the game's files or among the system's fonts
//! ([`system_font`]), else with Noto Sans JP (bundled, cut to the
//! characters of the release's scripts, SIL Open Font Licence 1.1), the
//! bytes read as Shift-JIS; the web build fetches it for that release only.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use atlantis_data::exe::CjkBreaker;
use atlantis_data::TextCodePage;

pub mod cjk;
pub use cjk::{cjk_subtitle_rows, split_subtitle_cjk, CjkSplit};

/// The subtitle font, Liberation Sans Regular.
static FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf");

/// The port's Liberation Sans Regular (metrics of Arial), the stand-in for
/// a system font when a game's own font file is missing.
pub fn system_font_data() -> &'static [u8] {
    FONT_DATA
}

/// The CJK subtitle fonts of the port, Noto Serif TC and SC Regular, cut
/// to the characters of the Chinese releases' chapter scripts with ASCII
/// and the punctuation (`assets/fonts/make_cjk_subset.py`). Built into the
/// desktop program; the web build fetches them ([`provide_cjk_font`]).
#[cfg(not(target_arch = "wasm32"))]
static NOTO_SERIF_TC: &[u8] = include_bytes!("../../../assets/fonts/NotoSerifTC-Subset.otf");
#[cfg(not(target_arch = "wasm32"))]
static NOTO_SERIF_SC: &[u8] = include_bytes!("../../../assets/fonts/NotoSerifSC-Subset.otf");
/// The Japanese subtitle font of the port, Noto Sans JP Regular, cut to the
/// characters of the Japanese DVD's scripts (`assets/fonts/make_cjk_subset.py`).
#[cfg(not(target_arch = "wasm32"))]
static NOTO_SANS_JP: &[u8] = include_bytes!("../../../assets/fonts/NotoSansJP-Subset.otf");

/// The CJK subtitle fonts the web page fetched, Big5's, GBK's and
/// Shift-JIS's.
#[cfg(target_arch = "wasm32")]
static FETCHED_CJK: [std::sync::OnceLock<&'static [u8]>; 3] = [
    std::sync::OnceLock::new(),
    std::sync::OnceLock::new(),
    std::sync::OnceLock::new(),
];

/// The file name of the port's CJK subtitle font for texts in
/// `code_page` (in `assets/fonts`), `None` for the single-byte code pages.
pub fn cjk_font_file(code_page: TextCodePage) -> Option<&'static str> {
    match code_page {
        TextCodePage::Windows950 => Some("NotoSerifTC-Subset.otf"),
        TextCodePage::Windows936 => Some("NotoSerifSC-Subset.otf"),
        TextCodePage::Windows932 => Some("NotoSansJP-Subset.otf"),
        TextCodePage::Windows1252 | TextCodePage::Dos850 => None,
    }
}

/// The web build: hands over the port's CJK subtitle font for texts in
/// `code_page` ([`cjk_font_file`]), which its page fetched, before the
/// game starts. False when `data` is no font (the subtitles then draw no
/// CJK character) or one was already given. The desktop build has them
/// built in.
#[cfg(target_arch = "wasm32")]
pub fn provide_cjk_font(code_page: TextCodePage, data: Vec<u8>) -> bool {
    let slot = match code_page {
        TextCodePage::Windows950 => &FETCHED_CJK[0],
        TextCodePage::Windows936 => &FETCHED_CJK[1],
        TextCodePage::Windows932 => &FETCHED_CJK[2],
        TextCodePage::Windows1252 | TextCodePage::Dos850 => return false,
    };
    if slot.get().is_some() || FontRef::try_from_slice(&data).is_err() {
        return false;
    }
    // Kept for the page's life, as the desktop build's.
    slot.set(Box::leak(data.into_boxed_slice())).is_ok()
}

/// The port's CJK subtitle font for texts in `code_page`.
#[cfg(not(target_arch = "wasm32"))]
fn bundled_cjk(code_page: TextCodePage) -> Option<&'static [u8]> {
    match code_page {
        TextCodePage::Windows950 => Some(NOTO_SERIF_TC),
        TextCodePage::Windows936 => Some(NOTO_SERIF_SC),
        TextCodePage::Windows932 => Some(NOTO_SANS_JP),
        TextCodePage::Windows1252 | TextCodePage::Dos850 => None,
    }
}

/// The port's CJK subtitle font for texts in `code_page`, once the page
/// has fetched it.
#[cfg(target_arch = "wasm32")]
fn bundled_cjk(code_page: TextCodePage) -> Option<&'static [u8]> {
    match code_page {
        TextCodePage::Windows950 => FETCHED_CJK[0].get().copied(),
        TextCodePage::Windows936 => FETCHED_CJK[1].get().copied(),
        TextCodePage::Windows932 => FETCHED_CJK[2].get().copied(),
        TextCodePage::Windows1252 | TextCodePage::Dos850 => None,
    }
}

/// The pixels per em of the subtitle font.
pub const SUBTITLE_EM_PX: f32 = 13.0;

/// The pixels per em of the Chinese builds' subtitle font.
/// atlantis-cd-zh-tw-696832 FUN_0041ad5c @ 0x0041ad77..0x0041ad99 (and
/// atlantis-cd-zh-cn-692736's, the same) calls CreateFontA(16, 0, 0, 0,
/// FW_DONTCARE, 0, 0, 0, DEFAULT_CHARSET, 0, 0, 0, 0, "MingLi") (the height
/// from WinMain @ 0x00418f89, the name at 0x004ac2e8); no Windows has a
/// face "MingLi", so GDI takes the default face of the system's character
/// set, MingLiU on a Traditional Chinese Windows, SimSun on a Simplified
/// one (inferred), whose cell is their em: 16 pixels per em. The size
/// Windows renders it at and whether it drew those faces' embedded 16-pixel
/// bitmaps are [UNKNOWN]; the port draws the outlines at 16 pixels per em,
/// thresholded (monochrome), as it does Arial's. The Japanese DVD build's
/// FUN_0041aec0 @ 0x0041aedb makes the same call with the face
/// "ＭＳ Ｐゴシック" (0x004ac2d8; the height from WinMain @ 0x00418f51),
/// MS PGothic, whose cell is its em too.
pub const CJK_SUBTITLE_EM_PX: f32 = 16.0;

/// `FUN_00430178`: the x of every subtitle line.
pub const SUBTITLE_X: i32 = 5;

/// The width a subtitle line may take: the screen less 5-pixel margins.
pub const SUBTITLE_WIDTH: i32 = 640 - 5 - 5;

/// Text colour and shadow (white, black shadow at +1, +1).
pub const SUBTITLE_COLOR: u16 = 0xffff;
pub const SUBTITLE_SHADOW: u16 = 0x0000;

/// `FUN_00430178`'s row step: the lines 14 pixels apart.
pub const SUBTITLE_ROW_STEP: i32 = 14;

/// How much lower the Japanese DVD build draws its Japanese rows: the
/// bottom row's base 0x1f2 instead of 0x1ce (FUN_0042f86c @
/// 0x0042f887..0x0042f897, while the subtitle language counter is 0).
pub const JAPANESE_ROWS_LOWER: i32 = 0x1f2 - 0x1ce;

/// `FUN_0042ffe8` on the subtitle buffer `FUN_0042844c` fills: a space (the
/// byte after ']') then the text. Under 101 bytes it is one line; otherwise
/// each line ends before the last space within the next 100 bytes and the
/// following line starts at that space, until the remaining length is 0.
///
/// A window of 100 bytes with no space makes the original write a NUL every
/// 0x6e bytes past its line buffer until a write faults; no shipped text
/// has one. The port cuts such a window at 100 bytes.
pub fn split_subtitle(text: &[u8]) -> Vec<Vec<u8>> {
    let mut b = Vec::with_capacity(text.len() + 1);
    b.push(b' ');
    b.extend_from_slice(text);
    let mut lines = Vec::new();
    let mut remaining = b.len() as i64;
    if remaining < 0x65 {
        lines.push(b);
        return lines;
    }
    let mut start = 0usize;
    let mut p = 100usize;
    loop {
        if remaining < 0x65 {
            lines.push(b[start.min(b.len())..].to_vec());
            break;
        }
        // Back from p to the first space (a CR also stops it; the buffer
        // holds none).
        let mut back = 0usize;
        while back < 100 && b.get(p - back).copied() != Some(b' ') {
            back += 1;
        }
        if back == 100 {
            back = 0;
        }
        let end = (start + 100 - back).min(b.len());
        lines.push(b[start.min(end)..end].to_vec());
        remaining -= (100 - back) as i64;
        start = p - back;
        p = start + 100;
        if remaining <= 0 {
            break;
        }
    }
    lines
}

/// `FUN_00430178`: line `i` of `n` is drawn at y = 0x1ce - (n - i + 1) * 14.
pub fn subtitle_line_y(i: usize, n: usize) -> i32 {
    subtitle_row_y(i, n, SUBTITLE_ROW_STEP)
}

/// Line `i` of `n` rows `step` pixels apart: y = 0x1ce - (n - i + 1) *
/// step (`FUN_00430178`: 14; atlantis-cd-zh-tw-696832 and
/// atlantis-cd-zh-cn-692736 FUN_0042f798 @ 0x0042f7f8, 0x0042f828: 18).
pub fn subtitle_row_y(i: usize, n: usize, step: i32) -> i32 {
    0x1ce - (n as i32 - i as i32 + 1) * step
}

/// Where a subtitle face comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceSource {
    /// A font the port bundles.
    Bundled,
    /// The player's own font file, beside the game's files.
    Player,
}

#[derive(Clone)]
enum FaceData {
    Bundled(&'static [u8]),
    Player(Arc<[u8]>),
}

/// An outline font file and the face of it the subtitles are drawn with
/// (a collection, `.ttc`, holds several).
#[derive(Clone)]
pub struct FontFace {
    data: FaceData,
    index: u32,
}

impl PartialEq for FontFace {
    /// The same face of the same file (by address: the files are shared).
    fn eq(&self, other: &FontFace) -> bool {
        std::ptr::eq(self.bytes(), other.bytes()) && self.index == other.index
    }
}

impl Eq for FontFace {}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FontFace({:?}, {} bytes, face {})",
            self.source(),
            self.bytes().len(),
            self.index
        )
    }
}

/// The player's font for the subtitles of a release in `code_page`: its
/// file, beside the game's, and the family of the face to draw with. The
/// Chinese builds' request for the face "MingLi" in DEFAULT_CHARSET (see
/// [`CJK_SUBTITLE_EM_PX`]) gives on a Traditional Chinese Windows (code
/// page 950) MingLiU, of `mingliu.ttc`, on a Simplified one (936) SimSun,
/// of `simsun.ttc` (inferred). `None` for the other code pages: the port
/// draws Arial's texts with its own font.
pub fn player_font(code_page: TextCodePage) -> Option<(&'static str, &'static str)> {
    match code_page {
        TextCodePage::Windows950 => Some(("mingliu.ttc", "MingLiU")),
        TextCodePage::Windows936 => Some(("simsun.ttc", "SimSun")),
        TextCodePage::Windows932 => Some(("msgothic.ttc", "MS PGothic")),
        TextCodePage::Windows1252 | TextCodePage::Dos850 => None,
    }
}

/// The folders a system keeps its fonts in: `%WINDIR%\Fonts` on Windows,
/// the font folders of macOS and of the Linux desktops (one level of their
/// sub-folders is searched too).
#[cfg(not(target_arch = "wasm32"))]
fn system_font_dirs() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut dirs = Vec::new();
    if cfg!(windows) {
        for var in ["WINDIR", "SystemRoot"] {
            if let Some(w) = std::env::var_os(var) {
                dirs.push(PathBuf::from(w).join("Fonts"));
            }
        }
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if cfg!(target_os = "macos") {
            dirs.extend(
                [
                    "/Library/Fonts",
                    "/Library/Fonts/Microsoft",
                    "/System/Library/Fonts",
                ]
                .map(PathBuf::from),
            );
            dirs.extend(home.iter().map(|h| h.join("Library/Fonts")));
        } else {
            dirs.extend(
                home.iter()
                    .flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]),
            );
            dirs.extend(["/usr/local/share/fonts", "/usr/share/fonts"].map(PathBuf::from));
        }
    }
    dirs
}

/// The file `name` (any case) in `dirs` or one level below them, read.
#[cfg(not(target_arch = "wasm32"))]
pub fn font_in(dirs: &[std::path::PathBuf], name: &str) -> Option<Vec<u8>> {
    let find = |dir: &std::path::Path| -> Option<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
    };
    for dir in dirs {
        let mut at = find(dir);
        if at.is_none() {
            let subs = std::fs::read_dir(dir).into_iter().flatten().flatten();
            at = subs
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .find_map(|p| find(&p));
        }
        if let Some(p) = at {
            return std::fs::read(p).ok();
        }
    }
    None
}

/// The system's copy of the player's font of a release in `code_page`
/// ([`player_font`]), for the Japanese release only (whose program names
/// MS PGothic, a font of every Japanese Windows and of Microsoft Office):
/// `msgothic.ttc` among the system's fonts. The web build has none.
pub fn system_font(code_page: TextCodePage) -> Option<Vec<u8>> {
    if code_page != TextCodePage::Windows932 {
        return None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (name, _) = player_font(code_page)?;
        font_in(&system_font_dirs(), name)
    }
    #[cfg(target_arch = "wasm32")]
    None
}

/// Whether `head`, a file's first bytes, starts as a font or a font
/// collection does (`ttcf`, a TrueType or OpenType sfnt version). The web
/// page reads only these bytes of the player's font to know it need not
/// fetch the port's.
pub fn is_font_header(head: &[u8]) -> bool {
    matches!(
        head.get(..4),
        Some(b"ttcf" | b"OTTO" | b"true" | [0, 1, 0, 0])
    )
}

/// The face of `data` (a font, or a collection) whose family is `family`;
/// the first face that reads when none is; `None` when no face reads.
pub fn pick_face(data: &[u8], family: &str) -> Option<u32> {
    let n = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    let mut first = None;
    for i in 0..n {
        let Ok(face) = ttf_parser::Face::parse(data, i) else {
            continue;
        };
        first.get_or_insert(i);
        let named = face.names().into_iter().any(|n| {
            n.name_id == ttf_parser::name_id::FAMILY
                && n.to_string()
                    .is_some_and(|s| s.eq_ignore_ascii_case(family))
        });
        if named {
            return Some(i);
        }
    }
    first
}

/// Whether a face's outlines draw right only through its hinting programs,
/// which `ab_glyph` does not run: FreeType lists MingLiU and PMingLiU as
/// such ("tricky") up to their version 7.00 (Windows 7's), which is not.
fn needs_hinting(family: &str, revision: f32) -> bool {
    ["MingLiU", "PMingLiU"]
        .iter()
        .any(|f| f.eq_ignore_ascii_case(family))
        && revision < 7.0
}

impl FontFace {
    /// The port's font for texts in `code_page`: Liberation Sans, or for
    /// Big5 Noto Serif TC, for GBK Noto Serif SC (in the web build, the
    /// one its page fetched; Liberation Sans if it could not).
    pub fn bundled(code_page: TextCodePage) -> FontFace {
        let data = bundled_cjk(code_page).unwrap_or_else(|| {
            if code_page.is_double_byte() {
                log::warn!("no CJK subtitle font: the subtitles draw no CJK character");
            }
            FONT_DATA
        });
        FontFace {
            data: FaceData::Bundled(data),
            index: 0,
        }
    }

    /// The player's font file `data` for texts in `code_page` (see
    /// [`player_font`]): its face of that family (or its first), `None`
    /// when no face reads or the face needs its hinting to draw right.
    pub fn player(code_page: TextCodePage, data: Vec<u8>) -> Option<FontFace> {
        let (_, family) = player_font(code_page)?;
        let index = pick_face(&data, family)?;
        let face = ttf_parser::Face::parse(&data, index).ok()?;
        let name = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .find_map(|n| n.to_string())
            .unwrap_or_default();
        // head: the font revision, a 16.16 number at byte 4.
        let revision = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"head"))
            .and_then(|h| h.get(4..8))
            .map_or(0.0, |r| {
                i32::from_be_bytes([r[0], r[1], r[2], r[3]]) as f32 / 65536.0
            });
        if needs_hinting(&name, revision) {
            log::warn!("{name} {revision:.2}: its glyphs need its hinting; not used");
            return None;
        }
        FontRef::try_from_slice_and_index(&data, index).ok()?;
        Some(FontFace {
            data: FaceData::Player(data.into()),
            index,
        })
    }

    /// The face of a release in `code_page`'s subtitles: the player's
    /// `file` when there is one and it reads ([`FontFace::player`]), else
    /// the port's ([`FontFace::bundled`]).
    pub fn for_release(code_page: TextCodePage, file: Option<Vec<u8>>) -> FontFace {
        let name = player_font(code_page).map_or("", |(n, _)| n);
        match file.map(|d| FontFace::player(code_page, d)) {
            Some(Some(face)) => {
                log::info!("subtitles: the player's {name} (face {})", face.index);
                return face;
            }
            Some(None) => log::warn!("{name}: not a font the port can use"),
            None if !name.is_empty() => log::info!("no {name} beside the game's files"),
            None => {}
        }
        FontFace::bundled(code_page)
    }

    /// The bytes of the file.
    fn bytes(&self) -> &[u8] {
        match &self.data {
            FaceData::Bundled(d) => d,
            FaceData::Player(d) => d,
        }
    }

    /// Where the face comes from.
    pub fn source(&self) -> FaceSource {
        match self.data {
            FaceData::Bundled(_) => FaceSource::Bundled,
            FaceData::Player(_) => FaceSource::Player,
        }
    }

    /// The face, for `ab_glyph` (checked when the face was made).
    fn font(&self) -> FontRef<'_> {
        FontRef::try_from_slice_and_index(self.bytes(), self.index).expect("subtitle font")
    }

    /// Whether the face has a glyph for `c`.
    pub fn has_char(&self, c: char) -> bool {
        self.font().glyph_id(c).0 != 0
    }

    /// The ascent of a double-byte code page's line at `em_px`, in whole
    /// pixels: GDI's (the Windows ascent, rounded, as Wine computes it) for
    /// the player's face, whose cell is its em; for the port's, the top of
    /// its ideographs' em box (the typographic ascent, 0.88 em), where
    /// MingLiU's and SimSun's are (14 pixels).
    fn cjk_ascent(&self, em_px: f32) -> Option<i32> {
        let face = ttf_parser::Face::parse(self.bytes(), self.index).ok()?;
        let os2 = face.tables().os2?;
        let units = match self.source() {
            FaceSource::Player => os2.windows_ascender(),
            FaceSource::Bundled => os2.typographic_ascender(),
        };
        Some((f32::from(units) * em_px / f32::from(face.units_per_em())).round() as i32)
    }
}

/// One rasterised glyph: its coverage thresholded to on / off.
#[derive(Clone, Debug, Default)]
struct GlyphBits {
    /// The pen advance, rounded to whole pixels.
    advance: i32,
    /// The bitmap's offset from the pen (x) and the baseline (y).
    left: i32,
    top: i32,
    w: i32,
    h: i32,
    bits: Vec<bool>,
}

impl GlyphBits {
    /// The glyph of `c` at `scale`, none for a control character or a
    /// character the font lacks: the pixels at least half covered; with
    /// `scan`, the pixels a TrueType rasteriser's scan conversion turns on
    /// ([`GlyphBits::scan_converted`]).
    fn of(font: &FontRef<'_>, scale: PxScale, c: char, scan: bool) -> GlyphBits {
        if c < ' ' {
            return GlyphBits::default();
        }
        let id = font.glyph_id(c);
        if id.0 == 0 {
            return GlyphBits::default();
        }
        let advance = font.as_scaled(scale).h_advance(id).round() as i32;
        if scan {
            return GlyphBits {
                advance,
                ..Self::scan_converted(font, scale, id)
            };
        }
        let glyph = id.with_scale_and_position(scale, point(0.0, 0.0));
        let Some(outline) = font.outline_glyph(glyph) else {
            return GlyphBits {
                advance,
                ..GlyphBits::default()
            };
        };
        let bounds = outline.px_bounds();
        let (w, h) = (bounds.width() as i32, bounds.height() as i32);
        let mut cover = vec![0f32; (w * h).max(0) as usize];
        outline.draw(|x, y, c| {
            if let Some(p) = cover.get_mut(y as usize * w as usize + x as usize) {
                *p = c;
            }
        });
        let bits = cover.iter().map(|&c| c >= 0.5).collect();
        GlyphBits {
            advance,
            left: bounds.min.x as i32,
            top: bounds.min.y as i32,
            w,
            h,
            bits,
        }
    }

    /// The glyph `id` at `scale` as the TrueType scan converter turns its
    /// outline into pixels in monochrome (the rules of the OpenType
    /// specification's "TrueType instructions", scan conversion and
    /// dropout control, as GDI's rasteriser applies them; no hinting):
    ///
    /// 1. a pixel whose centre is inside the outline (non-zero winding) is
    ///    on;
    /// 2. a pixel whose centre is on the outline is on;
    /// 3. where a scan line through the pixel centres (a row or a column)
    ///    enters and leaves the outline between two adjacent centres and
    ///    neither pixel is on by 1 and 2, the one nearer the midpoint of
    ///    the two crossings is on (dropout control, which keeps the thin
    ///    horizontals of a Ming / Song face at 16 pixels).
    ///
    /// The curves are followed as 16 straight pieces each.
    fn scan_converted(font: &FontRef<'_>, scale: PxScale, id: ab_glyph::GlyphId) -> GlyphBits {
        let Some(outline) = font.outline(id) else {
            return GlyphBits::default();
        };
        let scaled = font.as_scaled(scale);
        let (hs, vs) = (scaled.h_scale_factor(), scaled.v_scale_factor());
        // Pixels, y down from the baseline.
        let at = |p: ab_glyph::Point| (p.x * hs, -p.y * vs);
        let mut lines: Vec<((f32, f32), (f32, f32))> = Vec::new();
        const PIECES: usize = 16;
        for curve in &outline.curves {
            let mut from = None;
            for i in 0..=PIECES {
                let t = i as f32 / PIECES as f32;
                let u = 1.0 - t;
                let p = match *curve {
                    ab_glyph::OutlineCurve::Line(a, b) => {
                        if i > 0 && i < PIECES {
                            continue;
                        }
                        at(point(a.x * u + b.x * t, a.y * u + b.y * t))
                    }
                    ab_glyph::OutlineCurve::Quad(a, b, c) => at(point(
                        u * u * a.x + 2.0 * u * t * b.x + t * t * c.x,
                        u * u * a.y + 2.0 * u * t * b.y + t * t * c.y,
                    )),
                    ab_glyph::OutlineCurve::Cubic(a, b, c, d) => at(point(
                        u * u * u * a.x
                            + 3.0 * u * u * t * b.x
                            + 3.0 * u * t * t * c.x
                            + t * t * t * d.x,
                        u * u * u * a.y
                            + 3.0 * u * u * t * b.y
                            + 3.0 * u * t * t * c.y
                            + t * t * t * d.y,
                    )),
                };
                if let Some(f) = from {
                    lines.push((f, p));
                }
                from = Some(p);
            }
        }
        if lines.is_empty() {
            return GlyphBits::default();
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &((ax, ay), (bx, by)) in &lines {
            x0 = x0.min(ax).min(bx);
            y0 = y0.min(ay).min(by);
            x1 = x1.max(ax).max(bx);
            y1 = y1.max(ay).max(by);
        }
        let (left, top) = (x0.floor() as i32, y0.floor() as i32);
        let (w, h) = (x1.ceil() as i32 - left + 1, y1.ceil() as i32 - top + 1);
        // The spans of the scan line at `c` (a pixel centre) along the
        // lines' `u` coordinate (x for a row, y for a column): the
        // crossings with their winding, paired where the winding leaves 0
        // and comes back to it.
        let spans = |c: f32, horizontal: bool| -> Vec<(f32, f32)> {
            let mut cross: Vec<(f32, i32)> = Vec::new();
            for &(a, b) in &lines {
                let (a, b) = if horizontal {
                    (a, b)
                } else {
                    ((a.1, a.0), (b.1, b.0))
                };
                // (u, v): v the scan direction's normal
                let (au, av, bu, bv) = (a.0, a.1, b.0, b.1);
                if av == bv {
                    continue;
                }
                let (dir, lo, hi) = if av < bv { (1, av, bv) } else { (-1, bv, av) };
                if c < lo || c >= hi {
                    continue;
                }
                cross.push((au + (c - av) * (bu - au) / (bv - av), dir));
            }
            cross.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut out = Vec::new();
            let mut wind = 0;
            let mut start = 0.0;
            for (u, d) in cross {
                if wind == 0 {
                    start = u;
                }
                wind += d;
                if wind == 0 {
                    out.push((start, u));
                }
            }
            out
        };
        let mut bits = vec![false; (w * h) as usize];
        let idx = |x: i32, y: i32| (y * w + x) as usize;
        let mut row_spans = Vec::with_capacity(h as usize);
        let mut col_spans = Vec::with_capacity(w as usize);
        // Rules 1 and 2 along the rows.
        for y in 0..h {
            let spans = spans((top + y) as f32 + 0.5, true);
            for &(a, b) in &spans {
                let first = (a - 0.5 - left as f32).ceil() as i32;
                let last = (b - 0.5 - left as f32).floor() as i32;
                for x in first.max(0)..=last.min(w - 1) {
                    bits[idx(x, y)] = true;
                }
            }
            row_spans.push(spans);
        }
        for x in 0..w {
            col_spans.push(spans((left + x) as f32 + 0.5, false));
        }
        // Rule 3, against the pixels of rules 1 and 2.
        let base = bits.clone();
        let on = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && base[idx(x, y)];
        for (y, spans) in row_spans.iter().enumerate() {
            let y = y as i32;
            for &(a, b) in spans {
                let lo = (a - 0.5 - left as f32).floor() as i32;
                if (b - 0.5 - left as f32).floor() as i32 != lo || on(lo, y) || on(lo + 1, y) {
                    continue;
                }
                let x = (((a + b) / 2.0).floor() as i32 - left).clamp(0, w - 1);
                bits[idx(x, y)] = true;
            }
        }
        for (x, spans) in col_spans.iter().enumerate() {
            let x = x as i32;
            for &(a, b) in spans {
                let lo = (a - 0.5 - top as f32).floor() as i32;
                if (b - 0.5 - top as f32).floor() as i32 != lo || on(x, lo) || on(x, lo + 1) {
                    continue;
                }
                let y = (((a + b) / 2.0).floor() as i32 - top).clamp(0, h - 1);
                bits[idx(x, y)] = true;
            }
        }
        GlyphBits {
            advance: 0,
            left,
            top,
            w,
            h,
            bits,
        }
    }
}

/// The subtitle font: rasterised once for the 256 byte values, or, in a
/// double-byte code page, for each character the first time it is drawn.
pub struct SubtitleFont {
    glyphs: Vec<GlyphBits>,
    /// A double-byte code page's glyphs, by character.
    cjk_glyphs: Mutex<HashMap<char, GlyphBits>>,
    /// How the text bytes read as letters (the original's GDI draws them
    /// in the ANSI code page, Windows-1252; Big5 or GBK for the Chinese
    /// builds).
    code_page: TextCodePage,
    /// The font file and face.
    face: FontFace,
    /// The ascent in whole pixels (rounded up), from the line's top to the
    /// baseline.
    ascent: i32,
    /// The pixels per em ([`SUBTITLE_EM_PX`] but for the port's own
    /// screens' titles, and [`CJK_SUBTITLE_EM_PX`]).
    em_px: f32,
    /// The rows' step of [`SubtitleFont::draw_subtitles`].
    row_step: i32,
    /// The width a row of [`SubtitleFont::draw_subtitles`] may take
    /// ([`SUBTITLE_WIDTH`], or none: [`SubtitleFont::with_rows_to_the_edge`]).
    max_w: i32,
}

impl Default for SubtitleFont {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtitleFont {
    /// The font for Windows-1252 texts.
    pub fn new() -> SubtitleFont {
        Self::with_code_page(TextCodePage::Windows1252)
    }

    /// The font for the texts of a release in `code_page`: the DOS code
    /// page texts of atlantis-cd-fr-688640 drawn with it (the build itself
    /// draws them in its bitmap font, glyph = byte) read as code page 850;
    /// the Big5 and GBK texts of the Chinese builds with the port's CJK
    /// font.
    pub fn with_code_page(code_page: TextCodePage) -> SubtitleFont {
        Self::with_face(code_page, FontFace::bundled(code_page))
    }

    /// The font for texts in `code_page` drawn with `face` (at
    /// [`CJK_SUBTITLE_EM_PX`] in a double-byte code page, else at
    /// [`SUBTITLE_EM_PX`]).
    pub fn with_face(code_page: TextCodePage, face: FontFace) -> SubtitleFont {
        let em_px = if code_page.is_double_byte() {
            CJK_SUBTITLE_EM_PX
        } else {
            SUBTITLE_EM_PX
        };
        Self::build(code_page, em_px, face)
    }

    /// The font at `em_px` pixels per em, for Windows-1252 texts (the
    /// titles of the port's own screens).
    pub fn with_size(em_px: f32) -> SubtitleFont {
        Self::build(
            TextCodePage::Windows1252,
            em_px,
            FontFace::bundled(TextCodePage::Windows1252),
        )
    }

    /// The font with its rows `step` pixels apart (see
    /// [`subtitle_row_y`]).
    pub fn with_row_step(mut self, step: u8) -> SubtitleFont {
        self.row_step = i32::from(step);
        self
    }

    /// The rows of [`SubtitleFont::draw_subtitles`] drawn whole up to the
    /// screen's edge, where the pixels are clipped (GDI's TextOutA on the
    /// screen surface: the Japanese DVD build's rows, whose longest
    /// Japanese ones and English ones in MS PGothic run past the screen),
    /// instead of ending before the first glyph past [`SUBTITLE_WIDTH`].
    pub fn with_rows_to_the_edge(mut self) -> SubtitleFont {
        self.max_w = i32::MAX / 4;
        self
    }

    /// The scale of `font` at `em_px` pixels per em: ab_glyph scales by
    /// the line height (ascent - descent).
    fn scale(font: &FontRef<'_>, em_px: f32) -> PxScale {
        let upem = font.units_per_em().unwrap_or(2048.0);
        PxScale::from(em_px * font.height_unscaled() / upem)
    }

    fn build(code_page: TextCodePage, em_px: f32, face: FontFace) -> SubtitleFont {
        let font = face.font();
        let scale = Self::scale(&font, em_px);
        let mut ascent = font.as_scaled(scale).ascent().ceil() as i32;
        let mut glyphs = Vec::new();
        if code_page.is_double_byte() {
            ascent = face.cjk_ascent(em_px).unwrap_or(ascent);
        } else {
            glyphs = (0..=255u8)
                .map(|b| GlyphBits::of(&font, scale, code_page.char_of(b), false))
                .collect();
        }
        SubtitleFont {
            glyphs,
            cjk_glyphs: Mutex::new(HashMap::new()),
            code_page,
            face,
            ascent,
            em_px,
            row_step: SUBTITLE_ROW_STEP,
            max_w: SUBTITLE_WIDTH,
        }
    }

    /// The code page the font reads the bytes in.
    pub fn code_page(&self) -> TextCodePage {
        self.code_page
    }

    /// The font file and face it draws with.
    pub fn face(&self) -> &FontFace {
        &self.face
    }

    /// The rows' step of [`SubtitleFont::draw_subtitles`].
    pub fn row_step(&self) -> i32 {
        self.row_step
    }

    /// `f` on each glyph of `text` with its character, in order: one per
    /// byte; in a double-byte code page one per character, a lead byte and
    /// the byte after it making one (as GDI reads the bytes in the ANSI
    /// code page; a byte or pair with no character draws nothing).
    ///
    /// In a double-byte code page a character of a pair the face has no
    /// glyph for (the code page's user-defined area, private use, which a
    /// Simplified row read one byte out of step can hold) draws nothing
    /// but keeps its cell, one em: MingLiU and SimSun give every two-byte
    /// character the same full-width advance, their em, so the characters
    /// after it stay where the original draws them (inferred; what those
    /// faces draw for such a character is [UNKNOWN]).
    fn each_glyph(&self, text: &[u8], mut f: impl FnMut(char, &GlyphBits) -> bool) {
        if self.code_page.is_double_byte() {
            let font = self.face.font();
            let scale = Self::scale(&font, self.em_px);
            let mut cache = self.cjk_glyphs.lock().unwrap_or_else(|e| e.into_inner());
            for c in self.code_page.decode(text).chars() {
                let g = cache.entry(c).or_insert_with(|| {
                    let mut g = GlyphBits::of(&font, scale, c, true);
                    if !c.is_ascii() && font.glyph_id(c).0 == 0 {
                        g.advance = self.em_px.round() as i32;
                    }
                    g
                });
                if !f(c, g) {
                    break;
                }
            }
        } else {
            for &b in text {
                if !f(self.code_page.char_of(b), &self.glyphs[usize::from(b)]) {
                    break;
                }
            }
        }
    }

    /// The width of `text` in pixels.
    pub fn width(&self, text: &[u8]) -> i32 {
        let mut w = 0;
        self.each_glyph(text, |_, g| {
            w += g.advance;
            true
        });
        w
    }

    /// Draws `text` with the top of its line at (x, y) on an RGB565 buffer
    /// of `width` x `height`, as ScummVM's `Font::drawString` with a width
    /// limit of `max_w`: a glyph that would end past x + max_w + 1 stops
    /// the line.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        text: &[u8],
        x: i32,
        y: i32,
        max_w: i32,
        color: u16,
    ) {
        let right = x + max_w + 1;
        let mut pen = x;
        let base = y + self.ascent;
        self.each_glyph(text, |_, g| {
            if pen + g.left + g.w > right {
                return false;
            }
            for gy in 0..g.h {
                let dy = base + g.top + gy;
                if dy < 0 || dy as usize >= height {
                    continue;
                }
                for gx in 0..g.w {
                    if !g.bits[(gy * g.w + gx) as usize] {
                        continue;
                    }
                    let dx = pen + g.left + gx;
                    if dx < 0 || dx as usize >= width {
                        continue;
                    }
                    dst[dy as usize * width + dx as usize] = color;
                }
            }
            pen += g.advance;
            true
        });
    }

    /// The glyphs [`SubtitleFont::draw`] draws for `text` at x = `x`
    /// with the width limit `max_w`: each character with its pen position.
    fn layout(&self, text: &[u8], x: i32, max_w: i32) -> Vec<(char, i32)> {
        let right = x + max_w + 1;
        let mut pen = x;
        let mut out = Vec::with_capacity(text.len());
        self.each_glyph(text, |c, g| {
            if pen + g.left + g.w > right {
                return false;
            }
            out.push((c, pen));
            pen += g.advance;
            true
        });
        out
    }

    /// [`SubtitleFont::draw`] at a larger scale, for the HD mode: the same
    /// glyphs at the same places of the 640x480 screen, the screen mapped
    /// to the `dw` x `dh` pixels at (`ox`, `oy`) of `out` (XRGB8888,
    /// `stride` pixels per row), rasterised from the outlines at that
    /// size and blended by their coverage (smoothed), in `color`
    /// (XRGB8888). Nothing is drawn outside those pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_hd(
        &self,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        text: &[u8],
        x: i32,
        y: i32,
        max_w: i32,
        color: u32,
    ) {
        let font = self.face.font();
        let (sx, sy) = (dw as f32 / 640.0, dh as f32 / 480.0);
        let scale = Self::scale(&font, self.em_px * sy);
        let base = oy as f32 + (y + self.ascent) as f32 * sy;
        let (x1, y1) = ((ox + dw) as i32, (oy + dh) as i32);
        for (c, pen) in self.layout(text, x, max_w) {
            let id = font.glyph_id(c);
            if c < ' ' || id.0 == 0 {
                continue;
            }
            let pos = point(ox as f32 + pen as f32 * sx, base);
            let Some(outline) = font.outline_glyph(id.with_scale_and_position(scale, pos)) else {
                continue;
            };
            let bounds = outline.px_bounds();
            let (bx, by) = (bounds.min.x as i32, bounds.min.y as i32);
            outline.draw(|gx, gy, cov| {
                let (px, py) = (bx + gx as i32, by + gy as i32);
                if px < ox as i32 || py < oy as i32 || px >= x1 || py >= y1 {
                    return;
                }
                let a = (cov.clamp(0.0, 1.0) * 256.0) as u32;
                if a == 0 {
                    return;
                }
                let d = &mut out[py as usize * stride + px as usize];
                // (the alpha too: a premultiplied overlay, see
                // `HdFrame::on_card`)
                *d = crate::upscale::lerp8888(*d, color, a.min(256));
            });
        }
    }

    /// The subtitle lines over a 640x480 RGB565 picture (`FUN_00430178`,
    /// the Chinese builds' FUN_0042f798): line i of n at (5,
    /// [`subtitle_row_y`]) in white over its black shadow at (+1, +1).
    /// `y_offset` moves them down (a picture taller than the screen).
    pub fn draw_subtitles(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        lines: &[Vec<u8>],
        y_offset: i32,
    ) {
        for (i, line) in lines.iter().enumerate() {
            let y = subtitle_row_y(i, lines.len(), self.row_step) + y_offset;
            self.draw(
                dst,
                width,
                height,
                line,
                SUBTITLE_X + 1,
                y + 1,
                self.max_w,
                SUBTITLE_SHADOW,
            );
            self.draw(
                dst,
                width,
                height,
                line,
                SUBTITLE_X,
                y,
                self.max_w,
                SUBTITLE_COLOR,
            );
        }
    }

    /// The text items of [`SubtitleFont::draw_subtitles`] for the HD mode
    /// (see [`crate::hd::HdText`]), drawn with this font's face.
    pub fn items(&self, lines: &[Vec<u8>], dx: i32, dy: i32) -> Vec<crate::hd::TextItem> {
        gdi_items(
            lines,
            dx,
            dy,
            self.code_page,
            Some(self.face.clone()),
            self.row_step,
            self.max_w,
        )
    }
}

/// The 1997 build's subtitle font: `DIALOG\FONTS.BIN` of the archives,
/// 256 glyphs of 8 x 8 bytes, glyph = the raw byte, a non-zero byte a
/// pixel (atlantis-cd-en-688128 loads it at start-up, 0x004191c2..
/// 0x004191f9, and draws with FUN_00410800). The shapes are the IBM PC
/// set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapFont {
    glyphs: Vec<u8>,
}

/// The size of `DIALOG\FONTS.BIN`.
pub const BITMAP_FONT_SIZE: usize = 256 * 64;

/// The row height and the x of the 1997 layout (atlantis-cd-en-688128
/// FUN_0042eb3c).
pub const BITMAP_ROW_H: i32 = 10;

/// FUN_0042e970's window: a text of fewer than 0x4c bytes (its leading
/// space included) is one row; else rows of a 75-byte window.
const BITMAP_WINDOW: usize = 0x4b;

impl BitmapFont {
    /// The font from the file's bytes, `None` for another size.
    pub fn new(data: &[u8]) -> Option<BitmapFont> {
        (data.len() == BITMAP_FONT_SIZE).then(|| BitmapFont {
            glyphs: data.to_vec(),
        })
    }

    /// Whether pixel (`col`, `row`) of glyph `b` is set.
    fn on(&self, b: u8, col: usize, row: usize) -> bool {
        self.glyphs[usize::from(b) * 64 + row * 8 + col] != 0
    }

    /// FUN_00410800: `text` at (x, y) on a buffer of `width` pixels per
    /// row, 8 pixels per byte, the set pixels in `color`. There is no
    /// clipping: the pixels are addressed as y * width + x, so a glyph
    /// past the right edge goes on at the left of the next pixel row (the
    /// port drops what falls outside the buffer).
    pub fn draw(&self, dst: &mut [u16], width: usize, text: &[u8], x: i32, y: i32, color: u16) {
        self.for_each_pixel(width, text, x, y, |i| {
            if let Some(p) = usize::try_from(i).ok().and_then(|i| dst.get_mut(i)) {
                *p = color;
            }
        });
    }

    /// The linear index (y * width + x) of each pixel FUN_00410800 sets.
    fn for_each_pixel(&self, width: usize, text: &[u8], x: i32, y: i32, mut f: impl FnMut(i64)) {
        let w = width as i64;
        let mut base = i64::from(y) * w + i64::from(x);
        for &b in text {
            for row in 0..8 {
                for col in 0..8 {
                    if self.on(b, col, row) {
                        f(base + row as i64 * w + col as i64);
                    }
                }
            }
            base += 8;
        }
    }

    /// The subtitle rows over a picture of `width` x `height`
    /// (atlantis-cd-en-688128 FUN_0042eb3c): row i of n, black at (6,
    /// y + 1) then white at (5, y), y = 0x1ce - (n - i + 1) * 10.
    /// `y_offset` moves them down (a picture taller than the screen).
    pub fn draw_subtitles(&self, dst: &mut [u16], width: usize, lines: &[Vec<u8>], y_offset: i32) {
        for (i, line) in lines.iter().enumerate() {
            let y = bitmap_line_y(i, lines.len()) + y_offset;
            self.draw(dst, width, line, SUBTITLE_X + 1, y + 1, SUBTITLE_SHADOW);
            self.draw(dst, width, line, SUBTITLE_X, y, SUBTITLE_COLOR);
        }
    }

    /// [`BitmapFont::draw`] for the HD mode: each font pixel of the
    /// 640x480 screen a sharp block of the `dw` x `dh` pixels at (`ox`,
    /// `oy`) of `out` (XRGB8888, `stride` pixels per row), in `color`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_hd(
        &self,
        out: &mut [u32],
        stride: usize,
        (ox, oy, dw, dh): (usize, usize, usize, usize),
        text: &[u8],
        x: i32,
        y: i32,
        color: u32,
    ) {
        let (sw, sh) = (crate::warp::SCREEN_W, crate::warp::SCREEN_H);
        self.for_each_pixel(sw, text, x, y, |i| {
            let Ok(i) = usize::try_from(i) else { return };
            let (px, py) = (i % sw, i / sw);
            if py >= sh {
                return;
            }
            let (x0, x1) = (ox + px * dw / sw, ox + (px + 1) * dw / sw);
            let (y0, y1) = (oy + py * dh / sh, oy + (py + 1) * dh / sh);
            for row in y0..y1 {
                if let Some(r) = out.get_mut(row * stride + x0..row * stride + x1) {
                    r.fill(color);
                }
            }
        });
    }
}

/// atlantis-cd-en-688128 FUN_0042eb3c: row `i` of `n` at y = 0x1ce -
/// (n - i + 1) * 10.
pub fn bitmap_line_y(i: usize, n: usize) -> i32 {
    0x1ce - (n as i32 - i as i32 + 1) * BITMAP_ROW_H
}

/// The subtitle text buffer of the bitmap-subtitle builds of 1997
/// ([`ExeBuild::bitmap_subtitles`]): atlantis-cd-en-688128 0x00603d68,
/// atlantis-cd-fr-688640 0x00603ac0, atlantis-cd-nl-688128 and
/// atlantis-cd-sv-688128 0x00603f68, 0x400 bytes in each (the object at
/// 0x00604168 / 0x00603ec0 / 0x00604368 follows it). It is zeroed once, at
/// start-up (`FUN_00422bd0` @ 0x00422c58, called from the WinMain start-up
/// code @ 0x00419499), and written only by the section executor for every
/// `[` line it runs (`FUN_00427420` @ 0x00427707..0x00427722) and by the
/// game-over screen for its narrator line (`FUN_004256a4` @
/// 0x00425a31..0x00425a50): the bytes up to the line's CR, then a NUL, the
/// rest left as it was. So after the NUL it holds zeros, or the tail of an
/// earlier, longer text, which the row breaker reads.
///
/// [`ExeBuild::bitmap_subtitles`]: atlantis_data::exe::ExeBuild::bitmap_subtitles
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleTextBuffer {
    buf: [u8; SubtitleTextBuffer::SIZE],
}

impl Default for SubtitleTextBuffer {
    fn default() -> Self {
        SubtitleTextBuffer {
            buf: [0; SubtitleTextBuffer::SIZE],
        }
    }
}

impl SubtitleTextBuffer {
    pub const SIZE: usize = 0x400;

    /// `FUN_00422bd0`'s clear.
    pub fn clear(&mut self) {
        self.buf.fill(0);
    }

    /// A copy loop: `bytes`, then a NUL. A text of 0x400 bytes or more
    /// would run on into the object after the buffer; none is shipped (the
    /// longest dialog line has 435 bytes), the port cuts it.
    pub fn store(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(Self::SIZE - 1);
        self.buf[..n].copy_from_slice(&bytes[..n]);
        self.buf[n] = 0;
    }

    /// The bytes up to the NUL.
    pub fn text(&self) -> &[u8] {
        let n = self.buf.iter().position(|&c| c == 0).unwrap_or(Self::SIZE);
        &self.buf[..n]
    }

    /// The byte at `i`, 0 past the buffer.
    pub fn at(&self, i: usize) -> u8 {
        self.buf.get(i).copied().unwrap_or(0)
    }

    /// The buffer as the first line after start-up leaves it: `text`, a
    /// NUL, zeros.
    pub fn fresh(text: &[u8]) -> SubtitleTextBuffer {
        let mut b = SubtitleTextBuffer::default();
        b.store(text);
        b
    }
}

/// atlantis-cd-en-688128 FUN_0042e970 on a fresh subtitle text buffer, a
/// space (the byte after ']') then the text ([`split_bitmap_buffer`]).
pub fn split_subtitle_1997(text: &[u8]) -> Vec<Vec<u8>> {
    split_bitmap_buffer(&fresh_buffer(text), BitmapBreaker::Nearest)
}

/// atlantis-cd-sv-688128 FUN_0042ea7c on a fresh subtitle text buffer
/// ([`split_bitmap_buffer`]): [`split_subtitle_1997`]'s window and scans,
/// but every row is cut at the space found backwards (the length left less
/// 75 - b per row; `0x0042eb50`, the forward choice removed), so no row is
/// over 75 bytes.
pub fn split_subtitle_1997_back(text: &[u8]) -> Vec<Vec<u8>> {
    split_bitmap_buffer(&fresh_buffer(text), BitmapBreaker::Back)
}

fn fresh_buffer(text: &[u8]) -> SubtitleTextBuffer {
    SubtitleTextBuffer::fresh(&[b" ", text].concat())
}

/// How a bitmap-subtitle row is cut (see [`ExeBuild::subtitle_cut_back_only`]).
///
/// [`ExeBuild::subtitle_cut_back_only`]: atlantis_data::exe::ExeBuild::subtitle_cut_back_only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapBreaker {
    /// The nearer space, forward or back (atlantis-cd-en-688128 FUN_0042e970).
    Nearest,
    /// The space back from the window end (atlantis-cd-sv-688128 FUN_0042ea7c).
    Back,
}

/// The rows of the subtitle text buffer: atlantis-cd-en-688128
/// `FUN_0042e970` (atlantis-cd-fr-688640 `FUN_0042eab4`, atlantis-cd-nl-688128
/// `FUN_0042e990`, the same code), or atlantis-cd-sv-688128 `FUN_0042ea7c`
/// with [`BitmapBreaker::Back`].
///
/// The text is the buffer up to its NUL (strlen @ 0x0042e9ce): up to 75
/// bytes, one row; otherwise, from the end p of a 75-byte window from the
/// row's start, it scans forward (f) and back (b) for a space (a CR stops a
/// scan one byte further) and breaks at the nearer one, the forward one
/// when f < b: the row is the 75 + f or 75 - b bytes up to the space, the
/// space starts the next row; the last row (75 bytes or fewer left) is the
/// rest. The scans read the buffer itself, the bytes after the NUL
/// included: when the text's last word crosses the window end the forward
/// scan runs into the zeros of the start-up clear (then b wins) or into
/// the tail of an earlier, longer text, whose spaces can make the forward
/// cut win, so the rows depend on what was shown before, as in the
/// original. A forward scan that leaves the 0x400-byte buffer reads the
/// object after it; its count is then at least 0x400 - p, more than 75
/// (so more than any b) for every text shorter than 0x3b5 bytes, the
/// backward cut whatever that memory holds: the port stops the scan there
/// and counts it as not found (no text of 0x3b5 bytes or more is shipped).
///
/// A 75-byte window with no space after the row's first byte makes the
/// original loop: [`BitmapBreaker::Nearest`] copies 0 bytes per pass and
/// NULs a row buffer each time, until it NULs the text's first byte and
/// the back scan runs below the buffer (emulated: 10 empty rows), and
/// [`BitmapBreaker::Back`] writes rows past its 8 x 0x55-byte buffer until
/// it faults (under emulation; what a real machine does is [UNKNOWN] in
/// both). No shipped text has such a window (the longest space-free run of
/// the dialog lines is 16 bytes). The port cuts the row at the forward
/// space (75 + f, or the rest of the text when there is none), or at 75
/// bytes for [`BitmapBreaker::Back`], as the ScummVM port does.
///
/// The rows are stored 0x55 bytes apart with no bound on their length: a
/// row of 85 bytes or more would run into the next row's buffer; the
/// shipped rows are at most 81 bytes long, not reproduced.
pub fn split_bitmap_buffer(buf: &SubtitleTextBuffer, breaker: BitmapBreaker) -> Vec<Vec<u8>> {
    const W: i64 = BITMAP_WINDOW as i64;
    const NOT_FOUND: i64 = 0x10000;
    let b = buf.text();
    let len = b.len() as i64;
    // The byte at i, a space before the buffer.
    let at = |i: i64| {
        if i < 0 {
            b' '
        } else {
            buf.at(i as usize)
        }
    };
    let row = |from: i64, n: i64| -> Vec<u8> {
        let from = from.clamp(0, len) as usize;
        let to = (from as i64 + n.max(0)).min(len) as usize;
        b[from..to].to_vec()
    };
    // 0x0042e9dd: up to 75 bytes, the whole text (strcpy @ 0x0042eb1f).
    if len <= W {
        return vec![b.to_vec()];
    }
    let mut lines = Vec::new();
    let (mut remaining, mut start, mut p) = (len, 0i64, W);
    while remaining > 0 {
        // 0x0042e9fc: 75 bytes or fewer left, the last row (0x0042eadb).
        if remaining <= W {
            lines.push(row(start, len));
            break;
        }
        // 0x0042ea06..0x0042ea21: forward from p to a space; a CR ends the
        // scan one byte further.
        let mut f = 0i64;
        while at(p + f) != b' ' {
            let c = at(p + f);
            f += 1;
            if c == b'\r' {
                break;
            }
            if p + f >= SubtitleTextBuffer::SIZE as i64 {
                f = NOT_FOUND;
                break;
            }
        }
        // 0x0042ea23..0x0042ea42: back from p, the same way.
        let mut k = 0i64;
        while at(p - k) != b' ' {
            let c = at(p - k);
            k += 1;
            if c == b'\r' {
                break;
            }
        }
        let n;
        if breaker == BitmapBreaker::Back {
            // atlantis-cd-sv-688128 FUN_0042ea7c @ 0x0042eb50: always the
            // backward cut, 75 - b bytes; the next row starts at the space.
            if W - k <= 0 {
                // no space in the window: cut at its end (see above)
                n = W;
                lines.push(row(start, n));
                start = p;
            } else {
                n = W - k;
                lines.push(row(start, n));
                start = p - k;
            }
            p = start + W;
        } else if f < k || W - k <= 0 {
            // 0x0042ea4c: 75 + f bytes (strncpy stops at the NUL); the
            // second test is the port's (see above).
            n = W + f;
            lines.push(row(start, n));
            start = p + f;
            p += n;
        } else {
            // 0x0042ea90: 75 - b bytes.
            n = W - k;
            lines.push(row(start, n));
            start = p - k;
            p = start + W;
        }
        remaining -= n;
    }
    lines
}

/// The subtitles of a build: the reference's GDI Arial layout (with the
/// Chinese builds' breaker), or the 1997 build's bitmap font and layout.
#[derive(Clone)]
pub enum Subtitler {
    Gdi(std::rc::Rc<SubtitleFont>, Option<CjkSplit>),
    Bitmap(std::sync::Arc<BitmapFont>, BitmapBreaker),
}

impl Subtitler {
    /// The subtitles with the Chinese rows' fix on or off (the Simplified
    /// Chinese breaker's skip after an ideographic space and its marks at
    /// character boundaries, see [`CjkSplit`]).
    pub fn with_row_fix(mut self, on: bool) -> Subtitler {
        if let Subtitler::Gdi(_, Some(cjk)) = &mut self {
            cjk.row_fix = on;
        }
        self
    }

    /// The rows of a dialog line's or a game-over narrator's subtitle: the
    /// 1997 bitmap builds break the subtitle text buffer `buffer`, which the
    /// caller has filled with that text ([`SubtitleTextBuffer`], its tail
    /// included); the others `text` ([`Subtitler::split`]).
    pub fn split_line(&self, text: &[u8], buffer: &SubtitleTextBuffer) -> Vec<Vec<u8>> {
        match self {
            Subtitler::Bitmap(_, breaker) => split_bitmap_buffer(buffer, *breaker),
            _ => self.split(text),
        }
    }

    /// The rows of a subtitle text, given as the dialog code copies it, up
    /// to the CR (its final blanks, [`crate::con_script::ConLine::trail`],
    /// included): the Chinese builds' breaker counts them; the other
    /// breakers are given the text without them, as before (the bitmap
    /// breaker on a fresh buffer, " " + the text).
    pub fn split(&self, text: &[u8]) -> Vec<Vec<u8>> {
        let trimmed = || {
            let n = text
                .iter()
                .rposition(|&c| !matches!(c, b' ' | b'\t'))
                .map_or(0, |i| i + 1);
            &text[..n]
        };
        match self {
            Subtitler::Gdi(_, None) => split_subtitle(trimmed()),
            Subtitler::Gdi(_, Some(cjk)) => split_subtitle_cjk(text, *cjk),
            Subtitler::Bitmap(_, breaker) => {
                split_bitmap_buffer(&fresh_buffer(trimmed()), *breaker)
            }
        }
    }

    /// The rows over a picture of `width` x `height` RGB565 pixels.
    pub fn draw_subtitles(
        &self,
        dst: &mut [u16],
        width: usize,
        height: usize,
        lines: &[Vec<u8>],
        y_offset: i32,
    ) {
        match self {
            Subtitler::Gdi(f, _) => {
                f.draw_subtitles(dst, width, height, lines, y_offset + self.rows_lower())
            }
            Subtitler::Bitmap(f, _) => f.draw_subtitles(dst, width, lines, y_offset),
        }
    }

    /// How much lower than the reference's the rows are drawn: the
    /// Japanese rows of the Japanese DVD build ([`JAPANESE_ROWS_LOWER`]).
    pub fn rows_lower(&self) -> i32 {
        match self {
            Subtitler::Gdi(_, Some(c)) if c.breaker == CjkBreaker::Spaces => JAPANESE_ROWS_LOWER,
            _ => 0,
        }
    }

    /// The text items for the HD mode (see [`subtitle_items`]).
    pub fn items(&self, lines: &[Vec<u8>], dx: i32, dy: i32) -> Vec<crate::hd::TextItem> {
        match self {
            Subtitler::Gdi(f, _) => f.items(lines, dx, dy + self.rows_lower()),
            Subtitler::Bitmap(f, _) => {
                let mut items = Vec::with_capacity(2 * lines.len());
                for (i, line) in lines.iter().enumerate() {
                    let y = bitmap_line_y(i, lines.len()) + dy;
                    for (o, color) in [(1, SUBTITLE_SHADOW), (0, SUBTITLE_COLOR)] {
                        items.push(crate::hd::TextItem {
                            x: SUBTITLE_X + dx + o,
                            y: y + o,
                            max_w: i32::MAX,
                            color,
                            text: line.clone(),
                            bitmap: Some(f.clone()),
                            code_page: TextCodePage::default(),
                            face: None,
                        });
                    }
                }
                items
            }
        }
    }
}

/// The text items of [`SubtitleFont::draw_subtitles`] for the HD mode
/// (see [`crate::hd::HdText`]): the shadow then the text of each line,
/// moved by (`dx`, `dy`) on the screen.
pub fn subtitle_items(
    lines: &[Vec<u8>],
    dx: i32,
    dy: i32,
    code_page: TextCodePage,
) -> Vec<crate::hd::TextItem> {
    gdi_items(
        lines,
        dx,
        dy,
        code_page,
        None,
        SUBTITLE_ROW_STEP,
        SUBTITLE_WIDTH,
    )
}

/// [`subtitle_items`] in `face` (the code page's bundled font for `None`),
/// the rows `step` pixels apart, `max_w` wide at most.
fn gdi_items(
    lines: &[Vec<u8>],
    dx: i32,
    dy: i32,
    code_page: TextCodePage,
    face: Option<FontFace>,
    step: i32,
    max_w: i32,
) -> Vec<crate::hd::TextItem> {
    let mut items = Vec::with_capacity(2 * lines.len());
    for (i, line) in lines.iter().enumerate() {
        let y = subtitle_row_y(i, lines.len(), step) + dy;
        for (o, color) in [(1, SUBTITLE_SHADOW), (0, SUBTITLE_COLOR)] {
            items.push(crate::hd::TextItem {
                x: SUBTITLE_X + dx + o,
                y: y + o,
                max_w,
                color,
                text: line.clone(),
                bitmap: None,
                code_page,
                face: face.clone(),
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line breaks of the original `FUN_0042ffe8` on these inputs, run
    /// under emulation (ScummVM's `video_dialog.h` cases).
    #[test]
    fn line_breaks_match_original() {
        let s = |t: &str| t.as_bytes().to_vec();
        assert_eq!(split_subtitle(b"Short line."), vec![s(" Short line.")]);
        assert_eq!(
            split_subtitle(
                b"The quick brown fox jumps over the lazy dog, then the lazy dog wakes up and \
                  chases the fox around the field."
            ),
            vec![
                s(" The quick brown fox jumps over the lazy dog, then the lazy dog wakes up and chases the fox around"),
                s(" the field."),
            ]
        );
        let words = ["word"; 30].join(" ");
        assert_eq!(
            split_subtitle(words.as_bytes()),
            vec![
                s(&format!(" {}", ["word"; 20].join(" "))),
                s(&format!(" {}", ["word"; 10].join(" "))),
            ]
        );
    }

    /// A window of 100 bytes without a space: the original faults (it
    /// writes NULs past its line buffer); the port cuts at 100 bytes.
    #[test]
    fn no_space_is_cut() {
        let x = vec![b'x'; 150];
        let lines = split_subtitle(&x);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 100);
        assert_eq!(lines.concat().len(), 151);
    }

    /// The windows without a space after the row's first byte, which no
    /// shipped text has: as the first line after start-up the original
    /// loops (FUN_0042e970: 10 empty rows for x * 75 under emulation) or
    /// faults (FUN_0042ea7c), what a real machine does is not known. The
    /// port's cuts, the ScummVM port's: the bitmap breaker at the forward
    /// space or the end of the text, the Swedish one at 75 bytes.
    #[test]
    fn windows_without_a_space() {
        let lens = |rows: Vec<Vec<u8>>| rows.iter().map(Vec::len).collect::<Vec<_>>();
        let x = |n: usize| "x".repeat(n);
        let short = format!("short {}", x(80));
        assert_eq!(lens(split_subtitle_1997(x(75).as_bytes())), [76]);
        assert_eq!(
            lens(split_subtitle_1997(format!("{} tail", x(76)).as_bytes())),
            [77, 5]
        );
        let rows = split_subtitle_1997(short.as_bytes());
        assert_eq!(rows[0], b" short");
        assert_eq!(lens(rows), [6, 81]);
        assert_eq!(lens(split_subtitle_1997_back(x(75).as_bytes())), [75, 1]);
        assert_eq!(lens(split_subtitle_1997_back(short.as_bytes())), [6, 75, 6]);
    }

    /// The synthetic texts of the emulation of atlantis-cd-sv-688128
    /// FUN_0042ea7c and atlantis-cd-en-688128 FUN_0042e970: the row lengths
    /// of each (the leading space included).
    #[test]
    fn back_breaker_matches_original() {
        let words = |w: &str, n: usize| vec![w; n].join(" ");
        let fox = "The quick brown fox jumps over the lazy dog, then the lazy dog \
                   wakes up and chases the fox around the field.";
        let cases: [(String, &[usize], &[usize]); 8] = [
            ("Short line.".into(), &[12], &[12]),
            (
                format!("{} ", words(&"y".repeat(49), 2)),
                &[50, 51],
                &[50, 51],
            ),
            ("x".repeat(74), &[75], &[75]),
            (
                format!("{} ", words(&"x".repeat(60), 3)),
                &[61, 61, 62],
                &[61, 61, 62],
            ),
            (words("word", 30), &[75, 75], &[75, 75]),
            (words("abcdefghi", 25), &[70, 70, 70, 40], &[70, 70, 70, 40]),
            (fox.into(), &[72, 37], &[76, 33]),
            (
                format!("{} {} ccc ddd", words("aaaaaaa", 9), "b".repeat(30)),
                &[72, 39],
                &[72, 39],
            ),
        ];
        let lens = |rows: Vec<Vec<u8>>| rows.iter().map(Vec::len).collect::<Vec<_>>();
        for (text, sv, en) in cases {
            let t = text.as_bytes();
            assert_eq!(lens(split_subtitle_1997_back(t)), sv, "{text}");
            assert_eq!(lens(split_subtitle_1997(t)), en, "{text}");
            assert_eq!(split_subtitle_1997_back(t).concat().len(), t.len() + 1);
        }
        let fox_rows = split_subtitle_1997_back(fox.as_bytes());
        assert!(fox_rows[0].ends_with(b"wakes up"));
        assert!(fox_rows[1].starts_with(b" and chases"));
    }

    /// A run of 75 bytes or more with no space (the original faults; no
    /// shipped text has one): the port cuts it at the window end.
    #[test]
    fn back_breaker_cuts_a_spaceless_run_at_75() {
        let lens = |n: usize| -> Vec<usize> {
            split_subtitle_1997_back(&vec![b'x'; n])
                .iter()
                .map(Vec::len)
                .collect()
        };
        assert_eq!(lens(75), [75, 1]);
        assert_eq!(lens(81), [75, 7]);
        assert_eq!(lens(86), [75, 12]);
        assert_eq!(lens(200), [75, 75, 51]);
        // A word past the window after a space: cut at the space, then at
        // 75 bytes.
        let text = format!("ab {}", "z".repeat(160));
        let rows = split_subtitle_1997_back(text.as_bytes());
        let l: Vec<usize> = rows.iter().map(Vec::len).collect();
        assert_eq!(l, [3, 75, 75, 11]);
    }

    /// Line i of n at y = 0x1ce - (n - i + 1) * 14: one line at 434, lines
    /// 14 px apart.
    #[test]
    fn line_y_matches_original() {
        assert_eq!(subtitle_line_y(0, 1), 0x1ce - 2 * 14);
        assert_eq!(subtitle_line_y(1, 2) - subtitle_line_y(0, 2), 14);
        assert_eq!(subtitle_line_y(1, 2), 0x1ce - 2 * 14);
    }

    #[test]
    fn font_draws_white_over_black_shadow() {
        let f = SubtitleFont::new();
        assert_eq!(f.ascent, 12);
        assert!(f.width(b"Seth") > 20 && f.width(b"Seth") < 30);
        let mut dst = vec![0x1234u16; 640 * 480];
        f.draw_subtitles(&mut dst, 640, 480, &[b" Hello, Seth!".to_vec()], 0);
        let white = dst.iter().filter(|&&p| p == SUBTITLE_COLOR).count();
        let black = dst.iter().filter(|&&p| p == SUBTITLE_SHADOW).count();
        assert!(white > 50, "{white} white pixels");
        assert!(black > 10, "{black} shadow pixels");
        // Everything inside the line's band.
        for (i, &p) in dst.iter().enumerate() {
            if p != 0x1234 {
                let y = (i / 640) as i32;
                assert!((434..434 + 16).contains(&y), "pixel at row {y}");
            }
        }
    }

    #[test]
    fn hd_text_draws_the_same_glyphs_larger() {
        let f = SubtitleFont::new();
        let line = b" Hello, Seth! This line is long enough".to_vec();
        // At scale 1 the smoothed glyphs fall where the 640 ones are.
        let mut lo = vec![0u16; 640 * 480];
        f.draw(&mut lo, 640, 480, &line, 5, 434, SUBTITLE_WIDTH, 0xffff);
        let (w, h) = (1920, 1440);
        let mut hd = vec![0u32; w * h];
        f.draw_hd(
            &mut hd,
            w,
            (0, 0, w, h),
            &line,
            5,
            434,
            SUBTITLE_WIDTH,
            0x00ff_ffff,
        );
        let lit: Vec<usize> = (0..w * h).filter(|&i| hd[i] & 0xff > 0x80).collect();
        assert!(lit.len() > 9 * 200, "{} lit pixels", lit.len());
        // Most of the HD text lies over the 640 text's pixels (the
        // outlines are the same, at 3 times the size).
        let over = lit
            .iter()
            .filter(|&&i| {
                let (x, y) = (i % w / 3, i / w / 3);
                (y.saturating_sub(1)..y + 2)
                    .any(|yy| (x.saturating_sub(1)..x + 2).any(|xx| lo[yy * 640 + xx] != 0))
            })
            .count();
        assert!(over * 100 >= lit.len() * 98, "{over} of {}", lit.len());
        // Smoothed: partly covered pixels.
        assert!(hd.iter().any(|&p| p & 0xff > 0 && p & 0xff < 0xff));
        // Clipped to the picture.
        let mut small = vec![0u32; 100 * 100];
        f.draw_hd(
            &mut small,
            100,
            (10, 10, 80, 60),
            &line,
            5,
            0,
            SUBTITLE_WIDTH,
            0x00ff_ffff,
        );
        for (i, &p) in small.iter().enumerate() {
            if p != 0 {
                let (x, y) = (i % 100, i / 100);
                assert!((10..90).contains(&x) && (10..70).contains(&y));
            }
        }
        let items = subtitle_items(&[line.clone(), line], 0, 0, TextCodePage::Windows1252);
        assert_eq!(items.len(), 4);
        assert_eq!(
            (items[0].x, items[0].y, items[0].color),
            (6, subtitle_line_y(0, 2) + 1, 0)
        );
        assert_eq!((items[3].x, items[3].y), (5, subtitle_line_y(1, 2)));
    }

    /// HD: each pixel of the 8x8 font a sharp block at the output's
    /// scale, where the 640x480 pixel is.
    #[test]
    fn bitmap_hd_draws_blocks() {
        let mut data = vec![0u8; BITMAP_FONT_SIZE];
        data[usize::from(b'A') * 64] = 1; // glyph 'A': its top-left pixel
        let f = std::sync::Arc::new(BitmapFont::new(&data).unwrap());
        assert!(BitmapFont::new(&data[1..]).is_none());
        let (w, h) = (1920, 1440);
        let mut hd = vec![0u32; w * h];
        f.draw_hd(&mut hd, w, (0, 0, w, h), b" A", 5, 100, 0xffffff);
        let lit: Vec<usize> = (0..w * h).filter(|&i| hd[i] != 0).collect();
        assert_eq!(lit.len(), 9);
        assert_eq!((lit[0] % w, lit[0] / w), (13 * 3, 100 * 3));
        let s = Subtitler::Bitmap(f, BitmapBreaker::Nearest);
        let items = s.items(&[b" A".to_vec()], 0, 0);
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].x, items[0].y), (6, bitmap_line_y(0, 1) + 1));
        assert!(items[1].bitmap.is_some());
    }

    #[test]
    fn width_limit_stops_the_line() {
        let f = SubtitleFont::new();
        let long = vec![b'W'; 100];
        let mut dst = vec![0u16; 640 * 20];
        f.draw(&mut dst, 640, 20, &long, 5, 0, SUBTITLE_WIDTH, 0xffff);
        for y in 0..20 {
            for x in 637..640 {
                assert_eq!(dst[y * 640 + x], 0);
            }
        }
    }

    /// "中文字幕" and "你好，世界。" in Big5 (the test's own text).
    const BIG5_LINES: [&[u8]; 2] = [
        b" \xa4\xa4\xa4\xe5\xa6\x72\xb9\xf5",
        b" \xa7\x41\xa6\x6e\xa1\x41\xa5\x40\xac\xc9\xa1\x43",
    ];

    /// The pixels `font` draws for `line` alone at (x, y).
    fn lit(font: &SubtitleFont, line: &[u8], x: i32, y: i32) -> Vec<bool> {
        let mut b = vec![0u16; 640 * 480];
        font.draw(&mut b, 640, 480, line, x, y, SUBTITLE_WIDTH, 0xffff);
        b.iter().map(|&p| p != 0).collect()
    }

    /// The first and last pixel rows of `px`.
    fn rows(px: &[bool]) -> (usize, usize) {
        let ys: Vec<usize> = (0..px.len()).filter(|&i| px[i]).map(|i| i / 640).collect();
        (*ys.iter().min().unwrap(), *ys.iter().max().unwrap())
    }

    /// The Chinese builds' rows (atlantis-cd-zh-tw-696832 FUN_0042f798):
    /// line i of n at (5, 0x1ce - (n - i + 1) * 18), white over the black
    /// shadow drawn first at (+1, +1), the Big5 pairs as characters of the
    /// bundled Noto Serif TC at 16 pixels per em (an ideograph 16 pixels
    /// wide), within the 16-pixel cell under the row's y.
    #[test]
    fn cjk_rows_and_shadow() {
        let f = SubtitleFont::with_code_page(TextCodePage::Windows950).with_row_step(18);
        assert_eq!(f.face().source(), FaceSource::Bundled);
        assert_eq!(f.ascent, 14);
        assert_eq!(f.width(b"\xa4\xa4\xa4\xe5"), 32);
        let lines: Vec<Vec<u8>> = BIG5_LINES.iter().map(|l| l.to_vec()).collect();
        let mut dst = vec![0x1234u16; 640 * 480];
        f.draw_subtitles(&mut dst, 640, 480, &lines, 0);
        let ys = [subtitle_row_y(0, 2, 18), subtitle_row_y(1, 2, 18)];
        assert_eq!(ys, [0x1ce - 3 * 18, 0x1ce - 2 * 18]);
        let mut white = vec![false; 640 * 480];
        let mut shadow = vec![false; 640 * 480];
        for (line, &y) in lines.iter().zip(&ys) {
            let (w, s) = (lit(&f, line, 5, y), lit(&f, line, 6, y + 1));
            let (top, bottom) = rows(&w);
            assert!(
                top >= y as usize && bottom < y as usize + 16,
                "{top}..{bottom}"
            );
            assert_eq!(rows(&s), (top + 1, bottom + 1));
            for i in 0..w.len() {
                white[i] |= w[i];
                shadow[i] |= s[i];
            }
        }
        // The rows 18 pixels apart, glyph for glyph (both start with an
        // ideograph whose top is at the same height).
        for i in 0..dst.len() {
            let want = if white[i] {
                SUBTITLE_COLOR
            } else if shadow[i] {
                SUBTITLE_SHADOW
            } else {
                0x1234
            };
            assert_eq!(dst[i], want, "pixel ({}, {})", i % 640, i / 640);
        }
        assert!(white.iter().filter(|&&p| p).count() > 300);
        // The same text, the bytes paired from the lead byte: a lone lead
        // byte at the end draws nothing.
        let mut a = vec![0u16; 640 * 480];
        let mut b = vec![0u16; 640 * 480];
        f.draw(
            &mut a,
            640,
            480,
            b"\xa4\xa4",
            5,
            100,
            SUBTITLE_WIDTH,
            0xffff,
        );
        f.draw(
            &mut b,
            640,
            480,
            b"\xa4\xa4\xa4",
            5,
            100,
            SUBTITLE_WIDTH,
            0xffff,
        );
        assert_eq!(a, b);
        assert_eq!(f.width(b"\xa4\xa4\xa4"), 16);
    }

    /// The CJK glyphs' scan conversion: the three thin horizontals of 三,
    /// under a pixel high at 16 pixels per em, each a row of at least 10
    /// pixels (dropout control); the Latin path's glyphs stay the pixels
    /// half covered.
    #[test]
    fn cjk_scan_conversion_keeps_thin_strokes() {
        for cp in [TextCodePage::Windows950, TextCodePage::Windows936] {
            let f = SubtitleFont::with_code_page(cp);
            let font = f.face.font();
            let scale = SubtitleFont::scale(&font, CJK_SUBTITLE_EM_PX);
            let g = GlyphBits::of(&font, scale, '三', true);
            assert_eq!(g.advance, 16);
            let runs = (0..g.h)
                .filter(|&y| {
                    let row = &g.bits[(y * g.w) as usize..((y + 1) * g.w) as usize];
                    row.split(|&b| !b).any(|r| r.len() >= 10)
                })
                .count();
            assert_eq!(runs, 3, "{cp:?}");
        }
    }

    /// The Simplified build's text (GBK) in Noto Serif SC; the other
    /// builds keep Liberation Sans and 14-pixel rows.
    #[test]
    fn faces_by_code_page() {
        let sc = SubtitleFont::with_code_page(TextCodePage::Windows936);
        assert_eq!(sc.ascent, 14);
        assert_eq!(sc.width(b"\xd6\xd0\xce\xc4"), 32);
        assert!(sc.face().has_char('厖') && sc.face().has_char('棗'));
        // AA A1, in the user-defined area (U+E000), no glyph: its cell.
        assert!(!sc.face().has_char('\u{e000}'));
        assert_eq!(sc.width(b"\xaa\xa1\xd6\xd0"), 32);
        assert_eq!(sc.layout(b"\xaa\xa1\xd6\xd0", 0, 640)[1], ('中', 16));
        assert_eq!(sc.row_step(), SUBTITLE_ROW_STEP);
        for cp in [TextCodePage::Windows1252, TextCodePage::Dos850] {
            let f = SubtitleFont::with_code_page(cp);
            assert_eq!(*f.face(), FontFace::bundled(TextCodePage::Windows1252));
            assert_eq!((f.ascent, f.row_step()), (12, 14));
        }
        assert_ne!(
            FontFace::bundled(TextCodePage::Windows950),
            FontFace::bundled(TextCodePage::Windows936)
        );
    }

    /// The file the web page fetches for a Chinese release is the font the
    /// desktop build has built in.
    #[test]
    fn cjk_font_file_is_the_bundled_font() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/");
        for (cp, data) in [
            (TextCodePage::Windows950, NOTO_SERIF_TC),
            (TextCodePage::Windows936, NOTO_SERIF_SC),
        ] {
            let name = cjk_font_file(cp).unwrap();
            assert_eq!(std::fs::read(format!("{dir}{name}")).unwrap(), data);
            assert_eq!(bundled_cjk(cp), Some(data));
        }
        for cp in [TextCodePage::Windows1252, TextCodePage::Dos850] {
            assert_eq!((cjk_font_file(cp), bundled_cjk(cp)), (None, None));
        }
    }

    /// The header test of the web page: the port's fonts and a collection
    /// of them read as fonts, and so does each face the player's font can
    /// give ([`FontFace::player`] reads it); other bytes do not.
    #[test]
    fn font_headers() {
        for data in [NOTO_SERIF_TC, NOTO_SERIF_SC] {
            assert!(is_font_header(data));
            let ttc = collection(&[data]);
            assert!(is_font_header(&ttc));
            assert!(FontFace::player(TextCodePage::Windows936, ttc).is_some());
        }
        for head in [&b""[..], b"OTT", b"PK\x03\x04", b"\0\0\0\0", b"wOFF"] {
            assert!(!is_font_header(head), "{head:?}");
        }
    }

    /// A font collection (`.ttc`) of `fonts`: each font's table offsets
    /// moved by where it lands.
    fn collection(fonts: &[&[u8]]) -> Vec<u8> {
        let mut out = b"ttcf".to_vec();
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend((fonts.len() as u32).to_be_bytes());
        let mut at = 12 + 4 * fonts.len();
        let mut starts = Vec::new();
        for f in fonts {
            starts.push(at);
            out.extend((at as u32).to_be_bytes());
            at += (f.len() + 3) & !3;
        }
        for (f, &start) in fonts.iter().zip(&starts) {
            out.resize(start, 0);
            let mut f = f.to_vec();
            let tables = usize::from(u16::from_be_bytes([f[4], f[5]]));
            for t in 0..tables {
                let r = 12 + 16 * t + 8;
                let off = u32::from_be_bytes([f[r], f[r + 1], f[r + 2], f[r + 3]]);
                f[r..r + 4].copy_from_slice(&(off + start as u32).to_be_bytes());
            }
            out.extend(f);
        }
        out
    }

    /// The player's font first, by its family in a collection; the port's
    /// when there is none, it does not read, or it needs its hinting.
    #[test]
    fn font_loading_falls_back() {
        let tc = TextCodePage::Windows950;
        assert_eq!(player_font(tc), Some(("mingliu.ttc", "MingLiU")));
        assert_eq!(
            player_font(TextCodePage::Windows936),
            Some(("simsun.ttc", "SimSun"))
        );
        assert_eq!(player_font(TextCodePage::Windows1252), None);
        // None, or a file that is no font: the bundled face.
        assert_eq!(FontFace::for_release(tc, None), FontFace::bundled(tc));
        let junk = FontFace::for_release(tc, Some(b"not a font at all".to_vec()));
        assert_eq!(junk, FontFace::bundled(tc));
        assert_eq!(junk.source(), FaceSource::Bundled);
        // A collection: the face of the family asked for, else the first.
        let ttc = collection(&[NOTO_SERIF_TC, NOTO_SERIF_SC]);
        assert_eq!(pick_face(&ttc, "Noto Serif SC"), Some(1));
        assert_eq!(pick_face(&ttc, "noto serif tc"), Some(0));
        assert_eq!(pick_face(&ttc, "MingLiU"), Some(0));
        assert_eq!(pick_face(NOTO_SERIF_SC, "SimSun"), Some(0));
        assert_eq!(
            pick_face(b"ttcf\0\x01\0\0\0\0\0\x01\0\0\0\x10", "MingLiU"),
            None
        );
        let player = FontFace::for_release(tc, Some(ttc));
        assert_eq!(player.source(), FaceSource::Player);
        assert!(player.has_char('中'));
        // Drawn with GDI's ascent (the Windows ascent: 1151 / 1000 em).
        let f = SubtitleFont::with_face(tc, player.clone()).with_row_step(18);
        assert_eq!(f.ascent, 18);
        assert_eq!(f.width(b"\xa4\xa4\xa4\xe5"), 32);
        let items = f.items(&[BIG5_LINES[0].to_vec()], 0, 0);
        assert_eq!(items[0].face.as_ref(), Some(&player));
        // MingLiU before version 7.00 needs its hinting.
        assert!(needs_hinting("MingLiU", 3.21) && needs_hinting("PMingLiU", 5.03));
        assert!(!needs_hinting("MingLiU", 7.0) && !needs_hinting("SimSun", 5.24));
    }

    /// HD: the Chinese rows' items carry the face and the 18-pixel step;
    /// the characters drawn larger over the 640 ones.
    #[test]
    fn cjk_hd_draws_the_same_glyphs_larger() {
        let f = SubtitleFont::with_code_page(TextCodePage::Windows950).with_row_step(18);
        let line = BIG5_LINES[1].to_vec();
        let items = f.items(&[BIG5_LINES[0].to_vec(), line.clone()], 0, 0);
        assert_eq!(items.len(), 4);
        assert_eq!((items[0].x, items[0].y), (6, subtitle_row_y(0, 2, 18) + 1));
        assert_eq!((items[3].x, items[3].y), (5, subtitle_row_y(1, 2, 18)));
        assert_eq!(items[3].code_page, TextCodePage::Windows950);
        assert_eq!(items[3].face.as_ref(), Some(f.face()));
        let lo = lit(&f, &line, 5, 400);
        let (w, h) = (1920, 1440);
        let mut hd = vec![0u32; w * h];
        f.draw_hd(
            &mut hd,
            w,
            (0, 0, w, h),
            &line,
            5,
            400,
            SUBTITLE_WIDTH,
            0x00ff_ffff,
        );
        let on: Vec<usize> = (0..w * h).filter(|&i| hd[i] & 0xff > 0x80).collect();
        assert!(on.len() > 9 * 150, "{} lit pixels", on.len());
        let over = on
            .iter()
            .filter(|&&i| {
                let (x, y) = (i % w / 3, i / w / 3);
                (y.saturating_sub(1)..y + 2)
                    .any(|yy| (x.saturating_sub(1)..x + 2).any(|xx| lo[yy * 640 + xx]))
            })
            .count();
        assert!(over * 100 >= on.len() * 97, "{over} of {}", on.len());
    }
}
