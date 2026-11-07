//! The cinematics and the intro: `AtlHnm_Start` (`Game.exe` @ 0x00420ee0)
//! with its fourth argument 0, the CryoMovie library linked into the
//! executable (open @ 0x006b89d0, play @ 0x006b9390).
//!
//! The picture and the sound are in the same HNM6 file: CryoMovie plays a
//! sound track when header byte +6 is not zero (@ 0x006b8929), its format
//! from the APC header of the first `AA` chunk
//! ([`atlantis_codecs::SoundRule::ApcHeader`]). The sound then drives the
//! frames: a thread (@ 0x006b9160) decodes the next frame each time one of
//! the 32 sound buffers of one frame's samples ends, the first 32 frames'
//! sound being queued at once. A file without sound is paced by a timer of
//! `1000 / fps` milliseconds (@ 0x006b9b11, truncated). Either way the
//! script's frame rate must be 15 or 25 (@ 0x004210e5).

use std::sync::Arc;

use atlantis3_data::spr::Depth;
use atlantis_codecs::{AudioFormat, DecodeError, HnmVideo, SoundRule};

/// A film decoded in the display's format (CryoMovie's play re-initialises
/// its buffers for the surface's: RGB565 or x8r8g8b8, @ 0x006b9619).
enum Video {
    Rgb565(HnmVideo<Arc<[u8]>>),
    Xrgb8888(HnmVideo<Arc<[u8]>, u32>),
}

macro_rules! video {
    ($v:expr, $x:ident => $body:expr) => {
        match $v {
            Video::Rgb565($x) => $body,
            Video::Xrgb8888($x) => $body,
        }
    };
}

/// The last frame of a film, in the display's format.
#[derive(Clone, Copy, Debug)]
pub enum Frame<'a> {
    Rgb565(&'a [u16]),
    Xrgb8888(&'a [u32]),
}

/// Why a cinematic does not play.
#[derive(Debug)]
pub enum CinemaError {
    /// The file is not in the install.
    NotFound(String),
    /// `AtlHnm_start(%s): frame rate incorrect` (@ 0x004210e5): only 15 and
    /// 25 frames per second play.
    FrameRate(u32),
    Decode(String, DecodeError),
}

impl std::fmt::Display for CinemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CinemaError::NotFound(n) => write!(f, "{n}: not found"),
            CinemaError::FrameRate(r) => write!(f, "frame rate incorrect ({r})"),
            CinemaError::Decode(n, e) => write!(f, "{n}: {e:?}"),
        }
    }
}

impl std::error::Error for CinemaError {}

/// A cinematic being played.
pub struct Cinema {
    name: String,
    video: Video,
    /// The script's frame rate (15 or 25).
    pub fps: u32,
    /// The frames shown so far (the subtitles' frame number,
    /// @ 0x00756214, counts them).
    shown: u32,
    ended: bool,
}

impl Cinema {
    /// Opens video `name` (`data`, its file) to play at `fps` frames per
    /// second.
    pub fn open(name: &str, data: Arc<[u8]>, fps: u32) -> Result<Cinema, CinemaError> {
        Cinema::open_depth(name, data, fps, Depth::Rgb565)
    }

    /// [`Cinema::open`] decoding into the display's format `depth`.
    pub fn open_depth(
        name: &str,
        data: Arc<[u8]>,
        fps: u32,
        depth: Depth,
    ) -> Result<Cinema, CinemaError> {
        if fps != 15 && fps != 25 {
            return Err(CinemaError::FrameRate(fps));
        }
        let err = |e| CinemaError::Decode(name.to_string(), e);
        let mut video = match depth {
            Depth::Rgb565 => Video::Rgb565(
                HnmVideo::with_rule(data, Vec::new(), SoundRule::ApcHeader).map_err(err)?,
            ),
            Depth::Xrgb8888 => Video::Xrgb8888(
                HnmVideo::open(data, Vec::new(), SoundRule::ApcHeader).map_err(err)?,
            ),
        };
        // No sound: the timer of `1000 / fps` ms (@ 0x006b90d8).
        video!(&mut video, v => v.set_regular_frame_delay(u64::from(1000 / fps)));
        Ok(Cinema {
            name: name.to_string(),
            video,
            fps,
            shown: 0,
            ended: false,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The picture's size.
    pub fn size(&self) -> (usize, usize) {
        video!(&self.video, v => (v.width(), v.height()))
    }

    /// The sound track's format, if the file has one.
    pub fn audio_format(&self) -> Option<AudioFormat> {
        video!(&self.video, v => v.audio_format())
    }

    /// The frames shown so far.
    pub fn frames_shown(&self) -> u32 {
        self.shown
    }

    /// The number of frames of the file.
    pub fn frame_count(&self) -> u32 {
        video!(&self.video, v => v.frame_count())
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// When the next frame is due, in milliseconds after the start: with
    /// sound, when the sound of the frames shown so far has played (the
    /// first frame's sound covering 32 frames, the start of each later
    /// frame is its own buffer's end); without, `1000 / fps` ms a frame.
    pub fn next_due_ms(&self) -> u64 {
        video!(&self.video, v => v.next_frame_start_ms())
    }

    /// Decodes the next frame; `false` at the end of the film (or after a
    /// decoding error, logged).
    pub fn advance(&mut self) -> bool {
        if self.ended {
            return false;
        }
        let r = video!(&mut self.video, v => v.next_frame().map(|r| r.map(|_| ())));
        match r {
            Some(Ok(())) => {
                self.shown += 1;
                true
            }
            Some(Err(e)) => {
                log::warn!("{}: {e:?}", self.name);
                self.ended = true;
                false
            }
            None => {
                self.ended = true;
                false
            }
        }
    }

    /// The sound decoded with the frames since the last call (signed 16
    /// bits, interleaved in stereo).
    pub fn take_audio(&mut self) -> Vec<i16> {
        video!(&mut self.video, v => v.take_audio())
    }

    /// The last frame decoded, RGB565 (empty when decoding in x8r8g8b8).
    pub fn frame(&self) -> &[u16] {
        match &self.video {
            Video::Rgb565(v) => v.frame(),
            Video::Xrgb8888(_) => &[],
        }
    }

    /// The last frame decoded, in the display's format.
    pub fn frame_px(&self) -> Frame<'_> {
        match &self.video {
            Video::Rgb565(v) => Frame::Rgb565(v.frame()),
            Video::Xrgb8888(v) => Frame::Xrgb8888(v.frame()),
        }
    }
}
