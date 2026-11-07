//! The lip-sync against the original under emulation
//! (`tests/golden/lipsync.txt`): for Hoggar lines of four speakers, the
//! analyser built from the line's voice and the speaker's record, then at
//! each frame the time, the nine weights and `PlayMorphingKey`'s factors,
//! bit for bit under 0x0e7f.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::Fpu;
use atlantis3_data::A3Files;
use atlantis3_engine::lipsync::{gw08, lerp_factors, t_of, LipSync};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/lipsync.txt");

fn field<'a>(l: &'a str, k: &str) -> &'a str {
    l.split(' ').find_map(|w| w.strip_prefix(k)).unwrap()
}

fn hexes(v: &[f32]) -> String {
    v.iter()
        .map(|x| format!("{:08x}", x.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn the_mouth_as_the_original_computes_it() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let persos =
        atlantis3_data::dialog::parse_info(&files.read("scenar/info.txt").unwrap()).unwrap();
    let fpu = Fpu::Chop;
    let mut cur: Option<LipSync> = None;
    let mut frames = 0;
    for l in GOLDEN.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = l.split(' ').collect();
        if w[0] == "LINE" {
            let speaker = field(l, "speaker=");
            let p = persos.iter().find(|p| p.name == speaker).unwrap();
            let apc = files
                .read(&format!("loc/voix/fra/hoggar/{}.apc", w[3]))
                .unwrap();
            let pcm = atlantis_codecs::decode_apc(&apc).unwrap().samples;
            let b: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            let md5: String = Md5::digest(&b).iter().map(|x| format!("{x:02x}")).collect();
            assert_eq!(md5, field(l, "pcm_md5="), "{l}");
            let gw = gw08(fpu, p.global_weight);
            assert_eq!(hexes(&[gw]), field(l, "gw08="), "{l}");
            let ls = LipSync::new(fpu, &pcm, p.smooth, p.stretch, p.tab_weights, gw);
            assert_eq!(hexes(&[ls.duration()]), field(l, "dur="), "{l}");
            cur = Some(ls);
        } else if w[0] == "F" {
            let k: f64 = w[1].parse().unwrap();
            let t = t_of(fpu, k * 0.04);
            assert_eq!(hexes(&[t]), field(l, "t="), "{l}");
            let out = cur.as_mut().unwrap().eval(t).unwrap();
            assert_eq!(hexes(&out), field(l, "out="), "{l}");
            assert_eq!(hexes(&lerp_factors(fpu, &out)), field(l, "lerp="), "{l}");
            frames += 1;
        }
    }
    assert_eq!(frames, 458);
}
