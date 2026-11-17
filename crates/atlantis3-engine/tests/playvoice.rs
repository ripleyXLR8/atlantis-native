//! `PLAYVOICE=` in an *Atlantis III* script (atlantis3-gog-fr-3473408
//! @ 0x0044902a..0x004490f2, the 2001 CD build @ 0x0044888a..0x0044894b):
//! the value, then four more numbers, each read past the previous one's
//! digits and the non-digits after them, and 0x0040da70(value, a, b, c, d)
//! = (dialog, line, r, g, b). chapi031.con:126's `PlayVoice=214,2,200,200,200`
//! gives the beast's "Grrrrr" a light grey caption, not a yellow one.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;
use atlantis3_engine::Game;

#[test]
fn playvoice_reads_five_numbers() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let mut g = Game::new(A3Files::open_dir(&dir).unwrap().0).unwrap();
    g.trace = Some(Vec::new());
    g.run_command("(PlayVoice=214,2,200,200,200)");
    g.run_command("(PlayVoice=163,1,200,180,50)");
    let trace = g.trace.take().unwrap();
    let voices: Vec<&String> = trace.iter().filter(|l| l.starts_with("voice ")).collect();
    assert_eq!(
        voices,
        ["voice 214 2 200 200 200", "voice 163 1 200 180 50"]
    );
    let rgb: Vec<[i32; 3]> = g
        .test_voice_slots()
        .iter()
        .filter(|v| v.busy)
        .map(|v| v.rgb)
        .collect();
    assert!(rgb.contains(&[200, 200, 200]), "{rgb:?}");
    assert!(rgb.contains(&[200, 180, 50]), "{rgb:?}");
}
