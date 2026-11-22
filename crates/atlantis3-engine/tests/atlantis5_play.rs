//! *Atlantis V* played headless from a new game (`ATLANTIS5_DVD_FR`: the
//! folder holding `datas/`, the French DVD's cabinets extracted; skipped
//! when unset): the player's own scripts run, the first film runs and is
//! skipped, the zeppelin's first view shows with its sprites and music,
//! clicks at the screen centre on the move zones walk through the views
//! (the `centring` turn first when the zone is off the centre), and a
//! click on a sprite runs the scene's `click_sprite` branch. Pictures go
//! to the folder `ATLANTIS5_SHOT_DIR` names, when set.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::{Hover, Play};
use atlantis5_script::Value;

fn dvd() -> Option<PathBuf> {
    match std::env::var_os("ATLANTIS5_DVD_FR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("SKIPPED: ATLANTIS5_DVD_FR is not set");
            None
        }
    }
}

fn shot(play: &Play, name: &str) -> Vec<u32> {
    let mut px = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
    play.render(&mut px);
    if let Some(dir) = std::env::var_os("ATLANTIS5_SHOT_DIR") {
        let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
        let _ = std::fs::write(PathBuf::from(dir).join(name), png);
    }
    px
}

fn frames(play: &mut Play, n: usize) {
    for _ in 0..n {
        play.update(1.0 / 30.0);
    }
}

#[test]
fn a_new_game_walks_the_zeppelin() {
    let Some(d) = dvd() else { return };
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    play.new_game();
    frames(&mut play, 3);
    // The chapter's film plays, the script waiting.
    assert!(play.stage().film_playing(), "cine01 plays");
    frames(&mut play, 30);
    let film = shot(&play, "a5-film.png");
    assert!(film.iter().any(|&p| p & 0xff_ffff != 0), "a film picture");
    play.skip();
    frames(&mut play, 3);
    assert!(!play.stage().film_playing());
    assert_eq!(play.game.state(), 4, "play");
    assert_eq!(
        play.stage().address(),
        Some((b"zeppelin".to_vec(), 3)),
        "the first view"
    );
    let sprites: Vec<String> = play.stage().sprites().into_iter().map(|s| s.0).collect();
    assert_eq!(
        sprites,
        [
            "glass_breaker",
            "coin",
            "glass_breaker_door",
            "cendar",
            "crab_breaker"
        ]
    );
    let (pitch, yaw) = play.stage().angles();
    assert_eq!((pitch, yaw), (0.0, 270.0));
    let music: Vec<String> = play.stage().music().into_iter().map(|m| m.0).collect();
    assert_eq!(music, ["zeppelin_background", "zeppelin_cracking"]);
    let first = shot(&play, "a5-zeppelin-3.png");
    assert!(first.iter().filter(|&&p| p & 0xff_ffff != 0).count() > VIEW_WIDTH * VIEW_HEIGHT / 2);

    // A walk by clicks: the centre turned onto a move zone not visited
    // yet, a click there.
    let mut visited = vec![3];
    for step in 0..4 {
        let zones: Vec<i32> = play
            .zones()
            .into_iter()
            .filter(|z| z.1.is_empty() && !visited.contains(&z.0))
            .map(|z| z.0)
            .collect();
        // Toward view 4 (the coin's) when it is in reach.
        let Some(&id) = zones.iter().find(|&&z| z == 4).or(zones.first()) else {
            break;
        };
        assert!(play.aim_at_zone(id));
        assert_eq!(play.hover(), Some(&Hover::Zone(id, Vec::new())));
        play.click();
        frames(&mut play, 3);
        let addr = play.stage().address().expect("a view");
        assert_eq!(addr.0, b"zeppelin".to_vec());
        assert_eq!(addr.1, id, "the zone's view");
        visited.push(id);
        shot(&play, &format!("a5-walk-{step}-view-{id}.png"));
        if id == 4 {
            break;
        }
    }
    assert_eq!(visited.last(), Some(&4), "walked through {visited:?}");

    // View 4: the coin on the table is a target (an item reachable from
    // view 4, `chapter_01.lua`'s Init_Coin); a click takes it in the hand.
    assert!(play.aim_at_sprite(b"coin"), "the coin under the centre");
    assert_eq!(play.stage().cursor, Some(2));
    play.click();
    frames(&mut play, 2);
    play.game
        .script()
        .exec("__hand = atl.hero.handled_item")
        .expect("lua");
    assert_eq!(
        play.game.script().global(b"__hand"),
        Value::Str(b"coin".to_vec())
    );
    // No longer reachable: not a target any more.
    frames(&mut play, 1);
    assert!(!matches!(play.hover(), Some(Hover::Sprite(o, _)) if o == b"coin"));

    // The centring turn: the centre a few degrees off a move zone's
    // direction (still inside it), a click; the camera turns for 0.75 s
    // before the view changes.
    let back = play
        .zones()
        .into_iter()
        .find(|z| z.1.is_empty())
        .map(|z| z.0)
        .expect("a move zone");
    assert!(play.aim_at_zone(back));
    let (p0, y0) = play.stage().angles();
    let mut off = None;
    for d in [8.0f32, 6.0, 5.0, 4.0] {
        play.look_angles(p0, y0 + d);
        if play.hover() == Some(&Hover::Zone(back, Vec::new())) {
            off = Some(d);
            break;
        }
    }
    let off = off.expect("the zone is wide enough");
    play.click();
    frames(&mut play, 1);
    let mut turned = 0;
    let mut y1 = y0 + off;
    while play.stage().address() == Some((b"zeppelin".to_vec(), 4)) {
        y1 = play.stage().angles().1;
        frames(&mut play, 1);
        turned += 1;
        assert!(turned < 60, "the view changes");
    }
    assert!(turned >= 20, "a turn of about 0.75 s: {turned} frames");
    let gap = (y1 - y0).rem_euclid(360.0).min((y0 - y1).rem_euclid(360.0));
    assert!(gap < 1.0, "turned toward the zone: {y0} {y1}");

    let trace = &play.stage().trace;
    assert!(!trace.iter().any(|l| l.starts_with("error")), "{trace:?}");
}

/// A new game to the zeppelin's first view (the film skipped).
fn to_first_view(d: &std::path::Path) -> Play {
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let mut play = Play::new(src, "french", None).expect("play");
    play.new_game();
    frames(&mut play, 3);
    play.skip();
    frames(&mut play, 3);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    play
}

/// The scripts' own `Atl_Address_Go` to `view`, without a transition.
fn go(play: &mut Play, view: i32) {
    assert!(play.game.run_string(&format!(
        "return function() Atl_Address_Go(\"zeppelin\", {view}, \"no_fx\") end"
    )));
    for _ in 0..60 {
        frames(play, 1);
        if play.stage().address() == Some((b"zeppelin".to_vec(), view)) {
            break;
        }
    }
    frames(play, 2);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), view)));
}

#[test]
fn the_zeppelins_clocks_and_its_randomized_gorilla() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    // View 8: `Display_Clocks` plays the corridor's clock with
    // `Atl_Sprite_PlayRebound(..., "loop", 0, 0, 0)`: frame 0 held.
    go(&mut play, 8);
    let clock = |play: &Play| {
        play.stage()
            .sprites()
            .into_iter()
            .find(|s| s.0 == "corridor_clock")
    };
    assert_eq!(
        clock(&play),
        Some(("corridor_clock".into(), "z1".into(), 0))
    );
    frames(&mut play, 90);
    assert_eq!(
        clock(&play),
        Some(("corridor_clock".into(), "z1".into(), 0))
    );
    shot(&play, "a5-zeppelin-8.png");
    // View 17: the gorilla's `stand` played once ("stop"), randomized
    // (`Atl_Sprite_Randomize("gorilla", 10)`): at its end it goes on as
    // `Stand` or `Tick1` instead of leaving.
    go(&mut play, 17);
    let gorilla = |play: &Play| {
        play.stage()
            .sprites()
            .into_iter()
            .find(|s| s.0 == "gorilla")
            .map(|s| s.1)
    };
    assert_eq!(gorilla(&play).as_deref(), Some("stand"));
    let mut seen = Vec::new();
    for _ in 0..30 * 60 {
        frames(&mut play, 1);
        let a = gorilla(&play).expect("the gorilla stays");
        if seen.last() != Some(&a) {
            seen.push(a);
        }
    }
    assert!(seen.len() > 1, "{seen:?}");
    assert!(
        seen[1..].iter().all(|a| a == "Stand" || a == "Tick1"),
        "{seen:?}"
    );
    if play.aim_at_sprite(b"gorilla") {
        frames(&mut play, 1);
    }
    shot(&play, "a5-zeppelin-17.png");
}
