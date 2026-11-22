//! *Atlantis V*'s dialogs, subject menu, film subtitles and text.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and not read).
//!
//! - Text: `font.txt`'s fonts are made by @ 0x0045fa40: a `LOGFONT` whose
//!   face is the `Type` word (`arial`, `"Courier New"`, `"Lucida sans
//!   unicode"`), height `Height` (a positive `lfHeight`: the cell height),
//!   `lfQuality` 4 (antialiased) when `Antialias` is `true`, else 3; the
//!   `Name` file (`cour.TTF`, `Atlantis.ttf`) is only given to
//!   `AddFontResourceA` first (@ 0x0046124f), which fails when it is
//!   missing (they are not shipped), so Windows draws the `Type` face.
//!   Native draws each `Type` with the Liberation face of the same
//!   metrics: `arial` with Liberation Sans, `Courier New` with Liberation
//!   Mono, Bold for a `Weight` nearer `FW_BOLD` than `FW_NORMAL` (the
//!   faces Windows has are 400 and 700). `Lucida sans unicode` and
//!   `lucida console` have no free metric twin: Native draws them with
//!   the system's own files when it has them (`l_10646.ttf`,
//!   `lucon.ttf`, Windows' fonts folder), which have no bold style, so a
//!   bold weight is emboldened as GDI simulates it; else Liberation Sans
//!   stands in (a known deviation, logged). Italic is not drawn: every
//!   font of the French file says `Italic false`.
//! - The dialog player (`CDialogPlayer` step @ 0x004329a0, drawing
//!   @ 0x00433910), driven by `Atl_Dialog_Play` (@ 0x00407ac0): see
//!   [`Player`].
//! - The subject menu (`CMenu_Dialog`, mode 9; its elements
//!   @ 0x00411a90): [`subject_layout`].
//! - Film subtitles (`CKinematicsPlayer`, step @ 0x0042b0f0, drawing
//!   @ 0x0042b220): [`FilmSubtitles`].

use std::collections::HashMap;
use std::sync::Arc;

use atlantis3_data::atlantis5_texts::{Dialog, Dialogs, Picturing, Subtitle};
use atlantis3_data::font_txt::{FontDef, FontTxt};
use atlantis_data::FileSource;

use crate::text::{layout, to_cp1252, GdiFont, Glyphs, Line, Zone};

/// The shipped fonts Native draws `font.txt`'s faces with, in
/// [`Face`]'s order.
static FACES: [&[u8]; 4] = [
    include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../../../assets/fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/LiberationMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/LiberationMono-Bold.ttf"),
];

/// The system's files of the faces Native ships no twin of, in
/// [`Face`]'s order after the shipped ones: Lucida Sans Unicode and
/// Lucida Console as Windows installs them.
pub const SYSTEM_FACES: [(&str, &str); 2] = [
    ("lucida sans unicode", "l_10646.ttf"),
    ("lucida console", "lucon.ttf"),
];

/// The face that draws a `font.txt` font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// Liberation Sans (Arial's metrics).
    Sans,
    /// Liberation Sans Bold (Arial Bold's).
    SansBold,
    /// Liberation Mono (Courier New's).
    Mono,
    /// Liberation Mono Bold (Courier New Bold's).
    MonoBold,
    /// The system's Lucida Sans Unicode (`l_10646.ttf`).
    LucidaSans,
    /// The system's Lucida Console (`lucon.ttf`).
    LucidaConsole,
}

impl Face {
    /// The face for `Type` `face` at GDI weight `weight`. GDI matches the
    /// face name without case (`arial` in the French file); Arial and
    /// Courier New come in weights 400 and 700, and the nearer one is
    /// taken [INFERRED: GDI's font mapper; the French file only uses
    /// `FW_NORMAL` and `FW_BOLD`]. Any other name (`Lucida sans unicode`,
    /// `lucida console`) gets Liberation Sans as a stand-in.
    pub fn of(face: &str, weight: i32) -> Face {
        let bold = weight > 550;
        match (face.to_ascii_lowercase().as_str(), bold) {
            ("courier new", false) => Face::Mono,
            ("courier new", true) => Face::MonoBold,
            (_, false) => Face::Sans,
            (_, true) => Face::SansBold,
        }
    }

    /// Whether this face has the metrics of `face` (else it stands in).
    pub fn twin_of(face: &str) -> bool {
        matches!(face.to_ascii_lowercase().as_str(), "arial" | "courier new")
    }

    /// The system face named `face` (any case), when it is one Native
    /// looks for.
    fn system(face: &str) -> Option<Face> {
        match face.to_ascii_lowercase().as_str() {
            "lucida sans unicode" => Some(Face::LucidaSans),
            "lucida console" => Some(Face::LucidaConsole),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// `font.txt`'s fonts with their glyphs.
pub struct Fonts {
    defs: Vec<FontDef>,
    glyphs: Vec<Option<Glyphs>>,
    fonts: Vec<Option<ab_glyph::FontVec>>,
    cache: std::cell::RefCell<HashMap<(Face, i32, bool), GdiFont>>,
}

impl Fonts {
    /// `fonts/<language>/font.txt` (`CTextManager` @ 0x004601f0), the
    /// system's Lucida faces from its fonts folder.
    pub fn load(src: &Arc<dyn FileSource>, language: &str) -> Fonts {
        Fonts::load_with(
            src,
            language,
            &atlantis_data::system_fonts::system_font_file,
        )
    }

    /// [`Fonts::load`] with the system's font files given by `system`
    /// (a file name → its bytes).
    pub fn load_with(
        src: &Arc<dyn FileSource>,
        language: &str,
        system: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Fonts {
        let defs: Vec<FontDef> = src
            .read(&format!("datas/fonts/{language}/font.txt"))
            .map(|d| FontTxt::parse_atlantis5(&d).fonts)
            .unwrap_or_default();
        let mut files: Vec<Option<Vec<u8>>> = FACES.iter().map(|f| Some(f.to_vec())).collect();
        for (name, file) in SYSTEM_FACES {
            // Only looked for when the file asks for the face.
            let asked = defs.iter().any(|d| d.face.eq_ignore_ascii_case(name));
            files.push(asked.then(|| system(file)).flatten());
        }
        let fonts: Vec<Option<ab_glyph::FontVec>> = files
            .iter()
            .map(|f| {
                f.as_ref()
                    .and_then(|f| ab_glyph::FontVec::try_from_vec(f.clone()).ok())
            })
            .collect();
        for d in &defs {
            if Face::twin_of(&d.face) {
                continue;
            }
            match Face::system(&d.face) {
                Some(f) if fonts[f.index()].is_some() => log::info!(
                    "font.txt: {} asks for {:?}: the system's {}",
                    d.id,
                    d.face,
                    SYSTEM_FACES[f.index() - FACES.len()].1
                ),
                _ => log::info!(
                    "font.txt: {} asks for {:?}, which has no shipped twin and is not installed: Liberation Sans stands in",
                    d.id,
                    d.face
                ),
            }
        }
        Fonts {
            defs,
            glyphs: files.into_iter().map(|f| f.and_then(Glyphs::new)).collect(),
            fonts,
            cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// The face that draws the font `id` (Liberation Sans for an unknown
    /// id): the system's Lucida face when the file names it and the
    /// system has it, else [`Face::of`].
    pub fn face(&self, id: &str) -> Face {
        let Some(d) = self.def(id) else {
            return Face::Sans;
        };
        match Face::system(&d.face) {
            Some(f) if self.fonts[f.index()].is_some() => f,
            _ => Face::of(&d.face, d.weight()),
        }
    }

    /// Whether the font `id` is a bold weight of a face that has no bold
    /// style (the system's Lucida faces): GDI emboldens it itself.
    pub fn simulated_bold(&self, id: &str) -> bool {
        matches!(self.face(id), Face::LucidaSans | Face::LucidaConsole)
            && self.def(id).is_some_and(|d| d.weight() > 550)
    }

    /// The font of `id` (the first with that id: @ 0x00461375).
    pub fn def(&self, id: &str) -> Option<&FontDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// The measures of the font `id` at its height.
    pub fn gdi(&self, id: &str) -> Option<GdiFont> {
        let h = self.def(id)?.height;
        let face = self.face(id);
        let bold = self.simulated_bold(id);
        let font = self.fonts[face.index()].as_ref()?;
        Some(
            self.cache
                .borrow_mut()
                .entry((face, h, bold))
                .or_insert_with(|| gdi_of(font, h, bold))
                .clone(),
        )
    }

    /// The lines of `text` in a `w` × `h` box with the layout `zone`.
    pub fn lines(&self, id: &str, text: &str, w: i32, h: i32, zone: Zone) -> Vec<Line> {
        let Some(g) = self.gdi(id) else {
            return Vec::new();
        };
        let bytes: Vec<u8> = text.chars().filter(|&c| c != '\r').map(to_cp1252).collect();
        layout(&g, &bytes, w, h, zone)
    }

    /// The height of `text` broken into lines `w` wide (@ 0x00461800: the
    /// lines × the line step).
    pub fn text_height(&self, id: &str, text: &str, w: i32) -> i32 {
        let Some(g) = self.gdi(id) else { return 0 };
        let n = self
            .lines(id, text, w, i32::MAX / 4, Zone::new(0, 0, 0, 0, 0, 0))
            .len();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let n = n as i32;
        n * g.tm_height
    }

    /// Draws `lines` of font `id` in `rgb` at offset (ox, oy) into `out`
    /// (`w` × `h`): blended by the glyphs' coverage for an antialiased
    /// font, else each pixel covered at least half drawn whole (GDI's
    /// `NONANTIALIASED_QUALITY` gives whole pixels; its exact rasterizer
    /// is not reproduced).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        id: &str,
        lines: &[Line],
        rgb: u32,
        out: &mut [u32],
        w: usize,
        h: usize,
        ox: i32,
        oy: i32,
    ) {
        let (Some(g), Some(glyphs)) = (self.gdi(id), &self.glyphs[self.face(id).index()]) else {
            return;
        };
        let antialias = self.def(id).is_none_or(|d| d.antialias);
        let moved: Vec<Line> = lines
            .iter()
            .map(|l| Line {
                x: l.x + ox,
                y: l.y + oy,
                bytes: l.bytes.clone(),
                dx: l.dx.clone(),
            })
            .collect();
        // The coverage first (white on black), then the colour.
        let mut cover = vec![0u32; w * h];
        glyphs.draw(&g, &moved, 0x00ff_ffff, &mut cover, w, h);
        if self.simulated_bold(id) {
            // GDI's emboldening: the glyph again one pixel to the right
            // [INFERRED: GDI's simulated bold, not measured here].
            let right: Vec<Line> = moved
                .iter()
                .map(|l| Line {
                    x: l.x + 1,
                    ..l.clone()
                })
                .collect();
            glyphs.draw(&g, &right, 0x00ff_ffff, &mut cover, w, h);
        }
        let (r, gg, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
        for (px, c) in out.iter_mut().zip(&cover) {
            let a = (c >> 16) & 0xff;
            if a == 0 {
                continue;
            }
            if !antialias {
                if a >= 128 {
                    *px = (*px & 0xff00_0000) | (rgb & 0x00ff_ffff);
                }
                continue;
            }
            let mix = |s: u32, d: u32| (s * a + d * (255 - a)) / 255;
            let (dr, dg, db) = ((*px >> 16) & 0xff, (*px >> 8) & 0xff, *px & 0xff);
            *px = (*px & 0xff00_0000) | (mix(r, dr) << 16) | (mix(gg, dg) << 8) | mix(b, db);
        }
    }

    /// Whether the font `id` is drawn antialiased.
    pub fn antialias(&self, id: &str) -> bool {
        self.def(id).is_none_or(|d| d.antialias)
    }
}

/// GDI's measures of a face at a positive `lfHeight` (the cell height),
/// from the font's own metrics: the line step is the height, the ascent
/// the font's scaled ascent, each byte's A, B, C from its bearing, width
/// and advance. A `simulated_bold` face is one pixel wider per character
/// (its B and the average width + 1) [INFERRED: GDI's emboldening of a
/// TrueType face without a bold style].
fn gdi_of(font: &ab_glyph::FontVec, height: i32, simulated_bold: bool) -> GdiFont {
    use ab_glyph::{Font, ScaleFont};
    #[allow(clippy::cast_precision_loss)]
    let scale = ab_glyph::PxScale::from(height as f32);
    let sf = font.as_scaled(scale);
    let mut abc = Vec::with_capacity(256);
    let mut total = 0;
    let mut count = 0;
    for b in 0u8..=255 {
        let ch = crate::text::cp1252(b);
        let id = sf.glyph_id(ch);
        let adv = sf.h_advance(id);
        let g = id.with_scale(scale);
        let (a, bb) = match font.outline_glyph(g) {
            Some(o) => {
                let r = o.px_bounds();
                (r.min.x, r.max.x - r.min.x)
            }
            None => (0.0, 0.0),
        };
        #[allow(clippy::cast_possible_truncation)]
        let (a, bb, adv) = (a.round() as i32, bb.round() as i32, adv.round() as i32);
        if b.is_ascii_alphabetic() {
            total += adv;
            count += 1;
        }
        let bb = bb + i32::from(simulated_bold);
        abc.push((a, bb, adv + i32::from(simulated_bold) - a - bb));
    }
    #[allow(clippy::cast_possible_truncation)]
    let ascent = sf.ascent().round() as i32;
    GdiFont {
        height,
        tm_height: height,
        ascent,
        ave_char_width: if count > 0 { total / count } else { height / 2 }
            + i32::from(simulated_bold),
        charset: 0,
        abc,
    }
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

/// The talkers of a dialog, each with the picturing its `talker` element
/// gives: the dialog's own one when it has one, else the talker set's
/// (formats/texts-xml; [INFERRED: the dialog reader's merge is not read
/// instruction by instruction]), in their first tirade's order.
pub fn talkers(set: &Dialogs, d: &Dialog) -> Vec<(String, Picturing)> {
    let mut out: Vec<(String, Picturing)> = Vec::new();
    for t in &d.tirades {
        if out.iter().any(|(n, _)| *n == t.talker) {
            continue;
        }
        out.push((t.talker.clone(), picturing(set, d, &t.talker)));
    }
    out
}

/// The picturing of `talker` in dialog `d`.
pub fn picturing(set: &Dialogs, d: &Dialog, talker: &str) -> Picturing {
    d.talkers
        .iter()
        .chain(set.talkers.iter())
        .find(|t| t.name == talker)
        .map_or(Picturing::None, |t| t.picturing)
}

/// The voice of tirade `k` (0-based) of dialog `id` (@ 0x00433db0):
/// `dialog/` + the language + `/` + `AT%.4d%.2d` (0x0066d2a0: the id,
/// k + 1) + `.` + `ogg`, under the sounds folder (the path table's entry
/// 9, @ 0x00432a9c).
pub fn voice_path(language: &str, id: u32, k: usize) -> String {
    format!("datas/sounds/dialog/{language}/AT{id:04}{:02}.ogg", k + 1)
}

/// The bubble's kind and its box, as the player's state 0 sets them per
/// picturing (@ 0x00432b5e..0x004335dc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `pict_3d`: `bubble_3d.spr`, state 3.
    ThreeD,
    /// `pict_2d`: the talker's film `videos/<talker>.bik` in the
    /// rectangle (40, H − 144)–(168, H − 16), `bubble_2d.spr`, state 2.
    TwoD,
    /// Any other: `bubble.spr`, state 1.
    None,
}

impl Kind {
    pub fn of(p: Picturing) -> Kind {
        match p {
            Picturing::ThreeD => Kind::ThreeD,
            Picturing::TwoD => Kind::TwoD,
            Picturing::None => Kind::None,
        }
    }

    /// The player's state for this kind.
    pub fn state(self) -> u32 {
        match self {
            Kind::ThreeD => 3,
            Kind::TwoD => 2,
            Kind::None => 1,
        }
    }

    /// The bubble's file under `sprites2d/dialog/` (the path table's
    /// entry 0xd).
    pub fn bubble(self) -> &'static str {
        match self {
            Kind::ThreeD => "bubble_3d.spr",
            Kind::TwoD => "bubble_2d.spr",
            Kind::None => "bubble.spr",
        }
    }
}

/// The text box of a tirade on a `w` × `h` screen: +0x40 = W − 0x30c
/// (W − 0x37c without a picture), +0x44 = H − 0x78, +0x48 = +0x50 =
/// 0x2f8 (760) wide, +0x54 = 100 high (@ 0x00432c7b..).
pub fn text_box(kind: Kind, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let x = match kind {
        Kind::None => w - 0x37c,
        _ => w - 0x30c,
    };
    (x, h - 0x78, 0x2f8, 100)
}

/// The bubble's hot spot (@ 0x00433910 → 0x00463d20): (W / 2, H − 10)
/// in state 1, else (W − 10, H − 10).
pub fn bubble_at(kind: Kind, w: i32, h: i32) -> (i32, i32) {
    match kind {
        Kind::None => (w / 2, h - 10),
        _ => (w - 10, h - 10),
    }
}

/// The 2D talker's film rectangle (@ 0x00432e48..: left 0x28, top
/// H − 0x90, right 0xa8, bottom H − 0x10).
pub fn portrait_rect(h: i32) -> (i32, i32, i32, i32) {
    (0x28, h - 0x90, 0xa8, h - 0x10)
}

/// The text's scroll (+0x5c) at time `t` of the tirade: after 4 s
/// (0x00655758, a double), (t − 4) · 0.33333334 (0x00655740 = 4.0f,
/// 0x006556f8); the drawing moves the text up by min(scroll · 30
/// (0x00655760), text height − 100) pixels.
pub fn scroll_px(t: f32, text_h: i32, box_h: i32) -> i32 {
    let scroll = if f64::from(t) > 4.0 {
        (t - f32::from_bits(0x4080_0000)) * f32::from_bits(0x3eaa_aaab)
    } else {
        0.0
    };
    // @ 0x00433910: the text's height is at least the box's.
    let room = text_h.max(box_h) - box_h;
    #[allow(clippy::cast_precision_loss)]
    let px = scroll * f32::from_bits(0x41f0_0000);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    if px < room as f32 {
        px as i32
    } else {
        room
    }
}

/// The lip-sync analyser's window (0x00655700, the float 0.3).
pub const LIP_WINDOW: f32 = 0.3;

/// A sample louder than this (|s| > 0x12c) counts as voice
/// (0x0045ccaa).
const LIP_QUIET: i32 = 0x12c;

/// The lip-sync analyser of a 3D talker's voice (@ 0x0045cb40, the dialog
/// player's +0x60): the voice file loaded whole as a sample
/// (`FSOUND_Sample_Load(FSOUND_UNMANAGED, path, 0x30, 0, 0)`) and locked;
/// its sample frames walked with a clock stepping by 1 / its rate (the
/// float `1.0 / freq` added on the x87 stack), a counter + 1 for a loud
/// frame (a sample, or in stereo either channel, with |s| > 300) and − 1
/// for a quiet one; when the clock passes 0.3 a window is kept: its start
/// (0, then the last's + 0.3 as a float) and whether the counter is ≥ 0;
/// clock and counter start again. The frames after the last full window
/// make none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LipSync {
    /// (start, talking) per window.
    pub windows: Vec<(f32, bool)>,
}

impl LipSync {
    /// The windows of `samples` (16-bit, `channels` interleaved, `rate`
    /// frames a second). A voice that cannot be read gives none (the
    /// load fails: the list stays empty).
    pub fn analyse(samples: &[i16], channels: u8, rate: u32) -> LipSync {
        let mut windows: Vec<(f32, bool)> = Vec::new();
        if rate == 0 || !(1..=2).contains(&channels) {
            return LipSync { windows };
        }
        // FILD freq, FDIVR 1.0, FSTP float (0x0045cc6d..0x0045cc7c); the
        // clock stays on the x87 stack, at the CRT's 53-bit precision
        // [INFERRED: nothing in the program changes the FPU's control].
        #[allow(clippy::cast_possible_truncation)]
        let step = f64::from((1.0 / f64::from(rate)) as f32);
        let window = f64::from(LIP_WINDOW);
        let (mut clock, mut count) = (0.0f64, 0i32);
        for frame in samples.chunks_exact(usize::from(channels)) {
            clock += step;
            let loud = frame.iter().any(|&v| i32::from(v).abs() > LIP_QUIET);
            count += if loud { 1 } else { -1 };
            if clock > window {
                #[allow(clippy::cast_possible_truncation)]
                let start = windows
                    .last()
                    .map_or(0.0, |w| (f64::from(w.0) + window) as f32);
                windows.push((start, count >= 0));
                clock = 0.0;
                count = 0;
            }
        }
        LipSync { windows }
    }

    /// Whether the talker talks at `t` (@ 0x0045ce90): the first window
    /// whose start + 0.3 is not below `t` says; after the last, no.
    pub fn talking(&self, t: f32) -> bool {
        self.windows
            .iter()
            .find(|w| f64::from(w.0) + f64::from(LIP_WINDOW) >= f64::from(t))
            .is_some_and(|w| w.1)
    }
}

/// The dialog player (`CDialogPlayer`, its step @ 0x004329a0): +0x28 the
/// tirade, +0x2c its time (a float), +0x30 the state, +0x38 whether the
/// voice file exists, +0x39 the end asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: u32,
    pub tirade: usize,
    pub count: usize,
    pub time: f32,
    /// 0 start a tirade, 1 no picture, 2 a 2D talker, 3 a 3D talker, 4
    /// over.
    pub state: u32,
    pub has_voice: bool,
    pub end_all: bool,
    pub kind: Kind,
}

/// What a step of the player asks of the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Start tirade `k`: its voice (when the file exists), its bubble.
    Start(usize, Kind),
    /// Stop the voice of tirade `k` (@ 0x00459a00) and go on.
    Next(usize),
    /// The dialog is over (state 4, @ 0x00433885: the vtable's +0x10, the
    /// bubble deleted).
    Over,
}

impl Player {
    pub fn new(id: u32, count: usize) -> Player {
        Player {
            id,
            tirade: 0,
            count,
            time: 0.0,
            state: if count == 0 { 4 } else { 0 },
            has_voice: false,
            end_all: false,
            kind: Kind::None,
        }
    }

    /// One step: `click` is a press of the left button this frame (a
    /// mouse event of type 0 and button 0 whose state has bit 0,
    /// @ 0x00432a25..0x00432a3b), `space` a key event of code 0x39 (the
    /// end of the whole dialog: +0x39); `voice_playing` whether the voice
    /// of the tirade still plays (@ 0x00459be0). At its start a set +0x39
    /// gives state 4. States 1, 2, 3: the tirade ends on a click (or
    /// Space) or, with its voice, when the voice has stopped; then the
    /// next tirade or state 4. The time grows by `dt` after (as a float).
    pub fn step(&mut self, dt: f64, click: bool, space: bool, voice_playing: bool) -> Option<Act> {
        if self.end_all {
            self.state = 4;
            self.end_all = false;
        }
        let mut skip = click;
        if space {
            skip = true;
            self.end_all = true;
        }
        let act = match self.state {
            0 => {
                if self.end_all {
                    return None;
                }
                self.time = 0.0;
                Some(Act::Start(self.tirade, self.kind))
            }
            1..=3 => {
                if skip || (self.has_voice && !voice_playing) {
                    let k = self.tirade;
                    self.tirade += 1;
                    self.state = if self.tirade < self.count { 0 } else { 4 };
                    Some(Act::Next(k))
                } else {
                    None
                }
            }
            4 => return Some(Act::Over),
            _ => None,
        };
        if matches!(self.state, 1..=3) || matches!(act, Some(Act::Next(_))) {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.time = (dt + f64::from(self.time)) as f32;
            }
        }
        act
    }

    /// State 0's end: the kind of the tirade's talker gives the state.
    pub fn started(&mut self, kind: Kind, has_voice: bool) {
        self.kind = kind;
        self.has_voice = has_voice;
        self.state = kind.state();
    }
}

// ---------------------------------------------------------------------------
// The subject menu
// ---------------------------------------------------------------------------

/// The subject images' offsets from the bubble's hot spot for `n`
/// subjects (1..=8), the tables @ 0x00411ae8..0x00411ddd (x: 19, 99,
/// 179; y: 7, 84, 161, 238), each image 64 × 64 (0x40) [CONFIRMED-STATIC:
/// the stores simulated, evidence `m2/menu-dialog-elements.asm`].
pub fn subject_layout(n: usize) -> Vec<(i32, i32)> {
    const X: [[i32; 8]; 8] = [
        [19, 0, 0, 0, 0, 0, 0, 0],
        [19, 99, 0, 0, 0, 0, 0, 0],
        [19, 99, 179, 0, 0, 0, 0, 0],
        [19, 99, 99, 19, 0, 0, 0, 0],
        [19, 99, 179, 179, 99, 0, 0, 0],
        [19, 99, 99, 19, 19, 99, 0, 0],
        [19, 99, 99, 19, 19, 99, 99, 0],
        [19, 99, 99, 19, 19, 99, 99, 19],
    ];
    const Y: [[i32; 8]; 8] = [
        [7, 0, 0, 0, 0, 0, 0, 0],
        [7, 7, 0, 0, 0, 0, 0, 0],
        [7, 7, 7, 0, 0, 0, 0, 0],
        [7, 7, 84, 84, 0, 0, 0, 0],
        [7, 7, 7, 84, 84, 0, 0, 0],
        [7, 7, 84, 84, 161, 161, 0, 0],
        [7, 7, 84, 84, 161, 161, 238, 0],
        [7, 7, 84, 84, 161, 161, 238, 238],
    ];
    if n == 0 {
        return Vec::new();
    }
    let row = n.min(8) - 1;
    (0..n.min(8)).map(|i| (X[row][i], Y[row][i])).collect()
}

/// The subject menu's bubble file: `subject_bubble_<n>.spr` for 1..=8
/// subjects, `subject_bubble_8.spr` beyond (@ 0x00411f43..), under
/// `sprites2d/dialog/`; placed at (W · 3 / 5, H / 3) (unsigned
/// divisions).
pub fn subject_bubble(n: usize) -> String {
    if n == 0 || n > 8 {
        "subject_bubble_8.spr".into()
    } else {
        format!("subject_bubble_{n}.spr")
    }
}

/// The subject menu's states (`CMenu_Dialog` +0x14, @ 0x00410f90): 0 the
/// character turning to face (until its `facing` sprite's time reaches
/// its length, then `listen` in a loop and the elements made), 1 the
/// menu, 2 the elements made again (after a subject's script), 5 closing
/// (`unfacing`, the music back).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuState {
    Facing,
    Open,
    Rebuild,
    Closing,
}

// ---------------------------------------------------------------------------
// Film subtitles
// ---------------------------------------------------------------------------

/// The film's subtitles (`CKinematicsPlayer` +0x34 the list, +0x38 the
/// next one, +0x3c the text shown): after each film step
/// (@ 0x0042b0f0), t = `__ftol`(rate × frame / rate) with Bink's
/// `FrameNum` and `FrameRate` (`BinkGetRealtime` @ 0x00456cf0,
/// 0x00456d70): the subtitle's `start` and `end` are frame numbers. Once
/// t ≥ start (and no text is shown) its text is shown; once t ≥ end it is
/// taken off and the next subtitle waits.
#[derive(Clone, Debug, Default)]
pub struct FilmSubtitles {
    list: Vec<Subtitle>,
    next: usize,
    pub shown: Option<String>,
}

impl FilmSubtitles {
    pub fn new(list: Vec<Subtitle>) -> FilmSubtitles {
        FilmSubtitles {
            list,
            next: 0,
            shown: None,
        }
    }

    /// The step after the film's frame `frame_num` (1-based) at `rate`
    /// frames per second.
    pub fn step(&mut self, frame_num: u32, rate: u32) {
        let Some(s) = self.list.get(self.next) else {
            return;
        };
        let t = if rate == 0 {
            0
        } else {
            // FILD / FIDIV then FMUL by the float rate, `__ftol`.
            let x = f64::from(frame_num) / f64::from(rate);
            #[allow(clippy::cast_possible_truncation)]
            let t = (x * f64::from(rate as f32)) as i64;
            t
        };
        if i64::from(s.start) <= t && self.shown.is_none() {
            self.shown = Some(s.text.clone());
        }
        if i64::from(s.end) <= t {
            self.shown = None;
            self.next += 1;
        }
    }
}

/// The film subtitle's layout (@ 0x0042b220): `AtlFont_Subtitle`, margins
/// 0, centred (2), bottom (1), over the whole screen: drawn black, then
/// white 2 pixels up and left.
pub const SUBTITLE_ZONE: Zone = Zone::new(0, 0, 0, 0, 2, 1);

#[cfg(test)]
mod tests {
    use super::*;

    /// A `font.txt` with the inventory's and the credits' Lucida fonts,
    /// a Lucida Console and an Arial.
    fn lucida_fonts(system: &dyn Fn(&str) -> Option<Vec<u8>>) -> Fonts {
        let txt = "[Font]\r\nNameID\tAtlFont_Inventory\r\nName\tAtlantis.ttf\r\nType\t\"Lucida sans unicode\"\r\nHeight\t15\r\nWeight\tFW_NORMAL\r\nItalic\tfalse\r\n[EndFont]\r\n\
[Font]\r\nNameID\tAtlFont_Credit\r\nName\tAtlantis.ttf\r\nType\t\"Lucida sans unicode\"\r\nHeight\t15\r\nWeight\tFW_BOLD\r\nItalic\tfalse\r\n[EndFont]\r\n\
[Font]\r\nNameID\tAtlFont_Console\r\nName\tAtlantis.ttf\r\nType\t\"lucida console\"\r\nHeight\t12\r\nItalic\tfalse\r\n[EndFont]\r\n\
[Font]\r\nNameID\tAtlFont_Titre\r\nName\tAtlantis.ttf\r\nType\tArial\r\nHeight\t15\r\nItalic\tfalse\r\n[EndFont]\r\n";
        let mut m = atlantis_data::MemFiles::new();
        m.insert("datas/fonts/french/font.txt", txt.as_bytes().to_vec());
        let src: Arc<dyn FileSource> = Arc::new(m);
        Fonts::load_with(&src, "french", system)
    }

    #[test]
    fn the_systems_lucida_faces() {
        // Without them: Liberation Sans stands in, bold from its file.
        let f = lucida_fonts(&|_| None);
        assert_eq!(f.face("AtlFont_Inventory"), Face::Sans);
        assert_eq!(f.face("AtlFont_Credit"), Face::SansBold);
        assert_eq!(f.face("AtlFont_Console"), Face::Sans);
        assert!(!f.simulated_bold("AtlFont_Credit"));
        // With them (a monospaced file standing for both here: the
        // system's files are never shipped): those faces, from those
        // files; Arial keeps its twin.
        let asked = std::cell::RefCell::new(Vec::new());
        let f = lucida_fonts(&|name| {
            asked.borrow_mut().push(name.to_string());
            Some(FACES[2].to_vec())
        });
        assert_eq!(*asked.borrow(), ["l_10646.ttf", "lucon.ttf"]);
        assert_eq!(f.face("AtlFont_Inventory"), Face::LucidaSans);
        assert_eq!(f.face("AtlFont_Console"), Face::LucidaConsole);
        assert_eq!(f.face("AtlFont_Titre"), Face::Sans);
        let g = f.gdi("AtlFont_Inventory").unwrap();
        let adv = |g: &GdiFont, c: u8| {
            let (a, b, cc) = g.abc[usize::from(c)];
            a + b + cc
        };
        assert_eq!(adv(&g, b'i'), adv(&g, b'm'), "the given file's glyphs");
        // FW_BOLD of a face without a bold style: emboldened, one pixel
        // wider a character, drawn twice.
        assert!(f.simulated_bold("AtlFont_Credit"));
        assert!(!f.simulated_bold("AtlFont_Inventory"));
        let b = f.gdi("AtlFont_Credit").unwrap();
        for c in *b"aW " {
            assert_eq!(adv(&b, c), adv(&g, c) + 1);
        }
        let draw = |id: &str| {
            let lines = f.lines(id, "Atlantis", 200, 20, Zone::new(0, 0, 0, 0, 0, 0));
            let mut px = vec![0u32; 200 * 20];
            f.draw(id, &lines, 0x00ff_ffff, &mut px, 200, 20, 0, 0);
            px.iter().filter(|&&p| p & 0xff > 128).count()
        };
        assert!(draw("AtlFont_Credit") > draw("AtlFont_Inventory"));
    }

    /// The real files, when `ATLANTIS_SYSTEM_FONTS` names a folder holding
    /// them (a Windows fonts folder): they load and draw, their cell the
    /// `Height` (the hhea ascent − descent, equal to their OS/2 win
    /// ascent + descent, which GDI's positive `lfHeight` takes).
    #[test]
    fn the_real_lucida_files() {
        let Some(dir) = std::env::var_os("ATLANTIS_SYSTEM_FONTS") else {
            eprintln!("SKIPPED: ATLANTIS_SYSTEM_FONTS is not set");
            return;
        };
        let dirs = [std::path::PathBuf::from(dir)];
        let f = lucida_fonts(&|name| atlantis_data::system_fonts::font_in(&dirs, name));
        assert_eq!(f.face("AtlFont_Inventory"), Face::LucidaSans);
        assert_eq!(f.face("AtlFont_Console"), Face::LucidaConsole);
        for id in ["AtlFont_Inventory", "AtlFont_Credit", "AtlFont_Console"] {
            let g = f.gdi(id).unwrap();
            assert_eq!(g.tm_height, f.def(id).unwrap().height);
            let lines = f.lines(
                id,
                "Pi\u{e8}ce de monnaie",
                300,
                20,
                Zone::new(0, 0, 0, 0, 0, 0),
            );
            let mut px = vec![0u32; 300 * 20];
            f.draw(id, &lines, 0x00ff_ffff, &mut px, 300, 20, 0, 0);
            assert!(px.iter().filter(|&&p| p & 0xff > 128).count() > 50, "{id}");
        }
    }

    #[test]
    fn lip_sync_windows() {
        // 22050 Hz mono: a loud 0.5 s, a quiet 0.5 s, 4500 loud frames.
        let mut v = vec![1000i16; 11025];
        v.extend(vec![-300i16; 11025]);
        v.extend(vec![-301i16; 4500]);
        let l = LipSync::analyse(&v, 1, 22050);
        // Windows of 6616 frames (the float step 1 / 22050 summed until
        // above 0.3): four full ones in 26,550 frames.
        assert_eq!(l.windows.len(), 4);
        assert_eq!(l.windows[0], (0.0, true));
        // Window 1 holds 4409 loud and 2207 quiet frames; window 2 is all
        // quiet; window 3: 2202 quiet, 4414 loud.
        assert!(l.windows[1].1);
        assert!(!l.windows[2].1);
        assert!(l.windows[3].1);
        assert_eq!(l.windows[1].0, 0.3);
        assert_eq!(l.windows[2].0, 0.3f32 + 0.3f32);
        assert!(l.talking(0.0) && l.talking(0.3) && !l.talking(0.7));
        assert!(!l.talking(5.0));
        // Stereo: quiet only when both channels are.
        let st: Vec<i16> = (0..2 * 7000)
            .map(|i| if i % 2 == 0 { 0 } else { 400 })
            .collect();
        assert!(LipSync::analyse(&st, 2, 22050).windows[0].1);
        assert!(LipSync::analyse(&[], 1, 22050).windows.is_empty());
    }

    fn sub(start: i32, end: i32) -> Subtitle {
        Subtitle {
            start,
            end,
            text: format!("{start}"),
        }
    }

    #[test]
    fn each_type_and_weight_gets_its_twin() {
        assert_eq!(Face::of("arial", 700), Face::SansBold);
        assert_eq!(Face::of("Arial", 400), Face::Sans);
        assert_eq!(Face::of("Courier New", 400), Face::Mono);
        assert_eq!(Face::of("courier new", 700), Face::MonoBold);
        assert_eq!(Face::of("Lucida sans unicode", 700), Face::SansBold);
        assert!(Face::twin_of("ARIAL") && Face::twin_of("Courier New"));
        assert!(!Face::twin_of("Lucida sans unicode"));
    }

    fn fonts_of(txt: &[u8]) -> Fonts {
        let mut f = Fonts::load(&(Arc::new(NoFiles) as Arc<dyn FileSource>), "x");
        f.defs = FontTxt::parse_atlantis5(txt).fonts;
        f
    }

    struct NoFiles;
    impl FileSource for NoFiles {
        fn size(&self, _: &str) -> Option<u64> {
            None
        }
        fn read_at(&self, _: &str, _: u64, _: usize) -> Option<Vec<u8>> {
            None
        }
        fn has_dir(&self, _: &str) -> bool {
            false
        }
    }

    #[test]
    fn courier_new_is_drawn_monospace_and_bold_is_wider() {
        let f = fonts_of(
            b"[Font]\r\nNameID M\r\nType \"Courier New\"\r\nHeight 16\r\n[EndFont]\r\n\
[Font]\r\nNameID R\r\nType arial\r\nHeight 20\r\n[EndFont]\r\n\
[Font]\r\nNameID B\r\nType arial\r\nHeight 20\r\nWeight FW_BOLD\r\n[EndFont]\r\n",
        );
        let adv = |g: &GdiFont, b: u8| {
            let (a, bb, c) = g.abc[b as usize];
            a + bb + c
        };
        let m = f.gdi("M").unwrap();
        assert_eq!(adv(&m, b'i'), adv(&m, b'W'));
        let (r, b) = (f.gdi("R").unwrap(), f.gdi("B").unwrap());
        assert_ne!(adv(&r, b'i'), adv(&r, b'W'));
        let width = |g: &GdiFont| b"Atlantis".iter().map(|&c| adv(g, c)).sum::<i32>();
        assert!(width(&b) > width(&r));
    }

    #[test]
    fn film_subtitles_follow_frame_numbers() {
        let mut f = FilmSubtitles::new(vec![sub(2, 4), sub(4, 6)]);
        f.step(1, 25);
        assert_eq!(f.shown, None);
        f.step(2, 25);
        assert_eq!(f.shown.as_deref(), Some("2"));
        f.step(3, 25);
        assert_eq!(f.shown.as_deref(), Some("2"));
        // At 4 the first ends; the second starts at the next step.
        f.step(4, 25);
        assert_eq!(f.shown, None);
        f.step(5, 25);
        assert_eq!(f.shown.as_deref(), Some("4"));
        f.step(6, 25);
        assert_eq!(f.shown, None);
    }

    #[test]
    fn subject_layouts() {
        assert_eq!(subject_layout(1), vec![(19, 7)]);
        assert_eq!(
            subject_layout(4),
            vec![(19, 7), (99, 7), (99, 84), (19, 84)]
        );
        assert_eq!(subject_layout(8).len(), 8);
        assert_eq!(subject_layout(9).len(), 8);
        assert_eq!(subject_bubble(3), "subject_bubble_3.spr");
        assert_eq!(subject_bubble(9), "subject_bubble_8.spr");
    }

    #[test]
    fn the_player_steps_tirades() {
        let mut p = Player::new(7, 2);
        assert_eq!(
            p.step(0.1, false, false, false),
            Some(Act::Start(0, Kind::None))
        );
        p.started(Kind::ThreeD, true);
        assert_eq!(p.state, 3);
        // The voice plays: nothing.
        assert_eq!(p.step(0.1, false, false, true), None);
        // The voice ends: next.
        assert_eq!(p.step(0.1, false, false, false), Some(Act::Next(0)));
        assert_eq!(p.state, 0);
        assert!(matches!(
            p.step(0.1, false, false, false),
            Some(Act::Start(1, _))
        ));
        p.started(Kind::None, false);
        // No voice: only a click ends it.
        for _ in 0..100 {
            assert_eq!(p.step(0.1, false, false, false), None);
        }
        assert_eq!(p.step(0.1, true, false, false), Some(Act::Next(1)));
        assert_eq!(p.state, 4);
        assert_eq!(p.step(0.1, false, false, false), Some(Act::Over));
    }

    #[test]
    fn space_ends_the_dialog() {
        let mut p = Player::new(7, 3);
        let _ = p.step(0.1, false, false, false);
        p.started(Kind::TwoD, true);
        assert_eq!(p.step(0.1, false, true, true), Some(Act::Next(0)));
        // +0x39 set: the next step is state 4.
        assert_eq!(p.step(0.1, false, false, false), Some(Act::Over));
    }

    #[test]
    fn scroll() {
        assert_eq!(scroll_px(3.0, 300, 100), 0);
        assert_eq!(scroll_px(7.0, 300, 100), 30);
        assert_eq!(scroll_px(100.0, 300, 100), 200);
        assert_eq!(scroll_px(10.0, 80, 100), 0);
    }

    #[test]
    fn voice_names() {
        assert_eq!(
            voice_path("french", 110, 0),
            "datas/sounds/dialog/french/AT011001.ogg"
        );
    }
}
