//! The inventory, the documents and their screens against the installed
//! game: the layouts read from its files, every object and document the
//! node scripts name, the first node's tutorials, a use of a selected
//! object on the warp, and the save's elements.
//!
//! Needs the game: `ATLANTIS4_DATA` = the installed folder (see
//! `atlantis4_data::testing`); skipped cleanly with
//! `ATLANTIS4_SKIP_DATA_TESTS=1`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use atlantis4_data::lua::{tokens, Token};
use atlantis4_data::texts::{objects_path, parse_labelled, LANGUAGES};
use atlantis4_data::A4Files;
use atlantis4_engine::dialog::Action;
use atlantis4_engine::documents::page_stem;
use atlantis4_engine::layout_tree::resolve;
use atlantis4_engine::screens::{object_picture, Screens};
use atlantis4_engine::Game;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options, Script, Value};

const W: u32 = 1365;
const H: u32 = 768;

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// A new game with a clock the test sets (µs).
fn boot(dir: &Path) -> (Rc<RefCell<Game>>, Script, Rc<RefCell<u64>>) {
    let (files, _) = A4Files::open_dir(dir).unwrap();
    let clock = Rc::new(RefCell::new(5_000_000u64));
    let mut g = Game::new(files, H);
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || *c.borrow()));
    g.trace = Some(Vec::new());
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    let Some(BootValue::Str(first)) = boot.first_warp_path else {
        panic!("no firstWarpPath")
    };
    let script = new_game(&engine, Options::default(), &first).unwrap();
    (game, script, clock)
}

/// Sets the clock and runs the GUI's update and its callbacks.
fn at(game: &Rc<RefCell<Game>>, script: &mut Script, clock: &Rc<RefCell<u64>>, us: u64) {
    *clock.borrow_mut() = us;
    game.borrow_mut().now_ms = us as f64 / 1000.0;
    game.borrow_mut().update_gui(us as f64 / 1000.0);
    run_callbacks(game, script);
}

fn run_callbacks(game: &Rc<RefCell<Game>>, script: &mut Script) {
    loop {
        let cbs = game.borrow_mut().gui_callbacks();
        if cbs.is_empty() {
            break;
        }
        for (name, args) in cbs {
            script.callback(&name, &args).unwrap();
        }
    }
}

/// A click (press and release) at the centre of `r`.
fn click(game: &Rc<RefCell<Game>>, script: &mut Script, x: f64, y: f64) {
    let took = game.borrow_mut().gui_press(W, H, x, y);
    assert!(took, "the click at ({x}, {y}) reached the warp");
    let actions = game.borrow_mut().gui_release(W, H, x, y);
    for a in actions.unwrap_or_default() {
        match a {
            Action::Lua(name, args) => script.callback(&name, &args).unwrap(),
            Action::HideAnswers => game.borrow_mut().dialogs.hide_answers(),
        }
    }
    run_callbacks(game, script);
}

/// The string first arguments of calls to `names` in the node scripts
/// (`3D/**/*.lua`), comments left out.
fn script_args(dir: &Path, names: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dir.join("3D")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")) {
                continue;
            }
            let t = tokens(&std::fs::read(&p).unwrap());
            for w in t.windows(3) {
                if let [Token::Name(n), Token::Sym(b'('), Token::Str(s)] = w {
                    if names.contains(&n.as_str()) {
                        out.insert(s.clone());
                    }
                }
            }
        }
    }
    out
}

#[test]
fn the_screens_are_laid_out_from_the_files() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let lang = files.lang_ini().unwrap_or_else(|| "en".into());
    let s = Screens::load(&files, &lang);
    // One page of 8 slots; seven document pages, 16 then 3 slots each.
    assert_eq!(Screens::slots(&s.inventory), [8]);
    assert_eq!(Screens::slots(&s.documents), [16, 3, 3, 3, 3, 3, 3]);
    assert_eq!(
        s.inventory
            .global_str("textFormat")
            .map(|t| t.matches("%s").count()),
        Some(2)
    );
    // The slots: 87 × 87 of the 1365 × 768 picture, laid out at 1.01 of
    // the window (`inventory` 1.01 × 1.01, then PanScan 1365/768).
    let r = s.slot_rects(0, W, H);
    assert_eq!(r.len(), 8);
    for q in &r {
        assert!((q.width - 87.0 * 1.01).abs() < 0.01, "{q:?}");
        assert!((q.height - 87.0 * 1.01).abs() < 0.01, "{q:?}");
    }
    assert!(r[0].left < r[1].left && r[0].top < r[4].top);
    // The HUD button sits in the bottom-right corner, as large as its
    // picture.
    let b = s.hud_button(W, H).unwrap();
    assert!((b.left + b.width - f64::from(W)).abs() < 1e-3);
    assert!((b.top + b.height - f64::from(H)).abs() < 1e-3);
    assert!(b.width > 10.0 && b.height > 10.0);
}

#[test]
fn every_object_and_document_of_the_scripts_is_known() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let given = script_args(&dir, &["AddToBag", "AddToBagWithoutNotification"]);
    let removed = script_args(&dir, &["RemoveFromBag"]);
    let selected = script_args(&dir, &["Selected", "ObjectCount", "TestObjectLimit"]);
    let shown = script_args(&dir, &["ShowDocument", "AddDocument"]);
    // The story map's counts (the node scripts only).
    assert_eq!((given.len(), removed.len(), selected.len()), (27, 8, 22));
    assert!(removed.is_subset(&given) && selected.is_subset(&given));
    for lang in LANGUAGES {
        let names = parse_labelled(&files.read(&objects_path(lang)).unwrap(), "Object").unwrap();
        for id in &given {
            let l = names.iter().find(|l| &l.id == id);
            assert!(
                l.is_some_and(|l| !l.name.is_empty()),
                "{lang}: no name for {id}"
            );
        }
    }
    for id in &given {
        assert!(
            resolve(&files, &object_picture(id), "en").is_some(),
            "no picture for {id}"
        );
    }
    // The node scripts' documents and `OnGameEnter.lua`'s two lose
    // pictures; each has its first page in every language.
    let mut docs = shown;
    docs.extend(["BD_05".to_string(), "BD_09".to_string()]);
    let mut missing = Vec::new();
    for lang in LANGUAGES {
        for d in &docs {
            let stem = page_stem(d, 0);
            let found = [".png", ".jpg"]
                .iter()
                .any(|e| resolve(&files, &format!("{stem}{e}"), lang).is_some());
            if !found {
                missing.push(format!("{lang}:{d}"));
            }
        }
    }
    // `OnGameEnter.lua`'s lose pictures exist nowhere (its functions are
    // never called): `ShowDocument` would close them at once.
    missing.retain(|m| !m.ends_with(":BD_05") && !m.ends_with(":BD_09"));
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn the_first_nodes_tutorials_and_the_matches() {
    let Some(dir) = install() else { return };
    let (game, mut script, clock) = boot(&dir);
    // The intro's end: the matches, then the first tutorial.
    // The intro skipped: its end calls `OnMovieFinished`.
    game.borrow_mut().movie = None;
    script
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/intro.ogv".to_vec())],
        )
        .unwrap();
    {
        let g = game.borrow();
        assert_eq!(g.inventory.count("ALLUMETTES"), 1);
        assert_eq!(g.inventory.object((0, 0)).unwrap().id, "ALLUMETTES");
        assert_eq!(g.inventory.selected, None);
        assert_eq!(g.documents.shown.as_ref().unwrap().name, "Tuto_360");
    }
    // While it is shown the warp takes nothing; one click closes it 200 ms
    // later, and `OnDocumentClosed` shows the second.
    assert!(!game.borrow().warp_takes_input());
    click(&game, &mut script, 500.0, 500.0);
    at(&game, &mut script, &clock, 5_199_999);
    assert_eq!(
        game.borrow().documents.shown.as_ref().unwrap().name,
        "Tuto_360"
    );
    at(&game, &mut script, &clock, 5_200_000);
    assert_eq!(
        game.borrow().documents.shown.as_ref().unwrap().name,
        "Tuto_Marqueur"
    );
    // A double click closes it at once.
    *clock.borrow_mut() = 6_000_000;
    click(&game, &mut script, 10.0, 10.0);
    *clock.borrow_mut() = 6_100_000;
    click(&game, &mut script, 10.0, 10.0);
    assert!(game.borrow().documents.shown.is_none());
    assert!(game.borrow().warp_takes_input());
    // The frame shows the HUD's button over the warp, and only there.
    let mut with = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut with, W, H, None);
    game.borrow_mut().draw_gui = false;
    let mut without = vec![0u32; (W * H) as usize];
    game.borrow_mut().draw(&mut without, W, H, None);
    let b = game.borrow_mut().gui().hud_button(W, H).unwrap();
    let mut changed = 0;
    for (i, (a, c)) in with.iter().zip(&without).enumerate() {
        if a != c {
            let (x, y) = ((i % W as usize) as f64 + 0.5, (i / W as usize) as f64 + 0.5);
            assert!(b.contains(x, y), "pixel ({x}, {y}) changed outside the HUD");
            changed += 1;
        }
    }
    assert!(changed > 100, "{changed}");
}

#[test]
fn a_selected_object_is_used_on_the_warp_and_stays_selected() {
    let Some(dir) = install() else { return };
    let (game, mut script, clock) = boot(&dir);
    // The intro skipped: its end calls `OnMovieFinished`.
    game.borrow_mut().movie = None;
    script
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/intro.ogv".to_vec())],
        )
        .unwrap();
    game.borrow_mut().hide_document();
    run_callbacks(&game, &mut script);
    game.borrow_mut().hide_document();
    run_callbacks(&game, &mut script);
    game.borrow_mut().add_to_bag("COUTEAU", true);
    // The HUD button opens the menu on the inventory.
    let b = game.borrow_mut().gui().hud_button(W, H).unwrap();
    click(
        &game,
        &mut script,
        b.left + b.width / 2.0,
        b.top + b.height / 2.0,
    );
    assert!(!game.borrow().warp_takes_input());
    // The knife in slot 1: one click selects it, a second within 300 ms
    // closes the menu.
    let r = game.borrow_mut().gui().slot_rects(0, W, H)[1];
    let (x, y) = (r.left + r.width / 2.0, r.top + r.height / 2.0);
    *clock.borrow_mut() = 7_000_000;
    click(&game, &mut script, x, y);
    assert!(game.borrow().inventory.is_selected("COUTEAU"));
    assert!(!game.borrow().warp_takes_input());
    *clock.borrow_mut() = 7_200_000;
    click(&game, &mut script, x, y);
    assert!(game.borrow().warp_takes_input());
    // The knife on the panel of the Limule ship's node 13: it opens.
    script.change_warp(b"3D/Vaisseau_LM/0013.wp").unwrap();
    game.borrow_mut().trace = Some(Vec::new());
    script
        .callback("OnWarpObjectHit", &[Value::Str(b"xfa1".to_vec())])
        .unwrap();
    let trace = game.borrow_mut().trace.take().unwrap();
    assert!(trace.iter().any(|l| l == "CALL Selected"));
    assert!(
        trace.iter().any(|l| l == "CALL StartAnimation"),
        "{trace:?}"
    );
    assert!(!trace.iter().any(|l| l == "CALL DrawText"), "{trace:?}");
    // Still selected after the use and the node change.
    assert!(game.borrow().inventory.is_selected("COUTEAU"));
    // The save holds both objects in their slots, not the selection.
    script.run_chunk(b"SaveGame(\"9.xml\")", b"test");
    let (file, text) = game.borrow().last_save.clone().unwrap();
    assert_eq!(file, "9.xml");
    let text: String = text.iter().map(|&c| char::from(c)).collect();
    assert!(text.contains(
        "<Inventory>\n    <Page>\n        <Slot>\n            <Object id=\"ALLUMETTES\" />\n        </Slot>\n        <Slot>\n            <Object id=\"COUTEAU\" />\n        </Slot>\n        <Slot />"
    ));
    assert!(text.contains("\n<Diary />\n<PuzzleTimer>\n    <PuzzleEnCours value=\"0\" />"));
    assert!(text.contains("<WarpName name=\"3D/Vaisseau_LM/0013.wp\" />"));
    assert_eq!(text.matches("<Page>").count(), 1 + 7);
    assert_eq!(text.matches("<Slot").count(), 8 + 16 + 6 * 3);
}

/// Writes the screens as PNG files into `A4_SHOT_DIR` for a look: the
/// first tutorial, the HUD with a selected object, the inventory open.
#[test]
#[ignore]
fn screenshots() {
    let Some(dir) = install() else { return };
    let Some(out) = std::env::var_os("A4_SHOT_DIR") else {
        return;
    };
    let out = PathBuf::from(out);
    let (game, mut script, clock) = boot(&dir);
    game.borrow_mut().movie = None;
    script
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/intro.ogv".to_vec())],
        )
        .unwrap();
    let shot = |name: &str| {
        let mut px = vec![0u32; (W * H) as usize];
        game.borrow_mut().draw(&mut px, W, H, Some((700.0, 400.0)));
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
            .collect();
        let f = std::fs::File::create(out.join(name)).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), W, H);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgb).unwrap();
    };
    shot("1-tutorial.png");
    game.borrow_mut().hide_document();
    run_callbacks(&game, &mut script);
    game.borrow_mut().hide_document();
    run_callbacks(&game, &mut script);
    for id in ["COUTEAU", "LANTERNE", "CASQUE_C17"] {
        game.borrow_mut().add_to_bag(id, true);
    }
    at(&game, &mut script, &clock, 5_600_000);
    shot("2-notification.png");
    let b = game.borrow_mut().gui().hud_button(W, H).unwrap();
    click(
        &game,
        &mut script,
        b.left + b.width / 2.0,
        b.top + b.height / 2.0,
    );
    let r = game.borrow_mut().gui().slot_rects(0, W, H)[1];
    *clock.borrow_mut() = 9_000_000;
    click(
        &game,
        &mut script,
        r.left + r.width / 2.0,
        r.top + r.height / 2.0,
    );
    shot("3-inventory.png");
    game.borrow_mut().gui().close_menu();
    shot("4-hud.png");
}
