//! The inventory's hover label against the original's own code
//! (`tests/golden/inventory_label.txt`): for every pointer position of the
//! golden, the object hovered, the label's two layouts and the line drawn
//! (position, advances, text by md5), at 640, 800 and 1024 pixels wide.
//!
//! Needs the game for `Objects.txt`: `ATLANTIS3_DATA`.

use atlantis2_engine::script::BagEntry;
use atlantis3_data::A3Files;
use atlantis3_engine::inventory::{label, Inventory};
use atlantis3_engine::text::GdiFont;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/inventory_label.txt");

const OBJECTS: [&str; 12] = [
    "HOG_OBJ_TOUPIE",
    "HOG_OBJ_BATON",
    "HOG_OBJ_DISQUE",
    "HOG_OBJ_CRANE",
    "CMP_TELECOMMANDE",
    "CMP_OMEGA",
    "CMP_CLEF1",
    "CMP_CLEF2",
    "CMP_TORCHE_ELECTRIQUE",
    "CMP_COUTEAU",
    "CHF_OISEAUBOIS",
    "CHF_ECRIN_SCARABEE",
];

fn md5(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn the_hover_label_as_the_original() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let data = files
        .read(atlantis3_data::objects::objects_path("fra").trim_start_matches("Datas/"))
        .expect("Objects.txt");
    let names = atlantis3_data::objects::ObjectNames::parse(&data).unwrap();
    // The bag: entries 0..11 (ids do not matter to the label).
    let bag: Vec<BagEntry> = (0..12)
        .map(|i| BagEntry {
            sprite: 0,
            id: i,
            world: 1,
        })
        .collect();
    let (mut w, mut h, mut fh) = (800, 600, 21);
    let mut want: Vec<String> = Vec::new();
    let mut got: Vec<String> = Vec::new();
    let mut picks = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split_whitespace().collect();
        match f[0] {
            "screen" => {
                (w, h) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                fh = l
                    .split("AtlFont_Default:")
                    .nth(1)
                    .and_then(|s| s.split(';').next())
                    .unwrap()
                    .parse()
                    .unwrap();
            }
            "pick" => {
                picks += 1;
                let scroll: usize = f[2].parse().unwrap();
                let p = (f[3].parse().unwrap(), f[4].parse().unwrap());
                let inv = Inventory {
                    scroll,
                    ..Default::default()
                };
                if let Some(e) = inv.hovered(w as usize, h as usize, p, &bag) {
                    let text = names.name(OBJECTS[e.id as usize]).expect("a name");
                    let font = GdiFont::arial(fh).unwrap();
                    let (z1, z2, lines) = label(font, text, p, w, h);
                    for z in [z1, z2] {
                        got.push(format!(
                            "LAYOUT {} {} {} {} {} {}",
                            z.ml, z.mr, z.mt, z.mb, z.halign, z.valign
                        ));
                    }
                    for line in lines {
                        let dx: Vec<String> = line.dx.iter().map(i32::to_string).collect();
                        got.push(format!(
                            "DRAW {} {} n={} dx={} | {}",
                            line.x,
                            line.y,
                            line.bytes.len(),
                            dx.join(","),
                            md5(&line.bytes)
                        ));
                    }
                }
            }
            "LAYOUT" => want.push(l.trim().to_string()),
            "DRAW" => {
                let (a, text) = l.split_once(" | ").unwrap();
                let g: Vec<&str> = a.split_whitespace().collect();
                let n = g.iter().find(|s| s.starts_with("n=")).unwrap();
                let dx = g.iter().find(|s| s.starts_with("dx=")).unwrap();
                want.push(format!("DRAW {} {} {n} {dx} | {text}", g[1], g[2]));
            }
            _ => panic!("{l}"),
        }
    }
    assert_eq!(got.len(), want.len());
    for (a, b) in got.iter().zip(&want) {
        assert_eq!(a, b);
    }
    assert_eq!((picks, want.len()), (936, 1683));
}
