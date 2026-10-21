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
    ///   0x00429c5e, FUN_0041e830);
    /// - `ß` (0xe1, the German CD's EPI.TXT) in the sprite-font texts is
    ///   drawn and measured as two 's'; off, as the original: glyph 193,
    ///   past the fonts' 137 sprites, is not drawn and the pen moves by the
    ///   advance table's byte (7 in atlantis-cd-de-687104, FUN_00410430);
    /// - with the Dutch CD, CREDITS.TXT and the FONTMIN fonts are disc 1's
    ///   newest revision whatever the disc; off, the current disc's copy, as
    ///   the original reads them from the disc in the drive: disc 2 holds an
    ///   older revision (CREDITS.TXT one line shorter, FONTMIN's `j` and
    ///   `'` hotspots);
    /// - with the Italian CD, a positioned menu line (`<x,y>`) answers over
    ///   22 px per byte as in every other build, so the whole drawn word
    ///   highlights and takes the click; off, 16 px per byte as the original
    ///   (atlantis-cd-it-687104 FUN_0041ecb4 @ 0x0041f068): the right ends
    ///   of `OK` (11 px) and `Annulla` (18 px) do not answer;
    /// - with the Simplified Chinese CD, the subtitle row after an
    ///   ideographic space starts after the whole space; off, on its second
    ///   byte, as the original (atlantis-cd-zh-cn-692736 FUN_0042f424 @
    ///   0x0042f5ef), so that row reads its characters one byte out of step.
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
