//! The rules of the game-over screen.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`gameover.h`;
//! GPLv3, the ScummVM team), which follows GereGameOver, `FUN_00426590`,
//! called by the section executor `FUN_0042844c` when GAMEOVER is non-zero
//! after a line (0x00428c05), before the end of the walk:
//!  1. NEWSOUND != 0: the NewSound sample fades out (0x00495eb0 = 2, its
//!     volume from the effects volume, 0x00426631..0x00426647).
//!  2. `images\gover` + ("0" when < 10) + GAMEOVER + `.tga`, full screen.
//!  3. When the next line starts with "[Narra": its voice
//!     `wav\ata<section, 4 digits><letter>.apc` and its text, drawn by
//!     `FUN_00430178` when SUBTITLES is set. Then the wait
//!     ([`game_over_wait_ends`]).
//!  4. The screen is cleared; the place loop then reloads the player's
//!     checkpoint (main loop 0x0041975d -> `FUN_0042de5c`).

use crate::video_skip::VideoSkipState;

/// Step 2's image (an `IMAGES\` member).
pub fn game_over_image_name(n: i32) -> String {
    format!("GOVER{n:02}.TGA")
}

/// Step 3's voice (a `WAV\` member).
pub fn game_over_voice_name(section: i32, letter: char) -> String {
    format!("ATA{section:04}{letter}.APC")
}

/// Step 3: the subtitle is drawn when SUBTITLES is set, whatever the click
/// counter 0x00496098 (`FUN_00430178` returns at once otherwise,
/// 0x0043019a).
pub fn game_over_subtitle_drawn(subtitles: bool) -> bool {
    subtitles
}

/// Step 1.
pub fn game_over_fades_new_sound(new_sound: i32) -> bool {
    new_sound != 0
}

/// Step 3's wait, one pass. With the voice (0x00426b0c..0x00426b49): the
/// loop polls the input (`FUN_0041a298`) and ends when the sample is done,
/// a left press set 0x0049611c or Escape cleared 0x00496124 (bow mode, bar
/// request and OBJETENMAIN all 0). Without it (0x00426ae0..0x00426b05):
/// only the left press, or 5000 ms since the text was drawn; the poll comes
/// after the test. True when the wait ends.
pub fn game_over_wait_ends(
    s: &mut VideoSkipState,
    with_voice: bool,
    voice_done: bool,
    elapsed_ms: u64,
    left: bool,
    right: bool,
    escape: bool,
) -> bool {
    if with_voice {
        s.poll(left, right, escape);
        return voice_done || s.click || !s.keep_running;
    }
    if s.click || elapsed_ms >= 5000 {
        return true;
    }
    s.poll(left, right, escape);
    false
}
