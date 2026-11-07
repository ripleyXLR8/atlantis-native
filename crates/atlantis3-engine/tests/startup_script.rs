//! The interpreter from start-up to the first interactive frame against
//! the original's own run (`tests/golden/startup_script.txt`): chapter 0's
//! first `/INIT` (the funeral chamber), the menu's new game
//! (`(chapter=600)` → chapter 600 → chapter 1), then the warp frames whose
//! arrival scan plays cinematics 93, 1 and 2, each end resuming the
//! section. Compared: every line `SetVariables` runs, in order, the engine
//! calls (places, show / hide, view entries and their start angles,
//! cinematics, music) and the variables and flags at the end.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis2_engine::script::{AnimOp, Host, Script};
use atlantis3_data::A3Files;
use atlantis3_engine::wam_setup;

const GOLDEN: &str = include_str!("golden/startup_script.txt");

struct Rec {
    files: A3Files,
    calls: Vec<String>,
    cinema: bool,
}

impl Host for Rec {
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
        self.files.read(&format!("scenar/chapi{chapter:03}.con"))
    }
    fn load_wam(&mut self, name: &str) -> bool {
        self.calls.push(format!("CALL AtlWam_Load('{name}')"));
        true
    }
    fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {
        self.calls.push(format!(
            "CALL AtlWarp_EnterVue(view {view}; start yaw {:?} pitch {:?})",
            f64::from(yaw),
            f64::from(pitch)
        ));
    }
    fn anim(&mut self, op: AnimOp) {
        if let AnimOp::Show { kind, id, number } = op {
            self.calls
                .push(format!("CALL Show({id}, {kind}, {number})"));
        }
    }
    fn show_table(&mut self, id: i32, shown: bool, kind: i32) {
        if !shown {
            self.calls.push(format!("CALL Hide({id}, {kind}, 0)"));
        }
    }
    fn wam_commands(&mut self, wam: &str, flags: &[i32]) -> Vec<String> {
        wam_setup::commands(wam, flags)
    }
    fn play_cinema(&mut self, n: i32, fps: i32) -> bool {
        self.calls.push(format!(
            "CALL AtlHnm_start('cine{n:03}.hnm', 'cine{n:03}.txt', {fps}, 0) -> 1"
        ));
        self.cinema = true;
        true
    }
    fn start_music(&mut self, list: &str) -> bool {
        self.calls.push(format!("music {list}"));
        false
    }
}

#[test]
fn start_up_and_new_game_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let defnames = files.read("scenar/defnames.con").unwrap();
    let mut s = Script::new_a3(&defnames).unwrap();
    s.set_log = Some(Vec::new());
    let mut h = Rec {
        files,
        calls: Vec::new(),
        cinema: false,
    };
    let mut got = Vec::new();
    assert!(s.boot_a3(&mut h), "{:?}", s.fatal);
    assert!(s.new_game_a3(&mut h), "{:?}", s.fatal);
    for _ in 0..8 {
        s.gere_frame(&mut h, 1.884_955_6, 0.0);
        while std::mem::take(&mut h.cinema) {
            s.close_interrupt(&mut h);
        }
    }
    let sets: Vec<String> = s
        .set_log
        .take()
        .unwrap()
        .into_iter()
        .map(|l| format!("SET{l}"))
        .collect();
    got.append(&mut h.calls);
    let want_sets: Vec<String> = GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("LOG interp: "))
        .map(String::from)
        .collect();
    assert_eq!(sets, want_sets);
    let keep = |l: &&str| {
        [
            "CALL AtlWam_Load",
            "CALL Show",
            "CALL Hide",
            "CALL AtlWarp_EnterVue",
            "CALL AtlHnm_start",
        ]
        .iter()
        .any(|p| l.starts_with(p))
    };
    let want_calls: Vec<&str> = GOLDEN.lines().filter(keep).collect();
    let got_calls: Vec<&str> = got.iter().map(String::as_str).filter(keep).collect();
    assert_eq!(got_calls, want_calls);
    assert_eq!(got.iter().filter(|c| c.starts_with("music")).count(), 1);
    // The state after the last frame.
    for (name, v) in [
        ("CHAPTER", 1),
        ("NEARW77", 6),
        ("VUE", 4),
        ("LIMITEANGLEWARPX", -1),
        ("LIMITEANGLEWARPY", 70),
        ("CHANGECD", 2),
        ("MONDE", 1),
    ] {
        assert_eq!(s.var(name), Some(v), "{name}");
    }
    assert_eq!(s.var("ANGLEWARPX"), Some(108));
    assert_eq!((s.flags[1000], s.flags[1011], s.flags[1021]), (1, 4, 1));
    // The view slot the stubbed view entry leaves out of the golden.
    assert_eq!(s.flags[1001], 4);
    assert_eq!(s.chapter, 1);
    assert_eq!(s.a3.world, 1);
    let bag: Vec<(i32, u8)> = s.bag.iter().map(|e| (e.id, e.sprite)).collect();
    assert_eq!(bag, [(34, 1), (35, 2)]);
    assert!(!s.resume_pending);
}
