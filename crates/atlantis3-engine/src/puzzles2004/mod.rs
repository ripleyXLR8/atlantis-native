//! *Atlantis Evolution*'s 2004 CD program (atlantis4-cd-en-3022848): the
//! eight mini-games and their frame.
//!
//! The script keys (`SetVariables` FUN_0044c850 @ 0x0044d8e2..0x0044da9e,
//! [`atlantis2_engine::script::PuzzleOp`]): `INITPUZZLE=<name>`,
//! `STARTPUZZLE=<name>`, `FINPUZZLE=<name>`. Each calls a dispatcher that
//! compares the name with the eight `PUZ_` strings (CString `==`,
//! FUN_00449410) in the order Bulle, Defender, Frogger, Tron, Pong, Hanoi,
//! Caisses, Catapulte, and calls that game's function:
//!
//! | Game | Init (FUN_004759c0) | Start (FUN_00475f80) | Free (FUN_00475ca0) | Frame | Mode | `Puzzles` |
//! |---|---|---|---|---|---|---|
//! | Bulle | 0x0048d960 | 0x0048dca0 | 0x0048ea90 | 0x0048dcf0 | 0x0d | 3 |
//! | Defender | 0x004861d0 | 0x004870b0 | 0x00487100 | 0x00487460 | 0x10 | 2 |
//! | Frogger | 0x00493fc0 | 0x00494fb0 | 0x00495040 | 0x00495420 | 0x11 | 4 |
//! | Tron | 0x0049f260 | 0x0049f510 | 0x0049f560 | 0x0049f700 | 0x12 | 7 |
//! | Pong | 0x0049b1e0 | 0x0049b760 | 0x0049b7b0 | 0x0049b9a0 | 0x0f | 6 |
//! | Hanoi | 0x00499a00 | 0x00499ea0 | 0x00499ef0 | 0x0049a0a0 | 0x13 | 5 |
//! | Caisses | 0x00476be0 | 0x004770d0 | 0x00477120 | 0x00477320 | 0x15 | 0 |
//! | Catapulte | 0x00483690 | 0x00483e00 | 0x00483e50 | 0x00484180 | 0x14 | 1 |
//!
//! An unknown name calls nothing. Every init ends with its game's start
//! (e.g. @ 0x004770b4, @ 0x00483de0), so `INITPUZZLE` alone already shows
//! the game; the CD's scripts always follow it with `STARTPUZZLE`, which
//! starts it again. A start (e.g. FUN_004770d0): the display's format read
//! (FUN_00411bb0, kept in a local only), the 800 × 600 stretch surface
//! opened (FUN_00412c40(800, 600); a second open only logs), the loop mode
//! set to the game's (FUN_004a2620, the previous mode kept: the second
//! start keeps the game's own mode), and the game's `Puzzles` entry of
//! `Atlantis4.cfg` := 1 (0x00719b58 + 4 i). Those eight integers have no
//! other reader than the cfg's reader and writer (@ 0x0040b4d2,
//! @ 0x0040cee3; zeroed with the defaults @ 0x0040d153): "this game was
//! played once" flags, written back with the options.
//!
//! The main loop (@ 0x004a1524, table 0x004a15ec / 0x004a1618) calls
//! FUN_00476320(mode) for the modes 13..21; mode 14 has no game (the
//! defnames' `PUZ_BILLIARD` has none either). FUN_00476320: the game's
//! frame by mode (0x0d Bulle, 0x0f Pong, 0x10 Defender, 0x11 Frogger,
//! 0x12 Tron, 0x13 Hanoi, 0x14 Catapulte, 0x15 Caisses), then the light
//! dialog's pass (0x006e6f59 and `AtlDial_Continue` false →
//! `AtlDial_Close`), then the map when the object in the hand is `GAMEBOY`
//! (FUN_00409970, `AtlCarte`, mode 10, and FUN_00429a20) — the map is not
//! ported (no 2004 script names `GAMEBOY` in a hand: [UNKNOWN] use).
//! Escape does nothing in these modes (FUN_0042c560's table stops at
//! mode 8, @ 0x0042c572).
//!
//! Every game's frame starts the same way (e.g. Caisses @ 0x00477320):
//! its "loaded" byte clear → return; else, the byte 0x00719d98 clear
//! (it always is: no instruction writes it [CONFIRMED-STATIC], a debug
//! switch), `Interp_CloseInterrupt(1)` unless a light dialog runs
//! (0x006e6f59) — so the section that ran `STARTPUZZLE` goes on at the
//! game's first frame — and the `/puz` scan (FUN_00450f00,
//! [`atlantis2_engine::script::Script::puz_scan_2004`]): the first `/puz(...)`
//! entry of the chapter whose condition holds runs. That is how a game
//! reports to the script: it sets its variables (`CAISSES_GAGNE`,
//! `CATA_CLIC_EXIT`, `DEFENDER_GAGNE`, `BULLE_PERDU`, a flag...), and a
//! `/puz` entry reacts, typically with `FINPUZZLE`, a film and a chapter.
//! Then, with the game's end variable set, the frame returns without
//! drawing (the 0x00719d98 branch that would free the game itself is dead).
//!
//! A free (e.g. FUN_00477120): the game's play time added up (its own
//! doubles, no reader found [UNKNOWN]), the stretch surface closed
//! (FUN_00412db0), its banks freed, its sounds removed
//! (`AtlSound_RemoveSfx`), the loop mode := 3 (the panorama; the
//! 0x00719d98 branch would restore the kept mode).
//!
//! The games draw into the stretch surface (800 × 600: the getters
//! FUN_00411a30 / FUN_00411a50 give its size while it is open) and flip
//! (FUN_00412690, FUN_00412700, FUN_004127a0(0): the stretch to the screen
//! and the flip's texts). The free pointer is clamped to the stretch
//! surface (@ 0x0042dfc2..0x0042e00f) and drawn by FUN_00429460 (cursor
//! #29, set by the inits through FUN_00429160 / FUN_00429180(0x1d)).
//!
//! Port: [`PuzzleId`] names a game; [`Puzzle`] is a running game (one
//! file per game, `puzzles2004/<game>.rs`), built by [`init`] from the
//! game's init and driven through [`Host`] (the game's view of the
//! engine). The engine's side is `game::puzzles2004`.

pub mod catapulte;

use std::sync::Arc;

use atlantis3_data::spr::Bank;

pub use crate::inventory::Aff;
use crate::px::Surface;

/// The stretch surface the games draw into (FUN_00412c40(800, 600)).
pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 600;

/// One of the eight games.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PuzzleId {
    Bulle,
    Defender,
    Frogger,
    Tron,
    Pong,
    Hanoi,
    Caisses,
    Catapulte,
}

impl PuzzleId {
    /// The dispatchers' order (FUN_004759c0 @ 0x004759dc..0x00475c1f).
    pub const ALL: [PuzzleId; 8] = [
        PuzzleId::Bulle,
        PuzzleId::Defender,
        PuzzleId::Frogger,
        PuzzleId::Tron,
        PuzzleId::Pong,
        PuzzleId::Hanoi,
        PuzzleId::Caisses,
        PuzzleId::Catapulte,
    ];

    /// The script's name (the strings 0x006b1d28..0x006b1d84).
    pub fn name(self) -> &'static str {
        match self {
            PuzzleId::Bulle => "PUZ_BULLE",
            PuzzleId::Defender => "PUZ_DEFENDER",
            PuzzleId::Frogger => "PUZ_FROGGER",
            PuzzleId::Tron => "PUZ_TRON",
            PuzzleId::Pong => "PUZ_PONG",
            PuzzleId::Hanoi => "PUZ_HANOI",
            PuzzleId::Caisses => "PUZ_CAISSES",
            PuzzleId::Catapulte => "PUZ_CATAPULTE",
        }
    }

    /// The game a script names (CString `==`: the exact bytes).
    pub fn from_name(name: &str) -> Option<PuzzleId> {
        PuzzleId::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The loop mode its start sets (FUN_004a2620's argument) and that
    /// FUN_00476320 dispatches on.
    pub fn mode(self) -> u32 {
        match self {
            PuzzleId::Bulle => 0x0d,
            PuzzleId::Pong => 0x0f,
            PuzzleId::Defender => 0x10,
            PuzzleId::Frogger => 0x11,
            PuzzleId::Tron => 0x12,
            PuzzleId::Hanoi => 0x13,
            PuzzleId::Catapulte => 0x14,
            PuzzleId::Caisses => 0x15,
        }
    }

    /// Its entry of `Atlantis4.cfg`'s `Puzzles` (0x00719b58 + 4 i), set
    /// to 1 by its start.
    pub fn cfg_index(self) -> usize {
        match self {
            PuzzleId::Caisses => 0,
            PuzzleId::Catapulte => 1,
            PuzzleId::Defender => 2,
            PuzzleId::Bulle => 3,
            PuzzleId::Frogger => 4,
            PuzzleId::Hanoi => 5,
            PuzzleId::Pong => 6,
            PuzzleId::Tron => 7,
        }
    }
}

/// What a game sees of the engine. Coordinates are the stretch
/// surface's (800 × 600).
pub trait Host {
    /// The clock FUN_004b1700, seconds.
    fn clock(&self) -> f64;
    /// The previous frame routine's duration (FUN_004a2630, the float
    /// 0x00719d94), seconds.
    fn frame_time(&self) -> f32;
    /// The free pointer (0x006e9fd8, 0x006e9fdc).
    fn pointer(&self) -> (i32, i32);
    fn set_pointer(&mut self, x: i32, y: i32);
    /// The left click byte (0x006e9fe8): set by a press, cleared by the
    /// games themselves.
    fn click(&self) -> bool;
    fn set_click(&mut self, on: bool);
    /// A virtual key held (`GetKeyboardState`'s high bit).
    fn key(&self, vk: u8) -> bool;
    /// `srand(time(0))` (FUN_006695c0, FUN_0066755f).
    fn srand_time(&mut self);
    /// `rand()` (the CRT's, shared with the rest of the game).
    fn rand(&mut self) -> i32;
    /// A script variable by name (`FUN_0044bd50` then 0x0070d4f8[i]); 0
    /// for an unknown name.
    fn var(&self, name: &str) -> i32;
    fn set_var(&mut self, name: &str, v: i32);
    /// `FLAGn` (0x0070dbdc + 4 n).
    fn flag(&self, n: usize) -> i32;
    fn set_flag(&mut self, n: usize, v: i32);
    /// `spr\<file>` loaded at the display's depth (FUN_00418d50,
    /// FUN_00436790).
    fn bank(&mut self, file: &str) -> Option<Arc<Bank>>;
    /// A data file (`puzzles\...`, already under its disc's folder).
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
    /// `AtlSound_LoadSfx(sfx\<file>, 1, 0, 0)` (FUN_004177e0,
    /// FUN_0042ef10).
    fn sfx_load(&mut self, file: &str);
    /// `AtlSound_PlaySfx(sfx\<file>)` (FUN_0042f6d0): the effects'
    /// channel 6 (a new sound cuts the last).
    fn sfx_play(&mut self, file: &str);
    /// `AtlSound_RemoveSfx(sfx\<file>)` (FUN_0042fdd0).
    fn sfx_remove(&mut self, file: &str);
    /// The stretch surface (800 × 600, the display's format).
    fn surface(&mut self) -> &mut Surface;
    /// The free pointer drawn into the stretch surface (FUN_00429460).
    fn draw_pointer(&mut self);
    /// The flip (FUN_00412690, FUN_00412700, FUN_004127a0(0)).
    fn flip(&mut self);
    /// The frames' common start: `Interp_CloseInterrupt(1)` unless a light
    /// dialog runs, then the `/puz` scan. False when the scan ended the
    /// game (`FINPUZZLE`): its banks are freed, the frame must stop.
    fn script_prologue(&mut self) -> bool;
    /// Tests: whether calls are logged ([`Host::trace`]).
    fn tracing(&self) -> bool {
        false
    }
    fn trace(&mut self, _line: String) {}
    /// The display's size as FUN_00411a30 / FUN_00411a50 give it: the
    /// stretch surface's while it is open, else the screen's (the inits'
    /// FUN_0042e390 centres the pointer on it, before their start opens the
    /// stretch surface).
    fn display_size(&self) -> (i32, i32) {
        (WIDTH as i32, HEIGHT as i32)
    }
}

/// A running game.
pub trait Puzzle: std::any::Any {
    fn id(&self) -> PuzzleId;
    /// One pass of its loop mode (its frame function, from FUN_00476320).
    fn frame(&mut self, host: &mut dyn Host);
    /// `FINPUZZLE`'s game part (after the engine closed the stretch
    /// surface and set mode 3): banks dropped, sounds removed.
    fn free(&mut self, host: &mut dyn Host);
    /// Tests: the game's state.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// `INITPUZZLE`: the game's init (loads, set-up). The engine then runs
/// the start's common part ([`PuzzleId::mode`], [`PuzzleId::cfg_index`]).
/// `None` for the games ported elsewhere or not yet.
pub fn init(id: PuzzleId, host: &mut dyn Host) -> Option<Box<dyn Puzzle>> {
    match id {
        PuzzleId::Catapulte => Some(Box::new(catapulte::Catapulte::init(host))),
        _ => None,
    }
}

/// `AtlSpr_Aff(bank, frame, x, y, alpha)` (FUN_00436b70) into the stretch
/// surface; x `None` for 0x80000000 (the sprite's own position).
pub fn aff(
    host: &mut dyn Host,
    name: &'static str,
    bank: Option<&Arc<Bank>>,
    frame: usize,
    x: Option<i32>,
    y: i32,
    alpha: f32,
) {
    let Some(bank) = bank else {
        return;
    };
    let call = crate::with_px!(host.surface(), s => {
        crate::inventory::aff_at(s.as_mut_slice(), WIDTH, HEIGHT, name, bank, frame, x, y, alpha)
    });
    if host.tracing() {
        host.trace(format!(
            "aff {} {} {} {} {}",
            call.bank, call.index, call.x, call.y, call.alpha
        ));
    }
}

/// `SPR_COLLISION(bank, frame, x, y, px, py)` (FUN_004379b0, *Atlantis
/// III*'s @ 0x00437080): whether sprite `frame` drawn at (x, y) covers
/// (px, py); x `None` for the sprite's own position.
pub fn collision(
    bank: Option<&Arc<Bank>>,
    frame: usize,
    x: Option<i32>,
    y: i32,
    px: i32,
    py: i32,
) -> bool {
    bank.is_some_and(|b| b.opaque_at(frame, x, y, px, py))
}

/// `__ftol` (truncation toward zero, as the x87's chop mode the CRT sets).
pub fn ftol(v: f32) -> i32 {
    v as i32
}

/// `__ftol` of a double.
pub fn ftol_d(v: f64) -> i32 {
    v as i32
}
