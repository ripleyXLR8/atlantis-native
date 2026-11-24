//! Where the game's files come from: a directory on desktop, files the
//! player gives the browser on the web.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Random access to the player's files, by name relative to the game
/// directory (`\` or `/` separated, any case).
pub trait FileSource: Send + Sync {
    /// Size of a file, `None` when it does not exist.
    fn size(&self, name: &str) -> Option<u64>;
    /// `len` bytes at `offset` of a file.
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>>;
    /// Whether a directory exists (the GOG release ships its files unpacked).
    fn has_dir(&self, name: &str) -> bool;

    /// A hint that bytes `offset..offset + len` of `name` are read next
    /// (a cinematic read by parts): the web build reads them in the
    /// background meanwhile. Nothing by default; a source that wraps
    /// another passes it on.
    fn read_ahead(&self, name: &str, offset: u64, len: usize) {
        let _ = (name, offset, len);
    }

    /// The whole file.
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let size = self.size(name)?;
        self.read_at(name, 0, usize::try_from(size).ok()?)
    }

    /// A file of the player's beside the game's files, not one of the
    /// game's (a font of their Windows): in the game directory, or beside
    /// the images of the discs.
    fn read_beside(&self, name: &str) -> Option<Vec<u8>> {
        self.read(name)
    }

    /// The names of the entries of folder `dir` (`""`: the top), for the
    /// lookups that need more than a case-insensitive match (Atlantis II's
    /// names with accents); empty when the source cannot list them.
    fn list(&self, dir: &str) -> Vec<String> {
        let _ = dir;
        Vec::new()
    }
}

/// How many files a [`StdDir`] keeps open for its ranged reads.
const OPEN_FILES: usize = 16;

/// Files of a directory on disk. Names are matched without regard to case
/// (the game names its files in upper and lower case freely).
pub struct StdDir {
    root: PathBuf,
    // lower-case relative path -> real path; filled on first use of a directory
    index: Mutex<HashMap<String, PathBuf>>,
    /// The files last read by parts, the most recent last: a film or a
    /// cabinet read piece by piece keeps its handle, and a game that reads
    /// thousands of files does not run out of descriptors (256 by default
    /// on macOS, 1024 on Linux desktops).
    open: Mutex<VecDeque<(PathBuf, File)>>,
}

impl StdDir {
    pub fn new(root: impl Into<PathBuf>) -> StdDir {
        StdDir {
            root: root.into(),
            index: Mutex::new(HashMap::new()),
            open: Mutex::new(VecDeque::new()),
        }
    }

    fn key(name: &str) -> String {
        name.replace('\\', "/")
            .trim_matches('/')
            .to_ascii_lowercase()
    }

    /// Resolves `name` component by component, ignoring case.
    fn resolve(&self, name: &str) -> Option<PathBuf> {
        let key = Self::key(name);
        if let Some(p) = self.index.lock().unwrap().get(&key) {
            return Some(p.clone());
        }
        let mut path = self.root.clone();
        for part in key.split('/').filter(|p| !p.is_empty()) {
            let found = std::fs::read_dir(&path)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == part)?;
            path = found.path();
        }
        self.index.lock().unwrap().insert(key, path.clone());
        Some(path)
    }
}

impl FileSource for StdDir {
    fn size(&self, name: &str) -> Option<u64> {
        let p = self.resolve(name)?;
        let m = std::fs::metadata(p).ok()?;
        m.is_file().then_some(m.len())
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let p = self.resolve(name)?;
        let mut open = self.open.lock().unwrap();
        let entry = match open.iter().position(|(q, _)| *q == p) {
            Some(i) => open.remove(i)?,
            None => {
                if open.len() >= OPEN_FILES {
                    open.pop_front();
                }
                (p.clone(), File::open(&p).ok()?)
            }
        };
        open.push_back(entry);
        let (_, f) = open.back_mut()?;
        f.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0; len];
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    /// The whole file, read without keeping it open.
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let p = self.resolve(name)?;
        if !std::fs::metadata(&p).ok()?.is_file() {
            return None;
        }
        std::fs::read(p).ok()
    }

    fn has_dir(&self, name: &str) -> bool {
        self.resolve(name).is_some_and(|p| Path::new(&p).is_dir())
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let path = if Self::key(dir).is_empty() {
            Some(self.root.clone())
        } else {
            self.resolve(dir)
        };
        path.and_then(|p| std::fs::read_dir(p).ok())
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Files held in memory (the web build, tests).
#[derive(Default)]
pub struct MemFiles {
    files: HashMap<String, Vec<u8>>,
}

impl MemFiles {
    pub fn new() -> MemFiles {
        MemFiles::default()
    }

    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.files.insert(StdDir::key(name), data);
    }
}

impl FileSource for MemFiles {
    fn size(&self, name: &str) -> Option<u64> {
        self.files.get(&StdDir::key(name)).map(|d| d.len() as u64)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let d = self.files.get(&StdDir::key(name))?;
        let start = usize::try_from(offset).ok()?;
        d.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }

    fn has_dir(&self, name: &str) -> bool {
        let prefix = StdDir::key(name) + "/";
        self.files.keys().any(|k| k.starts_with(&prefix))
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let key = StdDir::key(dir);
        let prefix = if key.is_empty() { key } else { key + "/" };
        let mut out: Vec<String> = self
            .files
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_dir_keeps_few_files_open() {
        let base =
            std::env::var_os("ATLANTIS_NATIVE_DATA").map_or_else(std::env::temp_dir, PathBuf::from);
        let dir = base.join(format!("an-stddir-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Sub")).unwrap();
        let n = OPEN_FILES * 3;
        for i in 0..n {
            std::fs::write(dir.join("Sub").join(format!("F{i}.BIN")), [i as u8; 8]).unwrap();
        }
        let d = StdDir::new(&dir);
        for i in 0..n {
            let name = format!("sub/f{i}.bin");
            assert_eq!(d.read(&name), Some(vec![i as u8; 8]));
            assert_eq!(d.read_at(&name, 2, 3), Some(vec![i as u8; 3]));
        }
        assert!(d.open.lock().unwrap().len() <= OPEN_FILES);
        // The handles kept still read right, the oldest reopened.
        assert_eq!(d.read_at("sub/f0.bin", 7, 1), Some(vec![0]));
        assert_eq!(
            d.read_at(&format!("sub/f{}.bin", n - 1), 0, 8),
            Some(vec![(n - 1) as u8; 8])
        );
        assert_eq!(d.read_at("sub/f0.bin", 7, 2), None, "past the end");
        assert_eq!(d.read("sub"), None, "a folder");
        assert_eq!(d.read("sub/none.bin"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
