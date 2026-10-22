//! The place animations: one slot per entity state of the current view
//! (`AtlSpw_load` @ 0x0047c4e0, at most 0x22), drawn into the cube faces
//! before the warp (@ 0x0045b040 → @ 0x0047cf00) and stepped once per
//! 15 Hz tick (@ 0x0047d7f0), with the random hand-over between the states
//! of one entity (@ 0x0047d9a0).

use atlantis2_data::spw::{Blend, Clip, FaceBox, Spw, FACE};
use atlantis2_data::wam::EntityState;

/// `AtlSpw_load : n>MAX_SPW_VISIBLE`.
pub const MAX_SLOTS: usize = 0x22;

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
    /// +0xc4: the countdown of the random pause.
    pub delay: i32,
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
            delay: state.delay.wrapping_abs(),
        });
    }

    /// FUN_0047d7f0 on a 15 Hz tick: every playing slot one frame on, then
    /// the slots whose play has ended restart, hand over or stop. Returns
    /// the frame the first loop left in the last slot of `CHEVAL`'s id,
    /// whatever its kind (the value it writes into `FRAMECHEVAL`,
    /// @ 0x0047d84d..0x0047d872).
    pub fn step(&mut self) -> Option<i32> {
        let mut horse = None;
        for s in self.slots.iter_mut().filter(|s| s.active) {
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
            // Frame lists (+0x34 ≠ 1) are not ported: every Tibet state is a
            // single animated frame.
            if !s.active || s.state.single != 1 {
                continue;
            }
            if let Some((face, col, row)) = centre {
                if s.click != 0 {
                    let b = s.spw.face_box(s.frame, face).unwrap_or_default();
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
                if let Some(b) = s.spw.face_box(s.frame, face) {
                    if s.spw.draw(s.frame, face, pixels, Clip::FACE, &self.blend) {
                        dirty.push((face, b));
                    }
                }
            }
            if hit.cursor != 0 && click && hit.slot.is_none() && s.click > 0 {
                hit.slot = Some(n);
            }
        }
        (dirty, hit)
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
