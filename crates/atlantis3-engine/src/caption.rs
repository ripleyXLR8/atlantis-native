//! The caption of a cinematic (`loc\text\<lang>\CineNNN.txt`), faded in
//! and out against the film's frame number by the CryoMovie wait loop
//! (`AtlHnm_continue` @ 0x00421c7f..0x00421df6), once per new frame.
//!
//! What this module does not do yet is draw it: the original prints the
//! text with GDI (`AtlFont_Special`, the text library object @ 0x00755db0)
//! in a box whose geometry is not read yet; the colour and its steps are
//! the original's.

use atlantis2_engine::fpu::ftol;
use atlantis2_engine::Fpu;
use atlantis3_data::cine_txt::Caption;

/// What a frame did to the subtitle line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionOp {
    /// `AtlSubtitle_Set(text, r, g, b, 1)` (@ 0x0040e290).
    Set([u8; 3]),
    /// `AtlSubtitle_Clear` (@ 0x0040e330).
    Clear,
}

/// A caption and its opacity (@ 0x00756458, a float).
#[derive(Clone, Debug)]
pub struct CaptionFade {
    pub caption: Caption,
    pub opacity: f32,
}

/// The floats of the block: 0.02 (@ 0x007003f4), 0.05 (@ 0x007003e4),
/// 255, 230, 200 (@ 0x007003f0, 0x007003ec, 0x007003e8).
const DOWN: f32 = 0.02;
const UP: f32 = 0.05;
const RGB: [f32; 3] = [255.0, 230.0, 200.0];

impl CaptionFade {
    pub fn new(caption: Caption) -> CaptionFade {
        CaptionFade {
            caption,
            opacity: 0.0,
        }
    }

    fn colour(&self, fpu: Fpu) -> [u8; 3] {
        RGB.map(|k| ftol(fpu.mul(f64::from(self.opacity), f64::from(k))) as u8)
    }

    /// The step of frame `frame`, under the control word `fpu`: after the
    /// fade-out frame the opacity falls by 0.02 (cleared once at or below
    /// 0), else after the fade-in frame it grows by 0.05 while below 1 (and
    /// is set again at full colour on reaching 1). The colour is
    /// (ftol(o · 255), ftol(o · 230), ftol(o · 200)).
    pub fn frame(&mut self, frame: i32, fpu: Fpu) -> Vec<CaptionOp> {
        let mut ops = Vec::new();
        let o = f64::from(self.opacity);
        if frame > self.caption.fade_out_frame {
            if o > 0.0 {
                let v = fpu.sub(o, f64::from(DOWN));
                self.opacity = fpu.f32(v);
                // `fst` then the compare of the unrounded value.
                if v <= 0.0 {
                    self.opacity = 0.0;
                    ops.push(CaptionOp::Clear);
                } else {
                    ops.push(CaptionOp::Set(self.colour(fpu)));
                }
            }
        } else if frame > self.caption.fade_in_frame && o < 1.0 {
            self.opacity = fpu.f32(fpu.add(o, f64::from(UP)));
            ops.push(CaptionOp::Set(self.colour(fpu)));
            if self.opacity >= 1.0 {
                self.opacity = 1.0;
                ops.push(CaptionOp::Set([255, 230, 200]));
            }
        }
        ops
    }
}
