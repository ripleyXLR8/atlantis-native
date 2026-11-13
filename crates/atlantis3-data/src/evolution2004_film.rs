//! *Atlantis Evolution*'s 2004 program's films (`AtlBink`): the script's
//! `CINEMA=<name>,<options>` line, the `#CINEMATIQUES#` section of
//! `scenar\defnames.con` that turns the name into a `.bik` file, and the
//! files `AtlBink_Start` looks for.
//!
//! The RE note is atlantis4-re `subsystems/cd2004-films.md`; every rule
//! cites its address in atlantis4-cd-en-3022848.

/// The options of a `CINEMA=` line, as the `/set` interpreter reads them
/// (FUN_0044c850 @ 0x0044ccc6..0x0044d6c8).
#[derive(Clone, Debug, PartialEq)]
pub struct FilmOptions {
    /// The text after `=` up to the first `)` (copied to 0x006e01c8,
    /// @ 0x0044d43b..0x0044d482): the name is its leading run of letters,
    /// digits and `_`.
    pub text: Vec<u8>,
    /// `fps`: 25 by default (@ 0x0044cd89).
    pub fps: i32,
    /// `traj<n>`: a travel film of `binktraj\` (0 = none, @ 0x0044cd91).
    pub traj: i32,
    /// `nocinein` (@ 0x0044d138).
    pub nocinein: bool,
    /// `nocineout` (@ 0x0044d209).
    pub nocineout: bool,
    /// `cineout` (@ 0x0044d2da).
    pub cineout: bool,
    /// `%<x>`: the music's share kept during the film, 100 by default
    /// (@ 0x0044cdb8), clamped to 0..100 when stored in 0x006ac080
    /// (@ 0x0044d5ef..0x0044d61f).
    pub percent: f32,
}

/// The separators of the text tokenizer (FUN_0044ade0, the same as
/// *Atlantis III*'s atlantis3-gog-fr-3473408 @ 0x00445fc0).
fn is_separator(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'(' | b')')
}

/// FUN_0044a8a0: the digits of a word, the rest dropped.
fn digits(w: &[u8]) -> Vec<u8> {
    w.iter().copied().filter(u8::is_ascii_digit).collect()
}

impl FilmOptions {
    /// Reads the line from its `=` (`line` starts at the `=`, the text up
    /// to the end of the line).
    ///
    /// The words (separators of [`is_separator`]) are compared by their
    /// first letters with `basic_string`'s `==` (FUN_00449410, case
    /// sensitive): `fps` (3 letters, 0x006adffc), `traj` (4, 0x006afbe8),
    /// `nocinein` (8, 0x006afbdc), `nocineout` (9, 0x006afbd0), `cineout`
    /// (7, 0x006afbc8), `%` (1, 0x006afbc4). `fps` and `traj` take the
    /// `atoi` of the word's digits (FUN_0044a8a0 then FUN_00449a50), `%` the
    /// `atof` of them (FUN_00449a70). The first word starts with the `=`
    /// and never matches.
    pub fn parse(line: &[u8]) -> FilmOptions {
        let line = line
            .iter()
            .position(|&c| c == b'\r' || c == b'\n' || c == 0)
            .map_or(line, |e| &line[..e]);
        let value = line.strip_prefix(b"=").unwrap_or(line);
        let text_end = value.iter().position(|&c| c == b')').unwrap_or(value.len());
        let mut o = FilmOptions {
            text: value[..text_end].to_vec(),
            fps: 25,
            traj: 0,
            nocinein: false,
            nocineout: false,
            cineout: false,
            percent: 100.0,
        };
        for w in line.split(|&c| is_separator(c)).filter(|w| !w.is_empty()) {
            if w.starts_with(b"fps") {
                o.fps = atlantis2_data::con::atoi(&digits(w));
            }
            if w.starts_with(b"traj") {
                o.traj = atlantis2_data::con::atoi(&digits(w));
            }
            if w.starts_with(b"nocinein") {
                o.nocinein = true;
            }
            if w.starts_with(b"nocineout") {
                o.nocineout = true;
            }
            if w.starts_with(b"cineout") {
                o.cineout = true;
            }
            if w.starts_with(b"%") {
                let d = digits(w);
                o.percent = std::str::from_utf8(&d)
                    .ok()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0) as f32;
            }
        }
        o
    }

    /// The film's name: the leading run of letters, digits and `_` of the
    /// text, as `DefName_GetIdName` (FUN_00466f70) measures it.
    pub fn name(&self) -> &[u8] {
        let n = self
            .text
            .iter()
            .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
            .count();
        &self.text[..n]
    }

    /// The value stored in 0x006ac080: `%` clamped to 0..100
    /// (@ 0x0044d5ef..0x0044d61f).
    pub fn dial_percent(&self) -> f32 {
        self.percent.clamp(0.0, 100.0)
    }

    /// Whether the end fades out (0x006e01ec, @ 0x0044d5d1..0x0044d5e9):
    /// `cineout` given and `nocineout` not.
    pub fn cine_out(&self) -> bool {
        self.cineout && !self.nocineout
    }
}

/// The `#CINEMATIQUES#` section of `defnames.con` (section 6 of the
/// pointer table 0x006b0b6c): each entry's name and its first parameter,
/// the film's file name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cinematiques {
    /// (the name upper-cased, the first parameter).
    pub entries: Vec<(Vec<u8>, Vec<u8>)>,
}

/// The tag of the section (0x006b0bc0).
pub const TAG: &[u8] = b"#CINEMATIQUES#";

impl Cinematiques {
    /// Reads the section's entries: from the line that starts with the tag
    /// to the next line starting with `#`, the lines that are empty or
    /// start with `//` after blanks skipped (`DefName_FindFirst` /
    /// `FindNext`, FUN_00466a00 / FUN_00466bf0, the same as *Atlantis
    /// III*'s @ 0x004602f0 / 0x004604e0).
    ///
    /// The first parameter is the text after the line's first comma (a CR
    /// or LF before it: none, FUN_004677b0 with 1), blanks skipped,
    /// copied up to CR, LF, NUL, space, `,`, tab, `/` or `)`
    /// (`DefName_CopyParam` FUN_00467620).
    pub fn parse(defnames: &[u8]) -> Cinematiques {
        Self::parse_section(defnames, TAG)
    }

    /// [`Cinematiques::parse`] for the section that starts with `tag`
    /// (the 2004 program's sections 5..8 are read alike: `#SONS#`,
    /// [`crate::evolution2004_sfx`]).
    pub fn parse_section(defnames: &[u8], tag: &[u8]) -> Cinematiques {
        let mut out = Cinematiques::default();
        let mut inside = false;
        for raw in defnames.split(|&c| c == b'\n') {
            let line = raw.strip_suffix(b"\r").unwrap_or(raw);
            let t = {
                let s = line
                    .iter()
                    .position(|&c| c != b' ' && c != b'\t')
                    .unwrap_or(line.len());
                &line[s..]
            };
            if t.starts_with(b"#") {
                inside = t.starts_with(tag);
                continue;
            }
            if !inside || t.is_empty() || t.starts_with(b"//") {
                continue;
            }
            let n = t
                .iter()
                .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
                .count();
            if n == 0 {
                continue;
            }
            let name = t[..n].to_ascii_uppercase();
            let param = match t.iter().position(|&c| c == b',') {
                Some(c) => {
                    let p = &t[c + 1..];
                    let s = p
                        .iter()
                        .position(|&c| c != b' ' && c != b'\t')
                        .unwrap_or(p.len());
                    p[s..]
                        .iter()
                        .copied()
                        .take_while(|&c| {
                            !matches!(c, b'\r' | b'\n' | 0 | b' ' | b',' | b'\t' | b'/' | b')')
                        })
                        .collect()
                }
                None => Vec::new(),
            };
            out.entries.push((name, param));
        }
        out
    }

    /// `DefName_GetIdName(6, name)` then `GetParamFromId(6, id, 1)`
    /// (@ 0x0044d494..0x0044d4af): the file of the entry whose name has
    /// the query's length and equals it ignoring case (FUN_00467db0).
    /// `None` for an unknown name (the original logs `DefName_GetIdName :
    /// Nom de %s inconnu` and goes on with entry 0: not followed here,
    /// see the RE note).
    pub fn file_of(&self, name: &[u8]) -> Option<&[u8]> {
        if name.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, f)| f.as_slice())
    }
}

/// The files `AtlBink_Start` (FUN_00401d30) uses for a film that is not a
/// travel film, its fifth argument 0 (the script's call @ 0x0044d6c8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilmFiles {
    /// The subtitle cue file, `bink\<txt>` (FUN_004185d0 @ 0x00401db4),
    /// when it exists: read by `AtlBink_LoadUnderTitleFile`
    /// (FUN_00401590).
    pub cue: Option<String>,
    /// The film: `loc\bink\<Langue>\<file>` (FUN_00418350 @ 0x0040213e)
    /// when the cue file exists, else `bink\<file>` (FUN_004181e0
    /// @ 0x00401f85).
    pub video: String,
    /// The sound track: the film's path with its last three letters
    /// replaced by `wav` (0x006ac26c) when that file exists, else by `ogg`
    /// (0x006ea030, set to `ogg` @ 0x0042e830); `None` when neither
    /// exists (then Bink's own sound plays, @ 0x004026c4).
    pub sound: Option<String>,
    /// The caption file, `loc\text\<SousTitre>\<txt>` (FUN_00419880
    /// @ 0x004022ec), read by FUN_00401120 when it exists (not for a
    /// travel film).
    pub caption: String,
}

/// The cue file's name: the film's file with its last three letters
/// replaced by `txt` (@ 0x0044d625..0x0044d6a5).
pub fn txt_name(file: &str) -> String {
    let stem = &file[..file.len().saturating_sub(3)];
    format!("{stem}txt")
}

impl FilmFiles {
    /// The paths for `file` (from [`Cinematiques::file_of`]), relative to
    /// `Datas\` as [`crate::A3Files`] reads them; `exists` is the
    /// program's file test (FUN_0041ab50).
    pub fn resolve(
        file: &str,
        langue: &str,
        sous_titre: &str,
        exists: impl FnMut(&str) -> bool,
    ) -> FilmFiles {
        FilmFiles::resolve_loc(file, langue, sous_titre, false, exists)
    }

    /// [`FilmFiles::resolve`] with `AtlBink_Start`'s fifth argument `loc`
    /// (a cue file found *or* the argument set → the film, @ 0x0040213e,
    /// from `loc\bink\<Langue>\`); the start-up's logos pass 1.
    pub fn resolve_loc(
        file: &str,
        langue: &str,
        sous_titre: &str,
        loc: bool,
        exists: impl FnMut(&str) -> bool,
    ) -> FilmFiles {
        FilmFiles::resolve_bink(
            file,
            &txt_name(file),
            langue,
            sous_titre,
            false,
            loc,
            exists,
        )
    }

    /// `AtlBink_Start(file, txt, fps, traj, loc)`'s paths (FUN_00401d30
    /// §3 of the note) with its own cue name `txt` and travel flag `traj`:
    /// the cue `bink\<txt>` (@ 0x00401db4); a cue found or `loc` → the
    /// film `locink\<Langue>\<file>` (@ 0x0040213e); else a travel film
    /// takes `file` as given (the caller's full `binktraj\` path,
    /// FUN_00418740), else `bink\<file>` (@ 0x00401f85). The caption
    /// (@ 0x0040232e) is read only when not `traj`. The moves pass an
    /// empty `txt` (the empty string 0x006e0218, @ 0x00444098): `bink\`
    /// is a folder, not a file (no cue).
    pub fn resolve_bink(
        file: &str,
        txt: &str,
        langue: &str,
        sous_titre: &str,
        traj: bool,
        loc: bool,
        mut exists: impl FnMut(&str) -> bool,
    ) -> FilmFiles {
        let cue_path = format!("bink/{txt}");
        let cue = (!txt.is_empty() && exists(&cue_path)).then_some(cue_path);
        let video = if cue.is_some() || loc {
            format!("loc/bink/{langue}/{file}")
        } else if traj {
            file.to_string()
        } else {
            format!("bink/{file}")
        };
        let stem = &video[..video.len().saturating_sub(3)];
        let wav = format!("{stem}wav");
        let ogg = format!("{stem}ogg");
        let sound = if exists(&wav) {
            Some(wav)
        } else if exists(&ogg) {
            Some(ogg)
        } else {
            None
        };
        FilmFiles {
            cue,
            video,
            sound,
            caption: format!("loc/text/{sous_titre}/{txt}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_intro_line() {
        let o = FilmOptions::parse(b"=INTRO,nocinein,nocineout)\r\n/set(x=1)");
        assert_eq!(o.text, b"INTRO,nocinein,nocineout");
        assert_eq!(o.name(), b"INTRO");
        assert_eq!((o.fps, o.traj), (25, 0));
        assert!(o.nocinein && o.nocineout && !o.cineout);
        assert_eq!(o.dial_percent(), 100.0);
        assert!(!o.cine_out());
    }

    #[test]
    fn options_with_digits() {
        let o = FilmOptions::parse(b"=TRAJECTOIRE_CINTRE2,nocinein,nocineout,traj004)");
        assert_eq!(o.traj, 4);
        let o = FilmOptions::parse(b"=CHEL_TO_BLOC,nocineout,%25)");
        assert_eq!(o.dial_percent(), 25.0);
        let o = FilmOptions::parse(b"=X,%250,fps=15,cineout)");
        assert_eq!((o.dial_percent(), o.fps), (100.0, 15));
        assert!(o.cine_out());
        // Case sensitive: `NOCINEIN` is not the option.
        assert!(!FilmOptions::parse(b"=X,NOCINEIN)").nocinein);
    }

    #[test]
    fn the_section() {
        let d = b"#VARIABLES#\r\nINTRO\r\n#CINEMATIQUES#\r\n// c\r\n\r\n\
                  ARRIVEE_QUICK_C17,arriveec17.bik\r\n  intro , intro.bik // x\r\n\
                  #PUZZLES#\r\nINTRO2,no.bik\r\n";
        let c = Cinematiques::parse(d);
        assert_eq!(c.entries.len(), 2);
        assert_eq!(c.file_of(b"INTRO"), Some(&b"intro.bik"[..]));
        assert_eq!(
            c.file_of(b"arrivee_quick_c17"),
            Some(&b"arriveec17.bik"[..])
        );
        assert_eq!(c.file_of(b"INTRO2"), None);
        assert_eq!(c.file_of(b""), None);
    }

    #[test]
    fn the_paths() {
        let have = ["bink/intro.txt", "loc/bink/gbr/intro.ogg"];
        let f = FilmFiles::resolve("intro.bik", "gbr", "gbr", |p| have.contains(&p));
        assert_eq!(f.cue.as_deref(), Some("bink/intro.txt"));
        assert_eq!(f.video, "loc/bink/gbr/intro.bik");
        assert_eq!(f.sound.as_deref(), Some("loc/bink/gbr/intro.ogg"));
        assert_eq!(f.caption, "loc/text/gbr/intro.txt");
        let f = FilmFiles::resolve("jaws.bik", "gbr", "gbr", |_| false);
        assert_eq!(
            (f.cue, f.video.as_str(), f.sound),
            (None, "bink/jaws.bik", None)
        );
    }

    #[test]
    fn the_travel_paths() {
        // `CINEMA=TRAJECTOIRE_CINTRE1,..,traj006` (chapi018): the film as
        // given, its Ogg track beside it.
        let have = ["binktraj/trajcintre1.ogg"];
        let f = FilmFiles::resolve_bink(
            "binktraj/trajcintre1.bik",
            "trajcintre1.txt",
            "gbr",
            "gbr",
            true,
            false,
            |p| have.contains(&p),
        );
        assert_eq!(f.cue, None);
        assert_eq!(f.video, "binktraj/trajcintre1.bik");
        assert_eq!(f.sound.as_deref(), Some("binktraj/trajcintre1.ogg"));
        // A move's film: no cue name.
        let f = FilmFiles::resolve_bink(
            "binktraj/ka001002.bik",
            "",
            "gbr",
            "gbr",
            true,
            false,
            |p| p == "bink/",
        );
        assert_eq!((f.cue, f.video.as_str()), (None, "binktraj/ka001002.bik"));
    }
}
