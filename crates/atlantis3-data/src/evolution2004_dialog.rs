//! *Atlantis Evolution*'s 2004 program's dialogs (`CDialogue`,
//! atlantis4-cd-en-3022848): the script's `DIAL=` options, the speakers of
//! `scenar\info.txt`, the head lists of `scenar\com.txt`, the lines of
//! `loc\text\<SousTitre>\dial.txt` ([`crate::dialog::parse_dial_2004`]) and
//! the names of each line's voice, head film and background.
//!
//! The RE note is atlantis4-re `subsystems/cd2004-dialogs.md`; every rule
//! cites its address in atlantis4-cd-en-3022848. *Atlantis III*'s dialog
//! files ([`crate::dialog`]) are another format: no `[Mesh]`, `[Focale]`
//! or `[TabWeight]`, no voice column in `com.txt`.

use std::collections::BTreeMap;

use crate::dialog::DialTxt;
use crate::tokens::{atoi, latin1, strip_letters, ContentLines, Reader};

/// The speaker name standing for the player (`HERO`, 0x006acfa8): its
/// lines are spoken by the current player perso (FUN_0046ae90).
pub const HERO: &str = "HERO";

/// The player persos the start-up registers (FUN_00410410 @ 0x004a22bb ..
/// 0x004a2354, the list 0x006e0b40); `PLAYERNAME` accepts only these
/// (FUN_00410520 @ 0x0044df84).
pub const PLAYERS: [&str; 16] = [
    "Hewitt",
    "HewittVillage",
    "Hewitt2",
    "HewittAtlante",
    "HewittNuit",
    "HewittAtlante2",
    "HewittAtlante3",
    "HewittAtlante4",
    "HewittCosmo",
    "HewittCosmo2",
    "HewittCosmo3",
    "HewittCosmo4",
    "HewittServiteur",
    "HewittServiteur2",
    "HewittTane",
    "HewittSuspendu",
];

/// The player perso at start-up (`PLAYERNAME` := `Hewitt`,
/// FUN_0040eca0 @ 0x004a2377).
pub const DEFAULT_PLAYER: &str = "Hewitt";

/// Whether `name` is a player perso (FUN_00410520: a byte compare against
/// each name of the list 0x006e0b40).
pub fn is_player(name: &str) -> bool {
    PLAYERS.contains(&name)
}

/// The music's share kept during a dialog when `DIAL=` has no `%` word
/// (`MOV EDI,0x19` @ 0x0044e72f).
pub const DEFAULT_PERCENT: i32 = 25;

/// The options of a `DIAL=` line, as the `/set` interpreter reads them
/// (FUN_0044c850 @ 0x0044e6b7..0x0044e97e).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialOptions {
    /// The dialog numbers, in the line's order.
    pub numbers: Vec<i32>,
    /// `%<x>`: the music's share kept, at most 100 (no lower bound).
    pub percent: i32,
    /// `off` (in a word): the voice-over mode.
    pub voixoff: bool,
    /// `light` (in a word): the light mode, which is a voice-over too.
    pub light: bool,
}

impl DialOptions {
    /// Reads the line from its `=` (`line` starts at the `=`; the text up
    /// to the end of the line).
    ///
    /// The words (the shared tokenizer's separators, FUN_0044ade0) one by
    /// one: a word starting with `%` gives the percent (its letters
    /// removed by FUN_0044a7b0, `atoi`, then `min(100)`); else the word
    /// without its letters (FUN_0044a7b0), `atoi`: a number ≠ 0 is a
    /// dialog (pushed by FUN_00453a70); else a word containing `off`
    /// (`find` ≠ npos, @ 0x0044e889) sets the voice-over, else one
    /// containing `light` (@ 0x0044e8eb) the light mode. The light mode
    /// sets the voice-over too (@ 0x0044e929..0x0044e931). The text the
    /// words come from starts after the `=` (ESI = the `=` + 1,
    /// @ 0x0044ccc6 for every value key); an `=` given here is skipped.
    pub fn parse(line: &[u8]) -> DialOptions {
        let line = line
            .iter()
            .position(|&c| c == b'\r' || c == b'\n' || c == 0)
            .map_or(line, |e| &line[..e]);
        let mut o = DialOptions {
            numbers: Vec::new(),
            percent: DEFAULT_PERCENT,
            voixoff: false,
            light: false,
        };
        let words = line
            .split(|&c| c == b'=' || crate::tokens::is_separator(c))
            .filter(|w| !w.is_empty());
        for w in words {
            if w[0] == b'%' {
                o.percent = atoi(&strip_letters(&w[1..])).min(100);
                continue;
            }
            let n = Self::number(w);
            if n != 0 {
                o.numbers.push(n);
            } else if contains(w, b"off") {
                o.voixoff = true;
            } else if contains(w, b"light") {
                o.light = true;
            }
        }
        if o.light {
            o.voixoff = true;
        }
        o
    }

    /// A word's number: its letters removed (FUN_0044a7b0), then `atoi`
    /// (FUN_00449a50).
    pub fn number(w: &[u8]) -> i32 {
        atoi(&strip_letters(w))
    }

    /// The dialog played: the numbers shuffled by MSVC's
    /// `std::random_shuffle` (FUN_00453cf0 @ 0x0044e94a: for i = 1..n,
    /// `swap(v[i], v[(rand() & 0x7fff) % i])`), then `v[rand() % n]`
    /// (@ 0x0044e95d). `None` without a number (the original divides by
    /// zero there).
    pub fn pick(&self, rand: &mut dyn FnMut() -> i32) -> Option<i32> {
        let mut v = self.numbers.clone();
        if v.is_empty() {
            return None;
        }
        for i in 1..v.len() {
            let r = (rand() & 0x7fff) as usize;
            v.swap(i, r % i);
        }
        let r = rand() as u32 as usize;
        Some(v[r % v.len()])
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// A speaker of `scenar\info.txt` (`CDialogue::AddPersoData`
/// FUN_0046b880; the record FUN_00475530).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Perso2004 {
    /// The `[Perso]` name, as written (the map key; compared byte for
    /// byte).
    pub name: String,
    /// `[Nom]` words: the head films' prefixes (record +0x04); the name
    /// when there is none.
    pub noms: Vec<String>,
    /// `[Fond]` words: the backgrounds' prefixes (record +0x14); the
    /// first `[Nom]` when there is none.
    pub fonds: Vec<String>,
    /// `[IdText]` words: the place animations that show the speaker
    /// (record +0x24).
    pub id_texts: Vec<String>,
    /// `[Spw]`: the speaker is shown by a place animation, not a head
    /// film (record +0x30).
    pub spw: bool,
    /// `[Neutre]` numbers: the head films a line picks from (record
    /// +0x38).
    pub neutre: Vec<i32>,
}

impl Perso2004 {
    /// `Nom[i % count]` (FUN_00475860; the original divides by zero
    /// without a name, which [`parse_info_2004`] never leaves).
    pub fn nom(&self, i: usize) -> &str {
        self.noms
            .get(i % self.noms.len().max(1))
            .map_or("", String::as_str)
    }

    /// `Fond[i % count]` (FUN_004758b0).
    pub fn fond(&self, i: usize) -> &str {
        self.fonds
            .get(i % self.fonds.len().max(1))
            .map_or("", String::as_str)
    }
}

/// Reads `scenar\info.txt` (`AtlDial_MainInit` FUN_0040ed20 up to
/// @ 0x0040f7f6).
///
/// Content lines (empty and `//` lines skipped) whose first word is
/// `[Perso]` open a speaker named by the second word; the lines up to one
/// whose first word is `[FinPerso]` give, by their first word: `[Nom]`
/// (the next word to the names), `[Fond]` (to the backgrounds), `[Spw]`
/// (the flag), `[IdText]` (the next word to the place animations), else
/// `[Neutre]` (every following number, `atoi`). At `[FinPerso]`, no name →
/// the speaker's name (@ 0x0040f6a5), no background → the first name
/// (@ 0x0040f6c3). The end of the file inside a block logs
/// `AtlDial_MainInit() : unexpected end of file` (@ 0x0040f1b8) and ends
/// the block here.
pub fn parse_info_2004(data: &[u8]) -> Vec<Perso2004> {
    let mut out = Vec::new();
    let mut lines = ContentLines::new(data);
    while let Some((_, line)) = lines.next() {
        let mut w = Reader::new(line);
        if w.word() != b"[Perso]" {
            continue;
        }
        let mut p = Perso2004 {
            name: latin1(w.word()),
            ..Perso2004::default()
        };
        loop {
            let Some((_, line)) = lines.next() else {
                log::warn!("AtlDial_MainInit() : unexpected end of file. info.txt");
                break;
            };
            let mut w = Reader::new(line);
            match w.word() {
                b"[FinPerso]" => break,
                b"[Nom]" => p.noms.push(latin1(w.word())),
                b"[Fond]" => p.fonds.push(latin1(w.word())),
                b"[Spw]" => p.spw = true,
                b"[IdText]" => p.id_texts.push(latin1(w.word())),
                b"[Neutre]" => {
                    while !w.at_end() {
                        p.neutre.push(atoi(w.word()));
                    }
                }
                _ => {}
            }
        }
        // Empty words are not pushed (@ 0x0040f3c0..0x0040f4c3 test the
        // word ≠ "").
        p.noms.retain(|s| !s.is_empty());
        p.fonds.retain(|s| !s.is_empty());
        p.id_texts.retain(|s| !s.is_empty());
        if p.noms.is_empty() {
            p.noms.push(p.name.clone());
        }
        if p.fonds.is_empty() {
            p.fonds.push(p.noms[0].clone());
        }
        out.push(p);
    }
    out
}

/// A line of `scenar\com.txt`: `<dialog> <line> <n> [<n> ...]`
/// (@ 0x0040f940..0x0040fbec): the line's own head list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComLine2004 {
    pub dialog: i32,
    pub line: i32,
    pub list: Vec<i32>,
}

/// Reads `scenar\com.txt` (FUN_0040ed20 @ 0x0040f8c4..): content lines,
/// two `atoi` words then every following `atoi` word, given to
/// FUN_0046bdd0 (the entry's list := the numbers, FUN_0046af60).
pub fn parse_com_2004(data: &[u8]) -> Vec<ComLine2004> {
    ContentLines::new(data)
        .map(|(_, line)| {
            let mut w = Reader::new(line);
            let dialog = atoi(w.word());
            let line = atoi(w.word());
            let mut list = Vec::new();
            while !w.at_end() {
                list.push(atoi(w.word()));
            }
            ComLine2004 { dialog, line, list }
        })
        .collect()
}

/// A dialog line (`CDialogue::AddDialogEntry` FUN_0046bb40, the entry
/// FUN_0046aa90).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry2004 {
    /// The speaker as `dial.txt` names it (`HERO` kept: see
    /// [`Dialogs2004::speaker`]).
    pub speaker: String,
    /// The subtitle (`<void>` until `AddDialogText`, 0x006b14b0).
    pub text: Vec<u8>,
    /// The line's head list (`com.txt`; entry +0x30). Empty: the
    /// speaker's `[Neutre]` (entry +0x28 set).
    pub list: Vec<i32>,
    /// The list is drawn at random (entry +0x3c: `dial.txt`'s entries
    /// pass 1, @ 0x0040ff7e); else it cycles (+0x40).
    pub random: bool,
    /// The cycling position (+0x40).
    pub index: i32,
}

/// What the loader dropped, as the original does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadIssue2004 {
    /// A `dial.txt` line whose speaker has no `[Perso]` and is not `HERO`
    /// (`CDialogue_AddDialogEntry() : le personnage %s introuvable !!`,
    /// @ 0x0046bc56).
    UnknownSpeaker { dialog: i32, line: i32 },
    /// A `com.txt` line of no dialog (`AddDialogText() : numero de
    /// dialogue incorrect`, @ 0x0046be8d).
    UnknownDialog { dialog: i32, line: i32 },
    /// A `com.txt` line of no line (`numero de replique incorrect`,
    /// @ 0x0046bfd4).
    UnknownLine { dialog: i32, line: i32 },
}

/// The dialog manager's data (`CDialogue`'s static maps 0x00716d28 for
/// the speakers, 0x00716c40 for the dialogs).
#[derive(Clone, Debug, Default)]
pub struct Dialogs2004 {
    pub persos: BTreeMap<String, Perso2004>,
    pub entries: BTreeMap<i32, BTreeMap<i32, Entry2004>>,
    pub issues: Vec<LoadIssue2004>,
    /// The player perso (`PLAYERNAME`, 0x006e6f40).
    pub player: String,
}

impl Dialogs2004 {
    /// `AtlDial_MainInit` (FUN_0040ed20): `info.txt`, then `dial.txt`
    /// (FUN_0040fd20 @ 0x0040f7f6), then `com.txt`. A repeated speaker
    /// replaces the earlier one (FUN_0046b880's copy into the existing
    /// node); a repeated (dialog, line) replaces too [INFERRED: the map
    /// insert FUN_00473400 is not read].
    pub fn build(persos: Vec<Perso2004>, dial: &DialTxt, com: &[ComLine2004]) -> Dialogs2004 {
        let mut out = Dialogs2004 {
            player: DEFAULT_PLAYER.to_string(),
            ..Dialogs2004::default()
        };
        for p in persos {
            out.persos.insert(p.name.clone(), p);
        }
        for d in &dial.lines {
            if !out.add_entry(d.dialog, d.line, &d.speaker, Vec::new(), true) {
                out.issues.push(LoadIssue2004::UnknownSpeaker {
                    dialog: d.dialog,
                    line: d.line,
                });
                continue;
            }
            out.set_text(d.dialog, d.line, d.text.clone());
        }
        for c in com {
            match out.entries.get_mut(&c.dialog) {
                None => out.issues.push(LoadIssue2004::UnknownDialog {
                    dialog: c.dialog,
                    line: c.line,
                }),
                Some(d) => match d.get_mut(&c.line) {
                    None => out.issues.push(LoadIssue2004::UnknownLine {
                        dialog: c.dialog,
                        line: c.line,
                    }),
                    Some(e) => e.list = c.list.clone(),
                },
            }
        }
        out
    }

    /// `AddDialogEntry(d, l, speaker, list, random)` (FUN_0046bb40): a
    /// speaker without `[Perso]` other than `HERO` is refused (false).
    pub fn add_entry(
        &mut self,
        dialog: i32,
        line: i32,
        speaker: &str,
        list: Vec<i32>,
        random: bool,
    ) -> bool {
        if !self.persos.contains_key(speaker) && speaker != HERO {
            log::warn!("CDialogue_AddDialogEntry() : le personnage  {speaker} introuvable !!");
            return false;
        }
        self.entries.entry(dialog).or_default().insert(
            line,
            Entry2004 {
                speaker: speaker.to_string(),
                text: b"<void>".to_vec(),
                list,
                random,
                index: 0,
            },
        );
        true
    }

    /// `AddDialogText(d, l, text)` (FUN_0046c110).
    pub fn set_text(&mut self, dialog: i32, line: i32, text: Vec<u8>) {
        if let Some(e) = self.entries.get_mut(&dialog).and_then(|d| d.get_mut(&line)) {
            e.text = text;
        }
    }

    /// The number of lines of dialog `n` (the dialog node's size +0x1c).
    pub fn count(&self, n: i32) -> i32 {
        self.entries.get(&n).map_or(0, |d| d.len() as i32)
    }

    pub fn entry(&self, dialog: i32, line: i32) -> Option<&Entry2004> {
        self.entries.get(&dialog)?.get(&line)
    }

    /// `CDialogue::Open(n)`'s fallback (FUN_0046cd90 @ 0x0046ce5d..): an
    /// unknown dialog logs `CDialogue::Open() : le dialogue <n> n'existe
    /// pas` and becomes a one-line dialog of `HERO` saying `Dial <n>`.
    /// Whether the dialog existed.
    pub fn open(&mut self, n: i32) -> bool {
        if self.entries.contains_key(&n) {
            return true;
        }
        log::warn!("CDialogue::Open() : le dialogue {n} n'existe pas.");
        self.add_entry(n, 1, HERO, Vec::new(), true);
        self.set_text(n, 1, format!("Dial {n}").into_bytes());
        false
    }

    /// The speaker who says a line (FUN_0046ae90): `HERO` is the player
    /// perso.
    pub fn speaker<'a>(&'a self, e: &'a Entry2004) -> &'a str {
        if e.speaker == HERO {
            &self.player
        } else {
            &e.speaker
        }
    }

    /// The perso record of a speaker, an empty one when it has none (the
    /// map's `operator[]`, FUN_00471f80, inserts an empty record).
    pub fn perso(&self, speaker: &str) -> Perso2004 {
        self.persos.get(speaker).cloned().unwrap_or_default()
    }

    /// The head number of line (dialog, line) (FUN_0046d1c0 @ 0x0046dbf9):
    /// an empty list draws among the speaker's `[Neutre]` (`rand() %
    /// count`, 0 without one and no draw); a random list draws among its
    /// numbers; else the list cycles from its second number.
    pub fn head_number(&mut self, dialog: i32, line: i32, rand: &mut dyn FnMut() -> i32) -> i32 {
        let Some(e) = self.entries.get(&dialog).and_then(|d| d.get(&line)) else {
            return 0;
        };
        if e.list.is_empty() {
            let speaker = self.speaker(e).to_string();
            let p = self.perso(&speaker);
            if p.neutre.is_empty() {
                return 0;
            }
            return p.neutre[rand() as u32 as usize % p.neutre.len()];
        }
        let e = self
            .entries
            .get_mut(&dialog)
            .and_then(|d| d.get_mut(&line))
            .unwrap();
        if e.random {
            return e.list[rand() as u32 as usize % e.list.len()];
        }
        e.index = (e.index + 1) % e.list.len() as i32;
        e.list[e.index as usize]
    }
}

/// A line's voice: `AT%s%s.%s` (0x006b1798) with the dialog on 4 digits,
/// the line on 2 (FUN_00448cf0) and `ogg` (0x006ea030), in
/// `loc\voix\<Langue>\` (FUN_004710f0 → FUN_00417b10).
pub fn voice_name(dialog: i32, line: i32) -> String {
    format!("AT{dialog:04}{line:02}.ogg")
}

/// A head film: `binkdial\` + `Nom` + the head number on 4 digits +
/// `.bik` (0x006b1c80) (FUN_0046f980 → FUN_004188b0, the current disc's
/// folder).
pub fn head_name(nom: &str, number: i32) -> String {
    format!("binkdial/{nom}{number:04}.bik")
}

/// The two background pictures FUN_004700d0 tries, in order, for a line
/// whose speaker shows no place animation: `binkdial\` + `Fond` + the
/// view's warp name without its 4-byte extension + the head number
/// modulo 3 on 2 digits + `.jpg` (0x006b1c90); when that file is missing,
/// the name's last 4 digits replaced by `0000` (0x006b1c88).
pub fn background_names(fond: &str, warp: &str, number: i32) -> [String; 2] {
    let nn = format!("{:02}", number % 3);
    let cut = |n: usize| warp.get(..warp.len().saturating_sub(n)).unwrap_or("");
    [
        format!("binkdial/{fond}{}{nn}.jpg", cut(4)),
        format!("binkdial/{fond}{}0000{nn}.jpg", cut(8)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_options() {
        let o = DialOptions::parse(b"=4,off)\r\n/005");
        assert_eq!(o.numbers, vec![4]);
        assert!(o.voixoff && !o.light);
        assert_eq!(o.percent, DEFAULT_PERCENT);
        let o = DialOptions::parse(b"=153,154,155,156,157,light)");
        assert_eq!(o.numbers, vec![153, 154, 155, 156, 157]);
        assert!(o.voixoff && o.light);
        let o = DialOptions::parse(b"=224)");
        assert_eq!((o.numbers.clone(), o.voixoff), (vec![224], false));
        let o = DialOptions::parse(b"=7,%140)");
        assert_eq!(o.percent, 100);
        let o = DialOptions::parse(b"=7,%40)");
        assert_eq!(o.percent, 40);
    }

    /// One number: no shuffle draw, one pick draw. Two: one shuffle draw
    /// (always a swap: `r % 1` = 0), then the pick.
    #[test]
    fn dial_pick() {
        let o = DialOptions::parse(b"=4,off");
        let mut draws = 0;
        assert_eq!(
            o.pick(&mut || {
                draws += 1;
                7
            }),
            Some(4)
        );
        assert_eq!(draws, 1);
        let o = DialOptions::parse(b"=10,20");
        let mut seq = [5, 0].into_iter();
        assert_eq!(o.pick(&mut || seq.next().unwrap()), Some(20));
        let o = DialOptions::parse(b"=off");
        assert_eq!(o.pick(&mut || 0), None);
    }

    const INFO: &[u8] = b"// c\r\n[Perso]\t\tNarrateur\r\n[FinPerso]\r\n\r\n\
[Perso]\t\tHewitt2\r\n[Nom]\t\thh\r\n[Fond]\t\ther2\r\n[Neutre]\t\t0,1,2\r\n[FinPerso]\r\n\
[Perso] Cy\r\n[Nom] cy\r\n[Nom] ch\r\n[IdText] A_B\r\n[IdText] C\r\n[Spw]\r\n[FinPerso]\r\n";

    #[test]
    fn info() {
        let p = parse_info_2004(INFO);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].noms, vec!["Narrateur"]);
        assert_eq!(p[0].fonds, vec!["Narrateur"]);
        assert_eq!(p[1].noms, vec!["hh"]);
        assert_eq!(p[1].fonds, vec!["her2"]);
        assert_eq!(p[1].neutre, vec![0, 1, 2]);
        assert_eq!(p[2].noms, vec!["cy", "ch"]);
        assert_eq!(p[2].fonds, vec!["cy"]);
        assert_eq!(p[2].id_texts, vec!["A_B", "C"]);
        assert!(p[2].spw && !p[1].spw);
        assert_eq!(p[2].nom(3), "ch");
    }

    #[test]
    fn build_and_heads() {
        let dial = crate::dialog::parse_dial_2004(
            b"4\t1\tNarrateur\tPolishing.\r\n2\t1\tHewitt2\tHi.\r\n2\t2\tHERO\tYes.\r\n\
9\t1\tNobody\tLost.\r\n",
        );
        let com = parse_com_2004(b"2\t1\t5\t6\r\n3\t1\t0\r\n2\t7\t0\r\n");
        let mut d = Dialogs2004::build(parse_info_2004(INFO), &dial, &com);
        assert_eq!(d.count(4), 1);
        assert_eq!(d.count(2), 2);
        assert_eq!(d.count(9), 0);
        assert_eq!(d.entry(4, 1).unwrap().text, b"Polishing.");
        assert_eq!(d.entry(2, 1).unwrap().list, vec![5, 6]);
        assert_eq!(
            d.issues,
            vec![
                LoadIssue2004::UnknownSpeaker { dialog: 9, line: 1 },
                LoadIssue2004::UnknownDialog { dialog: 3, line: 1 },
                LoadIssue2004::UnknownLine { dialog: 2, line: 7 },
            ]
        );
        // HERO is the player perso.
        let e = d.entry(2, 2).unwrap().clone();
        assert_eq!(d.speaker(&e), "Hewitt");
        // A random list: rand() % 2.
        assert_eq!(d.head_number(2, 1, &mut || 3), 6);
        // HERO (Hewitt, no [Perso] here): no Neutre, 0 and no draw.
        let mut draws = 0;
        assert_eq!(
            d.head_number(2, 2, &mut || {
                draws += 1;
                0
            }),
            0
        );
        assert_eq!(draws, 0);
        // The fallback of an unknown dialog.
        assert!(!d.open(77));
        assert_eq!(d.entry(77, 1).unwrap().text, b"Dial 77");
        assert_eq!(d.entry(77, 1).unwrap().speaker, HERO);
        assert!(d.open(4));
    }

    #[test]
    fn names() {
        assert_eq!(voice_name(4, 1), "AT000401.ogg");
        assert_eq!(head_name("hh", 2), "binkdial/hh0002.bik");
        assert_eq!(
            background_names("hh", "c10001.jp6", 4),
            [
                "binkdial/hhc1000101.jpg".to_string(),
                "binkdial/hhc1000001.jpg".to_string()
            ]
        );
    }
}
