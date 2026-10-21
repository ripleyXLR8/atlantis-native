//! The game read straight from images of its CDs: ISO 9660 file systems in
//! `.iso` images (2048-byte sectors), raw `.bin` / `.img` / `.mdf` images
//! (2352-byte sectors, user data after a 16-byte Mode 1 or 24-byte Mode 2
//! header) and Nero images (a 300 KiB lead-in before the sectors). The
//! layout is found by the ISO 9660 primary volume descriptor ("CD001" in
//! sector 16). The CD release puts one BIGCD archive on each of its four
//! discs: a [`DiscSet`] serves the files of all the images as one folder.
//! Compressed images (zip, 7z, chd) are not read.

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

/// A file of the image: its first sector and size.
#[derive(Clone, Copy, Debug)]
struct Extent {
    lba: u64,
    size: u64,
}

/// One CD image.
pub struct DiscImage {
    path: PathBuf,
    file: Mutex<File>,
    lead_in: u64,
    sector_size: u64,
    data_offset: u64,
    /// Lower-case path ("dir/name.ext") -> extent.
    files: HashMap<String, Extent>,
    dirs: Vec<String>,
}

fn read_exact_at(file: &Mutex<File>, offset: u64, buf: &mut [u8]) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = file.lock().unwrap();
    f.seek(SeekFrom::Start(offset)).is_ok() && f.read_exact(buf).is_ok()
}

impl DiscImage {
    /// Opens an image whose layout holds an ISO 9660 file system.
    pub fn open(path: &Path) -> Option<DiscImage> {
        let file = Mutex::new(File::open(path).ok()?);
        for (lead_in, sector_size, data_offset) in LAYOUTS {
            let mut pvd = [0u8; 2048];
            let at = lead_in + 16 * sector_size + data_offset;
            if !read_exact_at(&file, at, &mut pvd) || pvd[0] != 1 || &pvd[1..6] != b"CD001" {
                continue;
            }
            let mut img = DiscImage {
                path: path.to_path_buf(),
                file,
                lead_in,
                sector_size,
                data_offset,
                files: HashMap::new(),
                dirs: Vec::new(),
            };
            let root = Extent {
                lba: u64::from(u32::from_le_bytes(pvd[158..162].try_into().ok()?)),
                size: u64::from(u32::from_le_bytes(pvd[166..170].try_into().ok()?)),
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
            let mut name = String::from_utf8_lossy(raw).to_ascii_lowercase();
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
                if depth < 3 {
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
            return read_exact_at(&self.file, self.lead_in + start, &mut out).then_some(out);
        }
        // Raw sectors: the 2048 user bytes of each one in turn.
        let mut done = 0usize;
        while done < len {
            let pos = start + done as u64;
            let (sector, within) = (pos / SECTOR, pos % SECTOR);
            let n = ((SECTOR - within) as usize).min(len - done);
            let at = self.lead_in + sector * self.sector_size + self.data_offset + within;
            if !read_exact_at(&self.file, at, &mut out[done..done + n]) {
                return None;
            }
            done += n;
        }
        Some(out)
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

    fn has_dir(&self, name: &str) -> bool {
        let k = Self::key(name);
        self.dirs.contains(&k)
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

    fn has_dir(&self, name: &str) -> bool {
        self.images.iter().any(|i| i.has_dir(name))
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
