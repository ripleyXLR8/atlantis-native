//! The Simplified Chinese CD's text hook ("GSoo", 2002): its table
//! `GSooAPI.res` and the two lookups its DLLs make.
//!
//! The Simplified Chinese CD (亚特兰蒂斯3, 2002) runs the English game
//! (atlantis3-cd-zh-cn-3682370 with GOG.com English's data) and puts its
//! Chinese in at run time: a third-party layer beside the program
//! (`GSooKernel.dll`, `GSooAPIRes.dll`) writes two detours into it
//! (atlantis3-re `binaries/atlantis3-cd-zh-cn-gsoo_dll.md`, read
//! statically, never run):
//!
//! - where the program copies a string out of a string object
//!   (atlantis3-cd-zh-cn-3682370 @ 0x00447b20; among its 25 callers the
//!   copies of `AtlFont_Print` and its two variants, the twins of
//!   atlantis3-gog-fr-3473408 @ 0x0041e2c0, 0x0041e200, 0x0041e390),
//!   GSooKernel.dll @ 0x100016b0 overwrites a string of 16 bytes or more
//!   found in the table with a 16-byte tag `GSoo_No.NNNNNNNN`
//!   ([`GsooTable::tag`]);
//! - where the ANSI renderer calls `ExtTextOutA` (@ 0x006f179a),
//!   GSooKernel.dll @ 0x100017c0 draws the Chinese of a tag
//!   ([`GsooTable::tag_chinese`]) or of a line equal to a whole English
//!   text ([`GsooTable::search`]) instead.
//!
//! The table is GBK text, CR LF: per entry a header `No.<8 digits>
//! base:...`, a line `GSoo_Eng:<English>` and a line `GSoo_Chi:<Chinese>`,
//! entries apart by a blank line. The lookups are `strstr` over the whole
//! file as the DLLs make them, so the port keeps the file's bytes and
//! searches them the same way (the first entry in the file wins).

use atlantis_data::FileSource;

/// The table's file, next to the program (`%s\GSooAPI.res`, read by
/// GSooKernel.dll @ 0x10001a00).
pub const TABLE_NAME: &str = "GSooAPI.res";

/// The hook's DLL, whose data holds the text drawn for a tag without a
/// Chinese line.
pub const KERNEL_NAME: &str = "GSooKernel.dll";

/// GSooKernel.dll's size and the file offset of its GBK string "no
/// Chinese resource for it" (VA 0x10003248; `.data` at VA 0x10003000,
/// file offset 0x3000).
const KERNEL_SIZE: usize = 24_576;
const KERNEL_MISSING_OFFSET: usize = 0x3248;

/// The tag's first eight bytes (GSooKernel.dll @ 0x100016e8, @ 0x100017e2).
pub const TAG_PREFIX: &[u8] = b"GSoo_No.";

/// A tag's length: `GSoo_` and `No.NNNNNNNN` (hook 2 asks `n` = 16,
/// @ 0x10001838).
pub const TAG_LEN: usize = 16;

/// What the draw hook finds for a tag (GSooKernel.dll @ 0x10001841..
/// 0x100018a9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagText<'a> {
    /// `No.NNNNNNNN` is not in the table: the call goes on with the tag,
    /// the Chinese font selected.
    NoEntry,
    /// The entry's Chinese, up to its CR LF; `tail` runs on to the table's
    /// end (the wrapping loop reads past the text's end, @ 0x10001922).
    Chinese { tail: &'a [u8], len: usize },
    /// No `GSoo_Chi:` after the entry: the DLL's own string (when its
    /// DLL is there to read it from; else nothing).
    Missing(&'a [u8]),
    /// A `GSoo_Chi:` without a CR LF after it: the call goes on with the
    /// tag, the Chinese font selected.
    NoEnd,
}

/// `GSooAPI.res` as the hook reads it.
#[derive(Clone, Debug)]
pub struct GsooTable {
    data: Vec<u8>,
    missing: Vec<u8>,
}

/// One entry of the table, for the tests and the font subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub number: u32,
    pub english: &'a [u8],
    pub chinese: &'a [u8],
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let hay = hay.get(from..)?;
    if needle.is_empty() {
        return Some(from);
    }
    hay.windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

impl GsooTable {
    /// The table from its bytes; `kernel`, GSooKernel.dll's, gives the
    /// string drawn for a tag without Chinese.
    pub fn new(data: Vec<u8>, kernel: Option<&[u8]>) -> GsooTable {
        let missing = kernel
            .filter(|k| k.len() == KERNEL_SIZE)
            .and_then(|k| {
                let s = &k[KERNEL_MISSING_OFFSET..];
                s.iter().position(|&b| b == 0).map(|e| s[..e].to_vec())
            })
            .unwrap_or_default();
        // The file is read into a mapping as a C string: it ends at its
        // first NUL, if any (`strstr`).
        let mut data = data;
        if let Some(e) = data.iter().position(|&b| b == 0) {
            data.truncate(e);
        }
        GsooTable { data, missing }
    }

    /// The install's table and DLL, when the table is there.
    pub fn load(src: &dyn FileSource) -> Option<GsooTable> {
        let data = src.read(TABLE_NAME)?;
        let kernel = src.read(KERNEL_NAME);
        Some(GsooTable::new(data, kernel.as_deref()))
    }

    /// The file's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// `SearchString16` (GSooAPIRes.dll @ 0x10001180(_, s, n, &len)): the
    /// leading spaces of `s` skipped (`n` less one each, unless 0), `n` 0
    /// meaning the rest of `s`; the first place of those `n` bytes in the
    /// table that follows `GSoo_Eng:` and is followed by CR LF
    /// `GSoo_Chi:` (the whole English line), the search going on after
    /// each other place; the answer, the Chinese after it up to the next
    /// CR LF (none: no answer): its offset in the table and its length.
    pub fn search(&self, s: &[u8], n: usize) -> Option<(usize, usize)> {
        let mut s = s;
        let mut n = n;
        while let [b' ', rest @ ..] = s {
            n = n.saturating_sub(1);
            s = rest;
        }
        if n == 0 {
            n = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        }
        let needle = s.get(..n)?;
        let t = &self.data;
        let mut at = find(t, needle, 0);
        while let Some(p) = at {
            let after_eng = p >= 9 && &t[p - 9..p] == b"GSoo_Eng:";
            if after_eng && t[p + n..].starts_with(b"\r\nGSoo_Chi:") {
                let c = p + n + 11;
                let e = find(t, b"\r\n", c)?;
                return Some((c, e - c));
            }
            if needle.is_empty() {
                return None;
            }
            at = find(t, needle, p + n);
        }
        None
    }

    /// The Chinese at `(offset, len)` of [`GsooTable::search`].
    pub fn text(&self, at: (usize, usize)) -> &[u8] {
        &self.data[at.0..at.0 + at.1]
    }

    /// Whether `s` begins as a tag does.
    pub fn is_tag(s: &[u8]) -> bool {
        s.starts_with(TAG_PREFIX)
    }

    /// Hook 1 (GSooKernel.dll @ 0x100016b0) on the string `s` a copy-out
    /// is about to copy: for a string of 16 bytes or more not beginning
    /// with `GSoo_No.` and found by [`GsooTable::search`] (`n` its
    /// length), the 16-byte tag it is overwritten with: `GSoo_` and the
    /// 11 bytes `No.NNNNNNNN` of the first `CR LF CR LF No.` before the
    /// match, walking back from the Chinese less the string's length.
    pub fn tag(&self, s: &[u8]) -> Option<[u8; TAG_LEN]> {
        let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        if len < TAG_LEN || Self::is_tag(s) {
            return None;
        }
        let (c, _) = self.search(s, len)?;
        let t = &self.data;
        let mut p = c.checked_sub(len)?;
        while !t[p..].starts_with(b"\r\n\r\nNo.") {
            p = p.checked_sub(1)?;
        }
        let mut tag = [0u8; TAG_LEN];
        tag[..5].copy_from_slice(b"GSoo_");
        tag[5..].copy_from_slice(t.get(p + 4..p + 15)?);
        Some(tag)
    }

    /// Hook 2's lookup of a tag drawn whole (`n` = 16): `strstr` of its
    /// `No.NNNNNNNN` in the table, then of `GSoo_Chi:` after it, the
    /// Chinese up to the next CR LF.
    pub fn tag_chinese(&self, tag: &[u8]) -> TagText<'_> {
        let t = &self.data;
        let Some(p) = tag.get(5..TAG_LEN).and_then(|k| find(t, k, 0)) else {
            return TagText::NoEntry;
        };
        let Some(q) = find(t, b"GSoo_Chi:", p) else {
            return TagText::Missing(&self.missing);
        };
        let c = q + 9;
        match find(t, b"\r\n", c) {
            Some(e) => TagText::Chinese {
                tail: &t[c..],
                len: e - c,
            },
            None => TagText::NoEnd,
        }
    }

    /// The entries with an English and a Chinese line, in the file's
    /// order (not what the hook reads: its lookups are
    /// [`GsooTable::search`] and [`GsooTable::tag_chinese`]).
    pub fn entries(&self) -> Vec<Entry<'_>> {
        let t = &self.data;
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(h) = find(t, b"\r\nNo.", at) {
            at = h + 5;
            let digits = &t[at..t.len().min(at + 8)];
            let Some(number) = std::str::from_utf8(digits)
                .ok()
                .filter(|d| d.len() == 8)
                .and_then(|d| d.parse().ok())
            else {
                continue;
            };
            let line_end = |from: usize| find(t, b"\r\n", from).unwrap_or(t.len());
            let eng = line_end(at) + 2;
            if !t.get(eng..).is_some_and(|r| r.starts_with(b"GSoo_Eng:")) {
                continue;
            }
            let eng_end = line_end(eng);
            let chi = eng_end + 2;
            if !t.get(chi..).is_some_and(|r| r.starts_with(b"GSoo_Chi:")) {
                continue;
            }
            out.push(Entry {
                number,
                english: &t[eng + 9..eng_end],
                chinese: &t[chi + 9..line_end(chi)],
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> GsooTable {
        let mut t = b"// banner //\r\n\r\n\r\n".to_vec();
        t.extend_from_slice(
            b"No.00000000 base:0x00000000,#A\r\nGSoo_Eng:Short\r\nGSoo_Chi:\xb6\xcc\r\n\r\n",
        );
        t.extend_from_slice(
            b"No.00000001 base:0x00000010,#B\r\nGSoo_Eng:A line of more than 16 bytes!\r\nGSoo_Chi:\xd5\xe2\xc0\xef\xa3\xa1\r\n\r\n",
        );
        t.extend_from_slice(b"No.00000002 xfhsm\r\nGSoo_Eng:bytes\r\nGSoo_Chi:\xb5\xd8\r\n\r\n");
        let mut k = vec![0u8; KERNEL_SIZE];
        k[KERNEL_MISSING_OFFSET..KERNEL_MISSING_OFFSET + 2].copy_from_slice(b"\xa1\xa3");
        GsooTable::new(t, Some(&k))
    }

    /// The whole English line matches, after the leading spaces; a part
    /// of a line does not, and the search goes on past it.
    #[test]
    fn search_matches_whole_lines() {
        let t = table();
        let at = t.search(b"  Short", 7).unwrap();
        assert_eq!(t.text(at), b"\xb6\xcc");
        assert_eq!(t.search(b"Shor", 4), None);
        // "bytes" first occurs inside entry 1's English, then whole in 2.
        assert_eq!(t.text(t.search(b"bytes", 0).unwrap()), b"\xb5\xd8");
        // n counts bytes of the line only.
        assert_eq!(t.text(t.search(b"bytes!!", 5).unwrap()), b"\xb5\xd8");
    }

    /// Hook 1: 16 bytes or more, found, not a tag yet.
    #[test]
    fn tags() {
        let t = table();
        assert_eq!(
            &t.tag(b"A line of more than 16 bytes!").unwrap(),
            b"GSoo_No.00000001"
        );
        assert_eq!(t.tag(b"Short"), None);
        assert_eq!(t.tag(b"GSoo_No.00000001"), None);
        assert_eq!(t.tag(b"Not in the table at all"), None);
        match t.tag_chinese(b"GSoo_No.00000001") {
            TagText::Chinese { tail, len } => assert_eq!(&tail[..len], b"\xd5\xe2\xc0\xef\xa3\xa1"),
            other => panic!("{other:?}"),
        }
        assert_eq!(t.tag_chinese(b"GSoo_No.00000009"), TagText::NoEntry);
        let e = t.entries();
        assert_eq!(e.len(), 3);
        assert_eq!(
            (e[1].number, e[1].english),
            (1, &b"A line of more than 16 bytes!"[..])
        );
    }

    /// A header without its Chinese line: the DLL's string.
    #[test]
    fn missing_chinese() {
        let mut k = vec![0u8; KERNEL_SIZE];
        k[KERNEL_MISSING_OFFSET] = b'?';
        let t = GsooTable::new(
            b"\r\n\r\nNo.00000005 x\r\nGSoo_Eng:abc\r\n".to_vec(),
            Some(&k),
        );
        assert_eq!(t.tag_chinese(b"GSoo_No.00000005"), TagText::Missing(b"?"));
    }
}
