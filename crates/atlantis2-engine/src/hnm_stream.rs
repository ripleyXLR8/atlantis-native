//! The dialog body video, `HnmStream` (@ 0x0046b4e0 open, @ 0x0046b750
//! next frame, @ 0x0046b790 draw, @ 0x0046b840 frame number): an HNM6
//! video of `datas_a2\hnm\dial\` looped while a line plays, drawn into the
//! back buffer through its `AL` mask.
//!
//! The stream shares the cinematic player's state (@ 0x00616800..): the
//! open (@ 0x0040b7a0) reads the header, then decodes two frames, so the
//! frame shown first is frame 1 and the frame number is 1; each next frame
//! adds 1; once the last frame is decoded (the end test @ 0x0040bd90:
//! decoded ≥ the header's count) the next call rewinds (@ 0x0040bef0:
//! frame 0 decoded again), decodes frame 1 and sets the number back to 1.
//! A 41-frame video thus shows frames 1..40 in a loop, and the head is
//! posed at the same numbers.
//!
//! The end test also ends a video when the first dword of the player's
//! read buffer is 0 (@ 0x0040bdda), a buffer this build never writes
//! (`GlobalAlloc` without zeroing): the cinematics play, so it is not 0 in
//! the game and the frame count decides, as here.

use std::sync::Arc;

use atlantis_codecs::{AlphaTables, DecodeError, HnmVideo};

/// The dialog's video stream.
pub struct HnmStream {
    path: String,
    data: Arc<[u8]>,
    video: HnmVideo<Arc<[u8]>>,
    /// The frame number (+0x14): −1 before the first frame.
    number: i32,
    /// The end flag of the player (@ 0x005da188).
    ended: bool,
    tables: AlphaTables,
}

impl HnmStream {
    /// `HnmStream::Open` (@ 0x0046b4e0): opens `data` (the file `path`),
    /// builds the RGB565 blend tables and decodes two frames.
    pub fn open(path: &str, data: Arc<[u8]>) -> Result<HnmStream, DecodeError> {
        let video = HnmVideo::new(data.clone())?;
        let mut s = HnmStream {
            path: path.to_string(),
            data,
            video,
            number: -1,
            ended: false,
            tables: AlphaTables::new(false),
        };
        s.next_frame()?;
        s.next_frame()?;
        if s.video.alpha().is_none() {
            // `ATTENTION: ouverture d'un HNM sans ALPHA`
            log::info!("{path}: no AL mask, drawn opaque");
        }
        Ok(s)
    }

    /// The file name the stream was opened with.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// One frame of the player (@ 0x0040c140), then the end test.
    fn decode(&mut self) -> Result<(), DecodeError> {
        match self.video.next_frame() {
            Some(Err(e)) => return Err(e),
            Some(Ok(_)) => {}
            None => self.ended = true,
        }
        let count = self.video.frame_count();
        if self.video.frames_decoded() >= count {
            self.ended = true;
        }
        Ok(())
    }

    /// `FUN_0046b750`: the next frame; at the end, back to frame 1.
    pub fn next_frame(&mut self) -> Result<(), DecodeError> {
        if !self.ended {
            self.decode()?;
            self.number += 1;
            return Ok(());
        }
        // @ 0x0040bef0: seek to the first superchunk, decode it into the
        // same surfaces, then one more frame.
        let old = std::mem::replace(&mut self.video, HnmVideo::new(self.data.clone())?);
        self.video = HnmVideo::with_buffers(self.data.clone(), old.into_buffers())?;
        self.ended = false;
        self.decode()?;
        self.ended = false;
        self.decode()?;
        self.number = 1;
        Ok(())
    }

    /// `FUN_0046b840`: the frame number, 0 while it is negative.
    pub fn number(&self) -> i32 {
        self.number.max(0)
    }

    /// The HD mode (a port addition): which pixels of a `pitch`-wide
    /// picture [`HnmStream::draw`] draws the character over, 1 where the
    /// `AL` mask's alpha is not 0 (every pixel of the frame without a
    /// mask), else 0, into `out`.
    pub fn coverage(&self, out: &mut [u8], pitch: usize) {
        out.fill(0);
        let (w, h) = (self.video.width(), self.video.height());
        if w == 0 || pitch < w {
            return;
        }
        let rows = h.min(out.len() / pitch);
        let Some(runs) = self.video.alpha() else {
            for y in 0..rows {
                out[y * pitch..y * pitch + w].fill(1);
            }
            return;
        };
        let mut p = 0usize;
        for pair in runs.as_chunks::<2>().0 {
            let (n, a) = (usize::from(pair[0]) + 1, pair[1]);
            if a != 0 {
                for q in p..(p + n).min(w * rows) {
                    out[(q / w) * pitch + q % w] = 1;
                }
            }
            p += n;
            if p >= w * h {
                break;
            }
        }
    }

    /// Whether the frame shown has an `AL` mask.
    pub fn has_alpha(&self) -> bool {
        self.video.alpha().is_some()
    }

    /// The frame shown (640 x 480 RGB565).
    pub fn frame(&self) -> &[u16] {
        self.video.frame()
    }

    /// `FUN_0046b790`: draws the frame into `dst` (rows of `pitch` pixels):
    /// through its `AL` mask (@ 0x0046b9b0), or opaque without one (@
    /// 0x004b6cc0, the overlapping rectangle from (0, 0)).
    pub fn draw(&self, dst: &mut [u16], pitch: usize, height: usize) {
        let (w, h) = (self.video.width(), self.video.height());
        match self.video.alpha() {
            Some(al) => {
                if let Err(e) = self.tables.blend(self.video.frame(), dst, al, w, h, pitch) {
                    log::warn!("{}: {e}", self.path);
                }
            }
            None => {
                let frame = self.video.frame();
                for y in 0..h.min(height) {
                    let n = w.min(pitch);
                    dst[y * pitch..y * pitch + n].copy_from_slice(&frame[y * w..y * w + n]);
                }
            }
        }
    }
}
