//! A warp package (`<node>/PC-MacOSX/<name>.wpa`) loaded for drawing: an
//! object's or an animation's tiles, their pictures and their pick meshes
//! (`TeWarp3::Anim::load`, atlantis4-gog-en-2363910 @ 0x004f2ad0; tiles
//! @ 0x004f1870).
//!
//! - A tile's codec follows its file (@ 0x004f1870): `.ogv` →
//!   `TeTheoraWithAlpha` (the stream and its `.alpha.ogv`); `.jpg` →
//!   `TeVideoWithAlpha` over `X.rgb.jpg` + `X.alpha.jpg` (objects), or a
//!   plain JPEG (backgrounds, drawn by [`crate::cube`] instead).
//! - Each opened tile writes its codec's frame count and frame rate into
//!   the animation (`anim+0x78`, `anim+0x7c` @ 0x004f2661..0x004f2696), so
//!   the **last** tile's values are the animation's: a JPEG gives 1 frame
//!   at 0 fps (`0x004b18b0`, `0x004b17d0`), a Theora stream its own. The
//!   XML's `fps` is not read.
//! - A tile draws the range of the animation's current frame
//!   (`tile+0x298[frame]`, filled forward from each range's first frame;
//!   past the table, the last range: @ 0x004f01ee..0x004f0241); the pick
//!   mesh is chosen the same way (`anim+0xb0`). Before the first range's
//!   first frame the table holds null (filled with 0 while it is empty,
//!   @ 0x004f1b85..0x004f1bff), and a null range is not drawn (the tile's
//!   texture is dropped, @ 0x004f0245 → 0x004f0366): 77 tiles of the data
//!   start after frame 0.
//! - Texture coordinates: a picture of `w × h` sits in a power-of-two
//!   texture with the matrix `scale(w/W, h/H)` (and, for Theora frames,
//!   the vertical flip), so a UV reaches the picture's texel
//!   `(u·w, (1 − v)·h)` counted from its top row, as the background
//!   atlases' (warp.md § 5, § 10.2).

use atlantis4_data::anim::{Anim, PickMesh, TileRange};
use atlantis4_data::image::decode_jpeg;
use atlantis4_data::video::{rgb::merge_jpeg_alpha, RgbTable, Video};
use atlantis4_data::wpa::{Package as Wpa, TileSource};
use atlantis4_data::{A4Files, WarpPath};

use crate::node::NodeError;

/// A picture, RGBA, rows top-down.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    /// The RGBA texel at (`x`, `y`), transparent black outside.
    #[inline]
    pub fn texel(&self, x: i64, y: i64) -> [u8; 4] {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return [0; 4];
        }
        let o = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[o], self.rgba[o + 1], self.rgba[o + 2], self.rgba[o + 3]]
    }

    /// The texel at (`x`, `y`), clamped to the edge (`GL_CLAMP_TO_EDGE`).
    #[inline]
    pub fn texel_clamped(&self, x: i64, y: i64) -> [u8; 4] {
        let x = x.clamp(0, i64::from(self.width.max(1)) - 1);
        let y = y.clamp(0, i64::from(self.height.max(1)) - 1);
        self.texel(x, y)
    }
}

/// Where a tile's pictures come from.
pub enum Codec {
    /// A JPEG and its grey alpha JPEG (`TeVideoWithAlpha`): one frame.
    Still(Picture),
    /// A Theora stream and its alpha stream (`TeTheoraWithAlpha`); the
    /// picture of `frame` once decoded.
    Theora {
        video: Box<Video>,
        frame: Option<usize>,
        picture: Picture,
    },
}

/// A tile of a package: its ranges and its pictures.
pub struct Tile {
    pub name: String,
    pub ranges: Vec<TileRange>,
    pub codec: Codec,
}

/// The index of the range (sorted by first frame) for `frame`: the last
/// range starting at or before it; a negative frame (the animation not
/// updated yet, `−1`) or one past the table, the last range; a frame
/// before the first range, none.
pub fn range_index(firsts: impl Iterator<Item = u32> + Clone, frame: i32) -> Option<usize> {
    let n = firsts.clone().count();
    if n == 0 {
        return None;
    }
    if frame < 0 {
        return Some(n - 1);
    }
    let f = frame as u32;
    let mut found = None;
    for (i, first) in firsts.enumerate() {
        if first <= f {
            found = Some(i);
        }
    }
    found
}

impl Tile {
    /// The range drawn at `frame`.
    pub fn range(&self, frame: i32) -> Option<&TileRange> {
        range_index(self.ranges.iter().map(|r| r.first_frame), frame).map(|i| &self.ranges[i])
    }
}

impl Codec {
    /// The picture to draw at `frame` (Theora: decoded up to it once;
    /// `name` for the log).
    pub fn picture(&mut self, name: &str, frame: i32, table: Option<&RgbTable>) -> Option<&Picture> {
        match self {
            Codec::Still(p) => Some(p),
            Codec::Theora {
                video,
                frame: cur,
                picture,
            } => {
                let want = usize::try_from(frame).unwrap_or(0);
                if *cur != Some(want) {
                    let table = table?;
                    match video.seek_rgba(table, want, &mut picture.rgba) {
                        Ok(true) => *cur = Some(want),
                        Ok(false) => return None,
                        Err(e) => {
                            log::warn!("{name}: frame {want}: {e}");
                            return None;
                        }
                    }
                }
                Some(picture)
            }
        }
    }
}

/// An object or animation package ready to draw.
pub struct Package {
    /// The frame count of the last tile's codec (`anim+0x78`).
    pub frame_count: u32,
    /// Its frame rate (`anim+0x7c`): 0 for JPEG, the stream's for Theora.
    pub fps: f32,
    pub tiles: Vec<Tile>,
    pub picks: Vec<PickMesh>,
}

impl Package {
    /// The pick mesh of `frame` (`anim+0xb0`, chosen as the tiles' ranges).
    pub fn pick(&self, frame: i32) -> Option<&PickMesh> {
        range_index(self.picks.iter().map(|p| p.first_frame), frame).map(|i| &self.picks[i])
    }

    /// The package `name` of the node `path`.
    pub fn load(files: &A4Files, path: &WarpPath, name: &str) -> Result<Package, NodeError> {
        let pkg_name = path.package(name);
        let src = files.source();
        let pkg = Wpa::open(src, &pkg_name)
            .ok_or_else(|| NodeError::Missing(pkg_name.clone()))?
            .map_err(|e| NodeError::Data(format!("{pkg_name}: {e}")))?;
        let read = |n: &str| -> Result<Vec<u8>, NodeError> {
            let e = pkg
                .entry(n)
                .ok_or_else(|| NodeError::Missing(format!("{pkg_name}:{n}")))?;
            pkg.read(src, &pkg_name, e)
                .ok_or_else(|| NodeError::Missing(format!("{pkg_name}:{n}")))
        };
        let bad = |what: &str, e: &dyn std::fmt::Display| {
            NodeError::Data(format!("{pkg_name}: {what}: {e}"))
        };
        let anim = Anim::parse(&read("anim.anim")?).map_err(|e| bad("anim.anim", &e))?;
        let mut tiles = Vec::with_capacity(anim.tiles.len());
        let (mut frame_count, mut fps) = (anim.frame_count, 0.0f32);
        for t in anim.tiles {
            let source = TileSource::resolve(&pkg, &t.name)
                .ok_or_else(|| NodeError::Missing(format!("{pkg_name}:{}", t.name)))?;
            let codec = match source {
                TileSource::Jpeg(n) => {
                    let img = decode_jpeg(&read(&n)?).map_err(|e| bad(&n, &e))?;
                    let rgba = match img.channels {
                        3 => img
                            .pixels
                            .as_chunks::<3>().0.iter()
                            .flat_map(|c| [c[0], c[1], c[2], 255])
                            .collect(),
                        _ => return Err(bad(&n, &"not RGB")),
                    };
                    Codec::Still(Picture {
                        width: img.width,
                        height: img.height,
                        rgba,
                    })
                }
                TileSource::JpegWithAlpha { rgb, alpha } => {
                    let c = decode_jpeg(&read(&rgb)?).map_err(|e| bad(&rgb, &e))?;
                    let a = decode_jpeg(&read(&alpha)?).map_err(|e| bad(&alpha, &e))?;
                    let rgba = merge_jpeg_alpha(&c, &a).map_err(|e| bad(&rgb, &e))?;
                    Codec::Still(Picture {
                        width: c.width,
                        height: c.height,
                        rgba,
                    })
                }
                TileSource::TheoraWithAlpha { rgb, alpha } => {
                    let c = read(&rgb)?;
                    let a = read(&alpha)?;
                    let video = Video::open(&c, Some(&a)).map_err(|e| bad(&rgb, &e))?;
                    let (w, h) = video.size();
                    Codec::Theora {
                        video: Box::new(video),
                        frame: None,
                        picture: Picture {
                            width: w,
                            height: h,
                            rgba: vec![0; w as usize * h as usize * 4],
                        },
                    }
                }
            };
            // `anim+0x78`, `anim+0x7c` from this tile's codec.
            match &codec {
                Codec::Still(_) => (frame_count, fps) = (1, 0.0),
                Codec::Theora { video, .. } => {
                    let (num, den) = video.fps();
                    frame_count = video.frame_count() as u32;
                    fps = num as f32 / den.max(1) as f32;
                }
            }
            tiles.push(Tile {
                name: t.name,
                ranges: t.ranges,
                codec,
            });
        }
        Ok(Package {
            frame_count,
            fps,
            tiles,
            picks: anim.picks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_fill_forward_and_the_last_takes_the_rest() {
        let firsts = [0u32, 5, 9];
        let idx = |f| range_index(firsts.iter().copied(), f);
        assert_eq!(range_index([3u32, 6].into_iter(), 2), None);
        assert_eq!(range_index([3u32, 6].into_iter(), 3), Some(0));
        assert_eq!(idx(0), Some(0));
        assert_eq!(idx(4), Some(0));
        assert_eq!(idx(5), Some(1));
        assert_eq!(idx(8), Some(1));
        assert_eq!(idx(9), Some(2));
        assert_eq!(idx(1000), Some(2));
        assert_eq!(idx(-1), Some(2));
        assert_eq!(range_index([].into_iter(), 0), None);
    }
}
