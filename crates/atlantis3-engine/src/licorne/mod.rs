//! The unicorn board, loop mode 11 `LICORNE` (atlantis3-gog-fr-3473408:
//! start @ 0x0042e7a0, frame @ 0x0042eb50, stop @ 0x0042e990; the board
//! object `CPuzzle_Licorne` @ 0x004b9c00..0x004bed40): an 11 x 7 maze seen
//! in perspective (a pre-rendered picture and its depth buffer), four
//! beacons the player takes from the tray and drops on cells, a unicorn
//! that, clicked, computes its whole path at once and gallops along it, a
//! three-headed beast 12 units behind that catches it when it stops short
//! of the target (0, 0).
//!
//! [`Board`] is the C code's globals and the board object; one call of
//! [`Board::frame`] is one mode-11 frame (input, the cursor's 25 Hz
//! clock, the 30 Hz animation clock and the run, sound 810, the outcome),
//! then [`Board::draw`] the frame's picture (background, Z from the
//! `.zbf`, the unicorn, the beast, the beacons, the carried beacon over a
//! cleared Z). The caller draws the cursor ([`Board::cursor`]) and flips.
//!
//! Every float operation runs under 0x0e7f ([`scene::K`]), each stored
//! value rounded as the original stores it.

pub mod scene;

use atlantis2_data::db;
use atlantis2_data::tga::Tga;
use atlantis2_engine::fpu::ftol;
use atlantis2_engine::head::{Camera, Quat, Vec3};
use atlantis2_engine::raster3d::Target;
use atlantis3_data::A3Files;

use scene::{PolyLog, Scene3, K, ROOT_OBJ, ROOT_SKEL};

/// The start cell (@ 0x00717210 / 0x00717214) and the target (@ 0x00759138
/// / 0x0075913c, BSS that nothing writes).
pub const START: (i32, i32) = (10, 6);
pub const TARGET: (i32, i32) = (0, 0);

/// The maze (@ 0x00758ec0, written by 0x0042b650): per row y, per cell x,
/// the openings N W S E (directions 0..3: y − 1, x − 1, y + 1, x + 1).
const MAZE: [&str; 7] = [
    "...E .W.E .WS. ...E .WS. ..SE .W.E .W.E .W.E .W.E .WS.",
    "...E .W.E NWS. ..S. N.S. N.S. ..SE .W.E .W.E .W.E NWS.",
    "..SE .W.E NW.E NW.. N.S. N.S. N.S. ..SE .W.E .W.E NWS.",
    "N.S. ..S. ..SE .W.E NW.. N... N.S. N..E .W.E .W.E NWS.",
    "N.SE NWSE NWS. ..SE .WSE .W.E NWS. ..SE .W.E .W.E NWS.",
    "N.S. N.S. N.S. N... N.S. ..S. N... N.S. ...E .W.. N.S.",
    "N... N... N..E .W.E NW.. N..E .W.E NW.E .W.E NW.. N...",
];

/// Whether cell (x, y) opens toward direction `d`.
pub fn open(x: i32, y: i32, d: usize) -> bool {
    if !(0..11).contains(&x) || !(0..7).contains(&y) {
        return false;
    }
    MAZE[y as usize].as_bytes()[x as usize * 5 + d] != b'.'
}

/// The 2D screen rows (@ 0x0042b650): left x, right x and y per row.
const ROW_LEFT: [i32; 7] = [88, 78, 67, 56, 45, 34, 22];
const ROW_RIGHT: [i32; 7] = [698, 705, 713, 722, 732, 743, 755];
const ROW_Y: [i32; 7] = [130, 183, 238, 296, 359, 426, 497];

/// The 2D screen position of cell (x, y) at `w` x `h` (@ 0x0042bab0).
pub fn cell_xy(x: i32, y: i32, w: i32, h: i32) -> (i32, i32) {
    let r = y as usize;
    let sx = (ROW_RIGHT[r] - ROW_LEFT[r]) * x / 10 + ROW_LEFT[r];
    let fx = K.mul(K.mul(f64::from(w), f64::from(0.00125f32)), f64::from(sx));
    let fy = K.mul(K.mul(f64::from(h), f64::from(0.001_666_666_7_f32)), f64::from(ROW_Y[r]));
    (ftol(fx), ftol(fy))
}

/// The tray zone (@ 0x0042bb30).
fn in_tray(px: i32, py: i32, w: i32, h: i32) -> bool {
    let x = px * 800 / w;
    let y = py * 600 / h;
    y > 540 && x > 345 && x < 515
}

/// A node of the unicorn's path (16 bytes @ 0x007587a0): the cell, the
/// direction taken toward the next node, the beacon headed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathNode {
    pub x: i32,
    pub y: i32,
    pub dir: i32,
    pub beacon: i32,
}

/// How a path ended (its log line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathEnd {
    Victory,
    DeadEnd,
    Loop,
}

fn neighbour(x: i32, y: i32, d: usize) -> Option<(i32, i32)> {
    if !open(x, y, d) {
        return None;
    }
    let (nx, ny) = [(x, y - 1), (x - 1, y), (x, y + 1), (x + 1, y)][d];
    ((0..11).contains(&nx) && (0..7).contains(&ny)).then_some((nx, ny))
}

/// One step (@ 0x0042df00): the next cell toward beacon `b` from (x, y),
/// (px, py) the cell left, `pdir` the direction taken from it.
fn step(bx: &[i32; 5], by: &[i32; 5], b: usize, x: i32, y: i32, prev: (i32, i32, i32)) -> Option<(i32, i32, usize)> {
    let (px, py, pdir) = prev;
    let mut cnt = 4;
    let mut n = [(-1, -1); 4];
    for (d, slot) in n.iter_mut().enumerate() {
        let c = neighbour(x, y, d);
        if c.is_none() {
            cnt -= 1;
        }
        let (nx, ny) = c.unwrap_or((-1, -1));
        if nx == px && ny == py && px != -1 && py != -1 {
            *slot = (-1, -1);
            cnt -= 1;
        } else {
            *slot = (nx, ny);
        }
    }
    let first = |n: &[(i32, i32); 4]| (0..4).find(|&i| n[i].0 != -1 && n[i].1 != -1);
    if cnt == 0 {
        return None;
    }
    if cnt == 1 {
        return first(&n).map(|i| (n[i].0, n[i].1, i));
    }
    let dist = |cx: i32, cy: i32| (cy - by[b]).abs() + (cx - bx[b]).abs();
    let mut i = 0;
    while n[i].0 == -1 {
        i += 1;
    }
    let mut best = dist(n[i].0, n[i].1);
    for j in i + 1..4 {
        if n[j].0 != -1 && n[j].1 != -1 && dist(n[j].0, n[j].1) < best {
            best = dist(n[j].0, n[j].1);
        }
    }
    for j in 0..4 {
        if n[j].0 != -1 && dist(n[j].0, n[j].1) > best {
            n[j] = (-1, -1);
            cnt -= 1;
        }
    }
    if cnt == 1 {
        return first(&n).map(|i| (n[i].0, n[i].1, i));
    }
    let dx = (x - bx[b]).abs();
    let dy = (y - by[b]).abs();
    if dx != dy && x != bx[b] && y != by[b] && best <= dist(x, y) {
        let mut i = 0;
        while n[i].0 == -1 {
            i += 1;
        }
        let skip = if dy < dx { n[i].0 == x } else { n[i].1 == y };
        if skip {
            i += 1;
            while n[i].0 == -1 {
                i += 1;
            }
        }
        return Some((n[i].0, n[i].1, i));
    }
    if pdir < 0 {
        // Never reached: the start cell has one opening.
        return first(&n).map(|i| (n[i].0, n[i].1, i));
    }
    let pd = pdir as usize;
    if n[pd].0 == -1 {
        let td = |c: (i32, i32)| (c.0 - TARGET.0).abs() + (c.1 - TARGET.1).abs();
        let mut i = 0;
        while n[i].0 == -1 {
            i += 1;
        }
        let mut k = i;
        let mut bd = td(n[i]);
        for j in i + 1..4 {
            if n[j].0 != -1 && n[j].1 != -1 && td(n[j]) < bd {
                bd = td(n[j]);
                k = j;
            }
        }
        return Some((n[k].0, n[k].1, k));
    }
    Some((n[pd].0, n[pd].1, pd))
}

/// The unicorn's path (@ 0x0042e2e0) from the start with the beacons'
/// cells (unplaced ones on their tray cells) and `placed` beacons placed.
pub fn path(beacons: &[(i32, i32); 4], placed: i32) -> (Vec<PathNode>, PathEnd) {
    let bx = [beacons[0].0, beacons[1].0, beacons[2].0, beacons[3].0, TARGET.0];
    let by = [beacons[0].1, beacons[1].1, beacons[2].1, beacons[3].1, TARGET.1];
    let mut nodes = vec![PathNode {
        x: START.0,
        y: START.1,
        dir: 0,
        beacon: 0,
    }];
    let mut b = 0usize;
    let mut loops = 0;
    let mut prev = (-1, -1, -1);
    let mut mark = (0, 0, 0);
    loop {
        let cur = *nodes.last().unwrap();
        let Some((nx, ny, nd)) = step(&bx, &by, b, cur.x, cur.y, prev) else {
            return (nodes, PathEnd::DeadEnd);
        };
        let last = nodes.len() - 1;
        nodes[last].dir = nd as i32;
        prev = (cur.x, cur.y, nd as i32);
        if cur.x == bx[b] && cur.y == by[b] {
            b = if (b as i32) < placed - 1 { b + 1 } else { 4 };
        }
        nodes.push(PathNode {
            x: nx,
            y: ny,
            dir: 0,
            beacon: b as i32,
        });
        let k = nodes.len() - 1;
        if loops == 0 {
            if nodes[..k].iter().any(|n| n.x == nx && n.y == ny && n.beacon == b as i32) {
                loops = 1;
                mark = (nx, ny, b);
            }
        } else if mark == (nx, ny, b) {
            loops += 1;
            if loops > 1 {
                return (nodes, PathEnd::Loop);
            }
        }
        if (nx, ny) == TARGET {
            return (nodes, PathEnd::Victory);
        }
    }
}

/// The direction angle of direction d (@ 0x0042c060).
fn dir_angle(d: i32) -> f64 {
    f64::from(match d {
        0 => std::f32::consts::PI,
        1 => std::f32::consts::FRAC_PI_2,
        3 => 4.712_389_f32,
        _ => 0.0,
    })
}

/// mid(a, b) as the run's code computes it: (b − a)·0.5 + a, x on the
/// x87 stack, y and z through float stores.
fn mid(a: &Vec3, b: &Vec3) -> Vec3 {
    let c = K;
    let x = c.f(c.add(c.mul(c.sub(b[0], a[0]), 0.5), a[0]));
    let dy = c.f(c.sub(b[1], a[1]));
    let dz = c.f(c.sub(b[2], a[2]));
    let y = c.f(c.add(c.f(c.mul(dy, 0.5)), a[1]));
    let z = c.f(c.add(c.f(c.mul(dz, 0.5)), a[2]));
    [x, y, z]
}

/// The beast's start point (@ 0x0042d3b0): a − (b − a)·0.5.
fn behind(a: &Vec3, b: &Vec3) -> Vec3 {
    let c = K;
    let x = c.f(c.sub(a[0], c.mul(c.sub(b[0], a[0]), 0.5)));
    let dy = c.f(c.sub(b[1], a[1]));
    let dz = c.f(c.sub(b[2], a[2]));
    let y = c.f(c.sub(a[1], c.f(c.mul(dy, 0.5))));
    let z = c.f(c.sub(a[2], c.f(c.mul(dz, 0.5))));
    [x, y, z]
}

/// p += dt · 25 (@ 0x0042c330).
fn advance(p: &mut f32, dt: f32) {
    *p = K.f32(K.add(K.mul(f64::from(dt), 25.0), f64::from(*p)));
}

/// The motion along a segment (@ 0x0042c350): mode 0 linear, 1 ease-in,
/// 2 ease-out.
fn motion(a: &Vec3, b: &Vec3, m: f32, t: f32, mode: i32) -> Vec3 {
    let c = K;
    let (m, t) = (f64::from(m), f64::from(t));
    let s = match mode {
        1 => c.div(c.mul(m, m), c.mul(t, t)),
        2 => {
            let d = c.sub(t, m);
            c.sub(1.0, c.div(c.mul(d, d), c.mul(t, t)))
        }
        _ => c.div(m, t),
    };
    let dx = c.sub(b[0], a[0]);
    let dy = c.f(c.sub(b[1], a[1]));
    let dz = c.f(c.sub(b[2], a[2]));
    let px = c.f(c.mul(dx, s));
    let py = c.f(c.mul(dy, s));
    let z = c.add(c.mul(s, dz), a[2]);
    [c.f(c.add(px, a[0])), c.f(c.add(py, a[1])), c.f(z)]
}

const TWO_PI: f64 = 6.283_185_482_025_146_5;

/// The heading blend (@ 0x0042c410) from angle a to b by m / t, the
/// shorter way; the result is left unrounded on the x87 stack.
fn blend(a: f32, b: f32, m: f32, t: f32) -> f64 {
    let c = K;
    let (a, b) = (f64::from(a), f64::from(b));
    let mut r = c.div(f64::from(m), f64::from(t));
    if !(r < 1.0 || r == 1.0) {
        r = 1.0;
    } else if r < 0.0 {
        r = 0.0;
    }
    let (d1, f2) = if a <= b {
        (c.sub(c.add(a, TWO_PI), b), b)
    } else {
        (c.sub(a, b), c.add(b, TWO_PI))
    };
    let d2 = c.f(c.sub(f2, a));
    let mut v = if d1 < d2 {
        c.sub(a, c.mul(r, d1))
    } else {
        c.add(c.mul(d2, r), a)
    };
    while v < 0.0 {
        v = c.add(v, TWO_PI);
    }
    while v > TWO_PI {
        v = c.sub(v, TWO_PI);
    }
    v
}

/// The rotation of angle `a` about the vertical axis, normalised as
/// @ 0x004bc320 does (@ 0x004fd570 then 1 / √(x² + y² + z² + w²)).
fn heading_quat(a: f32) -> Quat {
    let c = K;
    // @ 0x004fd4f0 with the axis (0, 1, 0): |axis| = 1.
    let h = c.mul(f64::from(a), 0.5);
    let s = c.div(libm::sin(h), 1.0);
    let q = [c.f(0.0 * s), c.f(s), c.f(0.0 * s), c.f(libm::cos(h))];
    let n2 = c.add(c.add(c.add(c.mul(q[0], q[0]), c.mul(q[1], q[1])), c.mul(q[2], q[2])), c.mul(q[3], q[3]));
    let inv = c.div(1.0, c.sqrt(n2));
    let invf = c.f(inv);
    [
        c.f(c.mul(invf, q[0])),
        c.f(c.mul(invf, q[1])),
        c.f(c.mul(invf, q[2])),
        c.f(c.mul(inv, q[3])),
    ]
}

/// An animated piece of the run: the unicorn (k = 0, @ 0x00758de0) or the
/// beast (k = 1, @ 0x00758e40), 0x60 bytes each.
#[derive(Clone, Debug, Default)]
pub struct Actor {
    pub cell: (i32, i32),
    pub state: i32,
    /// Animation frame (float), animation, next animation.
    pub frame: f32,
    pub anim: i32,
    pub next: i32,
    /// The node left and the node headed for (indices in the path).
    pub prev_node: i32,
    pub node: i32,
    /// The segment's ends.
    pub a: Vec3,
    pub b: Vec3,
    /// A turn's centre, radii and axis order.
    pub centre: Vec3,
    pub rx: f32,
    pub rz: f32,
    pub flag: bool,
    /// The segment's parameter (the golden's `um` / `tm`).
    pub m: f32,
    pub heading: f32,
    pub h0: f32,
    pub h1: f32,
}

/// A sound request of the board (`LoadSfx(name, 0, 1)` then `Play`,
/// `Stop`, `IsPlaying`).
pub trait Host {
    fn load(&mut self, name: &str);
    fn play(&mut self, name: &str);
    fn stop(&mut self, name: &str);
    fn playing(&mut self, name: &str) -> bool;
    /// A command the C code runs (`RunOneCommand`).
    fn command(&mut self, cmd: &str);
}

/// The frame's input: the pointer and the click bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub pointer: (i32, i32),
    pub left: bool,
    pub right: bool,
}

/// The cursor the frame draws: `Interface.spr` #40 at the pointer
/// (`AtlSpr_Aff`), or #13..24 centred on it (`AtlSpr_AffCentre`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Plain(i32, i32),
    Centred(i32, i32, i32),
}

/// The board.
#[derive(Clone, Debug)]
pub struct Board {
    pub w: i32,
    pub h: i32,
    uni: Scene3,
    beast: Scene3,
    beacons_scene: Scene3,
    camera: Camera,
    /// Cell world positions (+0x1e8): (x, y) for y = 0..6, the tray
    /// places (0..3, 7).
    cells: Vec<Vec3>,
    background: Vec<u16>,
    zbuf: Vec<u32>,
    pub beacons: [(i32, i32); 4],
    pub placed: i32,
    pub held: i32,
    pub new_beacon: bool,
    pub cursor: i32,
    pub running: bool,
    pub won: bool,
    pub lost: bool,
    pub end: bool,
    pub uni_a: Actor,
    pub beast_a: Actor,
    pub rt: f32,
    pub nodes: Vec<PathNode>,
    /// Clocks: the run's (t0 double, accumulator float, previous float),
    /// the cursor's (t0 double).
    t0: f64,
    acc: f32,
    prev: f32,
    cursor_t0: f64,
    /// The turn radii (@ 0x00758734, 0x00758730).
    r34: f32,
    r30: f32,
    /// The pointer after the frame (a drop moves it onto the cell).
    pub pointer: (i32, i32),
    /// Polygon logs of the last draw (tests).
    pub polys: Option<Vec<(&'static str, PolyLog)>>,
}

const DT: f32 = f32::from_bits(0x3d08_8889);
const TICK: f32 = 0.033_333_335;

fn read_db(files: &A3Files, name: &str) -> Option<db::Scene> {
    let data = files.read(&format!("mesh/{name}"))?;
    atlantis3_data::db::parse(&data).ok()
}

impl Board {
    /// The board object (@ 0x004b9ca0 / @ 0x004b9e40) for a `w` x `h`
    /// screen: the scenes and animations, the camera, the background and
    /// its depth buffer, the scales, the cell positions; then the start's
    /// own part (@ 0x0042e7a0) at `now`. `None` when a file is missing or
    /// the width is not one of 640, 800, 1024.
    pub fn new(files: &A3Files, w: usize, h: usize, now: f64) -> Option<Board> {
        let mut uni = Scene3::load(files, &read_db(files, "PL_Licorne.DB")?);
        for a in ["PL_LiP.DB", "PL_LiG.DB", "PL_LiP-G.DB", "PL_LIG-P.DB"] {
            uni.add_anim(&read_db(files, a)?);
        }
        let mut beast = Scene3::load(files, &read_db(files, "PL_Tribete.DB")?);
        for a in ["PL_TrC.DB", "PL_TrS.DB", "PL_TrR.DB"] {
            beast.add_anim(&read_db(files, a)?);
        }
        let mut beacons_scene = Scene3::load(files, &read_db(files, "PL_Balises.db")?);
        let cam_db = read_db(files, "PL_Dum&Cam.DB")?;
        let mut cam_scene = Scene3::load(files, &cam_db);
        let c01 = cam_db.cameras.iter().find(|c| c.name == "Camera01")?;
        let camera = Camera::sized(K, c01, w as f64, h as f64);
        if ![640, 800, 1024].contains(&w) {
            return None;
        }
        let bg = files
            .read(&format!("map/PL_Fond{w}.tga"))
            .and_then(|d| Tga::parse(&d).ok())?;
        let zbf = files
            .read(&format!("map/PL_Fond{w}.zbf"))
            .and_then(|d| atlantis3_data::zbf::Zbf::parse(&d).ok())?;
        let mut background = vec![0u16; w * h];
        for y in 0..h.min(bg.height) {
            let n = w.min(bg.width);
            background[y * w..y * w + n].copy_from_slice(&bg.pixels[y * bg.width..y * bg.width + n]);
        }
        let mut zbuf = vec![0u32; w * h];
        for y in 0..h.min(zbf.header.height as usize) {
            for x in 0..w.min(zbf.header.width as usize) {
                // @ 0x004eb6d0: z = trunc(f · 2³¹).
                let f = f64::from(zbf.depth[y * zbf.header.width as usize + x]) * 2_147_483_648.0;
                zbuf[y * w + x] = if f >= 4_294_967_296.0 { u32::MAX } else { f as u32 };
            }
        }
        // @ 0x004ba98b: the scales.
        for s in [&mut uni, &mut beast] {
            s.set_scale(ROOT_OBJ, f64::from(1.3f32));
            s.set_scale(ROOT_SKEL, f64::from(1.3f32));
        }
        for i in beacons_scene.objects.clone() {
            beacons_scene.set_scale(i, 1.5);
        }
        // @ 0x004baa64: the cell positions, `pdp_NN` and `pdp-piouNN`.
        let mut cells = vec![[0.0; 3]; 11 * 8];
        for y in 0..7 {
            for x in 0..11 {
                let n = cam_scene.object_node(&format!("pdp_{:02}", 1 + y * 11 + x))?;
                cells[x * 8 + y] = cam_scene.world_position(n);
            }
        }
        for x in 0..4 {
            let n = cam_scene.object_node(&format!("pdp-piou{:02}", x + 1))?;
            cells[x * 8 + 7] = cam_scene.world_position(n);
        }
        let mut b = Board {
            w: w as i32,
            h: h as i32,
            uni,
            beast,
            beacons_scene,
            camera,
            cells,
            background,
            zbuf,
            beacons: [(0, 7), (1, 7), (2, 7), (3, 7)],
            placed: 0,
            held: -1,
            new_beacon: false,
            cursor: 40,
            running: false,
            won: false,
            lost: false,
            end: false,
            uni_a: Actor::default(),
            beast_a: Actor::default(),
            rt: 0.0,
            nodes: Vec::new(),
            t0: 0.0,
            acc: 0.0,
            prev: 0.0,
            cursor_t0: 0.0,
            r34: 0.0,
            r30: 0.0,
            pointer: (w as i32 / 2, h as i32 / 2),
            polys: None,
        };
        // The start (@ 0x0042e7a0): the turn radii from cells (1, 1) and
        // (0, 0).
        let p11 = b.cell(1, 1);
        let p00 = b.cell(0, 0);
        b.r34 = K.f32(K.sub(p11[0], p00[0])).abs();
        b.r30 = K.f32(K.sub(p11[2], p00[2])).abs();
        b.reset(now);
        Some(b)
    }

    /// Development: the skin, the camera, the cells.
    #[doc(hidden)]
    pub fn dev_dump(&self) -> String {
        let mut out = self.uni.dev_skin("uni");
        out += &self.beast.dev_skin("tri");
        let c = &self.camera;
        let b = |x: f64| format!("{:08x}", (x as f32).to_bits());
        out += &format!(
            "cam {} {} {} {} near {} far {} slopes {} {} {} {} a {} b {}\n",
            b(c.fx), b(c.fy), b(c.cx), b(c.cy), b(c.near), b(c.far),
            b(c.slopes[0]), b(c.slopes[1]), b(c.slopes[2]), b(c.slopes[3]), b(c.a), b(c.b)
        );
        for x in 0..11 {
            for y in 0..8 {
                if y == 7 && x > 3 {
                    continue;
                }
                let p = self.cell(x, y);
                out += &format!("cell {x} {y} {} {} {}\n", b(p[0]), b(p[1]), b(p[2]));
            }
        }
        out
    }

    /// A cell's world position (@ 0x004bc680); (−1, −1) is far away.
    pub fn cell(&self, x: i32, y: i32) -> Vec3 {
        if x == -1 || y == -1 {
            return [100_000.0, 0.0, 100_000.0];
        }
        self.cells[(x * 8 + y) as usize]
    }

    fn beacon_node(&self, i: i32) -> Option<usize> {
        let name = match i {
            0 => "pion-bleu_80",
            1 => "pion-jaune_80",
            2 => "pion-rouge_80",
            3 => "pion-vert_80",
            // No name is written (the switch's default): the lookup falls
            // back to the list's first node.
            _ => "",
        };
        self.beacons_scene.object_node(name)
    }

    fn set_beacon(&mut self, i: i32, p: Vec3) {
        if let Some(n) = self.beacon_node(i) {
            self.beacons_scene.set_position(n, p);
        }
    }

    fn set_beacon_scale(&mut self, i: i32, s: f32) {
        if let Some(n) = self.beacon_node(i) {
            let s = if s == 0.0 { 1.0 } else { s };
            self.beacons_scene.set_scale(n, f64::from(s));
        }
    }

    /// The unicorn's (k = 0) or the beast's position (@ 0x004bc060 /
    /// 0x004bc0e0, @ 0x004bc120 / 0x004bc1a0): both roots of its scene.
    fn set_actor_pos(&mut self, k: usize, p: Vec3) {
        let s = if k == 0 { &mut self.uni } else { &mut self.beast };
        s.set_position(ROOT_SKEL, p);
        s.set_position(ROOT_OBJ, p);
    }

    /// The heading (@ 0x004bc320 / 0x004bc470).
    fn set_actor_heading(&mut self, k: usize, a: f32) {
        let q = heading_quat(a);
        let s = if k == 0 { &mut self.uni } else { &mut self.beast };
        s.set_rotation(ROOT_OBJ, q);
        s.set_rotation(ROOT_SKEL, q);
    }

    fn actor_pos(&mut self, k: usize) -> Vec3 {
        let s = if k == 0 { &mut self.uni } else { &mut self.beast };
        s.world_position(ROOT_OBJ)
    }

    /// The run's reset (@ 0x0042e570) at `now`.
    pub fn reset(&mut self, now: f64) {
        self.cursor = 40;
        self.running = false;
        self.lost = false;
        self.end = false;
        self.won = false;
        self.rt = 0.0;
        let u = &mut self.uni_a;
        u.cell = START;
        u.state = 0;
        u.heading = dir_angle(0) as f32;
        let p = self.cell(START.0, START.1);
        self.set_actor_pos(0, p);
        self.set_actor_heading(0, self.uni_a.heading);
        let u = &mut self.uni_a;
        u.frame = 0.0;
        u.anim = 0;
        u.next = 0;
        u.prev_node = 0;
        u.node = 0;
        let t = &mut self.beast_a;
        t.cell = (-1, -1);
        t.state = 0;
        t.heading = dir_angle(0) as f32;
        let p = self.cell(-1, -1);
        self.set_actor_pos(1, p);
        self.set_actor_heading(1, self.beast_a.heading);
        let t = &mut self.beast_a;
        t.frame = 0.0;
        t.anim = 0;
        t.next = 0;
        t.prev_node = 0;
        t.node = 0;
        for i in 0..4 {
            let (x, y) = self.beacons[i as usize];
            let p = self.cell(x, y);
            self.set_beacon(i, p);
        }
        self.t0 = now;
        self.prev = K.f32(K.add(K.sub(now, self.t0), 0.0));
        self.cursor_t0 = now;
    }

    /// The cell under the pointer (@ 0x0042bf70).
    fn cell_at(&self, px: i32, py: i32) -> Option<(i32, i32)> {
        for x in 0..11 {
            for y in 0..7 {
                let (cx, cy) = cell_xy(x, y, self.w, self.h);
                if (py - cy) * (py - cy) + (px - cx) * (px - cx) < 401 {
                    return Some((x, y));
                }
            }
        }
        let (cx, cy) = cell_xy(START.0, START.1, self.w, self.h);
        ((py - cy) * (py - cy) + (px - cx) * (px - cx) < 901).then_some(START)
    }

    /// The drop cell of the carried beacon `held` (@ 0x0042bb90); the
    /// beacon follows the pointer (scale 3.5 over a valid cell, 2.0
    /// elsewhere).
    pub fn drop_cell(&mut self, held: i32, px: i32, py: i32) -> Option<(i32, i32)> {
        let c = K;
        let a = self.cell(0, 0);
        let b = self.cell(10, 0);
        let cc = self.cell(0, 6);
        let fx = c.f(c.div(c.f(f64::from(px)), f64::from(self.w)));
        let fy = c.f(c.div(c.f(f64::from(py)), f64::from(self.h)));
        let q: Vec3 = std::array::from_fn(|i| {
            let s1 = c.f(c.mul(c.f(c.sub(cc[i], a[i])), fy));
            let s2 = c.f(c.mul(c.f(c.sub(b[i], a[i])), fx));
            c.f(c.add(c.f(c.add(s1, a[i])), s2))
        });
        let p0 = self.cell(0, 0);
        let p1 = self.cell(0, 1);
        let r: Vec3 = std::array::from_fn(|i| c.f(c.mul(f64::from(0.35f32), c.f(c.sub(p0[i], p1[i])))));
        let r2 = c.f(c.add(c.add(c.mul(r[0], r[0]), c.mul(r[1], r[1])), c.mul(r[2], r[2])));
        let mut hit = None;
        for x in 0..11 {
            for y in 0..7 {
                let p = self.cell(x, y);
                let dx = c.sub(q[0], p[0]);
                let dy = c.sub(q[1], p[1]);
                let dz = c.sub(q[2], p[2]);
                let d2 = c.add(c.add(c.mul(dz, dz), c.mul(dy, dy)), c.mul(dx, dx));
                if d2 < r2 {
                    hit = Some((x, y));
                }
            }
        }
        let valid = match hit {
            None => None,
            Some(cell) if cell == START || cell == TARGET => None,
            Some(cell) => {
                let taken = (0..self.placed).any(|i| i != held && self.beacons[i as usize] == cell);
                (!taken).then_some(cell)
            }
        };
        self.set_beacon(held, q);
        self.set_beacon_scale(held, if valid.is_some() { 3.5 } else { 2.0 });
        valid
    }

    fn sound(host: &mut dyn Host, name: &str) {
        host.load(name);
        host.play(name);
    }

    /// One mode-11 frame (@ 0x0042eb50) at `now` up to the draw; false
    /// when the board is won (the frame returns 0: the caller runs the
    /// stop and draws nothing).
    pub fn frame(&mut self, input: Input, now: f64, host: &mut dyn Host) -> bool {
        let (px, py) = input.pointer;
        self.pointer = input.pointer;
        let mut left = input.left;
        let right = input.right;
        if !self.running {
            if self.held == -1 {
                let mut plain = false;
                match self.cell_at(px, py) {
                    None => {
                        if !in_tray(px, py, self.w, self.h) || self.placed > 3 {
                            plain = true;
                        } else if left {
                            Self::sound(host, "sfx\\sound806.apc");
                            self.new_beacon = true;
                            self.held = self.placed;
                            left = false;
                        }
                    }
                    Some(cell) => {
                        let mut on_beacon = false;
                        for i in 0..self.placed {
                            if cell == self.beacons[i as usize] {
                                on_beacon = true;
                                if left {
                                    Self::sound(host, "sfx\\sound806.apc");
                                    left = false;
                                    self.held = i;
                                    let _ = left;
                                }
                            }
                        }
                        if cell == START {
                            if left {
                                Self::sound(host, "sfx\\sound803.apc");
                                let (nodes, end) = path(&self.beacons, self.placed);
                                log::debug!("Licorne: path of {} nodes, {end:?}", nodes.len());
                                self.nodes = nodes;
                                self.running = true;
                            }
                        } else if !on_beacon {
                            plain = true;
                        }
                    }
                }
                if plain {
                    self.cursor = 40;
                } else if self.cursor == 40 {
                    self.cursor = 13;
                }
            } else if !right {
                let held = self.held;
                match self.drop_cell(held, px, py) {
                    None => {}
                    Some((x, y)) if left => {
                        log::debug!("Balise #{held} placée en ({x};{y})");
                        Self::sound(host, "sfx\\sound809.apc");
                        self.beacons[held as usize] = (x, y);
                        let p = self.cell(x, y);
                        self.set_beacon(held, p);
                        self.set_beacon_scale(held, 2.0);
                        self.pointer = cell_xy(x, y, self.w, self.h);
                        self.cursor = 13;
                        self.held = -1;
                        if self.new_beacon {
                            self.new_beacon = false;
                            self.placed += 1;
                        }
                    }
                    Some(_) => {}
                }
            } else {
                Self::sound(host, "sfx\\sound805.apc");
                let held = self.held;
                self.set_beacon_scale(held, 2.0);
                let (x, y) = self.beacons[held as usize];
                let p = self.cell(x, y);
                self.set_beacon(held, p);
                self.held = -1;
                self.new_beacon = false;
                self.cursor = 40;
            }
        }
        // The cursor's clock (@ 0x0042ecd5).
        let d = K.add(K.sub(now, self.cursor_t0), 0.0);
        if d > K.div(1.0, 25.0) {
            self.cursor_t0 = now;
            if self.cursor != 40 && self.cursor != -1 {
                self.cursor += 1;
                if self.cursor > 24 {
                    self.cursor = 13;
                }
            }
        }
        self.clock(now, host);
        let s810 = "sfx\\sound810.apc";
        let playing = host.playing(s810);
        if !self.running {
            if playing {
                host.stop(s810);
            }
        } else if !playing {
            Self::sound(host, s810);
        }
        if self.running {
            if self.won {
                Self::sound(host, "sfx\\sound803.apc");
                for c in [
                    "(Flag147=0)",
                    "(Flag149=1)",
                    "(Flag146=1)",
                    "(StopMusik=1)",
                    "(SacADos=BAG_CORNE_LICORNE)",
                    "(ObjetEnMain=BAG_CORNE_LICORNE)",
                ] {
                    host.command(c);
                }
                return false;
            }
            if self.lost {
                Self::sound(host, "sfx\\sound804.apc");
                self.reset(now);
            }
        }
        true
    }

    /// The board's picture (@ 0x004bb3e0 with camera 0 and the carried
    /// beacon): the background over the whole screen, Z := the `.zbf`'s,
    /// the unicorn, the beast, the four beacons; with a beacon carried,
    /// Z cleared to 0 and that beacon drawn again (@ 0x004be580).
    pub fn draw(&mut self, screen: &mut [u16]) {
        let (w, h) = (self.w as usize, self.h as usize);
        screen[..w * h].copy_from_slice(&self.background);
        let mut z = self.zbuf.clone();
        let mut logs = self.polys.as_ref().map(|_| {
            ["uni", "tri", "bal", "held"].map(|g| (g, PolyLog::default()))
        });
        {
            let mut target = Target {
                pixels: screen,
                zbuf: &mut z,
                width: w,
                height: h,
            };
            let cam = self.camera.clone();
            self.uni
                .draw(&cam, &mut target, None, logs.as_mut().map(|l| &mut l[0].1));
            self.beast
                .draw(&cam, &mut target, None, logs.as_mut().map(|l| &mut l[1].1));
            self.beacons_scene
                .draw(&cam, &mut target, None, logs.as_mut().map(|l| &mut l[2].1));
            if self.held != -1 {
                target.zbuf.fill(0);
                if let Some(n) = self.beacon_node(self.held) {
                    let name = self.beacons_scene.nodes[n].name.clone();
                    self.beacons_scene.draw(
                        &cam,
                        &mut target,
                        Some(&name),
                        logs.as_mut().map(|l| &mut l[3].1),
                    );
                }
            }
        }
        if let Some(l) = logs {
            self.polys = Some(l.into_iter().collect());
        }
    }

    /// The cursor the frame draws (@ 0x0042f9ba): none while running or
    /// with a beacon carried; `Interface.spr` #40 at the pointer, else
    /// the animated frame centred on it.
    pub fn cursor(&self) -> Option<Cursor> {
        if self.running || self.held != -1 {
            return None;
        }
        let (x, y) = self.pointer;
        Some(if self.cursor == 40 {
            Cursor::Plain(x, y)
        } else {
            Cursor::Centred(self.cursor, x, y)
        })
    }

    /// The stop's sound part (@ 0x0042e990): sound 810 stopped if it plays.
    pub fn stop(&mut self, host: &mut dyn Host) {
        let s810 = "sfx\\sound810.apc";
        if host.playing(s810) {
            host.stop(s810);
        }
    }

    /// The 30 Hz clock (@ 0x0042daf0).
    fn clock(&mut self, now: f64, host: &mut dyn Host) {
        let c = K;
        let t = c.add(c.sub(now, self.t0), 0.0);
        self.acc = c.f32(c.add(c.sub(t, f64::from(self.prev)), f64::from(self.acc)));
        self.prev = c.f32(t);
        while self.acc >= TICK {
            self.acc = c.f32(c.sub(f64::from(self.acc), f64::from(TICK)));
            // The unicorn's animation.
            advance(&mut self.uni_a.frame, DT);
            while (self.uni.anim_frames(self.uni_a.anim) as f32) < self.uni_a.frame {
                let n = self.uni.anim_frames(self.uni_a.anim);
                self.uni_a.frame = c.f32(c.sub(f64::from(self.uni_a.frame), f64::from(n)));
                if self.uni_a.anim == 3 {
                    self.uni_a.next = 0;
                }
                self.uni_a.anim = self.uni_a.next;
            }
            self.uni.pose(self.uni_a.anim, ftol(f64::from(self.uni_a.frame)));
            // The beast's.
            advance(&mut self.beast_a.frame, DT);
            while (self.beast.anim_frames(self.beast_a.anim) as f32) < self.beast_a.frame {
                let n = self.beast.anim_frames(self.beast_a.anim);
                self.beast_a.frame = c.f32(c.sub(f64::from(self.beast_a.frame), f64::from(n)));
                if self.beast_a.anim == 2 {
                    self.beast_a.state = 5;
                    self.beast_a.next = 0;
                } else if self.beast_a.anim == 1 {
                    self.lost = true;
                }
                self.beast_a.anim = self.beast_a.next;
            }
            self.beast.pose(self.beast_a.anim, ftol(f64::from(self.beast_a.frame)));
            if self.running {
                advance(&mut self.rt, DT);
                while self.unicorn_step(DT, host) {}
                while self.beast_step(DT) {}
            }
        }
    }

    fn node_cell(&self, i: i32) -> (i32, i32) {
        self.nodes
            .get(i as usize)
            .map_or((0, 0), |n| (n.x, n.y))
    }

    fn node_pos(&self, i: i32) -> Vec3 {
        let (x, y) = self.node_cell(i);
        self.cell(x, y)
    }

    /// A turn's set-up (@ 0x0042c0a0) for actor k from p0 by p1 to p2,
    /// directions d0 then d1.
    fn turn_setup(&mut self, k: usize, p0: &Vec3, p1: &Vec3, p2: &Vec3, d0: i32, d1: i32) {
        let c = K;
        let (r34, r30) = (f64::from(self.r34), f64::from(self.r30));
        let a = if k == 0 { &mut self.uni_a } else { &mut self.beast_a };
        let h1x = c.mul(c.sub(p2[0], p1[0]), 0.5);
        let h1y = c.f(c.mul(c.f(c.sub(p2[1], p1[1])), 0.5));
        let h1z = c.f(c.mul(c.f(c.sub(p2[2], p1[2])), 0.5));
        let h0x = c.mul(c.sub(p1[0], p0[0]), 0.5);
        let h0y = c.f(c.mul(c.f(c.sub(p1[1], p0[1])), 0.5));
        let h0z = c.f(c.mul(c.f(c.sub(p1[2], p0[2])), 0.5));
        let m0y = c.f(c.add(h0y, p0[1]));
        let m0z = c.f(c.add(h0z, p0[2]));
        a.centre = [
            c.f(c.add(h1x, c.add(h0x, p0[0]))),
            c.f(c.add(h1y, m0y)),
            c.f(c.add(h1z, m0z)),
        ];
        a.h0 = dir_angle(d0) as f32;
        a.h1 = dir_angle(d1) as f32;
        let h = |r: f64, s: f64| c.f32(c.mul(r, s));
        let set = |a: &mut Actor, sx: f64, sz: f64, flag: bool| {
            a.rx = h(r34, sx);
            a.rz = h(r30, sz);
            a.flag = flag;
        };
        match (d0, d1) {
            (0, 1) => set(a, 0.5, 0.5, true),
            (0, 3) => set(a, -0.5, 0.5, true),
            (1, 0) => set(a, -0.5, -0.5, false),
            (1, 2) => set(a, -0.5, 0.5, false),
            (2, 1) => set(a, 0.5, -0.5, true),
            (2, 3) => set(a, -0.5, -0.5, true),
            (3, 0) => set(a, 0.5, -0.5, false),
            (3, 2) => set(a, 0.5, 0.5, false),
            _ => {}
        }
    }

    /// A turn's step (@ 0x0042c520) of actor k.
    fn turn_step(&mut self, k: usize) {
        let c = K;
        let a = if k == 0 { &mut self.uni_a } else { &mut self.beast_a };
        let th = c.mul(f64::from(a.m), f64::from(0.098_174_773_156_642_91_f32));
        let (u, v) = if a.flag {
            (libm::cos(th), libm::sin(th))
        } else {
            (libm::sin(th), libm::cos(th))
        };
        let x = c.f(c.add(c.mul(u, f64::from(a.rx)), a.centre[0]));
        let z = c.f(c.add(c.mul(v, f64::from(a.rz)), a.centre[2]));
        let p = [x, a.centre[1], z];
        let hd = blend(a.h0, a.h1, a.m, 16.0) as f32;
        a.heading = hd;
        self.set_actor_pos(k, p);
        self.set_actor_heading(k, hd);
    }

    /// The unicorn's state machine (@ 0x0042c6e0); true: call again.
    fn unicorn_step(&mut self, dt: f32, host: &mut dyn Host) -> bool {
        match self.uni_a.state {
            0 => {
                log::debug!("Licorne Debut");
                let u = &mut self.uni_a;
                u.anim = 2;
                u.next = 1;
                u.frame = 0.0;
                u.prev_node = 0;
                u.node = 1;
                let p0 = self.node_pos(0);
                let p1 = self.node_pos(1);
                let u = &mut self.uni_a;
                u.state = 1;
                u.a = p0;
                u.b = mid(&p0, &p1);
                u.m = 0.0;
                true
            }
            1 => {
                advance(&mut self.uni_a.m, dt);
                let u = &self.uni_a;
                let p = motion(&u.a, &u.b, u.m, 16.0, 1);
                self.set_actor_pos(0, p);
                if self.uni_a.m <= 16.0 {
                    return false;
                }
                log::debug!("Licorne Fin Depart");
                self.uni_a.m = K.f32(K.sub(f64::from(self.uni_a.m), 16.0));
                self.uni_a.state = 2;
                true
            }
            2 => {
                let un = self.uni_a.node;
                self.uni_a.cell = self.node_cell(un);
                let b = self.nodes[un as usize].beacon;
                let bc = if b == 4 {
                    TARGET
                } else {
                    self.beacons.get(b as usize).copied().unwrap_or((-9, -9))
                };
                if self.uni_a.cell == bc {
                    let far = self.cell(-1, -1);
                    self.set_beacon(b, far);
                    Self::sound(host, "sfx\\sound809.apc");
                }
                let count = self.nodes.len() as i32;
                if un >= count - 1 {
                    let p0 = self.node_pos(self.uni_a.prev_node);
                    let p1 = self.node_pos(un);
                    let u = &mut self.uni_a;
                    u.b = p1;
                    u.a = mid(&p0, &p1);
                    u.state = 3;
                    u.next = 3;
                    if self.node_cell(un) != TARGET {
                        self.end = true;
                    }
                    return true;
                }
                let pn = self.uni_a.prev_node;
                let p0 = self.node_pos(pn);
                let d0 = self.nodes[pn as usize].dir;
                let d1 = self.nodes[un as usize].dir;
                self.uni_a.prev_node = un;
                self.uni_a.node = un + 1;
                let p1 = self.node_pos(un);
                let p2 = self.node_pos(un + 1);
                self.uni_a.next = 1;
                if d0 != d1 {
                    log::debug!("Licorne Ca tourne");
                    self.turn_setup(0, &p0, &p1, &p2, d0, d1);
                    self.uni_a.state = 4;
                    return true;
                }
                log::debug!("Licorne Ligne droite");
                let u = &mut self.uni_a;
                u.state = 5;
                u.a = mid(&p0, &p1);
                u.b = mid(&p1, &p2);
                true
            }
            3 => {
                advance(&mut self.uni_a.m, dt);
                let u = &self.uni_a;
                let p = motion(&u.a, &u.b, u.m, 16.0, 2);
                self.set_actor_pos(0, p);
                if self.uni_a.m <= 16.0 {
                    return false;
                }
                log::debug!("Licorne Arret complet");
                self.uni_a.m = K.f32(K.sub(f64::from(self.uni_a.m), 16.0));
                self.uni_a.next = 0;
                if self.node_cell(self.uni_a.node) == TARGET {
                    log::debug!("Gagné !");
                    self.uni_a.state = 7;
                    self.beast_a.state = 6;
                } else {
                    log::debug!("Perdu !");
                    self.uni_a.state = 8;
                }
                false
            }
            4 => {
                advance(&mut self.uni_a.m, dt);
                self.turn_step(0);
                if self.uni_a.m <= 16.0 {
                    return false;
                }
                log::debug!("Licorne Fin course en courbe");
                self.uni_a.m = K.f32(K.sub(f64::from(self.uni_a.m), 16.0));
                self.uni_a.state = 2;
                true
            }
            5 => {
                advance(&mut self.uni_a.m, dt);
                let u = &self.uni_a;
                let p = motion(&u.a, &u.b, u.m, 20.0, 0);
                self.set_actor_pos(0, p);
                if self.uni_a.m <= 20.0 {
                    return false;
                }
                log::debug!("Licorne Fin course en ligne droite");
                self.uni_a.m = K.f32(K.sub(f64::from(self.uni_a.m), 20.0));
                self.uni_a.state = 2;
                true
            }
            7 => {
                self.won = true;
                false
            }
            _ => false,
        }
    }

    /// The beast's state machine (@ 0x0042d2d0); true: call again.
    fn beast_step(&mut self, dt: f32) -> bool {
        match self.beast_a.state {
            0 => {
                if self.rt > 12.0 {
                    let p0 = self.node_pos(0);
                    let p1 = self.node_pos(1);
                    let t = &mut self.beast_a;
                    t.cell = self.nodes.first().map_or((0, 0), |n| (n.x, n.y));
                    t.anim = 0;
                    t.next = 0;
                    t.prev_node = 0;
                    t.node = 1;
                    t.a = behind(&p0, &p1);
                    t.m = 0.0;
                    t.state = 1;
                    t.b = mid(&p0, &p1);
                    return true;
                }
            }
            1 | 5 => {
                advance(&mut self.beast_a.m, dt);
                let t = &self.beast_a;
                let p = motion(&t.a, &t.b, t.m, 20.0, 0);
                self.set_actor_pos(1, p);
                if self.beast_a.m > 20.0 {
                    self.beast_a.m = K.f32(K.sub(f64::from(self.beast_a.m), 20.0));
                    self.beast_a.state = 2;
                    return true;
                }
            }
            2 => {
                let tn = self.beast_a.node;
                self.beast_a.cell = self.node_cell(tn);
                if tn >= self.nodes.len() as i32 - 1 {
                    return false;
                }
                let pn = self.beast_a.prev_node;
                let p0 = self.node_pos(pn);
                let d0 = self.nodes[pn as usize].dir;
                let d1 = self.nodes[tn as usize].dir;
                self.beast_a.prev_node = tn;
                self.beast_a.node = tn + 1;
                let p1 = self.node_pos(tn);
                let p2 = self.node_pos(tn + 1);
                if d0 == d1 {
                    let t = &mut self.beast_a;
                    t.a = mid(&p0, &p1);
                    t.b = mid(&p1, &p2);
                    t.state = 5;
                    return true;
                }
                self.turn_setup(1, &p0, &p1, &p2, d0, d1);
                self.beast_a.state = 4;
                return true;
            }
            3 => {
                advance(&mut self.beast_a.m, dt);
                let t = &self.beast_a;
                let p = motion(&t.a, &t.b, t.m, 12.0, 0);
                self.set_actor_pos(1, p);
                let t = &self.beast_a;
                let hd = blend(t.h0, t.h1, t.m, 12.0) as f32;
                self.set_actor_heading(1, hd);
                if self.beast_a.m > 8.0 {
                    let far = self.cell(-1, -1);
                    self.set_actor_pos(0, far);
                }
                if self.beast_a.m > 16.0 {
                    self.beast_a.state = 7;
                }
            }
            4 => {
                advance(&mut self.beast_a.m, dt);
                self.turn_step(1);
                if self.beast_a.m > 16.0 {
                    self.beast_a.state = 2;
                    self.beast_a.m = K.f32(K.sub(f64::from(self.beast_a.m), 16.0));
                    return true;
                }
            }
            _ => {}
        }
        if self.end {
            self.end = false;
            let from = self.actor_pos(1);
            let to = self.actor_pos(0);
            let t = &mut self.beast_a;
            t.anim = 1;
            t.frame = 0.0;
            t.next = 0;
            t.state = 3;
            t.a = from;
            t.b = to;
            t.h0 = t.heading;
            t.m = 0.0;
            t.h1 = self.uni_a.heading;
        }
        false
    }
}
