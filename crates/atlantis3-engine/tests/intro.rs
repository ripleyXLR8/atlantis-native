//! The intro (`hnm\intro.hnm`) as CryoMovie plays it: 512 x 384, the sound
//! in its `AA` chunks (44100 Hz stereo, 2940 samples a frame: 15 frames a
//! second, the sound's clock), and the start-up's settings of the install.
//!
//! Needs the game: `ATLANTIS3_DATA` = the installed folder.

use std::sync::Arc;

use atlantis3_data::A3Files;
use atlantis3_engine::{Cinema, Startup};
use atlantis_codecs::AudioFormat;

fn files() -> Option<A3Files> {
    let dir = atlantis3_data::testing::install_dir()?;
    Some(A3Files::open_dir(&dir).expect("an Atlantis III install").0)
}

#[test]
fn startup_settings_of_the_install() {
    let Some(files) = files() else { return };
    let s = Startup::read(&files);
    assert_eq!(s.install_language, b"fra");
    assert_eq!(s.config.version, b"CD");
    assert_eq!(s.text_language(), "fra");
    assert_eq!(s.voice_language(), "fra");
    assert_eq!(s.screen(), (800, 600));
    assert_eq!(s.warp_view(), (640, 480));
    assert!(s.config.subtitle_on && s.config.cursor);
}

#[test]
fn intro_sound_and_pace() {
    let Some(files) = files() else { return };
    let data: Arc<[u8]> = files.read("hnm/intro.hnm").expect("intro.hnm").into();
    let mut c = Cinema::open("intro.hnm", data, 15).unwrap();
    assert_eq!(c.size(), (512, 384));
    assert_eq!(
        c.audio_format(),
        Some(AudioFormat {
            sample_rate: 44100,
            channels: 2
        })
    );
    assert!(c.advance());
    // The first frame holds the sound of 32 frames.
    assert_eq!(c.take_audio().len(), 32 * 2940 * 2);
    // 2940 samples at 44100 Hz: 66 2/3 ms a frame.
    assert_eq!(c.next_due_ms(), 66);
    assert!(c.advance());
    assert_eq!(c.take_audio().len(), 2940 * 2);
    assert_eq!(c.next_due_ms(), 133);
    let mut n = 2;
    while c.advance() {
        n += 1;
    }
    assert_eq!(n, c.frame_count());
    assert!(c.ended());
}

#[test]
fn frame_rates_the_original_refuses() {
    let data: Arc<[u8]> = Arc::from(vec![0u8; 64]);
    assert!(matches!(
        Cinema::open("x.hnm", data, 20),
        Err(atlantis3_engine::CinemaError::FrameRate(20))
    ));
}

/// A cinematic read by parts (the web build's way,
/// `atlantis2_engine::film_file`) plays as the file read whole: every
/// frame (RGB565 and 32 bits), its sound and its time. The intro, and the
/// new game's first film (`cine001.hnm`, 34 MB, the Hoggar's).
#[test]
fn a_cinematic_read_by_parts_plays_as_read_whole() {
    use atlantis2_engine::film_file;
    use atlantis3_data::spr::Depth;
    let Some(files) = files() else { return };
    for (path, depth) in [
        ("hnm/intro.hnm", Depth::Rgb565),
        ("hnm/HNM640/hoggar/cine001.hnm", Depth::Xrgb8888),
    ] {
        let open = |parts: bool| {
            film_file::set_by_parts(parts);
            let data = film_file::open(files.shared_source(), files.datas_path(path)).unwrap();
            film_file::set_by_parts(false);
            Cinema::open_depth(path, data, 15, depth).unwrap()
        };
        let (mut whole, mut parts) = (open(false), open(true));
        assert_eq!(whole.audio_format(), parts.audio_format());
        let mut n = 0;
        loop {
            let (a, b) = (whole.advance(), parts.advance());
            assert_eq!(a, b, "{path} frame {n}");
            if !a {
                break;
            }
            let same = match (whole.frame_px(), parts.frame_px()) {
                (
                    atlantis3_engine::cinema::Frame::Rgb565(x),
                    atlantis3_engine::cinema::Frame::Rgb565(y),
                ) => x == y,
                (
                    atlantis3_engine::cinema::Frame::Xrgb8888(x),
                    atlantis3_engine::cinema::Frame::Xrgb8888(y),
                ) => x == y,
                _ => false,
            };
            assert!(same, "{path} frame {n}");
            assert_eq!(whole.take_audio(), parts.take_audio(), "{path} frame {n}");
            assert_eq!(whole.next_due_ms(), parts.next_due_ms(), "{path} frame {n}");
            n += 1;
        }
        assert_eq!(n, whole.frame_count(), "{path}");
        assert!(parts.ended());
    }
}
