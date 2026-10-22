//! Which lines run: the walks over the chapter text.
//!
//! - `CtrlScenarIN` (@ 0x00483c90) starts after the first `/FININIT` and
//!   looks for `/con` byte by byte (`FUN_00484290`). The first `/con` after
//!   a section header is the section's head: false, the walk jumps to the
//!   next header; true and followed by `/set`, its body runs and the scan
//!   is over; true and followed by anything else, the `/con` lines after it
//!   are sub-conditions, the first true one runs its body. One body per
//!   scan. Section numbers have no effect.
//! - The subject scan (@ 0x0047fde0) is `CtrlScenarIN` that goes on to
//!   the next true entry while a body ends at a section header.
//! - `RunSection` (@ 0x00483f90) runs a body line by line up to the next
//!   `/con` or header; `RunLine` (@ 0x00484570) runs the first `/set` of a
//!   line, and after a `#` looks at the next line: so a `#/set(...)` line
//!   followed by an empty line is run (the section steps one byte and finds
//!   the `/set`), followed by a non-empty one it is not.
//! - `RunInit` (@ 0x00484040) walks `/INIT` .. `/FININIT` byte by byte,
//!   running each `/set` it lands on; a true `/con` there runs its section,
//!   a false one jumps to the next header. Once per chapter load.
//! - The timer scans (@ 0x00483de0) run the first `/tim1` (`/tim2`) of the
//!   whole text whose condition holds, while `TESTTIME1` (`TESTTIME2`) is
//!   set.

use super::{Host, Script, Src, CHAPTER_BUFFER, CR, LF};

const S: Src = Src::Chapter;

impl Script {
    /// `RunLine` (@ 0x00484570): the position after the `/set` line it ran,
    /// `None` when it ran nothing or the section must wait.
    pub fn run_line(&mut self, host: &mut dyn Host, mut p: usize) -> Option<usize> {
        loop {
            if self.at(S, p, b"/set") {
                let r = self.set_variables(host, S, p);
                return if r.is_some() && !self.resume_pending {
                    r
                } else {
                    None
                };
            }
            if self.at(S, p, b"/go") {
                return self.go(p);
            }
            if self.b(S, p) == b'#' {
                p = self.after_cr(S, p);
            }
            p += 1;
            if self.b(S, p) == CR || p >= self.len(S) {
                return None;
            }
        }
    }

    /// `RunSection` (@ 0x00483f90): runs lines from `p` up to the next
    /// `/con` or section header, or until a command makes it wait. The last
    /// `RunLine` result, or `None` at a `/con`.
    pub fn run_section(&mut self, host: &mut dyn Host, mut p: usize) -> Option<usize> {
        loop {
            let r = self.run_line(host, p);
            let q = r.unwrap_or(p);
            if self.resume_pending {
                return r;
            }
            if self.fatal.is_some() {
                return None;
            }
            p = q + 1;
            if self.at(S, p, b"/con") {
                return None;
            }
            if self.is_header(S, p) {
                return r;
            }
            if p >= self.len(S) {
                return None;
            }
        }
    }

    /// The `/go` helper (@ 0x004841f0), a leftover no script uses: the
    /// position of the header of section N.
    fn go(&mut self, p: usize) -> Option<usize> {
        let n = self.atoi_at(S, p + 3);
        self.section_number = n;
        (0..self.chapter_size).find(|&s| self.is_header(S, s) && self.atoi_at(S, s + 1) == n)
    }

    /// `CtrlScenarIN` (@ 0x00483c90; `CtrlScenarOUT` @ 0x00483cc0 is the
    /// same): runs the first true entry's body. `Some(0)` (the text's
    /// start) when no entry holds.
    pub fn ctrl_scenar_in(&mut self, host: &mut dyn Host) -> Option<usize> {
        log::debug!("# CtrlScenarIN #");
        if self.fatal.is_some() {
            return None;
        }
        let start = self.after_fininit();
        match self.find_true_con(host, start) {
            Some(body) => self.run_section(host, body),
            None => Some(0),
        }
    }

    /// The subject scan (@ 0x0047fde0), run when the subject menu answers
    /// a subject: from the start of the scans (after `/FININIT`) the body
    /// of the first true entry runs; while a body ends at a section header
    /// (`RunSection` gives a position), the next true entry from that
    /// body's start runs too. A body that ends at a `/con`, or that waits
    /// (a `dial=`), ends the scan. Unlike `CtrlScenarIN` it logs nothing.
    pub fn subject_scan(&mut self, host: &mut dyn Host) {
        if self.fatal.is_some() {
            return;
        }
        let start = self.after_fininit();
        let mut found = self.find_true_con(host, start);
        while let Some(body) = found {
            if self.run_section(host, body).is_none() {
                break;
            }
            found = self.find_true_con(host, body);
        }
    }

    /// `FUN_00483be0`: the position after the CR of the first line that
    /// starts with `/FININIT` (any case); the text's start without one.
    fn after_fininit(&self) -> usize {
        let mut p = 0;
        if self.b(S, 0) != 0 {
            while p < CHAPTER_BUFFER {
                if self.at_ci(S, p, b"/FININIT") {
                    break;
                }
                p = self.next_line(p);
                if self.b(S, p) == 0 {
                    break;
                }
            }
        }
        if self.b(S, p) != 0 && p < CHAPTER_BUFFER {
            return self.after_cr(S, p);
        }
        0
    }

    /// `FUN_00484760`: the start of the next line.
    fn next_line(&self, mut p: usize) -> usize {
        while !matches!(self.b(S, p), 0 | CR | LF) && p < CHAPTER_BUFFER {
            p += 1;
        }
        if self.b(S, p) == CR {
            p += 1;
        }
        if self.b(S, p) == LF {
            p += 1;
        }
        p
    }

    /// `FUN_00484550`: the byte before the next section header after `p`.
    fn before_header(&self, p: usize) -> usize {
        let mut i = p;
        while !self.is_header(S, i + 1) && i < self.len(S) {
            i += 1;
        }
        i
    }

    /// Notes the number of a header at `p` (`FUN_00484010`).
    fn note_header(&mut self, p: usize) -> bool {
        let h = self.is_header(S, p);
        if h {
            self.section_number = self.atoi_at(S, p + 1);
        }
        h
    }

    /// `FUN_00484290`: the body of the first true entry from `p`.
    fn find_true_con(&mut self, host: &mut dyn Host, mut p: usize) -> Option<usize> {
        let mut head = true;
        loop {
            if self.fatal.is_some() {
                return None;
            }
            if self.note_header(p) {
                head = true;
            }
            let mut u = p;
            if self.at(S, p, b"/con") {
                let cond = p + 4;
                match self.eval_condition(host, cond) {
                    None => {
                        u = if head {
                            self.before_header(cond)
                        } else {
                            p + 5
                        }
                    }
                    Some(r) => {
                        if !head || self.at(S, r + 2, b"/set") {
                            return Some(r + 2);
                        }
                        head = false;
                        u = r;
                    }
                }
            }
            p = u + 1;
            if p >= self.chapter_size {
                return None;
            }
        }
    }

    /// `RunInit` (@ 0x00484040): the `/INIT` block of the current chapter.
    /// Inside a running one (a `/set(chapter=N)` of the block) it returns
    /// at once and the running walk restarts on the new chapter's block.
    pub fn run_init(&mut self, host: &mut dyn Host) -> bool {
        if self.init_running {
            return true;
        }
        self.init_running = true;
        let done = self.init_walk(host);
        self.init_running = false;
        done
    }

    fn init_walk(&mut self, host: &mut dyn Host) -> bool {
        let Some(init) = self.find_init() else {
            self.fail("RunInit: no /INIT".into());
            return false;
        };
        let mut i2 = self.search_set_cool(host, init + 7);
        loop {
            if self.resume_pending {
                return true;
            }
            if self.fatal.is_some() {
                return false;
            }
            if self.chapter_reload {
                self.chapter_reload = false;
                let Some(init) = self.find_init() else {
                    self.fail("RunInit: no /INIT".into());
                    return false;
                };
                i2 = self.search_set_cool(host, init + 7);
            }
            let mut q = self.init_con(host, i2);
            if !self.init_skip {
                self.run_section(host, q);
                if self.resume_pending {
                    return true;
                }
                q += 1;
            } else {
                self.init_skip = false;
            }
            if self.b(S, q) == b'#' {
                q = self.after_cr(S, q);
            }
            if self.at(S, q, b"/FININIT") {
                return true;
            }
            if q > 0 && self.at(S, q - 1, b"/FININIT") {
                return true;
            }
            if q >= self.len(S) {
                self.fail("RunInit: no /FININIT".into());
                return false;
            }
            i2 = self.search_set_cool(host, q);
        }
    }

    /// The first `/INIT` of the text.
    fn find_init(&self) -> Option<usize> {
        (0..self.len(S)).find(|&p| self.at(S, p, b"/INIT"))
    }

    /// `SearchSetCool` (@ 0x00484630): runs the line at `p` if it starts
    /// with `/set`; the position after it, or `p`.
    fn search_set_cool(&mut self, host: &mut dyn Host, p: usize) -> usize {
        if !self.at(S, p, b"/set") {
            return p;
        }
        match self.set_variables(host, S, p) {
            Some(r) => r,
            None => {
                let line = self.line_text(S, p);
                self.fail(format!("SearchSetCool : /set impossible : {line}"));
                p
            }
        }
    }

    /// `FUN_00484450`: a `/con` met by the `/INIT` walk. The start of the
    /// body to run, or (with the skip flag set) where the walk goes on.
    fn init_con(&mut self, host: &mut dyn Host, mut p: usize) -> usize {
        let mut head = true;
        self.note_header(p);
        if self.at(S, p, b"/con") {
            loop {
                let mut i2 = p;
                if self.at(S, p, b"/con") {
                    let cond = p + 4;
                    match self.eval_condition(host, cond) {
                        None => {
                            if head {
                                let x = self.before_header(cond);
                                self.init_skip = true;
                                return x.saturating_sub(1);
                            }
                            i2 = p + 5;
                        }
                        Some(r) => {
                            if !head || self.at(S, r + 2, b"/set") {
                                return r + 2;
                            }
                            head = false;
                            i2 = r;
                        }
                    }
                }
                p = i2 + 1;
                if self.is_header(S, p) || p >= self.len(S) || self.fatal.is_some() {
                    break;
                }
            }
        }
        self.init_skip = true;
        p + 1
    }

    /// The timer scans (@ 0x00483de0).
    pub fn timer_scans(&mut self, host: &mut dyn Host) {
        if self.vars[self.keys.testtime1] != 0 {
            if let Some(body) = self.tim_scan(host, b"/tim1", b"/Tim1") {
                self.run_section(host, body);
            }
        }
        if self.vars[self.keys.testtime2] != 0 {
            if let Some(body) = self.tim_scan(host, b"/tim2", b"/Tim2") {
                self.run_section(host, body);
            }
        }
    }

    /// `FUN_00483e70` / `FUN_00483f00`: the body of the first true timer
    /// entry of the whole text.
    fn tim_scan(&mut self, host: &mut dyn Host, a: &[u8], b: &[u8]) -> Option<usize> {
        let mut p = 0;
        loop {
            if self.fatal.is_some() {
                return None;
            }
            self.note_header(p);
            if self.at(S, p, a) || self.at(S, p, b) {
                if let Some(r) = self.eval_condition(host, p + 5) {
                    return Some(r + 2);
                }
                p += 6;
            }
            if self.b(S, p) == b'#' {
                p = self.after_cr(S, p);
            }
            p += 1;
            if p >= self.chapter_size {
                return None;
            }
        }
    }

    /// `RunOneCommand` (@ 0x00481800): a `(KEY=value)` command of the
    /// engine through the `/set` handler, with no resume point.
    pub fn run_command(&mut self, host: &mut dyn Host, command: &[u8]) {
        let mut t = if command.len() < 0xfe {
            command.to_vec()
        } else {
            let t = String::from_utf8_lossy(command).into_owned();
            self.warn("RunOneCommand(%s) : commande overflow", &t);
            command[..0xfd].to_vec()
        };
        t.extend_from_slice(b"\r\n\r\n");
        self.command = t;
        self.in_command = true;
        self.set_variables(host, Src::Command, 0);
        self.in_command = false;
    }

    /// `RunOneCommand_AfterDisplay` (@ 0x00481790): a command for the next
    /// warp frame.
    pub fn queue_command(&mut self, command: &[u8]) {
        if command.len() > 0xfd {
            let t = String::from_utf8_lossy(command).into_owned();
            self.warn("RunOneCommand_AfterDisplay(%s) : commande overflow", &t);
            self.queued = Some(command[..0xfd].to_vec());
        } else {
            self.queued = Some(command.to_vec());
        }
    }
}
