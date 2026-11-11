//! Value formatting for call traces.
//!
//! The golden traces are made by a small C host linked with the reference
//! Lua 5.1.4 that follows the same rules, so the two outputs can be
//! compared byte for byte. Strings made only of
//! `[A-Za-z0-9_./\-]` and at most 80 bytes long (file names, ids) are
//! printed as they are; any other string (a line of text) is printed as
//! its FNV-1a 64-bit hash and length, so no game text reaches a trace.

use std::fmt::Write;

/// FNV-1a, 64 bits.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// `"name"` for a file name or id, `h:<hash>/<length>` for anything else.
pub fn fmt_str(out: &mut String, s: &[u8]) {
    let safe = s.len() <= 80
        && s.iter()
            .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'/' | b'\\' | b'-'));
    if safe {
        out.push('"');
        // Safe bytes are ASCII.
        out.extend(s.iter().map(|&c| char::from(c)));
        out.push('"');
    } else {
        let _ = write!(out, "h:{:016x}/{}", fnv1a(s), s.len());
    }
}

/// An integer as itself (`-0` for negative zero), anything else as the
/// bits of the double (`n:<16 hex digits>`).
pub fn fmt_num(out: &mut String, x: f64) {
    if x.is_finite() && x == x.floor() && x.abs() < 9_007_199_254_740_992.0 {
        if x == 0.0 && x.is_sign_negative() {
            out.push_str("-0");
        } else {
            // In range: exact as an i64.
            #[allow(clippy::cast_possible_truncation)]
            let i = x as i64;
            let _ = write!(out, "{i}");
        }
    } else {
        let _ = write!(out, "n:{:016x}", x.to_bits());
    }
}

/// Appends an error message on one line (newlines as `\n`), bytes as
/// they are.
pub fn push_escaped(out: &mut Vec<u8>, msg: &[u8]) {
    for &c in msg {
        if c == b'\n' {
            out.extend_from_slice(b"\\n");
        } else {
            out.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_numbers() {
        let mut s = String::new();
        fmt_str(&mut s, b"3D/Bateau_CA/0001.wp");
        s.push(' ');
        fmt_str(&mut s, b"Lua 5.1");
        s.push(' ');
        fmt_str(&mut s, b"");
        s.push(' ');
        fmt_num(&mut s, -1.0);
        s.push(' ');
        fmt_num(&mut s, -0.0);
        s.push(' ');
        fmt_num(&mut s, 0.5);
        assert_eq!(
            s,
            "\"3D/Bateau_CA/0001.wp\" h:4de6a7b0bcdb941f/7 \"\" -1 -0 n:3fe0000000000000"
        );
    }
}
