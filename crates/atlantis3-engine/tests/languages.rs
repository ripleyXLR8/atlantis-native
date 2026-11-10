//! GOG.com's four languages: what changes with the install's language
//! besides the texts, voices and films the game reads by name.
//!
//! - the edition's name, from the installer's registry `Language`;
//! - the intro (@ 0x00459360): `intro.hnm` alone in each language (the
//!   German branch's `intro_ger.hnm` is in no GOG.com depot);
//! - the fonts' character set (`font.txt`): Windows-1252 but in German,
//!   Central European;
//! - the scarab case's bank `Scarabee.spr`, which only the English and
//!   German installs have under that name: the case's sprites are drawn
//!   there and nowhere else.
//!
//! The story chain per language is `story.rs`'s
//! `whole_story_other_languages` and `rhea_old_man_at_875_8_by_language`.
//!
//! Needs the games: `ATLANTIS3_DATA` (French) and `ATLANTIS3_GOG_EN`,
//! `ATLANTIS3_GOG_DE`, `ATLANTIS3_GOG_ES`.

use std::path::{Path, PathBuf};

use atlantis3_data::testing::{dir_of, install_dir, GOG_LANGUAGES};
use atlantis3_data::A3Files;
use atlantis3_engine::text::{FontId, Texts, CHARSET_EE};
use atlantis3_engine::{Game, Mode};

/// (variable, install, registry language, edition's language, charset,
/// scarab bank).
struct Install {
    var: &'static str,
    dir: PathBuf,
    code: &'static str,
    name: &'static str,
    charset: u8,
    scarab: bool,
}

fn installs() -> Vec<Install> {
    let mut out = Vec::new();
    if let Some(dir) = install_dir() {
        out.push(Install {
            var: "ATLANTIS3_DATA",
            dir,
            code: "fra",
            name: "French",
            charset: 0,
            scarab: false,
        });
    }
    for (var, code) in GOG_LANGUAGES {
        let Some(dir) = dir_of(var) else { continue };
        let (name, charset, scarab) = match code {
            "gbr" => ("English", 0, true),
            "ger" => ("German", CHARSET_EE, true),
            _ => ("Spanish", 0, false),
        };
        out.push(Install {
            var,
            dir,
            code,
            name,
            charset,
            scarab,
        });
    }
    out
}

fn open(dir: &Path) -> Game {
    Game::new(A3Files::open_dir(dir).unwrap().0).expect("the game starts")
}

#[test]
fn edition_names_and_languages() {
    for i in installs() {
        let (files, _) = A3Files::open_dir(&i.dir).unwrap();
        assert_eq!(
            files.edition_label().unwrap(),
            format!("Atlantis III, GOG.com, {}", i.name),
            "{}",
            i.var
        );
        let g = open(&i.dir);
        let c = &g.startup.config;
        assert_eq!(
            (&c.langue[..], &c.sous_titre[..]),
            (i.code.as_bytes(), i.code.as_bytes())
        );
        assert_eq!(c.version, b"CD", "{}", i.var);
    }
}

/// `intro.hnm` alone, 327 frames at 15 a second, then nothing more.
#[test]
fn intro_alone_in_every_language() {
    for i in installs() {
        let mut g = open(&i.dir);
        g.start_intro();
        assert_eq!(g.video().map(|v| v.0), Some("intro.hnm"), "{}", i.var);
        let mut t = 0.0;
        let mut names = vec!["intro.hnm".to_string()];
        while t < 40.0 && g.mode() == Mode::Intro {
            t += 1.0 / 60.0;
            g.step(t);
            if let Some((n, _)) = g.video() {
                if names.last().map(String::as_str) != Some(n) {
                    names.push(n.to_string());
                }
            }
        }
        assert_eq!(g.mode(), Mode::Idle, "{}", i.var);
        assert_eq!(names, ["intro.hnm"], "{}", i.var);
        assert!((21.0..23.0).contains(&t), "{}: intro took {t} s", i.var);
    }
}

/// Every font's GDI measures in the install's character set; the German
/// credits' 0xe8 is measured as `č`.
#[test]
fn font_charsets() {
    for i in installs() {
        let (files, _) = A3Files::open_dir(&i.dir).unwrap();
        let texts = Texts::load(&files, i.code);
        for id in [
            FontId::Default,
            FontId::Bulle,
            FontId::Special,
            FontId::Titre,
            FontId::Credit,
        ] {
            for w in [640, 800, 1024] {
                let f = texts
                    .gdi(id, w)
                    .unwrap_or_else(|| panic!("{} {id:?} {w}", i.var));
                assert_eq!(f.charset, i.charset, "{} {id:?} {w}", i.var);
            }
        }
    }
}

/// The scarab case's first frames (closed, then opened by a left click,
/// the dead scarab's frames on the 1/15 s clock): the case's sprite calls
/// with the English and German bank, none with the French and Spanish
/// installs (no `Scarabee.spr` there).
#[test]
fn scarab_case_bank_by_language() {
    for i in installs() {
        let mut g = open(&i.dir);
        assert!(g.new_game());
        g.flat_warp = true;
        g.trace = Some(Vec::new());
        let mut t = 1000.0;
        assert!(g.board_screen_start(13, t));
        let frame = |g: &mut Game, t: &mut f64, left: bool| {
            *t += 0.1;
            assert!(g.board_screen_frame(*t, (400, 300), left, false));
            let calls: Vec<String> = g
                .trace
                .as_mut()
                .unwrap()
                .drain(..)
                .filter(|l| l.starts_with("affc spr\\Scarabee"))
                .collect();
            calls
        };
        let closed = frame(&mut g, &mut t, true);
        let open1 = frame(&mut g, &mut t, false);
        let open2 = frame(&mut g, &mut t, false);
        if i.scarab {
            assert_eq!(closed, ["affc spr\\Scarabee.spr#0@400,300"], "{}", i.var);
            assert_eq!(
                open1,
                [
                    "affc spr\\Scarabee.spr#1@400,300",
                    "affc spr\\Scarabee.spr#5@343,263"
                ],
                "{}",
                i.var
            );
            assert_eq!(open2[1], "affc spr\\Scarabee.spr#6@343,263", "{}", i.var);
        } else {
            assert!(
                closed.is_empty() && open1.is_empty() && open2.is_empty(),
                "{}",
                i.var
            );
        }
        // The subjects change with or without the bank (flag 72).
        assert_eq!(g.script().unwrap().flags[72], 1, "{}", i.var);
    }
}
