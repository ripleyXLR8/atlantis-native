//! The dialogs' load and openings against the original's `CDialogue`
//! under emulation (`tests/golden/dialog_plan.txt`, `srand(1)`): every
//! `rand()` the load draws over `com.txt`, then each Hoggar dialog's
//! opening picks and every line's speaker, stand, camera, mood, voice and
//! video files, field of view, hidden animations and text.
//!
//! Needs the game: `ATLANTIS3_DATA`.

use atlantis2_engine::crt::Rand;
use atlantis3_data::A3Files;
use atlantis3_engine::dialog::Dialogs;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/dialog_plan.txt");

fn field<'a>(l: &'a str, key: &str) -> &'a str {
    l.split(' ')
        .find_map(|w| w.strip_prefix(key))
        .unwrap_or_else(|| panic!("{key} in {l}"))
}

#[test]
fn every_hoggar_dialog_line_as_the_original_plans_it() {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let mut rand = Rand::new(1);
    let mut d = Dialogs::load(&files, "hoggar", "fra", &mut rand);
    // The load: the draws per com.txt line, in order.
    let loads: Vec<&str> = GOLDEN.lines().filter(|l| l.starts_with("LOAD ")).collect();
    assert_eq!(loads.len(), d.load_draws.len());
    for (l, ((dg, ln), draws)) in loads.iter().zip(&d.load_draws) {
        let w: Vec<&str> = l.split(' ').collect();
        assert_eq!(
            (w[1].parse::<i32>().unwrap(), w[2].parse::<i32>().unwrap()),
            (*dg, *ln),
            "{l}"
        );
        let r = field(l, "rand=");
        let got = if draws.is_empty() {
            "-".to_string()
        } else {
            draws
                .iter()
                .map(|x| x.value.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(got, r, "{l}");
        if l.contains("speaker=") {
            let e = d.entry(*dg, *ln).unwrap();
            assert_eq!(e.speaker, field(l, "speaker="), "{l}");
            assert_eq!(e.stand.to_string(), field(l, "stand="), "{l}");
            assert_eq!(e.camera.to_string(), field(l, "camera="), "{l}");
            assert_eq!(e.mood.to_string(), field(l, "mood="), "{l}");
            assert_eq!(e.voice, field(l, "voice="), "{l}");
        }
    }
    // The openings.
    let mut lines = 0;
    let mut pending: Vec<i32> = Vec::new();
    let mut video_open = String::new();
    for l in GOLDEN.lines() {
        let w: Vec<&str> = l.split(' ').collect();
        match w[0] {
            "OPEN" => {
                let n: i32 = w[1].parse().unwrap();
                let voixoff = field(l, "voixoff=") == "1";
                assert_eq!(d.count(n).to_string(), field(l, "lines="), "{l}");
                pending = d
                    .open(n, voixoff, &mut rand)
                    .iter()
                    .map(|x| x.value)
                    .collect();
                pending.reverse();
                video_open.clear();
            }
            "RAND" => {
                assert_eq!(pending.pop(), Some(w[2].parse().unwrap()), "{l}");
            }
            "CAMS" => assert!(pending.is_empty(), "{l}"),
            "LINE" => {
                let (n, i): (i32, i32) = (w[1].parse().unwrap(), w[2].parse().unwrap());
                let e = d.entry(n, i).unwrap().clone();
                let voixoff = field(l, "video=") == "-";
                let p = d.perso(&e.speaker).unwrap();
                assert_eq!(e.stand.to_string(), field(l, "stand="), "{l}");
                assert_eq!(e.camera.to_string(), field(l, "camera="), "{l}");
                assert_eq!(u8::from(e.rnd).to_string(), field(l, "rnd="), "{l}");
                assert_eq!(
                    u8::from(Dialogs::is_player(&e.speaker)).to_string(),
                    field(l, "player=")
                );
                let voice = d.voice_path(&e, "fra").replace('/', "\\");
                assert!(voice.eq_ignore_ascii_case(field(l, "voice=")), "{l}");
                if !voixoff {
                    let video = d.video_path(&e).replace('/', "\\");
                    assert!(video.eq_ignore_ascii_case(field(l, "video=")), "{l}");
                    let op = if video == video_open { "keep" } else { "open" };
                    assert_eq!(op, field(l, "video_op="), "{l}");
                    video_open = video;
                }
                let f = d.focale(&e);
                assert_eq!(format!("{:08x}", f.to_bits()), field(l, "focale="), "{l}");
                let ids = if p.id_texts.is_empty() {
                    "-".to_string()
                } else {
                    p.id_texts.join(",")
                };
                assert_eq!(ids, field(l, "idtext="), "{l}");
                let md5: String = Md5::digest(&e.text)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                assert_eq!(
                    format!("{}/{}", e.text.len(), &md5[..8]),
                    field(l, "text="),
                    "{l}"
                );
                lines += 1;
            }
            _ => {}
        }
    }
    assert_eq!(lines, 171);
}
