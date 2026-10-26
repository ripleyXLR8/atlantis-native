//! `ANIM_THE_FRAME` (@ 0x00483350..0x0048339e): the playing byte (+0xbc)
//! of the first record of an id, of any kind, set to the value ≠ 0; a
//! value other than 0 or 1 is the original's error (FUN_00480cd0 with the
//! message @ 0x005dfcb4), the byte written after it. No chapter uses the
//! key: the command is run here on `miniature.wam` view 13's wheels.
//! Needs `ATLANTIS2_DATA` and `ATLANTIS2_DISCS`.

use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::A2Files;
use atlantis2_engine::{Engine, Fpu, LoopType, Variant};

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

#[test]
fn anim_the_frame_sets_the_playing_byte_of_the_first_record() {
    let Some(files) = game() else { return };
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        "(monde=3)",
        "(changewam=miniature.wam)",
        "(vue=13)",
        "(chapter=55)",
    ] {
        e.run_command(c.as_bytes());
    }
    settle(&mut e);
    assert_eq!(e.place(), ("miniature.wam", 13));
    let k1 = e
        .script_state()
        .unwrap()
        .names
        .id(defnames::OBJETS, b"RoueK1")
        .unwrap() as i32;
    let first = e.anim.slots.iter().position(|s| s.id == k1).unwrap();
    let playing = |e: &Engine| -> Vec<bool> { e.anim.slots.iter().map(|s| s.playing).collect() };
    let before = playing(&e);
    for (value, on) in [(0, false), (1, true), (0, false)] {
        e.run_command(format!("(ANIM_THE_FRAME=RoueK1,{value})").as_bytes());
        let mut want = before.clone();
        want[first] = on;
        assert_eq!(playing(&e), want, "ANIM_THE_FRAME {value}");
        assert_eq!(e.script_state().unwrap().fatal, None);
    }
    e.run_command(b"(ANIM_THE_FRAME=RoueK1,2)");
    assert!(e.anim.slots[first].playing);
    assert_eq!(
        e.script_state().unwrap().fatal.as_deref(),
        Some("SetVariables(): ANIM_THE_FRAME != 0 ou 1")
    );
}
