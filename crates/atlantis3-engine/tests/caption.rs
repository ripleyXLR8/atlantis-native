//! The cinematics' caption fade against the original's own block
//! (`tests/golden/caption_fade.txt`): the opacity's bits and the colour
//! set (or the clear) at every frame of `Cine001.txt` and `Cine093.txt`.
//! The fade frames come from the golden; with `ATLANTIS3_DATA` the
//! install's caption files are checked to give the same.

use atlantis2_engine::Fpu;
use atlantis3_data::cine_txt::Caption;
use atlantis3_engine::caption::{CaptionFade, CaptionOp};

const GOLDEN: &str = include_str!("golden/caption_fade.txt");

#[test]
fn the_fade_frame_by_frame() {
    let mut blocks: Vec<(String, i32, i32, Vec<&str>)> = Vec::new();
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f[0] == "CAPTION" {
            blocks.push((
                f[1].to_string(),
                f[3].parse().unwrap(),
                f[5].parse().unwrap(),
                Vec::new(),
            ));
        } else if let Some(b) = blocks.last_mut() {
            b.3.push(line);
        }
    }
    assert_eq!(blocks.len(), 2);
    for (name, fin, fout, lines) in &blocks {
        let mut c = CaptionFade::new(Caption {
            fade_in_frame: *fin,
            fade_out_frame: *fout,
            text: Vec::new(),
        });
        let mut got = Vec::new();
        for frame in 1..=220 {
            let ops = c.frame(frame, Fpu::Double);
            if ops.is_empty() {
                continue;
            }
            let mut l = format!("FRAME {frame} {:08x}", c.opacity.to_bits());
            for op in ops {
                match op {
                    CaptionOp::Set([r, g, b]) => l.push_str(&format!(" SET {r} {g} {b} 1")),
                    CaptionOp::Clear => l.push_str(" CLEAR"),
                }
            }
            got.push(l);
        }
        // Without the opacity's decimal print (its bits are compared).
        let want: Vec<String> = lines
            .iter()
            .map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                format!("{} {} {} {}", f[0], f[1], f[2], f[4..].join(" "))
            })
            .collect();
        let got: Vec<String> = got
            .iter()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn the_install_captions() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = atlantis3_data::A3Files::open_dir(&dir).unwrap().0;
    for (name, fin, fout) in [("Cine001.txt", 50, 150), ("Cine093.txt", 105, 150)] {
        let c = Caption::parse(&files.read(&format!("loc/text/fra/{name}")).unwrap()).unwrap();
        assert_eq!((c.fade_in_frame, c.fade_out_frame), (fin, fout), "{name}");
    }
}
