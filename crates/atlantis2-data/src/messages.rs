//! `loc\Messages.txt`'s object names: the `[ObjectNameList]` ..
//! `[EndObjectNameList]` block, one `CON_NAME<blanks>shown name` line per
//! object, read by the bag's start-up (`AtlInventaire_MainInit`
//! @ 0x00474810) into a map from the `defnames.con` name to the name the
//! bag bar shows when the mouse is over an object.
//!
//! The map is a `std::map` of strings: keys compared byte by byte, case
//! included (@ 0x0040b410), and a repeated key keeps the last line's name
//! (the loader assigns the value of the node it finds or inserts,
//! @ 0x00474b0e..0x00474b12). Lines starting with `;` are comments. The
//! shown name is the rest of the line after the blanks that follow the key
//! [INFERRED: the tokenizer's read @ 0x0046c960 is not transcribed; the
//! shipped names hold spaces, so it reads to the end of the line].

/// The object names of `Messages.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectNames {
    names: Vec<(Vec<u8>, Vec<u8>)>,
}

impl ObjectNames {
    /// Reads the file's bytes (Windows-1252, kept as bytes).
    pub fn parse(data: &[u8]) -> ObjectNames {
        let mut out = ObjectNames::default();
        let mut inside = false;
        for raw in data.split(|&b| b == b'\n') {
            let line = trim(raw);
            if line.first() == Some(&b';') || line.is_empty() {
                continue;
            }
            if !inside {
                inside = line.starts_with(b"[ObjectNameList]");
                continue;
            }
            if line.starts_with(b"[EndObjectNameList]") {
                inside = false;
                continue;
            }
            let end = line
                .iter()
                .position(|&b| b == b' ' || b == b'\t')
                .unwrap_or(line.len());
            let (key, value) = (&line[..end], trim(&line[end..]));
            match out.names.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => *v = value.to_vec(),
                None => out.names.push((key.to_vec(), value.to_vec())),
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

    /// The number of distinct keys.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// `b` without its leading and trailing blanks and line ends.
fn trim(b: &[u8]) -> &[u8] {
    let blank = |c: &u8| matches!(c, b' ' | b'\t' | b'\r' | b'\n');
    let start = b.iter().position(|c| !blank(c)).unwrap_or(b.len());
    let end = b.iter().rposition(|c| !blank(c)).map_or(start, |e| e + 1);
    &b[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_case_and_repeats() {
        let text = b"KEY=x\r\n; comment\r\n[ObjectNameList]\r\nSTONE\t\t\tA stone\r\n\
                     ; again\r\nSTONE\t\tThe stone\r\nstone\tlower\r\nROD   Rod\r\n\
                     [EndObjectNameList]\r\nOUT\tno\r\n";
        let n = ObjectNames::parse(text);
        assert_eq!(n.len(), 3);
        assert_eq!(n.get(b"STONE"), b"The stone");
        assert_eq!(n.get(b"stone"), b"lower");
        assert_eq!(n.get(b"ROD"), b"Rod");
        assert_eq!(n.get(b"OUT"), b"");
        assert_eq!(n.get(b"MISSING"), b"");
    }
}
