//! What ends a video in `atlantis.exe`.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`video_skip.h`;
//! GPLv3, the ScummVM team).
//!
//! The frame loop `FUN_0041bc74` (every video: a move's transition, a CON
//! Cinema=, the start-up logos) goes on while 0x00496124 != 0 and the click
//! flag 0x0049611c == 0, and its input poll is `FUN_0041a298`, one per
//! frame:
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
