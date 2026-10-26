//! The engine's sound glue around videos and music against the original
//! (`tests/golden/sound_glue.txt`, the original's own functions run by
//! the emulator): voice 2 at a video's start and end (FUN_0040bf60,
//! FUN_0040bae0 and the flag @ 0x006d168d set by a film's track that
//! loaded or by a move's `TrajSound`), the "music back to 100 %" request
//! of a video's end that a new music cancels (FUN_0040bb90,
//! FUN_0040bb80), and a film's track read from the disc in the drive only
//! (film 44 silent; film 62 with its track, Tibet keeping the disc in the
//! drive).
//!
//! Each scenario of the golden that the shipped game can reach is played
//! here with the scripts' own commands; its `.end` row is what is
//! compared: whether the video's start played a sound on voice 2, whether
//! its end stopped voice 2, whether voice 2 still plays afterwards, and
//! the music percentage the next warp draw asked for.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped.

use std::path::PathBuf;

use atlantis2_data::A2Files;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::sound::VOICE_EFFECTS;
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/sound_glue.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

/// What a scenario ends with, as the golden's `.end` rows say it.
#[derive(Debug, PartialEq)]
struct End {
    start_play2: bool,
    end_stop2: bool,
    voice2_after: bool,
    draw_percent: Option<i32>,
}

fn golden(scenario: &str) -> End {
    let key = format!("glue.{scenario}.end ");
    let row = GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix(key.as_str()))
        .unwrap_or_else(|| panic!("{scenario} not in the golden"));
    let field = |name: &str| {
        row.split(' ')
            .find_map(|f| f.strip_prefix(name)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("{name} in {row}"))
    };
    assert_eq!(
        GOLDEN
            .lines()
            .find_map(|l| l.strip_prefix(&format!("glue.{scenario}.fatal "))),
        Some("0")
    );
    End {
        start_play2: field("start_play2") == "1",
        end_stop2: field("end_stop2") == "1",
        voice2_after: field("voice2_after") == "playing",
        draw_percent: field("draw_percent").parse().ok(),
    }
}

/// A new game's film ended, then the commands, the films and dialogs
/// they start ended, until the panorama is back.
fn started(files: A2Files, commands: &[&str]) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in commands {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    e
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
    assert_eq!(e.loop_type, LoopType::Warp);
}

/// Tibet's view 2 of chapter 75, music 1 playing at its full volume.
fn in_tibet(files: A2Files) -> Engine {
    let mut e = started(
        files,
        &[
            "(changewam=tibet.wam)",
            "(vue=2)",
            "(chapter=75)",
            "(startmusik=1)",
        ],
    );
    music_settles(&mut e, 1);
    e
}

/// Passes run (a 15 Hz tick every fourth) until music `music` plays at
/// the music volume, its fades done.
fn music_settles(e: &mut Engine, music: i32) {
    for n in 0..4000 {
        if e.music_playing() == Some(music) && e.music.state == 4 && e.music.volume == e.music.max {
            return;
        }
        if n % 4 == 0 {
            e.tick();
        }
        e.pass(None);
    }
    panic!(
        "music {music} does not settle: {:?} state {}",
        e.music_playing(),
        e.music.state
    );
}

/// The glue's observations around one video: `start` starts it (a script
/// command or a click on a move), then the video ends at once and one
/// warp pass runs.
fn around_video(e: &mut Engine, start: impl FnOnce(&mut Engine), after_end: &[&str]) -> End {
    e.trace = true;
    e.events.clear();
    start(e);
    for _ in 0..200 {
        if e.loop_type == LoopType::Video {
            break;
        }
        e.tick();
        e.pass(None);
    }
    assert_eq!(e.loop_type, LoopType::Video, "no video started");
    let start_play2 = e.events.iter().any(|ev| matches!(ev, Event::Sound(_)));
    let before = e.voices().is_playing(VOICE_EFFECTS);
    e.end_video();
    let end_stop2 = before && !e.voices().is_playing(VOICE_EFFECTS);
    for c in after_end {
        e.run_command(c.as_bytes());
    }
    let target = e.music.target;
    e.pass(None);
    let draw_percent = (e.music.target != target).then(|| e.music.target * 100 / e.music.max);
    End {
        start_play2,
        end_stop2,
        voice2_after: e.voices().is_playing(VOICE_EFFECTS),
        draw_percent,
    }
}

fn cinema(n: u32) -> impl FnOnce(&mut Engine) {
    move |e: &mut Engine| e.run_command(format!("(cinema={n})").as_bytes())
}

/// A click on the zone of view 2 that leads to view 3 (a move with a
/// video, 79 frames, `tests/golden/moves_tibet.txt`).
fn move_to_3(e: &mut Engine) {
    let view = e.place().1;
    let z = e
        .wam()
        .view(view)
        .unwrap()
        .zones
        .iter()
        .position(|z| z.kind == 1 && z.value == 3)
        .unwrap();
    let (yaw, pitch) = e.zone_aim(view, z).unwrap();
    e.camera.enter(yaw + 0.01, pitch);
    e.pass(None);
    assert_eq!(e.centre_zone().map(|(i, _)| i), Some(z));
    let t = e.clock() + 10.0;
    e.click(t);
}

/// M5: a video's end asks the next warp draw for the music at 100 %; a
/// new music between them cancels it, the current one again does not.
#[test]
fn a_new_music_cancels_the_music_back_to_full_after_a_film() {
    if game().is_none() {
        return;
    }
    for (scenario, after) in [
        ("film_then_draw", &[][..]),
        ("film_then_startmusik", &["(startmusik=20)"][..]),
        ("film_then_same_music", &["(startmusik=1)"][..]),
    ] {
        let mut e = in_tibet(game().unwrap());
        let got = around_video(&mut e, cinema(8), after);
        let want = golden(scenario);
        assert_eq!(got.draw_percent, want.draw_percent, "{scenario}");
    }
}

/// M5, the whole fade (60 passes a second on the clock): after film 8
/// (the music faded to 20 %) and `startmusik=20`, the old music only fades
/// out, then music 20 starts. The original's FUN_0040bb80 cancels the
/// video end's request (golden `film_then_startmusik`), so no fade back
/// up to 100 % comes between.
#[test]
fn no_swell_between_a_film_and_the_next_music() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files);
    let mut t = e.clock();
    let mut step = |e: &mut Engine| {
        t += PASS;
        e.set_clock(t);
        e.pass(None);
    };
    e.run_command(b"(cinema=8)");
    assert_eq!(e.loop_type, LoopType::Video);
    // The fade to 20 % runs during the video.
    for _ in 0..120 {
        step(&mut e);
    }
    assert_eq!(e.loop_type, LoopType::Video);
    assert_eq!(e.music.volume, e.music.max * 20 / 100);
    e.end_video();
    e.run_command(b"(startmusik=20)");
    assert_eq!(golden("film_then_startmusik").draw_percent, None);
    let mut last = e.music.volume;
    for _ in 0..600 {
        step(&mut e);
        if e.music_playing() == Some(20) {
            return;
        }
        assert!(
            e.music.volume <= last,
            "the old music swells: {last} -> {}",
            e.music.volume
        );
        last = e.music.volume;
    }
    panic!("music 20 never starts: state {}", e.music.state);
}

/// M6: voice 2 is stopped at a video's end only when the video had a
/// sound of its own (a film's track that loaded, a `TrajSound`).
#[test]
fn voice_2_stops_at_a_video_s_end_only_when_the_video_owns_it() {
    if game().is_none() {
        return;
    }
    let films: [(&str, u32); 2] = [
        ("newsound_then_film_without_track", 18),
        ("newsound_then_film_with_track", 8),
    ];
    for (scenario, film) in films {
        let mut e = in_tibet(game().unwrap());
        e.run_command(b"(NewSound=1)");
        assert!(e.voices().is_playing(VOICE_EFFECTS), "{scenario}");
        let got = around_video(&mut e, cinema(film), &[]);
        assert_eq!(got, golden(scenario), "{scenario}");
    }
    let moves: [(&str, &[&str]); 3] = [
        (
            "newsound_then_move_with_trajsound",
            &["(TrajSound=sound512)"],
        ),
        (
            "newsound_then_move_with_missing_trajsound",
            &["(TrajSound=nosuch)"],
        ),
        ("newsound_then_move_without_trajsound", &[]),
    ];
    for (scenario, commands) in moves {
        let mut e = in_tibet(game().unwrap());
        e.run_command(b"(NewSound=1)");
        for c in commands {
            e.run_command(c.as_bytes());
        }
        let mut got = around_video(&mut e, move_to_3, &[]);
        // A move's video ends where the golden's scenario stops (no warp
        // draw compared).
        got.draw_percent = None;
        assert_eq!(got, golden(scenario), "{scenario}");
    }
    // `NEWSOUND` of a file the disc does not have plays nothing.
    let mut e = in_tibet(game().unwrap());
    e.run_command(b"(NewSound=2)");
    assert_eq!(
        e.voices().is_playing(VOICE_EFFECTS),
        golden("newsound_missing").voice2_after
    );
}

/// M7: film 44 runs from disc 4 (`monde=6`, chapter 27); its track is on
/// disc 2 only, so the original plays the film silent, and a sound on
/// voice 2 goes on through it.
#[test]
fn film_44_plays_without_its_track_from_another_disc() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files);
    e.run_command(b"(monde=6)");
    assert_eq!(e.files().current_disc(), 4);
    e.run_command(b"(NewSound=103)");
    assert!(e.voices().is_playing(VOICE_EFFECTS));
    let mut got = around_video(&mut e, cinema(44), &[]);
    got.draw_percent = None;
    assert!(!e
        .events
        .iter()
        .any(|ev| matches!(ev, Event::Sound(s) if s.eq_ignore_ascii_case("cine044.apc"))));
    assert_eq!(got, golden("film_track_not_on_the_current_disc"));
}

/// The disc of the golden's `.disc` row: the current disc @ 0x00656a88
/// when the scenario ends.
fn golden_disc(scenario: &str) -> u8 {
    let key = format!("glue.{scenario}.disc ");
    let row = GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix(key.as_str()))
        .unwrap_or_else(|| panic!("{scenario}.disc not in the golden"));
    row.split(' ')
        .find_map(|f| f.strip_prefix("disc="))
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("disc in {row}"))
}

/// `monde=5` keeps the disc in the drive: chapter 78 leaves Shamballa
/// (`monde=4`, disc 4) with `monde=5` then `cinema=62` (lines 26-27), and
/// `cine062.apc` is on disc 4 only. On one drive the original's
/// FUN_0040e0e0 finds disc 4 there and plays the film with its track
/// (golden `tibet_after_shamballa_one_drive`).
#[test]
fn film_62_after_shamballa_plays_its_track_from_the_disc_in_the_drive() {
    let Some(files) = game() else { return };
    let scenario = "tibet_after_shamballa_one_drive";
    let mut e = in_tibet(files);
    e.run_command(b"(monde=4)");
    assert_eq!(e.files().current_disc(), 4);
    e.run_command(b"(monde=5)");
    assert_eq!(e.files().current_disc(), golden_disc(scenario));
    let mut got = around_video(&mut e, cinema(62), &[]);
    got.draw_percent = None;
    assert!(e
        .events
        .iter()
        .any(|ev| matches!(ev, Event::Sound(s) if s.eq_ignore_ascii_case("cine062.apc"))));
    assert_eq!(got, golden(scenario));
}
