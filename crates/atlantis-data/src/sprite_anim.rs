//! Place sprite animation and the active flag of the place sprite table.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/sprite_anim.cpp, GPLv3, the ScummVM team).
//!
//! The active flag (+0x04 of the place sprite table 0x00590510) has these
//! writers only (operand scan of atlantis.exe): the list load FUN_00424a20
//! (0x00424f7a) with FUN_00427d30 for the hidden ids (0x00427d93),
//! HidePerso FUN_00427bd0 (0x00427c57), ShowPerso FUN_00427eb0 (0x00427f30),
//! NEWWSPRANIM / STOPWSPRANIM FUN_00428da8 (0x00428e07, 0x00428ecc) and the
//! end of a type-2 cycle in FUN_00425304 (0x004255d7). Entering a place
//! (FUN_0042431c) writes the frame, the frame count and the SPW buffers
//! only, so a flag set by a script stays until the next list load (a WAM
//! load).

use crate::sprite_list::SpritePlaceEntry;

/// The 200 Hz tick counter of atlantis.exe (0x006ce118, AIL timer set up at
/// 0x004310ac): one tick every 5 ms.
pub const SPRITE_TICK_MS: u32 = 5;

/// Per-row tick of the last frame step (0x006cc578), indexed by the row's
/// index in its view; shared by all views and never reset.
pub const SPRITE_ANIM_SLOTS: usize = 256;

fn view_rows(entries: &[SpritePlaceEntry], vue: u16) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.place_id == vue)
        .map(|(i, _)| i)
        .collect()
}

/// FUN_00425304, animation part, for the rows of view `vue` in list order:
/// an active row with SPW data whose type is not 0, 3 or 4 sets
/// `wspr_frame` to the frame of the view's current animation row (index
/// `current` in the view), then steps its frame when
/// `ticks - last_tick[i] > 13`; when its frame equals its frame count it
/// returns to 0, `wspr_frame` becomes 0 and a type 2 row turns inactive.
/// With `hide_npcs` (0x00496188 == 1, a talking-head line) the persos
/// (ids from 0x200) are skipped. True when a frame or an active flag
/// changed.
pub fn animate_place_sprites(
    entries: &mut [SpritePlaceEntry],
    vue: u16,
    current: usize,
    ticks: u32,
    last_tick: &mut [u32; SPRITE_ANIM_SLOTS],
    wspr_frame: &mut i32,
    hide_npcs: bool,
) -> bool {
    let idx = view_rows(entries, vue);
    let mut changed = false;
    for (k, &i) in idx.iter().enumerate().take(SPRITE_ANIM_SLOTS) {
        let e = &entries[i];
        if !e.active || e.frame_count == 0 {
            continue;
        }
        if hide_npcs && e.perso_id >= 0x200 {
            continue;
        }
        if matches!(e.kind, 0 | 3 | 4) {
            continue;
        }
        *wspr_frame = idx.get(current).map_or(0, |&c| entries[c].frame);
        let e = &mut entries[i];
        if ticks.wrapping_sub(last_tick[k]) > 13 {
            last_tick[k] = ticks;
            e.frame += 1;
            changed = true;
        }
        if e.frame == e.frame_count {
            e.frame = 0;
            *wspr_frame = 0;
            if e.kind == 2 {
                e.active = false;
            }
            changed = true;
        }
    }
    changed
}

/// FUN_00428da8, NEWWSPRANIM=n (1-based): activates the view's row n - 1,
/// makes it the current animation row and resets its frame. False when
/// there is no such row.
pub fn start_place_sprite_anim(
    entries: &mut [SpritePlaceEntry],
    vue: u16,
    n: i32,
    current: &mut usize,
) -> bool {
    let idx = view_rows(entries, vue);
    if n < 1 || n as usize > idx.len() {
        return false;
    }
    let e = &mut entries[idx[n as usize - 1]];
    e.active = true;
    *current = n as usize - 1;
    e.frame = 0;
    true
}

/// FUN_00428da8, STOPWSPRANIM=n: deactivates the view's row n - 1 and
/// resets its frame. False when there is no such row.
pub fn stop_place_sprite_anim(entries: &mut [SpritePlaceEntry], vue: u16, n: i32) -> bool {
    let idx = view_rows(entries, vue);
    if n < 1 || n as usize > idx.len() {
        return false;
    }
    let e = &mut entries[idx[n as usize - 1]];
    e.active = false;
    e.frame = 0;
    true
}

/// FUN_00427d30, run by the list load for each row: a row whose id is in
/// the hidden-perso list starts inactive.
pub fn apply_hidden(entries: &mut [SpritePlaceEntry], hidden: &[i32]) {
    for e in entries.iter_mut() {
        if e.perso_id > 0 && hidden.contains(&e.perso_id) {
            e.active = false;
        }
    }
}

/// FUN_00427eb0 (ShowPerso, active) / FUN_00427bd0 (HidePerso, inactive):
/// every row of the perso, in every view.
pub fn set_perso_active(entries: &mut [SpritePlaceEntry], perso: i32, active: bool) {
    for e in entries.iter_mut().filter(|e| e.perso_id == perso) {
        e.active = active;
    }
}

/// FUN_0042431c for view `vue`: frame 0 (the caller sets the frame count
/// from the SPW header); the active flag is left as it is.
pub fn enter_place(entries: &mut [SpritePlaceEntry], vue: u16) {
    for e in entries.iter_mut().filter(|e| e.place_id == vue) {
        e.frame = 0;
    }
}

/// FUN_0041bfa0 @ 0x0041c61d..0x0041c6a0, once per warp frame after the
/// frame is drawn: a left click in bow mode is consumed and, when
/// AngleWarpY < 20, sets TirFleche to 1; then, while TirFleche is not 0,
/// the arrow frame (0x0049619d) steps by one and at 15 returns to 0 with
/// TirFleche. True when the click was consumed.
pub fn bow_frame_step(
    bow_mode: bool,
    click: bool,
    angle_warp_y: i32,
    tir_fleche: &mut i32,
    arrow_frame: &mut u8,
) -> bool {
    let mut consumed = false;
    if click && bow_mode {
        consumed = true;
        if angle_warp_y < 20 {
            *tir_fleche = 1;
        }
    }
    if *tir_fleche != 0 {
        *arrow_frame = arrow_frame.wrapping_add(1);
        if *arrow_frame == 15 {
            *arrow_frame = 0;
            *tir_fleche = 0;
        }
    }
    consumed
}

#[cfg(test)]
mod tests {
    use super::*;

    // From the static reading of FUN_0041bfa0 @ 0x0041c61d..0x0041c6a0: the
    // click is consumed only in bow mode and shoots only when
    // AngleWarpY < 20; the arrow frame steps while TirFleche is set and
    // both reset at 15.
    #[test]
    fn bow_frame_steps() {
        let (mut tir, mut frame) = (0, 0u8);
        assert!(!bow_frame_step(false, true, 0, &mut tir, &mut frame));
        assert_eq!((tir, frame), (0, 0));
        assert!(bow_frame_step(true, true, 20, &mut tir, &mut frame));
        assert_eq!(tir, 0);
        assert!(bow_frame_step(true, true, 19, &mut tir, &mut frame));
        assert_eq!((tir, frame), (1, 1));
        for i in 2..15 {
            assert!(!bow_frame_step(true, false, 0, &mut tir, &mut frame));
            assert_eq!((tir, frame), (1, i));
        }
        bow_frame_step(true, false, 0, &mut tir, &mut frame);
        assert_eq!((tir, frame), (0, 0));
        // A script-set TirFleche runs the arrow outside bow mode too.
        tir = 1;
        bow_frame_step(false, false, 0, &mut tir, &mut frame);
        assert_eq!(frame, 1);
    }

    #[test]
    fn start_stop_rows_of_a_view() {
        let row = |place_id, active| SpritePlaceEntry {
            place_id,
            active,
            ..SpritePlaceEntry::default()
        };
        let mut es = vec![row(1, false), row(2, false), row(2, true)];
        let mut current = 0;
        assert!(start_place_sprite_anim(&mut es, 2, 1, &mut current));
        assert!(es[1].active && current == 0);
        assert!(stop_place_sprite_anim(&mut es, 2, 2));
        assert!(!es[2].active);
        assert!(!start_place_sprite_anim(&mut es, 2, 3, &mut current));
        assert!(!stop_place_sprite_anim(&mut es, 2, 0));
    }
}
