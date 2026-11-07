//! The speakers of `scenar\info.txt` as the original stores them
//! (`tests/golden/dialog_speakers.txt`, `AtlDial_MainInit` run under
//! emulation): mesh, hidden animations, smoothing, stretch, the nine
//! weights (0 in slot 0 for every speaker with `[TabWeightNN]` lines),
//! the global weight and the cameras' fields of view.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis3_data::A3Files;

const GOLDEN: &str = include_str!("golden/dialog_speakers.txt");

fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split(' ').find_map(|w| w.strip_prefix(k)).unwrap()
}

fn hexes(v: &[f32]) -> String {
    if v.is_empty() {
        return "-".into();
    }
    v.iter()
        .map(|x| format!("{:08x}", x.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn every_speaker_as_the_original_stores_it() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let info = files.read("scenar/info.txt").unwrap();
    let persos = atlantis3_data::dialog::parse_info(&info).unwrap();
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| l.starts_with("SPEAKER ")) {
        let name = l.split(' ').nth(1).unwrap();
        let p = persos
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(p.mesh, field(l, "mesh="), "{l}");
        let ids = if p.id_texts.is_empty() {
            "-".to_string()
        } else {
            p.id_texts.join(",")
        };
        assert_eq!(ids, field(l, "idtext="), "{l}");
        assert_eq!(hexes(&[p.smooth]), field(l, "smooth24="), "{l}");
        assert_eq!(hexes(&[p.stretch]), field(l, "stretch28="), "{l}");
        assert_eq!(hexes(&p.tab_weights), field(l, "w="), "{l}");
        assert_eq!(hexes(&[p.global_weight]), field(l, "global50="), "{l}");
        assert_eq!(hexes(&p.focale1), field(l, "focale1="), "{l}");
        assert_eq!(hexes(&p.focale2), field(l, "focale2="), "{l}");
        n += 1;
    }
    assert_eq!(n, 31);
}
