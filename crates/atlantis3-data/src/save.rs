//! The saved games: `Datas\save\game<P>_<N>.sav` (text) and their
//! thumbnails `Datas\menu\spr_<W>\32bits\save\game<P>_<N>.sav.spr`.
//!
//! The writer (@ 0x00459ee0) writes through a binary file stream, so lines
//! end in a bare LF:
//!
//! ```text
//! CurrentCD = <int>
//! DateAndTime = MM/DD/YY HH:MM:SS
//! Wam = <file name of the place>
//! Monde = <int>
//! Vue = <int>
//! Chapitre = <int>
//! NBFlag = <count>
//! <index> = <value>             (count lines: the non-zero flags)
//! NBObjet = <count>
//! <object name>                 (count lines: the inventory)
//! //== Sauvegarde des dialogues ==
//! NBPersoAvecDial = <count>
//! Perso = <0..255>              (count blocks of:)
//! NBSuj = <count>
//! Etat = <0|1>                  (count triples)
//! ID = <int>
//! NBSprite = <int>
//! Quaternion0 = %f              (..3: the camera's orientation)
//! ```
//!
//! The loader (@ 0x0045b750) reads by position, not by key: each value is
//! the text two bytes after the line's first `=`. Flag lines are read up to
//! the `=` for the index and up to the end of line for the value; object
//! names keep only their letters (@ 0x00445d00). The dialog section is read
//! @ 0x0043cac0. The rest of the file is then read with the tokenizer: lines
//! starting with `//` are skipped and `QuaternionK = v` lines set the
//! camera, in any order.
//!
//! A save made from the menu gets three thumbnails (85 x 65, 65 x 50 and
//! 52 x 40 pixels for the 1024, 800 and 640 menus): `SPRCRYO1` followed by
//! the sprite the encoder @ 0x0069c390 makes from a stretched copy of the
//! screen (@ 0x00410500). The script's quick saves (`quick<P>_<N>.sav`)
//! get none. The thumbnail bytes are left opaque here.

use crate::atl::{atof, atoi, TextCursor};

/// The folder of the saves (`save\` @ 0x00715ad0 under `Datas\`).
pub const SAVE_DIR: &str = "Datas/save";

/// The magic the writer puts before a thumbnail (@ 0x0071b8d4).
pub const THUMB_MAGIC: &[u8; 8] = b"SPRCRYO1";

/// The thumbnails: menu width, sprite width and height (the surfaces made
/// @ 0x0040edb0, in the order the writer writes them).
pub const THUMBNAILS: [(u32, u32, u32); 3] = [(1024, 85, 65), (800, 65, 50), (640, 52, 40)];

/// The flag table (DAT_0077c4ec..0x0077e4ec): 2048 ints.
pub const FLAG_COUNT: usize = 2048;

/// The dialog table (DAT_0076e960): 256 characters of 16 subjects.
pub const PERSO_COUNT: usize = 256;
/// Subjects per character (0xc4-byte record: a count and 16 x 12 bytes).
pub const SUBJECTS_PER_PERSO: usize = 16;

/// The header line of the dialog section (@ 0x0071824c).
pub const DIALOG_HEADER: &[u8] = b"//== Sauvegarde des dialogues ==";

/// The player number of the file names (DAT_0071b890), 1 unless changed.
pub const DEFAULT_PLAYER: i32 = 1;

/// The name of a menu save: `game<player>_<slot>.sav` (@ 0x0045e300 takes
/// the first slot from 1 up whose file does not exist; @ 0x0045e6c0 the
/// last one that does).
pub fn game_save_name(player: i32, slot: i32) -> String {
    format!("game{player}_{slot}.sav")
}

/// The name of a script quick save: `quick<player>_<n>.sav` (@ 0x0044c6d0).
pub fn quick_save_name(player: i32, n: i32) -> String {
    format!("quick{player}_{n}.sav")
}

/// The path of save `name` in the install.
pub fn save_path(name: &str) -> String {
    format!("{SAVE_DIR}/{name}")
}

/// The path of the thumbnail of save `name` for the menus of width
/// `menu_width` (@ 0x0045b134..0x0045b46a: the sprite path @ 0x00418060 of
/// `save\<name>.spr`).
pub fn thumbnail_path(menu_width: u32, name: &str) -> String {
    format!("Datas/menu/spr_{menu_width}/32bits/save/{name}.spr")
}

/// The letters of `s` only (@ 0x00445d00: `A..Z`, `a..z`).
pub fn letters_only(s: &[u8]) -> Vec<u8> {
    s.iter().copied().filter(u8::is_ascii_alphabetic).collect()
}

/// Whether the writer makes thumbnails for save `name`: not when the
/// letters of `save\<name>` are `savequicksav` (@ 0x0045b0c6).
pub fn has_thumbnail(name: &str) -> bool {
    let mut path = b"save\\".to_vec();
    path.extend_from_slice(name.as_bytes());
    letters_only(&path) != b"savequicksav"
}

/// `DateAndTime`: `_strdate` and `_strtime` of the local time (@ 0x006d3a0b,
/// 0x006d3a8f), `MM/DD/YY HH:MM:SS`. Read back by nobody.
pub fn date_and_time(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Vec<u8> {
    format!(
        "{:02}/{:02}/{:02} {:02}:{:02}:{:02}",
        month % 100,
        day % 100,
        year % 100,
        hour % 100,
        minute % 100,
        second % 100
    )
    .into_bytes()
}

/// One subject of a character's dialogs (12 bytes from DAT_0076e964).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subject {
    /// `Etat`: the byte at +8, written 1 when non-zero, read as `== 1`.
    pub etat: bool,
    /// `ID`: the int at +4.
    pub id: i32,
    /// `NBSprite`: the int at +0.
    pub nb_sprite: i32,
}

/// The dialogs of one character with at least one subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersoDialogs {
    /// The character, 0..255.
    pub perso: i32,
    /// Its subjects (the count is `NBSuj`; at most 16 fit the table).
    pub subjects: Vec<Subject>,
}

/// A saved game.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveGame {
    /// `CurrentCD` (DAT_00789d38): the loader runs `(changecd=<v>)`.
    pub current_cd: i32,
    /// `DateAndTime`: written, skipped by the loader.
    pub date_and_time: Vec<u8>,
    /// `Wam` (DAT_0077ade0): the place file, e.g. `<world>.wam`; the loader
    /// loads it (@ 0x0043eb00) and runs the place's own set-up.
    pub wam: Vec<u8>,
    /// `Monde` (DAT_0077bd04): the loader runs `(monde=<v>)`.
    pub monde: i32,
    /// `Vue` (DAT_0077b9c8): the loader runs `(Vue=<v>)` at the end.
    pub vue: i32,
    /// `Chapitre` (DAT_0077e508; the previous one goes to DAT_0077e4f4).
    pub chapitre: i32,
    /// The flag lines `index = value`, in file order (the writer: every
    /// non-zero flag in index order). The loader clears the 2048 flags
    /// first and does not check the index.
    pub flags: Vec<(i32, i32)>,
    /// The inventory's object names, in order (the list DAT_0075658c).
    pub objects: Vec<Vec<u8>>,
    /// The dialog section.
    pub dialogs: Vec<PersoDialogs>,
    /// `Quaternion0..3` (DAT_0077b678..0x0077b684, f32): the camera; the
    /// loader sets it only when all four were found.
    pub quaternion: [Option<f32>; 4],
}

impl SaveGame {
    /// Reads a save as the loader does (@ 0x0045b750). It never fails: a
    /// short file gives empty lines, read as 0 or empty (the original
    /// would read past the end).
    pub fn parse(data: &[u8]) -> SaveGame {
        let mut r = Lines { data, pos: 0 };
        let current_cd = atoi(value(&r.line()));
        let date_and_time = value(&r.line()).to_vec();
        let wam = value(&r.line()).to_vec();
        let monde = atoi(value(&r.line()));
        let vue = atoi(value(&r.line()));
        let chapitre = atoi(value(&r.line()));
        let nb_flag = atoi(value(&r.line()));
        let mut flags = Vec::new();
        for _ in 0..nb_flag.max(0) {
            let index = atoi(&r.until(b'='));
            let v = atoi(&r.until(b'\n'));
            flags.push((index, v));
        }
        let nb_objet = atoi(value(&r.line()));
        let mut objects = Vec::new();
        for _ in 0..nb_objet.max(0) {
            objects.push(letters_only(&r.line()));
        }
        // @ 0x0043cac0: the header line, then the counts.
        r.line();
        let nb_perso = atoi(value(&r.line()));
        let mut dialogs = Vec::new();
        for _ in 0..nb_perso.max(0) {
            let perso = atoi(value(&r.line()));
            let nb_suj = atoi(value(&r.line()));
            let mut subjects = Vec::new();
            for _ in 0..nb_suj.max(0) {
                let etat = atoi(value(&r.line())) == 1;
                let id = atoi(value(&r.line()));
                let nb_sprite = atoi(value(&r.line()));
                subjects.push(Subject {
                    etat,
                    id,
                    nb_sprite,
                });
            }
            dialogs.push(PersoDialogs { perso, subjects });
        }
        // "Chargement de la suite du fichier": the rest with the tokenizer.
        let mut quaternion = [None; 4];
        let mut f = TextCursor::new(&data[r.pos.min(data.len())..]);
        while !f.eof() {
            let line = f.line();
            if line.starts_with(b"//") {
                continue;
            }
            let mut l = TextCursor::new(line);
            let k = match l.token() {
                b"Quaternion0" => 0,
                b"Quaternion1" => 1,
                b"Quaternion2" => 2,
                b"Quaternion3" => 3,
                _ => continue,
            };
            l.skip_token();
            quaternion[k] = Some(atof(l.token()) as f32);
        }
        SaveGame {
            current_cd,
            date_and_time,
            wam,
            monde,
            vue,
            chapitre,
            flags,
            objects,
            dialogs,
            quaternion,
        }
    }

    /// The file the writer (@ 0x00459ee0) makes. Missing quaternion
    /// components are written as 0.
    pub fn write(&self) -> Vec<u8> {
        let mut s = Vec::new();
        let mut kv = |key: &str, v: &[u8]| {
            s.extend_from_slice(key.as_bytes());
            s.extend_from_slice(v);
            s.push(b'\n');
        };
        kv("CurrentCD = ", self.current_cd.to_string().as_bytes());
        kv("DateAndTime = ", &self.date_and_time);
        kv("Wam = ", &self.wam);
        kv("Monde = ", self.monde.to_string().as_bytes());
        kv("Vue = ", self.vue.to_string().as_bytes());
        kv("Chapitre = ", self.chapitre.to_string().as_bytes());
        kv("NBFlag = ", self.flags.len().to_string().as_bytes());
        for (i, v) in &self.flags {
            kv(&format!("{i} = "), v.to_string().as_bytes());
        }
        kv("NBObjet = ", self.objects.len().to_string().as_bytes());
        for o in &self.objects {
            kv("", o);
        }
        kv("", DIALOG_HEADER);
        kv(
            "NBPersoAvecDial = ",
            self.dialogs.len().to_string().as_bytes(),
        );
        for p in &self.dialogs {
            kv("Perso = ", p.perso.to_string().as_bytes());
            kv("NBSuj = ", p.subjects.len().to_string().as_bytes());
            for sj in &p.subjects {
                kv("Etat = ", if sj.etat { b"1" } else { b"0" });
                kv("ID = ", sj.id.to_string().as_bytes());
                kv("NBSprite = ", sj.nb_sprite.to_string().as_bytes());
            }
        }
        for (k, q) in self.quaternion.iter().enumerate() {
            let text = format_f(f64::from(q.unwrap_or(0.0)));
            kv(&format!("Quaternion{k} = "), text.as_bytes());
        }
        s
    }

    /// The camera, when the four components were read.
    pub fn camera(&self) -> Option<[f32; 4]> {
        let [a, b, c, d] = self.quaternion;
        Some([a?, b?, c?, d?])
    }

    /// The flag table the loader builds: all 0, then each line's value at
    /// its index (indices outside 0..2048, which the original writes out of
    /// its table, are dropped).
    pub fn flag_table(&self) -> Vec<i32> {
        let mut t = vec![0; FLAG_COUNT];
        for &(i, v) in &self.flags {
            match usize::try_from(i).ok().and_then(|i| t.get_mut(i)) {
                Some(slot) => *slot = v,
                None => log::warn!("save: flag index {i} out of the table"),
            }
        }
        t
    }

    /// The flag lines the writer makes from a table: the non-zero entries
    /// in index order.
    pub fn set_flags_from_table(&mut self, table: &[i32]) {
        self.flags = table
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| (v != 0).then_some((i as i32, v)))
            .collect();
    }
}

/// The loader's byte-by-byte line reader (@ 0x00418a20 one byte at a time,
/// compared with LF): a CR stays in the line.
struct Lines<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Lines<'_> {
    /// The bytes up to `end` (consumed, not returned) or the end of data.
    fn until(&mut self, end: u8) -> Vec<u8> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let n = rest.iter().position(|&b| b == end).unwrap_or(rest.len());
        self.pos += (n + 1).min(rest.len());
        rest[..n].to_vec()
    }

    fn line(&mut self) -> Vec<u8> {
        self.until(b'\n')
    }
}

/// The value of a `key = value` line: from two bytes after the first `=`
/// to the end (@ 0x004458a0, 0x00445480); without `=`, from the second
/// byte.
fn value(line: &[u8]) -> &[u8] {
    let start = line.iter().position(|&b| b == b'=').map_or(1, |p| p + 2);
    line.get(start..).unwrap_or(&[])
}

/// `printf("%f")` of the original's C library: six decimals. The digits are
/// those of the 17 significant digits of `v`, rounded half up at the sixth
/// decimal `[INFERRED: the classic MSVC conversion; ties such as 0.0078125
/// are where it differs from Rust's exact rounding]`.
pub fn format_f(v: f64) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-1.#IND00"
        } else {
            "1.#QNAN0"
        }
        .to_string();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-1.#INF00" } else { "1.#INF00" }.to_string();
    }
    let neg = v.is_sign_negative();
    let a = v.abs();
    // value = 0.d0 d1 d2 ... x 10^point
    let (mut digits, point): (Vec<u8>, i32) = if a == 0.0 {
        (vec![0], 1)
    } else {
        let sci = format!("{a:.16e}");
        let (m, e) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
        let digits = m
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|c| c - b'0')
            .collect();
        (digits, e.parse::<i32>().unwrap_or(0) + 1)
    };
    let keep = point + 6;
    if keep < 0 {
        digits.clear();
    } else {
        let keep = keep as usize;
        if digits.len() > keep {
            let up = digits[keep] >= 5;
            digits.truncate(keep);
            if up {
                let mut carry = true;
                for d in digits.iter_mut().rev() {
                    if *d == 9 {
                        *d = 0;
                    } else {
                        *d += 1;
                        carry = false;
                        break;
                    }
                }
                if carry {
                    digits.insert(0, 1);
                }
            }
        } else {
            digits.resize(keep, 0);
        }
    }
    let n = digits.len();
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    if n > 6 {
        s.extend(digits[..n - 6].iter().map(|&d| char::from(b'0' + d)));
    } else {
        s.push('0');
    }
    s.push('.');
    s.push_str(&"0".repeat(6usize.saturating_sub(n)));
    s.extend(
        digits[n.saturating_sub(6)..]
            .iter()
            .map(|&d| char::from(b'0' + d)),
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveGame {
        SaveGame {
            current_cd: 1,
            date_and_time: date_and_time(2001, 3, 9, 14, 5, 7),
            wam: b"place.wam".to_vec(),
            monde: 2,
            vue: 17,
            chapitre: 104,
            flags: vec![(3, 1), (40, -2), (2047, 9)],
            objects: vec![b"ObjetA".to_vec(), b"ObjetB".to_vec()],
            dialogs: vec![
                PersoDialogs {
                    perso: 0,
                    subjects: vec![Subject {
                        etat: true,
                        id: 5,
                        nb_sprite: 2,
                    }],
                },
                PersoDialogs {
                    perso: 255,
                    subjects: vec![
                        Subject {
                            etat: false,
                            id: -1,
                            nb_sprite: 0,
                        },
                        Subject {
                            etat: true,
                            id: 7,
                            nb_sprite: 3,
                        },
                    ],
                },
            ],
            quaternion: [Some(0.5), Some(-0.25), Some(0.0), Some(0.8125)],
        }
    }

    #[test]
    fn writer_text() {
        let text = sample().write();
        let expected: &[u8] = b"CurrentCD = 1\nDateAndTime = 03/09/01 14:05:07\nWam = place.wam\n\
Monde = 2\nVue = 17\nChapitre = 104\nNBFlag = 3\n3 = 1\n40 = -2\n2047 = 9\n\
NBObjet = 2\nObjetA\nObjetB\n//== Sauvegarde des dialogues ==\nNBPersoAvecDial = 2\n\
Perso = 0\nNBSuj = 1\nEtat = 1\nID = 5\nNBSprite = 2\n\
Perso = 255\nNBSuj = 2\nEtat = 0\nID = -1\nNBSprite = 0\nEtat = 1\nID = 7\nNBSprite = 3\n\
Quaternion0 = 0.500000\nQuaternion1 = -0.250000\nQuaternion2 = 0.000000\n\
Quaternion3 = 0.812500\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn round_trip() {
        let s = sample();
        let back = SaveGame::parse(&s.write());
        assert_eq!(back, s);
        assert_eq!(back.camera(), Some([0.5, -0.25, 0.0, 0.8125]));
        let t = back.flag_table();
        assert_eq!((t[3], t[40], t[2047], t[0]), (1, -2, 9, 0));
        let mut s2 = s.clone();
        s2.set_flags_from_table(&t);
        assert_eq!(s2.flags, s.flags);
    }

    #[test]
    fn loader_quirks() {
        // Keys are not checked; values start two bytes after `=`; object
        // names keep their letters; a missing quaternion gives no camera.
        let g = SaveGame::parse(
            b"x=_7\nignored\nW = a.wam\nM = 3\nV = 4\nC = 5\nN = 1\n12 = 6\nN = 1\nab-c1d\n\
//== x\nN = 0\n// Quaternion0 = 1\nQuaternion1 = 0.5\n",
        );
        assert_eq!(g.current_cd, 7);
        assert_eq!(g.wam, b"a.wam");
        assert_eq!(g.flags, vec![(12, 6)]);
        assert_eq!(g.objects, vec![b"abcd".to_vec()]);
        assert_eq!(g.quaternion, [None, Some(0.5), None, None]);
        assert_eq!(g.camera(), None);
    }

    #[test]
    fn names_and_numbers() {
        assert_eq!(save_path(&game_save_name(1, 3)), "Datas/save/game1_3.sav");
        assert_eq!(
            thumbnail_path(800, "game1_3.sav"),
            "Datas/menu/spr_800/32bits/save/game1_3.sav.spr"
        );
        assert!(has_thumbnail(&game_save_name(1, 3)));
        assert!(!has_thumbnail(&quick_save_name(1, 2)));
        assert_eq!(format_f(1.0), "1.000000");
        assert_eq!(format_f(-0.0), "-0.000000");
        assert_eq!(format_f(0.0000004), "0.000000");
        assert_eq!(format_f(0.0000006), "0.000001");
        assert_eq!(format_f(0.0078125), "0.007813");
        assert_eq!(format_f(f64::from(0.1f32)), "0.100000");
        assert_eq!(format_f(-123.4567891), "-123.456789");
        assert_eq!(format_f(9.9999999), "10.000000");
    }
}
