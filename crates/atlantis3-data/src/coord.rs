//! `scenar\Coord<wam>.txt`: the 3D positions of the views ("warps") and of
//! the sound sources ("persos") of a WAM, for the place sounds.
//!
//! `AtlWam_Load` (@ 0x0043eb00) builds the name from the WAM's: `Coord` +
//! the WAM file name without its last four characters + `.txt`
//! (@ 0x0043ed01..0x0043ed2a), in `scenar\` (@ 0x00417e80), and hands it to
//! `AtlBruitages_InitWam` (@ 0x00401470). A missing file is logged
//! (`AtlFile_LoadTextFile`, @ 0x00418d00) and reads as empty: four of the
//! eleven shipped WAMs have none (`Couloir`, `Interdim`, `PMagi`, `Rhea`).
//! Windows opens `CoordHoggar.txt` for `hoggar.wam`: names compare without
//! case.
//!
//! The reader works line by line with the engine's tokenizer: a line
//! starts at the first character that is not a separator (space, tab, CR,
//! LF, `,`, `;`, `(`, `)`, @ 0x00445fc0) and ends at CR or LF
//! (@ 0x00444f60); tokens are split at the same separators (@ 0x00445e00).
//! Outside the lists:
//!
//! - a line starting with `//` is skipped;
//! - a line whose first token is `[WarpList]` opens the view list, read
//!   until a line whose first token is `[EndWarpList]`; in it, a line whose
//!   first token, letters only (@ 0x00445d00), is `Warp` gives a view
//!   number (`atoi` of the token's digits only, @ 0x00445c30) and the next
//!   three tokens as x, y, z (`atof`, stored as f32, @ 0x00444c50). A
//!   number below 1 is logged (`n° de warp incorrect`) and the line
//!   skipped; a missing token reads as `atof("") = 0`. Other lines are
//!   ignored;
//! - `[PersoList]` ... `[EndPersoList]` likewise, with `Perso` lines: the
//!   second token is a name, the next three x, y, z;
//! - a list not closed before the end of the file is a fatal error
//!   (`AtlBruitages_InitWam() : unexpected end of file.`).
//!
//! Both lists are maps: a repeated key keeps the last line. The view
//! number is the number of a view of the WAM: `AtlBruitages_InitWarp`
//! (@ 0x00401f30) looks the current view's number (view +0x10) up in the
//! list and sets the listener position from it `[CONFIRMED-STATIC]`. The
//! perso names are matched with the place's sound sources `[INFERRED]`.

use std::collections::BTreeMap;
use std::fmt;

/// The positions read from one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coord {
    /// View number → position (x, y, z).
    pub warps: BTreeMap<u32, [f32; 3]>,
    /// Sound source name → position.
    pub persos: BTreeMap<String, [f32; 3]>,
    /// `Warp` lines with a number below 1, skipped (the original logs
    /// them).
    pub bad_warp_lines: usize,
}

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordError {
    /// A list is not closed (the original stops with a fatal error).
    UnexpectedEof,
}

impl fmt::Display for CoordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoordError::UnexpectedEof => write!(f, "Coord file: unexpected end of file"),
        }
    }
}

impl std::error::Error for CoordError {}

/// The tokenizer's separators (@ 0x00445fc0).
fn is_sep(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'(' | b')')
}

/// The lines as the engine reads them (@ 0x00444f60).
fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut pos = 0;
    std::iter::from_fn(move || {
        while pos < data.len() && is_sep(data[pos]) {
            pos += 1;
        }
        if pos >= data.len() {
            return None;
        }
        let start = pos;
        while pos < data.len() && data[pos] != b'\r' && data[pos] != b'\n' {
            pos += 1;
        }
        Some(&data[start..pos])
    })
}

fn tokens(line: &[u8]) -> Vec<&[u8]> {
    line.split(|&c| is_sep(c))
        .filter(|t| !t.is_empty())
        .collect()
}

/// `atof` (@ 0x006d4da5) of a token: the longest leading decimal number,
/// 0 when there is none; the engine stores it as f32.
pub fn atof(token: &[u8]) -> f32 {
    crate::tokens::atof(token)
}

/// `atoi` (@ 0x006d4d9a) of a string of digits, wrapping like 32-bit C.
fn atoi_digits(digits: &[u8]) -> i32 {
    digits.iter().fold(0i32, |n, &c| {
        n.wrapping_mul(10).wrapping_add(i32::from(c - b'0'))
    })
}

fn first_token(line: &[u8]) -> &[u8] {
    tokens(line).first().copied().unwrap_or(&[])
}

fn xyz(toks: &[&[u8]]) -> [f32; 3] {
    let mut v = [0.0; 3];
    for (i, x) in v.iter_mut().enumerate() {
        *x = toks.get(i).map_or(0.0, |t| atof(t));
    }
    v
}

impl Coord {
    /// Reads a file as `AtlBruitages_InitWam` does (@ 0x00401470).
    pub fn parse(data: &[u8]) -> Result<Coord, CoordError> {
        let mut c = Coord::default();
        let mut it = lines(data);
        while let Some(line) = it.next() {
            if line.starts_with(b"//") {
                continue;
            }
            let mut first = first_token(line);
            if first == b"[WarpList]" {
                loop {
                    let line = it.next().ok_or(CoordError::UnexpectedEof)?;
                    let toks = tokens(line);
                    first = toks.first().copied().unwrap_or(&[]);
                    if first == b"[EndWarpList]" {
                        break;
                    }
                    let letters: Vec<u8> = first
                        .iter()
                        .copied()
                        .filter(u8::is_ascii_alphabetic)
                        .collect();
                    if letters != b"Warp" {
                        continue;
                    }
                    let digits: Vec<u8> =
                        first.iter().copied().filter(u8::is_ascii_digit).collect();
                    let n = atoi_digits(&digits);
                    if n < 1 {
                        c.bad_warp_lines += 1;
                        continue;
                    }
                    c.warps.insert(n as u32, xyz(&toks[1..]));
                }
            }
            if first == b"[PersoList]" {
                loop {
                    let line = it.next().ok_or(CoordError::UnexpectedEof)?;
                    let toks = tokens(line);
                    let first = toks.first().copied().unwrap_or(&[]);
                    if first == b"[EndPersoList]" {
                        break;
                    }
                    let letters: Vec<u8> = first
                        .iter()
                        .copied()
                        .filter(u8::is_ascii_alphabetic)
                        .collect();
                    if letters != b"Perso" {
                        continue;
                    }
                    let name: String = toks
                        .get(1)
                        .map(|t| t.iter().map(|&b| char::from(b)).collect())
                        .unwrap_or_default();
                    let pos = xyz(toks.get(2..).unwrap_or(&[]));
                    c.persos.insert(name, pos);
                }
            }
        }
        Ok(c)
    }

    /// The file name for a WAM file name: `Coord` + the name without its
    /// last four characters + `.txt` (`hoggar.wam` → `Coordhoggar.txt`,
    /// found as `CoordHoggar.txt` on a case-blind file system).
    pub fn file_name(wam: &str) -> String {
        let cut = wam.char_indices().rev().nth(3).map_or(0, |(i, _)| i);
        format!("Coord{}.txt", &wam[..cut])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"//----\r\n// comment\r\n\r\n[WarpList]\r\n\
Warp001\t\t1.5 -2 3\r\nWarp002\t\t4 5\r\nWarp000\t1 1 1\r\nNote 9 9 9\r\n\
Warp001 7 8 9\r\n[EndWarpList]\r\n\r\n[PersoList]\r\n\
Perso\tAB001\t\t-1 2.25 3e2\r\n[EndPersoList]\r\n";

    #[test]
    fn reads_both_lists() {
        let c = Coord::parse(SAMPLE).unwrap();
        assert_eq!(c.warps.len(), 2);
        // A repeated number keeps the last line.
        assert_eq!(c.warps[&1], [7.0, 8.0, 9.0]);
        // A missing coordinate reads as 0.
        assert_eq!(c.warps[&2], [4.0, 5.0, 0.0]);
        assert_eq!(c.bad_warp_lines, 1);
        assert_eq!(c.persos["AB001"], [-1.0, 2.25, 300.0]);
    }

    #[test]
    fn unclosed_list_is_fatal() {
        assert_eq!(
            Coord::parse(b"[WarpList]\r\nWarp001 1 2 3\r\n"),
            Err(CoordError::UnexpectedEof)
        );
        assert_eq!(
            Coord::parse(b"[PersoList]\r\n"),
            Err(CoordError::UnexpectedEof)
        );
    }

    #[test]
    fn separators_and_numbers() {
        let c = Coord::parse(b"[WarpList] x\n(Warp7);1,2,3\n[EndWarpList]").unwrap();
        assert_eq!(c.warps[&7], [1.0, 2.0, 3.0]);
        assert_eq!(atof(b"12.5abc"), 12.5);
        assert_eq!(atof(b"-.5"), -0.5);
        assert_eq!(atof(b"e5"), 0.0);
        assert_eq!(atof(b"2e"), 2.0);
    }

    #[test]
    fn file_name_from_wam() {
        assert_eq!(Coord::file_name("hoggar.wam"), "Coordhoggar.txt");
        assert_eq!(Coord::file_name("CFuner.wam"), "CoordCFuner.txt");
    }
}
