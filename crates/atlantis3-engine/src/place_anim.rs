//! The place animations: one slot per entity state of the current view
//! (`AtlSpw_load` @ 0x00438530, at most 64), drawn into the cube faces
//! every warp frame (@ 0x00440bf0 → @ 0x00439130 → @ 0x00439920, `AFF_SPW`
//! per face) and stepped from the clock once per 15 Hz tick
//! (@ 0x00439c90).
//!
//! The original draws each face's sprites clipped to the part of the face
//! the screen shows (@ 0x00440cc0 → @ 0x004fe5b0) into the faces, then
//! restores the pixels after the cube is drawn; here the sprites are drawn
//! whole into copies of the faces. The two differ only where the bilinear
//! filter reads a texel just outside the visible part (one pixel in one
//! frame of 180 measured).

use atlantis2_data::wam::{EntityState, Wam};
use atlantis2_engine::crt::Rand;
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis3_data::A3Files;
use std::collections::HashMap;
use std::sync::Arc;

/// `AtlSpw_load : n>MAX_SPW_VISIBLE` (0x3f).
pub const MAX_SLOTS: usize = 0x40;

/// A slot (0x490 bytes @ 0x007594cc + 0x490 k).
pub struct Slot {
    /// The SPW file's base name.
    pub name: String,
    /// −8: the entity's kind (WAM entity +0x38); −4: its `defnames.con`
    /// index (+0x14).
    pub kind: i32,
    pub id: i32,
    bank: Arc<Bank>,
    /// +0x08: the frame drawn (−1: none, the play ended).
    pub frame: i32,
    /// +0x0c: the last frame: the state's end − 1, or the frame count.
    pub last: i32,
    /// +0x10: the file's frame count.
    pub count: i32,
    /// +0x14: the first frame (state +0x20).
    pub first: i32,
    /// +0x18: the click kind (state +0x18).
    pub click: i32,
    /// +0x1c: the state record.
    pub state: EntityState,
    /// +0x28: shown.
    pub shown: bool,
    /// +0x2c: 1 = the frame +0x08, else the frame list.
    pub mode: i32,
    /// +0x438: drawn on odd 15 Hz ticks only.
    pub blink: bool,
    /// +0x43c: the countdown of the random pause after a play.
    pub pause: i32,
    /// +0x440: the pause runs (the clock does not move the frame).
    pub waiting: bool,
    /// +0x44c: the time the play started (seconds): 0 after the load.
    pub start: f64,
    /// +0x46c: a play is to start again at once (1 at load, 0 after the
    /// first step).
    pub fresh: bool,
    /// +0x434: animated (`ANIM_THE_FRAME=NAME,0` freezes the frame).
    pub animated: bool,
    /// +0x444: the hand-over's current weight (the state's, 100 for 0;
    /// +10 for every draw that does not pick it).
    pub weight: i32,
    /// +0x20, +0x24: a successor queued, and its slot.
    pub queued: bool,
    pub next: Option<usize>,
    /// +0x46d: the play reached its end.
    pub ended: bool,
    /// Not played by the clock at the view entry (mode 0): its plays are
    /// started by C code (a lid, a door...).
    pub driven: bool,
}

/// The view's slots.
#[derive(Default)]
pub struct Slots {
    pub slots: Vec<Slot>,
}

impl Slots {
    /// The slots of view `view` (the view entry's loop, @ 0x0043f840 →
    /// @ 0x00438530): every state of every entity shown in it, in file
    /// order; `shown` from the state, then from the script's show table
    /// (`shown(id, kind)`: Some(true / false)), and hidden when an earlier
    /// slot of the same entity is shown.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        files: &A3Files,
        cache: &mut HashMap<String, Option<Arc<Bank>>>,
        wam: &Wam,
        view: u32,
        world: &str,
        rand: &mut Rand,
        now: f64,
        shown: impl Fn(i32, i32) -> Option<bool>,
    ) -> Slots {
        Slots::load_with(
            files,
            cache,
            wam,
            view,
            world,
            &mut || rand.rand(),
            now,
            shown,
        )
    }

    /// [`Slots::load`] with any source of the CRT's `rand()` values.
    #[allow(clippy::too_many_arguments)]
    pub fn load_with(
        files: &A3Files,
        cache: &mut HashMap<String, Option<Arc<Bank>>>,
        wam: &Wam,
        view: u32,
        world: &str,
        rand: &mut dyn FnMut() -> i32,
        _now: f64,
        shown: impl Fn(i32, i32) -> Option<bool>,
    ) -> Slots {
        let mut out: Vec<Slot> = Vec::new();
        for e in &wam.entities {
            for s in e.state_records().filter(|s| s.view == view) {
                if out.len() >= MAX_SLOTS - 1 {
                    log::error!("AtlSpw_load : n>MAX_SPW_VISIBLE");
                    return Slots { slots: out };
                }
                let name = e.spw_name(&s);
                let path = format!("spw/spw512/{world}/{name}");
                let bank = cache
                    .entry(path.clone())
                    .or_insert_with(|| {
                        files
                            .read(&path)
                            .and_then(|d| Bank::parse(&d, Depth::Rgb565).ok())
                            .map(Arc::new)
                    })
                    .clone();
                let Some(bank) = bank else {
                    log::warn!("Can't load SPW {path}");
                    continue;
                };
                let count = bank.len() as i32;
                let kind = e.kind as i32;
                let id = e.index as i32;
                let pause = if s.delay == 0 {
                    0
                } else {
                    rand() % s.delay.unsigned_abs() as i32 + 10
                };
                let mut slot_shown = s.shown != 0;
                if let Some(v) = shown(id, kind) {
                    slot_shown = v;
                    if v && out.iter().any(|o| o.id == id && o.kind == kind && o.shown) {
                        slot_shown = false;
                    }
                }
                out.push(Slot {
                    name: name.trim_end_matches(".SPW").to_string(),
                    kind,
                    id,
                    bank,
                    frame: s.first,
                    last: if s.end == 0 { count } else { s.end - 1 },
                    count,
                    first: s.first,
                    click: s.click,
                    state: s,
                    shown: slot_shown,
                    mode: s.single,
                    driven: s.single == 0,
                    blink: false,
                    pause,
                    waiting: false,
                    // The loader leaves the start at 0: the first tick
                    // finds every shown slot past its end.
                    start: 0.0,
                    fresh: true,
                    animated: true,
                    weight: if s.weight == 0 { 100 } else { s.weight },
                    // Next-state chains (@ 0x00462c30): no Hoggar state
                    // has one; not resolved here.
                    queued: false,
                    next: None,
                    ended: false,
                });
            }
        }
        Slots { slots: out }
    }

    /// `ShowPerso` / `ShowObjet` (@ 0x0045f520): the first slot of (id,
    /// kind) whose animation is `number` (any for 0) shown, fresh, not
    /// ended, its clock restarted.
    pub fn show(&mut self, id: i32, kind: i32, number: i32, now: f64) {
        if let Some(s) = self.slots.iter_mut().find(|s| {
            s.id == id && s.kind == kind && (number == 0 || s.state.anim as i32 == number)
        }) {
            s.shown = true;
            s.fresh = true;
            s.ended = false;
            s.start = now;
        }
    }

    /// `HidePerso` / `HideObjet` without `,x` (@ 0x0045f610): every slot of
    /// (id, kind) hidden, at frame 0, its clock restarted.
    pub fn hide(&mut self, id: i32, kind: i32, now: f64) {
        for s in self
            .slots
            .iter_mut()
            .filter(|s| s.id == id && s.kind == kind)
        {
            s.shown = false;
            s.frame = 0;
            s.start = now;
        }
    }

    /// `PLAYANIMP` / `PLAYANIMO` (@ 0x0045fa90 with 2): the first slot j of
    /// (id, kind) playing animation `number` queued after the first shown
    /// slot k of (id, kind), which plays on to its file's end (k = j:
    /// restart). Nothing without j or k.
    pub fn play(&mut self, id: i32, kind: i32, number: i32) {
        let Some(j) = self
            .slots
            .iter()
            .position(|s| s.id == id && s.kind == kind && s.state.anim as i32 == number)
        else {
            return;
        };
        let Some(k) = self
            .slots
            .iter()
            .position(|s| s.id == id && s.kind == kind && s.shown)
        else {
            return;
        };
        let sk = &mut self.slots[k];
        sk.last = sk.count;
        sk.queued = true;
        sk.next = Some(j);
        let sj = &mut self.slots[j];
        sj.queued = false;
        sj.frame = 0;
        sj.last = if sj.state.end < 1 {
            sj.count
        } else {
            sj.state.end - 1
        };
    }

    /// `GOTOFRAMEPERSO` / `GOTOFRAMEOBJET` (@ 0x00449704 / 0x00449983):
    /// the first slot with that id (kind not tested) at frame `frame`.
    pub fn goto_frame(&mut self, id: i32, frame: i32) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id) {
            s.frame = frame;
        }
    }

    /// `ANIM_THE_FRAME` (@ 0x00449ab1): the first slot with that id
    /// animated or frozen.
    pub fn set_animated(&mut self, id: i32, on: bool) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id) {
            s.animated = on;
        }
    }

    /// `CONTINUEANIM` (@ 0x004495f1): the first shown slot plays on to its
    /// file's end.
    pub fn continue_anim(&mut self, id: i32) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id && s.shown) {
            s.last = s.count;
        }
    }

    /// `STOPANIM` (@ 0x0044969d): every character slot of id playing
    /// animation `number` hidden at frame 0.
    pub fn stop_anim(&mut self, id: i32, number: i32) {
        for s in self
            .slots
            .iter_mut()
            .filter(|s| s.id == id && s.kind == 1 && s.state.anim as i32 == number)
        {
            s.shown = false;
            s.frame = 0;
        }
    }

    /// The hand-over (@ 0x00439fa0) of slot k at the end of its play: a
    /// draw among the slots of the same entity whose state has a weight,
    /// by their current weights (`rand() % total`); the slot drawn gets
    /// its base weight back, every other one 10 more; another slot drawn
    /// is queued as k's successor. True when k first plays on to its
    /// file's end.
    fn handover(&mut self, k: usize, rand: &mut Rand) -> bool {
        if self.slots[k].next.is_some() || self.slots[k].state.weight < 1 {
            return false;
        }
        let (kind, id) = (self.slots[k].kind, self.slots[k].id);
        let cands: Vec<usize> = (0..self.slots.len())
            .filter(|&j| {
                let s = &self.slots[j];
                s.kind == kind && s.id == id && s.state.weight > 0
            })
            .take(16)
            .collect();
        let total: i32 = cands.iter().map(|&j| self.slots[j].weight).sum();
        if total <= 0 {
            return false;
        }
        let mut r = rand.rand() % total;
        let mut chosen = None;
        let mut result = false;
        for &j in &cands {
            if chosen.is_none() && r < self.slots[j].weight {
                chosen = Some(j);
                let base = self.slots[j].state.weight;
                self.slots[j].weight = if base == 0 { 100 } else { base };
                if j != k {
                    let sk = &mut self.slots[k];
                    if sk.last != sk.count {
                        sk.frame = sk.last;
                        sk.last = sk.count;
                        result = true;
                    }
                    sk.queued = true;
                    sk.next = Some(j);
                    let sj = &mut self.slots[j];
                    sj.next = None;
                    sj.queued = false;
                    sj.frame = 0;
                    sj.last = if sj.state.end < 1 {
                        sj.count
                    } else {
                        sj.state.end - 1
                    };
                }
            } else {
                if chosen.is_none() {
                    r -= self.slots[j].weight;
                }
                self.slots[j].weight += 10;
            }
        }
        result
    }

    /// The 15 Hz step (@ 0x00439c90): each shown, animated slot outside
    /// its pause shows the frame of its clock, `ftol((now − start) · 15)`
    /// (more than one frame), −1 past its last one (ended unless fresh)
    /// with its clock restarted; then each shown slot at −1: its random
    /// pause, the hand-over (not in a dialog), a fresh slot's first frame,
    /// its own loop (or hidden: end −1, or end = first), or the queued
    /// successor shown in its place. `dialog`: the loop is in mode 5.
    pub fn step(&mut self, now: f64, rand: &mut Rand) {
        self.step_in(now, rand, false);
    }

    pub fn step_in(&mut self, now: f64, rand: &mut Rand, dialog: bool) {
        for s in &mut self.slots {
            if !s.shown || s.mode == 0 || !s.animated || s.waiting {
                continue;
            }
            if s.last > 1 {
                s.frame = ((now - s.start) * 15.0) as i32;
            }
            if s.frame > s.last - 1 {
                s.frame = -1;
                if !s.ended && !s.fresh {
                    s.ended = true;
                }
            }
            if s.frame == -1 && s.last > 1 {
                s.start = now;
            }
        }
        for k in 0..self.slots.len() {
            let s = &mut self.slots[k];
            if !s.shown {
                continue;
            }
            if s.frame != -1 {
                s.fresh = false;
                continue;
            }
            if s.state.delay != 0 {
                s.waiting = true;
                if s.pause != 0 {
                    s.pause -= 1;
                    continue;
                }
                s.pause = rand.rand() % s.state.delay.unsigned_abs() as i32 + 10;
                s.waiting = false;
            }
            if !dialog && self.handover(k, rand) {
                continue;
            }
            let s = &mut self.slots[k];
            if s.fresh {
                s.frame = s.first;
                continue;
            }
            if !s.queued && s.next.is_none() {
                if s.state.end != -1 {
                    s.frame = s.first;
                    if s.state.end != 0 {
                        s.last = s.state.end;
                    }
                    if s.state.end == 0 || s.state.end != s.state.first {
                        continue;
                    }
                }
                s.shown = false;
            } else {
                s.shown = false;
                if let Some(j) = s.next {
                    let sj = &mut self.slots[j];
                    sj.shown = true;
                    sj.fresh = true;
                    sj.frame = sj.first;
                    sj.start = now;
                }
            }
        }
    }

    /// The place-sprite hit test of the sprite pass (@ 0x00440bf0 →
    /// @ 0x00439920), at the centre ray's face pixel (u, v): every shown
    /// slot whose state is clickable, whose frame's sprite on that face
    /// (`GET_SPWZ`: x, y, w, h with the hot spot at (0, 0)) has x < u <
    /// x + w + 1 and y < v < y + h + 1, and whose pixel there is opaque
    /// (`SPW_COLLISION`); the last one wins. The slot and the cursor kind
    /// (2 for an entity of kind 1 or 3, 1 for 2 or 4).
    pub fn hit(&self, face: usize, u: i32, v: i32) -> Option<(usize, i32)> {
        let mut out = None;
        for (k, s) in self.slots.iter().enumerate() {
            if !s.shown || s.click == 0 || s.frame < 0 {
                continue;
            }
            let Some(info) = s.bank.face_info(s.frame as usize, face) else {
                continue;
            };
            let (x, y) = (-i32::from(info.xoff), -i32::from(info.yoff));
            let (w, h) = (i32::from(info.w), i32::from(info.h));
            if !(x < u && u < x + w + 1 && y < v && v < y + h + 1) {
                continue;
            }
            if s.bank.face_opaque_at(s.frame as usize, face, u, v) {
                out = Some((k, if matches!(s.kind, 1 | 3) { 2 } else { 1 }));
            }
        }
        out
    }

    /// The sprite pass (@ 0x00440bf0): every shown slot's frame drawn into
    /// the faces with `AFF_SPW`, in slot order; `tick` is the 15 Hz tick
    /// count (blinking slots are drawn on odd ticks).
    pub fn draw(&self, faces: &mut [Vec<u16>; 6], tick: u32) {
        for s in &self.slots {
            if !s.shown || s.mode != 1 || s.frame < 0 || (s.blink && tick & 1 == 0) {
                continue;
            }
            for (face, px) in faces.iter_mut().enumerate() {
                let mut t = Target {
                    pixels: px,
                    pitch: 512,
                    ymin: 0,
                    ymax: 512,
                    xmin: 0,
                    xmax: 512,
                };
                s.bank.draw_face(&mut t, s.frame as usize, face);
            }
        }
    }
}
