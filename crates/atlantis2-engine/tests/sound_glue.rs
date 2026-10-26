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

use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::music;
use atlantis2_engine::sound::{VOICE_EFFECTS, VOICE_MUSIC};
use atlantis2_engine::{Engine, Event, Fpu, LoopType, Variant};

const GOLDEN: &str = include_str!("golden/sound_glue.txt");
const FADE_027F: &str = include_str!("golden/music_fade_027f.txt");
const FADE_0E7F: &str = include_str!("golden/music_fade_0e7f.txt");
const MUSIC: &str = include_str!("golden/music.txt");

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
    for _ in 0..4000 {
        if e.music_playing() == Some(music) && e.music.state == 4 && e.music.volume == e.music.max {
            return;
        }
        // The clock moves (the fades run on it), a pass a 60th of a
        // second.
        e.set_clock(e.clock() + PASS);
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

/// The golden's `.table` row: the sound table's loaded entries
/// (`index:name`) when the scenario ends, `-` for none.
fn golden_table(scenario: &str) -> String {
    let key = format!("glue.{scenario}.table ");
    GOLDEN
        .lines()
        .find_map(|l| l.strip_prefix(key.as_str()))
        .unwrap_or_else(|| panic!("{scenario}.table not in the golden"))
        .to_string()
}

/// Native's sound table in the golden's form.
fn table(e: &Engine) -> String {
    let rows: Vec<String> = e
        .sound_table()
        .iter()
        .map(|(i, n)| format!("{i}:{n}"))
        .collect();
    if rows.is_empty() {
        "-".into()
    } else {
        rows.join(" ")
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

/// The video end's request is taken by the warp draw (FUN_0040bb90 is
/// called by FUN_0045a9f0 only, atlantis2-cd-fr-2199552 @ 0x0045aa10), so
/// a pass whose draw is skipped (`NextWarpInvalide` = 2,
/// @ 0x0045a3fd..0x0045a41a) keeps it for the next drawn pass.
#[test]
fn a_skipped_draw_keeps_the_music_back_to_full_request() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files);
    e.run_command(b"(cinema=8)");
    assert_eq!(e.loop_type, LoopType::Video);
    e.end_video();
    e.run_command(b"(NextWarpInvalide=2)");
    let target = e.music.target;
    e.pass(None);
    e.pass(None);
    assert_eq!(e.music.target, target, "taken while the screen is frozen");
    e.run_command(b"(NextWarpInvalide=0)");
    e.pass(None);
    assert_eq!(
        Some(e.music.target * 100 / e.music.max),
        golden("film_then_draw").draw_percent
    );
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
        // The harness's table starts empty: Tibet's place sounds out.
        e.sound_table_clear();
        e.run_command(b"(NewSound=1)");
        assert!(e.voices().is_playing(VOICE_EFFECTS), "{scenario}");
        let got = around_video(&mut e, cinema(film), &[]);
        assert_eq!(got, golden(scenario), "{scenario}");
        // L19: the film's track went into the table the `NEWSOUND` is in,
        // and the `NEWSOUND`'s entry stays loaded.
        assert_eq!(table(&e), golden_table(scenario), "{scenario}");
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
        e.sound_table_clear();
        e.run_command(b"(NewSound=1)");
        for c in commands {
            e.run_command(c.as_bytes());
        }
        let mut got = around_video(&mut e, move_to_3, &[]);
        // A move's video ends where the golden's scenario stops (no warp
        // draw compared).
        got.draw_percent = None;
        assert_eq!(got, golden(scenario), "{scenario}");
        // The move's place sounds are the view's: only the scenario's
        // entries are compared.
        let ours: Vec<String> = table(&e)
            .split(' ')
            .filter(|r| r.contains("sound001") || r.contains("sound512"))
            .map(String::from)
            .collect();
        assert_eq!(ours.join(" "), golden_table(scenario), "{scenario}");
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

/// L16 (review 2026-10-25, fixed with M6 in 94f08ac): a film whose video
/// no disc has is a cut that keeps its loaded track and voice 2's flag
/// (FUN_0040bf60 touches neither): the next video, a move, plays that
/// track and frees it at its end (golden `film_does_not_open`). Film 4
/// has a track on disc 1 and no video anywhere; no script starts it.
#[test]
fn a_film_that_does_not_open_keeps_its_track_for_the_next_video() {
    let Some(files) = game() else { return };
    let scenario = "film_does_not_open";
    let mut e = in_tibet(files);
    assert_eq!(e.files().current_disc(), 1);
    e.sound_table_clear();
    e.trace = true;
    e.events.clear();
    e.run_command(b"(cinema=4)");
    assert_eq!(e.loop_type, LoopType::Warp, "a cut");
    assert!(e
        .events
        .iter()
        .any(|ev| matches!(ev, Event::VideoStart { frames: None, .. })));
    assert!(!e.voices().is_playing(VOICE_EFFECTS));
    let mut got = around_video(&mut e, move_to_3, &[]);
    assert!(e
        .events
        .iter()
        .any(|ev| matches!(ev, Event::Sound(s) if s.eq_ignore_ascii_case("cine004.apc"))));
    got.draw_percent = None;
    assert_eq!(got, golden(scenario));
    let ours: Vec<String> = table(&e)
        .split(' ')
        .filter(|r| r.contains("cine004"))
        .map(String::from)
        .collect();
    assert_eq!(ours.join(" "), golden_table(scenario).replace('-', ""));
}

/// L19: one sound table and one handle (@ 0x00673f34) for `NEWSOUND`,
/// the films' and moves' tracks, the place sounds and the puzzles: a
/// track loaded over a `NEWSOUND` that still plays leaves its entry
/// loaded for good, a `NEWSOUND` while voice 2 plays frees the entry the
/// handle names first, and the dispatcher's FUN_0047a2f0 frees the
/// handle's entry once voice 2 is silent (golden `.table` rows; the
/// table starts empty in the harness, so Tibet's place sounds are taken
/// out first).
#[test]
fn newsound_and_the_tracks_share_the_sound_table() {
    if game().is_none() {
        return;
    }
    let voice_ends = |e: &mut Engine| e.voices().stop(VOICE_EFFECTS);
    // newsound sound001, film 8 with its track, its end, a pass, newsound
    // sound512 (voice 2 silent: nothing freed first).
    let mut e = in_tibet(game().unwrap());
    e.sound_table_clear();
    e.run_command(b"(NewSound=1)");
    let _ = around_video(&mut e, cinema(8), &[]);
    e.run_command(b"(NewSound=512)");
    assert_eq!(table(&e), golden_table("newsound_leaks_under_a_film"));
    // newsound sound001, newsound sound512 over it, voice 2 played out,
    // the dispatcher's pass.
    let mut e = in_tibet(game().unwrap());
    e.sound_table_clear();
    e.run_command(b"(NewSound=1)");
    e.run_command(b"(NewSound=512)");
    voice_ends(&mut e);
    e.pass(None);
    assert_eq!(table(&e), golden_table("newsound_over_newsound"));
    // film 8 and its end, then newsound sound001, newsound sound512,
    // voice 2 played out, the pass, newsound sound001 again.
    let mut e = in_tibet(game().unwrap());
    e.sound_table_clear();
    let _ = around_video(&mut e, cinema(8), &[]);
    e.run_command(b"(NewSound=1)");
    e.run_command(b"(NewSound=512)");
    voice_ends(&mut e);
    e.pass(None);
    e.run_command(b"(NewSound=1)");
    assert_eq!(
        table(&e),
        golden_table("newsound_after_a_film_frees_its_freed_entry")
    );
}

/// The music stopped (`stopmusik`: the fade to 0, then the stop), passes
/// run until the player is idle.
fn music_stopped(e: &mut Engine) {
    e.run_command(b"(stopmusik)");
    for _ in 0..4000 {
        if e.music.state == 0 {
            return;
        }
        e.set_clock(e.clock() + PASS);
        e.pass(None);
    }
    panic!("the music does not stop: state {}", e.music.state);
}

/// L23: the original runs its music state machine (FUN_00476e10) inside
/// its long loads: at the panorama load of a view's entry (eight times),
/// before each place animation, around a video's start and end and
/// before each transition sprite bank. A music asked for before a `vue=`
/// opens during that view's entry, not at the next pass.
#[test]
fn a_music_asked_for_before_a_view_entry_opens_during_it() {
    let Some(files) = game() else { return };
    let mut e = in_tibet(files);
    music_stopped(&mut e);
    // Music 20's copy taken as made (no please-wait page).
    e.media.copies.insert("20".into());
    e.run_command(b"(startmusik=20)");
    assert_eq!(e.music.state, 2, "an open at a mark, pending");
    e.run_command(b"(vue=3)");
    assert_eq!(e.place().1, 3);
    assert_eq!(e.music.state, 4, "opened during the entry");
    assert_eq!(e.music_playing(), Some(20));
}

/// L22: the fade's arithmetic runs under the process's x87 control word
/// (the 15 Hz task and the dispatcher run under the word the frame left:
/// 0x0e7f once the Pentium II warp has drawn), not the start-up one: in
/// Tibet `cinema=` arms the fade to 20 % with the duration of
/// `music_fade_0e7f.txt` (row 0253: music volume 64, to 12), one unit
/// below 0x027f's.
#[test]
fn the_music_fade_runs_under_the_current_control_word() {
    let Some(files) = game() else { return };
    let duration = |golden: &str| {
        golden
            .lines()
            .find_map(|l| l.strip_prefix("0253.armed "))
            .and_then(|r| r.split(' ').find_map(|f| f.strip_prefix("duration=")))
            .map(|d| u32::from_str_radix(d, 16).unwrap())
            .unwrap()
    };
    let (want, other) = (duration(FADE_0E7F), duration(FADE_027F));
    assert_ne!(want, other);
    let mut e = in_tibet(files);
    assert_eq!(e.camera.fpu, Fpu::Chop);
    e.music.max = 64;
    e.music.volume = 64;
    e.music.state = 4;
    e.run_command(b"(cinema=8)");
    assert_eq!(e.music.target, 12);
    assert_eq!(e.music.duration.to_bits(), want);
}

/// L18: a random mark (`startmusik=20`, its entry without `-mark`) drawn
/// at the `rand()` = 0x7fff edge is one past the file's marks
/// (`music.txt`'s `randedge` rows), which `AtlMuzik_GotoMark` reports as
/// a fatal error (the original saves and quits; Native stops the music
/// and the scripts). With the minor bug fixes the music plays from its
/// last mark.
#[test]
fn a_random_mark_at_the_rand_edge() {
    if game().is_none() {
        return;
    }
    let marks = {
        let files = game().unwrap();
        let spp = files
            .read(Folder::ApcMusic, &music::spp_name(20))
            .expect("muzik20.spp");
        music::parse_spp(&spp).unwrap().len() as i32
    };
    let edge = MUSIC
        .lines()
        .find_map(|l| l.strip_prefix(&format!("randedge.n{marks} ")))
        .map(|d| d.parse::<i32>().unwrap());
    let mut r = Rand::new(0xf01b_f641);
    let draw = music::random_mark(&mut r, marks, 0);
    assert_eq!(draw, marks + 1);
    if let Some(edge) = edge {
        assert_eq!(draw, edge);
    }
    for fixes in [false, true] {
        let mut e = in_tibet(game().unwrap());
        e.enhancements.minor_bug_fixes = fixes;
        music_stopped(&mut e);
        e.media.copies.insert("20".into());
        e.run_command(b"(startmusik=20)");
        assert_eq!(e.music.state, 2);
        e.rand = Rand::new(0xf01b_f641);
        e.trace = true;
        e.events.clear();
        e.pass(None);
        if fixes {
            assert_eq!(e.music.state, 4);
            assert!(e
                .events
                .iter()
                .any(|ev| matches!(ev, Event::Music { music: 20, mark } if *mark == marks)));
            assert!(e.script_state().unwrap().fatal.is_none());
        } else {
            assert_eq!(e.music.state, 0);
            assert!(!e.voices().is_playing(VOICE_MUSIC));
            let fatal = e.script_state().unwrap().fatal.clone().unwrap_or_default();
            assert!(fatal.starts_with("AtlMuzik_GotoMark"), "{fatal}");
        }
    }
}

/// L17: a load puts the save's music list back (entry 12 is the music
/// player's own list, `Interp_MusikScript` @ 0x006d0444, copied back with
/// the globals) even when the save's music is 0 and nothing new starts
/// (FUN_004785a0 @ 0x004785df): the music playing goes on, and its next
/// track is drawn from the save's list.
#[test]
fn a_load_without_music_puts_the_save_s_list_back() {
    use atlantis2_data::save::entry;
    let Some(files) = game() else { return };
    let mut e = in_tibet(files);
    assert_ne!(e.music_list(), b"5,6)");
    let mut d = e.save_data();
    d.set_i32(entry::MUSIC, 0);
    let list = d.entry_mut(entry::MUSIC_LIST);
    list.fill(0);
    list[..4].copy_from_slice(b"5,6)");
    e.load_data(&d).unwrap();
    assert_eq!(e.music_list(), b"5,6)");
    assert_eq!(e.music_playing(), Some(1));
}

/// Review N1: the music state machine (FUN_00476fc0) runs on every
/// dispatcher pass (@ 0x0049ce32) and once more in the menu frame
/// (@ 0x00486ecf, through the thunk @ 0x0047a280): in a menu, a fade's
/// end (the volume at its target) moves the machine on in the same pass,
/// so `stopmusik`'s stop (state 5, FUN_004778f0) always comes in the pass
/// after the one that reached the target. In the panorama, with one call
/// a pass, it comes a pass later unless the 15 Hz task's fade step
/// (@ 0x0040caa0, after the frame) takes the state on in between: the
/// fade is started at the four phases of the task.
#[test]
fn the_music_machine_runs_twice_a_pass_in_the_menus() {
    if game().is_none() {
        return;
    }
    let mut gaps = [Vec::new(), Vec::new()];
    for in_menu in [false, true] {
        for phase in 0..4 {
            let mut e = in_tibet(game().unwrap());
            if in_menu {
                e.open_main_menu();
                assert_eq!(e.loop_type, LoopType::Menu);
            }
            let mut t = e.clock();
            let mut step = |e: &mut Engine| {
                t += PASS;
                e.set_clock(t);
                e.pass(None);
            };
            for _ in 0..phase {
                step(&mut e);
            }
            e.run_command(b"(stopmusik=1)");
            assert_eq!(e.music.state, 3, "stopmusik fades");
            let mut silent = None;
            let mut stop = None;
            for n in 0..2000 {
                step(&mut e);
                if silent.is_none() && e.music.volume == 0 {
                    silent = Some(n);
                }
                // The stop (FUN_004778f0) leaves the state 0.
                if e.music.state == 0 {
                    stop = Some(n);
                    break;
                }
            }
            let (silent, stop) = (silent.expect("fades to 0"), stop.expect("stops"));
            gaps[usize::from(in_menu)].push(stop - silent);
        }
    }
    assert!(gaps[0].contains(&2), "the panorama: {:?}", gaps[0]);
    assert_eq!(gaps[1], [1; 4], "a menu");
}
