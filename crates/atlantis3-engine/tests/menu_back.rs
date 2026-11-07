//! The in-game menu's `BT_BACK` against the original under emulation
//! (`tests/golden/menu_back.txt`, @ 0x0043fd00 called by page 1's
//! `BT_BACK` @ 0x0045476d): `hoggar.wam` view 15 entered, the camera turned
//! to (150°, −10°), Escape (the angles stored @ 0x0042ac35), every slot's
//! frame set to 3 and the six faces overwritten (0, then 0x5a: what the
//! options' display switch leaves in them), then the menu left. The
//! original reads the view's cube again into the faces (each face's md5
//! equal to the entry's), sets the next entry's start angles to the Escape
//! angles, keeps the camera, loads the slots again (frames back to 0) and
//! runs the view hook. So after a display switch the panorama is back as
//! soon as the menu closes.
//!
//! Needs the game: `ATLANTIS3_DATA`.

mod common;

use atlantis2_engine::script::a3::deg_to_rad_a3;
use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::px::Faces;
use atlantis3_engine::Game;
use common::board::md5_screen;

const GOLDEN: &str = include_str!("golden/menu_back.txt");

fn faces_md5(g: &Game) -> Vec<String> {
    match g.place_faces() {
        Faces::Rgb565(f) => f.iter().map(|x| md5_screen(x)).collect(),
        Faces::Xrgb8888(_) => panic!("16-bit run"),
    }
}

fn hex(l: &str, k: &str) -> u32 {
    let v = l.split([' ', ',']).find_map(|w| w.strip_prefix(k)).unwrap();
    u32::from_str_radix(v, 16).unwrap()
}

/// Within `n` units in the last place (the port's camera and the
/// original's differ by rounding).
fn near(a: f32, b: u32, n: i64) -> bool {
    (i64::from(a.to_bits() as i32) - i64::from(b as i32)).abs() <= n
}

fn golden_slot_frames(l: &str) -> Vec<i32> {
    l.split(' ')
        .filter_map(|w| w.split(':').find_map(|f| f.strip_prefix("frame=")))
        .map(|f| f.parse().unwrap())
        .collect()
}

#[test]
fn menu_back_enters_the_view_again_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let entry: Vec<String> = GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("entry face "))
        .map(|l| l.split(' ').nth(1).unwrap().to_string())
        .collect();
    assert_eq!(entry.len(), 6);
    let esc = GOLDEN.lines().find(|l| l.starts_with("escape ")).unwrap();
    let (esc_yaw, esc_pitch) = (hex(esc, "yaw="), hex(esc, "pitch="));
    for case in ["zero", "fill"] {
        let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
        assert!(g.board_begin("hoggar.wam", 1));
        g.board_wam_setup();
        g.board_enter(15, 100.0);
        assert_eq!(faces_md5(&g), entry, "the entry's cube");
        let n = g.slots().slots.len();
        assert!(n > 0);
        // The camera turned, then Escape stores its angles.
        let fpu = Fpu::Chop;
        g.board_camera(deg_to_rad_a3(fpu, 150), deg_to_rad_a3(fpu, -10));
        g.escape();
        let (y, p) = g.escape_angles();
        assert!(near(y, esc_yaw, 8) && near(p, esc_pitch, 8), "{y} {p}");
        let aim = g.board_aim();
        for s in &mut g.test_slots_mut().slots {
            s.frame = 3;
        }
        g.test_fill_faces(if case == "zero" { 0 } else { 0x5a });
        g.menu_back();
        let want = |k: &str| {
            GOLDEN
                .lines()
                .find_map(|l| l.strip_prefix(&format!("case {case} {k}")))
                .unwrap()
                .to_string()
        };
        assert_eq!(want("faces="), "equal");
        assert_eq!(faces_md5(&g), entry, "{case}: the cube read again");
        // The next entry's start angles := the Escape angles.
        let s = g.script_mut().unwrap();
        assert_eq!((s.start_x, s.start_y), (y, p), "{case}");
        let start = want("start=");
        assert!(
            near(y, u32::from_str_radix(&start[..8], 16).unwrap(), 8),
            "{case}"
        );
        // The camera kept.
        assert_eq!(want("camera="), "same");
        assert_eq!(g.board_aim(), aim, "{case}: the camera kept");
        // The slots loaded again: every frame back to the golden's.
        let frames: Vec<i32> = g.slots().slots.iter().map(|s| s.frame).collect();
        let golden = golden_slot_frames(&want("slots "));
        assert_eq!(frames, golden, "{case}: the slots loaded again");
        assert!(frames.iter().all(|&f| f == 0));
    }
    // The calls, in the original's order: cube, start angles, slots
    // cleared, one load per slot, the view hook (its one command), the
    // coffer; no place sounds, no view entry.
    let calls: Vec<&str> = GOLDEN
        .lines()
        .filter_map(|l| l.strip_prefix("case zero call "))
        .map(|l| l.split(' ').next().unwrap())
        .collect();
    assert!(!calls.contains(&"place_sounds") && !calls.contains(&"view_entry"));
    assert_eq!(calls[..3], ["cube_load", "start_angles", "slots_clear"]);
}
