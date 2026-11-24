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
    assert!(
        play.stage().music_muted(),
        "a 1-argument film mutes the music"
    );
    frames(&mut play, 30);
    let film = shot(&play, "a5-film.png");
    assert!(film.iter().any(|&p| p & 0xff_ffff != 0), "a film picture");
    play.skip();
    frames(&mut play, 3);
    assert!(!play.stage().film_playing());
    assert!(!play.stage().music_muted());
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

/// The pixels the game's cursor changes on the picture: the picture with
/// it against the view alone, as (x, y).
fn cursor_pixels(play: &Play, name: &str) -> Vec<(usize, usize)> {
    let with = shot(play, name);
    let mut view = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
    play.stage().render(&mut view);
    (0..VIEW_WIDTH * VIEW_HEIGHT)
        .filter(|&i| with[i] != view[i])
        .map(|i| (i % VIEW_WIDTH, i / VIEW_WIDTH))
        .collect()
}

/// The game's cursor in play (@ 0x0042bfd0, 0x0042c030, 0x0042c660): the
/// pick's type at the screen centre, animated at 15 frames a second, the
/// hand's object below it, the aim hidden by the crosshair option with an
/// empty hand, a follower's picture at the top left.
#[test]
fn the_play_cursor_shows_the_pick_the_hand_and_the_followers() {
    use atlantis3_engine::atlantis5_play::CURSOR_AIM;
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    assert!(play.cursor_drawn());
    let (cx, cy) = (VIEW_WIDTH / 2, VIEW_HEIGHT / 2);
    let near = |p: &[(usize, usize)], x: usize, y: usize, r: usize| {
        p.iter()
            .filter(|&&(px, py)| px.abs_diff(x) < r && py.abs_diff(y) < r)
            .count()
    };

    // Nothing under the centre: the aim (type 4), drawn with the
    // crosshair option only.
    let aim = [
        (-60.0, 0.0),
        (60.0, 0.0),
        (-60.0, 90.0),
        (-60.0, 180.0),
        (60.0, 180.0),
    ]
    .into_iter()
    .find(|&(p, y)| {
        play.look_angles(p, y);
        frames(&mut play, 1);
        play.cursor.kind() == CURSOR_AIM
    });
    assert!(aim.is_some(), "an angle with nothing under the centre");
    assert!(near(&cursor_pixels(&play, "a5-cursor-aim.png"), cx, cy, 48) > 0);
    play.stage_mut().config.crosshair = false;
    assert!(cursor_pixels(&play, "a5-cursor-aim-off.png").is_empty());
    play.stage_mut().config.crosshair = true;

    // A move zone under the centre: `cursor_move` (type 0), restarted
    // on its first frame, then looping at 15 frames a second.
    let zone = play
        .zones()
        .into_iter()
        .find(|z| z.1.is_empty())
        .map(|z| z.0)
        .expect("a move zone");
    assert!(play.aim_at_zone(zone));
    frames(&mut play, 1);
    assert_eq!(play.cursor.kind(), 0);
    assert_eq!(play.cursor.frame(), 0, "1/30 s: still the first frame");
    let px = cursor_pixels(&play, "a5-cursor-move.png");
    assert!(near(&px, cx, cy, 48) > 50, "the cursor at the centre");
    assert_eq!(near(&px, cx, cy, 100), px.len(), "nothing else");
    frames(&mut play, 14);
    // 15/30 s: ⌊0.5 · 15⌋ = 7 of cursor_move's 20 frames.
    assert_eq!(play.cursor.frame(), 7);

    // An object in the hand: its picture (`objects_icon/coin.spr`) at
    // (x + 5, y + 30).
    play.game
        .script()
        .exec("atl.hero.handled_item = \"coin\"")
        .expect("lua");
    frames(&mut play, 1);
    assert_eq!(play.cursor.hand(), Some("coin.spr"));
    let px = cursor_pixels(&play, "a5-cursor-hand.png");
    assert!(
        near(&px, cx + 5, cy + 30, 40) > 50,
        "the coin below the centre"
    );
    play.game
        .script()
        .exec("atl.hero.handled_item = nil")
        .expect("lua");
    frames(&mut play, 1);
    assert_eq!(play.cursor.hand(), None);

    // A follower: its picture at (10, 10).
    play.game
        .script()
        .exec("Atl_Hero_AddFollower(\"sing_like_rain\")")
        .expect("lua");
    frames(&mut play, 1);
    let px = cursor_pixels(&play, "a5-cursor-follower.png");
    assert!(near(&px, 40, 40, 40) > 50, "the follower at the top left");

    // A menu (state 3): no game cursor.
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.game.state(), 3);
    assert!(!play.cursor_drawn());
}

/// State 3's frame (@ 0x0041b780): the scene's update (the sprites' step
/// @ 0x00441c30) only for modes 8, 9 and 0xc; under the in-game menu the
/// gorilla's sprite holds its frame, and goes on once the menu is left.
#[test]
fn the_in_game_menu_freezes_the_sprites() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 17);
    frames(&mut play, 10);
    let gorilla = |play: &Play| {
        play.stage()
            .sprites()
            .into_iter()
            .find(|s| s.0 == "gorilla")
            .expect("the gorilla")
    };
    play.escape();
    frames(&mut play, 1);
    assert_eq!(play.game.state(), 3);
    let held = gorilla(&play);
    frames(&mut play, 60);
    assert_eq!(gorilla(&play), held, "frozen in the in-game menu");
    assert!(play.click_menu("resume"));
    let mut moved = false;
    for _ in 0..120 {
        frames(&mut play, 1);
        moved |= gorilla(&play) != held;
    }
    assert_eq!(play.game.state(), 4);
    assert!(moved, "stepped again in play");
}

/// `zoom_and_fade` (@ 0x0042e640, fx 2): the view darkened by
/// (256 − ftol(k · 255)) / 256 while the transition runs, black at its
/// view change, no cursor; the view as before once it ends.
#[test]
fn zoom_and_fade_darkens_the_view() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 8);
    let mean = |px: &[u32]| -> u64 {
        px.iter()
            .map(|p| u64::from((p >> 16) & 0xff) + u64::from((p >> 8) & 0xff) + u64::from(p & 0xff))
            .sum::<u64>()
            / px.len() as u64
    };
    let before = mean(&shot(&play, "a5-fade-before.png"));
    assert!(before > 20, "a lit view");
    assert!(play
        .game
        .run_string("return function() Atl_Address_Go(\"zeppelin\", 17, \"zoom_and_fade\") end"));
    let mut darkest = u64::MAX;
    let mut cursor = false;
    for _ in 0..60 {
        frames(&mut play, 1);
        if play.game.transition_fx().is_some() {
            darkest = darkest.min(mean(&shot(&play, "a5-fade.png")));
            cursor |= play.cursor_drawn();
        }
        if !play.game.busy() && play.game.transition_fx().is_none() {
            break;
        }
    }
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 17)));
    assert!(darkest < 2, "black at the view change: {darkest}");
    assert!(!cursor, "no cursor during the fade");
    frames(&mut play, 2);
    assert_eq!(play.stage().fade, 0.0);
    assert!(mean(&shot(&play, "a5-fade-after.png")) > 20);
}

/// `Atl_Video_Play`'s 5-argument form (@ 0x0040c990): RECT (left, top,
/// right, bottom) = arguments (3, 2, 5, 4); the view blended with
/// `oldpaper.tga` under the film, the film 1:1 in the rectangle; the music
/// at half its level meanwhile (@ 0x0045caf0), not muted; back after.
#[test]
fn a_five_argument_film_plays_over_the_paper() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    let view = shot(&play, "a5-paper-view.png");
    let music = play.stage().config.music;
    assert!(play
        .game
        .run_string("return function() Atl_Video_Play(\"is0001\", 144, 256, 432, 768) end"));
    frames(&mut play, 1);
    assert!(play.stage().film_playing());
    assert_eq!(play.stage().film_rect(), Some([256, 144, 768, 432]));
    assert!(!play.stage().music_muted());
    frames(&mut play, 40);
    assert!((play.stage().music_level() - music * 0.5).abs() < 1e-4);
    let px = shot(&play, "a5-paper-film.png");
    // Outside the rectangle: neither the view nor black.
    let outside = (0..VIEW_HEIGHT * VIEW_WIDTH)
        .filter(|i| (i % VIEW_WIDTH) < 256 || (i / VIEW_WIDTH) < 144)
        .filter(|&i| px[i] != view[i])
        .count();
    assert!(outside > 100_000, "the paper over the view: {outside}");
    play.skip();
    for _ in 0..40 {
        frames(&mut play, 1);
    }
    assert!(!play.stage().film_playing());
    assert!((play.stage().music_level() - music).abs() < 1e-4);
}

/// A randomized sprite whose next animation has no file (@ 0x0044218d
/// throws `... changement de sprite annulé`, 0x0066e468): the catch of
/// `CGameManager::Process` (@ 0x0042f3c6) logs it and the game goes on;
/// that frame the rest of the list and the sound update are skipped; the
/// sprite stays, ended.
#[test]
fn a_randomized_sprite_without_its_next_file_stays() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 17);
    // `Tick2` always (tick2 100): the gorilla has no such file.
    assert!(play
        .game
        .run_string("return function() Atl_Sprite_Randomize(\"gorilla\", 0, 100) end"));
    let gorilla = |play: &Play| {
        play.stage()
            .sprites()
            .into_iter()
            .find(|s| s.0 == "gorilla")
            .map(|s| s.1)
    };
    for _ in 0..30 * 20 {
        frames(&mut play, 1);
        assert_eq!(gorilla(&play).as_deref(), Some("stand"));
    }
    assert_eq!(play.game.state(), 4);
}

/// The mouse's moves of a frame make one move (@ 0x0047c8a0: one
/// `GetDeviceState` a frame), so the look's rule against an opposing
/// speed (@ 0x0042cdc0: a third of it taken off) runs once a frame,
/// whatever the number of system events.
#[test]
fn a_frame_s_mouse_moves_turn_the_camera_once() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    assert_eq!(play.game.state(), 4);
    assert!(!play.game.busy());
    play.look(1000.0, 0.0);
    frames(&mut play, 1);
    let s = play.stage().look_speed()[0];
    assert!((s + 6.0).abs() < 1e-4, "at the clamp: {s}");
    // A reversal as a 1000 Hz mouse gives it: 16 events of 1 count. One
    // move of −16 counts: ax = 16 · dt · 0.3, minus a third of −6.
    for _ in 0..16 {
        play.look(-1.0, 0.0);
    }
    frames(&mut play, 1);
    let dt = 1.0f32 / 30.0;
    let want = -6.0 + (16.0 * dt * 0.3 + 6.0 * f32::from_bits(0x3eaa_aaab));
    let s = play.stage().look_speed()[0];
    assert!((s - want).abs() < 1e-3, "{s} against {want}");
}

/// `Atl_PersistantSprite_Stop` (@ 0x0040bdc0) on an object that is not
/// persistent (@ 0x004421e0 false) only logs: its ordinary sprite stays.
#[test]
fn persistant_sprite_stop_leaves_an_ordinary_sprite() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 4);
    let coin = |play: &Play| play.stage().sprites().into_iter().any(|s| s.0 == "coin");
    assert!(coin(&play));
    assert!(play
        .game
        .run_string("return function() Atl_PersistantSprite_Stop(\"coin\") end"));
    frames(&mut play, 1);
    assert!(coin(&play), "not persistent: kept");
}

/// State 4's frame ends with a second sprite pick when one was found
/// (@ 0x0042f000 case 4, 0x0042f464..0x0042f471: +0x3c read after the
/// sound, @ 0x00430b70 again, which clears it first): a sprite the
/// sprites' step removed is not the next frame's click target.
#[test]
fn a_sprite_that_ends_is_not_the_next_click_target() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 4);
    assert!(play.aim_at_sprite(b"coin"), "the coin under the centre");
    frames(&mut play, 1);
    assert!(matches!(play.hover(), Some(Hover::Sprite(o, _)) if o == b"coin"));
    let coin = |play: &Play| play.stage().sprites().into_iter().find(|s| s.0 == "coin");
    let animation = coin(&play).expect("the coin's sprite").1;
    assert!(play.game.run_string(&format!(
        "return function() Atl_Sprite_Play(\"coin\", \"{animation}\", \"stop\") end"
    )));
    for _ in 0..60 {
        frames(&mut play, 1);
        if coin(&play).is_none() {
            break;
        }
    }
    assert!(coin(&play).is_none(), "the coin's sprite ended");
    assert!(
        !matches!(play.hover(), Some(Hover::Sprite(o, _)) if o == b"coin"),
        "{:?}",
        play.hover()
    );
}

/// `Atl_Video_Play` (both forms, before the open @ 0x0042aa90: also when
/// the film is missing) and `Atl_Dialog_Play` (once the id is valid) zero
/// the camera controller's look speeds and damping clock (@ 0x0042cd80 on
/// [[0x0068a528]+0x30]+0x4c): the view does not glide after them.
#[test]
fn films_and_dialogs_stop_the_mouse_look() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    let swing = |play: &mut Play| {
        play.look(1000.0, 0.0);
        frames(play, 1);
        assert!(play.stage().look_speed()[0] != 0.0);
    };
    for call in [
        "Atl_Video_Play(\"no_such_film\")",
        "Atl_Video_Play(\"no_such_film\", 144, 256, 432, 768)",
        "Atl_Dialog_Play(1)",
    ] {
        swing(&mut play);
        assert!(play
            .game
            .run_string(&format!("return function() {call} end")));
        frames(&mut play, 1);
        assert_eq!(play.stage().look_speed(), [0.0, 0.0], "{call}");
        for _ in 0..600 {
            if !play.game.busy() {
                break;
            }
            if play.stage().film_playing() {
                play.skip();
            }
            play.space();
            frames(&mut play, 1);
        }
        assert!(!play.game.busy(), "{call}");
    }
    // An unknown dialog leaves them.
    swing(&mut play);
    let s = play.stage().look_speed();
    assert!(play
        .game
        .run_string("return function() Atl_Dialog_Play(9999) end"));
    assert_eq!(play.stage().look_speed(), s);
}

/// `Atl_Video_Play`'s loops (@ 0x0040c990) do not run the scene's
/// update (@ 0x0044c380): the view's sprites are where they were when
/// the film ends.
#[test]
fn a_film_freezes_the_view_s_sprites() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    go(&mut play, 17);
    frames(&mut play, 5);
    let before = play.stage().sprites();
    assert!(play
        .game
        .run_string("return function() Atl_Video_Play(\"is0001\") end"));
    frames(&mut play, 1);
    assert!(play.stage().film_playing());
    frames(&mut play, 60);
    assert!(play.stage().film_playing());
    assert_eq!(play.stage().sprites(), before);
    play.skip();
    for _ in 0..40 {
        frames(&mut play, 1);
    }
    assert!(!play.stage().film_playing());
    // Stepped again after it.
    frames(&mut play, 60);
    assert_ne!(play.stage().sprites(), before);
}

/// The scripts' `Atl_Address_Go` to a view of any scene, without a
/// transition.
fn go_to(play: &mut Play, scene: &str, view: i32) {
    assert!(play.game.run_string(&format!(
        "return function() Atl_Address_Go(\"{scene}\", {view}, \"no_fx\") end"
    )));
    for _ in 0..60 {
        frames(play, 1);
        if play.stage().address() == Some((scene.as_bytes().to_vec(), view)) && !play.game.busy() {
            break;
        }
    }
    assert_eq!(
        play.stage().address(),
        Some((scene.as_bytes().to_vec(), view))
    );
}

/// A persistent sprite's record lives on through a view without its
/// `z1` file (@ 0x004424d0: the next record, nothing erased) and is made
/// even when the view of the call has none (@ 0x0040ba20 →
/// 0x00442290 first): it shows again in the next view that has it.
#[test]
fn a_persistent_sprite_crosses_a_view_without_its_file() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    let lightning = |play: &Play| {
        play.stage()
            .sprites()
            .iter()
            .any(|s| s.0 == "floor12_lightning")
    };
    // etage12 view 3 has no floor12_lightning file; view 4 has one.
    go_to(&mut play, "etage12", 3);
    assert!(play
        .game
        .run_string("return function() Atl_PersistantSprite_Play(\"floor12_lightning\", 10) end"));
    frames(&mut play, 1);
    assert!(!lightning(&play));
    go_to(&mut play, "etage12", 4);
    assert!(
        lightning(&play),
        "the record made in view 3 shows in view 4"
    );
    go_to(&mut play, "etage12", 3);
    assert!(!lightning(&play));
    go_to(&mut play, "etage12", 4);
    assert!(lightning(&play), "the record kept through view 3");
}

/// `Atl_Video_Play`'s loop (@ 0x0040c990) steps the film before its
/// first draw and leaves after the draw of the iteration whose step
/// reported the end (`FrameNum` = `Frames` − 1): the first picture is
/// frame 1, and the frame decoded last is shown once.
#[test]
fn a_film_shows_its_first_and_last_decoded_frames() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    assert!(play
        .game
        .run_string("return function() Atl_Video_Play(\"is0001\") end"));
    // Before any update: the first step was made at the call.
    let (num, frames_n) = play.stage().film_frame().expect("a film");
    assert_eq!(num, 2, "frame 1 decoded, FrameNum on 2");
    let mut last = None;
    for _ in 0..frames_n * 4 {
        frames(&mut play, 1);
        match play.stage().film_frame() {
            Some(f) => last = Some(f.0),
            None => break,
        }
    }
    assert!(!play.stage().film_playing());
    assert_eq!(last, Some(frames_n - 1), "drawn once at the end");
}

/// A playlist whose track is a RIFF WAVE file (PCM 16-bit stereo,
/// 22050 Hz: `zep_engine.wav`, the only track of
/// `zeppelin_working_engine_room`) plays as an Ogg one: the original opens
/// every track with FMOD's `FSOUND_Stream_Open` (@ 0x0045acf0), which
/// reads WAVE.
#[test]
fn a_wave_playlist_plays() {
    use atlantis3_engine::audio::{Audio, CHANNEL_MUSIC_2004};
    let Some(d) = dvd() else { return };
    let src: Arc<dyn atlantis_data::FileSource> =
        Arc::new(Atlantis5Source::open(&d.join("datas")).expect("the extracted tree"));
    let audio = Audio::new();
    let mut play = Play::new(src, "french", Some(audio.clone())).expect("play");
    play.new_game();
    frames(&mut play, 3);
    play.skip();
    frames(&mut play, 3);
    assert_eq!(play.game.state(), 4, "play");
    assert!(play.game.run_string(
        "return function() Atl_Music_StopAll(0) \
         Atl_Music_Play(\"zeppelin_working_engine_room\", 3) end"
    ));
    // Past its 3 s fade-in.
    frames(&mut play, 120);
    let music: Vec<String> = play.stage().music().into_iter().map(|m| m.0).collect();
    assert_eq!(music, ["zeppelin_working_engine_room"]);
    assert!(audio.is_playing(CHANNEL_MUSIC_2004 + 3));
    for (ch, other) in (0..5).map(|c| (c, CHANNEL_MUSIC_2004 + c)) {
        assert_eq!(audio.is_playing(other), ch == 3, "channel {ch}");
    }
    let mut out = vec![0f32; 44100];
    audio.fill(&mut out, 44100, 2);
    let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.01, "{peak}");
}

/// Two presses in one long frame: one click on the last frame's pick, the
/// other dropped (the original has at most one press a frame,
/// @ 0x0047c8a0, and picks again after each).
#[test]
fn two_presses_in_one_play_frame_click_once() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    let zone = play
        .zones()
        .into_iter()
        .find(|z| z.1.is_empty())
        .map(|z| z.0)
        .expect("a move zone");
    assert!(play.aim_at_zone(zone));
    // The scene's script goes to the zone's view: counted, no transition,
    // so the game is free again for a second click in the same frame.
    play.game
        .script()
        .exec("__gone = 0 Atl_Address_Go = function() __gone = __gone + 1 end")
        .expect("lua");
    play.click();
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().global(b"__gone"), Value::Num(1.0));
}

/// A press carries both buttons' state (@ 0x0043bbc0); in play
/// (@ 0x004300d0) a left press is a click only with the buttons exactly 1,
/// a right press acts only with exactly 4: with the other button held,
/// neither does anything.
#[test]
fn a_press_with_the_other_button_held_does_nothing_in_play() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    let zone = play
        .zones()
        .into_iter()
        .find(|z| z.1.is_empty())
        .map(|z| z.0)
        .expect("a move zone");
    assert!(play.aim_at_zone(zone));
    play.game
        .script()
        .exec("__gone = 0 Atl_Address_Go = function() __gone = __gone + 1 end")
        .expect("lua");
    // The left press with the right button held: buttons 5.
    play.set_buttons(true, true);
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().global(b"__gone"), Value::Num(0.0));
    // The right press with the left button held: no inventory.
    play.right_press();
    frames(&mut play, 1);
    assert!(!play.stage().inventory_open_now());
    assert_eq!(play.game.state(), 4);
    // Alone, each acts.
    play.set_buttons(true, false);
    play.click();
    frames(&mut play, 1);
    assert_eq!(play.game.script().global(b"__gone"), Value::Num(1.0));
    play.set_buttons(false, true);
    play.right_press();
    frames(&mut play, 1);
    assert!(play.stage().inventory_open_now());
}

/// A transition's loop (@ 0x00405390) steps the sprites only through its
/// step (@ 0x0042d900), which calls the scene's update in `centring`'s
/// phase 0 and in `zoom_and_fade`'s steps but the last, never in the
/// step that changes the view by `no_fx` or `centring`: the new view's
/// first picture shows its sprites with no time played, and their first
/// step is the play frame after `Atl_Address_Go` returns.
#[test]
fn a_view_change_frame_steps_no_sprite() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    for (fx, back) in [("\"no_fx\"", 8), ("nil", 8)] {
        go(&mut play, back);
        // View 17's script plays the gorilla's `stand`.
        let fx = if fx == "nil" {
            String::new()
        } else {
            format!(", {fx}")
        };
        assert!(play.game.run_string(&format!(
            "return function() Atl_Address_Go(\"zeppelin\", 17{fx}) end"
        )));
        for _ in 0..60 {
            frames(&mut play, 1);
            if play.stage().address() == Some((b"zeppelin".to_vec(), 17)) {
                break;
            }
        }
        assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 17)));
        assert_eq!(play.stage().sprite_time(b"gorilla"), Some(0.0), "{fx}");
        frames(&mut play, 1);
        assert!(!play.game.busy());
        assert_eq!(
            play.stage().sprite_time(b"gorilla"),
            Some(1.0 / 30.0),
            "{fx}: stepped once after the transition"
        );
    }
}

/// The data with one panorama replaced (a damaged or another edition's
/// file whose faces are 8² instead of 768²).
struct SmallFaces {
    inner: Atlantis5Source,
    name: String,
    jp6: Vec<u8>,
}

impl atlantis_data::FileSource for SmallFaces {
    fn size(&self, name: &str) -> Option<u64> {
        if name.eq_ignore_ascii_case(&self.name) {
            return Some(self.jp6.len() as u64);
        }
        self.inner.size(name)
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        if name.eq_ignore_ascii_case(&self.name) {
            let at = usize::try_from(offset).ok()?;
            return self.jp6.get(at..at + len).map(<[u8]>::to_vec);
        }
        self.inner.read_at(name, offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.inner.has_dir(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        self.inner.list(dir)
    }
}

/// A JP6 (version 0x20001) of six 8 × 8 faces (one JPEG made for the
/// test, not the game's).
fn small_jp6() -> Vec<u8> {
    const JPEG: &str = "ffd8ffdb004300100b0c0e0c0a100e0d0e1211101318281a181616183123251d283a\
333d3c3933383740485c4e404457453738506d51575f626768673e4d71797064785c656763ffdb004301111212\
1815182f1a1a2f634238426363636363636363636363636363636363636363636363636363636363636363636363\
636363636363636363636363636363ffc00011080008000803011100021101031101ffc400140001000000000000\
00000000000000000003ffc40014100100000000000000000000000000000000ffc4001501010100000000000000\
000000000000000405ffc40014110100000000000000000000000000000000ffda000c03010002110311003f0035\
515fffd9";
    let jpg: Vec<u8> = (0..JPEG.len() / 2)
        .map(|i| u8::from_str_radix(&JPEG[2 * i..2 * i + 2], 16).expect("hex"))
        .collect();
    let mut d = vec![0u8; 0x6e];
    d[..4].copy_from_slice(b"JP6\0");
    d[4..8].copy_from_slice(&0x20001u32.to_le_bytes());
    d[8..10].copy_from_slice(&8u16.to_le_bytes());
    d[10..12].copy_from_slice(&8u16.to_le_bytes());
    for i in 0..6 {
        let off = d.len();
        d.extend_from_slice(b"JPG\0");
        d.extend_from_slice(&(4 + jpg.len() as u32).to_le_bytes());
        d.extend_from_slice(&jpg);
        let at = 0xe + 16 * i;
        d[at..at + 4].copy_from_slice(&0x10u32.to_le_bytes());
        d[at + 4..at + 8].copy_from_slice(&(off as u32).to_le_bytes());
        d[at + 8..at + 12].copy_from_slice(&(8 + jpg.len() as u32).to_le_bytes());
        d[at + 12..at + 16].copy_from_slice(&(0x40u32 | 1 << i).to_le_bytes());
    }
    d
}

/// A panorama whose faces are not 768² is refused at its load, as a bad
/// file (the view kept), instead of stopping the renderer at the next
/// picture.
#[test]
fn a_panorama_with_other_faces_is_refused() {
    let Some(d) = dvd() else { return };
    let inner = Atlantis5Source::open(&d.join("datas")).expect("the extracted tree");
    let panorama =
        atlantis3_engine::atlantis5::panorama_name(&inner, "zeppelin", 8).expect("view 8");
    let src: Arc<dyn atlantis_data::FileSource> = Arc::new(SmallFaces {
        inner,
        name: format!("datas/warps/{panorama}"),
        jp6: small_jp6(),
    });
    let mut play = Play::new(src, "french", None).expect("play");
    play.new_game();
    frames(&mut play, 3);
    play.skip();
    frames(&mut play, 3);
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    assert!(play
        .game
        .run_string("return function() Atl_Address_Go(\"zeppelin\", 8, \"no_fx\") end"));
    frames(&mut play, 3);
    shot(&play, "a5-small-faces.png");
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
}

/// A wheel turn is a mouse move event (@ 0x0047c8a0 calls the move
/// handler when lX, lY or lZ is not 0): in play @ 0x0042cdc0 zeroes the
/// damping clock with the speeds unchanged, so the view glides on
/// undamped while the wheel turns.
#[test]
fn a_wheel_turn_holds_the_view_s_glide() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    play.look(40.0, 0.0);
    frames(&mut play, 1);
    let s = play.stage().camera_speed();
    assert!(s[0] != 0.0, "turning");
    // dt · 20 = 2/3 a frame: the clock never passes 1 while it is zeroed
    // every frame.
    for _ in 0..5 {
        play.wheel();
        frames(&mut play, 1);
        assert_eq!(play.stage().camera_speed(), s);
    }
    frames(&mut play, 1);
    assert_ne!(play.stage().camera_speed(), s, "damped without the wheel");
}

/// A press of the third or fourth button is a press event whose buttons
/// are the left and right buttons' state (@ 0x0047c8a0, @ 0x0043bbc0):
/// with the right button held it is a right press (`== 4`) and opens the
/// inventory; with neither held it does nothing.
#[test]
fn a_middle_press_takes_the_held_buttons() {
    let Some(d) = dvd() else { return };
    let mut play = to_first_view(&d);
    play.other_press();
    frames(&mut play, 1);
    assert_eq!(play.game.state(), 4);
    play.set_buttons(false, true);
    play.other_press();
    frames(&mut play, 1);
    assert!(play.stage().inventory_open_now());
}
