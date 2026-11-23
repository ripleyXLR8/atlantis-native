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

    /// Written whole to `<name>.tmp` beside it, then renamed over it: a
    /// write cut short (a full disk, a crash) leaves the earlier file as
    /// it was.
    fn write(&self, name: &str, data: &[u8]) -> bool {
        use std::io::Write;
        if std::fs::create_dir_all(&self.0).is_err() {
            return false;
        }
        let path = self.0.join(name);
        let tmp = self.0.join(format!("{name}.tmp"));
        let written = std::fs::File::create(&tmp)
            .and_then(|mut f| {
                f.write_all(data)?;
                f.sync_all()
            })
            .and_then(|()| std::fs::rename(&tmp, &path));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        true
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
        if let Some(d) = self.read(from) {
            self.write(to, &d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DirSaves, SaveFiles};

    fn folder(name: &str) -> std::path::PathBuf {
        let base = std::env::var_os("ATLANTIS_NATIVE_DATA")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let d = base.join(format!("a5-saves-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_failed_write_keeps_the_earlier_save() {
        let d = folder("atomic");
        let saves = DirSaves(d.clone());
        assert!(saves.write("slot_1", b"first"));
        assert!(saves.write("slot_1", b"second"));
        assert_eq!(saves.read("slot_1").as_deref(), Some(&b"second"[..]));
        saves.copy("slot_1", "autoslot_1");
        assert_eq!(saves.read("autoslot_1").as_deref(), Some(&b"second"[..]));
        // The temporary file cannot be made (a folder holds its name):
        // the write fails and slot_1 is untouched.
        std::fs::create_dir_all(d.join("slot_1.tmp")).expect("blocker");
        assert!(!saves.write("slot_1", b"third"));
        assert_eq!(saves.read("slot_1").as_deref(), Some(&b"second"[..]));
        let names: Vec<_> = std::fs::read_dir(&d)
            .expect("folder")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
