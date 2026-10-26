//! `loc\Messages.txt`'s object names: the `[ObjectNameList]` ..
//! `[EndObjectNameList]` block, one `CON_NAME<blanks>shown name` line per
//! object, read by the bag's start-up (`AtlInventaire_MainInit`
//! @ 0x00474810) into a map from the `defnames.con` name to the name the
//! bag bar shows when the mouse is over an object.
//!
//! The map is a `std::map` of strings: keys compared byte by byte (@
//! 0x0040b410), and a repeated key keeps the last line's name (the loader
//! assigns the value of the node it finds or inserts, @ 0x00474b0e..
//! 0x00474b12). The file is read by the tokenizer @ 0x0046bee0..0x0046d450
//! [CONFIRMED-EMULATED: the loader run on the installed file and on a
//! synthetic one]:
//!
//! - lines end at LF; a CR is dropped and the byte after it kept
//!   (FUN_0046c790), so a CR LF ends the line;
//! - outside the block, a line starting with `;` is a comment; the block
//!   starts at a line that is exactly `[ObjectNameList]` and ends at one
//!   that is exactly `[EndObjectNameList]` (no blank around them);
//! - in the block every line is an entry, `;` lines included: the key is
//!   the first word, upper-cased (`toupper`, FUN_0046cdc0), after the
//!   blanks (space, tab, CR, LF, `,` and `;`, FUN_0046d450); the shown
//!   name is the rest of the line after the blanks that follow the key, to
//!   its end, trailing blanks kept (FUN_0046c960).
//!
//! The loader checks each key against `defnames.con`'s objects and stops
//! the game when one is not there (FUN_004106e0, FUN_00480cd0): the
//! shipped file has none. A block left open at the end of the file would
//! make the loader read empty lines for ever; here the file's end ends it.

/// The object names of `Messages.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectNames {
    names: Vec<(Vec<u8>, Vec<u8>)>,
}

impl ObjectNames {
    /// Reads the file's bytes (Windows-1252, kept as bytes).
    pub fn parse(data: &[u8]) -> ObjectNames {
        let mut out = ObjectNames::default();
        let mut pos = 0;
        while pos < data.len() {
            let line = read_line(data, &mut pos);
            if line.first() == Some(&b';') || line.as_slice() != b"[ObjectNameList]" {
                continue;
            }
            loop {
                if pos >= data.len() {
                    break;
                }
                let line = read_line(data, &mut pos);
                if line.as_slice() == b"[EndObjectNameList]" {
                    break;
                }
                let start = skip_blanks(&line, 0);
                let end = line[start..]
                    .iter()
                    .position(|&c| blank(c))
                    .map_or(line.len(), |e| start + e);
                let key = line[start..end].to_ascii_uppercase();
                let value = line[skip_blanks(&line, end)..].to_vec();
                match out.names.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, v)) => *v = value,
                    None => out.names.push((key, value)),
                }
            }
        }
        out
    }

    /// The name shown for the `defnames.con` name `key`. The bar looks it
    /// up with `operator[]` (@ 0x00475b10), which inserts an empty name for
    /// a key not in the file: such an object shows an empty text.
    pub fn get(&self, key: &[u8]) -> &[u8] {
        self.names
            .iter()
            .find(|(k, _)| k == key)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// The keys and their names, in the file's first order of the keys.
    pub fn entries(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.names.iter().map(|(k, v)| (k.as_slice(), v.as_slice()))
    }

    /// The number of distinct keys.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// The tokenizer's blanks (FUN_0046d450).
fn blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',' | b';')
}

/// The first byte from `at` on that is not a blank.
fn skip_blanks(b: &[u8], at: usize) -> usize {
    b[at..]
        .iter()
        .position(|&c| !blank(c))
        .map_or(b.len(), |p| at + p)
}

/// A line from `*pos` (FUN_0046c790): to the next LF, a CR dropped with
/// the byte after it kept (so CR LF ends the line).
fn read_line(data: &[u8], pos: &mut usize) -> Vec<u8> {
    let mut line = Vec::new();
    while *pos < data.len() {
        let mut c = data[*pos];
        *pos += 1;
        if c == b'\r' {
            let Some(&n) = data.get(*pos) else { break };
            *pos += 1;
            c = n;
        }
        if c == b'\n' {
            break;
        }
        line.push(c);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loader's rules, as the original reads the synthetic file of
    /// `input_ui.txt`'s `names, synthetic file` section (its keys and names
    /// there, in the map's order).
    #[test]
    fn names_as_the_original_reads_them() {
        let text = b"KEY=x\r\n; comment\r\n [ObjectNameList]\r\n[ObjectNameList] \r\n\
                     [ObjectNameList]\r\nSTONE\t\t\tA stone\r\n; again\r\n  ; spaced comment\r\n\
                     STONE\t\tThe stone\r\nstone\tlower\r\nROD   Rod  \r\n\
                     PIPE ,;, piped ; name\t\r\nCR\tone\rtwo\r\nEMPTY\r\nAFTER\tafter\r\n\
                     \tLEAD\tled\r\nTAB\t\t\r\n[EndObjectNameList] \r\nLATE\tlate\r\n\
                     [EndObjectNameList]\r\nOUT\tno\r\n";
        let n = ObjectNames::parse(text);
        let mut got: Vec<(&[u8], &[u8])> = n
            .names
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
            .collect();
        got.sort();
        let want: [(&[u8], &[u8]); 12] = [
            (b"AFTER", b"after"),
            (b"AGAIN", b""),
            (b"CR", b"onetwo"),
            (b"EMPTY", b""),
            (b"LATE", b"late"),
            (b"LEAD", b"led"),
            (b"PIPE", b"piped ; name\t"),
            (b"ROD", b"Rod  "),
            (b"SPACED", b"comment"),
            (b"STONE", b"lower"),
            (b"TAB", b""),
            (b"[ENDOBJECTNAMELIST]", b""),
        ];
        assert_eq!(got, want);
        assert_eq!(n.get(b"STONE"), b"lower");
        assert_eq!(n.get(b"stone"), b"");
        assert_eq!(n.get(b"OUT"), b"");
    }

    /// A block left open ends with the file.
    #[test]
    fn an_open_block_ends_with_the_file() {
        let n = ObjectNames::parse(b"[ObjectNameList]\nA\tb");
        assert_eq!(n.get(b"A"), b"b");
        assert_eq!(n.len(), 1);
    }
}
