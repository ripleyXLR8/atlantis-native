//! Prints the characters (not ASCII) of a release's chapter scripts, as
//! UTF-8, one line: what a subset of a CJK subtitle font must hold
//! (`assets/fonts/make_cjk_subset.py`).
//!
//!     cargo run -p atlantis-engine --example con_chars -- <game folder>
//!
//! The folder holds the game's files or the images of its discs. Every
//! `SCENAR\*.CON` member is decoded in the build's code page; in a
//! double-byte code page also every subtitle row the build's breaker cuts
//! from each dialog line, with and without the minor bug fixes
//! ([`cjk_subtitle_rows`]): a row can start on a character's second byte
//! (atlantis-cd-zh-cn-692736 FUN_0042f424: after an ideographic space @
//! 0x0042f5ef, or after a mark read across two characters, @
//! 0x0042f53c..0x0042f546 and 0x0042f620..0x0042f62b), and then reads its
//! pairs one byte out of step, so that the characters such a row draws are
//! in the subset too.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::{Game, StdDir};
use atlantis_engine::subtitles::{split_subtitle_cjk, CjkSplit};

/// The dialog lines of a CON file: the text after `]` and its space.
fn dialog_lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|&c| c == b'\n').filter_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() != Some(&b'[') {
            return None;
        }
        let rest = &line[line.iter().position(|&c| c == b']')? + 1..];
        Some(rest.strip_prefix(b" ").unwrap_or(rest))
    })
}

fn main() {
    let Some(dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: con_chars <game folder>");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    let game = Game::open(Arc::new(StdDir::new(dir)))
        .or_else(|_| {
            let set = DiscSet::open_folder(dir).ok_or(atlantis_data::GameError::NoArchives)?;
            Game::open(Arc::new(set))
        })
        .unwrap_or_else(|e| {
            eprintln!("{}: {e}", dir.display());
            std::process::exit(1);
        });
    let cp = game.exe.build.text_code_page;
    let splits: Vec<CjkSplit> = [false, true]
        .into_iter()
        .filter_map(|fixes| CjkSplit::of_build(game.exe.build, fixes))
        .collect();
    let mut names: Vec<String> = game
        .big
        .names()
        .filter(|n| n.starts_with("SCENAR\\") && n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    names.sort();
    let mut chars = BTreeSet::new();
    for name in &names {
        let Some(data) = game.big.read(name) else {
            continue;
        };
        let mut texts = vec![data.to_vec()];
        for line in dialog_lines(&data) {
            for &split in &splits {
                texts.extend(split_subtitle_cjk(line, split));
            }
        }
        for t in texts {
            chars.extend(cp.decode(&t).chars().filter(|&c| u32::from(c) > 0x7e));
        }
    }
    chars.remove(&'\0');
    eprintln!(
        "{}: {} scripts, {} characters",
        game.exe.build.name,
        names.len(),
        chars.len()
    );
    println!("{}", chars.into_iter().collect::<String>());
}
