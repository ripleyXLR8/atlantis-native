//! The puzzles: StartPuzzle 0..16, run by the puzzle zones 1000..1016.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`con_exec.cpp`
//! `playPuzzle` / `runPuzzleZone`, `puzzle_dispatch.h`, `puzzle_assets.cpp`
//! and the puzzles' own files; GPLv3, the ScummVM team), which follow the
//! dispatcher `FUN_0043a1e0` of `atlantis.exe` and the puzzles' functions.
//!
//! # How a puzzle runs
//!
//! A script arms puzzle `n` with `StartPuzzle=n`; a click on zone
//! `1000 + n` then runs it (`FUN_004240fc`, [`crate::scripts`]), through
//! [`play_puzzle`]: the dispatcher's checks ([`check`]), the puzzle LCG
//! seeded from the clock, the puzzle's own loop, the samples stopped as the
//! puzzle's exit does, and FinPuzzle set to the result: 255 won, 2 lost or
//! left, 1 refused (an object in hand, a missing file, the game quit).
//!
//! # Writing a puzzle
//!
//! Each puzzle is one module with two halves, as the C++ has a game class
//! and a puzzle class:
//!
//! - the game: a struct with the original's state (its fields named after
//!   the original's globals) and an `async fn run(&mut self, io)` that is
//!   the original's loop, line for line. Everything it does outside its
//!   state (input, drawing, sounds, the frame) goes through a trait of its
//!   own (`ThroneIo`, `TaquinIo`, ...) whose methods are the C++ virtual
//!   functions. The methods at which a test compares the state with the
//!   original's take the game (`present(&mut self, g: &ThroneGame)`), as
//!   the C++ tests read the state there.
//! - the engine side: a `play(e, hooks)` function that loads the files,
//!   implements the trait on a [`PuzzleHost`] and runs the game.
//!
//! The tests (`tests/puzzles.rs`) implement the same traits to replay the
//! original's runs pass by pass, and drive the whole engine with scripted
//! input.
//!
//! [`PuzzleHost`] (in [`assets`]) holds what every puzzle's engine side
//! uses: the 640x480 work screen and its pictures, the PUZZLES\ sprites,
//! the CURSEURS cursors, the input read at the frame boundaries, the
//! puzzle samples on the six Miles sample handles, the frame present.
//! [`PuzzleCursor`] is the cursor animation every puzzle shares,
//! [`PuzzleState`] what the original keeps in memory from one puzzle to the
//! next.
//!
//! To add a puzzle: a module here, an arm in [`play_puzzle`]'s match, and
//! its persistent state (if any) in [`PuzzleState`].

pub mod assets;
pub mod crab;
pub mod eclipse;
pub mod hens;
pub mod hexa;
pub mod meta;
pub mod muria;
pub mod ogre;
pub mod pieces;
pub mod rat;
pub mod snakes;
pub mod taquin;
pub mod throne;

pub use assets::{
    load_class_map, load_spr, load_tga, read_coverage, read_puzzle_file, Coverage, PuzzleHost,
    PuzzleInput, PuzzleMouse, PuzzleScreen, PuzzleSounds, SamplePlayer, MAP_SIZE,
};

use crate::engine::{Engine, Event};
use crate::hooks::ScriptHooks;

/// StartPuzzle numbers with a puzzle: 0..16.
pub const NUM_PUZZLES: i32 = 17;

/// The entry function of each StartPuzzle (`FUN_0043a1e0`'s jump table).
pub const ENTRIES: [u32; NUM_PUZZLES as usize] = [
    0x0044154c, 0x004441e8, 0x0044203c, 0x004459c0, 0x00438bf0, 0x0043fcb4, 0x00446300, 0x0044659c,
    0x0043b0f4, 0x0044763c, 0x0044763c, 0x0044763c, 0x0044763c, 0x004404fc, 0x00446830, 0x0043d28c,
    0x0043ee54,
];

/// The entry function of StartPuzzle `id`, `None` above 16.
pub fn entry(id: i32) -> Option<u32> {
    usize::try_from(id)
        .ok()
        .and_then(|i| ENTRIES.get(i).copied())
}

/// The checks of the dispatcher `FUN_0043a1e0` before it runs a puzzle:
/// StartPuzzle 15 first empties the hand (OBJETENMAIN = 0, 0x0043a248);
/// then puzzles 0..5, 8, 9 and above 14 do not start while an object is
/// held and return `DAT_00496d0c` (1); numbers above 16 have no entry and
/// return 0. `held` is OBJETENMAIN, updated in place. `None` when the
/// puzzle starts, else the dispatcher's result.
pub fn check(id: i32, held: &mut i32) -> Option<i32> {
    if id == 15 {
        *held = 0;
    }
    if (id < 6 || id == 8 || id == 9 || id > 14) && *held != 0 {
        return Some(1);
    }
    if !(0..NUM_PUZZLES).contains(&id) {
        return Some(0);
    }
    None
}

/// The puzzle of each StartPuzzle number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleKind {
    /// 0: the eclipse at CHAPI012 (`FUN_0044154c`).
    Eclipse,
    /// 1: the hens on the levers at CHAPI034 (`FUN_004441e8`).
    Hens,
    /// 2: the ogre and the hero, at CHAPI018 (`FUN_0044203c`).
    OgreGame,
    /// 3: the six rotating pieces at CHAPI029 (`FUN_004459c0`).
    RotatingPieces,
    /// 4: the rat's run at CHAPI041 (`FUN_00438bf0`).
    Rat,
    /// 5: the throne lock at CHAPI044 (`FUN_0043fcb4`).
    Throne,
    /// 6, 7, 14: the sliding puzzles at CHAPI045..051 (`FUN_00446300`,
    /// `FUN_0044659c`, `FUN_00446830`); the StartPuzzle number.
    Taquin(i32),
    /// 8: the four snakes at CHAPI034 (`FUN_0043b0f4`).
    Snakes,
    /// 9..12: the metal pieces at CHAPI046 (`FUN_0044763c`), stage 1..4.
    Meta(i32),
    /// 13: the hexagon at CHAPI060 (`FUN_004404fc`).
    Hexa,
    /// 15: the crab race at CHAPI066 (`FUN_0043d28c`).
    Crab,
    /// 16: Muria's tablets at CHAPI069 and 070 (`FUN_0043ee54`).
    Muria,
}

impl PuzzleKind {
    pub fn of(id: i32) -> Option<PuzzleKind> {
        Some(match id {
            0 => PuzzleKind::Eclipse,
            1 => PuzzleKind::Hens,
            2 => PuzzleKind::OgreGame,
            3 => PuzzleKind::RotatingPieces,
            4 => PuzzleKind::Rat,
            5 => PuzzleKind::Throne,
            6 | 7 | 14 => PuzzleKind::Taquin(id),
            8 => PuzzleKind::Snakes,
            9..=12 => PuzzleKind::Meta(id - 8),
            13 => PuzzleKind::Hexa,
            15 => PuzzleKind::Crab,
            16 => PuzzleKind::Muria,
            _ => return None,
        })
    }

    /// Whether the port plays it (the others count as won at once).
    pub fn translated(self) -> bool {
        matches!(
            self,
            PuzzleKind::Eclipse
                | PuzzleKind::Hens
                | PuzzleKind::RotatingPieces
                | PuzzleKind::Snakes
                | PuzzleKind::Throne
                | PuzzleKind::Taquin(_)
                | PuzzleKind::Meta(_)
                | PuzzleKind::Hexa
                | PuzzleKind::Crab
                | PuzzleKind::Muria
                | PuzzleKind::Rat
                | PuzzleKind::OgreGame
        )
    }
}

/// The cursor animation every puzzle shares (0x00496ea8 mode, 0x00496eaa
/// frame, 0x00496eac step): a refused click bounces the CURSEURS frame
/// between 2 and 4, then down to -1, three times.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PuzzleCursor {
    pub mode: i16,
    pub frame: i16,
    pub step: i16,
}

impl PuzzleCursor {
    /// The refusal that most puzzles share (`FUN_004404ac`, `FUN_00448a80`,
    /// ...): frame 2, mode 2, step 1. False (nothing changes) when the
    /// frame is 2 already.
    pub fn refuse(&mut self) -> bool {
        if self.frame == 2 {
            return false;
        }
        self.frame = 2;
        self.mode = 2;
        self.step = 1;
        true
    }

    /// The animation runs (mode 2..4).
    pub fn bouncing(&self) -> bool {
        self.mode >= 2
    }

    /// `FUN_00448958`: the frame to draw now, then one step: three bounces
    /// of the frame between 4 and -1.
    pub fn bounce(&mut self) -> i16 {
        let f = self.frame;
        self.frame += self.step;
        if self.frame == 4 {
            self.step = -1;
        }
        if self.frame <= -1 {
            self.mode += 1;
            self.step = 1;
        }
        if self.mode == 5 {
            self.mode = 0;
        }
        f
    }
}

/// The C rand of `atlantis.exe` (Watcom, @ 0x00453cd1). srand
/// (0x00453cf5) has no caller, so the seed starts at 1 at every launch.
pub fn crt_rand_next(seed: &mut u32) -> i32 {
    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    ((*seed >> 16) & 0x7fff) as i32
}

/// The puzzle LCG `FUN_00447524` (s16 seed 0x007bec20): 0..n.
pub fn puzzle_lcg_next(seed: &mut u16, n: i32) -> i32 {
    let mut mask = 0;
    let mut v = i32::from(n as i16);
    while v > 0 {
        mask = mask * 2 + 1;
        v = i32::from(v as i16 >> 1);
    }
    loop {
        *seed = (i32::from(*seed as i16).wrapping_mul(-0x342f) + 1) as u16;
        let v = (i32::from(*seed as i16) / 256) & mask;
        if i32::from(v as i16) <= n {
            return v;
        }
    }
}

/// What the original keeps in its memory from one puzzle to the next, for
/// the whole session (none of it is in the checkpoints).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleState {
    /// The shared cursor animation.
    pub cursor: PuzzleCursor,
    /// The C rand's seed (shared by the rat, crab and ogre puzzles).
    pub crt_rand_seed: u32,
    /// The puzzle LCG 0x007bec20, seeded by the dispatcher.
    pub lcg_seed: u16,
    /// The metal pieces between their stages.
    pub meta: Option<meta::MetaMemory>,
    /// The crab records (static data: phase and timer carry over).
    pub crabs: Option<[crab::Crab; crab::NUM_CRABS]>,
    /// The frames of the eclipse's records 0 and 1 (static data: they
    /// turn on their own and are not reset).
    pub eclipse: Option<[i16; 2]>,
    /// The snakes of the last snakes run (their places and pieces are not
    /// reset).
    pub snakes: Option<[snakes::Snake; snakes::NUM_SNAKES]>,
    /// The rat's run state (static data at 0x004969c0, partly reset by
    /// each run).
    pub rat: Option<rat::RatRules>,
}

impl Default for PuzzleState {
    fn default() -> Self {
        PuzzleState {
            cursor: PuzzleCursor::default(),
            crt_rand_seed: 1,
            lcg_seed: 0,
            meta: None,
            crabs: None,
            eclipse: None,
            snakes: None,
            rat: None,
        }
    }
}

impl PuzzleState {
    /// The C rand (`FUN_00453cd1`).
    pub fn crt_rand(&mut self) -> i32 {
        crt_rand_next(&mut self.crt_rand_seed)
    }
}

/// `FUN_0043a1e0`: runs StartPuzzle `id` and sets FinPuzzle to its result,
/// which it returns: 255 won, 2 lost or left, 1 refused or quit, 0 no such
/// puzzle. The puzzles not translated yet count as won at once (255), as
/// in the ScummVM port, so that the chapters they gate go on.
pub async fn play_puzzle(e: &mut Engine, hooks: &mut dyn ScriptHooks, id: i32) -> i32 {
    let mut held = e.vars.get("objetenmain");
    let refused = check(id, &mut held);
    if held == 0 && e.vars.get("objetenmain") != 0 {
        e.vars.set("objetenmain", 0);
    }
    if let Some(r) = refused {
        e.vars.set("finpuzzle", r);
        e.emit(Event::Puzzle { id, result: r });
        return r;
    }
    // The dispatcher seeds the puzzle LCG from the clock.
    e.puzzles.lcg_seed = e.sys.millis() as u16;
    let kind = PuzzleKind::of(id);
    let result = match kind {
        Some(PuzzleKind::Eclipse) => eclipse::play(e, hooks).await,
        Some(PuzzleKind::Hens) => hens::play(e, hooks).await,
        Some(PuzzleKind::RotatingPieces) => pieces::play(e, hooks).await,
        Some(PuzzleKind::Snakes) => snakes::play(e, hooks).await,
        Some(PuzzleKind::Throne) => throne::play(e, hooks).await,
        Some(PuzzleKind::Taquin(v)) => taquin::play(e, hooks, v).await,
        Some(PuzzleKind::Meta(stage)) => meta::play(e, hooks, stage).await,
        Some(PuzzleKind::Hexa) => hexa::play(e, hooks).await,
        Some(PuzzleKind::Crab) => crab::play(e, hooks).await,
        Some(PuzzleKind::Muria) => muria::play(e, hooks).await,
        Some(PuzzleKind::Rat) => rat::play(e, hooks).await,
        Some(PuzzleKind::OgreGame) => ogre::play(e, hooks).await,
        // Every id 0..=16 has a puzzle; `check` has already refused the
        // others with 0, the answer kept here.
        None => 0,
    };
    // FUN_0043d0e0 at the puzzles' exits, the last call before the return
    // (call sites 0x00438ec8 .. 0x00447819): puzzles 0, 2, 15 and 16 make
    // it themselves (after their end sequences); the hens (FUN_004441e8)
    // only on the leave (@ 0x00444429), not on the win (0x0044443d ..
    // 0x00444450); the taquins 7 and 14 (FUN_0044659c, FUN_00446830)
    // never. A sound still playing then reads 8 and the next puzzle's
    // players skip its sample.
    match id {
        1 if result != 255 => assets::stop_samples(e),
        3..=6 | 8..=13 => assets::stop_samples(e),
        _ => {}
    }
    e.vars.set("finpuzzle", result);
    e.emit(Event::Puzzle { id, result });
    result
}
