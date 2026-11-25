//! GOG.com's four languages and the Italian *Atlantis Collection*: what
//! changes with the install's language besides the texts, voices and
//! films the game reads by name.
//!
//! - the edition's name, from the installer's registry `Language`;
//! - the intro (@ 0x00459360): `intro.hnm` alone in each GOG.com language
//!   (the German branch's `intro_ger.hnm` is in no GOG.com depot), then
//!   `intro_ita.hnm` in Italian;
//! - the fonts' character set (`font.txt`): Windows-1252 but in German,
//!   Central European;
//! - the scarab case's bank `Scarabee.spr`, which only the English and
//!   German installs have under that name: the case's sprites are drawn
//!   there and nowhere else (the Italian install names it `Scarabée.SPR`,
//!   as the French and Spanish).
//!
//! The story chain per language is `story.rs`'s
//! `whole_story_other_languages` and `rhea_old_man_at_875_8_by_language`.
//!
//! Needs the games: `ATLANTIS3_DATA` (French) and `ATLANTIS3_GOG_EN`,
//! `ATLANTIS3_GOG_DE`, `ATLANTIS3_GOG_ES`, `ATLANTIS3_IT_COLLECTION`.

use std::path::{Path, PathBuf};

use atlantis3_data::testing::{dir_of, install_dir, it_collection, GOG_LANGUAGES};
use atlantis3_data::A3Files;
use atlantis3_engine::text::{FontId, Texts, CHARSET_EE};
use atlantis3_engine::{Game, Mode};

/// (variable, install, registry language, edition's name, charset,
/// scarab bank, the intro's films).
struct Install {
    var: &'static str,
    dir: PathBuf,
    code: &'static str,
    label: String,
    charset: u8,
    scarab: bool,
    intro: &'static [&'static str],
}

fn installs() -> Vec<Install> {
    let mut out = Vec::new();
    if let Some(dir) = install_dir() {
        out.push(Install {
            var: "ATLANTIS3_DATA",
            dir,
            code: "fra",
            label: "Atlantis III, GOG.com, French".into(),
            charset: 0,
            scarab: false,
            intro: &["intro.hnm"],
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
            label: format!("Atlantis III, GOG.com, {name}"),
            charset,
            scarab,
            intro: &["intro.hnm"],
        });
    }
    if let Some(dir) = it_collection() {
        out.push(Install {
            var: "ATLANTIS3_IT_COLLECTION",
            dir,
            code: "ita",
            label: "Atlantis III, Atlantis Collection, Italian".into(),
            charset: 0,
            scarab: false,
            intro: &["intro.hnm", "intro_ita.hnm"],
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
        assert_eq!(files.edition_label().unwrap(), i.label, "{}", i.var);
        let g = open(&i.dir);
        let c = &g.startup.config;
        assert_eq!(
            (&c.langue[..], &c.sous_titre[..]),
            (i.code.as_bytes(), i.code.as_bytes())
        );
        assert_eq!(c.version, b"CD", "{}", i.var);
    }
}

/// `intro.hnm` alone (327 frames, about 22 s), then `intro_ita.hnm` in
/// Italian, then nothing more: each film to its last frame (a film with
/// a sound track keeps the sound's time, `intro_ita.hnm`'s 172 frames
/// about 7 s).
#[test]
fn intro_alone_in_every_language() {
    for i in installs() {
        let mut g = open(&i.dir);
        let frames: Vec<u32> = i
            .intro
            .iter()
            .map(|n| {
                let d = g.files().read(&format!("hnm/{n}")).expect(n);
                atlantis3_data::hnm_header::HnmHeader::parse(&d)
                    .expect(n)
                    .frame_count()
            })
            .collect();
        g.start_intro();
        assert_eq!(g.video().map(|v| v.0), Some("intro.hnm"), "{}", i.var);
        let mut t = 0.0;
        let mut names = vec!["intro.hnm".to_string()];
        let mut shown = vec![0];
        while t < 60.0 && g.mode() == Mode::Intro {
            t += 1.0 / 60.0;
            g.step(t);
            if let Some((n, f)) = g.video() {
                if names.last().map(String::as_str) != Some(n) {
                    names.push(n.to_string());
                    shown.push(0);
                }
                *shown.last_mut().unwrap() = f;
            }
        }
        assert_eq!(g.mode(), Mode::Idle, "{}", i.var);
        assert_eq!(names, i.intro, "{}", i.var);
        for (k, (&s, &n)) in shown.iter().zip(&frames).enumerate() {
            assert!(s + 1 >= n, "{} {}: {s} of {n} frames", i.var, i.intro[k]);
        }
        if i.intro.len() == 1 {
            assert!((21.0..23.0).contains(&t), "{}: intro took {t} s", i.var);
        } else {
            assert!((27.0..31.0).contains(&t), "{}: intro took {t} s", i.var);
        }
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
