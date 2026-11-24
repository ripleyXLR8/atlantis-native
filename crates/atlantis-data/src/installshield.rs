//! InstallShield cabinets (`ISc(`, the format of InstallShield 6 and
//! later's `data1.hdr` + `data1.cab` ... `dataN.cab`) read in place: the
//! files of the groups an installation copies, served as a
//! [`FileSource`], with no install step.
//!
//! *The Secrets of Atlantis*'s North-American CDs (2007) hold the game's
//! data in such a set, its volumes spread over the three discs. Only what
//! the format's public description (the `unshield` project's notes on
//! the "version 5 and later" cabinets) gives and those cabinets use is
//! read: major versions 6 and later (0x57-byte file descriptors), files
//! stored or compressed, a file split across volumes, files sharing
//! another's data (links). An obfuscated file (flag 2) is refused: no
//! cabinet seen here has one, so the scheme is not implemented untested.
//!
//! Layout (all little-endian; offsets in the descriptor are from its start):
//!
//! | Part | Fields |
//! |---|---|
//! | Common header (20 bytes, at 0 of every `.hdr` and `.cab`) | `ISc(`, u32 version, u32 volume info, u32 offset of the cabinet descriptor, u32 its size |
//! | Volume header (`.cab`, 64 bytes after the common header) | u64 data offset, u32 first and last file index, then for the first file and the last file: u64 offset, u64 expanded size, u64 compressed size (each u64 as two u32, low first) — the first file's part when it continues from the volume before, the last file's when it goes on in the next |
//! | Cabinet descriptor (`.hdr`) | +0x0c u32 file table's offset, +0x1c u32 directories, +0x28 u32 files, +0x2c u32 descriptors' offset in the file table, +0x3e 71 u32 heads of the file groups' lists |
//! | File table | u32 per directory: its name's offset (from the table); the descriptors at the second offset, 0x57 bytes each |
//! | File descriptor | +0x00 u16 flags (1 split, 2 obfuscated, 4 compressed, 8 invalid), +0x02 u64 expanded size, +0x0a u64 compressed size, +0x12 u64 data offset, +0x1a MD5 of the file, +0x3a u32 name offset, +0x3e u16 directory, +0x4c u32 previous link, +0x50 u32 next link, +0x54 u8 link flags (1: shares the previous file's data), +0x55 u16 volume |
//! | File group list entry | u32 name offset, u32 descriptor offset, u32 next entry |
//! | File group descriptor | u32 name offset, 0x12 bytes, u32 first file, u32 last file |
//!
//! A file split across volumes takes, in the volume it starts in, the
//! last file's part (its compressed size there for a compressed file;
//! for a stored file the "expanded" field, as the format's description
//! reads it: no stored file is split in the cabinets seen here), and in
//! each next volume the first file's part.
//!
//! A compressed file is a run of chunks, each a u16 size and that many
//! bytes of raw deflate (a complete stream of its own, 32 KiB of output in
//! the files seen here); a split file's chunks run on across the volume's
//! end into the next volume. Reading a file at an offset decodes its
//! chunks from its start the first time (the sizes of their output are
//! known only once decoded), then keeps their places.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::files::FileSource;

/// The common header's signature.
const MAGIC: &[u8; 4] = b"ISc(";
/// The file descriptor's size (major version 6 and later).
const DESCRIPTOR: usize = 0x57;
/// The heads of the file groups' lists in the cabinet descriptor.
const GROUP_HEADS: usize = 71;

/// File flags.
pub const FILE_SPLIT: u16 = 1;
pub const FILE_OBFUSCATED: u16 = 2;
pub const FILE_COMPRESSED: u16 = 4;
pub const FILE_INVALID: u16 = 8;
/// Link flag: the file's data are the previous linked file's.
pub const LINK_PREV: u8 = 1;

/// A chunk's output is at most this big (a damaged chunk is refused, not
/// decoded into memory without end).
const MAX_CHUNK_OUT: usize = 1 << 20;

/// The volumes are read by aligned parts of this many bytes, the last
/// [`PARTS_KEPT`] kept (a chunk's u16 size is a 2-byte read).
pub const PART: u64 = 1 << 20;
const PARTS_KEPT: usize = 16;
/// The files whose chunk places are kept.
const STATES_KEPT: usize = 256;

/// Why a set is not one this module reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IsError {
    /// No `<prefix>1.hdr` with the signature.
    NotInstallShield,
    Truncated,
    /// A major version this module does not read (before 6).
    Version(u32),
}

impl std::fmt::Display for IsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsError::NotInstallShield => write!(f, "not an InstallShield cabinet"),
            IsError::Truncated => write!(f, "the InstallShield header is truncated"),
            IsError::Version(v) => write!(f, "InstallShield cabinets of version {v} are not read"),
        }
    }
}

impl std::error::Error for IsError {}

/// One file of the set, as its descriptor says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsFile {
    /// Its index in the file table.
    pub index: u32,
    /// Its directory (`datas\Scenes`; empty for the install folder's top).
    pub dir: String,
    pub name: String,
    pub flags: u16,
    pub size: u64,
    pub compressed: u64,
    pub data_offset: u64,
    pub md5: [u8; 16],
    pub volume: u16,
    pub link_previous: u32,
    pub link_flags: u8,
}

impl IsFile {
    /// `dir\name`, or `name` at the top.
    pub fn path(&self) -> String {
        if self.dir.is_empty() {
            self.name.clone()
        } else {
            format!("{}\\{}", self.dir, self.name)
        }
    }
}

/// A file group: its name and its files' indexes (first..=last).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileGroup {
    pub name: String,
    pub first: u32,
    pub last: u32,
}

impl FileGroup {
    /// Whether the installation copies its files as the product's (the
    /// setup's own groups are named `<Support>...`, `<Disk1>...`,
    /// `<Engine>...`).
    pub fn installs(&self) -> bool {
        !self.name.starts_with('<')
    }
}

/// A volume's header (major version 6 and later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeHeader {
    pub data_offset: u64,
    pub first_file: u32,
    pub last_file: u32,
    pub first_offset: u64,
    pub first_expanded: u64,
    pub first_compressed: u64,
    pub last_offset: u64,
    pub last_expanded: u64,
    pub last_compressed: u64,
}

impl VolumeHeader {
    fn parse(d: &[u8]) -> Option<VolumeHeader> {
        let u = |i: usize| u32_at(d, 20 + 4 * i).map(u64::from);
        let q = |i: usize| Some(u(i)? | (u(i + 1)? << 32));
        Some(VolumeHeader {
            data_offset: q(0)?,
            first_file: u32_at(d, 28)?,
            last_file: u32_at(d, 32)?,
            first_offset: q(4)?,
            first_expanded: q(6)?,
            first_compressed: q(8)?,
            last_offset: q(10)?,
            last_expanded: q(12)?,
            last_compressed: q(14)?,
        })
    }
}

/// A part of a file's stored bytes: (volume, offset, length).
type Segment = (u16, u64, u64);

/// A volume found: its name, size and header.
type Volume = (String, u64, VolumeHeader);

/// A part of a volume read: (volume, part number, bytes).
type Part = (u16, u64, Arc<Vec<u8>>);

/// What is known of a compressed file's chunks.
#[derive(Default)]
struct FileState {
    /// Per chunk walked: its place in the stored bytes (its u16 size's)
    /// and its output's offset in the file.
    chunks: Vec<(u64, u64)>,
    /// The stored bytes walked so far (the next chunk's place).
    walked: u64,
    /// The output of the chunks walked.
    out_end: u64,
    /// The chunks decoded last (index, output), the latest last.
    decoded: VecDeque<(usize, Arc<Vec<u8>>)>,
    /// Use counter, for dropping the least used states.
    used: u64,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// The NUL-terminated string at `at` (the system's code page: each byte
/// a character, as the cabinets' names are ASCII).
fn c_string(d: &[u8], at: usize) -> Option<String> {
    let rest = d.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    Some(rest[..end].iter().map(|&b| char::from(b)).collect())
}

/// The major version of the common header's version field (as the format's
/// description computes it).
pub fn major_version(version: u32) -> u32 {
    match version >> 24 {
        1 => (version >> 12) & 0xf,
        2 | 4 => (version & 0xffff) / 100,
        _ => 0,
    }
}

/// The cabinet set's name prefix in `src`'s top folder: `data` for
/// `data1.hdr` (any case), the shortest such name first.
pub fn header_name(src: &dyn FileSource) -> Option<String> {
    if src.size("data1.hdr").is_some() {
        return Some("data1.hdr".to_string());
    }
    let mut names: Vec<String> = src
        .list("")
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with("1.hdr"))
        .collect();
    names.sort_by_key(|n| (n.len(), n.to_ascii_lowercase()));
    names.into_iter().next()
}

/// An InstallShield cabinet set read through `src`: `<prefix>1.hdr` and
/// `<prefix><n>.cab` found there by name.
pub struct InstallShield {
    src: Arc<dyn FileSource>,
    /// `data` for `data1.hdr`.
    prefix: String,
    pub major: u32,
    files: Vec<IsFile>,
    groups: Vec<FileGroup>,
    /// Volume number → its header, `None` when the volume is missing or
    /// unreadable; filled on first use.
    volumes: Mutex<HashMap<u16, Option<Volume>>>,
    /// The volumes' parts read last: (volume, part number, bytes).
    parts: Mutex<VecDeque<Part>>,
    states: Mutex<HashMap<u32, Arc<Mutex<FileState>>>>,
    clock: std::sync::atomic::AtomicU64,
}

impl InstallShield {
    /// The set whose header is in `src`'s top folder ([`header_name`]).
    pub fn open(src: Arc<dyn FileSource>) -> Result<InstallShield, IsError> {
        let hdr = header_name(&*src).ok_or(IsError::NotInstallShield)?;
        Self::open_header(src, &hdr)
    }

    /// The set whose header is `hdr` (`data1.hdr`) in `src`.
    pub fn open_header(src: Arc<dyn FileSource>, hdr: &str) -> Result<InstallShield, IsError> {
        let t = IsError::Truncated;
        let size = src.size(hdr).ok_or(IsError::NotInstallShield)?;
        let head = src
            .read_at(hdr, 0, 20.min(size as usize))
            .ok_or(t.clone())?;
        if head.len() < 20 || &head[..4] != MAGIC {
            return Err(IsError::NotInstallShield);
        }
        let major = major_version(u32_at(&head, 4).ok_or(t.clone())?);
        if major < 6 {
            return Err(IsError::Version(major));
        }
        let desc_at = u64::from(u32_at(&head, 12).ok_or(t.clone())?);
        let len =
            usize::try_from(size.checked_sub(desc_at).ok_or(t.clone())?).map_err(|_| t.clone())?;
        let d = src.read_at(hdr, desc_at, len).ok_or(t.clone())?;
        let table_at = u32_at(&d, 0x0c).ok_or(t.clone())? as usize;
        let n_dirs = u32_at(&d, 0x1c).ok_or(t.clone())? as usize;
        let n_files = u32_at(&d, 0x28).ok_or(t.clone())? as usize;
        let desc_off = u32_at(&d, 0x2c).ok_or(t.clone())? as usize;
        let table = d.get(table_at..).ok_or(t.clone())?;
        let mut dirs = Vec::with_capacity(n_dirs);
        for i in 0..n_dirs {
            let at = u32_at(table, 4 * i).ok_or(t.clone())? as usize;
            dirs.push(c_string(table, at).ok_or(t.clone())?);
        }
        let mut files = Vec::with_capacity(n_files);
        for i in 0..n_files {
            let o = desc_off + i * DESCRIPTOR;
            let r = table.get(o..o + DESCRIPTOR).ok_or(t.clone())?;
            let name_at = u32_at(r, 0x3a).ok_or(t.clone())? as usize;
            let dir = usize::from(u16_at(r, 0x3e).ok_or(t.clone())?);
            let name = if name_at == 0 {
                String::new()
            } else {
                c_string(table, name_at).ok_or(t.clone())?
            };
            files.push(IsFile {
                index: i as u32,
                dir: dirs.get(dir).cloned().unwrap_or_default(),
                name,
                flags: u16_at(r, 0).ok_or(t.clone())?,
                size: u64_at(r, 0x02).ok_or(t.clone())?,
                compressed: u64_at(r, 0x0a).ok_or(t.clone())?,
                data_offset: u64_at(r, 0x12).ok_or(t.clone())?,
                md5: r[0x1a..0x2a].try_into().map_err(|_| t.clone())?,
                link_previous: u32_at(r, 0x4c).ok_or(t.clone())?,
                link_flags: r[0x54],
                volume: u16_at(r, 0x55).ok_or(t.clone())?,
            });
        }
        let mut groups = Vec::new();
        for k in 0..GROUP_HEADS {
            let mut at = u32_at(&d, 0x3e + 4 * k).ok_or(t.clone())? as usize;
            // The lists are short: a loop in a damaged one stops.
            let mut guard = 0;
            while at != 0 && guard < 1024 {
                let entry = d.get(at..at + 12).ok_or(t.clone())?;
                let g = u32_at(entry, 4).ok_or(t.clone())? as usize;
                let name_at = u32_at(&d, g).ok_or(t.clone())? as usize;
                groups.push(FileGroup {
                    name: c_string(&d, name_at).ok_or(t.clone())?,
                    first: u32_at(&d, g + 4 + 0x12).ok_or(t.clone())?,
                    last: u32_at(&d, g + 8 + 0x12).ok_or(t.clone())?,
                });
                at = u32_at(entry, 8).ok_or(t.clone())? as usize;
                guard += 1;
            }
        }
        let lower = hdr.to_ascii_lowercase();
        let prefix = hdr[..lower.rfind("1.hdr").unwrap_or(hdr.len())].to_string();
        Ok(InstallShield {
            src,
            prefix,
            major,
            files,
            groups,
            volumes: Mutex::new(HashMap::new()),
            parts: Mutex::new(VecDeque::new()),
            states: Mutex::new(HashMap::new()),
            clock: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Every file of the table, in its order.
    pub fn files(&self) -> &[IsFile] {
        &self.files
    }

    pub fn groups(&self) -> &[FileGroup] {
        &self.groups
    }

    /// The files an installation copies: those of the groups that install
    /// ([`FileGroup::installs`]), valid and named, in the table's order.
    pub fn installed_files(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .groups
            .iter()
            .filter(|g| g.installs())
            .flat_map(|g| g.first..=g.last)
            .map(|i| i as usize)
            .filter(|&i| {
                self.files
                    .get(i)
                    .is_some_and(|f| f.flags & FILE_INVALID == 0 && !f.name.is_empty())
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The name of volume `n` (`data2.cab`).
    pub fn volume_name(&self, n: u16) -> String {
        format!("{}{n}.cab", self.prefix)
    }

    /// Volume `n`'s name as found, size and header; `None` when it is
    /// missing or not a volume.
    pub fn volume(&self, n: u16) -> Option<Volume> {
        let mut vols = self.volumes.lock().unwrap();
        vols.entry(n)
            .or_insert_with(|| {
                let name = self.volume_name(n);
                let size = self.src.size(&name)?;
                let head = self.src.read_at(&name, 0, 84)?;
                if head.get(..4)? != MAGIC {
                    return None;
                }
                Some((name, size, VolumeHeader::parse(&head)?))
            })
            .clone()
    }

    /// The volumes the installed files' data are in, in order: the
    /// volumes of their descriptors, and those a split file runs on into
    /// (known once the volume before it is there).
    pub fn volumes_needed(&self) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::new();
        for i in self.installed_files() {
            let f = &self.files[self.data_of(i)];
            if f.size > 0 && !out.contains(&f.volume) {
                out.push(f.volume);
            }
        }
        // A split file's next volume.
        let mut k = 0;
        while k < out.len() {
            let v = out[k];
            if let Some((_, _, h)) = self.volume(v) {
                let last = self.files.get(h.last_file as usize);
                if last.is_some_and(|f| f.flags & FILE_SPLIT != 0 && f.volume == v)
                    && !out.contains(&(v + 1))
                {
                    out.push(v + 1);
                }
            }
            k += 1;
        }
        out.sort_unstable();
        out
    }

    /// The names of the volumes needed ([`InstallShield::volumes_needed`])
    /// that `src` does not hold.
    pub fn missing_volumes(&self) -> Vec<String> {
        self.volumes_needed()
            .into_iter()
            .filter(|&v| self.volume(v).is_none())
            .map(|v| self.volume_name(v))
            .collect()
    }

    /// The index of the file whose data file `i` has (itself, or the
    /// first of its links).
    pub fn data_of(&self, mut i: usize) -> usize {
        let mut guard = 0;
        while let Some(f) = self.files.get(i) {
            if f.link_flags & LINK_PREV == 0 || f.link_previous as usize >= i || guard > 4096 {
                break;
            }
            i = f.link_previous as usize;
            guard += 1;
        }
        i
    }

    /// Where file `i`'s stored bytes are, volume by volume.
    fn segments(&self, i: usize) -> Option<Vec<Segment>> {
        let r = self.data_of(i);
        let f = &self.files[r];
        let compressed = f.flags & FILE_COMPRESSED != 0;
        let total = if compressed { f.compressed } else { f.size };
        let mut out = Vec::new();
        let mut v = f.volume;
        let (_, size, h) = self.volume(v)?;
        let mut len = total;
        if h.last_file as usize == r {
            let part = if compressed {
                h.last_compressed
            } else {
                h.last_expanded
            };
            if part > 0 && part < total {
                len = part;
            }
        }
        if f.data_offset.checked_add(len)? > size {
            return None;
        }
        out.push((v, f.data_offset, len));
        let mut left = total - len;
        while left > 0 {
            v = v.checked_add(1)?;
            let (_, size, h) = self.volume(v)?;
            if h.first_file as usize != r {
                return None;
            }
            let part = if compressed {
                h.first_compressed
            } else {
                h.first_expanded
            };
            let len = part.min(left);
            if len == 0 || h.first_offset.checked_add(len)? > size {
                return None;
            }
            out.push((v, h.first_offset, len));
            left -= len;
        }
        Some(out)
    }

    /// `len` bytes at `at` of volume `v`, through the kept parts.
    fn read_volume(&self, v: u16, at: u64, len: usize) -> Option<Vec<u8>> {
        let (name, size, _) = self.volume(v)?;
        if at.checked_add(len as u64)? > size {
            return None;
        }
        if len as u64 > 2 * PART {
            return self.src.read_at(&name, at, len);
        }
        let mut out = Vec::with_capacity(len);
        let mut pos = at;
        let end = at + len as u64;
        while pos < end {
            let n = pos / PART;
            let part = self.part(v, &name, size, n)?;
            let from = (pos - n * PART) as usize;
            let to = ((end - n * PART) as usize).min(part.len());
            if from >= to {
                return None;
            }
            out.extend_from_slice(&part[from..to]);
            pos = n * PART + to as u64;
        }
        Some(out)
    }

    fn part(&self, v: u16, name: &str, size: u64, n: u64) -> Option<Arc<Vec<u8>>> {
        {
            let parts = self.parts.lock().unwrap();
            if let Some((_, _, p)) = parts.iter().find(|(pv, pn, _)| *pv == v && *pn == n) {
                return Some(p.clone());
            }
        }
        let at = n * PART;
        let len = usize::try_from(PART.min(size - at)).ok()?;
        let data = Arc::new(self.src.read_at(name, at, len)?);
        let mut parts = self.parts.lock().unwrap();
        parts.push_back((v, n, data.clone()));
        while parts.len() > PARTS_KEPT {
            parts.pop_front();
        }
        Some(data)
    }

    /// `len` bytes at `at` of file `i`'s stored bytes (`segs`).
    fn read_stored(&self, segs: &[Segment], at: u64, len: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut pos = at;
        let end = at.checked_add(len as u64)?;
        let mut base = 0u64;
        for &(v, off, seg_len) in segs {
            if pos >= end {
                break;
            }
            if pos < base + seg_len {
                let from = pos - base;
                let n = (seg_len - from).min(end - pos);
                out.extend(self.read_volume(v, off + from, usize::try_from(n).ok()?)?);
                pos += n;
            }
            base += seg_len;
        }
        (pos == end).then_some(out)
    }

    fn state(&self, i: usize) -> Arc<Mutex<FileState>> {
        let now = self
            .clock
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut states = self.states.lock().unwrap();
        if !states.contains_key(&(i as u32)) && states.len() >= STATES_KEPT {
            // The least used half goes.
            let mut used: Vec<(u64, u32)> = states
                .iter()
                .map(|(k, s)| (s.lock().map_or(0, |s| s.used), *k))
                .collect();
            used.sort_unstable();
            for (_, k) in used.into_iter().take(STATES_KEPT / 2) {
                states.remove(&k);
            }
        }
        let s = states.entry(i as u32).or_default().clone();
        s.lock().unwrap().used = now;
        s
    }

    /// `len` bytes at `offset` of file `index` (of [`InstallShield::files`]);
    /// `None` past its end, for an obfuscated file, a volume missing or
    /// data that do not decode.
    pub fn read_file_at(&self, index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
        let r = self.data_of(index);
        let f = self.files.get(r)?;
        if offset.checked_add(len as u64)? > f.size || f.flags & FILE_OBFUSCATED != 0 {
            return None;
        }
        if len == 0 {
            return Some(Vec::new());
        }
        let segs = self.segments(r)?;
        if f.flags & FILE_COMPRESSED == 0 {
            return self.read_stored(&segs, offset, len);
        }
        let stored: u64 = segs.iter().map(|s| s.2).sum();
        let state = self.state(r);
        let mut st = state.lock().unwrap();
        let mut out = Vec::with_capacity(len);
        let end = offset + len as u64;
        let mut pos = offset;
        while pos < end {
            // The chunk holding `pos`: walked already, or walked now.
            let k = loop {
                if pos < st.out_end {
                    let k = st.chunks.partition_point(|c| c.1 <= pos) - 1;
                    break k;
                }
                if st.walked + 2 > stored {
                    return None;
                }
                let at = st.walked;
                let n = self.read_stored(&segs, at, 2)?;
                let n = u64::from(u16::from_le_bytes([n[0], n[1]]));
                let data = self.decode(&segs, at + 2, n, stored)?;
                let k = st.chunks.len();
                let start = st.out_end;
                st.chunks.push((at, start));
                st.walked = at + 2 + n;
                st.out_end += data.len() as u64;
                keep(&mut st.decoded, k, data);
            };
            let data = match st.decoded.iter().find(|d| d.0 == k) {
                Some(d) => d.1.clone(),
                None => {
                    let at = st.chunks[k].0;
                    let n = self.read_stored(&segs, at, 2)?;
                    let n = u64::from(u16::from_le_bytes([n[0], n[1]]));
                    let data = self.decode(&segs, at + 2, n, stored)?;
                    keep(&mut st.decoded, k, data.clone());
                    data
                }
            };
            let start = st.chunks[k].1;
            let from = (pos - start) as usize;
            let to = ((end - start) as usize).min(data.len());
            if from >= to {
                return None;
            }
            out.extend_from_slice(&data[from..to]);
            pos = start + to as u64;
        }
        Some(out)
    }

    /// The chunk of `n` stored bytes at `at`, decoded.
    fn decode(&self, segs: &[Segment], at: u64, n: u64, stored: u64) -> Option<Arc<Vec<u8>>> {
        if n == 0 || at.checked_add(n)? > stored {
            return None;
        }
        let raw = self.read_stored(segs, at, usize::try_from(n).ok()?)?;
        miniz_oxide::inflate::decompress_to_vec_with_limit(&raw, MAX_CHUNK_OUT)
            .ok()
            .map(Arc::new)
    }

    /// The whole of file `index`.
    pub fn read_file(&self, index: usize) -> Option<Vec<u8>> {
        let size = usize::try_from(self.files.get(index)?.size).ok()?;
        self.read_file_at(index, 0, size)
    }
}

fn keep(decoded: &mut VecDeque<(usize, Arc<Vec<u8>>)>, k: usize, data: Arc<Vec<u8>>) {
    decoded.push_back((k, data));
    while decoded.len() > 4 {
        decoded.pop_front();
    }
}

/// A name as a key: `/`-separated, no `.` or empty component, lower case.
fn key(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

/// The files an installation copies from a set ([`InstallShield::installed_files`]),
/// as the folder they are copied to (names matched without regard to
/// case; a name met twice is the first file's).
pub struct IsFiles {
    cab: Arc<InstallShield>,
    index: HashMap<String, usize>,
    dirs: HashMap<String, Vec<String>>,
}

impl IsFiles {
    pub fn new(cab: Arc<InstallShield>) -> IsFiles {
        let mut index = HashMap::new();
        let mut dirs: HashMap<String, Vec<String>> = HashMap::new();
        for i in cab.installed_files() {
            let path = cab.files[i].path();
            let k = key(&path);
            if index.contains_key(&k) {
                continue;
            }
            index.insert(k, i);
            let parts: Vec<&str> = path
                .split(['\\', '/'])
                .filter(|p| !p.is_empty() && *p != ".")
                .collect();
            for d in 0..parts.len() {
                let dir = key(&parts[..d].join("/"));
                let entry = dirs.entry(dir).or_default();
                if !entry.iter().any(|e| e.eq_ignore_ascii_case(parts[d])) {
                    entry.push(parts[d].to_string());
                }
            }
        }
        IsFiles { cab, index, dirs }
    }

    pub fn cabinet(&self) -> &Arc<InstallShield> {
        &self.cab
    }

    /// The table's index of the file `name`.
    pub fn file_index(&self, name: &str) -> Option<usize> {
        self.index.get(&key(name)).copied()
    }
}

impl FileSource for IsFiles {
    fn size(&self, name: &str) -> Option<u64> {
        Some(self.cab.files[self.file_index(name)?].size)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.cab.read_file_at(self.file_index(name)?, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        let k = key(name);
        !k.is_empty() && self.dirs.contains_key(&k)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.dirs.get(&key(dir)).cloned().unwrap_or_default()
    }
}

/// Several folders read as one: a name is the first folder's that holds
/// it (the discs of a set copied side by side, each disc's volumes found
/// in its own folder). Each part is a source and a folder in it (`""`:
/// its top, `CD2/`: a folder below).
pub struct Layered {
    parts: Vec<(Arc<dyn FileSource>, String)>,
}

impl Layered {
    pub fn new(parts: Vec<(Arc<dyn FileSource>, String)>) -> Layered {
        let parts = parts
            .into_iter()
            .map(|(s, p)| {
                let p = p.trim_matches(['/', '\\']).to_string();
                let p = if p.is_empty() { p } else { format!("{p}/") };
                (s, p)
            })
            .collect();
        Layered { parts }
    }

    /// The parts, each a source and its folder (`""` or ending with `/`).
    pub fn parts(&self) -> &[(Arc<dyn FileSource>, String)] {
        &self.parts
    }

    /// The part holding `name`, and the name there.
    pub fn holder(&self, name: &str) -> Option<(&Arc<dyn FileSource>, String)> {
        self.parts.iter().find_map(|(s, p)| {
            let n = format!("{p}{name}");
            s.size(&n).is_some().then_some((s, n))
        })
    }
}

impl FileSource for Layered {
    fn size(&self, name: &str) -> Option<u64> {
        let (s, n) = self.holder(name)?;
        s.size(&n)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let (s, n) = self.holder(name)?;
        s.read_at(&n, offset, len)
    }

    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        if let Some((s, n)) = self.holder(name) {
            s.read_ahead(&n, offset, len);
        }
    }

    fn has_dir(&self, name: &str) -> bool {
        self.parts
            .iter()
            .any(|(s, p)| s.has_dir(&format!("{p}{name}")))
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (s, p) in &self.parts {
            for n in s.list(&format!("{p}{dir}")) {
                if !out.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
                    out.push(n);
                }
            }
        }
        out
    }

    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.parts.first()?.0.read_beside(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    /// A synthetic file: (dir index, name, data, compressed, chunk size).
    struct Spec<'a> {
        dir: u16,
        name: &'a str,
        data: Vec<u8>,
        compressed: bool,
    }

    /// The stored bytes of `data`: chunks of `chunk` bytes of output, each
    /// a u16 size and a raw deflate stream.
    fn chunks(data: &[u8], chunk: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for c in data.chunks(chunk) {
            let z = miniz_oxide::deflate::compress_to_vec(c, 6);
            out.extend((z.len() as u16).to_le_bytes());
            out.extend(z);
        }
        out
    }

    /// A set of `volumes` volumes (`data1.cab` ...) holding `files` one
    /// after the other, each volume holding at most `per_volume` stored
    /// bytes (a file crossing the limit is split); the last file a link to
    /// the first. Returns the set's files.
    fn build(files: &[Spec], dirs: &[&str], per_volume: u64) -> MemFiles {
        // Stored bytes of each file.
        let stored: Vec<Vec<u8>> = files
            .iter()
            .map(|f| {
                if f.compressed {
                    chunks(&f.data, 32768)
                } else {
                    f.data.clone()
                }
            })
            .collect();
        // Lay them out over volumes.
        let mut vols: Vec<Vec<u8>> = vec![Vec::new()];
        let mut descs = Vec::new();
        let zero = VolumeHeader {
            data_offset: 0x200,
            first_file: 0,
            last_file: 0,
            first_offset: 0,
            first_expanded: 0,
            first_compressed: 0,
            last_offset: 0,
            last_expanded: 0,
            last_compressed: 0,
        };
        let mut heads: Vec<VolumeHeader> = vec![zero];
        for (i, s) in stored.iter().enumerate() {
            let mut v = vols.len() - 1;
            let start = 0x200 + vols[v].len() as u64;
            let start_vol = v as u16 + 1;
            let mut rest = &s[..];
            let mut split = false;
            heads[v].last_file = i as u32;
            loop {
                let room = per_volume - vols[v].len() as u64;
                if (rest.len() as u64) <= room {
                    if split {
                        heads[v].first_offset = 0x200;
                        heads[v].first_compressed = rest.len() as u64;
                        // A compressed file's whole size; a stored one's
                        // part (as its compressed size).
                        heads[v].first_expanded = if files[i].compressed {
                            files[i].data.len() as u64
                        } else {
                            rest.len() as u64
                        };
                    }
                    vols[v].extend(rest);
                    break;
                }
                let (now, later) = rest.split_at(room as usize);
                if !split {
                    heads[v].last_offset = start;
                } else {
                    heads[v].first_offset = 0x200;
                    heads[v].first_compressed = now.len() as u64;
                    heads[v].first_expanded = if files[i].compressed {
                        files[i].data.len() as u64
                    } else {
                        now.len() as u64
                    };
                }
                heads[v].last_compressed = now.len() as u64;
                heads[v].last_expanded = if files[i].compressed {
                    files[i].data.len() as u64
                } else {
                    now.len() as u64
                };
                vols[v].extend(now);
                rest = later;
                split = true;
                vols.push(Vec::new());
                heads.push(VolumeHeader {
                    data_offset: 0x200,
                    first_file: i as u32,
                    last_file: i as u32,
                    ..zero
                });
                v += 1;
            }
            descs.push((start_vol, start, s.len() as u64, split));
        }
        // The header: common header, descriptor at 0x200.
        let mut table = Vec::new();
        let n_dirs = dirs.len();
        let names_at = 4 * n_dirs;
        let mut names = Vec::new();
        let mut dir_offs = Vec::new();
        for d in dirs {
            dir_offs.push(names_at + names.len());
            names.extend(d.as_bytes());
            names.push(0);
        }
        let mut file_name_offs = Vec::new();
        for f in files {
            file_name_offs.push(names_at + names.len());
            names.extend(f.name.as_bytes());
            names.push(0);
        }
        for o in &dir_offs {
            table.extend((*o as u32).to_le_bytes());
        }
        table.extend(&names);
        let desc_off = table.len();
        let n = files.len() + 1;
        for i in 0..n {
            let mut r = vec![0u8; DESCRIPTOR];
            let (src, link) = if i == files.len() {
                (0, true)
            } else {
                (i, false)
            };
            let f = &files[src];
            let (vol, off, st, split) = descs[src];
            let mut flags = if f.compressed { FILE_COMPRESSED } else { 0 };
            if split {
                flags |= FILE_SPLIT;
            }
            r[0..2].copy_from_slice(&flags.to_le_bytes());
            r[2..10].copy_from_slice(&(f.data.len() as u64).to_le_bytes());
            r[10..18].copy_from_slice(&st.to_le_bytes());
            r[18..26].copy_from_slice(&off.to_le_bytes());
            let name_at = if link {
                // "copy.bin", appended below.
                names_at + names.len()
            } else {
                file_name_offs[src]
            };
            r[0x3a..0x3e].copy_from_slice(&(name_at as u32).to_le_bytes());
            let dir = if link { 0 } else { f.dir };
            r[0x3e..0x40].copy_from_slice(&dir.to_le_bytes());
            if link {
                r[0x4c..0x50].copy_from_slice(&0u32.to_le_bytes());
                r[0x54] = LINK_PREV;
                // A link's own volume is not its data's.
                r[0x55..0x57].copy_from_slice(&9u16.to_le_bytes());
            } else {
                r[0x55..0x57].copy_from_slice(&vol.to_le_bytes());
            }
            table.extend(r);
        }
        let link_name = table.len();
        table.extend(b"copy.bin\0");
        // Patch the link's name offset (it was computed before the
        // descriptors were laid).
        let o = desc_off + files.len() * DESCRIPTOR + 0x3a;
        table[o..o + 4].copy_from_slice(&(link_name as u32).to_le_bytes());
        // The descriptor: the file table at 0x400, two groups.
        let mut d = vec![0u8; 0x400];
        d[0x0c..0x10].copy_from_slice(&0x400u32.to_le_bytes());
        d[0x1c..0x20].copy_from_slice(&(n_dirs as u32).to_le_bytes());
        d[0x28..0x2c].copy_from_slice(&(n as u32).to_le_bytes());
        d[0x2c..0x30].copy_from_slice(&(desc_off as u32).to_le_bytes());
        // Group list entries at 0x200 and 0x20c, descriptors at 0x240 and
        // 0x280, names at 0x300.
        let group = |d: &mut Vec<u8>,
                     head: usize,
                     entry: usize,
                     g: usize,
                     name: usize,
                     first: u32,
                     last: u32,
                     label: &str| {
            d[0x3e + 4 * head..0x42 + 4 * head].copy_from_slice(&(entry as u32).to_le_bytes());
            d[entry..entry + 4].copy_from_slice(&(name as u32).to_le_bytes());
            d[entry + 4..entry + 8].copy_from_slice(&(g as u32).to_le_bytes());
            d[g..g + 4].copy_from_slice(&(name as u32).to_le_bytes());
            d[g + 4 + 0x12..g + 8 + 0x12].copy_from_slice(&first.to_le_bytes());
            d[g + 8 + 0x12..g + 12 + 0x12].copy_from_slice(&last.to_le_bytes());
            d[name..name + label.len()].copy_from_slice(label.as_bytes());
        };
        group(&mut d, 3, 0x200, 0x240, 0x300, 0, n as u32 - 1, "Files");
        group(&mut d, 5, 0x20c, 0x280, 0x340, 0, 0, "<Support>Files");
        d.extend(table);
        let mut hdr = MAGIC.to_vec();
        hdr.extend(0x0100_9500u32.to_le_bytes());
        hdr.extend(0u32.to_le_bytes());
        hdr.extend(0x200u32.to_le_bytes());
        hdr.extend((d.len() as u32).to_le_bytes());
        hdr.resize(0x200, 0);
        hdr.extend(d);
        let mut m = MemFiles::new();
        m.insert("DATA1.HDR", hdr);
        for (k, (v, h)) in vols.iter().zip(&heads).enumerate() {
            let mut c = MAGIC.to_vec();
            c.extend(0x0100_9500u32.to_le_bytes());
            c.extend([0u8; 12]);
            for q in [
                h.data_offset,
                u64::from(h.first_file) | (u64::from(h.last_file) << 32),
                h.first_offset,
                h.first_expanded,
                h.first_compressed,
                h.last_offset,
                h.last_expanded,
                h.last_compressed,
            ] {
                c.extend(q.to_le_bytes());
            }
            c.resize(0x200, 0);
            c.extend(v);
            m.insert(&format!("data{}.cab", k + 1), c);
        }
        m
    }

    /// `n` bytes that deflate shrinks by about half: noise in the low
    /// bits.
    fn data(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761) | 1;
        (0..n)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                ((i / 64) as u8 & 0xf0) | (x as u8 & 0x0f)
            })
            .collect()
    }

    #[test]
    fn versions() {
        assert_eq!(major_version(0x0100_9500), 9);
        assert_eq!(major_version(0x0100_6000), 6);
        assert_eq!(major_version(0x0200_0258), 6);
        assert_eq!(major_version(0x0100_5000), 5);
    }

    #[test]
    fn files_read_back_across_volumes_and_links() {
        let specs = [
            Spec {
                dir: 1,
                name: "A.bin",
                data: data(100_000, 3),
                compressed: true,
            },
            Spec {
                dir: 0,
                name: "top.txt",
                data: b"hello".to_vec(),
                compressed: false,
            },
            Spec {
                dir: 2,
                name: "big.bin",
                data: data(300_000, 5),
                compressed: true,
            },
            Spec {
                dir: 1,
                name: "raw.bin",
                data: data(50_000, 11),
                compressed: false,
            },
        ];
        let m = build(&specs, &["", "datas\\Scenes", "datas\\Videos"], 40_000);
        let src: Arc<dyn FileSource> = Arc::new(m);
        let cab = Arc::new(InstallShield::open(src).unwrap());
        assert_eq!(cab.major, 9);
        assert_eq!(cab.groups().len(), 2);
        assert!(cab.volumes_needed().len() > 3, "split over volumes");
        assert!(cab.missing_volumes().is_empty());
        let f = IsFiles::new(cab.clone());
        assert_eq!(f.list(""), ["datas", "top.txt", "copy.bin"]);
        assert_eq!(f.list("DATAS"), ["Scenes", "Videos"]);
        assert!(f.has_dir("datas/videos"));
        for s in &specs {
            let p = if s.dir == 0 {
                s.name.to_string()
            } else {
                format!(
                    "{}/{}",
                    ["", "datas/scenes", "datas/videos"][s.dir as usize],
                    s.name
                )
            };
            assert_eq!(f.size(&p), Some(s.data.len() as u64), "{p}");
            assert_eq!(f.read(&p).as_deref(), Some(&s.data[..]), "{p}");
        }
        // Parts, backwards, across chunks and volumes.
        let big = &specs[2].data;
        for (at, len) in [
            (250_000u64, 40_000usize),
            (32_760, 20),
            (0, 1),
            (299_999, 1),
        ] {
            assert_eq!(
                f.read_at("datas/videos/BIG.BIN", at, len).unwrap(),
                big[at as usize..at as usize + len]
            );
        }
        assert_eq!(f.read_at("datas/videos/big.bin", 299_999, 2), None);
        // The link reads its first file's data, from the first file's
        // volume.
        assert_eq!(f.read("copy.bin").unwrap(), specs[0].data);
    }

    #[test]
    fn a_missing_volume_is_named() {
        let specs = [
            Spec {
                dir: 0,
                name: "a",
                data: data(60_000, 3),
                compressed: false,
            },
            Spec {
                dir: 0,
                name: "b",
                data: data(60_000, 7),
                compressed: false,
            },
        ];
        let mut m = build(&specs, &[""], 50_000);
        let mut without = MemFiles::new();
        for n in ["data1.hdr", "data1.cab", "data2.cab"] {
            without.insert(n, m.read(n).unwrap());
        }
        m = without;
        let cab = InstallShield::open(Arc::new(m)).unwrap();
        assert_eq!(cab.volumes_needed(), [1, 2, 3]);
        assert_eq!(cab.missing_volumes(), ["data3.cab"]);
        // The first file is whole; the second runs on into data3.cab.
        assert_eq!(cab.read_file(0).unwrap(), specs[0].data);
        assert_eq!(cab.read_file(1), None);
    }

    #[test]
    fn not_a_set() {
        let mut m = MemFiles::new();
        m.insert("data1.hdr", b"MSCF and more bytes than twenty".to_vec());
        assert!(matches!(
            InstallShield::open(Arc::new(m)),
            Err(IsError::NotInstallShield)
        ));
        let mut m = MemFiles::new();
        let mut h = MAGIC.to_vec();
        h.extend(0x0100_5000u32.to_le_bytes());
        h.resize(64, 0);
        m.insert("data1.hdr", h);
        assert!(matches!(
            InstallShield::open(Arc::new(m)),
            Err(IsError::Version(5))
        ));
        assert!(matches!(
            InstallShield::open(Arc::new(MemFiles::new())),
            Err(IsError::NotInstallShield)
        ));
    }

    #[test]
    fn layered_folders() {
        let mut a = MemFiles::new();
        a.insert("CD1/data1.hdr", vec![1]);
        a.insert("CD2/data3.cab", vec![3]);
        a.insert("CD2/data1.hdr", vec![9]);
        let a: Arc<dyn FileSource> = Arc::new(a);
        let l = Layered::new(vec![(a.clone(), "CD1".into()), (a.clone(), "CD2/".into())]);
        assert_eq!(l.read("data1.hdr"), Some(vec![1]));
        assert_eq!(l.read("DATA3.CAB"), Some(vec![3]));
        assert_eq!(l.size("data4.cab"), None);
        assert_eq!(l.list(""), ["data1.hdr", "data3.cab"]);
    }
}
