//! Microsoft cabinets (`MSCF`, version 1.3) read in place: the files of one
//! or more cabinets served as a [`FileSource`], with no install step.
//!
//! *Atlantis V*'s discs hold their data in such cabinets (`DATAS.cab` and
//! one per language), which the game's installer unpacks with its
//! `CABSetup` plug-in. Only what those cabinets use is read: folders
//! compressed with MSZIP (type 1) or stored (type 0), in a single cabinet
//! (no file continued from or into another cabinet); a header reserve is
//! skipped.
//!
//! Layout (Microsoft's "Microsoft Cabinet Format", 1997):
//!
//! | Part | Fields |
//! |---|---|
//! | `CFHEADER` (36 bytes) | `MSCF`, u32 0, u32 cabinet size, u32 0, u32 offset of the first `CFFILE`, u32 0, u8 minor 3, u8 major 1, u16 folders, u16 files, u16 flags, u16 set id, u16 index; with flag 4 a u16/u8/u8 reserve size and the header's reserve; with flags 1 and 2 the previous and next cabinet's names |
//! | `CFFOLDER` (8 bytes + reserve) | u32 offset of its first `CFDATA`, u16 blocks, u16 compression |
//! | `CFFILE` (16 bytes + name) | u32 size, u32 offset in its folder's uncompressed bytes, u16 folder, u16 date, u16 time, u16 attributes, the name NUL-terminated |
//! | `CFDATA` (8 bytes + reserve + data) | u32 checksum, u16 compressed size, u16 uncompressed size (at most 32 KiB) |
//!
//! An MSZIP block is `CK` and a complete deflate stream, whose matches may
//! reach back into the previous blocks' last 32 KiB of output. A folder is
//! therefore one stream: reading a file deep in a large folder decodes it
//! from the start the first time. The reader keeps the 32 KiB history at
//! every [`CHECKPOINT_BLOCKS`]-th block it passes and its last position,
//! so a later read starts from the nearest point before it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use miniz_oxide::inflate::core::{decompress, inflate_flags, DecompressorOxide};
use miniz_oxide::inflate::TINFLStatus;

use crate::files::FileSource;

/// The deflate window: how far back an MSZIP block's matches reach.
const WINDOW: usize = 32 * 1024;

/// The history is kept at every this-many blocks of a folder (32 KiB per
/// 2 MiB of data at most).
pub const CHECKPOINT_BLOCKS: usize = 64;

/// Why a file is not a cabinet this module reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CabError {
    NotCabinet,
    Truncated,
    /// A compression other than stored or MSZIP (Quantum, LZX).
    Compression(u16),
    /// A file continued from or into another cabinet.
    Spanned,
}

impl std::fmt::Display for CabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CabError::NotCabinet => write!(f, "not a Microsoft cabinet"),
            CabError::Truncated => write!(f, "the cabinet is truncated"),
            CabError::Compression(c) => write!(f, "cabinet compression {c:#x} is not read"),
            CabError::Spanned => write!(f, "a file spans several cabinets"),
        }
    }
}

impl std::error::Error for CabError {}

/// One file of a cabinet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CabFile {
    /// The name as stored (`.\Datas\Scenes\etage0.wam`).
    pub name: String,
    pub size: u32,
    pub folder: u16,
    /// Its offset in the folder's uncompressed bytes.
    pub offset: u32,
    pub attributes: u16,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    /// Offset of the block's data (after its header and reserve).
    data_at: u64,
    packed: u16,
    unpacked: u16,
    /// The folder's uncompressed bytes before the block.
    start: u64,
}

#[derive(Default)]
struct FolderState {
    /// Filled on first use.
    blocks: Option<Vec<Block>>,
    /// Block index → the last 32 KiB of output before it.
    checkpoints: HashMap<usize, Vec<u8>>,
    /// The last block decoded: its index, its output, the history before
    /// it.
    last: Option<(usize, Vec<u8>, Vec<u8>)>,
}

struct Folder {
    first_data: u64,
    blocks: u16,
    compression: u16,
    state: Mutex<FolderState>,
}

/// One cabinet, read through `src` (its file `path` there).
pub struct Cabinet {
    src: Arc<dyn FileSource>,
    path: String,
    data_reserve: u64,
    folders: Vec<Folder>,
    files: Vec<CabFile>,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

impl Cabinet {
    /// Opens the cabinet `path` of `src`: its header, folders and file
    /// list (the data are read on demand).
    pub fn open(src: Arc<dyn FileSource>, path: &str) -> Result<Cabinet, CabError> {
        let size = src.size(path).ok_or(CabError::NotCabinet)?;
        let head = src
            .read_at(path, 0, 36.min(size as usize))
            .ok_or(CabError::Truncated)?;
        if head.len() < 36 || &head[..4] != b"MSCF" {
            return Err(CabError::NotCabinet);
        }
        let t = CabError::Truncated;
        let files_at = u64::from(u32_at(&head, 16).ok_or(t.clone())?);
        let (minor, major) = (head[24], head[25]);
        if (major, minor) != (1, 3) {
            return Err(CabError::NotCabinet);
        }
        let n_folders = usize::from(u16_at(&head, 26).ok_or(t.clone())?);
        let n_files = usize::from(u16_at(&head, 28).ok_or(t.clone())?);
        let flags = u16_at(&head, 30).ok_or(t.clone())?;
        // The header's fixed part and its options, then the folders.
        let mut at = 36u64;
        let (mut folder_reserve, mut data_reserve) = (0u64, 0u64);
        if flags & 4 != 0 {
            let r = src.read_at(path, at, 4).ok_or(t.clone())?;
            let header_reserve = u64::from(u16_at(&r, 0).ok_or(t.clone())?);
            folder_reserve = u64::from(r[2]);
            data_reserve = u64::from(r[3]);
            at += 4 + header_reserve;
        }
        if flags & 3 != 0 {
            return Err(CabError::Spanned);
        }
        let table = src
            .read_at(path, at, n_folders * (8 + folder_reserve as usize))
            .ok_or(t.clone())?;
        let mut folders = Vec::with_capacity(n_folders);
        for k in 0..n_folders {
            let o = k * (8 + folder_reserve as usize);
            let compression = u16_at(&table, o + 6).ok_or(t.clone())?;
            if compression & 0xf > 1 {
                return Err(CabError::Compression(compression));
            }
            folders.push(Folder {
                first_data: u64::from(u32_at(&table, o).ok_or(t.clone())?),
                blocks: u16_at(&table, o + 4).ok_or(t.clone())?,
                compression: compression & 0xf,
                state: Mutex::new(FolderState::default()),
            });
        }
        // The file entries: read in one piece up to the first folder's data.
        let end = folders
            .iter()
            .map(|f| f.first_data)
            .filter(|&o| o > files_at)
            .min()
            .unwrap_or(size)
            .min(size);
        let list = src
            .read_at(path, files_at, (end - files_at) as usize)
            .ok_or(t.clone())?;
        let mut files = Vec::with_capacity(n_files);
        let mut o = 0usize;
        for _ in 0..n_files {
            let size = u32_at(&list, o).ok_or(t.clone())?;
            let offset = u32_at(&list, o + 4).ok_or(t.clone())?;
            let folder = u16_at(&list, o + 8).ok_or(t.clone())?;
            let attributes = u16_at(&list, o + 14).ok_or(t.clone())?;
            let name_end = list[o + 16..]
                .iter()
                .position(|&b| b == 0)
                .ok_or(t.clone())?;
            let raw = &list[o + 16..o + 16 + name_end];
            // Attribute 0x80: the name is UTF-8; otherwise the system's
            // code page (ASCII in every name of the game's cabinets).
            let name = if attributes & 0x80 != 0 {
                String::from_utf8_lossy(raw).into_owned()
            } else {
                raw.iter().map(|&b| char::from(b)).collect()
            };
            if usize::from(folder) >= n_folders {
                return Err(CabError::Spanned);
            }
            files.push(CabFile {
                name,
                size,
                folder,
                offset,
                attributes,
            });
            o += 16 + name_end + 1;
        }
        Ok(Cabinet {
            src,
            path: path.to_string(),
            data_reserve,
            folders,
            files,
        })
    }

    /// The files, in the cabinet's order.
    pub fn files(&self) -> &[CabFile] {
        &self.files
    }

    /// The folder's blocks, read from their headers on first use.
    fn blocks(&self, folder: &Folder, state: &mut FolderState) -> Option<()> {
        if state.blocks.is_some() {
            return Some(());
        }
        let mut v = Vec::with_capacity(usize::from(folder.blocks));
        let (mut at, mut start) = (folder.first_data, 0u64);
        for _ in 0..folder.blocks {
            let h = self.src.read_at(&self.path, at, 8)?;
            let packed = u16_at(&h, 4)?;
            let unpacked = u16_at(&h, 6)?;
            let data_at = at + 8 + self.data_reserve;
            v.push(Block {
                data_at,
                packed,
                unpacked,
                start,
            });
            at = data_at + u64::from(packed);
            start += u64::from(unpacked);
        }
        state.blocks = Some(v);
        Some(())
    }

    /// Decodes block `b` after `history` (the 32 KiB before it).
    fn decode(&self, folder: &Folder, b: &Block, history: &[u8]) -> Option<Vec<u8>> {
        let packed = self
            .src
            .read_at(&self.path, b.data_at, usize::from(b.packed))?;
        let n = usize::from(b.unpacked);
        if folder.compression == 0 {
            return (packed.len() == n).then_some(packed);
        }
        if packed.get(..2) != Some(b"CK") {
            log::warn!("{}: an MSZIP block without its CK", self.path);
            return None;
        }
        let mut out = Vec::with_capacity(history.len() + n);
        out.extend_from_slice(history);
        out.resize(history.len() + n, 0);
        let mut r = DecompressorOxide::new();
        let (status, _, written) = decompress(
            &mut r,
            &packed[2..],
            &mut out,
            history.len(),
            inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        if status != TINFLStatus::Done || written != n {
            log::warn!(
                "{}: an MSZIP block decodes to {written} bytes ({status:?}), not {n}",
                self.path
            );
            return None;
        }
        Some(out.split_off(history.len()))
    }

    /// `len` bytes at `offset` of folder `k`'s uncompressed bytes.
    fn read_folder(&self, k: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
        let folder = self.folders.get(k)?;
        let mut st = folder.state.lock().unwrap();
        self.blocks(folder, &mut st)?;
        let blocks = st.blocks.clone()?;
        let mut out = Vec::with_capacity(len);
        if len == 0 {
            return Some(out);
        }
        let end = offset.checked_add(len as u64)?;
        let first = blocks.partition_point(|b| b.start + u64::from(b.unpacked) <= offset);
        if first >= blocks.len() {
            return None;
        }
        // Where to start decoding: the last block decoded, or the nearest
        // checkpoint before `first`.
        let mut from = (first / CHECKPOINT_BLOCKS) * CHECKPOINT_BLOCKS;
        while from > 0 && !st.checkpoints.contains_key(&from) {
            from -= CHECKPOINT_BLOCKS;
        }
        let mut history: Vec<u8> = st.checkpoints.get(&from).cloned().unwrap_or_default();
        let mut cached: Option<(usize, Vec<u8>)> = None;
        if let Some((i, data, before)) = &st.last {
            if *i == first {
                cached = Some((*i, data.clone()));
                history = before.clone();
                from = *i;
            } else if *i < first && *i + 1 >= from {
                let mut h = before.clone();
                h.extend_from_slice(data);
                let cut = h.len().saturating_sub(WINDOW);
                history = h.split_off(cut);
                from = *i + 1;
            }
        }
        let mut b = from;
        while b < blocks.len() && blocks[b].start < end {
            let block = blocks[b];
            if b.is_multiple_of(CHECKPOINT_BLOCKS) && b > 0 {
                st.checkpoints.entry(b).or_insert_with(|| history.clone());
            }
            let data = match cached.take() {
                Some((i, d)) if i == b => d,
                _ => self.decode(folder, &block, &history)?,
            };
            let (s, e) = (block.start, block.start + data.len() as u64);
            if e > offset {
                let lo = offset.max(s) - s;
                let hi = end.min(e) - s;
                out.extend_from_slice(&data[lo as usize..hi as usize]);
            }
            let before = history.clone();
            history.extend_from_slice(&data);
            let cut = history.len().saturating_sub(WINDOW);
            history.drain(..cut);
            st.last = Some((b, data, before));
            b += 1;
        }
        (out.len() == len).then_some(out)
    }

    /// `len` bytes at `offset` of file `index` (of [`Cabinet::files`]).
    pub fn read_file_at(&self, index: usize, offset: u64, len: usize) -> Option<Vec<u8>> {
        let f = self.files.get(index)?;
        if offset.checked_add(len as u64)? > u64::from(f.size) {
            return None;
        }
        self.read_folder(usize::from(f.folder), u64::from(f.offset) + offset, len)
    }
}

/// A file name as a key: `/`-separated, lower case, no `.` component.
fn key(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

/// The files of several cabinets as one folder (names matched without
/// regard to case; a name in two cabinets is taken from the first).
pub struct CabFiles {
    cabinets: Vec<Cabinet>,
    /// Key → (cabinet, file).
    index: HashMap<String, (usize, usize)>,
    /// Folder key → entry names.
    dirs: HashMap<String, Vec<String>>,
}

impl CabFiles {
    pub fn new(cabinets: Vec<Cabinet>) -> CabFiles {
        let mut index = HashMap::new();
        let mut dirs: HashMap<String, Vec<String>> = HashMap::new();
        for (c, cab) in cabinets.iter().enumerate() {
            for (i, f) in cab.files.iter().enumerate() {
                let k = key(&f.name);
                if index.contains_key(&k) {
                    continue;
                }
                index.insert(k.clone(), (c, i));
                // Each folder on the path lists its next component.
                let parts: Vec<&str> = f
                    .name
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
        }
        CabFiles {
            cabinets,
            index,
            dirs,
        }
    }

    pub fn cabinets(&self) -> &[Cabinet] {
        &self.cabinets
    }
}

impl FileSource for CabFiles {
    fn size(&self, name: &str) -> Option<u64> {
        let (c, i) = *self.index.get(&key(name))?;
        Some(u64::from(self.cabinets[c].files[i].size))
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let (c, i) = *self.index.get(&key(name))?;
        self.cabinets[c].read_file_at(i, offset, len)
    }

    fn has_dir(&self, name: &str) -> bool {
        let k = key(name);
        !k.is_empty() && self.dirs.contains_key(&k)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        self.dirs.get(&key(dir)).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    /// A cabinet of `files` (name, data) in one folder of `compression`,
    /// cut into blocks of `block` bytes.
    fn build(files: &[(&str, &[u8])], block: usize, mszip: bool) -> Vec<u8> {
        let all: Vec<u8> = files.iter().flat_map(|f| f.1.iter().copied()).collect();
        let mut entries = Vec::new();
        let mut off = 0u32;
        for (n, d) in files {
            entries.extend((d.len() as u32).to_le_bytes());
            entries.extend(off.to_le_bytes());
            entries.extend(0u16.to_le_bytes());
            entries.extend([0u8; 4]);
            entries.extend(0x20u16.to_le_bytes());
            entries.extend(n.as_bytes());
            entries.push(0);
            off += d.len() as u32;
        }
        let mut data = Vec::new();
        let mut n_blocks = 0u16;
        for chunk in all.chunks(block) {
            let payload = if mszip {
                // Each block a stored deflate block (valid MSZIP: a full
                // deflate stream).
                let mut p = b"CK".to_vec();
                p.push(1);
                p.extend((chunk.len() as u16).to_le_bytes());
                p.extend((!(chunk.len() as u16)).to_le_bytes());
                p.extend(chunk);
                p
            } else {
                chunk.to_vec()
            };
            data.extend(0u32.to_le_bytes());
            data.extend((payload.len() as u16).to_le_bytes());
            data.extend((chunk.len() as u16).to_le_bytes());
            data.extend(payload);
            n_blocks += 1;
        }
        let files_at = 36 + 8;
        let data_at = files_at + entries.len();
        let mut c = b"MSCF".to_vec();
        c.extend(0u32.to_le_bytes());
        c.extend(((data_at + data.len()) as u32).to_le_bytes());
        c.extend(0u32.to_le_bytes());
        c.extend((files_at as u32).to_le_bytes());
        c.extend(0u32.to_le_bytes());
        c.extend([3, 1]);
        c.extend(1u16.to_le_bytes());
        c.extend((files.len() as u16).to_le_bytes());
        c.extend([0u8; 6]);
        c.extend((data_at as u32).to_le_bytes());
        c.extend(n_blocks.to_le_bytes());
        c.extend(u16::from(mszip).to_le_bytes());
        c.extend(entries);
        c.extend(data);
        c
    }

    fn files(cab: Vec<u8>) -> CabFiles {
        let mut m = MemFiles::new();
        m.insert("x.cab", cab);
        CabFiles::new(vec![Cabinet::open(Arc::new(m), "x.cab").unwrap()])
    }

    #[test]
    fn stored_and_mszip_read_back() {
        let a: Vec<u8> = (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let b = b"hello".to_vec();
        for mszip in [false, true] {
            let f = files(build(
                &[(".\\Datas\\A.bin", &a), (".\\Datas\\Sub\\b.TXT", &b)],
                10_000,
                mszip,
            ));
            assert_eq!(f.size("datas/a.bin"), Some(70_000));
            assert_eq!(f.read("DATAS\\SUB\\B.txt").unwrap(), b);
            assert_eq!(f.read("datas/a.bin").unwrap(), a);
            // Backwards and across blocks.
            assert_eq!(
                f.read_at("datas/a.bin", 65_000, 5).unwrap(),
                a[65_000..65_005]
            );
            assert_eq!(
                f.read_at("datas/a.bin", 9_990, 30).unwrap(),
                a[9_990..10_020]
            );
            assert_eq!(f.read_at("datas/a.bin", 69_999, 2), None);
            assert!(f.has_dir("datas/sub"));
            assert_eq!(f.list("Datas"), ["A.bin", "Sub"]);
            assert_eq!(f.list(""), ["Datas"]);
        }
    }

    #[test]
    fn mszip_history_reaches_the_previous_block() {
        // A real deflate stream per block, the second block's matches in
        // the first block's output.
        use miniz_oxide::deflate::core::{
            compress, create_comp_flags_from_zip_params, CompressorOxide, TDEFLFlush,
        };
        let text = b"the quick brown fox jumps over the lazy dog, ".repeat(40);
        let (x, y) = text.split_at(1000);
        // One raw deflate stream of x || y cut at a sync flush after x:
        // the second part's matches may reach into x, as MSZIP allows.
        let flags = create_comp_flags_from_zip_params(6, -15, 0);
        let mut c = CompressorOxide::new(flags);
        let mut first = vec![0u8; 4096];
        let (_, _, n1) = compress(&mut c, x, &mut first, TDEFLFlush::Sync);
        first.truncate(n1);
        // A sync flush ends on a byte boundary; a final empty block closes
        // the first stream: the static final empty block (0x03 0x00).
        first.extend([0x03, 0x00]);
        let mut second = vec![0u8; 4096];
        let (_, _, n2) = compress(&mut c, y, &mut second, TDEFLFlush::Finish);
        second.truncate(n2);
        let mut cab = build(&[("t.txt", &text)], 1000, true);
        // Replace the two blocks' payloads: rebuild by hand.
        let data_at = u32_at(&cab, 36).unwrap() as usize;
        cab.truncate(data_at);
        for (p, u) in [(&first, x.len()), (&second, y.len())] {
            let mut payload = b"CK".to_vec();
            payload.extend(p.iter());
            cab.extend(0u32.to_le_bytes());
            cab.extend((payload.len() as u16).to_le_bytes());
            cab.extend((u as u16).to_le_bytes());
            cab.extend(payload);
        }
        cab[40..42].copy_from_slice(&2u16.to_le_bytes());
        let f = files(cab);
        assert_eq!(f.read("t.txt").unwrap(), text);
        assert_eq!(f.read_at("t.txt", 1500, 20).unwrap(), text[1500..1520]);
    }
}
