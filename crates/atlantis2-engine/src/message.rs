//! The message viewer of the script key `MESSAGE_CLIC=<id>` (loop type 10,
//! `AtlMessage_*`, atlantis2-cd-fr-2199552 @ 0x00476590..0x00476ab8; the
//! same code in 1.01): the line `<id>=text` of `loc\Messages.txt` drawn
//! in `AtlFT_Big_Default` at the centre of the frozen panorama until a
//! click or two seconds. No shipped script uses the key.
//!
//! - The table (`AtlMessage_MainInit` @ 0x00476590): the whole file, read
//!   at start-up.
//! - The id (@ 0x00476820): the value up to a CR or a LF, at most 255
//!   bytes.
//! - The look-up (@ 0x00476640): the id's key is its run of letters,
//!   digits and `_` from its start (`isalnum` of the C locale, mask 0x107,
//!   @ 0x005ad9f0); the first line of the table whose own run has the same
//!   length and bytes holds the text: what follows the line's first `=`,
//!   up to a NUL, CR or LF, at most 1024 bytes. Lines are not otherwise
//!   parsed: a `;` comment or an empty line has an empty run, which an id
//!   with an empty run matches. Without a line, the text is
//!   `Unknown message ID :` followed by the id (`sprintf("%s%s")`).
//!   A byte from 0x80 up is not a letter here: the original reads the
//!   C runtime's table at a negative index for it (a `char` sign-extended),
//!   [INFERRED: outside the table, not a port of its bytes].
//! - The layout (@ 0x00476860): a text at most 640 pixels wide is drawn
//!   at the centre; a wider one is cut in two at half its length in bytes
//!   (an unknown id at the end of `Unknown message ID :`), and only the
//!   second half is drawn, 20 pixels below the centre, unless the id was
//!   unknown, when the first half is drawn too, 20 pixels above (the flag
//!   @ 0x005dc954 tested there is 1 at start-up and set to 1 again at
//!   every end). Each line's position is (W / 2 − w / 2, H / 2 − h / 2)
//!   in C's integer division, W x H the logical screen, w x h the line's
//!   `GetTextExtentPoint32A`.
//! - The colour: `SetTextColor(0x0fffffff)` (@ 0x0047688e), not
//!   `0x00ffffff` as every other text of the game: the high byte's bit 0
//!   makes it a `PALETTEINDEX(0xffff)`, which a DC without a palette
//!   takes from the default palette, whose first entry is black when the
//!   index is out of range [INFERRED: Wine's GDI (`get_pixel_color`); a
//!   run on Windows would settle it].

use crate::subtitle::TextOut;

/// The colour of the message's text (see the module's notes).
pub const COLOUR: u32 = 0x000000;

/// The prefix of an unknown id's text (@ 0x005dc9a0).
const UNKNOWN: &[u8] = b"Unknown message ID :";

/// `isalnum` of the C locale (mask 0x107), and `_`.
fn key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The length of the key run at the start of `s`.
fn key_len(s: &[u8]) -> usize {
    s.iter().take_while(|&&b| key_byte(b)).count()
}

/// The id of a `MESSAGE_CLIC` value (@ 0x00476820).
pub fn id_of(value: &[u8]) -> Vec<u8> {
    value
        .iter()
        .take_while(|&&b| b != b'\r' && b != b'\n')
        .take(255)
        .copied()
        .collect()
}

/// The text of `id` in `table` (@ 0x00476640) and whether a line held it.
pub fn lookup(table: &[u8], id: &[u8]) -> (bool, Vec<u8>) {
    let n = key_len(id);
    let mut p = 0;
    while p < table.len() {
        let line = &table[p..];
        if key_len(line) == n && line[..n] == id[..n] {
            let mut out = Vec::new();
            let mut seen = false;
            for &c in &line[n..] {
                if c == 0 || c == b'\r' || c == b'\n' || out.len() >= 0x400 {
                    break;
                }
                if seen {
                    out.push(c);
                } else if c == b'=' {
                    seen = true;
                }
            }
            return (true, out);
        }
        while p < table.len() && table[p] != b'\r' && table[p] != b'\n' {
            p += 1;
        }
        if p >= table.len() {
            break;
        }
        if table[p] == b'\r' {
            p += 1;
        }
        if p < table.len() && table[p] == b'\n' {
            p += 1;
        }
    }
    let mut out = UNKNOWN.to_vec();
    out.extend_from_slice(id);
    (false, out)
}

/// The `TextOutA` calls of the text (@ 0x00476860) on a `w` x `h`
/// logical screen; `extent` is `GetTextExtentPoint32A`.
pub fn layout(
    text: &[u8],
    found: bool,
    extent: impl Fn(&[u8]) -> (i32, i32),
    w: i32,
    h: i32,
) -> Vec<TextOut> {
    let call = |bytes: &[u8], dy: i32| {
        let (tw, th) = extent(bytes);
        TextOut {
            x: w / 2 - tw / 2,
            y: h / 2 - th / 2 + dy,
            colour: COLOUR,
            bytes: bytes.to_vec(),
        }
    };
    // The text as a C string.
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    if extent(text).0 <= 640 {
        return vec![call(text, 0)];
    }
    let cut = if found {
        text.len() / 2
    } else {
        UNKNOWN.len()
    };
    let (first, second) = text.split_at(cut.min(text.len()));
    let mut calls = Vec::new();
    if !found {
        calls.push(call(first, -20));
    }
    calls.push(call(second, 20));
    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[u8] = b"; comment\r\nobNOT_IN_DEMO=Non accessible\r\nPAUSED=PAUSED\r\n\
                           TEST3=test message 3\r\nKEY  = spaced\r\nLONG=abcdefghij";

    #[test]
    fn lookup_as_the_original() {
        assert_eq!(lookup(TABLE, b"PAUSED"), (true, b"PAUSED".to_vec()));
        assert_eq!(lookup(TABLE, b"TEST3)"), (true, b"test message 3".to_vec()));
        // What follows the first `=`, its blanks kept.
        assert_eq!(lookup(TABLE, b"KEY"), (true, b" spaced".to_vec()));
        // A prefix is not a key.
        assert_eq!(
            lookup(TABLE, b"TEST"),
            (false, b"Unknown message ID :TEST".to_vec())
        );
        // An empty key matches the first line without one (the comment),
        // which has no `=`.
        assert_eq!(lookup(TABLE, b")"), (true, Vec::new()));
        // The last line, without an end.
        assert_eq!(lookup(TABLE, b"LONG"), (true, b"abcdefghij".to_vec()));
        assert_eq!(id_of(b"PAUSED\r\n/set"), b"PAUSED");
    }

    #[test]
    fn a_wide_text_shows_its_second_half_only() {
        // 10 pixels a byte.
        let ext = |b: &[u8]| (b.len() as i32 * 10, 20);
        let short = layout(b"0123456789", true, ext, 640, 480);
        assert_eq!(short.len(), 1);
        assert_eq!((short[0].x, short[0].y), (320 - 50, 240 - 10));
        let long = vec![b'a'; 81];
        let calls = layout(&long, true, ext, 800, 600);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].bytes.len(), 41);
        assert_eq!((calls[0].x, calls[0].y), (400 - 205, 300 - 10 + 20));
        let mut unknown = UNKNOWN.to_vec();
        unknown.extend(std::iter::repeat_n(b'x', 60));
        let calls = layout(&unknown, false, ext, 640, 480);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].bytes, UNKNOWN);
        assert_eq!((calls[0].x, calls[0].y), (320 - 100, 240 - 10 - 20));
        assert_eq!(calls[1].bytes.len(), 60);
    }
}
