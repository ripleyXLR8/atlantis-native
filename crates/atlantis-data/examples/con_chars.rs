//! Prints the characters (not ASCII) of a release's chapter scripts, as
//! UTF-8, one line: what a subset of a CJK subtitle font must hold
//! (`assets/fonts/make_cjk_subset.py`).
//!
//!     cargo run -p atlantis-data --example con_chars -- <game folder>
//!
//! The folder holds the game's files or the images of its discs. Every
//! `SCENAR\*.CON` member is decoded in the build's code page; in a
//! double-byte code page also from the second byte of each `A1 A1`
//! (an ideographic space in code page 936), where the Simplified Chinese
//! build's breaker starts a row one byte out of step
//! (atlantis-cd-zh-cn-692736 FUN_0042f424 @ 0x0042f5ef), so that the
//! characters such a row reads are in the subset too.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use atlantis_data::disc_image::DiscSet;
use atlantis_data::{Game, StdDir};

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
        let mut texts = vec![&data[..]];
        if cp.is_double_byte() {
            for i in 0..data.len().saturating_sub(1) {
                if data[i] == 0xa1 && data[i + 1] == 0xa1 {
                    let end = data[i + 1..]
                        .iter()
                        .position(|&b| b == b'\r' || b == b'\n')
                        .map_or(data.len(), |e| i + 1 + e);
                    texts.push(&data[i + 1..end]);
                }
            }
        }
        for t in texts {
            chars.extend(cp.decode(t).chars().filter(|&c| u32::from(c) > 0x7e));
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
