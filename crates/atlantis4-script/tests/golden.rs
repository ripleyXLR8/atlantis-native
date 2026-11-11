//! The boot and the scripts against golden call traces made by the
//! reference Lua 5.1.4 (a small C host linked with it, with the same stub
//! bindings and the same sequence). Needs the game: `ATLANTIS4_DATA` =
//! the installed folder.
//!
//! - `boot.trace`: the boot, collector stopped, compared byte for byte;
//! - `probe.trace`: the boot, then every line of `probe_bindings.lua`
//!   (argument checks, conversions, error messages of all 54 bindings);
//! - `walk.sum`: the boot, then every node script entered in path order
//!   with its callbacks and every other script in a layout state,
//!   collector stopped: per-section line counts and hashes of the trace;
//! - `walk-gc.sum`: the same with the collector running, the save's
//!   globals compared as a set (their order depends on when the
//!   collector runs, in C Lua too).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use atlantis4_script::record::RecordingEngine;
use atlantis4_script::{
    new_game, read_boot_vars, BootValue, Collector, EngineRef, ErrorMode, GuiState, Options,
    Script, Value,
};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden");

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(GOLDEN).join(name)).expect("golden file")
}

fn recorder(root: PathBuf, collector: Collector) -> (Rc<RefCell<RecordingEngine>>, EngineRef) {
    let read = move |p: &[u8]| std::fs::read(root.join(String::from_utf8_lossy(p).as_ref())).ok();
    let rec = Rc::new(RefCell::new(RecordingEngine::new(
        Box::new(read),
        collector == Collector::Stopped,
    )));
    let engine: EngineRef = rec.clone();
    (rec, engine)
}

fn boot(root: &Path, collector: Collector) -> (Rc<RefCell<RecordingEngine>>, Script) {
    let (rec, engine) = recorder(root.to_path_buf(), collector);
    let options = Options {
        errors: ErrorMode::Log,
        collector,
    };
    let vars = read_boot_vars(&engine, options).expect("boot variables");
    let first = match vars.first_warp_path {
        Some(BootValue::Str(s)) => s,
        other => panic!("firstWarpPath is {other:?}"),
    };
    let script = new_game(&engine, options, &first).expect("new game");
    (rec, script)
}

fn dump_save(rec: &Rc<RefCell<RecordingEngine>>, script: &Script, collector: Collector) {
    let globals = script.globals().expect("globals");
    rec.borrow_mut()
        .dump_save(&globals, collector == Collector::Running);
}

/// The first line where two traces differ, for the failure message.
fn first_difference(got: &[u8], want: &[u8]) -> String {
    let g: Vec<&[u8]> = got.split(|&c| c == b'\n').collect();
    let w: Vec<&[u8]> = want.split(|&c| c == b'\n').collect();
    for i in 0..g.len().max(w.len()) {
        let (a, b) = (
            g.get(i).copied().unwrap_or(b"<end>"),
            w.get(i).copied().unwrap_or(b"<end>"),
        );
        if a != b {
            return format!(
                "line {}:\n  got:  {}\n  want: {}",
                i + 1,
                String::from_utf8_lossy(a),
                String::from_utf8_lossy(b)
            );
        }
    }
    "no difference".into()
}

fn assert_same(got: &[u8], want: &[u8], what: &str) {
    assert!(
        got == want,
        "{what} differs from the reference Lua 5.1.4: {}",
        first_difference(got, want)
    );
}

#[test]
fn boot_trace_is_lua_5_1_4s() {
    let Some(root) = atlantis4_data::testing::install_dir() else {
        return;
    };
    let (rec, script) = boot(&root, Collector::Stopped);
    dump_save(&rec, &script, Collector::Stopped);
    assert_same(&rec.borrow().out, &golden("boot.trace"), "boot trace");
}

#[test]
fn binding_checks_and_errors_are_lua_5_1_4s() {
    let Some(root) = atlantis4_data::testing::install_dir() else {
        return;
    };
    let (rec, mut script) = boot(&root, Collector::Stopped);
    let probe = golden("probe_bindings.lua");
    for (n, line) in probe.split(|&c| c == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.starts_with(b"--") {
            continue;
        }
        let no = n + 1;
        rec.borrow_mut().line(format!("PROBE {no}").as_bytes());
        script.run_chunk(line, format!("probe:{no}").as_bytes());
    }
    dump_save(&rec, &script, Collector::Stopped);
    assert_same(&rec.borrow().out, &golden("probe.trace"), "probe trace");
}

/// `tracesum.py`: a line per section (starting at `WARP ` or `GUIFILE `)
/// with its line count and hash, then the whole trace's.
fn summary(data: &[u8]) -> Vec<u8> {
    use atlantis4_script::trace::fnv1a;
    let mut lines: Vec<&[u8]> = data.split(|&c| c == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }
    let mut out = Vec::new();
    let mut sec: Vec<&[u8]> = Vec::new();
    let mut head: &[u8] = b"HEAD";
    let flush = |out: &mut Vec<u8>, head: &[u8], sec: &[&[u8]]| {
        if sec.is_empty() {
            return;
        }
        let mut bytes = Vec::new();
        for l in sec {
            bytes.extend_from_slice(l);
            bytes.push(b'\n');
        }
        out.extend_from_slice(head);
        out.extend_from_slice(
            format!(" lines={} h={:016x}\n", sec.len(), fnv1a(&bytes)).as_bytes(),
        );
    };
    for l in &lines {
        if l.starts_with(b"WARP ") || l.starts_with(b"GUIFILE ") {
            flush(&mut out, head, &sec);
            sec.clear();
            head = l;
        }
        sec.push(l);
    }
    flush(&mut out, head, &sec);
    out.extend_from_slice(
        format!("TOTAL lines={} h={:016x}\n", lines.len(), fnv1a(data)).as_bytes(),
    );
    out
}

fn lua_files(root: &Path) -> Vec<String> {
    fn scan(root: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(dir) = std::fs::read_dir(root.join(rel)) else {
            return;
        };
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if e.path().is_dir() {
                scan(root, &r, out);
            } else if name.len() > 4 && name.ends_with(".lua") {
                out.push(r);
            }
        }
    }
    let mut out = Vec::new();
    scan(root, "", &mut out);
    out.sort();
    out
}

fn is_scene(p: &str) -> bool {
    let Some(rest) = p.strip_prefix("3D/") else {
        return false;
    };
    let Some(slash) = rest.rfind('/') else {
        return false;
    };
    let stem = &rest[slash + 1..rest.len() - 4];
    stem.bytes().all(|c| c.is_ascii_digit())
}

/// Every script, as trace_host.c's `walk`.
fn walk(root: &Path, collector: Collector) -> Vec<u8> {
    let (rec, mut script) = boot(root, collector);
    let engine: EngineRef = rec.clone();
    let options = Options {
        errors: ErrorMode::Log,
        collector,
    };
    let s = |v: &str| Value::Str(v.as_bytes().to_vec());
    let mut scenes: u32 = 0;
    for p in lua_files(root) {
        if matches!(
            p.as_str(),
            "3D/OnGameEnter.lua" | "Diary/Diary.lua" | "Inventory/InventoryScripts.lua"
        ) {
            continue;
        }
        if is_scene(&p) {
            let wp = format!("{}wp", &p[..p.len() - 3]);
            rec.borrow_mut().markers.clear();
            script.change_warp(wp.as_bytes()).expect("change_warp");
            let markers = std::mem::take(&mut rec.borrow_mut().markers);
            for m in markers {
                script
                    .callback("OnWarpObjectHit", &[Value::Str(m)])
                    .expect("callback");
            }
            script
                .callback("OnMovieFinished", &[s("Videos/intro.ogv")])
                .expect("callback");
            script
                .callback("OnDialogFinished", &[s("70-2")])
                .expect("callback");
            script
                .callback("OnAnswered", &[Value::Num(1.0)])
                .expect("callback");
            script
                .callback("OnAnswered", &[Value::Num(2.0)])
                .expect("callback");
            script.callback("OnDialogClosed", &[]).expect("callback");
            script
                .callback("OnAnimationFinished", &[s("hua2")])
                .expect("callback");
            script
                .callback("OnObjectDroppedOnAvatar", &[s("Flute")])
                .expect("callback");
            script
                .callback("OnObjectDroppedOnAvatar", &[s("Verre_Eau")])
                .expect("callback");
            script
                .callback("OnCombinationFinished", &[s("Flute_Selenite")])
                .expect("callback");
            script
                .callback(
                    "OnShowedDocumentButtonValidated",
                    &[s("Enquete_08_zoomed_0"), s("button0")],
                )
                .expect("callback");
            rec.borrow_mut().markers.clear();
            scenes += 1;
            if scenes.is_multiple_of(50) {
                dump_save(&rec, &script, collector);
            }
        } else {
            rec.borrow_mut().line(format!("GUIFILE {p}").as_bytes());
            let mut gui = GuiState::new(&engine, options).expect("layout state");
            gui.run_file(p.as_bytes());
        }
    }
    dump_save(&rec, &script, collector);
    let out = rec.borrow().out.clone();
    out
}

fn check_walk(collector: Collector, golden_name: &str) {
    let Some(root) = atlantis4_data::testing::install_dir() else {
        return;
    };
    let trace = walk(&root, collector);
    let got = summary(&trace);
    let want = golden(golden_name);
    if got != want {
        // Keep the trace for a diff against trace_host's.
        let out = std::env::temp_dir().join(format!("atlantis4-script-{golden_name}.trace"));
        let _ = std::fs::write(&out, &trace);
        panic!(
            "{golden_name} differs (trace written to {}): {}",
            out.display(),
            first_difference(&got, &want)
        );
    }
}

/// With the collector stopped the whole trace, globals walks included,
/// is Lua 5.1.4's to the byte.
#[test]
fn every_script_runs_as_in_lua_5_1_4() {
    check_walk(Collector::Stopped, "walk.sum");
}

/// With the collector running, the save's globals order depends on when
/// collections run (in C Lua too); the walks are compared as sets.
#[test]
fn saves_are_the_same_sets_with_the_collector_running() {
    check_walk(Collector::Running, "walk-gc.sum");
}

/// The signature table is the one checked against the executable.
#[test]
fn signatures_are_the_executables() {
    let mut got = String::new();
    for s in &atlantis4_script::tolua::SIGNATURES {
        got.push_str(&s.render());
        got.push('\n');
    }
    let file = golden("tolua_signatures.txt");
    let want: Vec<u8> = file
        .split_inclusive(|&c| c == b'\n')
        .filter(|l| !l.starts_with(b"#"))
        .flatten()
        .copied()
        .collect();
    assert_same(got.as_bytes(), &want, "signature table");
}
