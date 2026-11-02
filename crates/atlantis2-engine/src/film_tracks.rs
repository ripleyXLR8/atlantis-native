//! The bug fix [`crate::BugFix::FilmLanguage`]: the track of the language
//! installed for films 19, 55 and 66.
//!
//! The original reads a film's track from `datas_a2\apc\` (`cine%03d.apc`
//! @ 0x0047a5c0 → 0x00402760). `datas_a2\apc\dial\`, the installed
//! language's voices, holds a copy of the films' tracks on GOG.com and on
//! the Italian collection, the same bytes but for these three films, where
//! `apc\` holds another language's: GOG's English in every language, a
//! track that sounds Russian on the collection.
//!
//! The choice is made on the files, so that no list of editions or
//! languages is needed and an unknown edition stays safe: a `dial\` track
//! that is missing or does not [`fits`] leaves the original's, which is
//! every CD's case (no `cine*` in `dial\`); on GOG.com's English the two are
//! one file, so the same samples play.

use atlantis2_data::files::{A2Files, Folder};
use atlantis_codecs::{ApcHeader, APC_HEADER_SIZE};

/// The tracks concerned. The other films' are one file in both folders
/// on every edition known.
pub const TRACKS: [&str; 3] = ["cine019.apc", "cine055.apc", "cine066.apc"];

/// How far the two tracks' lengths may be: one frame of a film, at 15 frames
/// a second. (The collection's `cine019` is 2 samples shorter than its own
/// `dial\` track.)
const FRAMES_PER_SECOND: u32 = 15;

/// Whether `own` can stand for `original`: a whole track of the same rate,
/// channels and length.
pub fn fits(original: &[u8], own: &[u8]) -> bool {
    let (Ok(a), Ok(b)) = (ApcHeader::parse(original), ApcHeader::parse(own)) else {
        return false;
    };
    let body = (own.len() - APC_HEADER_SIZE) as u64 * 2;
    a.sample_rate == b.sample_rate
        && a.stereo == b.stereo
        && a.frames.abs_diff(b.frames) <= a.sample_rate / FRAMES_PER_SECOND
        && b.frames > 0
        && body >= u64::from(b.frames) * u64::from(b.channels())
}

/// The installed language's track to play for the track `name`, `None` when
/// the original's file is the one to play.
pub fn own_track(files: &A2Files, name: &str) -> Option<Vec<u8>> {
    if !TRACKS.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return None;
    }
    let original = files.read_current(Folder::Apc, name)?;
    let own = files.read_current(Folder::ApcDial, name)?;
    if !fits(&original, &own) {
        log::warn!("{name}: the track of apc\\dial does not fit its film");
        return None;
    }
    Some(own)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlantis_data::FileSource;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// A track: its header and body, in memory.
    fn track(frames: u32, rate: u32, stereo: bool, fill: u8, bytes: usize) -> Vec<u8> {
        let mut t = b"CRYO_APC1.20".to_vec();
        t.extend(frames.to_le_bytes());
        t.extend(rate.to_le_bytes());
        t.extend([0; 8]);
        t.extend(u32::from(stereo).to_le_bytes());
        t.resize(APC_HEADER_SIZE + bytes, fill);
        t
    }

    fn whole(frames: u32, fill: u8) -> Vec<u8> {
        track(frames, 22050, true, fill, frames as usize + 1)
    }

    struct Memory(HashMap<String, Vec<u8>>);

    impl FileSource for Memory {
        fn size(&self, name: &str) -> Option<u64> {
            self.0.get(name).map(|d| d.len() as u64)
        }
        fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
            let d = self.0.get(name)?;
            let start = offset as usize;
            d.get(start..start + len).map(<[u8]>::to_vec)
        }
        fn has_dir(&self, _: &str) -> bool {
            false
        }
        fn list(&self, _: &str) -> Vec<String> {
            Vec::new()
        }
    }

    /// One disc holding these files.
    fn game(entries: &[(&str, Vec<u8>)]) -> A2Files {
        let mut map = HashMap::new();
        map.insert("datas_a2/spf/sphex.spr".to_string(), vec![1]);
        for (name, data) in entries {
            map.insert((*name).to_string(), data.clone());
        }
        let disc: Arc<dyn FileSource> = Arc::new(Memory(map));
        A2Files::new(
            Arc::new(Memory(HashMap::new())),
            [("disc".to_string(), disc)],
        )
    }

    const APC: &str = "datas_a2/apc/cine019.apc";
    const DIAL: &str = "datas_a2/apc/dial/cine019.apc";

    #[test]
    fn the_track_of_dial_when_it_fits() {
        let files = game(&[(APC, whole(1000, 1)), (DIAL, whole(1002, 2))]);
        assert_eq!(own_track(&files, "cine019.apc"), Some(whole(1002, 2)));
    }

    #[test]
    fn the_same_file_gives_the_same_bytes() {
        let files = game(&[(APC, whole(1000, 1)), (DIAL, whole(1000, 1))]);
        assert_eq!(own_track(&files, "cine019.apc"), Some(whole(1000, 1)));
    }

    #[test]
    fn no_other_track_is_ever_taken() {
        let files = game(&[
            ("datas_a2/apc/cine020.apc", whole(1000, 1)),
            ("datas_a2/apc/dial/cine020.apc", whole(1000, 2)),
            ("datas_a2/apc/sound001.apc", whole(1000, 1)),
            ("datas_a2/apc/dial/sound001.apc", whole(1000, 2)),
        ]);
        assert_eq!(own_track(&files, "cine020.apc"), None);
        assert_eq!(own_track(&files, "sound001.apc"), None);
    }

    #[test]
    fn a_missing_track_leaves_the_original() {
        let files = game(&[(APC, whole(1000, 1))]);
        assert_eq!(own_track(&files, "cine019.apc"), None);
        // No track at all where the original reads: nothing plays, as there.
        let files = game(&[(DIAL, whole(1000, 2))]);
        assert_eq!(own_track(&files, "cine019.apc"), None);
    }

    #[test]
    fn what_does_not_fit_leaves_the_original() {
        let original = whole(22050, 1);
        for own in [
            Vec::new(),
            b"not a track at all, not a track at all".to_vec(),
            // Only a header, or a body cut short.
            track(22050, 22050, true, 2, 0),
            track(22050, 22050, true, 2, 22049),
            // Another rate, other channels.
            track(22050, 44100, true, 2, 22051),
            track(22050, 22050, false, 2, 22051),
            // Longer or shorter than the film by more than a frame.
            whole(22050 + 1471, 2),
            whole(22050 - 1471, 2),
            // Empty.
            track(0, 22050, true, 2, 1),
        ] {
            assert!(!fits(&original, &own), "{}", own.len());
            let files = game(&[(APC, original.clone()), (DIAL, own)]);
            assert_eq!(own_track(&files, "cine019.apc"), None);
        }
        assert!(fits(&original, &whole(22050 + 1470, 2)));
    }
}
