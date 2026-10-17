//! Where the game's files come from: a directory on desktop, files the
//! player gives the browser on the web.

use std::collections::HashMap;
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

    /// The whole file.
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        let size = self.size(name)?;
        self.read_at(name, 0, usize::try_from(size).ok()?)
    }
}

/// Files of a directory on disk. Names are matched without regard to case
/// (the game names its files in upper and lower case freely).
pub struct StdDir {
    root: PathBuf,
    // lower-case relative path -> real path; filled on first use of a directory
    index: Mutex<HashMap<String, PathBuf>>,
    open: Mutex<HashMap<PathBuf, File>>,
}

impl StdDir {
    pub fn new(root: impl Into<PathBuf>) -> StdDir {
        StdDir {
            root: root.into(),
            index: Mutex::new(HashMap::new()),
            open: Mutex::new(HashMap::new()),
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
        if !open.contains_key(&p) {
            open.insert(p.clone(), File::open(&p).ok()?);
        }
        let f = open.get_mut(&p)?;
        f.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0; len];
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.resolve(name).is_some_and(|p| Path::new(&p).is_dir())
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
}
