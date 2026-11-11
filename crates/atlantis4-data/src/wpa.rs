//! `.wpa` warp packages: an uncompressed directory of named files.
//!
//! Read by `TePackage::open` (atlantis4-gog-en-2363910 @ 0x004da110):
//!
//! | Field | Type | Read at |
//! |---|---|---|
//! | version | `u8`, must be 1 (else the error string @ 0x005b1420 is logged and the package refused) | @ 0x004da161, test @ 0x004da163 |
//! | count | `u32` | @ 0x004da1a2 |
//! | per entry: name | `u32` length + bytes, no terminator (the string reader @ 0x00506d10) | @ 0x004da1e3 |
//! | per entry: offset | `u32`, from the start of the file | @ 0x004da209 |
//! | per entry: size | `u32` | @ 0x004da24f |
//!
//! All little-endian. In the GOG.com data every package has version 1,
//! its first entry starts right after the directory and its last entry
//! ends at the end of the file (all 2,613 packages).

use atlantis_data::FileSource;

/// One file of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// From the start of the package file.
    pub offset: u32,
    pub size: u32,
}

/// A package's directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub entries: Vec<Entry>,
    /// Where the directory ends (the first byte after the last entry's
    /// size field).
    pub directory_end: usize,
}

/// Why a package cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WpaError {
    /// The version byte is not 1.
    Version(u8),
    /// The directory runs past the bytes given.
    Truncated,
    /// An entry lies outside the file.
    EntryOutside(String),
}

impl std::fmt::Display for WpaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WpaError::Version(v) => write!(f, "package version {v}, only 1 is read"),
            WpaError::Truncated => write!(f, "package directory truncated"),
            WpaError::EntryOutside(n) => write!(f, "package entry {n} outside the file"),
        }
    }
}

impl std::error::Error for WpaError {}

/// A little-endian reader over bytes, failing with [`WpaError::Truncated`].
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn u32(&mut self) -> Result<u32, WpaError> {
        let b = self
            .data
            .get(self.at..self.at + 4)
            .ok_or(WpaError::Truncated)?;
        self.at += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, len: usize) -> Result<&[u8], WpaError> {
        let end = self.at.checked_add(len).ok_or(WpaError::Truncated)?;
        let b = self.data.get(self.at..end).ok_or(WpaError::Truncated)?;
        self.at = end;
        Ok(b)
    }
}

/// Bytes as a string, one character per byte (the names are ASCII).
pub(crate) fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

impl Package {
    /// The directory at the start of `data` (which may be only a prefix of
    /// the file).
    pub fn parse_directory(data: &[u8]) -> Result<Package, WpaError> {
        let version = *data.first().ok_or(WpaError::Truncated)?;
        if version != 1 {
            return Err(WpaError::Version(version));
        }
        let mut c = Cursor { data, at: 1 };
        let count = c.u32()?;
        let mut entries = Vec::new();
        for _ in 0..count {
            let len = c.u32()? as usize;
            let name = latin1(c.bytes(len)?);
            let offset = c.u32()?;
            let size = c.u32()?;
            entries.push(Entry { name, offset, size });
        }
        Ok(Package {
            entries,
            directory_end: c.at,
        })
    }

    /// The whole package file `data`: its directory, every entry checked
    /// to lie inside the file.
    pub fn parse(data: &[u8]) -> Result<Package, WpaError> {
        let p = Package::parse_directory(data)?;
        for e in &p.entries {
            if u64::from(e.offset) + u64::from(e.size) > data.len() as u64 {
                return Err(WpaError::EntryOutside(e.name.clone()));
            }
        }
        Ok(p)
    }

    /// The directory of the package `name` of `src`, reading only its
    /// start.
    pub fn open(src: &dyn FileSource, name: &str) -> Option<Result<Package, WpaError>> {
        let size = src.size(name)?;
        let mut want = 4096u64;
        loop {
            let len = usize::try_from(want.min(size)).ok()?;
            let head = src.read_at(name, 0, len)?;
            match Package::parse_directory(&head) {
                Err(WpaError::Truncated) if want < size => want *= 4,
                Ok(p) => {
                    let outside = p
                        .entries
                        .iter()
                        .find(|e| u64::from(e.offset) + u64::from(e.size) > size);
                    return Some(match outside {
                        Some(e) => Err(WpaError::EntryOutside(e.name.clone())),
                        None => Ok(p),
                    });
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }

    /// The entry named `name` (exact case, the first if several).
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// The bytes of entry `e` in the package file `data`.
    pub fn slice<'a>(&self, data: &'a [u8], e: &Entry) -> Option<&'a [u8]> {
        let start = e.offset as usize;
        data.get(start..start.checked_add(e.size as usize)?)
    }

    /// The bytes of entry `e` of the package `name` of `src`.
    pub fn read(&self, src: &dyn FileSource, name: &str, e: &Entry) -> Option<Vec<u8>> {
        src.read_at(name, u64::from(e.offset), e.size as usize)
    }
}

/// What a tile of an `anim.anim` names, resolved against its package.
///
/// A tile names `X.jpg` or `X.ogv`. In the GOG.com data a `.jpg` tile is
/// either an entry of that name (the 28 atlases of a background) or the
/// pair `X.rgb.jpg` + `X.alpha.jpg` (objects), and every `.ogv` tile has its
/// `X.alpha.ogv`. The executable builds the `.rgb.` and `.alpha.` names from
/// the tile's (strings atlantis4-gog-en-2363910 @ 0x005b0c34, pushed
/// @ 0x004cf306, and @ 0x005b0a74, pushed @ 0x004cf0d6; `.alpha.ogv`
/// @ 0x005b0ea8, pushed @ 0x004d18a9); the order in which it tries the
/// names is not read yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileSource {
    /// A JPEG of that name.
    Jpeg(String),
    /// A colour JPEG and a grey alpha JPEG.
    JpegWithAlpha { rgb: String, alpha: String },
    /// A Theora colour stream and its alpha stream.
    TheoraWithAlpha { rgb: String, alpha: String },
}

impl TileSource {
    /// How the tile `name` is found in `package`, or `None` when its files
    /// are missing.
    pub fn resolve(package: &Package, name: &str) -> Option<TileSource> {
        let has = |n: &str| package.entry(n).is_some();
        let (stem, ext) = name.rsplit_once('.')?;
        match ext {
            "jpg" if has(name) => Some(TileSource::Jpeg(name.to_string())),
            "jpg" | "ogv" => {
                let rgb = if ext == "jpg" {
                    format!("{stem}.rgb.{ext}")
                } else {
                    name.to_string()
                };
                let alpha = format!("{stem}.alpha.{ext}");
                (has(&rgb) && has(&alpha)).then(|| {
                    if ext == "jpg" {
                        TileSource::JpegWithAlpha { rgb, alpha }
                    } else {
                        TileSource::TheoraWithAlpha { rgb, alpha }
                    }
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut dir = vec![1u8];
        dir.extend((entries.len() as u32).to_le_bytes());
        let dir_len: usize = 5 + entries.iter().map(|(n, _)| 12 + n.len()).sum::<usize>();
        let mut offset = dir_len;
        let mut body: Vec<u8> = Vec::new();
        for (n, d) in entries {
            dir.extend((n.len() as u32).to_le_bytes());
            dir.extend(n.as_bytes());
            dir.extend((offset as u32).to_le_bytes());
            dir.extend((d.len() as u32).to_le_bytes());
            offset += d.len();
            body.extend(*d);
        }
        dir.extend(body);
        dir
    }

    #[test]
    fn a_package_lists_its_files() {
        let data = build(&[("000000.rgb.jpg", b"abc"), ("000000.alpha.jpg", b"de")]);
        let p = Package::parse(&data).unwrap();
        assert_eq!(p.entries.len(), 2);
        let e = p.entry("000000.alpha.jpg").unwrap();
        assert_eq!(p.slice(&data, e), Some(&b"de"[..]));
        assert_eq!(p.directory_end, data.len() - 5);
        assert_eq!(
            TileSource::resolve(&p, "000000.jpg"),
            Some(TileSource::JpegWithAlpha {
                rgb: "000000.rgb.jpg".into(),
                alpha: "000000.alpha.jpg".into()
            })
        );
        assert_eq!(TileSource::resolve(&p, "000001.jpg"), None);
    }

    #[test]
    fn other_versions_and_short_files_are_refused() {
        assert_eq!(Package::parse(&[2, 0, 0, 0, 0]), Err(WpaError::Version(2)));
        assert_eq!(Package::parse(&[1, 1, 0, 0, 0]), Err(WpaError::Truncated));
        let mut data = build(&[("a", b"xyz")]);
        data.pop();
        assert_eq!(
            Package::parse(&data),
            Err(WpaError::EntryOutside("a".into()))
        );
    }
}
