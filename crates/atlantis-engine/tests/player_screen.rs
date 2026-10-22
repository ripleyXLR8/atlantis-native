//! The player screen's blocks (`PlayerScreenState`: the click, the delete,
//! the name entry, the exits and the exit's commit, in the order
//! `Engine::player_screen` runs them) against the original's loop
//! `FUN_0041dea4` run frame by frame under emulation (golden
//! `ui_player_screen.txt`: 17 scenarios, 177 frames): typing 17 keys,
//! Backspace on the 16th character and below, refused keys, clicks while
//! typing, DELETE (cancelled, confirmed, on the new player row), the
//! sticky double-click flag, OK and Enter commits, five players. Every
//! printed field is compared, the rows after each frame and the player
//! each exit starts. Needs no game data.

use std::collections::HashMap;

use atlantis_data::saves::{Players, MAX_PLAYERS, NAME_LEN};
use atlantis_engine::ui::player_screen::{
    player_screen_exit, PlayerScreenExit, PlayerScreenExitResult, PlayerScreenState,
};

const GOLDEN: &str = include_str!("golden/ui_player_screen.txt");
const MARKER: &[u8] = b"#New Player";

fn unesc(v: &str) -> Vec<u8> {
    let b = v.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1] == b'x' {
            out.push(u8::from_str_radix(&v[i + 2..i + 4], 16).unwrap());
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

fn esc(v: &[u8]) -> String {
    v.iter()
        .map(|&c| {
            if (0x21..=0x7e).contains(&c) && !b"\\|=".contains(&c) {
                char::from(c).to_string()
            } else {
                format!("\\x{c:02x}")
            }
        })
        .collect()
}

/// The `key=value` words after "in" and after "out".
fn tokens(line: &str) -> (HashMap<&str, &str>, HashMap<&str, &str>) {
    let (mut inp, mut out) = (HashMap::new(), HashMap::new());
    let mut cur: Option<&mut HashMap<&str, &str>> = None;
    for t in line.split(' ') {
        match t {
            "in" => cur = Some(&mut inp),
            "out" => cur = Some(&mut out),
            _ => {
                if let (Some(m), Some((k, v))) = (cur.as_mut(), t.split_once('=')) {
                    m.insert(k, v);
                }
            }
        }
    }
    (inp, out)
}

/// The player list as the original holds it: the names, `count` rows.
fn players_of(names: &str) -> Players {
    let mut p = Players::new(MARKER);
    p.rows = [[0; NAME_LEN]; MAX_PLAYERS];
    let names: Vec<Vec<u8>> = names.split('|').map(unesc).collect();
    for (i, n) in names.iter().enumerate() {
        p.rows[i][..n.len()].copy_from_slice(n);
    }
    p.count = names.len();
    p
}

fn rows(p: &Players) -> String {
    (0..p.count)
        .map(|i| esc(p.name(i)))
        .collect::<Vec<_>>()
        .join("|")
}

/// The golden run through the port's blocks; `keep_fifth_name` is the
/// minor bug fixes' rule (the original: false).
fn run(keep_fifth_name: bool, mut each: impl FnMut(&str, &HashMap<&str, &str>, &Players)) {
    let (mut players, mut st) = (Players::new(MARKER), PlayerScreenState::default());
    let (mut scenarios, mut frames, mut exits) = (0, 0, 0);
    for line in GOLDEN.lines() {
        if let Some(rest) = line.strip_prefix("S ") {
            let (_, names) = rest.split_once(" names=").unwrap();
            let (names, _) = names.split_once(' ').unwrap();
            players = players_of(names);
            st = PlayerScreenState::default();
            scenarios += 1;
            continue;
        }
        if !line.starts_with("F ") {
            continue;
        }
        frames += 1;
        let where_ = &line[..line.find(" in ").unwrap()];
        let (inp, out) = tokens(line);
        // The window procedure's writes, then the frame.
        if inp["key"] != "-" {
            st.key = u8::from_str_radix(inp["key"], 16).unwrap();
        }
        if inp["dbl"] == "1" {
            st.dbl_flag = true;
        }
        let click = inp["click"] == "1";
        let record = match inp["rec"].parse::<usize>().unwrap() {
            255 => None,
            r => Some(r),
        };
        let grid: i32 = inp["grid"].parse().unwrap();
        let mut del = "-".to_string();
        if st.click(click, record, grid, &players) {
            del = st.row.to_string();
            let confirmed = inp["sure"] == "3";
            if confirmed {
                players.delete(st.row as usize);
            }
            st.delete_done(confirmed);
        }
        let shown = if st.field.is_empty() {
            "-".to_string()
        } else {
            esc(&st.field)
        };
        let mut exit = st.exit_before_draw(&players, false, false);
        if exit == PlayerScreenExit::None {
            exit = st.exit_after_draw(click, record);
        }
        let mut end = PlayerScreenExitResult::default();
        if exit != PlayerScreenExit::None {
            end = player_screen_exit(&mut st, exit, record, &mut players, keep_fifth_name);
            exits += 1;
        }
        let exit_name = match exit {
            PlayerScreenExit::None => "-",
            PlayerScreenExit::Enter => "enter",
            PlayerScreenExit::Double => "dbl",
            PlayerScreenExit::Ok => "ok",
        };
        // full / last / cur are '?' where the original has not written
        // them yet: no value to compare.
        let unknown = |k: &str, v: String| if out[k] == "?" { "?".to_string() } else { v };
        let port = format!(
            "row={} len={} full={} last={} blink={} cur={} dbl={} key={:02x} field={} shown={} del={} exit={} commit={} atl={} enterdraw={}",
            st.row,
            st.len,
            unknown("full", u8::from(st.full).to_string()),
            unknown("last", format!("{:02x}", st.last_key)),
            u8::from(st.blink_on),
            unknown("cur", st.current.to_string()),
            u8::from(st.dbl_flag),
            st.key,
            esc(&st.field),
            shown,
            del,
            exit_name,
            u8::from(end.commit),
            u8::from(exit != PlayerScreenExit::None && end.write),
            if exit == PlayerScreenExit::Enter {
                esc(&st.field)
            } else {
                "-".to_string()
            },
        );
        let keys = [
            "row",
            "len",
            "full",
            "last",
            "blink",
            "cur",
            "dbl",
            "key",
            "field",
            "shown",
            "del",
            "exit",
            "commit",
            "atl",
            "enterdraw",
        ];
        let orig = keys
            .iter()
            .map(|k| format!("{k}={}", out[k]))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(port, orig, "{where_}");
        if exit != PlayerScreenExit::None {
            // The player the exit starts: the original's current row.
            assert_ne!(out["cur"], "?", "{where_}");
            assert_eq!(end.chosen, Some(out["cur"].parse().unwrap()), "{where_}");
        }
        each(where_, &out, &players);
    }
    assert_eq!((scenarios, frames), (17, 177));
    assert!(exits >= 15);
}

/// The original: every field, and the rows after each frame (the fifth
/// player of five emptied by OK included).
#[test]
fn player_screen_matches_original() {
    run(false, |where_, out, players| {
        assert_eq!(rows(players), out["names"], "{where_}");
        assert_eq!(players.count.to_string(), out["count"], "{where_}");
    });
}

/// The minor bug fixes keep the fifth name on OK; the rest is the
/// original's.
#[test]
fn player_screen_keeps_the_fifth_name_with_the_fixes() {
    let mut kept = 0;
    run(true, |where_, out, players| {
        if where_ == "F five_ok_last 1" {
            assert_eq!(out["names"], "A1|B2|C3|D4|");
            assert_eq!(rows(players), "A1|B2|C3|D4|E5");
            kept += 1;
        } else if !where_.starts_with("F five_") {
            assert_eq!(rows(players), out["names"], "{where_}");
        }
    });
    assert_eq!(kept, 1);
}
