//! The subtitle breaker of the Chinese CDs, `FUN_0042f424` of
//! atlantis-cd-zh-tw-696832 (Big5) and atlantis-cd-zh-cn-692736 (GB2312):
//! a 75-byte window cut backwards after a double-byte punctuation mark
//! (in place of the other GDI builds' 100-byte window cut at a space,
//! [`super::split_subtitle`]); and their parent's, the Japanese DVD build's
//! `FUN_0042f618` with its Japanese texts: a row per space
//! ([`japanese_rows`]).
//!
//! Both read the subtitle buffer `0x00603cc0` (a space, the byte after
//! `]`, then the text) and write rows of 0x6e bytes at `0x006041c0`, their
//! count returned in AL. The words below name the function's locals:
//!
//! - `left` (`[ebp-0x30]`): the buffer's length (`strlen`, the space
//!   included), less each row's share;
//! - `s` (`[ebp-0x14]`): where the next row starts, first `0x603cc1` (the
//!   text, without the space);
//! - `e` (`[ebp-0x18]`): the end of the scan window, first `0x603d0b`
//!   (buffer + 75);
//! - `k` (`[ebp-0x28]`): how far back from `e` the scan is.
//!
//! The row is `strncpy`'d from `s` with a count of `75 - k + add`, where
//! the checks that give `add` read `s + 75 - k` (and `s + 74 - k`): the
//! mark's trail (and lead) byte for the first row, whose `s` is 74 bytes
//! before `e`, but one byte past them for the rows after a mark, whose
//! `s` is 73 bytes before `e`. The port keeps that arithmetic as it is.

use std::ops::Range;

use atlantis_data::exe::{CjkBreaker, ExeBuild};
use atlantis_data::TextCodePage;

/// The window: under 76 bytes (the space included) the buffer is one row
/// (`cmp [ebp-0x30],0x4b; jle`, atlantis-cd-zh-tw-696832 @ 0x0042f4a7,
/// 0x0042f4dd).
const WINDOW: isize = 0x4b;

/// How a Chinese build breaks its subtitles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CjkSplit {
    /// The cut marks (never [`CjkBreaker::None`] here).
    pub breaker: CjkBreaker,
    /// The texts' code page, for the character boundaries of the port's
    /// own cut (a window without a mark, see [`cjk_subtitle_rows`]) and of
    /// the rows' fix.
    pub code_page: TextCodePage,
    /// The rows' fix ([`crate::enhancements::BugFix::ChineseRows`]), in
    /// the Simplified build: after an ideographic
    /// space the next row starts two bytes on, past the whole space,
    /// instead of on its second byte (27 of the 72 shipped rows out of
    /// step); and the breaker reads a mark only at a character boundary,
    /// where the original tracks no lead bytes and also takes a pair
    /// across two characters for a mark (the trail byte of `。` and the
    /// lead byte of `　`, A3 A1, read as `！`: the other 45). Every row
    /// then decodes. No change in the Traditional build, whose shipped
    /// rows all decode.
    pub row_fix: bool,
}

impl CjkSplit {
    /// The breaker of `build`, `None` for the builds with the reference's
    /// 100-byte window.
    pub fn of_build(build: &ExeBuild, row_fix: bool) -> Option<CjkSplit> {
        (build.cjk_breaker != CjkBreaker::None).then_some(CjkSplit {
            breaker: build.cjk_breaker,
            code_page: build.text_code_page,
            row_fix,
        })
    }

    /// Whether the breaker reads its marks at character boundaries only
    /// (the rows' fix in the Simplified build).
    fn at_boundaries(self) -> bool {
        self.row_fix && self.breaker == CjkBreaker::Gb2312
    }

    /// Whether the scan stops at the pair (`lead`, `trail`): a punctuation
    /// mark, or a CR as lead byte.
    fn is_cut(self, lead: u8, trail: u8) -> bool {
        match self.breaker {
            // atlantis-cd-zh-tw-696832 @ 0x0042f510..0x0042f566: A1 41 `，`,
            // A1 43 `。`, A1 46 `；`, A1 48 `？`, A1 49 `！`, A1 4B `…`;
            // 0x0042f56c: CR.
            CjkBreaker::Big5 => {
                lead == 0xa1 && matches!(trail, 0x41 | 0x43 | 0x46 | 0x48 | 0x49 | 0x4b)
                    || lead == b'\r'
            }
            // atlantis-cd-zh-cn-692736 @ 0x0042f510..0x0042f552: A1 A1 (the
            // ideographic space), A3 AC `，`, A1 A3 `。`, A3 BF `？`, A3 A1
            // `！`; 0x0042f55c: CR.
            CjkBreaker::Gb2312 => {
                matches!(
                    (lead, trail),
                    (0xa1, 0xa1) | (0xa3, 0xac) | (0xa1, 0xa3) | (0xa3, 0xbf) | (0xa3, 0xa1)
                ) || lead == b'\r'
            }
            CjkBreaker::None | CjkBreaker::Spaces => false,
        }
    }

    /// The count's addend and the next row's skip from the cut, given the
    /// bytes at `s + 75 - k` (`at`) and `s + 74 - k` (`before`).
    fn after_cut(self, at: u8, before: u8) -> (isize, isize) {
        match self.breaker {
            // atlantis-cd-zh-tw-696832 @ 0x0042f59f..0x0042f615: add 1 when
            // the byte is 41 43 46 48 49 4B (a mark's trail byte), skip 2.
            CjkBreaker::Big5 => (
                isize::from(matches!(at, 0x41 | 0x43 | 0x46 | 0x48 | 0x49 | 0x4b)),
                2,
            ),
            CjkBreaker::Gb2312 => match (before, at) {
                // atlantis-cd-zh-cn-692736 @ 0x0042f5da..0x0042f5ef: A1 A1,
                // the row ends before it (add -1) and the next starts one
                // byte past its lead byte (skip 1), on its trail byte: that
                // row reads its pairs one byte out of step. With the rows'
                // fix, two bytes on, as after the other marks.
                (0xa1, 0xa1) => (-1, if self.row_fix { 2 } else { 1 }),
                // @ 0x0042f58b..0x0042f5d3: add 1 after A3 AC, A1 A3, A3 BF,
                // A3 A1; skip 2.
                (0xa3, 0xac) | (0xa1, 0xa3) | (0xa3, 0xbf) | (0xa3, 0xa1) => (1, 2),
                _ => (0, 2),
            },
            CjkBreaker::None | CjkBreaker::Spaces => (0, 2),
        }
    }
}

/// atlantis-dvd-ja-696832 `FUN_0042f618` with the subtitle language
/// counter (0x00486098) at 0, its Japanese texts: the rows, as ranges of
/// the buffer (a space, then `text`). The text is copied byte by byte from
/// after the buffer's leading space; each ASCII space ends a row (its NUL
/// written in the space's place, the next row started), a CR or the NUL
/// ends the text, and the row then open is the last, counted even when
/// empty (@ 0x0042f798..0x0042f84e). No window and no width: a row is
/// drawn whole (the Shift-JIS trail bytes are never 0x20, so a row never
/// starts inside a character). The shipped texts end their sentences with
/// a space: 2477 of the 2542 lines end with one, whose empty last row
/// lifts the others by a row. (The original writes rows past its eighth
/// beyond its buffer; no shipped text has more than six.)
pub fn japanese_rows(text: &[u8]) -> Vec<Range<usize>> {
    let end = text
        .iter()
        .position(|&c| c == b'\r' || c == 0)
        .unwrap_or(text.len());
    let mut rows = Vec::new();
    let mut start = 1;
    for (i, &c) in text[..end].iter().enumerate() {
        if c == b' ' {
            rows.push(start..i + 1);
            start = i + 2;
        }
    }
    rows.push(start..end + 1);
    rows
}

/// `FUN_0042f424` on the subtitle buffer: the rows, as ranges of the
/// buffer (a space, then `text`: the line's text as the dialog code copies
/// it, up to the CR, its final blanks included).
///
/// The buffer under 76 bytes is one row, the space included. Otherwise,
/// while `left` is over 75: the scan goes back from `e` (k = 1, 2...) to
/// the first pair (`e - k`, `e - k + 1`) that is a cut; the row is the
/// `75 - k + add` bytes from `s` (stopping at the text's end, `strncpy`);
/// then `left -= 76 - k`, `s = e - k + skip`, `e = e - k + 75`
/// (atlantis-cd-zh-tw-696832 @ 0x0042f4dd..0x0042f65a, 0x0042f67b..
/// 0x0042f692; atlantis-cd-zh-cn-692736 @ 0x0042f4dd..0x0042f634). The
/// rest is the last row (`strcpy` @ 0x0042f660 / 0x0042f63a), unless
/// `left` is not positive.
///
/// The scan has no lower bound. After a cut it finds that cut again at
/// k = 75 at the latest, so a later window without a mark gives a row of
/// `add` bytes and no progress but `left` one less, until the rest is the
/// last row; past 8 rows the original writes beyond its 8-row buffer
/// (`memset` of 0x370 bytes @ 0x0042f4b1), with effects [UNKNOWN]. No
/// shipped text does it.
///
/// A first window without a mark (zh-tw: CHAPI084's game-over line, 80
/// bytes) sends the original's scan back past the buffer's start, into
/// whatever lies before it: under emulation it never returns; what the
/// game does is [UNKNOWN]. The port, deviating, cuts the row at the last
/// character boundary within 75 bytes and goes on as if a two-byte mark
/// ended there; it does the same for any scan that finds no cut within
/// 75 bytes.
///
/// With the rows' fix, in the Simplified build only, a pair is a
/// mark only when its first byte starts a character (counted from the
/// row's start `s`, the lead bytes taking two bytes): the scan passes over
/// a pair across two characters (zh-cn @ 0x0042f4ed..0x0042f568: the trail
/// byte of `。` and the lead byte of `　`, A3 A1, read as `！`), and so do
/// the checks of `add` and `skip` (@ 0x0042f56a..0x0042f5ef), which in
/// the rows after a mark read its trail byte and the next character's lead
/// byte: such a pair gives add 0 and skip 2, as the checks' other pairs.
/// Then every row starts and ends at a character boundary. (The buffer
/// never holds a CR: the dialog code and the game over copy the line up
/// to it.) A later window without a mark finds no cut, the last one lying
/// before `s`, and takes the port's cut.
pub fn cjk_subtitle_rows(text: &[u8], split: CjkSplit) -> Vec<Range<usize>> {
    if split.breaker == CjkBreaker::Spaces {
        return japanese_rows(text);
    }
    let mut b = Vec::with_capacity(text.len() + 1);
    b.push(b' ');
    b.extend_from_slice(text);
    let len = b.len();
    if len as isize <= WINDOW {
        return std::iter::once(0..len).collect();
    }
    // Nothing past the text is read: the first window ends at 75, inside
    // the buffer; with `p` the lead byte of the last cut, a later window
    // ends at `e = p + 75` and `left` is at most `len - p - 1`, so while
    // `left` is over 75 the scan ends two bytes before the NUL at the
    // latest; the checks read `p + 1` or `p + 2`, and the copies stop at
    // the NUL. (After it lie the bytes of longer earlier lines: the
    // dialog code writes only the NUL, atlantis-cd-zh-tw-696832 @
    // 0x00428014.)
    let at = |i: isize| -> u8 {
        usize::try_from(i)
            .ok()
            .and_then(|i| b.get(i).copied())
            .unwrap_or(0)
    };
    // A row of up to `n` bytes from `s`, ending at the text's end.
    let row = |s: isize, n: isize| -> Range<usize> {
        let s = s.clamp(0, len as isize) as usize;
        s..(s + n.max(0) as usize).min(len)
    };
    // Whether `pos` starts a character, counted from the row's start `s`
    // (the lead bytes taking two bytes), for the rows' fix; any byte
    // otherwise.
    let starts = |s: isize, pos: isize| -> bool {
        if !split.at_boundaries() {
            return true;
        }
        let mut c = s;
        while c < pos {
            c += 1 + isize::from(split.code_page.is_lead_byte(at(c)));
        }
        c == pos
    };
    let mut rows = Vec::new();
    let mut left = len as isize;
    let (mut s, mut e) = (1isize, WINDOW);
    loop {
        if left <= WINDOW {
            rows.push(row(s, len as isize));
            break;
        }
        let (count, skip, k) = match (1..=WINDOW)
            .find(|&k| split.is_cut(at(e - k), at(e - k + 1)) && starts(s, e - k))
        {
            Some(k) => {
                let (add, skip) = if starts(s, s + WINDOW - 1 - k) {
                    split.after_cut(at(s + WINDOW - k), at(s + WINDOW - 1 - k))
                } else {
                    // add [ebp-0x20] 0, skip [ebp-0x1c] 2 (@ 0x0042f58b)
                    (0, 2)
                };
                // (a count of -1 would be a strncpy of 4 GB; no text
                // gives one)
                (WINDOW - k + add, skip, k)
            }
            None => {
                // The port's cut: the whole characters from s within 75
                // bytes, as if a two-byte mark ended after them.
                let mut c = s;
                while c < len as isize {
                    let w = if split.code_page.is_lead_byte(at(c)) {
                        2
                    } else {
                        1
                    };
                    if c + w - s > WINDOW {
                        break;
                    }
                    c = (c + w).min(len as isize);
                }
                (c - s, 2, e - (c - 2))
            }
        };
        rows.push(row(s, count));
        left -= WINDOW - k + 1;
        s = e - k + skip;
        e = e - k + WINDOW;
        // (left falls by at least one a row but in the port's cut of a
        // window whose start is past the text, which no text reaches)
        if left <= 0 || rows.len() > len {
            break;
        }
    }
    rows
}

/// [`cjk_subtitle_rows`] as the rows' bytes.
pub fn split_subtitle_cjk(text: &[u8], split: CjkSplit) -> Vec<Vec<u8>> {
    let mut b = Vec::with_capacity(text.len() + 1);
    b.push(b' ');
    b.extend_from_slice(text);
    cjk_subtitle_rows(text, split)
        .into_iter()
        .map(|r| b[r].to_vec())
        .collect()
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use atlantis_data::exe::{BUILDS, DVD_JA, ZH_CN, ZH_TW};

    /// 一二三四五六七八九十 in Big5 and in GB2312.
    const TEN_BIG5: &[u8] =
        b"\xa4\x40\xa4\x47\xa4\x54\xa5\x7c\xa4\xad\xa4\xbb\xa4\x43\xa4\x4b\xa4\x45\xa4\x51";
    const TEN_GB: &[u8] =
        b"\xd2\xbb\xb6\xfe\xc8\xfd\xcb\xc4\xce\xe5\xc1\xf9\xc6\xdf\xb0\xcb\xbe\xc5\xca\xae";

    /// The text of `parts`: a number is that many times the ten
    /// characters, a byte string itself.
    enum P {
        Ten(usize),
        B(&'static [u8]),
    }

    fn text(ten: &[u8], parts: &[P]) -> Vec<u8> {
        let mut t = Vec::new();
        for p in parts {
            match p {
                P::Ten(n) => (0..*n).for_each(|_| t.extend_from_slice(ten)),
                P::B(b) => t.extend_from_slice(b),
            }
        }
        t
    }

    fn rows(t: &[u8], build: &ExeBuild, fixes: bool) -> Vec<Range<usize>> {
        cjk_subtitle_rows(t, CjkSplit::of_build(build, fixes).unwrap())
    }

    #[test]
    fn only_the_chinese_builds_have_it() {
        for b in &BUILDS {
            let cjk = CjkSplit::of_build(b, false);
            assert_eq!(
                cjk.is_some(),
                b.text_code_page.is_double_byte(),
                "{}",
                b.name
            );
        }
    }

    /// atlantis-cd-zh-tw-696832 FUN_0042f424 run unmodified under
    /// emulation on these texts: the rows' places in the buffer (a space,
    /// then the text), read from the `strncpy` / `strcpy` sources.
    #[test]
    fn big5_rows_match_original() {
        use P::*;
        let t = |p: &[P]| text(TEN_BIG5, p);
        let cases: Vec<(Vec<u8>, Vec<Range<usize>>)> = vec![
            // 短句。: one row, its space included.
            (b"\xb5\x75\xa5\x79\xa1\x43".to_vec(), vec![0..7]),
            // 74 bytes: the buffer is 75, one row.
            (b"\xa4\x40".repeat(37), vec![0..75]),
            // A cut after `，` (A1 41), the rest.
            (
                t(&[Ten(3), B(b"\xa1\x41"), Ten(2), B(b"\xa1\x43"), Ten(1)]),
                vec![1..63, 63..125],
            ),
            // `；` `？` `！` `…`.
            (
                t(&[
                    Ten(3),
                    B(b"\xa1\x46"),
                    Ten(3),
                    B(b"\xa1\x48"),
                    Ten(3),
                    B(b"\xa1\x49"),
                    Ten(3),
                    B(b"\xa1\x4b"),
                    Ten(1),
                ]),
                vec![1..63, 63..125, 125..187, 187..249, 249..269],
            ),
            // After the first row, the addend reads the byte after the
            // mark: an `A` there makes the row one byte longer, and the
            // next row starts on it again.
            (
                t(&[
                    Ten(3),
                    B(b"\xa1\x41"),
                    Ten(3),
                    B(b"\xa1\x43A"),
                    Ten(3),
                    B(b"\xa1\x43"),
                    Ten(1),
                ]),
                vec![1..63, 63..126, 125..188, 188..208],
            ),
            // A CR: the row ends after it, the next starts 2 bytes past it
            // (on the next character's trail byte).
            (t(&[Ten(2), B(b"\r"), Ten(3)]), vec![1..42, 43..102]),
            // A later window without a mark: the scan finds the last cut
            // again (k = 75); empty rows, `left` one less each, until the
            // rest is the last row.
            (t(&[Ten(3), B(b"\xa1\x41"), Ten(5), B(b"\xa1\x43")]), {
                let mut v = vec![1..63];
                v.extend((0..28).map(|_| 63..63));
                v.push(63..165);
                v
            }),
        ];
        for (text, want) in cases {
            assert_eq!(rows(&text, &ZH_TW, false), want, "{text:02x?}");
            // No change with the rows' fix.
            assert_eq!(rows(&text, &ZH_TW, true), want);
        }
    }

    /// atlantis-cd-zh-cn-692736 FUN_0042f424 under emulation, as above;
    /// with the rows' fix, the same function with the skip after an
    /// ideographic space patched from 1 to 2 (@ 0x0042f5ef) and the pairs
    /// read by the scan (@ 0x0042f510) and by the checks (@ 0x0042f599)
    /// cleared when their first byte does not start a character, counted
    /// from the row's start.
    #[test]
    fn gb2312_rows_match_original() {
        use P::*;
        let t = |p: &[P]| text(TEN_GB, p);
        let space = b"\xa1\xa1";
        // The text, its rows, its rows with the rows' fix.
        type Case = (Vec<u8>, Vec<Range<usize>>, Vec<Range<usize>>);
        let cases: Vec<Case> = vec![
            // 短句。
            (
                b"\xb6\xcc\xbe\xe4\xa1\xa3\xa1\xa1".to_vec(),
                vec![0..9],
                vec![0..9],
            ),
            (
                t(&[Ten(3), B(b"\xa3\xac"), Ten(2), B(b"\xa1\xa3"), Ten(1)]),
                vec![1..63, 63..125],
                vec![1..63, 63..125],
            ),
            // An ideographic space: the row ends before it, the next
            // starts on its second byte (out of step); with the fixes,
            // after it.
            (
                t(&[Ten(3), B(space), Ten(3), B(b"\xa1\xa3"), Ten(1)]),
                vec![1..61, 62..125, 125..145],
                vec![1..61, 63..125, 125..145],
            ),
            // In a later row the checks read one byte on: the row keeps
            // the space.
            (
                t(&[
                    Ten(3),
                    B(b"\xa3\xac"),
                    Ten(3),
                    B(space),
                    Ten(3),
                    B(b"\xa1\xa3"),
                ]),
                vec![1..63, 63..125, 125..187],
                vec![1..63, 63..125, 125..187],
            ),
            // `？` `！`, and `。　` across a window's end.
            (
                t(&[
                    Ten(3),
                    B(b"\xa3\xbf"),
                    Ten(3),
                    B(b"\xa3\xa1"),
                    Ten(3),
                    B(b"\xa1\xa3\xa1\xa1"),
                    Ten(1),
                ]),
                vec![1..63, 63..125, 125..189, 189..209],
                vec![1..63, 63..125, 125..189, 189..209],
            ),
            // `。　` ending the text past the window: the scan takes the
            // trail byte of `。` and the lead byte of the space for `！`
            // (A3 A1); the last row is the space's trail byte alone. With
            // the fixes, the cut after `。`, the space the last row.
            (
                [b"\xd2\xbb".repeat(36), b"\xa1\xa3\xa1\xa1".to_vec()].concat(),
                vec![1..76, 76..77],
                vec![1..75, 75..77],
            ),
            // A later row whose window ends on `。`, a space after it: the
            // checks read the trail byte of `。` and the lead byte of the
            // space as `！` (A3 A1), add 1, and the row takes the space's
            // lead byte; the next starts on the space. With the fixes, the
            // row ends after `。`.
            (
                t(&[
                    Ten(3),
                    B(b"\xa3\xac"),
                    Ten(3),
                    B(&TEN_GB[..12]),
                    B(b"\xa1\xa3"),
                    B(space),
                    Ten(1),
                ]),
                vec![1..63, 63..138, 137..159],
                vec![1..63, 63..137, 137..159],
            ),
            (
                t(&[Ten(2), B(b"\r"), Ten(3)]),
                vec![1..42, 43..102],
                vec![1..42, 43..102],
            ),
        ];
        for (text, orig, fixed) in cases {
            assert_eq!(rows(&text, &ZH_CN, false), orig, "{text:02x?}");
            assert_eq!(rows(&text, &ZH_CN, true), fixed, "{text:02x?}");
        }
    }

    /// With the rows' fix, a Simplified later window without a mark:
    /// the last cut lies before the row's start, so the scan finds none
    /// and the port cuts at the last character boundary within 75 bytes
    /// (the original gives empty rows, as in the Traditional build).
    #[test]
    fn gb2312_fixed_later_window_without_a_mark() {
        let t = text(
            TEN_GB,
            &[P::Ten(3), P::B(b"\xa3\xac"), P::Ten(5), P::B(b"\xa1\xa3")],
        );
        let mut orig = vec![1..63];
        orig.extend((0..28).map(|_| 63..63));
        orig.push(63..165);
        assert_eq!(rows(&t, &ZH_CN, false), orig);
        assert_eq!(rows(&t, &ZH_CN, true), vec![1..63, 63..137, 137..165]);
    }

    /// A first window without a mark: the port's cut at the last character
    /// boundary within 75 bytes (the original's scan leaves the buffer).
    #[test]
    fn no_mark_cuts_at_a_character_boundary() {
        // 80 ASCII bytes: 75, then 5.
        let ascii = vec![b'.'; 80];
        assert_eq!(rows(&ascii, &ZH_TW, false), vec![1..76, 76..81]);
        // `a` then 40 two-byte characters: the 38th would end at 77.
        let mut t = b"a".to_vec();
        t.extend(TEN_BIG5.repeat(4));
        assert_eq!(rows(&t, &ZH_TW, false), vec![1..76, 76..82]);
        let mut t = b"ab".to_vec();
        t.extend(TEN_GB.repeat(4));
        let r = rows(&t, &ZH_CN, false);
        assert_eq!(r, vec![1..75, 75..83]);
        // Every row whole characters.
        let b = [b" ".as_slice(), &t].concat();
        for x in r {
            assert!(!ZH_CN.text_code_page.decode(&b[x]).contains('\0'));
        }
        // Longer: rows of up to 75 bytes, all the text in them.
        let long = vec![b'x'; 300];
        let r = rows(&long, &ZH_TW, false);
        assert!(r.iter().all(|x| x.len() <= 75));
        assert_eq!(r.iter().map(|x| x.len()).sum::<usize>(), 300);
    }

    /// The dialog presenter's subtitler breaks the lines of a Chinese
    /// build with it, the rows' fix as it stands at the line.
    #[test]
    fn subtitler_uses_it() {
        use crate::subtitles::{SubtitleFont, Subtitler};
        let t = text(
            TEN_GB,
            &[
                P::Ten(3),
                P::B(b"\xa1\xa1"),
                P::Ten(3),
                P::B(b"\xa1\xa3"),
                P::Ten(1),
            ],
        );
        let font = std::rc::Rc::new(SubtitleFont::new());
        let s = Subtitler::Gdi(font.clone(), CjkSplit::of_build(&ZH_CN, false));
        let lens = |s: &Subtitler| s.split(&t).iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(lens(&s), [60, 63, 20]);
        assert_eq!(lens(&s.clone().with_row_fix(true)), [60, 62, 20]);
        // The other builds: the 100-byte window cut at a space.
        let s = Subtitler::Gdi(font.clone(), None).with_row_fix(true);
        assert_eq!(s.split(&t), crate::subtitles::split_subtitle(&t));

        // A final blank: 72 bytes, `。`, a space. The Chinese breaker counts
        // it (a buffer of 76 bytes: the row, then the space alone, as the
        // original's emulation on 17 shipped Traditional lines); the
        // others are given the text without it.
        let mut t = text(TEN_BIG5, &[P::Ten(3)]);
        t.extend_from_slice(&TEN_BIG5[..12]);
        t.extend_from_slice(b"\xa1\x43");
        let blank = [t.as_slice(), b" "].concat();
        let s = Subtitler::Gdi(font.clone(), CjkSplit::of_build(&ZH_TW, false));
        assert_eq!(s.split(&t), [[b" ".as_slice(), &t].concat()]);
        assert_eq!(s.split(&blank), [t.clone(), b" ".to_vec()]);
        let s = Subtitler::Gdi(font, None);
        assert_eq!(s.split(&blank), s.split(&t));
        assert_eq!(s.split(b"a \t"), s.split(b"a"));
    }

    /// atlantis-dvd-ja-696832 FUN_0042f618 run unmodified under emulation
    /// with the language counter at 0 (the samples of the golden
    /// video_subtitles_dvd_ja.txt): a row per space, the last always.
    #[test]
    fn japanese_rows_match_original() {
        let split = CjkSplit::of_build(&DVD_JA, false).unwrap();
        let rows = |t: &[u8]| split_subtitle_cjk(t, split);
        let v = |r: &[&[u8]]| r.iter().map(|x| x.to_vec()).collect::<Vec<_>>();
        assert_eq!(rows(b"a b c"), v(&[b"a", b"b", b"c"]));
        assert_eq!(rows(b"a b c "), v(&[b"a", b"b", b"c", b""]));
        assert_eq!(rows(b"a  b"), v(&[b"a", b"", b"b"]));
        assert_eq!(rows(b" a"), v(&[b"", b"a"]));
        assert_eq!(rows(b""), v(&[b""]));
        assert_eq!(rows(&b"word ".repeat(30)).len(), 31);
        let long = [b"x".repeat(60), b" ".to_vec()].concat().repeat(3);
        assert_eq!(
            rows(&long),
            v(&[&[b'x'; 60], &[b'x'; 60], &[b'x'; 60], b""])
        );
        // The ranges are the buffer's (" " + the text).
        assert_eq!(japanese_rows(b"ab c"), [1..3, 4..5]);
        // A Shift-JIS pair whose trail byte is 0x40..0x7e: never a cut.
        let t = b"\x83\x5a\x83\x58\x81\x42 \x82\xa0";
        assert_eq!(rows(t), v(&[b"\x83\x5a\x83\x58\x81\x42", b"\x82\xa0"]));
    }

    /// The Japanese build's subtitler: the rows at the spaces and 36 px
    /// lower while its texts are Japanese, the reference's rows otherwise.
    #[test]
    fn japanese_subtitler() {
        use crate::subtitles::{split_subtitle, SubtitleFont, Subtitler, JAPANESE_ROWS_LOWER};
        let font = std::rc::Rc::new(SubtitleFont::new().with_row_step(18));
        let ja = Subtitler::Gdi(font.clone(), CjkSplit::of_build(&DVD_JA, false));
        let en = Subtitler::Gdi(font, None);
        let t = b"one two ";
        assert_eq!(ja.split(t), [b"one".to_vec(), b"two".to_vec(), Vec::new()]);
        assert_eq!(en.split(t), split_subtitle(b"one two"));
        assert_eq!((ja.rows_lower(), en.rows_lower()), (JAPANESE_ROWS_LOWER, 0));
        assert_eq!(JAPANESE_ROWS_LOWER, 36);
        let items = ja.items(&ja.split(t), 0, 0);
        // Row i of n at 0x1f2 - (n - i + 1) * 18 (the shadow first, 1 px
        // lower): row 0 of 3 at 426, the empty last row at 462.
        assert_eq!(items[1].y, 0x1f2 - 4 * 18);
        assert_eq!(items[5].y, 462);
        assert_eq!(en.items(&en.split(t), 0, 0)[1].y, 0x1ce - 2 * 18);
    }

    #[test]
    fn split_gives_the_rows_bytes() {
        let t = text(TEN_BIG5, &[P::Ten(3), P::B(b"\xa1\x41"), P::Ten(2)]);
        let r = split_subtitle_cjk(&t, CjkSplit::of_build(&ZH_TW, false).unwrap());
        assert_eq!(r, vec![t[..62].to_vec(), t[62..].to_vec()]);
    }
}
