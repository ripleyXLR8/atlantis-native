//! *Atlantis Evolution*'s 2004 sound effects: the `#SONS#` section of
//! `defnames.con` and the file names the program derives (atlantis4-re
//! `subsystems/cd2004-sfx.md`; addresses are atlantis4-cd-en-3022848's).
//!
//! The section names the scripts' sounds (`SON_PORTE_FERME,PORTE_FERME.wav`);
//! the CD's `cd0\datas\sfx\` holds Ogg Vorbis files only, which
//! `AtlSound_LoadSfx` (FUN_0042ef10) finds by putting `ogg` (the
//! `std::string` 0x006ea030, built from 0x006ae780) in place of the name's
//! last three characters.

use crate::evolution2004_film::Cinematiques;

/// The tag of the section (0x006b0bd0, entry 5 of the pointer table
/// 0x006b0b6c).
pub const TAG: &[u8] = b"#SONS#";

/// The `#SONS#` entries: the name upper-cased and its first parameter, the
/// file (`DefName_GetIdName(5, name)` FUN_00466f70, then
/// `GetParamFromId(5, id, 1)` FUN_00467690 and `DefName_CopyParam`
/// FUN_00467620, @ 0x0044ea69..0x0044ea86 for `PLAYSOUND`).
pub fn sons(defnames: &[u8]) -> Cinematiques {
    Cinematiques::parse_section(defnames, TAG)
}

/// `AtlSound_LoadSfx`'s first file (@ 0x0042efb0..0x0042efe6): the name
/// without its last three characters, then `ogg`.
pub fn ogg_name(name: &str) -> String {
    let cut = name.len().saturating_sub(3);
    match name.get(..cut) {
        Some(stem) => format!("{stem}ogg"),
        None => format!("{name}ogg"),
    }
}

/// The two names a place animation's slot tries at its frame 0 (FUN_00439fc0
/// @ 0x0043a07e..0x0043a0f6 and @ 0x0043a36a..0x0043a40e): the first ten
/// characters of the slot's SPW name, then its first four, each followed by
/// `.` (0x006ae6e4) and `ogg` (0x006ea030). `std::string::substr` keeps a
/// shorter name whole.
pub fn anim_sound_names(spw: &str) -> (String, String) {
    let head = |n: usize| spw.get(..n.min(spw.len())).unwrap_or(spw);
    (format!("{}.ogg", head(10)), format!("{}.ogg", head(4)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_gives_the_files() {
        let d = b"#CINEMATIQUES#\r\nINTRO,intro.bik\r\n#SONS#\r\n// Syntaxe\r\n\
                  SON_PORTE_FERME,PORTE_FERME.wav\r\nHUBLOT_CLAQUE,hua2.wav \r\n\
                  #PUZZLES#\r\nX,y\r\n";
        let s = sons(d);
        assert_eq!(s.file_of(b"son_porte_ferme"), Some(&b"PORTE_FERME.wav"[..]));
        assert_eq!(s.file_of(b"HUBLOT_CLAQUE"), Some(&b"hua2.wav"[..]));
        assert_eq!(s.file_of(b"INTRO"), None);
        assert_eq!(s.file_of(b"X"), None);
    }

    #[test]
    fn the_ogg_takes_the_last_three_characters() {
        assert_eq!(ogg_name("sfx/PORTE_FERME.wav"), "sfx/PORTE_FERME.ogg");
        assert_eq!(ogg_name("ab"), "ogg");
    }

    #[test]
    fn a_slot_tries_ten_then_four_characters() {
        assert_eq!(
            anim_sound_names("hua2001002"),
            ("hua2001002.ogg".into(), "hua2.ogg".into())
        );
        assert_eq!(anim_sound_names("ab"), ("ab.ogg".into(), "ab.ogg".into()));
    }
}
