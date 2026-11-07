//! The dialog loop against the original under emulation
//! (`tests/golden/dialog_ticks.txt`): seven Hoggar dialogs pass by pass —
//! lines, voices, body videos and their frames, the head's frame, the
//! field of view and the camera set-up, the reverse shots, the hidden
//! place animations, the frame draws and the end — with the harness's
//! voice model (3 `IsPlaying` calls) and a click in dialog 11.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/dialog_ticks.txt");

/// A golden event in the port's words, or `None` for one the port does
/// not log.
fn norm(e: &str) -> Option<String> {
    let name = e.split('(').next().unwrap();
    let arg = e.split_once('(').map_or("", |x| x.1).trim_end_matches(')');
    Some(match name {
        "Stop" | "Turn" | "Orient" | "Render" => name.to_string(),
        "OrientReset" => "Horizontalize".to_string(),
        "VideoOpen" | "Play" => e.to_string(),
        "LoadSfx" => format!("LoadSfx({})", arg.split(',').next().unwrap()),
        "VideoFrame" => e.to_string(),
        "HeadStep" => format!("HeadStep({})", arg.split(", ").nth(1).unwrap()),
        "SetFov" => format!("SetFov({})", &arg[..8]),
        "AnimHide" => format!("{name}({})", arg.split(',').next().unwrap()),
        // `AnimState` was stubbed (7) in the harness: whether a hidden
        // animation is shown again depends on the place's slots.
        "AnimShow" => return None,
        "CloseInterrupt" => "CloseInterrupt".to_string(),
        _ if e.starts_with("IsPlaying") => format!("IsPlaying->{}", e.rsplit("->").next().unwrap()),
        _ => return None,
    })
}

#[test]
fn the_dialog_loop_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).unwrap();
    assert!(g.new_game());
    let mut t = 0.0;
    while g.mode() != Mode::Warp || g.video().is_some() {
        t += 1.0 / 30.0;
        g.step(t);
        if g.video().is_some() {
            g.finish_video();
        }
    }
    g.reload_dialogs(1);
    g.voice_model = Some(3);
    // The harness stubbed the speakers' ticks and the heads.
    g.dialog_heads = false;
    g.dialog_events = Some(Vec::new());
    // Blocks: a header line and its events.
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(e) = l.strip_prefix("  EV ") {
            if e.starts_with("click flag") {
                blocks.last_mut().unwrap().1.push("CLICK".into());
            } else if let Some(n) = norm(e) {
                blocks.last_mut().unwrap().1.push(n);
            }
        } else {
            blocks.push((l.to_string(), Vec::new()));
        }
    }
    let mut checked = 0;
    let mut i = 0;
    while i < blocks.len() {
        let (head, want) = &blocks[i];
        let w: Vec<&str> = head.split(' ').collect();
        let mut want = want.clone();
        let click = want.first().is_some_and(|e| e == "CLICK");
        if click {
            want.remove(0);
            g.click();
        }
        let running = match w[0] {
            "START" => {
                g.start_dialog_now(w[1].parse().unwrap(), head.contains("voixoff=1"));
                true
            }
            "TICK" => g.dialog_pass(true),
            "FRAME" => g.dialog_pass(false),
            _ => panic!("{head}"),
        };
        if !running {
            // The close's events follow in the next block.
            i += 1;
            let (close, more) = &blocks[i];
            assert!(
                close.starts_with("CLOSE"),
                "{head}: the dialog ended, the original's did not"
            );
            want.extend(more.iter().filter(|e| !e.starts_with("SetFov")).cloned());
        }
        let got: Vec<String> = std::mem::take(g.dialog_events.as_mut().unwrap())
            .into_iter()
            .filter(|e| !e.starts_with("AnimShow"))
            .collect();
        assert_eq!(got, want, "{head}");
        checked += 1;
        i += 1;
    }
    assert!(checked > 100, "{checked}");
}
