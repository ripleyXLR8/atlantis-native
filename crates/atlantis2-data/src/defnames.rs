//! `scenar\defnames.con`: the names the CON scripts use, in five sections.
//!
//! The original reads the file whole (`LoadDefNames` @ 0x004847d0) and
//! parses it in place (`DefName_Init` @ 0x00410b70); every lookup walks the
//! text again (`DefName_FindFirst` @ 0x004102c0, `DefName_FindNext`
//! @ 0x004103e0). The sections are, in the order of the pointer table
//! @ 0x005dac0c: 0 `#PERSONNAGES#`, 1 `#OBJETS#`, 2 `#SUJETS#`,
//! 3 `#VARIABLES#`, 4 `#ZONES_CLIC#`, then `#END_OF_FILE#`.
//!
//! - A line is skipped when, after blanks and tabs, it starts with `//` or
//!   is empty (`FUN_00410df0`); a line starting with a section tag switches
//!   the section; any other line is an entry.
//! - An entry's name is the run of letters, digits and `_` at its start,
//!   upper-cased in place; its index is its rank in the section, from 0.
//! - Parameters follow the name, after commas: parameter n is the text
//!   after the n-th comma of the entry's line, blanks skipped
//!   (`FUN_004109d0`); its number is the C `atoi` of that text, 0 when
//!   there is none (`FUN_00410e50`).
//! - A query matches an entry when its own alphanumeric run (so `NAME)` or
//!   `NAME,x` work) has the same length and is equal ignoring case
//!   (`FUN_00410da0`, `_strnicmp`). An unknown name, an empty one or one of
//!   32 characters or more is a fatal error of the game
//!   (`DefName_GetIdName` @ 0x004105a0); `DefName_GetIdNameCool`
//!   @ 0x004106e0 gives 0 for an unknown name instead.

use crate::con::atoi;

/// The five sections.
pub const PERSONNAGES: usize = 0;
pub const OBJETS: usize = 1;
pub const SUJETS: usize = 2;
pub const VARIABLES: usize = 3;
pub const ZONES_CLIC: usize = 4;

/// The section tags, in the order of the pointer table @ 0x005dac0c; the
/// sixth ends the file.
const TAGS: [&[u8]; 6] = [
    b"#PERSONNAGES#",
    b"#OBJETS#",
    b"#SUJETS#",
    b"#VARIABLES#",
    b"#ZONES_CLIC#",
    b"#END_OF_FILE#",
];

/// The size of the script variable table (`int vars[300]` @ 0x006cfe90).
pub const MAX_VARIABLES: usize = 300;

/// Why `defnames.con` is refused (each a fatal error of the original).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefNamesError {
    /// No `#END_OF_FILE#` after the sections (`DefName_Init` returns 0).
    Unfinished,
    /// A name of 32 characters or more (`DefName_Init: nom %s trop long`).
    NameTooLong(String),
    /// More than 300 lines counted in `#VARIABLES#`
    /// (`LoadDefNames: Trop de Variables`).
    TooManyVariables(usize),
}

impl std::fmt::Display for DefNamesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefNamesError::Unfinished => write!(f, "defnames.con: no #END_OF_FILE#"),
            DefNamesError::NameTooLong(n) => write!(f, "defnames.con: name {n} too long"),
            DefNamesError::TooManyVariables(n) => {
                write!(f, "defnames.con: {n} variables (300 at most)")
            }
        }
    }
}

impl std::error::Error for DefNamesError {}

/// A failed name lookup, which the original treats as fatal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// `DefName_GetIdName: nom %s name requis`: the query has no name.
    Empty(usize),
    /// `... trop long`: 32 characters or more.
    TooLong(usize, String),
    /// `DefName_GetIdName: Nom de %s inconnu:%s`.
    Unknown(usize, String),
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = |s: &usize| String::from_utf8_lossy(TAGS[*s]).into_owned();
        match self {
            NameError::Empty(s) => write!(f, "{}: empty name", tag(s)),
            NameError::TooLong(s, n) => write!(f, "{}: name {n} too long", tag(s)),
            NameError::Unknown(s, n) => write!(f, "{}: unknown name {n}", tag(s)),
        }
    }
}

impl std::error::Error for NameError {}

#[derive(Clone, Debug)]
struct Entry {
    /// The upper-cased name.
    name: Vec<u8>,
    /// Where the entry's line starts, blanks skipped.
    line: usize,
}

/// The parsed `defnames.con` (empty by default: every lookup fails).
#[derive(Clone, Debug, Default)]
pub struct DefNames {
    text: Vec<u8>,
    sections: [Vec<Entry>; 5],
}

/// `isalnum` or `_` in the "C" locale.
pub fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The length of the name run at the start of `q`.
fn run(q: &[u8]) -> usize {
    q.iter().take_while(|&&c| is_name_byte(c)).count()
}

/// The start of the next line (`FUN_00410d10`): past the CR and the LF.
fn next_line(t: &[u8], mut p: usize) -> usize {
    while p < t.len() && !matches!(t[p], 0 | b'\r' | b'\n') {
        p += 1;
    }
    if t.get(p) == Some(&b'\r') {
        p += 1;
    }
    if t.get(p) == Some(&b'\n') {
        p += 1;
    }
    p
}

/// Blanks and tabs (`FUN_00410d70`).
fn skip_blanks(t: &[u8], mut p: usize) -> usize {
    while p < t.len() && matches!(t[p], b' ' | b'\t') {
        p += 1;
    }
    p
}

impl DefNames {
    /// Parses the file: `DefName_Init`'s pass (sections, names upper-cased
    /// in place, line counts), then each section's entries as
    /// `DefName_FindFirst` / `DefName_FindNext` enumerate them.
    pub fn parse(data: &[u8]) -> Result<DefNames, DefNamesError> {
        let mut text = data.to_vec();
        let end = text.len();
        // Per section: the first line counted, the line of the next tag,
        // the count (@ 0x00659298, stride 12).
        let mut start = [None::<usize>; 5];
        let mut stop = [end; 5];
        let mut counted = [0usize; 5];
        let mut section = 6usize;
        let mut p = 0usize;
        while section != 5 {
            if p >= end || text[p] == 0 {
                return Err(DefNamesError::Unfinished);
            }
            p = skip_blanks(&text, p);
            if text.get(p) == Some(&b'\r') {
                p = next_line(&text, p);
            }
            let at = &text[p.min(end)..];
            // The last tag that matches wins (they never share a prefix).
            let tag = TAGS.iter().rposition(|t| at.starts_with(t));
            if let Some(s) = tag {
                if section < 5 {
                    stop[section] = p;
                }
                section = s;
            } else if !at.starts_with(b"//") && section < 5 {
                start[section].get_or_insert(p);
                let n = run(at);
                for c in &mut text[p..p + n] {
                    *c = c.to_ascii_uppercase();
                }
                if n > 31 {
                    let name = String::from_utf8_lossy(&text[p..p + n]).into_owned();
                    return Err(DefNamesError::NameTooLong(name));
                }
                counted[section] += 1;
            }
            // Before the first tag the original does not move on (it
            // would loop); the files start with comments and a tag.
            p = next_line(&text, p);
        }
        if counted[VARIABLES] > MAX_VARIABLES {
            return Err(DefNamesError::TooManyVariables(counted[VARIABLES]));
        }
        let mut sections: [Vec<Entry>; 5] = Default::default();
        for s in 0..5 {
            let Some(mut p) = start[s] else { continue };
            let end = stop[s];
            loop {
                // FUN_00410df0, then the loop of FindFirst / FindNext: past
                // blanks, `//` lines and empty lines.
                loop {
                    p = skip_blanks(&text, p);
                    if p >= end {
                        break;
                    }
                    if text[p..].starts_with(b"//") || text[p] == b'\r' {
                        p = next_line(&text, p);
                    } else {
                        break;
                    }
                }
                if p >= end || text[p] == b'#' || !is_name_byte(text[p]) {
                    // A line that starts otherwise is fatal in FindNext
                    // ("caractere non..."); the shipped file has none.
                    break;
                }
                let n = run(&text[p..]);
                sections[s].push(Entry {
                    name: text[p..p + n].to_vec(),
                    line: p,
                });
                p = next_line(&text, p + n);
            }
        }
        Ok(DefNames { text, sections })
    }

    /// The number of entries of a section.
    pub fn len(&self, section: usize) -> usize {
        self.sections[section].len()
    }

    /// The upper-cased name of an entry.
    pub fn name(&self, section: usize, index: usize) -> Option<&[u8]> {
        self.sections[section].get(index).map(|e| e.name.as_slice())
    }

    fn find(&self, section: usize, query: &[u8]) -> Option<usize> {
        let n = run(query);
        self.sections[section]
            .iter()
            .position(|e| e.name.len() == n && e.name.eq_ignore_ascii_case(&query[..n]))
    }

    /// `DefName_GetIdName` (@ 0x004105a0): the index of the name at the
    /// start of `query`; an error where the original stops the game.
    pub fn id(&self, section: usize, query: &[u8]) -> Result<usize, NameError> {
        let n = run(query);
        let shown = || {
            let end = query
                .iter()
                .position(|&c| c == b'\r')
                .unwrap_or(query.len());
            String::from_utf8_lossy(&query[..end.min(63)]).into_owned()
        };
        if n == 0 {
            return Err(NameError::Empty(section));
        }
        if n >= 32 {
            return Err(NameError::TooLong(section, shown()));
        }
        self.find(section, query)
            .ok_or_else(|| NameError::Unknown(section, shown()))
    }

    /// `DefName_GetIdNameCool` (@ 0x004106e0): the same, 0 when unknown.
    pub fn id_cool(&self, section: usize, query: &[u8]) -> usize {
        self.find(section, query).unwrap_or(0)
    }

    /// The text of parameter `n` (1 = after the first comma) of an entry,
    /// up to the end of the file (`FUN_004109d0`); `None` when the line
    /// has fewer commas.
    pub fn param_text_of(&self, section: usize, index: usize, n: usize) -> Option<&[u8]> {
        let t = &self.text;
        let mut p = self.sections[section].get(index)?.line;
        for _ in 0..n {
            while p < t.len() && !matches!(t[p], b',' | b'\r' | b'\n') {
                p += 1;
            }
            if t.get(p) != Some(&b',') {
                return None;
            }
            p = skip_blanks(t, p + 1);
        }
        Some(&t[p..])
    }

    /// `DefName_GetParam` (@ 0x00410a30) by name.
    pub fn param_text(&self, section: usize, query: &[u8], n: usize) -> Option<&[u8]> {
        self.param_text_of(section, self.find(section, query)?, n)
    }

    /// The number of parameter 1 by name (`FUN_00410e50`): `atoi` of its
    /// text, 0 without one or for an unknown name.
    pub fn param(&self, section: usize, query: &[u8]) -> i32 {
        self.param_text(section, query, 1).map_or(0, atoi)
    }

    /// The same by index (`FUN_00410e80` through `FUN_00410930`).
    pub fn param_of(&self, section: usize, index: usize) -> i32 {
        self.param_text_of(section, index, 1).map_or(0, atoi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &[u8] = b"// header, with a comma\r\n\r\n#PERSONNAGES#\r\n// keep DUMMY\r\nDUMMY\r\n\
        Guide\r\n\r\n//--- more, here\r\n  Hero_2 , 7\r\n#OBJETS#\r\nDUMMY\r\nBall\t, 5\t,ball.spr,\tZone_B //c\r\n\
        #SUJETS#\r\n#VARIABLES#\r\nChapter\r\nflag\r\n#ZONES_CLIC#\r\nDUMMY\r\n#END_OF_FILE#\r\n";

    #[test]
    fn sections_names_and_parameters() {
        let d = DefNames::parse(FILE).unwrap();
        assert_eq!([0, 1, 2, 3, 4].map(|s| d.len(s)), [3, 2, 0, 2, 1]);
        assert_eq!(d.name(0, 1), Some(&b"GUIDE"[..]));
        assert_eq!(d.id(0, b"guide)\r"), Ok(1));
        assert_eq!(d.id(0, b"HERO_2,x"), Ok(2));
        assert_eq!(d.param(0, b"hero_2"), 7);
        assert_eq!(d.param(1, b"BALL"), 5);
        assert_eq!(d.param_of(1, 1), 5);
        assert!(d
            .param_text(1, b"BALL", 2)
            .unwrap()
            .starts_with(b"ball.spr,"));
        assert!(d
            .param_text(1, b"BALL", 3)
            .unwrap()
            .starts_with(b"Zone_B //c"));
        assert_eq!(d.param_text(1, b"BALL", 4), None);
        assert_eq!(d.param(0, b"GUIDE"), 0);
        assert_eq!(d.id(3, b"FLAG=1"), Ok(1));
        // A prefix is not a match; an unknown name is an error, 0 when cool.
        assert!(matches!(d.id(0, b"GUID"), Err(NameError::Unknown(0, _))));
        assert_eq!(d.id_cool(0, b"GUID"), 0);
        assert!(matches!(d.id(0, b"=x"), Err(NameError::Empty(0))));
        assert!(matches!(
            d.id(0, &[b'A'; 32]),
            Err(NameError::TooLong(0, _))
        ));
    }

    #[test]
    fn lf_lines_and_names_after_the_end() {
        let d = DefNames::parse(
            b"// comment\n#PERSONNAGES#\nDUMMY\n  shaman // the old man\n#OBJETS#\nDUMMY\nCHEVAL,26\n\
              #ZONES_CLIC#\nDUMMY\nzone_a\n#END_OF_FILE#\nLATE\n",
        )
        .unwrap();
        assert_eq!(d.id(0, b"shaman)"), Ok(1));
        assert!(matches!(
            d.id(0, b"SHAMANW10"),
            Err(NameError::Unknown(0, _))
        ));
        assert_eq!(d.param(1, b"cheval"), 26);
        assert_eq!(d.param(0, b"SHAMAN"), 0);
        assert_eq!(d.id(4, b"ZONE_A"), Ok(1));
        assert_eq!(d.name(4, 1), Some(&b"ZONE_A"[..]));
        // A name after `#END_OF_FILE#` belongs to no section.
        assert_eq!(d.len(4), 2);
        assert!(matches!(d.id(4, b"LATE"), Err(NameError::Unknown(4, _))));
    }

    #[test]
    fn refused_files() {
        assert_eq!(
            DefNames::parse(b"#VARIABLES#\r\nA\r\n").unwrap_err(),
            DefNamesError::Unfinished
        );
        let long = [
            b"#VARIABLES#\r\n".as_slice(),
            &[b'A'; 32],
            b"\r\n#END_OF_FILE#\r\n",
        ]
        .concat();
        assert!(matches!(
            DefNames::parse(&long),
            Err(DefNamesError::NameTooLong(_))
        ));
        let many: Vec<u8> = std::iter::once(b"#VARIABLES#\r\n".to_vec())
            .chain((0..301).map(|i| format!("V{i}\r\n").into_bytes()))
            .chain(std::iter::once(b"#END_OF_FILE#\r\n".to_vec()))
            .flatten()
            .collect();
        assert_eq!(
            DefNames::parse(&many).unwrap_err(),
            DefNamesError::TooManyVariables(301)
        );
    }
}
