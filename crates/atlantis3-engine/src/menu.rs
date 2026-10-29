//! The menu pages (loop mode 6) built from `menu\desc\<page>.atl`, and the
//! save thumbnails.
//!
//! Every page is one interface (a vector of elements) loaded once at
//! start-up (@ 0x00451570) and kept for the whole session: the elements'
//! frames, clocks and hover flags carry from one visit to the next. The
//! menu frame (@ 0x004591a0) runs the handler of the page 0x00781120 (0
//! `i_debut` @ 0x00452000, 1 `i_princ` @ 0x004543e0, 2 `i_load`
//! @ 0x00452f30, 4 `i_outil` @ 0x00454830, 5 `i_option` @ 0x00454e30, 6
//! `i_langue` @ 0x004560e0, 8 `i_save` @ 0x00456f40), copies the page
//! surface (@ 0x0077fb00, kept between frames: a frame that draws nothing
//! shows the last one) to the screen, prints the tooltip text
//! (@ 0x00414820) and, on the load page, the saves' dates (@ 0x00454100).
//!
//! Each handler uses the same element machinery: the hover test
//! (@ 0x004261d0), the slider drag (@ 0x00426970, pages 2 and 4), the click
//! test (@ 0x00426470, on the button's release) and the element pass
//! (@ 0x00425bc0: a frame per element class, the tooltip box after 0.5 s).
//! Hit tests are `SPR_COLLISION` on the element's frame field (+0x220c),
//! not on the frame drawn. The sub-state 0x007812cc is 0 while a page
//! waits for the opening animation of `i_princ` (its covers, the yes / no
//! animation), 1 once it runs.
//!
//! The pages ask the game for what they do not own through [`Host`] (the
//! saves) and report their effects as [`Event`]s, in the order of the
//! original's calls; the game ([`crate::Game`]) carries them out.
//!
//! Comments cite atlantis3-gog-fr-3473408 (`Game.exe`).

use atlantis3_data::atl::{
    langue_layout, menu_sprite_path, parse_tooltips, tooltip_path, Element, ElementType, Layout,
};
use atlantis3_data::cfg::Config;
use atlantis3_data::save::game_save_name;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::spr_encode::{darken_black, ThumbEncoder};
use atlantis3_data::A3Files;

use crate::text::{layout, FontId, GdiFont, Line, Texts, Zone};

/// `__ftol` (@ 0x006d34bc): truncation.
fn ftol(v: f64) -> i32 {
    v as i32
}

/// One element, its bank and the state the machinery keeps in the
/// 0x2288-byte element.
pub struct Item {
    pub el: Element,
    pub bank: Option<Bank>,
    /// +0x220c: the current frame (the hit tests' sprite).
    pub frame: i32,
    /// +0x2218: an animation runs.
    pub playing: bool,
    /// +0x221b: hovered (set by @ 0x004261d0).
    pub hovered: bool,
    /// +0x221c: pressed (set by @ 0x00426470 / @ 0x00426970, cleared by
    /// @ 0x00426d70).
    pub pressed: bool,
    /// The animation clock: +0x2230 (start, f64), +0x2238 and +0x2240
    /// (f64), +0x222c (the f32 offset).
    t0: f64,
    acc: f64,
    acc2: f64,
    off: f32,
    /// +0x2248: when the hover began (the tooltip delay).
    hover_start: f64,
}

impl Item {
    fn new(el: Element, bank: Option<Bank>) -> Item {
        // The bank's load (@ 0x00425010 @ 0x0042535a..0x00425420): a
        // `SLIDER` starts at its last frame, and so does a `BT_LOCK`
        // neither selectable nor clickable (the active radio of a pair);
        // every other element at frame 0. (The code tests `BT_LOCK1` the
        // same way, but the original's run shows the languages page's
        // `LANGUE1` / `SOUSTITRE1` at frame 0 before their page sets
        // them: not reproduced for `BT_LOCK1`.)
        let last = match el.element_type() {
            ElementType::Slider => true,
            ElementType::BtLock => !el.selectable && !el.clickable,
            _ => false,
        };
        let frame = match (&bank, last) {
            (Some(b), true) => b.len() as i32 - 1,
            _ => 0,
        };
        Item {
            el,
            bank,
            frame,
            playing: false,
            hovered: false,
            pressed: false,
            t0: 0.0,
            acc: 0.0,
            acc2: 0.0,
            off: 0.0,
            hover_start: 0.0,
        }
    }

    fn kind(&self) -> ElementType {
        self.el.element_type()
    }

    /// +0x2214: the bank's number of sprites.
    fn count(&self) -> i32 {
        self.bank.as_ref().map_or(0, |b| b.len() as i32)
    }

    /// x (`None`: 0x80000000, the bank's default position) and y.
    fn xy(&self) -> (Option<i32>, i32) {
        if self.el.has_pos() {
            (Some(self.el.pos[0]), self.el.pos[1])
        } else {
            (None, self.el.pos[1])
        }
    }

    /// `SPR_COLLISION(bank, pointer, x, y, frame)` (@ 0x0069d130) on the
    /// frame field.
    fn opaque(&self, p: (i32, i32)) -> bool {
        let (x, y) = self.xy();
        self.frame >= 0
            && self
                .bank
                .as_ref()
                .is_some_and(|b| b.opaque_at(self.frame as usize, x, y, p.0, p.1))
    }

    /// The clock reset (@ 0x004266d7..0x00426717, @ 0x00425dc0): every
    /// field zeroed, the start at `now`, the offset `(now − start) + acc`.
    fn start_clock(&mut self, now: f64) {
        self.acc = 0.0;
        self.acc2 = 0.0;
        self.t0 = now;
        self.off = ((now - self.t0) + self.acc2) as f32;
    }

    /// The clock's frame (@ 0x00425d63..0x00425d80): `ftol(((now − start)
    /// + acc − offset) / frame time)`, the x87 at 53 bits (the CRT's
    /// control word 0x027f): f64 operations.
    fn clock(&self, now: f64) -> i32 {
        ftol((((now - self.t0) + self.acc2) - f64::from(self.off)) / f64::from(self.el.frame_time))
    }

    /// The end of a one-shot animation (@ 0x00425d85..0x00425dc0): frame
    /// 0, stopped, or started again with `loop`.
    fn step_animation(&mut self, now: f64) {
        let f = self.clock(now);
        self.frame = f;
        if f >= self.count() {
            self.frame = 0;
            self.playing = false;
            if self.el.looping {
                self.playing = true;
                self.start_clock(now);
            }
        }
    }
}

/// One sprite drawn: the bank's name (`nomspr`), the frame, and the
/// position (None: the default one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    pub bank: String,
    pub frame: i32,
    pub pos: Option<(i32, i32)>,
}

/// The input bytes the menus read: the click flag 0x007592b8 (set on press
/// and, in mode 6, on release @ 0x004316e9), the "held" byte 0x007592ba
/// (set on press, cleared on release) and the left button's bit of
/// 0x0078a81c.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub click: bool,
    pub held: bool,
    pub button: bool,
}

/// The tooltip box globals (@ 0x007559b0..0x007559bc) and its flag
/// (@ 0x007559cc), shared by every page and kept between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TipBox {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub shown: bool,
}

/// The box's colours (0x007559c8, 0x007559a0, 0x007559a8).
pub const TIP_FILL: u32 = 0xff4b_6074;
pub const TIP_LIGHT: u32 = 0xff94_b0c9;
pub const TIP_DARK: u32 = 0xff35_4859;

/// A shape drawn on the page surface: a rectangle fill (@ 0x004e8bd0, the
/// rectangle as top, bottom, right, left, inclusive, not clipped) or a line
/// (@ 0x004e9a00, inclusive, clipped to the surface); ARGB colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Fill {
        rect: [i32; 4],
        colour: u32,
    },
    Line {
        p0: (i32, i32),
        p1: (i32, i32),
        colour: u32,
    },
}

/// A text call: `AtlFont_SetLayout` (@ 0x0041e190) or one line's
/// `ExtTextOutA` of the print that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextCall {
    Layout(Zone),
    Draw { line: Line, font: FontId, rgb: u32 },
}

/// 0x004e8280 on the 565 page surface: each channel truncated.
fn argb565(c: u32) -> u16 {
    (((c >> 19) & 0x1f) << 11 | ((c >> 10) & 0x3f) << 5 | ((c >> 3) & 0x1f)) as u16
}

/// 0x004e8bd0 on a 16-bit surface: rows top..=bottom, columns
/// left..=right, written at `y × pitch + x` with no clipping (a box
/// clamped to the right edge writes its last column into the next row's
/// first pixel); pixels outside the buffer are skipped (the original
/// writes outside the surface).
fn fill(surf: &mut [u16], w: usize, rect: [i32; 4], colour: u32) {
    let c = argb565(colour);
    let [top, bottom, right, left] = rect;
    for y in top..=bottom {
        for x in left..=right {
            let i = i64::from(y) * w as i64 + i64::from(x);
            if i >= 0 && (i as usize) < surf.len() {
                surf[i as usize] = c;
            }
        }
    }
}

/// 0x004e9a00 for the box's lines (all horizontal or vertical): the clip
/// rectangle (0, 0) − (W − 1, H − 1) of @ 0x004e94e0 (both ends outside on
/// one side: nothing; else the outside end moved onto the edge), then
/// every pixel from p0 to p1 inclusive (@ 0x004ea900).
fn line(surf: &mut [u16], w: usize, h: usize, p0: (i32, i32), p1: (i32, i32), colour: u32) {
    let (wm, hm) = (w as i32 - 1, h as i32 - 1);
    let code = |(x, y): (i32, i32)| {
        let mut c = 0;
        if x < 0 {
            c |= 2;
        }
        if x > wm {
            c |= 1;
        }
        if y < 0 {
            c |= 4;
        }
        if y > hm {
            c |= 8;
        }
        c
    };
    if code(p0) & code(p1) != 0 {
        return;
    }
    let clamp = |(x, y): (i32, i32)| (x.clamp(0, wm), y.clamp(0, hm));
    let (a, b) = (clamp(p0), clamp(p1));
    let c = argb565(colour);
    for y in a.1.min(b.1)..=a.1.max(b.1) {
        for x in a.0.min(b.0)..=a.0.max(b.0) {
            surf[y as usize * w + x as usize] = c;
        }
    }
}

/// A page (one interface).
pub struct Page {
    pub name: String,
    pub items: Vec<Item>,
    /// +0x3c: the element whose tooltip shows (set by the element pass).
    pub tip_el: Option<usize>,
    /// +0x40: the tooltips are on (set by the page handlers).
    pub tooltips: bool,
}

/// The tooltip's context of an element pass.
pub struct TipCtx<'a> {
    pub font: Option<&'static GdiFont>,
    pub pointer: (i32, i32),
    pub tip: &'a mut TipBox,
    pub shapes: &'a mut Vec<Shape>,
    pub texts: &'a mut Vec<TextCall>,
}

impl Page {
    /// The page `name` (`i_debut`, `i_save`, ...) for a screen `w` x `h`,
    /// without its tooltips.
    pub fn load(files: &A3Files, name: &str, w: usize, h: usize) -> Option<Page> {
        Page::load_with(files, name, None, w, h)
    }

    /// The page `name` with the tooltip file `tooltips` (language, file
    /// name without `.txt`) applied (@ 0x00422970).
    pub fn load_with(
        files: &A3Files,
        name: &str,
        tooltips: Option<(&str, &str)>,
        w: usize,
        h: usize,
    ) -> Option<Page> {
        let path = atlantis3_data::atl::layout_path(name);
        let data = files.read(path.trim_start_matches("Datas/"))?;
        let mut layout = Layout::parse(&data, (w as i32, h as i32));
        if let Some((lang, t)) = tooltips {
            if let Some(d) = files.read(tooltip_path(lang, t).trim_start_matches("Datas/")) {
                layout.apply_tooltips(&parse_tooltips(&d));
            }
        }
        let items = layout
            .elements
            .into_iter()
            .map(|el| {
                let bank = el.nomspr.as_ref().and_then(|n| {
                    let p = menu_sprite_path(w as i32, n);
                    files
                        .read(p.trim_start_matches("Datas/"))
                        .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok())
                });
                Item::new(el, bank)
            })
            .collect();
        Some(Page {
            name: name.to_string(),
            items,
            tip_el: None,
            tooltips: false,
        })
    }

    /// The element named `nomref` (@ 0x00426db0: the first).
    pub fn find(&self, nomref: &[u8]) -> Option<usize> {
        self.items.iter().position(|it| it.el.nomref == nomref)
    }

    fn name_of(&self, k: usize) -> &[u8] {
        &self.items[k].el.nomref
    }

    /// @ 0x00426d70: every element's "pressed" byte cleared.
    pub fn clear_pressed(&mut self) {
        for it in &mut self.items {
            it.pressed = false;
        }
    }

    fn hoverable(it: &Item) -> bool {
        it.el.displayed && it.el.selectable && it.kind() != ElementType::BtLock
    }

    /// The hover test's answer at `pointer` without its effects (tests).
    pub fn hit(&self, pointer: (i32, i32)) -> Option<usize> {
        self.items
            .iter()
            .position(|it| Self::hoverable(it) && it.opaque(pointer))
    }

    /// The click test's element at `pointer` without its effects (tests:
    /// the first displayed and clickable element opaque there).
    pub fn click_hit(&self, pointer: (i32, i32)) -> Option<usize> {
        self.items
            .iter()
            .position(|it| it.el.displayed && it.el.clickable && it.opaque(pointer))
    }

    /// The element a click (on release) would hit: its `nomref` (tests).
    pub fn clicked(&self, pointer: (i32, i32)) -> Option<&[u8]> {
        self.click_hit(pointer).map(|k| self.name_of(k))
    }

    /// The hover test (@ 0x004261d0): the first element, in vector order,
    /// displayed, selectable, not a `BT_LOCK`, opaque at the pointer, is
    /// hovered (on its first hovered frame its hover clock starts; a
    /// `BT_ANIME2` not playing starts its animation clock, and on its first
    /// hovered frame the clock's offset is set so that it resumes from its
    /// frame); the elements after it are not hovered. A tested element
    /// that misses is not hovered; skipped elements before the hit keep
    /// their byte.
    pub fn hover(&mut self, p: (i32, i32), now: f64) -> Option<usize> {
        let n = self.items.len();
        for k in 0..n {
            let it = &mut self.items[k];
            if !Self::hoverable(it) {
                continue;
            }
            if !it.opaque(p) {
                it.hovered = false;
                continue;
            }
            if !it.hovered {
                it.hover_start = now;
            }
            if it.kind() == ElementType::BtAnime2 {
                if !it.playing {
                    // @ 0x004263a4..0x004263ea
                    it.acc = 0.0;
                    it.acc2 = 0.0;
                    it.t0 = now;
                    it.off = ((now - it.t0) + it.acc2) as f32;
                    it.playing = true;
                }
                if !it.hovered {
                    // @ 0x004263f9..0x00426418
                    it.off = (((now - it.t0) + it.acc2)
                        - f64::from(it.frame) * f64::from(it.el.frame_time))
                        as f32;
                }
            }
            it.hovered = true;
            for later in &mut self.items[k + 1..] {
                later.hovered = false;
            }
            return Some(k);
        }
        None
    }

    /// The click test (@ 0x00426470): only when the button was released
    /// (the click flag set, "held" clear). The first element, in vector
    /// order, displayed and clickable and opaque at the pointer is
    /// pressed; by type: `IMG_ANIME` / `BT_IMG_ANIME` at frame 0 start
    /// their clock, `BT_ANIME2` goes to frame 0, `BT_LOCK` toggles 0 ↔ 2,
    /// `BT_LOCK1` cycles; an element with a `link` (not `CLICK_TEXT`)
    /// becomes the active radio (frame 2, not selectable nor clickable)
    /// and its linked element the inactive one (frame 0, both on); its
    /// `soundOnClickUp` runs as `(PlaySound = v)`. The tested elements
    /// before it are not pressed.
    pub fn click(
        &mut self,
        input: Input,
        p: (i32, i32),
        now: f64,
        out: &mut Vec<Event>,
    ) -> Option<usize> {
        if input.held || !input.click {
            return None;
        }
        for k in 0..self.items.len() {
            let it = &mut self.items[k];
            if !(it.el.displayed && it.el.clickable) {
                continue;
            }
            if !it.opaque(p) {
                it.pressed = false;
                continue;
            }
            it.pressed = true;
            let kind = it.kind();
            match kind {
                ElementType::ImgAnime | ElementType::BtImgAnime => {
                    if it.frame == 0 {
                        it.start_clock(now);
                    }
                }
                ElementType::BtAnime2 => it.frame = 0,
                ElementType::BtLock => it.frame = if it.frame == 2 { 0 } else { 2 },
                ElementType::BtLock1 => {
                    it.frame = if it.frame == it.count() - 1 {
                        0
                    } else {
                        it.frame + 1
                    }
                }
                _ => {}
            }
            let link = it
                .el
                .link
                .clone()
                .filter(|l| !l.is_empty() && kind != ElementType::ClickText);
            let sound = it.el.sound_on_click_up.clone();
            if let Some(link) = link {
                // @ 0x004267be..0x004268b7
                it.frame = 2;
                it.el.selectable = false;
                it.el.clickable = false;
                if let Some(j) = self.find(&link) {
                    let o = &mut self.items[j];
                    o.frame = 0;
                    o.el.selectable = true;
                    o.el.clickable = true;
                }
            }
            if let Some(s) = sound.filter(|s| !s.is_empty()) {
                out.push(Event::Command(play_sound(&s)));
            }
            return Some(k);
        }
        None
    }

    /// The slider drag (@ 0x00426970): while the button is held, the first
    /// displayed, clickable element opaque at the pointer is pressed; an
    /// `IMG_ANIME` at frame 0 starts its clock; a `SLIDER`'s frame follows
    /// the pointer over its travel (`xmin`..`xmax`, or `ymin`..`ymax` when
    /// `xmin` or `xmax` is 0; 1024-pixel units truncated by 0.625 / 0.78125
    /// at 640 / 800): `(p − lo) × n / (hi − lo)` within 0..n − 1; its
    /// `soundOnClickDown` runs.
    pub fn drag(
        &mut self,
        input: Input,
        p: (i32, i32),
        width: usize,
        now: f64,
        out: &mut Vec<Event>,
    ) -> Option<usize> {
        if !input.held {
            return None;
        }
        for k in 0..self.items.len() {
            let it = &mut self.items[k];
            if !(it.el.displayed && it.el.clickable) {
                continue;
            }
            if !it.opaque(p) {
                it.pressed = false;
                continue;
            }
            it.pressed = true;
            match it.kind() {
                ElementType::ImgAnime => {
                    if it.frame == 0 {
                        it.start_clock(now);
                    }
                }
                ElementType::Slider => {
                    let (lo, hi, v) = if it.el.xmin == 0 || it.el.xmax == 0 {
                        (it.el.ymin, it.el.ymax, p.1)
                    } else {
                        (it.el.xmin, it.el.xmax, p.0)
                    };
                    // @ 0x00426b40..0x00426ba4: f32 factors (0x00700404).
                    let s = match width {
                        640 => 0.625f32,
                        800 => 0.78125,
                        _ => 1.0,
                    };
                    let lo = ftol(f64::from(lo) * f64::from(s));
                    let hi = ftol(f64::from(hi) * f64::from(s));
                    let n = it.count();
                    if hi != lo {
                        let f = (v - lo) * n / (hi - lo);
                        it.frame = f.min(n - 1).max(0);
                    }
                }
                _ => {}
            }
            if let Some(s) = it.el.sound_on_click_down.clone().filter(|s| !s.is_empty()) {
                out.push(Event::Command(play_sound(&s)));
            }
            return Some(k);
        }
        None
    }

    /// The element pass (@ 0x00425bc0): every element in vector order
    /// (`ZONE_TEXT` and `CLICK_TEXT` skipped; not displayed: stopped,
    /// nothing drawn; displayed without a bank: hidden) drawn by its class
    /// with `AFF_SPR` at its position on the page surface, then the
    /// tooltip: the first displayed, hovered element with a `bulle` becomes
    /// the page's tooltip element, and when the page's tooltips are on the
    /// box is set up (@ 0x00414330).
    #[allow(clippy::too_many_arguments)]
    pub fn pass(
        &mut self,
        surf: &mut [u16],
        w: usize,
        h: usize,
        now: f64,
        button: bool,
        mut tip: Option<TipCtx<'_>>,
    ) -> Vec<Draw> {
        self.tip_el = None;
        let mut out = Vec::new();
        for it in &mut self.items {
            let kind = it.kind();
            if matches!(kind, ElementType::ZoneText | ElementType::ClickText) {
                continue;
            }
            if !it.el.displayed {
                it.playing = false;
                continue;
            }
            if it.bank.is_none() {
                // "Il manque %s"
                it.el.displayed = false;
                continue;
            }
            let n = it.count();
            let drawn = match kind {
                ElementType::ImgAnime => {
                    if it.el.looping {
                        it.playing = true;
                    }
                    let f = it.frame;
                    if it.pressed {
                        it.playing = true;
                    }
                    if it.playing {
                        it.step_animation(now);
                    }
                    f
                }
                ElementType::BtImgAnime => {
                    if it.el.looping {
                        it.playing = true;
                    }
                    if it.hovered && !it.pressed && !it.playing {
                        1
                    } else {
                        let f = it.frame;
                        if it.pressed {
                            it.playing = true;
                        }
                        if it.playing {
                            it.step_animation(now);
                        }
                        f
                    }
                }
                ElementType::BtAnime2 => {
                    if it.hovered {
                        it.playing = true;
                        let f = it.frame;
                        // `cdq; idiv` (@ 0x00425f90): the remainder keeps
                        // the clock's sign.
                        let c = if n > 0 { it.clock(now) % n } else { 0 };
                        it.frame = if c == 0 { 1 } else { c };
                        f
                    } else {
                        0
                    }
                }
                ElementType::BtAnime => {
                    if it.hovered {
                        if button && it.el.clickable {
                            2
                        } else {
                            1
                        }
                    } else if !it.el.selectable && n >= 4 {
                        3
                    } else {
                        0
                    }
                }
                ElementType::BtLock => {
                    if it.pressed && button {
                        1
                    } else {
                        it.frame
                    }
                }
                _ => it.frame,
            };
            // @ 0x00414bd0: zoom 1 → AFF_SPR (no shipped element zooms).
            let (x, y) = it.xy();
            if let (Some(bank), Ok(fi)) = (&it.bank, usize::try_from(drawn)) {
                let mut t = Target::screen(surf, w, h);
                t.xmax = w as i32;
                t.ymax = h as i32;
                bank.draw(&mut t, x, y, fi);
            }
            out.push(Draw {
                bank: it
                    .el
                    .nomspr
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .unwrap_or_default(),
                frame: drawn,
                pos: x.map(|x| (x, y)),
            });
        }
        self.tip_el = self
            .items
            .iter()
            .position(|it| it.el.displayed && it.el.bulle && it.hovered);
        if let (true, Some(k), Some(ctx)) = (self.tooltips, self.tip_el, tip.as_mut()) {
            let it = &self.items[k];
            tip_box(
                surf,
                w,
                h,
                now - it.hover_start,
                it.el.tooltip.as_deref().unwrap_or_default(),
                ctx,
            );
        }
        out
    }

    /// The hover and the element pass without tooltips: the start page's
    /// frame without a click (tests).
    pub fn frame(
        &mut self,
        screen: &mut [u16],
        w: usize,
        h: usize,
        pointer: (i32, i32),
        now: f64,
    ) -> Vec<Draw> {
        self.hover(pointer, now);
        self.pass(screen, w, h, now, false, None)
    }
}

/// `(PlaySound = v)` (@ 0x00426945, format 0x00716dd0).
fn play_sound(v: &[u8]) -> Vec<u8> {
    let mut s = b"(PlaySound = ".to_vec();
    s.extend_from_slice(v);
    s.push(b')');
    s
}

/// The tooltip box (@ 0x00414330): nothing before 0.5 s of hover (the flag
/// cleared); else x0 = px + 50, y0 = py − 20, x1 = x0 + 20 × length,
/// y1 = py − 2, the text measured in `AtlFont_Bulle` with the layout (x0,
/// W − x1, y0, H − y1, left, bottom): the box is line 0's rectangle
/// (@ 0x006b41b0; the other lines' rectangles go to a local and change
/// nothing), clamped to the right edge only; the fill and the two-pixel
/// bevel on the page surface; the flag set.
fn tip_box(surf: &mut [u16], w: usize, h: usize, held: f64, text: &[u8], ctx: &mut TipCtx<'_>) {
    if held < 0.5 {
        ctx.tip.shown = false;
        return;
    }
    let (px, py) = ctx.pointer;
    let (wi, hi) = (w as i32, h as i32);
    let mut b = TipBox {
        x0: px + 50,
        y0: py - 20,
        x1: px + 50 + 20 * text.len() as i32,
        y1: py - 2,
        shown: false,
    };
    let zone = Zone::new(b.x0, wi - b.x1, b.y0, hi - b.y1, 0, 1);
    ctx.texts.push(TextCall::Layout(zone));
    if let Some(font) = ctx.font {
        if let Some(l0) = layout(font, text, wi, hi, zone).first() {
            b.x0 = l0.x;
            b.y0 = l0.y;
            b.x1 = l0.x + l0.dx.iter().sum::<i32>();
            b.y1 = l0.y + font.tm_height;
        }
    }
    if wi < b.x1 {
        b.x0 += wi - b.x1;
        b.x1 = wi;
    }
    let (x0, y0, x1, y1) = (b.x0, b.y0, b.x1, b.y1);
    let mut shapes = vec![Shape::Fill {
        rect: [y0, y1, x1, x0],
        colour: TIP_FILL,
    }];
    for (p0, p1, colour) in [
        ((x1, y0), (x1, y1 - 1), TIP_DARK),
        ((x0 - 2, y0 - 2), (x0 - 2, y1 + 1), TIP_LIGHT),
        ((x0 - 2, y0 - 2), (x1 + 2, y0 - 2), TIP_LIGHT),
        ((x0 - 1, y0 - 1), (x0 - 1, y1), TIP_LIGHT),
        ((x0 - 1, y0 - 1), (x1 + 1, y0 - 1), TIP_LIGHT),
        ((x0 - 2, y1 + 1), (x1 + 2, y1 + 1), TIP_DARK),
        ((x1 + 2, y1 + 1), (x1 + 2, y0 - 2), TIP_DARK),
        ((x0 - 1, y1), (x1 + 1, y1), TIP_DARK),
        ((x1 + 1, y1), (x1 + 1, y0 - 1), TIP_DARK),
    ] {
        shapes.push(Shape::Line { p0, p1, colour });
    }
    for s in &shapes {
        match *s {
            Shape::Fill { rect, colour } => fill(surf, w, rect, colour),
            Shape::Line { p0, p1, colour } => line(surf, w, h, p0, p1, colour),
        }
    }
    ctx.shapes.extend(shapes);
    b.shown = true;
    *ctx.tip = b;
}

/// What the pages ask of the game: the saves (names as `game1_3.sav`).
pub trait Host {
    /// The AtlFile existence test (@ 0x004191e0) of save `name`.
    fn exists(&mut self, name: &str) -> bool;
    /// A save's text.
    fn read(&mut self, name: &str) -> Option<Vec<u8>>;
    /// The thumbnail bank of save `name` for the menus `width` pixels wide
    /// (`menu\spr_<W>\32bits\save\<name>.spr`).
    fn thumbnail(&mut self, width: usize, name: &str) -> Option<Vec<u8>>;
}

/// An effect of a menu frame, in call order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A script command (`RunOneCommand` @ 0x00447840): the clicks'
    /// `(PlaySound = 500)`, the start page's `(chapter=666)`.
    Command(Vec<u8>),
    /// `Atlantis3.cfg` written (@ 0x00408f80), before the reaction to a
    /// click of `i_princ`.
    WriteCfg,
    /// An existence test (@ 0x004191e0) and its answer.
    Exists(String, bool),
    /// The start page's player n (@ 0x0045f0c0) and the resets of a player
    /// click (@ 0x00451190, @ 0x00459e70, @ 0x004c1e20, bag, subjects).
    Player(i32),
    /// The start page's empty slot: the new game `(chapter=600)`.
    NewGame,
    /// The flags and the bag reset then the loader (@ 0x0045b750).
    Load(String),
    /// The save writer (@ 0x00459ee0), thumbnails included.
    Save(String),
    /// Back to the game (@ 0x00451280, loop mode 3).
    Resume,
    /// `BT_QUIT` of `i_princ`: loop mode 6, the start page.
    StartPage,
    /// `BT_QUIT` of the start page (@ 0x00457690).
    Quit,
    /// A volume setter ran (@ 0x004341c0.. @ 0x00434220).
    Volumes,
    /// An option changed at once (smoothing, crosshair, mouse Y, subtitles,
    /// texture size).
    Options,
    /// `BT_BACK` of the options with a size or depth pending: the display
    /// re-created at that size (@ 0x0040fa60 and what follows).
    DisplayChange(i32, i32),
    /// `BT_BACK` of the languages: fonts, `Dial.txt`, `Objects.txt` and the
    /// tooltip files read again.
    ReloadTexts,
}

/// What a menu frame reads and changes outside the menus.
pub struct Env<'a> {
    pub files: &'a A3Files,
    /// The options and volumes (the globals they stand for).
    pub config: &'a mut Config,
    pub texts: Option<&'a Texts>,
    pub host: &'a mut dyn Host,
    /// The player (0x0071b890).
    pub player: i32,
    pub now: f64,
    pub pointer: (i32, i32),
}

impl Env<'_> {
    fn exists(&mut self, name: &str, out: &mut Vec<Event>) -> bool {
        let e = self.host.exists(name);
        out.push(Event::Exists(name.to_string(), e));
        e
    }
}

/// The page numbers (0x00781120).
pub const PAGE_START: u8 = 0;
pub const PAGE_MAIN: u8 = 1;
pub const PAGE_LOAD: u8 = 2;
pub const PAGE_VOLUMES: u8 = 4;
pub const PAGE_OPTIONS: u8 = 5;
pub const PAGE_LANGUAGES: u8 = 6;
pub const PAGE_SAVE: u8 = 8;

/// `...` (0x0071b4ac): an empty slot of the save list.
const EMPTY: &str = "...";

/// The menus: the pages, the page shown and its sub-state, the input
/// bytes, the tooltip box, the save list of the load page, the pending
/// display choices, the page surface, the thumbnail encoder and the
/// screen captured for the thumbnails.
pub struct Menus {
    pub width: usize,
    pub height: usize,
    pages: Vec<(u8, Page)>,
    /// 0x00781120, 0x007812cc.
    pub page: u8,
    pub sub: u8,
    pub input: Input,
    pub tip: TipBox,
    /// The page surface (@ 0x0077fb00).
    pub surface: Vec<u16>,
    /// The load page: the scroll 0x007812d0, the names (0x0077e650) and
    /// dates (0x0077fc98) of the list, the count 0x0077fa54.
    pub scroll: i32,
    pub count: usize,
    names: Vec<String>,
    dates: Vec<Vec<u8>>,
    /// The load page's layout (0x0071b1c8 date bottom, 0x0071b1cc date
    /// left, 0x0071b1d0 thumbnail y, 0x0071b1d4 thumbnail x, 0x0071b1d8
    /// date top, 0x0071b1dc row step).
    slots: [i32; 6],
    /// The pending screen size (0x0077e644, a 16-bit store: 0xffff none)
    /// and depth / texture size (0x00781098).
    pub res_pending: i32,
    pub depth_pending: bool,
    /// The start page's scripted demo runs (0x007812d4).
    pub demo: bool,
    /// The last frame's sprite calls (all passes, in order), shapes and
    /// text calls (tests).
    pub draws: Vec<Draw>,
    pub shapes: Vec<Shape>,
    pub texts: Vec<TextCall>,
    /// The thumbnail encoder, its palette history kept for the session
    /// (0x007d7be0).
    pub encoder: ThumbEncoder,
    /// The screen captured at Escape (@ 0x00410400): RGB565, width, height.
    pub capture: Option<(Vec<u16>, usize, usize)>,
    /// Where `Atlantis3.cfg` is written (none: not written) and the last
    /// bytes written.
    pub cfg_path: Option<std::path::PathBuf>,
    pub cfg_written: Option<Vec<u8>>,
}

impl Default for Menus {
    fn default() -> Self {
        Menus {
            width: 0,
            height: 0,
            pages: Vec::new(),
            page: PAGE_START,
            sub: 0,
            input: Input::default(),
            tip: TipBox::default(),
            surface: Vec::new(),
            scroll: 0,
            count: 0,
            names: vec![EMPTY.to_string(); 256],
            dates: vec![EMPTY.as_bytes().to_vec(); 256],
            slots: [0; 6],
            res_pending: 0,
            depth_pending: false,
            demo: false,
            draws: Vec::new(),
            shapes: Vec::new(),
            texts: Vec::new(),
            encoder: ThumbEncoder::new(),
            capture: None,
            cfg_path: None,
            cfg_written: None,
        }
    }
}

impl Menus {
    /// Whether the pages are loaded for a `w` x `h` screen.
    pub fn loaded(&self, w: usize, h: usize) -> bool {
        !self.pages.is_empty() && self.width == w && self.height == h
    }

    /// The nine interfaces' pages of the menus (@ 0x00451570) for a screen
    /// `w` x `h`, the tooltips in the language of `config` (`SousTitre`),
    /// the language page `i_langue_<Langue>` (or `_dvd`). The start page's
    /// tooltips are on.
    pub fn load(&mut self, files: &A3Files, w: usize, h: usize, config: &Config) {
        let lang = String::from_utf8_lossy(&config.sous_titre).into_owned();
        let langue =
            String::from_utf8_lossy(&langue_layout(&config.version, &config.langue)).into_owned();
        let list: [(u8, &str, Option<&str>); 7] = [
            (PAGE_START, "i_debut", Some("i_debut")),
            (PAGE_MAIN, "i_princ", Some("i_princ")),
            (PAGE_LOAD, "i_load", None),
            (PAGE_VOLUMES, "i_outil", Some("i_outil")),
            (PAGE_OPTIONS, "i_option", Some("i_option")),
            (PAGE_LANGUAGES, &langue, Some("i_langue")),
            (PAGE_SAVE, "i_save", Some("i_save")),
        ];
        self.pages = list
            .iter()
            .filter_map(|&(n, name, t)| {
                let p = Page::load_with(files, name, t.map(|t| (lang.as_str(), t)), w, h);
                if p.is_none() {
                    log::warn!("menu\\desc\\{name}.atl missing: page {n} not shown");
                }
                p.map(|p| (n, p))
            })
            .collect();
        if let Some(p) = self.get_mut(PAGE_START) {
            p.tooltips = true;
        }
        self.width = w;
        self.height = h;
        self.surface = vec![0; w * h];
        self.slots = slot_layout(w);
    }

    /// Page `n`.
    pub fn get(&self, n: u8) -> Option<&Page> {
        self.pages.iter().find(|(k, _)| *k == n).map(|(_, p)| p)
    }

    pub fn get_mut(&mut self, n: u8) -> Option<&mut Page> {
        self.pages.iter_mut().find(|(k, _)| *k == n).map(|(_, p)| p)
    }

    /// Element `name` of page `n`.
    fn item(&mut self, n: u8, name: &[u8]) -> Option<&mut Item> {
        let p = self.get_mut(n)?;
        let k = p.find(name)?;
        Some(&mut p.items[k])
    }

    /// The page put in 0x00781120 with sub-state 0 (the Escape handler,
    /// tests).
    pub fn open(&mut self, page: u8) {
        self.page = page;
        self.sub = 0;
    }

    /// The left button pressed (@ 0x004315a5): click flag and "held".
    pub fn press(&mut self) {
        self.input = Input {
            click: true,
            held: true,
            button: true,
        };
    }

    /// The left button released (@ 0x004316e9): "held" cleared and, in
    /// mode 6, the click flag set.
    pub fn release(&mut self) {
        self.input.held = false;
        self.input.button = false;
        self.input.click = true;
    }

    /// The input as the menu harness sets it before a frame (tests): the
    /// click flag set when the button is held or was released this frame,
    /// else left as the game left it.
    pub fn harness_input(&mut self, released: bool, held: bool) {
        if released || held {
            self.input.click = true;
        }
        self.input.held = held;
        self.input.button = held;
    }

    /// @ 0x00451560: the click flag and "held" cleared.
    fn clear_flags(&mut self) {
        self.input.click = false;
        self.input.held = false;
    }

    /// One menu frame (@ 0x004591a0) on `screen` (the pointer is the
    /// game's): the page's handler, the page surface copied to the screen,
    /// the tooltip text, the load page's dates. Returns the effects.
    pub fn frame(&mut self, env: &mut Env<'_>, screen: &mut [u16]) -> Vec<Event> {
        self.draws.clear();
        self.shapes.clear();
        self.texts.clear();
        let mut ev = Vec::new();
        if self.pages.is_empty() {
            return ev;
        }
        match self.page {
            PAGE_START => self.page_start(env, &mut ev),
            PAGE_MAIN => self.page_main(env, &mut ev),
            PAGE_LOAD => self.page_load(env, &mut ev),
            PAGE_VOLUMES => self.page_volumes(env, &mut ev),
            PAGE_OPTIONS => self.page_options(env, &mut ev),
            PAGE_LANGUAGES => self.page_languages(env, &mut ev),
            PAGE_SAVE => self.page_save(env, &mut ev),
            _ => {}
        }
        // The display re-created by the options' `BT_BACK` (@ 0x0040fa60,
        // then 0x004eb290): a new, black page surface.
        if ev.iter().any(|e| matches!(e, Event::DisplayChange(..))) {
            self.surface.fill(0);
        }
        // @ 0x00459250: the page surface to the screen (0x004ecc40).
        if screen.len() == self.surface.len() {
            screen.copy_from_slice(&self.surface);
        }
        // The tooltip text of the page's tooltip element when its tooltips
        // are on (@ 0x00414820).
        let page = self.page;
        let tip = self.get(page).and_then(|p| {
            p.tip_el
                .filter(|_| p.tooltips)
                .map(|k| p.items[k].el.tooltip.clone().unwrap_or_default())
        });
        if let Some(text) = tip {
            self.tip_text(env, screen, &text);
        }
        if page == PAGE_LOAD && self.sub == 1 {
            self.load_slots(env, screen);
        }
        ev
    }

    /// @ 0x00414820: when the box is set, the bulle printed white in
    /// `AtlFont_Bulle` with the layout (x0, W − x0 − x1, y0, H − y1, left,
    /// bottom) — the right margin takes x0 off once too often, harmless:
    /// the text is left-aligned.
    fn tip_text(&mut self, env: &Env<'_>, screen: &mut [u16], text: &[u8]) {
        if !self.tip.shown {
            return;
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let b = self.tip;
        let zone = Zone::new(b.x0, w - b.x0 - b.x1, b.y0, h - b.y1, 0, 1);
        self.print(env, screen, FontId::Bulle, text, zone, 0xff_ffff);
    }

    /// `AtlFont_SetLayout` + `AtlFont_Print` on the screen, the calls kept.
    fn print(
        &mut self,
        env: &Env<'_>,
        screen: &mut [u16],
        font: FontId,
        text: &[u8],
        zone: Zone,
        rgb: u32,
    ) {
        let (w, h) = (self.width, self.height);
        self.texts.push(TextCall::Layout(zone));
        let Some(t) = env.texts else { return };
        if let Some(g) = t.gdi(font, w) {
            for line in layout(g, text, w as i32, h as i32, zone) {
                self.texts.push(TextCall::Draw { line, font, rgb });
            }
        }
        t.print(screen, w, h, font, text, zone, rgb);
    }

    /// The bulle font at this width.
    fn bulle(&self, env: &Env<'_>) -> Option<&'static GdiFont> {
        env.texts.and_then(|t| t.gdi(FontId::Bulle, self.width))
    }

    /// Hover, click (and drag) then the element pass of page `n`.
    fn run(
        &mut self,
        n: u8,
        env: &Env<'_>,
        drag: bool,
        ev: &mut Vec<Event>,
    ) -> (Option<usize>, Option<usize>) {
        let font = self.bulle(env);
        let (w, h) = (self.width, self.height);
        let input = self.input;
        let mut surface = std::mem::take(&mut self.surface);
        let mut tip = self.tip;
        let mut shapes = Vec::new();
        let mut texts = Vec::new();
        let Some(p) = self.get_mut(n) else {
            self.surface = surface;
            return (None, None);
        };
        p.hover(env.pointer, env.now);
        let d = if drag {
            p.drag(input, env.pointer, w, env.now, ev)
        } else {
            None
        };
        let c = p.click(input, env.pointer, env.now, ev);
        let draws = p.pass(
            &mut surface,
            w,
            h,
            env.now,
            input.button,
            Some(TipCtx {
                font,
                pointer: env.pointer,
                tip: &mut tip,
                shapes: &mut shapes,
                texts: &mut texts,
            }),
        );
        self.surface = surface;
        self.tip = tip;
        self.shapes.extend(shapes);
        self.texts.extend(texts);
        self.draws.extend(draws);
        (d, c)
    }

    /// An element pass of page `n` alone (the opening animations, the
    /// in-game menu under the save confirmation).
    fn pass_only(&mut self, n: u8, env: &Env<'_>) {
        let font = self.bulle(env);
        let (w, h) = (self.width, self.height);
        let button = self.input.button;
        let mut surface = std::mem::take(&mut self.surface);
        let mut tip = self.tip;
        let mut shapes = Vec::new();
        let mut texts = Vec::new();
        if let Some(p) = self.get_mut(n) {
            let draws = p.pass(
                &mut surface,
                w,
                h,
                env.now,
                button,
                Some(TipCtx {
                    font,
                    pointer: env.pointer,
                    tip: &mut tip,
                    shapes: &mut shapes,
                    texts: &mut texts,
                }),
            );
            self.draws.extend(draws);
        }
        self.surface = surface;
        self.tip = tip;
        self.shapes.extend(shapes);
        self.texts.extend(texts);
    }

    fn name(&self, n: u8, k: Option<usize>) -> Option<Vec<u8>> {
        let p = self.get(n)?;
        Some(p.items.get(k?)?.el.nomref.clone())
    }

    fn clear(&mut self, n: u8) {
        if let Some(p) = self.get_mut(n) {
            p.clear_pressed();
        }
    }

    fn set_tooltips(&mut self, n: u8, on: bool) {
        if let Some(p) = self.get_mut(n) {
            p.tooltips = on;
        }
    }

    fn playing(&mut self, n: u8, name: &[u8]) -> bool {
        self.item(n, name).is_some_and(|it| it.playing)
    }

    /// Page 0, the start page (@ 0x00452000): hover, click, element pass;
    /// a click on player n: the player and the resets, then the newest of
    /// the run `game<n>_1.sav`, `_2`, … loaded (page 1 next time), or the
    /// new game `(chapter=600)` when `_1` does not exist; player 5 with
    /// `SuperCine` starts the demo `(chapter=666)`; back to the game.
    /// `BT_QUIT` quits. Once a player is set, `BT_LOAD`, `BT_SAVE` and
    /// `BT_BACK` of `i_princ` are on (@ 0x00452c85).
    fn page_start(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        let (_, c) = self.run(PAGE_START, env, false, ev);
        if self.sub != 0 {
            return;
        }
        let mut player = env.player;
        match self.name(PAGE_START, c) {
            None => {
                self.clear(PAGE_START);
                self.clear_flags();
            }
            Some(n) if n.starts_with(b"BT_LOAD") && n.len() == 8 => {
                let k = i32::from(n[7].wrapping_sub(b'0'));
                player = k;
                ev.push(Event::Player(k));
                if k == 5 && env.config.super_cine {
                    self.demo = true;
                    ev.push(Event::Command(b"(chapter=666)".to_vec()));
                } else {
                    let mut newest = None;
                    for s in 1.. {
                        let name = game_save_name(k, s);
                        if !env.exists(&name, ev) {
                            break;
                        }
                        newest = Some(name);
                    }
                    match newest {
                        None => ev.push(Event::NewGame),
                        Some(name) => {
                            ev.push(Event::Load(name));
                            self.page = PAGE_MAIN;
                            self.sub = 0;
                        }
                    }
                }
                ev.push(Event::Resume);
                self.clear(PAGE_START);
                self.clear_flags();
            }
            Some(n) if n == b"BT_QUIT" => ev.push(Event::Quit),
            Some(_) => {}
        }
        if player != 0 {
            for name in [&b"BT_LOAD"[..], b"BT_SAVE", b"BT_BACK"] {
                if let Some(it) = self.item(PAGE_MAIN, name) {
                    it.el.selectable = true;
                    it.el.clickable = true;
                }
            }
        }
    }

    /// Page 1, the in-game menu (@ 0x004543e0): tooltips on; hover, click,
    /// element pass; any click writes `Atlantis3.cfg` (@ 0x00408f80) then:
    /// `BT_LOAD` the save list and page 2, `BT_SAVE` the yes / no animation
    /// started and page 8, `BT_OUTIL` page 4, `BT_OPTION` page 5 (each in
    /// sub-state 0), `BT_BACK` back to the game, `BT_QUIT` the start page.
    /// The click flags are cleared every frame.
    fn page_main(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        self.set_tooltips(PAGE_MAIN, true);
        let (_, c) = self.run(PAGE_MAIN, env, false, ev);
        let Some(name) = self.name(PAGE_MAIN, c) else {
            self.clear(PAGE_MAIN);
            self.clear_flags();
            return;
        };
        ev.push(Event::WriteCfg);
        self.clear(PAGE_MAIN);
        match name.as_slice() {
            b"BT_LOAD" => {
                self.build_list(env, ev);
                self.page = PAGE_LOAD;
                self.sub = 0;
                self.clear(PAGE_MAIN);
            }
            b"BT_SAVE" => {
                let now = env.now;
                if let Some(it) = self.item(PAGE_MAIN, b"BT_CONFIRMATION") {
                    it.frame = 0;
                    it.start_clock(now);
                    it.playing = true;
                }
                self.page = PAGE_SAVE;
                self.sub = 0;
            }
            b"BT_OUTIL" => {
                self.page = PAGE_VOLUMES;
                self.sub = 0;
                self.clear(PAGE_MAIN);
            }
            b"BT_OPTION" => {
                self.page = PAGE_OPTIONS;
                self.sub = 0;
                self.clear(PAGE_MAIN);
            }
            b"BT_BACK" => {
                self.clear(PAGE_MAIN);
                self.clear_flags();
                ev.push(Event::Resume);
            }
            b"BT_QUIT" => {
                self.page = PAGE_START;
                self.sub = 0;
                self.clear(PAGE_MAIN);
                self.clear_flags();
                ev.push(Event::StartPage);
            }
            _ => {}
        }
        self.clear_flags();
    }

    /// The save list (@ 0x004539d0): the 256 slots emptied, then for N =
    /// 256 down to 1 every existing `game<P>_<N>.sav` added with its
    /// `DateAndTime` (newest first); the slots' thumbnails set up for the
    /// current scroll.
    fn build_list(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        self.names = vec![EMPTY.to_string(); 256];
        self.dates = vec![EMPTY.as_bytes().to_vec(); 256];
        self.count = 0;
        for n in (1..=256).rev() {
            let name = game_save_name(env.player, n);
            if env.exists(&name, ev) {
                self.dates[self.count] = env
                    .host
                    .read(&name)
                    .map(|d| date_of(&d))
                    .unwrap_or_default();
                self.names[self.count] = name;
                self.count += 1;
            }
        }
        self.set_thumbnails(env, false);
    }

    /// `SPR_SAVE1..6` (@ 0x00453eca.., @ 0x004541e3..): for a used slot,
    /// its bank `save\<name>` (loaded again when the name changes,
    /// @ 0x00425460) at (X, Y + used × step), shown; `hide` first hides
    /// every one (the per-frame pass).
    fn set_thumbnails(&mut self, env: &mut Env<'_>, hide: bool) {
        let [_, _, y0, x, _, step] = self.slots;
        let (w, files) = (self.width, env.files);
        let base = (5 * self.scroll).max(0) as usize;
        let names: Vec<String> = (0..6)
            .map(|k| {
                self.names
                    .get(base + k)
                    .cloned()
                    .unwrap_or_else(|| EMPTY.into())
            })
            .collect();
        let mut y = y0;
        for (k, name) in names.iter().enumerate() {
            let el = format!("SPR_SAVE{}", k + 1);
            let host = &mut *env.host;
            let Some(it) = self.item(PAGE_LOAD, el.as_bytes()) else {
                continue;
            };
            if hide {
                it.el.displayed = false;
            }
            if name == EMPTY {
                continue;
            }
            let spr = format!("save\\{name}").into_bytes();
            if it.el.nomspr.as_deref() != Some(&spr[..]) {
                it.bank = host
                    .thumbnail(w, name)
                    .or_else(|| {
                        let p = menu_sprite_path(w as i32, &spr);
                        files.read(p.trim_start_matches("Datas/"))
                    })
                    .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok());
                it.el.nomspr = Some(spr);
            }
            it.el.pos[0] = x;
            it.el.pos[1] = y;
            y += step;
            it.el.displayed = true;
        }
    }

    /// The dates and thumbnails of the six slots (@ 0x00454100, after the
    /// page is on the screen): each date (`...` for an empty slot) in
    /// `AtlFont_Default`, white, with the layout (left, 10, top, bottom −
    /// top − k × step, left, bottom) — the zone's bottom follows the screen
    /// height, the thumbnails do not; then the thumbnails set for the next
    /// frame.
    fn load_slots(&mut self, env: &mut Env<'_>, screen: &mut [u16]) {
        let [bottom, left, _, _, top, step] = self.slots;
        for k in 0..6 {
            let i = (5 * self.scroll + k).max(0) as usize;
            let date = self.dates.get(i).cloned().unwrap_or_else(|| EMPTY.into());
            let zone = Zone::new(left, 10, top, bottom - top - k * step, 0, 1);
            self.print(env, screen, FontId::Default, &date, zone, 0xff_ffff);
        }
        self.set_thumbnails(env, true);
    }

    /// Page 2, the load page (@ 0x00452f30). Sub-state 0: tooltips off;
    /// while `i_princ`'s `BT_LOAD` cover plays, `i_princ` alone is drawn;
    /// then sub-state 1, tooltips on, scroll 0, `SL_LOAD` at frame 0, the
    /// layout by screen width (nothing drawn this frame). Sub-state 1:
    /// hover, drag, click, element pass; a drag of `SL_LOAD` sets the
    /// scroll to its frame; `BT_BACK` → page 1; `SAVEk` loads `game<P>_<N>
    /// .sav`, N = count − 5 × scroll − (k − 1), when it exists.
    fn page_load(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            self.set_tooltips(PAGE_MAIN, false);
            self.set_tooltips(PAGE_LOAD, false);
            if self.playing(PAGE_MAIN, b"BT_LOAD") {
                self.pass_only(PAGE_MAIN, env);
                return;
            }
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_LOAD, true);
            self.scroll = 0;
            if let Some(it) = self.item(PAGE_LOAD, b"SL_LOAD") {
                it.frame = 0;
            }
            self.clear(PAGE_LOAD);
            self.clear_flags();
            self.slots = slot_layout(self.width);
            return;
        }
        let (d, c) = self.run(PAGE_LOAD, env, true, ev);
        if let Some(d) = d {
            if let Some(it) = self.get(PAGE_LOAD).map(|p| &p.items[d]) {
                if it.el.nomref == b"SL_LOAD" {
                    self.scroll = it.frame;
                }
            }
            return;
        }
        let Some(name) = self.name(PAGE_LOAD, c) else {
            self.clear(PAGE_LOAD);
            self.clear_flags();
            return;
        };
        if name == b"BT_BACK" {
            self.page = PAGE_MAIN;
            self.sub = 0;
            if let Some(p) = self.get_mut(PAGE_LOAD) {
                p.tip_el = None;
            }
        }
        for k in 1..=6 {
            if name != format!("SAVE{k}").as_bytes() {
                continue;
            }
            let n = self.count as i32 - 5 * self.scroll - (k - 1);
            let save = game_save_name(env.player, n);
            if env.exists(&save, ev) {
                ev.push(Event::Load(save));
                self.page = PAGE_MAIN;
                self.sub = 0;
                ev.push(Event::Resume);
            }
        }
        self.clear(PAGE_LOAD);
        self.clear_flags();
    }

    /// Page 4, the volumes (@ 0x00454830). Sub-state 0: tooltips off, each
    /// slider at volume / 5; while `i_princ`'s `BT_OUTIL` cover plays,
    /// `i_princ` alone is drawn; then sub-state 1, tooltips on (nothing
    /// drawn). Sub-state 1: hover, drag, click, element pass; a dragged
    /// slider sets its volume to frame × 5 (the setters clamp to 0..99);
    /// `BT_BACK` → page 1; another click leaves the click flag set (the
    /// next frame tests it again); no click clears `i_option`'s pressed
    /// bytes (sic, @ 0x00454b9e) and the flags.
    fn page_volumes(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        const SLIDERS: [&[u8]; 4] = [b"SL_VOICE", b"SL_SFX", b"SL_MUSIC", b"SL_MASTER"];
        if self.sub == 0 {
            self.set_tooltips(PAGE_MAIN, false);
            self.set_tooltips(PAGE_VOLUMES, false);
            let c = &*env.config;
            let vols = [
                c.voices_volume,
                c.sfx_volume,
                c.music_volume,
                c.global_volume,
            ];
            for (name, v) in SLIDERS.iter().zip(vols) {
                if let Some(it) = self.item(PAGE_VOLUMES, name) {
                    it.frame = v / 5;
                }
            }
            if self.playing(PAGE_MAIN, b"BT_OUTIL") {
                self.pass_only(PAGE_MAIN, env);
                return;
            }
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_VOLUMES, true);
            self.clear(PAGE_MAIN);
            self.clear_flags();
            return;
        }
        let (d, c) = self.run(PAGE_VOLUMES, env, true, ev);
        if let Some(d) = d {
            let (name, f) = {
                let it = &self.get(PAGE_VOLUMES).unwrap().items[d];
                (it.el.nomref.clone(), it.frame)
            };
            let v = (f * 5).clamp(0, 99);
            let cfg = &mut *env.config;
            match name.as_slice() {
                b"SL_VOICE" => cfg.voices_volume = v,
                b"SL_SFX" => cfg.sfx_volume = v,
                b"SL_MUSIC" => cfg.music_volume = v,
                b"SL_MASTER" => cfg.global_volume = v,
                _ => return,
            }
            ev.push(Event::Volumes);
            self.clear(PAGE_VOLUMES);
            return;
        }
        match self.name(PAGE_VOLUMES, c) {
            Some(n) if n == b"BT_BACK" => {
                self.page = PAGE_MAIN;
                self.sub = 0;
                if let Some(p) = self.get_mut(PAGE_VOLUMES) {
                    p.tip_el = None;
                }
                self.clear(PAGE_VOLUMES);
                self.clear_flags();
            }
            Some(_) => {}
            None => {
                self.clear(PAGE_OPTIONS);
                self.clear_flags();
            }
        }
    }

    /// Page 5, the options (@ 0x00454e30). Sub-state 0: tooltips off;
    /// while `i_princ`'s `BT_OPTION` cover plays, `i_princ` alone is drawn;
    /// then sub-state 1, tooltips on, nothing pending, and every button set
    /// from the state (the radios' active button at its last frame, not
    /// selectable nor clickable; the toggles at 2 when on; the screen size
    /// as 0 / 1 / 2; with `Version = CD` the texture button and its LED off
    /// and the warp at 512) (nothing drawn this frame). Sub-state 1: hover,
    /// click, element pass, then the clicked button's effect at once; the
    /// size and depth on `BT_BACK`.
    fn page_options(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            self.set_tooltips(PAGE_MAIN, false);
            self.set_tooltips(PAGE_OPTIONS, false);
            if self.playing(PAGE_MAIN, b"BT_OPTION") {
                self.pass_only(PAGE_MAIN, env);
                return;
            }
            self.sub = 1;
            self.set_tooltips(PAGE_MAIN, true);
            self.set_tooltips(PAGE_OPTIONS, true);
            self.clear(PAGE_MAIN);
            self.clear_flags();
            self.res_pending = 0xffff;
            let width = self.width;
            let cfg = &mut *env.config;
            let radio = |m: &mut Menus, on: &[u8], off: &[u8]| {
                if let Some(it) = m.item(PAGE_OPTIONS, on) {
                    it.frame = it.count() - 1;
                    it.el.selectable = false;
                    it.el.clickable = false;
                }
                if let Some(it) = m.item(PAGE_OPTIONS, off) {
                    it.frame = 0;
                    it.el.selectable = true;
                    it.el.clickable = true;
                }
            };
            if cfg.smooth {
                radio(self, b"BT_SMOOTH", b"BT_NOSMOOTH");
            } else {
                radio(self, b"BT_NOSMOOTH", b"BT_SMOOTH");
            }
            if cfg.pixel_format.bits() == 16 {
                radio(self, b"BT_16BITS", b"BT_32BITS");
            } else {
                radio(self, b"BT_32BITS", b"BT_16BITS");
            }
            if let Some(it) = self.item(PAGE_OPTIONS, b"BT_RESOLUTION") {
                match width {
                    640 => it.frame = 0,
                    800 => it.frame = 1,
                    1024 => it.frame = 2,
                    _ => {}
                }
            }
            self.depth_pending = false;
            let toggles = [
                (&b"BT_VISEUR"[..], cfg.cursor),
                (&b"BT_SOURIS"[..], cfg.invert_mouse_y),
                (&b"BT_TITRE"[..], cfg.subtitle_on),
                (&b"BT_TEXTURE"[..], cfg.warp_res != 512),
            ];
            for (name, on) in toggles {
                if let Some(it) = self.item(PAGE_OPTIONS, name) {
                    it.frame = if on { 2 } else { 0 };
                }
            }
            if cfg.version == b"CD" {
                if let Some(it) = self.item(PAGE_OPTIONS, b"LED_text") {
                    it.el.clickable = false;
                    it.el.selectable = false;
                }
                if let Some(it) = self.item(PAGE_OPTIONS, b"BT_TEXTURE") {
                    it.frame = 0;
                    it.el.clickable = false;
                    it.el.selectable = false;
                }
                cfg.warp_res = 512;
                if let Some(it) = self.item(PAGE_OPTIONS, b"BT_LANGUE") {
                    it.el.clickable = true;
                    it.el.selectable = true;
                }
            }
            return;
        }
        let (_, c) = self.run(PAGE_OPTIONS, env, false, ev);
        let Some(name) = self.name(PAGE_OPTIONS, c) else {
            self.clear(PAGE_OPTIONS);
            self.clear_flags();
            return;
        };
        let frame = c
            .and_then(|k| self.get(PAGE_OPTIONS).map(|p| p.items[k].frame))
            .unwrap_or(0);
        let cfg = &mut *env.config;
        match name.as_slice() {
            b"BT_BACK" => {
                self.page = PAGE_MAIN;
                self.sub = 0;
                if let Some(p) = self.get_mut(PAGE_OPTIONS) {
                    p.tip_el = None;
                }
                self.clear(PAGE_OPTIONS);
                self.clear_flags();
                if self.res_pending != 0xffff || self.depth_pending {
                    let (mut w, mut h) = (self.width as i32, self.height as i32);
                    if let Some(&(rw, rh)) =
                        atlantis3_data::cfg::RESOLUTIONS.get(self.res_pending as usize)
                    {
                        (w, h) = (rw, rh);
                    }
                    ev.push(Event::DisplayChange(w, h));
                    self.res_pending = 0xffff;
                    self.depth_pending = false;
                }
                return;
            }
            b"BT_RESOLUTION" => {
                let size = [640, 800, 1024];
                self.res_pending = if size.get(frame as usize) == Some(&(self.width as i32)) {
                    0xffff
                } else {
                    frame & 0xffff
                };
            }
            b"BT_LANGUE" => {
                self.page = PAGE_LANGUAGES;
                self.sub = 0;
            }
            b"BT_SMOOTH" | b"BT_NOSMOOTH" => {
                cfg.smooth = !cfg.smooth;
                ev.push(Event::Options);
            }
            b"BT_32BITS" | b"BT_16BITS" => self.depth_pending = true,
            b"BT_VISEUR" => {
                cfg.cursor = frame != 0;
                ev.push(Event::Options);
            }
            b"BT_SOURIS" => {
                cfg.invert_mouse_y = frame != 0;
                ev.push(Event::Options);
            }
            b"BT_TITRE" => {
                cfg.subtitle_on = frame != 0;
                ev.push(Event::Options);
            }
            b"BT_TEXTURE" => {
                cfg.warp_res = if frame == 0 { 512 } else { 1024 };
                self.depth_pending = true;
                ev.push(Event::Options);
            }
            _ => {}
        }
        self.clear(PAGE_OPTIONS);
        self.clear_flags();
    }

    /// Page 6, the languages (@ 0x004560e0). Sub-state 0: while
    /// `i_option`'s `BT_LANGUE` plays (a `BT_ANIME`: never) `i_option` is
    /// drawn; then sub-state 1 and the lists set up: with one voice and one
    /// subtitle language installed both LEDs at 1 and the four arrows off
    /// (the list functions @ 0x0044d7c0, @ 0x0044d9f0, @ 0x0044dfd0 are not
    /// transcribed: the port has only the installed language) (nothing
    /// drawn). Sub-state 1: hover, click, element pass; no click: both LEDs
    /// at 0; `BT_BACK`: the texts read again, page 5 in sub-state 1.
    fn page_languages(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        if self.sub == 0 {
            if self.playing(PAGE_OPTIONS, b"BT_LANGUE") {
                self.pass_only(PAGE_OPTIONS, env);
                return;
            }
            self.sub = 1;
            for name in [&b"LED_LG"[..], b"LED_ST"] {
                if let Some(it) = self.item(PAGE_LANGUAGES, name) {
                    it.frame = 1;
                }
            }
            self.clear(PAGE_LANGUAGES);
            self.clear_flags();
            for name in [&b"BT_LG_UP"[..], b"BT_LG_DN", b"BT_ST_UP", b"BT_ST_DN"] {
                if let Some(it) = self.item(PAGE_LANGUAGES, name) {
                    it.el.selectable = false;
                    it.el.clickable = false;
                }
            }
            self.set_tooltips(PAGE_LANGUAGES, true);
            return;
        }
        let (_, c) = self.run(PAGE_LANGUAGES, env, false, ev);
        match self.name(PAGE_LANGUAGES, c) {
            None => {
                for name in [&b"LED_LG"[..], b"LED_ST"] {
                    if let Some(it) = self.item(PAGE_LANGUAGES, name) {
                        it.frame = 0;
                    }
                }
                self.clear(PAGE_LANGUAGES);
                self.clear_flags();
            }
            Some(n) if n == b"BT_BACK" => {
                ev.push(Event::ReloadTexts);
                self.page = PAGE_OPTIONS;
                self.sub = 1;
                if let Some(p) = self.get_mut(PAGE_LANGUAGES) {
                    p.tip_el = None;
                }
                self.clear(PAGE_LANGUAGES);
                self.clear_flags();
            }
            Some(_) => {
                self.clear(PAGE_LANGUAGES);
                self.clear_flags();
            }
        }
    }

    /// Page 8, the save confirmation (@ 0x00456f40). Sub-state 0:
    /// `BT_CONFIRMATION` (`yes_no` of `i_princ`) shown; while it plays,
    /// `i_princ` alone is drawn; then sub-state 1, held at its last frame,
    /// `i_save`'s tooltips on (nothing drawn). Sub-state 1: hover and click
    /// on `i_save`, the element passes of `i_princ` then `i_save`; `BT_YES`
    /// writes the next free `game<P>_<N>.sav` (@ 0x0045e300), both buttons
    /// return to page 1 and put the animation back at frame 0. The
    /// animation is hidden again at the end of every frame.
    fn page_save(&mut self, env: &mut Env<'_>, ev: &mut Vec<Event>) {
        let show = |m: &mut Menus, on: bool| {
            if let Some(it) = m.item(PAGE_MAIN, b"BT_CONFIRMATION") {
                it.el.displayed = on;
            }
        };
        if self.sub == 0 {
            self.set_tooltips(PAGE_MAIN, false);
            self.set_tooltips(PAGE_SAVE, false);
            show(self, true);
            if self.playing(PAGE_MAIN, b"BT_CONFIRMATION") {
                self.pass_only(PAGE_MAIN, env);
            } else {
                self.sub = 1;
                if let Some(it) = self.item(PAGE_MAIN, b"BT_CONFIRMATION") {
                    it.playing = false;
                    it.frame = it.count() - 1;
                }
                self.clear(PAGE_MAIN);
                self.clear_flags();
                self.set_tooltips(PAGE_MAIN, false);
                self.set_tooltips(PAGE_SAVE, true);
            }
            show(self, false);
            return;
        }
        show(self, true);
        let input = self.input;
        let c = match self.get_mut(PAGE_SAVE) {
            Some(p) => {
                p.hover(env.pointer, env.now);
                p.click(input, env.pointer, env.now, ev)
            }
            None => None,
        };
        self.pass_only(PAGE_MAIN, env);
        self.pass_only(PAGE_SAVE, env);
        if let Some(name) = self.name(PAGE_SAVE, c) {
            if name == b"BT_YES" {
                let player = env.player;
                let save = (1..)
                    .map(|n| game_save_name(player, n))
                    .find(|s| !env.exists(s, ev))
                    .unwrap_or_default();
                ev.push(Event::Save(save));
            }
            if name == b"BT_YES" || name == b"BT_NO" {
                self.page = PAGE_MAIN;
                self.sub = 0;
                self.clear(PAGE_SAVE);
                self.clear_flags();
                self.set_tooltips(PAGE_MAIN, true);
                self.set_tooltips(PAGE_SAVE, true);
            }
            if let Some(it) = self.item(PAGE_MAIN, b"BT_CONFIRMATION") {
                it.frame = 0;
            }
        }
        self.clear(PAGE_SAVE);
        self.clear_flags();
        show(self, false);
    }

    /// Opens the load page as `BT_LOAD` of the in-game menu does (the save
    /// list built, page 2 in sub-state 0).
    pub fn open_load(&mut self, env: &mut Env<'_>) -> Vec<Event> {
        let mut ev = Vec::new();
        self.build_list(env, &mut ev);
        self.page = PAGE_LOAD;
        self.sub = 0;
        ev
    }

    /// The save list's names and dates (the load page's slots), newest
    /// first.
    pub fn list(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.names[..self.count]
            .iter()
            .map(String::as_str)
            .zip(self.dates.iter().map(Vec::as_slice))
    }
}

/// The load page's layout by screen width (@ 0x004537ef..0x0045393a): the
/// 1024 values (bottom 540, left 436, thumbnail y 200, x 240, top 20, step
/// 90) truncated after × 0.625 or × 0.78125 at 640 and 800. Order: date
/// bottom, date left, thumbnail y, thumbnail x, date top, step.
fn slot_layout(width: usize) -> [i32; 6] {
    let v = [540, 436, 200, 240, 20, 90];
    let s = match width {
        640 => 0.625f32,
        800 => 0.78125,
        _ => 1.0,
    };
    v.map(|x| ftol(f64::from(x) * f64::from(s)))
}

/// A save's `DateAndTime` (@ 0x0045eab0): the text two bytes after the
/// `=` of the line the loader reads it from.
fn date_of(save: &[u8]) -> Vec<u8> {
    atlantis3_data::save::SaveGame::parse(save).date_and_time
}

/// The stretch model of the thumbnails' `Blt` (see [`thumbnails`]):
/// nearest neighbour, top-left aligned, `src = dst × W / w` in integers.
pub fn stretch(src: &[u16], w: usize, h: usize, tw: usize, th: usize) -> Vec<u16> {
    (0..tw * th)
        .map(|i| {
            let (x, y) = (i % tw, i / tw);
            src[(y * h / th) * w + x * w / tw]
        })
        .collect()
}

/// The software copy RGB565 → x8r8g8b8 (@ 0x004ecc40): each channel
/// shifted to the top of its byte (no replication), x = 0.
pub fn to_x888(src: &[u16]) -> Vec<u32> {
    src.iter()
        .map(|&p| {
            let p = u32::from(p);
            ((p >> 11) & 0x1f) << 19 | ((p >> 5) & 0x3f) << 10 | (p & 0x1f) << 3
        })
        .collect()
}

/// The thumbnail sizes (@ 0x0040edb0) in the writer's order, with the menu
/// width each is for.
pub const THUMBNAILS: [(usize, usize, usize); 3] = [(1024, 85, 65), (800, 65, 50), (640, 52, 40)];

/// The thumbnails of a captured screen (the writer @ 0x0045b089 →
/// @ 0x00410500), each `SPRCRYO1` + the encoder's output, for the menu
/// widths 1024, 800, 640.
///
/// For each size the original asks DirectDraw to `Blt` the whole capture
/// onto the whole thumbnail surface (no rectangles, no DDBLTFX: the
/// driver's stretch, not in the executable). Reproduced here is the model
/// the RE pinned (DirectDraw's emulation layer): nearest neighbour,
/// top-left aligned, integers, `src_x = dst_x × W / w`, `src_y = dst_y ×
/// H / h` on the RGB565 pixels — no filter, no aspect correction. Then
/// the software copy to x8r8g8b8 (@ 0x004ecc40: each channel shifted into
/// its byte, no replication of the high bits, x = 0), the pre-pass
/// @ 0x00410450 ([`darken_black`]) and the encoder @ 0x0069c390 with the
/// session's palette history (`encoder`), the pitch the width, and zero
/// bytes read past the pixels (the memory after a DirectDraw surface is
/// unknown).
pub fn thumbnails(
    capture: &[u16],
    w: usize,
    h: usize,
    encoder: &mut ThumbEncoder,
) -> Vec<(usize, Vec<u8>)> {
    THUMBNAILS
        .iter()
        .map(|&(menu, tw, th)| {
            let mut px = to_x888(&stretch(capture, w, h, tw, th));
            darken_black(&mut px);
            let mut bank = atlantis3_data::save::THUMB_MAGIC.to_vec();
            bank.extend(encoder.encode(&px, tw, th, tw, &[]));
            (menu, bank)
        })
        .collect()
}
