//! The chapter scripts `Datas\scenar\chapiNNN.con`, scanned at the level
//! of the CON language: sections, `/con` entries and their conditions,
//! `/set(...)` commands.
//!
//! *Atlantis III*'s interpreter is *Atlantis II*'s (the same scanner and
//! grammar: `/INIT` .. `/FININIT` sections, `/con(key op value)&(...)`
//! entries, `/set(KEY=value, ...)` lines, `#` comments); its keys are
//! looked up case-insensitively (the executable holds them in upper case,
//! e.g. `CHANGECD` @ 0x00718a7c, `WAITFORRECALAGE` @ 0x00718d0c). This
//! module does not run a script: it lists what each line asks, so that a
//! port can check every key of the shipped scripts against the keys it
//! knows. `defnames.con` is read by `atlantis2_data::DefNames`.

/// A `/con` term `(key op value)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    /// The key, lower case.
    pub key: String,
    /// `=`, `!`, `<` or `>`.
    pub op: u8,
    pub value: String,
}

/// One `/set(...)` command: its `key=value` pairs (a word without `=`
/// has an empty value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Set {
    /// Keys lower case, values as written.
    pub pairs: Vec<(String, String)>,
}

/// What a script holds, line by line (comments and empty lines left out).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    /// `/INIT` lines.
    pub inits: usize,
    /// `/FININIT` lines.
    pub fininits: usize,
    /// The `/con` entries' conditions.
    pub cons: Vec<Vec<Condition>>,
    /// The `/set` commands.
    pub sets: Vec<Set>,
    /// The section headers `/NNN` (a slash and digits), their numbers.
    pub sections: Vec<u32>,
    /// Lines that start with `/` and a letter and are none of the above
    /// (`/tim1`, `/tim2` in *Atlantis II*; none expected here).
    pub other_directives: Vec<String>,
    /// Lines that are none of the above and not `#` comments (`//`
    /// banners and their text, which no scan of the interpreter matches).
    pub other_lines: usize,
}

fn text(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// The terms `(...)` of a line, without nesting.
fn terms(line: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(o) = line[i..].iter().position(|&c| c == b'(') {
        let start = i + o + 1;
        let Some(c) = line[start..].iter().position(|&c| c == b')') else {
            break;
        };
        out.push(&line[start..start + c]);
        i = start + c + 1;
    }
    out
}

fn condition(term: &[u8]) -> Option<Condition> {
    let at = term.iter().position(|c| b"=!<>".contains(c))?;
    let key = text(term[..at].trim_ascii()).to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    Some(Condition {
        key,
        op: term[at],
        value: text(term[at + 1..].trim_ascii()),
    })
}

/// The pairs of a `/set(...)` line: its first term, split on `,`.
fn set_pairs(line: &[u8]) -> Vec<(String, String)> {
    let Some(term) = terms(line).into_iter().next() else {
        return Vec::new();
    };
    term.split(|&c| c == b',')
        .filter(|p| !p.trim_ascii().is_empty())
        .map(|p| match p.iter().position(|&c| c == b'=') {
            Some(at) => (
                text(p[..at].trim_ascii()).to_ascii_lowercase(),
                text(p[at + 1..].trim_ascii()),
            ),
            // `/set(killsuj,CHF_...,DIAL_...)`: words without a value.
            None => (text(p.trim_ascii()).to_ascii_lowercase(), String::new()),
        })
        .collect()
}

impl Script {
    /// Scans a chapter script.
    pub fn scan(data: &[u8]) -> Script {
        let mut s = Script::default();
        for line in data.split(|&c| c == b'\n' || c == b'\r') {
            let line = line.trim_ascii();
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with(b"/fininit") {
                s.fininits += 1;
            } else if lower.starts_with(b"/init") {
                s.inits += 1;
            } else if lower.starts_with(b"/set") {
                s.sets.push(Set {
                    pairs: set_pairs(line),
                });
            } else if lower.starts_with(b"/con") {
                s.cons
                    .push(terms(line).into_iter().filter_map(condition).collect());
            } else if line.len() > 1 && line[0] == b'/' && line[1].is_ascii_digit() {
                let digits: Vec<u8> = line[1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .copied()
                    .collect();
                s.sections.push(text(&digits).parse().unwrap_or(0));
            } else if line.len() > 1 && line[0] == b'/' && line[1].is_ascii_alphabetic() {
                let word: Vec<u8> = line[1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .copied()
                    .collect();
                s.other_directives.push(text(&word));
            } else {
                s.other_lines += 1;
            }
        }
        s
    }
}

/// A key with its trailing digits folded to `#` (`flag12` → `flag#`), as
/// the keys that name a numbered variable are counted.
pub fn fold_digits(key: &str) -> String {
    let t = key.trim_end_matches(|c: char| c.is_ascii_digit());
    if t.len() < key.len() {
        format!("{t}#")
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let s = Script::scan(
            b"# comment\r\n// banner\r\n/000\r\n/INIT\r\n/set(Vue=4, AngleWarpX=-90)\r\n/FININIT\r\n\
              /con(flag12=1)&(objetenmain!CRAYON)\r\n/set(flag12=0)\r\n",
        );
        assert_eq!((s.inits, s.fininits), (1, 1));
        assert_eq!((s.sections.as_slice(), s.other_lines), (&[0u32][..], 1));
        assert_eq!(s.sets.len(), 2);
        assert_eq!(
            s.sets[0].pairs,
            [
                ("vue".to_string(), "4".to_string()),
                ("anglewarpx".to_string(), "-90".to_string())
            ]
        );
        assert_eq!(s.cons.len(), 1);
        assert_eq!(s.cons[0][1].op, b'!');
        assert_eq!(fold_digits(&s.cons[0][0].key), "flag#");
        assert_eq!(fold_digits("vue"), "vue");
    }
}
