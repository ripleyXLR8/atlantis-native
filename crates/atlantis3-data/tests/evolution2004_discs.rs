//! The discs of *Atlantis Evolution*'s 2004 CD: the North-American set
//! (`ATLANTIS4_DISCS_US`) and the European English set (`ATLANTIS4_DISCS_EU`),
//! each a folder holding the images of the four discs (any form
//! [`DiscSet::open_folder`] reads: `.iso`, or the raw `.bin` of a
//! `.bin`/`.cue` pair). Each set's images are told apart by their files
//! ([`disc_number`]), and the European discs hold the North-American
//! discs' files (atlantis4-re `discs/cd-us-2004.md`), so the European CD
//! installs the same game. Either variable unset skips the tests that need
//! it (a `SKIPPED` line on the standard error).

use std::io::Write;
use std::path::PathBuf;

use atlantis3_data::evolution2004::disc_number;
use atlantis_data::disc_image::{DiscImage, DiscSet};
use atlantis_data::FileSource;

/// The folder named by `var`, or `None` after a `SKIPPED` line.
fn discs(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => {
            let thread = std::thread::current();
            let test = thread.name().unwrap_or("?");
            let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {var} not set");
            None
        }
    }
}

/// The set's four images in disc order, each checked to be one disc.
fn by_disc(dir: &std::path::Path) -> Vec<DiscImage> {
    let set = DiscSet::open_folder(dir).unwrap_or_else(|| panic!("{}: no image", dir.display()));
    let mut images: Vec<(u8, DiscImage)> = set
        .images
        .into_iter()
        .map(|i| (disc_number(&i), i))
        .collect();
    images.sort_by_key(|(n, _)| *n);
    let numbers: Vec<u8> = images.iter().map(|(n, _)| *n).collect();
    assert_eq!(numbers, [1, 2, 3, 4], "{}", dir.display());
    images.into_iter().map(|(_, i)| i).collect()
}

/// Every file of `image`'s root (the discs have one folder, `directx9`,
/// on disc 1).
fn files(image: &DiscImage) -> Vec<String> {
    let mut out = Vec::new();
    for e in image.list("") {
        if image.has_dir(&e) {
            out.extend(image.list(&e).into_iter().map(|f| format!("{e}/{f}")));
        } else {
            out.push(e);
        }
    }
    out
}

#[test]
fn each_set_holds_the_four_discs() {
    for var in ["ATLANTIS4_DISCS_US", "ATLANTIS4_DISCS_EU"] {
        let Some(d) = discs(var) else { continue };
        let images = by_disc(&d);
        assert!(images[0].size("Atlantis Evolution.msi").is_some());
        assert!(images[0].size("Data1.cab").is_some());
    }
}

/// The European discs' files are the North-American discs' files: the
/// same names and sizes, and the same bytes (read a megabyte at a time).
#[test]
fn the_european_discs_hold_the_north_american_files() {
    let (Some(us), Some(eu)) = (discs("ATLANTIS4_DISCS_US"), discs("ATLANTIS4_DISCS_EU")) else {
        return;
    };
    const CHUNK: u64 = 1 << 20;
    for (n, (u, e)) in by_disc(&us).iter().zip(by_disc(&eu).iter()).enumerate() {
        let names = files(u);
        assert_eq!(names, files(e), "disc {}", n + 1);
        for name in &names {
            let size = u.size(name).unwrap();
            assert_eq!(e.size(name), Some(size), "disc {}: {name}", n + 1);
            let mut at = 0;
            while at < size {
                let len = CHUNK.min(size - at) as usize;
                assert!(
                    u.read_at(name, at, len) == e.read_at(name, at, len),
                    "disc {}: {name} differs at {at}",
                    n + 1
                );
                at += len as u64;
            }
        }
    }
}
