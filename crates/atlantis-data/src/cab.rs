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
//! so a later read starts from the nearest point before it; with an index
//! folder ([`set_index_dir`]) those checkpoints and the blocks' sizes are
//! kept in files between runs.
//!
//! On a disc every read is a seek: the cabinet's file is read by aligned
//! parts of [`CHUNK`] bytes, the last few kept, and the block headers are
//! walked as the decoding reaches them, so a folder's bytes are read once
//! in large parts (the French DVD's start: 441 reads of 1 MiB, where one
//! read per header and per block made 35,584).

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
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

impl Block {
    fn end(&self) -> u64 {
        self.start + u64::from(self.unpacked)
    }
}

#[derive(Default)]
struct FolderState {
    /// The blocks walked so far, from the first: the headers are read
    /// only as far as a read needs (a header per block, each a seek on
    /// a disc, 32,489 of them in *Atlantis V*'s largest folder).
    blocks: Vec<Block>,
    /// The header of the next block to walk.
    next_at: u64,
    /// The index file has been looked for.
    prepared: bool,
    /// Block index → the last 32 KiB of output before it.
    checkpoints: HashMap<usize, Vec<u8>>,
    /// The last block decoded: its index, its output, the history before
    /// it.
    last: Option<(usize, Vec<u8>, Vec<u8>)>,
    /// Where the checkpoints are kept between runs, when they are.
    index: Option<IndexFile>,
}

struct Folder {
    first_data: u64,
    blocks: u16,
    compression: u16,
    state: Mutex<FolderState>,
}

/// The cabinet's file is read by aligned parts of this many bytes, the
/// last [`CHUNKS_KEPT`] kept: the small reads (an 8-byte block header, a
/// block's few KiB) come from them, so a disc sees few, large reads.
pub const CHUNK: u64 = 1 << 20;

/// The parts kept per cabinet.
const CHUNKS_KEPT: usize = 16;

/// The folder where the cabinets' checkpoint indexes are kept, for the
/// cabinets opened after it is set (`None`: none kept).
static INDEX_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Keeps the folders' checkpoints (the 32 KiB of output before every
/// [`CHECKPOINT_BLOCKS`]-th block, and the blocks' sizes) in `dir`
/// between runs, for the cabinets opened from now on: a read deep in a
/// large folder then starts from the nearest checkpoint a previous run
/// passed instead of the folder's start. `None` keeps none.
pub fn set_index_dir(dir: Option<PathBuf>) {
    *INDEX_DIR.lock().unwrap() = dir;
}

/// One cabinet, read through `src` (its file `path` there).
pub struct Cabinet {
    src: Arc<dyn FileSource>,
    path: String,
    size: u64,
    data_reserve: u64,
    folders: Vec<Folder>,
    files: Vec<CabFile>,
    /// The parts of the file read last (offset, bytes), oldest first.
    chunks: Mutex<VecDeque<(u64, Arc<Vec<u8>>)>>,
    /// Where the checkpoint indexes are kept.
    index_dir: Option<PathBuf>,
    /// A hash of the cabinet's header tables, naming its indexes.
    identity: u64,
}

/// FNV-1a, 64 bits.
fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A folder's checkpoints in a file: a header naming the cabinet and the
/// folder, then records (`B`: block sizes from a block on; `C`: a
/// checkpoint), each its tag, u32 length, the payload and the FNV-1a of
/// tag and payload. Appended as a run decodes; a damaged tail is dropped.
struct IndexFile {
    file: std::fs::File,
    /// The blocks written so far.
    saved: usize,
}

const INDEX_MAGIC: &[u8; 8] = b"ANCABIX1";

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
        // A file list past the end (a copy cut short, a damaged header).
        let len = end.checked_sub(files_at).ok_or(t.clone())?;
        let list = src
            .read_at(path, files_at, usize::try_from(len).map_err(|_| t.clone())?)
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
        let mut id = table.clone();
        id.extend_from_slice(&list);
        Ok(Cabinet {
            src,
            path: path.to_string(),
            size,
            data_reserve,
            folders,
            files,
            chunks: Mutex::new(VecDeque::new()),
            index_dir: INDEX_DIR.lock().unwrap().clone(),
            identity: fnv(&id),
        })
    }

    /// The files, in the cabinet's order.
    pub fn files(&self) -> &[CabFile] {
        &self.files
    }

    /// Keeps this cabinet's checkpoint indexes in `dir` (`None`: none),
    /// in place of the folder [`set_index_dir`] gave when it was opened.
    /// For the folders not read yet.
    pub fn use_index_dir(&mut self, dir: Option<PathBuf>) {
        self.index_dir = dir;
    }

    /// `len` bytes at `at` of the cabinet's file, through the kept parts
    /// (a read larger than two parts is made directly).
    fn read_raw(&self, at: u64, len: usize) -> Option<Vec<u8>> {
        let end = at.checked_add(len as u64)?;
        if end > self.size {
            return None;
        }
        if len as u64 > 2 * CHUNK {
            return self.src.read_at(&self.path, at, len);
        }
        let mut out = Vec::with_capacity(len);
        let mut pos = at;
        while pos < end {
            let c = pos / CHUNK * CHUNK;
            let chunk = self.chunk(c)?;
            let lo = (pos - c) as usize;
            let hi = ((end - c) as usize).min(chunk.len());
            if hi <= lo {
                return None;
            }
            out.extend_from_slice(&chunk[lo..hi]);
            pos = c + hi as u64;
        }
        Some(out)
    }

    /// The part of the file at `c` (a multiple of [`CHUNK`]).
    fn chunk(&self, c: u64) -> Option<Arc<Vec<u8>>> {
        let mut kept = self.chunks.lock().unwrap();
        if let Some(i) = kept.iter().position(|e| e.0 == c) {
            let e = kept.remove(i)?;
            let data = e.1.clone();
            kept.push_back(e);
            return Some(data);
        }
        let n = CHUNK.min(self.size - c) as usize;
        let data = Arc::new(self.src.read_at(&self.path, c, n)?);
        if kept.len() >= CHUNKS_KEPT {
            kept.pop_front();
        }
        kept.push_back((c, data.clone()));
        Some(data)
    }

    /// Reads the next block's header; false after the folder's last
    /// block. The headers are walked as the decoding reaches them, so
    /// that a disc's parts are read once for both.
    fn walk_one(&self, folder: &Folder, st: &mut FolderState) -> Option<bool> {
        if st.blocks.len() >= usize::from(folder.blocks) {
            return Some(false);
        }
        if st.blocks.is_empty() {
            st.next_at = folder.first_data;
        }
        let at = st.next_at;
        let h = self.read_raw(at, 8)?;
        let packed = u16_at(&h, 4)?;
        let unpacked = u16_at(&h, 6)?;
        let data_at = at + 8 + self.data_reserve;
        let start = st.blocks.last().map_or(0, Block::end);
        st.blocks.push(Block {
            data_at,
            packed,
            unpacked,
            start,
        });
        st.next_at = data_at + u64::from(packed);
        Some(true)
    }

    /// The index file's name for folder `k`.
    fn index_path(&self, dir: &Path, k: usize) -> PathBuf {
        let name: String = self
            .path
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        dir.join(format!(
            "{name}-{:x}-{:016x}-f{k}.idx",
            self.size, self.identity
        ))
    }

    /// The index file's header for folder `k`.
    fn index_header(&self, folder: &Folder, k: usize) -> Vec<u8> {
        let mut h = INDEX_MAGIC.to_vec();
        h.extend(self.size.to_le_bytes());
        h.extend(self.identity.to_le_bytes());
        h.extend((k as u32).to_le_bytes());
        h.extend(folder.first_data.to_le_bytes());
        h.extend(folder.blocks.to_le_bytes());
        h.extend(folder.compression.to_le_bytes());
        h
    }

    /// On a folder's first use: its index file read (the blocks and
    /// checkpoints a previous run kept) and kept open to add to. Only an
    /// MSZIP folder larger than a checkpoint's span has one.
    fn prepare(&self, k: usize, folder: &Folder, st: &mut FolderState) {
        if st.prepared {
            return;
        }
        st.prepared = true;
        let Some(dir) = &self.index_dir else { return };
        if folder.compression != 1 || usize::from(folder.blocks) <= CHECKPOINT_BLOCKS {
            return;
        }
        let path = self.index_path(dir, k);
        let header = self.index_header(folder, k);
        let old = std::fs::read(&path).unwrap_or_default();
        let mut good = 0;
        if old.starts_with(&header) {
            good = header.len();
            st.next_at = folder.first_data;
            // The records, up to the first damaged or out-of-order one.
            while let Some((tag, payload, next)) = index_record(&old, good) {
                let ok = match tag {
                    b'B' => self.load_blocks(folder, st, payload),
                    b'C' => load_checkpoint(st, payload),
                    _ => false,
                };
                if !ok {
                    break;
                }
                good = next;
            }
        }
        // The file is never cut in place: another instance of the game may
        // be adding to it (a record it is writing reads as damaged here).
        // What is kept goes to a file of this process's, renamed over the
        // index; a writer still holding the old file then adds to that one
        // alone. Records are appended (O_APPEND), so two writers of one
        // file never overwrite each other; at worst the loader stops at a
        // record out of order and the cache is shorter.
        let opened = std::fs::create_dir_all(dir).and_then(|()| {
            if good == 0 || good < old.len() {
                let keep = if good == 0 { &header[..] } else { &old[..good] };
                let tmp = path.with_extension(format!("idx.{}.tmp", process_tag()));
                let written =
                    std::fs::write(&tmp, keep).and_then(|()| std::fs::rename(&tmp, &path));
                if let Err(e) = written {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e);
                }
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
        });
        match opened {
            Ok(file) => {
                log::debug!(
                    "{}: folder {k}: {} blocks and {} checkpoints from {}",
                    self.path,
                    st.blocks.len(),
                    st.checkpoints.len(),
                    path.display()
                );
                st.index = Some(IndexFile {
                    file,
                    saved: st.blocks.len(),
                })
            }
            Err(e) => log::warn!("{}: {e}", path.display()),
        }
    }

    /// A `B` record: blocks from `first` on (u16 packed, u16 unpacked).
    fn load_blocks(&self, folder: &Folder, st: &mut FolderState, p: &[u8]) -> bool {
        let Some(first) = u32_at(p, 0) else {
            return false;
        };
        let sizes = &p[4..];
        if first as usize != st.blocks.len()
            || !sizes.len().is_multiple_of(4)
            || st.blocks.len() + sizes.len() / 4 > usize::from(folder.blocks)
        {
            return false;
        }
        for s in sizes.as_chunks::<4>().0 {
            let (packed, unpacked) = (
                u16::from_le_bytes([s[0], s[1]]),
                u16::from_le_bytes([s[2], s[3]]),
            );
            let data_at = st.next_at + 8 + self.data_reserve;
            let start = st.blocks.last().map_or(0, Block::end);
            st.blocks.push(Block {
                data_at,
                packed,
                unpacked,
                start,
            });
            st.next_at = data_at + u64::from(packed);
        }
        true
    }

    /// Adds the checkpoint at block `b` (and the blocks walked since the
    /// last record) to the folder's index file.
    fn save_checkpoint(&self, st: &mut FolderState, b: usize, history: &[u8]) {
        let Some(ix) = &mut st.index else { return };
        let mut out = Vec::new();
        if ix.saved < st.blocks.len() {
            let mut p = (ix.saved as u32).to_le_bytes().to_vec();
            for blk in &st.blocks[ix.saved..] {
                p.extend(blk.packed.to_le_bytes());
                p.extend(blk.unpacked.to_le_bytes());
            }
            push_record(&mut out, b'B', &p);
        }
        let mut p = (b as u32).to_le_bytes().to_vec();
        p.extend_from_slice(history);
        push_record(&mut out, b'C', &p);
        match ix.file.write_all(&out) {
            Ok(()) => ix.saved = st.blocks.len(),
            Err(e) => {
                log::warn!("{}: the checkpoint index: {e}", self.path);
                st.index = None;
            }
        }
    }

    /// Decodes block `b` after `history` (the 32 KiB before it).
    fn decode(&self, folder: &Folder, b: &Block, history: &[u8]) -> Option<Vec<u8>> {
        let packed = self.read_raw(b.data_at, usize::from(b.packed))?;
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
        let mut guard = folder.state.lock().unwrap();
        let st = &mut *guard;
        let mut out = Vec::with_capacity(len);
        if len == 0 {
            return Some(out);
        }
        let end = offset.checked_add(len as u64)?;
        self.prepare(k, folder, st);
        // The first block holding `offset`, when walked already (else
        // the number walked: it lies beyond).
        let first = st.blocks.partition_point(|b| b.end() <= offset);
        // Where to start decoding: a stored folder's block itself; else
        // the last block decoded, or the nearest checkpoint before
        // `first`.
        let mut from = if folder.compression == 0 {
            first
        } else {
            let mut from = (first / CHECKPOINT_BLOCKS) * CHECKPOINT_BLOCKS;
            while from > 0 && !st.checkpoints.contains_key(&from) {
                from -= CHECKPOINT_BLOCKS;
            }
            from
        };
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
        loop {
            if b == st.blocks.len() && !self.walk_one(folder, st)? {
                break;
            }
            let block = st.blocks[b];
            if block.start >= end {
                break;
            }
            if folder.compression == 0 && block.end() <= offset {
                // A stored block before the bytes asked: not read.
                b += 1;
                continue;
            }
            if folder.compression != 0
                && b.is_multiple_of(CHECKPOINT_BLOCKS)
                && b > 0
                && !st.checkpoints.contains_key(&b)
            {
                st.checkpoints.insert(b, history.clone());
                self.save_checkpoint(st, b, &history);
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

    /// Decodes folder `k` toward its uncompressed offset `target`, at
    /// most about `budget` bytes further than the decoding has gone; how
    /// far it has gone then (at least `target` once reached, a stored
    /// folder at once).
    fn warm_folder(&self, k: usize, target: u64, budget: u64) -> Option<u64> {
        let folder = self.folders.get(k)?;
        if folder.compression == 0 {
            return Some(target);
        }
        // How far the decoding has gone: the last block decoded, or the
        // furthest checkpoint (a read from there needs nothing before it).
        let (reached, raw) = {
            let st = folder.state.lock().unwrap();
            let last = st
                .last
                .as_ref()
                .and_then(|(i, _, _)| st.blocks.get(*i))
                .map_or(0, Block::end);
            let kept = st
                .checkpoints
                .keys()
                .max()
                .and_then(|&b| st.blocks.get(b))
                .map_or(0, |b| b.start);
            // The next block header to walk: where the decoding reads on.
            let raw = if st.blocks.is_empty() {
                folder.first_data
            } else {
                st.next_at
            };
            (last.max(kept), raw)
        };
        if reached > target {
            return Some(reached);
        }
        // The cabinet's part after the one being read, asked ahead (the
        // web build reads it in the background, so that the next steps
        // find it there instead of waiting for it).
        let ahead = (raw / CHUNK + 1) * CHUNK;
        if ahead < self.size {
            self.src
                .read_ahead(&self.path, ahead, CHUNK.min(self.size - ahead) as usize);
        }
        let next = target.min(reached.saturating_add(budget));
        self.read_folder(k, next, 1)?;
        Some(next)
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

/// This process's number, for its temporary files (none on the web).
fn process_tag() -> u32 {
    #[cfg(not(target_family = "wasm"))]
    {
        std::process::id()
    }
    #[cfg(target_family = "wasm")]
    {
        0
    }
}

/// The record at `at` of an index file: its tag, payload and the next
/// record's offset; `None` at the end or on a damaged record.
fn index_record(d: &[u8], at: usize) -> Option<(u8, &[u8], usize)> {
    let tag = *d.get(at)?;
    let n = u32_at(d, at + 1)? as usize;
    let p_end = (at + 5).checked_add(n)?;
    let payload = d.get(at + 5..p_end)?;
    let sum = u64::from_le_bytes(d.get(p_end..p_end + 8)?.try_into().ok()?);
    let mut h = vec![tag];
    h.extend_from_slice(payload);
    (fnv(&h) == sum).then_some((tag, payload, p_end + 8))
}

fn push_record(out: &mut Vec<u8>, tag: u8, payload: &[u8]) {
    let mut h = vec![tag];
    h.extend_from_slice(payload);
    out.push(tag);
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out.extend(fnv(&h).to_le_bytes());
}

/// A `C` record: a block index (a multiple of [`CHECKPOINT_BLOCKS`],
/// walked) and the history before it.
fn load_checkpoint(st: &mut FolderState, p: &[u8]) -> bool {
    let Some(b) = u32_at(p, 0).map(|b| b as usize) else {
        return false;
    };
    let history = &p[4..];
    if b == 0
        || !b.is_multiple_of(CHECKPOINT_BLOCKS)
        || b >= st.blocks.len()
        || history.len() > WINDOW
    {
        return false;
    }
    st.checkpoints.insert(b, history.to_vec());
    true
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

    /// Decodes the folder holding `name` toward the file's start, at most
    /// about `budget` bytes further than its decoding has gone, so that
    /// reading it later does not decode the whole folder before it at
    /// once (the web build's start, a step per frame). True once the
    /// decoding has reached it (or it needs none); `None` for a name
    /// not in the cabinets.
    pub fn warm(&self, name: &str, budget: u64) -> Option<bool> {
        self.warm_progress(name, budget).map(|(at, to)| at >= to)
    }

    /// [`CabFiles::warm`] with how far: (the folder decoded up to, the
    /// file's offset in it).
    pub fn warm_progress(&self, name: &str, budget: u64) -> Option<(u64, u64)> {
        let (c, i) = *self.index.get(&key(name))?;
        let cab = &self.cabinets[c];
        let f = &cab.files[i];
        let to = u64::from(f.offset);
        if f.size == 0 {
            return Some((to, to));
        }
        let at = cab.warm_folder(usize::from(f.folder), to, budget)?;
        Some((at, to))
    }

    /// Decodes about `budget` bytes more of the first folder not yet
    /// decoded to its end, the smallest folders first: a later read
    /// anywhere in them then starts from a checkpoint near it. False once
    /// every folder has been (the web build, between frames).
    pub fn warm_next(&self, budget: u64) -> bool {
        let mut folders: Vec<(u64, usize, usize)> = Vec::new();
        for (c, cab) in self.cabinets.iter().enumerate() {
            let mut ends = vec![0u64; cab.folders.len()];
            for f in &cab.files {
                let e = u64::from(f.offset) + u64::from(f.size);
                let end = &mut ends[usize::from(f.folder)];
                *end = (*end).max(e);
            }
            for (k, end) in ends.into_iter().enumerate() {
                if end > 0 {
                    folders.push((end, c, k));
                }
            }
        }
        folders.sort_unstable();
        for (end, c, k) in folders {
            match self.cabinets[c].warm_folder(k, end - 1, budget) {
                Some(at) if at >= end - 1 => {}
                Some(_) => return true,
                // A folder that cannot be read: left to the game's reads.
                None => {}
            }
        }
        false
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
    fn a_file_list_past_the_end_is_truncated() {
        let cab = build(&[("a.txt", b"abc")], 100, false);
        // The header and the folder table only: coffFiles (44) lies past
        // the end.
        for cut in [36, 40, 44] {
            let mut m = MemFiles::new();
            m.insert("x.cab", cab[..cut].to_vec());
            assert!(matches!(
                Cabinet::open(Arc::new(m), "x.cab"),
                Err(CabError::Truncated)
            ));
        }
        // A corrupt coffFiles far beyond the file.
        let mut bad = cab.clone();
        bad[16..20].copy_from_slice(&0x7fff_0000u32.to_le_bytes());
        let mut m = MemFiles::new();
        m.insert("x.cab", bad);
        assert!(matches!(
            Cabinet::open(Arc::new(m), "x.cab"),
            Err(CabError::Truncated)
        ));
    }

    /// A source that counts the bytes read.
    struct Counted(MemFiles, std::sync::atomic::AtomicU64);

    impl FileSource for Counted {
        fn size(&self, name: &str) -> Option<u64> {
            self.0.size(name)
        }
        fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
            self.1
                .fetch_add(len as u64, std::sync::atomic::Ordering::Relaxed);
            self.0.read_at(name, offset, len)
        }
        fn has_dir(&self, name: &str) -> bool {
            self.0.has_dir(name)
        }
    }

    #[test]
    fn checkpoints_kept_between_runs() {
        // 300 MSZIP blocks of 4 KiB: 4 checkpoints, a 1.2 MB folder over
        // two parts.
        let a: Vec<u8> = (0..1_228_800u32).map(|i| (i * 13 % 241) as u8).collect();
        let cab = build(&[("big.bin", &a), ("tail.txt", b"end")], 4096, true);
        let dir = std::env::temp_dir().join(format!("an-cab-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let open = || {
            let mut m = MemFiles::new();
            m.insert("x.cab", cab.clone());
            let src = Arc::new(Counted(m, Default::default()));
            let mut c = Cabinet::open(src.clone(), "x.cab").unwrap();
            c.use_index_dir(Some(dir.clone()));
            (CabFiles::new(vec![c]), src)
        };
        let read = |f: &CabFiles, src: &Counted| {
            let before = src.1.load(std::sync::atomic::Ordering::Relaxed);
            let got = f.read_at("big.bin", 1_228_000, 800).unwrap();
            assert_eq!(got, a[1_228_000..]);
            assert_eq!(f.read("tail.txt").unwrap(), b"end");
            src.1.load(std::sync::atomic::Ordering::Relaxed) - before
        };
        let (f, src) = open();
        let cold = read(&f, &src);
        drop(f);
        let (f, src) = open();
        let warm = read(&f, &src);
        // From the last checkpoint (block 256): the second part only.
        assert!(cold as usize > a.len(), "{cold}");
        assert!(warm <= CHUNK, "{warm}");
        assert_eq!(f.read("big.bin").unwrap(), a);
        drop(f);
        // A damaged tail is dropped, the records before it kept.
        let file = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let mut bytes = std::fs::read(&file).unwrap();
        let n = bytes.len();
        bytes[n - 3] ^= 0xff;
        std::fs::write(&file, &bytes).unwrap();
        let (f, src) = open();
        read(&f, &src);
        assert_eq!(f.read("big.bin").unwrap(), a);
        // Cut at the damaged checkpoint, which was written again.
        assert_eq!(std::fs::read(&file).unwrap().len(), n);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A second instance opening the index while the first one is in the
    /// middle of a record does not cut the file under the first one: what
    /// the first adds afterwards cannot leave a hole in the index.
    #[test]
    fn a_second_instance_never_cuts_a_live_index() {
        let a: Vec<u8> = (0..1_228_800u32).map(|i| (i * 13 % 241) as u8).collect();
        let cab = build(&[("big.bin", &a)], 4096, true);
        let base =
            std::env::var_os("ATLANTIS_NATIVE_DATA").map_or_else(std::env::temp_dir, PathBuf::from);
        let dir = base.join(format!("an-cab-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let open = || {
            let mut m = MemFiles::new();
            m.insert("x.cab", cab.clone());
            let mut c = Cabinet::open(Arc::new(m), "x.cab").unwrap();
            c.use_index_dir(Some(dir.clone()));
            CabFiles::new(vec![c])
        };
        let f = open();
        assert_eq!(f.read("big.bin").unwrap(), a);
        drop(f);
        let file = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let whole = std::fs::read(&file).unwrap();
        // The first instance, its file open at the end, half-way through
        // a record.
        let mut first = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
        std::io::Seek::seek(&mut first, std::io::SeekFrom::End(0)).unwrap();
        let mut rec = Vec::new();
        push_record(&mut rec, b'C', &[7u8; 64]);
        first.write_all(&rec[..20]).unwrap();
        // The second instance opens the folder's index.
        let g = open();
        assert_eq!(g.read_at("big.bin", 0, 10).unwrap(), a[..10]);
        // The first one ends its record and adds another.
        first.write_all(&rec[20..]).unwrap();
        first.write_all(&rec).unwrap();
        drop(first);
        drop(g);
        // The index reads to its end (no hole), with the records it had.
        let mut m = MemFiles::new();
        m.insert("x.cab", cab.clone());
        let c = Cabinet::open(Arc::new(m), "x.cab").unwrap();
        let mut at = c.index_header(&c.folders[0], 0).len();
        let now = std::fs::read(&file).unwrap();
        assert!(now.starts_with(&whole));
        while at < now.len() {
            match index_record(&now, at) {
                Some((_, _, next)) => at = next,
                None => panic!("a damaged record at {at} of {}", now.len()),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn warm_reaches_the_file_by_steps() {
        let a: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let b = b"the file after".to_vec();
        let cab = build(&[("a.bin", &a), ("b.bin", &b)], 4096, true);
        let files = files(cab);
        let mut steps = 0;
        while !files.warm("b.bin", 50_000).unwrap() {
            steps += 1;
            assert!(steps < 20);
        }
        assert!(steps >= 5);
        assert_eq!(files.read("b.bin").unwrap(), b);
        assert_eq!(files.warm("b.bin", 1), Some(true));
        assert_eq!(files.warm("none", 1), None);
        // The folder whole, by steps.
        let mut steps = 0;
        while files.warm_next(100_000) {
            steps += 1;
            assert!(steps < 20);
        }
        assert!(!files.warm_next(100_000));
        assert_eq!(files.read_at("a.bin", 1000, 10).unwrap(), &a[1000..1010]);
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
