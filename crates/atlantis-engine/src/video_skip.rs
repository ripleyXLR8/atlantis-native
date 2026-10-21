//! What ends a video in `atlantis.exe`.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`video_skip.h`;
//! GPLv3, the ScummVM team).
//!
//! The frame loop `FUN_0041bc74` (every video: a move's transition, a CON
//! Cinema=, the start-up logos) goes on while 0x00496124 != 0 and the click
//! flag 0x0049611c == 0, and its input poll is `FUN_0041a298`, one after
//! each presented frame but the last ([`VideoSkipRun`]):
//!
//! - a left-button press sets 0x0049611c (and nothing else is read);
//! - a right-button press toggles the bar request 0x004960c4 unless a CON
//!   section plays from a click (0x00496170) or the menu is requested
//!   (0x00496138);
//! - Escape with a talking head (0x00496188) sets 0x00496134 and
//!   0x0049611c; else, with bow mode 0x00496164, the bar request and
//!   OBJETENMAIN all 0, 0x00496124 = 0 and 0x00496138 = 1;
//! - Space, Enter and the other keys are not read.
//!
//! When the poll ends the loop, the next frame is still presented
//! (0x0041be2e).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoSkipState {
    /// 0x00496164.
    pub bow_mode: bool,
    /// 0x004960c4.
    pub bar_request: bool,
    /// OBJETENMAIN != 0.
    pub object_in_hand: bool,
    /// 0x00496188.
    pub talking_head: bool,
    /// 0x00496170.
    pub section_play: bool,
    /// 0x00496138.
    pub menu_request: bool,
    /// 0x0049611c.
    pub click: bool,
    /// 0x00496124.
    pub keep_running: bool,
    /// 0x00496134, the CON executor's dialog-skip flag (`FUN_0042844c` @
    /// 0x00428b6d): set by Escape with a talking head, cleared again by the
    /// video's end (0x0041be5f), so no reader sees the video's value.
    pub skip_flag: bool,
}

impl Default for VideoSkipState {
    fn default() -> Self {
        VideoSkipState {
            bow_mode: false,
            bar_request: false,
            object_in_hand: false,
            talking_head: false,
            section_play: false,
            menu_request: false,
            click: false,
            keep_running: true,
            skip_flag: false,
        }
    }
}

impl VideoSkipState {
    /// One `FUN_0041a298` poll inside `FUN_0041bc74`; true when the video
    /// ends.
    pub fn poll(&mut self, left_press: bool, right_press: bool, escape_press: bool) -> bool {
        if left_press {
            self.click = true;
            return true;
        }
        if right_press && !self.section_play && !self.menu_request {
            self.bar_request = !self.bar_request;
        }
        if escape_press {
            if self.talking_head {
                self.skip_flag = true;
                self.click = true;
            } else if !self.bow_mode && !self.bar_request && !self.object_in_hand {
                self.keep_running = false;
                self.menu_request = true;
            }
        }
        self.click || !self.keep_running
    }

    /// After the loop: whether the sound fades out (cinema mode only).
    pub fn fades(&self, cinema_mode: bool) -> bool {
        cinema_mode && (self.click || self.menu_request)
    }
}

/// The frame control of `FUN_0041bc74`. Each loop pass presents the frame
/// decoded in the pass before (`FUN_0041b064` @ 0x0041bdec) and then polls
/// (`FUN_0041a298` @ 0x0041bdf7); every loop exit presents one more frame,
/// the last decoded one (0x0041be2e), with no poll after it. So each frame
/// is shown once, a poll follows every frame but the last, and after the
/// poll that ends the video one more frame is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoSkipRun {
    pub s: VideoSkipState,
    /// A poll ended the loop.
    pub stop_after_frame: bool,
    pub done: bool,
    /// 0x00496104.
    pub presented: u32,
}

impl VideoSkipRun {
    pub fn new(s: VideoSkipState) -> VideoSkipRun {
        VideoSkipRun {
            s,
            ..VideoSkipRun::default()
        }
    }

    /// After each presented frame (`last_frame`: the video has no frame
    /// after it, `FUN_00448ed8` == 0); true when the loop must break.
    pub fn frame_presented(&mut self, last_frame: bool) -> bool {
        self.presented += 1;
        if self.stop_after_frame || last_frame {
            self.done = true;
        }
        self.done
    }

    /// The poll that follows a presented frame.
    pub fn poll(&mut self, left_press: bool, right_press: bool, escape_press: bool) {
        if self.done || self.stop_after_frame {
            return;
        }
        if self.s.poll(left_press, right_press, escape_press) {
            self.stop_after_frame = true;
        }
    }

    /// The end of `FUN_0041bc74`: the fade decision (0x0041be69..0x0041be9e),
    /// then 0x00496124 = 1, 0x00496134 = 0 (0x0041be55, 0x0041be5f) and the
    /// flag resets of 0x0041bf5b..0x0041bf8d. True when the sound fades.
    pub fn finish(&mut self, cinema_mode: bool, bar: &mut crate::ui::BarSession) -> bool {
        let fade = self.s.fades(cinema_mode);
        self.s.keep_running = true;
        self.s.skip_flag = false;
        self.s.click = false;
        self.s.menu_request = false;
        self.s.bar_request = false;
        bar.flags_reset();
        fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let mut s = VideoSkipState::default();
        assert!(!s.poll(false, false, false));
        assert!(s.poll(true, false, false));
        let mut s = VideoSkipState::default();
        assert!(!s.poll(false, true, false));
        assert!(s.bar_request);
        // Escape with the bar requested does nothing.
        assert!(!s.poll(false, false, true));
        assert!(!s.poll(false, true, false));
        assert!(s.poll(false, false, true));
        assert!(s.menu_request && !s.keep_running);
        let mut s = VideoSkipState {
            talking_head: true,
            ..VideoSkipState::default()
        };
        assert!(s.poll(false, false, true));
        assert!(s.click && !s.menu_request);
    }
}
