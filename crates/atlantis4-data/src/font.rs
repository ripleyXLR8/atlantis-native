//! `.tef` fonts (`TESF`): glyph metrics and the glyph atlas, a PNG.
//!
//! Read by `TeFont2::load` (atlantis4-gog-en-2363910, log strings
//! `TeFont2::load: can't load %s` @ 0x005ac764 and @ 0x005ac784, pushed
//! @ 0x004a090e and @ 0x004a0a74); the reader's fields are not walked in
//! this executable yet. The layout below is ScummVM's `TeFont2::load`
//! (`engines/tetraedge/te/te_font2.cpp`, the same engine family), and every
//! one of the six `.tef` files of the install parses with it to a PNG
//! that ends the file [CONFIRMED-DATA]:
//!
//! | Field | Type |
//! |---|---|
//! | magic | `TESF` |
//! | glyph count | `u32` (256 in every file) |
//! | a point | two `i32` (`0`, the size in the name) |
//! | a vector | three `f32` (ascent, descent and height in the data: e.g. 16, 4, 19 for `Arial_r_16`) [INFERRED roles] |
//! | kerning | `u8` flag; if set a `u32` count and per pair two `u32` characters and two `f32` |
//! | glyphs | per glyph: `f32` width, height, x offset, y offset, advance, three `f32` [UNKNOWN roles], `f32` x, y in the atlas, then a `u32` |
//! | atlas | a PNG, to the end of the file |
//!
//! The ten `.font` files beside them have no magic, and no string of the
//! executable or script names them: they are not read.

/// One glyph's metrics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub advance: f32,
    /// Three values whose roles are not known.
    pub unknown: [f32; 3],
    /// Where the glyph is in the atlas.
    pub atlas: [f32; 2],
    /// The `u32` after each glyph.
    pub code: u32,
}

/// A kerning pair: two characters and an offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kerning {
    pub left: u32,
    pub right: u32,
    pub offset: [f32; 2],
}

/// A `.tef` font; the atlas is left encoded.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub point: [i32; 2],
    pub metrics: [f32; 3],
    pub kerning: Vec<Kerning>,
    pub glyphs: Vec<Glyph>,
    /// Where the PNG atlas starts in the file (it runs to the end).
    pub atlas_offset: usize,
}

/// Why a font cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    Magic,
    Truncated,
    /// The atlas is not a PNG.
    Atlas,
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::Magic => write!(f, "not a TESF font"),
            FontError::Truncated => write!(f, "font truncated"),
            FontError::Atlas => write!(f, "font atlas is not a PNG"),
        }
    }
}

impl std::error::Error for FontError {}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn bytes(&mut self, n: usize) -> Result<&[u8], FontError> {
        let end = self.at.checked_add(n).ok_or(FontError::Truncated)?;
        let b = self.data.get(self.at..end).ok_or(FontError::Truncated)?;
        self.at = end;
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32, FontError> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Result<f32, FontError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

impl Font {
    pub fn parse(data: &[u8]) -> Result<Font, FontError> {
        if !data.starts_with(b"TESF") {
            return Err(FontError::Magic);
        }
        let mut c = Cursor { data, at: 4 };
        let count = c.u32()? as usize;
        if count.saturating_mul(44) > data.len() {
            return Err(FontError::Truncated);
        }
        let point = [c.u32()? as i32, c.u32()? as i32];
        let metrics = [c.f32()?, c.f32()?, c.f32()?];
        let mut kerning = Vec::new();
        if c.bytes(1)?[0] != 0 {
            let n = c.u32()? as usize;
            if n.saturating_mul(16) > data.len() {
                return Err(FontError::Truncated);
            }
            for _ in 0..n {
                kerning.push(Kerning {
                    left: c.u32()?,
                    right: c.u32()?,
                    offset: [c.f32()?, c.f32()?],
                });
            }
        }
        let mut glyphs = Vec::with_capacity(count);
        for _ in 0..count {
            glyphs.push(Glyph {
                size: [c.f32()?, c.f32()?],
                offset: [c.f32()?, c.f32()?],
                advance: c.f32()?,
                unknown: [c.f32()?, c.f32()?, c.f32()?],
                atlas: [c.f32()?, c.f32()?],
                code: c.u32()?,
            });
        }
        if !data[c.at..].starts_with(PNG_SIGNATURE) {
            return Err(FontError::Atlas);
        }
        Ok(Font {
            point,
            metrics,
            kerning,
            glyphs,
            atlas_offset: c.at,
        })
    }

    /// The PNG atlas's bytes in `data`, the file this font was read from.
    pub fn atlas<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        &data[self.atlas_offset..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_reads_its_glyphs_and_finds_its_atlas() {
        let mut d = b"TESF".to_vec();
        d.extend(1u32.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend(16u32.to_le_bytes());
        for v in [16.0f32, 4.0, 19.0] {
            d.extend(v.to_le_bytes());
        }
        d.push(1);
        d.extend(1u32.to_le_bytes());
        d.extend(65u32.to_le_bytes());
        d.extend(86u32.to_le_bytes());
        d.extend((-1.0f32).to_le_bytes());
        d.extend(0.0f32.to_le_bytes());
        for v in 0..10 {
            d.extend((v as f32).to_le_bytes());
        }
        d.extend(65u32.to_le_bytes());
        d.extend(PNG_SIGNATURE);
        let f = Font::parse(&d).unwrap();
        assert_eq!(f.point, [0, 16]);
        assert_eq!(f.kerning[0].offset, [-1.0, 0.0]);
        assert_eq!(f.glyphs[0].advance, 4.0);
        assert_eq!(f.glyphs[0].code, 65);
        assert_eq!(f.atlas(&d), PNG_SIGNATURE);
        assert_eq!(Font::parse(b"TESX"), Err(FontError::Magic));
    }
}
