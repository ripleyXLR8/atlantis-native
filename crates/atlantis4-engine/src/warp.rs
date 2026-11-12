//! A node in play (`TeWarp3`, atlantis4-gog-en-2363910): its draw list of
//! animations, its markers and its exits, as the bindings change them.
//!
//! - **Draw list** (`warp+0xb4`): the `background` first (`ChangeWarp`
//!   starts it @ `0x004f4360`), then every object and animation of the XML
//!   in file order, unloaded (`0x004f3aa0`: an unknown name is appended).
//!   `PutObject` (`0x004f4c70`), `StartAnimation` (`0x004f4180`) and
//!   `StartAnimationPart` (`0x004f40f0`) move the animation to the end of
//!   the list, set its clickable flag (`anim+0xa5`), load its package and
//!   restart its frame animation with `first`, `count`, `loops` (`0, 1, −1`;
//!   `0, 2·10⁹, loops`; `first, last − first, loops`, negative frames
//!   counted from the end).
//! - `RemoveObject` (`0x004f0fb0`, `0x004f0a60`): the animation stops and
//!   its tiles and pick meshes are freed (`anim+0xa4 = 0`); it **stays in
//!   the list** and its markers are not touched (`STATIC`, read for this
//!   port).
//! - **Markers** (`warp+0x4d8`): one per XML marker; `SetMarker(name,
//!   image, n)` (`0x004f4fc0`) finds the exit named `3D\` + `name`
//!   (`0x004efc70`), else the `n`-th marker of the animation; `−1` hides
//!   it, else it shows `Marker_<image>.png`, its 8-frame strip restarted
//!   (see [`crate::markers`]).
//! - The finished signal of a frame animation calls Lua
//!   `OnAnimationFinished(name)`: `ChangeWarp` hooks `0x00523d40` on the
//!   warp's signal `+0xc4` (@ `0x00527337`), which calls
//!   `TeLuaScript::Execute("OnAnimationFinished", name)` (@ `0x00523dff`).

use atlantis4_data::scene::Scene;
use atlantis4_data::{A4Files, WarpPath};

use atlantis4_data::video::RgbTable;

use crate::camera::Camera;
use crate::canvas::Canvas;
use crate::frame_anim::FrameAnim;
use crate::markers::{place, MarkerSprite, MARKER_FRAMES};
use crate::node::{Node, NodeError};
use crate::package::{range_index, Package, Picture};
use crate::pick::AnimPick;
use crate::render::range_visible;

/// `StartAnimation`'s frame count (`0x004f4180`): all frames.
pub const ALL_FRAMES: u32 = 2_000_000_000;

/// An entry of the draw list.
pub struct Animation {
    pub name: String,
    /// `anim+0xa5`.
    pub clickable: bool,
    /// The loaded package; `None` before `PutObject` / `StartAnimation`
    /// or after `RemoveObject`.
    pub package: Option<Package>,
    pub anim: FrameAnim,
    /// The animation's markers (indices into [`Warp::markers`]).
    pub markers: Vec<usize>,
}

impl Animation {
    fn new(name: &str) -> Animation {
        Animation {
            name: name.to_string(),
            clickable: false,
            package: None,
            anim: FrameAnim {
                playing: false,
                start_ms: 0.0,
                loops: -1,
                reverse: false,
                n: 0,
                // `TeFrameAnim`'s constructor (`0x0056e9d0`): 25 fps.
                fps: 25.0,
                first: 0,
                count: 0,
                frame: -1,
            },
            markers: Vec::new(),
        }
    }
}

/// A marker of the warp.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpMarker {
    /// The XML point, `y` negated in format-1.0 files.
    pub point: [f32; 3],
    /// The sprite shown, `None` hidden (`marker+8` clear).
    pub sprite: Option<MarkerSprite>,
}

/// An exit: its name as the click and `SetMarker` see it (`3D\` +
/// `linkedWarp`) and its marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarpExit {
    pub name: String,
    pub marker: Option<usize>,
}

/// What a script asked that the warp cannot do, for the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WarpNote {
    /// No package of that name in the node.
    NoPackage(String, String),
    /// `SetMarker` on a name that is neither an exit nor an animation.
    NoMarkerTarget(String),
    /// `configMarker: Unknown marker %d on animation : %s`.
    UnknownMarker(u32, String),
}

/// A node in play.
pub struct Warp {
    pub node: Node,
    /// The draw list (without the background, drawn first by the cube).
    pub anims: Vec<Animation>,
    pub markers: Vec<WarpMarker>,
    pub exits: Vec<WarpExit>,
    /// The global marker flag (`warp+0x424`, from `App+0x86b0`).
    pub markers_shown: bool,
}

/// The exit name `SetMarker` and the click use: `3D\` + `linkedWarp`
/// (`0x004efc70`; live: `OnWarpObjectHit("3D\\Bateau_CA\\0002.wp")`).
pub fn exit_name(linked_warp: &str) -> String {
    format!("3D\\{linked_warp}")
}

fn register(anims: &mut Vec<Animation>, name: &str) -> usize {
    match anims.iter().position(|a| a.name == name) {
        Some(i) => i,
        None => {
            anims.push(Animation::new(name));
            anims.len() - 1
        }
    }
}

impl Warp {
    /// The node `path`: its background, then its XML (objects,
    /// animations, exits and their markers).
    pub fn load(files: &A4Files, path: &WarpPath) -> Result<Warp, NodeError> {
        let node = Node::load(files, path)?;
        Ok(Warp::from_node(node))
    }

    /// A warp around a loaded node.
    pub fn from_node(node: Node) -> Warp {
        let scene: &Scene = &node.scene;
        let mut anims: Vec<Animation> = Vec::new();
        let mut markers = Vec::new();
        // The node's marker order: objects', animations', exits' (as
        // `Node::markers`).
        for (name, points) in scene
            .objects
            .iter()
            .map(|o| (&o.name, &o.markers))
            .chain(scene.animations.iter().map(|a| (&a.name, &a.markers)))
        {
            if name == "background" {
                continue;
            }
            let i = register(&mut anims, name);
            for &p in points {
                anims[i].markers.push(markers.len());
                markers.push(WarpMarker {
                    point: p,
                    sprite: None,
                });
            }
        }
        let mut exits = Vec::new();
        for e in &scene.exits {
            let marker = e.markers.first().map(|&p| {
                markers.push(WarpMarker {
                    point: p,
                    sprite: None,
                });
                markers.len() - 1
            });
            // A second marker of an exit is kept as a marker too.
            for &p in e.markers.iter().skip(1) {
                markers.push(WarpMarker {
                    point: p,
                    sprite: None,
                });
            }
            exits.push(WarpExit {
                name: exit_name(&e.linked_warp),
                marker,
            });
        }
        Warp {
            node,
            anims,
            markers,
            exits,
            markers_shown: true,
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.anims.iter().position(|a| a.name == name)
    }

    /// `0x004f28f0` + move to the end + load: the animation `name`, last
    /// in the draw list, its package loaded if it was not.
    fn bring_last(&mut self, files: &A4Files, name: &str) -> Result<&mut Animation, WarpNote> {
        let i = match self.find(name) {
            Some(i) => i,
            None => register(&mut self.anims, name),
        };
        let a = self.anims.remove(i);
        self.anims.push(a);
        let a = self.anims.last_mut().expect("just pushed");
        if a.package.is_none() {
            match Package::load(files, &self.node.path, name) {
                Ok(p) => a.package = Some(p),
                Err(e) => return Err(WarpNote::NoPackage(name.to_string(), e.to_string())),
            }
        }
        let p = a.package.as_ref().expect("loaded");
        a.anim.n = p.frame_count;
        a.anim.fps = p.fps;
        Ok(a)
    }

    /// The common part of the three workers at `now_ms`: `first` and
    /// `count` as the original stores them.
    #[allow(clippy::too_many_arguments)]
    fn play(
        &mut self,
        files: &A4Files,
        name: &str,
        first: u32,
        count: u32,
        loops: i32,
        clickable: bool,
        now_ms: f64,
    ) -> Result<(), WarpNote> {
        let a = self.bring_last(files, name)?;
        a.clickable = clickable;
        a.anim.first = first;
        a.anim.count = count;
        a.anim.loops = loops;
        // Stop, then play (`+0x30` vfuncs 1 and 0): the clock restarts.
        a.anim.playing = true;
        a.anim.start_ms = now_ms;
        a.anim.frame = -1;
        a.anim.update(now_ms);
        Ok(())
    }

    /// `StartAnimation(name, loops, clickable)` (`0x004f4180`).
    pub fn start_animation(
        &mut self,
        files: &A4Files,
        name: &str,
        loops: i32,
        clickable: bool,
        now_ms: f64,
    ) -> Result<(), WarpNote> {
        self.play(files, name, 0, ALL_FRAMES, loops, clickable, now_ms)
    }

    /// `StartAnimationPart(name, first, last, loops, clickable)`
    /// (`0x004f40f0`): `last` exclusive; when `first < 0` both get the
    /// frame count added (`anim+0x78`).
    #[allow(clippy::too_many_arguments)]
    pub fn start_animation_part(
        &mut self,
        files: &A4Files,
        name: &str,
        first: i32,
        last: i32,
        loops: i32,
        clickable: bool,
        now_ms: f64,
    ) -> Result<(), WarpNote> {
        // The package gives the frame count the negative frames need.
        let n = self.bring_last(files, name)?.anim.n as i32;
        let (f, l) = if first < 0 {
            (first.wrapping_add(n), last.wrapping_add(n))
        } else {
            (first, last)
        };
        self.play(
            files,
            name,
            f as u32,
            l.wrapping_sub(f) as u32,
            loops,
            clickable,
            now_ms,
        )
    }

    /// `PutObject(name, clickable)` (`0x004f4c70`): frame 0 alone, no end.
    pub fn put_object(
        &mut self,
        files: &A4Files,
        name: &str,
        clickable: bool,
        now_ms: f64,
    ) -> Result<(), WarpNote> {
        self.play(files, name, 0, 1, -1, clickable, now_ms)
    }

    /// `RemoveObject(name)`: stopped and unloaded, kept in the list.
    pub fn remove_object(&mut self, name: &str) {
        if let Some(i) = self.find(name) {
            let a = &mut self.anims[i];
            a.anim.playing = false;
            a.package = None;
        }
    }

    /// `SetMarker(name, image, n)` at `now_ms`.
    pub fn set_marker(
        &mut self,
        name: &str,
        image: i32,
        n: u32,
        now_ms: f64,
    ) -> Result<(), WarpNote> {
        let full = format!("3D\\{name}");
        let marker = if let Some(e) = self.exits.iter().find(|e| e.name == full) {
            e.marker
        } else if let Some(i) = self.find(name) {
            match self.anims[i].markers.get(n as usize) {
                Some(&m) => Some(m),
                None => return Err(WarpNote::UnknownMarker(n, name.to_string())),
            }
        } else {
            return Err(WarpNote::NoMarkerTarget(name.to_string()));
        };
        let Some(m) = marker else {
            return Err(WarpNote::UnknownMarker(n, name.to_string()));
        };
        self.markers[m].sprite = (image != -1).then_some(MarkerSprite {
            image: image as u32,
            start_ms: now_ms,
        });
        Ok(())
    }

    /// The frame animations at `now_ms`: the names whose animation
    /// finished on this update (`OnAnimationFinished`), in list order.
    pub fn update(&mut self, now_ms: f64) -> Vec<String> {
        let mut finished = Vec::new();
        for a in &mut self.anims {
            if !a.anim.playing {
                continue;
            }
            let ev = a.anim.update(now_ms);
            if ev.finished {
                finished.push(a.name.clone());
            }
        }
        finished
    }

    /// The draw list's pick meshes (draw order) for [`crate::pick::click`].
    pub fn picks(&self) -> Vec<AnimPick> {
        self.anims
            .iter()
            .filter_map(|a| {
                let p = a.package.as_ref()?.pick(a.anim.frame)?;
                let v = |i: u32| p.vertices.get(i as usize).copied().unwrap_or([0.0; 3]);
                Some(AnimPick {
                    name: a.name.clone(),
                    clickable: a.clickable,
                    triangles: p
                        .indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|t| [v(t[0]), v(t[1]), v(t[2])])
                        .collect(),
                })
            })
            .collect()
    }

    /// Draws the draw list over the background in `out` (`TeWarp3::draw`
    /// @ 0x004f0b50: each animation in list order, each tile in file
    /// order, the range of the animation's frame; a tile none of whose
    /// triangles is on screen is neither decoded nor drawn).
    pub fn draw_layers(&mut self, cam: &Camera, table: Option<&RgbTable>, c: &mut dyn Canvas) {
        for a in &mut self.anims {
            let Some(p) = a.package.as_mut() else {
                continue;
            };
            let frame = a.anim.frame;
            for t in &mut p.tiles {
                let Some(ri) = range_index(t.ranges.iter().map(|r| r.first_frame), frame) else {
                    continue;
                };
                let range = &t.ranges[ri];
                if !range_visible(cam, range) {
                    continue;
                }
                if let Some(pic) = t.codec.picture(&t.name, frame, table) {
                    c.mesh(cam, range, pic);
                }
            }
        }
    }

    /// Draws the shown markers in array order (`0x00569b60` places them;
    /// the 480 × 320 layer after the warp), each the band of its frame
    /// at `now_ms` of `Marker_<image>.png` (`sprites` gives the picture).
    pub fn draw_markers<'p>(
        &self,
        cam: &Camera,
        now_ms: f64,
        c: &mut dyn Canvas,
        mut sprites: impl FnMut(u32) -> Option<&'p Picture>,
    ) {
        if !self.markers_shown {
            return;
        }
        for m in &self.markers {
            let Some(s) = m.sprite else { continue };
            let Some(pic) = sprites(s.image) else {
                continue;
            };
            let (w, h) = (pic.width, pic.height / MARKER_FRAMES);
            let Some((x, y_up)) = place(cam, m.point, w as f32, h as f32) else {
                continue;
            };
            let band = MarkerSprite::band(s.frame(now_ms));
            let top = f64::from(cam.height) - y_up - f64::from(h);
            c.sprite(pic, (0, band * h, w, h), x, top);
        }
    }
}
