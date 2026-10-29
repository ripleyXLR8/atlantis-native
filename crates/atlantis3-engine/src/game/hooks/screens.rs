//! The full-screen loop modes' frames (the main loop's table
//! @ 0x004d5e30): what each one does to the script's world on a frame,
//! with the click bytes of that frame (left @ 0x007592b8, right
//! @ 0x007592b9) and the screen bank's sprite under the cursor (the hit
//! test @ 0x00437080, one call per sprite index in the loop's order). A
//! handler returning 0 makes the main loop call the mode's `Stop`
//! ([`Game::stop_screen`]).
//!
//! The two maps and the jewel case are drawn ([`crate::maps`],
//! `screens_draw`); the other screens' drawing, their hover animations
//! and the boards played inside
//! two of them (the unicorn's 3D board, the game boy's reflex dice) are
//! M4; the outcome of those boards is given by [`Game::screen_outcome`].

use std::sync::Arc;

use atlantis2_engine::script::Host;
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
    /// `LICORNE`: the board's "won" byte (@ 0x007586d4, set by the run's
    /// state 7 @ 0x0042d289; the board is M4).
    pub licorne_won: bool,
    /// `GBOY`: the result phase (@ 0x00756054), its blinks (@ 0x00756058)
    /// and its clock's last time (@ 0x00755f08). The result itself is
    /// the tower's dice (@ 0x00756060, `tourmagi::State::dice`).
    pub gboy_result: bool,
    pub gboy_blinks: i32,
    pub gboy_clock: f64,
    /// `AtlCarte`'s bank `Carte.spr` (@ 0x0074e928), loaded with
    /// `ldmorts.wam` (@ 0x00405bf0).
    pub carte_bank: Option<Arc<Bank>>,
    /// `AtlCarteTour`'s banks `CarteTour<W>.spr` (@ 0x0074e968) and
    /// `PionsTour.spr` (@ 0x0074eb98), loaded by its first start
    /// (@ 0x00406a10), and its pawns' animation.
    pub tour_board: Option<Arc<Bank>>,
    pub tour_pawns: Option<Arc<Bank>>,
    pub tour_anim: maps::TourAnim,
}

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
        // @ 0x0041f660: the result cleared (0x00756054, 0x00756058,
        // 0x00756060 := 0); the dice's faces and clocks (visual, M4).
        Screen::GBoy => {
            st.gboy_result = false;
            st.gboy_blinks = 0;
            st.gboy_clock = now;
            h.g.puzzles.tourmagi.dice = 0;
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
        // @ 0x00431da0, 0x00409a40, 0x004cfee0 (0x0071f0e8 := 0, read by
        // nobody), 0x0042e7a0: nothing the script sees besides the common
        // part.
        _ => {}
    }
}

/// One frame of the running screen; false = the handler returned 0.
pub(super) fn frame(
    h: &mut Hx,
    screen: Screen,
    left: bool,
    right: bool,
    sprite: Option<i32>,
) -> bool {
    match screen {
        Screen::Scarabee => scarabee(h, left, right),
        Screen::Carte => carte(h, left, right, sprite),
        Screen::CarteTour => {
            // @ 0x00406780: drawn, then any click closes (@ 0x004069c9).
            h.g.tour_draw();
            !(left || right)
        }
        Screen::Telecommande => telecommande(h, left, right, sprite),
        Screen::CraneBagdad => crane(h, left, right),
        Screen::Ecrin => {
            let _ = sprite;
            ecrin(h, left)
        }
        Screen::GBoy => gboy(h),
        Screen::Licorne => licorne(h),
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
/// drawn position (`sprite` when a test gives it, else the pointer's hit)
/// → chapter 12, 14, 13, 15, 16 (all but 15: the fisherman gone once his
/// scale is taken, flag 69); the current chapter → nothing; else
/// `LDM_CHAPITRE_DEPART` := the chapter, `LDM_CHAPITRE_ARRIVEE` := the
/// target, `(chapter=500)` and leave. A left click on nothing clears the
/// clicks; a right click leaves.
fn carte(h: &mut Hx, left: bool, right: bool, sprite: Option<i32>) -> bool {
    if let Some(r) = maps::carte_refusal(|f| h.flag(f as usize)) {
        h.set_var(r.var(), 1);
        return false;
    }
    h.g.carte_draw();
    if !left {
        return !right;
    }
    let hit = sprite.or_else(|| h.g.carte_hit());
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

/// `TELECOMMANDE`, the remote (@ 0x004d0550; the hover animation
/// @ 0x004d0040 is visual). A left click: the first one shows the
/// Atlantean figure (`flag427` := 1, `flag425` := 0); then in
/// `hoggar.wam` button 0 = the Hoggar (`flag404` := 1 and
/// `(PlaySound=51)` once), button 0x0e = Bagdad (flag 400, or 401 after
/// flag 908), button 0x1d = Paleo (flag 402, or 403 after flag 605);
/// elsewhere button 0x2c = home, `(Chapter=607)`. The loop stays; a
/// right click leaves.
fn telecommande(h: &mut Hx, left: bool, right: bool, sprite: Option<i32>) -> bool {
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
    let hit = |k: i32| sprite == Some(k);
    if hit(0) && hoggar {
        if h.flag(404) == 0 {
            h.set_flag(404, 1);
            h.run("(PlaySound=51)");
            // visual: the hover bytes reset (@ 0x004cffc0).
        }
    } else if hit(0x0e) && hoggar {
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
    } else if hit(0x1d) && hoggar {
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
    } else if hit(0x2c) && !hoggar {
        // visual: the hover bytes reset (@ 0x004cffc0, 0x0078972f := 0).
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
    if left {
        let st = &mut h.g.puzzles.screens;
        if let Some(k) = hovered {
            st.ecrin_taken[k] = true;
            st.ecrin_count += 1;
        }
    }
    true
}

/// `GBOY`, the tower's dice (@ 0x0041f800). M4: the reflex play (a left
/// click within 15 pixels of (width − 97, height − 32) while the cursor
/// runs 43..141 at 15 Hz) that sets the result @ 0x00756060 and starts
/// the result phase. The result phase: six blinks of 0.3 s, then the
/// loop leaves (the stop scans and restarts music 6). The right button
/// does nothing.
fn gboy(h: &mut Hx) -> bool {
    let now = h.now();
    let st = &mut h.g.puzzles.screens;
    if st.gboy_result && now - st.gboy_clock > GBOY_BLINK {
        st.gboy_clock = now;
        st.gboy_blinks += 1;
        if st.gboy_blinks > 5 {
            st.gboy_result = false;
            return false;
        }
    }
    true
}

/// `LICORNE`, the unicorn board (@ 0x0042eb50). M4: the 3D board (four
/// beacons placed, the unicorn's run, failures restart it). Its win
/// (@ 0x0042f6a9): sound 803, `(Flag147=0)`, `(Flag149=1)`,
/// `(Flag146=1)`, `(StopMusik=1)`, the horn in the bag and in the hand,
/// and the loop leaves. No other exit (the right button drops a held
/// beacon).
fn licorne(h: &mut Hx) -> bool {
    if !h.g.puzzles.screens.licorne_won {
        return true;
    }
    h.g.puzzles.screens.licorne_won = false;
    h.g.new_sound("sound803.apc");
    h.run("(Flag147=0)");
    h.run("(Flag149=1)");
    h.run("(Flag146=1)");
    h.run("(StopMusik=1)");
    h.run("(SacADos=BAG_CORNE_LICORNE)");
    h.run("(ObjetEnMain=BAG_CORNE_LICORNE)");
    false
}

impl Game {
    /// One frame of the running screen with the given click bytes; its
    /// stop when the handler returns 0.
    pub(in crate::game) fn screen_step(&mut self, left: bool, right: bool, sprite: Option<i32>) {
        let Some(screen) = self.puzzles.screen else {
            return;
        };
        let Some(mut s) = self.script.take() else {
            return;
        };
        let stay = {
            let mut h = Hx { g: self, s: &mut s };
            frame(&mut h, screen, left, right, sprite)
        };
        self.script = Some(s);
        if !stay {
            self.stop_screen();
        }
    }

    /// One frame of the running screen with a click (tests): `left` a
    /// left click, else a right one; `sprite` the screen bank's sprite
    /// under the cursor (the hit test @ 0x00437080).
    pub fn screen_click(&mut self, left: bool, sprite: Option<i32>) {
        self.screen_step(left, !left, sprite);
        self.main_loop_hook();
    }

    /// Tests: the outcome of a board the port does not play (M4): the
    /// unicorn's win (`value` ignored), the game boy's dice `value`
    /// (1..6) with its result phase started.
    #[doc(hidden)]
    pub fn screen_outcome(&mut self, value: i32) {
        match self.puzzles.screen {
            Some(Screen::Licorne) => self.puzzles.screens.licorne_won = true,
            Some(Screen::GBoy) => {
                self.puzzles.tourmagi.dice = value;
                self.puzzles.screens.gboy_result = true;
                self.puzzles.screens.gboy_blinks = 0;
                self.puzzles.screens.gboy_clock = self.now;
            }
            _ => {}
        }
    }
}
