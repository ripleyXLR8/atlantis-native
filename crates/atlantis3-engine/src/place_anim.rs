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
use atlantis3_data::spr::{Bank, Depth, Pixel, Target};
use atlantis3_data::A3Files;
use std::collections::HashMap;
use std::sync::Arc;

/// `AtlSpw_load : n>MAX_SPW_VISIBLE` (0x3f).
pub const MAX_SLOTS: usize = 0x40;

/// The entries of a slot's frame list (+0x34, 0x400 bytes).
pub const FRAME_LIST: usize = 0x100;

impl Slot {
    /// The frames the list draws, in its order: the entries before the
    /// first −2, the −1 ones skipped (@ 0x00439130's mode-0 loop).
    pub fn listed_frames(&self) -> impl Iterator<Item = i32> + '_ {
        self.frames
            .iter()
            .copied()
            .take_while(|&f| f != -2)
            .filter(|&f| f != -1)
    }

    /// Every entry of the list that is not −2, in table order (what a
    /// harness reading the slot's memory sees).
    pub fn frame_entries(&self) -> impl Iterator<Item = i32> + '_ {
        self.frames.iter().copied().filter(|&f| f != -2)
    }

    /// The frames drawn this frame: the clock's frame (+0x08) in mode 1,
    /// else the frame list's (@ 0x00439130 → @ 0x00439920 per frame).
    pub fn drawn_frames(&self) -> Vec<i32> {
        if self.mode == 1 {
            if self.frame >= 0 {
                vec![self.frame]
            } else {
                Vec::new()
            }
        } else {
            self.listed_frames().filter(|&f| f >= 0).collect()
        }
    }

    /// The slot's clocks restarted (+0x44c := now, the pause sums 0), as
    /// the C code does after setting a frame and the mode.
    pub fn restart_clock(&mut self, now: f64) {
        self.start = now;
        self.offset = 0.0;
    }

    /// The 2004 program's clock frame (@ 0x0043b11a..0x0043b12f):
    /// `ftol(((now − start) + acc − offset) · fps)`, acc (+0x464) 0 after
    /// every write of it.
    fn clock_frame(&self, now: f64) -> i32 {
        ((now - self.start - self.offset) * f64::from(self.fps)) as i32
    }
}

/// The 2004 program's reading of a WAM state's words (the record as
/// @ 0x00439040 reads it: shorts at +0x10, +0x12 and +0x1c, the next
/// state's text at +0x1e).
mod evo_state {
    use atlantis2_data::wam::EntityState;

    /// +0x10 (short): the hand-over weight.
    pub fn weight(s: &EntityState) -> i32 {
        i32::from(s.weight as u16 as i16)
    }

    /// +0x12 (short): the limit of the plays in a row of a slot the
    /// hand-over draws again.
    pub fn reps(s: &EntityState) -> i32 {
        i32::from((s.weight as u32 >> 16) as u16 as i16)
    }

    /// +0x1c (short): the frames a second; 15 when not above 0.
    pub fn fps(s: &EntityState) -> i32 {
        let f = i32::from(s.next as u16 as i16);
        if f > 0 {
            f
        } else {
            15
        }
    }

    /// +0x1e: the next state's letter (0 = none).
    pub fn next_letter(s: &EntityState) -> u8 {
        (s.next >> 16) as u8
    }

    /// `atoi(state + 0x1f)`: the next state's animation number, and
    /// whether its text is empty.
    pub fn next_anim(s: &EntityState) -> (i32, bool) {
        let mut b = vec![(s.next >> 24) as u8];
        for w in [s.first, s.end, s.shown, s.single] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        (atlantis3_data::atl::atoi(&b), b[0] == 0)
    }
}

/// A slot (0x490 bytes @ 0x007594cc + 0x490 k).
pub struct Slot {
    /// The SPW file's base name.
    pub name: String,
    /// −8: the entity's kind (WAM entity +0x38); −4: its `defnames.con`
    /// index (+0x14).
    pub kind: i32,
    pub id: i32,
    pub(crate) bank: Arc<Bank>,
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
    /// +0x34..+0x433: the frame list drawn when the mode is not 1 (256
    /// entries, −2 free, −1 skipped; [`Slots::add_frame`],
    /// [`Slots::sub_frame`]). Empty after the load.
    pub frames: [i32; FRAME_LIST],
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
    /// The 2004 program's slot (0x4a0 bytes @ 0x006ea3b4 + 0x4a0 k,
    /// loaded by @ 0x00439040): +0x448 the frames a second, +0x44c the
    /// state's ([`evo_state::fps`]).
    pub fps: i32,
    pub fps_base: i32,
    /// +0x46c: subtracted from the clock (at the load −(frame / fps),
    /// integer division; 0 after a restart).
    pub offset: f64,
    /// +0x474: the plays in a row of this slot drawn by the hand-over;
    /// +0x478 their limit (−1 none; the state's +0x12 at the load, set by
    /// the hand-over when 0).
    pub reps: i32,
    pub max_reps: i32,
    /// (WAM entity, state) of the slot's state record (+0x1c), and of the
    /// record its next state names (+0x20 at the load, resolved into the
    /// slot +0x24 after the view's loads); whether its state names one
    /// (+0x1e ≠ 0).
    pub rec: (usize, usize),
    pub link: Option<(usize, usize)>,
    pub chain: bool,
}

/// The view's slots.
#[derive(Default)]
pub struct Slots {
    pub slots: Vec<Slot>,
    /// The 2004 program's loader, step and hand-over
    /// ([`A3Files::is_evolution_2004`]).
    pub evolution: bool,
    /// The display's format the banks are prepared for.
    pub depth: Depth,
    /// The place animations' folder under `spw\`: `spw512`, `SPW1024`
    /// with WarpRes 1024 (@ 0x004164c0, [`crate::travel::spw_folder`]).
    pub spw: &'static str,
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
        now: f64,
        shown: impl Fn(i32, i32) -> Option<bool>,
    ) -> Slots {
        Slots::load_depth(
            files,
            cache,
            wam,
            view,
            world,
            rand,
            now,
            shown,
            Depth::Rgb565,
            crate::travel::spw_folder(512),
        )
    }

    /// [`Slots::load_with`] for a display of format `depth`, the place
    /// animations read from `spw\<spw>\<world>\`.
    #[allow(clippy::too_many_arguments)]
    pub fn load_depth(
        files: &A3Files,
        cache: &mut HashMap<String, Option<Arc<Bank>>>,
        wam: &Wam,
        view: u32,
        world: &str,
        rand: &mut dyn FnMut() -> i32,
        now: f64,
        shown: impl Fn(i32, i32) -> Option<bool>,
        depth: Depth,
        spw: &'static str,
    ) -> Slots {
        let mut out = Slots {
            slots: Vec::new(),
            evolution: files.is_evolution_2004(),
            depth,
            spw,
        };
        'load: for (ei, e) in wam.entities.iter().enumerate() {
            for (si, s) in e.state_records().enumerate() {
                if s.view != view {
                    continue;
                }
                if out.slots.len() >= MAX_SLOTS - 1 {
                    log::error!("AtlSpw_load : n>MAX_SPW_VISIBLE");
                    if out.evolution {
                        break 'load;
                    }
                    return out;
                }
                let name = e.spw_name(&s);
                out.push_state(
                    files,
                    cache,
                    e,
                    s,
                    (ei, si),
                    &name,
                    world,
                    rand,
                    now,
                    &shown,
                );
            }
        }
        if out.evolution {
            out.evo_entry(now, rand);
        }
        out
    }

    /// The 2004 view entry after its loads (FUN_00442c80): each slot's
    /// next-state record turned into that record's slot, or dropped when
    /// the view has none (@ 0x004431be..: then again FUN_00439720), then
    /// the end-of-play pass over every shown slot at frame −1 (the loads
    /// leave −1 for a first frame of 0), whatever its mode
    /// (@ 0x004431df..0x00443418).
    fn evo_entry(&mut self, now: f64, rand: &mut dyn FnMut() -> i32) {
        for k in 0..self.slots.len() {
            let Some(target) = self.slots[k].link else {
                continue;
            };
            match self.slots.iter().position(|s| s.rec == target) {
                Some(j) => {
                    self.slots[k].queued = true;
                    self.slots[k].next = Some(j);
                }
                None => {
                    self.slots[k].queued = false;
                    self.slots[k].link = None;
                }
            }
        }
        for k in 0..self.slots.len() {
            if self.slots[k].shown && self.slots[k].frame == -1 {
                self.evo_end(k, now, rand, false);
            }
        }
    }

    /// `AtlSpw_LoadOne` (@ 0x00438530): a slot for the state `s` of the
    /// entity `e` from the SPW file `name` (`spw\spw512\<world>\`),
    /// appended; `shown` from the state, then from the script's show table
    /// (@ 0x0045f720: `shown(id, kind)`), and hidden when an earlier slot
    /// of the same entity is shown; the random pause drawn when the state
    /// has one. Nothing when the file is missing.
    #[allow(clippy::too_many_arguments)]
    pub fn push_state(
        &mut self,
        files: &A3Files,
        cache: &mut HashMap<String, Option<Arc<Bank>>>,
        e: &atlantis2_data::wam::Entity,
        s: EntityState,
        rec: (usize, usize),
        name: &str,
        world: &str,
        rand: &mut dyn FnMut() -> i32,
        now: f64,
        shown: &dyn Fn(i32, i32) -> Option<bool>,
    ) {
        let depth = self.depth;
        // The 2004 program: `spw\<name>` on the current disc (the view
        // entry @ 0x0044307d..0x004430ab formats the name and passes it to
        // the `spw` path builder FUN_00418070; no size or world folder).
        let path = if files.is_evolution_2004() {
            format!("spw/{name}")
        } else {
            format!("spw/{}/{world}/{name}", self.spw)
        };
        let key = match depth {
            Depth::Rgb565 => path.clone(),
            Depth::Xrgb8888 => format!("{path}#x888"),
        };
        let bank = cache
            .entry(key)
            .or_insert_with(|| {
                files
                    .read(&path)
                    .and_then(|d| Bank::parse(&d, depth).ok())
                    .map(Arc::new)
            })
            .clone();
        let Some(bank) = bank else {
            log::warn!("Can't load SPW {path}");
            return;
        };
        let count = bank.len() as i32;
        let kind = e.kind as i32;
        let id = e.index as i32;
        let base = name
            .strip_suffix(".SPW")
            .or_else(|| name.strip_suffix(".spw"))
            .unwrap_or(name);
        if files.is_evolution_2004() {
            self.push_state_2004(e, s, rec, base, bank, now, shown);
            return;
        }
        let out = &mut self.slots;
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
            name: base.to_string(),
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
            frames: [-2; FRAME_LIST],
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
            fps: 15,
            fps_base: 15,
            offset: 0.0,
            reps: 0,
            max_reps: 0,
            rec,
            link: None,
            chain: false,
        });
    }

    /// The 2004 loader FUN_00439040 (@ 0x004394a5..0x00439664): frame =
    /// the state's first frame, **−1 for 0**; last = end − 1, or the
    /// file's count for end 0; +0x14 = 0 (the frame a loop or a hand-over
    /// starts again at); the mode, click kind and shown flag from the
    /// state; fps from the state (+0x1c, 15 when not above 0); weight
    /// +0x10 and play limit +0x12 as they are; no pause drawn; the clocks
    /// started now, less frame / fps (integer division); the next state
    /// +0x1e: the state itself when it names itself, else the record of
    /// the entity in this view with that letter and animation number (1
    /// for none) (FUN_00469550). Shown as in *Atlantis III*: the state's
    /// flag, then the show table, hidden when an earlier slot of the
    /// entity is shown.
    #[allow(clippy::too_many_arguments)]
    fn push_state_2004(
        &mut self,
        e: &atlantis2_data::wam::Entity,
        s: EntityState,
        rec: (usize, usize),
        name: &str,
        bank: Arc<Bank>,
        now: f64,
        shown: &dyn Fn(i32, i32) -> Option<bool>,
    ) {
        let count = bank.len() as i32;
        let kind = e.kind as i32;
        let id = e.index as i32;
        let frame = if s.first == 0 { -1 } else { s.first };
        let fps = evo_state::fps(&s);
        let letter = evo_state::next_letter(&s);
        let link = (letter != 0)
            .then(|| {
                let (anim, empty) = evo_state::next_anim(&s);
                if letter == s.letter as u8 && anim == s.anim as i32 {
                    return Some(rec);
                }
                e.state_records()
                    .position(|t| {
                        t.view == s.view
                            && t.letter as u8 == letter
                            && ((empty && t.anim == 1) || (!empty && t.anim as i32 == anim))
                    })
                    .map(|si| (rec.0, si))
            })
            .flatten();
        let out = &mut self.slots;
        let mut slot_shown = s.shown != 0;
        if let Some(v) = shown(id, kind) {
            slot_shown = v;
            if out.iter().any(|o| o.id == id && o.kind == kind && o.shown) {
                slot_shown = false;
            }
        }
        out.push(Slot {
            name: name.to_string(),
            kind,
            id,
            bank,
            frame,
            last: if s.end == 0 { count } else { s.end - 1 },
            count,
            first: 0,
            click: s.click,
            state: s,
            shown: slot_shown,
            mode: s.single,
            frames: [-2; FRAME_LIST],
            driven: s.single == 0,
            blink: false,
            pause: 0,
            waiting: false,
            start: now,
            fresh: false,
            animated: true,
            weight: evo_state::weight(&s),
            queued: link.is_some(),
            next: None,
            ended: false,
            fps,
            fps_base: fps,
            offset: -f64::from(frame / fps),
            reps: 0,
            max_reps: evo_state::reps(&s),
            rec,
            link,
            chain: letter != 0,
        });
    }

    /// `ShowPerso` / `ShowObjet` (@ 0x0045f520): the first slot of (id,
    /// kind) whose animation is `number` (any for 0) shown, fresh, not
    /// ended, its clock restarted.
    pub fn show(&mut self, id: i32, kind: i32, number: i32, now: f64) {
        if let Some(s) = self.slots.iter_mut().find(|s| {
            s.id == id && s.kind == kind && (number == 0 || s.state.anim as i32 == number)
        }) {
            s.shown = true;
            if self.evolution {
                // FUN_004660d0 (@ 0x00466127..0x0046622f): shown, clocks
                // restarted, the state's frames a second (the script's
                // optional rate is never given in the 2004 scripts), the
                // byte +0x47c cleared; the frame is left as it is.
                s.restart_clock(now);
                s.fps = s.fps_base;
                return;
            }
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
            // The 2004 program's FUN_004663d0: the same, the clocks
            // restarted (@ 0x00466409..0x0046642c).
            s.restart_clock(now);
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
        if self.evolution {
            // FUN_00466540 with 2 (@ 0x0046665b): j's rate the
            // state's (no script rate in the 2004 scripts).
            sj.fps = sj.fps_base;
        }
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

    /// `AddSpwToFrameTab` (@ 0x00438a40, also `ADDFRAMEPERSO` /
    /// `ADDFRAMEOBJET`): `frame` written at the first −2 entry of the
    /// list of EVERY slot of kind 2 with that id (no duplicate test; a
    /// full list is left as it is). False, as the original's result,
    /// when no such slot exists (log `AddSpwToFrameTab impossible for -
    /// non trouvé -`) or the last such slot's list is full (`Trop
    /// d'entrée`).
    pub fn add_frame(&mut self, id: i32, frame: i32) -> bool {
        let mut last: Option<usize> = None;
        for s in self.slots.iter_mut().filter(|s| s.id == id && s.kind == 2) {
            let free = s.frames.iter().position(|&f| f == -2);
            if let Some(i) = free {
                s.frames[i] = frame;
            }
            last = Some(free.unwrap_or(FRAME_LIST));
        }
        match last {
            None => {
                log::warn!("AddSpwToFrameTab impossible for - non trouvé - : {id}");
                false
            }
            Some(FRAME_LIST) => {
                log::warn!("AddSpwToFrameTab impossible - Trop d'entrée - for {id}");
                false
            }
            Some(_) => true,
        }
    }

    /// `SubSpwToFrameTab` (@ 0x00438ae0, also `SUPFRAMEPERSO` /
    /// `SUPFRAMEOBJET`): in the FIRST slot of kind 2 with that id whose
    /// list has a −2, the LAST entry equal to `frame` before it gets the
    /// list's last entry, which becomes −2 (a swap-remove: the drawing
    /// order changes). The search walks on to the next slot of that id
    /// only past a full list, keeping what it found. False (log
    /// `SubSpwToFrameTab impossible !!`) when `frame` is not there; true
    /// also when no slot of that id exists.
    pub fn sub_frame(&mut self, id: i32, frame: i32) -> bool {
        let mut found: Option<usize> = None;
        let mut target: Option<(usize, usize)> = None;
        'slots: for (k, s) in self.slots.iter().enumerate() {
            if s.id != id || s.kind != 2 {
                continue;
            }
            for (j, &f) in s.frames.iter().enumerate() {
                if f == frame {
                    found = Some(j);
                }
                if f == -2 || found == Some(0xff) {
                    target = Some((k, j));
                    break 'slots;
                }
            }
        }
        let Some((k, end)) = target else {
            return true;
        };
        let Some(j) = found else {
            log::warn!("SubSpwToFrameTab impossible !! for :{id} {frame}");
            return false;
        };
        // @ 0x00438b5a: list[j] := list[end − 1]; list[end − 1] := −2.
        let l = &mut self.slots[k].frames;
        if end > 0 {
            l[j] = l[end - 1];
            l[end - 1] = -2;
        }
        true
    }

    /// @ 0x00438c00: the list of every slot of kind 2 with that id
    /// emptied (all −2).
    pub fn clear_frames(&mut self, id: i32) {
        for s in self.slots.iter_mut().filter(|s| s.id == id && s.kind == 2) {
            s.frames.fill(-2);
        }
    }

    /// @ 0x00438ba0: whether `frame` is one of the 256 entries of the list
    /// of a slot of kind 2 with that id (past the −2 too).
    pub fn has_frame(&self, id: i32, frame: i32) -> bool {
        self.slots
            .iter()
            .filter(|s| s.id == id && s.kind == 2)
            .any(|s| s.frames.contains(&frame))
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
        if self.evolution {
            self.evo_step(now, &mut || rand.rand(), dialog);
            return;
        }
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

    /// The 2004 step FUN_0043b0a0, at the end of every warp draw
    /// (FUN_00444870 @ 0x00444917, no 15 Hz gate): each shown slot of a
    /// mode, animated, not pausing, shows its clock's frame
    /// ([`Slot::clock_frame`], more than one frame), −1 past its last one
    /// with its clocks restarted (@ 0x0043b0e1..0x0043b1a4); then each
    /// shown slot of a mode at −1 ends its play ([`Slots::evo_end`],
    /// @ 0x0043b1c4..0x0043b3fc). The end of a play also calls the
    /// script's resume FUN_00450f30 when the slot's byte +0x47c is set
    /// (@ 0x0043b150..0x0043b15a): no writer of 1 found [UNKNOWN], not
    /// ported. `dialog`: the loop mode is 6 (@ 0x0043b282; [UNKNOWN]
    /// which mode that is), no hand-over.
    fn evo_step(&mut self, now: f64, rand: &mut dyn FnMut() -> i32, dialog: bool) {
        for s in &mut self.slots {
            if !s.shown || s.mode == 0 || !s.animated || s.waiting {
                continue;
            }
            if s.last > 1 {
                s.frame = s.clock_frame(now);
            }
            if s.frame > s.last - 1 {
                s.frame = -1;
            }
            if s.frame == -1 && s.last > 1 {
                s.restart_clock(now);
            }
        }
        for k in 0..self.slots.len() {
            let s = &self.slots[k];
            if s.shown && s.mode != 0 && s.frame == -1 {
                self.evo_end(k, now, rand, dialog);
            }
        }
    }

    /// The end of a play of slot k in the 2004 program (the step's second
    /// loop @ 0x0043b1ed..0x0043b3fc, the view entry's @ 0x004431f8..):
    /// no pause count: not pausing, mode 1, clocks restarted; a pause
    /// count: count := last − the clock's frame (0 below 1), and while
    /// not 0 nothing else. Then, with no successor queued, no pause count,
    /// a mode and not `dialog`: the hand-over ([`Slots::evo_handover`]),
    /// which ends it when it answers 1. Not pausing: no successor → hidden
    /// for a state end of −1, else frame +0x14 (0) and last := the state
    /// end when not 0 (**every other state plays again**); a successor j
    /// → k hidden, j shown at its +0x14, j's own successor dropped unless
    /// its state names a next state, j's clocks restarted.
    fn evo_end(&mut self, k: usize, now: f64, rand: &mut dyn FnMut() -> i32, dialog: bool) {
        let s = &mut self.slots[k];
        if s.pause == 0 {
            s.waiting = false;
            s.mode = 1;
            s.restart_clock(now);
        } else {
            s.pause = (s.last - s.clock_frame(now)).max(0);
            if s.pause != 0 {
                return;
            }
        }
        let s = &self.slots[k];
        if !s.queued
            && s.next.is_none()
            && s.pause == 0
            && s.mode != 0
            && !dialog
            && self.evo_handover(k, rand)
        {
            return;
        }
        let s = &mut self.slots[k];
        if s.waiting {
            return;
        }
        if !s.queued && s.next.is_none() {
            if s.state.end == -1 {
                s.shown = false;
            } else {
                s.frame = s.first;
                if s.state.end != 0 {
                    s.last = s.state.end;
                }
            }
            return;
        }
        s.shown = false;
        if let Some(j) = s.next {
            let sj = &mut self.slots[j];
            sj.shown = true;
            sj.frame = sj.first;
            if !sj.chain {
                sj.next = None;
                sj.queued = false;
            }
            sj.restart_clock(now);
        }
    }

    /// The 2004 hand-over FUN_0043b420(k): nothing (0) when k has a
    /// successor or its state's weight (+0x10) is below 1. Candidates: the
    /// slots of k's entity whose state weight is above 0 (more than 16:
    /// log `MAX_ANIM`); every other slot whose state weight is 0 gets the
    /// play limit 1. A candidate without a limit gets 1 when the weights
    /// sum below 100, −1 (none) when alone, else −1 for the first of the
    /// largest weight and 1 for the others. Then `r = rand() % 100` along
    /// the weights (which never change): the slot drawn is k → a play
    /// more (none past its limit: the draw goes on as if it had missed);
    /// another j → queued as k's successor (k plays on to its file's end
    /// first: answer 1 when it was not there yet), j at frame 0 with its
    /// last frame and a play more. The slot drawn: no pause, mode 1.
    /// Nothing drawn (weights below 100): k pauses for a play's length
    /// (count := last, pausing, mode 0); with 100 or more, the first
    /// candidate other than k is queued as above (no pause). The weights
    /// and limits here are the WAM's; the cabin's are 100 or 0.
    fn evo_handover(&mut self, k: usize, rand: &mut dyn FnMut() -> i32) -> bool {
        if self.slots[k].next.is_some() || evo_state::weight(&self.slots[k].state) < 1 {
            return false;
        }
        let (kind, id) = (self.slots[k].kind, self.slots[k].id);
        let mut cands: Vec<(usize, i32)> = Vec::new();
        let (mut total, mut best, mut best_w) = (0, 0, 0);
        for j in 0..self.slots.len() {
            let w = evo_state::weight(&self.slots[j].state);
            let s = &mut self.slots[j];
            if s.kind == kind && s.id == id && w > 0 {
                total += s.weight;
                if best_w < s.weight {
                    best_w = s.weight;
                    best = cands.len();
                }
                cands.push((j, s.weight));
                if cands.len() > 16 {
                    log::error!("!!! Erreur !!! MAX_ANIM");
                }
            } else if w == 0 {
                s.max_reps = 1;
            }
        }
        let n = cands.len();
        for (i, &(j, _)) in cands.iter().enumerate() {
            let s = &mut self.slots[j];
            if s.max_reps == 0 {
                s.max_reps = if total < 100 || (n > 1 && i != best) {
                    1
                } else {
                    -1
                };
            }
        }
        if n == 0 {
            return false;
        }
        let mut r = rand() % 100;
        let mut chosen = false;
        let mut result = false;
        for &(j, w) in &cands {
            if chosen || w <= r {
                if !chosen {
                    r -= w;
                }
                self.slots[j].reps = 0;
                continue;
            }
            chosen = true;
            if j == k {
                let s = &mut self.slots[k];
                if s.max_reps == -1 {
                    s.reps = 0;
                } else if s.reps < s.max_reps {
                    s.reps += 1;
                } else {
                    chosen = false;
                    r -= w;
                    s.reps = 0;
                    continue;
                }
            } else {
                result |= self.queue_successor(k, j);
                let sj = &mut self.slots[j];
                if !sj.chain {
                    sj.next = None;
                    sj.queued = false;
                }
            }
            let sj = &mut self.slots[j];
            sj.pause = 0;
            sj.waiting = false;
            sj.mode = 1;
        }
        if chosen {
            return result;
        }
        if total >= 100 {
            if let Some(&(j, _)) = cands.iter().find(|c| c.0 != k) {
                result |= self.queue_successor(k, j);
                let sj = &mut self.slots[j];
                sj.next = None;
                sj.queued = false;
                return result;
            }
        }
        let s = &mut self.slots[k];
        s.pause = s.last;
        s.waiting = true;
        s.mode = 0;
        result
    }

    /// The hand-over's queueing of j after k (FUN_0043b420
    /// @ 0x0043b5e1..0x0043b672, again @ 0x0043b744..0x0043b7d4): k plays on to its
    /// file's end (true when it was not there yet: frame := last), queued
    /// with successor j; j at frame 0, last = state end − 1 (the file's
    /// count for 0), a play more unless it has no limit.
    fn queue_successor(&mut self, k: usize, j: usize) -> bool {
        let sk = &mut self.slots[k];
        let mut result = false;
        if sk.last != sk.count {
            sk.frame = sk.last;
            sk.last = sk.count;
            result = true;
        }
        sk.queued = true;
        sk.next = Some(j);
        let sj = &mut self.slots[j];
        sj.frame = 0;
        sj.last = if sj.state.end == 0 {
            sj.count
        } else {
            sj.state.end - 1
        };
        if sj.max_reps == -1 {
            sj.reps = 0;
        } else {
            sj.reps += 1;
        }
        result
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
            if !s.shown || s.click == 0 {
                continue;
            }
            for frame in s.drawn_frames() {
                let Some(info) = s.bank.face_info(frame as usize, face) else {
                    continue;
                };
                let (x, y) = (-i32::from(info.xoff), -i32::from(info.yoff));
                let (w, h) = (i32::from(info.w), i32::from(info.h));
                if !(x < u && u < x + w + 1 && y < v && v < y + h + 1) {
                    continue;
                }
                if s.bank.face_opaque_at(frame as usize, face, u, v) {
                    out = Some((k, if matches!(s.kind, 1 | 3) { 2 } else { 1 }));
                }
            }
        }
        out
    }

    /// The sprite pass (@ 0x00440bf0): every shown slot drawn into the
    /// faces with `AFF_SPW` (each face's own size as the clip), in slot order: in mode 1 its frame, else
    /// each frame of its list in the list's order (@ 0x00439130); `tick`
    /// is the 15 Hz tick count (blinking slots are drawn on odd ticks).
    pub fn draw<P: Pixel>(&self, faces: &mut [Vec<P>; 6], tick: u32) {
        for s in &self.slots {
            if !s.shown || (s.blink && tick & 1 == 0) {
                continue;
            }
            for frame in s.drawn_frames() {
                for (face, px) in faces.iter_mut().enumerate() {
                    // The face's surface: 512 x 512 (*Atlantis III*),
                    // 768 x 768 (the 2004 program's `jp6` faces).
                    let n = px.len().isqrt();
                    let mut t = Target {
                        pixels: px,
                        pitch: n,
                        ymin: 0,
                        ymax: n as i32,
                        xmin: 0,
                        xmax: n as i32,
                    };
                    s.bank.draw_face(&mut t, frame as usize, face);
                }
            }
        }
    }
}
