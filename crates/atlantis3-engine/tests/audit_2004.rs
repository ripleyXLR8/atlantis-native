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
