//! How many reads *Atlantis V* makes from the French DVD's files
//! (`ATLANTIS5_DVD_FR`; skipped without it): the disc's files (`files/`,
//! as a drive gives them) and the disc's image (`soa-fr.iso`), each read
//! through a counting source. On an optical drive each read is a seek:
//! opening the data, the start up to the window, and the first view must
//! need few, large reads.
//!
//! The counts are printed (`--nocapture`) with a modelled DVD time (a
//! seek of 100 ms per read, 10 MB/s).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use atlantis3_data::atlantis5::{Atlantis5Source, Form};
use atlantis3_engine::atlantis5_play::Play;
use atlantis_data::disc_image::DiscImage;
use atlantis_data::{FileSource, StdDir};

/// A source that counts the reads made of another.
struct Counting {
    inner: Arc<dyn FileSource>,
    reads: AtomicU64,
    bytes: AtomicU64,
}

impl Counting {
    fn snapshot(&self) -> (u64, u64) {
        (
            self.reads.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        )
    }
}

impl FileSource for Counting {
    fn size(&self, name: &str) -> Option<u64> {
        self.inner.size(name)
    }
    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(len as u64, Ordering::Relaxed);
        self.inner.read_at(name, offset, len)
    }
    fn has_dir(&self, name: &str) -> bool {
        self.inner.has_dir(name)
    }
    fn list(&self, dir: &str) -> Vec<String> {
        self.inner.list(dir)
    }
}

fn report(what: &str, (r0, b0): (u64, u64), (r1, b1): (u64, u64), t: Instant) -> (u64, u64) {
    let (reads, bytes) = (r1 - r0, b1 - b0);
    let model = reads as f64 * 0.1 + bytes as f64 / 10e6;
    println!(
        "{what}: {reads} reads, {:.1} MB, {:.1} s here, modelled DVD {model:.0} s",
        bytes as f64 / 1e6,
        t.elapsed().as_secs_f64()
    );
    (reads, bytes)
}

/// Opens the data through `inner`, starts the game up to the window
/// (the logos' first film), then plays a new game to its first view.
fn measure(
    label: &str,
    inner: Arc<dyn FileSource>,
    form: Form,
    index: Option<PathBuf>,
) -> [(u64, u64); 3] {
    let c = Arc::new(Counting {
        inner,
        reads: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    });
    let t = Instant::now();
    let src =
        Atlantis5Source::from_disc_indexed(c.clone(), form, index).expect("the disc's cabinets");
    let open = report(&format!("{label} open"), (0, 0), c.snapshot(), t);
    let s0 = c.snapshot();
    let t = Instant::now();
    let src: Arc<dyn FileSource> = Arc::new(src);
    let mut play = Play::new(src.clone(), "french", None).expect("play");
    play.boot();
    play.update(1.0 / 30.0);
    let start = report(&format!("{label} start"), s0, c.snapshot(), t);
    let s1 = c.snapshot();
    let t = Instant::now();
    let mut play = Play::new(src, "french", None).expect("play");
    play.new_game();
    for _ in 0..600 {
        play.update(1.0 / 30.0);
        if play.stage().film_playing() {
            play.skip();
        }
        if play.game.state() == 4 && !play.game.busy() {
            break;
        }
    }
    assert_eq!(play.stage().address(), Some((b"zeppelin".to_vec(), 3)));
    let first = report(&format!("{label} first view"), s1, c.snapshot(), t);
    [open, start, first]
}

fn check(counts: [(u64, u64); 3]) {
    let [open, start, first] = counts;
    // Header tables read in one piece each; data in parts of 1 MiB
    // (before: 35,584 reads to the start, 76,717 to the first view, most
    // of them a block's 8-byte header).
    assert!(open.0 <= 16, "open: {} reads", open.0);
    assert!(start.0 <= 500, "start: {} reads", start.0);
    assert!(first.0 <= 800, "first view: {} reads", first.0);
    for (r, b) in counts {
        assert!(
            b / r.max(1) > 512 * 1024 || r <= 16,
            "{r} reads of {b} bytes"
        );
    }
}

#[test]
fn reads_from_the_disc_files() {
    let Some(d) = atlantis3_data::testing::a5_dvd_fr() else {
        return;
    };
    let files = d.join("files");
    let counts = measure(
        "files/",
        Arc::new(StdDir::new(&files)),
        Form::DiscFolder,
        None,
    );
    check(counts);
}

#[test]
fn reads_from_the_disc_image() {
    let Some(d) = atlantis3_data::testing::a5_dvd_fr() else {
        return;
    };
    let iso = d.join("soa-fr.iso");
    if !Path::new(&iso).is_file() {
        eprintln!("SKIPPED: no soa-fr.iso");
        return;
    }
    let img = DiscImage::open(&iso).expect("the image");
    let counts = measure("soa-fr.iso", Arc::new(img), Form::DiscImage, None);
    check(counts);
}

/// A second run with the checkpoint index the first one kept starts its
/// decoding near what it reads: the start reads a few parts, not the
/// films' folder from its beginning.
#[test]
fn a_second_run_with_the_checkpoint_index() {
    let Some(d) = atlantis3_data::testing::a5_dvd_fr() else {
        return;
    };
    let index = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("a5-cab-index");
    let _ = std::fs::remove_dir_all(&index);
    let files = d.join("files");
    let run = |label: &str| {
        measure(
            label,
            Arc::new(StdDir::new(&files)),
            Form::DiscFolder,
            Some(index.clone()),
        )
    };
    let first = run("files/, new index");
    let second = run("files/, index kept");
    assert_eq!(first[0], second[0], "the same open");
    assert!(
        second[1].1 * 10 < first[1].1,
        "start: {:?} then {:?}",
        first[1],
        second[1]
    );
    assert!(
        second[2].1 * 4 < first[2].1,
        "first view: {:?} then {:?}",
        first[2],
        second[2]
    );
    // The bytes read from a checkpoint a previous run kept are the
    // extracted tree's.
    let disc = Atlantis5Source::from_disc_indexed(
        Arc::new(StdDir::new(&files)),
        Form::DiscFolder,
        Some(index.clone()),
    )
    .unwrap();
    let tree = Atlantis5Source::open(&d.join("datas")).unwrap();
    for name in ["datas/videos/logo_aie.bik", "datas/videos/cine01.bik"] {
        let size = tree.size(name).unwrap();
        let at = size - 100_000;
        assert_eq!(
            disc.read_at(name, at, 100_000),
            tree.read_at(name, at, 100_000),
            "{name}"
        );
    }
    let kept: u64 = std::fs::read_dir(&index)
        .unwrap()
        .flatten()
        .map(|e| e.metadata().unwrap().len())
        .sum();
    println!("index: {:.1} MB", kept as f64 / 1e6);
    let _ = std::fs::remove_dir_all(&index);
}
