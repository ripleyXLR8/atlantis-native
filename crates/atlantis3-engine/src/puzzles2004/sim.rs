//! A [`Host`] without the engine (tests, and look-ahead in the data
//! tests): fixed frame time, variables in a map, no banks, no files.

use std::collections::HashMap;
use std::sync::Arc;

use atlantis3_data::spr::Bank;

use super::{Host, HEIGHT, WIDTH};
use crate::px::Surface;

/// The host's state, all public.
#[derive(Clone, Debug, Default)]
pub struct SimHost {
    pub now: f64,
    pub dt: f32,
    pub pointer: (i32, i32),
    pub click: bool,
    pub keys: Vec<u8>,
    pub key_code: i32,
    pub vars: HashMap<String, i32>,
    /// The sounds played, in order.
    pub sounds: Vec<String>,
    /// MSVC's `rand` state.
    pub seed: u32,
    /// The pointer's vertical motion (0x006e9fe4).
    pub mouse_dy: f32,
    surface: Option<Surface>,
}

impl SimHost {
    pub fn new(dt: f32) -> SimHost {
        SimHost {
            dt,
            ..SimHost::default()
        }
    }
}

impl Host for SimHost {
    fn clock(&self) -> f64 {
        self.now
    }
    fn frame_time(&self) -> f32 {
        self.dt
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
        self.keys.contains(&vk)
    }
    fn srand_time(&mut self) {}
    fn rand(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.seed >> 16) & 0x7fff) as i32
    }
    fn var(&self, name: &str) -> i32 {
        self.vars.get(name).copied().unwrap_or(0)
    }
    fn set_var(&mut self, name: &str, v: i32) {
        self.vars.insert(name.to_string(), v);
    }
    fn flag(&self, _n: usize) -> i32 {
        0
    }
    fn set_flag(&mut self, _n: usize, _v: i32) {}
    fn bank(&mut self, _file: &str) -> Option<Arc<Bank>> {
        None
    }
    fn read(&mut self, _path: &str) -> Option<Vec<u8>> {
        None
    }
    fn sfx_load(&mut self, _file: &str) {}
    fn sfx_play(&mut self, file: &str) {
        self.sounds.push(file.to_string());
    }
    fn sfx_remove(&mut self, _file: &str) {}
    fn surface(&mut self) -> &mut Surface {
        self.surface
            .get_or_insert_with(|| Surface::Rgb565(vec![0; WIDTH * HEIGHT]))
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
    fn mouse_dy(&self) -> f32 {
        self.mouse_dy
    }
    fn set_mouse_dy(&mut self, dy: f32) {
        self.mouse_dy = dy;
    }
}
