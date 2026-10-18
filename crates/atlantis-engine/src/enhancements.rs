//! Where the port may differ from the original game, by choice.
//!
//! The groups mirror the enhancement classes of ScummVM's CryOmni3D
//! Atlantis engine (`kEnhMinorBugFixes`, `kEnhRestoredContent`,
//! `kEnhUIUX`). The original game, bug for bug, is every group off
//! ([`Enhancements::original`]); the defaults are those of the ScummVM
//! port. Fixes of bugs that crash or block the original game are not
//! optional and are not listed here.

/// The optional differences from the original game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enhancements {
    /// Fixes of minor bugs (default on):
    /// - the music loops seamlessly; off, it plays through the original's
    ///   streaming ring with its audible seams (`FUN_00432ea8`);
    /// - the dialog SPF overlay (the guards' coif, the pilot's scarf) is
    ///   drawn over the talking head; the original draws it into the
    ///   locked surface, which the back-buffer copy then overwrites
    ///   (`FUN_0042ee88` @ 0x0042f57e), so it is never seen;
    /// - with the French May 1997 build, LOADMUSIK copies its tracks one by
    ///   one whatever the music volume, as the other builds do; off, it
    ///   copies nothing below a current music volume of 5 and all or
    ///   nothing on its first track (atlantis-cd-fr-688640 FUN_00429184 @
    ///   0x00429c5e, FUN_0041e830).
    pub minor_bug_fixes: bool,
    /// Content the original never shows (default off):
    /// - each condition evaluation starts from a clear key buffer, so
    ///   CHAPI063's Anna timer lines can play (the original keeps the
    ///   buffer across a scan, so INVENTnnn reads a longer number);
    /// - with the 2001 build, a second death in the same section plays the
    ///   next narrator line, as the 1999 build does (`FUN_00426590`).
    pub restored_content: bool,
    /// Interface changes (default off):
    /// - the error box of a talking head whose INITDIAL row is missing
    ///   (`FUN_0042ee88` @ 0x0042f036) is not shown; the line is skipped
    ///   either way.
    pub ui_ux: bool,
}

impl Default for Enhancements {
    fn default() -> Self {
        Enhancements {
            minor_bug_fixes: true,
            restored_content: false,
            ui_ux: false,
        }
    }
}

impl Enhancements {
    /// The original game: every group off.
    pub fn original() -> Self {
        Enhancements {
            minor_bug_fixes: false,
            restored_content: false,
            ui_ux: false,
        }
    }
}
