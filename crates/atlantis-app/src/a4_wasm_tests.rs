//! Atlantis Evolution's decoders and scripts in WebAssembly, against the
//! same goldens as their desktop tests: the boot and the bindings' probe
//! traced by the reference Lua 5.1.4 (`atlantis4-script/tests/golden`),
//! the RGBA frames of four videos (Theora, the original's colour table and
//! alpha) and the samples of Vorbis files decoded by the game's own
//! libvorbisfile (`atlantis4-data/tests/golden`).
//!
//! Run in Node.js by `wasm-bindgen-test-runner` (the crate's
//! `--target wasm32-unknown-unknown` tests); the
//! game's files are read through Node's `fs` from `ATLANTIS4_DATA`. Without
//! it each test returns at once, unless `ATLANTIS4_REQUIRE_DATA=1`.

use std::cell::RefCell;
use std::rc::Rc;

use atlantis4_data::video::{self, RgbTable, Video};
use atlantis4_data::wpa::Package;
use atlantis4_script::record::RecordingEngine;
use atlantis4_script::{
    new_game, read_boot_vars, BootValue, Collector, EngineRef, ErrorMode, Options, Script,
};
use md5::{Digest, Md5};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test as test;

#[wasm_bindgen(module = "fs")]
extern "C" {
    #[wasm_bindgen(js_name = readFileSync, catch)]
    fn read_file_sync(path: &str) -> Result<Vec<u8>, JsValue>;
    #[wasm_bindgen(js_name = existsSync)]
    fn exists_sync(path: &str) -> bool;
}

/// An environment variable of the Node.js process.
fn env(name: &str) -> Option<String> {
    let process = js_sys::Reflect::get(&js_sys::global(), &"process".into()).ok()?;
    let env = js_sys::Reflect::get(&process, &"env".into()).ok()?;
    js_sys::Reflect::get(&env, &name.into()).ok()?.as_string()
}

/// The install, or `None` (the test then does not run).
fn install() -> Option<String> {
    let dir = env("ATLANTIS4_DATA").filter(|d| !d.is_empty());
    if dir.is_none() {
        assert!(
            env("ATLANTIS4_REQUIRE_DATA").as_deref() != Some("1"),
            "ATLANTIS4_DATA is not set"
        );
    }
    dir
}

/// A file of the install, its name in any case (as the game names them):
/// each folder looked up without regard to case, as on Windows.
fn read(dir: &str, name: &str) -> Option<Vec<u8>> {
    let direct = format!("{dir}/{name}");
    if exists_sync(&direct) {
        return read_file_sync(&direct).ok();
    }
    let mut path = dir.to_string();
    for part in name.replace('\\', "/").split('/') {
        let entries = readdir(&path)?;
        let found = entries.iter().find(|e| e.eq_ignore_ascii_case(part))?;
        path = format!("{path}/{found}");
    }
    read_file_sync(&path).ok()
}

#[wasm_bindgen(module = "fs")]
extern "C" {
    #[wasm_bindgen(js_name = readdirSync, catch)]
    fn readdir_sync(path: &str) -> Result<Vec<JsValue>, JsValue>;
}

fn readdir(path: &str) -> Option<Vec<String>> {
    Some(
        readdir_sync(path)
            .ok()?
            .iter()
            .filter_map(JsValue::as_string)
            .collect(),
    )
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The boot, with the recording engine of the reference traces.
fn boot(dir: &str) -> (Rc<RefCell<RecordingEngine>>, Script) {
    let root = dir.to_string();
    let reader = move |p: &[u8]| read(&root, &String::from_utf8_lossy(p));
    let rec = Rc::new(RefCell::new(RecordingEngine::new(Box::new(reader), true)));
    let engine: EngineRef = rec.clone();
    let options = Options {
        errors: ErrorMode::Log,
        collector: Collector::Stopped,
    };
    let vars = read_boot_vars(&engine, options).expect("boot variables");
    let Some(BootValue::Str(first)) = vars.first_warp_path else {
        panic!("no firstWarpPath");
    };
    let script = new_game(&engine, options, &first).expect("new game");
    (rec, script)
}

fn dump(rec: &Rc<RefCell<RecordingEngine>>, script: &Script) -> Vec<u8> {
    let globals = script.globals().expect("globals");
    rec.borrow_mut().dump_save(&globals, false);
    rec.borrow().out.clone()
}

#[test]
fn the_boot_trace_is_lua_5_1_4s() {
    let Some(dir) = install() else { return };
    let (rec, script) = boot(&dir);
    let got = dump(&rec, &script);
    let want = include_bytes!("../../atlantis4-script/tests/golden/boot.trace");
    assert!(got == want, "{}", String::from_utf8_lossy(&got));
}

#[test]
fn the_bindings_probe_is_lua_5_1_4s() {
    let Some(dir) = install() else { return };
    let (rec, mut script) = boot(&dir);
    let probe = include_bytes!("../../atlantis4-script/tests/golden/probe_bindings.lua");
    for (n, line) in probe.split(|&c| c == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.starts_with(b"--") {
            continue;
        }
        let no = n + 1;
        rec.borrow_mut().line(format!("PROBE {no}").as_bytes());
        script.run_chunk(line, format!("probe:{no}").as_bytes());
    }
    let got = dump(&rec, &script);
    let want = include_bytes!("../../atlantis4-script/tests/golden/probe.trace");
    assert!(got[..] == want[..], "the probe trace differs");
}

/// A stream's bytes: a file, or `file:entry` of a warp package.
fn stream(dir: &str, name: &str) -> Option<Vec<u8>> {
    match name.split_once(".wpa:") {
        Some((file, entry)) => {
            let data = read(dir, &format!("{file}.wpa"))?;
            let p = Package::parse(&data).ok()?;
            let e = p.entry(entry)?;
            Some(p.slice(&data, e)?.to_vec())
        }
        None => read(dir, name),
    }
}

#[test]
fn four_videos_convert_to_the_models_rgba() {
    let Some(dir) = install() else { return };
    let exe = read(&dir, atlantis4_data::exe::EXE_NAME).expect("the executable");
    let table = RgbTable::from_exe(&exe).expect("the colour table");
    let text = include_str!("../../atlantis4-data/tests/golden/video.txt");
    let mut checked = 0;
    for l in text.lines().filter(|l| l.starts_with("rgba\t")) {
        let f: Vec<&str> = l.split('\t').collect();
        let (name, size, frames, want) = (f[1], f[2], f[3], f[4]);
        let colour = stream(&dir, name).expect(name);
        let alpha_name = match name.split_once(':') {
            Some((p, e)) => format!("{p}:{}", video::alpha_name(e).unwrap()),
            None => video::alpha_name(name).unwrap(),
        };
        let alpha = stream(&dir, &alpha_name);
        let mut v = Video::open(&colour, alpha.as_deref()).unwrap();
        let (w, h) = v.size();
        assert_eq!(format!("{w}x{h}"), size, "{name}");
        let mut out = vec![0u8; (w * h * 4) as usize];
        let mut md5 = Md5::new();
        let mut n = 0;
        while v.next_rgba(&table, &mut out).unwrap() {
            md5.update(&out);
            n += 1;
        }
        assert_eq!(n.to_string(), frames, "{name}");
        assert_eq!(hex(&md5.finalize()), want, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 4);
}

#[test]
fn vorbis_samples_equal_the_games_decoder() {
    let Some(dir) = install() else { return };
    let golden = include_str!("../../atlantis4-data/tests/golden/vorbis.txt");
    // Every 16th of the 564 files the game's own decoder made (36 files:
    // ambiences, musics, voices, effects).
    let lines: Vec<&str> = golden
        .lines()
        .filter(|l| !l.starts_with('#') && l.split(' ').count() >= 4)
        .collect();
    let mut checked = 0;
    for l in lines.iter().step_by(16) {
        let mut f = l.splitn(4, ' ');
        let (md5, frames, ch, path) = (
            f.next().unwrap(),
            f.next().unwrap(),
            f.next().unwrap(),
            f.next().unwrap(),
        );
        let data = read(&dir, path).unwrap_or_else(|| panic!("{path}"));
        let (channels, _, pcm) =
            atlantis4_data::vorbis::decode(&data).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut h = Md5::new();
        for s in &pcm {
            h.update(s.to_le_bytes());
        }
        assert_eq!(hex(&h.finalize()), md5, "{path}");
        assert_eq!((pcm.len() / channels).to_string(), frames, "{path}");
        assert_eq!(channels.to_string(), ch, "{path}");
        checked += 1;
    }
    assert!(checked >= 30, "{checked}");
}
