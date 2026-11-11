//! Saves, loading, the chapter saves, the deaths and the end.
//!
//! `the_globals_print_as_the_originals_writer` checks the save's globals
//! against the original's own writer run in Unicorn
//! (`tests/golden/a4_lua_save_globals.txt`, see its header; synthetic
//! globals only). The other tests need the game: `ATLANTIS4_DATA` = the
//! installed folder (see `atlantis4_data::testing`); they skip cleanly
//! with `ATLANTIS4_SKIP_DATA_TESTS=1`. Saves go to memory, or to a
//! temporary folder of the test's own, never to the player's.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use atlantis4_data::A4Files;
use atlantis4_engine::saves::{self, BackupFolder};
use atlantis4_engine::savexml::{save_text, SaveParts, XmlNode};
use atlantis4_engine::Game;
use atlantis4_script::{
    new_game, read_boot_vars, Call, Engine, EngineRef, LoadedValue, Options, Reply, Script, Value,
};

const H: u32 = 768;

fn install() -> Option<PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// An engine that only reads nothing (the globals' tests).
struct Bare;

impl Engine for Bare {
    fn call(&mut self, _: &Call) -> Reply {
        Reply::None
    }

    fn read_script(&mut self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

/// The golden's chunk and its part B (a state with the libraries), as
/// lines.
fn golden() -> (String, Vec<String>) {
    let path = format!(
        "{}/tests/golden/a4_lua_save_globals.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let t = std::fs::read_to_string(path).unwrap();
    let chunk: String = t
        .split("== chunk\n")
        .nth(1)
        .unwrap()
        .split("== A:")
        .next()
        .unwrap()
        .to_string();
    let b: Vec<String> = t
        .split("== B: lua_newstate + luaL_openlibs\n")
        .nth(1)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    (chunk, b)
}

/// The golden's `\xNN` written back as bytes.
fn unescape(l: &str) -> Vec<u8> {
    let b = l.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"\\x") {
            out.push(u8::from_str_radix(&l[i + 2..i + 4], 16).unwrap());
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

fn empty_parts() -> SaveParts {
    SaveParts {
        inventory: XmlNode::new("Inventory"),
        documents: XmlNode::new("DocumentsBrowser"),
        diary: XmlNode::new("Diary"),
        puzzle_timer: XmlNode::new("PuzzleTimer"),
        warp_name: b"3D/LZ/0004.wp".to_vec(),
        orientation: (0.0, 0.0),
        music: Vec::new(),
        played_time: 0.0,
    }
}

/// The globals' lines of a save (after `PlayedTime`).
fn global_lines(text: &[u8]) -> BTreeSet<Vec<u8>> {
    let lines: Vec<&[u8]> = text.split(|&c| c == b'\n').collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with(b"<PlayedTime"))
        .unwrap();
    lines[at + 1..]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_vec())
        .collect()
}

#[test]
fn the_globals_print_as_the_originals_writer() {
    let (chunk, want) = golden();
    let engine: EngineRef = Rc::new(RefCell::new(Bare));
    let mut script = Script::new(&engine, Options::default()).unwrap();
    script.run_chunk(chunk.as_bytes(), b"golden");
    let g = script.globals().unwrap();
    let text = save_text(&empty_parts(), &g);
    // The globals as a set (their order follows the collector). The game state holds `tolua`
    // and the bindings too, all of types the save skips.
    let got = global_lines(&text);
    // (Its first line is the declaration.)
    let want: BTreeSet<Vec<u8>> = want[1..].iter().map(|l| unescape(l)).collect();
    assert_eq!(got, want);
    assert!(
        text.starts_with(b"<?xml version=\"1.0\" ?>\n<GameVersion major=\"1\" minor=\"0\" />\n")
    );
}

#[test]
fn the_globals_read_back_as_the_originals_reader() {
    let (chunk, want) = golden();
    let mut file = b"<?xml version=\"1.0\" ?>\n".to_vec();
    for l in &want[1..] {
        file.extend(unescape(l));
        file.push(b'\n');
    }
    let els = saves::parse(&saves::to_disk(&file)).unwrap();
    let g = saves::globals(&els);
    let get = |n: &str| {
        g.iter()
            .find(|(k, _)| k == n.as_bytes())
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    assert_eq!(get("flagTrue"), LoadedValue::Boolean(true));
    assert_eq!(get("flagFalse"), LoadedValue::Boolean(false));
    assert_eq!(get("fraction"), LoadedValue::Number(0.1));
    assert_eq!(get("tiny"), LoadedValue::Number(0.0));
    assert_eq!(get("big"), LoadedValue::Number(123_456_789_012.0));
    assert_eq!(get("quote"), LoadedValue::String(b"say \"hi\"".to_vec()));
    assert_eq!(get("ctrl"), LoadedValue::String(b"tab\tnl\n".to_vec()));
    assert_eq!(get("high"), LoadedValue::String(b"caf\xe9".to_vec()));
    assert_eq!(get("amp"), LoadedValue::String(b"a&b<c>d".to_vec()));
    // TinyXML's pass-through on save, its entity on load: `&#x41;` comes
    // back as "A".
    assert_eq!(get("entity"), LoadedValue::String(b"A".to_vec()));
    assert!(chunk.contains("entity = \"&#x41;\""));
}

#[test]
fn a_save_on_disk_has_crlf_and_no_bom() {
    let dir = std::env::temp_dir().join(format!("a4-saves-test-{}", std::process::id()));
    let mut f = BackupFolder::Dir(dir.join("Atlantis Evolution"));
    let text = save_text(&empty_parts(), &[]);
    assert!(f.write("Backup/save.xml", &saves::to_disk(&text)));
    let disk = std::fs::read(dir.join("Atlantis Evolution/save.xml")).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(disk.starts_with(b"<?xml version=\"1.0\" ?>\r\n<GameVersion"));
    assert!(!disk.windows(2).any(|w| w[1] == b'\n' && w[0] != b'\r'));
    assert_eq!(
        disk.len(),
        text.len() + text.iter().filter(|&&c| c == b'\n').count()
    );
}

// ---------------------------------------------------------------------------
// With the game
// ---------------------------------------------------------------------------

struct Run {
    game: Rc<RefCell<Game>>,
    script: Script,
    clock: Rc<RefCell<u64>>,
}

/// A new game, the intro's end skipped and its two tutorials closed.
fn boot(dir: &Path) -> Run {
    let (files, _) = A4Files::open_dir(dir).unwrap();
    let clock = Rc::new(RefCell::new(5_000_000u64));
    let mut g = Game::new(files, H);
    let c = clock.clone();
    g.clock_us = Some(Box::new(move || *c.borrow()));
    g.sounds.headless = true;
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    // Memory saves.
    game.borrow_mut().configure_saves(&boot, None);
    game.borrow_mut().enter_game();
    let first = game.borrow().first_warp_path.clone();
    let mut script = new_game(&engine, Options::default(), &first).unwrap();
    game.borrow_mut().movie = None;
    script
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/intro.ogv".to_vec())],
        )
        .unwrap();
    for _ in 0..2 {
        game.borrow_mut().hide_document();
        let cbs = game.borrow_mut().gui_callbacks();
        for (n, a) in cbs {
            script.callback(&n, &a).unwrap();
        }
    }
    Run {
        game,
        script,
        clock,
    }
}

fn node(game: &Rc<RefCell<Game>>) -> String {
    String::from_utf8(game.borrow().warp_name().to_vec()).unwrap()
}

/// The text up to `PlayedTime` (exact) and the globals (a set).
fn split(text: &[u8]) -> (Vec<u8>, BTreeSet<Vec<u8>>) {
    let at = text.windows(11).position(|w| w == b"<PlayedTime").unwrap();
    let end = at + text[at..].iter().position(|&c| c == b'\n').unwrap() + 1;
    (text[..end].to_vec(), global_lines(text))
}

#[test]
fn save_load_save_gives_the_same_save() {
    let Some(dir) = install() else { return };
    let Run {
        game,
        mut script,
        clock: _clock,
    } = boot(&dir);
    {
        let mut g = game.borrow_mut();
        g.add_to_bag("COUTEAU", false);
        g.add_to_bag("CASQUE_C17", false);
        g.gui();
        g.documents.add("TutoInventaire");
    }
    script.run_chunk(
        b"TEST_S = \"it's \\\"q\\\" <&> caf\\233 tab\\tnl\\n\" TEST_N = 12.5 TEST_B = true TEST_F = false",
        b"test",
    );
    script.change_warp(b"3D/Vaisseau_LM/0002.wp").unwrap();
    game.borrow_mut().view.set_yaw(37.5);
    game.borrow_mut().view.set_pitch(-12.25);
    let g1 = script.globals().unwrap();
    assert!(game.borrow_mut().save_to("4.xml", &g1));
    let text1 = game.borrow().last_save.clone().unwrap().1;
    // The file is the text with CRLF.
    assert_eq!(
        game.borrow().backup.read("Backup/4.xml").unwrap(),
        saves::to_disk(&text1)
    );
    // `Game::leave` (the autosave), then the load: no `OnWarpLeave`, no
    // autosave from the load's node change.
    Game::leave(&game, Some(&script));
    assert_eq!(game.borrow().last_save.as_ref().unwrap().0, "save.xml");
    drop(script);
    {
        let mut g = game.borrow_mut();
        g.last_save = None;
        g.trace = Some(Vec::new());
    }
    let mut script = Game::load_from(&game, None, Options::default(), "4.xml").unwrap();
    {
        let mut g = game.borrow_mut();
        let trace = g.trace.take().unwrap();
        assert!(g.last_save.is_none(), "the load saved");
        assert!(!trace.iter().any(|l| l == "ENTER OnWarpLeave"));
        assert!(trace.iter().any(|l| l == "ENTER OnWarpEnter"));
        // Game::load's order: OnGameEnter, then the diary and inventory
        // scripts, then the node.
        let files: Vec<&String> = trace.iter().filter(|l| l.starts_with("FILE ")).collect();
        assert_eq!(
            files,
            [
                "FILE 3D/OnGameEnter.lua",
                "FILE Diary/Diary.lua",
                "FILE Inventory/InventoryScripts.lua",
                "FILE 3D/Vaisseau_LM/0002.lua"
            ]
        );
        assert_eq!(node_of(&g), "3D/Vaisseau_LM/0002.wp");
        assert_eq!((g.view.yaw, g.view.pitch), (37.5, -12.25));
        assert_eq!(g.inventory.count("COUTEAU"), 1);
        assert_eq!(
            g.sounds.music.path(),
            "Sounds/Ambiances/LIMULE_ANGOISSE.ogg"
        );
        assert!(g.sounds.music.is_playing());
    }
    let g2 = script.globals().unwrap();
    assert!(game.borrow_mut().save_to("4.xml", &g2));
    let text2 = game.borrow().last_save.clone().unwrap().1;
    let (head1, set1) = split(&text1);
    let (head2, set2) = split(&text2);
    assert_eq!(
        String::from_utf8_lossy(&head1),
        String::from_utf8_lossy(&head2)
    );
    assert_eq!(set1, set2);
    assert!(set1.contains(
        b"<String name=\"TEST_S\" value='it&apos;s &quot;q&quot; &lt;&amp;&gt; caf\xe9 tab&#x09;nl&#x0A;' />"
            .as_slice()
    ));
    // The script state answers as before.
    script.run_chunk(
        b"assert(TEST_N == 12.5 and TEST_B and TEST_F == false)",
        b"t",
    );
}

fn node_of(g: &Game) -> String {
    String::from_utf8(g.warp_name().to_vec()).unwrap()
}

#[test]
fn a_broken_or_missing_save_does_as_the_original() {
    let Some(dir) = install() else { return };
    let Run { game, script, .. } = boot(&dir);
    game.borrow_mut()
        .backup
        .write("Backup/3.xml", b"<Inventory>");
    // Broken: OnGameEnter.lua only, no node.
    let s = Game::load_from(&game, Some(&script), Options::default(), "3.xml").unwrap();
    assert!(game.borrow().warp.is_none());
    assert_eq!(node(&game), "");
    // Missing: a new game.
    let _s = Game::load_from(&game, Some(&s), Options::default(), "2.xml").unwrap();
    assert_eq!(node(&game), "3D/Bateau_CA/0001.wp");
}

/// The six chapter saves: each node's first entry writes `N.xml`, the
/// loading menu's slot `N` shows it, and loading it enters that node.
#[test]
fn the_chapter_saves_are_written_and_loaded() {
    let Some(dir) = install() else { return };
    let Run {
        game,
        mut script,
        clock,
    } = boot(&dir);
    // A minute played.
    *clock.borrow_mut() += 60_000_000;
    let chapters = [
        (0, "3D/EntreeC17_C2/0002.wp"),
        (1, "3D/Village49_VI/0015.wp"),
        (2, "3D/Domaine_MB/0001.wp"),
        (3, "3D/JA_DieuxLand/0024.wp"),
        (4, "3D/LZ/0006.wp"),
        (5, "3D/TA/0031.wp"),
    ];
    assert!(game.borrow().chapter_slots().iter().all(|s| !s.present));
    for (n, warp) in chapters {
        script.change_warp(warp.as_bytes()).unwrap();
        // A second entry does not write it again.
        game.borrow_mut().last_save = None;
        script.change_warp(warp.as_bytes()).unwrap();
        assert_ne!(
            game.borrow().last_save.as_ref().map(|s| s.0.clone()),
            Some(format!("{n}.xml"))
        );
    }
    let slots = game.borrow().chapter_slots();
    assert_eq!(slots.len(), 6);
    for s in &slots {
        assert!(s.present && s.parsed, "{s:?}");
        assert_eq!(s.played_time, Some(60.0));
    }
    for (n, warp) in chapters {
        script = Game::load_from(
            &game,
            Some(&script),
            Options::default(),
            &format!("{n}.xml"),
        )
        .unwrap();
        assert_eq!(node(&game), warp);
        // The save was made before its flag was set: entering again
        // writes it again, as the original.
        assert_eq!(
            game.borrow().last_save.as_ref().map(|s| s.0.clone()),
            Some(format!("{n}.xml"))
        );
    }
    // The main menu's new game removes them all.
    assert!(game.borrow().has_autosave());
    let _s = Game::start_new_game(&game, Some(&script), Options::default()).unwrap();
    assert!(game.borrow().chapter_slots().iter().all(|s| !s.present));
    assert_eq!(node(&game), "3D/Bateau_CA/0001.wp");
}

/// Runs the game until `done`: movies end at once (`OnMovieFinished`),
/// otherwise the clock goes on by a second (sounds, animations, dialog
/// lines and their callbacks). Returns the movies played, in order.
fn run_until(r: &mut Run, done: impl Fn(&Game, &[String]) -> bool) -> Vec<String> {
    let mut movies = Vec::new();
    for _ in 0..400 {
        if done(&r.game.borrow(), &movies) {
            return movies;
        }
        let playing = r.game.borrow().movie.as_ref().map(|m| m.path.clone());
        if let Some(p) = playing {
            movies.push(p);
            *r.clock.borrow_mut() += 1_000_000_000;
            let now = *r.clock.borrow() as f64 / 1000.0;
            let ended = r.game.borrow_mut().update_movie(now);
            if let Some(name) = ended {
                r.script
                    .callback("OnMovieFinished", &[Value::Str(name.into_bytes())])
                    .unwrap();
            }
            continue;
        }
        *r.clock.borrow_mut() += 1_000_000;
        let now = *r.clock.borrow() as f64 / 1000.0;
        let finished = r.game.borrow_mut().update(now);
        let sounds = r.game.borrow_mut().sound_callbacks();
        for (n, a) in sounds {
            r.script.callback(&n, &a).unwrap();
        }
        for name in finished {
            r.script
                .callback("OnAnimationFinished", &[Value::Str(name.into_bytes())])
                .unwrap();
        }
        let dialogs = r.game.borrow_mut().dialog_callbacks();
        for (n, a) in dialogs {
            r.script.callback(&n, &a).unwrap();
        }
        r.game.borrow_mut().update_gui(now);
        let cbs = r.game.borrow_mut().gui_callbacks();
        for (n, a) in cbs {
            r.script.callback(&n, &a).unwrap();
        }
    }
    panic!("not reached; movies {movies:?}, node {}", node(&r.game));
}

/// The nine reachable lose movies of the node scripts: each from its
/// node and condition, through the scripts and the engine (clicks,
/// finite animations, dialog lines, movies) to the node it returns to.
#[test]
fn every_death_plays_its_movie_and_returns_to_its_node() {
    let Some(dir) = install() else { return };
    enum Do {
        Enter,
        Hit(&'static str),
    }
    let cases: [(&str, &str, &str, Do, &str, &str); 10] = [
        (
            "zapvillage",
            "TIMER_WARP_ACTIVATED = true TIMER_WARP = 1 VI03_FirstEnter = true Homme_Fourche_parti = true",
            "3D/Village49_VI/0003.wp",
            Do::Enter,
            "Videos/zapvillage.ogv",
            "3D/Village49_VI/0009.wp",
        ),
        (
            "zapforet",
            "MB_Rencontre_Miranda = false MI_FUITE_ACTIVE = true",
            "3D/Foret_MI/0001.wp",
            Do::Hit("3D\\Village49_VI\\0015.wp"),
            "Videos/zapforet.ogv",
            "3D/Foret_MI/0002.wp",
        ),
        (
            "zapaerogare (a guard's animation ends)",
            "ER_FirstEnter = true",
            "3D/ER_ZoneEmbarcation/0001.wp",
            Do::Enter,
            "Videos/zapaerogare.ogv",
            "3D/ER_ZoneEmbarcation/0001.wp",
        ),
        (
            "zapaerogare (the guarded exit)",
            "ER_GardeW8_en_Place = true",
            "3D/ER_ZoneEmbarcation/0007.wp",
            Do::Hit("3D\\ER_ZoneEmbarcation\\0008.wp"),
            "Videos/zapaerogare.ogv",
            "3D/ER_ZoneEmbarcation/0001.wp",
        ),
        (
            "mortgrottex",
            "V0_Tronc_OPEN = true",
            "3D/Grotte_V0/0006.wp",
            Do::Hit("3D\\Grotte_V0\\0002.wp"),
            "Videos/mortgrottex.ogv",
            "3D/Grotte_V0/0002.wp",
        ),
        (
            "mortplante",
            "MB_PLANTE_NEUTRALISE = false",
            "3D/Domaine_MB/0017.wp",
            Do::Hit("3D\\Domaine_MB\\0018.wp"),
            "Videos/mortplante.ogv",
            "3D/Domaine_MB/0016.wp",
        ),
        (
            "jaws",
            "A1_MONSTRE_LOIN = false",
            "3D/Caverne_A1/0002.wp",
            Do::Hit("3D\\Caverne_A1\\0003.wp"),
            "Videos/jaws.ogv",
            "3D/Caverne_A1/0001.wp",
        ),
        (
            "morthangar",
            "HA_SAC_Taken = false",
            "3D/HA_VaisseauCargo/0006.wp",
            Do::Hit("3D\\HA_VaisseauCargo\\0007.wp"),
            "Videos/morthangar.ogv",
            "3D/HA_VaisseauCargo/0003.wp",
        ),
        (
            "bulleperdu",
            "C1_CASQUE_C17_Taken = false",
            "3D/Centre17_C1/0008.wp",
            Do::Hit("3D\\Reincertion"),
            "Videos/bulleperdu.ogv",
            "3D/Centre17_C1/0001.wp",
        ),
        (
            "fin_perdu",
            "TA_VIEUX_IN = 'TA14' TA14_PANTERE_ACTIVE = false",
            "3D/TA/0014.wp",
            Do::Hit("dfa1"),
            "Videos/fin_perdu.ogv",
            "3D/TA/0016.wp",
        ),
    ];
    let mut r = boot(&dir);
    for (what, setup, start, action, lose, back) in cases {
        r.script.run_chunk(setup.as_bytes(), b"setup");
        r.script.change_warp(start.as_bytes()).unwrap();
        if let Do::Hit(name) = action {
            r.script
                .callback("OnWarpObjectHit", &[Value::Str(name.as_bytes().to_vec())])
                .unwrap();
        }
        let movies = run_until(&mut r, |g, movies| {
            g.movie.is_none() && movies.iter().any(|m| m == lose) && node_of(g) == back
        });
        assert!(movies.iter().any(|m| m == lose), "{what}: {movies:?}");
        assert!(r.game.borrow().warp.is_some(), "{what}: no node");
        // No game over: the next node change autosaves and play goes on.
        eprintln!("{what}: {movies:?} -> {back}");
    }
}

/// `FinishGame` after the epilogue: the planks close over the game in
/// 1,000 ms, then `Game::leave` autosaves at `LZ/0004` and the game is
/// over (the original's main menu follows).
#[test]
fn the_end_closes_the_planks_then_leaves_the_game() {
    let Some(dir) = install() else { return };
    let mut r = boot(&dir);
    r.script.change_warp(b"3D/LZ/0004.wp").unwrap();
    let t0 = *r.clock.borrow() as f64 / 1000.0;
    r.game.borrow_mut().now_ms = t0;
    r.script
        .callback(
            "OnMovieFinished",
            &[Value::Str(b"Videos/epilogue.ogv".to_vec())],
        )
        .unwrap();
    assert!(r.game.borrow().planks.is_some());
    assert!(!r.game.borrow().warp_takes_input());
    let draw = |g: &Rc<RefCell<Game>>| {
        let mut px = vec![0u32; 1365 * 768];
        g.borrow_mut().draw(&mut px, 1365, 768, None);
        px
    };
    let before = draw(&r.game);
    r.game.borrow_mut().update(t0 + 999.0);
    assert!(!r.game.borrow_mut().take_ended());
    r.game.borrow_mut().update(t0 + 1000.0);
    // Closed: the planks cover the middle of the window.
    let after = draw(&r.game);
    assert_ne!(before[384 * 1365 + 682], after[384 * 1365 + 682]);
    assert!(r.game.borrow_mut().take_ended());
    assert!(!r.game.borrow_mut().take_ended());
    Game::leave(&r.game, Some(&r.script));
    let g = r.game.borrow();
    assert_eq!(g.last_save.as_ref().unwrap().0, "save.xml");
    let text = String::from_utf8_lossy(&g.last_save.as_ref().unwrap().1).into_owned();
    assert!(text.contains("<WarpName name=\"3D/LZ/0004.wp\" />"));
    assert!(g.warp.is_none());
    assert!(g.has_autosave());
}
