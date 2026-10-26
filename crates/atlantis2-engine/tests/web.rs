//! The spider's web (views 198-265 of `toile.wam`) against the original's
//! own results (`tests/golden/web.txt`): the data as loaded (A), the move
//! step for a spider on every node and the player on every node (B), and
//! every recorded operation of sections C (the entries, the load hook), D
//! (the clicks), E (warp passes) and G (the drawing), each from the state
//! the row gives (or the one the rows before left) to the state, commands
//! and results the row records; in G also each frame's samples (G+) and
//! every object drawn (G*: the pipeline's clip codes, eye, codes, faces and
//! projections).
//!
//! They need the game: `ATLANTIS2_DATA` = the installed folder,
//! `ATLANTIS2_DISCS` = a folder holding the four CD images (or copies of
//! the CDs); without them the tests are skipped.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use atlantis2_data::wam::Wam;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::atl3d::{Model, ObjectStage, View, MESH_FLAGS, SENTINEL_PLANE};
use atlantis2_engine::camera::Camera;
use atlantis2_engine::engine::web::{self, Creature, Slot, Web, WebHost, SLOTS};
use atlantis2_engine::raster3d::{Target, HEIGHT, WIDTH};
use atlantis2_engine::Fpu;
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/web.txt");
/// The control word of the reference (0x0e7f, the warp's).
const K: Fpu = Fpu::Chop;
const OBJECTS: [&str; 5] = ["CHENILLE", "SAUTERELLE", "CRANE_ARAIG2", "EPEE", "PLUME"];
const MARK: u16 = 0xf81f;

fn md5(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hname(name: &str) -> String {
    let bytes: Vec<u8> = name.chars().map(|c| c as u8).collect();
    md5(&bytes)[..8].to_string()
}

/// A slot's name in the golden (its file's stem).
fn slot_name(s: Slot) -> &'static str {
    s.file().split('.').next().unwrap()
}

fn slot_of(name: &str) -> Slot {
    SLOTS
        .into_iter()
        .find(|&s| slot_name(s) == name)
        .unwrap_or_else(|| panic!("slot {name}"))
}

/// Python's `%g` for the values the golden holds.
fn pg(x: f32) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Python's `%.9g`.
fn g9(x: f32) -> String {
    let v = f64::from(x);
    if v == 0.0 {
        return "0".into();
    }
    let e = v.abs().log10().floor() as i32;
    let decimals = (8 - e).max(0) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn rec_hex(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| format!("{w:x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn pylist(v: &[String]) -> String {
    let items: Vec<String> = v.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", items.join(", "))
}

struct Game {
    files: A2Files,
    wam: Wam,
    vue: Vec<u8>,
    coords: Vec<[f32; 3]>,
    models: RefCell<HashMap<Slot, Arc<Model>>>,
}

impl Game {
    fn open() -> Option<Game> {
        let (Some(dir), Some(discs)) = (
            std::env::var_os("ATLANTIS2_DATA"),
            std::env::var_os("ATLANTIS2_DISCS"),
        ) else {
            atlantis2_data::testing::missing("ATLANTIS2_DATA or ATLANTIS2_DISCS not set");
            return None;
        };
        let (mut files, _) =
            A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
        files.set_monde(6);
        let wam = Wam::parse(&files.read(Folder::Wam, "toile.wam").expect("toile.wam")).unwrap();
        let vue = files
            .read(Folder::ThreeD, "pz_araig/toile.vue")
            .expect("toile.vue");
        let (coords, errors) = web::parse_vue(K, &vue);
        assert!(errors.is_empty(), "{errors:?}");
        Some(Game {
            files,
            wam,
            vue,
            coords,
            models: RefCell::new(HashMap::new()),
        })
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.files.read(Folder::ThreeD, name)
    }

    fn model(&self, slot: Slot) -> Option<Arc<Model>> {
        if let Some(m) = self.models.borrow().get(&slot) {
            return Some(Arc::clone(m));
        }
        let m = web::load_model(K, slot, |n| self.read(n))?;
        self.models.borrow_mut().insert(slot, Arc::clone(&m));
        Some(m)
    }

    fn view(&self, v: i32) -> Option<&atlantis2_data::wam::View> {
        self.wam.places[0].views.iter().find(|x| x.id as i32 == v)
    }
}

/// The game around the module, as the harness keeps it.
struct Mock<'g> {
    g: &'g Game,
    vars: BTreeMap<&'static str, i32>,
    bag: Vec<String>,
    hand: i32,
    view: i32,
    next: i32,
    pending: u32,
    mode: i32,
    cmds: Vec<String>,
    errors: Vec<String>,
}

fn object_id(name: &str) -> i32 {
    OBJECTS
        .iter()
        .position(|&o| o.eq_ignore_ascii_case(name))
        .map_or(0, |i| i as i32 + 1)
}

impl Mock<'_> {
    fn new(g: &Game) -> Mock<'_> {
        Mock {
            g,
            vars: BTreeMap::new(),
            bag: Vec::new(),
            hand: 0,
            view: 0,
            next: 0,
            pending: 0,
            mode: 0,
            cmds: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// The harness's effects of RunOneCommand: the bag, the hand, the
    /// stage variables.
    fn apply(&mut self, cmd: &str) {
        let body = cmd.trim_matches(|c| c == '(' || c == ')');
        let Some((k, v)) = body.split_once('=') else {
            return;
        };
        match k.trim().to_ascii_lowercase().as_str() {
            "sacados" => {
                let neg = v.to_ascii_uppercase().starts_with("NOT_");
                let name = if neg { &v[4..] } else { v }.to_ascii_uppercase();
                self.bag.retain(|b| *b != name);
                if !neg {
                    self.bag.push(name);
                }
            }
            "objetenmain" => self.hand = v.parse().unwrap(),
            "finaraign1" => {
                self.vars.insert("finaraign1", v.parse().unwrap());
            }
            "finaraign2" => {
                self.vars.insert("finaraign2", v.parse().unwrap());
            }
            "finaraign3" => {
                self.vars.insert("finaraign3", v.parse().unwrap());
            }
            _ => {}
        }
    }

    fn set_view(&mut self, v: i32) {
        self.view = v;
        self.vars.insert("VUE", v);
    }
}

impl WebHost for Mock<'_> {
    fn var(&self, name: &str) -> i32 {
        self.vars.get(name).copied().unwrap_or(0)
    }
    fn run(&mut self, cmd: &[u8]) {
        let s = String::from_utf8_lossy(cmd).into_owned();
        self.apply(&s);
        self.cmds.push(s);
    }
    fn run_after(&mut self, cmd: &[u8]) {
        self.cmds
            .push(format!("after:{}", String::from_utf8_lossy(cmd)));
    }
    fn in_bag(&self, name: &[u8]) -> bool {
        let n = String::from_utf8_lossy(name);
        self.bag.iter().any(|b| b.eq_ignore_ascii_case(&n))
    }
    fn object_id(&self, name: &[u8]) -> i32 {
        object_id(&String::from_utf8_lossy(name))
    }
    fn hand(&self) -> i32 {
        self.hand
    }
    fn hold(&mut self, name: &[u8]) {
        self.hand = object_id(&String::from_utf8_lossy(name));
    }
    fn view(&self) -> i32 {
        self.view
    }
    fn view_is_yu(&self) -> bool {
        self.g
            .view(self.view)
            .is_some_and(|v| v.name.to_ascii_uppercase().starts_with("YU"))
    }
    fn destinations(&self, view: i32) -> Option<Vec<i32>> {
        self.g
            .view(view)
            .map(|v| v.destinations.iter().map(|d| d.target as i32).collect())
    }
    fn set_view_now(&mut self, view: i32) {
        self.next = view;
        self.set_view(view);
    }
    fn move_pending(&mut self) {
        self.pending = 1;
    }
    fn mode(&self) -> i32 {
        self.mode
    }
    fn set_mode(&mut self, mode: i32) {
        self.mode = mode;
    }
    fn read_3d(&self, name: &str) -> Option<Vec<u8>> {
        if name == "pz_araig/toile.vue" {
            return Some(self.g.vue.clone());
        }
        self.g.read(name)
    }
    fn load_model(&mut self, slot: Slot) -> Option<Arc<Model>> {
        self.g.model(slot)
    }
    fn fatal(&mut self, msg: String) {
        self.errors.push(format!("fatal:{msg}"));
    }
    fn warn(&mut self, msg: String) {
        self.errors.push(format!("warning:{msg}"));
    }
    fn fpu(&self) -> Fpu {
        K
    }
}

fn prey_of(name: &str) -> Option<&'static [u8]> {
    match name {
        "CHENILLE" => Some(b"CHENILLE"),
        "SAUTERELLE" => Some(b"SAUTERELLE"),
        "CRANE_ARAIG2" => Some(b"CRANE_ARAIG2"),
        "EPEE" => Some(b"EPEE"),
        "PLUME" => Some(b"PLUME"),
        _ => None,
    }
}

/// The slots by address, as the golden lists them.
fn slots_by_address() -> Vec<Slot> {
    let mut v = SLOTS.to_vec();
    v.sort_by_key(|s| s.address());
    v
}

/// The harness's `full()`.
fn state(w: &Web, m: &Mock) -> String {
    let hand = if (1..=5).contains(&m.hand) {
        OBJECTS[m.hand as usize - 1].to_string()
    } else {
        m.hand.to_string()
    };
    let models: Vec<&str> = slots_by_address()
        .into_iter()
        .filter(|&s| w.has(s))
        .map(slot_name)
        .collect();
    let cre: Vec<String> = w.creatures.iter().map(|c| rec_hex(&c.words())).collect();
    let prey = w
        .prey
        .map_or("-".to_string(), |p| String::from_utf8_lossy(p).into_owned());
    format!(
        "stage={} loaded={} files={} cater={} hopper={} skull={} sword={} lure={} feather={} \
         second={} second_i={} lost={} attack={} facing={} delay={} pickable={} mode={} prey={} \
         bag=[{}] hand={} fin={},{},{} monde={} vue={} view={} next={} pending={} models=[{}] \
         cre=[{}]",
        w.stage,
        u8::from(w.loaded),
        u8::from(w.files),
        w.cater,
        w.hopper,
        w.skull,
        w.sword,
        w.lure,
        w.feather,
        w.second,
        w.second_i,
        w.lost,
        w.attack,
        w.facing,
        w.delay,
        u8::from(w.pickable),
        m.mode,
        prey,
        m.bag.join(","),
        hand,
        m.var("finaraign1"),
        m.var("finaraign2"),
        m.var("finaraign3"),
        m.var("MONDE"),
        m.var("VUE"),
        m.view,
        m.next,
        m.pending,
        models.join(","),
        cre.join(" "),
    )
}

/// A state of the golden made again.
fn parse_state<'g>(g: &'g Game, s: &str) -> (Web, Mock<'g>) {
    let (head, cre) = s.split_once(" cre=[").expect("cre");
    let cre = cre.trim_end_matches(']');
    let mut w = Web::default();
    w.coords = g.coords.clone();
    let mut m = Mock::new(g);
    for tok in head.split(' ') {
        let (k, v) = tok.split_once('=').unwrap_or_else(|| panic!("token {tok}"));
        let int = || v.parse::<i32>().unwrap_or_else(|_| panic!("{k}={v}"));
        match k {
            "stage" => w.stage = int(),
            "loaded" => w.loaded = int() != 0,
            "files" => w.files = int() != 0,
            "cater" => w.cater = int(),
            "hopper" => w.hopper = int(),
            "skull" => w.skull = int(),
            "sword" => w.sword = int(),
            "lure" => w.lure = int(),
            "feather" => w.feather = int(),
            "second" => w.second = int(),
            "second_i" => w.second_i = int(),
            "lost" => w.lost = int(),
            "attack" => w.attack = int(),
            "facing" => w.facing = int(),
            "delay" => w.delay = int(),
            "pickable" => w.pickable = int() != 0,
            "mode" => m.mode = int(),
            "prey" => w.prey = prey_of(v),
            "bag" => {
                let v = v.trim_matches(|c| c == '[' || c == ']');
                m.bag = v
                    .split(',')
                    .filter(|x| !x.is_empty())
                    .map(String::from)
                    .collect();
            }
            "hand" => m.hand = v.parse().unwrap_or_else(|_| object_id(v)),
            "fin" => {
                for (i, x) in v.split(',').enumerate() {
                    let name = ["finaraign1", "finaraign2", "finaraign3"][i];
                    m.vars.insert(name, x.parse().unwrap());
                }
            }
            "monde" => {
                m.vars.insert("MONDE", int());
            }
            "vue" => {
                m.vars.insert("VUE", int());
            }
            "view" => m.view = int(),
            "next" => m.next = int(),
            "pending" => m.pending = int() as u32,
            "models" => {
                let v = v.trim_matches(|c| c == '[' || c == ']');
                for n in v.split(',').filter(|x| !x.is_empty()) {
                    let s = slot_of(n);
                    w.models[s.index()] = g.model(s);
                    if s == Slot::ARtoil {
                        w.artoil_len = w.models[s.index()].as_ref().unwrap().length;
                    }
                }
            }
            _ => panic!("key {k}"),
        }
    }
    for r in cre.split(' ').filter(|x| !x.is_empty()) {
        let words: Vec<u32> = r
            .split(',')
            .map(|x| u32::from_str_radix(x, 16).unwrap())
            .collect();
        w.creatures
            .push(Creature::from_words(words.try_into().unwrap()));
    }
    (w, m)
}

fn camera(yaw: u32, pitch: u32) -> Camera {
    let mut cam = Camera::new(K, WIDTH as u32, HEIGHT as u32);
    cam.enter(f32::from_bits(yaw), f32::from_bits(pitch));
    cam
}

fn quads(w: &Web) -> String {
    w.quads
        .iter()
        .flatten()
        .map(|p| rec_hex(&p.map(f32::to_bits)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn draws(w: &mut Web) -> String {
    let d: Vec<String> = w
        .drawn
        .drain(..)
        .map(|(s, t, o)| format!("{}@{}/{}", slot_name(s), pg(t), u8::from(o)))
        .collect();
    d.join(" ")
}

fn placements(w: &mut Web) -> String {
    let mut out = Vec::new();
    for (s, p, q) in w.placed.drain(..) {
        let name = s.map_or("0x0", slot_name);
        out.push(format!("pos:{name}:{}", rec_hex(&p.map(f32::to_bits))));
        out.push(format!("rot:{name}:{}", rec_hex(&q.map(f32::to_bits))));
    }
    out.join(" ")
}

/// The frame's head and samples (the harness's `frame_rows`).
fn frame_rows(pixels: &[u16], zbuf: &[u32]) -> (String, String) {
    let img: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    let z: Vec<u8> = zbuf.iter().flat_map(|p| p.to_le_bytes()).collect();
    let drawn: Vec<usize> = (0..pixels.len()).filter(|&i| pixels[i] != MARK).collect();
    let (bx, samples) = if drawn.is_empty() {
        ("-".to_string(), String::new())
    } else {
        let xs = drawn.iter().map(|i| i % WIDTH);
        let ys = drawn.iter().map(|i| i / WIDTH);
        let bx = format!(
            "{},{}-{},{}",
            xs.clone().min().unwrap(),
            ys.clone().min().unwrap(),
            xs.max().unwrap(),
            ys.max().unwrap()
        );
        let step = (drawn.len() / 32).max(1);
        let s: Vec<String> = drawn
            .iter()
            .step_by(step)
            .take(32)
            .map(|&i| {
                format!(
                    "{},{}:{:04x}:{:08x}",
                    i % WIDTH,
                    i / WIDTH,
                    pixels[i],
                    zbuf[i]
                )
            })
            .collect();
        (bx, s.join(" "))
    };
    let zwords = zbuf.iter().filter(|&&v| v != 0).count();
    (
        format!(
            "frame {} z {} drawn {} zwords {} box {}",
            md5(&img),
            md5(&z),
            drawn.len(),
            zwords,
            bx
        ),
        samples,
    )
}

/// A G* row of the golden in [`stage_row`]'s form. Its `f=` (the flags
/// FUN_0042c8d0 receives) is the pipeline's 0x6300000 with, in bits 0-1,
/// FUN_004d6210's texture-coordinate channels that have a source: checked
/// to be so (channel 0, the mesh's UV set, always) and left out, the port
/// drawing with the mesh's UV set alone (the frames' md5s pin the pixels).
/// The whole buffers' `cam=` (camera-space vertices @ pipeline + 0x04) and
/// `proj=` (+ 0x14) are left out: the projection of an object inside every
/// plane (@ 0x0057eac0) writes no camera-space vertex, the transform
/// @ 0x0057eec0 writes 12 of each vertex's 16 bytes, and both projections
/// skip the vertices no face uses, so these hold what the objects drawn
/// before left; `uproj=`, the used vertices' entries, is compared.
fn stage_of_golden(rest: &str) -> String {
    let mut out = Vec::new();
    for kv in rest.split(' ') {
        let (key, value) = kv.split_once('=').unwrap();
        match key {
            "f" => {
                let f = u32::from_str_radix(value, 16).unwrap();
                assert!(f & !3 == 0x630_0000 && f & 1 == 1, "flags {f:x}");
            }
            "cam" | "proj" => {}
            _ => out.push(kv),
        }
    }
    out.join(" ")
}

/// A G* row's comparable part (`or`, `and`, the vertex and face counts,
/// the eye, codes, faces and the used vertices' projections) of an object
/// drawn.
fn stage_row(st: &ObjectStage) -> String {
    let eye: Vec<u8> = st.eye.iter().flat_map(|v| v.to_le_bytes()).collect();
    let uproj: Vec<u8> = st
        .projected
        .iter()
        .zip(&st.codes)
        .filter(|(_, &c)| c & 0x80 != 0)
        .flat_map(|(p, _)| p.iter().flat_map(|v| v.to_le_bytes()))
        .collect();
    format!(
        "or={:x} and={:x} nv={} nf={} eye={} codes={} faces={} uproj={}",
        st.or,
        st.and,
        st.codes.len(),
        st.faces.len(),
        eye.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        md5(&st.codes),
        md5(&st.faces),
        md5(&uproj)
    )
}

thread_local! {
    /// For [`no_face_crosses_the_near_or_far_plane`]: when set, each pass
    /// of sections E and F first looks for faces crossing the near or far
    /// plane ([`near_far_sweep`]): the passes where a wide view finds one,
    /// the views of the game's own field that do, the objects looked at.
    static SWEEP: RefCell<Option<[usize; 3]>> = const { RefCell::new(None) };
}

/// The creatures' faces as the pass is about to draw them, against the
/// near (1.5) and far (100000) planes of the warp camera at the player's
/// node (`ObjectStage::clipped`, from the clip codes of the objects not
/// inside every plane). First twelve wide views (yaw every 90°, pitch
/// −60°, 0°, 60°, a quarter of the focal lengths: about 145° across):
/// a vertex nearer to the eye than the near plane shows there. Only when
/// one does, the game's own view (76°) every 10° of yaw and of pitch
/// within the ±70° the views allow (the per-view reset's limits; a 5°
/// grid finds none either). Nothing is rasterised (a 1 x 1 target); the
/// state is put back after.
fn near_far_sweep(w: &mut Web, m: &mut Mock) {
    if !w.loaded || SWEEP.with(|s| s.borrow().is_none()) {
        return;
    }
    let saved = (
        w.poses.clone(),
        w.drawn.len(),
        w.lost,
        w.placed.len(),
        w.quads,
    );
    let cmds = m.cmds.len();
    let errors = m.errors.len();
    let (mut px, mut zb) = ([0u16; 1], [0u32; 1]);
    let mut views = |w: &mut Web, m: &mut Mock, angles: &[(f32, f32)], wide: bool| {
        let (mut clipped, mut objects) = (0, 0);
        for &(yaw, pitch) in angles {
            let mut cam = Camera::new(K, WIDTH as u32, HEIGHT as u32);
            cam.enter(yaw.to_radians(), pitch.to_radians());
            let mut view = View::of(&cam, WIDTH, HEIGHT);
            if wide {
                view = view.scaled(K, 4, 0, WIDTH, HEIGHT);
            }
            // The round lost is the pass's own draw's.
            let lost = std::mem::take(&mut w.lost);
            let stages = {
                let mut t = Target {
                    pixels: &mut px,
                    zbuf: &mut zb,
                    width: 1,
                    height: 1,
                };
                w.draw(m, Some(&view), Some(&mut t))
            };
            w.lost = lost;
            let hit: usize = stages.iter().flatten().map(|o| o.clipped).sum();
            clipped += usize::from(hit > 0);
            objects += stages.iter().flatten().filter(|o| !o.rejected).count();
        }
        (clipped, objects)
    };
    let wide: Vec<(f32, f32)> = (0..4)
        .flat_map(|y| [-60.0, 0.0, 60.0].map(|p| (y as f32 * 90.0, p)))
        .collect();
    let (near_eye, mut objects) = views(w, m, &wide, true);
    let mut crossing = 0;
    if near_eye > 0 {
        let game: Vec<(f32, f32)> = (0..36)
            .flat_map(|y| (-7..=7).map(move |p| (y as f32 * 10.0, p as f32 * 10.0)))
            .collect();
        let (c, o) = views(w, m, &game, false);
        crossing = c;
        objects += o;
    }
    w.poses = saved.0;
    w.drawn.truncate(saved.1);
    w.lost = saved.2;
    w.placed.truncate(saved.3);
    w.quads = saved.4;
    m.cmds.truncate(cmds);
    m.errors.truncate(errors);
    SWEEP.with(|s| {
        if let Some(t) = s.borrow_mut().as_mut() {
            t[0] += usize::from(near_eye > 0);
            t[1] += crossing;
            t[2] += objects;
        }
    });
}

/// What an operation printed after the state.
struct Done {
    extra: String,
    brief: bool,
    /// Section F's passes: the md5 of their texts, after the commands.
    md5: Option<String>,
    samples: Option<String>,
    stages: Vec<ObjectStage>,
}

fn run_op(w: &mut Web, m: &mut Mock, op: &str) -> Done {
    let mut done = Done {
        extra: String::new(),
        brief: false,
        md5: None,
        samples: None,
        stages: Vec::new(),
    };
    let parts: Vec<&str> = op.split(' ').collect();
    let hex = |s: &str| u32::from_str_radix(s, 16).unwrap();
    match parts[0] {
        "none" => {}
        "enter" => {
            let v: i32 = parts[1].parse().unwrap();
            m.set_view(v);
            if m.mode == web::WEB_MODE {
                w.entry(m);
            } else if v == web::ENTRY_VIEW {
                w.entry198(m);
            }
        }
        "move" => {
            let r = w.move_click(m, parts[1].parse().unwrap());
            done.extra = format!(" ret={}", u8::from(r));
        }
        // The web's quit (FUN_00491260), as a load's puzzle reset runs it.
        "quit" => w.quit(m),
        "load" => {
            let v: i32 = parts[1].parse().unwrap();
            let saved = match w.load_hook(m, v) {
                Some(v) => {
                    m.view = 0;
                    v
                }
                None => v,
            };
            done.extra = format!(" saved={saved}");
        }
        "zone" => {
            let cam = camera(hex(parts[1]), hex(parts[2]));
            let mut click = parts[3] == "1";
            w.build_quads(K, m.view);
            let hover = match w.hover(K, &cam) {
                Some(0) => 0,
                Some(_) => 0xd0,
                None => -1,
            };
            let hit = w.hit(K, &cam);
            let r = w.zone_click(m, &cam, &mut click);
            done.extra = format!(
                " hover={hover} hit={} ret={} click={} quads={}",
                u8::from(hit),
                u8::from(r),
                u8::from(click),
                quads(w)
            );
        }
        "pass" => {
            near_far_sweep(w, m);
            if w.loaded {
                w.draw(m, None, None);
            }
            if w.loaded {
                w.tick(m);
            }
            done.extra = format!(" draws=[{}] placed=[{}]", draws(w), placements(w));
            done.brief = true;
        }
        "passes" => {
            // Section F: N passes, each one's text as section E's, their
            // md5 after the state.
            let n: usize = parts[1].parse().unwrap();
            let mut texts = Vec::new();
            let (mut cmds, mut errors) = (Vec::new(), Vec::new());
            for _ in 0..n {
                m.cmds.clear();
                m.errors.clear();
                near_far_sweep(w, m);
                if w.loaded {
                    w.draw(m, None, None);
                }
                if w.loaded {
                    w.tick(m);
                }
                let extra = format!(" draws=[{}] placed=[{}]", draws(w), placements(w));
                texts.push(format!(
                    "{}{extra} cmds={} errors={}",
                    state(w, m),
                    pylist(&m.cmds),
                    pylist(&m.errors)
                ));
                cmds.append(&mut m.cmds);
                errors.append(&mut m.errors);
            }
            m.cmds = cmds;
            m.errors = errors;
            done.md5 = Some(md5(texts.join("\n").as_bytes()));
        }
        "hand" => m.hand = object_id(parts[1]),
        "draw" => {
            let cam = camera(hex(parts[1]), hex(parts[2]));
            let view = View::of(&cam, WIDTH, HEIGHT);
            let mut pixels = vec![MARK; WIDTH * HEIGHT];
            let mut zbuf = vec![0u32; WIDTH * HEIGHT];
            let stages = {
                let mut t = Target::screen(&mut pixels, &mut zbuf);
                w.draw(m, Some(&view), Some(&mut t))
            };
            let (head, samples) = frame_rows(&pixels, &zbuf);
            // The routines asked (FUN_004cea20, by the opacity flag) and the
            // clip codes (FUN_0042c8d0) of the objects the box tests let
            // through.
            let mut flags = std::collections::BTreeSet::new();
            for (st, d) in stages.iter().zip(&w.drawn) {
                if st.iter().any(|o| !o.rejected) {
                    flags.insert(if d.2 { "60000e" } else { "20000e" });
                }
            }
            let flat: Vec<ObjectStage> = stages.into_iter().flatten().collect();
            let clips: Vec<String> = flat
                .iter()
                .filter(|s| !s.rejected)
                .map(|s| format!("{:x}/{:x}", s.or, s.and))
                .collect();
            let fl: Vec<&str> = flags.into_iter().collect();
            done.extra = format!(
                " | {head} flags {} clips {} key 0 draws [{}] quads {}",
                fl.join(","),
                clips.join(" "),
                draws(w),
                quads(w)
            );
            w.placed.clear();
            done.samples = Some(samples);
            done.stages = flat;
        }
        _ => panic!("op {op}"),
    }
    if parts[0] != "pass" {
        // The harness records the placements and draws of passes only.
        w.placed.clear();
        w.drawn.clear();
    }
    done
}

/// A row of sections C, D, E or G: `TAG|label | [in: S | ]op: O | rest`.
struct Row<'a> {
    tag: &'a str,
    label: &'a str,
    input: Option<&'a str>,
    op: &'a str,
    rest: &'a str,
}

fn parse_row(line: &str) -> Row<'_> {
    let (head, rest) = line.split_once(" | ").unwrap();
    let (tag, label) = head.split_once('|').unwrap();
    let (input, rest) = match rest.strip_prefix("in: ") {
        Some(r) => {
            let (i, r) = r.split_once(" | ").unwrap();
            (Some(i), r)
        }
        None => (None, rest),
    };
    let r = rest.strip_prefix("op: ").expect("op");
    let (op, rest) = r.split_once(" | ").unwrap();
    Row {
        tag,
        label,
        input,
        op,
        rest,
    }
}

fn rows_of(tags: &[&str]) -> Vec<&'static str> {
    GOLDEN
        .lines()
        .filter(|l| tags.iter().any(|t| l.starts_with(&format!("{t}|"))))
        .collect()
}

/// Replays the operation rows of a section; returns the mismatches.
fn replay(g: &Game, tag: &str) -> (usize, Vec<String>) {
    let mut bad = Vec::new();
    let mut cur: Option<(Web, Mock)> = None;
    let lines: Vec<&str> = GOLDEN.lines().collect();
    let mut n = 0;
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with(&format!("{tag}|")) {
            continue;
        }
        let row = parse_row(line);
        assert_eq!(row.tag, tag);
        if let Some(s) = row.input {
            let (w, m) = parse_state(g, s);
            let again = state(&w, &m);
            assert_eq!(again, s, "state round trip of {}", row.label);
            cur = Some((w, m));
        }
        let (w, m) = cur.as_mut().expect("a state");
        m.cmds.clear();
        m.errors.clear();
        let done = run_op(w, m, row.op);
        let mut text = format!(
            "out: {}{} cmds={} errors={}",
            state(w, m),
            done.extra,
            pylist(&m.cmds),
            pylist(&m.errors)
        );
        if let Some(h) = &done.md5 {
            text += &format!(" md5={h}");
        }
        let full = text.clone();
        let text = if done.brief && m.cmds.is_empty() && m.errors.is_empty() {
            format!("md5: {}", md5(text.as_bytes()))
        } else {
            text
        };
        n += 1;
        if text != row.rest {
            bad.push(format!(
                "{tag}|{} op {} (line {}):\n  want {}\n  got  {}",
                row.label,
                row.op,
                i + 1,
                row.rest,
                full
            ));
        }
        if let Some(samples) = done.samples {
            // The frame's samples (G+) and every object drawn (G*), each
            // compared.
            let want = lines[i + 1];
            let got = format!("G+|{} | {}", row.label, samples);
            n += 1;
            if want != got {
                bad.push(format!("G+|{}:\n  want {want}\n  got  {got}", row.label));
            }
            // The objects the box tests let through (FUN_0042c8d0).
            let drawn: Vec<&ObjectStage> = done.stages.iter().filter(|s| !s.rejected).collect();
            let objects: Vec<&str> = lines[i + 2..]
                .iter()
                .take_while(|l| l.starts_with("G*|"))
                .copied()
                .collect();
            if objects.len() != drawn.len() {
                bad.push(format!(
                    "G*|{}: {} objects drawn, {} in the golden",
                    row.label,
                    drawn.len(),
                    objects.len()
                ));
            }
            for (k, (want, st)) in objects.iter().zip(drawn).enumerate() {
                let (head, rest) = want.split_once(" | ").unwrap();
                assert_eq!(head, format!("G*|{} object {k}", row.label));
                n += 1;
                let got = stage_row(st);
                if stage_of_golden(rest) != got {
                    bad.push(format!(
                        "G*|{} object {k}:\n  want {rest}\n  got  {got}",
                        row.label
                    ));
                }
            }
        }
        m.cmds.clear();
        m.errors.clear();
    }
    (n, bad)
}

fn check(tag: &str) {
    let Some(g) = Game::open() else { return };
    let (n, bad) = replay(&g, tag);
    // Every row of the section, G's samples (G+) and objects (G*) too.
    let rows = GOLDEN
        .lines()
        .filter(|l| {
            l.starts_with(tag) && matches!(l.as_bytes().get(tag.len()), Some(b'|' | b'+' | b'*'))
        })
        .count();
    assert_eq!(n, rows, "{tag} rows replayed");
    if !bad.is_empty() {
        for b in bad.iter().take(12) {
            eprintln!("{b}");
        }
        panic!("{} of {n} {tag} rows differ", bad.len());
    }
}

#[test]
fn a_data_and_models() {
    let Some(g) = Game::open() else { return };
    let mut out = Vec::new();
    out.push(format!(
        "toile.vue: {} positions; errors []",
        g.coords.len()
    ));
    for (k, p) in g.coords.iter().enumerate() {
        out.push(format!(
            "A|pos {} {:08x} {:08x} {:08x}",
            201 + k,
            p[0].to_bits(),
            p[1].to_bits(),
            p[2].to_bits()
        ));
    }
    for v in 198..=265 {
        let Some(view) = g.view(v) else { continue };
        let (r, s) = if v > 200 {
            let r = web::ring(v);
            (r, web::sector(v, r))
        } else {
            (-1, -1)
        };
        let d: Vec<String> = view
            .destinations
            .iter()
            .map(|d| d.target.to_string())
            .collect();
        out.push(format!(
            "A|view {v} ring {r} sector {s} dests [{}]",
            d.join(", ")
        ));
    }
    let mut seen = Vec::new();
    for stage in 1..=4 {
        let mut w = Web::default();
        let mut m = Mock::new(&g);
        w.stage = stage;
        w.load_files(&mut m);
        let loaded: Vec<String> = slots_by_address()
            .into_iter()
            .filter(|&s| w.has(s))
            .map(|s| {
                format!(
                    "{}:len={}:valid=1:parent_root=1:scale={}",
                    slot_name(s),
                    pg(w.length(s).unwrap()),
                    g9(s.scale())
                )
            })
            .collect();
        out.push(format!(
            "A|files stage {stage}: {} files={}",
            loaded.join(" "),
            u8::from(w.files)
        ));
        for s in slots_by_address() {
            if !w.has(s) || seen.contains(&s) {
                continue;
            }
            seen.push(s);
            let model = w.models[s.index()].as_ref().unwrap();
            let info = model.objects_info();
            for (k, o) in info.iter().enumerate() {
                let pname = o
                    .parent
                    .map_or(slot_name(s).to_string(), |p| info[p].name.clone());
                let mut row = format!(
                    "A|model {} object {k} {} parent {} local {}",
                    slot_name(s),
                    hname(&o.name),
                    hname(&pname),
                    rec_hex(&o.local.map(f32::to_bits))
                );
                if let Some(mesh) = &o.mesh {
                    let mats: Vec<String> = mesh
                        .mats
                        .iter()
                        .map(|(name, tex)| {
                            let n = name.as_deref().map_or("-".to_string(), hname);
                            match tex {
                                Some((tw, th, px)) => {
                                    let b: Vec<u8> =
                                        px.iter().flat_map(|p| p.to_le_bytes()).collect();
                                    format!("{n}:{tw}x{th}:{}", md5(&b))
                                }
                                None => format!("{n}:none"),
                            }
                        })
                        .collect();
                    let groups: Vec<String> = mesh.groups.iter().map(|c| c.to_string()).collect();
                    // The corners and the list's end mark 0xffff.
                    let corners: Vec<u8> = mesh
                        .corners
                        .iter()
                        .flatten()
                        .chain(&[0xffff])
                        .flat_map(|c| c.to_le_bytes())
                        .collect();
                    let planes: Vec<u8> = mesh
                        .planes
                        .iter()
                        .flatten()
                        .flat_map(|c| c.to_le_bytes())
                        .collect();
                    let last: Vec<u8> = SENTINEL_PLANE
                        .iter()
                        .flat_map(|c| c.to_le_bytes())
                        .collect();
                    let hexs = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
                    row += &format!(
                        " mats [{}] mesh nv {} nf {} flags {:x} groups [{}] corners {} planes {} {} first {} last {} box {}",
                        mats.join(" "),
                        mesh.vertices,
                        mesh.faces,
                        MESH_FLAGS,
                        groups.join(", "),
                        md5(&corners),
                        mesh.faces + 1,
                        md5(&planes),
                        hexs(&planes[..16.min(planes.len())]),
                        hexs(&last),
                        rec_hex(&mesh.bbox.map(f32::to_bits))
                    );
                }
                out.push(row);
            }
        }
    }
    let want: Vec<&str> = GOLDEN
        .lines()
        .filter(|l| l.starts_with("A|") || l.starts_with("toile.vue:"))
        .collect();
    let mut bad = 0;
    for (i, (a, b)) in want.iter().zip(&out).enumerate() {
        if a != b {
            bad += 1;
            if bad <= 8 {
                eprintln!("row {i}:\n  want {a}\n  got  {b}");
            }
        }
    }
    assert_eq!(want.len(), out.len(), "rows");
    assert_eq!(bad, 0, "A rows differ");
}

#[test]
fn b_move_step() {
    let Some(g) = Game::open() else { return };
    let want = rows_of(&["B"]);
    let mut got = Vec::new();
    for (stage, kind) in [(1, 2), (2, 2), (3, 1)] {
        let mut base = Web::default();
        base.coords = g.coords.clone();
        let mut m = Mock::new(&g);
        base.stage = stage;
        base.load_files(&mut m);
        base.files = false;
        base.loaded = true;
        let walker = if kind == 1 {
            Slot::ARstby
        } else {
            Slot::PARstby
        };
        for c in 201..=265 {
            let mut tos = Vec::new();
            let mut lines = String::new();
            for p in 201..=265 {
                let mut w = base.clone();
                w.creatures = vec![Creature {
                    kind,
                    model: Some(walker),
                    from: c,
                    to: c,
                    steps: if kind == 1 { 2 } else { 0 },
                    offset: web::OFFSET,
                    ..Creature::default()
                }];
                m.view = p;
                w.step(&mut m);
                let r = w.creatures[0];
                tos.push(r.to.to_string());
                lines += &format!("{p} | {} {} {}\n", rec_hex(&r.words()), w.attack, w.second);
            }
            got.push(format!(
                "B|{stage} {kind} {c} | {} | {}",
                tos.join(" "),
                md5(lines.as_bytes())
            ));
        }
    }
    assert_eq!(want.len(), got.len());
    let bad: Vec<_> = want.iter().zip(&got).filter(|(a, b)| a != b).collect();
    for (a, b) in bad.iter().take(8) {
        eprintln!("want {a}\ngot  {b}");
    }
    assert!(bad.is_empty(), "{} B rows differ", bad.len());
}

#[test]
fn c_entries_and_load_hook() {
    check("C");
}

#[test]
fn d_clicks() {
    check("D");
}

#[test]
fn e_passes() {
    check("E");
}

#[test]
fn g_drawing() {
    check("G");
}

#[test]
fn f_rounds_won() {
    check("F");
}

/// Review L40: the web's faces never cross the near or the far plane, so
/// the clipping the port leaves out (@ 0x004380b0, its interpolation of
/// both texture channels) is never needed: every pass of sections E and F
/// (each stage's rounds, the attacks to the round lost, the strikes, the
/// prey, four rounds won) with the creatures looked at in every direction
/// of the game's own field of view ([`near_far_sweep`]).
#[test]
fn no_face_crosses_the_near_or_far_plane() {
    if Game::open().is_none() {
        return;
    }
    SWEEP.with(|s| *s.borrow_mut() = Some([0; 3]));
    check("E");
    check("F");
    let [near_eye, crossing, objects] = SWEEP.with(|s| s.borrow_mut().take()).unwrap();
    // Creatures come within the near plane's distance of the eye (beside
    // or behind it: the wide views), but never into the game's own field.
    assert!(
        near_eye > 400,
        "passes with a creature at the eye: {near_eye}"
    );
    assert!(objects > 1_000_000, "objects looked at: {objects}");
    assert_eq!(
        crossing, 0,
        "views with a face crossing the near or far plane"
    );
}

// ---------------------------------------------------------------------------
// Rounds played: the player's moves, waiting each time for the spiders to
// stand, as a player with the mouse must (the move click is taken while a
// spider walks or turns).

/// A warp pass of the web: the draw's logic, then the tick.
fn web_pass(w: &mut Web, m: &mut Mock) {
    if w.loaded {
        w.draw(m, None, None);
    }
    if w.loaded {
        w.tick(m);
    }
    w.placed.clear();
    w.drawn.clear();
}

/// Passes until every spider stands and no round is lost (or the web is
/// left); the number of passes.
fn settle(w: &mut Web, m: &mut Mock) -> usize {
    for n in 1..=400 {
        web_pass(w, m);
        let standing = w.creatures.iter().all(|c| {
            !matches!(c.kind, 1 | 2) || (c.t == 0.0 && c.target == c.heading && c.from == c.to)
        });
        if !w.loaded || (standing && w.lost == 0) {
            return n;
        }
    }
    panic!("the spiders do not stop");
}

/// What a player's step did.
#[derive(Debug, PartialEq)]
enum Step {
    Moved,
    /// The click was taken by the web (the strike, the feather laid).
    Taken,
    /// The round was lost (back to `view`).
    Lost(i32),
}

/// A move zone of value `v` clicked from the current view: the web's
/// click, then (when it does not take it) the view entered; the spiders
/// let settle. A round lost sends the player back (FUN_00491df0).
fn step(w: &mut Web, m: &mut Mock, v: i32) -> Step {
    let taken = w.move_click(m, v);
    if !taken {
        m.set_view(v);
        if m.mode == web::WEB_MODE {
            w.entry(m);
        } else if v == web::ENTRY_VIEW {
            w.entry198(m);
        }
    }
    let before = m.next;
    settle(w, m);
    if m.next != before && m.pending == 1 {
        m.pending = 0;
        m.next = 0;
        return Step::Lost(m.view);
    }
    if taken {
        Step::Taken
    } else {
        Step::Moved
    }
}

/// The camera of the view looking at creature `i` (the direction of its
/// position, 20 below its height, as the click quads are built).
fn look_at(w: &Web, view: i32, i: usize) -> Camera {
    let p = w.creature_pos(K, view, i);
    let (x, y, z) = (p[0], p[1] - 20.0, p[2]);
    let yaw = z.atan2(x) as f32;
    let pitch = y.atan2((x * x + z * z).sqrt()) as f32;
    let mut cam = Camera::new(K, WIDTH as u32, HEIGHT as u32);
    cam.enter(yaw, pitch);
    cam
}

/// The click on spider `i` (FUN_00491290 through the warp pass: the
/// quads of a draw looking at it, then the click), the spiders let
/// settle.
fn strike(w: &mut Web, m: &mut Mock, i: usize) -> Step {
    let cam = look_at(w, m.view, i);
    w.build_quads(K, m.view);
    let mut click = true;
    let taken = w.zone_click(m, &cam, &mut click);
    settle(w, m);
    if taken {
        Step::Taken
    } else {
        Step::Moved
    }
}

/// A round from view 198 at stage `stage` (`finaraign1..3` as the stage
/// needs, the bag `bag`), the player's actions `route`: a view (a move
/// zone clicked), 0 (the hand emptied), -1 (the first spider clicked),
/// -2 (the sword taken from the bag), -3 (the feather taken from the
/// bag); the outcome of each step.
fn play<'g>(g: &'g Game, stage: i32, bag: &[&str], route: &[i32]) -> (Web, Mock<'g>, Vec<Step>) {
    let mut w = Web::default();
    w.coords = g.coords.clone();
    let mut m = Mock::new(g);
    for (i, name) in ["finaraign1", "finaraign2", "finaraign3"]
        .iter()
        .enumerate()
    {
        m.vars.insert(name, i32::from(stage > i as i32 + 1));
    }
    m.vars.insert("MONDE", 6);
    m.bag = bag.iter().map(|s| s.to_string()).collect();
    m.set_view(198);
    w.entry198(&mut m);
    let mut steps = Vec::new();
    for &v in route {
        let s = match v {
            0 => {
                m.hand = 0;
                continue;
            }
            -1 => strike(&mut w, &mut m, 0),
            -2 => {
                m.hand = object_id("EPEE");
                continue;
            }
            -3 => {
                m.hand = object_id("PLUME");
                continue;
            }
            _ => step(&mut w, &mut m, v),
        };
        let lost = matches!(s, Step::Lost(_));
        steps.push(s);
        if lost {
            break;
        }
    }
    (w, m, steps)
}

/// The shortest routes from view 198 that win a round of `stage`
/// (breadth first over the web's states after each step).
fn search(g: &Game, stage: i32, bag: &[&str], won: impl Fn(&Web, &Mock) -> bool) -> Vec<i32> {
    use std::collections::{HashSet, VecDeque};
    let (w, m, _) = play(g, stage, bag, &[]);
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((w, m, Vec::<i32>::new()));
    while let Some((w, m, route)) = queue.pop_front() {
        let key = state(&w, &m);
        if !seen.insert(key) {
            continue;
        }
        let mut dests = m.destinations(m.view).unwrap_or_default();
        if stage == 3 {
            if m.hand == object_id("EPEE") {
                dests.push(-1);
            } else if m.bag.iter().any(|b| b == "EPEE") {
                dests.push(-2);
            }
            if m.hand != object_id("PLUME") && m.bag.iter().any(|b| b == "PLUME") {
                dests.push(-3);
            }
        }
        for v in dests {
            if v == web::RIVER || (v == web::ENTRY_VIEW && m.view != 199 && m.view != 200) {
                continue;
            }
            let mut w2 = w.clone();
            let mut m2 = Mock {
                g,
                vars: m.vars.clone(),
                bag: m.bag.clone(),
                hand: m.hand,
                view: m.view,
                next: m.next,
                pending: m.pending,
                mode: m.mode,
                cmds: Vec::new(),
                errors: Vec::new(),
            };
            let s = match v {
                -1 => strike(&mut w2, &mut m2, 0),
                -2 => {
                    m2.hand = object_id("EPEE");
                    Step::Taken
                }
                -3 => {
                    m2.hand = object_id("PLUME");
                    Step::Taken
                }
                _ => step(&mut w2, &mut m2, v),
            };
            let mut r = route.clone();
            r.push(v);
            if matches!(s, Step::Lost(_)) {
                continue;
            }
            if won(&w2, &m2) {
                return r;
            }
            if r.len() < 40 {
                queue.push_back((w2, m2, r));
            }
        }
    }
    Vec::new()
}

#[test]
#[ignore = "finds the routes of the rounds the flow plays"]
fn find_routes() {
    let Some(g) = Game::open() else { return };
    let r1 = search(&g, 1, &[], |_, m| m.var("finaraign1") == 1);
    eprintln!("stage 1: {r1:?}");
    let r2 = search(&g, 2, &[], |_, m| m.var("finaraign2") == 1);
    eprintln!("stage 2: {r2:?}");
    let r3 = search(&g, 3, &["PLUME"], |_, m| m.var("finaraign3") == 1);
    eprintln!("stage 3: {r3:?}");
    let r4 = search(&g, 4, &[], |w, m| {
        m.bag.iter().any(|b| b == "SAUTERELLE") && !w.loaded && (m.view == 199 || m.view == 200)
    });
    eprintln!("stage 4: {r4:?}");
}
