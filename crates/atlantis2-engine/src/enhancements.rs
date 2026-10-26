//! Where the port may differ from the original game, by choice: the
//! groups of the first game's `Enhancements` (those of ScummVM's CryOmni3D
//! Atlantis engine), and the first game's HD mode. The original game, bug
//! for bug, is every group off ([`Enhancements::original`], `--original`).

use crate::hd::HdOptions;

/// The optional differences from the original game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enhancements {
    /// Fixes of minor bugs (default on):
    /// - every music track is heard to its end: the next track of the
    ///   `startmusik` list starts right after its last sample; off, as the
    ///   original (`check_buffer_music` @ 0x00477590, `AtlMuzik_GotoMark`
    ///   @ 0x00477e80), the next track is written behind the play cursor
    ///   when the decoder, about 5.8 s ahead, reaches the end, and the
    ///   last ~5.8 s of the old one are never heard;
    /// - a track that ends with an empty `startmusik` list, or with loops
    ///   left (`*n`), plays again from its start; off, as the original, the
    ///   rewind's file check compares the PCM size with the file's
    ///   (@ 0x00477b00), fails and closes the file, and the ring loops its
    ///   last 5.9 s until another music command;
    /// - in Ireland's two puzzle views (Brigid's cross, view 2; the ogham
    ///   stones, view 6) a click on a puzzle zone runs its puzzle and
    ///   nothing else; off, as the original (FUN_0049ca80 @ 0x0049cb9b),
    ///   it also sets `CLICZONE` to the zone's plain value, which the
    ///   scripts read as the index of a named zone: in chapter 12 the
    ///   value-4 zone answers for the weeping head (with an empty hand,
    ///   back to Tibet), in chapter 14 the value-9 zone with the horse
    ///   plays the water's film, and so on
    ///   ([`crate::engine::irish_plain_value_zone`]).
    pub minor_bug_fixes: bool,
    /// Interface conveniences the original lacks (default on; the first
    /// game's `ui_ux` group):
    /// - the arrow keys turn the view (each held key stands for a mouse
    ///   move of 8 counts a frame); off, as the original, whose key-down
    ///   callback (atlantis2-cd-fr-2199552 @ 0x004762f0) knows Escape,
    ///   Space and Alt+Enter only, the view turns with the mouse alone.
    pub ui_ux: bool,
    /// The HD mode (default off; the first game's `graphics = hd`,
    /// `--hd`): the panorama drawn at the window's resolution with the
    /// original's projection (see [`crate::hd`]); the engine's 640x480
    /// frame stays the original's either way.
    pub hd: bool,
    /// The HD mode's picture options, the first game's
    /// (`hd_panorama`, `hd_sharpen`, `hd_interface`,
    /// `hd_sharp_subtitles`, `hd_widescreen`, `hd_video_sides`); used
    /// only with [`Enhancements::hd`].
    pub hd_options: HdOptions,
}

impl Default for Enhancements {
    fn default() -> Self {
        Enhancements {
            minor_bug_fixes: true,
            ui_ux: true,
            hd: false,
            hd_options: HdOptions::default(),
        }
    }
}

impl Enhancements {
    /// The original game: every group off (and the original's 640x480
    /// picture).
    pub fn original() -> Self {
        Enhancements {
            minor_bug_fixes: false,
            ui_ux: false,
            hd: false,
            hd_options: HdOptions::default(),
        }
    }
}
