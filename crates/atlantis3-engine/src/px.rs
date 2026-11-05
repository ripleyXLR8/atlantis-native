//! The display's pixel format: RGB565 (`PixelFormat = 16bits`, the
//! default) or x8r8g8b8 (`32bits`). The original makes its screen, the
//! menus' page surface, the cube faces, the dialog surfaces and every
//! texture in the display's format, and its decoders and blitters write
//! that format directly: on a 32-bit display nothing is drawn in 16 bits
//! and converted.
//!
//! - the cube faces: the warp init (`Game.exe` @ 0x0043f420) makes them in
//!   the format of the screen (0x0040f0e0), `CHnmStream::Open` gets the
//!   bits a pixel from the display (@ 0x0040efc0), the HNM decoder writes
//!   x8r8g8b8 through its 32-bit colour writer (@ 0x006a349d);
//! - the panorama: the texture setter (@ 0x004fc3a0) flags a 32-bit face
//!   (mode bit 0x4000), the selector (@ 0x004fc540) picks `warp32BFPII`
//!   (span @ 0x00539000);
//! - the videos: CryoMovie's play re-initialises its buffers for the
//!   surface's format (@ 0x006b9619: 0x20), `CHnmStream` as the faces;
//! - the sprites: the banks are prepared for the display (`Spr_InitGraph`
//!   with its masks) and drawn by the 32-bit blitters of `spr_p6.dll`;
//! - the TGA maps (heads, the unicorn board): copied into the display's
//!   format (@ 0x00418e70 → @ 0x004ecc40), 24 bits → `R << 16 | G << 8 |
//!   B`; the heads drawn by `Zi32Texture32Linear32AsmPII` (span
//!   @ 0x0055c000, 4-byte texels);
//! - the texts: GDI on the surface's DC, the colour as it is.

use atlantis2_engine::WarpTexel;
use atlantis3_data::spr::Depth;
use atlantis_codecs::HnmPixel;

use crate::menu::PagePixel;

/// A pixel of the screen and of the surfaces drawn into it.
pub trait ScreenPixel:
    PagePixel + HnmPixel + WarpTexel + PartialEq + std::fmt::Debug + Send + Sync
{
    /// The surfaces of this format.
    fn surface(v: Vec<Self>) -> Surface;
    /// The pixel's colour, 8 bits a channel (R, G, B).
    fn rgb8(self) -> [u8; 3];
    /// GDI's pixel of a colour on a surface of this format (the texts):
    /// RGB565 keeps the top 5 / 6 / 5 bits, x8r8g8b8 the colour (x 0).
    fn from_rgb8(r: u8, g: u8, b: u8) -> Self;
    /// A 24-bit TGA texel copied into a surface of this format
    /// (@ 0x004ecc40).
    fn from_tga(r: u8, g: u8, b: u8) -> Self {
        Self::from_rgb8(r, g, b)
    }
}

impl ScreenPixel for u16 {
    fn surface(v: Vec<u16>) -> Surface {
        Surface::Rgb565(v)
    }

    fn rgb8(self) -> [u8; 3] {
        [
            ((self >> 11) << 3) as u8,
            (((self >> 5) & 0x3f) << 2) as u8,
            ((self & 0x1f) << 3) as u8,
        ]
    }

    fn from_rgb8(r: u8, g: u8, b: u8) -> u16 {
        (u16::from(r) >> 3) << 11 | (u16::from(g) >> 2) << 5 | u16::from(b) >> 3
    }
}

impl ScreenPixel for u32 {
    fn surface(v: Vec<u32>) -> Surface {
        Surface::Xrgb8888(v)
    }

    fn rgb8(self) -> [u8; 3] {
        [(self >> 16) as u8, (self >> 8) as u8, self as u8]
    }

    fn from_rgb8(r: u8, g: u8, b: u8) -> u32 {
        u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
    }
}

/// A surface's pixels in the display's format.
#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Rgb565(Vec<u16>),
    Xrgb8888(Vec<u32>),
}

impl Default for Surface {
    fn default() -> Self {
        Surface::Rgb565(Vec::new())
    }
}

/// Runs `$body` with `$v` bound to the surface's pixels, `u16` or `u32`
/// (the body is compiled once for each).
#[macro_export]
#[doc(hidden)]
macro_rules! with_px {
    ($s:expr, $v:ident => $body:expr) => {
        match $s {
            $crate::px::Surface::Rgb565($v) => $body,
            $crate::px::Surface::Xrgb8888($v) => $body,
        }
    };
}

impl Surface {
    /// `n` black pixels of `depth`.
    pub fn new(depth: Depth, n: usize) -> Surface {
        match depth {
            Depth::Rgb565 => Surface::Rgb565(vec![0; n]),
            Depth::Xrgb8888 => Surface::Xrgb8888(vec![0; n]),
        }
    }

    pub fn depth(&self) -> Depth {
        match self {
            Surface::Rgb565(_) => Depth::Rgb565,
            Surface::Xrgb8888(_) => Depth::Xrgb8888,
        }
    }

    pub fn len(&self) -> usize {
        with_px!(self, v => v.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every pixel black (0).
    pub fn clear(&mut self) {
        with_px!(self, v => v.fill(0))
    }

    /// The RGB565 pixels, `None` on a 32-bit surface.
    pub fn as_565(&self) -> Option<&[u16]> {
        match self {
            Surface::Rgb565(v) => Some(v),
            Surface::Xrgb8888(_) => None,
        }
    }

    /// The x8r8g8b8 pixels, `None` on a 16-bit surface.
    pub fn as_x888(&self) -> Option<&[u32]> {
        match self {
            Surface::Xrgb8888(v) => Some(v),
            Surface::Rgb565(_) => None,
        }
    }

    /// Copies `src` when both have the format and the size; false
    /// otherwise.
    pub fn copy_from(&mut self, src: &Surface) -> bool {
        match (self, src) {
            (Surface::Rgb565(d), Surface::Rgb565(s)) if d.len() == s.len() => {
                d.copy_from_slice(s);
                true
            }
            (Surface::Xrgb8888(d), Surface::Xrgb8888(s)) if d.len() == s.len() => {
                d.copy_from_slice(s);
                true
            }
            _ => false,
        }
    }

    /// The pixels as R, G, B bytes, 8 bits a channel (an RGB565 channel
    /// shifted to the top of its byte).
    pub fn to_rgb(&self) -> Vec<[u8; 3]> {
        with_px!(self, v => v.iter().map(|p| p.rgb8()).collect())
    }

    /// The pixels as little-endian bytes (2 or 4 a pixel), for digests.
    pub fn le_bytes(&self) -> Vec<u8> {
        match self {
            Surface::Rgb565(v) => v.iter().flat_map(|p| p.to_le_bytes()).collect(),
            Surface::Xrgb8888(v) => v.iter().flat_map(|p| p.to_le_bytes()).collect(),
        }
    }
}

/// Six cube faces in the display's format.
#[derive(Clone, Debug, PartialEq)]
pub enum Faces {
    Rgb565([Vec<u16>; 6]),
    Xrgb8888([Vec<u32>; 6]),
}

impl Default for Faces {
    fn default() -> Self {
        Faces::Rgb565(Default::default())
    }
}

impl Faces {
    pub fn depth(&self) -> Depth {
        match self {
            Faces::Rgb565(_) => Depth::Rgb565,
            Faces::Xrgb8888(_) => Depth::Xrgb8888,
        }
    }

    /// Whether a face is missing (no cube loaded).
    pub fn any_empty(&self) -> bool {
        match self {
            Faces::Rgb565(f) => f.iter().any(Vec::is_empty),
            Faces::Xrgb8888(f) => f.iter().any(Vec::is_empty),
        }
    }

    /// The RGB565 faces, `None` at 32 bits.
    pub fn as_565(&self) -> Option<&[Vec<u16>; 6]> {
        match self {
            Faces::Rgb565(f) => Some(f),
            Faces::Xrgb8888(_) => None,
        }
    }

    /// The x8r8g8b8 faces, `None` at 16 bits.
    pub fn as_x888(&self) -> Option<&[Vec<u32>; 6]> {
        match self {
            Faces::Xrgb8888(f) => Some(f),
            Faces::Rgb565(_) => None,
        }
    }
}
