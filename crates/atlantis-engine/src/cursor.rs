//! The warp cursor of `atlantis.exe` `FUN_00422fcc`, called once per
//! warp-loop frame by `FUN_0041bfa0`: which CURSEURS.SPR sprite shows what
//! the pointer is over (a place to go to, an object, a character) and its
//! animation.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`warp_cursor.cpp`;
//! GPLv3, the ScummVM team).

/// The animation state; it lives in globals of the original, all whole
/// numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarpCursorState {
    /// 0x004968ac: 0 none, 1/4 click, 2 object, 3 talk, 5 navigate.
    pub mode: u8,
    /// 0x004968b0: the CURSEURS.SPR sprite of the next frame.
    pub frame: i32,
    /// 0x004968bc: +1 or -1.
    pub step: i32,
    /// 0x004968b4.
    pub last: i32,
    /// 0x00496994.
    pub first: u8,
    /// 0x004968b8: set by the click handler `FUN_004278a0`.
    pub click_anim: bool,
}

impl Default for WarpCursorState {
    fn default() -> Self {
        WarpCursorState {
            mode: 0,
            frame: 0,
            step: 1,
            last: 0,
            first: 0,
            click_anim: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarpCursorInput {
    /// `FUN_00422bb0`: the zone under the cursor.
    pub zone: u32,
    /// `FUN_00422d5c`: the object or character id under the cursor.
    pub id: u32,
    /// 0x611210\[player\]: the held object, 0 for none.
    pub held: u32,
    /// 0x0049616c: set while `FUN_004212cc` turns the view.
    pub hidden: bool,
    /// 0x004960c8 == 1 (edge-scroll look mode).
    pub edge_scroll: bool,
    /// The 2001 build's rule for ids 0x219 / 0x21a, also in the US
    /// retail CD's build (`ExeBuild::cursor_id_rule`).
    pub id_rule: bool,
}

impl Default for WarpCursorInput {
    fn default() -> Self {
        WarpCursorInput {
            zone: 0,
            id: 0,
            held: 0,
            hidden: false,
            edge_scroll: false,
            id_rule: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarpCursorResult {
    /// The CURSEURS.SPR sprite drawn this frame, -1 for none.
    pub sprite: i32,
    /// 0x006cd7f8: the zone a click acts on.
    pub zone: u32,
    /// 0x006cd7e8: the id a click acts on.
    pub id: u32,
}

fn start_anim(st: &mut WarpCursorState, mode: u8, frame: i32, last_add: i32, first_add: i32) {
    st.mode = mode;
    st.step = 1;
    st.frame = frame;
    st.last = frame + last_add;
    st.first = (frame + first_add) as u8;
}

/// One frame of `FUN_00422fcc`.
pub fn warp_cursor_step(st: &mut WarpCursorState, input: &WarpCursorInput) -> WarpCursorResult {
    // Zone and id the cursor acts on (0x00422fe8..0x00423138). While an
    // object is held, ordinary zones (below 1000) do not count; zones
    // 2001..2999 and above 3000 take the held object onto the id.
    let mut zone = input.zone;
    let mut id = input.id;
    if zone < 1000 && input.held != 0 {
        zone = 0;
    }
    if zone > 2000 && zone < 3000 && zone - 2000 == input.held {
        id = zone - 2000;
        zone = 0;
    } else if zone > 2000 && zone < 3000 && id == 0 {
        zone = 0;
    }
    if zone > 3000 {
        id = if zone - 3000 == input.held {
            zone - 3000
        } else {
            0
        };
        zone = 0;
    }
    let mut res = WarpCursorResult {
        sprite: -1,
        zone,
        id,
    };

    // Ids 0x219 and 0x21a show the navigate cursor (0x0042313d).
    if input.id_rule && (id == 0x219 || id == 0x21a) {
        id = 0;
        zone = 100;
    }

    if !st.click_anim && !input.hidden {
        if (0x100..=0x1ff).contains(&id) {
            if st.mode != 2 {
                start_anim(st, 2, 18, 4, 0);
            }
        } else if id < 0x201 && zone < 0x201 {
            if zone == 0 || zone > 0x1ff {
                start_anim(st, 0, 0, 4, 0); // every frame: nothing drawn
            } else if st.mode != 5 {
                start_anim(st, 5, 9, 5, 0);
            }
        } else if st.mode != 3 {
            start_anim(st, 3, 26, 20, 0);
        }

        // Frame 0 is not drawn (0x0042330d). Objects and navigation go back
        // and forth; the talk cursor loops 26..45.
        if st.frame != 0 {
            res.sprite = st.frame;
            st.frame += st.step;
            if (st.frame == st.last || st.frame == i32::from(st.first)) && st.mode != 3 {
                st.step = -st.step;
            }
            if st.frame == st.last && st.mode == 3 {
                st.frame = 26;
            }
        }
        // Edge-scroll mode draws sprite 5 when nothing animates (0x00423470).
        if input.edge_scroll && st.frame == 0 {
            res.sprite = 5;
        }
    }

    // The click animation (0x0042353b): mode 4 (no zone) draws sprite 0 for
    // one frame, as its end test compares with (uint8)-3; mode 1 plays
    // 3..7..1. Navigate (mode 5) continues from where it is.
    if st.click_anim {
        if zone == 0 {
            if st.mode != 4 && st.mode != 1 {
                start_anim(st, 4, 0, 4, -3);
            }
        } else if st.mode != 1 && st.mode != 4 && st.mode != 5 {
            start_anim(st, 1, 3, 4, -3);
        }
        res.sprite = st.frame;
        st.frame += st.step;
        if st.frame == st.last {
            st.step = -1;
        }
        if st.frame <= i32::from(st.first) {
            st.click_anim = false;
            st.mode = 0;
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    // CURSEURS.SPR sprite drawn on 16 consecutive frames, -1 = none.
    // Original: FUN_00422fcc emulated for both look modes (0x004960c8 = 0
    // mouse-look, 1 edge-scroll); "click" presets 0x004968b8 as
    // FUN_004278a0 does.
    #[test]
    fn sequences_match_original() {
        type Seq = [[i32; 16]; 2];
        let none: [i32; 16] = [-1; 16];
        let fives: [i32; 16] = [5; 16];
        let nav = [9, 10, 11, 12, 13, 14, 13, 12, 11, 10, 9, 10, 11, 12, 13, 14];
        let talk = [
            26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41,
        ];
        let obj = [
            18, 19, 20, 21, 22, 21, 20, 19, 18, 19, 20, 21, 22, 21, 20, 19,
        ];
        let mut click_nothing = [[-1; 16], [5; 16]];
        click_nothing[0][0] = 0;
        click_nothing[1][0] = 0;
        let click_zone = [3, 4, 5, 6, 7, 6, 5, 4, 3, 2, 1, 9, 10, 11, 12, 13];
        let click_npc = [
            0, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
        ];
        let cases: [(&str, u32, u32, u32, bool, Seq); 13] = [
            ("nothing", 0, 0, 0, false, [none, fives]),
            ("navigation zone 12", 12, 0, 0, false, [nav, nav]),
            ("npc 600", 0, 600, 0, false, [talk, talk]),
            ("scene object 300", 0, 300, 0, false, [obj, obj]),
            (
                "zone 12 while holding 300",
                12,
                0,
                300,
                false,
                [none, fives],
            ),
            ("id 0x219", 0, 0x219, 0, false, [nav, nav]),
            ("zone 2300 holding 300", 2300, 0, 300, false, [obj, obj]),
            ("zone 2300 holding 301", 2300, 0, 301, false, [none, fives]),
            ("zone 3300 holding 300", 3300, 0, 300, false, [obj, obj]),
            ("zone 600", 600, 0, 0, false, [talk, talk]),
            ("click, nothing", 0, 0, 0, true, click_nothing),
            ("click, zone 12", 12, 0, 0, true, [click_zone, click_zone]),
            ("click, npc 600", 0, 600, 0, true, [click_npc, click_npc]),
        ];
        for (name, zone, id, held, click, want) in cases {
            for (mode, want) in want.iter().enumerate() {
                let mut st = WarpCursorState {
                    click_anim: click,
                    ..WarpCursorState::default()
                };
                let input = WarpCursorInput {
                    zone,
                    id,
                    held,
                    edge_scroll: mode == 1,
                    ..WarpCursorInput::default()
                };
                let got: Vec<i32> = (0..16)
                    .map(|_| warp_cursor_step(&mut st, &input).sprite)
                    .collect();
                assert_eq!(got, want, "{name} (mode {mode})");
            }
        }
    }

    // The GOG build has no 0x219 / 0x21a rule: the talk cursor.
    #[test]
    fn id_rule_off() {
        let mut st = WarpCursorState::default();
        let input = WarpCursorInput {
            id: 0x219,
            id_rule: false,
            ..WarpCursorInput::default()
        };
        assert_eq!(warp_cursor_step(&mut st, &input).sprite, 26);
    }
}
