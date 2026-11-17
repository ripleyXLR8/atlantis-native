//! How a cinematic's file is read: whole when it starts, as the originals
//! read it (the desktop), or by parts as its frames are decoded (the web
//! build, [`by_parts`]).
//!
//! The browser reads the player's files synchronously on the page's
//! thread, about 12 ms a MB: a cinematic of 34 MB read whole stops the
//! page for 0.4 s as it starts, the music and the ambience with it. Read
//! by parts ([`atlantis_codecs::HnmParts`]), the decoder takes the next
//! [`PART`] bytes when it reaches them (the first one as it opens, about
//! 7 ms in the browser), and each read asks the source to read the
//! following part in the background meanwhile
//! ([`FileSource::read_ahead`]), so that the next one is at hand (in
//! headless Edge every later part of Atlantis II's opening film was). The
//! frames, the sound and their times are the same either way (the
//! codec's `files_read_by_parts_decode_as_whole`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use atlantis_codecs::{HnmData, HnmParts};
use atlantis_data::FileSource;

/// The size of a part read.
pub const PART: usize = 512 << 10;

static BY_PARTS: AtomicBool = AtomicBool::new(cfg!(target_arch = "wasm32"));

/// Whether the cinematics are read by parts: in the web build; off on the
/// desktop, which reads them whole as the originals do.
pub fn by_parts() -> bool {
    BY_PARTS.load(Ordering::Relaxed)
}

/// Reads the cinematics by parts (`true`) or whole: for the tests.
pub fn set_by_parts(on: bool) {
    BY_PARTS.store(on, Ordering::Relaxed);
}

/// File `path` of `src`, whole or by parts ([`by_parts`]); `None` when it
/// cannot be read.
pub fn open(src: Arc<dyn FileSource>, path: String) -> Option<HnmData> {
    if !by_parts() {
        return src.read(&path).map(HnmData::from);
    }
    let size = src.size(&path)?;
    Some(HnmData::from(HnmParts::new(
        size,
        PART,
        Box::new(move |offset, len| {
            let data = src.read_at(&path, offset, len);
            let next = offset + len as u64;
            if next < size {
                src.read_ahead(&path, next, PART);
            }
            data
        }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_codecs::HnmBytes;
    use std::sync::Mutex;

    /// A source that records the reads and the read-ahead hints.
    struct Hints(
        atlantis_data::MemFiles,
        Mutex<Vec<(u64, usize)>>,
        Mutex<Vec<(u64, usize)>>,
    );

    impl FileSource for Hints {
        fn size(&self, name: &str) -> Option<u64> {
            self.0.size(name)
        }
        fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
            self.2.lock().unwrap().push((offset, len));
            self.0.read_at(name, offset, len)
        }
        fn has_dir(&self, name: &str) -> bool {
            self.0.has_dir(name)
        }
        fn read_ahead(&self, _name: &str, offset: u64, len: usize) {
            self.1.lock().unwrap().push((offset, len));
        }
    }

    /// Whole on the desktop; by parts, the same bytes, each part's read
    /// announcing the next.
    #[test]
    fn whole_or_by_parts() {
        let bytes: Vec<u8> = (0..(PART * 2 + 1000))
            .map(|i| (i * 7 % 251) as u8)
            .collect();
        let mut mem = atlantis_data::MemFiles::new();
        mem.insert("hnm/a.hnm", bytes.clone());
        let src = Arc::new(Hints(mem, Mutex::new(Vec::new()), Mutex::new(Vec::new())));
        assert!(!by_parts());
        let mut whole = open(src.clone(), "hnm/a.hnm".into()).unwrap();
        assert!(matches!(whole, HnmData::Vec(_)));
        src.2.lock().unwrap().clear();
        set_by_parts(true);
        let mut parts = open(src.clone(), "hnm/a.hnm".into()).unwrap();
        set_by_parts(false);
        assert!(open(src.clone(), "hnm/b.hnm".into()).is_none());
        assert!(matches!(parts, HnmData::Parts(_)));
        assert_eq!(parts.size(), bytes.len());
        let mut at = 0;
        for step in [64, 4, 100_000, 600_000, 1, 300_000] {
            let end = (at + step).min(bytes.len());
            assert_eq!(parts.bytes(at, end), whole.bytes(at, end));
            at = end;
        }
        assert_eq!(parts.bytes(at, bytes.len() + 1), None);
        // Read in order, each read announcing the next part, which holds
        // the next read: at hand in the browser.
        let reads = src.2.lock().unwrap().clone();
        let hints = src.1.lock().unwrap().clone();
        assert!(reads.len() >= 3, "{reads:?}");
        assert_eq!(reads[0], (0, PART));
        for (k, w) in reads.windows(2).enumerate() {
            let next = w[0].0 + w[0].1 as u64;
            assert_eq!(w[1].0, next, "{reads:?}");
            assert_eq!(hints[k], (next, PART), "{hints:?}");
            assert!(w[1].1 <= PART, "{reads:?}");
        }
    }
}
