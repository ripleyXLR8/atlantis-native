//! The place animations' sounds of *Atlantis III* (atlantis3-gog-fr-3473408
//! @ 0x00439130; the CD 2001 build's 0x00438ab0 is the same function).
//!
//! The warp's sprite pass (@ 0x00440bf0) calls 0x00439130 for every shown
//! slot (+0x28), in slot order, on every drawn warp frame (@ 0x00440c61).
//! For a slot in mode 1 (+0x2c) on frame 0 (+0x08), with no dialog running
//! (the byte 0x00755750, set @ 0x0040d130, cleared @ 0x0040e49e), it:
//!
//! 1. names the sound `sfx\` + the first four characters of the slot's
//!    SPW name + `.apc` (0x00445480(…, 0, 4), `.apc` @ 0x0071812c, the
//!    `sfx\` prefix 0x00415df0); nothing more when that file does not
//!    exist (0x004191e0);
//! 2. loads it, `AtlSound_LoadSfx(name, 0, 1)` (0x004329e0), and stops it
//!    if it plays (0x00433780, 0x004335f0);
//! 3. finds its position: for a name starting `pd` (0x00718128, compared
//!    with case), the Coord `Warp` position of the view `VUE`
//!    (0x00402a00, (0, 0, 0) when absent) shifted by (300 k, 0, 100 k),
//!    k = 1, 2, −1, −2 for the third letter `a`, `b`, `c`, `d` (else 0)
//!    (@ 0x00439427..0x004394e0); and a `pd` name with the letter `a` and
//!    a fourth character `1` draws `rand()` and skips the sound when
//!    `rand() % 100 < 20` (@ 0x0043946f..0x00439481); for any other name,
//!    the Coord `Perso` position whose key is the name's first two
//!    characters and its characters 7..9, in upper case (0x00402740:
//!    `Mid(0, 2) + Mid(7, 3)`, `toupper` @ 0x00445760; `g1A2005007` →
//!    `G1007`), (−1, −1, −1) when absent. The view entry names a slot
//!    with the WAM state's letter, upper case in every shipped WAM
//!    (`g1A2005007`), so the `pd` letters `a`..`d` match only the tower's
//!    demon pawns, which its C code names in lower case (`pda1000000`,
//!    @ 0x004d46e0);
//! 4. plays it without a position (0x00433140) when a coordinate is −1
//!    (@ 0x004394ea..0x00439523), else sets the listener (0x00433430) and
//!    plays it in 3D there (0x00433280), as the place sounds.
//!
//! With a dialog running, a shown mode-1 slot's sound that plays is
//! stopped instead (@ 0x00439614..0x004397ba), whatever its frame.

use super::Game;

impl Game {
    /// The sounds of the shown place animations, before the frame's
    /// sprites ([`crate::game`]'s `draw_warp_only`).
    pub(crate) fn anim_sounds_a3(&mut self) {
        let dialog = self.running.is_some();
        let slots: Vec<(String, i32)> = self
            .slots
            .slots
            .iter()
            .filter(|s| s.shown && s.mode == 1)
            .map(|s| (s.name.clone(), s.frame))
            .collect();
        for (name, frame) in slots {
            let short: String = name.chars().take(4).collect();
            let sound = format!("sfx\\{short}.apc");
            if frame == 0 && !dialog {
                self.anim_sound_start(&name, &sound);
            }
            if dialog && self.sfx_playing(&sound) {
                self.sfx_stop(&sound);
            }
        }
    }

    /// Steps 1..4 of the module's rule for the slot `name`.
    fn anim_sound_start(&mut self, name: &str, sound: &str) {
        if self.files.size(&sound.replace('\\', "/")).is_none() {
            return;
        }
        self.sfx_load(sound, 0, 1);
        if !self.sfx_loaded(sound) {
            return;
        }
        if self.sfx_playing(sound) {
            self.sfx_stop(sound);
        }
        let b = name.as_bytes();
        let pos = if b.starts_with(b"pd") {
            let k: i32 = match b.get(2) {
                Some(b'a') => 1,
                Some(b'b') => 2,
                Some(b'c') => -1,
                Some(b'd') => -2,
                _ => 0,
            };
            // The whole-story harness stubs the warp's draw, sprite pass
            // included: no `rand()` there (`stub_rand`).
            if b.get(3) == Some(&b'1') && k == 1 && !self.stub_rand && self.rand.rand() % 100 < 20 {
                return;
            }
            let view = self.script.as_ref().map_or(0, |s| s.vars[s.keys.vue]);
            let w = u32::try_from(view)
                .ok()
                .and_then(|v| self.coord.warps.get(&v).copied())
                .unwrap_or([0.0; 3]);
            let fpu = self.camera.fpu;
            let add = |a: i32, b: f32| fpu.f32(fpu.add(f64::from(a), f64::from(b)));
            [add(300 * k, w[0]), w[1], add(100 * k, w[2])]
        } else {
            self.coord
                .persos
                .get(&anim_perso_key(name))
                .copied()
                .unwrap_or([-1.0; 3])
        };
        if pos.contains(&-1.0) {
            self.sfx_play(sound);
        } else {
            self.sfx_play_3d(sound, pos);
        }
    }
}

impl Game {
    /// Tests: the place's Coord `Warp` position of `view`.
    #[doc(hidden)]
    pub fn test_coord_warp(&self, view: u32) -> Option<[f32; 3]> {
        self.coord.warps.get(&view).copied()
    }
}

/// The Coord `Perso` key of a slot's SPW name (0x00402740): its first two
/// characters and its characters 7..9, in upper case.
pub(crate) fn anim_perso_key(name: &str) -> String {
    let b = name.as_bytes();
    let head = &b[..b.len().min(2)];
    let tail = b.get(7..).map_or(&[][..], |t| &t[..t.len().min(3)]);
    head.iter()
        .chain(tail)
        .map(|&c| char::from(c.to_ascii_uppercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::anim_perso_key;

    #[test]
    fn the_perso_key_of_a_slot() {
        assert_eq!(anim_perso_key("g1A2005007"), "G1007");
        assert_eq!(anim_perso_key("lna1029029"), "LN029");
        assert_eq!(anim_perso_key("ab"), "AB");
        assert_eq!(anim_perso_key("abcdefgh"), "ABH");
    }
}
