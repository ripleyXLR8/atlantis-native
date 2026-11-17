//! The game read straight from images of its CDs: ISO 9660 file systems in
//! `.iso` images (2048-byte sectors), raw `.bin` / `.img` / `.mdf` images
//! (2352-byte sectors, user data after a 16-byte Mode 1 or 24-byte Mode 2
//! header) and Nero images (a 300 KiB lead-in before the sectors). The
//! layout is found by the ISO 9660 primary volume descriptor ("CD001" in
//! sector 16). Long names are read from the Joliet tree when the image has
//! one (a supplementary volume descriptor with a UCS-2 escape), as Windows
//! does; Atlantis II's discs need it, their ISO 9660 names being 8.3
//! (`SNA100~1.SPW`). The CD release puts one BIGCD archive on each of its
//! four discs: a [`DiscSet`] serves the files of all the images as one
//! folder.
//! Compressed images (zip, 7z, chd) are not read.
//!
//! An image's bytes come from an [`ImageBytes`]: its file on the desktop,
//! a file the player picked in the browser (read by parts, never whole),
//! so that both read the same file system the same way.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::files::FileSource;

const SECTOR: u64 = 2048;

/// The sector layouts tried: (bytes before sector 0, sector size, user
/// data offset in a sector).
const LAYOUTS: [(u64, u64, u64); 6] = [
    (0, 2048, 0),
    (0, 2352, 16),
    (0, 2352, 24),
    (0, 2336, 8),
    (307_200, 2048, 0),
    (352_800, 2352, 16),
];

/// The bytes of an image, read by parts: its file on the desktop, a
/// picked file in the browser.
pub trait ImageBytes: Send + Sync {
    /// Fills `buf` with the bytes at `offset`; false when they cannot all
    /// be read.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool;

    /// A hint that bytes `offset..offset + len` are read next
    /// ([`FileSource::read_ahead`] of a file of the image, brought to the
    /// image's bytes). Nothing by default.
    fn read_ahead(&self, offset: u64, len: u64) {
        let _ = (offset, len);
    }
}

/// An image's file on disk.
struct FileBytes(Mutex<File>);

impl ImageBytes for FileBytes {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = self.0.lock().unwrap();
        f.seek(SeekFrom::Start(offset)).is_ok() && f.read_exact(buf).is_ok()
    }
}

/// An image held in memory (tests, small images).
impl ImageBytes for Vec<u8> {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool {
        let Ok(start) = usize::try_from(offset) else {
            return false;
        };
        match self.get(start..start.saturating_add(buf.len())) {
            Some(src) => {
                buf.copy_from_slice(src);
                true
            }
            None => false,
        }
    }
}

/// Raw sectors read at once at most (2.3 MB of 2352-byte sectors): a
/// larger read of a raw image is made in parts of this many.
const RAW_BATCH: u64 = 1024;

/// A file of the image: its first sector and size.
#[derive(Clone, Copy, Debug)]
struct Extent {
    lba: u64,
    size: u64,
}

/// One CD image.
pub struct DiscImage {
    path: PathBuf,
    bytes: Box<dyn ImageBytes>,
    lead_in: u64,
    sector_size: u64,
    data_offset: u64,
    /// Lower-case path ("dir/name.ext") -> extent.
    files: HashMap<String, Extent>,
    dirs: Vec<String>,
    /// Names are UCS-2 (big-endian): the Joliet tree.
    joliet: bool,
}

impl DiscImage {
    /// Opens an image whose layout holds an ISO 9660 file system.
    pub fn open(path: &Path) -> Option<DiscImage> {
        let file = FileBytes(Mutex::new(File::open(path).ok()?));
        DiscImage::from_bytes(path, Box::new(file))
    }

    /// The image whose bytes `bytes` gives (`path` names it: its file, or
    /// the picked file's path in the browser), if its layout holds an ISO
    /// 9660 file system. Only the descriptors and the directories are read
    /// here.
    pub fn from_bytes(path: impl Into<PathBuf>, bytes: Box<dyn ImageBytes>) -> Option<DiscImage> {
        for (lead_in, sector_size, data_offset) in LAYOUTS {
            let mut pvd = [0u8; 2048];
            let at = lead_in + 16 * sector_size + data_offset;
            if !bytes.read_at(at, &mut pvd) || pvd[0] != 1 || &pvd[1..6] != b"CD001" {
                continue;
            }
            let mut img = DiscImage {
                path: path.into(),
                bytes,
                lead_in,
                sector_size,
                data_offset,
                files: HashMap::new(),
                dirs: Vec::new(),
                joliet: false,
            };
            // A Joliet supplementary descriptor after the primary one (up to
            // the set terminator) gives the long names.
            let mut vd = pvd;
            for n in 17..32u64 {
                let mut d = [0u8; 2048];
                let at = lead_in + n * sector_size + data_offset;
                if !img.bytes.read_at(at, &mut d) || &d[1..6] != b"CD001" || d[0] == 255 {
                    break;
                }
                if d[0] == 2
                    && d[88] == b'%'
                    && d[89] == b'/'
                    && matches!(d[90], b'@' | b'C' | b'E')
                {
                    vd = d;
                    img.joliet = true;
                    break;
                }
            }
            let root = Extent {
                lba: u64::from(u32::from_le_bytes(vd[158..162].try_into().ok()?)),
                size: u64::from(u32::from_le_bytes(vd[166..170].try_into().ok()?)),
            };
            img.walk(root, "", 0);
            return Some(img);
        }
        None
    }

    /// The image's file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory records of `dir` (recursively, a few levels).
    fn walk(&mut self, dir: Extent, prefix: &str, depth: usize) {
        let Some(data) = self.read_extent(dir, 0, dir.size.min(1 << 20) as usize) else {
            return;
        };
        let mut pos = 0usize;
        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                // Records do not cross sectors: the rest of this one is empty.
                pos = (pos / SECTOR as usize + 1) * SECTOR as usize;
                continue;
            }
            let Some(rec) = data.get(pos..pos + len) else {
                break;
            };
            pos += len;
            if rec.len() < 34 {
                continue;
            }
            let name_len = rec[32] as usize;
            let Some(raw) = rec.get(33..33 + name_len) else {
                continue;
            };
            if raw == [0] || raw == [1] {
                continue; // "." and ".."
            }
            let mut name = if self.joliet {
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_be_bytes(c))
                    .collect();
                String::from_utf16_lossy(&units).to_ascii_lowercase()
            } else {
                String::from_utf8_lossy(raw).to_ascii_lowercase()
            };
            if let Some(i) = name.find(';') {
                name.truncate(i);
            }
            let name = name.trim_end_matches('.').to_string();
            let ext = Extent {
                lba: u64::from(u32::from_le_bytes(rec[2..6].try_into().unwrap())),
                size: u64::from(u32::from_le_bytes(rec[10..14].try_into().unwrap())),
            };
            let full = format!("{prefix}{name}");
            if rec[25] & 2 != 0 {
                self.dirs.push(full.clone());
                if depth < 8 {
                    self.walk(ext, &format!("{full}/"), depth + 1);
                }
            } else {
                self.files.insert(full, ext);
            }
        }
    }

    /// `len` bytes at `offset` of the extent (fewer at its end).
    fn read_extent(&self, ext: Extent, offset: u64, len: usize) -> Option<Vec<u8>> {
        if offset > ext.size {
            return None;
        }
        let len = (len as u64).min(ext.size - offset) as usize;
        let mut out = vec![0u8; len];
        let start = ext.lba * SECTOR + offset;
        if self.sector_size == SECTOR && self.data_offset == 0 {
            return self
                .bytes
                .read_at(self.lead_in + start, &mut out)
                .then_some(out);
        }
        // Raw sectors: the sectors spanned read together (by batches of
        // `RAW_BATCH`), the 2048 user bytes of each one taken in turn.
        let end = start + len as u64;
        let mut pos = start;
        while pos < end {
            let first = pos / SECTOR;
            let batch_end = end.min((first + RAW_BATCH) * SECTOR);
            let (from, to) = self.raw_span(pos, batch_end);
            let mut raw = vec![0u8; usize::try_from(to - from).ok()?];
            if !self.bytes.read_at(from, &mut raw) {
                return None;
            }
            while pos < batch_end {
                let (sector, within) = (pos / SECTOR, pos % SECTOR);
                let n = (SECTOR - within).min(batch_end - pos) as usize;
                let at = (self.raw_offset(sector) + within - from) as usize;
                let done = (pos - start) as usize;
                out[done..done + n].copy_from_slice(&raw[at..at + n]);
                pos += n as u64;
            }
        }
        Some(out)
    }

    /// The image offset of sector `sector`'s user data.
    fn raw_offset(&self, sector: u64) -> u64 {
        self.lead_in + sector * self.sector_size + self.data_offset
    }

    /// The image's bytes that hold user bytes `start..end` (`end` >
    /// `start`): from the first one's to the last one's.
    fn raw_span(&self, start: u64, end: u64) -> (u64, u64) {
        let last = (end - 1) / SECTOR;
        (
            self.raw_offset(start / SECTOR) + start % SECTOR,
            self.raw_offset(last) + (end - 1) % SECTOR + 1,
        )
    }

    fn key(name: &str) -> String {
        name.replace('\\', "/")
            .trim_matches('/')
            .to_ascii_lowercase()
    }

    /// Whether the image holds `name` (any case).
    pub fn has_file(&self, name: &str) -> bool {
        self.files.contains_key(&Self::key(name))
    }
}

impl FileSource for DiscImage {
    fn size(&self, name: &str) -> Option<u64> {
        self.files.get(&Self::key(name)).map(|e| e.size)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let ext = *self.files.get(&Self::key(name))?;
        let data = self.read_extent(ext, offset, len)?;
        (data.len() == len).then_some(data)
    }

    /// Passed on to the image's bytes, as the bytes that hold the part:
    /// the same span a read of the part then reads (a raw image's part of
    /// at most `RAW_BATCH` sectors in one read).
    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        let Some(&ext) = self.files.get(&Self::key(name)) else {
            return;
        };
        let len = (len as u64).min(ext.size.saturating_sub(offset));
        if len == 0 {
            return;
        }
        let start = ext.lba * SECTOR + offset;
        let (from, to) = self.raw_span(start, start + len);
        self.bytes.read_ahead(from, to - from);
    }

    fn has_dir(&self, name: &str) -> bool {
        let k = Self::key(name);
        self.dirs.contains(&k)
    }

    /// The entries of folder `dir` (lower case).
    fn list(&self, dir: &str) -> Vec<String> {
        let k = Self::key(dir);
        let prefix = if k.is_empty() { k } else { k + "/" };
        let mut out: Vec<String> = self
            .files
            .keys()
            .chain(self.dirs.iter())
            .filter_map(|p| p.strip_prefix(&prefix))
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .map(str::to_string)
            .collect();
        out.sort();
        out
    }
}

/// The images of a game's discs, read as one folder: each file from the
/// first image that holds it (the images in name order, disc 1 first).
pub struct DiscSet {
    pub images: Vec<DiscImage>,
}

/// The extensions of the images [`DiscSet::open_folder`] looks at.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["iso", "bin", "img", "mdf", "nrg"];

impl DiscSet {
    /// The CD images in `dir` (those holding an ISO 9660 file system),
    /// `None` when there are none.
    pub fn open_folder(dir: &Path) -> Option<DiscSet> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| {
                        IMAGE_EXTENSIONS
                            .contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
                    })
            })
            .collect();
        paths.sort();
        let images: Vec<DiscImage> = paths.iter().filter_map(|p| DiscImage::open(p)).collect();
        (!images.is_empty()).then_some(DiscSet { images })
    }

    fn holder(&self, name: &str) -> Option<&DiscImage> {
        self.images.iter().find(|i| i.has_file(name))
    }
}

impl FileSource for DiscSet {
    fn size(&self, name: &str) -> Option<u64> {
        self.holder(name)?.size(name)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.holder(name)?.read_at(name, offset, len)
    }

    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        if let Some(i) = self.holder(name) {
            i.read_ahead(name, offset, len);
        }
    }

    fn has_dir(&self, name: &str) -> bool {
        self.images.iter().any(|i| i.has_dir(name))
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let mut out: Vec<String> = self.images.iter().flat_map(|i| i.list(dir)).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The file in the images' folder (not in the images).
    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        let dir = self.images.first()?.path().parent()?;
        crate::files::StdDir::new(dir).read(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ISO 9660 image: the root holds GAME/ATLANTIS.EXE ("hello")
    /// and BIGCD1.BIG (3000 bytes, crossing a sector).
    fn iso_sectors() -> Vec<[u8; 2048]> {
        let mut s = vec![[0u8; 2048]; 23];
        let rec = |name: &[u8], lba: u32, size: u32, dir: bool| -> Vec<u8> {
            let mut r = vec![0u8; 33 + name.len() + (name.len() + 1) % 2];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&lba.to_le_bytes());
            r[10..14].copy_from_slice(&size.to_le_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        s[16][0] = 1;
        s[16][1..6].copy_from_slice(b"CD001");
        let root = rec(&[0], 18, 2048, true);
        s[16][156..156 + root.len()].copy_from_slice(&root);
        s[17][0] = 255;
        s[17][1..6].copy_from_slice(b"CD001");
        let mut dir = Vec::new();
        dir.extend(rec(&[0], 18, 2048, true));
        dir.extend(rec(&[1], 18, 2048, true));
        dir.extend(rec(b"BIGCD1.BIG;1", 20, 3000, false));
        dir.extend(rec(b"GAME", 19, 2048, true));
        s[18][..dir.len()].copy_from_slice(&dir);
        let mut sub = Vec::new();
        sub.extend(rec(&[0], 19, 2048, true));
        sub.extend(rec(&[1], 18, 2048, true));
        sub.extend(rec(b"ATLANTIS.EXE;1", 22, 5, false));
        s[19][..sub.len()].copy_from_slice(&sub);
        for i in 0..3000 {
            s[20 + i / 2048][i % 2048] = (i % 251) as u8;
        }
        s[22][..5].copy_from_slice(b"hello");
        s
    }

    fn write(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("atlantis-disc-{}-{name}", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn check(img: &DiscImage) {
        assert_eq!(img.read("game/atlantis.exe").unwrap(), b"hello");
        let big = img.read("BIGCD1.BIG").unwrap();
        assert_eq!(big.len(), 3000);
        assert!(big.iter().enumerate().all(|(i, &b)| b == (i % 251) as u8));
        assert_eq!(
            img.read_at("bigcd1.big", 2040, 20).unwrap(),
            big[2040..2060]
        );
        assert!(img.has_dir("GAME"));
        assert_eq!(img.size("nothing"), None);
        assert_eq!(img.list(""), ["bigcd1.big", "game"]);
        assert_eq!(img.list("GAME"), ["atlantis.exe"]);
    }

    #[test]
    fn iso_and_raw_images() {
        let sectors = iso_sectors();
        let iso: Vec<u8> = sectors.iter().flatten().copied().collect();
        let p = write("a.iso", &iso);
        check(&DiscImage::open(&p).expect("2048-byte image"));
        // The same sectors raw: sync and header, data, EDC/ECC.
        let mut raw = Vec::new();
        for s in &sectors {
            raw.extend_from_slice(&[0u8; 16]);
            raw.extend_from_slice(s);
            raw.extend_from_slice(&[0u8; 288]);
        }
        let q = write("b.bin", &raw);
        check(&DiscImage::open(&q).expect("2352-byte image"));
        // Nero: a 300 KiB lead-in.
        let mut nrg = vec![0u8; 307_200];
        nrg.extend_from_slice(&iso);
        let r = write("c.nrg", &nrg);
        check(&DiscImage::open(&r).expect("Nero image"));
        assert!(DiscImage::open(&write("d.bin", &[0u8; 100_000])).is_none());
        for f in [p, q, r] {
            let _ = std::fs::remove_file(f);
        }
    }

    /// Joliet: the supplementary tree's long names, as Windows reads them.
    #[test]
    fn joliet_long_names() {
        let mut s = iso_sectors();
        s.resize(26, [0u8; 2048]);
        let rec = |name: &[u8], lba: u32, size: u32, dir: bool| -> Vec<u8> {
            let mut r = vec![0u8; 33 + name.len() + (name.len() + 1) % 2];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&lba.to_le_bytes());
            r[10..14].copy_from_slice(&size.to_le_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        let mut svd = [0u8; 2048];
        svd[0] = 2;
        svd[1..6].copy_from_slice(b"CD001");
        svd[88..91].copy_from_slice(b"%/E");
        let root = rec(&[0], 24, 2048, true);
        svd[156..156 + root.len()].copy_from_slice(&root);
        s[17] = svd;
        let long: Vec<u8> = "Datas_A2"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let file: Vec<u8> = "sna1013013.SPW;1"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let mut dir = Vec::new();
        dir.extend(rec(&[0], 24, 2048, true));
        dir.extend(rec(&[1], 24, 2048, true));
        dir.extend(rec(&long, 23, 2048, true));
        s[24][..dir.len()].copy_from_slice(&dir);
        let mut sub = Vec::new();
        sub.extend(rec(&[0], 23, 2048, true));
        sub.extend(rec(&[1], 24, 2048, true));
        sub.extend(rec(&file, 25, 4, false));
        s[23][..sub.len()].copy_from_slice(&sub);
        s[25][..4].copy_from_slice(b"long");
        let iso: Vec<u8> = s.iter().flatten().copied().collect();
        let p = write("joliet.iso", &iso);
        let img = DiscImage::open(&p).expect("image");
        assert_eq!(
            img.read("DATAS_A2/SNA1013013.spw").as_deref(),
            Some(&b"long"[..])
        );
        // The primary tree's names are not used then.
        assert_eq!(img.size("game/atlantis.exe"), None);
        let _ = std::fs::remove_file(p);
    }

    /// An image read through a source that gives its bytes by parts (the
    /// browser's picked file): every layout, the reads it is asked for
    /// recorded.
    struct Parts {
        data: Vec<u8>,
        reads: Mutex<Vec<(u64, usize)>>,
        ahead: Mutex<Vec<(u64, u64)>>,
    }

    impl ImageBytes for std::sync::Arc<Parts> {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool {
            self.reads.lock().unwrap().push((offset, buf.len()));
            self.data.read_at(offset, buf)
        }

        fn read_ahead(&self, offset: u64, len: u64) {
            self.ahead.lock().unwrap().push((offset, len));
        }
    }

    /// The test image with a large file (`big.dat`, 5 MB from sector 23,
    /// over `RAW_BATCH` sectors), in the layout `(lead_in, sector size,
    /// data offset)`.
    fn layout_image(layout: (u64, u64, u64)) -> (Vec<u8>, Vec<u8>) {
        let (lead_in, size, offset) = layout;
        let big_len = 5 << 20;
        let mut sectors = iso_sectors();
        // The root's record of big.dat, after the others.
        let mut r = vec![0u8; 34 + 8];
        r[0] = r.len() as u8;
        r[2..6].copy_from_slice(&23u32.to_le_bytes());
        r[10..14].copy_from_slice(&(big_len as u32).to_le_bytes());
        r[32] = 7;
        r[33..40].copy_from_slice(b"BIG.DAT");
        let used = sectors[18].iter().rposition(|&b| b != 0).unwrap() + 2;
        sectors[18][used..used + r.len()].copy_from_slice(&r);
        let big: Vec<u8> = (0..big_len).map(|i| (i * 7 % 253) as u8).collect();
        for c in big.chunks(2048) {
            let mut s = [0u8; 2048];
            s[..c.len()].copy_from_slice(c);
            sectors.push(s);
        }
        let mut img = vec![0xAAu8; lead_in as usize];
        for s in &sectors {
            img.extend(std::iter::repeat_n(0x55u8, offset as usize));
            img.extend_from_slice(s);
            img.extend(std::iter::repeat_n(0x33u8, (size - 2048 - offset) as usize));
        }
        (img, big)
    }

    /// Read by parts, every layout gives the bytes the whole file holds;
    /// no read of the image is larger than a raw batch, and a part read
    /// ahead is the span its read then reads.
    #[test]
    fn reads_by_parts_give_the_same_bytes() {
        for layout in LAYOUTS {
            let (img, big) = layout_image(layout);
            let parts = std::sync::Arc::new(Parts {
                data: img,
                reads: Mutex::new(Vec::new()),
                ahead: Mutex::new(Vec::new()),
            });
            let disc = DiscImage::from_bytes("t.iso", Box::new(parts.clone())).expect("image");
            assert_eq!(disc.read("game/atlantis.exe").unwrap(), b"hello");
            assert_eq!(
                disc.read("big.dat").as_deref(),
                Some(&big[..]),
                "{layout:?}"
            );
            for part in [1usize, 1000, 2048, 4096, 65536, 512 << 10] {
                // The small parts over the first sectors only.
                let whole = if part < 2048 {
                    &big[..20_000]
                } else {
                    &big[..]
                };
                let mut joined = Vec::new();
                let mut at = 0;
                while at < whole.len() {
                    let n = part.min(whole.len() - at);
                    joined.extend(disc.read_at("BIG.DAT", at as u64, n).unwrap());
                    at += n;
                }
                assert!(joined == whole, "{layout:?} by {part}");
            }
            // A part read ahead, then read: one read, the span announced.
            parts.reads.lock().unwrap().clear();
            disc.read_ahead("big.dat", 3000, 100_000);
            let ahead = parts.ahead.lock().unwrap()[0];
            assert_eq!(
                disc.read_at("big.dat", 3000, 100_000).unwrap(),
                big[3000..103_000]
            );
            assert_eq!(
                *parts.reads.lock().unwrap(),
                [(ahead.0, ahead.1 as usize)],
                "{layout:?}"
            );
            parts.reads.lock().unwrap().clear();
            let _ = disc.read("big.dat");
            let largest = parts
                .reads
                .lock()
                .unwrap()
                .iter()
                .map(|r| r.1)
                .max()
                .unwrap();
            if layout.1 != 2048 {
                assert!(largest as u64 <= RAW_BATCH * layout.1, "{layout:?}");
            }
            // Past the end: nothing.
            assert_eq!(disc.read_at("big.dat", big.len() as u64 - 10, 11), None);
        }
    }

    /// A file of the player's beside the images is read from the folder,
    /// not from the images.
    #[test]
    fn read_beside_the_images() {
        let dir = std::env::temp_dir().join(format!("atlantis-beside-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let iso: Vec<u8> = iso_sectors().iter().flatten().copied().collect();
        std::fs::write(dir.join("disc1.iso"), &iso).unwrap();
        std::fs::write(dir.join("MingLiU.TTC"), b"font").unwrap();
        let set = DiscSet::open_folder(&dir).expect("one image");
        assert_eq!(
            set.read_beside("mingliu.ttc").as_deref(),
            Some(&b"font"[..])
        );
        assert_eq!(set.read("mingliu.ttc"), None);
        assert_eq!(set.read_beside("simsun.ttc"), None);
        assert_eq!(
            set.read("game/atlantis.exe").as_deref(),
            Some(&b"hello"[..])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
