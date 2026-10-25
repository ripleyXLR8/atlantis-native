//! Conditions: `(key op value)` terms joined by `&` and `|`
//! (`EvalCondition` @ 0x00480fd0, `CondValue` @ 0x00481300).
//!
//! - The key is the bytes after `(` up to a stop byte `= < > ! # , + -`
//!   (`FUN_00481290`), a-z upper-cased (`FUN_004812d0`), looked up in
//!   `#VARIABLES#`; 64 bytes or more is an error and the condition false.
//!   `FLAG` and a digit test `flags[nnn]`, copied first into the variable
//!   `FLAG`.
//! - The operators `=`, `<`, `>`, `!` compare `vars[key]`, read after the
//!   value was resolved, with the value as signed 32-bit numbers
//!   (`FUN_004815c0`, `FUN_004815f0`, `FUN_00481620`, `FUN_00481650`); any
//!   other stop byte leaves the term out.
//! - The byte after a term decides its group (`FUN_00481240`, which looks
//!   ahead from the key for the next `(`, `|`, `&` or CR): `|` makes an OR
//!   group with the next term, `&` or the end of the line ends the group,
//!   `)(` keeps the previous mode. A true term of an OR group jumps past
//!   the next `&` (`FUN_00481270`), or makes the whole condition true; a
//!   false term outside an OR group makes it false. So `|` binds tighter
//!   than `&`.
//! - The result is the position of the line's CR (true) or `None`.

use super::{Host, Script, Src, CR, LF};
use atlantis2_data::defnames;

/// `FUN_00481290`: true while the byte is part of a key.
pub(crate) fn is_key_byte(c: u8) -> bool {
    !matches!(c, b'=' | b'<' | b'>' | b'!' | b'#' | b',' | b'+' | b'-')
}

impl Script {
    /// `EvalCondition` (@ 0x00480fd0) on the chapter text at `p` (the byte
    /// after `/con`, `/tim1`...): the position of the CR that ends a true
    /// condition, `None` for a false one.
    pub fn eval_condition(&mut self, host: &mut dyn Host, mut p: usize) -> Option<usize> {
        const SRC: Src = Src::Chapter;
        let size = self.chapter_size;
        if size < 1 {
            return None;
        }
        let mut or_mode = false;
        let mut count = 0usize;
        loop {
            if self.fatal.is_some() {
                return None;
            }
            let c = self.b(SRC, p);
            if c == CR || c == LF {
                return Some(p);
            }
            p += 1;
            if c == b'(' {
                or_mode = self.group_mode(p, or_mode);
                // The key, up to the byte before a stop byte.
                let mut key = Vec::with_capacity(16);
                let mut last;
                loop {
                    key.push(self.b(SRC, p));
                    if key.len() > 63 {
                        log::warn!("script: Nom de Variable Trop long");
                        return None;
                    }
                    last = p;
                    p += 1;
                    if !is_key_byte(self.b(SRC, p)) {
                        break;
                    }
                }
                for c in &mut key {
                    if c.is_ascii_lowercase() {
                        *c -= 0x20;
                    }
                }
                let k = if key.starts_with(b"FLAG") && key.get(4).is_some_and(u8::is_ascii_digit) {
                    self.keys.flag
                } else {
                    match self.names.id(defnames::VARIABLES, &key) {
                        Ok(k) => k,
                        Err(e) => {
                            self.fail(e.to_string());
                            return None;
                        }
                    }
                };
                // The operator at `p`; each test may move on to the term
                // after the next `&` (true term of an OR group).
                let mut q = p;
                for op in *b"=<>!" {
                    if self.b(SRC, q) != op {
                        continue;
                    }
                    // `=` reads it 2 bytes after the key's last byte: the same place.
                    debug_assert!(op != b'=' || last + 1 == q);
                    let value = q + 1;
                    let v = self.cond_value(host, value, k);
                    let var = self.vars[k];
                    let holds = match op {
                        b'=' => var == v,
                        b'<' => var < v,
                        b'>' => var > v,
                        _ => var != v,
                    };
                    if self.fatal.is_some() {
                        return None;
                    }
                    if !holds {
                        if !or_mode {
                            return None;
                        }
                    } else if or_mode {
                        match self.after_amp(q) {
                            Some(next) => q = next,
                            None => {
                                // No `&` left: true, at the line's CR.
                                let mut e = value;
                                while self.b(SRC, e) != CR {
                                    e += 1;
                                    if e >= self.len(SRC) {
                                        return None;
                                    }
                                }
                                return Some(e);
                            }
                        }
                    }
                }
                p = q;
            }
            count += 1;
            if count >= size {
                return None;
            }
        }
    }

    /// `FUN_00481240`: the group mode of the term whose key starts at `p`.
    fn group_mode(&self, mut p: usize, current: bool) -> bool {
        loop {
            match self.b(Src::Chapter, p) {
                b'(' => return current,
                b'|' => return true,
                b'&' | CR => return false,
                0 if p >= self.len(Src::Chapter) => return false,
                _ => p += 1,
            }
        }
    }

    /// `FUN_00481270`: the position after the next `&` of the line.
    fn after_amp(&self, mut p: usize) -> Option<usize> {
        loop {
            let c = self.b(Src::Chapter, p);
            if c == CR || p >= self.len(Src::Chapter) {
                return None;
            }
            p += 1;
            if c == b'&' {
                return Some(p);
            }
        }
    }

    /// `CondValue` (@ 0x00481300): the number a term compares
    /// `vars[key]` with, from the value at `v`.
    pub(crate) fn cond_value(&mut self, host: &mut dyn Host, v: usize, key: usize) -> i32 {
        const SRC: Src = Src::Chapter;
        let k = &self.keys;
        if key == k.flag {
            // FLAGnnn: flags[nnn] into vars[FLAG], nnn after "(FLAG".
            let r = self.atoi_at(SRC, v);
            let mut open = v;
            while open > 0 && self.b(SRC, open) != b'(' {
                open -= 1;
            }
            let n = self.atoi_at(SRC, open + 5);
            self.vars[key] = usize::try_from(n)
                .ok()
                .and_then(|n| self.flags.get(n).copied())
                .unwrap_or(0);
            return r;
        }
        // Blanks and tabs (FUN_004847a0).
        let mut s = v;
        while matches!(self.b(SRC, s), b' ' | b'\t') && s < super::CHAPTER_BUFFER {
            s += 1;
        }
        if self.b(SRC, s).is_ascii_digit() {
            return self.atoi_at(SRC, s);
        }
        if key == k.numframeobjet || key == k.numframeperso {
            // NAME,n: NAME's current frame into the key's variable (0 for
            // an object without a record, −1 for a character), then n.
            let (section, none) = if key == k.numframeobjet {
                (defnames::OBJETS, 0)
            } else {
                (defnames::PERSONNAGES, -1)
            };
            let id = self.id(section, SRC, s);
            self.vars[key] = host.anim_frame(id).unwrap_or(none);
            let mut c = s;
            while self.b(SRC, c) != b',' && c < self.len(SRC) {
                c += 1;
            }
            return self.atoi_at(SRC, c + 1);
        }
        let section = if key == k.clicperso {
            defnames::PERSONNAGES
        } else if key == k.sujet {
            defnames::SUJETS
        } else if key == k.objetenmain || key == k.clicobjet || key == k.deposeobj {
            defnames::OBJETS
        } else {
            defnames::ZONES_CLIC
        };
        if key != k.sacados {
            if section == defnames::SUJETS {
                // A subject is compared by its parameter, not its index.
                return self.param(defnames::SUJETS, SRC, s);
            }
            return self.id(section, SRC, s);
        }
        // SACADOS: X in the bag, or NOT_X not in it. vars[SACADOS] gets X's
        // sprite when the test passes, and the value is that sprite; −1
        // when it fails.
        let not = self.at_ci(SRC, s, b"NOT_");
        if not {
            s += 4;
        }
        let sprite = self.param(defnames::OBJETS, SRC, s);
        let id = self.id(defnames::OBJETS, SRC, s);
        let held = self.bag.iter().any(|e| e.id == id);
        if held != not {
            self.vars[key] = sprite;
            return sprite;
        }
        if held {
            // In the bag but NOT_X asked: vars[SACADOS] is still set.
            self.vars[key] = sprite;
        }
        -1
    }
}
