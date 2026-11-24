//! *Atlantis V* (2006), the engine's third program: a view drawn as the
//! program draws it. Addresses are atlantis5-dvd-int-3891200's (the
//! international DVD's program; the French DVD's is encrypted and not read).
//!
//! This is the first milestone's piece: a view (`Atl_Address_Go(scene,
//! view)`), its panorama, the sprites a view entry places, the camera of
//! `Atl_Camera_LookAt` and `Atl_Camera_SetFOV`, drawn by the cube renderer
//! of *Atlantis III* and the 2004 program. The scripts do not run yet: a
//! caller gives the calls ([`FIRST_VIEW`] holds the new game's, found by
//! running the game's own scripts under Lua 5.0.3 with the C functions
//! recorded).
//!
//! What comes from where:
//!
//! - the view's panorama: the scene's WAM (`datas/scenes/<scene>.wam`, the
//!   path table's `scenes/`) names each view's file (`ze0003.jp6` for
//!   view 3 of `zeppelin`), read from `datas/warps/` (the table's
//!   `warps/`, string 0x0066cec4); the JP6 reader is the 2004 program's
//!   (@ 0x00478100, 0x00478250, 0x00478930 = atlantis4-cd-en-3022848
//!   @ 0x006644b0, 0x00664600, 0x00664ce0), so the faces come in its
//!   order ([`atlantis3_data::evolution2004_jp6`]), and the warp loader
//!   (@ 0x00448eb0 → `AtlLoadWarp_LoadJpeg6Panorama` @ 0x004491b0) puts
//!   face i into cube slot i as the 2004 program's FUN_0042d8b0 does;
//! - a sprite: `(<scene>)_(<view %.3d>)_(<object>)_(<animation>).spw` in
//!   `datas/sprites3d/` (@ 0x00449a80: `(`, `)_(`, `%.3d`, `).spw`,
//!   0x0066e788..0x0066e798), an `SPWCRYO` file of the shared sprite DLL
//!   (`spr_p5.dll`, the 2004 program's file), each face drawn into its cube
//!   face;
//! - the camera: `Atl_Camera_LookAt(pitch, yaw)` with two arguments
//!   (@ 0x00406f00) sets the pitch (@ 0x004447a0) and the yaw
//!   (@ 0x004448c0) of the warp camera, each without limits kept as
//!   `fmod(angle, 360)` (0x006557d8); the camera's yaw node is turned by
//!   −yaw · π · (1/180) about (0, 1, 0), its child pitch node by pitch · π
//!   · (1/180) about (1, 0, 0) (π 0x00656248, 1/180 0x00655410);
//!   `Atl_Camera_SetFOV(deg)` (@ 0x00407340) gives π · deg · (1/180) to
//!   @ 0x0048c490, which is the 2004 program's FUN_004c60b0 and
//!   *Atlantis III*'s @ 0x004fa0b0 ([`Camera::set_fov`]).
//! - the screen: the 3D view's area is 1024 × 576 (the rectangle
//!   {0, 576, 1024, 0} stored at 0x006b0b28 @ 0x0044d7ec; the back buffer's
//!   height is its width × 0.5625, 0x00655834).

use atlantis2_data::wam::Wam;
use atlantis2_engine::camera::Camera;
use atlantis2_engine::fpu::Fpu;
use atlantis2_engine::warp::{A3Raster, Renderer, Variant};
use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis_data::FileSource;

use crate::px::Faces;

/// The 3D view's width and height (the rectangle at 0x006b0b28).
pub const VIEW_WIDTH: usize = 1024;
pub const VIEW_HEIGHT: usize = 576;

/// The faces of the panoramas (every `warps/*.jp6` of the disc).
pub const FACE: usize = 768;

/// A sprite's file (@ 0x00449a80).
pub fn sprite_path(scene: &str, view: u32, object: &str, animation: &str) -> String {
    format!("datas/sprites3d/({scene})_({view:03})_({object})_({animation}).spw")
}

/// What a view entry asks the engine to draw, in the scripts' words.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewCalls {
    /// `Atl_Address_Go(scene, view, ...)`.
    pub scene: &'static str,
    pub view: u32,
    /// `Atl_Camera_LookAt(pitch, yaw)` (degrees).
    pub look_at: (f32, f32),
    /// `Atl_Camera_SetFOV(deg)`.
    pub fov: f32,
    /// `Atl_Sprite_Play(object, animation, playtype[, priority])`.
    pub sprites: &'static [(&'static str, &'static str, &'static str, Option<i32>)],
}

/// The new game's first view: `on_new_game.lua` → `Atl_InitChapter(1)` →
/// `chapter_01.lua` (`Atl_Camera_LookAt(0, 270)`, `Atl_Address_Go("zeppelin",
/// 3, "no_fx")`) → `on_enter_view_zeppelin.lua` for view 3 in chapter 1:
/// five sprites, then `Atl_Camera_SetFOV(76)`. Recorded by running the
/// scripts under Lua 5.0.3 with every C function recorded (the C side's
/// part of it: `Atl_Address_Go` runs the view-entry script inside the call,
/// the step @ 0x0042d900 building `on_enter_view_` + scene).
pub const FIRST_VIEW: ViewCalls = ViewCalls {
    scene: "zeppelin",
    view: 3,
    look_at: (0.0, 270.0),
    fov: 76.0,
    sprites: &[
        ("crab_breaker", "z1", "stay", None),
        ("cendar", "z1", "stay", None),
        ("glass_breaker", "z1", "stay", Some(1)),
        ("glass_breaker_door", "z1", "stay", None),
        ("coin", "z1", "loop", None),
    ],
};

/// The sprites in the order of the program's list (@ 0x00441830: a new
/// sprite goes before the first one whose priority, +0x20 of the sprite,
/// is lower or equal, so the list runs from the highest priority down and,
/// among equals, from the last played), with their priority
/// (`Atl_Sprite_Play`'s fourth argument, 0 without one, @ 0x0040a0d0).
/// The sprites are drawn from the list's head [UNKNOWN: the drawing loop
/// is not read yet, so whether the head is drawn first or last is not
/// known; in the first view only the glass breaker and its door overlap].
pub fn sprite_list<'a>(
    calls: &'a [(&'a str, &'a str, &'a str, Option<i32>)],
) -> Vec<(&'a str, &'a str, &'a str, i32)> {
    let mut list: Vec<(&str, &str, &str, i32)> = Vec::new();
    for &(object, anim, play, priority) in calls {
        let p = priority.unwrap_or(0);
        let at = list.iter().position(|s| s.3 <= p).unwrap_or(list.len());
        list.insert(at, (object, anim, play, p));
    }
    list
}

/// Why a view cannot be drawn.
#[derive(Debug)]
pub enum ViewError {
    Missing(String),
    Bad(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::Missing(p) => write!(f, "{p}: not found"),
            ViewError::Bad(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ViewError {}

/// The panorama file of `view` in `scene` (the WAM's view name).
/// The renderer draws 768² faces only (`A3Raster { face: FACE }`): a
/// panorama with another size is refused at its load, as a bad file.
pub(crate) fn check_faces(path: &str, faces: &[Vec<u32>]) -> Result<(), ViewError> {
    if faces.iter().any(|f| f.len() != FACE * FACE) {
        return Err(ViewError::Bad(format!("{path}: faces not {FACE}²")));
    }
    Ok(())
}

pub fn panorama_name(src: &dyn FileSource, scene: &str, view: u32) -> Result<String, ViewError> {
    let path = format!("datas/scenes/{scene}.wam");
    let data = src.read(&path).ok_or(ViewError::Missing(path.clone()))?;
    let wam = Wam::parse(&data).map_err(|e| ViewError::Bad(format!("{path}: {e:?}")))?;
    wam.view(view)
        .map(|v| v.name.clone())
        .ok_or_else(|| ViewError::Bad(format!("{path}: no view {view}")))
}

/// The camera after `Atl_Camera_LookAt(pitch, yaw)` and
/// `Atl_Camera_SetFOV(fov)` (degrees), for the 3D view's area.
pub fn camera(fpu: Fpu, pitch: f32, yaw: f32, fov: f32) -> Camera {
    let (w, h) = (VIEW_WIDTH as u32, VIEW_HEIGHT as u32);
    let mut c = Camera::new(fpu, w, h);
    let pi = f64::from(f32::from_bits(0x4049_0fdb));
    let inv180 = f64::from(f32::from_bits(0x3bb6_0b61));
    // @ 0x004448c0 / 0x004447a0: fmod by 360 without limits.
    let yaw = fpu.f32(libm::fmod(f64::from(yaw), 360.0));
    let pitch = fpu.f32(libm::fmod(f64::from(pitch), 360.0));
    // The yaw node about the world's (0, 1, 0), then the pitch node, its
    // child, about its own (1, 0, 0): `Camera::yaw` turns about the world's
    // axis, `Camera::pitch` about the camera's.
    c.q = [0.0, 0.0, 0.0, 1.0];
    c.yaw(fpu.f32(fpu.mul(fpu.mul(-f64::from(yaw), pi), inv180)));
    c.pitch(fpu.f32(fpu.mul(fpu.mul(f64::from(pitch), pi), inv180)));
    let fov = fpu.f32(fpu.mul(fpu.mul(pi, f64::from(fov)), inv180));
    c.set_fov(fov, w, h);
    c
}

/// A view ready to draw: the panorama's faces with the view's sprites
/// drawn into them (32-bit x8r8g8b8), and its camera.
pub struct View {
    pub scene: String,
    pub view: u32,
    pub panorama: String,
    /// The faces in cube order, sprites drawn.
    pub faces: [Vec<u32>; 6],
    /// The sprite files drawn, in drawing order.
    pub sprites: Vec<String>,
    pub camera: Camera,
}

impl View {
    /// Loads `calls`' view from `src` (an
    /// [`atlantis3_data::atlantis5::Atlantis5Source`]). Each sprite's first
    /// frame is drawn [the play types' timing is not ported yet], in the
    /// order of [`sprite_list`].
    pub fn load(src: &dyn FileSource, calls: &ViewCalls) -> Result<View, ViewError> {
        let panorama = panorama_name(src, calls.scene, calls.view)?;
        let path = format!("datas/warps/{panorama}");
        let data = src.read(&path).ok_or(ViewError::Missing(path.clone()))?;
        let Faces::Xrgb8888(mut faces) =
            crate::game::faces_of_jp6(&data, Depth::Xrgb8888).map_err(ViewError::Bad)?
        else {
            return Err(ViewError::Bad("faces in another format".into()));
        };
        check_faces(&path, &faces)?;
        let mut sprites = Vec::new();
        for (object, anim, _, priority) in sprite_list(calls.sprites) {
            let p = sprite_path(calls.scene, calls.view, object, anim);
            let Some(d) = src.read(&p) else {
                // `LUA_3DSprite_Play -> Le fichier %s n'existe pas`
                // (0x0066a590): logged, nothing drawn.
                log::warn!("{p}: not found");
                continue;
            };
            let bank = Bank::parse(&d, Depth::Xrgb8888)
                .map_err(|e| ViewError::Bad(format!("{p}: {e}")))?;
            for (face, px) in faces.iter_mut().enumerate() {
                let mut t = Target {
                    pixels: px,
                    pitch: FACE,
                    ymin: 0,
                    ymax: FACE as i32,
                    xmin: 0,
                    xmax: FACE as i32,
                };
                bank.draw_face(&mut t, 0, face);
            }
            log::debug!("{p}: priority {priority}");
            sprites.push(p);
        }
        let (pitch, yaw) = calls.look_at;
        Ok(View {
            scene: calls.scene.to_string(),
            view: calls.view,
            panorama,
            faces,
            sprites,
            camera: camera(Fpu::Double, pitch, yaw, calls.fov),
        })
    }

    /// The view drawn into a 1024 × 576 x8r8g8b8 picture by the cube
    /// renderer (`warpPII`, bilinear, 768-texel faces).
    pub fn render(&self, smooth: bool) -> Vec<u32> {
        let mut r = Renderer::new(Variant::PII, self.camera.fpu, VIEW_WIDTH, VIEW_HEIGHT);
        r.a3 = Some(A3Raster { smooth, face: FACE });
        let mut out = vec![0u32; VIEW_WIDTH * VIEW_HEIGHT];
        r.render(
            &self.faces,
            &self.camera.matrix(),
            self.camera.projection(),
            &mut out,
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            sprite_path("zeppelin", 3, "cendar", "z1"),
            "datas/sprites3d/(zeppelin)_(003)_(cendar)_(z1).spw"
        );
    }

    #[test]
    fn the_sprite_list() {
        let l = sprite_list(FIRST_VIEW.sprites);
        let names: Vec<&str> = l.iter().map(|s| s.0).collect();
        assert_eq!(
            names,
            [
                "glass_breaker",
                "coin",
                "glass_breaker_door",
                "cendar",
                "crab_breaker"
            ]
        );
    }

    #[test]
    fn the_look_at_camera() {
        // Yaw 0, pitch 0: the identity (the cube's `ft` face ahead).
        let c = camera(Fpu::Double, 0.0, 0.0, 76.0);
        let m = c.matrix();
        assert!(
            (m[0] - 1.0).abs() < 1e-6 && (m[4] - 1.0).abs() < 1e-6 && (m[8] - 1.0).abs() < 1e-6
        );
        // 76° over 1024 pixels: fx = 512 / tan(38°).
        assert!((c.fx - 512.0 / 38f32.to_radians().tan()).abs() < 0.01);
        // Yaw 270 and −90 are the same turn.
        let a = camera(Fpu::Double, 0.0, 270.0, 76.0).matrix();
        let b = camera(Fpu::Double, 0.0, -90.0, 76.0).matrix();
        for k in 0..9 {
            assert!((a[k] - b[k]).abs() < 1e-5, "{a:?} {b:?}");
        }
    }
}
