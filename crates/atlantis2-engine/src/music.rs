//! The music files and choices: `muzikNN.apc` decoded from its start or
//! from a `.spp` mark, the random mark (`AtlMuzik_GetRandomMark`
//! @ 0x00478090) and the `startmusik` list (`Interp_SelectMusic`
//! @ 0x004851f0). The player's states and fades are in [`crate::sound`];
//! the engine runs them.
//!
//! The original streams a track through a 0x80000-byte ring kept about
//! 5.8 s ahead of what is heard, and switches to the next track when the
//! decoder (not the listener) reaches the end. Native decodes a track whole
//! and switches when it has been heard to its end: the original's cut of
//! the last ~5.8 s of a track followed by another is not reproduced (an
//! open choice, see `docs/TECHNICAL.md`).

use std::sync::Arc;

use atlantis_codecs::{ApcDecoder, ApcHeader, APC_HEADER_SIZE};

use crate::crt::Rand;
use crate::sound::Pcm;

/// The music file of track `n` (@ 0x005deb38: `muzik%02d.apc`).
pub fn music_name(n: i32) -> String {
    format!("muzik{n:02}.apc")
}

/// Its marks' file (`music_name_to_spp_name` @ 0x00477d00).
pub fn spp_name(n: i32) -> String {
    format!("muzik{n:02}.spp")
}

/// One `.spp` record (0x38 bytes): where a resume starts and the decoder
/// state there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The byte offset in the ADPCM data (after the 0x20-byte header).
    pub offset: u32,
    pub stereo: bool,
    /// The frames left to decode from there.
    pub frames_left: u32,
    pub predictors: [i32; 2],
    pub step_indexes: [u32; 2],
}

/// `Load_Spp` (@ 0x00477340): the records; `None` when the size is not a
/// multiple of 0x38 (`fichier SPP invalide`).
pub fn parse_spp(data: &[u8]) -> Option<Vec<Mark>> {
    if !data.len().is_multiple_of(0x38) {
        return None;
    }
    let le = |r: &[u8], o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    Some(
        data.as_chunks::<0x38>()
            .0
            .iter()
            .map(|r| Mark {
                offset: le(r, 0),
                stereo: le(r, 0x04) & 1 != 0,
                frames_left: le(r, 0x18),
                predictors: [le(r, 0x1c) as i32, le(r, 0x20) as i32],
                step_indexes: [le(r, 0x24), le(r, 0x28)],
            })
            .collect(),
    )
}

/// Decodes a music file from its start (`mark` `None`) or from a mark
/// (`AtlMuzik_GotoMark` @ 0x00477e80: the record's state over the
/// decoder's, the file read from `offset + 0x20`), up to the frames the
/// state has left.
pub fn decode_music(data: &[u8], mark: Option<&Mark>) -> Option<Pcm> {
    let header = ApcHeader::parse(data).ok()?;
    let (mut dec, start, frames, stereo) = match mark {
        None => (
            ApcDecoder::new(&header),
            APC_HEADER_SIZE,
            header.frames as usize,
            header.stereo,
        ),
        Some(m) => (
            ApcDecoder::resume(m.stereo, m.predictors, m.step_indexes),
            m.offset as usize + APC_HEADER_SIZE,
            m.frames_left as usize,
            m.stereo,
        ),
    };
    let channels: u16 = if stereo { 2 } else { 1 };
    let wanted = frames * usize::from(channels);
    let body = data.get(start..).unwrap_or(&[]);
    let body = &body[..body.len().min(wanted.div_ceil(2))];
    let mut samples = Vec::with_capacity(wanted);
    dec.decode(body, &mut samples);
    samples.truncate(wanted);
    Some(Pcm {
        rate: header.sample_rate,
        channels,
        samples: Arc::new(samples),
    })
}

/// `AtlMuzik_GetRandomMark` (@ 0x00478090): up to 10 draws of
/// `rand() · (count + 1) / 0x7fff` while the draw equals the current mark.
/// 0 is the file's start; `count + 1` (from `rand()` = 0x7fff) is the
/// invalid mark the original reports as fatal.
pub fn random_mark(rand: &mut Rand, count: i32, current: i32) -> i32 {
    let mut m = current;
    for _ in 0..10 {
        if m != current {
            break;
        }
        m = rand.rand() * (count + 1) / 0x7fff;
    }
    m
}

/// One entry of a `startmusik` list: `mus[-mark][*loops]` (mark −1 = a
/// random one, loops 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub music: i32,
    pub mark: i32,
    pub loops: i32,
}

/// The list's parser in `Interp_SelectMusic` (with @ 0x00485180: skip to a
/// digit, `atoi`, skip the digits and blanks), up to `)` or the end.
pub fn parse_list(text: &[u8]) -> Vec<Entry> {
    let mut entries = Vec::new();
    if text.is_empty() {
        return entries;
    }
    let num = |i: &mut usize| -> i32 {
        while *i < text.len() && !text[*i].is_ascii_digit() && text[*i] != b'\r' {
            *i += 1;
        }
        let v = atlantis2_data::con::atoi(&text[*i..]);
        while *i < text.len() && text[*i].is_ascii_digit() {
            *i += 1;
        }
        while *i < text.len() && matches!(text[*i], b' ' | b'\t') {
            *i += 1;
        }
        v
    };
    let mut i = 0;
    loop {
        if text.get(i) == Some(&b')') {
            break;
        }
        let music = num(&mut i);
        let mut e = Entry {
            music,
            mark: -1,
            loops: 0,
        };
        if text.get(i) == Some(&b'-') {
            e.mark = num(&mut i);
        }
        if text.get(i) == Some(&b'*') {
            e.loops = num(&mut i);
        }
        entries.push(e);
        if i >= text.len() {
            break;
        }
    }
    entries
}

/// `Interp_SelectMusic` (@ 0x004851f0) on the list: the entry to play and
/// the number of entries; `None` when the list is empty. One `rand()` is
/// drawn first; with two entries naming the current music the other one
/// comes in 85 % of the draws; with more, up to 10 draws of an index while
/// it equals the current music's number (an index compared with a
/// number). The `rand()` = 0x7fff edge (an entry past the list, read from
/// the original's stack) gives `None` here.
pub fn select(
    rand: &mut Rand,
    text: &[u8],
    current_music: i32,
    current_mark: i32,
) -> Option<(Entry, usize)> {
    let e = parse_list(text);
    let r = rand.rand() * 100 / 0x7fff;
    if e.is_empty() {
        return None;
    }
    let n = e.len();
    if n == 1 {
        return Some((e[0], 1));
    }
    let idx = if n == 2 {
        if current_music == e[0].music || current_music == e[1].music {
            let (same, other) = if e[0].music == e[1].music {
                if current_mark == e[0].mark {
                    (0, 1)
                } else {
                    (1, 0)
                }
            } else if current_music == e[0].music {
                (0, 1)
            } else {
                (1, 0)
            };
            if r < 85 {
                other
            } else {
                same
            }
        } else {
            (r * 2 / 100) as usize
        }
    } else {
        let mut idx = current_music;
        for _ in 0..10 {
            if idx != current_music {
                break;
            }
            idx = (i64::from(rand.rand()) * n as i64 * 100 / 0x31_ff9c) as i32;
        }
        idx as usize
    };
    e.get(idx).map(|&x| (x, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists() {
        assert_eq!(
            parse_list(b"20-3)"),
            [Entry {
                music: 20,
                mark: 3,
                loops: 0
            }]
        );
        assert_eq!(
            parse_list(b"1-2*3,4)"),
            [
                Entry {
                    music: 1,
                    mark: 2,
                    loops: 3
                },
                Entry {
                    music: 4,
                    mark: -1,
                    loops: 0
                }
            ]
        );
        assert!(parse_list(b"").is_empty());
    }
}
