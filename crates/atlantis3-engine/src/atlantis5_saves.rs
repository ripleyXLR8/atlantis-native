//! Where *Atlantis V*'s saves are kept (`saves/autoslot`, `saves/slot_<n>`):
//! a folder on the desktop ([`DirSaves`]), the browser's storage in the
//! web build (its own [`SaveFiles`]).

use std::path::PathBuf;

/// The player's save files, by name (`autoslot`, `slot_0` ..).
pub trait SaveFiles {
    /// The file's bytes; `None` when it is not there.
    fn read(&self, name: &str) -> Option<Vec<u8>>;
    /// Writes the file whole; false when it could not be kept.
    fn write(&self, name: &str, data: &[u8]) -> bool;
    /// Deletes the file (nothing when it is not there).
    fn remove(&self, name: &str);
    /// Whether the file is there.
    fn exists(&self, name: &str) -> bool;
    /// When the file was last written (seconds since 1970, UTC; 0 when
    /// unknown); `None` when it is not there.
    fn modified(&self, name: &str) -> Option<i64>;
    /// Copies file `from` to `to` (nothing when `from` is not there).
    fn copy(&self, from: &str, to: &str) {
        if let Some(d) = self.read(from) {
            self.write(to, &d);
        }
    }
}

/// The saves in a folder of the player's disk.
pub struct DirSaves(pub PathBuf);

impl SaveFiles for DirSaves {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(name)).ok()
    }

    fn write(&self, name: &str, data: &[u8]) -> bool {
        std::fs::create_dir_all(&self.0).is_ok() && std::fs::write(self.0.join(name), data).is_ok()
    }

    fn remove(&self, name: &str) {
        let p = self.0.join(name);
        if p.is_file() {
            let _ = std::fs::remove_file(p);
        }
    }

    fn exists(&self, name: &str) -> bool {
        self.0.join(name).is_file()
    }

    fn modified(&self, name: &str) -> Option<i64> {
        let md = std::fs::metadata(self.0.join(name)).ok()?;
        Some(
            md.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0)),
        )
    }

    fn copy(&self, from: &str, to: &str) {
        let (f, t) = (self.0.join(from), self.0.join(to));
        if f.is_file() {
            let _ = std::fs::copy(f, t);
        }
    }
}
