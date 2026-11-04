//! The texts the flip draws over every frame: the subtitle line and the
//! three `PLAYVOICE` slots (@ 0x0040de60).
//!
//! The flip (@ 0x0040fdf0) is the end of every mode's frame: the warp
//! (@ 0x004400e5, after the frame hook and the click scan, not when the
//! frame started a cinematic or a dialog; @ 0x0043ffaa / 0x0044001b /
//! 0x00440067 when a script wait ends), the inventory and the subject menu
//! (@ 0x00427cc6 / 0x00427ed1), the loop-mode screens, the menu pages, a
//! dialog (after its line, @ 0x0040ffeb) and a video's frame. Each time,
//! after the frame is composed (so over the warp, the inventory bar, the
//! subject menu, the pointer), it runs @ 0x0040de60 on the back buffer:
//!
//! - `SubTitleOn` off: nothing, not even the slots' release;
//! - the subtitle line not empty: a caption (`AtlFont_Special`, centred) or
//!   a cue line / a puzzle's text (`AtlFont_Default`, black shadow at
//!   (−1, +1)); the voice slots are neither drawn nor released (their
//!   voices go on);
//! - else each slot k (0, 1, 2) in order: busy and its voice playing → its
//!   text with the shadow, bottom margin 5 + 15 k (a slot keeps its line:
//!   a two-line text overlaps the slot above it); busy and stopped → freed.

use super::sfx::VoiceSlot;
use super::Game;
use crate::text::{FontId, Zone, CAPTION, CUE_SHADOW, CUE_TEXT};

/// One `AtlFont_SetLayout` + `AtlFont_Print` of the flip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPrint {
    pub font: FontId,
    pub zone: Zone,
    pub text: Vec<u8>,
    /// 0xRRGGBB.
    pub rgb: u32,
}

/// The state of a voice slot (tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSlotState {
    pub busy: bool,
    pub text: Vec<u8>,
    pub name: String,
    pub rgb: [i32; 3],
}

fn rgb_of(r: i32, g: i32, b: i32) -> u32 {
    // `r & 0xff | g << 8 | b << 16` as a COLORREF (@ 0x0041e2c0), read
    // back as 0xRRGGBB.
    ((r as u32 & 0xff) << 16) | ((g as u32 & 0xff) << 8) | (b as u32 & 0xff)
}

impl Game {
    /// @ 0x0040de60 without the drawing: what it prints, in order, and the
    /// voice slots it frees.
    pub(crate) fn flip_plan(&mut self) -> Vec<TextPrint> {
        let mut out = Vec::new();
        if !self.startup.config.subtitle_on {
            return out;
        }
        if let Some(sub) = self.subtitle.as_ref().filter(|s| !s.text.is_empty()) {
            let [r, g, b] = sub.rgb.map(i32::from);
            let rgb = rgb_of(r, g, b);
            if sub.caption {
                out.push(TextPrint {
                    font: FontId::Special,
                    zone: CAPTION,
                    text: sub.text.clone(),
                    rgb,
                });
            } else {
                out.push(TextPrint {
                    font: FontId::Default,
                    zone: CUE_SHADOW,
                    text: sub.text.clone(),
                    rgb: 0,
                });
                out.push(TextPrint {
                    font: FontId::Default,
                    zone: CUE_TEXT,
                    text: sub.text.clone(),
                    rgb,
                });
            }
            return out;
        }
        for k in 0..self.csounds.voices.len() {
            if !self.csounds.voices[k].busy {
                continue;
            }
            let name = self.csounds.voices[k].name.clone();
            if !self.sfx_playing(&name) {
                self.csounds.voices[k] = VoiceSlot::default();
                continue;
            }
            let v = &self.csounds.voices[k];
            let m = 5 + 15 * k as i32;
            out.push(TextPrint {
                font: FontId::Default,
                zone: Zone::new(9, 11, 36, m - 1, 0, 1),
                text: v.text.clone(),
                rgb: 0,
            });
            out.push(TextPrint {
                font: FontId::Default,
                zone: Zone::new(10, 10, 35, m, 0, 1),
                text: v.text.clone(),
                rgb: rgb_of(v.rgb[0], v.rgb[1], v.rgb[2]),
            });
        }
        out
    }

    /// The flip's texts (@ 0x0040de60) drawn on the screen.
    pub(crate) fn flip_texts(&mut self) {
        let plan = self.flip_plan();
        if plan.is_empty() {
            return;
        }
        let (w, h) = (self.width, self.height);
        self.texts();
        let mut screen = std::mem::take(&mut self.screen);
        let t = self.texts.as_ref().unwrap();
        for p in &plan {
            t.print(&mut screen, w, h, p.font, &p.text, p.zone, p.rgb);
        }
        self.screen = screen;
    }

    /// Tests: @ 0x0040de60's plan at a flip (the slots it frees freed).
    #[doc(hidden)]
    pub fn test_flip_plan(&mut self) -> Vec<TextPrint> {
        self.flip_plan()
    }

    /// Tests: the voice slots.
    #[doc(hidden)]
    pub fn test_voice_slots(&self) -> Vec<VoiceSlotState> {
        self.csounds
            .voices
            .iter()
            .map(|v| VoiceSlotState {
                busy: v.busy,
                text: v.text.clone(),
                name: v.name.clone(),
                rgb: v.rgb,
            })
            .collect()
    }

    /// Tests: the subtitle line's text (empty when cleared) and colour.
    #[doc(hidden)]
    pub fn test_subtitle(&self) -> Option<(Vec<u8>, [u8; 3], bool)> {
        self.subtitle
            .as_ref()
            .map(|s| (s.text.clone(), s.rgb, s.caption))
    }

    /// Tests: the C code's text calls: `AtlSubtitle_Set(GetTextFromDial(d,
    /// l), r, g, b, 0)` (@ 0x0040e290, the star room's tips), `PLAYVOICE`
    /// (@ 0x0040da70), the clear (@ 0x0040e330), the slots' clear
    /// (@ 0x0040db90), the stop by name (@ 0x0040dcc0), a voice that ends
    /// by itself (its sound stopped, the slot untouched), `SubTitleOn` and
    /// the world whose dialogs name the voices.
    #[doc(hidden)]
    pub fn test_text_op(&mut self, op: &str, args: &[i32], name: &str) {
        match op {
            "sub" => self.c_subtitle_set(
                args[0],
                args[1],
                [args[2] as u8, args[3] as u8, args[4] as u8],
            ),
            // The setter with flag 1 (a cinematic caption's branch of the
            // flip, as the videos' captions set it).
            "caption" => {
                self.c_subtitle_set(
                    args[0],
                    args[1],
                    [args[2] as u8, args[3] as u8, args[4] as u8],
                );
                if let Some(sub) = self.subtitle.as_mut() {
                    sub.caption = true;
                }
            }
            "clear" => self.c_subtitle_clear(),
            "voice" => self.play_voice_c(args[0], args[1], args[2], args[3], args[4]),
            "clearvoices" => self.voices_clear(),
            "stop" => self.voice_stop_named(name),
            "end" => self.sfx_stop(name),
            "subon" => self.startup.config.subtitle_on = args[0] != 0,
            "world" => {
                self.world = args[0];
                self.load_dialogs(args[0]);
            }
            _ => panic!("{op}"),
        }
    }

    /// Tests: the voice file of a dialog's line (`GetApcNameFromDial`).
    #[doc(hidden)]
    pub fn test_voice_name(&self, dialog: i32, line: i32) -> String {
        self.dialog_voice_name(dialog, line)
    }
}
