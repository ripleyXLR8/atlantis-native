//! The CON script interpreter and the music against the original
//! `atlantis.exe` (2001 English build): the parse of every CHAPInnn.CON
//! (the entry scanners `FUN_004280f8` / `FUN_004281f8` / `FUN_0042bef4` /
//! `FUN_0042bfa4`, the section executor `FUN_0042844c`, the /INIT runner
//! `FUN_00425eb0`), the condition evaluator `FUN_004298ac`, the /set parser
//! `FUN_0042a100`, the subject table, the clocks, the music lists and the
//! music ring, and the scans of the place loop `FUN_00423ad8`.
//!
//! The expected values come from the original code run under emulation
//! (see the headers of the golden files); they hold numbers, file names and
//! digests only. Ported from ScummVM's `test/engines/cryomni3d/atlantis/`
//! (`scripts_con_parse.h`, `scripts_con_eval.h`, `scripts_con_subjects.h`,
//! `scripts_con_musik.h`, `scripts_music.h`, `audio_apc_data.h`).
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory;
//! without it they are skipped. Run them with `--release`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use atlantis_data::{Game, StdDir};
use atlantis_engine::audio::Mixer;
use atlantis_engine::con::script::{latin1_bytes, ConText};
use atlantis_engine::con::{
    arrival_after_chapter_load, con_angle_radians, con_entry_allowed, con_escape_skip,
    con_music_apply, con_music_list, con_track_letter, evaluate, parse_con_set, ConClocks, ConItem,
    ConKeyBuffer, ConScript, ConSection, ConSubjects, ConVariableNames, CON_PLAY_COUNTERS,
};
use atlantis_engine::hooks::DirectLoader;
use atlantis_engine::music::MusicRing;
use atlantis_engine::sys::{Runner, Sys};
use atlantis_engine::{Engine, Event, Options, Scripts};
use md5::{Digest, Md5};

fn md5_hex(b: &[u8]) -> String {
    Md5::digest(b).iter().map(|b| format!("{b:02x}")).collect()
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
}

/// "key value" lines.
fn golden_map(text: &str) -> HashMap<&str, &str> {
    lines(text).filter_map(|l| l.rsplit_once(' ')).collect()
}

fn game(test: &str) -> Option<Game> {
    let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
        eprintln!("ATLANTIS_DATA not set: {test} skipped");
        return None;
    };
    Some(Game::open(Arc::new(StdDir::new(dir))).expect("cannot open the game directory"))
}

fn con_files(g: &Game, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = g
        .big
        .names()
        .filter(|n| n.starts_with(prefix) && n.ends_with(".CON"))
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

fn read(g: &Game, name: &str) -> Vec<u8> {
    g.big.read(name).unwrap_or_else(|| panic!("{name} missing"))
}

fn varias(g: &Game) -> ConVariableNames {
    ConVariableNames::load(&read(g, "SCENAR\\VARIAS.CON"))
}

/// The VARIAS.CON names in lower case, in file order, empty lines skipped.
fn varias_lower(g: &Game) -> Vec<String> {
    read(g, "SCENAR\\VARIAS.CON")
        .split(|&c| c == b'\r' || c == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| String::from_utf8_lossy(l).to_ascii_lowercase())
        .collect()
}

// The canonical dump of a parse (the golden generator's format).

fn canonical_dump(script: &ConScript) -> Vec<u8> {
    let mut out = String::new();
    for (cmd, _) in script.init_cmds() {
        out += &format!("I {cmd}\n");
    }
    for sec in script.sections() {
        // An entry the /con scanner still finds with every play counter at
        // 0xff is "con*".
        let kind = if !sec.is_entry {
            "none".to_string()
        } else if sec.is_timer {
            format!("tim{}", sec.timer_num)
        } else if sec.is_con {
            if sec.repeatable || sec.con_limit > 0xff {
                "con*".into()
            } else {
                "con".into()
            }
        } else {
            "sel".into()
        };
        out += &format!("S {} {kind}\n", sec.id);
        for it in &sec.items {
            match it {
                ConItem::Set { cmd, .. } => out += &format!("SET {cmd}\n"),
                ConItem::Line(l) => out += &format!("DLG {}|{}|{}\n", l.speaker, l.params, l.text),
            }
        }
        if sec.goto_id != 0 {
            out += &format!("GO {}\n", sec.goto_id);
        }
    }
    latin1_bytes(&out)
}

// The deterministic variable states of the golden generator.

#[derive(Clone, Debug, Default)]
struct VarState {
    vars: HashMap<String, i32>,
    inventory: Vec<i32>,
}

fn lcg(x: u32) -> u32 {
    x.wrapping_mul(1_103_515_245).wrapping_add(12345)
}

fn rnd(x: u32) -> u32 {
    (x >> 16) & 0x7fff
}

/// The raw condition lines: '/' + sel|con|tim (any case) at a line start.
fn cond_lines(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let st = i;
        while i < data.len() && data[i] != b'\r' && data[i] != b'\n' {
            i += 1;
        }
        let line = data[st..i].to_vec();
        if i < data.len() && data[i] == b'\r' && i + 1 < data.len() && data[i + 1] == b'\n' {
            i += 2;
        } else if i < data.len() {
            i += 1;
        }
        if line.len() >= 4 && line[0] == b'/' {
            let d = line[1..4].to_ascii_lowercase();
            if d == b"sel" || d == b"con" || d == b"tim" {
                out.push(line);
            }
        }
    }
    out
}

fn make_state(line_idx: usize, k: usize, var_names: &[String], line: &[u8]) -> VarState {
    let mut runs = Vec::new();
    let mut pos = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if line[i].is_ascii_digit() {
            let st = i;
            let mut n = 0i32;
            while i < line.len() && line[i].is_ascii_digit() {
                n = n.wrapping_mul(10).wrapping_add(i32::from(line[i] - b'0'));
                i += 1;
            }
            runs.push(n);
            pos.push(st);
        } else {
            i += 1;
        }
    }
    let mut cands = vec![0];
    for &r in &runs {
        for v in [(r - 1).max(0), r, r + 1] {
            if !cands.contains(&v) {
                cands.push(v);
            }
        }
    }
    let low = line.to_ascii_lowercase();
    let mut inv = Vec::new();
    for (i, &r) in runs.iter().enumerate() {
        if pos[i] >= 6 && &low[pos[i] - 6..pos[i]] == b"invent" && !inv.contains(&r) {
            inv.push(r);
        }
    }
    let mut x = ((line_idx as u32) + 1)
        .wrapping_mul(7919)
        .wrapping_add((k as u32).wrapping_mul(104_729));
    let mut st = VarState::default();
    for v in var_names {
        x = lcg(x);
        st.vars
            .insert(v.clone(), cands[rnd(x) as usize % cands.len()]);
    }
    for &id in &inv {
        x = lcg(x);
        if rnd(x) & 1 != 0 {
            st.inventory.push(id);
        }
    }
    st
}

fn eval_at(
    names: &ConVariableNames,
    text: &[u8],
    offset: u32,
    st: &VarState,
    key: Option<&mut ConKeyBuffer>,
) -> bool {
    let value = |n: &str| st.vars.get(n).copied().unwrap_or(0);
    evaluate(names, &value, &st.inventory, text, offset, key)
}

/// The evaluator on one condition line laid out as the golden generator
/// lays it out (line + "\r\n").
fn port_eval(names: &ConVariableNames, line: &[u8], st: &VarState) -> bool {
    let mut text = line.to_vec();
    text.extend_from_slice(b"\r\n");
    eval_at(names, &text, 0, st, None)
}

/// The condition line of an entry, as the scanner reads it.
fn entry_line(script: &ConScript, sec: &ConSection) -> Vec<u8> {
    let t = script.text();
    let s = sec.cond_offset.unwrap() as usize;
    let mut e = s;
    while e < t.len() && t[e] != b'\r' && t[e] != b'\n' {
        e += 1;
    }
    t[s..e].to_vec()
}

fn gotos(script: &ConScript, body: u32) -> Option<&ConSection> {
    script.sections().iter().find(|s| s.body_offset == body)
}

// ---------------------------------------------------------------------------
// The parse.
// ---------------------------------------------------------------------------

/// Synthetic CON files, expected from the original interpreter.
#[test]
fn synthetic_parse_matches_original() {
    const CASES: [(&str, &str, &str); 9] = [
        ("two_con_blocks",
         "/1\r\n/con(ClicPerso=600)\r\n/suj21\r\n[Seth] A\r\n/go5\r\n/con(ClicPerso=600)\r\n/suj2\r\n/go6\r\n/5\r\n/sel(Vue=1)\r\n[Seth] B\r\n/6\r\n/sel(Vue=2)\r\n[Seth] C\r\n/000\r\n",
         "S 1 con\nDLG Seth||A\nGO 5\nS 1 con\nGO 6\nS 5 sel\nDLG Seth||B\nS 6 sel\nDLG Seth||C\n"),
        ("go_mid_section",
         "/1\r\n/sel(Vue=1)\r\n[Seth] A\r\n/go2\r\n[Seth] after go\r\n/set(FlagTemp1=1)\r\n/2\r\n/sel(Vue=2)\r\n[Seth] B\r\n/000\r\n",
         "S 1 sel\nDLG Seth||A\nGO 2\nS 2 sel\nDLG Seth||B\n"),
        ("header_forms",
         "/3 \r\n/sel(Vue=1)\r\n[Seth] A\r\n/014\r\n/con*(ClicPerso=600)\r\n[Seth] B\r\n/000\r\n",
         "S 3 sel\nDLG Seth||A\nS 14 con*\nDLG Seth||B\n"),
        ("directive_case",
         "/1\r\n/SEL(Vue=1)\r\n[Seth] A\r\n/2\r\n/Sel(Vue=2)\r\n/SET(FlagTemp1=1)\r\n/Set(FlagTemp2=1)\r\n/sEt(FlagTemp3=1)\r\n/3\r\n/CON(ClicPerso=600)\r\n[Seth] C\r\n/4\r\n/TIM1(Vue=4)\r\n[Seth] D\r\n/000\r\n",
         "S 1 none\nDLG Seth||A\nS 2 sel\nSET FlagTemp2=1\nS 3 none\nDLG Seth||C\nS 4 none\nDLG Seth||D\n"),
        ("comments_and_text",
         "/1\r\n/sel(Vue=1)\r\n# comment /set(FlagTemp1=9)\r\n[Seth] A /set(FlagTemp2=1) inline\r\n[Seth]no space\r\n[Seth,] empty params\r\n/set(FlagTemp3=1)  \r\n/000\r\n",
         "S 1 sel\nSET FlagTemp2=1) inline\nDLG Seth||no space\nDLG Seth||empty params\nSET FlagTemp3=1\n"),
        ("init_block",
         "/INIT\r\n/set(HidePerso=1)\r\n# note\r\n/set(HidePerso=2)\r\n/FININIT\r\n/1\r\n/sel(Vue=1)\r\n/set(vue=2)\r\n/000\r\n",
         "I HidePerso=1\nS 1 sel\nSET vue=2\n"),
        ("timers",
         "/1\r\n/tim1(Time1>3)\r\n[Seth] A\r\n/2\r\n/Tim2(Time2>3)\r\n[Seth] B\r\n/3\r\n/tim3(Time1>3)\r\n[Seth] C\r\n/000\r\n",
         "S 1 tim1\nDLG Seth||A\nS 2 tim2\nDLG Seth||B\nS 3 none\nDLG Seth||C\n"),
        ("con_counts",
         "/1\r\n/con2(ClicPerso=600)\r\n[Seth] A\r\n/2\r\n/con*(ClicPerso=600)\r\n[Seth] B\r\n/000\r\n",
         "S 1 con\nDLG Seth||A\nS 2 con*\nDLG Seth||B\n"),
        ("sel_twice",
         "/10\r\n/sel(Vue=57)\r\n/set(time2=0)\r\n/sel(stopsound=1)\r\n/set(startmusik=1)\r\n/000\r\n",
         "S 10 sel\nSET time2=0\nSET startmusik=1\nS 10 sel\nSET startmusik=1\n"),
    ];
    for (name, text, expected) in CASES {
        let script = ConScript::parse(text.as_bytes());
        assert_eq!(
            String::from_utf8_lossy(&canonical_dump(&script)),
            expected,
            "{name}"
        );
    }
    // The angles of a dialog header and the /suj lists.
    let script = ConScript::parse(
        b"/1\r\n/con(ClicPerso=600)\r\n/suj1,2\r\n[Seth, 12, -3, cam2] x\r\n/000\r\n",
    );
    let sec = &script.sections()[0];
    assert_eq!(sec.suj_ids, vec![1, 2]);
    assert_eq!(sec.clic_perso, 600);
    match &sec.items[0] {
        ConItem::Line(l) => assert_eq!(l.angle, Some((12, -3))),
        other => panic!("{other:?}"),
    }
}

/// /con play limits: the entries the original /con scanner finds with
/// every play counter at 0, 1, 2 and 255.
#[test]
fn con_play_limits_match_original() {
    let text = "/1\r\n/con(ClicPerso=600)\r\n[Seth] A\r\n/2\r\n/con2(ClicPerso=600)\r\n[Seth] B\r\n\
                /3\r\n/con*(ClicPerso=600)\r\n[Seth] C\r\n/4\r\n/con300(ClicPerso=600)\r\n[Seth] D\r\n/000\r\n";
    let script = ConScript::parse(text.as_bytes());
    for (counter, ids) in [(0u8, "1 2 3 4"), (1, "2 3 4"), (2, "3 4"), (255, "3 4")] {
        let counters = vec![counter; CON_PLAY_COUNTERS];
        let got: Vec<String> = script
            .sections()
            .iter()
            .filter(|s| s.is_entry && s.is_con && con_entry_allowed(s, &counters))
            .map(|s| s.id.to_string())
            .collect();
        assert_eq!(got.join(" "), ids, "counter {counter}");
    }
}

/// Zone click / object in hand: the section the original `FUN_00425cf4`
/// runs (its evaluator and /con scanner under emulation); 0 = none.
#[test]
fn con_click_entry_matches_original() {
    let Some(g) = game("con_click_entry_matches_original") else {
        return;
    };
    let names = varias(&g);
    let text = "/1\r\n/con(ClicPerso=600)\r\n[Seth] npc\r\n\
                /2\r\n/con*(ClicZone=12)\r\n/suj3\r\n[Seth] subject\r\n\
                /3\r\n/con*(ClicZone=12)\r\n[Seth] zone 12\r\n\
                /4\r\n/con(ClicZone=12)&(FlagTemp1=1)\r\n[Seth] zone 12 flag\r\n\
                /5\r\n/con*(ObjetEnMain=300)\r\n[Seth] object\r\n/000\r\n";
    let script = ConScript::parse(text.as_bytes());
    let cases = [
        ("zone12", "cliczone", 12, None, 0u8, 3),
        ("zone12_flag", "cliczone", 12, Some(("flagtemp1", 1)), 0, 3),
        ("zone13", "cliczone", 13, None, 0, 0),
        ("object300", "objetenmain", 300, None, 0, 5),
        ("npc600", "clicperso", 600, None, 0, 1),
        ("npc600_played", "clicperso", 600, None, 1, 0),
    ];
    for (name, var, val, var2, counter, section) in cases {
        let mut st = VarState::default();
        st.vars.insert(var.into(), val);
        if let Some((k, v)) = var2 {
            st.vars.insert(k.into(), v);
        }
        let counters = vec![counter; CON_PLAY_COUNTERS];
        let idx = script.find_con_click_entry(&counters, |sec| {
            eval_at(&names, script.text(), sec.cond_offset.unwrap(), &st, None)
        });
        assert_eq!(
            idx.map_or(0, |i| script.sections()[i].id),
            section,
            "{name}"
        );
    }
}

/// Every SCENAR\CHAPI*.CON of the four discs against the original
/// interpreter's canonical dump.
#[test]
fn parse_matches_original_interpreter() {
    let Some(g) = game("parse_matches_original_interpreter") else {
        return;
    };
    let golden = golden_map(include_str!("golden/scripts_con_parse.txt"));
    let files = con_files(&g, "SCENAR\\CHAPI");
    assert_eq!(files.len(), golden.len());
    let mut bad = Vec::new();
    for f in &files {
        let script = ConScript::parse(&read(&g, f));
        let key = &f[7..];
        if golden.get(key).copied() != Some(md5_hex(&canonical_dump(&script)).as_str()) {
            bad.push(key.to_string());
        }
    }
    assert!(bad.is_empty(), "{} files differ: {bad:?}", bad.len());
    eprintln!("scripts_con_parse: {} files", files.len());
}

/// The walk of an entry point as the section executor runs it: per dialog
/// line "<section><letter>", the walk ending at a gameover=N != 0 or a
/// chapter=, one /go after another; `escape_after`: Escape after that many
/// played lines (the lines from there are listed).
fn walk(script: &ConScript, start: &ConSection, escape_after: Option<usize>) -> (String, String) {
    let mut got = String::new();
    let mut narrator = String::new();
    let mut cur = start.clone();
    let mut played = 0;
    for _ in 0..64 {
        let mut line_idx = 0;
        let mut stop = false;
        let mut k = 0;
        while k < cur.items.len() && !stop {
            match &cur.items[k] {
                ConItem::Line(_) => {
                    if escape_after.is_none_or(|j| played >= j) {
                        got += &format!(" {}{}", cur.id, con_track_letter(line_idx));
                    }
                    line_idx += 1;
                    played += 1;
                    if Some(played) == escape_after {
                        k = con_escape_skip(&cur.items, k + 1, &mut line_idx);
                        continue;
                    }
                }
                ConItem::Set { cmd, .. } => {
                    let low = cmd.to_ascii_lowercase();
                    if low.starts_with("chapter=") {
                        stop = true;
                    } else if low.starts_with("gameover=")
                        && ConText(&low.as_bytes()[9..]).atoi(0) != 0
                    {
                        if let Some(ConItem::Line(l)) = cur.items.get(k + 1) {
                            if l.speaker.starts_with("Narra") {
                                narrator = format!(
                                    "WAV\\ATA{:04}{}.APC",
                                    cur.id,
                                    con_track_letter(line_idx)
                                );
                            }
                        }
                        stop = true;
                    }
                }
            }
            k += 1;
        }
        if stop || cur.goto_id <= 0 {
            break;
        }
        match script.goto_section(cur.goto_id) {
            Some(t) => cur = t,
            None => break,
        }
    }
    (got, narrator)
}

/// "<offset>:<section><letter>" tokens after the ':' as " SECTIONLETTER".
fn want_tokens(l: &str) -> String {
    let rest = l.split_once(':').map_or("", |(_, r)| r);
    rest.split_whitespace()
        .map(|tok| {
            let t = tok.split_once(':').map_or(tok, |(_, t)| t);
            format!(" {}", t.to_ascii_uppercase())
        })
        .collect()
}

/// The track letter of every dialog line and game-over narrator, against
/// `FUN_0042844c` / `FUN_00426590` run under emulation on every entry point
/// of the CHAPInnn.CON files.
#[test]
fn track_letters_match_original() {
    let Some(g) = game("track_letters_match_original") else {
        return;
    };
    let mut scripts: HashMap<String, ConScript> = HashMap::new();
    let (mut walks, mut dlg, mut narr, mut bad) = (0, 0, 0, 0);
    for l in lines(include_str!("golden/video_dialog_letters.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        let (con, body): (&str, u32) = (t[1], t[2].parse().unwrap());
        let script = scripts
            .entry(con.to_string())
            .or_insert_with(|| ConScript::parse(&read(&g, &format!("SCENAR\\{con}.CON"))));
        let Some(start) = gotos(script, body) else {
            bad += 1;
            continue;
        };
        let (got, narrator) = walk(script, start, None);
        if t[0] == "W" {
            walks += 1;
            let want = want_tokens(l);
            dlg += want.split_whitespace().count();
            if got != want {
                bad += 1;
                eprintln!("{con} body {body}: port{got}, original{want}");
            }
        } else {
            narr += 1;
            let want = l.split_once(" narrator ").map_or(String::new(), |(_, r)| {
                r.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
            });
            if want != narrator {
                bad += 1;
                eprintln!("{con} body {body}: port narrator '{narrator}', original '{want}'");
            }
        }
    }
    eprintln!("track letters: {walks} walks, {dlg} lines, {narr} narrators");
    assert_eq!((walks, dlg, narr, bad), (1197, 2797, 364, 0));
}

/// Escape during a talking-head line, after every played line of every
/// walk: the lines played after it, against `FUN_0042844c` run under
/// emulation with 0x00496134 set after line j.
#[test]
fn escape_skip_matches_original() {
    let Some(g) = game("escape_skip_matches_original") else {
        return;
    };
    let mut scripts: HashMap<String, ConScript> = HashMap::new();
    let (mut rows, mut skipping, mut bad) = (0, 0, 0);
    for l in lines(include_str!("golden/video_dialog_escape.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        let (con, body): (&str, u32) = (t[1], t[2].parse().unwrap());
        let esc: usize = t[6].parse().unwrap();
        let skipped: usize = t[8].trim_end_matches(':').parse().unwrap();
        rows += 1;
        if skipped != 0 {
            skipping += 1;
        }
        let script = scripts
            .entry(con.to_string())
            .or_insert_with(|| ConScript::parse(&read(&g, &format!("SCENAR\\{con}.CON"))));
        let Some(start) = gotos(script, body) else {
            bad += 1;
            continue;
        };
        let (got, _) = walk(script, start, Some(esc));
        let want = want_tokens(l);
        if got != want {
            bad += 1;
            if bad <= 10 {
                eprintln!("{con} body {body} esc {esc}: port{got}, original{want}");
            }
        }
    }
    eprintln!("escape skip: {rows} rows, {skipping} skipping");
    assert_eq!((rows, skipping, bad), (2797, 1546, 0));
}

// ---------------------------------------------------------------------------
// The evaluator, the /set parser, the clocks.
// ---------------------------------------------------------------------------

/// Synthetic condition lines, expected from the original evaluator.
#[test]
fn synthetic_conditions_match_original() {
    let Some(g) = game("synthetic_conditions_match_original") else {
        return;
    };
    let names = varias(&g);
    type Case = (
        &'static str,
        &'static [(&'static str, i32)],
        &'static [i32],
        bool,
    );
    const CASES: [Case; 51] = [
        ("/sel(Vue=3)", &[("VUE", 3)], &[], true),
        ("/sel(Vue=3)", &[("VUE", 4)], &[], false),
        ("/sel(Vue<3)", &[("VUE", 2)], &[], true),
        ("/sel(Vue<3)", &[("VUE", 3)], &[], false),
        ("/sel(Vue>3)", &[("VUE", 4)], &[], true),
        ("/sel(Vue>3)", &[("VUE", 3)], &[], false),
        ("/sel(Vue!3)", &[("VUE", 4)], &[], true),
        ("/sel(Vue!3)", &[("VUE", 3)], &[], false),
        ("/sel(AngleWarpX<45)", &[("ANGLEWARPX", -1)], &[], false),
        ("/sel(AngleWarpX>315)", &[("ANGLEWARPX", -1)], &[], true),
        ("/sel(ParleTemp1>=20)", &[("PARLETEMP1", 5)], &[], false),
        ("/sel(ParleTemp1>=20)", &[("PARLETEMP1", 21)], &[], true),
        ("/sel(ParleTemp1>=20)", &[("PARLETEMP1", 20)], &[], false),
        ("/sel(FlagTemp1= 2)", &[("FLAGTEMP1", 2)], &[], true),
        ("/sel(FlagTemp1=-1)", &[("FLAGTEMP1", 1)], &[], true),
        ("/sel(FlagTemp1=x)", &[("FLAGTEMP1", 0)], &[], false),
        ("/sel(vUe=7)", &[("VUE", 7)], &[], true),
        (
            "/sel(Arrivee>0)&(Vue=10)",
            &[("ARRIVEEVUE", 9), ("VUE", 10)],
            &[],
            true,
        ),
        ("/sel(FlagTemp=1)", &[("FLAGTEMP1", 1)], &[], true),
        ("/sel(Foo=0)", &[], &[], false),
        ("/sel(Vue=1)|(Foo=0)", &[("VUE", 1)], &[], true),
        ("/sel(Vue=2)|(Foo=0)", &[("VUE", 1)], &[], false),
        ("/sel(Vue =1)", &[("VUE", 1)], &[], false),
        (
            "/sel(Vue=1)&(FlagTemp1=1)",
            &[("VUE", 1), ("FLAGTEMP1", 1)],
            &[],
            true,
        ),
        (
            "/sel(Vue=1)&(FlagTemp1=1)",
            &[("VUE", 1), ("FLAGTEMP1", 0)],
            &[],
            false,
        ),
        ("/sel(Vue=1)|(Vue=2)", &[("VUE", 2)], &[], true),
        ("/sel(Vue=1)|(Vue=2)", &[("VUE", 3)], &[], false),
        (
            "/con(ClicPerso=578)|(ClicPerso=532)&(ParleTemp1=0)",
            &[("CLICPERSO", 578), ("PARLETEMP1", 0)],
            &[],
            true,
        ),
        (
            "/con(ClicPerso=578)|(ClicPerso=532)&(ParleTemp1=0)",
            &[("CLICPERSO", 578), ("PARLETEMP1", 1)],
            &[],
            false,
        ),
        (
            "/con(ClicPerso=578)|(ClicPerso=532)&(ParleTemp1=0)",
            &[("CLICPERSO", 532), ("PARLETEMP1", 1)],
            &[],
            false,
        ),
        (
            "/con(ClicPerso=578)|(ClicPerso=532)&(ParleTemp1=0)",
            &[("CLICPERSO", 1), ("PARLETEMP1", 0)],
            &[],
            false,
        ),
        (
            "/sel(FlagTemp1=0)&(Vue=1)|(Vue=2)&(FlagTemp2=1)",
            &[("FLAGTEMP1", 0), ("VUE", 1), ("FLAGTEMP2", 0)],
            &[],
            false,
        ),
        (
            "/sel(FlagTemp1=0)&(Vue=1)|(Vue=2)&(FlagTemp2=1)",
            &[("FLAGTEMP1", 0), ("VUE", 2), ("FLAGTEMP2", 1)],
            &[],
            true,
        ),
        (
            "/sel(FlagTemp1=0)&(Vue=1)|(Vue=2)&(FlagTemp2=1)",
            &[("FLAGTEMP1", 1), ("VUE", 1), ("FLAGTEMP2", 1)],
            &[],
            false,
        ),
        (
            "/tim1(anglewarpX>200)|(time1>6)&(vue=238)|(vue=239)",
            &[("ANGLEWARPX", 250), ("TIME1", 0), ("VUE", 1)],
            &[],
            false,
        ),
        (
            "/tim1(anglewarpX>200)|(time1>6)&(vue=238)|(vue=239)",
            &[("ANGLEWARPX", 0), ("TIME1", 7), ("VUE", 239)],
            &[],
            true,
        ),
        (
            "/tim1(anglewarpX>200)|(time1>6)&(vue=238)|(vue=239)",
            &[("ANGLEWARPX", 0), ("TIME1", 0), ("VUE", 238)],
            &[],
            false,
        ),
        (
            "/sel(Vue=1)(FlagTemp1=1)",
            &[("VUE", 1), ("FLAGTEMP1", 0)],
            &[],
            false,
        ),
        (
            "/sel(Vue=1)(FlagTemp1=1)",
            &[("VUE", 2), ("FLAGTEMP1", 1)],
            &[],
            false,
        ),
        (
            "/con*ClicPerso=598)|(ClicPerso=571)",
            &[("CLICPERSO", 598)],
            &[],
            false,
        ),
        (
            "/con*ClicPerso=598)|(ClicPerso=571)",
            &[("CLICPERSO", 571)],
            &[],
            true,
        ),
        ("/sel", &[], &[], true),
        ("/sel()", &[], &[], false),
        ("/sel(invent271=1)", &[], &[271], true),
        ("/sel(invent271=1)", &[], &[], false),
        ("/sel(invent269=0)", &[], &[], true),
        ("/sel(invent269=0)", &[], &[269], false),
        ("/sel(invent269>0)", &[], &[269], false),
        ("/sel(invent269>0)", &[], &[], true),
        (
            "/sel(Vue=1)&(invent271=1)|(Vue=2)",
            &[("VUE", 1)],
            &[],
            false,
        ),
        ("/sel(invent271=1)|(Vue=2)", &[("VUE", 2)], &[], true),
    ];
    for (i, (text, vars, inv, expected)) in CASES.iter().enumerate() {
        let st = VarState {
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), *v))
                .collect(),
            inventory: inv.to_vec(),
        };
        assert_eq!(
            port_eval(&names, text.as_bytes(), &st),
            *expected,
            "case {i} '{text}'"
        );
    }
}

/// Time1= / Time2= and the per-frame clock update `FUN_0041c75c` against
/// the original code under emulation (the clock stubbed).
#[test]
fn clocks_match_original() {
    let restart = [
        (100_000, "Time1=0", 100_000, 0, 57005, 57005),
        (100_000, "Time1=30", 70000, 0, 57005, 57005),
        (100_000, "Time1=200", 100_000, 200, 57005, 57005),
        (100_000, "Time1=100", 100_000, 100, 57005, 57005),
        (100_000, "Time2=7", 57005, 57005, 93000, 0),
        (5000, "Time2=5", 57005, 57005, 5000, 5),
        (5000, "Time2=4", 57005, 57005, 1000, 0),
    ];
    for (now, cmd, t1s, t1o, t2s, t2o) in restart {
        let mut c = ConClocks {
            time1_start: 0xdead,
            time2_start: 0xdead,
            time1_offset: 0xdead,
            time2_offset: 0xdead,
            ..ConClocks::default()
        };
        let timer = i32::from(cmd.as_bytes()[4] - b'0');
        c.restart(timer, cmd[6..].parse().unwrap(), now);
        assert_eq!(
            (c.time1_start, c.time1_offset, c.time2_start, c.time2_offset),
            (t1s, t1o, t2s, t2o),
            "{cmd}"
        );
    }
    // now, chapter start, time1 start / offset, time2 start / offset ->
    // Random, ChapterTime, Time1, Time2.
    type Update = (u32, u32, u32, i32, u32, i32, i32, i32, i32, i32);
    let update: [Update; 4] = [
        (100_000, 40000, 99000, 5, 100_000, 0, 160, 60, 6, 0),
        (123_456, 0, 3456, 0, 23999, 12, 64, 123, 120, 111),
        (
            100_000, 100_500, 100_500, 3, 90000, 0, 160, 4_294_966, 4_294_969, 10,
        ),
        (74565, 74565, 73728, 0, 0, 0, 69, 0, 0, 74),
    ];
    for (now, chap, t1s, t1o, t2s, t2o, random, ct, time1, time2) in update {
        let c = ConClocks {
            chapter_start: chap,
            time1_start: t1s,
            time1_offset: t1o,
            time2_start: t2s,
            time2_offset: t2o,
        };
        assert_eq!((now & 0xff) as i32, random);
        assert_eq!(c.chapter_time(now), ct);
        assert_eq!(c.timer_value(1, now), time1);
        assert_eq!(c.timer_value(2, now), time2);
    }
}

/// /set parsing against `FUN_0042a100` under emulation (each line followed
/// by "\r\n/1\r\n").
#[test]
fn set_parse_matches_original() {
    let Some(g) = game("set_parse_matches_original") else {
        return;
    };
    let names = varias(&g);
    let cases = [
        ("/set(FlagTemp1=5)", true, "flagtemp1", 5, 0),
        ("/set(FlagTemp1=-5)", true, "flagtemp1", 5, 0),
        ("/set(FlagTemp1=abc)", true, "flagtemp1", 1, 0), // the 1 of the next line
        ("/set(parletemp=1)", true, "parletemp1", 1, 0),
        ("/set(FlagTemp=1)", true, "flagtemp1", 1, 0),
        ("/set(Foo=1)", false, "", 0, 0),
        ("/set(abortdepart=1)", true, "abortdepart", 1, 0),
        ("/set(newsound=14)", true, "newsound", 14, 0),
        ("/set(startpuzzle=2)", true, "startpuzzle", 2, 0),
        ("/set(Disable=7)", true, "disable", 7, 0),
        ("/set(TestTime1=1)", true, "testtime1", 1, 0),
        ("/set(gameover=1)", true, "gameover", 1, 0),
        ("/set(invent262=1)", true, "", 1, 262),
        ("/set(invent262=0)", true, "", 0, 262),
        ("/set(invent262=2)", true, "", 2, 262),
        ("/set(changewam=atlan4.wam)", true, "changewam", 0, 0),
    ];
    for (line, valid, name, value, invent) in cases {
        let text = format!("{line}\r\n/1\r\n");
        let set = parse_con_set(&names, text.as_bytes(), 0);
        assert_eq!(set.is_some(), valid, "{line}");
        let Some(set) = set else { continue };
        if invent != 0 {
            assert_eq!(set.invent, Some((invent, value)), "{line}");
        } else if let Some(wam) = &set.wam_name {
            assert_eq!(
                (set.name.as_str(), wam.as_str()),
                (name, "atlan4.wam"),
                "{line}"
            );
        } else {
            assert_eq!((set.name.as_str(), set.value), (name, value), "{line}");
        }
    }
}

/// AngleWarpX= / AngleWarpY=: the angle the original /set handler stores,
/// as float bits.
#[test]
fn set_view_angle_matches_original() {
    for (value, bits) in [
        (90, 0x3fc9_0fdbu32),
        (0, 0),
        (359, 0x40c8_80e1),
        (45, 0x3f49_0fdb),
        (20, 0x3eb2_b8c3),
        (30, 0x3f06_0a92), // "-30" reads 30
    ] {
        assert_eq!(con_angle_radians(value).to_bits(), bits, "{value}");
    }
}

/// Every condition line of every CHAPI*.CON under 64 deterministic
/// variable states: the digest of the result bits per file.
#[test]
fn conditions_match_original() {
    let Some(g) = game("conditions_match_original") else {
        return;
    };
    let names = varias(&g);
    let golden = golden_map(include_str!("golden/scripts_con_eval.txt"));
    let var_names = varias_lower(&g);
    assert_eq!(var_names.len(), 77); // VARIAS.CON has 77 names
    let files = con_files(&g, "SCENAR\\CHAPI");
    assert_eq!(files.len(), golden.len());
    let mut bad = Vec::new();
    let mut evaluations = 0;
    for f in &files {
        let data = read(&g, f);
        let mut bits = Vec::new();
        for (i, line) in cond_lines(&data).iter().enumerate() {
            for k in 0..64 {
                let st = make_state(i, k, &var_names, line);
                bits.push(if port_eval(&names, line, &st) {
                    b'1'
                } else {
                    b'0'
                });
            }
        }
        evaluations += bits.len();
        if golden.get(&f[7..]).copied() != Some(md5_hex(&bits).as_str()) {
            bad.push(f[7..].to_string());
        }
    }
    eprintln!(
        "scripts_con_eval: {} files, {evaluations} evaluations",
        files.len()
    );
    assert!(bad.is_empty(), "{} files differ: {bad:?}", bad.len());
}

/// One scan keeps `FUN_004298ac`'s key buffer across its evaluations, so an
/// INVENT key can read the digit a PARLETEMPn key left after it (CHAPI063's
/// /tim2 scan). The original scanners with the real evaluator, the buffer
/// zeroed at the start of each scan, for the INVENT lines x 64 states; and
/// with the buffer cleared at every evaluation (the restored content).
#[test]
fn scan_key_buffer_matches_original() {
    let Some(g) = game("scan_key_buffer_matches_original") else {
        return;
    };
    let names = varias(&g);
    let var_names = varias_lower(&g);
    let counters = vec![0u8; CON_PLAY_COUNTERS];
    let mut cur = String::new();
    let (mut data, mut conds, mut script) = (Vec::new(), Vec::new(), ConScript::default());
    let (mut cases, mut differs, mut bad) = (0, 0, 0);
    for l in lines(include_str!("golden/scripts_scan_keystack.txt")) {
        if l.starts_with("cases") {
            continue;
        }
        let t: Vec<&str> = l.split_whitespace().collect();
        let (file, line_idx, kind) = (t[0], t[1].parse::<usize>().unwrap(), t[2]);
        let k: usize = t[3].trim_end_matches(':').parse().unwrap();
        let expected: i32 = t[4].parse().unwrap();
        let reset: i32 = t[6].parse().unwrap();
        if cur != file {
            cur = file.to_string();
            data = read(&g, &format!("SCENAR\\{file}"));
            conds = cond_lines(&data);
            script = ConScript::parse(&data);
        }
        let st = make_state(line_idx, k, &var_names, &conds[line_idx]);
        for mode in 0..2 {
            let mut key = [0u8; 16];
            let mut test = |sec: &ConSection| {
                let key = (mode == 0).then_some(&mut key);
                eval_at(&names, script.text(), sec.cond_offset.unwrap(), &st, key)
            };
            let idx = match kind {
                "sel" => script.find_sel_entry(&mut test),
                // FUN_004281f8 itself: /Suj bodies included.
                "con" => script.sections().iter().position(|s| {
                    s.is_entry && s.is_con && con_entry_allowed(s, &counters) && test(s)
                }),
                _ => {
                    let timer = if kind == "tim1" { 1 } else { 2 };
                    script
                        .sections()
                        .iter()
                        .position(|s| s.is_entry && s.is_timer && s.timer_num == timer && test(s))
                }
            };
            // The body starts on the line after the condition line.
            let got = idx.map_or(-1, |i| {
                let off = script.sections()[i].cond_offset.unwrap() as usize;
                1 + data[..off].windows(2).filter(|w| w == b"\r\n").count() as i32
            });
            let want = if mode == 0 { expected } else { reset };
            if mode == 0 {
                cases += 1;
                if expected != reset {
                    differs += 1;
                }
            }
            if got != want {
                bad += 1;
                if bad <= 10 {
                    eprintln!("{file} line {line_idx} {kind} state {k} mode {mode}: port {got}, original {want}");
                }
            }
        }
    }
    assert_eq!((cases, differs, bad), (4096, 64, 0));
}

/// The place loop's first arrival scan after a chapter load, for the 74
/// original checkpoints and the new game: ArriveeVue and the /sel section
/// fired.
#[test]
fn arrival_after_load_matches_original() {
    let Some(g) = game("arrival_after_load_matches_original") else {
        return;
    };
    let names = varias(&g);
    let text = include_str!("golden/scripts_arrival_after_load.txt");
    let mut vars_line = HashMap::new();
    let mut inv_line = HashMap::new();
    let mut ck = Vec::new();
    for l in lines(text) {
        let (tag, rest) = l.split_once(' ').unwrap();
        let (key, val) = rest.split_once(' ').unwrap_or((rest, ""));
        match tag {
            "CK" => ck.push(l),
            "VARS" => {
                vars_line.insert(key, val);
            }
            "INV" => {
                inv_line.insert(key, val);
            }
            _ => {}
        }
    }
    assert_eq!(ck.len(), 75);
    let pairs = |s: &str| -> Vec<(i32, i32)> {
        s.split_whitespace()
            .filter_map(|p| p.split_once(':'))
            .map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap()))
            .collect()
    };
    let mut bad = 0;
    for l in ck {
        // CK <name> chapter C vue V lastvue L arriveevue A sel S ...
        let t: Vec<&str> = l.split_whitespace().collect();
        let name = t[1];
        let chapter: u32 = t[3].parse().unwrap();
        let last_vue: u16 = t[7].parse().unwrap();
        let arrivee_orig: i32 = t[9].parse().unwrap();
        let expected = if t[11].starts_with('-') {
            -1
        } else {
            t[11].parse().unwrap()
        };
        let arrivee = arrival_after_chapter_load(last_vue);
        if arrivee.unwrap_or(0) != arrivee_orig {
            bad += 1;
        }
        let mut got = -1;
        if let Some(arrivee) = arrivee {
            let script = ConScript::parse(&read(&g, &format!("SCENAR\\CHAPI{chapter:03}.CON")));
            let mut st = VarState::default();
            for (idx, v) in pairs(vars_line[name]) {
                st.vars.insert(names.name(idx as usize).to_string(), v);
            }
            st.vars.insert("arriveevue".into(), arrivee);
            for (id, v) in pairs(inv_line.get(name).copied().unwrap_or("")) {
                assert_eq!(v, 1);
                st.inventory.push(id);
            }
            let idx =
                script.find_sel_entry(|sec| port_eval(&names, &entry_line(&script, sec), &st));
            got = idx.map_or(-1, |i| script.sections()[i].id);
        }
        if got != expected {
            bad += 1;
            eprintln!("{name}: port /sel {got}, original {expected}");
        }
    }
    assert_eq!(bad, 0);
}

/// ArriveeVue after a move into a sub-WAM: 0x00423def stores the place
/// left. The evaluator given the place left must give the original's bits;
/// given the destination, the other column.
#[test]
fn subwam_arrival_matches_original() {
    let Some(g) = game("subwam_arrival_matches_original") else {
        return;
    };
    let names = varias(&g);
    let var_names = varias_lower(&g);
    let (mut rows, mut bad) = (0, 0);
    for l in lines(include_str!("golden/scripts_subwam_arrival.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        let chapter: u32 = t[2].parse().unwrap();
        let (src, dst) = t[4].split_once("->").unwrap();
        let (src, dst): (i32, i32) = (src.parse().unwrap(), dst.parse().unwrap());
        let line_idx: usize = t[6].parse().unwrap();
        let (orig, old) = (&t[7][5..], &t[8][5..]);
        let conds = cond_lines(&read(&g, &format!("SCENAR\\CHAPI{chapter:03}.CON")));
        let (mut fixed, mut prev) = (String::new(), String::new());
        for k in 0..64 {
            let mut st = make_state(line_idx, k, &var_names, &conds[line_idx]);
            st.vars.insert("vue".into(), dst);
            st.vars.insert("arriveevue".into(), src);
            fixed.push(if port_eval(&names, &conds[line_idx], &st) {
                '1'
            } else {
                '0'
            });
            st.vars.insert("arriveevue".into(), dst);
            prev.push(if port_eval(&names, &conds[line_idx], &st) {
                '1'
            } else {
                '0'
            });
        }
        rows += 1;
        if fixed != orig || prev != old {
            bad += 1;
            eprintln!("sub-WAM arrival: {l}");
        }
    }
    assert_eq!((rows, bad), (17, 0));
}

/// The sound of the moves whose video has a same-name APC: the departure
/// scan with VUE = source, DEPARTVUE = CLICZONE = destination over 64
/// states, and the NEWSOUND values the section found sets (the section's
/// /set items and one /go).
#[test]
fn transition_sound_matches_original() {
    let Some(g) = game("transition_sound_matches_original") else {
        return;
    };
    let names = varias(&g);
    let var_names = varias_lower(&g);
    let (mut rows, mut states, mut bad) = (0, 0, 0);
    let mut cache: HashMap<u32, (ConScript, Vec<Vec<u8>>)> = HashMap::new();
    for l in lines(include_str!("golden/scripts_transition_sound.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        let chapter: u32 = t[2].parse().unwrap();
        let (src, dst) = t[4].split_once("->").unwrap();
        let (src, dst): (i32, i32) = (src.parse().unwrap(), dst.parse().unwrap());
        let reference: usize = t[8].trim_end_matches(':').parse().unwrap();
        rows += 1;
        let (script, conds) = cache.entry(chapter).or_insert_with(|| {
            let data = read(&g, &format!("SCENAR\\CHAPI{chapter:03}.CON"));
            (ConScript::parse(&data), cond_lines(&data))
        });
        let want: Vec<&str> = l.split_once('|').unwrap().1.split_whitespace().collect();
        assert_eq!(want.len(), 64);
        for (k, w) in want.iter().enumerate() {
            let mut st = if k == 0 {
                VarState {
                    vars: var_names.iter().map(|v| (v.clone(), 0)).collect(),
                    inventory: Vec::new(),
                }
            } else {
                make_state(reference, k, &var_names, &conds[reference])
            };
            st.vars.insert("vue".into(), src);
            st.vars.insert("departvue".into(), dst);
            st.vars.insert("cliczone".into(), dst);
            st.vars.insert("arriveevue".into(), 0);
            let idx = script.find_sel_entry(|sec| port_eval(&names, &entry_line(script, sec), &st));
            let got = match idx {
                None => "-:-".to_string(),
                Some(i) => {
                    let mut sec = script.sections()[i].clone();
                    let id = sec.id;
                    let mut snd = Vec::new();
                    for hop in 0..2 {
                        for it in &sec.items {
                            if let ConItem::Set { cmd, .. } = it {
                                if cmd.len() >= 9 && cmd[..9].eq_ignore_ascii_case("newsound=") {
                                    snd.push(ConText(&cmd.as_bytes()[9..]).atoi(0).to_string());
                                }
                            }
                        }
                        if hop == 0 && sec.goto_id > 0 {
                            match script.goto_section(sec.goto_id) {
                                Some(t) => sec = t,
                                None => break,
                            }
                        } else {
                            break;
                        }
                    }
                    let snd = if snd.is_empty() {
                        "-".to_string()
                    } else {
                        snd.join(",")
                    };
                    format!("/{id}:{snd}")
                }
            };
            states += 1;
            if got != *w {
                bad += 1;
                if bad < 10 {
                    eprintln!(
                        "chapter {chapter} move {src}->{dst} state {k}: port {got}, original {w}"
                    );
                }
            }
        }
    }
    eprintln!("transition sound: {rows} moves, {states} states");
    assert!(rows > 0);
    assert_eq!(bad, 0);
}

/// DEPARTVUE after a script Vue=N: `FUN_00421cfc` stores N and nothing
/// clears it before a move ends, so the scans that follow see N.
#[test]
fn departvue_after_vue_matches_original() {
    let Some(g) = game("departvue_after_vue_matches_original") else {
        return;
    };
    let names = varias(&g);
    let var_names = varias_lower(&g);
    let (mut rows, mut bad) = (0, 0);
    let mut cache: HashMap<u32, Vec<Vec<u8>>> = HashMap::new();
    for l in lines(include_str!("golden/scripts_departvue_after_vue.txt")) {
        // D chapter C site S kind K Vue=N from F line L orig= port=
        let t: Vec<&str> = l.split_whitespace().collect();
        let chapter: u32 = t[2].parse().unwrap();
        let vue: i32 = t[7][4..].parse().unwrap();
        let src: i32 = t[9].parse().unwrap();
        let line_idx: usize = t[11].parse().unwrap();
        let (orig, old) = (&t[12][5..], &t[13][5..]);
        let conds = cache
            .entry(chapter)
            .or_insert_with(|| cond_lines(&read(&g, &format!("SCENAR\\CHAPI{chapter:03}.CON"))));
        let (mut fixed, mut prev) = (String::new(), String::new());
        for k in 0..64 {
            let mut st = make_state(line_idx, k, &var_names, &conds[line_idx]);
            st.vars.insert("vue".into(), vue);
            st.vars.insert("arriveevue".into(), src);
            let mut v = atlantis_engine::con::Vars::default();
            v.place_looked_up(vue);
            st.vars.insert("departvue".into(), v.get("departvue"));
            fixed.push(if port_eval(&names, &conds[line_idx], &st) {
                '1'
            } else {
                '0'
            });
            st.vars.insert("departvue".into(), 0);
            prev.push(if port_eval(&names, &conds[line_idx], &st) {
                '1'
            } else {
                '0'
            });
        }
        rows += 1;
        if fixed != orig || prev != old {
            bad += 1;
            if bad < 10 {
                eprintln!("DepartVue after Vue=: {l}");
            }
        }
    }
    assert_eq!((rows, bad), (327, 0));
}

// ---------------------------------------------------------------------------
// The subject table, the music lists, the music ring.
// ---------------------------------------------------------------------------

/// "perso:id,id;..." for the non-empty records, ascending perso.
fn canon_subjects(t: &ConSubjects) -> String {
    (0..ConSubjects::RECORDS as i32)
        .map(|r| 0x200 + r)
        .filter(|&p| !t.subjects(p).is_empty())
        .map(|p| {
            let ids: Vec<String> = t.subjects(p).iter().map(u8::to_string).collect();
            format!("{p}:{}", ids.join(","))
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// One /set line as the golden generator runs it (the line, then
/// "\r\n/1\r\n"): the key up to one of "=<>!#,", the value at the first
/// digit among the 16 bytes from there; the key matched by prefix.
fn apply_suj(t: &mut ConSubjects, line: &[u8]) {
    let mut buf = line.to_vec();
    buf.extend_from_slice(b"\r\n/1\r\n");
    let mut p = buf
        .iter()
        .position(|&c| c == b'(')
        .map_or(buf.len(), |p| p + 1);
    let mut key = Vec::new();
    while p < buf.len() && !b"=<>!#,".contains(&buf[p]) {
        key.push(buf[p].to_ascii_lowercase());
        p += 1;
    }
    let v = (0..16)
        .map(|i| p + i)
        .find(|&q| buf.get(q).is_some_and(u8::is_ascii_digit))
        .unwrap_or(p) as u32;
    if key.starts_with(b"enable") {
        t.enable(&buf, v);
    } else if key.starts_with(b"disable") {
        t.disable(&buf, v);
    }
}

/// EnableSuj / DisableSuj sequences against `FUN_00425f6c` /
/// `FUN_0042606c` under emulation ("RUNAWAY": the original reads past the
/// line, not compared).
#[test]
fn subject_sequences_match_original() {
    let cases: [(&str, &[&str], &[&str]); 11] = [
        (
            "enable_dup_order",
            &["/set(enableSuj,600=12,5,1,2)", "/set(enableSuj,600=5,7)"],
            &["600:12,5,1,2", "600:12,5,1,2,7"],
        ),
        (
            "disable_shift",
            &[
                "/set(enableSuj,600=12,5,1,2)",
                "/set(disableSuj,600=5)",
                "/set(disableSuj,600=9)",
            ],
            &["600:12,5,1,2", "600:12,1,2", "600:12,1,2"],
        ),
        (
            "disable_list_dup",
            &[
                "/set(enableSuj,587=1,2,26,36)",
                "/set(DisableSuj,587=1,2,26,36,1,38)",
            ],
            &["587:1,2,26,36", ""],
        ),
        (
            "zero_subject",
            &["/set(enableSuj,600=0,3)", "/set(disableSuj,600=0)"],
            &["600:0,3", "600:3"],
        ),
        ("byte_subject", &["/set(enableSuj,600=300)"], &["RUNAWAY"]),
        (
            "byte_subject_256",
            &["/set(enableSuj,600=256,4)"],
            &["600:0,6,4"],
        ),
        (
            "three_digit_subject",
            &["/set(enableSuj,600=123,7)"],
            &["600:123,7"],
        ),
        (
            "perso_low_byte",
            &[
                "/set(enableSuj,767=1)",
                "/set(disableSuj,767=1)",
                "/set(enableSuj,512=4)",
            ],
            &["767:1", "", "512:4"],
        ),
        (
            "sixteen",
            &[
                "/set(enableSuj,700=1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17)",
                "/set(disableSuj,700=1)",
            ],
            &[
                "700:1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17",
                "700:2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17",
            ],
        ),
        ("prefix_key", &["/set(Enable,601=3)"], &["601:3"]),
        (
            "space_after_comma",
            &["/set(enableSuj,600=1, 2)"],
            &["RUNAWAY"],
        ),
    ];
    for (name, cmds, steps) in cases {
        let mut t = ConSubjects::default();
        for (i, (cmd, step)) in cmds.iter().zip(steps.iter()).enumerate() {
            apply_suj(&mut t, cmd.as_bytes());
            if *step == "RUNAWAY" {
                break;
            }
            assert_eq!(canon_subjects(&t), *step, "{name} step {i}");
        }
    }
    // Lookups outside 0x200..0x2ff find nothing; an EnableSuj for such a
    // perso changes nothing.
    let mut t = ConSubjects::default();
    apply_suj(&mut t, b"/set(enableSuj,999=1)");
    apply_suj(&mut t, b"/set(enableSuj,600=4)");
    assert_eq!(canon_subjects(&t), "600:4");
    assert!(t.subjects(0x1ff).is_empty() && t.subjects(0x300).is_empty());
    assert!(t.has(600, 4) && !t.has(600, 5));
    t.clear();
    assert!(t.subjects(600).is_empty());
}

/// Every EnableSuj / DisableSuj line of every CON file, replayed in file
/// order from an empty table: md5 of the table after each line.
#[test]
fn subject_commands_match_original() {
    let Some(g) = game("subject_commands_match_original") else {
        return;
    };
    let golden = golden_map(include_str!("golden/scripts_suj.txt"));
    let (mut with_lines, mut bad) = (0, Vec::new());
    for f in con_files(&g, "SCENAR\\") {
        let data = read(&g, &f);
        let mut t = ConSubjects::default();
        let mut digest = String::new();
        let mut n = 0;
        for i in 0..data.len().saturating_sub(5) {
            if !data[i..i + 5].eq_ignore_ascii_case(b"/set(") {
                continue;
            }
            let after = &data[i + 5..];
            let starts =
                |p: &[u8]| after.len() >= p.len() && after[..p.len()].eq_ignore_ascii_case(p);
            if !(starts(b"enablesuj,") || starts(b"disablesuj,")) {
                continue;
            }
            let end = data[i..]
                .iter()
                .position(|&c| c == b'\r' || c == b'\n')
                .map_or(data.len(), |e| i + e);
            apply_suj(&mut t, &data[i..end]);
            digest += &canon_subjects(&t);
            digest.push('\n');
            n += 1;
        }
        if n == 0 {
            continue;
        }
        with_lines += 1;
        if golden.get(&f[7..]).copied() != Some(md5_hex(digest.as_bytes()).as_str()) {
            bad.push(f[7..].to_string());
        }
    }
    assert_eq!(with_lines, golden.len());
    assert!(bad.is_empty(), "{bad:?}");
}

/// LoadMusik / DelMusik against `FUN_0041f1d0` / `FUN_0041faa0` under
/// emulation: the copies and the set of tracks in the folder after.
#[test]
fn musik_lists_match_original() {
    let bits = |s: &str| -> u32 {
        s.split(',')
            .filter(|t| !t.is_empty())
            .map(|t| t.parse::<u32>().unwrap())
            .filter(|&v| v < 32)
            .fold(0, |b, v| b | 1 << v)
    };
    let cases = [
        ("/set(loadmusik=14)", 0, "", "13", "13"),
        ("/set(loadmusik=14)", 0, "13", "", "13"),
        ("/set(loadmusik=14)", 14, "", "", ""),
        ("/set(loadmusik=1,2,3)", 0, "1", "0,2", "0,1,2"),
        ("/set(loadmusik=9,10,11)", 0, "", "8,9", "8,9"),
        ("/set(loadmusik=10,11)", 0, "", "9", "9"),
        ("/set(loadmusik=20,21)", 0, "", "19,20", "19,20"),
        (
            "/set(delmusik=1,2,3,4,5,6,20)",
            0,
            "0,1,2,3,4,5,13,19",
            "",
            "13",
        ),
        ("/set(delmusik=14)", 14, "13", "", "13"),
        ("/set(delmusik=10,11)", 0, "9,10", "", "10"),
        (
            "/set(loadmusik=26,9,10,22,31,3,4,5,6)",
            0,
            "",
            "25,8,9",
            "8,9,25",
        ),
        (
            "/set(delmusik=1,2,3,4,5,6,7,8,11,13,14,15)",
            0,
            "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15",
            "",
            "8,9,11,15",
        ),
        (
            "/set(loadmusik=12,21,16,17,18,19,25,26,27,30)",
            0,
            "",
            "11,20,15,16,17,18,24,25,26,29",
            "11,15,16,17,18,20,24,25,26,29",
        ),
        ("/set(loadmusik=33)", 0, "", "", ""),
        ("/set(delmusik=33,2)", 0, "1,31", "", "31"),
    ];
    for (line, current, before, copies, after) in cases {
        let text = format!("{line}\r\n/1\r\n");
        let p = text.find('=').unwrap() as u32 + 1;
        let load = line.contains("loadmusik");
        let mut on_disk = bits(before);
        let got = con_music_apply(
            load,
            &con_music_list(text.as_bytes(), p),
            current,
            &mut on_disk,
        );
        let got: Vec<String> = got.iter().map(u8::to_string).collect();
        assert_eq!(got.join(","), copies, "{line}");
        assert_eq!(on_disk, bits(after), "{line}");
    }
}

/// The music ring of the original with the position law measured under
/// Wine: the PCM heard over two loops of each of the 31 tracks.
#[test]
fn music_ring_matches_original() {
    let Some(g) = game("music_ring_matches_original") else {
        return;
    };
    let (mut tracks, mut bad) = (0, Vec::new());
    for l in lines(include_str!("golden/audio_music_ring_wine.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        let (name, heard, md5): (&str, u64, &str) = (t[1], t[3].parse().unwrap(), t[5]);
        tracks += 1;
        let mut ring = MusicRing::wine(Arc::new(read(&g, &format!("WAV\\{name}"))));
        assert!(ring.start(), "{name}");
        // Whole periods until two loops are heard.
        let mut frames = 0u64;
        let mut t = 0u32;
        let mut h = Md5::new();
        let mut pcm = vec![0i16; 2 * 256];
        while frames < 2 * u64::from(ring.header_frames()) {
            let nf = ring.frames_of_tick(t);
            t += 1;
            ring.tick(nf, &mut pcm);
            let bytes: Vec<u8> = pcm[..2 * nf].iter().flat_map(|s| s.to_le_bytes()).collect();
            h.update(&bytes);
            frames += nf as u64;
        }
        let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if frames * 4 != heard || got != md5 {
            bad.push(name.to_string());
        }
    }
    assert_eq!(tracks, 31);
    assert!(bad.is_empty(), "{bad:?}");
}

/// Every StartMusik / NewSound value of the scripts names an entry of the
/// executable's tables, and the file is in the archives.
#[test]
fn script_sound_ids_in_range() {
    let Some(g) = game("script_sound_ids_in_range") else {
        return;
    };
    let (music, sounds) = (&g.exe.music_names, &g.exe.newsound_names);
    let (mut seen, mut bad) = (0, Vec::new());
    for f in con_files(&g, "SCENAR\\CHAPI") {
        let t = read(&g, &f).to_ascii_lowercase();
        for (key, table, from) in [
            (&b"startmusik="[..], music, 1usize),
            (&b"newsound="[..], sounds, 0),
        ] {
            for i in 0..t.len().saturating_sub(key.len()) {
                if &t[i..i + key.len()] != key {
                    continue;
                }
                seen += 1;
                let v = ConText(&t[i + key.len()..]).atoi(0);
                let name = usize::try_from(v)
                    .ok()
                    .filter(|&v| v >= 1)
                    .and_then(|v| table.get(v - from));
                match name {
                    Some(n) if g.big.contains(&format!("WAV\\{n}")) => {}
                    _ => bad.push(format!("{f} {}{v}", String::from_utf8_lossy(key))),
                }
            }
        }
    }
    eprintln!("{seen} StartMusik / NewSound values");
    assert!(seen > 400);
    assert!(bad.is_empty(), "{bad:?}");
}

// ---------------------------------------------------------------------------
// The scripts running in the engine.
// ---------------------------------------------------------------------------

/// The position of the first event matching `f` from `from`.
fn find(events: &[Event], from: usize, f: impl Fn(&Event) -> bool) -> usize {
    events[from..]
        .iter()
        .position(f)
        .map(|i| from + i)
        .unwrap_or_else(|| panic!("event not found after {from}: {:#?}", &events[from..]))
}

/// A new game from the start, headless, with no input: CHAPI001's /INIT
/// (its Vue=450), the arrival scan of place 450 and its section 1: the
/// music, the intro cinematic, the cinematics after it with their sound, the
/// dialog line, then place 1 and its music.
#[test]
fn new_game_plays_the_intro() {
    let Some(g) = game("new_game_plays_the_intro") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let opts = Options {
        trace: Some(trace.clone()),
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut frame = 0u64;
    let mut voices_after_cinema = 0;
    // The sound mixed and dropped, so that the dialog line's voice ends.
    let mut sound = vec![0.0f32; 2 * (22050 * 16 / 1000)];
    while frame < 40_000 {
        assert!(runner.step(frame * 16), "the engine ended");
        sys.mixer.fill(&mut sound, 22050, 2);
        frame += 1;
        let t = trace.borrow();
        if voices_after_cinema == 0 && t.iter().any(|e| matches!(e, Event::NewSound(1, _))) {
            voices_after_cinema = sys.mixer.voice_count();
        }
        if matches!(t.last(), Some(Event::Music(1))) {
            break;
        }
    }
    let ev = trace.borrow();
    let names: Vec<String> = ev.iter().map(|e| format!("{e:?}")).collect();
    let i = find(&ev, 0, |e| *e == Event::WamLoaded("ATLAN1.WAM".into()));
    let i = find(&ev, i, |e| {
        matches!(e, Event::PlaceEntered { place: 450, .. })
    });
    let i = find(&ev, i, |e| *e == Event::ChapterLoaded(1));
    let i = find(&ev, i, |e| {
        matches!(
            e,
            Event::Section {
                chapter: 1,
                id: 1,
                kind: "sel"
            }
        )
    });
    let i = find(&ev, i, |e| *e == Event::Music(20));
    // CINEM020's sound is the music track 20 (33INTRO.APC); CINEM220 has
    // its own.
    let i = find(
        &ev,
        i,
        |e| matches!(e, Event::Video { name, frames, .. } if name == "CINEM020.HNM" && *frames > 1000),
    );
    let i = find(&ev, i, |e| {
        *e == Event::NewSound(1, "WAV\\CINEM220.APC".into())
    });
    let i = find(&ev, i, |e| {
        matches!(
            e,
            Event::Line {
                section: 1,
                letter: 'A',
                ..
            }
        )
    });
    let i = find(
        &ev,
        i,
        |e| matches!(e, Event::Video { name, .. } if name == "CINEM051.HNM"),
    );
    let i = find(&ev, i, |e| {
        matches!(e, Event::PlaceEntered { place: 1, .. })
    });
    find(&ev, i, |e| *e == Event::Music(1));
    // The intro's music and its sound played together.
    assert!(voices_after_cinema >= 2, "{voices_after_cinema} voices");
    eprintln!(
        "{} events in {frame} frames: {:?}",
        ev.len(),
        &names[names.len().saturating_sub(12)..]
    );
}

/// Chapter=2 from chapter 1: the next CON loaded and its /INIT run, the
/// arrival scan with ArriveeVue = LastVue, and the place loop's own arrival
/// scan queued (LastVue != 0).
#[test]
fn chapter_change_loads_the_next_con() {
    let Some(g) = game("chapter_change_loads_the_next_con") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let result = Rc::new(RefCell::new(None));
    let (s2, t2, r2) = (sys.clone(), trace.clone(), result.clone());
    let task = async move {
        let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
        e.trace = Some(t2);
        let mut scripts = Scripts::new();
        e.new_game(&mut scripts, false).await;
        assert_eq!((scripts.chapter, e.current_place_id), (1, Some(450)));
        scripts.exec_set(&mut e, None, "chapter=2").await;
        *r2.borrow_mut() = Some((
            scripts.chapter,
            e.vars.get("chapter"),
            scripts.script.sections().len(),
            e.arrival_scan_pending,
            e.arrival_from_vue,
        ));
    };
    let mut runner = Runner::new(sys, task);
    let mut frame = 0;
    while runner.step(frame * 16) {
        frame += 1;
        assert!(frame < 100_000);
    }
    let (chapter, var, sections, pending, from) = result.borrow().unwrap();
    assert_eq!((chapter, var), (2, 2));
    assert!(sections > 0);
    assert!(pending);
    assert_eq!(from, 1);
    let ev = trace.borrow();
    find(&ev, 0, |e| *e == Event::ChapterLoaded(2));
}

/// The game's scripts, with the view aimed at the next move of a route at
/// each arrival (as a player would turn to it).
struct Route {
    scripts: Scripts,
    goal: u16,
    target: Rc<RefCell<Option<u16>>>,
}

impl Route {
    /// The first move of a shortest route to the goal in the current WAM,
    /// and a view whose centre is over its zone.
    fn aim(&self, e: &mut Engine) {
        let Some(cur) = e.current_place_id else {
            return;
        };
        let mut prev: HashMap<u16, u16> = HashMap::new();
        let mut queue = std::collections::VecDeque::from([cur]);
        while let Some(p) = queue.pop_front() {
            if p == self.goal {
                break;
            }
            let Some(place) = e.wam.places.iter().find(|x| x.id == p) else {
                continue;
            };
            for t in &place.transitions {
                if t.dst_place != cur && !prev.contains_key(&t.dst_place) {
                    prev.insert(t.dst_place, p);
                    queue.push_back(t.dst_place);
                }
            }
        }
        let mut hop = self.goal;
        while prev.get(&hop) != Some(&cur) {
            match prev.get(&hop) {
                Some(&p) => hop = p,
                None => return,
            }
        }
        let place = e.place().unwrap().clone();
        let mut w = e.warp.clone();
        for b in -12..=12 {
            for k in 0..512 {
                let (alpha, beta) = (k as f32 * w.two_pi() / 512.0, b as f32 * 0.05);
                w.set_alpha(alpha);
                w.set_beta(beta);
                let (px, py) = w.screen_to_panorama(320, 240);
                if place.hit_test(px as i16, py as i16) == hop {
                    e.set_view(alpha, beta);
                    *self.target.borrow_mut() = Some(hop);
                    return;
                }
            }
        }
    }
}

impl atlantis_engine::ScriptHooks for Route {
    fn chapter_loaded<'a>(
        &'a mut self,
        e: &'a mut Engine,
        chapter: u32,
        keep_state: bool,
    ) -> atlantis_engine::hooks::HookFuture<'a, ()> {
        self.scripts.chapter_loaded(e, chapter, keep_state)
    }

    fn arrival_scan<'a>(
        &'a mut self,
        e: &'a mut Engine,
        vue: u16,
        arrivee: i32,
    ) -> atlantis_engine::hooks::HookFuture<'a, ()> {
        Box::pin(async move {
            self.scripts.arrival_scan(e, vue, arrivee).await;
            self.aim(e);
        })
    }

    fn warp_frame<'a>(
        &'a mut self,
        e: &'a mut Engine,
    ) -> atlantis_engine::hooks::HookFuture<'a, bool> {
        self.scripts.warp_frame(e)
    }

    fn npc_click<'a>(
        &'a mut self,
        e: &'a mut Engine,
        id: u32,
        zone: u32,
    ) -> atlantis_engine::hooks::HookFuture<'a, ()> {
        self.scripts.npc_click(e, id, zone)
    }

    fn zone_click<'a>(
        &'a mut self,
        e: &'a mut Engine,
        zone: u32,
    ) -> atlantis_engine::hooks::HookFuture<'a, ()> {
        self.scripts.zone_click(e, zone)
    }

    fn departure_scan<'a>(
        &'a mut self,
        e: &'a mut Engine,
        dest: u16,
    ) -> atlantis_engine::hooks::HookFuture<'a, atlantis_engine::Departure> {
        self.scripts.departure_scan(e, dest)
    }
}

/// From the new game's start, walking from place 1 to place 14 of
/// ATLAN1.WAM, where CHAPI001 sets Chapter=2: the departure and arrival
/// scans of every move, and the chapter change loading CHAPI002.
#[test]
fn walking_to_place_14_enters_chapter_2() {
    let Some(g) = game("walking_to_place_14_enters_chapter_2") else {
        return;
    };
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sys = Sys::new(Mixer::new());
    let target = Rc::new(RefCell::new(None));
    let opts = Options {
        trace: Some(trace.clone()),
        hooks: Box::new(Route {
            scripts: Scripts::new(),
            goal: 14,
            target: target.clone(),
        }),
        ..Options::default()
    };
    let mut runner = Runner::new(sys.clone(), atlantis_engine::run(sys.clone(), g, opts));
    let mut now = 0u64;
    let mut step = |n: u64, runner: &mut Runner| {
        for _ in 0..n {
            assert!(runner.step(now), "the engine ended");
            now += 16;
        }
    };
    let loaded2 = |t: &RefCell<Vec<Event>>| t.borrow().contains(&Event::ChapterLoaded(2));
    let mut clicks = 0;
    while !loaded2(&trace) && clicks < 40 {
        // Wait for the warp view (the mouse locked), then click.
        let mut waited = 0;
        while !sys.mouse_locked() && !loaded2(&trace) {
            step(1, &mut runner);
            waited += 1;
            assert!(waited < 100_000, "no warp view");
        }
        if loaded2(&trace) {
            break;
        }
        step(10, &mut runner);
        let (x, y) = sys.mouse();
        sys.push_input(atlantis_engine::sys::Input::MouseDown {
            button: atlantis_engine::sys::MouseButton::Left,
            x,
            y,
        });
        step(2, &mut runner);
        sys.push_input(atlantis_engine::sys::Input::MouseUp {
            button: atlantis_engine::sys::MouseButton::Left,
            x,
            y,
        });
        clicks += 1;
        // Until the warp loop is left.
        let mut n = 0;
        while sys.mouse_locked() && n < 200 {
            step(1, &mut runner);
            n += 1;
        }
    }
    let places: Vec<u16> = trace
        .borrow()
        .iter()
        .filter_map(|e| match e {
            Event::PlaceEntered { place, .. } => Some(*place),
            _ => None,
        })
        .collect();
    eprintln!("{clicks} clicks, places {places:?}");
    let ev = trace.borrow();
    let from = ev
        .iter()
        .rposition(|e| matches!(e, Event::PlaceEntered { place: 13, .. }))
        .unwrap_or(0);
    for e in ev[from..]
        .iter()
        .filter(|e| !matches!(e, Event::Set(c) if c.to_ascii_lowercase().starts_with("hideperso")))
    {
        eprintln!("  {e:?}");
    }
    drop(ev);
    assert!(
        loaded2(&trace),
        "no chapter 2 after {clicks} clicks: places {places:?}"
    );
    assert!(places.contains(&14));
}
