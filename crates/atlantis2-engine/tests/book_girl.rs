//! The wooden girl of Ireland's book (bug fix `fix_wooden_girl`).
//!
//! In `livre.wam` the wooden girl is an object: the entity `FILLE_BOIS`
//! of kind 2, shown at load on views 51-57 and 59-61, while the girl the
//! goddess frees, `FILLE_LIVRE`, is a character hidden at load.
//! `defnames.con` names `FILLE_BOIS` in both its character and its object
//! sections, and chapter 16, once the goddess has answered about the
//! statue (`dial=60`) or the poet (`dial=61`), runs
//! `hideperso=FILLE_BOIS` then `showperso=FILLE_LIVRE`. `hideperso` looks
//! for a character (FUN_0048c6b0 matches a record's id and kind, then
//! FUN_0048c730 the show table's row of that id and kind): the character
//! `FILLE_BOIS` has no record in the book, so with the original's rules
//! (`--original`) both girls are drawn there. With the fix on (the
//! default) `hideperso` also hides the object of that name, and only the
//! freed girl is drawn. `FILLE_BOIS` is the only name of both sections
//! that a script hides or shows, so the fix changes nothing else.
//!
//! Reported by Klaas Van den Broeck (@Klaasvdb),
//! github.com/Klaasvdb/atlantis2-notes (original-bugs.md #14).
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::path::PathBuf;

use atlantis2_data::{defnames, A2Files, Folder};
use atlantis2_engine::subjects::cell;
use atlantis2_engine::{BugFix, BugFixes, Engine, Enhancements, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA or ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn settle(e: &mut Engine) {
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
}

fn id(e: &Engine, section: usize, name: &str) -> i32 {
    e.script_state()
        .unwrap()
        .names
        .id(section, name.as_bytes())
        .unwrap() as i32
}

/// What the book shows after the goddess's answer about `subject`
/// (`STATUE` or `POETE`) with the bug fixes `fixes`.
struct Book {
    /// The dialog the answer started.
    dialog: u32,
    /// The wooden girl's records (the object `FILLE_BOIS`) on view 56:
    /// active or not.
    wooden: Vec<bool>,
    /// The freed girl's records (the character `FILLE_LIVRE`).
    freed: Vec<bool>,
    /// The cube faces with the place animations drawn, and again with
    /// the wooden girl's records left out: whether they differ.
    wooden_drawn: bool,
    freed_drawn: bool,
}

fn book_after(fixes: BugFixes, subject: &str) -> Book {
    let files = game().unwrap();
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.enhancements = Enhancements {
        fixes,
        ..if fixes == BugFixes::NONE {
            Enhancements::original()
        } else {
            Enhancements::default()
        }
    };
    e.start_new_game().unwrap();
    e.end_video();
    // Ireland's view 2 in chapter 16, as the goddess's film and her first
    // dialog leave it (chapter 16, lines 4-9).
    for c in [
        "(monde=1)",
        "(changewam=bigile1.wam)",
        "(vue=2)",
        "(chapter=16)",
    ] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    for c in [
        "(fincroix=1)",
        "(flagdeesse=1)",
        "(showperso=DEESSE)",
        "(enablesuj=DEESSE,DEESSE,NUADA,AILILL,STATUE,POETE)",
    ] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    assert_eq!(e.place(), ("bigile1.wam", 2));
    assert_eq!(e.script_state().unwrap().chapter, 16);

    // The goddess's subject menu open (a click on her), `subject` chosen
    // with the mouse.
    let goddess = id(&e, defnames::PERSONNAGES, "DEESSE");
    assert!(e.script_state_mut().unwrap().set_var("CLICPERSO", goddess));
    e.subject_menu.open = true;
    let s = e.script_state().unwrap();
    let value = s.names.param(defnames::SUJETS, subject.as_bytes());
    let list = s.subjects.list(goddess);
    let i = list.iter().position(|x| x.value == value).unwrap();
    let (x, y) = cell(list.len() as i32, i as i32);
    e.pointer = (x + 31, y + 31);
    e.trace = true;
    e.click(1.0e6);
    let mut dialog = None;
    for _ in 0..50 {
        e.tick();
        e.pass(None);
        for ev in std::mem::take(&mut e.events) {
            if let Event::DialogStart {
                number,
                started: true,
            } = ev
            {
                dialog = Some(number);
            }
        }
        if dialog.is_some() {
            break;
        }
    }
    let dialog = dialog.expect("the goddess's answer");
    // The dialog played out; the section goes on (`hideperso=FILLE_BOIS`,
    // `showperso=FILLE_LIVRE`).
    settle(&mut e);
    assert!(e.dialog().is_none());

    // Into the book (chapter 5 enters it from view 50 of `bigile1.wam`
    // with the pencil), its view 56, where both girls have records.
    for c in ["(changewam=livre.wam)", "(vue=56)"] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    assert_eq!(e.place(), ("livre.wam", 56));
    let wooden_id = id(&e, defnames::OBJETS, "FILLE_BOIS");
    let freed_id = id(&e, defnames::PERSONNAGES, "FILLE_LIVRE");
    let of = |e: &Engine, kind: i32, id: i32| -> Vec<bool> {
        e.anim
            .slots
            .iter()
            .filter(|s| s.kind == kind && s.id == id)
            .map(|s| s.active)
            .collect()
    };
    let drawn = |e: &Engine, kind: i32, id: i32| -> bool {
        let mut with = e.faces().clone();
        e.anim.draw(&mut with, None, false);
        let mut anim = e.anim.clone();
        for s in anim
            .slots
            .iter_mut()
            .filter(|s| s.kind == kind && s.id == id)
        {
            s.active = false;
        }
        let mut without = e.faces().clone();
        anim.draw(&mut without, None, false);
        with != without
    };
    Book {
        dialog,
        wooden: of(&e, 2, wooden_id),
        freed: of(&e, 1, freed_id),
        wooden_drawn: drawn(&e, 2, wooden_id),
        freed_drawn: drawn(&e, 1, freed_id),
    }
}

/// The names and their sections as the data has them: `FILLE_BOIS` is a
/// character and an object, the book's entity of that name an object;
/// chapter 16 hides it with `hideperso` only. (The script's lines are
/// compared through their MD5, not quoted.)
#[test]
fn the_wooden_girl_is_an_object_that_chapter_16_hides_as_a_character() {
    let Some(files) = game() else { return };
    let e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    let files = e.files();
    let names = &e.script_state().unwrap().names;
    assert!(names.id(defnames::PERSONNAGES, b"FILLE_BOIS").is_ok());
    assert!(names.id(defnames::OBJETS, b"FILLE_BOIS").is_ok());
    let wam = files.read(Folder::Wam, "livre.wam").unwrap();
    let wam = atlantis2_data::wam::Wam::parse(&wam).unwrap();
    let girls: Vec<(u32, Vec<u32>)> = wam
        .entities
        .iter()
        .filter(|x| x.con_name == "FILLE_BOIS")
        .map(|x| {
            (
                x.kind,
                x.state_records()
                    .filter(|s| s.shown == 1)
                    .map(|s| s.view)
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        girls,
        vec![(2, vec![51, 52, 53, 54, 55, 56, 57, 59, 60, 61])]
    );
    let text = files.read(Folder::Scenar, "chapi016.con").unwrap();
    let lines: Vec<&[u8]> = text
        .split(|&c| c == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .collect();
    let named: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.windows(10).any(|w| w == b"FILLE_BOIS"))
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(named, vec![53, 64]);
    for n in named {
        let md5 = format!("{:x}", Md5::digest(lines[n - 1]));
        // `hideperso=` of the name, nothing after it.
        assert_eq!(md5, "628ae115f345d85b1c4a99f0734511ab", "line {n}");
    }
}

#[test]
fn with_the_original_s_rules_both_girls_are_drawn_in_the_book() {
    if game().is_none() {
        return;
    }
    for (subject, number) in [("STATUE", 60), ("POETE", 61)] {
        let b = book_after(BugFixes::NONE, subject);
        assert_eq!(b.dialog, number);
        assert_eq!(b.wooden, vec![true], "{subject}");
        assert!(b.freed.contains(&true), "{subject}: {:?}", b.freed);
        assert!(b.wooden_drawn && b.freed_drawn, "{subject}");
    }
}

#[test]
fn with_the_fix_only_the_freed_girl_is_drawn() {
    if game().is_none() {
        return;
    }
    for fixes in [BugFixes::only(BugFix::WoodenGirl), BugFixes::ALL] {
        for (subject, number) in [("STATUE", 60), ("POETE", 61)] {
            let b = book_after(fixes, subject);
            assert_eq!(b.dialog, number);
            assert_eq!(b.wooden, vec![false], "{subject}");
            assert!(b.freed.contains(&true), "{subject}: {:?}", b.freed);
            assert!(!b.wooden_drawn && b.freed_drawn, "{subject}");
        }
    }
    // Every other fix on, this one off: the original's book.
    let mut others = BugFixes::ALL;
    others.set(BugFix::WoodenGirl, false);
    let b = book_after(others, "STATUE");
    assert_eq!(b.wooden, vec![true]);
    assert!(b.wooden_drawn && b.freed_drawn);
}
