//! The spider's web of Xibalba (`toile.wam`, views 198-265, chapters 35
//! and 38; the original's module `pz_araig`, @ 0x0048f450..0x00492680).
//!
//! The web is 65 nodes, views 201-265: four rings of sixteen (201-216 the
//! outer one, 249-264 the inner one; no edge between 230 and 231) and the
//! centre, 265; views 199 and 200 lead out (to 213 and 215), 198 is the
//! way in. Each node has a position (`toile.vue`, FUN_00490fb0); the
//! graph is the views' destinations (FUN_00490c90). The player moves from
//! node to node; at each of his arrivals the spiders take a step
//! (FUN_004905c0, the chase FUN_004908d0) and walk it in real time
//! (FUN_00491670: 0.02 of the edge per warp frame, turning 0.07 rad per
//! frame). A spider whose step ends on the player's node, or onto which
//! the player moves, attacks: it walks 30 % of the edge, plays its attack
//! and at the attack's end the round is lost (FUN_00491b30: film 46, the
//! prey taken back, the player put back on 199 or 200).
//!
//! The chase (FUN_004908d0), from the spider's node S to one of its
//! neighbours, the player on P: on the centre, the neighbour nearest the
//! player; when the way round is long (ring(S) + ring(P) + |ring(S) −
//! ring(P)| + the sector distance ≥ 10), or S is outside P's ring, the
//! neighbour nearest the centre (with two fixes around the gap); on P's
//! sector, the neighbour nearest the player; else the next node of its
//! ring towards P's sector (across the gap: 214 or 248, 213 or 247), but
//! the farthest from the centre when that node is the other spider's.
//! Distances in 3D from `toile.vue`, ties to the first in the view's
//! destinations order.
//!
//! Four stages (@ 0x005e1340, FUN_0048f930, from `finaraign1..3`):
//! 1 the small spider and the caterpillar on the centre (`CHENILLE`), 2
//! the small spider and the grasshopper on 251 (`SAUTERELLE`; a second
//! small spider comes out of the centre when the first reaches the outer
//! ring while the player is on it), 3 the big spider (two steps per
//! move) with the skull on the centre (and a second skull on 257), the
//! sword on 247 (struck from 247 while the spider stands on 248: film 48,
//! the spider stunned four steps) and the feather (`PLUME` laid on a node
//! lures the spider: when the step after brings it onto the player, it
//! turns back), 4 as 2 for a replay. A round is won by leaving by 199 or
//! 200 with the prey.
//!
//! The randomness: none. The module calls no random generator; every
//! choice is a nearest or farthest distance in the views' destinations
//! order.
//!
//! The 3D (the spiders, the prey, the skulls, the sword and, for the
//! feather, `bouclier.DB`) is drawn over the panorama by [`crate::atl3d`]
//! in the warp's draw (FUN_00491b30 from FUN_0045a930); a click on a
//! spider (dialogs 187-189, the strike) is picked through quads around
//! the first and the second spider (FUN_00491e40, FUN_00458fe0), built by
//! that draw.
//!
//! The arithmetic follows the original's x87 code under the control word
//! in force ([`Fpu`], 0x0e7f in the reference).

use std::sync::Arc;

use atlantis2_data::db::Scene;
use atlantis2_data::defnames;
use atlantis2_data::tga::Tga;
use atlantis2_data::Folder;

use super::Engine;
use crate::atl3d::{Model, ObjectStage, View};
use crate::camera::{axis_angle, Camera};
use crate::fpu::Fpu;
use crate::raster3d::Target;

/// The puzzle mode of the web (@ 0x0048f566).
pub const WEB_MODE: i32 = 1;

/// The views (@ 0x0049bdc5, 0x0048f9b5, 0x0048fa26, 0x0048ff88).
pub const ENTRY_VIEW: i32 = 198;
pub const FIRST_NODE: i32 = 201;
pub const CENTRE: i32 = 265;
pub const RIVER: i32 = 314;

/// π as the code reads it (@ 0x005c678c); 2π is π + π (`fadd st, st`).
const PI: f64 = std::f32::consts::PI as f64;
/// The turn speed (@ 0x00490257: 0x3d8f5c29).
const TURN: f32 = f32::from_bits(0x3d8f_5c29);
/// The walkers' heading offset (@ 0x00490280: π / 2).
pub const OFFSET: f32 = f32::from_bits(0x3fc9_0fdb);
/// The big spider's start heading (@ 0x00490411).
const BIG_HEADING: f32 = f32::from_bits(0xbfb5_c28f);
/// The step of the walk (@ 0x005c63c8), the attack's halt (@ 0x005c66e8).
const WALK: f32 = f32::from_bits(0x3ca3_d70a);
const HALT: f32 = f32::from_bits(0x3e99_999a);
/// The creatures' height above the nodes (@ 0x005e1348: 80).
const LIFT: f32 = 80.0;
/// The click quads' lift (@ 0x005c66f4: 20) and angular half sizes
/// (@ 0x005c66f0: −8π, @ 0x005c66ec: −6π, over the distance).
const QUAD_LIFT: f32 = 20.0;

/// The model slots (@ 0x006d18f8..0x006d2568), with their files and
/// scales (FUN_0048f570).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    PARmar,
    PARstby,
    Chenille,
    Sauterelle,
    ARmar,
    ARstby,
    ARmen,
    ARtoil,
    Crane,
    Bouclier,
    Macahuitl,
}

pub const SLOTS: [Slot; 11] = [
    Slot::PARmar,
    Slot::PARstby,
    Slot::Chenille,
    Slot::Sauterelle,
    Slot::ARmar,
    Slot::ARstby,
    Slot::ARmen,
    Slot::ARtoil,
    Slot::Crane,
    Slot::Bouclier,
    Slot::Macahuitl,
];

/// `Atl3D_LoadDatabase` of a slot's file read by `read` (paths under
/// `datas_a2\3d\`, `/` for `\`), at the slot's scale, under `k`.
pub fn load_model(
    k: Fpu,
    slot: Slot,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Option<Arc<Model>> {
    let scene = read(&format!("pz_araig/{}", slot.file())).and_then(|d| Scene::parse(&d).ok())?;
    Some(Arc::new(Model::load(k, &scene, slot.scale(), |map| {
        read(&format!("pz_araig/{map}")).and_then(|d| Tga::parse(&d).ok())
    })))
}

/// A slot's node as the placement FUN_00490db0 leaves it: the position,
/// the rotation, the animation time (+0x84).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub time: f32,
}

impl Slot {
    /// The slot's address (its value in a record).
    pub fn address(self) -> u32 {
        match self {
            Slot::PARmar => 0x006d_2220,
            Slot::PARstby => 0x006d_1da0,
            Slot::Chenille => 0x006d_2180,
            Slot::Sauterelle => 0x006d_18f8,
            Slot::ARmar => 0x006d_20c0,
            Slot::ARstby => 0x006d_1fe0,
            Slot::ARmen => 0x006d_24c0,
            Slot::ARtoil => 0x006d_19a0,
            Slot::Crane => 0x006d_1e90,
            Slot::Bouclier => 0x006d_1f38,
            Slot::Macahuitl => 0x006d_2568,
        }
    }

    pub fn from_address(a: u32) -> Option<Slot> {
        SLOTS.into_iter().find(|s| s.address() == a)
    }

    /// The file in `datas_a2\3d\pz_araig\` (@ 0x005e14b4..0x005e13dc).
    pub fn file(self) -> &'static str {
        match self {
            Slot::PARmar => "PARmar.DB",
            Slot::PARstby => "PARstby.DB",
            Slot::Chenille => "chenille.DB",
            Slot::Sauterelle => "sauterelle.DB",
            Slot::ARmar => "ARmar.DB",
            Slot::ARstby => "ARstby.DB",
            Slot::ARmen => "ARmen.DB",
            Slot::ARtoil => "ARtoil.DB",
            Slot::Crane => "crane.DB",
            Slot::Bouclier => "bouclier.DB",
            Slot::Macahuitl => "macahuitl.db",
        }
    }

    /// The scale given at the load (FUN_004a2f40 with @ 0x0048f584: 1.46
    /// the spiders, 3 the others, 15 the shield).
    pub fn scale(self) -> f32 {
        match self {
            Slot::PARmar
            | Slot::PARstby
            | Slot::ARmar
            | Slot::ARstby
            | Slot::ARmen
            | Slot::ARtoil => f32::from_bits(0x3fba_e148),
            Slot::Bouclier => 15.0,
            _ => 3.0,
        }
    }

    pub fn index(self) -> usize {
        SLOTS.iter().position(|&s| s == self).unwrap()
    }
}

/// A creature record (0x30 bytes @ 0x006d22c0, ten at most).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Creature {
    /// 1 the big spider, 2 a small spider, 3 the caterpillar, 4 the
    /// grasshopper, 5 a skull, 6 the feather, 7 the sword; 0 removed.
    pub kind: i32,
    pub model: Option<Slot>,
    pub from: i32,
    pub to: i32,
    /// The way along the edge, 0..1 (−1 in the frame of an arrival).
    pub t: f32,
    /// +0x14, written only by the record's set-up.
    pub unused: u32,
    /// The big spider's steps left (+0x18).
    pub steps: i32,
    /// The animation frame (+0x1c).
    pub frame: i32,
    pub heading: f32,
    pub target: f32,
    /// The turn speed, ±0.07; 0 when not turning.
    pub speed: f32,
    /// The model's heading offset (+0x2c).
    pub offset: f32,
}

impl Creature {
    /// The record's twelve words, as the original holds them.
    pub fn words(&self) -> [u32; 12] {
        [
            self.kind as u32,
            self.model.map_or(0, Slot::address),
            self.from as u32,
            self.to as u32,
            self.t.to_bits(),
            self.unused,
            self.steps as u32,
            self.frame as u32,
            self.heading.to_bits(),
            self.target.to_bits(),
            self.speed.to_bits(),
            self.offset.to_bits(),
        ]
    }

    /// A record from its twelve words.
    pub fn from_words(w: [u32; 12]) -> Creature {
        Creature {
            kind: w[0] as i32,
            model: Slot::from_address(w[1]),
            from: w[2] as i32,
            to: w[3] as i32,
            t: f32::from_bits(w[4]),
            unused: w[5],
            steps: w[6] as i32,
            frame: w[7] as i32,
            heading: f32::from_bits(w[8]),
            target: f32::from_bits(w[9]),
            speed: f32::from_bits(w[10]),
            offset: f32::from_bits(w[11]),
        }
    }

    fn walker(&self) -> bool {
        self.kind == 1 || self.kind == 2
    }
}

/// What the module needs of the game around it.
pub trait WebHost {
    /// A script variable (by name).
    fn var(&self, name: &str) -> i32;
    /// `RunOneCommand` (FUN_00481800).
    fn run(&mut self, cmd: &[u8]);
    /// `RunOneCommand_AfterDisplay` (FUN_00481790).
    fn run_after(&mut self, cmd: &[u8]);
    /// FUN_004753d0: the object of that name in the bag.
    fn in_bag(&self, name: &[u8]) -> bool;
    /// `DefName_GetIdName(1, name)`.
    fn object_id(&self, name: &[u8]) -> i32;
    /// `OBJETENMAIN`.
    fn hand(&self) -> i32;
    /// FUN_00475380: the object of that name into the hand.
    fn hold(&mut self, name: &[u8]);
    /// The current view's number (+0x10 of @ 0x006d85d4).
    fn view(&self) -> i32;
    /// Whether the current view's warp name starts with `YU` (any case).
    fn view_is_yu(&self) -> bool;
    /// The destinations of a view of the current place, in file order
    /// (FUN_004121a0, FUN_00490c90); `None` when the view is not there.
    fn destinations(&self, view: i32) -> Option<Vec<i32>>;
    /// FUN_00491df0's change: the next view (@ 0x006d85e4) and the current
    /// one (FUN_0040fe50: `LASTVUE`, `VUE`) made `view`.
    fn set_view_now(&mut self, view: i32);
    /// @ 0x006d85e8 = 1: the move completes at the end of the frame.
    fn move_pending(&mut self);
    fn mode(&self) -> i32;
    fn set_mode(&mut self, mode: i32);
    /// A file of `datas_a2\3d\` (`/` for `\`).
    fn read_3d(&self, name: &str) -> Option<Vec<u8>>;
    /// `Atl3D_LoadDatabase` of a slot's file (FUN_0042a920 with its
    /// scale, FUN_004a2f40): [`load_model`] on the file.
    fn load_model(&mut self, slot: Slot) -> Option<Arc<Model>>;
    /// The fatal error (FUN_00480cd0) and the warning (FUN_00480c20).
    fn fatal(&mut self, msg: String);
    fn warn(&mut self, msg: String);
    /// The x87 control word the module's code runs under.
    fn fpu(&self) -> Fpu;
}

/// The module's state.
#[derive(Clone, Debug)]
pub struct Web {
    /// @ 0x005e1340 (initial 3).
    pub stage: i32,
    /// @ 0x006d2668, @ 0x006d2669.
    pub loaded: bool,
    pub files: bool,
    pub creatures: Vec<Creature>,
    /// The prey and objects words (@ 0x006d2610..0x006d2624).
    pub cater: i32,
    pub hopper: i32,
    pub skull: i32,
    pub sword: i32,
    pub lure: i32,
    pub feather: i32,
    /// The second spider (@ 0x006d2628, its index @ 0x006d262c).
    pub second: i32,
    pub second_i: i32,
    /// The round lost (@ 0x006d2638), the attack (@ 0x006d263c), the
    /// facing request (@ 0x006d2640, never set but to 0).
    pub lost: i32,
    pub attack: i32,
    pub facing: i32,
    /// The prey for the hand (@ 0x006d2644) and its delay (@ 0x006d2664).
    pub prey: Option<&'static [u8]>,
    pub delay: i32,
    /// @ 0x005e1344 (initial 1, written 1 only).
    pub pickable: bool,
    /// `toile.vue` (@ 0x006d1a40, x y z per node from 201).
    pub coords: Vec<[f32; 3]>,
    /// The step's globals (@ 0x006d2090, 0x006d2170, 0x006d1f34,
    /// 0x006d1f30, 0x006d2560).
    player: [f32; 3],
    home: [f32; 3],
    pview: i32,
    pring: i32,
    psector: i32,
    /// The click quads (@ 0x006d1d60.., @ 0x006d1d50..).
    pub quads: [[[f32; 3]; 4]; 2],
    /// The models, by slot (shared: a draw copies one before posing it),
    /// and where the placements put their nodes.
    pub models: Vec<Option<Arc<Model>>>,
    pub poses: Vec<Pose>,
    /// The struck big spider's model's length (+0x90 of @ 0x006d19a0),
    /// which its free keeps: the words before `toile.vue`'s table are that
    /// slot's last ones, read as the positions of views 199 and 200.
    pub artoil_len: f32,
    /// What the placements and draws did, for the checks (cleared by
    /// their reader): (slot, position, rotation) per placement; (slot,
    /// time, opaque) per draw.
    pub placed: Vec<(Option<Slot>, [f32; 3], [f32; 4])>,
    pub drawn: Vec<(Slot, f32, bool)>,
}

impl Default for Web {
    fn default() -> Web {
        Web {
            stage: 3,
            loaded: false,
            files: false,
            creatures: Vec::new(),
            cater: 0,
            hopper: 0,
            skull: 0,
            sword: 0,
            lure: 0,
            feather: 0,
            second: 0,
            second_i: 0,
            lost: 0,
            attack: 0,
            facing: 0,
            prey: None,
            delay: 0,
            pickable: true,
            coords: Vec::new(),
            player: [0.0; 3],
            home: [0.0; 3],
            pview: 0,
            pring: 0,
            psector: 0,
            quads: [[[0.0; 3]; 4]; 2],
            models: vec![None; SLOTS.len()],
            poses: vec![Pose::default(); SLOTS.len()],
            artoil_len: 0.0,
            placed: Vec::new(),
            drawn: Vec::new(),
        }
    }
}

/// The ring of a node (FUN_00490cf0): 1 outer .. 4 inner, 5 the centre.
pub fn ring(view: i32) -> i32 {
    if view < 0xd9 {
        1
    } else if view < 0xe9 {
        2
    } else if view < 0xf9 {
        3
    } else {
        4 + i32::from(view > 0x108)
    }
}

/// A node's sector in its ring (FUN_00490d30), 0..15; 0 for the centre.
pub fn sector(view: i32, ring: i32) -> i32 {
    let s = match ring {
        1 => view - 0xc9,
        2 => view - 0xda,
        3 => view - 0xeb,
        4 => view - 0xfc,
        _ => return 0,
    };
    if s < 0 {
        s + 16
    } else {
        s
    }
}

/// FUN_004c8950 on the axis (0, 1, 0): (0·s, 1·s, 0·s, cos(a / 2)) with
/// s = sin(a / 2) / |axis| (`fsin`, `fcos`), stored as floats.
fn y_rotation(k: Fpu, angle: f32) -> [f32; 4] {
    let h = k.mul(f64::from(angle), 0.5);
    let len = k.f(k.sqrt(1.0));
    let s = k.div(libm::sin(h), len);
    [
        k.f32(0.0 * s),
        k.f32(k.mul(1.0, s)),
        k.f32(0.0 * s),
        k.f32(libm::cos(h)),
    ]
}

/// FUN_00458e50: a 3 x 3 matrix times a vector, rows in the original's
/// order of sums, stored as floats.
fn mat_vec(k: Fpu, m: &[f64; 9], v: [f64; 3]) -> [f64; 3] {
    let x = |a, b| k.mul(a, b);
    [
        k.f(k.add(k.add(x(v[2], m[2]), x(v[1], m[1])), x(v[0], m[0]))),
        k.f(k.add(k.add(x(v[0], m[3]), x(v[2], m[5])), x(v[1], m[4]))),
        k.f(k.add(k.add(x(v[0], m[6]), x(v[2], m[8])), x(v[1], m[7]))),
    ]
}

/// FUN_004c6d10 then FUN_00458e50: `v` turned about `axis` by `angle`.
fn turn(k: Fpu, axis: [f64; 3], angle: f32, v: [f64; 3]) -> [f64; 3] {
    let m = axis_angle(k, axis.map(|c| c as f32), angle);
    mat_vec(k, &m, v)
}

/// FUN_00458210: whether the direction `d` is inside the polygon `pts`:
/// for each edge (P, Q) the triple product (the cross product's x and y
/// stored as floats) not below 0.
pub fn inside(k: Fpu, d: [f32; 3], pts: &[[f32; 3]]) -> bool {
    let m = |a, b| k.mul(a, b);
    let [r0, r1, r2] = d.map(f64::from);
    for i in 0..pts.len() {
        let p = pts[i].map(f64::from);
        let q = pts[(i + 1) % pts.len()].map(f64::from);
        let c0 = k.f(k.sub(m(p[1], q[2]), m(q[1], p[2])));
        let c1 = k.f(k.sub(m(q[0], p[2]), m(q[2], p[0])));
        let c2 = k.sub(m(p[0], q[1]), m(q[0], p[1]));
        let s = k.add(k.add(m(c2, r2), m(r1, c1)), m(r0, c0));
        if s < 0.0 || s.is_nan() {
            return false;
        }
    }
    true
}

impl Web {
    /// A node's position (`toile.vue` entry `view` − 201, @ 0x006d1a40).
    /// Views 199 and 200 (the outer ring's way out, among its nodes'
    /// destinations) read the twelve bytes before the table: the end of
    /// the slot @ 0x006d19a0 (+0x88, +0x8c, +0x90 the length; +0x94,
    /// +0x98, +0x9c its valid word), as floats. Nothing else is read
    /// outside the table.
    fn pos(&self, view: i32) -> [f64; 3] {
        match view - FIRST_NODE {
            -2 => [0.0, 0.0, f64::from(self.artoil_len)],
            -1 => [
                0.0,
                0.0,
                f64::from(f32::from_bits(u32::from(self.has(Slot::ARtoil)))),
            ],
            i => usize::try_from(i)
                .ok()
                .and_then(|i| self.coords.get(i))
                .copied()
                .unwrap_or([0.0; 3])
                .map(f64::from),
        }
    }

    /// FUN_0048f930: the stage from `finaraign1..3`.
    pub fn compute_stage(&mut self, host: &dyn WebHost) {
        self.stage = if host.var("finaraign1") == 0 {
            1
        } else if host.var("finaraign2") == 0 {
            2
        } else {
            3 + i32::from(host.var("finaraign3") != 0)
        };
    }

    /// FUN_0048f530: the entry of view 198 outside the mode 1. (Its
    /// `(sacados=PLUME)` tests a word nothing writes, @ 0x006d0744.)
    pub fn entry198(&mut self, host: &mut dyn WebHost) {
        host.run(b"(sacados=NOT_EPEE)");
        self.compute_stage(host);
        self.free();
        self.load_files(host);
        host.set_mode(WEB_MODE);
    }

    /// FUN_0048f570: the stage's models: the small spider's two in stages
    /// 1, 2 and 4; the caterpillar in 1; the grasshopper in 2, and in 4
    /// while `SAUTERELLE` is not in the bag; the big spider's four, the
    /// skull, the shield and the sword in 3. A file missing quits the web.
    pub fn load_files(&mut self, host: &mut dyn WebHost) {
        let st = self.stage;
        let mut wanted = Vec::new();
        if matches!(st, 1 | 2 | 4) {
            wanted.extend([Slot::PARmar, Slot::PARstby]);
        }
        if st == 1 {
            wanted.push(Slot::Chenille);
        }
        if st == 2 || (st == 4 && !host.in_bag(b"SAUTERELLE")) {
            wanted.push(Slot::Sauterelle);
        }
        if st == 3 {
            wanted.extend([
                Slot::ARmar,
                Slot::ARstby,
                Slot::ARmen,
                Slot::ARtoil,
                Slot::Crane,
                Slot::Bouclier,
                Slot::Macahuitl,
            ]);
        }
        for slot in wanted {
            let Some(model) = host.load_model(slot) else {
                // `Atl3D_LoadDatabase`: `Fichier 3d inexistant`, a failure.
                host.warn(format!(
                    "Fichier 3d inexistant ( pz_araig\\{} )",
                    slot.file()
                ));
                self.quit(host);
                return;
            };
            if slot == Slot::ARtoil {
                self.artoil_len = model.length;
            }
            self.models[slot.index()] = Some(model);
        }
        self.files = true;
    }

    /// Whether a slot holds its model (the slot's +0x9c).
    pub fn has(&self, slot: Slot) -> bool {
        self.models[slot.index()].is_some()
    }

    /// A model's animation length (+0x90), for the checks.
    pub fn length(&self, slot: Slot) -> Option<f32> {
        self.models[slot.index()].as_ref().map(|m| m.length)
    }

    /// FUN_004911d0: every model freed, the creatures cleared.
    pub fn free(&mut self) {
        self.models.fill(None);
        self.clear();
        self.files = false;
    }

    /// FUN_00491260: the quit, in the mode 1 only.
    pub fn quit(&mut self, host: &mut dyn WebHost) {
        if host.mode() == WEB_MODE {
            self.free();
            host.set_mode(0);
            self.loaded = false;
        }
    }

    /// FUN_00490050.
    fn clear(&mut self) {
        self.creatures.clear();
        self.second = 0;
    }

    /// FUN_00490070: a creature added, its heading set up; returns the
    /// count before (its index). Nine are counted at most.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        host: &dyn WebHost,
        kind: i32,
        model: Slot,
        from: i32,
        to: i32,
        offset: f32,
        heading: f32,
    ) -> i32 {
        let n = self.creatures.len();
        let c = Creature {
            kind,
            model: Some(model),
            from,
            to,
            offset,
            heading,
            ..Creature::default()
        };
        // The record at the count, which grows only below 9 (`count + 1 <
        // 10`): a tenth record is written and its heading set up, but
        // never counted (nothing here keeps it).
        if n < 9 {
            self.creatures.push(c);
            self.heading_to(host, n, to);
        }
        n as i32
    }

    /// FUN_00490100: the creatures of a kind removed (kind 0).
    fn drop_kind(&mut self, kind: i32) {
        for c in &mut self.creatures {
            if c.kind == kind {
                c.kind = 0;
            }
        }
    }

    /// FUN_00490d90: a creature's model, its frame back to 0.
    fn anim(&mut self, i: usize, model: Slot) {
        if let Some(c) = self.creatures.get_mut(i) {
            c.model = Some(model);
            c.frame = 0;
        }
    }

    /// FUN_00490130 (towards its `to`) and FUN_00491a00 (towards a view):
    /// the target heading from the creature's node, the turn speed by the
    /// sign of the difference brought into −π..π.
    fn heading_to(&mut self, host: &dyn WebHost, i: usize, towards: i32) {
        let k = host.fpu();
        let Some(c) = self.creatures.get(i).copied() else {
            return;
        };
        let a = self.pos(towards);
        let b = self.pos(c.from);
        let dx = k.sub(a[0], b[0]);
        let dy = k.sub(a[1], b[1]);
        let target = if dx == 0.0 && dy == 0.0 {
            c.heading
        } else {
            k.f32(libm::atan2(dy, dx))
        };
        let d = wrap(k, k.sub(f64::from(target), f64::from(c.heading)));
        let speed = if d == 0.0 || d.is_nan() {
            0.0
        } else if d <= 0.0 {
            -TURN
        } else {
            TURN
        };
        let c = &mut self.creatures[i];
        c.target = target;
        c.speed = speed;
    }

    /// FUN_0048f9a0: a view entered in the mode 1.
    pub fn entry(&mut self, host: &mut dyn WebHost) {
        let view = host.view();
        if !self.loaded {
            if (FIRST_NODE..=CENTRE).contains(&view) {
                self.loaded = true;
                self.compute_stage(host);
                if !self.files {
                    self.load_files(host);
                }
                self.first_view(host);
            } else if !(host.view_is_yu() && (ENTRY_VIEW..=CENTRE).contains(&view)) {
                self.quit(host);
            }
            return;
        }
        if view == 199 || view == 200 {
            self.leave(host);
            return;
        }
        for c in &mut self.creatures {
            c.steps = if c.kind == 1 { 2 } else { 0 };
        }
        if self.facing == 1 {
            // Never: @ 0x006d2640 is written 0 only (FUN_004929d0 would
            // turn the view to 98.6°, 15°).
            self.facing = 0;
        } else {
            self.step(host);
        }
        self.put(host, None);
        if view == CENTRE {
            if self.stage == 1 {
                if self.cater == 0 {
                    self.cater = 1;
                    self.drop_kind(3);
                    host.run(b"(sacados=CHENILLE)");
                    self.prey = Some(b"CHENILLE");
                    host.run(b"(NewSound=104)");
                }
            } else if self.stage == 3 && self.skull == 0 {
                self.skull = 1;
                self.drop_kind(5);
                host.run(b"(sacados=CRANE_ARAIG2)");
                self.prey = Some(b"CRANE_ARAIG2");
            }
        }
        if view == 0xfb && (self.stage == 2 || self.stage == 4) && self.hopper == 0 {
            self.hopper = 1;
            self.drop_kind(4);
            host.run(b"(sacados=SAUTERELLE)");
            self.prey = Some(b"SAUTERELLE");
            host.run(b"(NewSound=105)");
        }
        if self.stage == 3 && view == 0xf7 && self.sword == 0 {
            self.sword = 1;
            self.drop_kind(7);
            host.run(b"(sacados=EPEE)");
            self.prey = Some(b"EPEE");
        }
    }

    /// FUN_0048f9a0 on 199 or 200 while loaded: the chain of the stages'
    /// tests, each exit recomputing the stage for the tests after it.
    fn leave(&mut self, host: &mut dyn WebHost) {
        if self.stage == 1 {
            if self.cater != 1 {
                self.loaded = false;
                return;
            }
            host.run(b"(finaraign1=1)");
            self.free();
            self.clear();
            self.compute_stage(host);
        }
        if self.stage == 2 {
            if self.hopper != 1 {
                self.loaded = false;
                return;
            }
            host.run(b"(finaraign2=1)");
            self.free();
            self.clear();
            self.compute_stage(host);
        }
        if self.stage == 4 {
            if self.hopper != 1 {
                self.loaded = false;
                return;
            }
            self.free();
            self.clear();
        }
        if self.stage == 3 {
            host.run(b"(sacados=NOT_EPEE)");
            if self.skull == 1 {
                host.run(b"(finaraign3=1)");
                self.free();
                self.clear();
                self.compute_stage(host);
                self.loaded = false;
                return;
            }
            if self.feather != 0 {
                host.run(b"(sacados=PLUME)");
            }
        }
        self.loaded = false;
    }

    /// FUN_0048fd60: the web's first view: `toile.vue`, the creatures,
    /// the click zones; in stage 3 the big spider's round's commands.
    fn first_view(&mut self, host: &mut dyn WebHost) {
        self.read_vue(host);
        self.place(host);
        self.facing = 0;
        self.attack = 0;
        self.quads = [[[0.0; 3]; 4]; 2];
        self.pickable = true;
        if self.stage == 3 {
            host.run(b"(flag92=1)");
            host.run(b"(enablesuj=CHAUVE_SOURIS,DEESSE_ARAIGNEE)");
            host.run(b"(enablesuj=PRETRESSE,DEESSE_ARAIGNEE)");
        }
    }

    /// FUN_00490fb0: `pz_araig\toile.vue`.
    pub fn read_vue(&mut self, host: &mut dyn WebHost) {
        let k = host.fpu();
        let Some(data) = host.read_3d("pz_araig/toile.vue") else {
            host.fatal("Fichier introuvable:pz_araig\\toile.vue".into());
            return;
        };
        let (coords, errors) = parse_vue(k, &data);
        for e in errors {
            host.fatal(e);
        }
        self.coords = coords;
    }

    /// FUN_00490260: the creatures of the stage.
    fn place(&mut self, host: &mut dyn WebHost) {
        self.clear();
        let view = host.view();
        match self.stage {
            1 => {
                let to = if view == 0xd7 { 0xfa } else { 0x108 };
                self.add(host, 2, Slot::PARstby, CENTRE, to, OFFSET, 0.0);
                self.add(host, 3, Slot::Chenille, CENTRE, CENTRE, 0.0, 0.0);
                self.cater = 0;
            }
            2 => {
                let to = if view == 0xd5 { 0x108 } else { 0xfa };
                self.add(host, 2, Slot::PARstby, CENTRE, to, OFFSET, 0.0);
                self.add(host, 4, Slot::Sauterelle, 0xfb, 0xfb, 0.0, 0.0);
                self.hopper = 0;
            }
            3 => {
                let to = if view == 0xd7 { 0xfa } else { 0x108 };
                self.add(host, 1, Slot::ARstby, CENTRE, to, OFFSET, BIG_HEADING);
                self.creatures[0].steps = 1;
                self.anim(0, Slot::ARmar);
                self.add(host, 5, Slot::Crane, CENTRE, CENTRE, 0.0, 0.0);
                self.add(host, 5, Slot::Crane, 0x101, 0x101, 0.0, 0.0);
                self.add(host, 7, Slot::Macahuitl, 0xf7, 0xf7, 0.0, 0.0);
                self.skull = 0;
                self.sword = 0;
                self.lure = 0;
                self.feather = 0;
            }
            4 => {
                let to = if view == 0xd5 { 0x108 } else { 0xfa };
                self.add(host, 2, Slot::PARstby, CENTRE, to, OFFSET, 0.0);
                if !host.in_bag(b"SAUTERELLE") {
                    self.add(host, 4, Slot::Sauterelle, 0xfb, 0xfb, 0.0, 0.0);
                    self.hopper = 0;
                } else {
                    self.hopper = 1;
                }
            }
            _ => {}
        }
        self.put(host, None);
    }

    /// FUN_004905c0: the spiders' step at the player's arrival (and
    /// again for the big spider's second step).
    pub fn step(&mut self, host: &mut dyn WebHost) {
        if !self.loaded {
            return;
        }
        let k = host.fpu();
        let view = host.view();
        let p = self.pos(view);
        self.player = [p[0] as f32, k.f32(-p[2]), p[1] as f32];
        let h = self.pos(CENTRE);
        self.home = [h[0] as f32, k.f32(-h[2]), h[1] as f32];
        self.pview = view;
        self.pring = ring(view);
        self.psector = sector(view, self.pring);
        let mut prev = -1;
        let mut i = 0;
        while i < self.creatures.len() {
            if self.creatures[i].walker() {
                let mut hold = false;
                if self.lure == 2 {
                    hold = true;
                    self.lure += 1;
                } else if self.lure > 2 {
                    self.lure += 1;
                }
                if self.sword > 1 {
                    if self.sword < 6 {
                        hold = true;
                    }
                    self.sword += 1;
                }
                if !hold {
                    let mut next = self.chase(host, i, -1);
                    if next == self.feather && self.feather != 0 {
                        self.lure = 1;
                    }
                    if self.second == 1 && i == 0 && (self.stage == 2 || self.stage == 4) {
                        let other = self.chase(host, self.second_i as usize, -1);
                        if next == other {
                            next = self.chase(host, 0, next);
                        }
                    }
                    if next != prev {
                        self.creatures[i].to = next;
                        self.heading_to(host, i, next);
                        prev = next;
                    }
                    if ring(next) == 1
                        && self.pring == 1
                        && self.second == 0
                        && (self.stage == 2 || self.stage == 4)
                    {
                        self.second = 2;
                    }
                    if next == self.pview {
                        self.attack = 1;
                    }
                    let c = self.creatures[i];
                    if c.kind == 1 {
                        self.creatures[i].steps -= 1;
                        self.anim(i, Slot::ARmar);
                    } else if c.kind == 2 && c.to != c.from {
                        self.anim(i, Slot::PARmar);
                    }
                }
            }
            i += 1;
        }
        if self.second == 2 {
            self.second = 1;
            let to = match self.pview {
                0xd4 => 0x107,
                0xd5 => 0x108,
                0xd6 => 0xf9,
                0xd7 => 0xfa,
                _ => 0x109,
            };
            let angle = k.f32(k.mul(PI, 0.5));
            self.second_i = self.add(host, 2, Slot::PARstby, CENTRE, to, angle, 0.0);
        }
        if self.attack == 1 && self.second != 0 {
            let mut keep = -1;
            for j in 0..self.creatures.len() {
                if self.creatures[j].kind != 2 {
                    continue;
                }
                if self.creatures[j].to == self.pview && keep == -1 {
                    keep = j as i32;
                    continue;
                }
                self.creatures[j].to = self.creatures[j].from;
                self.anim(j, Slot::PARstby);
            }
        }
    }

    /// FUN_004908d0: the next node of walker `i`, `exclude` the node not
    /// to take (the other spider's), −1 none.
    fn chase(&mut self, host: &mut dyn WebHost, i: usize, exclude: i32) -> i32 {
        let k = host.fpu();
        let m = |a, b| k.mul(a, b);
        let mut hold = false;
        if self.lure == 2 {
            hold = true;
            self.lure += 1;
        } else if self.lure > 2 {
            self.lure += 1;
        }
        if self.sword > 1 {
            if self.sword < 6 {
                hold = true;
            }
            self.sword += 1;
        }
        if hold {
            return -1;
        }
        let from = self.creatures[i].from;
        let nbs = match host.destinations(from) {
            Some(d) => d,
            None => {
                host.fatal(format!(
                    "pz_araig.cpp : find_connected_cases() : case {from} non trouvée"
                ));
                Vec::new()
            }
        };
        let (mut best_p, mut best_h, mut far) = (999_999.0f32, 999_999.0f32, 0.0f32);
        let (mut idx_p, mut idx_h, mut idx_far) = (-1, -1, -1);
        let pp = self.player.map(f64::from);
        let hh = self.home.map(f64::from);
        for &nb in nbs.iter().take(0x100) {
            let n = self.pos(nb);
            let nn = [n[0], k.f(-n[2]), n[1]];
            let dist = |o: [f64; 3]| {
                let dx = k.sub(nn[0], o[0]);
                let dy = k.sub(nn[1], o[1]);
                let dz = k.sub(nn[2], o[2]);
                k.sqrt(k.add(k.add(m(dz, dz), m(dy, dy)), m(dx, dx)))
            };
            let d = dist(pp);
            if d < f64::from(best_p) {
                best_p = k.f32(d);
                idx_p = nb;
            }
            let d = dist(hh);
            if d < f64::from(best_h) {
                best_h = k.f32(d);
                idx_h = nb;
            }
            if d > f64::from(far) {
                far = k.f32(d);
                idx_far = nb;
            }
        }
        let s = from;
        let sring = ring(s);
        let ssec = sector(s, sring);
        let mut diff = ssec - self.psector;
        let dir = if diff < -7 {
            1
        } else if diff > 0 {
            if diff > 8 {
                -1
            } else {
                1
            }
        } else {
            -1
        };
        if diff < 0 {
            diff = -diff;
        }
        if diff > 8 {
            diff = (diff - 16).abs();
        }
        if s == CENTRE {
            return idx_p;
        }
        if (10 - self.pring) - sring <= (sring - self.pring).abs() + diff {
            return idx_h;
        }
        if sring < self.pring {
            if idx_h == 0xe7 {
                if (0xe1..=0xe6).contains(&self.pview) {
                    return 0xd5;
                }
            } else if idx_h == 0xe6
                && ((0xe7..=0xe8).contains(&self.pview) || (0xd9..=0xdc).contains(&self.pview))
            {
                idx_h = 0xd6;
            }
            return idx_h;
        }
        if ssec == self.psector {
            return idx_p;
        }
        let mut n = s;
        if dir == 1 {
            n -= 1;
            n = match n {
                200 => 0xd8,
                0xd8 => 0xe8,
                0xe8 => 0xf8,
                0xf8 => 0x108,
                0xe6 => {
                    if self.pring != 1 {
                        0xf8
                    } else {
                        0xd6
                    }
                }
                _ => n,
            };
        } else {
            n += 1;
            n = match n {
                0xd9 => 0xc9,
                0xe9 => 0xd9,
                0xf9 => 0xe9,
                0x109 => 0xf9,
                0xe7 => {
                    if self.pring != 1 {
                        0xf7
                    } else {
                        0xd5
                    }
                }
                _ => n,
            };
        }
        if n == exclude && self.pring < sring {
            return idx_far;
        }
        n
    }

    /// FUN_0048fdd0: a move click towards view `v` in the mode 1; true when
    /// consumed.
    pub fn move_click(&mut self, host: &mut dyn WebHost, v: i32) -> bool {
        if self.loaded && !self.creatures.is_empty() {
            for c in &self.creatures {
                // `fcomp` with 0.0, then the target with the heading:
                // unordered counts as equal.
                let moving = !(c.t == 0.0 || c.t.is_nan());
                let turning = !(c.target == c.heading || c.target.is_nan() || c.heading.is_nan());
                if c.walker() && (moving || turning) {
                    return true;
                }
            }
            let mut hit: Option<usize> = None;
            for i in 0..self.creatures.len() {
                let c = self.creatures[i];
                if !(c.walker() && c.to == v) {
                    continue;
                }
                if self.stage == 3 && host.view() == 0xf7 && v == 0xf8 {
                    if host.hand() == host.object_id(b"EPEE") {
                        self.sword = 2;
                        self.anim(i, Slot::ARtoil);
                        self.creatures[i].frame = 0x1e;
                        host.run(b"(cinema=48)");
                    }
                } else {
                    hit = Some(i);
                }
            }
            if let Some(i) = hit {
                self.attack = 1;
                let model = match self.creatures[i].kind {
                    1 => Slot::ARmar,
                    2 => Slot::PARmar,
                    _ => return true,
                };
                self.anim(i, model);
                self.creatures[i].to = host.view();
                return true;
            }
        }
        if self.sword == 2 && v == 0xf8 {
            return true;
        }
        if v == RIVER {
            self.quit(host);
        } else if (v == 199 || v == 200) && host.view() != ENTRY_VIEW {
            host.run(b"(FuiteToileAraig=1)");
            host.run(b"(NextWarpInvalide=1)");
        }
        if host.hand() == host.object_id(b"PLUME")
            && v > 200
            && v < 0x10a
            && self.has(Slot::Bouclier)
        {
            self.add(host, 6, Slot::Bouclier, v, v, 0.0, 0.0);
            self.feather = v;
            host.run(b"(sacados=NOT_plume)");
            host.run(b"(objetenmain=0)");
            return true;
        }
        false
    }

    /// FUN_00458fe0 on a click quad at the screen's centre (W / 2, H / 2).
    fn on_quad(&self, k: Fpu, cam: &Camera, q: usize) -> bool {
        inside(k, cam.ray(cam.cx, cam.cy), &self.quads[q])
    }

    /// FUN_004913c0: whether the screen's centre is on a spider's quad
    /// (loaded, pickable; the second quad in stage 2, and in stage 4 with
    /// the second spider out).
    pub fn hit(&self, k: Fpu, cam: &Camera) -> bool {
        if !(self.pickable && self.loaded) {
            return false;
        }
        if self.on_quad(k, cam, 0) {
            return true;
        }
        (self.stage == 2 || (self.stage == 4 && self.second != 0)) && self.on_quad(k, cam, 1)
    }

    /// FUN_00491450: the zone under the screen's centre, 0 the first
    /// spider's, 1 the second's (stages 2 and 4 with the second spider
    /// out). Its caller stores it in @ 0x006d85e0, which nothing reads
    /// before the next frame overwrites it.
    pub fn hover(&self, k: Fpu, cam: &Camera) -> Option<usize> {
        if !(self.loaded && self.pickable) {
            return None;
        }
        if self.on_quad(k, cam, 0) {
            return Some(0);
        }
        if (self.stage == 2 || self.stage == 4) && self.second != 0 && self.on_quad(k, cam, 1) {
            return Some(1);
        }
        None
    }

    /// FUN_00491290: a zone click (before the zone's own, from FUN_0049ca80
    /// @ 0x0049caf2) while the click flag `click` (@ 0x006569c9) is set:
    /// true when it takes the click (the zone's own is then not run);
    /// `click` cleared when the centre is on a quad.
    pub fn zone_click(&mut self, host: &mut dyn WebHost, cam: &Camera, click: &mut bool) -> bool {
        let k = host.fpu();
        if !self.loaded || !*click || !self.hit(k, cam) || !self.pickable {
            return false;
        }
        let first = self.on_quad(k, cam, 0);
        let on = if self.stage == 2 || self.stage == 4 {
            first || (self.second != 0 && self.on_quad(k, cam, 1))
        } else {
            first
        };
        if on {
            *click = false;
        }
        match self.stage {
            1 => host.run(b"(dial=187)"),
            2 | 4 => host.run(b"(dial=188)"),
            3 => {
                if host.hand() == host.object_id(b"EPEE") {
                    self.sword = 2;
                } else {
                    host.run(b"(dial=189)");
                }
            }
            _ => {}
        }
        true
    }

    /// FUN_00491610: the tick (the puzzle mode 1, every warp frame).
    pub fn tick(&mut self, host: &mut dyn WebHost) {
        if !self.loaded {
            return;
        }
        if let Some(p) = self.prey {
            if self.delay == 0 {
                self.delay = 1;
            } else {
                host.hold(p);
                self.prey = None;
                self.delay = 0;
            }
        }
        self.moves(host);
        // FUN_00491450's zone then goes to @ 0x006d85e0, which the next
        // frame's turn step overwrites before anything reads it.
    }

    /// FUN_00491670: the walkers' frames, turns and walks.
    fn moves(&mut self, host: &mut dyn WebHost) {
        if !self.loaded {
            return;
        }
        let k = host.fpu();
        let two_pi = k.add(PI, PI);
        let mut i = 0;
        while i < self.creatures.len() {
            let c = self.creatures[i];
            if !c.walker() {
                i += 1;
                continue;
            }
            self.creatures[i].frame = c.frame + 1;
            let len = c
                .model
                .and_then(|s| self.models[s.index()].as_ref())
                .map_or(0.0, |m| m.length);
            let mut attack = self.attack;
            if f64::from((c.frame + 1) as f32) >= f64::from(len) {
                self.creatures[i].frame = 0;
                if attack >= 2 && matches!(c.model, Some(Slot::ARmen | Slot::PARmar)) {
                    attack += 1;
                    self.attack = attack;
                    if attack >= 2 {
                        attack = 0;
                        self.lost = 1;
                        self.attack = 0;
                        host.move_pending();
                    }
                }
            }
            let c = self.creatures[i];
            if c.speed != 0.0 && !c.speed.is_nan() {
                // Turning.
                let d0 = wrap(k, k.sub(f64::from(c.target), f64::from(c.heading)));
                // `fst` then `fcomp`: the sum compared before its rounding.
                let sum = k.add(f64::from(c.heading), f64::from(c.speed));
                let mut h = k.f32(sum);
                if sum < -PI || sum.is_nan() {
                    let mut x = f64::from(h);
                    loop {
                        x = k.add(two_pi, x);
                        if x >= -PI || x.is_nan() {
                            break;
                        }
                    }
                    h = k.f32(x);
                }
                if f64::from(h) > PI {
                    let mut x = f64::from(h);
                    loop {
                        x = k.sub(x, two_pi);
                        if x <= PI || x.is_nan() {
                            break;
                        }
                    }
                    h = k.f32(x);
                }
                self.creatures[i].heading = h;
                let d1 = wrap(k, k.sub(f64::from(c.target), f64::from(h)));
                let prod = k.mul(d1, d0);
                if d1 == 0.0 || d1.is_nan() || prod < 0.0 || prod.is_nan() {
                    let cc = &mut self.creatures[i];
                    cc.speed = 0.0;
                    cc.heading = cc.target;
                    if cc.from == cc.to {
                        let m = if cc.kind == 1 {
                            Slot::ARstby
                        } else {
                            Slot::PARstby
                        };
                        self.anim(i, m);
                    }
                }
            } else if c.from != c.to {
                let view = host.view();
                if self.lure == 1 && c.to == view && view != self.feather {
                    // The lure: the spider turns back.
                    self.creatures[i].to = c.from;
                    self.lure = 2;
                    self.anim(i, Slot::ARstby);
                    self.creatures[i].steps = 0;
                    self.attack = 0;
                } else {
                    if attack < 2 && self.lost == 0 {
                        self.creatures[i].t = k.f32(k.add(f64::from(c.t), f64::from(WALK)));
                    }
                    let t = self.creatures[i].t;
                    if attack == 1 && t >= HALT {
                        self.attack = 2;
                        if c.kind == 1 {
                            self.anim(i, Slot::ARmen);
                        }
                    }
                    let t = self.creatures[i].t;
                    if t >= 1.0 {
                        let cc = &mut self.creatures[i];
                        cc.from = cc.to;
                        cc.t = -1.0;
                        if !(cc.kind == 1 && cc.steps != 0) {
                            self.heading_to(host, i, view);
                        }
                    }
                }
            }
            self.put(host, Some(i));
            i += 1;
        }
        let mut j = 0;
        while j < self.creatures.len() {
            if self.creatures[j].t.to_bits() == (-1.0f32).to_bits() {
                self.creatures[j].t = 0.0;
                if self.creatures[j].kind == 1 && self.creatures[j].steps > 0 {
                    self.step(host);
                }
            }
            j += 1;
        }
    }

    /// FUN_00490db0: the models placed from the player's node: the
    /// position along the edge (x, −z + 80, y), the rotation about Y by
    /// −(heading + offset), the animation time.
    pub fn put(&mut self, host: &dyn WebHost, only: Option<usize>) {
        self.put_at(host.fpu(), host.view(), only);
    }

    /// [`Web::put`] from the player's node `view`, under `k`.
    fn put_at(&mut self, k: Fpu, view: i32, only: Option<usize>) {
        let p = self.pos(view);
        let (px, py) = (p[0], p[1]);
        let npz = k.f(-p[2]);
        let lift = f64::from(LIFT);
        let range = match only {
            Some(i) => i..i + 1,
            None => 0..self.creatures.len(),
        };
        for i in range {
            let Some(c) = self.creatures.get(i).copied() else {
                continue;
            };
            let a = self.pos(c.from);
            let b = self.pos(c.to);
            let t = f64::from(c.t);
            let ax = k.sub(a[0], px);
            let nza = k.f(-a[2]);
            let ly = k.f(k.add(k.sub(nza, npz), lift));
            let lz = k.f(k.sub(a[1], py));
            let bx = k.sub(b[0], px);
            let nzb = k.f(-b[2]);
            let by = k.f(k.add(k.sub(nzb, npz), lift));
            let bz = k.f(k.sub(b[1], py));
            let dxx = k.sub(bx, ax);
            let dyy = k.f(k.sub(by, ly));
            let dzz = k.f(k.sub(bz, lz));
            let x = k.add(k.mul(dxx, t), ax);
            let sy = k.f(k.mul(dyy, t));
            let sz = k.f(k.mul(dzz, t));
            let pos = [k.f32(x), k.f32(k.add(sy, ly)), k.f32(k.add(sz, lz))];
            let angle = k.f32(-k.add(f64::from(c.offset), f64::from(c.heading)));
            let q = y_rotation(k, angle);
            if let Some(s) = c.model.filter(|s| self.models[s.index()].is_some()) {
                self.poses[s.index()] = Pose {
                    position: pos,
                    rotation: q,
                    time: c.frame as f32,
                };
            }
            self.placed.push((c.model, pos, q));
        }
    }

    /// The creatures drawn again into `out` with `view` (the HD mode's
    /// picture, not the original's), each placed as the draw of the frame
    /// placed it, from the player's node `player`, under `k`. Nothing of
    /// the module's state changes.
    pub fn redraw(&mut self, k: Fpu, player: i32, view: &View, out: &mut Target) {
        let placed = self.placed.len();
        for i in 0..self.creatures.len() {
            self.put_at(k, player, Some(i));
            let c = self.creatures[i];
            if c.kind == 0 {
                continue;
            }
            let opaque = !matches!(c.kind, 1..=3);
            if let Some(s) = c.model {
                self.draw_slot(k, s, view, opaque, out);
            }
        }
        self.placed.truncate(placed);
    }

    /// `Atl3D_draw_DataBase` of a slot's model at its pose.
    fn draw_slot(
        &mut self,
        k: Fpu,
        slot: Slot,
        view: &View,
        opaque: bool,
        out: &mut Target,
    ) -> Vec<ObjectStage> {
        let pose = self.poses[slot.index()];
        let Some(m) = self.models[slot.index()].as_mut() else {
            return Vec::new();
        };
        let m = Arc::make_mut(m);
        m.position = pose.position;
        m.rotation = pose.rotation;
        m.time = pose.time;
        m.draw(k, view, opaque, out)
    }

    /// FUN_00492680: creature `i`'s position from the player's node, as
    /// the click quads take it.
    pub fn creature_pos(&self, k: Fpu, view: i32, i: usize) -> [f64; 3] {
        let c = self.creatures.get(i).copied().unwrap_or_default();
        let p = self.pos(view);
        let a = self.pos(c.from);
        let b = self.pos(c.to);
        let lift = f64::from(LIFT);
        let npz = k.f(-p[2]);
        let ax = k.f(k.sub(a[0], p[0]));
        let ay = k.f(k.add(k.sub(k.f(-a[2]), npz), lift));
        let az = k.f(k.sub(a[1], p[1]));
        let bx = k.sub(b[0], p[0]);
        let by = k.f(k.add(k.sub(k.f(-b[2]), npz), lift));
        let bz = k.f(k.sub(b[1], p[1]));
        let dx = k.sub(bx, ax);
        let dy = k.f(k.sub(by, ay));
        let dz = k.f(k.sub(bz, az));
        let t = f64::from(c.t);
        let sy = k.f(k.mul(dy, t));
        let sz = k.f(k.mul(dz, t));
        [
            k.f(k.add(k.mul(dx, t), ax)),
            k.f(k.add(sy, ay)),
            k.f(k.add(sz, az)),
        ]
    }

    /// FUN_00491e40: the click quads, around the first creature and, in
    /// stage 2 (and 4 with it), the second spider: the creature's
    /// direction (20 lower) turned by ±a about the axes across it, a =
    /// −8π (stage 3) or −6π over its distance.
    pub fn build_quads(&mut self, k: Fpu, view: i32) {
        let p = self.creature_pos(k, view, 0);
        let v = [p[0], k.f(k.sub(p[1], f64::from(QUAD_LIFT))), p[2]];
        let n2 = k.add(
            k.add(k.mul(v[0], v[0]), k.mul(v[1], v[1])),
            k.mul(v[2], v[2]),
        );
        let factor = if self.stage == 3 { -8.0 } else { -6.0 };
        let angle = k.f32(k.div(k.mul(PI, factor), k.sqrt(n2)));
        let quad = |k: Fpu, v: [f64; 3]| {
            let right = [k.f(k.sub(0.0, v[2])), 0.0, k.f(k.sub(v[0], 0.0))];
            let up = [
                k.f(k.sub(k.mul(right[2], v[1]), k.mul(right[1], v[2]))),
                k.f(k.sub(k.mul(v[2], right[0]), k.mul(right[2], v[0]))),
                k.f(k.sub(k.mul(right[1], v[0]), k.mul(v[1], right[0]))),
            ];
            (right, up)
        };
        let neg = |a: [f64; 3]| a.map(|c| -c);
        let (right, up) = quad(k, v);
        let a1 = turn(k, right, angle, turn(k, neg(up), angle, v));
        let a2 = turn(k, right, angle, turn(k, up, angle, v));
        let a3 = turn(k, neg(right), angle, turn(k, up, angle, v));
        let a4 = turn(k, neg(right), angle, turn(k, neg(up), angle, v));
        self.quads[0] = [a1, a2, a3, a4].map(|p| p.map(|c| c as f32));
        if self.stage == 2 || (self.stage == 4 && self.second != 0) {
            let p = self.creature_pos(k, view, self.second_i as usize);
            let v = [p[0], k.f(k.sub(p[1], f64::from(QUAD_LIFT))), p[2]];
            let (right, up) = quad(k, v);
            let b1 = turn(k, right, angle, turn(k, neg(up), angle, v));
            let b2 = turn(k, right, angle, turn(k, up, angle, v));
            let b3 = turn(k, neg(right), angle, turn(k, up, angle, v));
            let b4 = turn(k, neg(right), angle, turn(k, neg(up), angle, v));
            self.quads[1] = [b1, b2, b3, b4].map(|p| p.map(|c| c as f32));
        }
    }

    /// FUN_00491b30: the warp's draw of the web (FUN_0045a930, the mode
    /// 1): the Z buffer cleared, every creature placed and drawn
    /// (FUN_00491d80; the prey and objects through the `Opacity`
    /// routine), the click quads; then the round lost, when it is.
    /// `out` is `None` when no picture is wanted (the logic only).
    pub fn draw(
        &mut self,
        host: &mut dyn WebHost,
        view: Option<&View>,
        out: Option<&mut Target>,
    ) -> Vec<Vec<ObjectStage>> {
        let mut stages = Vec::new();
        if !self.loaded {
            return stages;
        }
        let k = host.fpu();
        let mut out = out;
        if let Some(t) = out.as_deref_mut() {
            t.zbuf.fill(0);
        }
        for i in 0..self.creatures.len() {
            self.put(host, Some(i));
            let c = self.creatures[i];
            if c.kind == 0 {
                continue;
            }
            let opaque = !matches!(c.kind, 1..=3);
            let Some(slot) = c.model else { continue };
            if self.models[slot.index()].is_none() {
                // `Atl3D_draw_DataBase(NULL / invalid)`: fatal in the
                // original; the game's data never reach it.
                host.fatal("Atl3D_draw_DataBase NULL (valid=0)".into());
                continue;
            }
            self.drawn
                .push((slot, self.poses[slot.index()].time, opaque));
            if let (Some(v), Some(t)) = (view, out.as_deref_mut()) {
                stages.push(self.draw_slot(k, slot, v, opaque, t));
            }
        }
        self.build_quads(k, host.view());
        if self.lost != 0 {
            self.lost = 0;
            host.run_after(b"(cinema=46)");
            if self.stage == 3 {
                if self.feather != 0 {
                    host.run(b"(sacados=PLUME)");
                }
                host.run(b"(sacados=NOT_EPEE)");
                if self.skull != 0 {
                    host.run(b"(sacados=NOT_CRANE_ARAIG2)");
                }
            }
            if self.stage == 2 || self.stage == 4 {
                host.run(b"(sacados=NOT_SAUTERELLE)");
            }
            if self.stage == 1 {
                host.run(b"(sacados=NOT_CHENILLE)");
            }
            let back = if self.stage == 2 || self.stage == 4 {
                199
            } else {
                200
            };
            host.set_view_now(back);
            self.loaded = false;
            self.clear();
        }
        stages
    }

    /// FUN_0048f450: a save loaded (from FUN_004785a0) with the saved view
    /// `view`: in Xibalba's web (`MONDE` 6, views 198..265) the view
    /// becomes 198, the current view record is cleared, the stage's
    /// variables set from the bag, then the entry FUN_0048f530. Returns
    /// the view to enter.
    pub fn load_hook(&mut self, host: &mut dyn WebHost, view: i32) -> Option<i32> {
        if host.var("MONDE") != 6 || !(ENTRY_VIEW..=CENTRE).contains(&view) {
            return None;
        }
        if host.in_bag(b"CHENILLE") && self.stage == 1 {
            host.run(b"(finaraign1=1)");
        }
        if host.in_bag(b"SAUTERELLE") && self.stage == 2 {
            host.run(b"(finaraign2=1)");
        }
        if host.in_bag(b"CRANE_ARAIG2") && self.stage != 4 {
            self.skull = 1;
            host.run(b"(sacados=NOT_plume)");
            host.run(b"(finaraign3=1)");
        }
        self.entry198(host);
        Some(ENTRY_VIEW)
    }
}

/// A difference of angles brought into −π..π as the module's loops do
/// (2π added while below −π, taken while above π), on the x87 stack.
fn wrap(k: Fpu, d: f64) -> f64 {
    let two_pi = k.add(PI, PI);
    let mut d = d;
    if d < -PI {
        while d < -PI {
            d = k.add(d, two_pi);
        }
    }
    if d > PI {
        loop {
            d = k.sub(d, two_pi);
            if d <= PI || d.is_nan() {
                break;
            }
        }
    }
    d
}

/// `toile.vue`'s positions (FUN_00490fb0 with FUN_00498490): per
/// `frame`, its number checked (201..265), then past the identity
/// rotation `1 0 0 0 1 0 0 0 1` three numbers (`atof`, the double stored
/// as a float under `k`); 0x41 frames at most. The messages of the
/// original's checks are returned with the table.
pub fn parse_vue(k: Fpu, data: &[u8]) -> (Vec<[f32; 3]>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let end = data.len();
    let find = |from: usize, pat: &[u8]| -> Option<usize> {
        if from > end {
            return None;
        }
        data[from..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|p| p + from)
    };
    let mut at = find(0, b"frame");
    while let Some(p) = at {
        if p >= end || out.len() > 0x40 {
            break;
        }
        let mut cur = p + 6;
        let (n, next) = atof_skip(k, data, cur);
        cur = next;
        if !(201.0..=265.0).contains(&n) {
            errors.push(format!("fill_coord_warps() err num_warp({n:.6})"));
        }
        let Some(q) = find(cur, b"1 0 0 0 1 0 0 0 1") else {
            errors.push(format!("fill_coord_warps() err '[' Point {} ", out.len()));
            break;
        };
        cur = q + 0x12;
        let (x, c1) = atof_skip(k, data, cur);
        let (y, c2) = atof_skip(k, data, c1);
        let (z, c3) = atof_skip(k, data, c2);
        out.push([x as f32, y as f32, z as f32]);
        at = find(c3, b"frame");
    }
    (out, errors)
}

/// FUN_00498490: `atof` at `at` as a float (stored under `k`), then past
/// the number's characters (alphanumerics, `.`, `-`) and the others up to
/// the next alphanumeric, `.` or `-`.
fn atof_skip(k: Fpu, data: &[u8], at: usize) -> (f64, usize) {
    let s = &data[at.min(data.len())..];
    // `atof`: leading blanks, then the longest prefix that is a number.
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let start = i;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
    }
    let text = std::str::from_utf8(&s[start..i]).unwrap_or("0");
    let v: f64 = text.parse().unwrap_or(0.0);
    let v = f64::from(k.f32(v));
    let mut j = at;
    let num = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-';
    while j < data.len() && num(data[j]) {
        j += 1;
    }
    // Past the end of the text the original reads on into the heap until
    // such a character: the next search starts from where it stops, past
    // the buffer, and ends the loop.
    while j < data.len() && !num(data[j]) {
        j += 1;
    }
    (v, j)
}

/// The game around the web: the engine's scripts, bag, hand, views,
/// puzzle mode and files.
struct EngineHost<'a> {
    e: &'a mut Engine,
}

impl WebHost for EngineHost<'_> {
    fn var(&self, name: &str) -> i32 {
        self.e.script_var_or_lent(name).unwrap_or(0)
    }

    fn run(&mut self, cmd: &[u8]) {
        self.e.with_script(|s, h| s.run_command(h, cmd));
    }

    fn run_after(&mut self, cmd: &[u8]) {
        self.e.with_script(|s, _| s.queue_command(cmd));
    }

    fn in_bag(&self, name: &[u8]) -> bool {
        // FUN_004753d0: the name's index (FUN_004105a0, fatal when
        // unknown: the module's names are all in the game's
        // `defnames.con`), then the bag's entries (FUN_004753a0).
        match self.e.names.id(defnames::OBJETS, name) {
            Ok(id) => self.e.bag_entries().iter().any(|b| b.id == id as i32),
            Err(_) => false,
        }
    }

    fn object_id(&self, name: &[u8]) -> i32 {
        self.e
            .names
            .id(defnames::OBJETS, name)
            .map_or(-1, |id| id as i32)
    }

    fn hand(&self) -> i32 {
        self.e.objetenmain()
    }

    fn hold(&mut self, name: &[u8]) {
        // FUN_00475380 → FUN_00475340: `OBJETENMAIN` and the hand pair
        // (the object's sprite in the objects bank).
        let Some(id) = self.e.object_index(name) else {
            return;
        };
        let sprite = self.e.names.param(defnames::OBJETS, name);
        self.e.set_objetenmain(id);
        self.e.set_hand_pair(sprite, true);
        self.e.with_script(|s, _| s.hold(id));
    }

    fn view(&self) -> i32 {
        self.e.view as i32
    }

    fn view_is_yu(&self) -> bool {
        self.e.current_view().is_some_and(|v| {
            v.name.len() >= 2 && v.name.as_bytes()[..2].eq_ignore_ascii_case(b"YU")
        })
    }

    fn destinations(&self, view: i32) -> Option<Vec<i32>> {
        // FUN_004121a0 on the current place (@ 0x006d85d8).
        let place = self.e.wam.places.first()?;
        let v = place.views.iter().find(|v| v.id as i32 == view)?;
        Some(v.destinations.iter().map(|d| d.target as i32).collect())
    }

    fn set_view_now(&mut self, view: i32) {
        // @ 0x00491e18..0x00491e26: the next view (@ 0x006d85e4) and
        // FUN_0040fe50 (the current view record, `LASTVUE`, `VUE`).
        let id = view as u32;
        self.e.next_view = self.e.wam.view(id).map(|v| v.id);
        self.e.view = id;
        self.e.vars.vue = id;
        if let Some(s) = self.e.con.as_mut() {
            s.set_current_view(view);
        }
    }

    fn move_pending(&mut self) {
        self.e.move_pending = true;
    }

    fn mode(&self) -> i32 {
        self.e.puzzle_mode()
    }

    fn set_mode(&mut self, mode: i32) {
        self.e.set_puzzle_mode(mode);
    }

    fn read_3d(&self, name: &str) -> Option<Vec<u8>> {
        self.e.files.read(Folder::ThreeD, name)
    }

    fn load_model(&mut self, slot: Slot) -> Option<Arc<Model>> {
        // The original reads the file again at each load; its model is
        // the same each time.
        if let Some(m) = self.e.web_models.get(&slot) {
            return Some(Arc::clone(m));
        }
        let k = self.fpu();
        let files = &self.e.files;
        let m = load_model(k, slot, |n| files.read(Folder::ThreeD, n))?;
        self.e.web_models.insert(slot, Arc::clone(&m));
        Some(m)
    }

    fn fatal(&mut self, msg: String) {
        // FUN_00480cd0: an emergency save, then the game quits; here the
        // scripts stop. The game's data never reach it.
        log::error!("{msg}");
        self.e.with_script(|s, _| s.fail(msg));
    }

    fn warn(&mut self, msg: String) {
        log::warn!("{msg}");
    }

    fn fpu(&self) -> Fpu {
        self.e.camera.fpu
    }
}

impl Engine {
    /// Runs `f` on the web with the engine as its host.
    fn with_web<R>(&mut self, f: impl FnOnce(&mut Web, &mut dyn WebHost) -> R) -> R {
        let mut w = std::mem::take(&mut self.web);
        let r = f(&mut w, &mut EngineHost { e: self });
        self.web = w;
        r
    }

    /// The web's entry of view 198 outside the mode 1 (FUN_0048f530).
    pub(super) fn web_entry198(&mut self) {
        self.with_web(|w, h| w.entry198(h));
    }

    /// The web's entry in the mode 1 (FUN_0048f9a0).
    pub(super) fn web_entry(&mut self) {
        self.with_web(|w, h| w.entry(h));
    }

    /// The web's move click (FUN_0048fdd0) on a move zone of value
    /// `value`: true when it takes the click.
    pub(super) fn web_move_click(&mut self, value: i32) -> bool {
        self.with_web(|w, h| w.move_click(h, value))
    }

    /// The web's zone click (FUN_0049ca80 @ 0x0049caf2 → FUN_00491290):
    /// true when it takes the click.
    pub(super) fn web_zone_click(&mut self) -> bool {
        let cam = self.camera.clone();
        let mut click = self.click;
        let r = self.with_web(|w, h| w.zone_click(h, &cam, &mut click));
        self.click = click;
        r
    }

    /// The web's tick (FUN_00491610, the puzzle mode 1).
    pub(super) fn web_tick(&mut self) {
        self.with_web(|w, h| w.tick(h));
    }

    /// The web's quit (FUN_00491260, from the reset FUN_0049bf70).
    pub(super) fn web_quit(&mut self) {
        self.with_web(|w, h| w.quit(h));
    }

    /// The web's part of a load (FUN_0048f450 from FUN_004785a0) for the
    /// saved view `view`: the view to enter instead.
    pub(super) fn web_load_hook(&mut self, view: i32) -> Option<i32> {
        let v = self.with_web(|w, h| w.load_hook(h, view));
        if v.is_some() {
            // FUN_0048f450, which ran FUN_0048f530.
            for function in [0x0048_f450, 0x0048_f530] {
                self.event(super::Event::PuzzleRan { function });
            }
        }
        v
    }

    /// The web's draw (FUN_00491b30 from FUN_0045a930 in the mode 1, after
    /// the cube and the place animations' step): the creatures over the
    /// cube `out` (`None`: the logic only), the click quads, the round
    /// lost. With the HD mode on, the creatures are drawn again at the HD
    /// size over the HD view, by the same pipeline with the camera
    /// scaled.
    pub(super) fn web_draw(&mut self, out: Option<&mut [u16]>) {
        if self.puzzle_mode() != WEB_MODE {
            return;
        }
        let (w, h) = (self.renderer.width(), self.renderer.height());
        let view = View::of(&self.camera, w, h);
        let mut zbuf = std::mem::take(&mut self.web_z);
        zbuf.resize(w * h, 0);
        let drawn_any = match out {
            Some(px) => {
                let mut t = Target {
                    pixels: px,
                    zbuf: &mut zbuf,
                    width: w,
                    height: h,
                };
                let stages = self.with_web(|wb, host| wb.draw(host, Some(&view), Some(&mut t)));
                stages.iter().flatten().any(|s| !s.rejected)
            }
            None => {
                self.with_web(|wb, host| wb.draw(host, None, None));
                false
            }
        };
        self.web_z = zbuf;
        self.web.drawn.clear();
        self.web.placed.clear();
        if drawn_any {
            self.hd_web(&view);
        }
    }
}
