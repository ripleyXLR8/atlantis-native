//! Where the port may differ from the original game, by choice: the
//! groups of the first game's `Enhancements` (those of ScummVM's CryOmni3D
//! Atlantis engine). The original game, bug for bug, is every group off
//! ([`Enhancements::original`], `--original`).

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
    ///   last 5.9 s until another music command.
    pub minor_bug_fixes: bool,
}

impl Default for Enhancements {
    fn default() -> Self {
        Enhancements {
            minor_bug_fixes: true,
        }
    }
}

impl Enhancements {
    /// The original game: every group off.
    pub fn original() -> Self {
        Enhancements {
            minor_bug_fixes: false,
        }
    }
}
