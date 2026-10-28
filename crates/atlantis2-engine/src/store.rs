//! Where the engine keeps its own files: the saves (`save/ATLpp_ss.SAV`),
//! the original's options file `atlant2.cfg` and the music copies
//! (`music_copies.txt`). Any [`SaveStore`] holds them, by those relative
//! paths: a folder on disk ([`DirFiles`], the desktop's per-user folder),
//! the browser's storage (the web build's), memory (tests).

use std::path::{Path, PathBuf};

pub use atlantis_engine::SaveStore;

/// The saves' folder in the store.
pub const SAVE_FOLDER: &str = "save";

/// Files of a folder on disk, by paths relative to it (`/` between their
/// parts), the folders made on the first write. Names are kept as given:
/// `save/ATL04_00.SAV` is the file the original writes.
#[derive(Clone, Debug)]
pub struct DirFiles {
    dir: PathBuf,
}

impl DirFiles {
    pub fn new(dir: impl Into<PathBuf>) -> DirFiles {
        DirFiles { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file of `name` on disk.
    pub fn path(&self, name: &str) -> PathBuf {
        name.split(['/', '\\'])
            .filter(|p| !p.is_empty())
            .fold(self.dir.clone(), |p, part| p.join(part))
    }
}

impl SaveStore for DirFiles {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(name)).ok()
    }

    fn write(&self, name: &str, data: &[u8]) -> bool {
        let path = self.path(name);
        let made = match path.parent() {
            Some(parent) => std::fs::create_dir_all(parent),
            None => Ok(()),
        };
        match made.and_then(|()| std::fs::write(&path, data)) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                false
            }
        }
    }

    fn remove(&self, name: &str) {
        let path = self.path(name);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("{}: {e}", path.display());
            }
        }
    }

    /// The files of the folder and of its `save` folder (lower case).
    fn list(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (sub, prefix) in [("", ""), (SAVE_FOLDER, "save/")] {
            let Ok(rd) = std::fs::read_dir(self.path(sub)) else {
                continue;
            };
            for e in rd.flatten() {
                if e.path().is_file() {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    out.push(format!("{prefix}{n}"));
                }
            }
        }
        out.sort();
        out
    }
}

/// The path of a save in the store.
pub fn save_path(file_name: &str) -> String {
    format!("{SAVE_FOLDER}/{file_name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_by_relative_paths() {
        let dir = std::env::temp_dir().join(format!("a2-dirfiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = DirFiles::new(&dir);
        assert_eq!(s.read("atlant2.cfg"), None);
        assert!(s.write("atlant2.cfg", b"cfg"));
        assert!(s.write(&save_path("ATL04_00.SAV"), b"save"));
        // The name kept as given, in its folder.
        assert!(dir.join("save").join("ATL04_00.SAV").is_file());
        assert_eq!(s.read("save/ATL04_00.SAV").as_deref(), Some(&b"save"[..]));
        assert_eq!(s.list(), ["atlant2.cfg", "save/atl04_00.sav"]);
        s.remove("save/ATL04_00.SAV");
        s.remove("save/ATL04_01.SAV");
        assert_eq!(s.read("save/ATL04_00.SAV"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
