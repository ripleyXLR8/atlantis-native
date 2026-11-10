//! The full-screen loop modes' frames (the main loop's table
//! @ 0x004d5e30): what each one does to the script's world on a frame,
//! with the click bytes of that frame (left @ 0x007592b8, right
//! @ 0x007592b9); the screens test their bank's sprites at the pointer
//! themselves (the hit test @ 0x00437080, one call per sprite index in
//! the loop's order). A
//! handler returning 0 makes the main loop call the mode's `Stop`
//! ([`Game::stop_screen`]).
//!
//! The two maps, the jewel case, the game boy, the remote and the
//! unicorn's 3D board ([`crate::licorne`]) are drawn ([`crate::maps`],
//! `screens_draw`); every hit test is the bank's own (`SPR_COLLISION`)
//! at the pointer.
//!
//! Each handler has one call of the flip (@ 0x0040fdf0, the texts of
//! @ 0x0040de60: `Game::flip_texts`), after its drawing and before its
//! clicks; the paths that leave before drawing do not flip
//! [CONFIRMED-STATIC]: the map's refusals (@ 0x004057ec), the scarab's
//! close with `VUE` > 27 (@ 0x004320fc), the jewel case's tick with the
//! three gems taken (@ 0x004131f4), the game boy's sixth blink
//! (@ 0x0041fb6f), the unicorn's win (@ 0x0042f82c). The draws' other
//! guards are the screens' open bytes (map 0x0074e960, tower map
//! 0x0074ebac, scarab 0x0075931c, game boy 0x0075604c, remote
//! 0x00789724), set by the starts and 1 while the screen runs; the
//! skull's handler always flips.

use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{Game, Hx, Screen};
use crate::maps;

/// The screens' globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// `SCARABEE`: the case open (@ 0x00759324; not reset by the start).
    pub scarab_open: bool,
    /// `ECRIN`: the gems taken (@ 0x00755990..0x00755992), their count
    /// (@ 0x0075599c), the fade (@ 0x00715860: 0.0, then 1.0 from the
    /// first tick), the 15 Hz clock's last time (@ 0x00755978), the glow
    /// cursor (@ 0x00755998: `Interface.spr` 13..24 on a gem, 40 none)
    /// and the case bank `Ecrin%d.spr` (@ 0x00755960, %d the display's
    /// depth).
    pub ecrin_taken: [bool; 3],
    pub ecrin_count: i32,
    pub ecrin_fade: f32,
    pub ecrin_clock: f64,
    pub ecrin_cursor: i32,
    pub ecrin_bank: Option<Arc<Bank>>,
    /// `LICORNE`: the board object (@ 0x00759140, built by the start;
    /// never freed by the stop).
    pub licorne: Option<Box<crate::licorne::Board>>,
    /// `GBOY` ([`Gboy`]); its result is the tower's dice (@ 0x00756060,
    /// `tourmagi::State::dice`).
    pub gboy: Gboy,
    /// `GBoy.spr` (@ 0x00756020) and `Boitier.spr` (@ 0x00755ef0), loaded
    /// with `tourmagi.wam` (@ 0x0041fd80).
    pub gboy_bank: Option<Arc<Bank>>,
    pub boitier_bank: Option<Arc<Bank>>,
    /// `AtlCarte`'s bank `Carte.spr` (@ 0x0074e928), loaded with
    /// `ldmorts.wam` (@ 0x00405bf0).
    pub carte_bank: Option<Arc<Bank>>,
    /// `AtlCarteTour`'s banks `CarteTour<W>.spr` (@ 0x0074e968) and
    /// `PionsTour.spr` (@ 0x0074eb98), loaded by its first start
    /// (@ 0x00406a10), and its pawns' animation.
    pub tour_board: Option<Arc<Bank>>,
    pub tour_pawns: Option<Arc<Bank>>,
    pub tour_anim: maps::TourAnim,
    /// `TELECOMMANDE`'s bank `Telecommande.spr` (@ 0x00789710), opened
    /// again by every WAM load (@ 0x004c3799 → @ 0x004d0c30).
    pub tele_bank: Option<Arc<Bank>>,
    /// The remote's hover animation (@ 0x004d0040), by button in the
    /// loop's order ([`TELE_BUTTONS`]: power, Bagdad, Paleo, home): the
    /// hovered bytes (@ 0x0078972e, 0x0078972d, 0x0078972c, 0x0078972f),
    /// the frames (@ 0x0078973c, 0x00789738, 0x00789734, 0x00789740) and
    /// the clock's last time (@ 0x007896d0). Neither the start nor the
    /// stop resets them (only @ 0x004cffc0 does).
    pub tele_hover: [bool; 4],
    pub tele_frames: [i32; 4],
    pub tele_clock: f64,
}

/// The game boy's globals (`AtlGBoy`, @ 0x0041f660..0x0041fd80).
#[derive(Debug, Clone, Default)]
pub struct Gboy {
    /// The running value 43..141 (@ 0x00716220; 42 at the start).
    pub cursor: i32,
    /// The index picked by the click (@ 0x00716224) and its copy
    /// (@ 0x0071622c), −1 none.
    pub pick: i32,
    pub k: i32,
    /// The count animation's frame (@ 0x00716228), −1 none.
    pub anim: i32,
    /// The button hit this frame (@ 0x0075605c).
    pub hit: bool,
    /// The result phase (@ 0x00756054) and its blinks (@ 0x00756058).
    pub phase: bool,
    pub blinks: i32,
    /// The six faces' frames (@ 0x00755ed8[6]: 7i at rest).
    pub faces: [i32; 6],
    /// The count started (@ 0x00755fb2).
    pub go: bool,
    /// The 15 Hz accumulator (@ 0x00756064) and its last time
    /// (@ 0x00756068), floats; 0 = no frame yet.
    pub acc: f32,
    pub last: f32,
    /// The start's time (@ 0x00755fd0) and the result phase's clock
    /// (@ 0x00755f08), doubles of the game's clock.
    pub start: f64,
    pub blink_clock: f64,
}

/// The game boy's tables (`AtlGBoy_MainInit` @ 0x0041f3c0): the picks'
/// thresholds (@ 0x00755fb4), the cursor offsets once the threshold is
/// reached (@ 0x00756034), the count animations' first and last frames
/// (@ 0x00755ec0, @ 0x00755fe8), the frames where each face starts
/// (@ 0x00755f20 + 24k, −1 none), the range of a click (@ 0x00716230,
/// @ 0x00716234).
const GBOY_THRESHOLDS: [i32; 6] = [116, 123, 126, 128, 129, 133];
const GBOY_OFFSETS: [i32; 6] = [26, 46, 62, 76, 89, 98];
const GBOY_ANIM: [(i32, i32); 6] = [
    (241, 260),
    (261, 284),
    (285, 312),
    (313, 344),
    (345, 380),
    (381, 420),
];
const GBOY_FACES: [[i32; 6]; 6] = [
    [255, -1, -1, -1, -1, -1],
    [278, 273, -1, -1, -1, -1],
    [305, 300, 295, -1, -1, -1],
    [335, 330, 325, 320, -1, -1],
    [370, 364, 359, 353, 349, -1],
    [417, 411, 406, 399, 394, 387],
];
const GBOY_RANGE: (i32, i32) = (113, 141);
/// The cursor's step (@ 0x007003d4, a float 1/15).
const GBOY_STEP: f32 = 1.0 / 15.0;

impl State {
    pub fn new() -> State {
        State::default()
    }

    /// No byte of 0x00459e70 belongs to the screens (the remote's
    /// 0x0071f0e8, cleared by its start, is read by no code).
    pub fn after_load(&mut self) {}
}

/// The ECRIN and SCARABEE clocks' period (@ 0x00715868, @ 0x00717730:
/// 15.0 → 1/15 s).
const TICK: f64 = 1.0 / 15.0;
/// The game boy's result blink period (@ 0x00716218: 0.3 s).
const GBOY_BLINK: f64 = 0.3;

/// The game boy's anchors from the screen's size (@ 0x0041f80d..
/// 0x0041f84b): the box (W − 400, H − 300), the game boy (W − 250,
/// H − 210), the button (W − 97, H − 32).
pub(crate) fn gboy_anchors(w: i32, h: i32) -> [(i32, i32); 3] {
    [(w - 400, h - 300), (w - 250, h - 210), (w - 97, h - 32)]
}

/// A start's own part, after the common one ([`Hx::start_screen`]).
pub(super) fn start(h: &mut Hx, screen: Screen) {
    let now = h.now();
    let st = &mut h.g.puzzles.screens;
    match screen {
        // @ 0x00412e50: nothing taken, the fade at 0, the clock at the
        // click's time, the pointer centred (@ 0x00431780), the cursor
        // reset (@ 0x004282e0) and `Interface.spr` #40 (@ 0x00427f80).
        Screen::Ecrin => {
            st.ecrin_taken = [false; 3];
            st.ecrin_count = 0;
            st.ecrin_fade = 0.0;
            st.ecrin_clock = now;
            st.ecrin_cursor = ECRIN_NO_GEM;
            h.g.pointer = maps::centre(h.g.width, h.g.height);
        }
        // @ 0x0041f660: the start's time, nothing picked, the clocks at
        // 0, the cursor 42, the result 0, the faces at rest, the pointer
        // centred (@ 0x00431780), the cursor `Interface.spr` #40
        // (@ 0x00427f80).
        Screen::GBoy => {
            st.gboy = Gboy {
                cursor: 42,
                pick: -1,
                k: -1,
                anim: -1,
                hit: false,
                phase: false,
                blinks: 0,
                faces: [0, 7, 14, 21, 28, 35],
                go: false,
                acc: 0.0,
                last: 0.0,
                start: now,
                blink_clock: st.gboy.blink_clock,
            };
            h.g.puzzles.tourmagi.dice = 0;
            h.g.pointer = maps::centre(h.g.width, h.g.height);
        }
        // @ 0x00405450: the pointer centred (@ 0x00431780), the cursor
        // `Interface.spr` #40 (@ 0x00427f80), alpha 1.0.
        Screen::Carte => {
            h.g.pointer = maps::centre(h.g.width, h.g.height);
        }
        // @ 0x00406620: the banks loaded when one is empty (@ 0x00406a10),
        // the pawns' clock at now, the pointer centred (@ 0x00431780), the
        // cursor reset (@ 0x004282e0), alpha 1.0, the pawns' first frames
        // (A @ 0x00789ad8, B @ 0x00789ad4).
        Screen::CarteTour => {
            h.g.tour_banks();
            let t = &h.g.puzzles.tourmagi;
            let anim = maps::TourAnim::start(t.demon, t.player, now);
            h.g.puzzles.screens.tour_anim = anim;
            h.g.pointer = maps::centre(h.g.width, h.g.height);
        }
        // @ 0x0042e7a0: the board object built and loaded (@ 0x004b9ca0,
        // @ 0x004b9e40), the run reset, the pointer centred (@ 0x00431780),
        // the cursor `Interface.spr` (@ 0x00427f80).
        Screen::Licorne => {
            let (w, ht) = (h.g.width, h.g.height);
            let board = crate::licorne::Board::new(&h.g.files, w, ht, now);
            if board.is_none() {
                log::warn!("Licorne: the board's files are missing ({w} x {ht})");
            }
            h.g.puzzles.screens.licorne = board.map(Box::new);
            h.g.pointer = maps::centre(w, ht);
        }
        // @ 0x004cfee0 (0x0071f0e8 := 0, read by nobody): the pointer
        // centred (@ 0x00431780), the cursor `Interface.spr` #40
        // (@ 0x00427f80).
        Screen::Telecommande => {
            h.g.pointer = maps::centre(h.g.width, h.g.height);
        }
        // @ 0x00431da0, 0x00409a40: nothing the script sees besides the
        // common part.
        _ => {}
    }
}

/// One frame of the running screen; false = the handler returned 0.
pub(super) fn frame(h: &mut Hx, screen: Screen, left: bool, right: bool) -> bool {
    match screen {
        Screen::Scarabee => scarabee(h, left, right),
        Screen::Carte => carte(h, left, right),
        Screen::CarteTour => {
            // @ 0x00406780: drawn, then any click closes (@ 0x004069c9).
            h.g.tour_draw();
            h.g.flip_texts();
            !(left || right)
        }
        Screen::Telecommande => telecommande(h, left, right),
        Screen::CraneBagdad => crane(h, left, right),
        Screen::Ecrin => ecrin(h, left),
        Screen::GBoy => gboy(h, left, right),
        Screen::Licorne => licorne(h, left, right),
    }
}

/// `SCARABEE` (@ 0x00431ea0). The case open, by flags 53 / 54: the dead
/// scarab's subjects once (flag 72), the larva's (flag 56), the golden
/// scarab's (flag 57); with both flags and `VUE` > 27 the case closes,
/// `flag570` := 1 and the loop leaves (@ 0x004320eb) before the clicks
/// are looked at. Then a left click on the closed case opens it with
/// `(PlaySound=110)`; a right click closes it and leaves.
fn scarabee(h: &mut Hx, left: bool, right: bool) -> bool {
    if h.g.puzzles.screens.scarab_open {
        let (f53, f54) = (h.flag(53), h.flag(54));
        if f53 == 0 {
            if f54 == 0 && h.flag(72) == 0 {
                h.set_flag(72, 1);
                h.run("(killsuj,CHF_PSYCHOP,DIAL_ECRIN_SCARABEE)");
                h.run("(enablesuj,CHF_PSYCHOP,DIAL_SCARABEE)");
            }
            // visual: frames 1 and the dead scarab's animation 5..24 on
            // the 1/15 s clock (0x00717738) (M4).
        } else if f53 == 1 {
            if f54 == 0 {
                if h.flag(56) == 0 {
                    h.set_flag(56, 1);
                    h.run("(enablesuj,CHF_PSYCHOP,DIAL_LARVE)");
                    h.run("(killsuj,CHF_PSYCHOP,DIAL_SCARABEE)");
                }
            } else if f54 == 1 {
                if h.vue() > 27 {
                    h.g.puzzles.screens.scarab_open = false;
                    h.set_flag(570, 1);
                    return false;
                }
                if h.flag(57) == 0 {
                    h.set_flag(57, 1);
                    h.run("(enablesuj,CHF_PSYCHOP,DIAL_SCARABEEOR)");
                    h.run("(killsuj,CHF_PSYCHOP,DIAL_LARVE)");
                    h.run("(killsuj,CHF_PSYCHOP,DIAL_SCARABEE)");
                }
            }
        }
    }
    // The flip (@ 0x004321a7), before the clicks.
    h.g.flip_texts();
    if left && !h.g.puzzles.screens.scarab_open {
        h.run("(PlaySound=110)");
        h.g.puzzles.screens.scarab_open = true;
    }
    if right {
        h.g.puzzles.screens.scarab_open = false;
        return false;
    }
    true
}

/// `CARTE`, the Land of the Dead's map (@ 0x00405700). Every frame
/// first, before anything is drawn: more than one feather carried (flags
/// 535..544, less one for each pan of the scale holding one, flags 562 /
/// 563) → `TROP_DE_PLUMES` := 1 and leave; the high priest not met
/// (`flag80` = 0) → `GP_PAS_VU` := 1 and leave. Then the map is drawn
/// ([`maps::draw_carte`]). A left click on map sprite k = 1..5 at its
/// drawn position (the pointer's hit)
/// → chapter 12, 14, 13, 15, 16 (all but 15: the fisherman gone once his
/// scale is taken, flag 69); the current chapter → nothing; else
/// `LDM_CHAPITRE_DEPART` := the chapter, `LDM_CHAPITRE_ARRIVEE` := the
/// target, `(chapter=500)` and leave. A left click on nothing clears the
/// clicks; a right click leaves.
fn carte(h: &mut Hx, left: bool, right: bool) -> bool {
    if let Some(r) = maps::carte_refusal(|f| h.flag(f as usize)) {
        h.set_var(r.var(), 1);
        return false;
    }
    h.g.carte_draw();
    // The flip (@ 0x00405936), before the clicks.
    h.g.flip_texts();
    if !left {
        return !right;
    }
    let hit = h.g.carte_hit();
    let target = match hit {
        Some(k @ 1..=5) => maps::CARTE_CHAPTERS[k as usize - 1],
        _ => return true,
    };
    if maps::carte_fishermen(target, h.flag(69), h.flag(560)) {
        h.set_flag(560, 1);
        h.run("(HidePerso=LDM_PECHEUR01)");
        h.run("(HidePerso=LDM_PECHEUR02)");
        h.run("(HideObjet=LDM_POISSON2)");
    }
    let chapter = h.chapter();
    if chapter == target {
        return true;
    }
    h.set_var("LDM_CHAPITRE_DEPART", chapter);
    h.set_var("LDM_CHAPITRE_ARRIVEE", target);
    h.run("(chapter=500)");
    false
}

/// The remote's buttons in the loop's order (@ 0x004d0550, @ 0x004d0040):
/// the first sprite, the hit test's position from the screen's centre,
/// the drawing's x from the centre (the power button is drawn 2 pixels
/// left of where it is tested: −0x9a @ 0x004d0670, −0x98 @ 0x004d07aa)
/// and the last hover frame.
pub(crate) const TELE_BUTTONS: [(i32, (i32, i32), i32, i32); 4] = [
    (0x00, (-0x98, -0xd4), -0x9a, 13),
    (0x0e, (-0x8c, -0x13), -0x8c, 14),
    (0x1d, (0x30, -0x91), 0x30, 14),
    (0x2c, (-0x81, -0x8c), -0x81, 14),
];
/// The remote's picture: frame 0x3b, 0x3c once powered (flag 404).
pub(crate) const TELE_BODY: (usize, usize) = (0x3b, 0x3c);

/// `SPR_COLLISION` (@ 0x00437080) of the remote's button `k`
/// ([`TELE_BUTTONS`]) on a `w` × `h` screen at the pointer.
pub(in crate::game) fn tele_hit(bank: &Bank, k: usize, w: i32, h: i32, ptr: (i32, i32)) -> bool {
    let (sprite, (dx, dy), _, _) = TELE_BUTTONS[k];
    let (cx, cy) = (w / 2, h / 2);
    bank.opaque_at(sprite as usize, Some(cx + dx), cy + dy, ptr.0, ptr.1)
}

/// The remote's first button under the pointer, in the loop's order.
fn tele_button(h: &Hx) -> Option<usize> {
    let bank = h.g.puzzles.screens.tele_bank.as_ref()?;
    let (w, ht) = (h.g.width as i32, h.g.height as i32);
    (0..4).find(|&k| tele_hit(bank, k, w, ht, h.g.pointer))
}

/// @ 0x004cffc0: every button's hover byte and frame cleared.
fn tele_hover_reset(st: &mut State) {
    st.tele_hover = [false; 4];
    st.tele_frames = [0; 4];
}

/// The remote's hover animation (@ 0x004d0040), after the drawing: the
/// first button under the pointer (power only while flag 404 = 0; home
/// only outside `hoggar.wam`, the others only in it) starts its cycle
/// (every button reset, its byte set, the clock at now), then moves its
/// frame on every 1/15 s (power 0..13, the others 0..14). A button of
/// the wrong place, or none, resets them all; the power button once
/// powered leaves them as they are.
fn tele_hover(h: &mut Hx) {
    let hit = tele_button(h);
    let hoggar = h.g.wam_is("hoggar.wam");
    let powered = h.flag(404) != 0;
    let now = h.now();
    let st = &mut h.g.puzzles.screens;
    let Some(k) = hit else {
        tele_hover_reset(st);
        return;
    };
    if k == 0 && powered {
        return;
    }
    if !st.tele_hover[k] {
        tele_hover_reset(st);
        if hoggar == (k != 3) {
            st.tele_hover[k] = true;
            st.tele_clock = now;
        }
        return;
    }
    if now - st.tele_clock > TICK {
        st.tele_clock = now;
        let last = TELE_BUTTONS[k].3;
        let f = &mut st.tele_frames[k];
        *f = if *f < last { *f + 1 } else { 0 };
    }
}

/// `TELECOMMANDE`, the remote (@ 0x004d0550). Drawn
/// ([`Game::tele_draw`]) and flipped, then the hover animation
/// ([`tele_hover`]), then a left click: the first one shows the Atlantean
/// figure (`flag427` := 1, `flag425` := 0); then the first button under
/// the pointer, in the loop's order: in `hoggar.wam` button 0 = the
/// Hoggar (`flag404` := 1, `(PlaySound=51)` and the hover reset once),
/// button 0x0e = Bagdad (flag 400, or 401 after flag 908), button 0x1d =
/// Paleo (flag 402, or 403 after flag 605); elsewhere button 0x2c = home
/// (the hover reset, `(Chapter=607)`). The loop stays; a right click
/// leaves.
fn telecommande(h: &mut Hx, left: bool, right: bool) -> bool {
    let powered = h.flag(404) != 0;
    h.g.tele_draw(powered);
    // The flip (@ 0x004d0742), before the hover and the clicks.
    h.g.flip_texts();
    tele_hover(h);
    if !left {
        return !right;
    }
    if h.flag(427) == 0 {
        h.set_flag(427, 1);
        h.set_flag(425, 0);
        h.run("(ShowPerso=CMP_ATLANTEFIGE)");
        h.run("(HidePerso=CMP_ATLANTE)");
    }
    let hoggar = h.g.wam_is("hoggar.wam");
    let bank = h.g.puzzles.screens.tele_bank.clone();
    let (w, ht, ptr) = (h.g.width as i32, h.g.height as i32, h.g.pointer);
    let hit = |k: usize| bank.as_ref().is_some_and(|b| tele_hit(b, k, w, ht, ptr));
    if hit(0) && hoggar {
        if h.flag(404) == 0 {
            h.set_flag(404, 1);
            h.run("(PlaySound=51)");
            tele_hover_reset(&mut h.g.puzzles.screens);
        }
    } else if hit(1) && hoggar {
        hide_worlds(h);
        let show = if h.flag(908) == 0 {
            h.set_flag(400, 1);
            h.set_flag(401, 0);
            h.set_flag(402, 0);
            h.set_flag(403, 0);
            "(ShowObjet=CMP_BAGDAD)"
        } else {
            h.set_flag(400, 0);
            h.set_flag(401, 1);
            h.set_flag(402, 0);
            h.set_flag(403, 0);
            "(ShowObjet=CMP_BAGDADFIN)"
        };
        h.run(show);
        h.run("(PlaySound=50)");
    } else if hit(2) && hoggar {
        hide_worlds(h);
        h.set_flag(400, 0);
        h.set_flag(401, 0);
        let show = if h.flag(605) == 0 {
            h.set_flag(402, 1);
            h.set_flag(403, 0);
            "(ShowObjet=CMP_PALEO)"
        } else {
            h.set_flag(402, 0);
            h.set_flag(403, 1);
            "(ShowObjet=CMP_PALEOFIN)"
        };
        h.run(show);
        h.run("(PlaySound=50)");
    } else if hit(3) && !hoggar {
        // @ 0x004cffc0, then 0x0078972f := 0 again.
        tele_hover_reset(&mut h.g.puzzles.screens);
        h.run("(HideObjet=CMP_PALEO)");
        h.run("(HideObjet=CMP_PALEOFIN)");
        h.run("(HideObjet=CMP_BAGDAD)");
        h.run("(HideObjet=CMP_BAGDADFIN)");
        h.run("(PlaySound=50)");
        h.run("(Chapter=607)");
    }
    true
}

/// The remote's five destination objects hidden (@ 0x004d0912).
fn hide_worlds(h: &mut Hx) {
    h.run("(HideObjet=CMP_BAGDAD)");
    h.run("(HideObjet=CMP_BAGDADFIN)");
    h.run("(HideObjet=CMP_HOGGAR)");
    h.run("(HideObjet=CMP_PALEO)");
    h.run("(HideObjet=CMP_PALEOFIN)");
}

/// `CRANEBAGDAD`, a crystal skull (@ 0x00409b40): a left click goes to
/// the interdimension (as @ 0x00409cc0, with `flag901` := 1 before the
/// commands) and leaves; a right click leaves.
fn crane(h: &mut Hx, left: bool, right: bool) -> bool {
    // The flip (@ 0x00409bbb), before the clicks.
    h.g.flip_texts();
    if left {
        h.set_flag(901, 1);
        h.enter_interdimension();
        return false;
    }
    !right
}

/// `ECRIN`'s cursor when no gem is hovered (`Interface.spr` #40), and
/// the glow's frames.
const ECRIN_NO_GEM: i32 = 40;
const ECRIN_GLOW: (i32, i32) = (13, 24);

/// `SPR_COLLISION` (@ 0x00437080) of gem k (0..2: case bank sprite
/// k + 1, the table @ 0x00755928) drawn at the screen's centre, at the
/// pointer.
pub(in crate::game) fn ecrin_gem_hit(
    bank: &Bank,
    k: usize,
    w: i32,
    h: i32,
    ptr: (i32, i32),
) -> bool {
    bank.opaque_at(k + 1, Some(w / 2), h / 2, ptr.0, ptr.1)
}

/// `ECRIN`, the jewel case (@ 0x00412fc0). Until the first 1/15 s tick the
/// fade is 0 (the case drawn invisible, no cursor); that tick sets it
/// to 1. Then, while fewer than three gems are taken: the gem under the
/// pointer (`SPR_COLLISION` of each untaken gem at the screen's centre,
/// the last hit in order) starts the glow (cursor 13), none puts the
/// cursor back to 40, and each tick moves the glow on (13..24). The
/// frame is drawn and flipped, then a left click on the hovered gem
/// takes it (the right button does nothing). At the first tick with the
/// three taken the loop returns 0 before drawing (the stop puts the gems
/// in the bag). No sound.
fn ecrin(h: &mut Hx, left: bool) -> bool {
    let now = h.now();
    let ptr = h.g.pointer;
    let (w, ht) = (h.g.width as i32, h.g.height as i32);
    let bank = h.g.puzzles.screens.ecrin_bank.clone();
    let st = &mut h.g.puzzles.screens;
    let tick = now - st.ecrin_clock > TICK;
    let mut hovered = None;
    if st.ecrin_fade == 1.0 && st.ecrin_count != 3 {
        for k in 0..3 {
            let hit = bank
                .as_ref()
                .is_some_and(|b| ecrin_gem_hit(b, k, w, ht, ptr));
            if !st.ecrin_taken[k] && hit {
                hovered = Some(k);
            }
        }
        match hovered {
            Some(_) if st.ecrin_cursor == ECRIN_NO_GEM => st.ecrin_cursor = ECRIN_GLOW.0,
            None => st.ecrin_cursor = ECRIN_NO_GEM,
            _ => {}
        }
        if tick {
            st.ecrin_clock = now;
            if st.ecrin_cursor != ECRIN_NO_GEM {
                st.ecrin_cursor += 1;
                if st.ecrin_cursor > ECRIN_GLOW.1 {
                    st.ecrin_cursor = ECRIN_GLOW.0;
                }
            }
        }
    } else if tick {
        st.ecrin_clock = now;
        if st.ecrin_count == 0 {
            st.ecrin_fade = 1.0;
            log::debug!("Ecrin transp : {}", st.ecrin_fade);
        } else if st.ecrin_count == 3 {
            return false;
        } else {
            log::warn!("AtlEcrin_Continue(): Fade demandé en plein milieu de la gestion");
        }
    }
    h.g.ecrin_draw();
    // The flip (@ 0x00413321), before the click.
    h.g.flip_texts();
    if left {
        let st = &mut h.g.puzzles.screens;
        if let Some(k) = hovered {
            st.ecrin_taken[k] = true;
            st.ecrin_count += 1;
        }
    }
    true
}

/// `GBOY`, the tower's dice (@ 0x0041f800), in its order:
///
/// 1. a left click outside the result phase is used up: within 15 pixels
///    of the button (squared distance < 225) the hit byte is set and, with
///    the cursor in 113..=141 and nothing picked, the first threshold
///    above the cursor is picked (none from 133); elsewhere the hit byte
///    is cleared;
/// 2. the 15 Hz steps: the clock (the game's, less the start's) feeds a
///    float accumulator (the first frame only takes the time); for each
///    1/15 above it, the cursor moves on (from 141 back to 43 while
///    nothing is picked), the count animation moves on (a face starts at
///    its frame, adding one to the result; the last frame starts the
///    result phase), the running faces turn;
/// 3. the result phase: every 0.3 s a blink, the sixth ends the loop
///    (return 0, nothing drawn);
/// 4. the cursor at the picked threshold starts the count animation;
/// 5. the drawing ([`Game::gboy_draw`]).
///
/// The right button's byte is cleared, nothing else.
fn gboy(h: &mut Hx, left: bool, right: bool) -> bool {
    let _ = right;
    let (w, ht) = (h.g.width as i32, h.g.height as i32);
    let ptr = h.g.pointer;
    let fpu = h.g.camera.fpu;
    let now = h.now();
    let mut dice = h.g.puzzles.tourmagi.dice;
    let g = &mut h.g.puzzles.screens.gboy;
    // 1. The click.
    let button = gboy_anchors(w, ht)[2];
    if left && !g.phase {
        let (dx, dy) = (ptr.0 - button.0, ptr.1 - button.1);
        if dx * dx + dy * dy < 225 {
            g.hit = true;
            if (GBOY_RANGE.0..=GBOY_RANGE.1).contains(&g.cursor) && g.pick == -1 {
                for k in (0..6).rev() {
                    if g.cursor < GBOY_THRESHOLDS[k] {
                        g.pick = k as i32;
                        g.k = k as i32;
                        g.go = false;
                    }
                }
            }
        } else {
            g.hit = false;
        }
    } else {
        g.hit = false;
    }
    // 2. The clock and the 15 Hz steps (@ 0x0041f8e2..0x0041faaa).
    let t = fpu.sub(now, g.start);
    if g.last == 0.0 {
        g.last = fpu.f32(t);
    }
    g.acc = fpu.f32(fpu.add(fpu.sub(t, f64::from(g.last)), f64::from(g.acc)));
    g.last = fpu.f32(t);
    let step = f64::from(GBOY_STEP);
    while f64::from(g.acc) > step {
        g.acc = fpu.f32(fpu.sub(f64::from(g.acc), step));
        if g.cursor < GBOY_RANGE.1 {
            g.cursor += 1;
        } else if g.k == -1 {
            g.cursor = 43;
            g.pick = -1;
        }
        if g.anim != -1 {
            let k = g.k.clamp(0, 5) as usize;
            g.anim += 1;
            for (i, &at) in GBOY_FACES[k].iter().enumerate() {
                if g.anim == at {
                    g.faces[i] = 7 * i as i32 + 1;
                    dice += 1;
                }
            }
            if g.anim == GBOY_ANIM[k].1 {
                g.blink_clock = now;
                g.phase = true;
                g.blinks = 0;
                g.anim = -1;
            }
        }
        for (i, face) in g.faces.iter_mut().enumerate() {
            let rest = 7 * i as i32;
            if *face != rest {
                *face += 1;
                if *face > rest + 6 {
                    *face = rest;
                }
            }
        }
    }
    // 3. The result phase's blinks.
    if g.phase && now - g.blink_clock > GBOY_BLINK {
        g.blink_clock = now;
        g.blinks += 1;
        if g.blinks > 5 {
            g.phase = false;
            h.g.puzzles.tourmagi.dice = dice;
            return false;
        }
    }
    // 4. The count started when the cursor reaches the threshold.
    let mut offset = 0;
    if g.pick != -1 {
        let p = g.pick.clamp(0, 5) as usize;
        if g.cursor >= GBOY_THRESHOLDS[p] {
            offset = GBOY_OFFSETS[p];
            if g.k != -1 && !g.go {
                g.anim = GBOY_ANIM[g.k.clamp(0, 5) as usize].0;
                g.go = true;
            }
        }
    }
    h.g.puzzles.tourmagi.dice = dice;
    h.g.gboy_draw(offset);
    // The flip (@ 0x0041fd5e).
    h.g.flip_texts();
    true
}

/// The board's sound and command calls on the game.
struct LicorneHost<'a, 'b> {
    h: &'a mut Hx<'b>,
}

impl crate::licorne::Host for LicorneHost<'_, '_> {
    fn load(&mut self, name: &str) {
        self.h.g.sfx_load(name, 0, 1);
    }
    fn play(&mut self, name: &str) {
        self.h.g.sfx_play(name);
    }
    fn stop(&mut self, name: &str) {
        self.h.g.sfx_stop(name);
    }
    fn playing(&mut self, name: &str) -> bool {
        self.h.g.sfx_playing(name)
    }
    fn command(&mut self, cmd: &str) {
        self.h.run(cmd);
    }
}

/// `LICORNE`, the unicorn board (@ 0x0042eb50, [`crate::licorne`]): the
/// input, the clocks and the run, sound 810, the outcome, then the warp
/// frame (@ 0x00440690, every pixel of it covered), the board, the cursor
/// and the flip. Its win (@ 0x0042f6a9): sound 803, `(Flag147=0)`,
/// `(Flag149=1)`, `(Flag146=1)`, `(StopMusik=1)`, the horn in the bag and
/// in the hand, and the loop leaves without a draw (the stop, @ 0x0042e990,
/// stops sound 810: [`Game::stop_screen`]). No other exit but Escape
/// ([`Game::escape_screen`]; the right button drops a carried beacon); a
/// lost run starts again with the beacons where they are.
fn licorne(h: &mut Hx, left: bool, right: bool) -> bool {
    let Some(mut board) = h.g.puzzles.screens.licorne.take() else {
        return true;
    };
    let now = h.now();
    let input = crate::licorne::Input {
        pointer: h.g.pointer,
        left,
        right,
    };
    let stay = {
        let mut host = LicorneHost { h };
        board.frame(input, now, &mut host)
    };
    h.g.pointer = board.pointer;
    if stay {
        h.g.licorne_draw(&mut board);
        // The flip (@ 0x0042fa28).
        h.g.flip_texts();
    }
    h.g.puzzles.screens.licorne = Some(board);
    stay
}

impl Game {
    /// One frame of the running screen with the given click bytes; its
    /// stop when the handler returns 0.
    pub(in crate::game) fn screen_step(&mut self, left: bool, right: bool) {
        let Some(screen) = self.puzzles.screen else {
            return;
        };
        let Some(mut s) = self.script.take() else {
            return;
        };
        let stay = {
            let mut h = Hx { g: self, s: &mut s };
            frame(&mut h, screen, left, right)
        };
        self.script = Some(s);
        if !stay {
            self.stop_screen();
        }
    }

    /// One frame of the running screen with a click at the pointer
    /// (tests): `left` a left click, else a right one.
    pub fn screen_click(&mut self, left: bool) {
        self.screen_step(left, !left);
        self.main_loop_hook();
    }

    /// Tests: the outcome of a board written as a test walk skips it: the
    /// unicorn's win (`value` ignored), the game boy's dice `value` (1..6)
    /// with its result phase started.
    #[doc(hidden)]
    pub fn screen_outcome(&mut self, value: i32) {
        match self.puzzles.screen {
            Some(Screen::Licorne) => {
                if let Some(b) = self.puzzles.screens.licorne.as_mut() {
                    b.running = true;
                    b.won = true;
                }
            }
            Some(Screen::GBoy) => {
                self.puzzles.tourmagi.dice = value;
                let g = &mut self.puzzles.screens.gboy;
                g.anim = -1;
                g.phase = true;
                g.blinks = 0;
                g.blink_clock = self.now;
            }
            _ => {}
        }
    }
}
