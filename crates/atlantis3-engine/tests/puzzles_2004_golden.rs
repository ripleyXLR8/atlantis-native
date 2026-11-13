//! The 2004 mini-games against goldens of the original's frames run under
//! emulation (no game data needed): a mock [`Host`] with no banks (no
//! sprite is ever touched), the clock, the frame times, the pointer and
//! the click of the golden's steps, and MSVC's `rand` with the golden's
//! seed.
//!
//! `cd2004_catapulte.txt`: `a4cd_catapulte.py` (FUN_00484180).
//! `cd2004_defender.txt`: `a4cd_defender.py` (FUN_00487460).

use std::collections::HashMap;
use std::sync::Arc;

use atlantis3_data::spr::Bank;
use atlantis3_engine::puzzles2004::catapulte::{Catapulte, State};
use atlantis3_engine::puzzles2004::defender;
use atlantis3_engine::puzzles2004::{Host, Puzzle};
use atlantis3_engine::px::Surface;

/// The emulation harness's world.
pub struct Mock {
    pub clock: f64,
    pub frame_t: f32,
    pub pointer: (i32, i32),
    pub click: bool,
    pub right_click: bool,
    pub keys: [bool; 256],
    pub key_code: i32,
    pub seed: u32,
    pub vars: HashMap<String, i32>,
    pub flags: Vec<i32>,
    pub surface: Surface,
    pub log: Vec<String>,
}

impl Mock {
    pub fn new(seed: u32) -> Mock {
        Mock {
            clock: 0.0,
            frame_t: 0.0,
            pointer: (0, 0),
            click: false,
            right_click: false,
            keys: [false; 256],
            key_code: 0,
            seed,
            vars: HashMap::new(),
            flags: vec![0; 1000],
            surface: Surface::Xrgb8888(vec![0; 800 * 600]),
            log: Vec::new(),
        }
    }
}

impl Host for Mock {
    fn clock(&self) -> f64 {
        self.clock
    }
    fn frame_time(&self) -> f32 {
        self.frame_t
    }
    fn pointer(&self) -> (i32, i32) {
        self.pointer
    }
    fn set_pointer(&mut self, x: i32, y: i32) {
        self.pointer = (x, y);
    }
    fn click(&self) -> bool {
        self.click
    }
    fn set_click(&mut self, on: bool) {
        self.click = on;
    }
    fn key(&self, vk: u8) -> bool {
        self.keys[usize::from(vk)]
    }
    fn srand_time(&mut self) {}
    fn rand(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(214013).wrapping_add(2531011);
        let v = ((self.seed >> 16) & 0x7fff) as i32;
        self.log.push(format!("rand {v}"));
        v
    }
    fn var(&self, name: &str) -> i32 {
        self.vars.get(name).copied().unwrap_or(0)
    }
    fn set_var(&mut self, name: &str, v: i32) {
        self.vars.insert(name.to_string(), v);
    }
    fn flag(&self, n: usize) -> i32 {
        self.flags[n]
    }
    fn set_flag(&mut self, n: usize, v: i32) {
        self.flags[n] = v;
    }
    fn bank(&mut self, _file: &str) -> Option<Arc<Bank>> {
        None
    }
    fn read(&mut self, _path: &str) -> Option<Vec<u8>> {
        None
    }
    fn sfx_load(&mut self, _file: &str) {}
    fn sfx_play(&mut self, file: &str) {
        self.log.push(format!("sfx {file}"));
    }
    fn sfx_remove(&mut self, _file: &str) {}
    fn surface(&mut self) -> &mut Surface {
        &mut self.surface
    }
    fn draw_pointer(&mut self) {}
    fn flip(&mut self) {}
    fn script_prologue(&mut self) -> bool {
        true
    }
    fn key_code(&self) -> i32 {
        self.key_code
    }
    fn set_key_code(&mut self, code: i32) {
        self.key_code = code;
    }
    fn right_click(&self) -> bool {
        self.right_click
    }
    fn set_right_click(&mut self, on: bool) {
        self.right_click = on;
    }
}

fn golden(name: &str) -> Vec<(String, Vec<String>)> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for l in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some(n) = l.strip_prefix("== ") {
            out.push((n.to_string(), Vec::new()));
        } else if out.is_empty() {
            out.push((String::new(), vec![l.to_string()]));
        } else {
            out.last_mut().unwrap().1.push(l.to_string());
        }
    }
    out
}

fn bits(x: f32) -> String {
    format!("{:08x}", x.to_bits())
}

fn dump(c: &Catapulte, m: &Mock, tag: &str) -> String {
    let clouds: Vec<String> = c.clouds.iter().map(|k| bits(k.x)).collect();
    format!(
        "{tag} state={} prev={} screen={} pos={},{} vel={},{} acc={},{} wind={} clouds={} impacts={} hits={},{} click={}",
        c.state as i32,
        c.prev,
        c.screen,
        bits(c.ball.pos.0),
        bits(c.ball.pos.1),
        bits(c.ball.vel.0),
        bits(c.ball.vel.1),
        bits(c.ball.acc.0),
        bits(c.ball.acc.1),
        bits(c.wind),
        clouds.join(","),
        c.impacts.len(),
        c.hits.0,
        c.hits.1,
        u8::from(m.click)
    )
}

#[test]
fn catapulte_flights() {
    for (name, want) in golden("cd2004_catapulte.txt") {
        let mut m = Mock::new(12345);
        let mut c = Catapulte::init(&mut m);
        m.log.clear();
        c.ball.damp = 0.0;
        let mut got = Vec::new();
        for (k, line) in want
            .iter()
            .filter(|l| l.as_bytes()[0].is_ascii_digit())
            .enumerate()
        {
            let step = line.split(' ').nth(1).unwrap();
            let p: Vec<&str> = step.split(':').collect();
            match p[0] {
                "aim" => {
                    c.state = State::Aim;
                    c.screen = 2;
                    c.wind = p[1].parse::<f64>().unwrap() as f32;
                }
                "click" => {
                    m.pointer = (p[1].parse().unwrap(), p[2].parse().unwrap());
                    m.click = true;
                }
                "clock" => m.clock = p[1].parse().unwrap(),
                "frame" => {
                    m.frame_t = p[1].parse::<f64>().unwrap() as f32;
                    c.frame(&mut m);
                }
                _ => panic!("{line}"),
            }
            got.append(&mut m.log);
            got.push(dump(&c, &m, &format!("{k:03} {step}")));
        }
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(g, w, "{name}: line {i}");
        }
        assert_eq!(got.len(), want.len(), "{name}");
    }
}

/// `a4cd_defender.py`'s level.
const DEFENDER_INI: &str = "[Deviant]\nPos 681 520\nIntervalleTir 2\nTempsDernierTir 0\nProbabilite 100\n[End]\n[Deviant]\nPos 716 520\nIntervalleTir 2\nProbabilite 70\n[End]\n[Deviant]\nPos 1015 515\nIntervalleTir 3\nTempsDernierTir 1\nProbabilite 80\n[End]\n[Energie]\nPos 852 179\n[End]\n[Dieu] KAMA\nPos 3236 232\nIntervalleTir 900\nTempsDernierTir 0\nProbabilite 100\n[End]\n[Dieu] ENNA\nPos 6053 237\nIntervalleTir 500\nTempsDernierTir 0\nProbabilite 100\n[End]\n[Dieu] ZANAT\nPos 6100 240\nIntervalleTir 900\nTempsDernierTir 0\nProbabilite 100\n[End]\n";

fn fnv(h: &mut u32, w: u32) {
    for b in w.to_le_bytes() {
        *h = (*h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
}

fn shot_words(s: &defender::Shot) -> [u32; 10] {
    let f = |x: f32| x.to_bits();
    [
        f(s.pos.0),
        f(s.pos.1),
        f(s.start.0),
        f(s.start.1),
        f(s.vel.0),
        f(s.vel.1),
        s.kind as u32,
        f(s.anim),
        s.frames as u32,
        s.speed as u32,
    ]
}

fn dev_words(d: &defender::Deviant) -> [u32; 9] {
    let f = |x: f32| x.to_bits();
    [
        f(d.pos.0),
        f(d.pos.1),
        f(d.frame),
        d.frames as u32,
        f(d.speed),
        d.state as u32,
        f(d.interval),
        f(d.timer),
        d.proba as u32,
    ]
}

fn exp_words(e: &defender::Explosion) -> [u32; 3] {
    [e.pos.0.to_bits(), e.pos.1.to_bits(), e.frame.to_bits()]
}

fn energy_words(e: &defender::Energy) -> [u32; 5] {
    let f = |x: f32| x.to_bits();
    [
        f(e.pos.0),
        f(e.pos.1),
        f(e.frame),
        e.frames as u32,
        f(e.speed),
    ]
}

fn hex(w: &[u32]) -> String {
    w.iter()
        .map(|w| format!("{w:08x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn defender_line(st: &defender::State, m: &Mock, tag: &str) -> String {
    let mut h = 0x811c_9dc5u32;
    for s in &st.shots {
        shot_words(s).iter().for_each(|&w| fnv(&mut h, w));
    }
    for d in &st.deviants {
        dev_words(d).iter().for_each(|&w| fnv(&mut h, w));
    }
    for e in &st.explosions {
        exp_words(e).iter().for_each(|&w| fnv(&mut h, w));
    }
    let s = &st.ship;
    let gods: Vec<String> = st
        .gods
        .iter()
        .enumerate()
        .map(|(k, g)| {
            format!(
                "g{k}={},{},{},{},{},{},{},{}",
                bits(g.pos.0),
                bits(g.pos.1),
                bits(g.energy),
                bits(g.timer),
                u8::from(g.active),
                bits(g.lorenz[0]),
                bits(g.lorenz[1]),
                bits(g.lorenz[2])
            )
        })
        .collect();
    format!(
        "{tag} ship={},{},{},{} e={} sp={} sc={} pc={},{} acc={} ptr={},{} code={} clk={}{} lives={} vars={},{} n={},{},{},{} {} h={h:08x}",
        bits(s.screen.0),
        bits(s.screen.1),
        bits(s.world.0),
        bits(s.world.1),
        bits(st.energy),
        bits(st.speed),
        bits(st.scroll),
        st.page,
        st.column,
        u8::from(st.accel),
        m.pointer.0,
        m.pointer.1,
        m.key_code,
        u8::from(m.click),
        u8::from(m.right_click),
        st.lives,
        m.var("DEFENDER_GAGNE"),
        m.var("DEFENDER_PERDU"),
        st.shots.len(),
        st.deviants.len(),
        st.energies.len(),
        st.explosions.len(),
        gods.join(" ")
    )
}

/// `PUZ_DEFENDER`'s frames against the original's (`cd2004_defender.txt`):
/// the ship by the pointer and the arrows, shots, bombs, the speed keys,
/// the deviants' shots, the fall at 0 energy, the scroll's acceleration
/// and stops, the gods' attractors and shots, the win.
#[test]
fn defender_frames() {
    use atlantis3_engine::puzzles2004::defender::{Defender, Level};
    let (_, want) = golden("cd2004_defender.txt").pop().unwrap();
    let mut m = Mock::new(12345);
    let mut d = Defender::without_data(Level::parse(DEFENDER_INI.as_bytes()), 600);
    let mut got = Vec::new();
    for (k, line) in want.iter().filter_map(|l| l.strip_prefix("> ")).enumerate() {
        got.push(format!("> {line}"));
        let p: Vec<&str> = line.split(' ').collect();
        match p[0] {
            "ptr" => m.pointer = (p[1].parse().unwrap(), p[2].parse().unwrap()),
            "keys" => {
                m.keys = [false; 256];
                for c in p[1].chars() {
                    let vk = match c {
                        'U' => 0x26,
                        'D' => 0x28,
                        'L' => 0x25,
                        'R' => 0x27,
                        _ => continue,
                    };
                    m.keys[vk] = true;
                }
            }
            "code" => m.key_code = i32::from_str_radix(p[1].trim_start_matches("0x"), 16).unwrap(),
            "click" => m.click = true,
            "rclick" => m.right_click = true,
            "scroll" => d.st.scroll = p[1].parse::<f64>().unwrap() as f32,
            "energy" => d.st.energy = p[1].parse::<f64>().unwrap() as f32,
            "god" => {
                d.st.gods[p[1].parse::<usize>().unwrap()].energy =
                    p[2].parse::<f64>().unwrap() as f32
            }
            "frame" => {
                m.frame_t = p[1].parse::<f64>().unwrap() as f32;
                d.frame(&mut m);
                got.append(&mut m.log);
                got.push(defender_line(&d.st, &m, &format!("{k:04}")));
            }
            "dump" => {
                for s in &d.st.shots {
                    got.push(format!("  shots {}", hex(&shot_words(s))));
                }
                for s in &d.st.deviants {
                    got.push(format!("  devs {}", hex(&dev_words(s))));
                }
                for s in &d.st.energies {
                    got.push(format!("  energies {}", hex(&energy_words(s))));
                }
                for s in &d.st.explosions {
                    got.push(format!("  exps {}", hex(&exp_words(s))));
                }
            }
            _ => panic!("{line}"),
        }
    }
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g, w, "line {i}");
    }
    assert_eq!(got.len(), want.len());
}
