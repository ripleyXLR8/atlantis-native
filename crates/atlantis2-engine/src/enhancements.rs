//! Where the port may differ from the original game, by choice: the
//! groups of the first game's `Enhancements` (those of ScummVM's CryOmni3D
//! Atlantis engine), and the first game's HD mode. The original game, bug
//! for bug, is every group off ([`Enhancements::original`], `--original`).

use crate::hd::HdOptions;

/// A fix of a bug of the original, each one its own switch
/// ([`BugFixes`]; `settings.ini`'s [`BugFix::key`], the options menu's
/// "Choose the bug fixes...").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BugFix {
    /// Every music track is heard to its end: the next track of the
    /// `startmusik` list starts right after its last sample. Off, as the
    /// original (`check_buffer_music` @ 0x00477590, `AtlMuzik_GotoMark`
    /// @ 0x00477e80), the next track is written behind the play cursor
    /// when the decoder, about 5.8 s ahead, reaches the end, and the last
    /// ~5.8 s of the old one are never heard.
    MusicEnd,
    /// A track that ends with an empty `startmusik` list, or with loops
    /// left (`*n`), plays again from its start. Off, as the original, the
    /// rewind's file check compares the PCM size with the file's
    /// (@ 0x00477b00), fails and closes the file, and the ring loops its
    /// last 5.9 s until another music command.
    MusicRestart,
    /// A random music mark drawn at the `rand()` = 0x7fff edge (one draw
    /// in 32768) is the file's last mark. Off, as the original
    /// (`AtlMuzik_GetRandomMark` @ 0x00478090 gives one past the marks),
    /// `AtlMuzik_GotoMark` reports a fatal error (the original saves in
    /// slot 8 of player 9 and quits; here the music and the scripts stop).
    MusicMark,
    /// In Ireland's two puzzle views (Brigid's cross, view 2; the ogham
    /// stones, view 6) a click on a puzzle zone runs its puzzle and
    /// nothing else. Off, as the original (FUN_0049ca80 @ 0x0049cb9b), it
    /// also sets `CLICZONE` to the zone's plain value, which the scripts
    /// read as the index of a named zone: in chapter 12 the value-4 zone
    /// answers for the weeping head (with an empty hand, back to Tibet),
    /// in chapter 14 the value-9 zone with the horse plays the water's
    /// film, and so on ([`crate::engine::irish_plain_value_zone`]).
    IrelandClicks,
}

impl BugFix {
    /// Every fix, in the menu's order.
    pub const ALL: [BugFix; 4] = [
        BugFix::MusicEnd,
        BugFix::MusicRestart,
        BugFix::MusicMark,
        BugFix::IrelandClicks,
    ];

    /// Its `settings.ini` key.
    pub fn key(self) -> &'static str {
        match self {
            BugFix::MusicEnd => "fix_music_end",
            BugFix::MusicRestart => "fix_music_restart",
            BugFix::MusicMark => "fix_music_mark",
            BugFix::IrelandClicks => "fix_ireland_clicks",
        }
    }
}

/// The bug fixes, each on or off (default all on;
/// [`BugFixes::NONE`] is the original, bug for bug).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BugFixes {
    /// [`BugFix::MusicEnd`].
    pub music_end: bool,
    /// [`BugFix::MusicRestart`].
    pub music_restart: bool,
    /// [`BugFix::MusicMark`].
    pub music_mark: bool,
    /// [`BugFix::IrelandClicks`].
    pub ireland_clicks: bool,
}

impl BugFixes {
    /// Every fix on.
    pub const ALL: BugFixes = BugFixes::all(true);
    /// Every fix off: the original.
    pub const NONE: BugFixes = BugFixes::all(false);

    /// Every fix `on`.
    pub const fn all(on: bool) -> BugFixes {
        BugFixes {
            music_end: on,
            music_restart: on,
            music_mark: on,
            ireland_clicks: on,
        }
    }

    /// Fix `f` alone.
    pub fn only(f: BugFix) -> BugFixes {
        let mut x = BugFixes::NONE;
        x.set(f, true);
        x
    }

    pub fn get(&self, f: BugFix) -> bool {
        match f {
            BugFix::MusicEnd => self.music_end,
            BugFix::MusicRestart => self.music_restart,
            BugFix::MusicMark => self.music_mark,
            BugFix::IrelandClicks => self.ireland_clicks,
        }
    }

    pub fn set(&mut self, f: BugFix, on: bool) {
        *match f {
            BugFix::MusicEnd => &mut self.music_end,
            BugFix::MusicRestart => &mut self.music_restart,
            BugFix::MusicMark => &mut self.music_mark,
            BugFix::IrelandClicks => &mut self.ireland_clicks,
        } = on;
    }

    /// How many fixes are on.
    pub fn count(&self) -> usize {
        BugFix::ALL.iter().filter(|&&f| self.get(f)).count()
    }

    /// The music player's switches ([`crate::music::Stream`]).
    pub fn music(&self) -> crate::music::MusicFixes {
        crate::music::MusicFixes {
            end: self.music_end,
            restart: self.music_restart,
            mark: self.music_mark,
        }
    }
}

impl Default for BugFixes {
    fn default() -> Self {
        BugFixes::ALL
    }
}

/// The optional differences from the original game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enhancements {
    /// Fixes of minor bugs, each its own switch (default all on; see
    /// [`BugFix`]).
    pub fixes: BugFixes,
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
            fixes: BugFixes::ALL,
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
            fixes: BugFixes::NONE,
            ui_ux: false,
            hd: false,
            hd_options: HdOptions::default(),
        }
    }
}

impl Enhancements {
    /// `self` (the stored choices, `settings.ini`'s) with what a change
    /// from `before` to `after` (the options menu's) changed, every other
    /// switch kept as stored: a run's `--original`, `--hd` or
    /// `--widescreen`, which hold for that run only, are not written with
    /// an unrelated change.
    pub fn with_changes(&self, before: &Enhancements, after: &Enhancements) -> Enhancements {
        fn pick<T: PartialEq + Copy>(stored: T, before: T, after: T) -> T {
            if before != after {
                after
            } else {
                stored
            }
        }
        let mut x = *self;
        for f in BugFix::ALL {
            let on = pick(self.fixes.get(f), before.fixes.get(f), after.fixes.get(f));
            x.fixes.set(f, on);
        }
        x.ui_ux = pick(self.ui_ux, before.ui_ux, after.ui_ux);
        x.hd = pick(self.hd, before.hd, after.hd);
        let (s, b, a) = (&self.hd_options, &before.hd_options, &after.hd_options);
        x.hd_options = HdOptions {
            panorama: pick(s.panorama, b.panorama, a.panorama),
            sharpen: pick(s.sharpen, b.sharpen, a.sharpen),
            smooth_interface: pick(s.smooth_interface, b.smooth_interface, a.smooth_interface),
            sharp_subtitles: pick(s.sharp_subtitles, b.sharp_subtitles, a.sharp_subtitles),
            widescreen: pick(s.widescreen, b.widescreen, a.widescreen),
            video_sides: pick(s.video_sides, b.video_sides, a.video_sides),
        };
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run's `--original` (every fix off, `ui_ux` off) and `--hd` with
    /// `--widescreen`: a change of one fix, then of the HD mode, in the
    /// menu writes that change only, the stored choices kept.
    #[test]
    fn a_change_keeps_the_stored_choices() {
        let mut stored = Enhancements::default();
        stored.fixes.set(BugFix::MusicMark, false);
        stored.hd_options.sharpen = false;
        let mut run = Enhancements::original();
        run.hd = true;
        run.hd_options.widescreen = true;
        // One fix turned on in the menu.
        let mut after = run;
        after.fixes.set(BugFix::IrelandClicks, true);
        let s = stored.with_changes(&run, &after);
        let mut want = stored;
        want.fixes.set(BugFix::IrelandClicks, true);
        assert_eq!(s, want);
        // The HD mode turned off (the run's widescreen not written).
        let mut off = after;
        off.hd = false;
        let s2 = s.with_changes(&after, &off);
        assert_eq!(s2, want);
        // Every fix off ("All off"): written, the rest kept.
        let mut none = stored;
        none.fixes = BugFixes::NONE;
        let mut all = run;
        all.fixes = BugFixes::ALL;
        assert_eq!(none.with_changes(&run, &all).fixes, BugFixes::ALL);
        assert_eq!(stored.with_changes(&all, &run).fixes, BugFixes::NONE);
        // No change: nothing written.
        assert_eq!(stored.with_changes(&run, &run), stored);
    }
}
