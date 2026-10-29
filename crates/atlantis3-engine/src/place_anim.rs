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
    waiting: bool,
    /// +0x44c: the time the play started (seconds).
    start: f64,
    /// +0x46c: a play is to start again at once (1 at load, 0 after the
    /// first step).
    fresh: bool,
    /// +0x46d: the play reached its end.
    pub ended: bool,
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
                    rand.rand() % s.delay.unsigned_abs() as i32 + 10
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
                    blink: false,
                    pause,
                    waiting: false,
                    start: now,
                    fresh: true,
                    ended: false,
                });
            }
        }
        Slots { slots: out }
    }

    /// `ShowPerso` / `ShowObjet` (@ 0x0045f520): the first slot of (id,
    /// kind) whose animation is `number` (any for 0) shown, its play
    /// started again.
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
    /// (id, kind) hidden.
    pub fn hide(&mut self, id: i32, kind: i32, now: f64) {
        for s in self
            .slots
            .iter_mut()
            .filter(|s| s.id == id && s.kind == kind)
        {
            s.shown = false;
            s.start = now;
        }
    }

    /// The 15 Hz step (@ 0x00439c90): each shown slot of more than one
    /// frame, outside its pause, shows frame ftol((now − start) · 15); past
    /// its last frame, −1 and the end flag, its clock restarted; then a
    /// slot at −1 waits its random pause (15 Hz ticks, state +0x14) and
    /// plays again from its first frame, or hides (state end −1, or an end
    /// equal to its first frame). The hand-over to another state of the
    /// entity (@ 0x00439fa0, the states' weights) and the chains of next
    /// states are not ported yet.
    pub fn step(&mut self, now: f64, rand: &mut Rand) {
        for s in &mut self.slots {
            if !s.shown || s.waiting {
                continue;
            }
            if s.last > 1 {
                s.frame = ((now - s.start) * 15.0) as i32;
            }
            if s.frame > s.last - 1 {
                s.frame = -1;
                s.ended = true;
            }
            if s.frame == -1 && s.last > 1 {
                s.start = now;
            }
        }
        for s in &mut self.slots {
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
            if s.fresh {
                s.frame = s.first;
                continue;
            }
            if s.state.end == -1 {
                s.shown = false;
                continue;
            }
            s.frame = s.first;
            if s.state.end != 0 {
                s.last = s.state.end;
            }
            if s.state.end != 0 && s.state.end == s.state.first {
                s.shown = false;
            }
        }
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
