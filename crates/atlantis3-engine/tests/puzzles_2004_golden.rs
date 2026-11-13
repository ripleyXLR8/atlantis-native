//! The 2004 mini-games against goldens of the original's frames run under
//! emulation (no game data needed): a mock [`Host`] with no banks (no
//! sprite is ever touched), the clock, the frame times, the pointer and
//! the click of the golden's steps, and MSVC's `rand` with the golden's
//! seed.
//!
//! `cd2004_catapulte.txt`: `a4cd_catapulte.py` (FUN_00484180).

use std::collections::HashMap;
use std::sync::Arc;

use atlantis3_data::spr::Bank;
use atlantis3_engine::puzzles2004::catapulte::{Catapulte, State};
use atlantis3_engine::puzzles2004::{Host, Puzzle};
use atlantis3_engine::px::Surface;

/// The emulation harness's world.
pub struct Mock {
    pub clock: f64,
    pub frame_t: f32,
    pub pointer: (i32, i32),
    pub click: bool,
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
    fn key(&self, _vk: u8) -> bool {
        false
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
}

fn golden(name: &str) -> Vec<(String, Vec<String>)> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for l in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some(n) = l.strip_prefix("== ") {
            out.push((n.to_string(), Vec::new()));
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
