//! The chapter scripts, `SCENAR\CHAPI<nnn>.CON`.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`con_script.cpp`,
//! `con_script.h`; GPLv3, the ScummVM team).
//!
//! The original keeps no parsed structure: it walks the text every time.
//! [`ConScript`] keeps the text and lists what those walks find, in the
//! order they find it:
//!
//! - `/INIT`: `FUN_00425eb0` runs the commands of the following lines until
//!   a line has none;
//! - `/N`: the section number, the last "/digit" a walk passed;
//! - `/sel`, `/Sel`: automatic entry (`FUN_004280f8`);
//! - `/con`, `/Con`: player-triggered entry (`FUN_004281f8`); `/conN` allows
//!   N plays, `/con*` any number;
//! - `/tim1`, `/Tim1`, `/tim2`, `/Tim2`: timer entries (`FUN_0042bef4`,
//!   `FUN_0042bfa4`);
//! - `/Suj`, `/suj`: the subject gate of a /con entry, only as the first
//!   line of its body (`FUN_00425828`);
//! - `/set`, `/Set`: a game-state command; `/go`, `/Go`: a jump to "/N";
//! - `[Speaker,params] text`: a dialog line; `#`: a comment.
//!
//! Other spellings are not recognised. A section can hold several entry
//! lines; each is a separate entry point. The text is read as bytes; the
//! strings below hold one char per byte (Latin-1).

/// A dialog line, `[Speaker,params] text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConLine {
    /// e.g. "Seth", "Garde13".
    pub speaker: String,
    /// e.g. "cam3", "off" (comma-separated in the source).
    pub params: String,
    /// The text, trimmed at its end.
    pub text: String,
    /// The blanks the trim took from the end of a non-empty text: the
    /// dialog code copies the text up to the CR, so they are in the
    /// subtitle buffer too (atlantis-cd-zh-tw-696832 FUN_00427cc0 @
    /// 0x00427ff9..0x00428014, the same in atlantis-cd-zh-cn-692736), where
    /// the Chinese builds' breaker counts them.
    pub trail: String,
    /// `[Speaker,X,Y,camN]`: the view angles in degrees the dialog code
    /// writes into the view before the line (`FUN_0042844c` @
    /// 0x0042852b..0x004285fa): set when the byte right after the line's
    /// first ',' (before its ']') is a digit, with no blank or sign skipped;
    /// Y is read after the next ',' when that one is also before the ']'
    /// and followed by a digit, else Y = X.
    pub angle: Option<(i32, i32)>,
    /// The line as the file has it, from its '[' up to (not including) its
    /// CR, its final blanks kept: what the section executor and the
    /// game-over screen copy into the 1997 builds' subtitle text buffer
    /// ([`con_line_buffer_text`], [`con_narrator_buffer_text`]).
    pub raw: String,
}

/// One item of a section's walk: a dialog line or a /set command, in the
/// file's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConItem {
    Line(ConLine),
    /// The command (the text after "/set(", without a final ')') and the
    /// offset of its "/set" in the text, where `FUN_0042a100` reads it.
    Set {
        cmd: String,
        offset: u32,
    },
}

impl ConItem {
    pub fn is_set(&self) -> bool {
        matches!(self, ConItem::Set { .. })
    }
}

/// An entry point found by one of the original's scanners (/sel, /con,
/// /tim1, /tim2), or a section header no scanner reaches (`is_entry`
/// false: only reachable through /go).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConSection {
    /// The last "/N" before the entry line.
    pub id: i32,
    pub is_entry: bool,
    /// /con or /Con (player-triggered).
    pub is_con: bool,
    /// /con*: not limited by the play counter.
    pub repeatable: bool,
    /// /conN: plays allowed by the counter (0 counts as 1).
    pub con_limit: i32,
    /// /tim1, /Tim1, /tim2, /Tim2.
    pub is_timer: bool,
    /// 1 or 2.
    pub timer_num: i32,
    /// The subject gate of a /con entry: non-empty when its body starts
    /// with "/Suj" or "/suj" (`FUN_00425828` @ 0x00425ad3..0x00425af9, 4
    /// bytes, those two spellings only; a /suj line further down the body
    /// is plain text). The numbers `FUN_004264a8` reads from 4 bytes on, in
    /// its order (`/suj1,2`: the entry fits the chosen subject 1 or 2, when
    /// that subject is enabled for the clicked perso).
    pub suj_ids: Vec<i32>,
    /// /goN redirect (0 = none).
    pub goto_id: i32,
    /// Where the scanner starts the evaluation (`None` for a header).
    pub cond_offset: Option<u32>,
    /// The start of the entry line, or of the header.
    pub line_offset: u32,
    /// Where the executor starts.
    pub body_offset: u32,
    /// What the section executor runs, up to the /go.
    pub items: Vec<ConItem>,
}

impl ConSection {
    /// The first /suj id (0 = none), the subject menu's icon.
    pub fn suj_id(&self) -> i32 {
        self.suj_ids.first().copied().unwrap_or(0)
    }
}

/// The number of /con play counters: the byte array at 0x006cc6cc (0x5dc
/// bytes).
pub const CON_PLAY_COUNTERS: usize = 0x5dc;

/// `FUN_004281f8`: a /con entry is skipped while the play counter of its
/// section has reached the entry's limit; /con* entries are never skipped.
/// Sections past the end of the array read as 0 (the original reads other
/// data there).
pub fn con_entry_allowed(sec: &ConSection, counters: &[u8]) -> bool {
    if sec.repeatable {
        return true;
    }
    let played = usize::try_from(sec.id)
        .ok()
        .and_then(|i| counters.get(i))
        .copied()
        .unwrap_or(0);
    i32::from(played) < sec.con_limit
}

/// The track letter of a dialog line, the `<L>` of its voice, lip-sync and
/// head files `wav\ata<NNNN><L>.apc` / `.syc` (`FUN_0042f9e8` appends the
/// letter byte 0x004961a8). The section executor `FUN_0042844c` sets it to
/// 'a' at the start (0x004284cb), adds 1 after each played line (0x00428b21)
/// and per `[` line skipped after an Escape (0x00428b85); /go
/// (`FUN_00425c04`, 0x00425c4b) sets it back to 'a'. So it is the index of
/// the line among the `[` lines of its part of the walk. Upper case: the
/// names in the archives.
pub fn con_track_letter(line_idx: usize) -> char {
    char::from(b'A'.wrapping_add(line_idx as u8))
}

/// The bytes the section executor copies into the subtitle text buffer for
/// a `[` line (the bytes of [`ConLine::raw`]): from the byte after the line's
/// first ']' up to the CR (atlantis-cd-en-688128 `FUN_00427420` @
/// 0x0042764a..0x00427653, the ']' search, then the copy
/// 0x00427707..0x00427722; the reference's `FUN_0042844c` copies the same
/// way, 0x00428785), final blanks kept. The copy is a do-while: a ']' right
/// before the CR would copy the CR, the LF and the next line; no shipped
/// line has one, the port copies nothing then.
pub fn con_line_buffer_text(raw: &[u8]) -> &[u8] {
    match raw.iter().position(|&c| c == b']') {
        Some(close) if close + 1 < raw.len() => &raw[close + 1..],
        _ => &[],
    }
}

/// The game-over screen's copy of its narrator line (atlantis-cd-en-688128
/// `FUN_004256a4` @ 0x00425a31..0x00425a50): from the line's 13th byte (the
/// byte after "[Narratrice]" in all 144 game overs of every 1997 release)
/// up to the CR. A line of 12 bytes or fewer would copy its CR and on; none
/// is shipped, the port copies nothing then.
pub fn con_narrator_buffer_text(raw: &[u8]) -> &[u8] {
    raw.get(12..).unwrap_or(&[])
}

/// The narrator line of a game over: `FUN_00426590` reads the line right
/// after the /set(gameover=N) line when it starts with "[Narra"
/// (0x004267e0..0x004267f4) and takes its text from the byte after
/// "[Narratrice]". `items[set_idx]` is the gameover= item.
pub fn con_game_over_narrator(items: &[ConItem], set_idx: usize) -> Option<&ConLine> {
    match items.get(set_idx + 1) {
        Some(ConItem::Line(l)) if l.speaker.starts_with("Narra") => Some(l),
        _ => None,
    }
}

/// The 1999 build's alternative game-over narrations (`FUN_00426570` of
/// the GOG build @ 0x004267ce..0x00426882 and 0x004269e3; the 2001
/// reference resets the counter before the test, 0x004267d9). `count` is
/// the per-section byte 0x6cccac[section]: 2 or more -> 0; n > 0 -> the
/// line n lines after the first narrator line is used when it is also a
/// narrator line (else the counter goes back to 0); then the counter is
/// incremented. Returns how many lines after the first narrator line the
/// narration is (0 or n); the track letter grows by as much.
pub fn con_alt_game_over_narration(items: &[ConItem], set_idx: usize, count: &mut u8) -> usize {
    if *count >= 2 {
        *count = 0;
    }
    let mut pick = 0;
    if *count != 0 {
        let k = set_idx + 1 + usize::from(*count);
        match items.get(k) {
            Some(ConItem::Line(l)) if l.speaker.starts_with("Narra") => pick = usize::from(*count),
            _ => *count = 0,
        }
    }
    *count += 1;
    pick
}

/// Escape during a talking-head line (`FUN_0041a298` @ 0x0041a3c0 sets
/// 0x00496134 while 0x00496188 == 1): after the line the section executor
/// `FUN_0042844c` (0x00428b6d..0x00428bcd) walks the text up to the next
/// '/' and adds 1 to the track letter for each '[' it passes, then runs
/// that '/' line (/set or /go) and goes on. In the item list: the dialog
/// items from `next` on are skipped up to the first /set item (or the end
/// of the items, where the /go or the next header is), each one counted in
/// `line_idx`. Returns the index of the item to run next.
pub fn con_escape_skip(items: &[ConItem], mut next: usize, line_idx: &mut usize) -> usize {
    while next < items.len() && !items[next].is_set() {
        *line_idx += 1;
        next += 1;
    }
    next
}

/// The ArriveeVue of the place loop's first arrival scan after a chapter
/// load (a checkpoint load, a game-over reload or a new game: all go
/// through `FUN_0042de5c`): its chapter change `FUN_00427634` sets
/// ARRIVEEVUE to LastVue (0x0042777d), read from the checkpoint or 1 when
/// there is none; the place loop `FUN_00423ad8` runs the scan only when
/// ARRIVEEVUE != 0 (0x00423b48). `None` when no scan runs.
pub fn arrival_after_chapter_load(last_vue: u16) -> Option<i32> {
    let arrivee = i32::from(last_vue);
    (arrivee != 0).then_some(arrivee)
}

/// The script text as the original's walks read it: bytes past the end
/// read 0.
#[derive(Clone, Copy)]
pub struct ConText<'a>(pub &'a [u8]);

impl ConText<'_> {
    pub fn len(&self) -> u32 {
        self.0.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn at(&self, pos: u32) -> u8 {
        self.0.get(pos as usize).copied().unwrap_or(0)
    }

    /// strncmp(text + pos, s, n) == 0.
    pub fn matches(&self, pos: u32, s: &[u8]) -> bool {
        s.iter()
            .enumerate()
            .all(|(i, &c)| self.at(pos + i as u32) == c)
    }

    pub fn is_header(&self, pos: u32) -> bool {
        self.at(pos) == b'/' && self.at(pos + 1).is_ascii_digit()
    }

    /// `FUN_00451b7a` (atoi): white space, a sign, digits; wraps.
    pub fn atoi(&self, mut pos: u32) -> i32 {
        while matches!(self.at(pos), 9..=13 | b' ') {
            pos += 1;
        }
        let sign = self.at(pos);
        if sign == b'+' || sign == b'-' {
            pos += 1;
        }
        let mut n = 0u32;
        while self.at(pos).is_ascii_digit() {
            n = n
                .wrapping_mul(10)
                .wrapping_add(u32::from(self.at(pos) - b'0'));
            pos += 1;
        }
        (if sign == b'-' { n.wrapping_neg() } else { n }) as i32
    }

    /// The first CR or LF at or after `pos`.
    pub fn line_end(&self, mut pos: u32) -> u32 {
        while pos < self.len() && self.at(pos) != b'\r' && self.at(pos) != b'\n' {
            pos += 1;
        }
        pos
    }

    pub fn line_start(&self, mut pos: u32) -> u32 {
        while pos > 0 && self.at(pos - 1) != b'\r' && self.at(pos - 1) != b'\n' {
            pos -= 1;
        }
        pos
    }

    /// The bytes from `pos` to the end of the line.
    /// From `pos` up to (not including) the next CR, the stop of the
    /// executor's copy loops.
    pub fn up_to_cr(&self, pos: u32) -> &[u8] {
        let mut end = pos;
        while end < self.len() && self.at(end) != b'\r' {
            end += 1;
        }
        let start = (pos as usize).min(end as usize);
        &self.0[start..end as usize]
    }

    pub fn line(&self, pos: u32) -> &[u8] {
        let end = self.line_end(pos) as usize;
        let start = (pos as usize).min(end);
        &self.0[start..end]
    }
}

/// One char per byte.
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// The bytes of a [`latin1`] string.
pub fn latin1_bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

/// Trailing spaces, tabs and carriage returns removed.
fn rtrim(s: &[u8]) -> &[u8] {
    let mut n = s.len();
    while n > 0 && matches!(s[n - 1], b' ' | b'\t' | b'\r') {
        n -= 1;
    }
    &s[..n]
}

/// C's atoi on a byte string (`FUN_00451b7a`).
fn atoi_bytes(s: &[u8]) -> i32 {
    ConText(s).atoi(0)
}

/// `[Speaker,params] text` (the line trimmed at its end).
fn parse_dialog_line(line: &[u8]) -> ConLine {
    let mut close = 1;
    while close < line.len() && line[close] != b']' {
        close += 1;
    }
    let block = &line[1.min(line.len())..close.max(1).min(line.len())];
    let text: &[u8] = if close + 1 < line.len() {
        if line[close + 1] == b' ' {
            &line[close + 2..]
        } else {
            &line[close + 1..]
        }
    } else {
        &[]
    };
    let mut cl = ConLine {
        text: latin1(text),
        ..ConLine::default()
    };
    match block.iter().position(|&c| c == b',') {
        None => cl.speaker = latin1(block),
        Some(comma) => {
            cl.speaker = latin1(&block[..comma]);
            cl.params = latin1(&block[comma + 1..]);
        }
    }
    // The header angles, FUN_0042844c @ 0x0042852b..0x004285fa: the first
    // ',' and the first ']' from the '[' (strchr); when that ',' comes
    // before the ']' and the byte right after it is a digit, X = atoi there
    // (no blank or sign skipped before the digit). Then the next ',': when
    // it also comes before the ']' and is followed by a digit, Y = atoi
    // there, else Y = X (0x004285ee). The original's strchr runs on past
    // the line; the port looks in the line only (every shipped header has
    // its ']' on its line).
    let digit_after = |c: usize| line.get(c + 1).is_some_and(u8::is_ascii_digit);
    if let Some(c1) = line.iter().position(|&c| c == b',') {
        if c1 < close && digit_after(c1) {
            let x = atoi_bytes(&line[c1 + 1..]);
            let y = match line[c1 + 1..].iter().position(|&c| c == b',') {
                Some(d) if c1 + 1 + d < close && digit_after(c1 + 1 + d) => {
                    atoi_bytes(&line[c1 + 2 + d..])
                }
                _ => x,
            };
            cl.angle = Some((x, y));
        }
    }
    cl
}

/// What [`scan_command`] found.
enum Next {
    /// Past the CR after a /set.
    At(u32),
    /// A /go (the section's `goto_id` is set).
    Stop,
    /// The line has neither.
    None,
}

/// "/set(" + command + optional ")" on the rest of the line.
fn set_item(t: ConText, pos: u32) -> ConItem {
    let s = rtrim(t.line(pos));
    let mut cmd: &[u8] = if s.len() > 5 { &s[5..] } else { &[] };
    if cmd.last() == Some(&b')') {
        cmd = &cmd[..cmd.len() - 1];
    }
    ConItem::Set {
        cmd: latin1(cmd),
        offset: pos,
    }
}

/// `FUN_0042831c`: from `pos` to the end of the line, run the first /Set,
/// /set (`FUN_0042a100`) or /Go, /go (`FUN_00425c04`).
fn scan_command(t: ConText, mut pos: u32, sec: &mut ConSection) -> Next {
    while pos < t.len() {
        if t.matches(pos, b"/Set") || t.matches(pos, b"/set") {
            sec.items.push(set_item(t, pos));
            return Next::At(t.line_end(pos) + 1);
        }
        if t.matches(pos, b"/Go") || t.matches(pos, b"/go") {
            sec.goto_id = t.atoi(pos + 3);
            return Next::Stop;
        }
        if t.at(pos) == b'#' {
            return Next::None;
        }
        pos += 1;
        if t.at(pos) == b'\r' {
            return Next::None;
        }
    }
    Next::None
}

/// `FUN_0042844c`, the section executor, as a walk: the dialog lines and
/// the /set commands it runs from `pos`, up to a /go or the next "/digit".
fn read_body(t: ConText, mut pos: u32, sec: &mut ConSection) {
    while pos < t.len() {
        if t.at(pos) == b'[' {
            let line = t.line(pos);
            let mut l = parse_dialog_line(rtrim(line));
            if !l.text.is_empty() {
                l.trail = latin1(&line[rtrim(line).len()..]);
            }
            l.raw = latin1(t.up_to_cr(pos));
            sec.items.push(ConItem::Line(l));
            // The dialog code copies the text up to the CR (0x00428785).
            while pos < t.len() && t.at(pos) != b'\r' {
                pos += 1;
            }
        }
        match scan_command(t, pos, sec) {
            Next::Stop => return,
            Next::At(next) => pos = next,
            Next::None => {}
        }
        if t.at(pos) == b'#' {
            while pos < t.len() {
                let c = t.at(pos);
                pos += 1;
                if c == b'\n' {
                    break;
                }
            }
        }
        pos += 1;
        if t.is_header(pos) {
            return;
        }
    }
}

/// `FUN_00425828` @ 0x00425ad3..0x00425af9: a /con entry has a subject
/// gate when the 4 bytes at its body start are "/Suj" or "/suj"
/// (`FUN_0044a83f`, no case folding, no digit needed); a /suj line further
/// down the body is plain text that `FUN_0042831c` skips. The list is what
/// `FUN_004264a8` reads from 4 bytes on (0x004264e5..0x00426575): v =
/// atoi(p), then p += 2, + 1 when v > 9, + 1 when v > 99, until the byte at
/// p or the one before it is a CR. The port stops at the end of the text.
fn read_subject_gate(t: ConText, sec: &mut ConSection) {
    let b = sec.body_offset;
    if !t.matches(b, b"/Suj") && !t.matches(b, b"/suj") {
        return;
    }
    let mut p = b + 4;
    loop {
        let v = t.atoi(p);
        sec.suj_ids.push(v);
        p += 2;
        if v > 9 {
            p += 1;
        }
        if v > 99 {
            p += 1;
        }
        if t.at(p) == b'\r' || t.at(p - 1) == b'\r' || p >= t.len() {
            break;
        }
    }
}

/// `FUN_00425c04`: the first "/N" of the text.
fn find_header(t: ConText, id: i32) -> Option<u32> {
    (0..t.len()).find(|&pos| t.is_header(pos) && t.atoi(pos + 1) == id)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Sel,
    Con,
    Tim1,
    Tim2,
}

struct Entry {
    eval_offset: u32,
    id: i32,
    kind: EntryKind,
    con_limit: i32,
    repeatable: bool,
}

/// `FUN_00427fa8`: the section number is the last "/digit" the walk
/// passed.
fn track_section(t: ConText, pos: u32, id: &mut i32) {
    if t.is_header(pos) {
        *id = t.atoi(pos + 1);
    }
}

/// '#' skips to after the CR, then one more byte (the LF).
fn skip_comment(t: ConText, pos: &mut u32) {
    if t.at(*pos) == b'#' {
        while *pos < t.len() {
            let c = t.at(*pos);
            *pos += 1;
            if c == b'\r' {
                break;
            }
        }
        *pos += 1;
    }
}

/// The scanners `FUN_004280f8` (/sel) and `FUN_004281f8` (/con), walked
/// with an evaluator that always fails and no play counter, so that every
/// entry they can reach is listed in the order they meet it.
fn scan_sel_con(t: ConText, con: bool, out: &mut Vec<Entry>) {
    let mut id = 0;
    let mut pos = 0;
    while pos < t.len() {
        track_section(t, pos, &mut id);
        if !con && (t.matches(pos, b"/Sel") || t.matches(pos, b"/sel")) {
            out.push(Entry {
                eval_offset: pos,
                id,
                kind: EntryKind::Sel,
                con_limit: 1,
                repeatable: false,
            });
            pos += 1;
        } else if con && (t.matches(pos, b"/Con") || t.matches(pos, b"/con")) {
            pos += 4;
            let mut limit = t.atoi(pos);
            if limit == 0 {
                limit = 1;
            }
            out.push(Entry {
                eval_offset: pos,
                id,
                kind: EntryKind::Con,
                con_limit: limit,
                repeatable: t.at(pos) == b'*',
            });
            pos += 1;
        }
        skip_comment(t, &mut pos);
        pos += 1;
    }
}

/// `FUN_0042bef4` (/tim1, /Tim1) and `FUN_0042bfa4` (/tim2, /Tim2).
fn scan_timer(t: ConText, n: i32, out: &mut Vec<Entry>) {
    let (lower, upper): (&[u8], &[u8]) = if n == 1 {
        (b"/tim1", b"/Tim1")
    } else {
        (b"/tim2", b"/Tim2")
    };
    let mut id = 0;
    let mut pos = 0;
    while pos < t.len() {
        track_section(t, pos, &mut id);
        if t.matches(pos, lower) || t.matches(pos, upper) {
            pos += 5;
            out.push(Entry {
                eval_offset: pos,
                id,
                kind: if n == 1 {
                    EntryKind::Tim1
                } else {
                    EntryKind::Tim2
                },
                con_limit: 1,
                repeatable: false,
            });
            pos += 1;
        }
        pos += 1;
    }
}

/// A CON file: its text, the commands of its /INIT block and its entry
/// points.
#[derive(Clone, Debug, Default)]
pub struct ConScript {
    text: Vec<u8>,
    sections: Vec<ConSection>,
    init_cmds: Vec<(String, u32)>,
}

impl ConScript {
    pub fn parse(buf: &[u8]) -> ConScript {
        let mut s = ConScript {
            text: buf.to_vec(),
            ..ConScript::default()
        };
        let t = ConText(&s.text);

        // FUN_00425eb0: after "/INIT" + CRLF, run the commands line by line
        // until a line has none.
        let mut pos = 0;
        while pos < t.len() {
            if t.matches(pos, b"/INIT") {
                let mut at = Some(pos + 7);
                let mut guard = 0;
                while let Some(p) = at {
                    if guard >= t.len() {
                        break;
                    }
                    guard += 1;
                    let mut init = ConSection::default();
                    let next = scan_command(t, p, &mut init);
                    if let Some(ConItem::Set { cmd, offset }) = init.items.first() {
                        s.init_cmds.push((cmd.clone(), *offset));
                    }
                    at = match next {
                        Next::At(n) => Some(n),
                        Next::Stop => find_header(t, init.goto_id).filter(|&p| p > 0),
                        Next::None => None,
                    };
                }
                break;
            }
            skip_comment(t, &mut pos);
            pos += 1;
        }

        let mut entries = Vec::new();
        scan_sel_con(t, false, &mut entries);
        scan_sel_con(t, true, &mut entries);
        scan_timer(t, 1, &mut entries);
        scan_timer(t, 2, &mut entries);
        entries.sort_by_key(|e| e.eval_offset);

        // Sections with no entry point, reachable only through /go: every
        // "/N" at the start of a line whose number no entry carries.
        let mut headers = Vec::new();
        let mut pos = 0;
        while pos < t.len() {
            if t.is_header(pos) {
                let id = t.atoi(pos + 1);
                if id != 0 && !entries.iter().any(|e| e.id == id) {
                    headers.push((pos, id));
                }
            }
            pos = t.line_end(pos) + 1;
        }

        let (mut e, mut h) = (0, 0);
        while e < entries.len() || h < headers.len() {
            let mut sec = ConSection::default();
            if h >= headers.len() || (e < entries.len() && entries[e].eval_offset < headers[h].0) {
                let en = &entries[e];
                e += 1;
                sec.id = en.id;
                sec.is_entry = true;
                sec.is_con = en.kind == EntryKind::Con;
                sec.repeatable = en.repeatable;
                sec.con_limit = en.con_limit;
                sec.is_timer = matches!(en.kind, EntryKind::Tim1 | EntryKind::Tim2);
                sec.timer_num = match en.kind {
                    EntryKind::Tim1 => 1,
                    EntryKind::Tim2 => 2,
                    _ => 0,
                };
                sec.cond_offset = Some(en.eval_offset);
                sec.line_offset = t.line_start(en.eval_offset);
                // The evaluator stops at the end of the line; the body starts
                // two bytes later (callers of FUN_004298ac add 2).
                sec.body_offset = t.line_end(en.eval_offset) + 2;
                read_body(t, sec.body_offset, &mut sec);
                if sec.is_con {
                    read_subject_gate(t, &mut sec);
                }
            } else {
                let (offset, id) = headers[h];
                h += 1;
                sec.id = id;
                sec.line_offset = offset;
                sec.body_offset = offset;
                read_body(t, offset, &mut sec);
            }
            s.sections.push(sec);
        }

        s
    }

    /// The file as loaded: the condition evaluator reads the condition
    /// lines from it, as the original does.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    pub fn sections(&self) -> &[ConSection] {
        &self.sections
    }

    /// The /INIT commands and the offsets of their "/set".
    pub fn init_cmds(&self) -> &[(String, u32)] {
        &self.init_cmds
    }

    /// `FUN_00425c04` (/goN): the section that starts at the first "/N" of
    /// the text, with the items the executor runs from there.
    pub fn goto_section(&self, id: i32) -> Option<ConSection> {
        let t = ConText(&self.text);
        let pos = find_header(t, id)?;
        let mut out = ConSection {
            id,
            line_offset: pos,
            body_offset: pos,
            ..ConSection::default()
        };
        read_body(t, pos, &mut out);
        Some(out)
    }

    /// `FUN_00425cf4`'s choice after a zone click or with an object in
    /// hand: the index of the first /con entry the play counters allow,
    /// whose condition holds and whose body does not start with /Suj or
    /// /suj (0x00425d47..0x00425d6d: such entries are skipped).
    pub fn find_con_click_entry(
        &self,
        counters: &[u8],
        mut test: impl FnMut(&ConSection) -> bool,
    ) -> Option<usize> {
        let t = ConText(&self.text);
        self.sections.iter().position(|sec| {
            sec.is_entry
                && sec.is_con
                && con_entry_allowed(sec, counters)
                && test(sec)
                && !(t.matches(sec.body_offset, b"/Suj") || t.matches(sec.body_offset, b"/suj"))
        })
    }

    /// `FUN_004277c8` (the /sel scan, through the scanner `FUN_004280f8`):
    /// the index of the first /sel entry, in file order, whose condition
    /// holds.
    pub fn find_sel_entry(&self, mut test: impl FnMut(&ConSection) -> bool) -> Option<usize> {
        self.sections
            .iter()
            .position(|sec| sec.is_entry && !sec.is_con && !sec.is_timer && test(sec))
    }
}
