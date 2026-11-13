//! *Atlantis Evolution*'s 2004 program's saved games (atlantis4-cd-en-3022848):
//! `Datas\save\game<P>_<N>.sav` (text) and one thumbnail
//! `menu\spr_1024\32bits\save\game<P>_<N>.sav.spr` on the data disc.
//!
//! The writer FUN_0045f080 (log `-- Debut de la sauvegarde --`) writes,
//! through the AtlFile layer (FUN_0041a3f0, binary, LF), *Atlantis III*'s
//! lines ([`crate::save`]) with three additions: the 300 script variables
//! (0x0070d4f8, one bare value per line) after `Chapitre`, and after the
//! dialog section (FUN_0043eb60, *Atlantis III*'s @ 0x0043bc00 unchanged)
//! the sprite hide table, the zone table and the full-screen sprite:
//!
//! ```text
//! CurrentCD = <int>                 (0x00719d70)
//! DateAndTime = MM/DD/YY HH:MM:SS   (_strdate, _strtime)
//! Wam = <place file>                (FUN_00441a30)
//! Monde = <int>                     (0x0070d3f4)
//! Vue = <int>                       (0x0070d1d4)
//! Chapitre = <int>                  (0x0070fbf8)
//! <int>                             (300 lines: the variables 0x0070d4f8..)
//! NBFlag = <count>
//! <index> = <value>                 (the non-zero flags of 0x0070dbdc)
//! NBObjet = <count>
//! <object name>                     (the list FUN_0042b860 copies)
//! //== Sauvegarde des dialogues ==
//! NBPersoAvecDial = <count>         (… as Atlantis III)
//! NBSpwHideTable = %d               (0x006ffc18, entries 0x006fcc18, 12 bytes)
//! Id = %d / Type = %d / Etat = %d   (per entry: +0, +8, +4)
//! NBZoneTable = %d                  (0x006e7c44, entries 0x006e7418, 16 bytes)
//! Id = %d / Type = %d / Vue = %d / Etat = %d   (+0, +0xc, +4, +8)
//! SprPleinEcran = %s                (the CString 0x0070c970)
//! Quaternion0 = %f .. Quaternion3   (FUN_00445ca0: 0x0070cdd0..)
//! ```
//!
//! The loader FUN_00460b50 (log `-- Debut Load --`) reads the same order:
//! `LOADING` := 1, then `CurrentCD` (`Menu_TestChangeCD`, FUN_00452310),
//! `Wam` (loaded by FUN_00441a50), `Monde` (`(monde=n)` run), `Chapitre`
//! (the chapter loaded, its `/INIT` run), the hide table cleared, `(Vue=n)`
//! run, the 300 variables (cleared, then each line's `atoi`), the flags
//! (cleared, then each `index = value`), the objects (FUN_0042b960 clears
//! the list; each name looked up in the defnames and added, FUN_0042a980),
//! the dialog section (FUN_0043fa20, *Atlantis III*'s @ 0x0043cac0), the
//! two tables, the full-screen sprite and the quaternion. How the loader
//! reads the tables' and the sprite's lines past their counts is not fully
//! read `[UNKNOWN]`: this reader takes them in the writer's order.

use crate::atl::{atof, atoi, TextCursor};
use crate::save::{format_f, letters_only, value, Lines, PersoDialogs, Subject, DIALOG_HEADER};

/// The number of script variables saved (0x0070d4f8 .. 0x0070d9a8).
pub const VAR_COUNT: usize = 300;

/// The thumbnail's menu width: the writer builds one path with `1024`
/// (0x006b0860, @ 0x00460712) through the `menu\spr_` builder FUN_00419510.
pub const THUMB_MENU_WIDTH: usize = 1024;

/// The thumbnail's size: the surface 0x006e70dc made 85 x 65 in x8r8g8b8
/// (@ 0x004118c9..0x004118f6).
pub const THUMB_SIZE: (usize, usize) = (85, 65);

/// One entry of the sprite hide table (FUN_00466460 fills it: id, state,
/// type).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HideEntry {
    pub id: i32,
    pub kind: i32,
    pub etat: i32,
}

/// One entry of the zone table (0x006e7418, 16 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZoneEntry {
    pub id: i32,
    pub kind: i32,
    pub vue: i32,
    pub etat: i32,
}

/// A 2004 saved game.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveGame2004 {
    pub current_cd: i32,
    pub date_and_time: Vec<u8>,
    pub wam: Vec<u8>,
    pub monde: i32,
    pub vue: i32,
    pub chapitre: i32,
    /// The 300 variables, in index order.
    pub vars: Vec<i32>,
    /// The flag lines `index = value` (the non-zero flags in index order).
    pub flags: Vec<(i32, i32)>,
    pub objects: Vec<Vec<u8>>,
    pub dialogs: Vec<PersoDialogs>,
    pub hide_table: Vec<HideEntry>,
    pub zone_table: Vec<ZoneEntry>,
    /// `SprPleinEcran`: the full-screen sprite's name (empty: none).
    pub spr_plein_ecran: Vec<u8>,
    pub quaternion: [Option<f32>; 4],
}

impl Default for SaveGame2004 {
    fn default() -> Self {
        SaveGame2004 {
            current_cd: 1,
            date_and_time: Vec::new(),
            wam: Vec::new(),
            monde: 0,
            vue: 0,
            chapitre: 0,
            vars: vec![0; VAR_COUNT],
            flags: Vec::new(),
            objects: Vec::new(),
            dialogs: Vec::new(),
            hide_table: Vec::new(),
            zone_table: Vec::new(),
            spr_plein_ecran: Vec::new(),
            quaternion: [None; 4],
        }
    }
}

impl SaveGame2004 {
    /// The writer's text (FUN_0045f080).
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
        // @ 0x0045f9c3..0x0045fa97: every variable, `value` + `\n`.
        for k in 0..VAR_COUNT {
            let v = self.vars.get(k).copied().unwrap_or(0);
            kv("", v.to_string().as_bytes());
        }
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
        // @ 0x0046033e..0x004604f1: `sprintf` lines.
        kv(
            "NBSpwHideTable = ",
            self.hide_table.len().to_string().as_bytes(),
        );
        for e in &self.hide_table {
            kv("Id = ", e.id.to_string().as_bytes());
            kv("Type = ", e.kind.to_string().as_bytes());
            kv("Etat = ", e.etat.to_string().as_bytes());
        }
        kv(
            "NBZoneTable = ",
            self.zone_table.len().to_string().as_bytes(),
        );
        for e in &self.zone_table {
            kv("Id = ", e.id.to_string().as_bytes());
            kv("Type = ", e.kind.to_string().as_bytes());
            kv("Vue = ", e.vue.to_string().as_bytes());
            kv("Etat = ", e.etat.to_string().as_bytes());
        }
        kv("SprPleinEcran = ", &self.spr_plein_ecran);
        for (k, q) in self.quaternion.iter().enumerate() {
            let text = format_f(f64::from(q.unwrap_or(0.0)));
            kv(&format!("Quaternion{k} = "), text.as_bytes());
        }
        s
    }

    /// Reads a save in the loader's order (FUN_00460b50): the values two
    /// bytes after a line's `=`, the variables as whole lines, the flags up
    /// to the `=` and to the line's end, the objects' letters only; the
    /// quaternion lines by key in what follows. A short file reads as 0 or
    /// empty.
    pub fn parse(data: &[u8]) -> SaveGame2004 {
        let mut r = Lines::new(data);
        let current_cd = atoi(value(&r.line()));
        let date_and_time = value(&r.line()).to_vec();
        let wam = value(&r.line()).to_vec();
        let monde = atoi(value(&r.line()));
        let vue = atoi(value(&r.line()));
        let chapitre = atoi(value(&r.line()));
        let vars = (0..VAR_COUNT).map(|_| atoi(&r.line())).collect();
        let nb_flag = atoi(value(&r.line()));
        let mut flags = Vec::new();
        for _ in 0..nb_flag.max(0) {
            let index = atoi(&r.until(b'='));
            let v = atoi(&r.until(b'\n'));
            flags.push((index, v));
        }
        let nb_objet = atoi(value(&r.line()));
        let objects = (0..nb_objet.max(0))
            .map(|_| letters_only(&r.line()))
            .collect();
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
        let nb_hide = atoi(value(&r.line()));
        let hide_table = (0..nb_hide.clamp(0, 0x400))
            .map(|_| HideEntry {
                id: atoi(value(&r.line())),
                kind: atoi(value(&r.line())),
                etat: atoi(value(&r.line())),
            })
            .collect();
        let nb_zone = atoi(value(&r.line()));
        let zone_table = (0..nb_zone.max(0))
            .map(|_| ZoneEntry {
                id: atoi(value(&r.line())),
                kind: atoi(value(&r.line())),
                vue: atoi(value(&r.line())),
                etat: atoi(value(&r.line())),
            })
            .collect();
        let spr_plein_ecran = value(&r.line()).to_vec();
        let mut quaternion = [None; 4];
        let mut f = TextCursor::new(&data[r.pos().min(data.len())..]);
        while !f.eof() {
            let line = f.line();
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
        SaveGame2004 {
            current_cd,
            date_and_time,
            wam,
            monde,
            vue,
            chapitre,
            vars,
            flags,
            objects,
            dialogs,
            hide_table,
            zone_table,
            spr_plein_ecran,
            quaternion,
        }
    }

    /// The camera, when the four components were read.
    pub fn camera(&self) -> Option<[f32; 4]> {
        let [a, b, c, d] = self.quaternion;
        Some([a?, b?, c?, d?])
    }

    /// The flag lines of a table: its non-zero entries in index order.
    pub fn set_flags_from_table(&mut self, table: &[i32]) {
        self.flags = table
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| (v != 0).then_some((i as i32, v)))
            .collect();
    }

    /// The `DateAndTime` value the load page shows (FUN_00465740, *Atlantis
    /// III*'s @ 0x0045eab0: the second line's value).
    pub fn date_of(data: &[u8]) -> Vec<u8> {
        let mut r = Lines::new(data);
        r.line();
        value(&r.line()).to_vec()
    }
}

/// The thumbnail's path in the install: `menu\spr_1024\32bits\save\<name>.spr`
/// on the data disc (FUN_00419510 with `1024`, the save path's file part
/// from `GetFullPathNameA`, @ 0x004606fa..0x004607b1).
pub fn thumbnail_path(name: &str) -> String {
    format!("Datas/menu/spr_{THUMB_MENU_WIDTH}/32bits/save/{name}.spr")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveGame2004 {
        let mut vars = vec![0; VAR_COUNT];
        vars[0] = 7;
        vars[299] = -3;
        SaveGame2004 {
            current_cd: 2,
            date_and_time: b"11/13/26 10:20:30".to_vec(),
            wam: b"cabine.wam".to_vec(),
            monde: 1,
            vue: 4,
            chapitre: 1,
            vars,
            flags: vec![(3, 1), (2047, 9)],
            objects: vec![b"OBJET".to_vec()],
            dialogs: vec![PersoDialogs {
                perso: 2,
                subjects: vec![Subject {
                    etat: true,
                    id: 5,
                    nb_sprite: 1,
                }],
            }],
            hide_table: vec![HideEntry {
                id: 12,
                kind: 2,
                etat: 1,
            }],
            zone_table: vec![ZoneEntry {
                id: 4,
                kind: 1,
                vue: 3,
                etat: 0,
            }],
            spr_plein_ecran: b"Rien".to_vec(),
            quaternion: [Some(0.0), Some(0.5), Some(0.0), Some(0.866_025)],
        }
    }

    #[test]
    fn writes_the_original_s_lines_and_reads_them_back() {
        let s = sample();
        let text = s.write();
        let t = String::from_utf8_lossy(&text);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "CurrentCD = 2");
        assert_eq!(lines[5], "Chapitre = 1");
        assert_eq!(lines[6], "7");
        assert_eq!(lines[305], "-3");
        assert_eq!(lines[306], "NBFlag = 2");
        assert_eq!(lines[307], "3 = 1");
        assert!(t.contains("//== Sauvegarde des dialogues ==\nNBPersoAvecDial = 1\n"));
        assert!(t.contains("NBSpwHideTable = 1\nId = 12\nType = 2\nEtat = 1\n"));
        assert!(t.contains("NBZoneTable = 1\nId = 4\nType = 1\nVue = 3\nEtat = 0\n"));
        assert!(t.contains("SprPleinEcran = Rien\nQuaternion0 = 0.000000\n"));
        assert_eq!(SaveGame2004::parse(&text), s);
        assert_eq!(SaveGame2004::date_of(&text), b"11/13/26 10:20:30");
    }
}
