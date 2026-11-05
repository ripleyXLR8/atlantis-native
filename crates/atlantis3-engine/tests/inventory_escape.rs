//! Escape in the panorama and in the inventory against the original under
//! emulation (`tests/golden/inventory_escape.txt`): the golden lists the
//! functions the Escape handler enters in each case and the state after.
//! Here a place sound, a voice slot and the subtitle line are set, Escape
//! is pressed ([`Game::escape`]), and what the golden's calls imply is
//! checked: the place sounds stopped only with the stop of every sound
//! object (0x00433580), the subtitle line cleared with 0x0040e330, the
//! voice slots freed only with 0x0040db90, the loop mode after (6 with
//! the menu's page, or 3 when the inventory's close put back the mode it
//! had saved) and the inventory's open byte.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::audio::CHANNEL_PLACE;
use atlantis3_engine::{Game, Mode};

const GOLDEN: &str = include_str!("golden/inventory_escape.txt");

struct Run {
    name: String,
    calls: Vec<String>,
    after: String,
}

fn runs() -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        if let Some(r) = l.strip_prefix("run ") {
            out.push(Run {
                name: r.split_whitespace().next().unwrap().to_string(),
                calls: Vec::new(),
                after: String::new(),
            });
        } else if let Some(c) = l.strip_prefix("call ") {
            out.last_mut().unwrap().calls.push(c.to_string());
        } else if let Some(a) = l.strip_prefix("after ") {
            out.last_mut().unwrap().after = a.to_string();
        }
    }
    out
}

fn field(st: &str, key: &str) -> i32 {
    st.split_whitespace()
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("{st}: no {key}"))
        .parse()
        .unwrap()
}

#[test]
fn escape_in_the_panorama_and_the_inventory_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let runs = runs();
    assert_eq!(runs.len(), 2);
    for run in runs {
        let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).expect("the game starts");
        g.flat_warp = true;
        assert!(g.load_place("PMagi.wam"));
        g.enter_at(10, 0.0, 0.0);
        g.test_text_op("world", &[3], "");
        g.test_text_op("subon", &[1], "");
        let mut t = 100.0;
        t += 1.0 / 15.0;
        g.step(t);
        assert_eq!(g.mode(), Mode::Warp);
        if run.name == "inventory" {
            g.right_click();
            for _ in 0..3 {
                t += 1.0 / 64.0;
                g.step(t);
            }
            assert_eq!(g.mode(), Mode::Inventory, "{}", run.name);
        }
        // A place sound (a looping one, as the place's ambient sources), a
        // voice in slot 0 and the subtitle line.
        g.audio.play(
            CHANNEL_PLACE,
            22050,
            1,
            Arc::new(vec![0; 2205]),
            true,
            [1.0, 1.0],
        );
        g.test_text_op("voice", &[199, 1, 200, 255, 255], "");
        g.test_text_op("sub", &[216, 1, 200, 255, 255], "");
        assert!(g.test_voice_slots()[0].busy);
        assert!(g.test_subtitle().is_some());

        g.escape();

        let has = |c: &str| run.calls.iter().any(|l| l == c);
        assert_eq!(
            g.audio.is_playing(CHANNEL_PLACE),
            !has("place_sounds_stop_all"),
            "{}: the place sounds",
            run.name
        );
        assert_eq!(
            g.test_subtitle().is_none(),
            has("subtitle_clear"),
            "{}: the subtitle line",
            run.name
        );
        assert_eq!(
            g.test_voice_slots()[0].busy,
            !has("voices_clear"),
            "{}: the voice slot",
            run.name
        );
        match field(&run.after, "mode=") {
            6 => {
                assert_eq!(g.mode(), Mode::Menu, "{}", run.name);
                let page = field(&run.after, "page=");
                assert_eq!(g.menu_page().map(|p| i32::from(p.0)), Some(page));
            }
            3 => assert_eq!(g.mode(), Mode::Warp, "{}", run.name),
            m => panic!("{}: mode {m}", run.name),
        }
        assert_eq!(
            u8::from(g.inventory_open()),
            field(&run.after, "inv_open=") as u8,
            "{}: the inventory",
            run.name
        );
    }
}
