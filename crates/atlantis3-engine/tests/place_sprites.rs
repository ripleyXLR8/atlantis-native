//! The place animations against whole warp frames of the original
//! (`tests/golden/warp_first_hoggar5.txt`): the Hoggar's view 5 (the
//! neighbour of the first view) entered after the place's `CHANGEWAM`
//! commands, its seven slots as loaded (which are shown, at which frame),
//! then each golden frame drawn with the golden's camera matrix: the
//! sprites drawn into the faces, the cube, compared by MD5.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::Game;
use md5::{Digest, Md5};

const FIRST5: &str = include_str!("golden/warp_first_hoggar5.txt");

fn md5_hex(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn hoggar_view_5_with_its_animations() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut g = Game::new(files).unwrap();
    g.run_command("(monde=1)");
    g.run_command("(changewam=hoggar.wam)");
    g.run_command("(vue=5)");
    assert_eq!(g.place(), ("hoggar.wam", Some(5)));
    // The slots as the original loads them.
    let want = FIRST5
        .lines()
        .find_map(|l| l.strip_prefix("slots "))
        .unwrap();
    let got: Vec<String> = g
        .slots()
        .slots
        .iter()
        .enumerate()
        .map(|(k, s)| {
            format!(
                "{k}:{}:shown={}:mode={}:frame={}:count={}:list=-",
                s.name,
                u8::from(s.shown),
                s.mode,
                s.frame,
                s.count
            )
        })
        .collect();
    assert_eq!(got.join(" "), want.trim());
    let (mut n, mut bad) = (0, Vec::new());
    for line in FIRST5.lines().filter(|l| l.starts_with("full ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let fpu = if f[1] == "steady0e7f" {
            Fpu::Chop
        } else {
            Fpu::Double
        };
        let m: Vec<f32> = f[8]
            .split(',')
            .map(|b| {
                f32::from_bits(u32::from_le_bytes(
                    u32::from_str_radix(b, 16).unwrap().to_be_bytes(),
                ))
            })
            .collect();
        let screen = g.draw_with_matrix(&m.try_into().unwrap(), fpu);
        if md5_hex(screen) != f[7] {
            bad.push(format!("{} {} {}", f[1], f[5], f[6]));
        }
        n += 1;
    }
    assert_eq!(n, 24);
    assert!(
        bad.is_empty(),
        "{} of {n} frames differ: {bad:?}",
        bad.len()
    );
}
