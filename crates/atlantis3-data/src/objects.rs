//! Two `#NEW` lists of `loc\text\<language>\`: `Objects.txt`, the names
//! the inventory shows for the objects (read by @ 0x0042a120), and
//! `Errors.txt`, the messages of the error boxes (read by @ 0x00446660 each
//! time a message is wanted).
//!
//! Both are read with the shared reader of [`crate::tokens`]: each line
//! that is not empty and does not start with `//` must start with a word
//! whose first four bytes are `#NEW`, else the reader throws a C++
//! exception (@ 0x0042a4f5, @ 0x004469ac: the game stops). Then comes a
//! word (the key) and the rest of the line (after the separators that
//! follow the key, trailing blanks kept).

use crate::tokens::{atoi, latin1, ContentLines, Reader};

/// `Objects.txt` in the subtitle language's folder (@ 0x0042a148).
pub fn objects_path(language: &str) -> String {
    format!("Datas/loc/text/{language}/Objects.txt")
}

/// `Errors.txt` in the subtitle language's folder (@ 0x0044668d).
pub fn errors_path(language: &str) -> String {
    format!("Datas/loc/text/{language}/Errors.txt")
}

/// The size of the original's table of object names: 200 entries of a key
/// and a name (@ 0x00428f00 builds 400 strings @ 0x00756638). The reader
/// does not check it.
pub const OBJECTS_CAPACITY: usize = 200;

/// The error numbers the code asks for, with the call that uses them.
pub mod error_numbers {
    /// A text file line that should start with `#NEW` does not
    /// (@ 0x0040cdac, @ 0x00421969) or `info.txt` ends inside a block
    /// (@ 0x0040bd9d).
    pub const SCRIPT_FORMAT: i32 = 8;
    /// A `CDialogue` call before the dialog manager is ready
    /// (@ 0x004644c5).
    pub const DIALOG_NOT_READY: i32 = 9;
    /// The text manager failed to start (@ 0x0041d9a4).
    pub const FONT_INIT: i32 = 11;
}

/// A line that does not start with `#NEW`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotNewError {
    /// The byte offset of the line in the file.
    pub offset: usize,
}

impl std::fmt::Display for NotNewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line at byte {} does not start with #NEW", self.offset)
    }
}

impl std::error::Error for NotNewError {}

/// A `#NEW` line's key word and rest.
type NewLine<'a> = (&'a [u8], &'a [u8]);

/// The key word and the rest of each `#NEW` line.
fn new_lines(data: &[u8]) -> Result<Vec<NewLine<'_>>, NotNewError> {
    ContentLines::new(data)
        .map(|(offset, line)| {
            let mut w = Reader::new(line);
            if !w.word().starts_with(b"#NEW") {
                return Err(NotNewError { offset });
            }
            let key = w.word();
            Ok((key, w.line()))
        })
        .collect()
}

/// `Objects.txt`: `#NEW <KEY> <shown name>` lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectNames {
    /// The entries in file order (@ 0x0042a349 stores entry n's key at
    /// 0x00756638 + 40 n and its name 20 bytes further), the name as
    /// Windows-1252 bytes.
    pub entries: Vec<(String, Vec<u8>)>,
}

impl ObjectNames {
    /// Reads the file (@ 0x0042a120).
    pub fn parse(data: &[u8]) -> Result<ObjectNames, NotNewError> {
        Ok(ObjectNames {
            entries: new_lines(data)?
                .into_iter()
                .map(|(k, v)| (latin1(k), v.to_vec()))
                .collect(),
        })
    }

    /// The name shown for the object `key`: the inventory compares the key
    /// with every entry, case kept, and uses each match (@ 0x00429d72), so
    /// the last one wins.
    pub fn name(&self, key: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }
}

/// `Errors.txt`: `#NEW <number> <message>` lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorMessages {
    /// The entries in file order: `atoi` of the key, and the message as
    /// Windows-1252 bytes.
    pub entries: Vec<(i32, Vec<u8>)>,
}

impl ErrorMessages {
    /// Reads the file (@ 0x00446660).
    pub fn parse(data: &[u8]) -> Result<ErrorMessages, NotNewError> {
        Ok(ErrorMessages {
            entries: new_lines(data)?
                .into_iter()
                .map(|(k, v)| (atoi(k), v.to_vec()))
                .collect(),
        })
    }

    /// The message of error `number`: the first line with that number
    /// (the reader returns at the first match). Without one the original
    /// shows a fixed English text of the executable (@ 0x0071893c).
    pub fn message(&self, number: i32) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, v)| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_and_errors() {
        let o = ObjectNames::parse(
            b"// c\r\n#NEW\tA_KEY\t\tFirst name \r\n\r\n#NEWX\tB\tT\xe9\tx\r\n#NEW A_KEY again\r\n",
        )
        .unwrap();
        assert_eq!(o.entries.len(), 3);
        assert_eq!(o.entries[0], ("A_KEY".to_string(), b"First name ".to_vec()));
        assert_eq!(o.name("B"), Some(&b"T\xe9\tx"[..]));
        assert_eq!(o.name("A_KEY"), Some(&b"again"[..]));
        assert_eq!(o.name("a_key"), None);
        assert_eq!(
            ObjectNames::parse(b"#NEW A a\r\nNEW B b\r\n"),
            Err(NotNewError { offset: 10 })
        );
        let e = ErrorMessages::parse(b"#NEW\t001\tOne\r\n#NEW\t002\tTwo\r\n#NEW\t1\tAgain\r\n")
            .unwrap();
        assert_eq!(e.message(1), Some(&b"One"[..]));
        assert_eq!(e.message(2), Some(&b"Two"[..]));
        assert_eq!(e.message(3), None);
        assert_eq!(errors_path("fra"), "Datas/loc/text/fra/Errors.txt");
    }
}
