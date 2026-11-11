//! Theora videos as the original shows them: decoded frames
//! ([`theora`]), converted to RGBA by the original's own arithmetic
//! ([`rgb`]), a colour stream paired with its alpha stream, and the frame
//! a time falls on.
//!
//! - **Pairs** (`TeTheoraWithAlpha`): the video factory
//!   (atlantis4-gog-en-2363910 @ 0x00551de0) pairs `X.ogv` with
//!   `X.alpha.ogv` when that file exists ([`alpha_name`]). Frame *k* of
//!   the pair is packet *k* of each stream: the two streams of every pair
//!   of the data have as many packets, but not the same empty (repeat)
//!   ones, so the pairing is by packet index, never by decoded picture.
//! - **Time**: frame *k* starts at *k* / fps seconds, fps from the stream's
//!   own header (15, 24 or 25 in the data); [`frame_at`] is the frame
//!   shown after a time, as the frame animations do
//!   (`k = trunc(elapsed / 1000 · fps)`, `TeFrameAnim::update` @ 0x0056e850,
//!   here in exact integer arithmetic).

pub mod rgb;
pub mod theora;

pub use rgb::RgbTable;
pub use theora::{Picture, Plane, Theora};

/// Why a video cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoError {
    /// The Ogg container.
    Ogg(crate::ogg::OggError),
    /// No Theora stream in the file.
    NoTheora,
    /// The three Theora headers are missing or wrong.
    Headers,
    /// Not 4:2:0 (the original's converters handle no other).
    PixelFormat,
    /// The decoder refused a data packet (its index).
    Packet(usize, theora_rs::TheoraError),
    /// A colour and an alpha stream (or picture) that do not match.
    Pair,
    /// The executable is no known build.
    Exe(crate::exe::ExeError),
    /// No colour table, or the wrong one, in this build.
    NoTable(&'static str),
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoError::Ogg(e) => e.fmt(f),
            VideoError::NoTheora => f.write_str("no Theora stream"),
            VideoError::Headers => f.write_str("bad Theora headers"),
            VideoError::PixelFormat => f.write_str("Theora stream not 4:2:0"),
            VideoError::Packet(i, e) => write!(f, "Theora packet {i}: {e}"),
            VideoError::Pair => f.write_str("colour and alpha do not match"),
            VideoError::Exe(e) => e.fmt(f),
            VideoError::NoTable(b) => {
                write!(f, "no video colour table found in the executable ({b})")
            }
        }
    }
}

impl std::error::Error for VideoError {}

/// The name of a video's alpha companion: `X.ogv` → `X.alpha.ogv`.
pub fn alpha_name(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".ogv")?;
    if stem.ends_with(".alpha") {
        return None;
    }
    Some(format!("{stem}.alpha.ogv"))
}

/// The frame shown `elapsed_ms` after the start at `fps` (numerator,
/// denominator): ⌊elapsed · num / (den · 1000)⌋.
pub fn frame_at(fps: (u32, u32), elapsed_ms: u64) -> u64 {
    let (num, den) = (u128::from(fps.0), u128::from(fps.1.max(1)));
    (u128::from(elapsed_ms) * num / (den * 1000)) as u64
}

/// The first millisecond at which [`frame_at`] gives `frame`.
pub fn frame_start_ms(fps: (u32, u32), frame: u64) -> u64 {
    let (num, den) = (u128::from(fps.0.max(1)), u128::from(fps.1.max(1)));
    (u128::from(frame) * den * 1000).div_ceil(num) as u64
}

/// A video as RGBA frames: a colour stream and, for `TeTheoraWithAlpha`,
/// its alpha stream.
#[derive(Debug)]
pub struct Video {
    colour: Theora,
    alpha: Option<Theora>,
}

impl Video {
    /// A colour stream and its alpha companion if there is one. The two
    /// must have the same picture size and frame count (true of all 924
    /// pairs of the data).
    pub fn open(colour: &[u8], alpha: Option<&[u8]>) -> Result<Video, VideoError> {
        let colour = Theora::open(colour)?;
        let alpha = alpha.map(Theora::open).transpose()?;
        if let Some(a) = &alpha {
            if a.picture_size() != colour.picture_size() || a.frame_count() != colour.frame_count()
            {
                return Err(VideoError::Pair);
            }
        }
        Ok(Video { colour, alpha })
    }

    /// The picture's width and height.
    pub fn size(&self) -> (u32, u32) {
        self.colour.picture_size()
    }

    /// The colour stream's frame rate (numerator, denominator).
    pub fn fps(&self) -> (u32, u32) {
        self.colour.fps()
    }

    pub fn frame_count(&self) -> usize {
        self.colour.frame_count()
    }

    pub fn has_alpha(&self) -> bool {
        self.alpha.is_some()
    }

    /// The index of the frame [`Video::next_rgba`] decodes next.
    pub fn position(&self) -> usize {
        self.colour.position()
    }

    /// The colour stream.
    pub fn colour(&mut self) -> &mut Theora {
        &mut self.colour
    }

    /// The alpha stream, if any.
    pub fn alpha(&mut self) -> Option<&mut Theora> {
        self.alpha.as_mut()
    }

    /// Back to the first frame.
    pub fn rewind(&mut self) -> Result<(), VideoError> {
        self.colour.rewind()?;
        if let Some(a) = &mut self.alpha {
            a.rewind()?;
        }
        Ok(())
    }

    /// Decodes the next frame into `out` (width × height × 4 bytes, RGBA,
    /// rows top first): R, G, B from the colour stream, A from the alpha
    /// stream, 0 without one. `false` after the last frame.
    pub fn next_rgba(&mut self, table: &RgbTable, out: &mut [u8]) -> Result<bool, VideoError> {
        let Some(pic) = self.colour.next_frame()? else {
            return Ok(false);
        };
        table.to_rgba(&pic, out);
        if let Some(a) = &mut self.alpha {
            let full = a.full_range();
            let pic = a.next_frame()?.ok_or(VideoError::Pair)?;
            rgb::apply_alpha(&pic, full, out);
        }
        Ok(true)
    }

    /// Decodes up to frame `frame` (from the start again if it is behind)
    /// and leaves it in `out`; `false` if the video has fewer frames.
    pub fn seek_rgba(
        &mut self,
        table: &RgbTable,
        frame: usize,
        out: &mut [u8],
    ) -> Result<bool, VideoError> {
        if frame >= self.frame_count() {
            return Ok(false);
        }
        if frame < self.position() {
            self.rewind()?;
        }
        while self.position() < frame {
            self.colour.next_frame()?;
            if let Some(a) = &mut self.alpha {
                a.next_frame()?;
            }
        }
        self.next_rgba(table, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alpha_companion_is_named_after_the_colour_stream() {
        assert_eq!(
            alpha_name("Videos/Dialogs/x.ogv").as_deref(),
            Some("Videos/Dialogs/x.alpha.ogv")
        );
        assert_eq!(alpha_name("x.alpha.ogv"), None);
        assert_eq!(alpha_name("x.ogg"), None);
    }

    #[test]
    fn frames_follow_the_streams_rate() {
        assert_eq!(frame_at((15, 1), 0), 0);
        assert_eq!(frame_at((15, 1), 66), 0);
        assert_eq!(frame_at((15, 1), 67), 1);
        assert_eq!(frame_at((15, 1), 1000), 15);
        assert_eq!(frame_at((24, 1), 2_458), 58);
        assert_eq!(frame_at((25, 1), 40), 1);
        for fps in [(15, 1), (24, 1), (25, 1), (30_000, 1_001)] {
            for k in 0..200 {
                let t = frame_start_ms(fps, k);
                assert_eq!(frame_at(fps, t), k, "{fps:?} {k}");
                if t > 0 {
                    assert_eq!(frame_at(fps, t - 1), k - 1, "{fps:?} {k}");
                }
            }
        }
    }
}
