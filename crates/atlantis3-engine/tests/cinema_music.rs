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
