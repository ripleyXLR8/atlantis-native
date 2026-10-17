//! The executable's sound tables against the original atlantis.exe: the
//! music track names (0x00495eec) and the NewSound names (0x00496224).
//!
//! Needs the game (`ATLANTIS_DATA`); skipped without it.

mod common;

use common::{game, golden_map, md5_hex};

fn joined(names: &[String]) -> String {
    names
        .iter()
        .map(|n| n.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn music_and_newsound_tables_match_original() {
    let Some(game) = game("music_and_newsound_tables_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/scripts_music.txt"));
    let exe = &game.exe;
    assert_eq!(exe.music_names.len().to_string(), golden["music_count"]);
    assert_eq!(
        md5_hex(joined(&exe.music_names).as_bytes()),
        golden["music_names"]
    );
    assert_eq!(
        exe.newsound_names.len().to_string(),
        golden["newsound_count"]
    );
    assert_eq!(
        md5_hex(joined(&exe.newsound_names).as_bytes()),
        golden["newsound_names"]
    );
}
