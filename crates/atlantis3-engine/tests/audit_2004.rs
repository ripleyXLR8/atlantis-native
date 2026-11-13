//! *Atlantis Evolution*'s 2004 CD program (`ATLANTIS4_CD`): the rules the
//! audits of the port found different from the original
//! (atlantis4-cd-en-3022848), each checked on the game in the cabin after
//! the intro (chapter 1, `cabine.wam` view 1).

use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::game::{Game, Mode};

fn game() -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS4_CD").map(std::path::PathBuf::from) else {
        atlantis3_data::testing::missing("ATLANTIS4_CD not set");
        return None;
    };
    let src = Arc::new(Evolution2004Source::new(Arc::new(
        atlantis_data::StdDir::new(&dir),
    )));
    Some(Game::new(A3Files::evolution_2004(src)).expect("the 2004 program's game"))
}

/// The cabin's view 1 after the intro.
fn cabin() -> Option<(Game, f64)> {
    let mut g = game()?;
    assert!(g.new_game());
    g.step(0.0);
    g.skip();
    g.step(0.01);
    assert_eq!(g.place(), ("cabine.wam", Some(1)));
    assert_eq!(g.mode(), Mode::Warp);
    Some((g, 0.01))
}

fn var(g: &Game, name: &str) -> i32 {
    g.script()
        .and_then(|s| s.var(name))
        .unwrap_or_else(|| panic!("{name}"))
}

fn flag(g: &Game, n: usize) -> i32 {
    g.script().unwrap().flags[n]
}

/// 25 frames a second for `secs`.
fn run(g: &mut Game, t: &mut f64, secs: f64) {
    let end = *t + secs;
    while *t < end {
        *t += 0.04;
        g.step(*t);
    }
}

/// A shown place-animation slot in the cabin (an object's: the cabin
/// shows no character) and the `#PERSONNAGES#` name of the same id: the
/// slot lookups of `NUMFRAMEPERSO` and `GOTOFRAMEPERSO` compare the id
/// only, whatever the slot's kind (@ 0x0044cc28, 0x0044fdb6).
fn shown_perso(g: &Game) -> (usize, String) {
    let k = g
        .slots()
        .slots
        .iter()
        .position(|s| {
            s.shown
                && usize::try_from(s.id)
                    .is_ok_and(|i| g.names().name(defnames::PERSONNAGES, i).is_some())
        })
        .expect("a slot shown in the cabin");
    let id = g.slots().slots[k].id as usize;
    let name = g.names().name(defnames::PERSONNAGES, id).unwrap();
    (k, String::from_utf8_lossy(name).into_owned())
}

/// `FLAGn++`, `--`, `+=`, `-=` (@ 0x0044c904..0x0044c9f4): 127 lines
/// `flagN++` and 47 `flagN+=1` in the scripts; `flagN=NAME` stores
/// nothing (the `isdigit` test @ 0x0044c966).
#[test]
fn flag_operators() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(flag17=5)");
    g.run_command("(flag17++)");
    assert_eq!(flag(&g, 17), 6);
    g.run_command("(flag17+=3)");
    assert_eq!(flag(&g, 17), 9);
    g.run_command("(flag17-=2)");
    assert_eq!(flag(&g, 17), 7);
    g.run_command("(flag17--)");
    assert_eq!(flag(&g, 17), 6);
    g.run_command("(FLAG17=CHAPTER)");
    assert_eq!(flag(&g, 17), 6);
    g.run_command("(flag17=12)");
    assert_eq!(flag(&g, 17), 12);
}

/// A variable's `+=` and `-=` (@ 0x0044cb73, 0x0044cc7d): a number, a
/// variable's name (chapi043 `ETOILE_TOTAL+=1`, chapi028
/// `DIGICODE_TEST+=`), or `numframeperso,NAME` (chapi025 / chapi026
/// `ANIM_GARDE8+=numframeperso,AEROGARDEW2W4`): the frame of NAME's
/// shown slot.
#[test]
fn variable_add_and_subtract() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(ETOILE_TOTAL=4)");
    g.run_command("(ETOILE_TOTAL+=3)");
    assert_eq!(var(&g, "ETOILE_TOTAL"), 7);
    g.run_command("(ETOILE_TOTAL-=2)");
    assert_eq!(var(&g, "ETOILE_TOTAL"), 5);
    g.run_command("(DIGICODE_TEST=10)");
    g.run_command("(DIGICODE_TEST+=ETOILE_TOTAL)");
    assert_eq!(var(&g, "DIGICODE_TEST"), 15);
    let (k, name) = shown_perso(&g);
    g.test_slots_mut().slots[k].frame = 7;
    g.run_command("(ANIM_GARDE8=1)");
    g.run_command(&format!("(ANIM_GARDE8+=numframeperso,{name})"));
    assert_eq!(var(&g, "ANIM_GARDE8"), 8);
    // No such character shown: 0 added.
    g.run_command("(ANIM_GARDE8+=numframeperso,AEROGARDEW2W4)");
    assert_eq!(var(&g, "ANIM_GARDE8"), 8);
}

/// `VUE=` (@ 0x0044e185..0x0044e1be): `ARRIVEEVUE` := the new view.
#[test]
fn vue_sets_arriveevue_to_the_new_view() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(vue=2)");
    assert_eq!(var(&g, "ARRIVEEVUE"), 2);
    assert_eq!(var(&g, "VUE"), 2);
}

/// `CREDITS=1` (@ 0x004504c4..0x00450513; chapi041 /005 after
/// `cinema=FIN`): the credits page; Escape there with `CREDITS` = 1
/// (@ 0x0042c94f..0x0042c9a1): `CREDITS` 0, the start page, the waiting
/// section goes on; with another value Escape quits.
#[test]
fn credits_then_escape() {
    let Some((mut g, _)) = cabin() else { return };
    let text = b"/INIT\r\n/FININIT\r\n\r\n/001\r\n/con(flag5=0)\r\n/set(credits=1)\r\n\
                 /set(flag5=1)\r\n/000\r\n";
    assert!(g.script_mut().unwrap().set_chapter_text(text));
    g.ctrl_scenar_in();
    assert!(g.credits().is_some(), "the credits page");
    assert_eq!(g.mode(), Mode::Menu);
    assert_eq!(var(&g, "CREDITS"), 1);
    assert_eq!(flag(&g, 5), 0, "the section waits");
    assert!(!g.credits_escape(), "no quit");
    assert!(g.credits().is_none());
    assert_eq!(var(&g, "CREDITS"), 0);
    assert_eq!(g.menu_page().map(|p| p.0), Some(0), "the start page");
    assert_eq!(flag(&g, 5), 1, "the section went on");
    // Another value: Escape quits.
    g.run_command("(credits=2)");
    assert!(g.credits().is_some());
    assert!(g.credits_escape());
}

/// `GOTOFRAMEPERSO=NAME,VAR` (@ 0x0044fd8c..0x0044fd9b): `atol` 0 and not
/// `0`: the frame is the variable's value (chapi025 `ANIM_GARDE8`).
#[test]
fn gotoframeperso_from_a_variable() {
    let Some((mut g, _)) = cabin() else { return };
    let (k, name) = shown_perso(&g);
    g.run_command("(ANIM_GARDE8=3)");
    g.run_command(&format!("(GOTOFRAMEPERSO={name},ANIM_GARDE8)"));
    assert_eq!(g.slots().slots[k].frame, 3);
    g.run_command(&format!("(GOTOFRAMEPERSO={name},0)"));
    assert_eq!(g.slots().slots[k].frame, 0);
}

/// `ANGLEWARPY` of 500 or more is 500 − v (@ 0x0044dc2c; chapi030
/// `anglewarpy=510`, `523`); `ANGLEWARPX` is not changed.
#[test]
fn anglewarpy_above_500() {
    let Some((mut g, _)) = cabin() else { return };
    let y = |g: &Game| g.script().unwrap().angle_y;
    g.run_command("(anglewarpy=-10)");
    let minus10 = y(&g);
    g.run_command("(anglewarpy=0)");
    g.run_command("(anglewarpy=510)");
    assert_eq!(y(&g), minus10);
    g.run_command("(anglewarpy=-23)");
    let minus23 = y(&g);
    g.run_command("(anglewarpy=523)");
    assert_eq!(y(&g), minus23);
}

/// The second's tick (FUN_00451a20): `RND` := rand() % 100 (chapi011's
/// `rnd>50`).
#[test]
fn rnd_every_second() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.set_var("RND", -1);
    g.queue_rand(1234);
    run(&mut g, &mut t, 1.2);
    assert_eq!(var(&g, "RND"), 34);
}

/// `PLAYERNAME=` stores the perso's index in the player list
/// (FUN_0040eca0 @ 0x0040eccc); the loader sets the perso of that index
/// again (@ 0x004645af..0x004645d7).
#[test]
fn playername_index_and_load() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(PLAYERNAME=HewittCosmo)");
    assert_eq!(g.player_2004(), "HewittCosmo");
    assert_eq!(var(&g, "PLAYERNAME"), 8);
    let name = g.quick_save_now(1);
    let data = g.saved(&name).unwrap().to_vec();
    g.run_command("(PLAYERNAME=Hewitt)");
    assert_eq!(g.player_2004(), "Hewitt");
    assert_eq!(var(&g, "PLAYERNAME"), 0);
    assert!(g.load_save_text(&name, data));
    assert_eq!(var(&g, "PLAYERNAME"), 8);
    assert_eq!(g.player_2004(), "HewittCosmo");
}

/// The 2004 program never reads or writes the flags 897 and 1000..1031
/// (*Atlantis III*'s per-world slots); its scripts use 1001 and 1011 as
/// counters.
#[test]
fn no_world_slot_flags() {
    let Some((mut g, _)) = cabin() else { return };
    for (f, v) in [(897, 4), (1001, 5), (1011, 3), (1021, 2)] {
        g.run_command(&format!("(flag{f}={v})"));
    }
    g.run_command("(vue=2)");
    g.run_command("(changewam=cabine.wam)");
    g.run_command("(vue=1)");
    assert_eq!(
        [897, 1001, 1011, 1021].map(|f| flag(&g, f)),
        [4, 5, 3, 2],
        "the view, place and chapter slots"
    );
}

/// The loader: the followers emptied (FUN_0043d710 @ 0x00460cd7) and the
/// pitching stopped unless `TANGAGEWARP` = 1 (@ 0x004648ba..0x004648d3;
/// `TANGAGEWARP=` stores nothing, so it stops).
#[test]
fn the_loader_clears_followers_and_pitching() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(ADDSUIVEUR=MIRANDA)");
    g.run_command("(tangagewarp=1,0,0,1,2,3,0)");
    assert!(g.ship().on);
    let name = g.quick_save_now(1);
    let data = g.saved(&name).unwrap().to_vec();
    assert!(g.load_save_text(&name, data));
    assert!(g.followers_2004().is_empty());
    assert!(!g.ship().on);
}

/// A new game clears the hide table (0x006fcc18, its count 0x006ffc18
/// := 0 @ 0x00457b16).
#[test]
fn new_game_clears_the_hide_table() {
    let Some((mut g, _)) = cabin() else { return };
    g.run_command("(hideperso=MIRANDA)");
    let miranda = g.names().id(defnames::PERSONNAGES, b"MIRANDA").unwrap() as i32;
    let has = |g: &Game| g.script().unwrap().shown.iter().any(|r| r.id == miranda);
    assert!(has(&g));
    assert!(g.new_game());
    assert!(!has(&g));
}

/// `CHANGECD=` stores nothing (@ 0x00450919..0x0045093a); `MONDE=` is a
/// plain store (no branch); `PLAYVOICE=` reads five numbers (@ 0x0044ec99).
#[test]
fn changecd_monde_playvoice() {
    let Some((mut g, _)) = cabin() else { return };
    g.set_var("CHANGECD", -7);
    g.run_command("(changecd=1)");
    assert_eq!(var(&g, "CHANGECD"), -7);
    g.run_command("(monde=3)");
    assert_eq!(var(&g, "MONDE"), 3);
    assert_eq!(g.script().unwrap().a3.world, 1, "0x0070d3f4 stays 1");
    g.trace = Some(Vec::new());
    g.run_command("(playvoice=1,2,3,4,5)");
    let trace = g.trace.take().unwrap();
    assert!(trace.iter().any(|l| l == "voice 1 2 3 4 5"), "{trace:?}");
}

/// No *Atlantis III* place hook: the view entry leaves `LIMITEANGLEWARPY`
/// at the new game's 50 (no `(LIMITEANGLEWARPY=70)`), `flag147` = 1 starts
/// no Licorne screen (chapi030 sets it).
#[test]
fn no_atlantis3_place_hooks() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    assert_eq!(var(&g, "LIMITEANGLEWARPY"), 50);
    g.run_command("(vue=2)");
    assert_eq!(var(&g, "LIMITEANGLEWARPY"), 50);
    g.run_command("(flag147=1)");
    run(&mut g, &mut t, 0.2);
    assert_eq!(g.puzzle_screen(), None);
    assert_eq!(g.mode(), Mode::Warp);
}

/// The light dialog's pass (FUN_00476320) is in the warp frame's branch
/// of `Interp_GereFrame`, not while a turn runs (0x006e7c2c ≠ 0): a light
/// dialog whose voice ended keeps its line until the turn ends.
#[test]
fn light_dialog_waits_for_the_turn() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.run_command("(dial=153,light)");
    t += 0.04;
    g.step(t);
    let (n, line, _, light) = g.dialog_2004().expect("the light dialog");
    assert!(light);
    let (yaw, _) = g.angles_deg();
    g.run_command(&format!("(anglewarpx={})", (yaw as i32 + 180) % 360));
    g.run_command("(INITRECALAGE=1)");
    assert!(g.turning());
    // Every voice ended; the turn runs on (12 of its ticks a step at most).
    t += 60.0;
    g.step(t);
    assert!(g.turning(), "the turn still runs");
    assert_eq!(g.dialog_2004().map(|d| (d.0, d.1)), Some((n, line)));
    while g.turning() {
        t += 0.04;
        g.step(t);
    }
    t += 0.04;
    g.step(t);
    assert_ne!(g.dialog_2004().map(|d| (d.0, d.1)), Some((n, line)));
}

/// `NUMFRAMEVIDEO` := 0 is `AtlBink_Start`'s (@ 0x004028cd..0x004028e9):
/// at the film's start after the cine-in, not at the `CINEMA=` line.
#[test]
fn numframevideo_at_the_film_start() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    g.run_command("(cinema=INTRO)");
    assert!(g.film_cine_2004().is_some(), "the cine-in");
    g.set_var("NUMFRAMEVIDEO", 99);
    while g.film_2004().is_none() {
        t += 0.04;
        g.step(t);
        assert!(t < 5.0);
    }
    assert_eq!(var(&g, "NUMFRAMEVIDEO"), 0);
}

/// A place sprite past pixel 511 of its 768-pixel face: the 2004 centre
/// ray (FUN_0043b810) projects through face cameras of the cube's size
/// (FUN_004c6d70(768): centre 384, viewport 0..767). `village.wam` view
/// 9's `GAMEBOY` (`gbA1009009`, face 5, x 180..277, y 538..688) is
/// clicked into the bag and the hand.
#[test]
fn a_sprite_low_on_a_768_face() {
    let Some((mut g, mut t)) = cabin() else { return };
    assert!(g.load_place("village.wam"));
    g.enter_at(9, 0.0, 0.0);
    g.run_command("(showobjet=GAMEBOY)");
    t += 0.04;
    g.step(t);
    let gameboy = g.names().id(defnames::OBJETS, b"GAMEBOY").unwrap() as i32;
    assert!(g.click_sprite(gameboy, 2), "the Gameboy under the centre");
    t += 0.04;
    g.step(t);
    let bag: Vec<i32> = g.script().unwrap().bag.iter().map(|e| e.id).collect();
    assert!(bag.contains(&gameboy), "{bag:?}");
    assert_eq!(g.object_in_hand(), gameboy);
}
