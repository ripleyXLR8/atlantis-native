//! The CON variables, the condition evaluator, the /set parser, the subject
//! table, the music lists and the clocks.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`con_eval.cpp`,
//! `con_eval.h`; GPLv3, the ScummVM team).

use super::script::{latin1, ConText};

/// The variable names of `VARIAS.CON`; the line number is the variable
/// index.
#[derive(Clone, Debug, Default)]
pub struct ConVariableNames {
    /// The file upper-cased (`FUN_00426cb0` runs `FUN_00429df0` on it).
    text: Vec<u8>,
    /// One lower-case name per line.
    names: Vec<String>,
}

impl ConVariableNames {
    pub fn load(data: &[u8]) -> ConVariableNames {
        let text = data.iter().map(u8::to_ascii_uppercase).collect();
        let mut names = Vec::new();
        let mut line = Vec::new();
        for &c in data {
            if c == b'\n' {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                names.push(latin1(&line).to_ascii_lowercase());
                line.clear();
            } else {
                line.push(c);
            }
        }
        if !line.is_empty() {
            names.push(latin1(&line).to_ascii_lowercase());
        }
        ConVariableNames { text, names }
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// The lower-case name of a variable.
    pub fn name(&self, index: usize) -> &str {
        &self.names[index]
    }

    /// `FUN_00429d20`: the index of the first line that starts with the
    /// bytes of `key` (strncmp), or `None`.
    pub fn index(&self, key: &[u8]) -> Option<usize> {
        let mut pos = 0;
        let mut index = 0;
        while pos < self.text.len() {
            let mut matched = true;
            for (i, &b) in key.iter().enumerate() {
                let a = self.text.get(pos + i).copied().unwrap_or(0);
                if a != b {
                    matched = false;
                    break;
                }
                if a == 0 {
                    break;
                }
            }
            if matched {
                return Some(index);
            }
            while pos < self.text.len() && self.text[pos] != b'\n' {
                pos += 1;
            }
            pos += 1;
            index += 1;
        }
        None
    }
}

/// A /set command as `FUN_0042a100` reads it from the CON text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConSetCommand {
    /// INVENTnnn=v (`FUN_0042b328`): the object id and the value.
    pub invent: Option<(i32, i32)>,
    /// ChangeWam=name: a file name, no number.
    pub wam_name: Option<String>,
    /// The lower-case VARIAS.CON name of the key.
    pub name: String,
    pub value: i32,
    /// The text offset of the byte that ended the key.
    pub key_end: u32,
    /// The text offset of the first digit of the value.
    pub value_pos: u32,
}

/// `FUN_00429cb0`: false on a key terminator.
fn is_key_char(c: u8) -> bool {
    !b"=<>!#,".contains(&c)
}

/// C's atoi over a fixed buffer from `pos`.
fn atoi_buf(buf: &[u8], pos: usize) -> i32 {
    let mut t = buf.get(pos..).unwrap_or(&[]).to_vec();
    t.push(0);
    ConText(&t).atoi(0)
}

/// `FUN_0042a100`'s parse: from `pos` (the "/set"), skip to '(', copy the
/// key up to one of "=<>!#,", upper-case it; INVENT: id = atoi(key + 6),
/// value = atoi(byte after the terminator); otherwise look the key up by
/// prefix (unknown: `None`, the original shows an error dialog); ChangeWam
/// takes the text up to ')'; any other key takes the first digit run among
/// the 16 bytes from the terminator (none: `None`, the original returns 0
/// and does nothing).
pub fn parse_con_set(names: &ConVariableNames, text: &[u8], pos: u32) -> Option<ConSetCommand> {
    let t = ConText(text);
    let size = t.len();
    let mut p = pos;
    // The head loop is bounded by the size of the CON file, like the
    // evaluator.
    for _ in 0..size {
        let c = t.at(p);
        p += 1;
        if c == b'(' {
            break;
        }
    }
    // The key buffer is 13 zeroed bytes (0x0042a129); longer keys overflow
    // the stack in the original and are cut here.
    let mut key = [0u8; 13];
    let mut len = 0usize;
    loop {
        if len < key.len() {
            key[len] = t.at(p);
        }
        len += 1;
        p += 1;
        if !(is_key_char(t.at(p)) && p < size + 1) {
            break;
        }
    }
    let mut out = ConSetCommand {
        key_end: p,
        ..ConSetCommand::default()
    };
    let klen = len.min(key.len());
    key[..klen].make_ascii_uppercase();

    if &key[..6] == b"INVENT" {
        out.invent = Some((atoi_buf(&key, 6), t.atoi(p + 1)));
        return Some(out);
    }
    let Some(index) = names.index(&key[..klen]) else {
        log::warn!("CON /set: unknown variable '{}'", latin1(&key[..klen]));
        return None;
    };
    out.name = names.name(index).to_string();
    if out.name == "changewam" {
        let mut q = p + 1;
        let mut wam = Vec::new();
        while q < size && t.at(q) != b')' {
            wam.push(t.at(q));
            q += 1;
        }
        out.wam_name = Some(latin1(&wam));
        return Some(out);
    }
    for i in 0..16 {
        if t.at(p + i).is_ascii_digit() {
            out.value = t.atoi(p + i);
            out.value_pos = p + i;
            return Some(out);
        }
    }
    None
}

/// The per-perso subject table (0x00602f10): 0x100 records of 0xe1 bytes,
/// record perso - 0x200, holding the subject ids in the order they were
/// enabled.
#[derive(Clone, Debug)]
pub struct ConSubjects {
    records: Vec<Vec<u8>>,
}

impl Default for ConSubjects {
    fn default() -> Self {
        ConSubjects {
            records: vec![Vec::new(); Self::RECORDS],
        }
    }
}

impl ConSubjects {
    pub const RECORDS: usize = 0x100;

    /// `FUN_00427634`: memset(0x00602f10, 0, 0xe100).
    pub fn clear(&mut self) {
        self.records.iter_mut().for_each(Vec::clear);
    }

    /// The list walk of `FUN_00425f6c` / `FUN_0042606c`: a subject id is a
    /// byte (atoi truncated), the pointer then steps 2, +1 above 9, +1 above
    /// 99 (on the byte), until the byte before it is ')'. The original reads
    /// on past the line when no ')' falls there; this stops at the end of
    /// the text.
    fn walk(t: ConText, mut p: u32, mut f: impl FnMut(u8)) {
        loop {
            let subject = t.atoi(p) as u8;
            f(subject);
            p += 2;
            if subject > 9 {
                p += 1;
            }
            if subject > 99 {
                p += 1;
            }
            if t.at(p - 1) == b')' {
                break;
            }
            if p > t.len() {
                log::warn!("CON EnableSuj / DisableSuj: no ')' ends the subject list");
                break;
            }
        }
    }

    /// `FUN_00425f6c` (EnableSuj), on the CON text from `pos`, the first
    /// digit after the key: record atoi - 0x200; the list starts 4 bytes on
    /// (a 3-digit perso and '=').
    pub fn enable(&mut self, text: &[u8], pos: u32) {
        let t = ConText(text);
        let record = t.atoi(pos).wrapping_sub(0x200);
        let Some(list) = usize::try_from(record)
            .ok()
            .and_then(|r| self.records.get_mut(r))
        else {
            // The original writes outside the table.
            log::warn!(
                "CON EnableSuj: perso {} outside the subject table",
                record + 0x200
            );
            return;
        };
        Self::walk(t, pos + 4, |s| {
            if !list.contains(&s) {
                list.push(s);
            }
        });
    }

    /// `FUN_0042606c` (DisableSuj): record = the low byte of atoi (the same
    /// record as EnableSuj for persos 0x200..0x2ff); the first match is
    /// removed and the entries after it shift down.
    pub fn disable(&mut self, text: &[u8], pos: u32) {
        let t = ConText(text);
        let list = &mut self.records[usize::from(t.atoi(pos) as u8)];
        Self::walk(t, pos + 4, |s| {
            if let Some(k) = list.iter().position(|&x| x == s) {
                list.remove(k);
            }
        });
    }

    /// The enabled subjects of a perso, in order (empty outside the table).
    pub fn subjects(&self, perso: i32) -> &[u8] {
        usize::try_from(perso - 0x200)
            .ok()
            .and_then(|r| self.records.get(r))
            .map_or(&[], Vec::as_slice)
    }

    pub fn has(&self, perso: i32, subject: i32) -> bool {
        self.subjects(perso)
            .iter()
            .any(|&s| i32::from(s) == subject)
    }

    /// For the saves.
    pub fn set_subjects(&mut self, perso: i32, list: &[u8]) {
        if let Some(r) = usize::try_from(perso - 0x200)
            .ok()
            .and_then(|r| self.records.get_mut(r))
        {
            *r = list.to_vec();
        }
    }
}

/// The track list of LoadMusik / DelMusik (`FUN_0041f1d0` /
/// `FUN_0041faa0`) from `pos`, the first digit after the key: track indexes
/// (value - 1, a byte) while the byte before the pointer is not ')' and the
/// index is not 0xff; the pointer steps 2, + 1 above 9, + 1 above 99 on the
/// INDEX, so a value of 10 or 100 desynchronises the walk (CHAPI072).
pub fn con_music_list(text: &[u8], pos: u32) -> Vec<u8> {
    let t = ConText(text);
    let mut out = Vec::new();
    let mut idx = t.atoi(pos).wrapping_sub(1) as u8;
    let mut p = pos;
    while t.at(p.wrapping_sub(1)) != b')' && idx != 0xff && p <= t.len() {
        out.push(idx);
        p += 2;
        if idx > 9 {
            p += 1;
        }
        if idx > 99 {
            p += 1;
        }
        idx = t.atoi(p).wrapping_sub(1) as u8;
    }
    out
}

/// LoadMusik / DelMusik on the set of tracks in the install folder (bit i
/// = track index i): LoadMusik acts only when the first index is below
/// 0x20, skips the playing track (`current` = index + 1, 0x00772915) and
/// copies a track that is not there (its index goes to the result);
/// DelMusik removes every listed track but the playing one.
///
/// With `first_value_rule` (the 1997 build, atlantis-cd-en-688128
/// FUN_00429044 @ 0x00429b12 / 0x00429b85) neither command does anything
/// when its first value is the playing track. That build's DelMusik
/// (FUN_0041ee78) also deletes the playing track's file, which is open
/// then; whether the delete succeeds is not established, so the playing
/// track stays as in the reference.
pub fn con_music_apply(
    load: bool,
    list: &[u8],
    current: i32,
    on_disk: &mut u32,
    first_value_rule: bool,
) -> Vec<u8> {
    let mut copies = Vec::new();
    if list.is_empty() || (load && list[0] >= 0x20) {
        return copies;
    }
    if first_value_rule && i32::from(list[0]) + 1 == current {
        return copies;
    }
    for &idx in list {
        if i32::from(idx) + 1 == current || idx >= 32 {
            continue;
        }
        let bit = 1u32 << idx;
        if load {
            if *on_disk & bit == 0 {
                copies.push(idx);
                *on_disk |= bit;
            }
        } else {
            *on_disk &= !bit;
        }
    }
    copies
}

/// LoadMusik of atlantis-cd-fr-688640 (FUN_0041e830), all or nothing on
/// the first listed track: nothing when its index is 0x20 or more, when its
/// copy is there (FUN_0041ee40) or when it is the open track (`current` =
/// index + 1, 0x007e1f01) (0x0041e862..0x0041e891); otherwise every listed
/// track is copied (FUN_0041e9d0), with no test of its copy or of the open
/// track (0x0041e926..0x0041e96e). So a list whose first track is there
/// copies none of the others, and one whose first track is missing copies
/// the present ones again. The result is the copied indexes, in order; an
/// index past the 32 the set holds is left out (the original copies the
/// name it reads past the table, which names no archive member).
pub fn con_music_load_all_or_nothing(list: &[u8], current: i32, on_disk: &mut u32) -> Vec<u8> {
    let Some(&first) = list.first() else {
        return Vec::new();
    };
    if first >= 0x20 || *on_disk & (1u32 << first) != 0 || i32::from(first) + 1 == current {
        return Vec::new();
    }
    let copies: Vec<u8> = list.iter().copied().filter(|&idx| idx < 32).collect();
    for &idx in &copies {
        *on_disk |= 1u32 << idx;
    }
    copies
}

/// `FUN_0042a100`, AngleWarpX= / AngleWarpY=: the view angle in radians,
/// value * pi / 180 computed on the x87 and stored as a float (pi is the
/// float `FUN_0045179a` keeps at 0x00bcb594).
pub fn con_angle_radians(value: i32) -> f32 {
    let pi = std::f32::consts::PI;
    (f64::from(value) * f64::from(pi) / 180.0) as f32
}

/// The CON clocks: ChapterTime, Time1 and Time2, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConClocks {
    /// 0x006ce078.
    pub chapter_start: u32,
    /// 0x006ce088.
    pub time1_start: u32,
    /// 0x006ce084.
    pub time2_start: u32,
    /// 0x006ce04c.
    pub time1_offset: i32,
    /// 0x006ce050.
    pub time2_offset: i32,
}

impl ConClocks {
    /// `FUN_0041c75c`: (now - start) / 1000, unsigned.
    pub fn chapter_time(&self, now: u32) -> i32 {
        (now.wrapping_sub(self.chapter_start) / 1000) as i32
    }

    /// `FUN_0041c75c`: (now - start) / 1000, unsigned, plus the offset.
    pub fn timer_value(&self, timer: i32, now: u32) -> i32 {
        if timer == 1 {
            self.time1_offset
                .wrapping_add((now.wrapping_sub(self.time1_start) / 1000) as i32)
        } else {
            self.time2_offset
                .wrapping_add((now.wrapping_sub(self.time2_start) / 1000) as i32)
        }
    }

    /// `FUN_0042a100`, Time1= / Time2=: start = now - value s, or, when the
    /// clock is not that far yet, start = now and the value becomes the
    /// offset.
    pub fn restart(&mut self, timer: i32, value: i32, now: u32) {
        let ms = (value as u32).wrapping_mul(1000);
        let (start, offset) = if timer == 1 {
            (&mut self.time1_start, &mut self.time1_offset)
        } else {
            (&mut self.time2_start, &mut self.time2_offset)
        };
        *offset = 0;
        if now > ms {
            *start = now - ms;
        } else {
            *start = now;
            *offset = value;
        }
    }

    /// `FUN_0042c59c` / `FUN_0041c838`: the timers resume from their current
    /// values, ChapterTime starts again from 0.
    pub fn rebase(&mut self, now: u32, time1: i32, time2: i32) {
        self.time1_start = now;
        self.time2_start = now;
        self.chapter_start = now;
        self.time1_offset = time1;
        self.time2_offset = time2;
    }
}

/// The 16-byte key buffer of `FUN_004298ac` (EBP-0x34). The original never
/// clears or terminates it, and within one scan every evaluation runs in
/// the same stack frame, so a short key leaves the tail of an earlier,
/// longer key in place (PARLETEMP1..3 leave a digit after an INVENTnnn
/// key). A scan keeps one buffer across its evaluations.
pub type ConKeyBuffer = [u8; 16];

/// `FUN_00429bf4`: the separator after the current term. 1 for '|', 0 for
/// '&' or the end of the line, unchanged when another '(' comes first.
fn pending_or(t: ConText, mut pos: u32, pending: i32) -> i32 {
    loop {
        let c = t.at(pos);
        if c == b'(' {
            return pending;
        }
        if c == b'|' {
            return 1;
        }
        if c == b'&' {
            return 0;
        }
        pos += 1;
        if c == b'\r' || pos > t.len() + 1 {
            return 0;
        }
    }
}

/// `FUN_00429c64`: the position after the next '&' of the line.
fn skip_to_and(t: ConText, mut pos: u32) -> Option<u32> {
    loop {
        let c = t.at(pos);
        if c == b'\r' {
            return None;
        }
        pos += 1;
        if c == b'&' {
            return Some(pos);
        }
        if pos > t.len() + 1 {
            return None;
        }
    }
}

/// `FUN_00429eb0` / 0x00429f44 / 0x00429fd8 / 0x0042a06c: the value is the
/// first digit run within 16 bytes after the operator; no digit fails the
/// test. The compare is unsigned.
fn compare_term(t: ConText, pos: u32, op: u8, var: u32) -> bool {
    for i in 0..16 {
        if !t.at(pos + i).is_ascii_digit() {
            continue;
        }
        let v = t.atoi(pos + i) as u32;
        return match op {
            b'=' => var == v,
            b'<' => var < v,
            b'>' => var > v,
            _ => var != v,
        };
    }
    false
}

/// Evaluates the condition line of a CON section (/sel, /con, /tim) the way
/// `FUN_004298ac` does: on the script text itself, against the variables
/// named in VARIAS.CON (`value`, by lower-case name) and the inventory
/// records (`inventory`, the object id of each record).
///
/// True when the condition line starting at `text[offset]` holds. The
/// scans may run past the line, as in the original; bytes past the end of
/// the text read as 0. `key`: the scan's buffer, kept between calls; `None`
/// evaluates with a zeroed buffer of its own.
pub fn evaluate(
    names: &ConVariableNames,
    value: &dyn Fn(&str) -> i32,
    inventory: &[i32],
    text: &[u8],
    offset: u32,
    key: Option<&mut ConKeyBuffer>,
) -> bool {
    let t = ConText(text);
    // What the buffer holds at the start of a scan is undefined in the
    // original: zero here, as in the emulation that produced the reference
    // results.
    let mut own = [0u8; 16];
    let key = key.unwrap_or(&mut own);
    let mut pending = 0;
    let mut p = offset;

    // The original bounds the loop by the size of the CON file.
    for _ in 0..t.len() {
        let c = t.at(p);
        if c == b'\r' || c == b'\n' {
            return true;
        }
        p += 1;
        if c != b'(' {
            continue;
        }

        pending = pending_or(t, p, pending);
        let mut len = 0;
        loop {
            key[len] = t.at(p);
            len += 1;
            if len >= 13 {
                return false;
            }
            p += 1;
            if !is_key_char(t.at(p)) {
                break;
            }
        }
        key[..len].make_ascii_uppercase();

        // FUN_00429e54: "INVENT" then atoi over the rest of the buffer.
        let obj_id = if &key[..6] == b"INVENT" {
            atoi_buf(&key[..], 6)
        } else {
            0
        };
        if obj_id == 0 {
            let Some(index) = names.index(&key[..len]) else {
                // The original shows an error dialog here.
                log::warn!("CON condition: unknown variable '{}'", latin1(&key[..len]));
                return false;
            };
            let var = value(names.name(index)) as u32;
            for op in *b"=<>!" {
                if t.at(p) != op {
                    continue;
                }
                if !compare_term(t, p + 1, op, var) {
                    if pending == 0 {
                        return false;
                    }
                } else if pending == 1 {
                    match skip_to_and(t, p) {
                        None => return true,
                        Some(next) => p = next,
                    }
                }
            }
        } else {
            // The operator is ignored: the term holds when a record with this
            // id has the value after the operator (records hold 1), or when
            // there is no such record and that value starts with '0'.
            let mut found = false;
            for &id in inventory {
                if id != obj_id {
                    continue;
                }
                if t.atoi(p + 1) == 1 {
                    found = true;
                    break;
                }
                if pending != 1 {
                    return false;
                }
                pending = 0;
            }
            if !found && t.at(p + 1) != b'0' {
                if pending == 1 {
                    pending = 0;
                } else {
                    return false;
                }
            }
        }
    }
    false
}
