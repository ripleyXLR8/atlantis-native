//! The transition sprites over the travel videos against the original
//! under emulation (`tests/golden/travel_sprites.txt`): eight moves of
//! `hoggar.wam`, two of them with the entity of a bank hidden. Per move,
//! the banks the move opens (`id_spf` order, hidden entities left out);
//! per video frame, the sprite calls (sprite n − 1 of every bank at its
//! default position at frame n) and the MD5 of the 640 × 480 stretch
//! surface the video frame and the sprites are drawn into, before the
//! stretch to the screen; the last frame ends the video.
//!
//! The same at 640 × 480, 800 × 600 and 1024 × 768 (`travel_sprites_640.txt`,
//! `travel_sprites_1024.txt`): the screen size changes nothing, the
//! stretch surface is the video's size. The banks' folder follows WarpRes
//! (`SPF800` iff 1024; the goldens ran with 512).
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use atlantis2_data::wam::Wam;
use atlantis3_data::spr::{Bank, Depth};
use atlantis3_data::A3Files;
use atlantis3_engine::travel::{
    destination, draw_transition_sprites, spf_folder, transition_paths,
};
use atlantis_codecs::{HnmVideo, SoundRule};
use md5::{Digest, Md5};

const GOLDEN_640: &str = include_str!("golden/travel_sprites_640.txt");
const GOLDEN_800: &str = include_str!("golden/travel_sprites.txt");
const GOLDEN_1024: &str = include_str!("golden/travel_sprites_1024.txt");

fn md5_u16(px: &[u16]) -> String {
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One move of the golden: its tag, banks and frames (N, ret, md5,
/// draws).
#[derive(Default)]
struct Travel {
    tag: String,
    list: Vec<String>,
    frames: Vec<(u32, u32, String, String)>,
}

fn travels(golden: &str) -> Vec<Travel> {
    let mut out: Vec<Travel> = Vec::new();
    for line in golden.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "travel" if f[2].starts_with("list=") => {
                let list = f[2].strip_prefix("list=").unwrap();
                out.push(Travel {
                    tag: f[1].to_string(),
                    list: if list == "-" {
                        Vec::new()
                    } else {
                        list.split(',').map(str::to_string).collect()
                    },
                    frames: Vec::new(),
                });
                assert_eq!(f[3], "mode=4", "{line}");
            }
            "frame" if f[3].starts_with("ret=") => {
                let t = out.last_mut().unwrap();
                assert_eq!(t.tag, f[1], "{line}");
                let md5 = f[4].strip_prefix("md5=640x480:").unwrap();
                t.frames.push((
                    f[2].parse().unwrap(),
                    f[3].strip_prefix("ret=").unwrap().parse().unwrap(),
                    md5.to_string(),
                    f[5].strip_prefix("draws=").unwrap().to_string(),
                ));
            }
            _ => {}
        }
    }
    out
}

/// The golden of one screen size replayed.
fn replay(golden: &str) {
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let wam = Wam::parse(&files.read("wam/hoggar.wam").unwrap()).unwrap();
    let travels = travels(golden);
    assert_eq!(travels.len(), 8);
    let (mut banks_checked, mut frames_checked) = (0, 0);
    for t in &travels {
        // FROM_TO or FROM_TO_hide_NAME.
        let (moves, hide) = match t.tag.split_once("_hide_") {
            Some((m, h)) => (m, Some(h)),
            None => (t.tag.as_str(), None),
        };
        let (from, to) = moves.split_once('_').unwrap();
        let (from, to): (u32, u32) = (from.parse().unwrap(), to.parse().unwrap());
        let paths = transition_paths(&wam, 512, "hoggar", from, to, |name| Some(name) == hide);
        let want: Vec<String> = t.list.iter().map(|p| p.replace('\\', "/")).collect();
        assert_eq!(paths.len(), want.len(), "{}: {paths:?}", t.tag);
        for (a, b) in paths.iter().zip(&want) {
            assert!(a.eq_ignore_ascii_case(b), "{}: {a} != {b}", t.tag);
        }
        banks_checked += paths.len();
        let banks: Vec<Bank> = paths
            .iter()
            .map(|p| Bank::parse(&files.read(p).unwrap(), Depth::Rgb565).unwrap())
            .collect();
        let d = destination(&wam, from, to).unwrap();
        let data = files
            .read(&format!("hnmtraj/HNM640/hoggar/{}", d.video))
            .unwrap();
        let mut v = HnmVideo::with_rule(data, Vec::new(), SoundRule::ApcHeader).unwrap();
        let (w, h) = (v.width(), v.height());
        assert_eq!((w, h), (640, 480), "{}", t.tag);
        assert_eq!(t.frames.len() as u32, v.frame_count(), "{}", t.tag);
        for (k, (n, ret, md5, draws)) in t.frames.iter().enumerate() {
            assert_eq!(*n as usize, k, "{}", t.tag);
            v.next_frame().unwrap().unwrap();
            // The calls: sprite N of every bank (video frame N + 1), then
            // the flip; the video ends with its last frame.
            let calls: Vec<String> = t
                .list
                .iter()
                .map(|b| format!("{b}#{n}@def,0*0000803f"))
                .chain(std::iter::once("flip".to_string()))
                .collect();
            assert_eq!(&calls.join(","), draws, "{} frame {n}", t.tag);
            assert_eq!(*ret == 0, k + 1 == t.frames.len(), "{} frame {n}", t.tag);
            let mut frame = v.frame().to_vec();
            draw_transition_sprites(&mut frame, w, h, &banks, n + 1);
            assert_eq!(&md5_u16(&frame), md5, "{} frame {n}", t.tag);
            frames_checked += 1;
        }
    }
    assert_eq!((banks_checked, frames_checked), (11, 712));
}

#[test]
fn transition_sprites_as_the_original_draws_them_at_640() {
    replay(GOLDEN_640);
}

#[test]
fn transition_sprites_as_the_original_draws_them_at_800() {
    replay(GOLDEN_800);
}

#[test]
fn transition_sprites_as_the_original_draws_them_at_1024() {
    replay(GOLDEN_1024);
}

/// WarpRes 1024 selects `spf\SPF800\`, which the GOG install does not
/// ship: the move opens no bank.
#[test]
fn warp_res_1024_takes_the_spf800_folder() {
    assert_eq!(spf_folder(512), "spf/SPF640");
    assert_eq!(spf_folder(1024), "spf/SPF800");
    let Some(dir) = atlantis3_data::testing::install_dir() else {
        return;
    };
    let files = A3Files::open_dir(&dir).unwrap().0;
    let wam = Wam::parse(&files.read("wam/hoggar.wam").unwrap()).unwrap();
    let paths = transition_paths(&wam, 1024, "hoggar", 4, 5, |_| false);
    assert!(!paths.is_empty());
    for p in &paths {
        assert!(p.starts_with("spf/SPF800/hoggar/"), "{p}");
        assert!(files.read(p).is_none(), "{p}");
    }
}
