//! The place animations: one slot per entity state of the current view
//! (`AtlSpw_load` @ 0x0047c4e0, at most 0x22), drawn into the cube faces
//! before the warp (@ 0x0045b040 → @ 0x0047cf00) and stepped once per
//! 15 Hz tick (@ 0x0047d7f0), with the random hand-over between the states
//! of one entity (@ 0x0047d9a0). A state that is not one animated frame
//! (record +0x2c ≠ 1) draws a list of frames instead, which the puzzles
//! fill (@ 0x0047c880, @ 0x0047c910, @ 0x0047c9c0).

use atlantis2_data::spw::{Blend, Clip, FaceBox, Spw, FACE};
use atlantis2_data::wam::EntityState;

/// `AtlSpw_load : n>MAX_SPW_VISIBLE`.
pub const MAX_SLOTS: usize = 0x22;

/// A slot's frame list (+0x3c): 128 bytes (`MAX_FRAME_TAB`), filled with
/// [`LIST_END`] at load; the frames are signed bytes.
pub const FRAME_TAB: usize = 0x80;
/// The end of a frame list (0xfe, −2).
pub const LIST_END: u8 = 0xfe;
/// An entry the draw skips (0xff, −1).
pub const LIST_SKIP: u8 = 0xff;

/// MSVC's `rand` (@ 0x005ae42b): `seed = seed × 214013 + 2531011`, bits
/// 16..30. The game seeds it with `GetTickCount()` at the SPW set-up
/// (@ 0x0047d740).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsvcRand(pub u32);

impl MsvcRand {
    pub fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7fff) as i32
    }
}

/// The show / hide table (@ 0x006afdd0, count @ 0x006c2dfc): entries
/// (id, state, kind) set by taking an object and by the scripts'
/// `showperso` / `hideobjet`... keys; it overrides "shown at load".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShowTable(pub Vec<(i32, i32, i32)>);

impl ShowTable {
    /// FUN_0048c7b0: the state of (id, kind): 0 hidden, 1 shown, 2 no
    /// entry.
    pub fn get(&self, id: i32, kind: i32) -> i32 {
        self.0
            .iter()
            .find(|e| e.0 == id && e.2 == kind)
            .map_or(2, |e| e.1)
    }

    /// FUN_0048c730: sets the entry of (id, kind), added at the end when
    /// new.
    pub fn set(&mut self, id: i32, state: i32, kind: i32) {
        match self.0.iter_mut().find(|e| e.0 == id && e.2 == kind) {
            Some(e) => e.1 = state,
            None => self.0.push((id, state, kind)),
        }
    }
}

/// A slot (200 bytes @ 0x0067bfb0 + 200 n).
#[derive(Clone, Debug)]
pub struct Slot {
    /// The SPW file's name.
    pub name: String,
    /// +0x00: the entity's kind, 1 character, 2 object.
    pub kind: i32,
    /// +0x04: the entity's id (its `defnames.con` index).
    pub id: i32,
    spw: Spw,
    /// +0x10: the current frame, −1 when a play has ended.
    pub frame: i32,
    /// +0x14: the end (the last frame + 1).
    pub end: i32,
    /// +0x18: the file's frame count.
    pub count: i32,
    /// +0x1c: the start frame.
    pub start: i32,
    /// +0x20: the click kind.
    pub click: i32,
    /// +0x24: the state record.
    pub state: EntityState,
    /// +0x28: hands over to `chain` when it ends.
    pub handing: bool,
    /// +0x2c: the slot it hands over to, −1 none.
    pub chain: i32,
    /// +0x30: drawn and stepped.
    pub active: bool,
    /// +0x38: for an object, its first `defnames.con` parameter (its
    /// sprite in the objects' bank).
    pub object_sprite: i32,
    /// +0x3c: the frames drawn when the state is not one animated frame
    /// (+0x34 ≠ 1), in list order up to [`LIST_END`], [`LIST_SKIP`]
    /// skipped.
    pub frames: [u8; FRAME_TAB],
    /// +0xc4: the countdown of the random pause.
    pub delay: i32,
    /// +0xbc (a byte): the slot plays, 1 at load (@ 0x0047c664); the
    /// step moves only the frames of active slots that play
    /// (@ 0x0047d82c). The chest's hooks clear it (@ 0x0049393b,
    /// @ 0x00493a7c) to hold their frame.
    pub playing: bool,
}

impl Slot {
    /// The box of its current frame on a face, if it has a sprite there.
    pub fn frame_box(&self, face: usize) -> Option<FaceBox> {
        self.spw.face_box(self.frame, face)
    }
}

/// What a draw found under the screen centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CentreHit {
    /// The cursor state @ 0x006c2df4: 2 over a clickable character, 1
    /// over a clickable object, 0 none.
    pub cursor: i32,
    /// The slot under the centre, recorded on a click (@ 0x00661640).
    pub slot: Option<usize>,
}

/// The slots of the current view.
#[derive(Clone, Debug)]
pub struct Anim {
    pub slots: Vec<Slot>,
    pub table: ShowTable,
    pub rand: MsvcRand,
    blend: Blend,
    /// `CHEVAL`'s `#OBJETS#` index (`DefName_GetIdName(1, "CHEVAL")`): the
    /// slots of that id carry the horse's frame across views in
    /// `FRAMECHEVAL` (@ 0x0047d84d, @ 0x0047c764); none without it.
    pub cheval: Option<i32>,
    /// The original's warnings of the frame lists (`FUN_00480c20`: their
    /// format strings), in order, for tests and tools.
    pub warnings: Vec<&'static str>,
}

/// The texel under the screen centre: (face, column, row).
pub type Texel = (usize, i32, i32);

impl Anim {
    /// The SPW set-up (@ 0x0047d740): blend tables, no slot, `rand`
    /// seeded.
    pub fn new(seed: u32) -> Anim {
        Anim {
            slots: Vec::new(),
            table: ShowTable::default(),
            rand: MsvcRand(seed),
            blend: Blend::rgb565(),
            cheval: None,
            warnings: Vec::new(),
        }
    }

    /// FUN_0047c480: frees every slot (at each view entry).
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// FUN_0047c4e0: a slot for one entity state of the entered view.
    /// `kind` and `id` are the entity's, `object_sprite` its parameter.
    pub fn load(
        &mut self,
        name: &str,
        spw: Spw,
        kind: i32,
        id: i32,
        object_sprite: i32,
        state: EntityState,
    ) {
        if self.slots.len() >= MAX_SLOTS {
            log::warn!("AtlSpw_load : n>MAX_SPW_VISIBLE ({name})");
            return;
        }
        let count = spw.frame_count();
        let end = if state.end == 0 { count } else { state.end - 1 };
        if state.next != 0 {
            // FUN_004123b0 (state chains) is not ported: no Tibet state has
            // a next state.
            log::warn!("{name}: state chains are not supported");
        }
        let mut active = state.shown != 0;
        let shown = self.table.get(id, kind);
        if shown != 2 {
            active = shown != 0;
            // When the table has an entry, another active slot of the same
            // entity and kind keeps this one hidden.
            if self
                .slots
                .iter()
                .any(|s| s.id == id && s.kind == kind && s.active)
            {
                active = false;
            }
        }
        if state.single != 1 && count >= 0x80 {
            log::warn!("nbframe>MAX_FRAME_TAB for {count} ({name})");
        }
        self.slots.push(Slot {
            name: name.to_string(),
            kind,
            id,
            spw,
            frame: state.first,
            end,
            count,
            start: state.first,
            click: state.click,
            state,
            handing: false,
            chain: -1,
            active,
            object_sprite,
            frames: [LIST_END; FRAME_TAB],
            delay: state.delay.wrapping_abs(),
            playing: true,
        });
    }

    /// FUN_0047d7f0 on a 15 Hz tick: every active slot that plays
    /// (+0x30 and the byte +0xbc, @ 0x0047d827..0x0047d832) one frame
    /// on, then the active slots whose play has ended (frame −1, whether
    /// they play or not) restart, hand over or stop. Returns
    /// the frame the first loop left in the last slot of `CHEVAL`'s id,
    /// whatever its kind (the value it writes into `FRAMECHEVAL`,
    /// @ 0x0047d84d..0x0047d872).
    pub fn step(&mut self) -> Option<i32> {
        let mut horse = None;
        for s in self.slots.iter_mut().filter(|s| s.active && s.playing) {
            s.frame = if s.frame < 0 || s.frame >= s.end - 1 {
                -1
            } else {
                s.frame + 1
            };
            if Some(s.id) == self.cheval {
                horse = Some(s.frame);
            }
        }
        for i in 0..self.slots.len() {
            if !self.slots[i].active || self.slots[i].frame != -1 {
                continue;
            }
            let delay = self.slots[i].state.delay;
            if delay != 0 {
                if self.slots[i].delay != 0 {
                    self.slots[i].delay -= 1;
                    continue;
                }
                let r = self.rand.rand();
                self.slots[i].delay = r.wrapping_mul(delay.wrapping_abs()) / 0x7fff + 10;
            }
            if self.hand_over(i) {
                continue;
            }
            let s = &mut self.slots[i];
            if !s.handing && s.chain == -1 {
                if s.state.end == -1 {
                    s.active = false;
                } else {
                    s.frame = s.start;
                    if s.state.end != 0 {
                        s.end = s.state.end;
                    }
                }
            } else {
                s.active = false;
                let (start, chain) = (s.start, s.chain);
                if let Some(c) = self.slots.get_mut(chain as usize) {
                    c.active = true;
                    c.frame = start;
                }
            }
        }
        horse
    }

    /// FUN_0047d9a0: a slot that is not chained and whose state has a
    /// weight draws among the weighted slots of the same id. Returns true
    /// when it picked another one and first plays on to its file's last
    /// frame.
    fn hand_over(&mut self, i: usize) -> bool {
        let me = &self.slots[i];
        if me.chain != -1 || me.state.weight <= 0 {
            return false;
        }
        let id = me.id;
        let mut weights = Vec::new();
        for (k, s) in self.slots.iter().enumerate() {
            if s.id == id && s.state.weight > 0 {
                weights.push((s.state.weight, k));
                if weights.len() > 0x10 {
                    log::warn!("Erreur MAX_ANIM dans ChoixAnimSpw");
                }
            }
        }
        if weights.is_empty() {
            return false;
        }
        let r = ((self.rand.rand() as u32 & 0x3ff) / 10).min(100) as i32;
        let mut sum = 0;
        let mut pick = None;
        for &(w, k) in &weights {
            sum += w;
            if sum >= r {
                pick = Some(k);
                break;
            }
        }
        let Some(k) = pick else { return false };
        if k == i {
            self.slots[i].frame = 0;
            return false;
        }
        let (end, count) = (self.slots[i].end, self.slots[i].count);
        {
            let me = &mut self.slots[i];
            if end != count {
                me.frame = end;
                me.end = count;
            }
            me.handing = true;
            me.chain = k as i32;
        }
        let other = &mut self.slots[k];
        other.chain = -1;
        other.handing = false;
        other.frame = 0;
        other.end = if other.state.end == 0 {
            other.count
        } else {
            other.state.end - 1
        };
        end != count
    }

    /// FUN_0045b040 (and FUN_0047ce60 / FUN_0047cf00 per slot): draws the
    /// active slots' frames into the faces, later slots over earlier ones,
    /// and tests the texel under the screen centre against each one's box
    /// on that face. Returns the boxes drawn (to restore after the warp)
    /// and what is under the centre; a slot is recorded under the centre
    /// only when `click` is set.
    pub fn draw(
        &self,
        faces: &mut [Vec<u16>; 6],
        centre: Option<Texel>,
        click: bool,
    ) -> (Vec<(usize, FaceBox)>, CentreHit) {
        let mut dirty = Vec::new();
        let mut hit = CentreHit::default();
        for (n, s) in self.slots.iter().enumerate() {
            if !s.active {
                continue;
            }
            // FUN_0047ce60: one animated frame (+0x34 = 1 and +0x30 = 1)
            // draws its current frame; otherwise the list's frames in
            // order up to 0xfe, 0xff skipped (signed bytes).
            let list: Vec<i32> = if s.state.single == 1 {
                vec![s.frame]
            } else {
                s.frames
                    .iter()
                    .take_while(|&&b| b != LIST_END)
                    .filter(|&&b| b != LIST_SKIP)
                    .map(|&b| i32::from(b as i8))
                    .collect()
            };
            for &frame in &list {
                if let Some((face, col, row)) = centre {
                    if s.click != 0 {
                        let b = s.spw.face_box(frame, face).unwrap_or_default();
                        let (x, y, w, h) = (
                            i32::from(b.x),
                            i32::from(b.y),
                            i32::from(b.w),
                            i32::from(b.h),
                        );
                        if row < y + h + 1 && y < row && x < col && col < x + w + 1 {
                            match s.kind {
                                1 | 3 => hit.cursor = 2,
                                2 | 4 => hit.cursor = 1,
                                _ => {}
                            }
                        }
                    }
                }
                for (face, pixels) in faces.iter_mut().enumerate() {
                    if let Some(b) = s.spw.face_box(frame, face) {
                        if s.spw.draw(frame, face, pixels, Clip::FACE, &self.blend) {
                            dirty.push((face, b));
                        }
                    }
                }
            }
            // The byte FUN_0047ce60 returns: FUN_0047cf00's (cursor state
            // ≠ 0) for the last frame drawn; 0 when a list drew none.
            if hit.cursor != 0 && !list.is_empty() && click && hit.slot.is_none() && s.click > 0 {
                hit.slot = Some(n);
            }
        }
        (dirty, hit)
    }

    /// FUN_0047c880 (`AddSpwToFrameTab`): `frame` (as a byte) written at
    /// the end of the list of every slot of `id`, whatever its kind and
    /// state. False, with the original's warning, when no slot has `id` or
    /// the last such slot's list is full (128 entries: the earlier ones
    /// are still written).
    pub fn add_frame(&mut self, id: i32, frame: i32) -> bool {
        let mut last = None;
        for s in self.slots.iter_mut().filter(|s| s.id == id) {
            let at = s.frames.iter().position(|&b| b == LIST_END);
            if let Some(i) = at {
                s.frames[i] = frame as u8;
            }
            last = Some(at);
        }
        match last {
            None => {
                log::warn!("AddSpwToFrameTab impossible for - non trouvé - :{id}");
                self.warnings
                    .push("AddSpwToFrameTab impossible for - non trouvé - :%d");
                false
            }
            Some(None) => {
                log::warn!("AddSpwToFrameTab impossible - Trop d'entrée - for :{id}");
                self.warnings
                    .push("AddSpwToFrameTab impossible - Trop d'entrée - for :%d");
                false
            }
            Some(Some(_)) => true,
        }
    }

    /// FUN_0047c910 (`SubSpwToFrameTab`): in the first slot of `id` whose
    /// list has an end, the last entry equal to `frame` (a signed byte)
    /// is replaced by the list's last entry, which is then the end. False,
    /// with a warning, when that list does not hold `frame`; true when no
    /// slot of `id` has a list with an end (nothing changed). The match
    /// index is set once (@ 0x0047c91a), not per slot: the last match k
    /// (0..126) of a full list is carried to the next slot of `id` (a
    /// match at 127 stops the walk in its own slot, @ 0x0047c948). The
    /// port keeps that, but not one case: when the list that ends the
    /// walk has its end at index 0 (k carried into it, or `frame` = −2
    /// matching that end byte), the original copies the byte below the
    /// list (slot +0x3b) into entry k and writes the end there
    /// (@ 0x0047c979..0x0047c988); the port writes neither. Out of reach
    /// of any data.
    pub fn sub_frame(&mut self, id: i32, frame: i32) -> bool {
        let mut found: Option<usize> = None;
        for s in self.slots.iter_mut().filter(|s| s.id == id) {
            let mut end = None;
            for (j, &b) in s.frames.iter().enumerate() {
                if i32::from(b as i8) == frame {
                    found = Some(j);
                }
                if b == LIST_END || found == Some(0x7f) {
                    end = Some(j);
                    break;
                }
            }
            let Some(end) = end else { continue };
            let Some(at) = found else {
                log::warn!("SubSpwToFrameTab impossible !! for :{id} {frame}");
                self.warnings
                    .push("SubSpwToFrameTab impossible !! for :%d %d");
                return false;
            };
            if end > 0 {
                s.frames[at] = s.frames[end - 1];
                s.frames[end - 1] = LIST_END;
            }
            return true;
        }
        true
    }

    /// FUN_0047c9c0: whether a slot of `id` has `frame` (a signed byte)
    /// anywhere in its 128 bytes, past the end included.
    pub fn has_frame(&self, id: i32, frame: i32) -> bool {
        self.slots
            .iter()
            .filter(|s| s.id == id)
            .any(|s| s.frames.iter().any(|&b| i32::from(b as i8) == frame))
    }

    /// Slot `n`'s frame list up to its end, as signed bytes (for tests and
    /// tools).
    pub fn frame_list(&self, n: usize) -> Vec<i32> {
        self.slots.get(n).map_or(Vec::new(), |s| {
            s.frames
                .iter()
                .take_while(|&&b| b != LIST_END)
                .map(|&b| i32::from(b as i8))
                .collect()
        })
    }

    /// Takes an object (FUN_0045a650, click kind 1): hidden for good (the
    /// table), its slot stopped (FUN_0048c6b0).
    pub fn take(&mut self, slot: usize) -> Option<i32> {
        let id = self.slots.get(slot)?.id;
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id && s.kind == 2) {
            s.active = false;
            s.frame = 0;
        }
        self.table.set(id, 0, 2);
        Some(id)
    }
}

/// Puts back the face texels under the boxes drawn (the faces' clean copy
/// `clean`), as the save / restore around the warp does (@ 0x0045b120,
/// @ 0x004d9ec0).
pub fn restore(faces: &mut [Vec<u16>; 6], clean: &[Vec<u16>; 6], dirty: &[(usize, FaceBox)]) {
    for &(face, b) in dirty {
        let (x0, y0) = (usize::from(b.x), usize::from(b.y));
        let x1 = (x0 + usize::from(b.w)).min(FACE);
        let y1 = (y0 + usize::from(b.h)).min(FACE);
        for y in y0..y1 {
            let r = y * FACE;
            faces[face][r + x0..r + x1].copy_from_slice(&clean[face][r + x0..r + x1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msvc_rand() {
        // srand(1): 41, 18467, 6334, ...
        let mut r = MsvcRand(1);
        assert_eq!([r.rand(), r.rand(), r.rand()], [41, 18467, 6334]);
    }

    #[test]
    fn show_table() {
        let mut t = ShowTable::default();
        assert_eq!(t.get(5, 2), 2);
        t.set(5, 0, 2);
        t.set(5, 1, 1);
        t.set(5, 1, 2);
        assert_eq!((t.get(5, 2), t.get(5, 1), t.0.len()), (1, 1, 2));
    }
}
