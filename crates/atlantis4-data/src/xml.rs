//! The XML of the game's data, read as the executable's TinyXML reads it
//! (TinyXML's class names in the RTTI of atlantis4-gog-en-2363910, its
//! messages from file +0x1aed24).
//!
//! What the data needs [CONFIRMED-DATA, all 415 `.xml` files]:
//! - no root element is required: the scenes, `Texts/<lang>.xml`,
//!   `Objects.xml`, `Documents.xml` and `Diary.xml` are flat lists of
//!   elements at the top (TinyXML's document holds any number);
//! - an optional `<?xml ...?>` declaration (398 scene files have one, the
//!   17 other files none), no comment, no CDATA, no byte-order mark;
//! - single-byte text: values are decoded as Windows-1252 (the French,
//!   German, Spanish and Italian texts hold bytes 0x80-0xff that are no
//!   UTF-8; the code page is inferred from their letters);
//! - the only entity used is `&quot;` (28 times, in `Texts/`); TinyXML's
//!   five named entities and `&#...;` are decoded, any other `&` is kept as
//!   is (TinyXML's rule).
//!
//! Attribute values are kept exactly, white space included (TinyXML does
//! not condense attribute values); text between elements is kept trimmed.

/// An element and what it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// The element's text, trimmed (empty in the game's files).
    pub text: String,
}

impl Element {
    /// The value of attribute `name` (exact case), the first if repeated.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The children named `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The first child named `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Why a file is not XML the reader accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlError {
    /// Byte offset of the problem.
    pub at: usize,
    pub what: &'static str,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XML error at byte {}: {}", self.at, self.what)
    }
}

impl std::error::Error for XmlError {}

/// Windows-1252 bytes as text.
pub fn cp1252(b: &[u8]) -> String {
    encoding_rs::WINDOWS_1252
        .decode_without_bom_handling(b)
        .0
        .into_owned()
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':') || c >= 0x80
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> XmlError {
        XmlError { at: self.at, what }
    }

    fn skip_ws(&mut self) {
        while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn starts(&self, p: &[u8]) -> bool {
        self.s[self.at..].starts_with(p)
    }

    fn skip_past(&mut self, end: &[u8], what: &'static str) -> Result<(), XmlError> {
        let rest = &self.s[self.at..];
        let i = rest
            .windows(end.len())
            .position(|w| w == end)
            .ok_or_else(|| self.err(what))?;
        self.at += i + end.len();
        Ok(())
    }

    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.at;
        while self.at < self.s.len() && is_name_byte(self.s[self.at]) {
            self.at += 1;
        }
        if self.at == start {
            return Err(self.err("name expected"));
        }
        Ok(cp1252(&self.s[start..self.at]))
    }

    /// Declarations, comments and white space before an element or a
    /// closing tag.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts(b"<?") {
                self.skip_past(b"?>", "unterminated declaration")?;
            } else if self.starts(b"<!--") {
                self.skip_past(b"-->", "unterminated comment")?;
            } else if self.starts(b"<!") {
                self.skip_past(b">", "unterminated markup")?;
            } else {
                return Ok(());
            }
        }
    }

    fn element(&mut self) -> Result<Element, XmlError> {
        // At '<'.
        self.at += 1;
        let mut el = Element {
            name: self.name()?,
            ..Element::default()
        };
        loop {
            self.skip_ws();
            match self.s.get(self.at) {
                Some(b'/') => {
                    if self.s.get(self.at + 1) != Some(&b'>') {
                        return Err(self.err("'>' expected after '/'"));
                    }
                    self.at += 2;
                    return Ok(el);
                }
                Some(b'>') => {
                    self.at += 1;
                    break;
                }
                Some(_) => {
                    let key = self.name()?;
                    self.skip_ws();
                    if self.s.get(self.at) != Some(&b'=') {
                        return Err(self.err("'=' expected"));
                    }
                    self.at += 1;
                    self.skip_ws();
                    let quote = *self
                        .s
                        .get(self.at)
                        .ok_or_else(|| self.err("value expected"))?;
                    if quote != b'"' && quote != b'\'' {
                        return Err(self.err("quoted value expected"));
                    }
                    self.at += 1;
                    let start = self.at;
                    while self.at < self.s.len() && self.s[self.at] != quote {
                        self.at += 1;
                    }
                    if self.at == self.s.len() {
                        return Err(self.err("unterminated value"));
                    }
                    let value = decode_entities(&self.s[start..self.at]);
                    self.at += 1;
                    el.attributes.push((key, value));
                }
                None => return Err(self.err("unterminated tag")),
            }
        }
        // Content.
        let mut text = Vec::new();
        loop {
            let start = self.at;
            while self.at < self.s.len() && self.s[self.at] != b'<' {
                self.at += 1;
            }
            text.extend_from_slice(&self.s[start..self.at]);
            if self.at == self.s.len() {
                return Err(self.err("missing closing tag"));
            }
            if self.starts(b"</") {
                self.at += 2;
                let name = self.name()?;
                if name != el.name {
                    return Err(self.err("closing tag of another element"));
                }
                self.skip_ws();
                if self.s.get(self.at) != Some(&b'>') {
                    return Err(self.err("'>' expected"));
                }
                self.at += 1;
                el.text = decode_entities(&text).trim().to_string();
                return Ok(el);
            }
            if self.starts(b"<!--") || self.starts(b"<?") || self.starts(b"<!") {
                self.skip_misc()?;
                continue;
            }
            el.children.push(self.element()?);
        }
    }
}

/// Bytes of a value with TinyXML's entities decoded, as text.
fn decode_entities(b: &[u8]) -> String {
    let text = cp1252(b);
    if !text.contains('&') {
        return text;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let named = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ];
        if let Some((e, c)) = named.iter().find(|(e, _)| rest.starts_with(e)) {
            out.push(*c);
            rest = &rest[e.len()..];
            continue;
        }
        if let Some(num) = rest.strip_prefix("&#") {
            if let Some(end) = num.find(';') {
                let digits = &num[..end];
                let v = match digits.strip_prefix('x') {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => digits.parse().ok(),
                };
                if let Some(c) = v.and_then(char::from_u32) {
                    out.push(c);
                    rest = &num[end + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

/// Every top-level element of a file.
pub fn parse(data: &[u8]) -> Result<Vec<Element>, XmlError> {
    let mut p = Parser { s: data, at: 0 };
    let mut out = Vec::new();
    loop {
        p.skip_misc()?;
        if p.at == data.len() {
            return Ok(out);
        }
        if data[p.at] != b'<' || p.starts(b"</") {
            return Err(p.err("element expected"));
        }
        out.push(p.element()?);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rootless_file_gives_every_top_element() {
        let x = b"<?xml version=\"1.0\" ?>\r\n<a k=\"1\" />\r\n<b n='x'>\r\n  <c v=\"&quot;q&quot; &amp; &#233;&zz;\"/>\r\n</b>\r\n";
        let els = parse(x).unwrap();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].attr("k"), Some("1"));
        assert_eq!(els[1].attr("n"), Some("x"));
        assert_eq!(
            els[1].child("c").unwrap().attr("v"),
            Some("\"q\" & \u{e9}&zz;")
        );
    }

    #[test]
    fn values_are_windows_1252() {
        let els = parse(b"<d t=\"caf\xe9 \x85\"/>").unwrap();
        assert_eq!(els[0].attr("t"), Some("caf\u{e9} \u{2026}"));
    }

    #[test]
    fn broken_files_are_refused() {
        assert!(parse(b"<a>").is_err());
        assert!(parse(b"<a></b>").is_err());
        assert!(parse(b"<a k=1/>").is_err());
        assert!(parse(b"text").is_err());
    }
}
