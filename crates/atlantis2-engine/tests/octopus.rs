//! The octopus fight of Atlantis (`atlantis.wam`, the `AT` views 1-19,
//! chapter 82) against the original (`tests/golden/octopus.txt`, its
//! header there): the module's data (rows A), `random(n)` (B), the
//! sectors of the view's yaw (S), the entry per view, seed and dead
//! tentacles (C), the hit shake and the camera's reset (K), the tick alone
//! on 76 states (T), the cursor (U), the dispatchers, the quit and the
//! puzzle reset (H), the warp frame pass by pass with players who shoot
//! and who do not (E), and the files drawn: the octopus's on the faces,
//! the crosshair at the centre (G).
//!
//! The T, K and U rows start from the same state (`fresh`): the camera's
//! node the identity, no shake, the mode 0, the crosshair's frame 0, no
//! cursor, no hand, the six `BigTenta<k>Died` 0, `EnergieJoueur` 10, the
//! view's limits (no yaw limit, the pitch within 70), the crystal not in
//! the bag, `srand(seed)`, then view 16 entered at (0, 0). Their changes are
//! tokens: `rK.F=V` a field of record K (`c` countdown, `st` state, `at`
//! the attacking tentacle, `n` the tentacles left, `hb` the small body's
//! hits, `bb` / `sb` the blinks, `sec` the sector, `p` / `l` present and
//! looked at, `body` / `death` / `tsI` slots, `tI` a tentacle's hits),
//! `sN.F=V` slot N's frame / click kind / active / playing, `copy=A>B`, `hit`
//! the sprite a click hit, `crystal`, `E`, `dK` (`BigTenta<K>Died`), `big`,
//! `count`, `cross`, `mode`, `yaw` / `pitch` (the radians' bits given to
//! `AngleWarpXNow` / `YNow`), `view` (the current view's number), `seed`,
//! `shake` (counter, rounds, the field of view's bits), `zone7af=Z` (view
//! 16's zone Z made type 0x7af).
//!
//! The fight played with the mouse is `tests/flow.rs`'s
//! (`the_end_of_the_game_with_the_mouse`), its two ends
//! `tests/endgame.rs`'s.
//!
//! Needs the game: `ATLANTIS2_DATA` and `ATLANTIS2_DISCS` (see
//! `tests/warp.rs`); without them the tests are skipped (B and S's
//! control word, tenths and bounds run always; S's yaw rows need the
//! game).

use std::cell::RefCell;
use std::path::PathBuf;

use atlantis2_data::defnames;
use atlantis2_data::spr::{SprBank, Target};
use atlantis2_data::spw::Spw;
use atlantis2_data::wam::EntityState;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::anim::Anim;
use atlantis2_engine::crt::Rand;
use atlantis2_engine::engine::{
    octopus_data, octopus_random, octopus_sector, Shake, OCTOPUS_ENTRY, OCTOPUS_MODE, OCTOPUS_TICK,
};
use atlantis2_engine::{CentreDraw, CursorBank, Engine, Event, Fpu, LoopType, Variant};
use md5::{Digest, Md5};

const GOLDEN: &str = include_str!("golden/octopus.txt");

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        atlantis2_data::testing::missing("ATLANTIS2_DATA / ATLANTIS2_DISCS not set");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

thread_local! {
    /// The warnings this test's thread logged.
    static LOGGED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            LOGGED.with(|l| l.borrow_mut().push(r.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn capture() {
    static CAPTURE: Capture = Capture;
    if log::set_logger(&CAPTURE).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    LOGGED.with(|l| l.borrow_mut().clear());
}

/// The module's warnings logged since the last call, as the golden's
/// `warning:<text>` entries.
fn warnings() -> String {
    let w: Vec<String> = LOGGED.with(|l| {
        l.borrow_mut()
            .drain(..)
            .filter(|m| m.starts_with("load_poulpes"))
            .map(|m| format!("warning:{m}"))
            .collect()
    });
    if w.is_empty() {
        "-".into()
    } else {
        w.join(";")
    }
}

/// The golden's rows starting with `prefix`, in order.
fn rows(prefix: &str) -> Vec<&'static str> {
    GOLDEN.lines().filter(|l| l.starts_with(prefix)).collect()
}

fn lst<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    let v: Vec<String> = xs.into_iter().map(|x| x.to_string()).collect();
    format!("[{}]", v.join(","))
}

/// The generator whose next `rand()` is `r`.
fn forced(r: i32) -> Rand {
    let inv: u32 = {
        // 214013's inverse mod 2^32.
        let mut x: u32 = 1;
        for _ in 0..5 {
            x = x.wrapping_mul(2u32.wrapping_sub(214_013u32.wrapping_mul(x)));
        }
        x
    };
    Rand::new(((r as u32) << 16).wrapping_sub(2_531_011).wrapping_mul(inv))
}

fn calls(from: Rand, to: Rand) -> usize {
    let mut r = from;
    for n in 0..10_000 {
        if r == to {
            return n;
        }
        r.rand();
    }
    panic!("rand state not reached");
}

#[test]
fn random_as_the_original() {
    let golden = rows("B r ");
    assert_eq!(golden.len(), 8);
    for row in golden {
        let r: i32 = row.split_whitespace().nth(2).unwrap().parse().unwrap();
        let cells: Vec<String> = [0, 1, 2, 0x27, 0x32, 100]
            .iter()
            .map(|&n| format!("{n}:{}", octopus_random(&mut forced(r), n)))
            .collect();
        assert_eq!(format!("B r {r} -> {}", cells.join(" ")), row);
    }
}

#[test]
fn the_sectors_as_the_original() {
    // The golden's control word, the one the rows S are run under.
    let fpu = Fpu::Chop;
    assert_eq!(format!("S cw {:04x}", fpu.control_word()), rows("S cw ")[0]);
    let tenths = rows("S tenths ")[0];
    let mut runs = Vec::new();
    let (mut prev, mut start) = (None, 0);
    for t in -4000..=4000 {
        let s = octopus_sector(fpu, t as f32 / 10.0);
        if Some(s) != prev {
            if let Some(p) = prev {
                runs.push(format!(
                    "{:.1}..{:.1}:{p}",
                    f64::from(start) / 10.0,
                    f64::from(t - 1) / 10.0
                ));
            }
            prev = Some(s);
            start = t;
        }
    }
    runs.push(format!(
        "{:.1}..400.0:{}",
        f64::from(start) / 10.0,
        prev.unwrap()
    ));
    assert_eq!(format!("S tenths {}", runs.join(" ")), tenths);
    let bounds = rows("S bound ");
    assert_eq!(bounds.len(), 24);
    for row in bounds {
        let w: Vec<&str> = row.split_whitespace().collect();
        for cell in &w[3..] {
            let (bits, s) = cell.split_once(':').unwrap();
            let y = f32::from_bits(u32::from_str_radix(bits, 16).unwrap());
            assert_eq!(octopus_sector(fpu, y).to_string(), s, "{row}");
        }
    }
}

// --- an engine in Atlantis ---------------------------------------------------

/// A game in chapter 82 in `atlantis.wam` view 16: a new game's film
/// ended, then the world, the place, the view and the chapter.
fn in_atlantis(files: A2Files) -> Engine {
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480).unwrap();
    e.start_new_game().unwrap();
    e.end_video();
    for c in [
        "(monde=4)",
        "(changewam=atlantis.wam)",
        "(vue=16)",
        "(chapter=82)",
    ] {
        e.run_command(c.as_bytes());
    }
    for _ in 0..400 {
        match e.loop_type {
            LoopType::Video => e.end_video(),
            LoopType::Dialog => e.end_dialog(),
            _ => {}
        }
        e.tick();
        e.pass(None);
        if e.loop_type == LoopType::Warp && !e.turning() {
            break;
        }
    }
    e.trace = true;
    e
}

fn run(e: &mut Engine, c: &str) {
    e.run_command(c.as_bytes());
}

fn var(e: &Engine, name: &str) -> i32 {
    e.script_state().and_then(|s| s.var(name)).unwrap_or(0)
}

/// The T, K and U rows' common start (the module doc's).
fn fresh(e: &mut Engine, seed: u32) {
    e.camera.fpu = Fpu::Chop;
    e.camera.q = [0.0, 0.0, 0.0, 1.0];
    e.shake = Shake {
        counter: 0,
        repeats: 0,
        fov: 76.0,
    };
    e.set_puzzle_mode(0);
    e.octopus.cross = 0;
    e.cursor = Default::default();
    e.hand = None;
    e.hand_puzzle = None;
    for k in 1..=6 {
        run(e, &format!("(BigTenta{k}Died=0)"));
    }
    run(e, "(EnergieJoueur=10)");
    run(e, "(LIMITEANGLEWARPX=-1)");
    run(e, "(LIMITEANGLEWARPY=70)");
    run(e, "(sacados=NOT_CRISTAL_ENGLOUTI)");
    let state = e.sprite_under_centre().1;
    e.set_sprite_under_centre(None, state);
    e.rand = Rand::new(seed);
    e.enter_at(16, 0.0, 0.0).unwrap();
    e.set_sprite_under_centre(None, state);
}

fn slot_of(v: &str) -> Option<usize> {
    if v == "-" {
        None
    } else {
        Some(v.parse().unwrap())
    }
}

fn bits(v: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(v, 16).unwrap())
}

/// The tokens, in order.
fn apply(e: &mut Engine, tokens: &[&str]) {
    for t in tokens {
        let (k, v) = t.split_once('=').unwrap();
        let int = || v.parse::<i32>().unwrap();
        if let Some((r, f)) = k.strip_prefix('r').and_then(|x| x.split_once('.')) {
            let r = &mut e.octopus.records[r.parse::<usize>().unwrap()];
            match f {
                "c" => r.countdown = int(),
                "bb" => r.big_blink = int(),
                "st" => r.state = int(),
                "at" => r.attacking = int(),
                "n" => r.left = int(),
                "hb" => r.body_hits = int(),
                "sb" => r.small_blink = int(),
                "sec" => r.sector = int(),
                "body" => r.body = slot_of(v),
                "death" => r.death = slot_of(v),
                "p" => r.present = int() != 0,
                "l" => r.looked = int() != 0,
                _ if f.starts_with("ts") => {
                    r.tentacles[f[2..].parse::<usize>().unwrap()] = slot_of(v)
                }
                _ if f.starts_with('t') => r.hits[f[1..].parse::<usize>().unwrap()] = int(),
                _ => panic!("{t}"),
            }
        } else if let Some((n, f)) = k.strip_prefix('s').and_then(|x| x.split_once('.')) {
            let s = &mut e.anim.slots[n.parse::<usize>().unwrap()];
            match f {
                "f" => s.frame = int(),
                "k" => s.click = int(),
                "a" => s.active = int() != 0,
                "p" => s.playing = int() != 0,
                _ => panic!("{t}"),
            }
        } else {
            match k {
                "copy" => {
                    let (a, b) = v.split_once('>').unwrap();
                    let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
                    e.octopus.records[b] = e.octopus.records[a];
                }
                "hit" => {
                    let state = e.sprite_under_centre().1;
                    let n = int();
                    e.set_sprite_under_centre((n >= 0).then_some(n as usize), state);
                }
                "crystal" => run(
                    e,
                    if v == "1" {
                        "(sacados=CRISTAL_ENGLOUTI)"
                    } else {
                        "(sacados=NOT_CRISTAL_ENGLOUTI)"
                    },
                ),
                "E" => run(e, &format!("(EnergieJoueur={v})")),
                "big" => e.octopus.big = int() != 0,
                "count" => e.octopus.count = int(),
                "cross" => e.octopus.cross = int(),
                "mode" => e.set_puzzle_mode(int()),
                "yaw" => e.camera.set_yaw(bits(v)),
                "pitch" => e.camera.pitch_from_here(bits(v)),
                "view" => e.set_view_number(v.parse().unwrap()),
                "seed" => e.rand = Rand::new(v.parse().unwrap()),
                "shake" => {
                    let w: Vec<&str> = v.split(',').collect();
                    e.shake = Shake {
                        counter: w[0].parse().unwrap(),
                        repeats: w[1].parse().unwrap(),
                        fov: bits(w[2]),
                    };
                }
                "zone7af" => {
                    let z = int() as usize;
                    let v16 = e
                        .wam_mut()
                        .places
                        .iter_mut()
                        .flat_map(|p| p.views.iter_mut())
                        .find(|v| v.id == 16)
                        .unwrap();
                    v16.zones[z].kind = 0x7af;
                }
                _ if k.starts_with('d') => run(e, &format!("(BigTenta{}Died={v})", &k[1..])),
                _ => panic!("{t}"),
            }
        }
    }
}

// --- the rows -------------------------------------------------------------

fn slot_no(n: Option<usize>) -> String {
    n.map_or("-".into(), |n| n.to_string())
}

fn globals_row(e: &Engine) -> String {
    let o = &e.octopus;
    format!(
        "mode {} big {} count {} cross {} bank {} table {} prefix {} body {}",
        e.puzzle_mode(),
        u8::from(o.big),
        o.count,
        o.cross,
        if o.bank.is_some() { "viseur" } else { "0x0" },
        lst(o.sectors),
        o.prefix,
        o.body
    )
}

fn rec_row(e: &Engine, k: usize) -> String {
    let r = &e.octopus.records[k];
    let t: Vec<String> = (0..6)
        .map(|i| format!("{}:{}", slot_no(r.tentacles[i]), r.hits[i]))
        .collect();
    format!(
        "r{k} c{} p{} l{} st{} at{} n{} hb{} bb{} sb{} sec{} body {} death {} t[{}]",
        r.countdown,
        u8::from(r.present),
        u8::from(r.looked),
        r.state,
        r.attacking,
        r.left,
        r.body_hits,
        r.big_blink,
        r.small_blink,
        r.sector,
        slot_no(r.body),
        slot_no(r.death),
        t.join(",")
    )
}

fn recs_row(e: &Engine) -> String {
    let v: Vec<String> = (0..e.octopus.live()).map(|k| rec_row(e, k)).collect();
    if v.is_empty() {
        "none".into()
    } else {
        v.join(" ; ")
    }
}

fn octo_slots(e: &Engine) -> Vec<usize> {
    let mut v: Vec<usize> = e.octopus.records[..e.octopus.live()]
        .iter()
        .flat_map(|r| {
            r.tentacles
                .iter()
                .chain([&r.body, &r.death])
                .flatten()
                .copied()
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn slot_row(e: &Engine, n: usize) -> String {
    let s = &e.anim.slots[n];
    format!(
        "s{n}:{}/{}{} f{} e{} k{} a{} p{}",
        s.id,
        char::from(s.state.letter as u8),
        s.state.anim,
        s.frame,
        s.end,
        s.click,
        u8::from(s.active),
        u8::from(s.playing)
    )
}

fn slots_row(e: &Engine) -> String {
    let v: Vec<String> = octo_slots(e).iter().map(|&n| slot_row(e, n)).collect();
    if v.is_empty() {
        "none".into()
    } else {
        v.join(" ")
    }
}

fn camera_row(e: &Engine) -> String {
    let c = &e.camera;
    let (y, p) = c.angles();
    format!(
        "q {:08x},{:08x},{:08x},{:08x} yp {:08x},{:08x} ({:.4},{:.4}) fov {:08x} fx {:08x} shake {}/{}/{:08x}",
        c.q[0].to_bits(),
        c.q[1].to_bits(),
        c.q[2].to_bits(),
        c.q[3].to_bits(),
        y.to_bits(),
        p.to_bits(),
        f64::from(y).to_degrees(),
        f64::from(p).to_degrees(),
        c.fov.to_bits(),
        c.fx.to_bits(),
        e.shake.counter,
        e.shake.repeats,
        e.shake.fov.to_bits()
    )
}

fn vars_row(e: &Engine) -> String {
    let mut v = vec![format!("EnergieJoueur={}", var(e, "EnergieJoueur"))];
    for k in 1..=6 {
        v.push(format!(
            "BigTenta{k}Died={}",
            var(e, &format!("BigTenta{k}Died"))
        ));
    }
    v.join(" ")
}

/// The crystal's id, from the golden's `A ids` row.
fn crystal_id() -> i32 {
    rows("A ids ")[0]
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix("CRISTAL_ENGLOUTI="))
        .unwrap()
        .parse()
        .unwrap()
}

fn bag_row(e: &Engine) -> String {
    let id = crystal_id();
    match e.bag_entries().iter().find(|b| b.id == id) {
        Some(b) => format!("crystal in bag world {}", b.world),
        None => "crystal not in bag".into(),
    }
}

fn zones_row(e: &Engine) -> String {
    let v = e.wam().view(16).unwrap();
    v.zones
        .iter()
        .map(|z| z.kind.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn after_row(e: &Engine) -> String {
    format!(
        "{} | {} | {} | {} | {} | {} | zones {}",
        globals_row(e),
        recs_row(e),
        slots_row(e),
        camera_row(e),
        vars_row(e),
        bag_row(e),
        zones_row(e)
    )
}

fn entry_row(e: &Engine) -> String {
    format!(
        "{} | {} | {} | {}",
        globals_row(e),
        recs_row(e),
        slots_row(e),
        camera_row(e)
    )
}

/// The commands and scans the fight ran, as the golden's `cmds`.
fn commands(evs: &[Event]) -> Vec<String> {
    evs.iter()
        .filter_map(|ev| match ev {
            Event::OctopusCommand(c) => Some(c.to_string()),
            Event::OctopusScan => Some("scan".into()),
            _ => None,
        })
        .collect()
}

fn banks(before: bool, after: bool) -> String {
    match (before, after) {
        (false, true) => lst(["load viseur.spr"]),
        (true, false) => lst(["free 0x33330000"]),
        _ => "[]".into(),
    }
}

/// The golden's `warn` field against the scripts' fatal error: `-` when
/// none, the original's fatal error otherwise.
fn warn_matches(e: &Engine, golden: &str) -> bool {
    let fatal = e.script_state().and_then(|s| s.fatal.as_ref()).is_some();
    if golden == "-" {
        !fatal
    } else {
        golden.starts_with("fatal:") && fatal
    }
}

fn split_row(row: &str) -> (&str, Vec<&str>, &str) {
    // "<tag> <label> | <tokens> -> <rest>"
    let (head, rest) = row.split_once(" -> ").unwrap();
    let (label, tokens) = head.rsplit_once(" | ").unwrap();
    let tokens = if tokens == "-" {
        Vec::new()
    } else {
        tokens.split(' ').collect()
    };
    (label, tokens, rest)
}

#[test]
fn the_data_as_the_original() {
    let Some(files) = game() else { return };
    let e = in_atlantis(files);
    for row in rows("A entity ") {
        let w: Vec<&str> = row.split_whitespace().collect();
        let ent = e
            .wam()
            .entities
            .iter()
            .find(|x| x.con_name == w[2])
            .unwrap();
        let states: Vec<String> = ent
            .state_records()
            .map(|s| format!("{}{}@{}", char::from(s.letter as u8), s.anim, s.view))
            .collect();
        assert_eq!(
            format!(
                "A entity {} id {} prefix {} states {}",
                ent.con_name,
                ent.index,
                ent.prefix,
                states.join(",")
            ),
            row
        );
    }
    // The module's constants, as the port holds them.
    let data = octopus_data();
    assert_eq!(
        format!("A strings {}", data.strings.join(" | ")),
        rows("A strings ")[0]
    );
    let floats: Vec<String> = data
        .floats
        .iter()
        .map(|(a, b)| format!("{a:08x}={b:08x}"))
        .collect();
    assert_eq!(
        format!("A floats {}", floats.join(" ")),
        rows("A floats ")[0]
    );
    let doubles: Vec<String> = data
        .doubles
        .iter()
        .map(|(a, b)| format!("{a:08x}={b:016x}"))
        .collect();
    assert_eq!(
        format!("A doubles {}", doubles.join(" ")),
        rows("A doubles ")[0]
    );
    assert_eq!(
        format!("A shake table {}", lst(data.rolls)),
        rows("A shake table ")[0]
    );
    // The ids, in the sections the port reads them from: the crystal an
    // object, the bodies characters.
    let names = &e.script_state().unwrap().names;
    let mut ids = vec![format!(
        "CRISTAL_ENGLOUTI={}",
        names.id(defnames::OBJETS, b"CRISTAL_ENGLOUTI").unwrap()
    )];
    for n in [
        "GDP_CORPS_V1",
        "GDP_CORPS_V2",
        "PTP_CORPS_V1",
        "PTP_CORPS_V4",
    ] {
        ids.push(format!(
            "{n}={}",
            names.id(defnames::PERSONNAGES, n.as_bytes()).unwrap()
        ));
    }
    assert_eq!(format!("A ids {}", ids.join(" ")), rows("A ids ")[0]);
    // Every row of A is one of the above.
    let checked = [
        "A entity ",
        "A strings ",
        "A floats ",
        "A doubles ",
        "A shake table ",
        "A ids ",
    ];
    for row in rows("A ") {
        assert!(checked.iter().any(|p| row.starts_with(p)), "{row}");
    }
}

#[test]
fn the_entry_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    let golden = rows("C ");
    let mut n = 0;
    for row in &golden {
        let (head, rest) = row.split_once(" -> ").unwrap();
        let w: Vec<&str> = head.split_whitespace().collect();
        let before = e.octopus.bank.is_some();
        let from;
        if w[1] == "view" {
            let (view, seed): (u32, u32) = (w[2].parse().unwrap(), w[4].parse().unwrap());
            let (yaw, pitch) = w[6].split_once(',').unwrap();
            let (yaw, pitch): (f64, f64) = (yaw.parse().unwrap(), pitch.parse().unwrap());
            if view == 19 {
                // No panorama of view 19 on any disc: the port refuses
                // the entry before the hook.
                continue;
            }
            e.set_puzzle_mode(0);
            e.camera.q = [0.0, 0.0, 0.0, 1.0];
            e.shake = Shake {
                counter: 0,
                repeats: 0,
                fov: 76.0,
            };
            e.rand = Rand::new(seed);
            from = e.rand;
            e.enter_at(view, yaw.to_radians() as f32, pitch.to_radians() as f32)
                .unwrap();
            let got = format!(
                "{head} -> {} | rand {} state {} | banks {}",
                entry_row(&e),
                calls(from, e.rand),
                e.rand.state,
                banks(before, e.octopus.bank.is_some())
            );
            assert_eq!(format!("{got} | warn -"), **row);
        } else if w[1] == "dead" {
            let dead: Vec<u32> = w[2]
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|x| x.parse().unwrap())
                .collect();
            for k in 1..=6 {
                run(
                    &mut e,
                    &format!("(BigTenta{k}Died={})", u8::from(dead.contains(&k))),
                );
            }
            e.camera.q = [0.0, 0.0, 0.0, 1.0];
            e.shake = Shake {
                counter: 0,
                repeats: 0,
                fov: 76.0,
            };
            e.rand = Rand::new(7);
            from = e.rand;
            e.enter_at(16, 0.0, 0.0).unwrap();
            let got = format!(
                "{head} -> {} | rand {} state {} | banks {} | warn -",
                entry_row(&e),
                calls(from, e.rand),
                e.rand.state,
                banks(before, e.octopus.bank.is_some())
            );
            assert_eq!(got, **row);
        } else {
            // The hook again on view 16's slots.
            for k in 1..=6 {
                run(&mut e, &format!("(BigTenta{k}Died=0)"));
            }
            e.camera.q = [0.0, 0.0, 0.0, 1.0];
            e.rand = Rand::new(3);
            e.enter_at(16, 0.0, 0.0).unwrap();
            e.rand = Rand::new(3);
            from = e.rand;
            capture();
            e.octopus_entry();
            let got = format!(
                "{head} -> {} | rand {} | slots {} | banks {} | warn {}",
                entry_row(&e),
                calls(from, e.rand),
                e.anim.slots.len(),
                banks(true, e.octopus.bank.is_some()),
                warnings()
            );
            assert_eq!(got, **row);
        }
        let _ = rest;
        n += 1;
    }
    assert!(n >= 25, "{n}");
}

#[test]
fn the_tick_s_yaw_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    let golden = rows("S yaw ");
    assert!(!golden.is_empty());
    for row in golden {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (xb, pb) = w[6].split_once(',').unwrap();
        e.camera.q = [0.0, 0.0, 0.0, 1.0];
        e.shake = Shake {
            counter: 0,
            repeats: 0,
            fov: 76.0,
        };
        e.rand = Rand::new(1);
        e.enter_at(16, 0.0, 0.0).unwrap();
        e.camera.set_yaw(bits(xb));
        e.camera.pitch_from_here(bits(pb));
        let fpu = e.camera.fpu;
        let (y, p) = e.camera.angles();
        let deg = atlantis2_engine::engine::octopus_degrees(fpu, y);
        let got = format!(
            "S yaw {} pitch {} set {} -> read {:08x},{:08x} deg {:08x} ({:.4}) sector {}",
            w[2],
            w[4],
            w[6],
            y.to_bits(),
            p.to_bits(),
            deg.to_bits(),
            f64::from(deg),
            octopus_sector(fpu, deg)
        );
        assert_eq!(got, row);
    }
}

#[test]
fn the_shake_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    let golden = rows("K ");
    let mut i = 0;
    let mut n = 0;
    while i < golden.len() {
        let (label, tokens, _) = split_row(golden[i]);
        fresh(&mut e, 1);
        apply(&mut e, &tokens);
        let tok = if tokens.is_empty() {
            "-".to_string()
        } else {
            tokens.join(" ")
        };
        assert_eq!(format!("{label} | {tok} -> {}", camera_row(&e)), golden[i]);
        i += 1;
        while i < golden.len() && golden[i].starts_with("K   ") {
            let call = golden[i][4..].split(' ').next().unwrap();
            match call {
                "step" => e.shake_step(),
                "reset" => e.camera_reset(),
                h => e.shake_start(h[3..].parse().unwrap()),
            }
            assert_eq!(format!("K   {call} -> {}", camera_row(&e)), golden[i]);
            i += 1;
            n += 1;
        }
    }
    assert!(n > 60, "{n}");
}

#[test]
fn the_tick_alone_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    let golden = rows("T ");
    assert!(golden.len() >= 70, "{}", golden.len());
    for row in golden {
        let (label, tokens, rest) = split_row(row);
        // A case that ended the fight left another chapter: back to 82.
        let chapter = e.script_state().map_or(0, |s| s.chapter);
        if e.place() != ("atlantis.wam", 16) || chapter != 82 {
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
            for c in [
                "(monde=4)",
                "(changewam=atlantis.wam)",
                "(vue=16)",
                "(chapter=82)",
            ] {
                run(&mut e, c);
            }
            if e.loop_type == LoopType::Video {
                e.end_video();
            }
        }
        fresh(&mut e, 1);
        apply(&mut e, &tokens);
        if !tokens.iter().any(|t| t.starts_with("mode=")) {
            e.set_puzzle_mode(OCTOPUS_MODE);
        }
        let before = e.octopus.bank.is_some();
        let from = e.rand;
        e.events.clear();
        e.octopus_tick();
        let evs = std::mem::take(&mut e.events);
        let warn = rest.rsplit_once(" | warn ").unwrap().1;
        let got = format!(
            "{label} | {} -> {} | cmds {} | rand {} | banks {}",
            if tokens.is_empty() {
                "-".to_string()
            } else {
                tokens.join(" ")
            },
            after_row(&e),
            lst(commands(&evs)),
            calls(from, e.rand),
            banks(before, e.octopus.bank.is_some())
        );
        assert_eq!(format!("{got} | warn {warn}"), row);
        assert!(warn_matches(&e, warn), "{row}");
        if let Some(s) = e.script_state() {
            if s.fatal.is_some() {
                // A fatal error stops the scripts: a new game for the rest.
                let files = game().unwrap();
                e = in_atlantis(files);
            }
        }
    }
}

#[test]
fn the_cursor_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    let golden = rows("U ");
    assert_eq!(golden.len(), 20);
    for row in golden {
        let (label, tokens, _) = split_row(row);
        fresh(&mut e, 1);
        e.set_puzzle_mode(OCTOPUS_MODE);
        e.set_sprite_under_centre(None, 0);
        e.bar.open = false;
        e.set_centre_zone(None);
        let mut rest = Vec::new();
        let mut zone_kind = None;
        for t in &tokens {
            let (k, v) = t.split_once('=').unwrap();
            match k {
                "state" => e.set_sprite_under_centre(None, v.parse().unwrap()),
                "bag" => e.bar.open = v == "1",
                "zone" => zone_kind = Some(v.parse::<i32>().unwrap()),
                "hand" => e.hand = Some((0, 3)),
                _ => rest.push(*t),
            }
        }
        apply(&mut e, &rest);
        let saved = e.wam().view(16).unwrap().zones[0].kind;
        if let Some(kind) = zone_kind {
            let v16 = e
                .wam_mut()
                .places
                .iter_mut()
                .flat_map(|p| p.views.iter_mut())
                .find(|v| v.id == 16)
                .unwrap();
            v16.zones[0].kind = kind;
            e.set_centre_zone(Some(0));
        }
        let plan = e.octopus_cursor_alone();
        let r = u8::from(plan.is_some());
        let draws: Vec<String> = plan
            .unwrap_or_default()
            .iter()
            .map(|d| match d {
                CentreDraw::Crosshair { frame, x, y } => format!("spr viseur#{frame}@{x},{y}"),
                CentreDraw::Sprite { bank, frame, x, y } => {
                    let b = match bank {
                        CursorBank::Interface => "interface",
                        CursorBank::Objects => "objects",
                        CursorBank::Puzzle => "puzzle",
                        CursorBank::Menu => "menu",
                    };
                    format!("spr {b}#{frame}@{x},{y}")
                }
                CentreDraw::Line { .. } => "line".into(),
            })
            .collect();
        let cursor = match e.cursor.shown {
            Some(f) => format!("interface#{f}"),
            None => "none".into(),
        };
        let hand = match e.hand {
            Some((_, s)) => format!("objects#{s}"),
            None => "none".into(),
        };
        let tok = if tokens.is_empty() {
            "-".to_string()
        } else {
            tokens.join(" ")
        };
        assert_eq!(
            format!(
                "{label} | {tok} -> {r} | {} | cursor {cursor} range {} hand {hand}",
                lst(draws),
                e.cursor.start
            ),
            row
        );
        let v16 = e
            .wam_mut()
            .places
            .iter_mut()
            .flat_map(|p| p.views.iter_mut())
            .find(|v| v.id == 16)
            .unwrap();
        v16.zones[0].kind = saved;
        e.bar.open = false;
        e.set_centre_zone(None);
    }
}

/// The functions of the golden's H rows the engine's events show entered,
/// in order.
fn entered(e: &Engine) -> String {
    let v: Vec<&str> = e
        .events
        .iter()
        .filter_map(|x| match x {
            Event::PuzzleRan { function } => match *function {
                OCTOPUS_ENTRY => Some("octopus-entry"),
                OCTOPUS_TICK => Some("octopus-tick"),
                0x0048_df10 => Some("souls-entry"),
                0x0049_c340 => Some("ring-entry"),
                _ => None,
            },
            Event::OctopusQuit => Some("octopus-quit"),
            Event::CameraReset => Some("camera-reset"),
            _ => None,
        })
        .collect();
    if v.is_empty() {
        "-".into()
    } else {
        v.join(",")
    }
}

/// `f` run with the current view the golden's fake record: view 16's
/// record named `name` and numbered `number` (a record already numbered
/// so set aside), the current view's number `number`; then all put back.
fn with_fake_view(e: &mut Engine, name: &str, number: u32, f: impl FnOnce(&mut Engine)) {
    let mut saved = Vec::new();
    for (pi, p) in e.wam_mut().places.iter_mut().enumerate() {
        for (vi, v) in p.views.iter_mut().enumerate() {
            if v.id == 16 {
                saved.push((pi, vi, v.id, v.name.clone()));
                v.name = name.into();
                v.id = number;
            } else if v.id == number {
                saved.push((pi, vi, v.id, v.name.clone()));
                v.id = u32::MAX;
            }
        }
    }
    e.set_view_number(number);
    f(e);
    for (pi, vi, id, name) in saved {
        let v = &mut e.wam_mut().places[pi].views[vi];
        v.id = id;
        v.name = name;
    }
    e.set_view_number(16);
}

#[test]
fn the_dispatchers_as_the_original() {
    let Some(files) = game() else { return };
    let mut e = in_atlantis(files);
    // The entry dispatcher (FUN_0049bd50) itself, on a fake view record
    // as the golden's.
    for row in rows("H entry ") {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (name, number): (&str, u32) = (w[2], w[3].parse().unwrap());
        fresh(&mut e, 1);
        e.octopus_quit();
        let mut got = String::new();
        with_fake_view(&mut e, name, number, |e| {
            e.events.clear();
            e.entry_hooks();
            got = format!(
                "H entry {name} {number} -> {} | mode {} | {}",
                entered(e),
                e.puzzle_mode(),
                recs_row(e)
            );
        });
        assert_eq!(got, row);
    }
    for row in rows("H tick mode ") {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (mode, pause): (i32, i32) = (w[3].parse().unwrap(), w[5].parse().unwrap());
        if pause != 0 {
            // The pause byte (@ 0x006d85f0) is not in the port.
            continue;
        }
        fresh(&mut e, 1);
        e.set_puzzle_mode(mode);
        let first = e.puzzle_tick();
        let entered = if first == Some(OCTOPUS_TICK) {
            "octopus-tick"
        } else {
            "-"
        };
        assert_eq!(
            format!(
                "H tick mode {mode} pause 0 -> {entered} | mode {}",
                e.puzzle_mode()
            ),
            row
        );
    }
    // The quit, and the port's puzzle reset of a load. The golden's
    // reset rows start with the other modules' functions, which its
    // harness stubs (their own goldens cover them): the port's reset is
    // compared from the octopus's quit on.
    for row in rows("H quit mode ")
        .into_iter()
        .chain(rows("H reset mode "))
    {
        let w: Vec<&str> = row.split_whitespace().collect();
        let (label, mode): (&str, i32) = (w[1], w[3].parse().unwrap());
        fresh(&mut e, 1);
        e.camera.set_yaw(bits("40490fdb"));
        e.set_puzzle_mode(mode);
        let before = e.octopus.bank.is_some();
        e.events.clear();
        let others = if label == "quit" {
            e.octopus_quit();
            ""
        } else {
            e.puzzles_reset();
            "grid1-leave,grid2-leave,grid3-leave,ring-quit,canoe-quit,web-quit,souls-quit,"
        };
        // In the modes 1, 2 and 3 the web's, the turtle ring's and the
        // souls' quits, which the golden's harness stubs, clear their mode
        // (FUN_00491260, FUN_0049c270, FUN_0048eb10), in the port as in
        // the original.
        let want = if label == "reset" && (1..=3).contains(&mode) {
            row.replacen(&format!("| mode {mode} |"), "| mode 0 |", 1)
        } else {
            row.to_string()
        };
        assert_eq!(
            format!(
                "H {label} mode {mode} -> {others}{} | mode {} | banks {} | {}",
                entered(&e),
                e.puzzle_mode(),
                banks(before, e.octopus.bank.is_some()),
                camera_row(&e)
            ),
            want
        );
    }
}

// --- the warp frames --------------------------------------------------------

fn frame_row(e: &Engine) -> String {
    let (y, _) = e.camera.angles();
    let d: Vec<String> = (1..=6)
        .map(|i| format!("d{i}={}", var(e, &format!("BigTenta{i}Died"))))
        .collect();
    format!(
        "mode {} cross {} | {} | {} | yaw {:08x} fov {:08x} shake {}/{} | E={} {}",
        e.puzzle_mode(),
        e.octopus.cross,
        recs_row(e),
        slots_row(e),
        y.to_bits(),
        e.camera.fov.to_bits(),
        e.shake.counter,
        e.shake.repeats,
        var(e, "EnergieJoueur"),
        d.join(" ")
    )
}

/// The first record in state 2 or 4: its attacking tentacle's slot.
fn attacker(e: &Engine) -> Option<usize> {
    e.octopus.records[..e.octopus.live()]
        .iter()
        .find(|r| r.state == 2 || r.state == 4)
        .and_then(|r| r.tentacles[r.attacking as usize])
}

fn draws(e: &Engine) -> Vec<String> {
    e.centre_draws
        .iter()
        .map(|d| match d {
            CentreDraw::Crosshair { frame, x, y } => format!("viseur#{frame}@{x},{y}"),
            CentreDraw::Sprite { bank, frame, x, y } => {
                let b = match bank {
                    CursorBank::Interface => "interface",
                    CursorBank::Objects => "objects",
                    CursorBank::Puzzle => "puzzle",
                    CursorBank::Menu => "menu",
                };
                format!("{b}#{frame}@{x},{y}")
            }
            CentreDraw::Line { .. } => "line".into(),
        })
        .collect()
}

#[test]
fn the_warp_frames_as_the_original() {
    let Some(files) = game() else { return };
    let golden = rows("E ");
    let runs: Vec<usize> = (0..golden.len())
        .filter(|&i| golden[i].starts_with("E run "))
        .collect();
    assert_eq!(runs.len(), 4);
    let mut files = Some(files);
    for (r, &at) in runs.iter().enumerate() {
        let head: Vec<&str> = golden[at].split_whitespace().collect();
        let (label, seed, crystal, yaw, shoot): (&str, u32, bool, &str, bool) = (
            head[2],
            head[4].parse().unwrap(),
            head[6] == "1",
            head[8],
            head[10] == "1",
        );
        let end = runs.get(r + 1).copied().unwrap_or(golden.len());
        let mut want: Vec<&str> = golden[at..end].to_vec();
        let last = want.pop().unwrap();
        assert!(last.starts_with(&format!("E end {label} ")));
        let mut e = in_atlantis(files.take().unwrap_or_else(|| game().unwrap()));
        fresh(&mut e, seed);
        if crystal {
            run(&mut e, "(sacados=CRISTAL_ENGLOUTI)");
        }
        e.camera.set_yaw(bits(yaw));
        e.set_puzzle_mode(OCTOPUS_MODE);
        e.forced_centre = None;
        e.events.clear();
        let mut got = vec![format!(
            "E run {label} seed {seed} crystal {} yaw {yaw} shoot {} -> {}",
            u8::from(crystal),
            u8::from(shoot),
            frame_row(&e)
        )];
        let t0 = e.ticks;
        let mut prev = None;
        let mut clock = 100.0;
        // The runs' pass counts (the golden's).
        let passes = match label {
            "won" => 8000,
            "away" => 600,
            _ => 6000,
        };
        for n in 0..passes {
            if n % 4 == 0 {
                e.tick();
            }
            let mut click = false;
            e.forced_centre = None;
            if shoot {
                if let Some(a) = attacker(&e) {
                    let s = &e.anim.slots[a];
                    if let Some(b) = s.frame_box(1).filter(|b| b.w > 0 && b.h > 0) {
                        let (x, y, w, h) = (
                            i32::from(b.x),
                            i32::from(b.y),
                            i32::from(b.w),
                            i32::from(b.h),
                        );
                        e.forced_centre = Some((1, x + w / 2, y + h / 2));
                        clock += 10.0;
                        e.click(clock);
                        click = true;
                    }
                }
            }
            e.pass(None);
            let evs = std::mem::take(&mut e.events);
            let cmds: Vec<String> = commands(&evs).into_iter().filter(|c| c != "scan").collect();
            let films: Vec<String> = evs
                .iter()
                .filter_map(|ev| match ev {
                    Event::VideoStart { name, .. } => Some(format!("AtlHnm_start({name})")),
                    _ => None,
                })
                .collect();
            let d = draws(&e);
            let key = (
                (0..2)
                    .map(|k| {
                        let mut r = e.octopus.records[k];
                        r.countdown = 0;
                        r
                    })
                    .collect::<Vec<_>>(),
                octo_slots(&e)
                    .iter()
                    .map(|&j| (e.anim.slots[j].click, e.anim.slots[j].active))
                    .collect::<Vec<_>>(),
                d.clone(),
                e.puzzle_mode(),
            );
            let changed = prev.as_ref() != Some(&key);
            if changed || !cmds.is_empty() || !films.is_empty() || click || n % 50 == 49 {
                let mut row = format!(
                    "E p{n} t{}{} | {} | {} | {}",
                    e.ticks.wrapping_sub(t0),
                    if click { " click" } else { "" },
                    frame_row(&e),
                    lst(d),
                    lst(cmds.iter())
                );
                if !films.is_empty() {
                    row += &format!(" | {}", films.join("; "));
                }
                got.push(row);
            }
            prev = Some(key);
            if cmds.iter().any(|c| c.starts_with("(chapter=")) {
                break;
            }
        }
        if let Some(i) = (0..got.len().max(want.len()))
            .find(|&i| got.get(i).map(String::as_str) != want.get(i).copied())
        {
            panic!(
                "run {label}, row {i}:\n got {:?}\nwant {:?}",
                got.get(i),
                want.get(i)
            );
        }
        assert!(
            last.starts_with(&format!("E end {label} rand state {} ", e.rand.state)),
            "{last}"
        );
    }
}

// --- the drawing --------------------------------------------------------

#[test]
fn the_octopus_s_files_are_drawn_as_the_original() {
    let Some(files) = game() else { return };
    let bg: Vec<u16> = (0..512u64 * 512)
        .map(|i| (((i * 2_654_435_761) & 0xffff_ffff) >> 16) as u16)
        .collect();
    let md5_of = |px: &[u16]| -> String {
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        format!("{:x}", Md5::digest(&bytes))
    };
    let bg_md5 = md5_of(&bg);
    let golden: Vec<&str> = rows("G ")
        .into_iter()
        .filter(|r| !r.starts_with("G AFF_SPR") && !r.starts_with("G spr\\"))
        .collect();
    assert!(golden.len() > 50, "{}", golden.len());
    let mut got = Vec::new();
    let mut data = Vec::new();
    for row in &golden {
        if let Some(rest) = row.strip_prefix("G file ") {
            let name = rest.split(' ').next().unwrap();
            data = files.read(Folder::Spw, name).expect("the octopus's SPW");
            let spw = Spw::parse(data.clone()).unwrap();
            got.push(format!(
                "G file {name} md5 {:x} frames {}",
                Md5::digest(&data),
                spw.frame_count()
            ));
            continue;
        }
        let w: Vec<&str> = row.split(' ').collect();
        let (name, frame): (&str, i32) = (w[1], w[3].parse().unwrap());
        let spw = Spw::parse(data.clone()).unwrap();
        let mut a = Anim::new();
        a.load(
            name,
            Spw::parse(data.clone()).unwrap(),
            1,
            100,
            0,
            EntityState {
                letter: 65,
                anim: 1,
                shown: 1,
                single: 1,
                first: frame,
                ..Default::default()
            },
        );
        a.slots[0].frame = frame;
        let mut faces: [Vec<u16>; 6] = std::array::from_fn(|_| bg.clone());
        let (_, hit) = a.draw(&mut faces, None, true);
        let al = i32::from(hit.slot.is_some());
        let mut parts = Vec::new();
        for (k, f) in faces.iter().enumerate() {
            let b = spw.face_box(frame, k).unwrap_or_default();
            let (x, y, bw, bh) = (
                i32::from(b.x),
                i32::from(b.y),
                i32::from(b.w),
                i32::from(b.h),
            );
            let bx = format!("box {x},{y},{bw},{bh}");
            let h = md5_of(f);
            if h == bg_md5 {
                parts.push(format!("f{k} bg {bx}"));
                continue;
            }
            let n = f.iter().zip(&bg).filter(|(a, b)| a != b).count();
            let mut samples = Vec::new();
            if bw > 0 && bh > 0 {
                for (px, py) in [(x + bw / 2, y + bh / 2), (x, y), (x + bw - 1, y + bh - 1)] {
                    if (0..512).contains(&px) && (0..512).contains(&py) {
                        samples.push(format!("{px},{py}={:04x}", f[(py * 512 + px) as usize]));
                    }
                }
            }
            parts.push(format!("f{k} {h} n{n} {bx} {}", samples.join(" ")));
        }
        got.push(format!(
            "G {name} frame {frame} AL {al} | {}",
            parts.join(" | ")
        ));
    }
    assert_eq!(got, golden);
    // The crosshair at the centre.
    let data = files.read(Folder::Spr, "viseur.spr").unwrap();
    let bank = SprBank::parse(&data).unwrap();
    assert!(rows("G spr\\viseur.SPR md5 ")[0].contains(&format!("{:x}", Md5::digest(&data))));
    let (w, h) = (640usize, 480usize);
    let bg: Vec<u16> = (0..w * h)
        .map(|i| ((i as u64 * 2_654_435_761) as u32 >> 16) as u16)
        .collect();
    let aff = rows("G AFF_SPR ");
    assert_eq!(aff.len(), 3);
    for (f, row) in aff.iter().enumerate() {
        let mut px = bg.clone();
        bank.draw(&mut Target::screen(&mut px, w, h), Some(320), 240, f);
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        assert_eq!(
            format!("G AFF_SPR viseur#{f}@320,240 {:x}", Md5::digest(&bytes)),
            *row
        );
    }
}

/// The shots of `the_warp_frames_as_the_original` force the texel under
/// the centre to the middle of the attacker's box on face 1, as the
/// original's harness does (it replaces FUN_0047db60). That texel is where
/// the camera aims: turned to a texel of face 1, the port's FUN_0047db60
/// (compared with the original's in `tests/centre.rs`) gives that texel,
/// or one beside it at a face's edge.
#[test]
fn a_forced_texel_is_one_the_camera_aims_at() {
    use atlantis2_engine::Camera;
    for y in (3..509).step_by(23) {
        for x in (3..509).step_by(23) {
            // The face's camera (focal 254.99998, centre 256) sees the
            // texel's middle in this direction; face 1 is turned half a
            // turn about the vertical axis (its quaternion (0, 1, 0, 0)).
            let f = f64::from(f32::from_bits(0x437e_ffff));
            let (px, py) = (
                (f64::from(x) + 0.5 - 256.0) / f,
                (f64::from(y) + 0.5 - 256.0) / f,
            );
            let d = [-px, py, -1.0];
            let yaw = d[2].atan2(d[0]);
            let pitch = d[1].atan2((d[2] * d[2] + d[0] * d[0]).sqrt());
            let mut cam = Camera::new(Fpu::Chop, 640, 480);
            cam.enter(yaw as f32, pitch as f32);
            let (face, tx, ty) = cam.texel(cam.cx, cam.cy).unwrap();
            assert_eq!(face, 1, "texel {x},{y}");
            assert!(
                (tx - x).abs() <= 1 && (ty - y).abs() <= 1,
                "texel {x},{y}: the camera sees {tx},{ty}"
            );
        }
    }
}
