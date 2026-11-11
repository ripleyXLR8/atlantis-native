//! `PlayMovie`'s full-screen movie: the frame for a time and the layouts
//! against numbers the original executable's own code produced
//! (`tests/golden/a4_video_movie.txt`, see its header), then the intro
//! of a new game against the installed game.
//!
//! The game-data tests need `ATLANTIS4_DATA` (see
//! `atlantis4_data::testing`) and skip cleanly with
//! `ATLANTIS4_SKIP_DATA_TESTS=1`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use atlantis4_data::A4Files;
use atlantis4_engine::frame_anim::FrameAnim;
use atlantis4_engine::layout::{percent_position, percent_size, RatioMode, SAFE_AREA_RATIO};
use atlantis4_engine::movie::{self, MovieCue, MovieLayout, VideoSprite};
use atlantis4_engine::Game;
use atlantis4_script::{new_game, read_boot_vars, BootValue, EngineRef, Options, Script, Value};

fn golden(prefix: &str) -> Vec<String> {
    let path = format!(
        "{}/tests/golden/a4_video_movie.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

/// The engine clock's µs as the original hands them to the animations.
fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

#[test]
fn the_movie_frame_for_a_time_is_the_originals() {
    let lines = golden("FRAME ");
    let mut ends = 0;
    for l in &lines {
        let (head, tail) = l.split_once("->").unwrap();
        let a: Vec<&str> = head.split_whitespace().collect();
        let (fps, n): (f32, u32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
        let (start, now): (u64, u64) = (a[3].parse().unwrap(), a[4].parse().unwrap());
        // The video sprite's animation: 1 loop, first 0, count −1.
        let mut fa = FrameAnim {
            playing: true,
            start_ms: ms(start),
            loops: 1,
            reverse: false,
            n,
            fps,
            first: 0,
            count: u32::MAX,
            frame: a[5].parse().unwrap(),
        };
        let ev = fa.update(ms(now));
        let t: Vec<&str> = tail.split_whitespace().collect();
        assert_eq!(fa.frame, t[0].parse::<i32>().unwrap(), "{l}");
        let mut evs = String::new();
        if ev.stopped {
            evs.push_str("XS");
            ends += 1;
        }
        if ev.finished {
            evs.push('F');
        }
        if ev.changed {
            evs.push('C');
        }
        if evs.is_empty() {
            evs.push('-');
        }
        assert_eq!(evs, t[1], "{l}");
    }
    assert_eq!(lines.len(), 4_910);
    assert!(ends > 300);
}

/// Where a whole-microsecond time and the exact quotient disagree, the
/// original follows the doubles: e.g. 17,560,000 µs after the start at
/// 25 fps is frame 438, not 439.
#[test]
fn frame_boundaries_follow_the_originals_doubles() {
    let mut fa = FrameAnim {
        playing: true,
        start_ms: ms(1_000_000),
        loops: 1,
        reverse: false,
        n: 1578,
        fps: 25.0,
        first: 0,
        count: u32::MAX,
        frame: -1,
    };
    fa.update(ms(18_560_000));
    assert_eq!(fa.frame, 438);
    assert_eq!(atlantis4_data::video::frame_at((25, 1), 17_560), 439);
}

fn parse_f32(s: &str) -> f32 {
    s.parse::<f64>().unwrap() as f32
}

#[test]
fn the_movie_layouts_are_the_originals_to_the_bit() {
    let lines = golden("LAYOUT ");
    for l in &lines {
        let (head, tail) = l.split_once("->").unwrap();
        let a: Vec<&str> = head.split_whitespace().collect();
        let win = (parse_f32(a[1]), parse_f32(a[2]));
        let (pw, ph): (u32, u32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
        let want: Vec<f32> = tail.split_whitespace().map(parse_f32).collect();
        let got = match a[3] {
            "background" => {
                let s = percent_size(
                    movie::LAYER_SIZE,
                    win,
                    1.0,
                    RatioMode::None,
                    SAFE_AREA_RATIO,
                );
                let p = percent_position((0.5, 0.5), (0.5, 0.5), s, win);
                [s.0, s.1, p.0, p.1]
            }
            "video" => {
                let r = (f64::from(pw) / f64::from(ph)) as f32;
                let s = percent_size(
                    movie::LAYER_SIZE,
                    win,
                    r,
                    RatioMode::LetterBox,
                    SAFE_AREA_RATIO,
                );
                let p = percent_position((0.5, 0.5), (0.5, 0.5), s, win);
                [s.0, s.1, p.0, p.1]
            }
            "skip" => {
                let s = (pw as f32, ph as f32);
                let p = percent_position(movie::SKIP_POSITION, movie::SKIP_ANCHOR, s, win);
                [s.0, s.1, p.0, p.1]
            }
            w => panic!("{w}"),
        };
        assert_eq!(got.to_vec(), want, "{l}");
        // The same through the window rectangles the drawing uses.
        if a[3] == "video" {
            let m = MovieLayout::new(win.0 as u32, win.1 as u32, (pw, ph), (75, 75));
            assert_eq!(m.video.width, f64::from(want[0]), "{l}");
            assert_eq!(
                m.video.left,
                f64::from(win.0) / 2.0 + f64::from(want[2]) - f64::from(want[0]) / 2.0
            );
        }
    }
    assert_eq!(lines.len(), 40);
}

/// The dialogs' talking head is `PanScan` (`GUI/HD/dialog.lua`): the
/// same size function against the original's code run under Unicorn
/// (sizes printed to 6 digits), inside `dialogLockButton` (1.01 × 1.01).
#[test]
fn pan_scan_gives_the_dialog_videos_size() {
    for (win, want) in [
        ((1365.0, 768.0), (1385.14, 775.68)),
        ((1024.0, 768.0), (1385.14, 775.68)),
        ((1280.0, 1024.0), (1731.43, 1034.24)),
        ((1920.0, 1080.0), (1947.86, 1090.8)),
        ((800.0, 600.0), (1082.14, 606.0)),
    ] {
        let lock = percent_size((1.01, 1.01), win, 1.0, RatioMode::None, SAFE_AREA_RATIO);
        let r = (800.0f64 / 448.0) as f32;
        let s = percent_size((1.0, 1.0), lock, r, RatioMode::PanScan, SAFE_AREA_RATIO);
        assert!(
            (s.0 - want.0).abs() < 0.01 && (s.1 - want.1).abs() < 0.01,
            "{win:?} {s:?}"
        );
    }
}

fn install() -> Option<std::path::PathBuf> {
    atlantis4_data::testing::install_dir()
}

/// Every video `PlayMovie` names in the scripts.
fn movie_calls(dir: &Path) -> Vec<String> {
    fn walk(p: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(p).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua")) {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut out = Vec::new();
    for f in files {
        let text: String = std::fs::read(&f)
            .unwrap()
            .iter()
            .map(|&c| char::from(c))
            .collect();
        for line in text.lines() {
            let code = line.split("--").next().unwrap_or_default();
            if let Some(i) = code.find("PlayMovie(\"") {
                let rest = &code[i + 11..];
                if let Some(e) = rest.find('"') {
                    out.push(rest[..e].to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_scripts_movies_are_found_and_open() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let calls = movie_calls(&dir);
    let mut missing = Vec::new();
    for v in &calls {
        if movie::resolve(&files, v).is_none() {
            missing.push(v.clone());
            continue;
        }
        let s = VideoSprite::open(&files, v).unwrap();
        // Full-screen videos have no alpha companion.
        assert!(!s.has_alpha(), "{v}");
        assert!(
            [(800, 448), (800, 600), (704, 394)].contains(&s.size()),
            "{v} {:?}",
            s.size()
        );
    }
    // `AtterissageObus.ogv` (from a function no script calls) is in no
    // folder of the install.
    assert_eq!(missing, ["Videos/AtterissageObus.ogv"]);
    assert!(calls.len() > 60);
    assert_eq!(
        movie::resolve(&files, movie::SKIP_PICTURE).as_deref(),
        Some("2D/HD/btn_back.png")
    );
    assert_eq!(
        movie::resolve(&files, movie::BACKGROUND_PICTURE).as_deref(),
        Some("2D/BlackScreen.png")
    );
}

/// The clock of these tests, in µs.
const T0: u64 = 5_000_000;

fn boot(dir: &Path) -> (Rc<RefCell<Game>>, Script) {
    let (files, _) = A4Files::open_dir(dir).unwrap();
    let mut g = Game::new(files, 768);
    g.trace = Some(Vec::new());
    g.clock_us = Some(Box::new(|| T0));
    g.now_ms = ms(T0);
    let game = Rc::new(RefCell::new(g));
    let engine: EngineRef = game.clone();
    let boot = read_boot_vars(&engine, Options::default()).unwrap();
    let Some(BootValue::Str(first)) = boot.first_warp_path else {
        panic!("no firstWarpPath")
    };
    game.borrow_mut().trace = Some(Vec::new());
    let script = new_game(&engine, Options::default(), &first).unwrap();
    (game, script)
}

#[test]
fn a_new_game_plays_the_intro_then_calls_on_movie_finished() {
    let Some(dir) = install() else { return };
    let (game, mut script) = boot(&dir);
    {
        let g = game.borrow();
        let m = g.movie.as_ref().expect("the intro plays");
        assert_eq!(m.path, "Videos/intro.ogv");
        assert!(!m.skip_shown);
        let s = m.sprite.as_ref().unwrap();
        assert_eq!((s.size(), s.anim.n, s.anim.fps), ((800, 448), 1578, 25.0));
        assert_eq!((s.anim.frame, s.anim.start_ms), (0, ms(T0)));
        assert!(s.picture().is_some());
        assert_eq!(
            g.movie_cues,
            [MovieCue::Start {
                sound: "Videos/intro.ogg".into()
            }]
        );
        // The node is set up behind it.
        assert_eq!(
            g.warp.as_ref().unwrap().node.path.stem(),
            "3D/Bateau_CA/0001"
        );
    }
    // The whole window is the video (1378.65 × 772.04 over 1365 × 768).
    let mut out = vec![0x00ff_00ffu32; 1365 * 768];
    game.borrow_mut().draw(&mut out, 1365, 768, None);
    assert!(out.iter().all(|&p| p >> 24 == 0));
    // Frame 1 at 40 ms, the last (1577) until 63.12 s.
    let end = T0 + 1578 * 40_000;
    for (t, frame) in [
        (T0 + 39_999, 0),
        (T0 + 40_000, 1),
        (T0 + 1_000_000, 25),
        (end - 1, 1577),
    ] {
        assert_eq!(game.borrow_mut().update_movie(ms(t)), None);
        assert_eq!(
            game.borrow()
                .movie
                .as_ref()
                .unwrap()
                .sprite
                .as_ref()
                .unwrap()
                .anim
                .frame,
            frame
        );
    }
    game.borrow_mut().trace = Some(Vec::new());
    let name = game.borrow_mut().update_movie(ms(end)).expect("the end");
    assert_eq!(name, "Videos/intro.ogv");
    {
        let g = game.borrow();
        assert!(g.movie.is_none());
        assert_eq!(g.movie_cues.last(), Some(&MovieCue::End));
    }
    script
        .callback("OnMovieFinished", &[Value::Str(name.into_bytes())])
        .unwrap();
    let got = std::mem::take(game.borrow_mut().trace.as_mut().unwrap());
    // The original's live trace after the intro's end.
    assert_eq!(
        got[..3],
        [
            "ENTER OnMovieFinished",
            "CALL AddToBagWithoutNotification",
            "CALL ShowDocument"
        ]
    );
}

/// Peak of the mix's next `ms` milliseconds (the sound side runs on the
/// game clock here: no output pulls it).
fn mix_peak(game: &Rc<RefCell<Game>>, from_us: u64, ms_len: u64) -> i32 {
    let core = game.borrow().sounds.core();
    let mut peak = 0;
    for k in 1..=ms_len / 50 {
        game.borrow_mut().update(ms(from_us + k * 50_000));
        let mut buf = vec![0i16; 2 * 2205];
        core.lock().unwrap().pull(&mut buf);
        peak = buf.iter().map(|&s| i32::from(s).abs()).fold(peak, i32::max);
    }
    peak
}

#[test]
fn the_intros_sound_starts_and_stops_with_the_movie() {
    let Some(dir) = install() else { return };
    // Natural end, then skip.
    for skip in [false, true] {
        let (game, _script) = boot(&dir);
        {
            let mut g = game.borrow_mut();
            // The node's `PlayMusic` then `PlayMovie("Videos/intro.ogv",
            // "Videos/intro.ogg")`: the music cut, the sound file on the
            // `video` channel.
            assert_eq!(g.sounds.music.path(), "Sounds/Musiques/muzik20.ogg");
            assert!(!g.sounds.music.is_playing());
            assert_eq!(g.sounds.video.path(), "Videos/intro.ogg");
            assert!(g.sounds.video.is_playing());
            // The node's ambience goes on under the movie; silenced here so
            // that the mix is the intro's sound alone.
            g.sounds.stop_sound("Sounds/Ambiances/Bateau.ogg");
        }
        // The intro's sound is heard while the picture runs.
        assert!(mix_peak(&game, T0, 2_000) > 0);
        let name = if skip {
            let (w, h) = (1365, 768);
            let mut g = game.borrow_mut();
            g.movie_press(w, h, 10.0, 10.0);
            assert_eq!(g.movie_release(w, h, 10.0, 10.0), None);
            g.movie_press(w, h, 1266.0, 37.0);
            g.movie_release(w, h, 1266.0, 37.0)
        } else {
            game.borrow_mut().update_movie(ms(T0 + 1578 * 40_000))
        };
        assert_eq!(name.as_deref(), Some("Videos/intro.ogv"));
        let g = game.borrow();
        assert!(g.movie.is_none());
        // The intro's sound stopped, the music played again.
        assert!(!g.sounds.video.is_playing(), "skip {skip}");
        assert!(g.sounds.music.is_playing(), "skip {skip}");
    }
}

#[test]
fn a_click_shows_the_skip_button_and_a_click_on_it_ends_the_intro() {
    let Some(dir) = install() else { return };
    let (game, _script) = boot(&dir);
    let (w, h) = (1365, 768);
    // The original's live session clicked there once: it showed the
    // button, it did not skip.
    let mut g = game.borrow_mut();
    g.movie_press(w, h, 1266.0, 37.0);
    assert_eq!(g.movie_release(w, h, 1266.0, 37.0), None);
    assert!(g.movie.as_ref().unwrap().skip_shown);
    let mut out = vec![0u32; 1365 * 768];
    g.draw(&mut out, w, h, None);
    // The 75 × 75 button drawn from (1228, 8.5).
    let before = out.clone();
    g.movie.as_mut().unwrap().skip_shown = false;
    g.draw(&mut out, w, h, None);
    let changed: Vec<usize> = (0..out.len()).filter(|&i| out[i] != before[i]).collect();
    assert!(!changed.is_empty());
    for &i in &changed {
        let (x, y) = (i % 1365, i / 1365);
        assert!(
            (1228..1303).contains(&x) && (8..=83).contains(&y),
            "{x} {y}"
        );
    }
    g.movie.as_mut().unwrap().skip_shown = true;
    g.movie_press(w, h, 1266.0, 37.0);
    assert_eq!(
        g.movie_release(w, h, 1266.0, 37.0).as_deref(),
        Some("Videos/intro.ogv")
    );
    assert!(g.movie.is_none());
}

#[test]
fn a_pillarboxed_movie_has_black_sides() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let mut g = Game::new(files, 768);
    g.clock_us = Some(Box::new(|| 0));
    g.play_movie("Videos/clipcosmo.ogv", "Videos/clipcosmo.ogg");
    let mut out = vec![0x00ff_ffffu32; 1365 * 768];
    g.draw(&mut out, 1365, 768, None);
    // 1034.24 wide centred: 165.38 px of black each side.
    let row = &out[384 * 1365..385 * 1365];
    assert!(row[..165].iter().all(|&p| p == 0));
    assert!(row[1365 - 165..].iter().all(|&p| p == 0));
    // clipcosmo: 20 frames at 15 fps, so the end at 4/3 s, the first
    // whole microsecond past it.
    let end = (20u64 * 1_000_000).div_ceil(15);
    assert_eq!(g.update_movie(ms(end - 1)), None);
    assert_eq!(
        g.update_movie(ms(end)).as_deref(),
        Some("Videos/clipcosmo.ogv")
    );
}

/// A talking head (`Videos/Dialogs/PC-MacOSX/rf0001.ogv` and its
/// `.alpha.ogv`): its sprite plays once, and `play()` after its end
/// starts it again from frame 0 at the new time (what `Dialog2`'s
/// `onVideoFinished` does to loop the idle videos).
#[test]
fn a_talking_head_plays_once_and_play_restarts_it() {
    let Some(dir) = install() else { return };
    let (files, _) = A4Files::open_dir(&dir).unwrap();
    let g = Game::new(files.clone(), 768);
    let table = g.table.as_ref();
    let mut s = VideoSprite::open(&files, "Videos/Dialogs/rf0001.ogv").unwrap();
    assert!(s.has_alpha());
    assert_eq!((s.size(), s.anim.n, s.anim.fps), ((800, 448), 50, 15.0));
    s.play(ms(0), table);
    assert_eq!(s.anim.frame, 0);
    let first = s.picture().unwrap().clone();
    // 50 frames at 15 fps: the last until 3.333 s.
    assert!(!s.update(ms(3_333_333), table).stopped);
    assert_eq!(s.anim.frame, 49);
    assert!(s.update(ms(3_333_334), table).stopped);
    assert!(!s.anim.playing);
    // Playing again: frame 0 at the new start, the same picture.
    s.play(ms(10_000_000), table);
    assert_eq!((s.anim.frame, s.anim.start_ms), (0, ms(10_000_000)));
    assert_eq!(s.picture(), Some(&first));
    // While it plays, `play()` changes nothing.
    s.update(ms(10_500_000), table);
    s.play(ms(11_000_000), table);
    assert_eq!((s.anim.frame, s.anim.start_ms), (7, ms(10_000_000)));
}
