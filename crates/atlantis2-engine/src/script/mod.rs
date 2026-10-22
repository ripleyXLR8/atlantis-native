//! The CON interpreter: the chapter scripts `scenar\chapiNNN.con`, run as
//! the original runs them.
//!
//! The chapter is raw CRLF text in a 64000-byte buffer, never tokenised:
//! every scan walks it byte by byte from its start, and the rules of the
//! dialect (the `#` quirk, sub-conditions, `KEY=++` storing 0, `ARRIVEEVUE`
//! after `vue=`...) are behaviours of those walks. They are transcribed
//! walk by walk, each function after the original one it names:
//!
//! - [`eval`]: the conditions (`EvalCondition` @ 0x00480fd0, `CondValue`
//!   @ 0x00481300);
//! - [`set`]: the `/set(KEY=value)` handler (`SetVariables`
//!   @ 0x004818f0) with the script tables it keeps (subjects, bag,
//!   show/hide table, the object in hand);
//! - [`scan`]: which lines run (`CtrlScenarIN` @ 0x00483c90, `RunSection`
//!   @ 0x00483f90, `RunLine` @ 0x00484570, `RunInit` @ 0x00484040, the
//!   timer scans @ 0x00483de0);
//! - [`frame`]: when they run (the 15 Hz tick @ 0x0040caa0,
//!   `Interp_GereFrame` @ 0x00484b70, the click and departure scans of
//!   the warp frame, the per-view reset @ 0x00483ce0, start-up).
//!
//! Effects outside the script tables go through [`Host`].
//!
//! Atlantis II's dialect shares its text language with the first game's
//! (`/INIT` .. `/FININIT`, `/con` entries, `/tim1` / `/tim2`, `|` binding
//! tighter than `&`), but not the walks that read it: names are looked up
//! per key in `defnames.con` instead of by prefix in `VARIAS.CON`, compares
//! are signed, sections have heads and sub-conditions instead of play
//! counters, unknown names are fatal. So no code is shared with the first
//! game's interpreter.
//!
//! A fatal error of the original (an unknown name, a missing chapter) ends
//! the game there; here it is kept in [`Script::fatal`] and every scan
//! stops, for the host to report.

pub mod eval;
pub mod frame;
pub mod host;
pub mod scan;
pub mod set;
pub mod subjects;

use atlantis2_data::defnames::{self, DefNames, NameError};

use crate::fpu::Fpu;

pub use host::{AnimOp, DialStart, Host};
pub use subjects::{Subject, SubjectTable};

pub(crate) const CR: u8 = b'\r';
pub(crate) const LF: u8 = b'\n';

/// The chapter buffer (@ 0x00483af0: allocated once, 64000 bytes, reused
/// by every chapter; a longer file is fatal).
pub const CHAPTER_BUFFER: usize = 64000;

/// The story flags (`int flags[1024]` @ 0x006d0578).
pub const FLAGS: usize = 1024;

/// Which text a walk reads: the chapter buffer, or the command a
/// `RunOneCommand` (@ 0x00481800) copied (with `"\r\n\r\n"` appended).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Src {
    Chapter,
    Command,
}

/// One object of the bag (@ 0x00671a10, 12 bytes each, count
/// @ 0x00671a08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BagEntry {
    /// The object's sprite in the world's object bank (its `#OBJETS#`
    /// parameter, a byte).
    pub sprite: u8,
    /// Its `#OBJETS#` index.
    pub id: i32,
    /// The world it is shown in (`MONDE`, 6 counted as 2).
    pub world: i32,
}

/// One row of the show/hide table (@ 0x006afdd0, count @ 0x006c2dfc;
/// saved): what the scripts last said of a character or object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub id: i32,
    pub shown: bool,
    /// 1 character, 2 object.
    pub kind: i32,
}

macro_rules! keys {
    ($($field:ident = $name:literal),* $(,)?) => {
        /// The variable index of every key the interpreter names, resolved
        /// once by name as the original resolves it at each use
        /// (`FUN_004812c0` → `DefName_GetIdName(3, KEY)`).
        #[derive(Clone, Debug)]
        #[allow(non_snake_case)]
        pub struct Keys { $(pub $field: usize,)* }

        impl Keys {
            /// The names, as the original spells them.
            pub const NAMES: &'static [&'static [u8]] = &[$($name,)*];

            fn resolve(names: &DefNames) -> Result<Keys, NameError> {
                Ok(Keys { $($field: names.id(defnames::VARIABLES, $name)?,)* })
            }
        }
    };
}

keys! {
    chapter = b"CHAPTER", flag = b"FLAG", arriveevue = b"ARRIVEEVUE",
    lastvue = b"LASTVUE", vue = b"VUE", nextvue = b"NEXTVUE",
    next_warp_invalide = b"NextWarpInvalide", clicperso = b"CLICPERSO",
    clicobjet = b"CLICOBJET", cliczone = b"CLICZONE", sujet = b"SUJET",
    objetenmain = b"OBJETENMAIN", objetenmain_next_warp = b"Objetenmain_NextWarp",
    sacados = b"SACADOS", deposeobj = b"DEPOSEOBJ", numframeobjet = b"NUMFRAMEOBJET",
    numframeperso = b"NUMFRAMEPERSO", abortdepart = b"ABORTDEPART",
    departvue = b"DEPARTVUE", limiteanglewarpx = b"LIMITEANGLEWARPX",
    limiteanglewarpy = b"LIMITEANGLEWARPY", anglewarpx = b"ANGLEWARPX",
    anglewarpy = b"ANGLEWARPY", anglewarpxnow = b"AngleWarpXNow",
    anglewarpynow = b"AngleWarpYNow", zoomstart = b"ZoomStart",
    zoompsychostart = b"ZoomPsychoStart", changewam = b"CHANGEWAM",
    cinema = b"CINEMA", trajsound = b"TrajSound", disable = b"DISABLE",
    hideguest = b"HIDEGUEST", showguest = b"SHOWGUEST", enablesuj = b"ENABLESUJ",
    disablesuj = b"DISABLESUJ", killsuj = b"KILLSUJ", clearsuj = b"CLEARSUJ",
    dial = b"DIAL", changecd = b"CHANGECD", newsound = b"NEWSOUND",
    stopsound = b"STOPSOUND", loadmusik = b"LOADMUSIK", delmusik = b"DELMUSIK",
    startmusik = b"STARTMUSIK", stopmusik = b"STOPMUSIK", hideperso = b"HIDEPERSO",
    showperso = b"SHOWPERSO", hideobjet = b"HIDEOBJET", showobjet = b"SHOWOBJET",
    loadanim = b"LOADANIM", changeanimgotoperso = b"ChangeAnimGotoPerso",
    playanimp = b"PLAYANIMP", playanimo = b"PLAYANIMO", continueanim = b"CONTINUEANIM",
    stopanim = b"STOPANIM", gotoframeperso = b"GOTOFRAMEPERSO",
    gotoframeobjet = b"GOTOFRAMEOBJET", anim_the_frame = b"ANIM_THE_FRAME",
    addframeperso = b"ADDFRAMEPERSO", addframeobjet = b"ADDFRAMEOBJET",
    incperso = b"INCPERSO", addguest = b"ADDGUEST", subguest = b"SUBGUEST",
    monde = b"MONDE", exit = b"EXIT", changeobjectworld = b"ChangeObjectWorld",
    disablezoneclicnwnz = b"DisableZoneClicNWNZ", enablezoneclicnwnz = b"EnableZoneClicNWNZ",
    disablezoneclicnwnamez = b"DisableZoneClicNWNameZ",
    enablezoneclicnwnamez = b"EnableZoneClicNWNameZ", message_clic = b"MESSAGE_CLIC",
    viewbigspr = b"ViewBigSpr", credits = b"CREDITS", testtime1 = b"TESTTIME1",
    testtime2 = b"TESTTIME2", time1 = b"TIME1", time2 = b"TIME2", random = b"RANDOM",
    testspecialevent = b"TestSpecialEvent", nearw77 = b"NEARW77", gameover = b"GAMEOVER",
}

/// The interpreter and the script state it keeps.
#[derive(Clone, Debug)]
pub struct Script {
    pub names: DefNames,
    pub keys: Keys,
    /// `int vars[300]` @ 0x006cfe90.
    pub vars: Vec<i32>,
    /// `int flags[1024]` @ 0x006d0578.
    pub flags: Vec<i32>,
    chapter_text: Vec<u8>,
    /// The chapter's size (@ 0x006d1690).
    pub chapter_size: usize,
    command: Vec<u8>,
    /// The current chapter (@ 0x006d1694) and the previous one
    /// (@ 0x006d1580).
    pub chapter: i32,
    pub previous_chapter: i32,
    /// `/set(chapter=N)` ran: `RunInit` restarts on the new `/INIT`
    /// (@ 0x006d1698).
    pub chapter_reload: bool,
    /// `RunInit` is running (@ 0x006d1688).
    init_running: bool,
    /// @ 0x006d1684: the `/INIT` walk skips its next section call.
    init_skip: bool,
    /// A command left the section to resume later (@ 0x006d1699), at
    /// [`Script::resume_at`] (@ 0x006d0574).
    pub resume_pending: bool,
    pub resume_at: usize,
    /// A `RunOneCommand` is running (@ 0x006d169a): no resume point then.
    in_command: bool,
    /// `RunOneCommand_AfterDisplay` (@ 0x00481790): a command for the next
    /// warp frame (@ 0x006cfd90).
    pub queued: Option<Vec<u8>>,
    /// The number of the last section header a scan passed (@ 0x006cfb78;
    /// nothing reads it).
    pub section_number: i32,
    /// The subjects of the characters, by `#PERSONNAGES#` index.
    pub subjects: SubjectTable,
    pub bag: Vec<BagEntry>,
    pub shown: Vec<Shown>,
    /// The cursor sprite (@ 0x00673f94) and whether it is from the object
    /// bank (@ 0x00673f68, the bank @ 0x00671ef4 or none).
    pub cursor_sprite: i32,
    pub cursor_objects: bool,
    /// The hand drawn at the cursor (@ 0x00673fa0) and its x (@ 0x005def58,
    /// −1 at start).
    pub hand_drawn: bool,
    pub hand_x: i32,
    /// `ANGLEWARPX` / `ANGLEWARPY` in radians (@ 0x006cfd8c, 0x006d054c)
    /// and the start angles of the next view entry (@ 0x005e2f30,
    /// 0x005e2f34, initialised in the data section).
    pub angle_x: f32,
    pub angle_y: f32,
    pub start_x: f32,
    pub start_y: f32,
    /// The current view (the record @ 0x006d85d4; its number), its number
    /// as the per-view reset keeps it (@ 0x006cfb70), the last `vue=`
    /// value (@ 0x006cfc80 = @ 0x006d157c, no reader known) and the
    /// `NEXTVUE` view (@ 0x006d85e4).
    pub current_view: Option<i32>,
    pub view_number: i32,
    pub vue_value: i32,
    pub next_view: Option<i32>,
    /// `SHOWGUEST` (@ 0x005df971, 1 at start), `INCPERSO` (@ 0x006cfb74).
    pub show_guest: bool,
    pub inc_perso: u8,
    /// The game quits (@ 0x006d85d0).
    pub quit: bool,
    /// The place file's name (@ 0x006d0340) and the `startmusik` list
    /// (@ 0x006d0444).
    pub wam: Vec<u8>,
    pub music: Vec<u8>,
    /// A click to handle (@ 0x006569c9), its companion (@ 0x006569a0), a
    /// double click (@ 0x006569cb), the space-travel overlay (@ 0x00671ef0)
    /// and the cinematic being started (@ 0x006d168c).
    pub click: bool,
    pub click_aux: i32,
    pub double_click: bool,
    pub overlay: bool,
    pub cinema_starting: bool,
    /// The x87 control word the script's float code runs under.
    pub fpu: Fpu,
    /// The 15 Hz tick count (@ 0x006569d8) and the seconds accumulator
    /// (@ 0x006569ec).
    pub ticks: i32,
    pub seconds: f32,
    /// The four "a tick passed" flags (@ 0x00656a4c, 0x00656a50,
    /// 0x00656a54, 0x00656a58), set by every tick; their readers clear
    /// them: the place animations (0x00656a4c, @ 0x0047d7f0), the
    /// buttons and the frame dispatcher (0x00656a50), the cursor
    /// (0x00656a54, @ 0x00474500). No Native reader takes them yet.
    pub tick_flags: [bool; 4],
    /// The first fatal error, after which nothing runs.
    pub fatal: Option<String>,
    /// The original's warnings (`FUN_00480c20`, their format strings), in
    /// order.
    pub warnings: Vec<String>,
    /// For comparisons with emulation goldens made with `RunInit` stubbed:
    /// `/set(chapter=N)` then loads the chapter without running its
    /// `/INIT`.
    #[doc(hidden)]
    pub stub_run_init: bool,
    /// For comparisons with emulation goldens: the chapter positions
    /// `SetVariables` is called with.
    #[doc(hidden)]
    pub trace: Option<Vec<usize>>,
    /// For logs: the chapter and the line number of each chapter line
    /// `SetVariables` runs.
    pub lines: Option<Vec<(i32, usize)>>,
}

impl Script {
    /// A new interpreter over `defnames.con`: every table empty, as after
    /// `LoadDefNames` and the flags' reset (@ 0x004846f0).
    pub fn new(defnames: &[u8]) -> Result<Script, String> {
        let names = DefNames::parse(defnames).map_err(|e| e.to_string())?;
        let keys = Keys::resolve(&names).map_err(|e| e.to_string())?;
        Ok(Script {
            names,
            keys,
            vars: vec![0; defnames::MAX_VARIABLES],
            flags: vec![0; FLAGS],
            chapter_text: vec![0; CHAPTER_BUFFER + 1],
            chapter_size: 0,
            command: Vec::new(),
            chapter: 0,
            previous_chapter: 0,
            chapter_reload: false,
            init_running: false,
            init_skip: false,
            resume_pending: false,
            resume_at: 0,
            in_command: false,
            queued: None,
            section_number: 0,
            subjects: SubjectTable::new(),
            bag: Vec::new(),
            shown: Vec::new(),
            cursor_sprite: 0,
            cursor_objects: false,
            hand_drawn: false,
            hand_x: -1,
            angle_x: 0.0,
            angle_y: 0.0,
            // Their values in the data section: −9° and 26°.
            start_x: f32::from_bits(0xbe20_d9d1),
            start_y: f32::from_bits(0x3ee8_5711),
            current_view: None,
            view_number: 0,
            vue_value: 0,
            next_view: None,
            show_guest: true,
            inc_perso: 0,
            quit: false,
            wam: Vec::new(),
            music: Vec::new(),
            click: false,
            click_aux: 0,
            double_click: false,
            overlay: false,
            cinema_starting: false,
            fpu: Fpu::Double,
            ticks: 0,
            seconds: 0.0,
            tick_flags: [false; 4],
            fatal: None,
            warnings: Vec::new(),
            stub_run_init: false,
            trace: None,
            lines: None,
        })
    }

    /// A variable by name (`ReadVar` @ 0x00484ff0); `None` when the name
    /// is not in `#VARIABLES#`.
    pub fn var(&self, name: &str) -> Option<i32> {
        let i = self.names.id(defnames::VARIABLES, name.as_bytes()).ok()?;
        Some(self.vars[i])
    }

    /// `Interp_WriteVar` (@ 0x00485010).
    pub fn set_var(&mut self, name: &str, value: i32) -> bool {
        match self.names.id(defnames::VARIABLES, name.as_bytes()) {
            Ok(i) => {
                self.vars[i] = value;
                true
            }
            Err(_) => false,
        }
    }

    /// `Interp_AddVar` (@ 0x00485040): the new value.
    pub fn add_var(&mut self, name: &str, delta: i32) -> Option<i32> {
        let i = self.names.id(defnames::VARIABLES, name.as_bytes()).ok()?;
        self.vars[i] = self.vars[i].wrapping_add(delta);
        Some(self.vars[i])
    }

    /// Puts a chapter's text in the buffer, as `LoadCurrentChap`
    /// (@ 0x00483af0) does after reading the file: the bytes beyond it
    /// keep what the previous chapter left there. False (and fatal) when
    /// it is larger than the buffer.
    pub fn set_chapter_text(&mut self, text: &[u8]) -> bool {
        if text.len() > CHAPTER_BUFFER {
            self.fail(format!(
                "LoadCurrentChap: chapter of {} bytes (64000 at most)",
                text.len()
            ));
            return false;
        }
        self.chapter_text[..text.len()].copy_from_slice(text);
        self.chapter_size = text.len();
        true
    }

    /// The chapter's text.
    pub fn chapter_text(&self) -> &[u8] {
        &self.chapter_text[..self.chapter_size]
    }

    /// `LoadCurrentChap` (@ 0x00483af0): reads `chapi%03d.con` through the
    /// host. False when it is missing.
    pub fn load_chapter(&mut self, host: &mut dyn Host, chapter: i32) -> bool {
        match host.read_chapter(chapter) {
            Some(text) => self.set_chapter_text(&text),
            None => false,
        }
    }

    /// The byte at `i` of a text (0 past its end).
    #[inline]
    pub(crate) fn b(&self, src: Src, i: usize) -> u8 {
        match src {
            Src::Chapter => self.chapter_text.get(i).copied().unwrap_or(0),
            Src::Command => self.command.get(i).copied().unwrap_or(0),
        }
    }

    /// The text from `i` on.
    pub(crate) fn tail(&self, src: Src, i: usize) -> &[u8] {
        let t = match src {
            Src::Chapter => &self.chapter_text,
            Src::Command => &self.command,
        };
        &t[i.min(t.len())..]
    }

    /// The length of a text's buffer (the walks' bound).
    pub(crate) fn len(&self, src: Src) -> usize {
        match src {
            Src::Chapter => self.chapter_text.len(),
            Src::Command => self.command.len(),
        }
    }

    /// `strncmp(text + i, s, n) == 0`.
    pub(crate) fn at(&self, src: Src, i: usize, s: &[u8]) -> bool {
        self.tail(src, i).starts_with(s)
    }

    /// `_strnicmp(text + i, s, n) == 0`.
    pub(crate) fn at_ci(&self, src: Src, i: usize, s: &[u8]) -> bool {
        let t = self.tail(src, i);
        t.len() >= s.len() && t[..s.len()].eq_ignore_ascii_case(s)
    }

    /// `/` and a digit: a section header (`FUN_00484010` also keeps its
    /// number, which nothing reads).
    pub(crate) fn is_header(&self, src: Src, i: usize) -> bool {
        self.b(src, i) == b'/' && self.b(src, i + 1).is_ascii_digit()
    }

    /// The position after the first CR at or after `i` (the walk that ends
    /// most `/set` branches); the buffer's end when there is none.
    pub(crate) fn after_cr(&self, src: Src, mut i: usize) -> usize {
        let n = self.len(src);
        while i < n && self.b(src, i) != CR {
            i += 1;
        }
        (i + 1).min(n)
    }

    /// The text up to the first CR (for the hooks' arguments).
    pub(crate) fn line_text(&self, src: Src, i: usize) -> String {
        let t = self.tail(src, i);
        let end = t.iter().position(|&c| c == CR || c == 0).unwrap_or(t.len());
        t[..end].iter().map(|&c| char::from(c)).collect()
    }

    /// `atoi(text + i)`.
    pub(crate) fn atoi_at(&self, src: Src, i: usize) -> i32 {
        atlantis2_data::con::atoi(self.tail(src, i))
    }

    /// A name lookup that is fatal when it fails (`DefName_GetIdName`).
    pub(crate) fn id(&mut self, section: usize, src: Src, i: usize) -> i32 {
        match self.names.id(section, self.tail(src, i)) {
            Ok(v) => v as i32,
            Err(e) => {
                self.fail(e.to_string());
                0
            }
        }
    }

    /// Parameter 1 of a name (`FUN_00410e50` → `DefName_GetParam`
    /// @ 0x00410a30, fatal for an unknown name).
    pub(crate) fn param(&mut self, section: usize, src: Src, i: usize) -> i32 {
        match self.names.id(section, self.tail(src, i)) {
            Ok(_) => self.names.param(section, self.tail(src, i)),
            Err(e) => {
                self.fail(e.to_string());
                0
            }
        }
    }

    /// Records the first fatal error.
    pub(crate) fn fail(&mut self, msg: String) {
        log::error!("script: {msg}");
        if self.fatal.is_none() {
            self.fatal = Some(msg);
        }
    }

    /// Records a warning (`FUN_00480c20`): its format string, as the
    /// original passes it, and what it was about for the log.
    pub(crate) fn warn(&mut self, format: &str, detail: &str) {
        log::warn!("script: {format} [{detail}]");
        self.warnings.push(format.to_string());
    }

    /// Sets the cursor (`FUN_0047c0c0`).
    pub(crate) fn set_cursor(&mut self, sprite: i32, objects: bool) {
        self.cursor_sprite = sprite;
        self.cursor_objects = objects;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `defnames.con` with the keys, two characters, an object with a
    /// sprite, a subject with a parameter and a zone.
    fn script() -> Script {
        let mut t =
            b"// test\r\n#PERSONNAGES#\r\nDUMMY\r\nHERO\r\n#OBJETS#\r\nDUMMY\r\nBALL , 5\r\n\
            #SUJETS#\r\nDUMMY\r\nWEATHER,7\r\n#VARIABLES#\r\n"
                .to_vec();
        for k in Keys::NAMES {
            t.extend_from_slice(k);
            t.extend_from_slice(b"\r\n");
        }
        t.extend_from_slice(b"Counter\r\n#ZONES_CLIC#\r\nDUMMY\r\nDOOR\r\n#END_OF_FILE#\r\n");
        Script::new(&t).unwrap()
    }

    struct Nothing;
    impl Host for Nothing {
        fn read_chapter(&mut self, _: i32) -> Option<Vec<u8>> {
            None
        }
    }

    fn cond(s: &mut Script, c: &str) -> bool {
        s.set_chapter_text(format!("/con{c}\r\n").as_bytes());
        s.eval_condition(&mut Nothing, 4).is_some()
    }

    fn set(s: &mut Script, line: &str) -> Option<usize> {
        s.set_chapter_text(format!("{line}\r\n/000\r\n").as_bytes());
        s.set_variables(&mut Nothing, Src::Chapter, 0)
    }

    #[test]
    fn set_forms() {
        let mut s = script();
        assert_eq!(set(&mut s, "/set(flag7=3)"), Some(14));
        assert_eq!(s.flags[7], 3);
        // FLAGnnn++ stores atoi("+)") = 0; KEY++ adds; KEY=++ stores 0.
        set(&mut s, "/set(FLAG7++)");
        assert_eq!(s.flags[7], 0);
        set(&mut s, "/set(counter=4)");
        set(&mut s, "/set(Counter++)");
        assert_eq!(s.var("COUNTER"), Some(5));
        set(&mut s, "/set(counter--)");
        assert_eq!(s.var("COUNTER"), Some(4));
        set(&mut s, "/set(counter=++)");
        assert_eq!(s.var("COUNTER"), Some(0));
        set(&mut s, "/set(counter=12abc)");
        assert_eq!(s.var("COUNTER"), Some(12));
        // A blank breaks FLAGnnn: the variable FLAG is written.
        set(&mut s, "/set(flag 9=2)");
        assert_eq!((s.flags[9], s.var("FLAG")), (0, Some(2)));
        // 999 is "no limit".
        set(&mut s, "/set(limiteanglewarpy=999)");
        assert_eq!(s.var("LIMITEANGLEWARPY"), Some(-1));
        // An unknown key is fatal.
        assert_eq!(set(&mut s, "/set(nothing=1)"), None);
        assert!(s.fatal.is_some());
    }

    #[test]
    fn tables() {
        let mut s = script();
        set(&mut s, "/set(enablesuj=HERO,WEATHER)");
        assert_eq!(
            s.subjects.list(1),
            [Subject {
                value: 7,
                index: 1,
                flag: 1
            }]
        );
        set(&mut s, "/set(disablesuj=HERO,WEATHER)");
        assert!(!s.subjects.list(1)[0].enabled());
        set(&mut s, "/set(killsuj=HERO,WEATHER)");
        assert!(s.subjects.list(1).is_empty());
        set(&mut s, "/set(sacados=ball)");
        assert_eq!(
            s.bag,
            [BagEntry {
                sprite: 5,
                id: 1,
                world: 0
            }]
        );
        assert!(cond(&mut s, "(sacados=BALL)"));
        assert!(!cond(&mut s, "(sacados=NOT_BALL)"));
        set(&mut s, "/set(sacados=NOT_BALL)");
        assert!(s.bag.is_empty());
        assert!(cond(&mut s, "(sacados=not_ball)"));
        set(&mut s, "/set(objetenmain=BALL)");
        assert_eq!((s.var("OBJETENMAIN"), s.cursor_sprite), (Some(1), 5));
        assert!(cond(&mut s, "(objetenmain=BALL)"));
        set(&mut s, "/set(hideperso=HERO)");
        assert_eq!(
            s.shown,
            [Shown {
                id: 1,
                shown: false,
                kind: 1
            }]
        );
        set(&mut s, "/set(showperso=HERO,2)");
        assert_eq!(
            s.shown,
            [Shown {
                id: 1,
                shown: true,
                kind: 1
            }]
        );
    }

    #[test]
    fn conditions() {
        let mut s = script();
        s.flags[1] = 1;
        s.flags[4] = 1;
        // | binds tighter than &: a & (b | c) & d.
        assert!(cond(&mut s, "(flag1=1)&(flag2=1)|(flag3=0)&(flag4=1)"));
        assert!(!cond(&mut s, "(flag1=1)&(flag2=1)|(flag3=1)&(flag4=1)"));
        assert!(!cond(&mut s, "(flag1=0)|(flag2=1)&(flag4=1)"));
        // Signed compares; a value name resolved by its key; subjects by
        // their parameter.
        s.vars[s.keys.time1] = -3;
        assert!(cond(&mut s, "(time1<0)"));
        s.vars[s.keys.cliczone] = 1;
        assert!(cond(&mut s, "(cliczone=DOOR)"));
        s.vars[s.keys.sujet] = 7;
        assert!(cond(&mut s, "(sujet=WEATHER)"));
        assert!(cond(&mut s, "(vue = 0)"));
        assert!(cond(&mut s, ""));
        // The tested flag is left in FLAG.
        assert!(cond(&mut s, "(flag4!0)"));
        assert_eq!(s.var("FLAG"), Some(1));
    }

    #[test]
    fn sections_and_comments() {
        let mut s = script();
        let text = b"/INIT\r\n/FININIT\r\n/001\r\n/con(flag1=1)\r\n/set(flag10=1)\r\n\
            /002\r\n/con(flag2=1)\r\n/con(flag3=1)\r\n/set(flag11=1)\r\n/con(flag4=1)\r\n/set(flag12=1)\r\n\
            /003\r\n/con(flag5=1)\r\n/set(flag15=1)\r\n#/set(flag13=1)\r\n\r\n#/set(flag14=1)\r\n\
            /set(flag16=1)\r\n/004\r\n/con(flag6=1)\r\n#comment\r\n/set(flag17=1)\r\n/000\r\n";
        let run = |s: &mut Script, on: &[usize]| {
            s.set_chapter_text(text);
            s.flags.iter_mut().for_each(|f| *f = 0);
            on.iter().for_each(|&f| s.flags[f] = 1);
            s.ctrl_scenar_in(&mut Nothing);
            (10..20).filter(|&f| s.flags[f] != 0).collect::<Vec<_>>()
        };
        assert_eq!(run(&mut s, &[1, 2, 3]), [10]);
        // A true head followed by /con: the first true sub-condition.
        assert_eq!(run(&mut s, &[2, 4]), [12]);
        assert_eq!(run(&mut s, &[2, 3, 4]), [11]);
        // A false head skips its sub-conditions.
        assert_eq!(run(&mut s, &[3]), Vec::<usize>::new());
        // `#/set` before an empty line runs, before a /set line it does not.
        assert_eq!(run(&mut s, &[5]), [13, 15, 16]);
        // A true head not followed by /set waits for sub-conditions: none.
        assert_eq!(run(&mut s, &[6]), Vec::<usize>::new());
    }

    #[test]
    fn clocks_and_angles() {
        let mut s = script();
        let mut now = 0.0;
        for _ in 0..15 {
            let last = now;
            now += 1.0 / 15.0;
            s.tick(now, last);
        }
        assert_eq!((s.var("TIME1"), s.var("RANDOM")), (Some(1), Some(15)));
        // A stall of over 10 s is dropped.
        s.tick(now + 12.0, now);
        assert_eq!((s.var("TIME1"), s.seconds), (Some(1), 0.0));
        assert_eq!(s.degrees((-90f32).to_radians()), 270);
        assert_eq!(s.degrees(0.5f32.to_radians()), 0);
    }
}
