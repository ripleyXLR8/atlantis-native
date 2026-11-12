//! The music playlists (`music\*.mls`) of *Atlantis Evolution*'s 2004
//! program, read by @ 0x00431d80 of atlantis4-cd-en-3022848 (log
//! `PlayList introuvable : %s`, 0x006aea58).
//!
//! A text file, line by line (the options' line reader, @ 0x00449d70):
//!
//! - a line starting with `//` is skipped;
//! - the first line, upper-cased (@ 0x0044a570), equal to `RND` (0x006acdec)
//!   makes the list random and is not a track;
//! - any other line is a track: its words are split at spaces only
//!   (@ 0x0044ac30 with `' '`). The first is the file, with an optional
//!   repeat count `<n>x` before it when it starts with a digit
//!   (`999xrespire02.ogg`, @ 0x00431fd6..); each other word is
//!   `key=value`, the key lower-cased (@ 0x0044a430): `vol`, `tpsfi` (the
//!   fade-in, ms), `tpsfo` (the fade-out, ms) take the value's first
//!   number (`atoi`), `mi` and `mo` all the numbers of the value (a set,
//!   separators of the options' reader between them); other keys are
//!   ignored. Defaults: volume 50, fades 2000 ms, one play.
//! - Beside a track's file, `<file minus its 3-letter extension>mrk` in the
//!   music folder holds markers (a count, then pairs of integers); the CD
//!   ships none.

use std::collections::BTreeSet;

use crate::atl::{atoi, TextCursor};

/// One track of a playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    /// The music file (`music\` folder of the current disc).
    pub file: Vec<u8>,
    /// How many times it plays (`<n>x`). Default 1.
    pub repeat: i32,
    /// `vol` (0x32 = 50 by default).
    pub volume: i32,
    /// `tpsfi`: the fade-in in ms (default 2000).
    pub fade_in_ms: i32,
    /// `tpsfo`: the fade-out in ms (default 2000).
    pub fade_out_ms: i32,
    /// `mi`: markers [UNKNOWN: their use is not read yet].
    pub mi: BTreeSet<i32>,
    /// `mo`: markers [UNKNOWN: their use is not read yet].
    pub mo: BTreeSet<i32>,
}

/// A playlist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playlist {
    /// The first line was `RND`.
    pub random: bool,
    pub tracks: Vec<Track>,
}

/// Splits at `sep` as @ 0x0044ac30 does: the `sep`s before the word are
/// skipped, the word runs to the next `sep`, which is consumed.
fn word<'a>(s: &'a [u8], pos: &mut usize, sep: u8) -> &'a [u8] {
    while *pos < s.len() && s[*pos] == sep {
        *pos += 1;
    }
    let start = *pos;
    while *pos < s.len() && s[*pos] != sep {
        *pos += 1;
    }
    let w = &s[start..*pos];
    if *pos < s.len() {
        *pos += 1;
    }
    w
}

/// The numbers of `value` read one after the other (@ 0x00449240 until the
/// end of the value).
fn numbers(value: &[u8]) -> BTreeSet<i32> {
    let mut c = TextCursor::new(value);
    let mut out = BTreeSet::new();
    loop {
        let t = c.token();
        if t.is_empty() && c.eof() {
            break;
        }
        out.insert(atoi(t));
    }
    out
}

impl Playlist {
    /// Reads a playlist file.
    pub fn parse(data: &[u8]) -> Playlist {
        let mut list = Playlist::default();
        let mut f = TextCursor::new(data);
        let mut n = 0;
        while !f.eof() {
            let line = f.line();
            n += 1;
            if line.is_empty() || line.starts_with(b"//") {
                continue;
            }
            if n == 1 && line.eq_ignore_ascii_case(b"RND") {
                list.random = true;
                continue;
            }
            let mut pos = 0;
            let first = word(line, &mut pos, b' ');
            let mut track = Track {
                file: first.to_vec(),
                repeat: 1,
                volume: 50,
                fade_in_ms: 2000,
                fade_out_ms: 2000,
                mi: BTreeSet::new(),
                mo: BTreeSet::new(),
            };
            if first.first().is_some_and(u8::is_ascii_digit) {
                if let Some(x) = first.iter().position(|&c| c == b'x') {
                    track.repeat = atoi(&first[..x]);
                    track.file = first[x + 1..].to_vec();
                }
            }
            while pos < line.len() {
                let w = word(line, &mut pos, b' ');
                if w.is_empty() {
                    continue;
                }
                let Some(eq) = w.iter().position(|&c| c == b'=') else {
                    continue;
                };
                let key = w[..eq].to_ascii_lowercase();
                let value = &w[eq + 1..];
                match key.as_slice() {
                    b"mi" => track.mi = numbers(value),
                    b"mo" => track.mo = numbers(value),
                    b"vol" => track.volume = atoi(TextCursor::new(value).token()),
                    b"tpsfi" => track.fade_in_ms = atoi(TextCursor::new(value).token()),
                    b"tpsfo" => track.fade_out_ms = atoi(TextCursor::new(value).token()),
                    _ => {}
                }
            }
            list.tracks.push(track);
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_and_options() {
        let p = Playlist::parse(
            b"respire01.ogg vol=100 tpsfi=0 tpsfo=0\r\n999xrespire02.ogg vol=100 tpsfi=0 tpsfo=0",
        );
        assert!(!p.random);
        assert_eq!(p.tracks.len(), 2);
        assert_eq!(p.tracks[0].file, b"respire01.ogg");
        assert_eq!(
            (
                p.tracks[0].volume,
                p.tracks[0].fade_in_ms,
                p.tracks[0].repeat
            ),
            (100, 0, 1)
        );
        assert_eq!(
            (p.tracks[1].file.as_slice(), p.tracks[1].repeat),
            (&b"respire02.ogg"[..], 999)
        );
        let p =
            Playlist::parse(b"rnd\r\n// x\r\nmuzik16.ogg  VOL=60 tpsfo=5000 mi=10,20 other=1\r\n");
        assert!(p.random);
        let t = &p.tracks[0];
        assert_eq!((t.volume, t.fade_in_ms, t.fade_out_ms), (60, 2000, 5000));
        assert_eq!(t.mi, [10, 20].into_iter().collect());
    }

    /// Every playlist of the CD (`ATLANTIS4_CD`) reads to tracks whose file
    /// is in the same music folder.
    #[test]
    fn the_cds_playlists() {
        let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
            crate::testing::missing("ATLANTIS4_CD not set");
            return;
        };
        let mut lists = 0;
        for disc in 1..=4 {
            let music = dir.join(format!("cd{disc}/datas/music"));
            let Ok(rd) = std::fs::read_dir(&music) else {
                continue;
            };
            let names: Vec<String> = std::fs::read_dir(&music)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
                .collect();
            for e in rd.flatten() {
                let p = e.path();
                if p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.to_ascii_lowercase())
                    .as_deref()
                    != Some("mls")
                {
                    continue;
                }
                lists += 1;
                let list = Playlist::parse(&std::fs::read(&p).unwrap());
                assert!(!list.tracks.is_empty(), "{}", p.display());
                for t in &list.tracks {
                    let f = String::from_utf8_lossy(&t.file).to_ascii_lowercase();
                    assert!(names.contains(&f), "{}: {f}", p.display());
                    assert!((0..=100).contains(&t.volume) && t.repeat >= 1);
                }
            }
        }
        assert_eq!(lists, 54);
    }
}
