//! The reader the game's text files share.
//!
//! A text file is loaded whole into one of the engine's strings
//! (`AtlFile_LoadTextFile` @ 0x00418d00), whose read cursor (the string's
//! +0x10) then moves through it by lines (@ 0x00444f60) and by words
//! (@ 0x00445e00). The files read this way here are `scenar\info.txt`,
//! `scenar\com.txt`, `loc\text\<language>\dial.txt`, `Objects.txt`,
//! `Errors.txt`, the cinematics' subtitle files and `font\font.txt`.
//!
//! The rules, from the code:
//!
//! - the separators are space, tab, CR, LF, `,`, `;`, `(` and `)`
//!   (@ 0x00445fc0);
//! - a line read skips the separators in front of it, so blank lines (and
//!   lines made of separators only) vanish, then runs to the first CR or
//!   LF, which it consumes; trailing blanks are kept;
//! - a word read skips the separators in front of it, then runs to the
//!   next separator, which it consumes;
//! - the readers skip an empty line (the end of the file) and a line
//!   whose first two bytes are `//`; a `//` after other words is not a
//!   comment;
//! - keys are compared byte for byte, case kept (@ 0x004446c0);
//! - numbers go through the C library: `atoi` (@ 0x006d4d0f) and `atof`
//!   (@ 0x006d4da5), the double stored as a float (`fstp dword`);
//! - bytes 0x80 and above pass through unchanged: the files are
//!   Windows-1252 and only ASCII is ever compared or converted.

/// Whether `c` separates words (@ 0x00445fc0).
pub fn is_separator(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'(' | b')')
}

/// The C library's `isspace` in the "C" locale, which `atoi` and `atof`
/// skip.
fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// A string with its read cursor.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// The cursor at the start of `data`.
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    /// Whether the cursor is at the end (@ 0x00445df0).
    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// The cursor's offset.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The next line (@ 0x00444f60), with the offset where it starts.
    pub fn line_at(&mut self) -> (usize, &'a [u8]) {
        let data = self.data;
        let mut p = self.pos;
        while p < data.len() && is_separator(data[p]) {
            p += 1;
        }
        let start = p;
        while p < data.len() {
            let c = data[p];
            p += 1;
            if c == b'\r' || c == b'\n' {
                self.pos = p;
                return (start, &data[start..p - 1]);
            }
        }
        self.pos = p;
        (start, &data[start..])
    }

    /// The next line (@ 0x00444f60).
    pub fn line(&mut self) -> &'a [u8] {
        self.line_at().1
    }

    /// The next word (@ 0x00445e00); empty at the end.
    pub fn word(&mut self) -> &'a [u8] {
        let data = self.data;
        let mut p = self.pos;
        while p < data.len() && is_separator(data[p]) {
            p += 1;
        }
        let start = p;
        while p < data.len() {
            let c = data[p];
            p += 1;
            if is_separator(c) {
                self.pos = p;
                return &data[start..p - 1];
            }
        }
        self.pos = p;
        &data[start..]
    }
}

/// The lines every reader looks at: the file's lines without the empty
/// one and the `//` ones, each with the offset where it starts.
#[derive(Clone, Debug)]
pub struct ContentLines<'a> {
    reader: Reader<'a>,
}

impl<'a> ContentLines<'a> {
    /// The content lines of `data`.
    pub fn new(data: &'a [u8]) -> ContentLines<'a> {
        ContentLines {
            reader: Reader::new(data),
        }
    }
}

impl<'a> Iterator for ContentLines<'a> {
    type Item = (usize, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.reader.at_end() {
            let (at, line) = self.reader.line_at();
            if !line.is_empty() && !line.starts_with(b"//") {
                return Some((at, line));
            }
        }
        None
    }
}

/// `atoi` (@ 0x006d4d0f): blanks, a sign, then the digits up to the first
/// other byte, the value wrapping on overflow; 0 without digits.
pub fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && is_c_space(s[i]) {
        i += 1;
    }
    let negative = s.get(i) == Some(&b'-');
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let mut v: i32 = 0;
    while let Some(&c) = s.get(i) {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add(i32::from(c - b'0'));
        i += 1;
    }
    if negative {
        v.wrapping_neg()
    } else {
        v
    }
}

/// `atoi` of the first word of `s`: the engine's "next word as integer"
/// (@ 0x004444f0) run on a fresh string.
pub fn atoi_word(s: &[u8]) -> i32 {
    atoi(Reader::new(s).word())
}

/// `atof`'s double (@ 0x006d4da5): blanks, then the longest prefix of the
/// form `[+-]digits[.digits][(e|E|d|D)[+-]digits]`; 0 without digits.
pub fn atof_f64(s: &[u8]) -> f64 {
    let mut i = 0;
    while i < s.len() && is_c_space(s[i]) {
        i += 1;
    }
    let mut num = String::new();
    if let Some(&c @ (b'-' | b'+')) = s.get(i) {
        num.push(char::from(c));
        i += 1;
    }
    let int_start = i;
    while s.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut digits = i - int_start;
    num.push('0');
    num.extend(s[int_start..i].iter().map(|&c| char::from(c)));
    num.push('.');
    if s.get(i) == Some(&b'.') {
        i += 1;
        let frac_start = i;
        while s.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        digits += i - frac_start;
        num.extend(s[frac_start..i].iter().map(|&c| char::from(c)));
    }
    num.push('0');
    if digits == 0 {
        return 0.0;
    }
    if matches!(s.get(i), Some(b'e' | b'E' | b'd' | b'D')) {
        let mut j = i + 1;
        let mut exp = String::from("e");
        if let Some(&c @ (b'-' | b'+')) = s.get(j) {
            exp.push(char::from(c));
            j += 1;
        }
        let exp_start = j;
        while s.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > exp_start {
            exp.extend(s[exp_start..j].iter().map(|&c| char::from(c)));
            num.push_str(&exp);
        }
    }
    num.parse().unwrap_or(0.0)
}

/// `atof` stored as a float, as every reader does (`fstp dword`). The
/// double is rounded to the nearest float: the x87 rounding mode in force
/// when the files are read is not established.
pub fn atof(s: &[u8]) -> f32 {
    atof_f64(s) as f32
}

/// The string without its ASCII letters (@ 0x00445b40).
pub fn strip_letters(s: &[u8]) -> Vec<u8> {
    s.iter()
        .copied()
        .filter(|c| !c.is_ascii_alphabetic())
        .collect()
}

/// The string without its digits (@ 0x00445a70).
pub fn strip_digits(s: &[u8]) -> Vec<u8> {
    s.iter().copied().filter(|c| !c.is_ascii_digit()).collect()
}

/// The string lower-cased in place by the C library's `tolower`
/// (@ 0x00445620 → @ 0x006d4dfc): `A`..`Z` only, as in the "C" locale (no
/// call to `setlocale` from the game's code was found).
pub fn lower(s: &[u8]) -> Vec<u8> {
    s.to_ascii_lowercase()
}

/// Bytes as text, one character per byte (the Latin-1 mapping; the names
/// and keys this is used for are ASCII).
pub fn latin1(s: &[u8]) -> String {
    s.iter().map(|&c| char::from(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_words() {
        let mut r = Reader::new(b"\r\n\r\n  a,b;(c)\td  \r\nnext");
        let (at, line) = r.line_at();
        assert_eq!((at, line), (6, &b"a,b;(c)\td  "[..]));
        let mut w = Reader::new(line);
        let words: Vec<&[u8]> = (0..4).map(|_| w.word()).collect();
        assert_eq!(words, [&b"a"[..], &b"b"[..], &b"c"[..], &b"d"[..]]);
        assert!(!w.at_end());
        assert_eq!(w.word(), b"");
        assert!(w.at_end());
        assert_eq!(r.line(), b"next");
        assert!(r.at_end());
        assert_eq!(r.line(), b"");
    }

    #[test]
    fn content_lines_skip_comments_and_blanks() {
        let lines: Vec<_> = ContentLines::new(b"// c\r\n\r\n x // y\r\n,,\r\n//\r\nz").collect();
        assert_eq!(lines, [(9, &b"x // y"[..]), (25, &b"z"[..])]);
    }

    #[test]
    fn numbers() {
        assert_eq!(atoi(b" -12x"), -12);
        assert_eq!(atoi(b"+7"), 7);
        assert_eq!(atoi(b"[03]"), 0);
        assert_eq!(atoi(b""), 0);
        assert_eq!(atoi(b"4294967297"), 1);
        assert_eq!(atoi_word(b"  15 16"), 15);
        assert_eq!(atof(b"1.9016"), 1.9016_f32);
        assert_eq!(atof(b"-.5e1x"), -5.0);
        assert_eq!(atof(b"2."), 2.0);
        assert_eq!(atof(b"3d2"), 300.0);
        assert_eq!(atof(b"4e"), 4.0);
        assert_eq!(atof(b"."), 0.0);
        assert_eq!(atof(b"-"), 0.0);
        assert_eq!(atof(b"abc"), 0.0);
    }

    #[test]
    fn strings() {
        assert_eq!(strip_letters(b"[TabWeight03]"), b"[03]");
        assert_eq!(strip_digits(b"[TabWeight03]"), b"[TabWeight]");
        assert_eq!(lower(b"STAND1\xc9"), b"stand1\xc9");
        assert_eq!(latin1(b"a\xe9"), "a\u{e9}");
    }
}
