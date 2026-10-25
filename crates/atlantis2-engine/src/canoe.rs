//! The canoe puzzle of Xibalba (`pz_pirogue`, loop type 9): a board of
//! nine squares seen from above at an angle, eight of them holding a
//! river tile the player moves and turns, the ninth the landing stage;
//! seven coloured stones lie on the tiles; a click on the stage sends the
//! hero paddling his canoe along the river the tiles make. The puzzle is
//! won when the canoe passes the seven stones in the order of the table
//! @ 0x005e2358 and reaches the end of the river.
//!
//! - The board (`pz_pirogue_init` FUN_00496a60): the squares' screen
//!   positions (FUN_004980b0), the tiles' sprites (`spr\SolPirog.spr`:
//!   id t = 4 k + r is square k's tile turned r quarters, 32 the stage's)
//!   and their depth tiles (FUN_004970f0), the waypoints of
//!   `traj_pirog.txt` (FUN_004982a0), each tile's waypoints a group named
//!   by its digit; then the shuffle (`melange_plateau` FUN_00498510).
//! - The river (`recalc_trajet` FUN_004988a0): from the first waypoint,
//!   segment by segment, each segment (from an `E` to an `S` waypoint, or
//!   back) joined to the nearest free end of another; a join across 20
//!   units or more breaks the river there (the waypoint's flag).
//! - A left click (FUN_00499470) takes a square's tile, lays it back, or
//!   swaps it with the clicked square's (their waypoint groups moved by
//!   the squares' distance, FUN_004987e0); on the stage it lays the tile
//!   back and starts the canoe (FUN_0049a2e0). A right click
//!   (FUN_00499640) turns the held or pointed tile a quarter (its
//!   waypoints rotated about the square's centre, FUN_00498660).
//! - The canoe (FUN_00499bb0, each frame while it moves) goes 1.5 units a
//!   frame towards the next waypoint; at each waypoint (FUN_0049a080) the
//!   hero's animation follows the waypoint's letter, a stone there is
//!   collected (`(NewSound=…)`, counted when in order), and the river's
//!   end without the seven stones in order stops it. The end of the river
//!   with them wins (FUN_0049a040): `finpirogue` = 1, `Pirogue_JustFinished`
//!   = 1 and a scan the first time; the loop then quits (FUN_0049a570).
//! - The frame (FUN_00498ca0): the background, the board's depth copied
//!   into the 3D Z buffer, the tiles (the pointed one at 50 %), the stone
//!   counter, the stones and the hero in 3D ([`crate::atl3d::ortho`]), the held
//!   tile at the pointer with its stones, the cursor.
//!
//! The loop has no clock of its own: everything counts frames (the
//! original's frames are unpaced; the port's are the warp's).

use atlantis2_data::canoe::{depth_tile, depth_tile_name, parse_trajectory, Waypoint};
use atlantis2_data::db::Scene;
use atlantis2_data::spr::{Mode, SprBank, SprDll, Target as SprTarget};
use atlantis2_data::tga::Tga;

use crate::atl3d::ortho::{
    axis_angle, axis_angle_matrix, draw_database, len2, look_rotation, mat_vec, normalize, vscale,
    Camera3, Database, DrawContext, DrawTarget, Library, Nodes, Records, ROOT,
};
use crate::fpu::Fpu;
use crate::warp::Variant;

/// The screen the puzzle draws on (FUN_00402960(0): the 640 x 480
/// surface).
pub const W: usize = 640;
pub const H: usize = 480;

/// The hero's animations (FUN_00496a60, in their load order @ 0x005e2468
/// .. 0x005e23fc): `C1` paddles (slot @ 0x006d7d70, the camera's database),
/// `g1` @ 0x006d2a88, `h1` @ 0x006d7f40, `a1` @ 0x006d2c58, `b1`
/// @ 0x006d7e18, `e1` @ 0x006d7c60.
pub const HERO_FILES: [&str; 6] = [
    "pz_pirog\\C1.db",
    "pz_pirog\\g1.db",
    "pz_pirog\\h1.db",
    "pz_pirog\\a1.db",
    "pz_pirog\\b1.db",
    "pz_pirog\\e1.db",
];
const C1: usize = 0;
const G1: usize = 1;
const H1: usize = 2;
const A1: usize = 3;

/// The stones (FUN_00499720: the tables @ 0x005e2bb8 .. 0x005e2c24 read
/// backwards): each one's `.db` and the waypoint it lies on.
pub const STONES: [(&str, &str); 7] = [
    ("PRbc.db", "W7A062"),
    ("PRbf.db", "W4A040"),
    ("PRjn.db", "W3A131"),
    ("PRog.db", "W2A157"),
    ("PRrg.db", "W1A175"),
    ("PRve.db", "W6A123"),
    ("PRvi.db", "W8A019"),
];

/// The order the stones must be passed in (@ 0x005e2358).
pub const STONE_ORDER: [usize; 7] = [6, 1, 0, 5, 2, 3, 4];

/// The river's first waypoint (@ 0x005e2a08) and the last the canoe
/// reaches before the end (@ 0x005e2ce0).
const FIRST_WAYPOINT: &[u8] = b"X9E1";
const LAST_WAYPOINT: &[u8] = b"W9A279";

/// The board's squares (@ 0x005e2270: column a, row b), their screen
/// position (FUN_004980b0: x = 121 a − 89 b + 270, y = 63 b + 86 + 46 a).
const SQUARES: [(i32, i32); 9] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (0, 1),
    (1, 1),
    (2, 1),
    (0, 2),
    (1, 2),
    (2, 2),
];

/// The tiles' sprite offsets (@ 0x005e2168, minus (0x10f, 0x6b),
/// FUN_00498080).
pub const TILE_OFFSETS: [(i32, i32); 33] = [
    (182, 84),
    (182, 87),
    (182, 104),
    (182, 94),
    (182, 86),
    (174, 98),
    (182, 80),
    (182, 75),
    (182, 96),
    (182, 92),
    (178, 93),
    (182, 77),
    (182, 100),
    (182, 99),
    (182, 82),
    (182, 92),
    (182, 73),
    (182, 101),
    (182, 103),
    (178, 99),
    (182, 84),
    (182, 90),
    (182, 102),
    (182, 96),
    (182, 79),
    (182, 68),
    (182, 88),
    (182, 80),
    (182, 79),
    (182, 87),
    (182, 79),
    (182, 83),
    (182, 85),
];

/// The squares' centres in the scene (FUN_00497910, @ 0x0049792d ..
/// 0x00497a45), floats.
pub const CENTRES: [[u32; 3]; 9] = [
    [0x424d62f8, 0xc25b46ed, 0xc1d346a2],
    [0x4316836f, 0xc25b46ed, 0xc1d346a2],
    [0x4379ae20, 0xc25b46ed, 0xc1d346a2],
    [0x424d62f8, 0xc319fc54, 0xc1d346a2],
    [0x4316836f, 0xc319fc54, 0xc1d346a2],
    [0x4379ae20, 0xc319fc54, 0xc1d346a2],
    [0x424d62f8, 0xc37d26ec, 0xc1d346a2],
    [0x43176522, 0xc37d26ec, 0xc1d346a2],
    [0x4379ae20, 0xc37d26ec, 0xc1d346a2],
];

/// The squares' spacing in the scene, x and y (@ 0x006d8270,
/// 0x006d2794).
pub const SPACING: [u32; 2] = [0x42c65562, 0xc2c65531];

/// The depth factors FUN_00497910 writes over its table of distance
/// ratios (@ 0x00497cb2 ..): (index, the double's bits).
const DEPTH_FACTORS: [(usize, u64); 48] = [
    (2, 0x3ff31f19e0000000),
    (3, 0x3ff200de40000000),
    (4, 0x3ff3c996c0000000),
    (5, 0x3ff5c55760000000),
    (6, 0x3ff46eb920000000),
    (7, 0x3ff69ae820000000),
    (10, 0x3ff1879f60000000),
    (13, 0x3ff4616b60000000),
    (14, 0x3ff2e176e0000000),
    (15, 0x3ff4ed9380000000),
    (16, 0x3feac636c0000000),
    (17, 0x3fed4b8e00000000),
    (19, 0x3fee4ed4e0000000),
    (20, 0x3ff0c5c9a0000000),
    (21, 0x3ff29098e0000000),
    (22, 0x3ff14d5520000000),
    (23, 0x3ff341c720000000),
    (24, 0x3fec7ef9e0000000),
    (25, 0x3fef35cc00000000),
    (26, 0x3ff1219da0000000),
    (27, 0x3ff02502e0000000),
    (28, 0x3ff1c31e00000000),
    (29, 0x3ff3c74540000000),
    (30, 0x3ff25c0ba0000000),
    (31, 0x3ff47ec9a0000000),
    (34, 0x3fee4868c0000000),
    (35, 0x3fed4a3c60000000),
    (37, 0x3ff1da1220000000),
    (38, 0x3ff06c3ca0000000),
    (39, 0x3ff2718860000000),
    (40, 0x3fe6c70220000000),
    (41, 0x3fe9059220000000),
    (42, 0x3febdb2700000000),
    (43, 0x3fea043a20000000),
    (44, 0x3fece1c360000000),
    (46, 0x3fedacbc80000000),
    (48, 0x3fe8835be0000000),
    (49, 0x3feadfefc0000000),
    (50, 0x3fedae4080000000),
    (51, 0x3febcdca80000000),
    (52, 0x3feee59f20000000),
    (53, 0x3ff1150660000000),
    (55, 0x3ff1af0060000000),
    (56, 0x3fe5447c40000000),
    (57, 0x3fe78cf9e0000000),
    (59, 0x3fe88f7ba0000000),
    (60, 0x3feaf7a4e0000000),
    (62, 0x3febae1480000000),
];

/// The camera (FUN_00496a60 @ 0x00496db8 ..): the orthographic scale
/// (@ 0x005e2158, 0x005e215c: 1.52), its position (396.495, −487.656,
/// 250.764) and the point it turns to (116.047, −106.573, −37.3446,
/// @ 0x005c6708 .. 0x005c6700).
const SCALE: [u32; 2] = [0x3fc28f5c, 0x3fc28f5c];
const CAMERA_AT: [u32; 3] = [0x43c63f5c, 0xc3f3d3f8, 0x437ac396];
const CAMERA_TO: [u32; 3] = [0x42e81810, 0xc2d52560, 0xc21560df];

/// π (@ 0x005c678c) and the constants of the turns (@ 0x005c66fc 0.75,
/// @ 0x005c66f8 −0.5, @ 0x005c62a0 0.5, @ 0x005c62bc 0.8), the step
/// (@ 0x005c6710 1.5) and the gap that breaks the river (@ 0x005c66f4
/// 20).
const PI_F: f32 = std::f32::consts::PI;
const STEP: f32 = 1.5;
const GAP: f32 = 20.0;

/// The cursor (`interfac.spr` frame 0x28, @ 0x006d806c).
pub const POINTER_FRAME: usize = 0x28;

/// A square of the board (0x18 bytes @ 0x006d2b80).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Square {
    /// The column and row.
    pub a: i32,
    pub b: i32,
    /// The screen position (+0x08, +0x0c).
    pub x: i32,
    pub y: i32,
    /// The tile id (+0x10), −1 empty.
    pub tile: i32,
    /// The waypoint group of the tile on it (+0x14): the tile's home
    /// square.
    pub group: i32,
}

/// The cursor pair (FUN_0047bef0): the pointer, or a tile in hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorPair {
    Pointer,
    Tile(i32),
    None,
}

/// What the puzzle does to the game around it.
pub trait CanoeHost {
    /// `RunOneCommand` (FUN_00481800).
    fn run_command(&mut self, cmd: &str);
    /// `Interp_ReadVar` / `Interp_WriteVar`.
    fn read_var(&mut self, name: &str) -> i32;
    fn write_var(&mut self, name: &str, value: i32);
    /// `CtrlScenarIN` (FUN_00483c90).
    fn scan(&mut self);
    /// The music state machine (FUN_00476fc0), which the loading runs.
    fn music_step(&mut self);
}

/// Why the puzzle could not start: a file missing (the original quits
/// the puzzle, FUN_0049a570) or the original's fatal error (see
/// [`Canoe::fatal`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanoeError {
    Missing(String),
    Fatal(String),
}

/// The command `pz_pirogue_init` runs first (@ 0x005e24fc, RunOneCommand
/// @ 0x00496add), before any load; the caller runs it (the scripts are
/// its).
pub const START_COMMAND: &str = "(flag103=1)";

/// The files the puzzle reads.
pub trait CanoeFiles {
    /// A file under `datas_a2\3d\`.
    fn three_d(&self, name: &str) -> Option<Vec<u8>>;
    /// A sprite bank of `spr\`.
    fn bank(&self, name: &str) -> Option<SprBank>;
}

/// The puzzle's state: the globals of the `pz_pirogue` module.
pub struct Canoe {
    pub nodes: Nodes,
    pub camera: Camera3,
    pub hero: Vec<Database>,
    pub stones: Vec<Database>,
    /// The 3D library of the databases (emptied by the init,
    /// @ 0x00496b3d, and the quit, @ 0x0049a660: FUN_0042a8d0): the
    /// heroes' meshes of the same names are those of `C1.db`.
    pub library: Library,
    /// The hero's database drawn and moved (@ 0x006d7fe4).
    pub current: usize,
    pub board: [Square; 9],
    pub waypoints: Vec<Waypoint>,
    /// @ 0x006d2770: the camera's distance to each square's centre.
    pub distances: [f32; 9],
    /// @ 0x006d8070: the depth factors, tile × 8 + square.
    pub factors: [f64; 64],
    /// @ 0x006d28f0: the depth tiles (w, h, values).
    pub depths: Vec<(usize, usize, Vec<u32>)>,
    /// The board's depth (@ 0x006d7ec0), the held tile's (@ 0x00659b58)
    /// and the 3D Z buffer (@ 0x006596a0), 640 x 480 each.
    pub zboard: Vec<u32>,
    pub zheld: Vec<u32>,
    pub zbuf: Vec<u32>,
    pub background: Vec<u16>,
    pub tiles: SprBank,
    pub counter: SprBank,
    pub pointer_bank: SprBank,
    /// @ 0x005e2150: the square under the pointer, −1 none.
    pub hovered: i32,
    /// The sprite library the tiles are drawn with (the pointed one at
    /// 50 %): `spr_p6.dll` by default, the engine's choice.
    pub spr_dll: SprDll,
    /// @ 0x005e2154: the tile in hand, −1 none; @ 0x006d7fe0 the square it
    /// came from; @ 0x005e2348, 0x005e234c the point it was taken at.
    pub held: i32,
    pub from: i32,
    pub hold_at: (i32, i32),
    /// @ 0x006d8284: the waypoint the canoe left; @ 0x006d8288 moving.
    pub index: usize,
    pub moving: bool,
    /// @ 0x006d8299: won.
    pub won: bool,
    /// @ 0x006d77fd: the letter of the current animation.
    pub letter: u8,
    /// @ 0x006d2b28: each stone still there (1) or taken (0).
    pub stone_here: [i32; 7],
    /// @ 0x006d7eb8: the stones passed in order; @ 0x006d8068 the stones
    /// passed; @ 0x006d2798 the seven in order.
    pub in_order: i32,
    pub passed: i32,
    pub all_in_order: bool,
    pub cursor: CursorPair,
    /// The polygon records' texture coordinates (@ 0x006e1520).
    pub records: Records,
    /// The routines of the CPU type.
    pub variant: Variant,
    /// The first of the original's fatal errors (the reporter
    /// FUN_00480cd0: an emergency save when a view is current, then the
    /// game quits, FUN_0049d960 @ 0x00480de9) the river's rebuild or the
    /// stones' search ran into: the message as the original formats it,
    /// or the port's own for its guards. The work stops there and the
    /// next frame returns 0; the caller stops the scripts. The game's data
    /// never reach it.
    pub fatal: Option<String>,
}

/// One quarter turn, as the float the calls pass (0x3fc90fdb).
fn quarter() -> f64 {
    f64::from(f32::from_bits(0x3fc9_0fdb))
}

fn f32v(v: [u32; 3]) -> [f64; 3] {
    v.map(|b| f64::from(f32::from_bits(b)))
}

impl Canoe {
    /// `pz_pirogue_init` (FUN_00496a60) after `(flag103=1)`: everything
    /// loaded, the camera set, the waypoints read and rebuilt, the board
    /// shuffled and its depth made.
    pub fn load(
        fpu: Fpu,
        variant: Variant,
        files: &dyn CanoeFiles,
        host: &mut dyn CanoeHost,
    ) -> Result<Canoe, CanoeError> {
        let missing = |n: &str| CanoeError::Missing(n.to_string());
        host.music_step();
        let tiles = files
            .bank("SolPirog.spr")
            .ok_or_else(|| missing("SolPirog.spr"))?;
        let counter = files
            .bank("ponPirog.spr")
            .ok_or_else(|| missing("ponPirog.spr"))?;
        let pointer_bank = files
            .bank("interfac.spr")
            .ok_or_else(|| missing("interfac.spr"))?;
        let mut nodes = Nodes::new(fpu);
        let mut texture = |name: &str| -> Option<Tga> {
            let data = files.three_d(&format!("pz_pirog\\{name}"))?;
            Tga::parse(&data).ok()
        };
        let mut hero = Vec::new();
        let mut library = Library::default();
        let mut camera = None;
        for (k, name) in HERO_FILES.iter().enumerate() {
            let data = files.three_d(name).ok_or_else(|| missing(name))?;
            let scene = Scene::parse(&data).map_err(|_| missing(name))?;
            if k == 0 {
                // The camera made with the first database
                // (@ 0x0042a9a3 .. 0x0042aa64).
                camera = Some(Camera3::orthographic(
                    &mut nodes,
                    SCALE.map(f32::from_bits),
                    W as i32,
                    H as i32,
                ));
            }
            hero.push(Database::load(
                &mut nodes,
                &mut library,
                &scene,
                &mut texture,
            ));
            if k + 1 < HERO_FILES.len() {
                host.music_step();
            }
        }
        let camera = camera.ok_or_else(|| missing(HERO_FILES[0]))?;
        let mut canoe = Canoe {
            nodes,
            camera: camera.clone(),
            hero,
            stones: Vec::new(),
            library,
            current: C1,
            board: [Square::default(); 9],
            waypoints: Vec::new(),
            distances: [0.0; 9],
            factors: [0.0; 64],
            depths: Vec::new(),
            zboard: vec![0; W * H],
            zheld: vec![0; W * H],
            zbuf: vec![0; W * H],
            background: vec![0; W * H],
            tiles,
            counter,
            pointer_bank,
            hovered: -1,
            spr_dll: SprDll::default(),
            held: -1,
            from: -1,
            hold_at: (-1, -1),
            index: 0,
            moving: false,
            won: false,
            letter: b'A',
            stone_here: [1; 7],
            in_order: 0,
            passed: 0,
            all_in_order: false,
            cursor: CursorPair::Pointer,
            records: Records::default(),
            variant,
            fatal: None,
        };
        // The depth tiles (`load_zbuffers_sol` FUN_004970f0).
        for id in 0..33 {
            host.music_step();
            let (w, h) = canoe
                .tiles
                .info(id)
                .map_or((0, 0), |i| (usize::from(i.w), usize::from(i.h)));
            let name = depth_tile_name(id);
            let data = files.three_d(&name).ok_or_else(|| missing(&name))?;
            canoe.depths.push((w, h, depth_tile(&data, w, h)));
        }
        // The squares (FUN_004980b0), the waypoints (FUN_004982a0).
        for (i, &(a, b)) in SQUARES.iter().enumerate() {
            canoe.board[i] = Square {
                a,
                b,
                x: a * 0x79 - b * 0x59 + 0x10e,
                y: b * 0x3f + 0x56 + a * 0x2e,
                tile: 4 * i as i32,
                group: i as i32,
            };
        }
        let traj = files
            .three_d(atlantis2_data::canoe::TRAJECTORY_FILE)
            .ok_or_else(|| missing("traj_pirog.txt"))?;
        canoe.waypoints = parse_trajectory(&traj, canoe.nodes.fpu == Fpu::Chop);
        // The camera: orthographic, placed and turned (@ 0x00496d9d ..
        // 0x00496f85), the hero turned upright (@ 0x00496f8f).
        let c = canoe.nodes.fpu;
        canoe.nodes.set_position(camera.node, f32v(CAMERA_AT));
        let p = canoe.nodes.position(camera.node);
        let to = f32v(CAMERA_TO);
        let d = [
            c.f(c.sub(to[0], p[0])),
            c.f(c.sub(to[1], p[1])),
            c.f(c.sub(to[2], p[2])),
        ];
        let q = look_rotation(c, &d, &[0.0, -1.0, 0.0]);
        canoe.nodes.set_rotation(camera.node, ROOT, q);
        let a1 = c.f(c.mul(f64::from(PI_F), 0.75));
        let r = axis_angle(c, &[0.0, 0.0, 1.0], a1);
        canoe.nodes.rotate_self(camera.node, r);
        let r = axis_angle(c, &[0.0, 0.0, 1.0], f64::from(f32::from_bits(0xbe2db7ae)));
        canoe.nodes.rotate_self(camera.node, r);
        canoe.nodes.translate_self(camera.node, [0.0, 15.0, 0.0]);
        let a2 = c.f(c.mul(f64::from(PI_F), -0.5));
        let r = axis_angle(c, &[1.0, 0.0, 0.0], a2);
        let slot = canoe.hero[C1].node;
        canoe.nodes.set_rotation(slot, slot, r);
        canoe.place_hero();
        canoe.measure();
        // The stones (FUN_00499720), the river (FUN_004988a0).
        for (file, _) in STONES {
            host.music_step();
            let name = format!("pz_pirog\\{file}");
            let data = files.three_d(&name).ok_or_else(|| missing(&name))?;
            let scene = Scene::parse(&data).map_err(|_| missing(&name))?;
            let db = Database::load(&mut canoe.nodes, &mut canoe.library, &scene, &mut texture);
            // @ 0x0049991d .. 0x0049996f: the slot and its first object
            // set at (0, 0, 0) of the root (the slot is there, the object
            // under it: its local position becomes 0, the file's dropped).
            canoe.nodes.set_position(db.node, [0.0; 3]);
            if let Some(o) = canoe.nodes.first_child(db.node) {
                canoe.nodes.set_position(o, [0.0; 3]);
            }
            canoe.stones.push(db);
        }
        canoe.reset_stones();
        canoe.rebuild();
        if let Some(m) = canoe.fatal.take() {
            return Err(CanoeError::Fatal(m));
        }
        // The background (@ 0x00497005 .. 0x004970bf), the shuffle, the
        // board's depth.
        let bg = files
            .three_d("pz_pirog\\fdpuzzle.tga")
            .and_then(|d| Tga::parse(&d).ok())
            .ok_or_else(|| missing("fdpuzzle.tga"))?;
        for y in 0..H.min(bg.height) {
            let n = W.min(bg.width);
            canoe.background[y * W..y * W + n]
                .copy_from_slice(&bg.pixels[y * bg.width..y * bg.width + n]);
        }
        canoe.shuffle();
        if let Some(m) = canoe.fatal.take() {
            return Err(CanoeError::Fatal(m));
        }
        host.music_step();
        canoe.make_board_depth();
        Ok(canoe)
    }

    /// The square centres and the depth factors (FUN_00497910): the
    /// camera's distance to each centre, the table of their ratios
    /// d[i] / d[j] (i, j < 8, stored as doubles), then the 48 factors the
    /// function writes over it.
    fn measure(&mut self) {
        let c = self.nodes.fpu;
        let m = |x, y| c.mul(x, y);
        let p = self.nodes.position(self.camera.node);
        for (i, ctr) in CENTRES.iter().enumerate() {
            let ctr = f32v(*ctr);
            let dx = c.sub(p[0], ctr[0]);
            let dy = c.sub(p[1], ctr[1]);
            let dz = c.sub(p[2], ctr[2]);
            let s = c.add(c.add(m(dz, dz), m(dy, dy)), m(dx, dx));
            self.distances[i] = c.f32(c.sqrt(s));
        }
        for i in 0..8 {
            for j in 0..8 {
                let q = c.div(f64::from(self.distances[i]), f64::from(self.distances[j]));
                // `fstp qword`: the 53-bit quotient as it is.
                self.factors[i * 8 + j] = q;
            }
        }
        for (k, bits) in DEPTH_FACTORS {
            self.factors[k] = f64::from_bits(bits);
        }
    }

    /// The square's tile's group of waypoints, as the digit of the
    /// waypoint names (@ 0x004986d4: group + '1').
    fn group_digit(&self, square: usize) -> u8 {
        b'1'.wrapping_add(self.board[square].group as u8)
    }

    /// FUN_00498660: the waypoints of square `sq`'s tile turned by `angle`
    /// about the square's centre (the matrix of @ 0x004c6d10 about the z
    /// axis applied to centre − point, the result added to the centre's x
    /// and y), then the river rebuilt.
    fn turn_waypoints(&mut self, sq: usize, angle: f64) {
        let c = self.nodes.fpu;
        let mt = axis_angle_matrix(c, &[0.0, 0.0, 1.0], angle);
        let digit = self.group_digit(sq);
        let ctr = f32v(CENTRES[sq]);
        for w in &mut self.waypoints {
            if w.name[1] != digit {
                continue;
            }
            let p = w.pos.map(f64::from);
            let rel = [
                c.f(c.sub(ctr[0], p[0])),
                c.f(c.sub(ctr[1], p[1])),
                c.f(c.sub(ctr[2], p[2])),
            ];
            let r = mat_vec(c, &mt, &rel);
            w.pos[0] = c.f32(c.add(r[0], ctr[0]));
            w.pos[1] = c.f32(c.add(r[1], ctr[1]));
        }
        self.rebuild();
    }

    /// FUN_004987e0: the waypoint groups of squares `p1` and `p2` (before
    /// their swap) moved by the squares' distance, then the river rebuilt.
    fn move_waypoints(&mut self, p1: usize, p2: usize) {
        if p1 == p2 {
            return;
        }
        let c = self.nodes.fpu;
        let sx = f64::from(f32::from_bits(SPACING[0]));
        let sy = f64::from(f32::from_bits(SPACING[1]));
        let (a1, b1) = SQUARES[p1];
        let (a2, b2) = SQUARES[p2];
        let dx = c.f(c.sub(c.mul(f64::from(a2), sx), c.mul(f64::from(a1), sx)));
        let dy = c.f(c.sub(c.mul(f64::from(b2), sy), c.mul(f64::from(b1), sy)));
        let g1 = self.group_digit(p1);
        let g2 = self.group_digit(p2);
        for w in &mut self.waypoints {
            let x = f64::from(w.pos[0]);
            let y = f64::from(w.pos[1]);
            if w.name[1] == g1 {
                w.pos[0] = c.f32(c.add(dx, x));
                w.pos[1] = c.f32(c.add(dy, y));
            } else if w.name[1] == g2 {
                w.pos[0] = c.f32(c.sub(x, dx));
                w.pos[1] = c.f32(c.sub(y, dy));
            }
        }
        self.rebuild();
    }

    /// The original's fatal error (FUN_00480cd0, see [`Canoe::fatal`]):
    /// logged, the first one kept.
    fn fail(&mut self, msg: String) {
        log::error!("{msg}");
        if self.fatal.is_none() {
            self.fatal = Some(msg);
        }
    }

    /// `recalc_trajet` (FUN_004988a0). Its errors are fatal in the
    /// original (FUN_00480cd0 @ 0x0049891b, 0x00498958, 0x00498a8c,
    /// 0x00498b24, which never return): the rebuild stops there.
    pub fn rebuild(&mut self) {
        let c = self.nodes.fpu;
        let n = self.waypoints.len();
        let mut out: Vec<Waypoint> = Vec::with_capacity(n);
        for w in &mut self.waypoints {
            w.flag = 0;
        }
        if n > 0 {
            let mut cur = 0usize;
            // The direction (@ [esp+0x14]).
            let mut dir: isize;
            loop {
                let w = self.waypoints[cur];
                let letter = w.name[2];
                if letter != b'E' && letter != b'S' {
                    // @ 0x00498916: the string @ 0x005e2a98.
                    self.fail(format!(
                        "recalc_trajet() err Debut de sequence {}",
                        w.name_str()
                    ));
                    return;
                }
                if cur == 0 {
                    dir = 1;
                } else {
                    let next = self.waypoints.get(cur + 1).map_or(0, |w| w.name[2]);
                    if matches!(next, b'E' | b'S' | 0) {
                        dir = -1;
                    } else if matches!(self.waypoints[cur - 1].name[2], b'E' | b'S') {
                        dir = 1;
                    } else {
                        // @ 0x00498953: the string @ 0x005e2a5c, the
                        // name and its second byte.
                        self.fail(format!(
                            "recalc_trajet() err Direction Debut de sequence {} case {}",
                            w.name_str(),
                            char::from(w.name[1])
                        ));
                        return;
                    }
                }
                let end = if letter == b'E' { b'S' } else { b'E' };
                loop {
                    let w = self.waypoints[cur];
                    out.push(w);
                    self.waypoints[cur].name[0] = b'_';
                    if w.name[2] == end {
                        break;
                    }
                    // The original's copy (@ 0x00498985 .. 0x004989b3) has
                    // no bound: a segment without its end runs on through
                    // the memory after the table. The port stops there
                    // (its own message).
                    let i = cur as isize + dir;
                    if out.len() >= n || i < 0 || i as usize >= n {
                        self.fail(format!(
                            "recalc_trajet(): segment de {} sans fin (port)",
                            w.name_str()
                        ));
                        return;
                    }
                    cur = i as usize;
                }
                let last = *out.last().unwrap_or(&Waypoint::default());
                let mut best = f64::from(9999.0f32);
                let mut found = None;
                for (k, w) in self.waypoints.iter().enumerate() {
                    if w.name[0] == b'_' || !matches!(w.name[2], b'E' | b'S') {
                        continue;
                    }
                    let dx = c.sub(f64::from(last.pos[0]), f64::from(w.pos[0]));
                    let dy = c.sub(f64::from(last.pos[1]), f64::from(w.pos[1]));
                    let dz = c.sub(f64::from(last.pos[2]), f64::from(w.pos[2]));
                    let d = c.sqrt(c.add(c.add(c.mul(dz, dz), c.mul(dy, dy)), c.mul(dx, dx)));
                    if d < best {
                        best = c.f(d);
                        found = Some(k);
                    }
                }
                if out.len() >= n {
                    break;
                }
                let Some(k) = found else {
                    // @ 0x00498a59 .. 0x00498a8c: the warning
                    // "----err-----" (FUN_00480c20), the table logged
                    // (FUN_00498b40, not reproduced), the new table over
                    // the old (the rest of it zeros, @ 0x004988a7), the
                    // table logged again, then the fatal error with the
                    // last record copied and the count (@ 0x005e2a10).
                    log::warn!("----err-----");
                    let len = out.len();
                    let name = last.name_str();
                    out.resize(n, Waypoint::default());
                    self.waypoints = out;
                    self.fail(format!(
                        "recalc_trajet(): p_trajet = NULL pour last_trajet={name} n={len} "
                    ));
                    return;
                };
                self.waypoints[k].flag = i32::from(best >= f64::from(GAP));
                cur = k;
            }
        }
        out.resize(n, Waypoint::default());
        self.waypoints = out;
        if self.waypoints.first().map(|w| &w.name[..4]) != Some(FIRST_WAYPOINT) {
            // @ 0x00498b1f: the string @ 0x005e29dc; the stones are not
            // placed.
            self.fail("recalc_trajet() : premier point déplacé!!".into());
            return;
        }
        self.place_stones();
    }

    /// The waypoint named `name` (FUN_00499b30): among the first 400;
    /// none is fatal (@ 0x00499b96, the string @ 0x005e2c9c).
    fn find(&mut self, name: &[u8]) -> Option<usize> {
        let r = self.waypoints.iter().take(400).position(|w| {
            let end = w.name.iter().position(|&c| c == 0).unwrap_or(8);
            &w.name[..end] == name
        });
        if r.is_none() {
            self.fail(format!(
                "find_mesh_name(): {} non trouvé",
                String::from_utf8_lossy(name)
            ));
        }
        r
    }

    /// FUN_00499aa0: each stone at its waypoint, then FUN_0049a290.
    fn place_stones(&mut self) {
        for (i, (_, name)) in STONES.iter().enumerate() {
            let Some(k) = self.find(name.as_bytes()) else {
                return;
            };
            let p = self.waypoints[k].pos.map(f64::from);
            if let Some(db) = self.stones.get(i) {
                self.nodes.set_position(db.node, p);
            }
        }
        self.reset_stones();
    }

    /// FUN_0049a290: the counters to 0, every stone back.
    fn reset_stones(&mut self) {
        self.in_order = 0;
        self.passed = 0;
        self.all_in_order = false;
        self.stone_here = [1; 7];
    }

    /// FUN_00498660 and FUN_004987e0 in the shuffle (`melange_plateau`
    /// FUN_00498510): squares 0, 0, 0, 1, 1, 2, 4, 4, 4, 5, 5, 6 turned a
    /// quarter each time, then the tiles of squares (1, 2), (0, 3),
    /// (7, 4), (5, 3), (6, 4) swapped.
    fn shuffle(&mut self) {
        for sq in [0usize, 0, 0, 1, 1, 2, 4, 4, 4, 5, 5, 6] {
            self.board[sq].tile = next_turn(self.board[sq].tile);
            self.turn_waypoints(sq, quarter());
        }
        for (a, b) in [(1usize, 2usize), (0, 3), (7, 4), (5, 3), (6, 4)] {
            let t = self.board[a].tile;
            self.board[a].tile = self.board[b].tile;
            self.board[b].tile = t;
            self.move_waypoints(a, b);
            let g = self.board[a].group;
            self.board[a].group = self.board[b].group;
            self.board[b].group = g;
        }
    }

    /// The depth factor of tile `tile` drawn on square `square`
    /// (@ 0x00498233 .. 0x00498259): 1 for the stage's tile.
    fn factor(&self, tile: i32, square: i32) -> f64 {
        if tile == 32 {
            1.0
        } else {
            let k = (square + (tile.div_euclid(4)) * 8) as usize;
            self.factors.get(k).copied().unwrap_or(0.0)
        }
    }

    /// FUN_00498210 / FUN_004990f0: tile `tile`'s depth written at (x, y)
    /// of `buf` (row stride 640, the rows past the right edge wrapping as
    /// the original's pointers do; a write outside the buffer, where the
    /// original writes outside its allocation, is dropped), scaled by the
    /// factor: the 32-bit values read as floats, each non-zero one
    /// f(value · factor).
    fn write_depth(
        c: Fpu,
        buf: &mut [u32],
        depth: &(usize, usize, Vec<u32>),
        x: i32,
        y: i32,
        factor: f64,
    ) {
        let (w, h, vals) = depth;
        for row in 0..*h {
            for col in 0..*w {
                let v = vals[row * w + col];
                if v == 0 {
                    continue;
                }
                let at = (y as i64 + row as i64) * W as i64 + x as i64 + col as i64;
                if at < 0 || at as usize >= buf.len() {
                    continue;
                }
                let z = c.f32(c.mul(f64::from(f32::from_bits(v)), factor));
                buf[at as usize] = z.to_bits();
            }
        }
    }

    /// `create_zbuffer_sol` (FUN_00498120): the board's depth cleared, then
    /// each square's tile's depth in the drawing order 1, 4, 2, 7, 5, 3, 8,
    /// 6, 9 (@ 0x00498144 ..).
    fn make_board_depth(&mut self) {
        let c = self.nodes.fpu;
        self.zboard.fill(0);
        for k in DRAW_ORDER {
            let sq = &self.board[k];
            if sq.tile < 0 {
                continue;
            }
            let t = sq.tile as usize;
            let (ox, oy) = TILE_OFFSETS[t];
            let (x, y) = (ox - 0x10f + sq.x, oy - 0x6b + sq.y);
            let f = self.factor(sq.tile, k as i32);
            Self::write_depth(c, &mut self.zboard, &self.depths[t], x, y, f);
        }
    }

    /// FUN_00498c50: the current database at the waypoint left, turned
    /// towards the next (FUN_0049a340).
    fn place_hero(&mut self) {
        let p = self
            .waypoints
            .get(self.index)
            .map_or([0.0; 3], |w| w.pos.map(f64::from));
        let node = self.hero[self.current].node;
        self.nodes.set_position(node, p);
        // FUN_0049a340.
        let c = self.nodes.fpu;
        let pos = self.nodes.position(node);
        let a = c.f(c.mul(-f64::from(PI_F), 0.5));
        let q = axis_angle(c, &[1.0, 0.0, 0.0], a);
        self.nodes.set_rotation(node, ROOT, q);
        let t = self
            .waypoints
            .get(self.index + 1)
            .map_or([0.0; 3], |w| w.pos.map(f64::from));
        let d = [c.f(c.sub(t[0], pos[0])), c.f(c.sub(t[1], pos[1])), 0.0];
        let q = look_rotation(c, &d, &[0.0, 0.0, 1.0]);
        self.nodes.set_rotation(node, ROOT, q);
        let a = c.f(-c.add(c.mul(f64::from(PI_F), 0.5), f64::from(0.8f32)));
        let q = axis_angle(c, &[0.0, 1.0, 0.0], a);
        self.nodes.set_rotation(node, node, q);
    }

    /// FUN_0049a2c0: stopped, the paddling animation at frame 1.
    fn stop(&mut self) {
        self.moving = false;
        self.current = C1;
        self.hero[C1].time = 1.0;
    }

    /// FUN_0049a310: a change of the board puts the canoe back.
    fn reset_canoe(&mut self) {
        self.current = C1;
        self.moving = false;
        self.index = 0;
        self.hero[C1].time = 1.0;
        self.reset_stones();
        self.place_hero();
    }

    /// FUN_0049a2e0: the canoe starts.
    fn start(&mut self) {
        self.moving = true;
        self.index = 0;
        self.current = C1;
        self.hero[C1].time = 1.0;
        self.reset_stones();
        self.place_hero();
    }

    /// FUN_00499610: the tile in hand laid back on its square.
    fn lay_back(&mut self) {
        if self.held != -1 {
            let t = self.held;
            self.held = -1;
            self.board[self.from as usize].tile = t;
            self.make_board_depth();
        }
    }

    /// The left click (FUN_00499470).
    pub fn left_click(&mut self) {
        let h = self.hovered;
        if h == 8 {
            self.lay_back();
            self.start();
            return;
        }
        if h == -1 {
            self.lay_back();
            return;
        }
        let hs = h as usize;
        if self.held == -1 {
            if self.board[hs].tile != -1 {
                self.reset_canoe();
                self.from = h;
                self.held = self.board[hs].tile;
                self.board[hs].tile = -1;
                self.hold_at = (self.board[hs].x + 0x10, self.board[hs].y + 0x36);
                self.cursor = CursorPair::Tile(self.held);
            } else {
                self.cursor = CursorPair::Pointer;
            }
            return;
        }
        if self.board[hs].tile == -1 {
            self.reset_canoe();
            let t = self.held;
            self.held = -1;
            self.board[hs].tile = t;
            self.make_board_depth();
        } else {
            self.reset_canoe();
            std::mem::swap(&mut self.board[hs].tile, &mut self.held);
            let from = self.from as usize;
            self.move_waypoints(hs, from);
            let g = self.board[hs].group;
            self.board[hs].group = self.board[from].group;
            self.board[from].group = g;
            self.hold_at = (self.board[from].x + 0x10, self.board[from].y + 0x36);
            self.board[from].tile = self.held;
            self.held = -1;
            self.make_board_depth();
        }
        self.cursor = if self.held == -1 {
            CursorPair::Pointer
        } else {
            CursorPair::Tile(self.held)
        };
    }

    /// The right click (FUN_00499640).
    pub fn right_click(&mut self) {
        if self.held == -1 {
            let h = self.hovered;
            if h != 8 && h != -1 {
                self.reset_canoe();
                let s = h as usize;
                self.board[s].tile = next_turn(self.board[s].tile);
                self.turn_waypoints(s, quarter());
                self.make_board_depth();
            }
        } else if self.held != 0x20 {
            self.reset_canoe();
            self.held = next_turn(self.held);
            self.cursor = CursorPair::Tile(self.held);
            self.turn_waypoints(self.from as usize, quarter());
        }
    }

    /// The square under (x, y) (FUN_00499270): the rows by the three lines
    /// between them and the board's edges, the columns likewise
    /// (FUN_00499430: the side of a line).
    pub fn square_at(x: i32, y: i32) -> i32 {
        let side = |x1: i32, y1: i32, x2: i32, y2: i32| -> i32 {
            let cross = (x1 - x)
                .wrapping_mul(y2 - y)
                .wrapping_sub((x2 - x).wrapping_mul(y1 - y));
            if cross < 1 {
                -1
            } else {
                1
            }
        };
        let mut row = -1;
        if side(0x10e, 0x56, 0x279, 0xe0) > 0 {
            row = 0;
            if side(0xb5, 0x95, 0x220, 0x11f) > 0 {
                row = 1;
                if side(0x5c, 0xd4, 0x1c7, 0x15e) > 0 {
                    row = 2;
                    if side(3, 0x113, 0x16e, 0x19d) > 0 {
                        row = -1;
                    }
                }
            }
        }
        if side(0x10e, 0x56, 3, 0x113) >= 0 {
            return -1;
        }
        let col = if side(0x187, 0x84, 0x7c, 0x141) >= 0 {
            0
        } else if side(0x200, 0xb2, 0xf5, 0x16f) >= 0 {
            1
        } else if side(0x279, 0xe0, 0x16e, 0x19d) >= 0 {
            2
        } else {
            return -1;
        };
        if row < 0 {
            -1
        } else {
            row * 3 + col
        }
    }

    /// One frame of loop type 9 (FUN_00498ca0) with the pointer at
    /// (x, y), the click flags and Escape; `out` the 640 x 480 screen.
    /// False when the loop ends (the quit, FUN_0049a570, is the caller's).
    pub fn frame(
        &mut self,
        host: &mut dyn CanoeHost,
        (x, y): (i32, i32),
        left: bool,
        right: bool,
        escape: bool,
        out: &mut [u16],
    ) -> bool {
        if self.fatal.is_some() {
            return false;
        }
        self.hovered = Self::square_at(x, y);
        if left {
            self.left_click();
        }
        if right {
            self.right_click();
        }
        if self.fatal.is_some() {
            // The original has quit in the click's rebuild.
            return false;
        }
        self.draw(x, y, out);
        if escape {
            return false;
        }
        if self.moving {
            self.step(host);
        }
        !self.won
    }

    /// The frame's picture (FUN_00498ca0 @ 0x00498cf8 .. 0x00498ffa).
    fn draw(&mut self, mx: i32, my: i32, out: &mut [u16]) {
        let c = self.nodes.fpu;
        out[..W * H].copy_from_slice(&self.background);
        if self.held != -1 {
            self.zheld.fill(0);
            let t = self.held as usize;
            let (ox, oy) = TILE_OFFSETS[t];
            let f = self.factor(self.held, self.from);
            Self::write_depth(
                c,
                &mut self.zheld,
                &self.depths[t],
                ox - 0x10f - 0x10 + mx,
                oy - 0x6b - 0x36 + my,
                f,
            );
        }
        self.zbuf.copy_from_slice(&self.zboard);
        // FUN_00499180: the tiles, the pointed one at 50 %; the counter.
        for k in DRAW_ORDER {
            let sq = self.board[k];
            if sq.tile < 0 {
                continue;
            }
            let t = sq.tile as usize;
            let (ox, oy) = TILE_OFFSETS[t];
            let mode = if self.hovered == k as i32 {
                Mode::Half
            } else {
                Mode::Opaque
            };
            aff(
                &self.tiles,
                out,
                t,
                Some(ox - 0x10f + sq.x),
                oy - 0x6b + sq.y,
                mode,
                self.spr_dll,
            );
        }
        if self.in_order != 0 {
            aff(
                &self.counter,
                out,
                (self.in_order - 1) as usize,
                None,
                i32::MIN,
                Mode::Opaque,
                self.spr_dll,
            );
        }
        // FUN_004999e0(from or 999, 1): the stones not on the held tile.
        let held_group = if self.held == -1 {
            999
        } else {
            self.board[self.from as usize].group
        };
        self.draw_stones(held_group, false, out);
        // The hero.
        let ctx = DrawContext {
            variant: self.variant,
        };
        {
            let mut t = DrawTarget {
                pixels: out,
                zbuf: &mut self.zbuf,
                width: W,
                height: H,
            };
            draw_database(
                &mut self.nodes,
                &self.camera,
                &self.library,
                &self.hero[self.current],
                ctx,
                &mut t,
                &mut self.records,
            );
        }
        // The held tile at the pointer (FUN_0047c060) and its stones with
        // the camera's centre moved by as much (@ 0x00498ec2 ..).
        if let CursorPair::Tile(t) = self.cursor {
            if self.held == -1 {
                return self.draw_pointer(mx, my, out);
            }
            let (ox, oy) = TILE_OFFSETS[self.held as usize];
            aff(
                &self.tiles,
                out,
                t as usize,
                Some(ox - 0x10f - 0x10 + mx),
                oy - 0x6b - 0x36 + my,
                Mode::Opaque,
                self.spr_dll,
            );
        }
        if self.held != -1 {
            let c = self.nodes.fpu;
            let (cx, cy) = (self.camera.cx, self.camera.cy);
            let ncx = c.f(c.sub(cx, f64::from(self.hold_at.0 - mx)));
            let ncy = c.f(c.sub(cy, f64::from(self.hold_at.1 - my)));
            let s = SCALE.map(|b| f64::from(f32::from_bits(b)));
            self.camera.set_centre(c, ncx, ncy);
            self.camera.set_scale(c, s[0], s[1]);
            self.draw_stones(self.board[self.from as usize].group, true, out);
            let c = self.nodes.fpu;
            self.camera.set_centre(c, cx, cy);
            self.camera.set_scale(c, s[0], s[1]);
        }
        self.draw_pointer(mx, my, out);
    }

    /// The cursor (@ 0x00498f8b): `interfac.spr` frame 0x28 with its hot
    /// spot at the pointer plus half its size.
    fn draw_pointer(&mut self, mx: i32, my: i32, out: &mut [u16]) {
        let (w, h) = self
            .pointer_bank
            .info(POINTER_FRAME)
            .map_or((0, 0), |i| (i32::from(i.w), i32::from(i.h)));
        aff(
            &self.pointer_bank,
            out,
            POINTER_FRAME,
            Some(w / 2 + mx),
            h / 2 + my,
            Mode::Opaque,
            self.spr_dll,
        );
    }

    /// `pierres_aff` (FUN_004999e0): the stones still there, those not on
    /// the tile of group `held_group` (`held` false, the 3D Z buffer) or
    /// only those (`held` true, the held tile's Z buffer).
    fn draw_stones(&mut self, held_group: i32, held: bool, out: &mut [u16]) {
        let ctx = DrawContext {
            variant: self.variant,
        };
        for (i, (_, name)) in STONES.iter().enumerate() {
            if self.stone_here[i] != 1 {
                continue;
            }
            let group = i32::from(name.as_bytes()[1]) - i32::from(b'1');
            if (group == held_group) != held {
                continue;
            }
            let zbuf = if held {
                &mut self.zheld
            } else {
                &mut self.zbuf
            };
            let mut t = DrawTarget {
                pixels: out,
                zbuf,
                width: W,
                height: H,
            };
            draw_database(
                &mut self.nodes,
                &self.camera,
                &self.library,
                &self.stones[i],
                ctx,
                &mut t,
                &mut self.records,
            );
        }
    }

    /// The canoe's step (FUN_00499bb0).
    fn step(&mut self, host: &mut dyn CanoeHost) {
        let cur_db = self.current;
        if self.hero[cur_db].delay != 0 {
            self.hero[cur_db].delay -= 1;
            return;
        }
        let c = self.nodes.fpu;
        self.hero[cur_db].next_frame(c);
        if cur_db == C1 && self.hero[C1].time == 0.0 {
            self.hero[C1].time = 6.0;
        }
        let at = self.waypoints[self.index].pos.map(f64::from);
        let pos = self.nodes.position(self.hero[self.current].node);
        loop {
            let c = self.nodes.fpu;
            let to = self
                .waypoints
                .get(self.index + 1)
                .map_or([0.0; 3], |w| w.pos.map(f64::from));
            let d = [
                c.f(c.sub(to[0], pos[0])),
                c.f(c.sub(to[1], pos[1])),
                c.f(c.sub(to[2], pos[2])),
            ];
            let n = normalize(c, &d);
            let s = vscale(c, &n, f64::from(STEP));
            let np = [
                c.f(c.add(s[0], pos[0])),
                c.f(c.add(s[1], pos[1])),
                c.f(c.add(s[2], pos[2])),
            ];
            let a = [
                c.f(c.sub(to[0], at[0])),
                c.f(c.sub(to[1], at[1])),
                c.f(c.sub(to[2], at[2])),
            ];
            let la = c.f(c.sqrt(c.f(len2(c, &a))));
            let b = [
                c.f(c.sub(np[0], at[0])),
                c.f(c.sub(np[1], at[1])),
                c.f(c.sub(np[2], at[2])),
            ];
            let lb = c.sqrt(len2(c, &b));
            let diff = c.sub(la, lb);
            if diff >= 0.0 {
                // Not there yet (@ 0x00499ec1).
                let node = self.hero[self.current].node;
                self.nodes.set_position(node, np);
                let ang = c.f(c.mul(-f64::from(PI_F), 0.5));
                let q = axis_angle(c, &[1.0, 0.0, 0.0], ang);
                self.nodes.set_rotation(node, ROOT, q);
                let dir = [c.f(c.sub(np[0], pos[0])), c.f(c.sub(np[1], pos[1])), 0.0];
                let q = look_rotation(c, &dir, &[0.0, 0.0, 1.0]);
                self.nodes.set_rotation(node, ROOT, q);
                let ang = c.f(-c.add(c.mul(f64::from(PI_F), 0.5), f64::from(0.8f32)));
                let q = axis_angle(c, &[0.0, 1.0, 0.0], ang);
                self.nodes.set_rotation(node, node, q);
                return;
            }
            let n = self.waypoints.len();
            if self.index >= n
                || self
                    .waypoints
                    .get(self.index + 2)
                    .is_some_and(|w| w.flag != 0)
            {
                self.stop();
                return;
            }
            self.arrive(host);
            self.index += 1;
            if self.index + 1 >= n {
                self.win(host);
                return;
            }
        }
    }

    /// FUN_0049a080: the waypoint reached (index + 1).
    fn arrive(&mut self, host: &mut dyn CanoeHost) {
        let next = self
            .waypoints
            .get(self.index + 1)
            .copied()
            .unwrap_or_default();
        let after = self.waypoints.get(self.index + 2).map_or(0, |w| w.name[2]);
        let bl = next.name[2];
        let cl = self.letter;
        if !(bl == b'E' || bl == b'S' || bl == cl) {
            let mut db = None;
            if cl == b'A' {
                db = match bl {
                    b'B' => Some(G1),
                    b'C' => Some(A1),
                    b'D' => Some(H1),
                    b'G' if after == b'H' => Some(G1),
                    b'H' if after == b'G' => Some(A1),
                    _ => None,
                };
            } else if (bl == b'A' && matches!(cl, b'B' | b'C' | b'D' | b'H' | b'G'))
                || (cl == b'G' && bl == b'H')
                || (cl == b'H' && bl == b'G')
            {
                // Two branches of FUN_0049a080 that choose
                // the same database.
                db = Some(C1);
            }
            if let Some(d) = db {
                if d != self.current {
                    self.switch(d);
                }
            }
            self.letter = bl;
        }
        let end = next.name.iter().position(|&c| c == 0).unwrap_or(8);
        let name = &next.name[..end];
        if let Some(i) = STONES.iter().position(|(_, n)| n.as_bytes() == name) {
            self.stone_here[i] = 0;
            if STONE_ORDER.get(self.passed as usize) == Some(&i) {
                self.in_order += 1;
            }
            self.passed += 1;
            let mut s = self.passed + 0x6a;
            if s > 0x6d {
                s -= 4;
            }
            host.run_command(&format!("(NewSound={s})"));
            if self.in_order == 7 {
                self.all_in_order = true;
            }
        }
        if name == LAST_WAYPOINT && !self.all_in_order {
            self.stop();
        }
    }

    /// FUN_0049a4e0: another animation: its frame 1, its position read
    /// and set again (its own: the original reads the new database's),
    /// 5 frames' wait.
    fn switch(&mut self, d: usize) {
        self.hero[d].time = 1.0;
        let node = self.hero[d].node;
        let p = self.nodes.position(node);
        self.nodes.set_position(node, p);
        self.current = d;
        self.hero[d].delay = 5;
    }

    /// FUN_0049a570's part in the puzzle: the cursor pair cleared
    /// (FUN_0047bef0(0, 0)); the rest is freeing.
    pub fn quit(&mut self) {
        self.cursor = CursorPair::None;
    }

    /// FUN_0049a040: the win.
    fn win(&mut self, host: &mut dyn CanoeHost) {
        host.write_var("finpirogue", 1);
        if host.read_var("Pirogue_JustFinished") == 0 {
            host.write_var("Pirogue_JustFinished", 1);
            host.scan();
        }
        self.won = true;
        self.stop();
    }
}

/// The squares in the order the tiles and their depths are drawn (the
/// tables @ 0x00498144 and in FUN_00499180, 1-based in the original).
pub const DRAW_ORDER: [usize; 9] = [0, 3, 1, 6, 4, 2, 7, 5, 8];

/// The next quarter turn of a tile id (@ 0x00499690): r = 3 → r = 0.
fn next_turn(t: i32) -> i32 {
    let r = t & (0x8000_0003u32 as i32);
    let r = if r < 0 { ((r - 1) | -4) + 1 } else { r };
    if r == 3 {
        t - 3
    } else {
        t + 1
    }
}

/// `AtlSpr_Aff` (FUN_0047bc40) on the 640 x 480 screen: the size and
/// position checks, then the sprite (`None` x: its default position).
fn aff(
    bank: &SprBank,
    out: &mut [u16],
    index: usize,
    x: Option<i32>,
    y: i32,
    mode: Mode,
    dll: SprDll,
) {
    let Some(info) = bank.info(index) else {
        log::warn!("AtlSpr_Aff num spr incorrect {index}");
        return;
    };
    if info.w > 0x280 || info.h > 0x1e0 {
        return;
    }
    if let Some(x) = x {
        if !(-0x280..=0x500).contains(&x) || !(-0x1e0..=0x3c0).contains(&y) {
            log::warn!("AtlSpr_Aff coord sprite incorrect {x} {y}");
            return;
        }
    }
    let mut t = SprTarget::screen(out, W, H);
    bank.draw_with(&mut t, x, y, index, mode, dll);
}
