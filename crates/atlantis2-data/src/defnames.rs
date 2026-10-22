//! `scenar\defnames.con`: the names the scripts and the places use, in
//! five sections; a name's index is its rank in its section.
//!
//! `DefName_Init` (@ 0x00410b70) parses the file in place: a line is
//! skipped when it starts (after blanks and tabs) with `//` or is empty; a
//! section tag switches the section; any other line is an entry whose name
//! is the run of letters, digits and `_` at its start, upper-cased, and
//! whose parameters follow after commas. `DefName_GetIdName` (@ 0x004105a0)
//! matches a name of the same length ignoring case.

/// The sections, in the order of the table @ 0x005dac0c.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// `#PERSONNAGES#`
    Characters = 0,
    /// `#OBJETS#`
    Objects = 1,
    /// `#SUJETS#`
    Subjects = 2,
    /// `#VARIABLES#`
    Variables = 3,
    /// `#ZONES_CLIC#`
    Zones = 4,
}

const TAGS: [&str; 5] = [
    "#PERSONNAGES#",
    "#OBJETS#",
    "#SUJETS#",
    "#VARIABLES#",
    "#ZONES_CLIC#",
];

/// One entry: its upper-cased name and the text after it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    rest: String,
}

/// The parsed file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefNames {
    sections: [Vec<Entry>; 5],
}

/// The run of letters, digits and `_` a name starts with.
fn name_run(s: &str) -> &str {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    &s[..end]
}

impl DefNames {
    /// Reads the file (bytes, Latin-1).
    pub fn parse(data: &[u8]) -> DefNames {
        let text: String = data.iter().map(|&b| char::from(b)).collect();
        let mut out = DefNames::default();
        let mut section: Option<usize> = None;
        for line in text.split(['\n', '\r']) {
            let line = line.trim_start_matches([' ', '\t']);
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line.starts_with("#END_OF_FILE#") {
                break;
            }
            if let Some(k) = TAGS.iter().position(|t| line.starts_with(t)) {
                section = Some(k);
                continue;
            }
            let Some(k) = section else { continue };
            let name = name_run(line);
            if name.is_empty() {
                continue;
            }
            out.sections[k].push(Entry {
                name: name.to_ascii_uppercase(),
                rest: line[name.len()..].to_string(),
            });
        }
        out
    }

    /// The index of `name` in `section` (the run at its start, compared
    /// ignoring case), none when absent (the original stops the game).
    pub fn index(&self, section: Section, name: &str) -> Option<usize> {
        let q = name_run(name);
        if q.is_empty() {
            return None;
        }
        self.sections[section as usize]
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(q))
    }

    /// The name at `index` of `section`.
    pub fn name(&self, section: Section, index: usize) -> Option<&str> {
        self.sections[section as usize]
            .get(index)
            .map(|e| e.name.as_str())
    }

    /// `DefName_GetParam` + `atoi` (@ 0x00410e50): the first parameter of
    /// `name`, 0 when it has none.
    pub fn param(&self, section: Section, name: &str) -> i32 {
        let Some(i) = self.index(section, name) else {
            return 0;
        };
        let rest = &self.sections[section as usize][i].rest;
        let Some((_, after)) = rest.split_once(',') else {
            return 0;
        };
        let s = after.trim_start();
        let (neg, digits) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let v = digits
            .bytes()
            .take_while(u8::is_ascii_digit)
            .fold(0i32, |v, d| {
                v.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
            });
        if neg {
            v.wrapping_neg()
        } else {
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_indices_parameters() {
        let d = b"// comment\n#PERSONNAGES#\nDUMMY\n  shaman // the old man\n\n#OBJETS#\nDUMMY\nCHEVAL,26\n#ZONES_CLIC#\nDUMMY\nzone_a\n#END_OF_FILE#\nLATE\n";
        let n = DefNames::parse(d);
        assert_eq!(n.index(Section::Characters, "SHAMAN"), Some(1));
        assert_eq!(n.index(Section::Characters, "shaman)"), Some(1));
        assert_eq!(n.index(Section::Characters, "SHAMANW10"), None);
        assert_eq!(n.index(Section::Objects, "cheval"), Some(1));
        assert_eq!(n.param(Section::Objects, "CHEVAL"), 26);
        assert_eq!(n.param(Section::Characters, "SHAMAN"), 0);
        assert_eq!(n.index(Section::Zones, "ZONE_A"), Some(1));
        assert_eq!(n.name(Section::Zones, 1), Some("ZONE_A"));
        assert_eq!(n.index(Section::Zones, "LATE"), None);
        assert_eq!(n.index(Section::Zones, ""), None);
    }
}
