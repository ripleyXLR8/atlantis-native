//! *Atlantis Evolution*'s 2004 sound effects and disc banks
//! (`ATLANTIS4_CD`): `PLAYSOUND` / `STOPSOUND` through `#SONS#` and
//! `AtlSound` (FUN_0042ef10, FUN_0042f6d0, FUN_0042fdd0), the place
//! animations' frame-0 sounds (FUN_00439fc0) and `CHANGECD`'s banks
//! (FUN_00452310), in the ship's cabin (`cabine.wam` view 1, chapter 1).

use std::sync::Arc;

use atlantis3_data::evolution2004::Evolution2004Source;
use atlantis3_data::A3Files;
use atlantis3_engine::audio::{CHANNEL_SFX3D_2004, CHANNEL_SOUND};
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

#[test]
fn playsound_plays_the_sons_ogg_and_stopsound_frees_it() {
    let Some((mut g, _)) = cabin() else { return };
    let door = "sfx\\PORTE_FERME.wav";
    assert_eq!(g.sfx_2004_state(door), (false, false));
    // `#SONS#`: SON_PORTE_FERME,PORTE_FERME.wav → cd0\datas\sfx\PORTE_FERME.ogg.
    g.run_command("(playsound=SON_PORTE_FERME)");
    assert_eq!(g.sfx_2004_state(door), (true, true));
    assert!(g.audio.is_playing(CHANNEL_SOUND));
    g.run_command("(stopsound=SON_PORTE_FERME)");
    assert_eq!(g.sfx_2004_state(door), (false, false));
    assert!(!g.audio.is_playing(CHANNEL_SOUND));
    // A name whose file is not on the CD (`cheewie.wav`): nothing loaded.
    g.run_command("(playsound=DANGER_CANON)");
    assert_eq!(g.sfx_2004_state("sfx\\cheewie.wav"), (false, false));
    assert!(!g.audio.is_playing(CHANNEL_SOUND));
}

#[test]
fn the_porthole_opening_sounds_at_its_first_frame() {
    let Some((mut g, mut t)) = cabin() else {
        return;
    };
    let a2 = g
        .slots()
        .slots
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case("hua2001002"))
        .expect("the opening");
    // The WAM's case (`huA2001002`): the program's path is
    // `sfx\huA2.ogg`, which Windows finds as `hua2.ogg`.
    let hua2 = format!("sfx\\{}.ogg", &g.slots().slots[a2].name[..4]);
    let hua2 = hua2.as_str();
    // Nothing yet: the open porthole `A3` (hua3001002) has no hua3.ogg.
    for _ in 0..5 {
        t += 0.04;
        g.step(t);
    }
    assert_eq!(g.sfx_2004_state(hua2), (false, false));
    let (id, kind) = (g.slots().slots[a2].id, g.slots().slots[a2].kind);
    // chapi001 /009: `playcutanimo=HUBLOT_OUVERT,2`.
    g.test_slots_mut().hide(id, kind, t);
    g.test_slots_mut().show(id, kind, 2, t);
    t += 0.01;
    g.step(t);
    // Frame 0 drawn: `sfx\hua2001002.ogg` missing, `sfx\hua2.ogg` loaded
    // (kind 1, 3D) and played on the first 3D channel.
    assert_eq!(g.sfx_2004_state(hua2), (true, true));
    assert!(g.audio.is_playing(CHANNEL_SFX3D_2004));
    // Past frame 0 it plays on (22292 bytes, about 1.3 s), not started
    // again on another channel.
    for _ in 0..10 {
        t += 0.04;
        g.step(t);
    }
    assert!(g.slots().slots[a2].frame > 0 || !g.slots().slots[a2].shown);
    assert_eq!(g.sfx_2004_state(hua2), (true, true));
    assert!(!g.audio.is_playing(CHANNEL_SFX3D_2004 + 1));
}

#[test]
fn changecd_loads_the_disc_banks() {
    let Some((mut g, _)) = cabin() else { return };
    // chapi000: `changecd=1` → spr\invent01.spr (objects and hand) and,
    // at 1024 x 768, spr\suj01.spr.
    let [objects, hand, subjects] = g.object_banks();
    assert!(objects.is_some_and(|n| n > 0), "{objects:?}");
    assert_eq!(objects, hand);
    assert!(subjects.is_some_and(|n| n > 0), "{subjects:?}");
    g.run_command("(changecd=4)");
    let [objects4, _, subjects4] = g.object_banks();
    assert!(objects4.is_some() && subjects4.is_some());
    assert_ne!((objects4, subjects4), (objects, subjects));
}
