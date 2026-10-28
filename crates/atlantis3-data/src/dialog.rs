//! The dialog scripts: `scenar\info.txt` (the speakers),
//! `scenar\com.txt` (each line's speaker, voice and staging) and
//! `loc\text\<language>\dial.txt` (each line's text), read together by
//! `AtlDial_MainInit` (@ 0x0040b9c0 for the first two, @ 0x0040cb10 for
//! the third) into the dialog manager `CDialogue`.
//!
//! Unlike *Atlantis II*, there is one set of files for the whole game (not
//! one per world), and only the text file is per language: its folder is
//! the `SousTitre` value of `Atlantis3.cfg` (@ 0x00407905), which the
//! options menu also sets (@ 0x0044edde).
//!
//! The files are read with the shared reader of [`crate::tokens`]. Each
//! speaker becomes a record of `CDialogue::AddPersoData` (@ 0x00464490),
//! each `com.txt` line an entry of `CDialogue::AddDialogEntry`
//! (@ 0x00464690) and each `dial.txt` line a text of
//! `CDialogue::AddDialogText` (@ 0x00465200); [`DialogScript::build`]
//! applies their rules.

use std::collections::BTreeMap;

use crate::tokens::{
    atof, atoi, atoi_word, latin1, lower, strip_digits, strip_letters, ContentLines, Reader,
};

/// `scenar\info.txt` (@ 0x0040ba04).
pub const INFO_PATH: &str = "Datas/scenar/info.txt";
/// `scenar\com.txt` (@ 0x0040c4c9).
pub const COM_PATH: &str = "Datas/scenar/com.txt";

/// The text file of the subtitle language `language` (the `SousTitre`
/// value, for example `fra`): `loc\text\<language>\dial.txt`
/// (@ 0x0040cb4a, the folder built @ 0x00418240).
pub fn dial_path(language: &str) -> String {
    format!("Datas/loc/text/{language}/dial.txt")
}

/// The number of `[TabWeightNN]` slots of a speaker (@ 0x0040bd0b fills 9
/// with 1.0).
pub const TAB_WEIGHTS: usize = 9;

/// The field of view a speaker's camera gets when its `[FocaleN]` list
/// has no value for it (@ 0x0047b9b0).
pub const DEFAULT_FOCALE: f32 = 76.0;

/// Why a dialog file cannot be read the way the game reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogError {
    /// A `[Perso]` block of `info.txt` reaches the end of the file without
    /// `[FinPerso]`: the original then shows its error box (@ 0x0040bd89)
    /// for ever.
    UnterminatedPerso {
        /// The speaker's name, as written.
        name: String,
    },
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogError::UnterminatedPerso { name } => {
                write!(f, "info.txt: [Perso] {name} has no [FinPerso]")
            }
        }
    }
}

impl std::error::Error for DialogError {}

/// A `[TabWeightNN]` line as the file has it.
#[derive(Clone, Debug, PartialEq)]
pub struct TabWeightLine {
    /// The key, `[TabWeight00]` .. `[TabWeight08]`.
    pub key: String,
    /// The slot the original writes: `atoi` of the key without its letters
    /// (@ 0x0040bfef .. 0x0040c031), so 0 for every key that starts with
    /// `[`.
    pub slot: i32,
    pub value: f32,
}

/// A speaker block `[Perso] <name>` .. `[FinPerso]` of `info.txt`
/// (@ 0x0040bcbd .. 0x0040c2b2), as `CDialogue::AddPersoData`
/// (@ 0x00464490) stores it (record built @ 0x0047b6e0).
#[derive(Clone, Debug, PartialEq)]
pub struct Perso {
    /// The second word of the `[Perso]` line, lower-cased (@ 0x0040c2ac):
    /// the name `com.txt` and `dial.txt` use, compared lower-cased.
    pub name: String,
    /// `[Mesh]`, lower-cased (@ 0x0040c29f): the prefix of the head files
    /// `<mesh>s<stand>.db` and of the dialog videos
    /// `<mesh>s<stand>c<camera>.hnm` (@ 0x00464690). Record +0x00.
    pub mesh: String,
    /// The `[IdText]` values, in order (empty ones dropped,
    /// @ 0x0040c1be). Record +0x14. Their use in the game is
    /// `[UNKNOWN]`: *Atlantis II*'s field of the same name names the
    /// place animations hidden while the speaker talks; to settle, find
    /// the readers of the record's +0x14.
    pub id_texts: Vec<String>,
    /// `[Smooth]`, 1.0 when absent. Record +0x24. Use `[UNKNOWN]` (the
    /// lip-sync smoothing in *Atlantis II*); to settle, the lip-sync code.
    pub smooth: f32,
    /// `[Stretch]`, 1.0 when absent. Record +0x28. Use `[UNKNOWN]`; to
    /// settle, the lip-sync code.
    pub stretch: f32,
    /// The nine weights as the original stores them (record +0x2c ..
    /// +0x4c): 1.0, except the slots the `[TabWeightNN]` lines write. With
    /// the shipped keys every line writes slot 0, so slot 0 holds the last
    /// line's value and slots 1..8 stay 1.0 (see [`TabWeightLine::slot`]).
    /// Their use is `[UNKNOWN]`: nine weights match the nine morph keys of
    /// the head's lip-sync (@ 0x0047d930: eight mouth keys and the
    /// expression key); to settle, the lip-sync code.
    pub tab_weights: [f32; TAB_WEIGHTS],
    /// The `[TabWeightNN]` lines as written, in order.
    pub tab_weight_lines: Vec<TabWeightLine>,
    /// `[GlobalWeight]`, 1.0 when absent. Record +0x50. Use `[UNKNOWN]`;
    /// to settle, the lip-sync code.
    pub global_weight: f32,
    /// `[Focale1]`: the field of view, in degrees, of each camera of stand
    /// 1 (record +0x54, read @ 0x0047b9b0). Every word to the line's end is
    /// one value: a trailing `// comment` adds a zero per word (one blank
    /// after the last value adds nothing: it is read with that value).
    pub focale1: Vec<f32>,
    /// `[Focale2]`: the same for stand 2 (record +0x64).
    pub focale2: Vec<f32>,
}

impl Perso {
    fn new(name: &[u8]) -> Perso {
        Perso {
            name: latin1(name),
            mesh: String::new(),
            id_texts: Vec::new(),
            smooth: 1.0,
            stretch: 1.0,
            tab_weights: [1.0; TAB_WEIGHTS],
            tab_weight_lines: Vec::new(),
            global_weight: 1.0,
            focale1: Vec::new(),
            focale2: Vec::new(),
        }
    }

    /// The weights the file's authors wrote, slot NN from `[TabWeightNN]`
    /// (the key's digits), 1.0 for the slots without a line. This is
    /// `[INFERRED]` from the key names; the original does not store them
    /// this way ([`Perso::tab_weights`]).
    pub fn written_tab_weights(&self) -> [f32; TAB_WEIGHTS] {
        let mut out = [1.0; TAB_WEIGHTS];
        for l in &self.tab_weight_lines {
            let digits: Vec<u8> = l.key.bytes().filter(u8::is_ascii_digit).collect();
            if let Some(slot) = std::str::from_utf8(&digits)
                .ok()
                .and_then(|d| d.parse::<usize>().ok())
                .filter(|&s| s < TAB_WEIGHTS)
            {
                out[slot] = l.value;
            }
        }
        out
    }

    /// The field of view of `camera` (1-based) in `stand` (1 or 2), or
    /// [`DEFAULT_FOCALE`] (`CDialogue::GetSpeakerFocale` @ 0x004685c0 →
    /// @ 0x0047b9b0).
    pub fn focale(&self, stand: i32, camera: i32) -> f32 {
        let list = match stand {
            1 => &self.focale1,
            2 => &self.focale2,
            _ => return DEFAULT_FOCALE,
        };
        usize::try_from(camera)
            .ok()
            .filter(|&c| c >= 1)
            .and_then(|c| list.get(c - 1))
            .copied()
            .unwrap_or(DEFAULT_FOCALE)
    }
}

/// Reads `info.txt` (@ 0x0040b9c0 .. 0x0040c3dc): lines outside the blocks
/// are skipped unless their first word is `[Perso]`; in a block, the first
/// word of each line is the key (case kept) and the next word its value;
/// unknown keys are skipped.
pub fn parse_info(data: &[u8]) -> Result<Vec<Perso>, DialogError> {
    let mut file = Reader::new(data);
    let mut out = Vec::new();
    while !file.at_end() {
        let line = file.line();
        if line.is_empty() || line.starts_with(b"//") {
            continue;
        }
        let mut words = Reader::new(line);
        if words.word() != b"[Perso]" {
            continue;
        }
        let mut p = Perso::new(words.word());
        loop {
            if file.at_end() {
                return Err(DialogError::UnterminatedPerso { name: p.name });
            }
            let mut words = Reader::new(file.line());
            let key = words.word();
            match key {
                b"[FinPerso]" => break,
                b"[Mesh]" => p.mesh = latin1(words.word()),
                b"[IdText]" => {
                    let id = words.word();
                    if !id.is_empty() {
                        p.id_texts.push(latin1(id));
                    }
                }
                b"[Smooth]" => p.smooth = atof(words.word()),
                b"[Stretch]" => p.stretch = atof(words.word()),
                b"[GlobalWeight]" => p.global_weight = atof(words.word()),
                b"[Focale1]" => p.focale1.extend(floats(&mut words)),
                b"[Focale2]" => p.focale2.extend(floats(&mut words)),
                _ if strip_digits(key) == b"[TabWeight]" => {
                    let slot = atoi(&strip_letters(key));
                    let value = atof(words.word());
                    // A slot outside 0..9 is written past the original's
                    // array of nine floats on the stack: not reproduced.
                    if let Some(w) = usize::try_from(slot)
                        .ok()
                        .and_then(|s| p.tab_weights.get_mut(s))
                    {
                        *w = value;
                    }
                    p.tab_weight_lines.push(TabWeightLine {
                        key: latin1(key),
                        slot,
                        value,
                    });
                }
                _ => {}
            }
        }
        // `tolower` in the "C" locale: ASCII only (see `tokens::lower`).
        p.name.make_ascii_lowercase();
        p.mesh.make_ascii_lowercase();
        out.push(p);
    }
    Ok(out)
}

/// The rest of a `[FocaleN]` line, one float per word, to the line's end.
fn floats(words: &mut Reader<'_>) -> Vec<f32> {
    let mut out = Vec::new();
    while !words.at_end() {
        out.push(atof(words.word()));
    }
    out
}

/// A line of `com.txt`: `<dialog> <line> <voice> <speaker> C<camera>
/// H<mood> [STAND<stand>]` (@ 0x0040c6fb .. 0x0040c992).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComLine {
    /// The dialog number (`atoi`).
    pub dialog: i32,
    /// The line number in the dialog (`atoi`).
    pub line: i32,
    /// The voice's name (record +0x38 of the entry, read by
    /// `CDialogue::GetApcNameFromDial` @ 0x004658e0).
    pub voice: String,
    /// The speaker, lower-cased (@ 0x0040c97f).
    pub speaker: String,
    /// The fifth word without its letters, `atoi` (@ 0x0040c750): the
    /// camera, 1-based: the `c<camera>` of the dialog video and the index
    /// into the speaker's `[FocaleN]` (@ 0x004685c0). 0 lets the loader
    /// pick one at random when it also picks the stand (@ 0x00464bea).
    pub camera: i32,
    /// The sixth word without its letters, `atoi` (@ 0x0040c7c8); 0 when
    /// the word is missing. It picks the head's expression key: `-base`
    /// for 0, `-h<mood>` otherwise (`PlayMorphingKey` @ 0x0047d930, via
    /// @ 0x00469870).
    pub mood: i32,
    /// `STAND<n>` (any case) as the seventh word: n, else 0
    /// (@ 0x0040c834 .. 0x0040c91d). 0 lets the loader pick a stand among
    /// the files that exist (@ 0x00464690): `<mesh>s<n>.db`, else
    /// `<mesh>s<n>c1.hnm`.
    pub stand: i32,
}

/// Reads `com.txt` (@ 0x0040c4bc .. 0x0040ca6c). Missing words read as
/// empty, so as 0.
pub fn parse_com(data: &[u8]) -> Vec<ComLine> {
    ContentLines::new(data)
        .map(|(_, line)| {
            let mut w = Reader::new(line);
            let dialog = atoi(w.word());
            let line = atoi(w.word());
            let voice = latin1(w.word());
            let speaker = latin1(&lower(w.word()));
            let camera = atoi_word(&strip_letters(w.word()));
            let mood = atoi_word(&strip_letters(w.word()));
            let s = lower(w.word());
            let stand = if s.starts_with(b"stand") {
                atoi_word(&strip_letters(&s))
            } else {
                0
            };
            ComLine {
                dialog,
                line,
                voice,
                speaker,
                camera,
                mood,
                stand,
            }
        })
        .collect()
}

/// A line of `dial.txt`: `#NEW <speaker> <voice> <dialog> <line> <text>`
/// (@ 0x0040ccdb .. 0x0040cd8e).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialLine {
    /// The speaker, lower-cased (@ 0x0040cd7e).
    pub speaker: String,
    /// The voice's name. The original reads it and drops it: the voice
    /// played is `com.txt`'s.
    pub voice: String,
    pub dialog: i32,
    pub line: i32,
    /// The subtitle: the rest of the line after the separators that follow
    /// the line number (so a text cannot start with a separator), trailing
    /// blanks kept, Windows-1252 bytes as the file has them.
    pub text: Vec<u8>,
}

/// `dial.txt` as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialTxt {
    pub lines: Vec<DialLine>,
    /// The lines whose first word is not exactly `#NEW`: the original shows
    /// an error box for each (@ 0x0040cd98) and skips it.
    pub rejected: usize,
}

/// Reads `dial.txt` (@ 0x0040cb10).
pub fn parse_dial(data: &[u8]) -> DialTxt {
    let mut out = DialTxt::default();
    for (_, line) in ContentLines::new(data) {
        let mut w = Reader::new(line);
        if w.word() != b"#NEW" {
            out.rejected += 1;
            continue;
        }
        let speaker = latin1(&lower(w.word()));
        let voice = latin1(w.word());
        let dialog = atoi(w.word());
        let number = atoi(w.word());
        out.lines.push(DialLine {
            speaker,
            voice,
            dialog,
            line: number,
            text: w.line().to_vec(),
        });
    }
    out
}

/// A dialog line as `CDialogue` holds it after the three files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogEntry {
    /// The `com.txt` line (the entry's fields @ 0x00463770: speaker, stand
    /// +0x14, camera +0x18, mood +0x1c, voice +0x38). A stand or camera of
    /// 0 is kept here; the original replaces it at load time by `rand()`
    /// among the files found (@ 0x00464690).
    pub com: ComLine,
    /// The `dial.txt` text, `None` while no line gave one (the original's
    /// default text @ 0x00463770, with the flag +0x34 clear).
    pub text: Option<Vec<u8>>,
}

/// What [`DialogScript::build`] dropped, as the original does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadIssue {
    /// A `com.txt` line whose speaker has no `[Perso]`: dropped without a
    /// message (the speaker lookup at the start of @ 0x00464690).
    UnknownComSpeaker { dialog: i32, line: i32 },
    /// A `dial.txt` line whose speaker has no `[Perso]` (message
    /// @ 0x0071c58c).
    UnknownDialSpeaker { dialog: i32, line: i32 },
    /// A `dial.txt` line whose dialog has no `com.txt` line (message
    /// @ 0x0071c53c).
    UnknownDialog { dialog: i32, line: i32 },
    /// A `dial.txt` line whose line number has no `com.txt` line (message
    /// @ 0x0071c4ec).
    UnknownLine { dialog: i32, line: i32 },
}

/// The dialog manager's data after `info.txt`, `com.txt` and `dial.txt`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DialogScript {
    /// The speakers, in file order.
    pub persos: Vec<Perso>,
    /// The lines by (dialog, line).
    pub entries: BTreeMap<(i32, i32), DialogEntry>,
    /// What was dropped, in file order.
    pub issues: Vec<LoadIssue>,
}

impl DialogScript {
    /// The three files in the original's order. Speakers are matched
    /// lower-cased. Repeated speakers, or repeated (dialog, line) keys, are
    /// `[UNKNOWN]` in the original (its maps' insertion rule is not
    /// studied; the shipped files have none): here the first speaker wins
    /// and a later key replaces an earlier one.
    pub fn build(persos: Vec<Perso>, com: &[ComLine], dial: &DialTxt) -> DialogScript {
        let mut out = DialogScript {
            persos,
            ..DialogScript::default()
        };
        for c in com {
            if out.perso(&c.speaker).is_none() {
                out.issues.push(LoadIssue::UnknownComSpeaker {
                    dialog: c.dialog,
                    line: c.line,
                });
                continue;
            }
            out.entries.insert(
                (c.dialog, c.line),
                DialogEntry {
                    com: c.clone(),
                    text: None,
                },
            );
        }
        for d in &dial.lines {
            let (dialog, line) = (d.dialog, d.line);
            if out.perso(&d.speaker).is_none() {
                out.issues
                    .push(LoadIssue::UnknownDialSpeaker { dialog, line });
            } else if !out.entries.keys().any(|&(n, _)| n == dialog) {
                out.issues.push(LoadIssue::UnknownDialog { dialog, line });
            } else if let Some(e) = out.entries.get_mut(&(dialog, line)) {
                e.text = Some(d.text.clone());
            } else {
                out.issues.push(LoadIssue::UnknownLine { dialog, line });
            }
        }
        out
    }

    /// The speaker named `name` (lower-case).
    pub fn perso(&self, name: &str) -> Option<&Perso> {
        self.persos.iter().find(|p| p.name == name)
    }

    /// The line `line` of dialog `dialog`.
    pub fn entry(&self, dialog: i32, line: i32) -> Option<&DialogEntry> {
        self.entries.get(&(dialog, line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &[u8] = b"// header\r\n\r\n[Perso]\t\tSOMEONE\r\n[Mesh]\t\tSo\r\n\
[Smooth]\t\t0.5\r\n[TabWeight00]\t2.0\r\n[TabWeight03]\t3.0\r\n[TabWeight08]\t0.25\r\n\
[Stretch]\t1.5\r\n[GlobalWeight]\t0.75\r\n[Focale1]\t\t60.0,50.0,40.0\r\n\
[Focale2]\t\t30.0 // c\r\n[IdText]\t\tA_B\r\n[IdText]\t\tC\r\n[FinPerso]\r\n\
stray line\r\n[Perso] Other\r\n[Mesh] Ot\r\n[FinPerso]";

    #[test]
    fn info_blocks() {
        let p = parse_info(INFO).unwrap();
        assert_eq!(p.len(), 2);
        let s = &p[0];
        assert_eq!((s.name.as_str(), s.mesh.as_str()), ("someone", "so"));
        assert_eq!((s.smooth, s.stretch, s.global_weight), (0.5, 1.5, 0.75));
        assert_eq!(s.id_texts, ["A_B", "C"]);
        assert_eq!(
            s.tab_weights,
            [0.25, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        );
        let slots: Vec<i32> = s.tab_weight_lines.iter().map(|l| l.slot).collect();
        assert_eq!(slots, [0, 0, 0]);
        let w = s.written_tab_weights();
        assert_eq!((w[0], w[3], w[8], w[1]), (2.0, 3.0, 0.25, 1.0));
        assert_eq!(s.focale1, [60.0, 50.0, 40.0]);
        // The two words of the comment add two zeros.
        assert_eq!(s.focale2, [30.0, 0.0, 0.0]);
        assert_eq!(s.focale(1, 2), 50.0);
        assert_eq!(s.focale(1, 4), DEFAULT_FOCALE);
        assert_eq!(s.focale(1, 0), DEFAULT_FOCALE);
        assert_eq!(s.focale(0, 1), DEFAULT_FOCALE);
        let o = &p[1];
        assert_eq!((o.name.as_str(), o.mesh.as_str()), ("other", "ot"));
        assert_eq!((o.smooth, o.tab_weights[0]), (1.0, 1.0));
        let bad = parse_info(b"[Perso] X\r\n[Mesh] x\r\n");
        assert_eq!(
            bad,
            Err(DialogError::UnterminatedPerso {
                name: "X".to_string()
            })
        );
    }

    #[test]
    fn com_dial_and_script() {
        let com = parse_com(
            b"10 1 SOME0001 SOMEONE C0 H2\r\n10 2 SOME0002 Someone C3 H0 STAND2\r\n\
// x\r\n11 1 NOBO0003 NOBODY C1 \r\n12 1 SOME0004 SOMEONE C0 H1 other\r\n",
        );
        assert_eq!(com.len(), 4);
        assert_eq!(
            com[0],
            ComLine {
                dialog: 10,
                line: 1,
                voice: "SOME0001".to_string(),
                speaker: "someone".to_string(),
                camera: 0,
                mood: 2,
                stand: 0,
            }
        );
        assert_eq!((com[1].camera, com[1].stand), (3, 2));
        assert_eq!((com[2].camera, com[2].mood), (1, 0));
        assert_eq!(com[3].stand, 0);
        let dial = parse_dial(
            b"#NEW SOMEONE SOME0001 10 1 First,  text.\r\n#NEW SOMEONE X 10 2   (T\xe9)\r\n\
#NEWS SOMEONE X 10 3 no\r\n#NEW NOBODY X 11 1 t\r\n#NEW SOMEONE X 13 1 t\r\n\
#NEW SOMEONE X 10 9 t\r\n",
        );
        assert_eq!(dial.rejected, 1);
        assert_eq!(dial.lines.len(), 5);
        assert_eq!(dial.lines[0].text, b"First,  text.");
        assert_eq!(dial.lines[1].text, b"T\xe9)");
        let persos = parse_info(INFO).unwrap();
        let s = DialogScript::build(persos, &com, &dial);
        assert_eq!(s.entries.len(), 3);
        assert_eq!(
            s.entry(10, 1).unwrap().text.as_deref(),
            Some(&b"First,  text."[..])
        );
        assert_eq!(s.entry(12, 1).unwrap().text, None);
        assert_eq!(
            s.issues,
            [
                LoadIssue::UnknownComSpeaker {
                    dialog: 11,
                    line: 1
                },
                LoadIssue::UnknownDialSpeaker {
                    dialog: 11,
                    line: 1
                },
                LoadIssue::UnknownDialog {
                    dialog: 13,
                    line: 1
                },
                LoadIssue::UnknownLine {
                    dialog: 10,
                    line: 9
                },
            ]
        );
        assert_eq!(dial_path("fra"), "Datas/loc/text/fra/dial.txt");
    }
}
