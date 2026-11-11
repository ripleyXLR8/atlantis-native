//! One Theora stream: its Ogg packets, its headers and its frames.
//!
//! The decoder is `theora-rs`, a Rust port of libtheora 1.2.0's decoder;
//! the original links libtheora 1.2.0 alpha (`Xiph.Org libtheora 1.2.0alpha
//! 20100924 (Ptalarbvorm)` in the executable). On every stream of the data
//! its frames equal libtheora's (see the game-data test `video.rs`).
//!
//! What `TeTheora` does around the decoder, and this wrapper too:
//!
//! - `TeTheora::open` calls `th_decode_alloc` (atlantis4-gog-en-2363910
//!   @ 0x004cc547) with no `th_decode_ctl` after it: the post-processing
//!   level stays libtheora's default 0 (also `theora-rs`'s);
//! - one frame per data packet; an empty packet is a frame that repeats
//!   the previous picture (libtheora's `TH_DUPFRAME`). The data has 32,401
//!   of them;
//! - the picture is the region `th_info` gives (offset counted from the
//!   top): luma from `(pic_y & !1, pic_x & !1)`, chroma from
//!   `(pic_y >> 1, pic_x >> 1)` (@ 0x004cc995..0x004cc9c2). Every stream of
//!   the data has `pic_x` = 0 and an even `pic_y` (0 or 4: 800 × 600 in
//!   800 × 608, 704 × 394 in 704 × 400), so the rounding never matters;
//! - while reading the headers, a comment starting with `ColorRange` whose
//!   text contains `FullRange` marks the stream full range
//!   (@ 0x004cc32f..0x004cc43c), which only an alpha stream's conversion
//!   reads ([`super::rgb::apply_alpha`]).

use theora_rs::{DecoderContext, HeaderParser, Info, OggPacket, PixelFmt, SetupInfo};

use super::VideoError;
use crate::ogg;

/// One plane of a decoded picture: `data[origin + y * stride + x]` is the
/// sample at column `x`, row `y` (row 0 at the top; the stride may be
/// negative, as libtheora's buffers are stored bottom-up).
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    data: &'a [u8],
    origin: usize,
    stride: isize,
}

impl<'a> Plane<'a> {
    /// A plane over `data`, its picture's first row at `origin`.
    ///
    /// # Panics
    ///
    /// When a row the plane is read at lies outside `data` (at the read).
    pub fn new(data: &'a [u8], origin: usize, stride: isize) -> Plane<'a> {
        Plane {
            data,
            origin,
            stride,
        }
    }

    /// The first `len` samples of row `y`.
    pub fn row(&self, y: usize, len: usize) -> &'a [u8] {
        let start = self.origin as isize + y as isize * self.stride;
        let start = usize::try_from(start).expect("plane row before its data");
        &self.data[start..start + len]
    }
}

/// A decoded 4:2:0 picture, cropped to the stream's picture region.
#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    pub width: usize,
    pub height: usize,
    pub y: Plane<'a>,
    /// Chroma planes: `(width + 1) / 2` × `(height + 1) / 2` samples.
    pub cb: Plane<'a>,
    pub cr: Plane<'a>,
}

impl Picture<'_> {
    /// Width and height of the chroma planes.
    pub fn chroma_size(&self) -> (usize, usize) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    /// The three planes cropped and packed one after the other (Y, Cb,
    /// Cr, rows top first): the layout of the reference decoder whose
    /// frame hashes the tests compare.
    pub fn packed(&self) -> Vec<u8> {
        let (cw, ch) = self.chroma_size();
        let mut out = Vec::with_capacity(self.width * self.height + 2 * cw * ch);
        for y in 0..self.height {
            out.extend_from_slice(self.y.row(y, self.width));
        }
        for p in [&self.cb, &self.cr] {
            for y in 0..ch {
                out.extend_from_slice(p.row(y, cw));
            }
        }
        out
    }
}

/// A Theora stream ready to decode, frame after frame.
pub struct Theora {
    info: Info,
    setup: SetupInfo,
    full_range: bool,
    /// The data packets (the three headers excluded).
    packets: Vec<Vec<u8>>,
    decoder: DecoderContext,
    /// The index of the next frame [`Theora::next_frame`] decodes.
    next: usize,
}

impl std::fmt::Debug for Theora {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Theora")
            .field("picture", &self.picture_size())
            .field("fps", &self.fps())
            .field("frames", &self.frame_count())
            .field("full_range", &self.full_range)
            .field("next", &self.next)
            .finish()
    }
}

/// Whether a comment marks the stream full range: it starts with
/// `ColorRange` (`strncmp(…, 10)`) and contains `FullRange` (`strstr`,
/// which stops at a NUL byte).
fn is_full_range(comment: &[u8]) -> bool {
    let text = comment.split(|&b| b == 0).next().unwrap_or_default();
    text.starts_with(b"ColorRange") && text.windows(9).any(|w| w == b"FullRange")
}

impl Theora {
    /// The Theora stream of an Ogg file (an `.ogv` file, whose first
    /// stream is an Ogg Skeleton, or an `.ogv` entry of a warp package):
    /// its three headers read, a decoder made.
    pub fn open(data: &[u8]) -> Result<Theora, VideoError> {
        let (_, streams) = ogg::read(data, usize::MAX).map_err(VideoError::Ogg)?;
        let mut packets = streams
            .into_values()
            .find(|p| p.first().is_some_and(|f| f.starts_with(b"\x80theora")))
            .ok_or(VideoError::NoTheora)?;
        if packets.len() < 3 {
            return Err(VideoError::Headers);
        }
        let data_packets = packets.split_off(3);
        let mut parser = HeaderParser::new();
        for (i, p) in packets.into_iter().enumerate() {
            let op = OggPacket {
                packet: p,
                b_o_s: i == 0,
                e_o_s: false,
                granulepos: 0,
                packetno: i as i64,
            };
            parser.push(&op).map_err(|_| VideoError::Headers)?;
        }
        if !parser.is_ready() {
            return Err(VideoError::Headers);
        }
        if parser.info.pixel_fmt != PixelFmt::Pf420 {
            return Err(VideoError::PixelFormat);
        }
        let decoder = parser.decoder().map_err(|_| VideoError::Headers)?;
        let full_range = parser
            .comment
            .user_comments
            .iter()
            .any(|c| is_full_range(c));
        let (info, _, setup) = parser.into_parts();
        Ok(Theora {
            info,
            setup: setup.ok_or(VideoError::Headers)?,
            full_range,
            packets: data_packets,
            decoder,
            next: 0,
        })
    }

    /// The picture's width and height.
    pub fn picture_size(&self) -> (u32, u32) {
        (self.info.pic_width, self.info.pic_height)
    }

    /// The coded frame's width and height (multiples of 16).
    pub fn frame_size(&self) -> (u32, u32) {
        (self.info.frame_width, self.info.frame_height)
    }

    /// Where the picture lies in the coded frame, from its top left.
    pub fn picture_offset(&self) -> (u32, u32) {
        (self.info.pic_x, self.info.pic_y)
    }

    /// The frame rate as numerator and denominator.
    pub fn fps(&self) -> (u32, u32) {
        (self.info.fps_numerator, self.info.fps_denominator)
    }

    /// The number of frames: one per data packet, empty ones included.
    pub fn frame_count(&self) -> usize {
        self.packets.len()
    }

    /// Whether a comment says `ColorRange=FullRange`.
    pub fn full_range(&self) -> bool {
        self.full_range
    }

    /// The index of the frame the next [`Theora::next_frame`] gives.
    pub fn position(&self) -> usize {
        self.next
    }

    /// Back to the first frame (a new decoder: every frame of the data
    /// depends on the frames since the previous intra frame).
    pub fn rewind(&mut self) -> Result<(), VideoError> {
        self.decoder = theora_rs::decoder_from_headers(&self.info, &self.setup)
            .map_err(|_| VideoError::Headers)?;
        self.next = 0;
        Ok(())
    }

    /// Decodes the next frame and gives its picture, or `None` after the
    /// last frame.
    pub fn next_frame(&mut self) -> Result<Option<Picture<'_>>, VideoError> {
        let Some(p) = self.packets.get(self.next) else {
            return Ok(None);
        };
        let index = self.next;
        let op = OggPacket {
            packet: p.clone(),
            b_o_s: false,
            e_o_s: index + 1 == self.packets.len(),
            granulepos: -1,
            packetno: (index + 3) as i64,
        };
        self.decoder
            .packetin(&op)
            .map_err(|e| VideoError::Packet(index, e))?;
        self.next += 1;
        self.picture().map(Some)
    }

    /// The last decoded frame's picture.
    fn picture(&self) -> Result<Picture<'_>, VideoError> {
        let index = self.next.saturating_sub(1);
        let buf = self
            .decoder
            .ycbcr_ref()
            .map_err(|e| VideoError::Packet(index, e))?;
        let (x, y) = (self.info.pic_x as usize, self.info.pic_y as usize);
        let plane = |i: usize, ox: usize, oy: usize| {
            let p = &buf[i];
            let stride = p.stride as isize;
            let origin = p.data_offset as isize + oy as isize * stride + ox as isize;
            Plane::new(p.data, origin as usize, stride)
        };
        Ok(Picture {
            width: self.info.pic_width as usize,
            height: self.info.pic_height as usize,
            y: plane(0, x & !1, y & !1),
            cb: plane(1, x >> 1, y >> 1),
            cr: plane(2, x >> 1, y >> 1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_range_needs_the_name_first_and_the_word_before_a_nul() {
        assert!(is_full_range(b"ColorRange=FullRange"));
        assert!(is_full_range(b"ColorRangeXFullRangeY"));
        assert!(!is_full_range(b"colorrange=FullRange"));
        assert!(!is_full_range(b"X ColorRange=FullRange"));
        assert!(!is_full_range(b"ColorRange=Full\0Range"));
        assert!(!is_full_range(b"ColorRange=\0FullRange"));
    }

    #[test]
    fn a_plane_reads_rows_top_first_with_a_negative_stride() {
        // Two rows stored bottom-up: the top row is the second one.
        let data = [1, 2, 3, 4, 5, 6];
        let p = Plane::new(&data, 3, -3);
        assert_eq!(p.row(0, 3), [4, 5, 6]);
        assert_eq!(p.row(1, 2), [1, 2]);
    }

    #[test]
    fn a_file_without_theora_is_refused() {
        assert!(matches!(Theora::open(b""), Err(VideoError::NoTheora)));
        assert!(matches!(Theora::open(b"junk"), Err(VideoError::Ogg(_))));
    }
}
