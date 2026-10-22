//! The CON interpreter against the original's own code run under emulation
//! (`tests/golden/scripts_con_*.txt`, headers there).
//!
//! The scans, `/INIT` and the tick need no game data: they run on synthetic
//! chapters with a synthetic `defnames.con` holding the keys. The names,
//! the conditions, the `/set` lines and the Tibet chapters need the game:
//! `ATLANTIS2_DATA` = the installed folder; without it those tests are
//! skipped. Names of `defnames.con` entries are written `{S:I}` in the
//! golden files and filled in from the game's file here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::defnames::DefNames;
use atlantis2_engine::script::{AnimOp, DialStart, Host, Keys, Script};
use md5::{Digest, Md5};

const EVAL: &str = include_str!("golden/scripts_con_eval.txt");
const SET: &str = include_str!("golden/scripts_con_set.txt");
const SCAN: &str = include_str!("golden/scripts_con_scan.txt");
const INIT: &str = include_str!("golden/scripts_con_init.txt");
const TICK: &str = include_str!("golden/scripts_con_tick.txt");
const TIBET: &str = include_str!("golden/scripts_con_tibet.txt");
const TIBET_SCAN: &str = include_str!("golden/scripts_con_tibet_scan.txt");
const NAMES: &str = include_str!("golden/scripts_con_names.txt");

fn rows(golden: &str) -> impl Iterator<Item = &str> {
    golden
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
}

fn hex(d: &[u8]) -> String {
    Md5::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

fn install() -> Option<PathBuf> {
    let dir = std::env::var_os("ATLANTIS2_DATA");
    if dir.is_none() {
        eprintln!("ATLANTIS2_DATA not set: test skipped");
    }
    dir.map(PathBuf::from)
}

fn scenar(name: &str) -> Option<Vec<u8>> {
    Some(std::fs::read(install()?.join("scenar").join(name)).expect(name))
}

/// A `defnames.con` with one entry per section and every key.
fn synthetic_defnames() -> Vec<u8> {
    let mut t =
        b"#PERSONNAGES#\r\nDUMMY\r\n#OBJETS#\r\nDUMMY\r\n#SUJETS#\r\nDUMMY\r\n#VARIABLES#\r\n"
            .to_vec();
    for k in Keys::NAMES {
        t.extend_from_slice(k);
        t.extend_from_slice(b"\r\n");
    }
    t.extend_from_slice(b"#ZONES_CLIC#\r\nDUMMY\r\n#END_OF_FILE#\r\n");
    t
}

/// The golden's `{S:I}` / `{S:I:l}` tokens replaced by the names.
fn fill(names: &DefNames, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let j = rest[i..].find('}').unwrap() + i;
        let f: Vec<&str> = rest[i + 1..j].split(':').collect();
        let name = names
            .name(f[0].parse().unwrap(), f[1].parse().unwrap())
            .unwrap();
        let name: String = name.iter().map(|&c| char::from(c)).collect();
        out.push_str(&if f.get(2) == Some(&"l") {
            name.to_ascii_lowercase()
        } else {
            name
        });
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

/// `%.6g`.
fn g6(v: f32) -> String {
    let v = f64::from(v);
    if v == 0.0 {
        return "0".into();
    }
    let e = v.abs().log10().floor() as i32;
    let decimals = (5 - e).max(0) as usize;
    let mut t = format!("{v:.decimals$}");
    if t.contains('.') {
        t = t.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    t
}

/// Python's `round(x, 6)` printed.
fn r6(v: f32) -> String {
    let t = format!("{:.6}", f64::from(v));
    let t = t.trim_end_matches('0');
    if t.ends_with('.') {
        format!("{t}0")
    } else {
        t.to_string()
    }
}

/// A host that records the calls as the golden harness prints its stubs.
#[derive(Default)]
struct Rec {
    calls: Vec<String>,
    chapter: Option<Vec<u8>>,
    traj: Option<String>,
}

impl Rec {
    fn push(&mut self, s: String) {
        self.calls.push(s);
    }
}

impl Host for Rec {
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
        self.push(format!("LoadCurrentChap({chapter})"));
        // The golden stubs RunInit as well (Script::stub_run_init).
        self.push("RunInit()".into());
        self.chapter.clone()
    }
    fn load_wam(&mut self, name: &str) -> bool {
        self.push(format!("AtlWam_Load('{name}')"));
        true
    }
    fn has_view(&mut self, view: i32) -> bool {
        self.push(format!("AtlWam_FindView({view})"));
        true
    }
    fn enter_view(&mut self, view: i32, _: f32, _: f32) {
        self.push(format!("AtlWarp_EnterView(view#{view})"));
    }
    fn turn_now(&mut self, yaw: bool, angle: f32) {
        let axis = if yaw { "X" } else { "Y" };
        self.push(format!("AtlWarp_SetAngle{axis}Now({})", g6(angle)));
    }
    fn zoom_start(&mut self, zoom: f32, fov: f32) {
        self.push(format!("AtlWarp_ZoomStart({}, {})", g6(zoom), g6(fov)));
    }
    fn zoom_psycho_start(&mut self, a: f32, b: f32) {
        self.push(format!("AtlWarp_ZoomPsychoStart({}, {})", g6(a), g6(b)));
    }
    fn enable_zone(&mut self, view: i32, zone: i32, on: bool) {
        self.push(format!(
            "AtlWam_EnableZone(vue, zone, on)({view}, {zone}, {})",
            i32::from(on)
        ));
    }
    fn anim(&mut self, _: AnimOp) {}
    fn music_step(&mut self) {
        self.push("Input_Refresh()".into());
    }
    fn cinema_sound(&mut self, apc: &str) {
        self.push(format!("AtlSound_CinemaApc('{apc}')"));
    }
    fn play_video(&mut self, hnm: &str, dialog: Option<i32>) -> bool {
        match dialog {
            Some(m) => self.push(format!("AtlHnm_startAt('{hnm}', {m})")),
            None => self.push(format!("AtlHnm_start('{hnm}')")),
        }
        true
    }
    fn dial_stop(&mut self) {
        self.push("AtlDial_stop()".into());
    }
    fn dial_start(&mut self, n: i32) -> DialStart {
        self.push(format!("AtlDial_start({n})"));
        DialStart::Started
    }
    fn music_volume(&mut self, percent: i32) {
        self.push(format!("AtlMuzik_Volume({percent})"));
    }
    fn new_sound(&mut self, apc: &str) {
        self.push(format!("AtlSound_NewSound('{apc}')"));
    }
    fn traj_sound(&mut self, apc: &str) {
        self.traj = Some(apc.to_string());
    }
    fn load_music(&mut self, list: &str) {
        self.push(format!("AtlMuzik_Load('{list}')"));
    }
    fn delete_music(&mut self, list: &str) {
        self.push(format!("AtlMuzik_Del('{list}')"));
    }
    fn start_music(&mut self, _: &str) {
        self.push("AtlMuzik_Start(0x006d0444)()".into());
    }
    fn stop_music(&mut self) {
        self.push("AtlMuzik_Stop()".into());
    }
    fn set_monde(&mut self, monde: i32) -> bool {
        self.push(format!("Interp_SetMonde({monde}, 1)"));
        true
    }
    fn change_cd(&mut self, disc: i32) -> bool {
        self.push(format!("AtlCD_Change({disc}, 1)"));
        true
    }
    fn bag_layout(&mut self, _visible: i32) {
        self.push("Inventaire_Layout()".into());
        self.push("Inventaire_Step()".into());
    }
    fn message_clic(&mut self, text: &str) {
        self.push(format!("MessageClic('{text}', 0)"));
    }
    fn view_big_sprite(&mut self, bank: &str) {
        self.push(format!("ViewBigSpr_Start('{bank}')"));
    }
    fn credits(&mut self) {
        self.push("Credits_Start()".into());
    }
}

/// What the golden harness compares before and after a line.
#[derive(Clone, PartialEq)]
struct Snap {
    vars: Vec<i32>,
    flags: Vec<i32>,
    subj: Vec<String>,
    bag: String,
    vis: String,
    globals: Vec<(&'static str, String)>,
}

fn snap(s: &Script, h: &Rec) -> Snap {
    let subj = s
        .subjects
        .iter()
        .map(|l| {
            l.iter()
                .map(|e| format!("{}/{}/{}", e.value, e.index, u8::from(e.enabled)))
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    let bag = s
        .bag
        .iter()
        .map(|e| format!("{}/spr{}/w{}", e.id, e.sprite, e.world))
        .collect::<Vec<_>>()
        .join(";");
    let vis = s
        .shown
        .iter()
        .map(|r| format!("{}/{}/{}", r.id, u8::from(r.shown), r.kind))
        .collect::<Vec<_>>()
        .join(";");
    let q = |b: &[u8]| format!("'{}'", String::from_utf8_lossy(b));
    let globals = vec![
        ("resume_pending", u8::from(s.resume_pending).to_string()),
        ("chapter_reload", u8::from(s.chapter_reload).to_string()),
        ("chapter_cur", s.chapter.to_string()),
        ("showguest", u8::from(s.show_guest).to_string()),
        ("incperso", s.inc_perso.to_string()),
        ("quit", u8::from(s.quit).to_string()),
        ("angle_x_rad", r6(s.angle_x)),
        ("angle_y_rad", r6(s.angle_y)),
        ("warp_x", r6(s.start_x)),
        ("warp_y", r6(s.start_y)),
        ("cursor_spr", s.cursor_sprite.to_string()),
        ("hand_flag", u8::from(s.hand_drawn).to_string()),
        ("hand_x", s.hand_x.to_string()),
        (
            "next_view_ptr",
            s.next_view.map_or("0".into(), |v| format!("view#{v}")),
        ),
        ("cur_view_no", s.view_number.to_string()),
        ("click_flag", u8::from(s.click).to_string()),
        ("click_aux", s.click_aux.to_string()),
        ("cinema_flag", u8::from(s.cinema_starting).to_string()),
        ("wam", q(&s.wam)),
        (
            "traj",
            h.traj.clone().map_or("''".into(), |t| format!("'{t}'")),
        ),
        ("musik", q(&s.music)),
    ];
    Snap {
        vars: s.vars.clone(),
        flags: s.flags.clone(),
        subj,
        bag,
        vis,
        globals,
    }
}

/// The harness's change list (its order: variables, flags, subjects, bag,
/// table, globals).
fn diff(a: &Snap, b: &Snap) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (x, y)) in a.vars.iter().zip(&b.vars).enumerate() {
        if x != y {
            out.push(format!("var[{i}]={y}"));
        }
    }
    for (i, (x, y)) in a.flags.iter().zip(&b.flags).enumerate() {
        if x != y {
            out.push(format!("flag[{i}]={y}"));
        }
    }
    for (i, (x, y)) in a.subj.iter().zip(&b.subj).enumerate() {
        if x != y {
            out.push(format!("subj[{i}]={y}"));
        }
    }
    if a.bag != b.bag {
        out.push(format!("bag={}", b.bag));
    }
    if a.vis != b.vis {
        out.push(format!("vis={}", b.vis));
    }
    for ((k, x), (_, y)) in a.globals.iter().zip(&b.globals) {
        if x != y {
            out.push(format!("{k}={y}"));
        }
    }
    out
}

fn game_script() -> Option<Script> {
    let defnames = scenar("defnames.con")?;
    let mut s = Script::new(&defnames).expect("defnames.con");
    s.stub_run_init = true;
    Some(s)
}

#[test]
fn every_defnames_entry_as_the_original_reads_it() {
    let Some(data) = scenar("defnames.con") else {
        return;
    };
    let names = DefNames::parse(&data).unwrap();
    let mut n = 0;
    for row in rows(NAMES).filter(|r| !r.starts_with("errors")) {
        let f: Vec<&str> = row.split_whitespace().collect();
        let (s, rank): (usize, usize) = (f[0].parse().unwrap(), f[1].parse().unwrap());
        let name = names.name(s, rank).unwrap_or_else(|| panic!("{row}"));
        assert_eq!(hex(name), f[2], "{row}");
        assert_eq!(name.len().to_string(), f[3], "{row}");
        assert_eq!(names.id(s, name).unwrap().to_string(), f[4], "{row}");
        assert_eq!(names.param(s, name).to_string(), f[5], "{row}");
        assert_eq!(names.param_of(s, rank).to_string(), f[6], "{row}");
        for p in 1..=3 {
            let t = names.param_text(s, name, p).map_or("-".to_string(), |t| {
                let end = t
                    .iter()
                    .take(128)
                    .position(|&c| matches!(c, b'\r' | b'\n' | 0))
                    .unwrap_or(t.len().min(128));
                hex(&t[..end])
            });
            assert_eq!(t, f[6 + p], "{row} parameter {p}");
        }
        n += 1;
    }
    let counts = [0, 1, 2, 3, 4].map(|s| names.len(s));
    assert_eq!(counts, [102, 208, 94, 258, 67]);
    assert_eq!(n, counts.iter().sum::<usize>());
}

#[test]
fn conditions_match_the_original() {
    let Some(mut s) = game_script() else { return };
    let names = s.names.clone();
    let mut h = Rec::default();
    let mut sujet = (0usize, 0i32, 0i32);
    for l in EVAL.lines().filter_map(|l| l.strip_prefix("# state ")) {
        let f: Vec<i32> = l
            .split_whitespace()
            .skip(1)
            .map(|x| x.parse().unwrap())
            .collect();
        match l.split_whitespace().next().unwrap() {
            "flag" => s.flags[f[0] as usize] = f[1],
            "var" => s.vars[f[0] as usize] = f[1],
            "bag" => s.bag.push(atlantis2_engine::script::BagEntry {
                sprite: f[1] as u8,
                id: f[0],
                world: 0,
            }),
            "sujet" => sujet = (f[0] as usize, f[1], f[2]),
            _ => panic!("{l}"),
        }
    }
    let (mut idx, mut conds) = (0, 0);
    for row in rows(EVAL) {
        let row = fill(&names, row);
        if let Some(r) = row.strip_prefix("idx ") {
            let f: Vec<&str> = r.split_whitespace().collect();
            let got = names.id(f[0].parse().unwrap(), f[1].as_bytes());
            if f[1] == "NO_SUCH_NAME" {
                assert!(got.is_err(), "{row}");
            } else {
                assert_eq!(got.unwrap().to_string(), f[2], "{row}");
            }
            idx += 1;
        } else if let Some(r) = row.strip_prefix("param1 ") {
            let f: Vec<&str> = r.split_whitespace().collect();
            let got = names.param(f[0].parse().unwrap(), f[1].as_bytes());
            assert_eq!(got.to_string(), f[2], "{row}");
        } else if row.starts_with("cond") {
            let q1 = row.find('\'').unwrap();
            let q2 = row.rfind('\'').unwrap();
            let cond = &row[q1 + 1..q2];
            let rest: Vec<&str> = row[q2 + 1..].split_whitespace().collect();
            s.vars[sujet.0] = if row.starts_with("cond(sujet var 6)") {
                sujet.1
            } else {
                sujet.2
            };
            s.set_chapter_text(format!("/con{cond}\r\n").as_bytes());
            let r = s.eval_condition(&mut h, 4);
            let want_true = rest[1] == "1";
            assert_eq!(r.is_some(), want_true, "{row}");
            let ret = r.map_or(-1, |p| p as i64);
            assert_eq!(format!("ret={ret}"), rest[2], "{row}");
            if let Some(v) = rest.get(3) {
                assert_eq!(
                    format!("sacados_var={}", s.vars[s.keys.sacados]),
                    *v,
                    "{row}"
                );
            }
            conds += 1;
        }
    }
    assert_eq!((idx, conds), (24, 46));
    assert!(s.fatal.is_none());
}

/// Runs one `/set` line as the harness does: the line, CRLF, `/000`, CRLF.
fn run_set(
    s: &mut Script,
    h: &mut Rec,
    line: &str,
) -> (Option<usize>, Vec<String>, Vec<String>, Vec<String>) {
    let text = format!("{line}\r\n/000\r\n");
    s.set_chapter_text(text.as_bytes());
    s.resume_pending = false;
    h.calls.clear();
    h.chapter = Some(text.into_bytes());
    s.warnings.clear();
    let before = snap(s, h);
    let r = s.set_variables(h, atlantis2_engine::script::Src::Chapter, 0);
    let after = snap(s, h);
    let changes = diff(&before, &after);
    let warnings = s.warnings.iter().map(|w| format!("warning:{w}")).collect();
    (r, h.calls.clone(), changes, warnings)
}

#[test]
fn set_lines_match_the_original() {
    let Some(mut s) = game_script() else { return };
    let names = s.names.clone();
    let mut h = Rec::default();
    // Chapter 75 at view 7, a variable (named in the header) = 377.
    s.current_view = Some(7);
    s.view_number = 7;
    s.chapter = 75;
    s.vars[s.keys.chapter] = 75;
    s.vars[s.keys.vue] = 7;
    let memo = SET
        .lines()
        .find_map(|l| {
            l.split_once(" with {3:")
                .map(|(_, r)| r.split('}').next().unwrap().to_string())
        })
        .unwrap();
    s.vars[memo.parse::<usize>().unwrap()] = 377;
    let mut n = 0;
    for row in rows(SET) {
        let row = fill(&names, row);
        let f: Vec<&str> = row.split(" | ").collect();
        let (r, calls, changes, warnings) = run_set(&mut s, &mut h, f[0]);
        let ret = r.map_or(-1, |p| p as i64);
        assert_eq!(format!("ret={ret}"), f[1], "{row}");
        let calls = if calls.is_empty() {
            "-".into()
        } else {
            calls.join(" ")
        };
        assert_eq!(format!("calls={calls}"), f[2], "{row}");
        let changes = if changes.is_empty() {
            "-".into()
        } else {
            changes.join(" ")
        };
        assert_eq!(format!("changes={changes}"), f[3], "{row}");
        let errors = f.get(4).map_or(String::new(), |e| e.to_string());
        let warnings = if warnings.is_empty() {
            String::new()
        } else {
            format!("errors={}", warnings.join(" "))
        };
        assert_eq!(warnings, errors, "{row}");
        n += 1;
    }
    assert_eq!(n, 90);
    assert!(s.fatal.is_none(), "{:?}", s.fatal);
}

#[test]
fn every_tibet_set_line_matches_the_original() {
    let Some(mut s) = game_script() else { return };
    let names = s.names.clone();
    let mut h = Rec::default();
    s.current_view = Some(2);
    let mut golden = rows(TIBET).map(|r| fill(&names, r)).peekable();
    let mut n = 0;
    for file in ["chapi000.con", "chapi075.con"] {
        let data = scenar(file).unwrap();
        for (no, raw) in data.split(|&c| c == b'\n').enumerate() {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            if !raw.starts_with(b"/set") {
                continue;
            }
            let line: String = raw.iter().map(|&c| char::from(c)).collect();
            let (r, calls, changes, warnings) = run_set(&mut s, &mut h, &line);
            let key = line[5..]
                .split('=')
                .next()
                .unwrap()
                .split('+')
                .next()
                .unwrap()
                .split('-')
                .next()
                .unwrap()
                .split(')')
                .next()
                .unwrap()
                .to_ascii_lowercase();
            let names: Vec<&str> = calls.iter().map(|c| c.split('(').next().unwrap()).collect();
            let mut got = format!(
                "{file}:{} key={key} ret={} calls={}:{} changes={}",
                no + 1,
                r.map_or(-1, |p| p as i64),
                calls.len(),
                names.join(","),
                changes.len()
            );
            if s.fatal.is_some() {
                got.push_str(" fatal");
            }
            if !warnings.is_empty() {
                got.push_str(" warn");
            }
            let want = golden.next().unwrap_or_default();
            assert_eq!(
                got.to_ascii_lowercase(),
                want.to_ascii_lowercase(),
                "{changes:?}"
            );
            n += 1;
        }
    }
    assert!(golden.next().is_none());
    assert_eq!(n, 430);
}

#[test]
fn tibet_scans_match_the_original() {
    let Some(mut s) = game_script() else { return };
    let text = scenar("chapi075.con").unwrap();
    let starts: Vec<usize> = std::iter::once(0)
        .chain(
            text.iter()
                .enumerate()
                .filter(|(_, &c)| c == b'\n')
                .map(|(i, _)| i + 1),
        )
        .collect();
    let line_of = |p: usize| starts.iter().rposition(|&s0| s0 <= p).unwrap() + 1;
    let mut h = Rec::default();
    let mut n = 0;
    for row in rows(TIBET_SCAN).filter(|r| r.starts_with("case")) {
        let (state, want) = row.split_once(" : ").unwrap();
        s.set_chapter_text(&text);
        s.vars.iter_mut().for_each(|v| *v = 0);
        s.flags.iter_mut().for_each(|v| *v = 0);
        s.current_view = Some(10);
        for kv in state.split_whitespace().skip(1) {
            let (k, v) = kv.split_once('=').unwrap();
            let v: i32 = v.parse().unwrap();
            let i: usize = k[1..].parse().unwrap();
            if k.starts_with('v') {
                s.vars[i] = v;
            } else {
                s.flags[i] = v;
            }
        }
        h.calls.clear();
        h.chapter = Some(text.clone());
        s.trace = Some(Vec::new());
        s.resume_pending = false;
        s.ctrl_scenar_in(&mut h);
        let mut resumes = 0;
        while s.resume_pending && resumes < 20 {
            resumes += 1;
            // The end of the video or dialog loop.
            s.resume_pending = false;
            let at = s.resume_at;
            s.run_section(&mut h, at);
        }
        let lines: Vec<String> = s
            .trace
            .take()
            .unwrap()
            .iter()
            .map(|&p| line_of(p).to_string())
            .collect();
        let calls: Vec<&str> = h
            .calls
            .iter()
            .map(|c| c.split('(').next().unwrap())
            .filter(|c| !c.starts_with("Input"))
            .collect();
        let lines = if lines.is_empty() {
            "-".into()
        } else {
            lines.join(",")
        };
        let got = format!("lines {lines} resumes={resumes} calls={}", calls.join(","));
        assert_eq!(got, want, "{state}");
        n += 1;
    }
    assert_eq!(n, 16);
    assert!(s.fatal.is_none(), "{:?}", s.fatal);
}

/// The synthetic chapter of a golden's `# chapter:` line.
fn chapter_of(golden: &str) -> Vec<u8> {
    let c = golden
        .lines()
        .find_map(|l| l.strip_prefix("# chapter: "))
        .unwrap();
    c.replace('|', "\r\n").into_bytes()
}

fn set_flags(s: &Script, range: std::ops::Range<usize>, skip: &BTreeMap<usize, i32>) -> String {
    let f: Vec<String> = range
        .filter(|&i| s.flags[i] != 0 && !skip.contains_key(&i))
        .map(|i| i.to_string())
        .collect();
    if f.is_empty() {
        "-".into()
    } else {
        f.join(",")
    }
}

fn case_flags(name: &str) -> BTreeMap<usize, i32> {
    if name == "none" {
        return BTreeMap::new();
    }
    name.split('+')
        .map(|f| (f[1..].parse().unwrap(), 1))
        .collect()
}

#[test]
fn scans_and_timers_match_the_original() {
    let mut s = Script::new(&synthetic_defnames()).unwrap();
    let mut h = Rec::default();
    let chapter = chapter_of(SCAN);
    let mut n = 0;
    for row in rows(SCAN) {
        s.set_chapter_text(&chapter);
        s.flags.iter_mut().for_each(|v| *v = 0);
        if let Some(r) = row.strip_prefix("scanIN ") {
            let (name, want) = r.split_once(": ").unwrap();
            for (f, v) in case_flags(name) {
                s.flags[f] = v;
            }
            let ret = s.ctrl_scenar_in(&mut h).unwrap_or(0);
            let got = format!(
                "set flags {} ret={ret}",
                set_flags(&s, 10..30, &BTreeMap::new())
            );
            assert_eq!(got, want, "{row}");
            n += 1;
        } else if let Some(r) = row.strip_prefix("timers ") {
            let (state, want) = r.split_once(": ").unwrap();
            for kv in state.split_whitespace() {
                let (k, v) = kv.split_once('=').unwrap();
                assert!(s.set_var(k, v.parse().unwrap()), "{k}");
            }
            s.timer_scans(&mut h);
            let got = format!("set flags {}", set_flags(&s, 10..30, &BTreeMap::new()));
            assert_eq!(got, want, "{row}");
            n += 1;
        }
    }
    assert_eq!(n, 18);
    assert!(s.fatal.is_none(), "{:?}", s.fatal);
}

#[test]
fn init_blocks_match_the_original() {
    let mut s = Script::new(&synthetic_defnames()).unwrap();
    let mut h = Rec::default();
    let chapter = chapter_of(INIT);
    let mut n = 0;
    for row in rows(INIT).filter(|r| r.starts_with("init ")) {
        let (name, want) = row[5..].split_once(": ").unwrap();
        s.set_chapter_text(&chapter);
        s.flags.iter_mut().for_each(|v| *v = 0);
        let preset = case_flags(name);
        for (&f, &v) in &preset {
            s.flags[f] = v;
        }
        assert!(s.run_init(&mut h));
        let got = format!("set flags {}", set_flags(&s, 1..14, &preset));
        assert_eq!(got, want, "{row}");
        n += 1;
    }
    assert_eq!(n, 5);
}

#[test]
fn the_clocks_match_the_original() {
    let mut s = Script::new(&synthetic_defnames()).unwrap();
    let mut now = 0.0f64;
    let mut n = 0;
    for row in rows(TICK) {
        let f: BTreeMap<&str, &str> = row
            .split_whitespace()
            .skip(2)
            .filter_map(|kv| kv.split_once('='))
            .collect();
        let dt: f64 = match f["dt"] {
            "0.066667" => 1.0 / 15.0,
            d => d.parse().unwrap(),
        };
        let last = now;
        now += dt;
        s.tick_flags = [false; 4];
        s.tick(now, last);
        let got = format!(
            "ticks={} acc={:.6} random={} time1={} time2={} flags={}",
            s.ticks,
            f64::from(s.seconds),
            s.var("RANDOM").unwrap(),
            s.var("TIME1").unwrap(),
            s.var("TIME2").unwrap(),
            s.tick_flags
                .map(|b| if b { '1' } else { '0' })
                .iter()
                .collect::<String>()
        );
        let want = format!(
            "ticks={} acc={} random={} time1={} time2={} flags={}",
            f["ticks"], f["acc"], f["random"], f["time1"], f["time2"], f["flags"]
        );
        assert_eq!(got, want, "{row}");
        n += 1;
    }
    assert_eq!(n, 39);
}

/// The run model in the warp frame, against the original's Tibet moves
/// under emulation (the same frame dispatcher, warp frame and tick, with
/// chapi075.con): a click on a move zone runs a departure scan with
/// `DEPARTVUE` = the target and nothing in it holds; the arrival in view 7
/// runs line 14 (a greeting, `dial=360`), the dialog's end line
/// 15; the arrival in view 10 runs line 294; the next frame runs nothing.
#[test]
fn arrival_and_departure_scans_follow_the_original_moves() {
    let Some(mut s) = game_script() else { return };
    s.stub_run_init = false;
    let text = scenar("chapi075.con").unwrap();
    let line_of = |p: usize| text[..p].iter().filter(|&&c| c == b'\n').count() + 1;
    let mut h = Rec {
        chapter: Some(text.clone()),
        ..Default::default()
    };
    assert!(s.set_chapter_text(&text));
    s.current_view = Some(6);
    s.view_number = 6;
    s.trace = Some(Vec::new());

    // Departure from view 6 towards 7.
    s.click = true;
    assert!(s.departure_scan(&mut h, Some(7)));
    assert_eq!(s.vars[s.keys.departvue], 0);
    assert!(s.trace.as_ref().unwrap().is_empty());

    // The move completes; the arrival scan runs in the next frame.
    s.arrive(7);
    assert_eq!(
        (s.var("LASTVUE"), s.var("VUE"), s.var("ARRIVEEVUE")),
        (Some(6), Some(7), Some(7))
    );
    let yaw = (-164.394f32).to_radians();
    s.gere_frame(&mut h, yaw, 0.26);
    let lines: Vec<usize> = s.trace.take().unwrap().into_iter().map(line_of).collect();
    assert_eq!(lines, [14]);
    assert!(s.resume_pending);
    assert_eq!(s.var("ARRIVEEVUE"), Some(0));
    assert_eq!(s.var("ANGLEWARPX"), Some(196));
    assert_eq!(s.var("LIMITEANGLEWARPY"), Some(70));
    // The dialog ends: line 15.
    s.trace = Some(Vec::new());
    s.dialog_ended(&mut h);
    let lines: Vec<usize> = s.trace.take().unwrap().into_iter().map(line_of).collect();
    assert_eq!(lines, [15]);
    assert_eq!(s.var("SUJET"), Some(-2));
    // Nothing more in the next frame.
    s.trace = Some(Vec::new());
    s.gere_frame(&mut h, yaw, 0.26);
    assert!(s.trace.take().unwrap().is_empty());

    // View 10: line 294 (flag 202) once.
    s.arrive(10);
    s.trace = Some(Vec::new());
    s.gere_frame(&mut h, 0.0, 0.0);
    let lines: Vec<usize> = s.trace.take().unwrap().into_iter().map(line_of).collect();
    assert_eq!(lines, [294]);
    assert_eq!(s.flags[202], 1);
    assert!(s.fatal.is_none(), "{:?}", s.fatal);
}

/// A new game: chapter 0 at start-up, its `/INIT` from the menu. The
/// opening film interrupts it; the rest (the music) runs after the film,
/// and chapter 0's first section sends the game to chapter 75 only on a
/// scan.
#[test]
fn a_new_game_runs_chapter_0s_init() {
    let Some(mut s) = game_script() else { return };
    s.stub_run_init = false;
    struct Game {
        rec: Rec,
    }
    impl Host for Game {
        fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
            self.rec.push(format!("LoadCurrentChap({chapter})"));
            scenar(&format!("chapi{chapter:03}.con"))
        }
        fn load_wam(&mut self, name: &str) -> bool {
            self.rec.load_wam(name)
        }
        fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {
            self.rec.push(format!(
                "enter {view} {:08x} {:08x}",
                yaw.to_bits(),
                pitch.to_bits()
            ));
        }
        fn play_video(&mut self, hnm: &str, dialog: Option<i32>) -> bool {
            self.rec.play_video(hnm, dialog)
        }
        fn start_music(&mut self, list: &str) {
            self.rec.push(format!("music {list}"));
        }
        fn set_monde(&mut self, monde: i32) -> bool {
            self.rec.set_monde(monde)
        }
    }
    let mut g = Game {
        rec: Rec::default(),
    };
    assert!(s.boot(&mut g));
    assert!(s.new_game(&mut g));
    let calls = g.rec.calls.join("; ");
    // -90° and 0° as the /set handler converts them (0xbfc90fdb, 0).
    assert_eq!(
        calls,
        "LoadCurrentChap(0); Interp_SetMonde(5, 1); AtlWam_Load('tibet.wam'); \
         enter 2 bfc90fdb 00000000; AtlHnm_start('cine069.hnm')"
    );
    assert!(s.resume_pending);
    assert_eq!(
        (s.var("MONDE"), s.var("VUE"), s.var("TESTTIME1")),
        (Some(5), Some(2), Some(1))
    );
    g.rec.calls.clear();
    s.video_ended(&mut g);
    assert_eq!(g.rec.calls, ["music 30)"]);
    assert_eq!(s.music, b"30)");
    // A click anywhere: chapter 0's /000 head (flag500 = 0) loads chapter
    // 75, whose /INIT runs at once; the reload flag the chapter change sets
    // makes RunInit run the block's first line twice (lines 2, 2, 3).
    g.rec.calls.clear();
    s.click = true;
    s.trace = Some(Vec::new());
    s.click_scan(&mut g, None);
    assert_eq!(s.chapter, 75);
    assert_eq!(s.var("VignetteSave"), Some(11));
    assert_eq!(g.rec.calls, ["LoadCurrentChap(75)", "music 30)"]);
    let t75 = scenar("chapi075.con").unwrap();
    let line_of = |p: usize| t75[..p].iter().filter(|&&c| c == b'\n').count() + 1;
    let traced = s.trace.take().unwrap();
    // The first position is chapter 0's own /set(chapter=75) line.
    let lines: Vec<usize> = traced[1..].iter().map(|&p| line_of(p)).collect();
    assert_eq!(lines, [2, 2, 3]);
    assert!(s.fatal.is_none(), "{:?}", s.fatal);
}

/// Lines the walks run twice, counted with `++` (no game data): lines 2..n
/// of a true `/con` section inside `/INIT`, and the first `/INIT` line of
/// a chapter loaded by `/set(chapter=N)` from a scan; a new game's `/INIT`
/// runs its lines once.
#[test]
fn lines_run_twice_as_in_the_original() {
    const REINIT: &str = include_str!("golden/scripts_con_reinit.txt");
    let chapter = |tag: &str| {
        REINIT
            .lines()
            .find_map(|l| l.strip_prefix(&format!("# {tag} chapter: ")))
            .unwrap()
            .replace('|', "\r\n")
            .into_bytes()
    };
    let (init, from, to) = (chapter("init"), chapter("from"), chapter("to"));
    let mut s = Script::new(&synthetic_defnames()).unwrap();
    let mut h = Rec::default();
    let counts = |s: &Script| {
        format!(
            "time1={} time2={} flag3={}",
            s.var("TIME1").unwrap(),
            s.var("TIME2").unwrap(),
            s.flags[3]
        )
    };
    let mut got = Vec::new();
    for f2 in [0, 1] {
        s.set_chapter_text(&init);
        s.vars.iter_mut().for_each(|v| *v = 0);
        s.flags.iter_mut().for_each(|v| *v = 0);
        s.flags[2] = f2;
        assert!(s.run_init(&mut h));
        got.push(format!("init flag2={f2}: {}", counts(&s)));
    }
    s.set_chapter_text(&from);
    s.vars.iter_mut().for_each(|v| *v = 0);
    s.flags.iter_mut().for_each(|v| *v = 0);
    s.flags[1] = 1;
    h.calls.clear();
    h.chapter = Some(to.clone());
    let r = s.ctrl_scenar_in(&mut h).unwrap_or(0);
    // Rec reports a stubbed RunInit after each load; this one ran.
    h.calls.retain(|c| c != "RunInit()");
    got.push(format!(
        "scan chapter: {} ret={r} calls={}",
        counts(&s),
        h.calls.join(" ")
    ));
    s.set_chapter_text(&to);
    s.vars.iter_mut().for_each(|v| *v = 0);
    assert!(s.run_init(&mut h));
    got.push(format!("new game init: {}", counts(&s)));
    let want: Vec<&str> = rows(REINIT).filter(|r| !r.starts_with("errors")).collect();
    assert_eq!(got, want);
}
