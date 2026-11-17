//! The music around a cinematic. `AtlHnm_start` (@ 0x004211f0), once the
//! film opened with CryoMovie, stops both music slots at once before the
//! playback (@ 0x0042121b: `push 0; call 0x00433d30`, the slots
//! 0x00759440 / 0x00759444 stopped, freed and zeroed); at the film's end
//! (@ 0x004220cf → 0x00433d00) the last track starts again from its
//! beginning. The travel videos (Hnm_Framer, the other branch
//! @ 0x00421144) leave the music alone.
//!
//! A new game: the Hoggar's chapter 1 starts its music, then cinematics
//! 93, 1 and 2.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::audio::CHANNEL_MUSIC;
use atlantis3_engine::{Game, Mode};

#[test]
fn the_music_stops_under_a_cinematic_and_starts_again_after() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    assert!(g.new_game());
    let mut t = 0.0;
    let mut films = 0;
    for _ in 0..20_000 {
        t += 1.0 / 30.0;
        g.step(t);
        if g.mode() == Mode::Video {
            // Under the film: no music.
            assert!(
                !g.audio.is_playing(CHANNEL_MUSIC),
                "music under the cinematic {:?}",
                g.video().map(|v| v.0.to_string())
            );
            films += 1;
            // Its end restarts the track; the next film of the chain,
            // started at once, stops it again.
            g.finish_video();
        }
        if g.mode() == Mode::Warp && g.video().is_none() && films >= 3 {
            break;
        }
    }
    assert!(films >= 3, "{films} cinematics");
    // After the last one: the track again.
    assert!(g.audio.is_playing(CHANNEL_MUSIC), "the music back");
}

/// The track restarted after a film is the last one that started
/// (0x00759328, written only on 0x00433930's success path
/// @ 0x00433ca0..0x00433ca5; `STOPMUSIK` 0x0044c610 keeps it): chapi031's
/// milky ball runs `StopMusik=1`, `StartMusik=38` (no `Muzik038` in any
/// world) and the unicorn board's film 77; after the film the original
/// plays chapter 31's `music\bagdad\Muzik06.apc` again.
#[test]
fn after_a_film_the_last_track_that_started() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    g.run_command("(monde=3)");
    g.run_command("(StartMusik=6)");
    assert!(g.audio.is_playing(CHANNEL_MUSIC), "Muzik06");
    g.run_command("(StopMusik=1)");
    g.run_command("(StartMusik=38)");
    assert!(!g.audio.is_playing(CHANNEL_MUSIC), "no Muzik038");
    g.run_command("(Cinema=77,fps=25)");
    assert_eq!(g.mode(), Mode::Video);
    g.finish_video();
    assert!(g.audio.is_playing(CHANNEL_MUSIC), "Muzik06 again");
}

/// The start page stops the music in every frame (0x00452000
/// @ 0x0045204d..0x00452050: 0x00433d30(0), both slots stopped at once):
/// after Escape and the in-game menu's `BT_QUIT` the start page is silent;
/// the in-game menu itself keeps the music.
#[test]
fn the_start_page_stops_the_music() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    g.run_command("(monde=1)");
    g.set_world(1);
    assert!(g.load_place("hoggar.wam"));
    g.reset_camera();
    g.enter_at(5, 0.0, 0.0);
    g.run_command("(StartMusik=4)");
    assert!(g.audio.is_playing(CHANNEL_MUSIC));
    let mut t = 100.0;
    g.step(t);
    g.escape();
    assert_eq!(g.mode(), Mode::Menu);
    for _ in 0..3 {
        t += 1.0 / 30.0;
        g.step(t);
    }
    assert!(g.audio.is_playing(CHANNEL_MUSIC), "the in-game menu");
    // `BT_QUIT` of `i_princ` (menu.rs): the start page.
    g.menus_mut().page = atlantis3_engine::menu::PAGE_START;
    t += 1.0 / 30.0;
    g.step(t);
    assert!(!g.audio.is_playing(CHANNEL_MUSIC), "the start page");
}
