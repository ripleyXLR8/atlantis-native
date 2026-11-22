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
//!   Native draws every face with Liberation Sans (the shipped font whose
//!   metrics are Arial's; Courier New and Lucida Sans Unicode have no free
//!   metric twin shipped), regular weight.
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

/// The shipped font Native draws `font.txt`'s faces with.
static LIBERATION_SANS: &[u8] = include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf");

/// `font.txt`'s fonts with their glyphs.
pub struct Fonts {
    defs: Vec<FontDef>,
    glyphs: Option<Glyphs>,
    font: Option<ab_glyph::FontVec>,
    cache: std::cell::RefCell<HashMap<i32, GdiFont>>,
}

impl Fonts {
    /// `fonts/<language>/font.txt` (`CTextManager` @ 0x004601f0).
    pub fn load(src: &Arc<dyn FileSource>, language: &str) -> Fonts {
        let defs = src
            .read(&format!("datas/fonts/{language}/font.txt"))
            .map(|d| FontTxt::parse_atlantis5(&d).fonts)
            .unwrap_or_default();
        Fonts {
            defs,
            glyphs: Glyphs::new(LIBERATION_SANS.to_vec()),
            font: ab_glyph::FontVec::try_from_vec(LIBERATION_SANS.to_vec()).ok(),
            cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// The font of `id` (the first with that id: @ 0x00461375).
    pub fn def(&self, id: &str) -> Option<&FontDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// The measures of the font `id` at its height.
    pub fn gdi(&self, id: &str) -> Option<GdiFont> {
        let h = self.def(id)?.height;
        let font = self.font.as_ref()?;
        Some(
            self.cache
                .borrow_mut()
                .entry(h)
                .or_insert_with(|| gdi_of(font, h))
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
        let (Some(g), Some(glyphs)) = (self.gdi(id), &self.glyphs) else {
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
/// and advance.
fn gdi_of(font: &ab_glyph::FontVec, height: i32) -> GdiFont {
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
        abc.push((a, bb, adv - a - bb));
    }
    #[allow(clippy::cast_possible_truncation)]
    let ascent = sf.ascent().round() as i32;
    GdiFont {
        height,
        tm_height: height,
        ascent,
        ave_char_width: if count > 0 { total / count } else { height / 2 },
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

    fn sub(start: i32, end: i32) -> Subtitle {
        Subtitle {
            start,
            end,
            text: format!("{start}"),
        }
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
