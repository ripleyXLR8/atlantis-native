//! The canoe puzzle of Xibalba against the original's
//! (`tests/golden/canoe_0e7f.txt`, `canoe_027f.txt`: the original's code
//! run under emulation from the start of the puzzle, its header there):
//! the loading (the init's command, cursor and control word; the depth
//! tiles and the tiles' offsets, the camera's distances, the squares'
//! centres, the depth factors, the spacing, the board after the shuffle,
//! the waypoints after the river's rebuild by the MD5 of their table, the
//! stones, the board's depth, the camera, its node and the databases'
//! nodes, the stones' and the slots' by an MD5), then every frame of a
//! played sequence (the squares
//! pointed, a tile turned, taken, carried, swapped, laid back, the canoe
//! started on the shuffled board until the river breaks, then the tiles
//! put back and turned, the canoe started again and paddled to the win):
//! the puzzle's state, its script commands, the picture's, the 3D Z
//! buffer's and the held tile's Z buffer's MD5 and pixel samples; a run
//! quit by Escape; the end rows (the win's frame count and the variables
//! written, the cursor after the quit). Under the Pentium II's routines
//! and control word 0x0e7f (the reference) and the C routines under
//! 0x027f. The original's fatal errors of the river's rebuild and the
//! stones' search on changed waypoints.
//!
//! The game-data tests need `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them they are skipped.

use std::collections::HashMap;
use std::path::PathBuf;

use atlantis2_data::spr::SprBank;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::canoe::{
    Canoe, CanoeError, CanoeFiles, CanoeHost, CursorPair, CENTRES, H, SPACING, START_COMMAND,
    STONES, TILE_OFFSETS, W,
};
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

/// The first 12 hex digits of the MD5 of a text (the golden's `h` fields).
fn h12(text: &str) -> String {
    md5(text.as_bytes())[..12].to_string()
}

/// Floats as the golden writes them: their bits.
fn hexf(v: &[f64]) -> String {
    v.iter()
        .map(|x| format!("{:08x}", (*x as f32).to_bits()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The waypoint table as the original's records (0x18 bytes each).
fn way_bytes(c: &Canoe) -> Vec<u8> {
    c.waypoints
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
        .collect()
}

/// The cursor pair as the golden writes it (set, frame).
fn cursor(c: &Canoe) -> String {
    match c.cursor {
        CursorPair::Pointer => "True:40".to_string(),
        CursorPair::Tile(t) => format!("True:{t}"),
        CursorPair::None => "False:0".to_string(),
    }
}

/// The puzzle's state in the golden's layout.
fn state(c: &Canoe) -> String {
    let tiles: Vec<String> = c
        .board
        .iter()
        .map(|s| format!("{}/{}", s.tile, s.group))
        .collect();
    let way = way_bytes(c);
    let names = ["C1", "g1", "h1", "a1", "b1", "e1"];
    let db = &c.hero[c.current];
    let (q, p, _) = c.nodes.local(db.node);
    let vis: String = c.stone_here.iter().map(|v| v.to_string()).collect();
    let cur = cursor(c);
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

/// The load rows the port computes (`L` rows of the golden), the
/// camera's left out (see [`camera_row`]): the tiles' offsets (FUN_00498080:
/// the table less (0x10f, 0x6b), the x written as an unsigned dword), the
/// squares' centres and the spacing, the port's tables.
fn load_rows(c: &Canoe) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (w, h, v)) in c.depths.iter().enumerate() {
        out.push(format!("L zb {i} {w} {h} {}", md5_32(v)));
        let (ox, oy) = TILE_OFFSETS[i];
        out.push(format!("L spr {i} {} {}", (ox - 0x10f) as u32, oy - 0x6b));
    }
    out.push(format!("L dist {}", hexf(&c.distances.map(f64::from))));
    let centres: Vec<String> = CENTRES
        .iter()
        .flatten()
        .map(|b| format!("{b:08x}"))
        .collect();
    out.push(format!("L centres {}", centres.join(" ")));
    for (i, f) in c.factors.iter().enumerate() {
        out.push(format!("L table {i} {:016x}", f.to_bits()));
    }
    out.push(format!("L spacing {:08x} {:08x}", SPACING[0], SPACING[1]));
    for (i, s) in c.board.iter().enumerate() {
        out.push(format!(
            "L board {i} {} {} {} {} {} {}",
            s.a, s.b, s.x, s.y, s.tile, s.group
        ));
    }
    out.push(format!(
        "L way {} {}",
        c.waypoints.len(),
        md5(&way_bytes(c))
    ));
    let node = |n: usize| {
        let (q, p, sc) = c.nodes.local(n);
        h12(&format!("{} {} {}", hexf(&q), hexf(&p), hexf(&[sc])))
    };
    for (i, s) in c.stones.iter().enumerate() {
        out.push(format!(
            "L stone {i} {} {} node h {}",
            STONES[i].1,
            c.stone_here[i],
            node(s.node)
        ));
    }
    out.push(format!("L zboard {}", md5_32(&c.zboard)));
    out.push("L cam".into());
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
    for (name, n, len) in slots {
        out.push(format!(
            "L slot {name} h {} len {:08x}",
            node(n),
            (len as f32).to_bits()
        ));
    }
    out
}

/// The golden's camera row (FUN_0042a920's camera, the 26 floats from +0,
/// the mode, the viewport) against the port's camera. Floats 8 .. 23
/// (+0x20 .. +0x5c) are left out: the constructor's perspective derive
/// writes them (FUN_004a4730 → FUN_004a4950 with the angle 0x3fa9c91f,
/// 640 x 480: mode 1) and FUN_004a4a30 in mode 2 does not (it writes +0x10,
/// +0x14 and +0x60, +0x64 only); the port's orthographic camera has no
/// such fields.
fn camera_row(c: &Canoe, row: &str) {
    let (floats, rest) = row["L cam ".len()..].split_once(" | ").unwrap();
    let f: Vec<&str> = floats.split(' ').collect();
    assert_eq!(f.len(), 26, "{row}");
    let cam = &c.camera;
    let fpu = c.nodes.fpu;
    let mine = [
        (0, cam.fx),
        (1, cam.fy),
        (2, cam.cx),
        (3, cam.cy),
        (4, fpu.div(1.0, cam.fx)),
        (5, fpu.div(1.0, cam.fy)),
        (6, cam.near),
        (7, cam.far),
        (24, cam.a),
        (25, cam.b),
    ];
    for (k, v) in mine {
        assert_eq!(
            f[k],
            format!("{:08x}", fpu.f32(v).to_bits()),
            "L cam float {k}: {row}"
        );
    }
    let vp: Vec<String> = cam
        .viewport
        .iter()
        .map(|v| format!("{:08x}", *v as u32))
        .collect();
    // The port's camera is the orthographic one, mode 2 (@ 0x004a4910).
    assert_eq!(rest, format!("mode 2 vp {}", vp.join(" ")));
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
    // `pz_pirogue_init` runs `(flag103=1)` before any load; that is the
    // caller's (the engine's `start_canoe`), with the port's constant.
    let load = |host: &mut Host| {
        let c = Canoe::load(fpu, variant, &files, host).expect("the canoe's files");
        assert!(
            host.events.is_empty(),
            "the loading's events: {:?}",
            host.events
        );
        c
    };
    let mut host = Host::default();
    let mut c = load(&mut host);
    let init = golden.lines().find(|l| l.starts_with("I ")).unwrap();
    assert_eq!(
        init,
        format!(
            "I init -> 1 | ev run {START_COMMAND} | cur {} | fpcw {:04x}",
            cursor(&c),
            c.nodes.fpu.control_word()
        )
    );
    // The loading.
    let want: Vec<&str> = golden.lines().filter(|l| l.starts_with("L ")).collect();
    let got = load_rows(&c);
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        if g == "L cam" {
            camera_row(&c, w);
        } else {
            assert_eq!(g, w);
        }
    }
    // The frames.
    let whole = whole_rows(golden);
    let mut out = vec![0u16; W * H];
    let mut quitted = false;
    // The frames since the last click or key: the win's count (the golden's
    // `E` row counts the frames after the start).
    let mut since_click = 0;
    let end = |golden: &str, tag: &str| {
        golden
            .lines()
            .find(|l| l.starts_with(tag))
            .unwrap()
            .to_string()
    };
    let won = end(golden, "E ");
    for i in inputs(golden) {
        if i.n == 100_000 {
            // The second run: loaded again, quit by Escape.
            assert!(quitted, "the first run ends with the win");
            let mut vars: Vec<_> = host.vars.iter().collect();
            vars.sort();
            let vars: Vec<String> = vars.iter().map(|(k, v)| format!("('{k}', {v})")).collect();
            let want = format!(
                "E won after {since_click} frames | vars [{}] | ",
                vars.join(", ")
            );
            assert!(c.won && won.starts_with(&want), "{want} / {won}");
            host = Host::default();
            c = load(&mut host);
            quitted = false;
        }
        if i.left || i.right || i.key != 0 {
            since_click = 0;
        } else {
            since_click += 1;
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
    // The cursor pair after the quit. The golden's `loaded 0` (@ 0x006d8278,
    // which makes FUN_00498ca0 quit) has no field here: the caller drops
    // the puzzle.
    assert_eq!(end(golden, "Q "), format!("Q cur {} loaded 0", cursor(&c)));
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

/// The game's files with the trajectory's first waypoint renamed (its
/// fourth character changed): the rebuilt river no longer starts at the
/// waypoint the original expects.
struct Renamed(Files);

impl CanoeFiles for Renamed {
    fn three_d(&self, name: &str) -> Option<Vec<u8>> {
        let mut d = self.0.three_d(name)?;
        if name == atlantis2_data::canoe::TRAJECTORY_FILE {
            let at = d
                .windows(4)
                .position(|w| w == b"X9E1")
                .expect("the first waypoint");
            d[at + 3] = b'2';
        }
        Some(d)
    }
    fn bank(&self, name: &str) -> Option<SprBank> {
        self.0.bank(name)
    }
}

/// The original's fatal errors (FUN_00480cd0, which saves and quits): the
/// river's first waypoint moved (@ 0x00498b24), a segment that starts on
/// neither end (@ 0x0049891b), a stone's waypoint not found (@ 0x00499b96);
/// the message kept, the work stopped, the next frame 0.
#[test]
fn the_canoes_fatal_errors() {
    let Some(game) = game() else { return };
    let files = Files(game);
    let mut host = Host::default();
    let renamed = Renamed(files);
    let r = Canoe::load(Fpu::Chop, Variant::PII, &renamed, &mut host);
    assert_eq!(
        r.err(),
        Some(CanoeError::Fatal(
            "recalc_trajet() : premier point déplacé!!".into()
        ))
    );
    let files = renamed.0;
    let fresh = |host: &mut Host| Canoe::load(Fpu::Chop, Variant::PII, &files, host).unwrap();
    let mut out = vec![0u16; W * H];

    let mut c = fresh(&mut host);
    assert_eq!(c.fatal, None);
    c.waypoints[0].name[2] = b'Q';
    let name = c.waypoints[0].name_str();
    c.rebuild();
    assert_eq!(
        c.fatal.as_deref(),
        Some(format!("recalc_trajet() err Debut de sequence {name}").as_str())
    );
    assert!(!c.frame(&mut host, (10, 10), false, false, false, &mut out));

    let mut c = fresh(&mut host);
    let stone = STONES[0].1;
    let k = c
        .waypoints
        .iter()
        .position(|w| w.name_str() == stone)
        .unwrap();
    c.waypoints[k].name[stone.len() - 1] ^= 1;
    c.rebuild();
    assert_eq!(
        c.fatal.as_deref(),
        Some(format!("find_mesh_name(): {} non trouvé", STONES[0].1).as_str())
    );
    assert!(!c.frame(&mut host, (10, 10), true, false, false, &mut out));
}
