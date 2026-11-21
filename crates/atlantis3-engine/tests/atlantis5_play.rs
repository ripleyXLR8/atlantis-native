//! *Atlantis V* played headless from a new game (`ATLANTIS5_DVD_FR`: the
//! folder holding `datas/`, the French DVD's cabinets extracted; skipped
//! when unset): the player's own scripts run, the first film runs and is
//! skipped, the zeppelin's first view shows with its sprites and music,
//! and clicks on the move zones walk through the views. Pictures go to
//! `/tmp/claude-1000/a5-runs/m1-shots/` when that folder exists.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis3_data::atlantis5::Atlantis5Source;
use atlantis3_engine::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use atlantis3_engine::atlantis5_play::Play;

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
    let dir = std::path::Path::new("/tmp/claude-1000/a5-runs/m1-shots");
    if dir.is_dir() {
        let png = atlantis2_engine::png::encode_xrgb8888(&px, VIEW_WIDTH, VIEW_HEIGHT);
        let _ = std::fs::write(dir.join(name), png);
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

    // The move zones in sight, then a walk by clicks: wherever a zone to
    // another view is in sight, it is clicked.
    let mut visited = vec![3];
    for step in 0..6 {
        let zones = play.clickable();
        let Some((x, y, id, _)) = zones.into_iter().find(|z| !visited.contains(&z.2)) else {
            // Look around: turn the camera by mouse drags until a new
            // zone shows.
            for _ in 0..10 {
                play.look(-40.0, 0.0, 1.0 / 30.0);
                frames(&mut play, 1);
            }
            frames(&mut play, 30);
            continue;
        };
        assert!(play.click(x, y), "click on zone {id} at ({x}, {y})");
        frames(&mut play, 5);
        let addr = play.stage().address().expect("a view");
        assert_eq!(addr.0, b"zeppelin".to_vec());
        assert_eq!(addr.1, id, "the zone's view");
        visited.push(id);
        shot(&play, &format!("a5-walk-{step}-view-{id}.png"));
    }
    assert!(visited.len() >= 3, "walked through {visited:?}");
    let trace = &play.stage().trace;
    assert!(!trace.iter().any(|l| l.starts_with("error")), "{trace:?}");
}
