//! The canoe puzzle of Xibalba against the original's
//! (`tests/golden/canoe_0e7f.txt`, `canoe_027f.txt`: the original's code
//! run under emulation from the start of the puzzle, its header there):
//! the loading (the depth tiles, the camera's distances and the depth
//! factors, the board after the shuffle, the waypoints after the river's
//! rebuild, the stones, the board's depth, the camera's and the
//! databases' nodes), then every frame of a played sequence (the squares
//! pointed, a tile turned, taken, carried, swapped, laid back, the canoe
//! started on the shuffled board until the river breaks, then the tiles
//! put back and turned, the canoe started again and paddled to the win):
//! the puzzle's state, its script commands, the picture's, the 3D Z
//! buffer's and the held tile's Z buffer's MD5 and pixel samples; a run
//! quit by Escape. Under the Pentium II's routines and control word 0x0e7f
//! (the reference) and the C routines under 0x027f.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::collections::HashMap;
use std::path::PathBuf;

use atlantis2_data::spr::SprBank;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::canoe::{Canoe, CanoeFiles, CanoeHost, CursorPair, H, STONES, W};
use atlantis2_engine::{Fpu, Variant};
use md5::{Digest, Md5};

const GOLDEN_0E7F: &str = include_str!("golden/canoe_0e7f.txt");
const GOLDEN_027F: &str = include_str!("golden/canoe_027f.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA / ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

struct Files(A2Files);

impl CanoeFiles for Files {
    fn three_d(&self, name: &str) -> Option<Vec<u8>> {
        self.0.read_from_disc(4, Folder::ThreeD, name)
    }
    fn bank(&self, name: &str) -> Option<SprBank> {
        self.0
            .read(Folder::Spr, name)
            .and_then(|d| SprBank::parse(&d).ok())
    }
}

/// The golden's host: the commands and the variables written and read,
/// the scans, as the emulation records them.
#[derive(Default)]
struct Host {
    events: Vec<String>,
    vars: HashMap<String, i32>,
}

impl CanoeHost for Host {
    fn run_command(&mut self, cmd: &str) {
        self.events.push(format!("run {cmd}"));
    }
    fn read_var(&mut self, name: &str) -> i32 {
        let v = *self.vars.get(name).unwrap_or(&0);
        self.events.push(format!("read {name}={v}"));
        v
    }
    fn write_var(&mut self, name: &str, value: i32) {
        self.vars.insert(name.to_string(), value);
        self.events.push(format!("write {name}={value}"));
    }
    fn scan(&mut self) {
        self.events.push("scan".into());
    }
    fn music_step(&mut self) {}
}

fn md5(b: &[u8]) -> String {
    format!("{:x}", Md5::digest(b))
}

fn md5_16(p: &[u16]) -> String {
    md5(&p.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
}

fn md5_32(p: &[u32]) -> String {
    md5(&p.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
}

/// Floats as the golden writes them: their bits.
fn hexf(v: &[f64]) -> String {
    v.iter()
        .map(|x| format!("{:08x}", (*x as f32).to_bits()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The puzzle's state in the golden's layout.
fn state(c: &Canoe) -> String {
    let tiles: Vec<String> = c
        .board
        .iter()
        .map(|s| format!("{}/{}", s.tile, s.group))
        .collect();
    let way: Vec<u8> = c
        .waypoints
        .iter()
        .flat_map(|w| {
            let mut r = Vec::new();
            for p in w.pos {
                r.extend(p.to_bits().to_le_bytes());
            }
            r.extend(w.name);
            r.extend(w.flag.to_le_bytes());
            r
        })
        .collect();
    let names = ["C1", "g1", "h1", "a1", "b1", "e1"];
    let db = &c.hero[c.current];
    let (q, p, _) = c.nodes.local(db.node);
    let vis: String = c.stone_here.iter().map(|v| v.to_string()).collect();
    let cur = match c.cursor {
        CursorPair::Pointer => "True:40".to_string(),
        CursorPair::Tile(t) => format!("True:{t}"),
        CursorPair::None => "False:0".to_string(),
    };
    format!(
        "hov {} held {} from {} hpos {},{} tiles {} way {} mv {} idx {} db {} t {:08x} d {} hero {} {} st {} {} {} {} won {} cur {}",
        c.hovered,
        c.held,
        c.from,
        c.hold_at.0,
        c.hold_at.1,
        tiles.join(","),
        &md5(&way)[..12],
        u8::from(c.moving),
        c.index,
        names[c.current],
        (db.time as f32).to_bits(),
        db.delay,
        hexf(&q),
        hexf(&p),
        c.in_order,
        c.passed,
        u8::from(c.all_in_order),
        vis,
        u8::from(c.won),
        cur
    )
}

/// The load rows the port computes (`L` rows of the golden).
fn load_rows(c: &mut Canoe) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (w, h, v)) in c.depths.iter().enumerate() {
        out.push(format!("L zb {i} {w} {h} {}", md5_32(v)));
    }
    out.push(format!("L dist {}", hexf(&c.distances.map(f64::from))));
    for (i, f) in c.factors.iter().enumerate() {
        out.push(format!("L table {i} {:016x}", f.to_bits()));
    }
    for (i, s) in c.board.iter().enumerate() {
        out.push(format!(
            "L board {i} {} {} {} {} {} {}",
            s.a, s.b, s.x, s.y, s.tile, s.group
        ));
    }
    let way = c.waypoints.len();
    out.push(format!("L way {way} @"));
    for (i, w) in c.waypoints.iter().enumerate() {
        let mut b = Vec::new();
        for p in w.pos {
            b.extend(p.to_bits().to_le_bytes());
        }
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        out.push(format!("L wp {i} {hex} {} {}", w.name_str(), w.flag));
    }
    for (i, s) in c.stones.iter().enumerate() {
        let (q, p, sc) = c.nodes.local(s.node);
        out.push(format!(
            "L stone {i} {} {} node {} {} {}",
            STONES[i].1,
            c.stone_here[i],
            hexf(&q),
            hexf(&p),
            hexf(&[sc])
        ));
    }
    out.push(format!("L zboard {}", md5_32(&c.zboard)));
    let (q, p, s) = c.nodes.local(c.camera.node);
    out.push(format!(
        "L camnode {} {} {}",
        hexf(&q),
        hexf(&p),
        hexf(&[s])
    ));
    let names = ["C1", "g1", "h1", "a1", "b1", "e1"];
    let slots: Vec<(String, usize, f64)> = names
        .iter()
        .zip(&c.hero)
        .map(|(n, d)| (n.to_string(), d.node, d.length))
        .chain(
            c.stones
                .iter()
                .enumerate()
                .map(|(i, d)| (format!("PR{i}"), d.node, d.length)),
        )
        .collect();
    for (name, node, len) in slots {
        let (q, p, s) = c.nodes.local(node);
        out.push(format!(
            "L slot {name} {} {} {} len {:08x}",
            hexf(&q),
            hexf(&p),
            hexf(&[s]),
            (len as f32).to_bits()
        ));
    }
    out
}

/// The 64 pixel samples of a row (every 80 pixels across from 20, every
/// 60 rows down from 20).
fn samples(out: &[u16]) -> String {
    let mut s = Vec::new();
    for y in (20..H).step_by(60) {
        for x in (20..W).step_by(80) {
            s.push(format!("{:04x}", out[y * W + x]));
        }
    }
    s.join(" ")
}

/// A frame's inputs.
struct Input {
    n: u32,
    x: i32,
    y: i32,
    left: bool,
    right: bool,
    key: i32,
    hash: String,
}

fn inputs(golden: &str) -> Vec<Input> {
    golden
        .lines()
        .filter(|l| l.starts_with("F "))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            Input {
                n: f[1].parse().unwrap(),
                x: f[2].parse().unwrap(),
                y: f[3].parse().unwrap(),
                left: f[4] == "1",
                right: f[5] == "1",
                key: f[6].parse().unwrap(),
                hash: f[8].to_string(),
            }
        })
        .collect()
}

fn whole_rows(golden: &str) -> HashMap<u32, &str> {
    golden
        .lines()
        .filter(|l| l.starts_with("S "))
        .map(|l| {
            let (n, rest) = l[2..].split_once(' ').unwrap();
            (n.parse().unwrap(), rest)
        })
        .collect()
}

fn replay(golden: &str) {
    let Some(files) = game() else { return };
    let head = golden
        .lines()
        .take_while(|l| l.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let (fpu, variant) = if head.contains("cpu type 8") {
        assert!(head.contains("0x0e7f"));
        (Fpu::Chop, Variant::PII)
    } else {
        assert!(head.contains("0x027f"));
        (Fpu::Double, Variant::C)
    };
    let files = Files(files);
    let load = |host: &mut Host| {
        host.run_command("(flag103=1)");
        Canoe::load(fpu, variant, &files, host).expect("the canoe's files")
    };
    let mut host = Host::default();
    let mut c = load(&mut host);
    let init = golden.lines().find(|l| l.starts_with("I ")).unwrap();
    assert!(
        init.contains(&format!("| ev {} |", host.events.join("; "))),
        "{init}"
    );
    host.events.clear();
    // The loading.
    let want: Vec<&str> = golden
        .lines()
        .filter(|l| {
            l.starts_with("L ")
                && !l.starts_with("L spr ")
                && !l.starts_with("L centres")
                && !l.starts_with("L spacing")
                && !l.starts_with("L cam ")
        })
        .collect();
    let got = load_rows(&mut c);
    let mut wi = want.iter();
    for g in &got {
        let w = wi.next().unwrap_or_else(|| panic!("no golden row for {g}"));
        if g.ends_with(" @") {
            assert!(w.starts_with(g.trim_end_matches('@')), "{g} / {w}");
        } else {
            assert_eq!(g, w);
        }
    }
    assert!(wi.next().is_none());
    // The frames.
    let whole = whole_rows(golden);
    let mut out = vec![0u16; W * H];
    let mut quitted = false;
    for i in inputs(golden) {
        if i.n == 100_000 {
            // The second run: loaded again, quit by Escape.
            assert!(quitted, "the first run ends with the win");
            host = Host::default();
            c = load(&mut host);
            host.events.clear();
            quitted = false;
        }
        assert!(!quitted, "frame {} after the end", i.n);
        let going = c.frame(&mut host, (i.x, i.y), i.left, i.right, i.key == 1, &mut out);
        if !going {
            c.quit();
            quitted = true;
        }
        let ev = if host.events.is_empty() {
            "-".to_string()
        } else {
            host.events.join("; ")
        };
        host.events.clear();
        let body = format!(
            "{} {} {} {} {} -> {} | {} | ev {ev} | img {} z {} zh {}",
            i.x,
            i.y,
            u8::from(i.left),
            u8::from(i.right),
            i.key,
            u8::from(going),
            state(&c),
            md5_16(&out),
            md5_32(&c.zbuf),
            md5_32(&c.zheld)
        );
        if let Some(w) = whole.get(&i.n) {
            assert_eq!(format!("{body} | {}", samples(&out)), *w, "frame {}", i.n);
        }
        assert_eq!(&md5(body.as_bytes())[..8], i.hash, "frame {}: {body}", i.n);
    }
    assert!(quitted, "the second run ends with Escape");
}

/// The reference: the Pentium II's routines under 0x0e7f.
#[test]
fn the_canoe_as_the_original_pentium_ii() {
    replay(GOLDEN_0E7F);
}

/// The C routines under 0x027f.
#[test]
fn the_canoe_as_the_original_c_routines() {
    replay(GOLDEN_027F);
}

/// The squares under the pointer (FUN_00499270) at a few points of each
/// square and outside the board.
#[test]
fn the_squares_under_the_pointer() {
    let centre = |i: i32| {
        let (a, b) = (i % 3, i / 3);
        (
            (270.0 + 121.0 * (a as f64 + 0.5) - 89.0 * (b as f64 + 0.5)) as i32,
            (86.0 + 46.0 * (a as f64 + 0.5) + 63.0 * (b as f64 + 0.5)) as i32,
        )
    };
    for i in 0..9 {
        let (x, y) = centre(i);
        assert_eq!(Canoe::square_at(x, y), i);
    }
    assert_eq!(Canoe::square_at(10, 10), -1);
    assert_eq!(Canoe::square_at(600, 450), -1);
    assert_eq!(Canoe::square_at(270, 80), -1);
}
