//! StartPuzzle 4 at CHAPI041: the rat's run (`FUN_00438bf0`). A rat runs
//! down a machine of levers, trap doors, a wheel and a cart, along
//! trajectories read from the PUZZLES\ TRJ files; the player sets the
//! machine's parts with the mouse (a click, or the button held) so that
//! the rat gets through and jumps at the end (255). Five rats, and a
//! clock: losing them all or running out of time loses (2); a click at
//! the side edges leaves (1).
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`rat_rules.cpp`,
//! `rat_game.cpp`, `puzzle_rat.cpp`; GPLv3, the ScummVM team).
//!
//! The original reads overlapping shorts, ints and floats in its static
//! data at 0x004969c0, so the state is kept byte for byte and addressed by
//! the original addresses ([`RatRules`]). Being static data, it carries
//! over from one run of the puzzle to the next, as in the original (only
//! the parts [`RatRules::reset`] and [`RatGame::run`] set are reset).

use std::collections::VecDeque;

use atlantis_data::sprite::SprSprite;

use super::{load_spr, load_tga, read_puzzle_file, PuzzleHost, PuzzleSounds, SamplePlayer};
use crate::engine::Engine;
use crate::hooks::ScriptHooks;
use crate::warp::{SCREEN_H, SCREEN_W};

/// The first address of the state.
pub const BASE: u32 = 0x0049_69c0;
/// The state the tests compare: 0x004969c0..0x00496cd8.
pub const STATE_SIZE: usize = 0x318;
/// The memory kept: up to 0x0049727f, because the rat's animation entry
/// (0x00496c7a + 6 * byte) is indexed by any byte value, and before the
/// first trajectory the byte is still 0x68 from the data section. That
/// entry is 0x00496eea..0x00496eef, over the start of puzzle 3's cursor
/// sprite name "puzsrp2c.spr" (0x00496eec, loaded by `FUN_004459c0` @
/// 0x00445a00): the original corrupts the name for the rest of the program
/// (see [`RatRules::animate_rat`]). The write stays in this copy and the
/// rotating pieces load PUZSRP2C.SPR by name, always (a game-breaking bug
/// of the original, with no original mode).
pub const MEM_SIZE: usize = 0x8c0;
/// The machine's parts: 30 records of 0x17 bytes at 0x004969c8.
pub const NUM_RECORDS: i32 = 30;

// The fields of a record.
pub const ABOVE: u32 = 0;
pub const ACTIVE: u32 = 1;
pub const PRESSED: u32 = 2;
pub const KIND: u32 = 5;
pub const X: u32 = 7;
pub const Y: u32 = 9;
pub const REST: u32 = 0xb;
pub const PRESSED_FRAME: u32 = 0xd;
pub const FRAME: u32 = 0xf;
pub const PARTNER: u32 = 0x11;
pub const COUNTER: u32 = 0x13;
pub const SOUND: u32 = 0x15;

// The globals.
const ENDED: u32 = 0x0049_69c2;
const STARTED: u32 = 0x0049_69c3;
const RAT_MODE: u32 = 0x0049_6ca4;
const RAT_BYTE: u32 = 0x0049_6ca5;
const RAT_X: u32 = 0x0049_6ca6;
const RAT_Y: u32 = 0x0049_6caa;
const RATS: u32 = 0x0049_6cae;
const LOSS_ANIM: u32 = 0x0049_6cc4;
const AMBIENT_AT: u32 = 0x0049_6cc8;
/// [0x00496d14], [0x00496d10].
const WIN: i32 = 255;
const LOST: i32 = 2;

/// The address of record `r`'s field `off`.
pub fn rec(r: i32, off: u32) -> u32 {
    (0x0049_69c8 + 0x17 * r) as u32 + off
}

/// The bytes at 0x004969c0..0x0049727f in the data section of atlantis.exe.
pub const RAT_INITIAL: [u8; MEM_SIZE] = [
    0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02, 0x00, 0x41,
    0x01, 0x63, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x39, 0x00, 0x06, 0x00, 0x40, 0x00, 0x41, 0x00, 0x40, 0x00,
    0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x26,
    0x00, 0x42, 0x00, 0x43, 0x00, 0x42, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
    0x03, 0x00, 0x03, 0x00, 0x1e, 0x00, 0x3c, 0x00, 0x44, 0x00, 0x4f, 0x00, 0x44, 0x00, 0xff, 0xff,
    0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x3c, 0x00, 0xda, 0x00, 0x50,
    0x00, 0x51, 0x00, 0x50, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00,
    0x00, 0x00, 0x3b, 0x00, 0xfa, 0x00, 0x52, 0x00, 0x53, 0x00, 0x52, 0x00, 0x10, 0x00, 0x00, 0x00,
    0x05, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x3a, 0x00, 0x2b, 0x01, 0x54, 0x00, 0x55,
    0x00, 0x54, 0x00, 0x11, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00,
    0x1a, 0x02, 0x61, 0x00, 0x56, 0x00, 0x57, 0x00, 0x56, 0x00, 0x13, 0x00, 0x00, 0x00, 0x07, 0x00,
    0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x1b, 0x02, 0xd4, 0x00, 0x58, 0x00, 0x59, 0x00, 0x58,
    0x00, 0x0e, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x1d, 0x02,
    0x13, 0x01, 0x5a, 0x00, 0x5b, 0x00, 0x5a, 0x00, 0x14, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x01,
    0x00, 0x0a, 0x00, 0x02, 0x00, 0x26, 0x02, 0x58, 0x01, 0x5c, 0x00, 0x62, 0x00, 0x5c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x6d, 0x00, 0x00, 0x00,
    0x63, 0x00, 0x64, 0x00, 0x63, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x0c,
    0x00, 0x00, 0x00, 0xa3, 0x00, 0x12, 0x00, 0x65, 0x00, 0x66, 0x00, 0x65, 0x00, 0xff, 0xff, 0x00,
    0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x0d, 0x00, 0x00, 0x00, 0x6f, 0x01, 0x57, 0x00, 0x67, 0x00,
    0x68, 0x00, 0x67, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00,
    0x00, 0x69, 0x01, 0xbd, 0x00, 0x69, 0x00, 0x6a, 0x00, 0x69, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff,
    0xff, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x2e, 0x01, 0xdd, 0x00, 0x6b, 0x00, 0x6c, 0x00,
    0x6b, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0xcf,
    0x00, 0xd9, 0x00, 0x6d, 0x00, 0x74, 0x00, 0x6d, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00,
    0x00, 0x00, 0x11, 0x00, 0x01, 0x00, 0x92, 0x00, 0x13, 0x01, 0x75, 0x00, 0x76, 0x00, 0x75, 0x00,
    0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x02, 0x12, 0x00, 0x00, 0x00, 0xce, 0x01, 0xae,
    0x00, 0x77, 0x00, 0x79, 0x00, 0x78, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x01,
    0x13, 0x00, 0x01, 0x00, 0xae, 0x01, 0x64, 0x00, 0x7a, 0x00, 0xb9, 0x00, 0x7a, 0x00, 0xff, 0xff,
    0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x14, 0x00, 0x02, 0x00, 0x52, 0x01, 0xf9, 0x00, 0xba,
    0x00, 0xc3, 0x00, 0xba, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x15, 0x00,
    0x02, 0x00, 0x84, 0x00, 0x46, 0x00, 0xc4, 0x00, 0xc7, 0x00, 0xc4, 0x00, 0xff, 0xff, 0x00, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x00, 0x16, 0x00, 0x01, 0x00, 0x3b, 0x01, 0xae, 0x01, 0xc8, 0x00, 0xc9,
    0x00, 0xc8, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x17, 0x00, 0x01, 0x00,
    0xb1, 0x00, 0x6a, 0x01, 0xca, 0x00, 0xcb, 0x00, 0xca, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x18, 0x00, 0x01, 0x00, 0x9d, 0x01, 0x69, 0x01, 0xcc, 0x00, 0xcd, 0x00, 0xcc,
    0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x04, 0x01,
    0x66, 0x00, 0xce, 0x00, 0xce, 0x00, 0xce, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00,
    0x00, 0x1a, 0x00, 0x00, 0x00, 0x76, 0x00, 0x64, 0x01, 0xcf, 0x00, 0xcf, 0x00, 0xcf, 0x00, 0xff,
    0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0xb0, 0x01, 0x63, 0x01,
    0xd0, 0x00, 0xd0, 0x00, 0xd0, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x1c,
    0x00, 0x00, 0x00, 0x4e, 0x01, 0x2e, 0x00, 0xd1, 0x00, 0xd1, 0x00, 0xd1, 0x00, 0xff, 0xff, 0x00,
    0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x1d, 0x00, 0x00, 0x00, 0x50, 0x01, 0x0b, 0x00, 0xd2, 0x00,
    0xd2, 0x00, 0xd2, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x03, 0x00, 0x03, 0x00,
    0x04, 0x00, 0x07, 0x00, 0x04, 0x00, 0x08, 0x00, 0x0b, 0x00, 0x08, 0x00, 0x0c, 0x00, 0x0f, 0x00,
    0x0c, 0x00, 0x10, 0x00, 0x13, 0x00, 0x10, 0x00, 0x14, 0x00, 0x17, 0x00, 0x14, 0x00, 0x18, 0x00,
    0x1f, 0x00, 0x18, 0x00, 0x00, 0x68, 0x00, 0x00, 0x20, 0x42, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x0c, 0xc3, 0x3d, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x75, 0x7a, 0x70,
    0x6f, 0x75, 0x6c, 0x30, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x70, 0x75, 0x7a, 0x70, 0x6f, 0x75, 0x6c,
    0x31, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x70, 0x75, 0x7a, 0x70, 0x6f, 0x75, 0x6c, 0x32, 0x2e, 0x73,
    0x70, 0x72, 0x00, 0x70, 0x7a, 0x66, 0x70, 0x69, 0x65, 0x63, 0x65, 0x2e, 0x73, 0x70, 0x72, 0x00,
    0x70, 0x7a, 0x66, 0x72, 0x61, 0x74, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x73, 0x65, 0x72,
    0x70, 0x6e, 0x65, 0x77, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x70, 0x75, 0x7a, 0x63, 0x79, 0x63,
    0x6c, 0x6f, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x70, 0x7a, 0x31, 0x6c, 0x2e, 0x73, 0x70, 0x72, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x70, 0x7a, 0x32, 0x6c, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x01, 0x96, 0x00, 0x04, 0x01, 0xc6, 0x00, 0xf5, 0x00,
    0xef, 0x00, 0x33, 0x01, 0xfe, 0x00, 0x67, 0x01, 0x02, 0x01, 0x88, 0x01, 0xde, 0x00, 0xb2, 0x01,
    0x07, 0x01, 0xae, 0x01, 0x27, 0x01, 0x89, 0x01, 0x40, 0x01, 0x1b, 0x01, 0xc7, 0x00, 0x50, 0x01,
    0xcb, 0x00, 0x5a, 0x01, 0xaf, 0x00, 0x6c, 0x01, 0xa4, 0x00, 0x90, 0x01, 0x14, 0x01, 0x55, 0x01,
    0x2c, 0x01, 0x64, 0x01, 0x68, 0x01, 0xa4, 0x01, 0x68, 0x01, 0x08, 0x07, 0x21, 0x08, 0x09, 0x2c,
    0x0e, 0x05, 0x2d, 0x0e, 0x07, 0x23, 0x0e, 0x09, 0x2c, 0x0f, 0x04, 0x39, 0x0f, 0x05, 0x2e, 0x0f,
    0x09, 0x37, 0x07, 0x06, 0x3a, 0x06, 0x05, 0x30, 0x06, 0x0b, 0x3a, 0x05, 0x04, 0x32, 0x05, 0x09,
    0x45, 0x05, 0x0b, 0x3a, 0x04, 0x03, 0x3e, 0x04, 0x0a, 0x3c, 0x04, 0x0b, 0x3a, 0x0b, 0x01, 0x41,
    0x0b, 0x0c, 0x1e, 0x02, 0x01, 0x3d, 0x02, 0x03, 0x2c, 0x02, 0x04, 0x4a, 0x0a, 0x01, 0x34, 0x0a,
    0x02, 0x18, 0x0c, 0x01, 0x35, 0x0c, 0x0d, 0x14, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x14, 0x00, 0xa3, 0x00, 0xbf, 0x01, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0xd8, 0x00,
    0xc8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x26, 0x00, 0x77, 0x00, 0xac, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x0c, 0x00, 0x71, 0x01, 0xcc, 0x01, 0x00, 0x00, 0x00, 0x00, 0x24, 0x00, 0xde, 0x01, 0xc6, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x16, 0x00, 0x9e, 0x01, 0xc5, 0x01, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00,
    0x54, 0x01, 0xcf, 0x01, 0x00, 0x00, 0x09, 0x00, 0x21, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x33, 0x01, 0x8f, 0x01, 0x00, 0x00, 0x33, 0x01, 0x8f, 0x01, 0x00, 0x00, 0x33, 0x01,
    0x8f, 0x01, 0x00, 0x00, 0x33, 0x01, 0x8f, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x05, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00,
    0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x70, 0x75, 0x7a, 0x73, 0x72, 0x70, 0x32, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x70,
    0x75, 0x7a, 0x73, 0x72, 0x70, 0x32, 0x2e, 0x62, 0x6d, 0x70, 0x00, 0x00, 0x70, 0x75, 0x7a, 0x73,
    0x72, 0x70, 0x32, 0x63, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x70, 0x75, 0x7a, 0x74, 0x61, 0x71, 0x31,
    0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x70, 0x75, 0x7a, 0x74, 0x61, 0x71, 0x32, 0x2e, 0x73, 0x70,
    0x72, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x04, 0x00, 0x06, 0x00, 0x02, 0x00, 0x03, 0x00,
    0x07, 0x00, 0x09, 0x00, 0x05, 0x00, 0x08, 0x00, 0x01, 0x00, 0x04, 0x00, 0x06, 0x00, 0x02, 0x00,
    0x03, 0x00, 0x07, 0x00, 0x09, 0x00, 0x05, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00,
    0x05, 0x00, 0x06, 0x00, 0x07, 0x00, 0x09, 0x00, 0x08, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00,
    0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x00, 0x00, 0x08, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xf1, 0x00, 0x2d, 0x01, 0x68, 0x01, 0xf1, 0x00, 0x2d, 0x01, 0x68, 0x01, 0xf1, 0x00, 0x2d, 0x01,
    0x68, 0x01, 0x98, 0x00, 0x98, 0x00, 0x98, 0x00, 0xd3, 0x00, 0xd3, 0x00, 0xd3, 0x00, 0x0f, 0x01,
    0x0f, 0x01, 0x0f, 0x01, 0xdc, 0x00, 0x28, 0x01, 0x74, 0x01, 0xdc, 0x00, 0x28, 0x01, 0x74, 0x01,
    0xdc, 0x00, 0x28, 0x01, 0x74, 0x01, 0x6a, 0x00, 0x6a, 0x00, 0x6a, 0x00, 0xb6, 0x00, 0xb6, 0x00,
    0xb6, 0x00, 0x03, 0x01, 0x03, 0x01, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x7a,
    0x6d, 0x65, 0x74, 0x61, 0x70, 0x31, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x32, 0x00, 0x02, 0x00, 0x08, 0x00,
    0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x32, 0x00, 0x03, 0x00, 0x10, 0x00, 0x13, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xbe, 0x00, 0x32, 0x00, 0x04, 0x00, 0x18, 0x00, 0x17, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x2c, 0x01, 0x32, 0x00, 0x06, 0x00, 0x28, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x90, 0x01, 0x32, 0x00, 0x05, 0x00, 0x20, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc2, 0x01,
    0x32, 0x00, 0x04, 0x00, 0x18, 0x00, 0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x02, 0x32, 0x00,
    0x01, 0x00, 0x04, 0x00, 0x3d, 0x01, 0x87, 0x01, 0x00, 0x00, 0x04, 0x00, 0x41, 0x01, 0xcf, 0x00,
    0x00, 0x00, 0x02, 0x00, 0x23, 0x01, 0xf2, 0x00, 0x00, 0x00, 0x06, 0x00, 0x5f, 0x01, 0x00, 0x01,
    0xff, 0xff, 0x00, 0x00, 0xe2, 0x00, 0x57, 0x01, 0xff, 0xff, 0x00, 0x00, 0xa5, 0x01, 0x6c, 0x01,
    0xff, 0xff, 0x00, 0x00, 0x01, 0x01, 0x8f, 0x00, 0x01, 0x00, 0x04, 0x00, 0x3d, 0x01, 0x87, 0x01,
    0x01, 0x00, 0x06, 0x00, 0x4e, 0x01, 0x97, 0x01, 0x01, 0x00, 0x05, 0x00, 0x6e, 0x01, 0xa5, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x1e, 0x01, 0x99, 0x01, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0xf0, 0x00,
    0xff, 0xff, 0x00, 0x00, 0xa5, 0x01, 0x6c, 0x01, 0xff, 0xff, 0x00, 0x00, 0x01, 0x01, 0x8f, 0x00,
    0x02, 0x00, 0x07, 0x00, 0xcc, 0x01, 0x25, 0x01, 0x02, 0x00, 0x03, 0x00, 0xb8, 0x01, 0xe1, 0x00,
    0x02, 0x00, 0x04, 0x00, 0xec, 0x01, 0x03, 0x01, 0x02, 0x00, 0x05, 0x00, 0xe0, 0x01, 0xd4, 0x00,
    0x02, 0x00, 0x01, 0x00, 0xbf, 0x01, 0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0xf0, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x01, 0x01, 0x8f, 0x00, 0x03, 0x00, 0x04, 0x00, 0x3e, 0x01, 0x8a, 0x00,
    0x03, 0x00, 0x06, 0x00, 0x6f, 0x01, 0x3a, 0x00, 0x03, 0x00, 0x05, 0x00, 0x30, 0x01, 0x6a, 0x00,
    0x03, 0x00, 0x04, 0x00, 0x3d, 0x01, 0x39, 0x00, 0x03, 0x00, 0x02, 0x00, 0x0f, 0x01, 0x4c, 0x00,
    0x03, 0x00, 0x01, 0x00, 0x5e, 0x01, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0xf0, 0x00,
    0x00, 0x00, 0x3f, 0x01, 0x40, 0x01, 0x40, 0x01, 0x3f, 0x01, 0x00, 0x00, 0xef, 0x00, 0xef, 0x00,
    0xf0, 0x00, 0xf0, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0xff, 0xff,
    0xff, 0xff, 0x01, 0x00, 0x01, 0x00, 0xfe, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0xff,
    0x02, 0x00, 0x02, 0x00, 0xfe, 0xff, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0xff, 0xff,
    0x00, 0x00, 0x01, 0x00, 0xff, 0xff, 0x01, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
    0xfe, 0xff, 0x02, 0x00, 0xfe, 0xff, 0x02, 0x00, 0xfe, 0xff, 0x70, 0x75, 0x7a, 0x74, 0x61, 0x71,
    0x33, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x04, 0x00, 0x06, 0x00,
    0x02, 0x00, 0x03, 0x00, 0x07, 0x00, 0x09, 0x00, 0x05, 0x00, 0xf7, 0x00, 0x2e, 0x01, 0x65, 0x01,
    0xf7, 0x00, 0x2e, 0x01, 0x65, 0x01, 0xf7, 0x00, 0x2e, 0x01, 0x65, 0x01, 0x76, 0x00, 0x77, 0x00,
    0x77, 0x00, 0xad, 0x00, 0xad, 0x00, 0xad, 0x00, 0xe4, 0x00, 0xe5, 0x00, 0xe5, 0x00, 0x70, 0x75,
    0x7a, 0x68, 0x65, 0x78, 0x61, 0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x32, 0x00, 0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x6e, 0x00, 0x32, 0x00, 0x03, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbe, 0x00, 0x32, 0x00,
    0x01, 0x00, 0x04, 0x00, 0xdc, 0x00, 0x2c, 0x01, 0x01, 0x00, 0x04, 0x00, 0x35, 0x01, 0x78, 0x00,
    0x01, 0x00, 0x02, 0x00, 0x90, 0x01, 0x2c, 0x01, 0x70, 0x75, 0x7a, 0x74, 0x72, 0x6f, 0x6e, 0x65,
    0x2e, 0x73, 0x70, 0x72, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x00, 0x00, 0x86, 0x01,
    0x94, 0x00, 0x08, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00, 0x85, 0x01, 0x1b, 0x01, 0x10, 0x00,
    0x08, 0x00, 0x05, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x95, 0x00, 0x18, 0x00, 0x08, 0x00, 0x02, 0x00,
    0x00, 0x00, 0xfe, 0x00, 0x1b, 0x01, 0x20, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x01,
    0x04, 0x00, 0x22, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x01, 0x95, 0x01, 0x24, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x04, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x70, 0x00, 0x95, 0x01, 0x28, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x01,
];

/// The rules of the rat's run on the original's state: `FUN_004387ec`,
/// `FUN_004395f4`, `FUN_0043947c` and `FUN_00439d58`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatRules {
    /// The bytes at [`BASE`]..
    pub mem: Box<[u8; MEM_SIZE]>,
    /// 0x00772d90: the clock stepped (record 18 follows it).
    pub clock_ticked: u32,
    /// 0x004974c0.
    pub result: i32,
}

impl Default for RatRules {
    fn default() -> Self {
        RatRules {
            mem: Box::new(RAT_INITIAL),
            clock_ticked: 0,
            result: 0,
        }
    }
}

/// FUN_00451d6e then FISTP: truncation toward zero.
fn chop(v: f64) -> i32 {
    v as i32
}

impl RatRules {
    fn at(a: u32) -> usize {
        (a - BASE) as usize
    }

    /// The byte, short, int and float at an original address.
    pub fn b(&self, a: u32) -> u8 {
        self.mem[Self::at(a)]
    }
    pub fn set_b(&mut self, a: u32, v: u8) {
        self.mem[Self::at(a)] = v;
    }
    pub fn s(&self, a: u32) -> i16 {
        let p = Self::at(a);
        i16::from_le_bytes([self.mem[p], self.mem[p + 1]])
    }
    pub fn set_s(&mut self, a: u32, v: i32) {
        let p = Self::at(a);
        self.mem[p..p + 2].copy_from_slice(&(v as i16).to_le_bytes());
    }
    pub fn i(&self, a: u32) -> i32 {
        let p = Self::at(a);
        i32::from_le_bytes([
            self.mem[p],
            self.mem[p + 1],
            self.mem[p + 2],
            self.mem[p + 3],
        ])
    }
    pub fn set_i(&mut self, a: u32, v: i32) {
        let p = Self::at(a);
        self.mem[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn f(&self, a: u32) -> f32 {
        f32::from_bits(self.i(a) as u32)
    }
    pub fn set_f(&mut self, a: u32, v: f32) {
        self.set_i(a, v.to_bits() as i32);
    }

    /// Record `r`'s field.
    fn rb(&self, r: i32, off: u32) -> u8 {
        self.b(rec(r, off))
    }
    fn set_rb(&mut self, r: i32, off: u32, v: u8) {
        self.set_b(rec(r, off), v);
    }
    fn rs(&self, r: i32, off: u32) -> i32 {
        i32::from(self.s(rec(r, off)))
    }
    fn set_rs(&mut self, r: i32, off: u32, v: i32) {
        self.set_s(rec(r, off), v);
    }

    /// `FUN_004387ec`: the reset before each rat.
    pub fn reset(&mut self) {
        for r in 0..NUM_RECORDS {
            self.set_rb(r, ABOVE, 0);
            self.set_rb(r, ACTIVE, 1);
            self.set_rb(r, PRESSED, 0);
            let rest = self.rs(r, REST);
            self.set_rs(r, FRAME, rest);
        }
        // Record 0 inactive and pressed, record 18 in its middle position,
        // record 19 turning one way, records 25..27 drawn above the rat.
        self.set_b(0x0049_69c9, 0);
        self.set_b(0x0049_6aaf, 1);
        self.set_b(0x0049_69ca, 1);
        self.set_b(0x0049_6b68, 2);
        self.set_b(0x0049_6b7f, 1);
        self.set_b(0x0049_6b7e, 1);
        self.set_b(0x0049_6c07, 1);
        self.set_b(0x0049_6c1e, 1);
        self.set_b(0x0049_6c35, 1);
        self.set_i(0x0049_6cc0, 0);
        let v = self.f(0x0049_6cb4);
        self.set_s(0x0049_6abd, chop(f64::from(v)));
        self.set_i(0x0049_6cb8, 0x3dc3_0c31);
    }

    /// `FUN_004395f4`: the click (`FUN_0043500c` first) and held-button
    /// rules; the sounds to play are pushed on `sounds`.
    pub fn clicks(&mut self, mx: i32, my: i32, click: bool, held: bool, sounds: &mut Vec<i32>) {
        if click {
            // FUN_0043500c: a click at the side edges leaves ([0x00496d0c]).
            if !(10..=600).contains(&mx) {
                self.result = 1;
            }
            let hit = |x: i32, y: i32| x - 10 < mx && mx < x + 0x3c && y < my && my < y + 0x1e;
            let open = |s: &Self, r: i32| s.b(ENDED) == 0 && s.rb(r, ACTIVE) == 1;
            if hit(self.rs(7, X), self.rs(7, Y)) && open(self, 7) {
                sounds.push(self.rs(7, SOUND));
                if self.b(0x0049_6b7f) == 1 {
                    self.set_b(0x0049_6b7f, 2);
                    let f = self.rs(7, PRESSED_FRAME);
                    self.set_rs(7, FRAME, f);
                } else {
                    self.set_b(0x0049_6b7f, 1);
                    let f = self.rs(7, REST);
                    self.set_rs(7, FRAME, f);
                }
            }
            if hit(self.rs(9, X), self.rs(9, Y)) && open(self, 9) {
                sounds.push(self.rs(9, SOUND));
            }
            if hit(self.rs(6, X), self.rs(6, Y)) && open(self, 6) {
                sounds.push(self.rs(6, SOUND));
            }
        }

        if held {
            for r in 1..=10 {
                let (x, y) = (self.rs(r, X), self.rs(r, Y));
                if !(x - 10 < mx && mx < x + 0x32 && y < my && my < y + 0x1e) {
                    continue;
                }
                if self.b(ENDED) != 0 || self.rb(r, ACTIVE) != 1 || self.rb(r, PRESSED) != 0 {
                    continue;
                }
                let snd = self.rs(r, SOUND);
                let playable = (0..0x19).contains(&snd);
                match r {
                    1 | 2 | 4 | 8 => {
                        if playable {
                            sounds.push(snd);
                        }
                        self.set_rb(r, PRESSED, 1);
                        let f = self.rs(r, PRESSED_FRAME);
                        self.set_rs(r, FRAME, f);
                        let p = self.rs(r, PARTNER);
                        self.set_rb(p, PRESSED, 1);
                        let f = self.rs(p, PRESSED_FRAME);
                        self.set_rs(p, FRAME, f);
                    }
                    5 => {
                        if playable {
                            sounds.push(snd);
                        }
                        let f = self.rs(r, PRESSED_FRAME);
                        self.set_rs(r, FRAME, f);
                        self.set_rb(r, PRESSED, 1);
                        let p = self.rs(r, PARTNER);
                        self.set_rb(p, PRESSED, 1);
                    }
                    3 => {
                        if playable {
                            sounds.push(snd);
                        }
                        self.set_rb(r, PRESSED, 1);
                    }
                    10 => {
                        if self.b(STARTED) == 0 && playable {
                            sounds.push(snd);
                        }
                        self.set_b(0x0049_6aaf, 0);
                        self.set_b(0x0049_6ab0, 1);
                        self.set_b(STARTED, 1);
                    }
                    _ => {}
                }
            }
        } else {
            for r in [1, 2, 4, 8] {
                if self.rb(r, ACTIVE) != 1 {
                    continue;
                }
                self.set_rb(r, PRESSED, 0);
                let f = self.rs(r, REST);
                self.set_rs(r, FRAME, f);
                let p = self.rs(r, PARTNER);
                self.set_rb(p, PRESSED, 0);
                let f = self.rs(p, REST);
                self.set_rs(p, FRAME, f);
            }
        }

        if held {
            // Record 9 held: every second pass the wheel (its partner)
            // turns.
            let (x, y) = (self.rs(9, X), self.rs(9, Y));
            if x - 10 < mx && mx < x + 0x32 && y < my && my < y + 0x28 {
                if self.i(0x0049_6ccc) == 1 {
                    self.set_i(0x0049_6ccc, 0);
                    self.set_rb(9, PRESSED, 1);
                    let f = self.rs(9, PRESSED_FRAME);
                    self.set_rs(9, FRAME, f);
                    let p = self.rs(9, PARTNER);
                    let f = self.rs(p, FRAME) + 1;
                    self.set_rs(p, FRAME, f);
                    if self.rs(p, FRAME) > self.rs(p, PRESSED_FRAME) {
                        let f = self.rs(p, PRESSED_FRAME);
                        self.set_rs(p, FRAME, f);
                    }
                } else {
                    self.set_i(0x0049_6ccc, 1);
                }
            }
            let (x, y) = (self.rs(6, X), self.rs(6, Y));
            if x - 10 < mx && mx < x + 0x32 && y < my && my < y + 0x28 {
                self.set_rb(6, PRESSED, 1);
                let f = self.rs(6, PRESSED_FRAME);
                self.set_rs(6, FRAME, f);
                let p = self.rs(6, PARTNER);
                let f = self.rs(p, PRESSED_FRAME);
                self.set_rs(p, FRAME, f);
            }
        } else {
            for r in [9, 6] {
                self.set_rb(r, PRESSED, 0);
                let f = self.rs(r, REST);
                self.set_rs(r, FRAME, f);
                let p = self.rs(r, PARTNER);
                let f = self.rs(p, REST);
                self.set_rs(p, FRAME, f);
            }
        }
    }

    /// `FUN_0043947c`: the rat's animation step.
    pub fn animate_rat(&mut self) {
        if self.b(RAT_BYTE) == 1 {
            self.set_b(RAT_BYTE, 6);
        }
        if self.b(RAT_BYTE) == 0 {
            let n = self.i(0x0049_6cd0).wrapping_add(1);
            self.set_i(0x0049_6cd0, n);
        }
        // The entry: {first, last, current}. With the data-section byte
        // 0x68 (the first rat's wait for the lever, mode 0) it is
        // 0x00496eea..0x00496eef: the original increments the short at
        // 0x00496eee, inside puzzle 3's sprite name at 0x00496eec, every
        // frame (0x00439513..0x0043951d) and resets it to 0 after 502
        // frames (0x0043955c..0x00439563); nothing restores the name. Here
        // the write stays in `mem` (see [`MEM_SIZE`]).
        let e = 0x0049_6c7a + 6 * u32::from(self.b(RAT_BYTE));
        let step = |s: &mut Self| {
            let v = i32::from(s.s(e + 4)) + 1;
            s.set_s(e + 4, v);
        };
        let wrap = |s: &mut Self| {
            if s.s(e + 4) > s.s(e + 2) {
                let v = s.s(e);
                s.set_s(e + 4, v.into());
            }
        };
        match self.b(RAT_MODE) {
            0 => {
                if self.b(RAT_BYTE) == 0 {
                    if self.i(0x0049_6cd0) > 10 {
                        step(self);
                        self.set_i(0x0049_6cd0, 0);
                    }
                } else {
                    step(self);
                }
                wrap(self);
            }
            1 => {
                let v = self.s(e);
                self.set_s(e + 4, v.into());
            }
            2 => {
                step(self);
                wrap(self);
            }
            _ => {}
        }
    }

    /// `FUN_00439d58` for record `r` at time `ticks` (200 Hz).
    pub fn update_record(&mut self, r: i32, ticks: u32) {
        if r == 9 || r == 6 || r == 5 || self.rb(r, PRESSED) == 0 {
            return;
        }
        match self.s(rec(r, KIND)) as u16 {
            0 => {
                if r == 0x12 && self.i(0x0049_6cc0) == 0 {
                    if self.clock_ticked == 1 {
                        // Record 18 steps with the clock, turning at its
                        // ends.
                        let v =
                            i32::from(self.s(0x0049_6b75)) + i32::from(self.i(0x0049_6cbc) as i16);
                        self.set_s(0x0049_6b75, v);
                        if self.s(0x0049_6b75) > self.s(0x0049_6b73) {
                            let v = i32::from(self.s(0x0049_6b71)) + 1;
                            self.set_s(0x0049_6b75, v);
                            self.set_i(0x0049_6cbc, -1);
                        } else if self.s(0x0049_6b75) < self.s(0x0049_6b71) {
                            let v = i32::from(self.s(0x0049_6b73)) - 1;
                            self.set_s(0x0049_6b75, v);
                            self.set_i(0x0049_6cbc, 1);
                        }
                        let cur = i32::from(self.s(0x0049_6b75));
                        if cur == i32::from(self.s(0x0049_6b71)) {
                            self.set_b(0x0049_6b68, 1);
                        } else if cur == i32::from(self.s(0x0049_6b73)) {
                            self.set_b(0x0049_6b68, 3);
                        } else if cur == i32::from(self.s(0x0049_6b73)) - 1 {
                            self.set_b(0x0049_6b68, 2);
                        }
                        self.clock_ticked = 0;
                    }
                    return;
                }
                if r == 0x12 {
                    return;
                }
                let f = if self.rb(r, PRESSED) == 0 {
                    self.rs(r, REST)
                } else {
                    self.rs(r, PRESSED_FRAME)
                };
                self.set_rs(r, FRAME, f);
            }
            1 => {
                if r == 0x13 {
                    if self.b(0x0049_6b7f) == 1 {
                        let v = i32::from(self.s(0x0049_6b8c)) + 1;
                        self.set_s(0x0049_6b8c, v);
                        if self.s(0x0049_6b8c) > self.s(0x0049_6b8a) {
                            let v = self.s(0x0049_6b88);
                            self.set_s(0x0049_6b8c, v.into());
                        }
                    } else if self.b(0x0049_6b7f) == 2 {
                        let v = i32::from(self.s(0x0049_6b8c)) - 1;
                        self.set_s(0x0049_6b8c, v);
                        if self.s(0x0049_6b8c) < self.s(0x0049_6b88) {
                            let v = self.s(0x0049_6b8a);
                            self.set_s(0x0049_6b8c, v.into());
                        }
                    }
                    return;
                }
                let c = self.rs(r, COUNTER) + 1;
                self.set_rs(r, COUNTER, c);
                if self.rs(r, COUNTER) > 0xf {
                    self.set_rb(r, PRESSED, 0);
                    let f = self.rs(r, FRAME) - 1;
                    self.set_rs(r, FRAME, f);
                }
            }
            2 => {
                if r == 0 {
                    // The clock: a step every 0x5dc ticks once the run
                    // started.
                    if ticks > self.i(0x0049_69c4) as u32 && self.b(STARTED) == 1 {
                        self.set_i(0x0049_69c4, ticks.wrapping_add(0x5dc) as i32);
                        let v = i32::from(self.s(0x0049_69d7)) + 1;
                        self.set_s(0x0049_69d7, v);
                        if self.i(0x0049_6cb0) != 0 {
                            let v = self.f(0x0049_6cb4) - self.f(0x0049_6cb8);
                            self.set_f(0x0049_6cb4, v);
                            self.set_s(0x0049_6abd, chop(f64::from(v) + 0.5));
                        }
                        if self.s(0x0049_69d7) > self.s(0x0049_69d5) {
                            self.set_b(ENDED, 1);
                            self.result = 0xa;
                            let v = self.s(0x0049_69d5);
                            self.set_s(0x0049_69d7, v.into());
                        }
                        self.clock_ticked = 1;
                    }
                    return;
                }
                let f = self.rs(r, FRAME) + 1;
                self.set_rs(r, FRAME, f);
                if self.rs(r, FRAME) > self.rs(r, PRESSED_FRAME) {
                    let f = self.rs(r, PRESSED_FRAME);
                    self.set_rs(r, FRAME, f);
                    if r == 10 {
                        self.set_i(0x0049_6cb0, 1);
                        self.set_b(0x0049_6aaf, 1);
                        self.set_b(0x0049_6ab0, 0);
                    } else if r == 0x10 {
                        self.set_b(0x0049_6b39, 0);
                    } else {
                        self.set_rb(r, PRESSED, 0);
                    }
                }
            }
            3 => {
                if self.rs(r, FRAME) >= self.rs(r, PRESSED_FRAME) {
                    let f = self.rs(r, PRESSED_FRAME);
                    self.set_rs(r, FRAME, f);
                    if r == 0x10 {
                        let n = self.i(0x0049_6cd4).wrapping_add(1);
                        self.set_i(0x0049_6cd4, n);
                        if self.i(0x0049_6cd4) > 0x32 {
                            let v = self.s(0x0049_6a46);
                            self.set_s(0x0049_6a4a, v.into());
                            self.set_b(0x0049_6a3d, 0);
                            self.set_b(0x0049_6b3a, 0);
                            let v = self.s(0x0049_6b43);
                            self.set_s(0x0049_6b47, v.into());
                            self.set_i(0x0049_6cd4, 0);
                        }
                    } else if r == 10 {
                        self.set_rb(r, PRESSED, 2);
                    } else {
                        self.set_rb(r, PRESSED, 0);
                        let f = self.rs(r, REST);
                        self.set_rs(r, FRAME, f);
                    }
                } else {
                    let f = self.rs(r, FRAME) + 1;
                    self.set_rs(r, FRAME, f);
                }
            }
            _ => {}
        }
    }

    /// Whether `FUN_00438fd4` updates record `r`: record 10 while
    /// inactive, the others while active.
    fn updated(&self, r: i32) -> bool {
        if r == 10 {
            self.rb(10, ACTIVE) == 0
        } else {
            self.rb(r, ACTIVE) == 1
        }
    }

    /// The updates of `FUN_00438fd4` without its drawing:
    /// `FUN_0043947c`, then `FUN_00439d58` for records 0..27.
    pub fn frame_updates(&mut self, ticks: u32) {
        self.animate_rat();
        for r in 0..28 {
            if self.updated(r) {
                self.update_record(r, ticks);
            }
        }
    }
}

/// `FUN_00434dfc`: slot k reads PUZZLES\<name> (the names at 0x004bdf48).
pub const SLOT_FILES: [&str; NUM_SLOTS] = [
    "T0001.TRJ",
    "T0002.TRJ",
    "T0003.TRJ",
    "T0004.TRJ",
    "T0005.TRJ",
    "T0006.TRJ",
    "T0007.TRJ",
    "T0007A.TRJ",
    "T0007B.TRJ",
    "T0008.TRJ",
    "T0009.TRJ",
    "T0010.TRJ",
    "T0011.TRJ",
    "T0011A.TRJ",
    "T0011B.TRJ",
    "T0012.TRJ",
    "T0013.TRJ",
    "T0014.TRJ",
    "T0015.TRJ",
    "T0020.TRJ",
    "T0021.TRJ",
    "T0022.TRJ",
    "T0023.TRJ",
    "T0024.TRJ",
    "T0025.TRJ",
    "T0099.TRJ",
    "T00025.TRJ",
    "T1000.TRJ",
    "T0004BB.TRJ",
    "T0000.TRJ",
];
pub const NUM_SLOTS: usize = 30;

/// One trajectory: per point the rat's animation byte and its position,
/// and the point reached.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slot {
    pub b: Vec<u8>,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub pos: i16,
}

impl Slot {
    /// A TRJ file: an s16 count n, n bytes, n x floats, n y floats.
    /// `None` for a short file.
    pub fn parse(d: &[u8]) -> Option<Slot> {
        let n = i16::from_le_bytes([*d.first()?, *d.get(1)?]);
        let n = usize::try_from(n).ok()?;
        if d.len() < 2 + 9 * n {
            return None;
        }
        let float = |p: usize| f32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]);
        Some(Slot {
            b: d[2..2 + n].to_vec(),
            x: (0..n).map(|i| float(2 + n + 4 * i)).collect(),
            y: (0..n).map(|i| float(2 + 5 * n + 4 * i)).collect(),
            pos: 0,
        })
    }

    fn len(&self) -> i32 {
        self.b.len() as i32
    }
}

/// The samples `FUN_00438bf0` loads into 0x007739b8 + 8 * slot (flip15
/// is not one).
pub const SOUNDS: [&str; 25] = [
    "FLIP1", "FLIP2", "FLIP3", "FLIP4", "FLIP5", "FLIP6", "FLIP7", "FLIP8", "FLIP9", "FLIP10",
    "FLIP11", "FLIP12", "FLIP13", "FLIP14", "FLIP16", "FLIP17", "FLIP18", "FLIP19", "FLIP20",
    "FLIP21", "FLIP22", "FLIP23", "FLIP24", "FLIP25", "FLIP26",
];

/// The game's I/O: the C++ `RatGame` virtual functions.
#[allow(async_fn_in_trait)]
pub trait RatIo {
    /// `FUN_0043318c` (the music stream refill), the first call of each
    /// pass: where the input is read, which the original's `FUN_0041a298`
    /// polls at the end of the previous pass (nothing in between reads
    /// it). The mouse, a new left click, the held flag.
    fn read_input(&mut self, g: &RatGame) -> (i32, i32, bool, bool);
    /// The background copies: 0 and 1, `FUN_00438fd4`'s two copies of the
    /// background; 2, the frame's copy before the present.
    fn draw_background(&mut self, which: i32);
    /// 'P' PZFPIECE, 'R' PZFRAT.
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32);
    /// `FUN_00438ee4` (the cursor) and `FUN_0041afd0`.
    async fn present(&mut self, g: &RatGame);
    /// One of [`SOUNDS`] (`FUN_0043cd60`).
    fn play_sound(&mut self, slot: i32);
    /// `FUN_00453cd1`.
    fn random_value(&mut self) -> i32;
    /// The 200 Hz timer 0x006ce118.
    fn ticks(&mut self) -> u32;
    /// `FUN_00438fd4`'s busy wait until the timer reaches `t`.
    fn wait_until(&mut self, t: u32);
    fn aborted(&self) -> bool {
        false
    }
}

/// The trajectory functions, each a loop of passes that ends by calling
/// the next one (all the calls are tail calls; the C++ recurses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Path {
    T0001,
    T0002,
    T0003,
    T0004,
    T0005,
    T0009,
    Wheel,
    T0006,
    T0007,
    T0007A,
    T0007B,
    T0008,
    Ride,
    T0010,
    T0011,
    T0011A,
    T0011B,
    T0012,
    T0013,
    T0014,
    T0015,
    T0020,
    T00025,
    T0021,
    T0022,
    T0023,
    T0025,
    T0099,
}

/// The rat's run: `FUN_00438bf0` and its trajectory functions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RatGame {
    pub rules: RatRules,
    pub slots: Vec<Slot>,
    /// 0x00772d94, set by the wheel's hold (0x00435d24).
    pub wheel_y: i32,
    /// The 1997 build's pass pacing (atlantis-cd-en-688128 FUN_00435a0c @
    /// 0x00435df9..0x00435e0e): wait until the tick reaches `next_tick`
    /// (0x007e2d4c), then `next_tick` = tick + 5.
    pub deadline_pacing: bool,
    pub next_tick: u32,
    /// The 1997 build's T0023 (atlantis-cd-en-688128 FUN_00434c6c): the
    /// search for the first point left of the rat goes on from the
    /// trajectory's position instead of point 0.
    pub keep_search_position: bool,
    mx: i32,
    my: i32,
    click: bool,
    held: bool,
}

impl RatGame {
    /// A game on `rules` (the static state) with the 30 trajectories.
    pub fn new(rules: RatRules, slots: Vec<Slot>) -> RatGame {
        RatGame {
            rules,
            slots,
            ..RatGame::default()
        }
    }

    fn result_set(&self, io: &impl RatIo) -> bool {
        self.rules.result != 0 || io.aborted()
    }

    fn play_sounds(io: &mut impl RatIo, sounds: &[i32]) {
        for &s in sounds {
            io.play_sound(s);
        }
    }

    /// The input and the click rules that start every pass.
    fn input(&mut self, io: &mut impl RatIo) {
        let (mx, my, click, held) = io.read_input(self);
        (self.mx, self.my, self.click, self.held) = (mx, my, click, held);
        let mut snd = Vec::new();
        self.rules.clicks(mx, my, click, held, &mut snd);
        Self::play_sounds(io, &snd);
    }

    /// The pass of every loop.
    async fn frame(&mut self, io: &mut impl RatIo) {
        self.input(io);
        self.update_and_draw(io);
        io.draw_background(2);
        io.present(self).await;
    }

    fn draw_record(&self, io: &mut impl RatIo, k: i32) {
        let r = &self.rules;
        io.draw_sprite('P', r.rs(k, X), r.rs(k, Y), r.rs(k, FRAME));
    }

    /// `FUN_00438fd4`: the updates and the drawing of a pass.
    fn update_and_draw(&mut self, io: &mut impl RatIo) {
        self.rules.animate_rat();
        io.draw_background(0);
        io.draw_background(1);
        let now = io.ticks();
        for k in 0..28 {
            if self.rules.updated(k) {
                self.rules.update_record(k, now);
            }
            if self.rules.rb(k, ABOVE) == 0 {
                self.draw_record(io, k);
            }
        }
        // The rat: (int16) of the truncated coordinates, the animation's
        // frame.
        let r = &self.rules;
        io.draw_sprite(
            'R',
            i32::from(chop(f64::from(r.f(RAT_X))) as i16),
            i32::from(chop(f64::from(r.f(RAT_Y))) as i16),
            i32::from(r.s(0x0049_6c7e + 6 * u32::from(r.b(RAT_BYTE)))),
        );
        for k in 1..30 {
            if self.rules.rb(k, ABOVE) == 1 {
                self.draw_record(io, k);
            }
        }
        self.draw_record(io, 28);
        self.draw_record(io, 29);
        if self.rules.b(RATS) != 0 {
            self.draw_rats_left(io);
        }
        // Each pass lasts at least 5 ticks (0x0077375c); the 1997 build
        // waits for the deadline the previous pass set, then sets the next.
        if self.deadline_pacing {
            io.wait_until(self.next_tick);
            self.next_tick = io.ticks().wrapping_add(5);
        } else {
            let t = io.ticks().wrapping_add(5);
            io.wait_until(t);
        }
        if io.ticks() > self.rules.i(AMBIENT_AT) as u32 {
            if io.random_value() % 3 == 0
                && self.rules.b(STARTED) == 1
                && self.rules.i(LOSS_ANIM) == 0
            {
                let t = io.ticks();
                io.play_sound(16 + (t & 3) as i32);
            }
            let t = io.ticks().wrapping_add(400);
            self.rules.set_i(AMBIENT_AT, t as i32);
        }
    }

    /// `FUN_004388d8`: the rats left.
    fn draw_rats_left(&self, io: &mut impl RatIo) {
        let mut x = i32::from(self.rules.b(RATS)) * 0x17 + 0x46;
        loop {
            io.draw_sprite('R', x, 0x1d6, 2);
            x -= 0x17;
            if x <= 0x46 {
                break;
            }
        }
    }

    /// `FUN_0043504c`: the next point of trajectory `k`; false past its
    /// end.
    fn step_slot(&mut self, k: usize) -> bool {
        let add = match k {
            0 | 0x13 | 0x1a | 0x19 | 0x14 | 0x17 | 0x16 | 0x18 | 0x15 => 1,
            6 => {
                if self.slots[6].pos < 0x32 {
                    3
                } else {
                    2
                }
            }
            9 => 3,
            _ => 2,
        };
        let s = &mut self.slots[k];
        s.pos = s.pos.wrapping_add(add);
        if i32::from(s.pos) >= s.len() {
            return false;
        }
        let p = s.pos as usize;
        let (b, x, y) = (s.b[p], s.x[p], s.y[p]);
        self.rules.set_b(RAT_BYTE, b);
        self.rules.set_f(RAT_X, x);
        self.rules.set_f(RAT_Y, y);
        true
    }

    /// `FUN_00437f6c`: the rat within 20 pixels of the jump (200, 300).
    fn near_jump(&self) -> bool {
        let dx = chop(f64::from(self.rules.f(RAT_X)) + -200.0).abs();
        let dy = chop(f64::from(self.rules.f(RAT_Y)) + -300.0).abs();
        let (dx, dy) = (dx as f32, dy as f32);
        let d = f64::from(dx * dx + dy * dy).sqrt() as f32;
        d < 20.0
    }

    /// A plain trajectory loop.
    async fn run_slot(&mut self, io: &mut impl RatIo, k: usize) {
        self.slots[k].pos = 0;
        loop {
            self.frame(io).await;
            let more = self.step_slot(k);
            if self.result_set(io) || !more {
                break;
            }
        }
    }

    /// A trajectory loop that ends when the jump lever (record 6) is
    /// pressed near the jump: true then.
    async fn run_slot_to_jump(&mut self, io: &mut impl RatIo, k: usize) -> bool {
        loop {
            self.frame(io).await;
            let mut more = self.step_slot(k);
            let mut jump = false;
            if self.rules.rb(6, PRESSED) == 1 && self.near_jump() {
                jump = true;
                more = false;
            }
            if self.result_set(io) || !more {
                return jump;
            }
        }
    }

    /// `FUN_00438bf0` after the loads: 255 won, 2 lost, 1 left by an edge
    /// click (0xa, the clock run out, is turned into 2).
    pub async fn run(&mut self, io: &mut impl RatIo) -> i32 {
        let r = &mut self.rules;
        r.result = 0;
        // 0x00438c3f; the rest of the state is kept from the last play.
        r.set_i(AMBIENT_AT, 0);
        r.set_i(0x0049_6cb0, 0);
        r.set_f(0x0049_6cb4, 98.0);
        r.set_b(RATS, 5);
        r.reset();
        let v = r.s(0x0049_6ab9);
        r.set_s(0x0049_6abd, v.into());
        r.set_b(ENDED, 0);
        self.first_rat(io).await;
        if self.rules.result == LOST {
            let n = self.rules.b(RATS).wrapping_sub(1);
            self.rules.set_b(RATS, n);
            self.rules.result = 0;
            self.lose_rat(io).await;
        }
        loop {
            if io.aborted() {
                break;
            }
            let r = &mut self.rules;
            r.set_i(0x0049_6cb0, 0);
            r.set_f(0x0049_6cb4, 98.0);
            r.reset();
            let v = r.s(0x0049_6ab9);
            r.set_s(0x0049_6abd, v.into());
            r.set_b(STARTED, 0);
            self.next_rat(io).await;
            if self.rules.result == LOST {
                let n = self.rules.b(RATS).wrapping_sub(1);
                self.rules.set_b(RATS, n);
                self.rules.result = 0;
                self.lose_rat(io).await;
                if self.rules.b(RATS) == 0 {
                    self.rules.result = LOST;
                }
            }
            if self.rules.result != 0 || self.rules.b(ENDED) != 0 {
                break;
            }
        }
        if self.rules.b(ENDED) == 1 {
            self.rules.result = LOST;
        }
        self.rules.result
    }

    /// The rat out of sight until the start lever (record 10) is pulled.
    async fn wait_start(&mut self, io: &mut impl RatIo) {
        loop {
            self.frame(io).await;
            if self.rules.b(STARTED) != 0 || self.rules.result != 0 || io.aborted() {
                break;
            }
        }
    }

    /// The parts active, then the passes until the first lever (record 1)
    /// is pressed, then the run.
    async fn wait_first_lever(&mut self, io: &mut impl RatIo) {
        for k in (0..10).chain(11..29) {
            self.rules.set_rb(k, ACTIVE, 1);
        }
        loop {
            self.frame(io).await;
            if self.rules.result != 0 || self.rules.rb(1, PRESSED) != 0 || io.aborted() {
                break;
            }
        }
        self.rules.set_rb(1, ACTIVE, 0);
        self.rules.set_b(RAT_MODE, 0);
        self.run_paths(io, Path::T0001).await;
    }

    /// `FUN_004382a4`.
    async fn first_rat(&mut self, io: &mut impl RatIo) {
        let r = &mut self.rules;
        r.set_b(STARTED, 0);
        r.set_f(RAT_X, -100.0);
        r.set_f(RAT_Y, -100.0);
        r.set_b(RAT_MODE, 0);
        self.wait_start(io).await;
        self.run_slot(io, 0x1b).await; // 0x004351ac: T1000, the drop
        let r = &mut self.rules;
        let y = r.f(RAT_Y) + 6.0;
        r.set_f(RAT_Y, y);
        let n = r.b(RATS).wrapping_sub(1);
        r.set_b(RATS, n);
        if r.result != 0 || io.aborted() {
            return;
        }
        self.wait_first_lever(io).await;
    }

    /// `FUN_00438548`.
    async fn next_rat(&mut self, io: &mut impl RatIo) {
        let r = &mut self.rules;
        r.set_b(STARTED, 0);
        r.set_f(RAT_X, -100.0);
        r.set_f(RAT_Y, -100.0);
        r.set_b(RAT_MODE, 0);
        self.wait_start(io).await;
        if self.rules.result != 0 || io.aborted() {
            return;
        }
        self.run_slot(io, 0x1b).await; // 0x004351ac: T1000, the drop
        let y = self.rules.f(RAT_Y) + 6.0;
        self.rules.set_f(RAT_Y, y);
        self.wait_first_lever(io).await;
    }

    /// `FUN_004389c4`: the rat lost, its icon blinking.
    async fn lose_rat(&mut self, io: &mut impl RatIo) {
        self.rules.set_i(LOSS_ANIM, 1);
        let x = (i32::from(self.rules.b(RATS)) + 1) * 0x17 + 0x46;
        let (mut count, mut shown) = (0, false);
        let mut next = io.ticks().wrapping_add(0xc8);
        io.play_sound(14);
        loop {
            self.input(io);
            self.update_and_draw(io);
            if io.ticks() > next {
                next = io.ticks().wrapping_add(0xc8);
                count += 5;
                shown = !shown;
            }
            if shown {
                io.draw_sprite('R', x, 0x1d6, 2);
            }
            io.draw_background(2);
            io.present(self).await;
            if self.rules.result != 0 || count >= 0xf || io.aborted() {
                break;
            }
        }
        self.rules.set_i(LOSS_ANIM, 0);
    }

    /// The trajectory functions from `first`, each followed by the one it
    /// calls last.
    async fn run_paths(&mut self, io: &mut impl RatIo, first: Path) {
        let mut next = Some(first);
        while let Some(p) = next {
            next = self.path(io, p).await;
        }
    }

    /// One trajectory function; the one it calls at its end.
    async fn path(&mut self, io: &mut impl RatIo, p: Path) -> Option<Path> {
        use Path::*;
        match p {
            // 0x004352d4
            T0001 => {
                self.rules.set_rb(3, ACTIVE, 0);
                self.run_slot(io, 0).await;
                self.rules.set_rb(1, ACTIVE, 1);
                if self.result_set(io) {
                    return None;
                }
                Some(if self.rules.rb(2, PRESSED) == 1 {
                    T0002
                } else {
                    T0003
                })
            }
            // 0x00435424
            T0003 => {
                self.rules.set_rb(25, ABOVE, 1);
                self.rules.set_rb(2, ACTIVE, 0);
                self.run_slot(io, 2).await;
                self.rules.set_rb(2, ACTIVE, 1);
                self.lost()
            }
            // 0x00435574
            T0002 => {
                self.rules.set_rb(2, ACTIVE, 0);
                self.run_slot(io, 1).await;
                self.rules.set_rb(3, ACTIVE, 1);
                self.rules.set_rb(2, ACTIVE, 1);
                self.then(io, T0004)
            }
            // 0x004356bc
            T0004 => self.t0004(io).await,
            // 0x00435a88
            T0005 => {
                self.rules.set_rb(27, ABOVE, 1);
                self.rules.set_rb(4, ACTIVE, 0);
                self.run_slot(io, 4).await;
                self.rules.set_rb(4, ACTIVE, 1);
                self.lost()
            }
            // 0x00435bd8
            T0009 => {
                self.rules.set_rb(4, ACTIVE, 0);
                self.run_slot(io, 0xa).await;
                self.rules.set_rb(4, ACTIVE, 1);
                self.then(io, Wheel)
            }
            // 0x00435f54
            Wheel => self.wheel(io).await,
            // 0x0043610c
            T0006 => {
                self.run_slot(io, 5).await;
                self.then(io, Wheel)
            }
            // 0x00436240
            T0007 => self.t0007(io).await,
            // 0x004363c0
            T0007A => {
                self.run_slot(io, 7).await;
                self.then(io, Wheel)
            }
            // 0x004364f4
            T0007B => {
                self.rules.set_rb(27, ABOVE, 1);
                self.run_slot(io, 8).await;
                let f = self.rules.rs(24, FRAME) + 1;
                self.rules.set_rs(24, FRAME, f);
                self.lost()
            }
            // 0x0043663c
            T0008 => {
                self.run_slot(io, 9).await;
                self.then(io, Ride)
            }
            // 0x00436770
            Ride => self.ride(io).await,
            // 0x0043696c
            T0013 => {
                self.run_slot(io, 0x10).await;
                if self.result_set(io) {
                    return None;
                }
                Some(if self.rules.rb(8, PRESSED) == 1 {
                    T0014
                } else {
                    T0015
                })
            }
            // 0x00436ab0
            T0014 => {
                self.run_slot(io, 0x11).await;
                self.then(io, Wheel)
            }
            // 0x00436be4
            T0015 => {
                self.rules.set_rb(27, ABOVE, 1);
                self.run_slot(io, 0x12).await;
                self.lost()
            }
            // 0x00436d24
            T0010 => {
                self.run_slot(io, 0xb).await;
                self.then(io, T0020)
            }
            // 0x00436e58
            T0011 => {
                self.run_slot(io, 0xc).await;
                if self.result_set(io) {
                    return None;
                }
                let r = &mut self.rules;
                let dir = r.i(0x0049_6cbc);
                let f = r.rs(18, FRAME) + i32::from(dir as i16);
                r.set_rs(18, FRAME, f);
                let st = r.rb(18, PRESSED).wrapping_add(dir as u8);
                r.set_rb(18, PRESSED, st);
                match st {
                    1 => Some(T0011A),
                    3 => Some(T0011B),
                    _ => None,
                }
            }
            // 0x00436fc8
            T0011A => {
                self.rules.set_rb(27, ABOVE, 1);
                self.run_slot(io, 0xd).await;
                self.lost()
            }
            // 0x00437108
            T0011B => {
                self.rules.set_rb(27, ABOVE, 1);
                self.run_slot(io, 0xe).await;
                self.then(io, T0020)
            }
            // 0x00437244
            T0012 => {
                self.rules.set_rb(27, ABOVE, 1);
                self.run_slot(io, 0xf).await;
                self.lost()
            }
            // 0x00437384
            T0020 => self.t0020(io).await,
            // 0x00437608
            T00025 => {
                self.slots[0x1a].pos = 0;
                let jump = self.run_slot_to_jump(io, 0x1a).await;
                self.jump(io, jump)
            }
            // 0x00437898
            T0021 => self.t0021(io).await,
            // 0x00437b38
            T0023 => {
                self.rules.set_rb(26, ABOVE, 1);
                self.rules.set_rb(23, ABOVE, 1);
                // T0023 starts at the first point whose x is not right of
                // the rat's (0x00437b6f clears the position first, as does
                // atlantis-cd-es-687104 FUN_004348fc @ 0x00434933; the
                // other 1997 builds search on from where the trajectory was:
                // past its end the original reads beyond the points, the
                // port moves nothing).
                let rx = self.rules.f(RAT_X);
                let keep = self.keep_search_position;
                let s = &mut self.slots[0x16];
                if !keep {
                    s.pos = 0;
                }
                while (s.pos as usize) + 1 < s.x.len() && s.x[s.pos as usize] > rx {
                    s.pos += 1;
                }
                let p = s.pos as usize;
                if let (Some(&x), Some(&y)) = (s.x.get(p), s.y.get(p)) {
                    self.rules.set_f(RAT_X, x);
                    self.rules.set_f(RAT_Y, y);
                }
                let jump = self.run_slot_to_jump(io, 0x16).await;
                self.jump(io, jump)
            }
            // 0x00437e3c
            T0025 => {
                self.run_slot(io, 0x18).await;
                self.rules.result = LOST;
                None
            }
            // 0x00438010
            T0022 => {
                self.slots[0x15].pos = 0;
                self.rules.set_rb(26, ABOVE, 1);
                self.rules.set_rb(23, ABOVE, 1);
                let jump = self.run_slot_to_jump(io, 0x15).await;
                self.jump(io, jump)
            }
            T0099 => {
                self.t0099(io).await;
                None
            }
        }
    }

    /// The end of a trajectory into the void: lost, unless the run ended.
    fn lost(&mut self) -> Option<Path> {
        if self.rules.result == 0 {
            self.rules.result = LOST;
        }
        None
    }

    /// `next`, unless the run ended.
    fn then(&self, io: &impl RatIo, next: Path) -> Option<Path> {
        (!self.result_set(io)).then_some(next)
    }

    /// The end of a trajectory to the jump: the jump, else lost.
    fn jump(&mut self, io: &impl RatIo, jump: bool) -> Option<Path> {
        if self.result_set(io) {
            return None;
        }
        if jump {
            return Some(Path::T0099);
        }
        self.rules.result = LOST;
        None
    }

    /// 0x004356bc.
    async fn t0004(&mut self, io: &mut impl RatIo) -> Option<Path> {
        // The rat waits at the end of T0002 + (10, 18) until record 3
        // opens.
        let s1 = &self.slots[1];
        let (x, y) = (
            s1.x.last().copied().unwrap_or(0.0),
            s1.y.last().copied().unwrap_or(0.0),
        );
        self.rules.set_f(RAT_X, x + 10.0);
        self.rules.set_f(RAT_Y, y + 18.0);
        self.rules.set_rb(21, PRESSED, 1);
        loop {
            self.frame(io).await;
            if self.rules.rb(3, PRESSED) != 0 || self.result_set(io) {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        self.rules.set_rb(3, ABOVE, 1);
        self.rules.set_rb(2, ACTIVE, 0);
        self.slots[0x1c].pos = 0;
        let f = self.rules.rs(12, PRESSED_FRAME);
        self.rules.set_rs(12, FRAME, f);
        loop {
            self.frame(io).await;
            let more = self.step_slot(0x1c);
            if self.slots[0x1c].pos == 0x1a {
                let f = self.rules.rs(12, REST);
                self.rules.set_rs(12, FRAME, f);
            }
            if self.result_set(io) || !more {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        self.rules.set_rb(2, ACTIVE, 0);
        self.run_slot(io, 3).await;
        if self.result_set(io) {
            return None;
        }
        self.rules.set_rb(2, ACTIVE, 1);
        Some(if self.rules.rb(4, PRESSED) == 0 {
            Path::T0005
        } else {
            Path::T0009
        })
    }

    /// 0x00435d24: the wheel held; the rat's height follows the wheel.
    async fn wheel_hold(&mut self, io: &mut impl RatIo) {
        self.wheel_y = 0;
        let base = 0xfd;
        loop {
            self.frame(io).await;
            let f = (self.rules.rs(20, FRAME) - 0xba) as u16;
            if (1..=9).contains(&f) {
                self.rules.set_f(RAT_Y, (base + i32::from(f) - 1) as f32);
            }
            self.wheel_y = chop(f64::from(self.rules.f(RAT_Y)));
            // The original has no other way out; the port's quit is one.
            if self.rules.rb(9, PRESSED) != 1 || io.aborted() {
                break;
            }
        }
    }

    /// 0x00435f54: the rat in the wheel until the wheel is turned; the
    /// height reached picks the way out.
    async fn wheel(&mut self, io: &mut impl RatIo) -> Option<Path> {
        let mut held = false;
        self.rules.set_b(RAT_BYTE, 0);
        self.rules.set_i(RAT_X, 0x43b0_0000); // 352.0
        self.rules.set_i(RAT_Y, 0x437d_0000); // 253.0
        loop {
            self.frame(io).await;
            self.rules.set_i(RAT_X, 0x43b0_0000);
            self.rules.set_i(RAT_Y, 0x437d_0000);
            if self.rules.rb(9, PRESSED) == 1 {
                self.wheel_hold(io).await;
                held = true;
            }
            if self.result_set(io) || held {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        io.play_sound(13);
        Some(if self.wheel_y < 0x100 {
            io.play_sound(10);
            Path::T0006
        } else if self.wheel_y < 0x105 {
            io.play_sound(11);
            Path::T0007
        } else {
            io.play_sound(12);
            Path::T0008
        })
    }

    /// 0x00436240.
    async fn t0007(&mut self, io: &mut impl RatIo) -> Option<Path> {
        self.slots[6].pos = 0;
        let mut bumped = false;
        loop {
            self.frame(io).await;
            if i32::from(self.slots[6].pos) > self.slots[6].len() - 0x23 && !bumped {
                bumped = true;
                let f = self.rules.rs(13, FRAME) + 1;
                self.rules.set_rs(13, FRAME, f);
            }
            let more = self.step_slot(6);
            if self.result_set(io) || !more {
                break;
            }
        }
        let f = self.rules.rs(13, FRAME) - 1;
        self.rules.set_rs(13, FRAME, f);
        if self.result_set(io) {
            return None;
        }
        Some(if self.rules.rb(8, PRESSED) == 1 {
            Path::T0007A
        } else {
            Path::T0007B
        })
    }

    /// 0x00436770: the cart, moved by record 19's direction.
    async fn ride(&mut self, io: &mut impl RatIo) -> Option<Path> {
        let s = &self.slots[9];
        let (x, y) = (
            s.x.last().copied().unwrap_or(0.0),
            s.y.last().copied().unwrap_or(0.0),
        );
        self.rules.set_f(RAT_X, x);
        self.rules.set_f(RAT_Y, y + 12.0);
        self.rules.set_b(RAT_BYTE, 0);
        let mut state = 1;
        loop {
            let x = f64::from(self.rules.f(RAT_X));
            if self.rules.b(0x0049_6b7f) == 2 {
                self.rules.set_f(RAT_X, (1.0 + x) as f32);
            } else if self.rules.b(0x0049_6b7f) == 1 {
                self.rules.set_f(RAT_X, (x - 1.0) as f32);
            }
            self.frame(io).await;
            // The original compares the float's bits as an int.
            if self.rules.i(RAT_X) < 0x43d7_8000 {
                state = 2;
            } else if self.rules.i(RAT_X) > 0x4401_c000 {
                state = 3;
            }
            if self.result_set(io) || state != 1 {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        if state == 2 {
            return Some(Path::T0013);
        }
        self.rules.set_i(0x0049_6cc0, 1);
        match self.rules.b(0x0049_6b68) {
            1 => Some(Path::T0010),
            2 => Some(Path::T0011),
            3 => Some(Path::T0012),
            _ => None,
        }
    }

    /// 0x00437384.
    async fn t0020(&mut self, io: &mut impl RatIo) -> Option<Path> {
        self.rules.set_f(RAT_X, -100.0);
        self.rules.set_f(RAT_Y, -100.0);
        let mut n = 0;
        loop {
            self.frame(io).await;
            n += 1;
            if self.result_set(io) || n >= 0x32 {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        self.slots[0x13].pos = 0;
        if self.rules.rb(16, PRESSED) == 1 {
            self.rules.set_rb(16, ACTIVE, 0);
            self.rules.set_rb(5, ACTIVE, 0);
        }
        loop {
            self.frame(io).await;
            let more = self.step_slot(0x13);
            if self.result_set(io) || !more {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        self.rules.set_rb(16, ACTIVE, 1);
        self.rules.set_rb(5, ACTIVE, 1);
        Some(if self.rules.rb(16, PRESSED) == 1 {
            Path::T00025
        } else {
            Path::T0021
        })
    }

    /// 0x00437898.
    async fn t0021(&mut self, io: &mut impl RatIo) -> Option<Path> {
        self.slots[0x14].pos = 0;
        let mut to_t0023 = false;
        loop {
            self.frame(io).await;
            let mut more = self.step_slot(0x14);
            if self.rules.rb(16, PRESSED) == 1 {
                to_t0023 = true;
                more = false;
            }
            if self.result_set(io) || !more || self.rules.rb(16, PRESSED) != 0 {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        if to_t0023 {
            return Some(Path::T0023);
        }
        self.slots[0x17].pos = 0;
        loop {
            self.frame(io).await;
            let more = self.step_slot(0x17);
            if self.slots[0x17].pos == 0x1a {
                io.play_sound(15);
            }
            if self.result_set(io) || !more || self.rules.rb(16, PRESSED) != 0 {
                break;
            }
        }
        if self.result_set(io) {
            return None;
        }
        Some(if self.rules.rb(16, PRESSED) == 0 {
            Path::T0025
        } else {
            Path::T0022
        })
    }

    /// The jump, shared by 0x00437608, 0x00437b38 and 0x00438010.
    async fn t0099(&mut self, io: &mut impl RatIo) {
        self.slots[0x19].pos = 0;
        loop {
            self.frame(io).await;
            let more = self.step_slot(0x19);
            if self.slots[0x19].pos == 0x1a {
                io.play_sound(15);
            }
            if self.result_set(io) || !more {
                break;
            }
        }
        // Even when the clock or an edge click set the result.
        self.rules.result = WIN;
    }
}

struct Host<'a> {
    host: PuzzleHost<'a>,
    /// PZFRAT.SPR (0x007bead4); PZFPIECE.SPR is the screen's bank.
    rat: Vec<SprSprite>,
    /// The held flag 0x00496114.
    held: bool,
    /// The left button's WM_LBUTTONDOWN (true) / WM_LBUTTONUP (false) not
    /// dispatched yet.
    button_messages: VecDeque<bool>,
    mx: i32,
    my: i32,
    /// The end of the pass's busy wait, in ticks.
    deadline: u32,
}

impl Host<'_> {
    fn now(&self) -> u32 {
        (self.host.e.sys.millis() / 5) as u32
    }
}

impl RatIo for Host<'_> {
    /// `FUN_0041a298`'s sampling. The click flag 0x0049611c is the left
    /// button's press edge (`FUN_0042e624`); on that edge `FUN_0041a298`
    /// sets it and returns before its message pump (0x0041a2b5..0x0041a2e4).
    /// The held flag 0x00496114 is written only by the window procedure on
    /// WM_LBUTTONDOWN / WM_LBUTTONUP (0x00419c47 / 0x00419c6a), and the pump
    /// of the active window dispatches one message per call (PeekMessage
    /// without removal, GetMessage, DispatchMessage, 0x0041a483..0x0041a4ce).
    /// So the held flag follows the button one pass after the click flag,
    /// and a tap within one pass still gives one held pass. The button
    /// messages are queued in their order and one is dispatched on each
    /// pass without a press edge; the other messages sharing that pump
    /// (mouse moves, ...), which could delay them further, are not
    /// modelled (unknown).
    fn read_input(&mut self, _g: &RatGame) -> (i32, i32, bool, bool) {
        let queue = &mut self.button_messages;
        let m = self
            .host
            .input
            .read_with(&self.host.e.sys, |down| queue.push_back(down));
        if !m.click {
            if let Some(down) = self.button_messages.pop_front() {
                self.held = down;
            }
        }
        (self.mx, self.my) = (m.x, m.y);
        (m.x, m.y, m.click, self.held)
    }
    fn draw_background(&mut self, which: i32) {
        // Copy 1 goes to a second buffer and copy 2 is the frame's copy to
        // the screen, which present makes.
        if which == 0 {
            self.host.draw_background();
        }
    }
    fn draw_sprite(&mut self, bank: char, x: i32, y: i32, frame: i32) {
        if bank != 'R' {
            self.host.draw_sprite(bank, x, y, frame);
        } else if let Some(spr) = usize::try_from(frame).ok().and_then(|f| self.rat.get(f)) {
            spr.draw(&mut self.host.screen.working, SCREEN_W, SCREEN_H, x, y);
        }
    }
    /// `FUN_00438ee4`: CURSEURS frame 11 outside x 10..600 (where a click
    /// leaves), else the standard cursor; then the frame, shown until the
    /// pass's wait is over.
    async fn present(&mut self, _g: &RatGame) {
        let (mx, my) = (self.mx, self.my);
        if !(10..=600).contains(&mx) {
            self.host.draw_cursor(11, mx, my);
        } else {
            self.host.draw_standard_cursor(mx, my);
        }
        loop {
            self.host.present().await;
            if self.now() >= self.deadline || self.host.aborted() {
                break;
            }
        }
    }
    fn play_sound(&mut self, slot: i32) {
        if let Some(name) = usize::try_from(slot).ok().and_then(|i| SOUNDS.get(i)) {
            self.host.play_sound(name, SamplePlayer::Cd60);
        }
    }
    /// `FUN_00438fd4` @ 0x00439418.
    fn random_value(&mut self) -> i32 {
        self.host.e.puzzles.crt_rand()
    }
    /// The busy wait is made by the present: until then the timer reads
    /// as if it were over.
    fn ticks(&mut self) -> u32 {
        self.now().max(self.deadline)
    }
    fn wait_until(&mut self, t: u32) {
        self.deadline = t;
    }
    fn aborted(&self) -> bool {
        self.host.aborted()
    }
}

/// StartPuzzle 4: 255 when the rat jumps, 2 when the rats or the clock
/// run out, 1 when the player leaves, a file is missing or the game is
/// quit.
pub async fn play(e: &mut Engine, hooks: &mut dyn ScriptHooks) -> i32 {
    let bg = load_tga(e, "PZFBACK.TGA").await;
    let pieces = load_spr(e, "PZFPIECE.SPR").await;
    let rat = load_spr(e, "PZFRAT.SPR").await;
    let mut slots = Vec::with_capacity(NUM_SLOTS);
    for name in SLOT_FILES {
        match read_puzzle_file(e, name)
            .await
            .and_then(|d| Slot::parse(&d))
        {
            Some(s) => slots.push(s),
            None => break,
        }
    }
    let (Some(bg), Some(pieces), Some(rat)) = (bg, pieces, rat) else {
        log::warn!("rat: missing files");
        return 1;
    };
    if slots.len() != NUM_SLOTS {
        log::warn!("rat: missing trajectories");
        return 1;
    }
    let sounds = PuzzleSounds::load(e, &SOUNDS).await;
    let rules = e.puzzles.rat.take().unwrap_or_default();
    let mut game = RatGame::new(rules, slots);
    game.deadline_pacing = e.game.exe.build.rat_deadline_pacing;
    game.keep_search_position = !e.game.exe.build.rat_search_restart;
    let host = PuzzleHost::new(e, hooks);
    let held = host.e.sys.buttons().0;
    let mut host = Host {
        host,
        rat,
        held,
        button_messages: VecDeque::new(),
        mx: 0,
        my: 0,
        deadline: 0,
    };
    host.host.screen.pictures = vec![bg];
    host.host.screen.frames = pieces;
    host.host.sounds = sounds;
    let r = game.run(&mut host).await;
    let e = host.host.e;
    // The static data as the run left it.
    e.puzzles.rat = Some(game.rules);
    if e.should_abort() || (r != 255 && r != 2) {
        if r == 0xa {
            2
        } else {
            1
        }
    } else {
        r
    }
}
