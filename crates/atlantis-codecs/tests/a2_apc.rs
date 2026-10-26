//! Atlantis II's sounds (`datas_a2\apc\*.apc`: the place sounds, the
//! films' and moves' tracks, `NEWSOUND`, the puzzles') decoded whole and
//! compared with what the original atlantis2.exe's one-shot decode gives:
//! the PCM size, rate, channels and the md5 of the 16-bit samples
//! (`tests/golden/a2_apc.txt`, 161 distinct files of the four discs).
//!
//! Needs the game's discs: `ATLANTIS2_DISCS` = a folder holding the four
//! CD images. Without it the test is skipped. Run it with `--release`.

use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::files::disc_candidates;
use atlantis2_data::{A2Files, Folder};
use atlantis_codecs::decode_apc;
use atlantis_data::MemFiles;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/a2_apc.txt");

#[test]
fn every_sound_decodes_as_the_original() {
    let Some(dir) = std::env::var_os("ATLANTIS2_DISCS") else {
        eprintln!("ATLANTIS2_DISCS not set: test skipped");
        return;
    };
    let files = A2Files::new(
        Arc::new(MemFiles::new()),
        disc_candidates(&PathBuf::from(dir)),
    );
    assert_eq!(files.disc_numbers(), vec![1, 2, 3, 4]);
    let mut n = 0;
    for line in GOLDEN.lines().filter(|l| l.starts_with("oneshot.")) {
        let mut f = line.split(' ');
        let path = f.next().unwrap().strip_prefix("oneshot.cd").unwrap();
        let field = |name: &str| {
            line.split(' ')
                .find_map(|x| x.strip_prefix(name)?.strip_prefix('='))
                .unwrap_or_else(|| panic!("{name} in {line}"))
        };
        let disc: u8 = path[..1].parse().unwrap();
        let name = path[2..].strip_prefix("apc/").unwrap();
        assert_eq!(field("ret"), "0", "{line}");
        let data = files
            .read_from_disc(disc, Folder::Apc, name)
            .unwrap_or_else(|| panic!("disc {disc}: {name}"));
        let apc = decode_apc(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let pcm: Vec<u8> = apc.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let md5: String = Md5::digest(&pcm)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            (
                pcm.len().to_string(),
                apc.sample_rate.to_string(),
                u8::from(apc.channels == 2).to_string(),
                md5
            ),
            (
                field("pcm").to_string(),
                field("rate").to_string(),
                field("stereo").to_string(),
                field("md5").to_string()
            ),
            "disc {disc}: {name}"
        );
        n += 1;
    }
    assert_eq!(n, 161);
}
