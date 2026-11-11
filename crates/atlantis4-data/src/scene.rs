//! A scene's description, `3D/<zone>/<node>.xml`, read by
//! `TeSceneWarp3::Load` (log tag at file +0x1b10d4; the element and
//! attribute names it looks up follow, file +0x1b10ec..+0x1b129c).
//!
//! The file is a flat list of elements (no root):
//!
//! | Element | Attributes | Children | Count in the data |
//! |---|---|---|---|
//! | `FileFormatVersion` | `major`, `minor` | | 398 (1.0 in 283, 1.1 in 115) |
//! | `object` | `name` (a warp package) | `marker` `x y z` | 484 |
//! | `animation` | `name` (a warp package), `fps` (15 in 1,410, 0 in 70) | optional `marker` (74 animations, one with two) | 1,480 |
//! | `exit` | `linkedWarp` (`<zone>\<node>.wp`, or a name), `nbWarpBlock` (always 16) | `camera` `angleX angleY`, `marker`, `block` `face offsetX offsetY` | 843 |
//!
//! The marker pictures the executable names next to each element are
//! `2D/Menus/InGame/Marker_2.png` (object, file +0x1b1158), `Marker_4.png`
//! (animation, +0x1b11c8) and `Marker_0.png` (exit, +0x1b124c), each with an
//! `_over` twin; exits are named `Exit_%02d` (+0x1b11f0). Numbers are read
//! here as `f64` then narrowed to `f32` (as C's `atof` into a `float`
//! field would); how the original converts them is not read yet.

use crate::xml::{self, Element, XmlError};

/// A point of a marker, on or near the ±500 cube.
pub type Point = [f32; 3];

/// An object of the scene: a warp package and its marker.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneObject {
    pub name: String,
    /// Every `marker` child, in file order (one in the data, except one
    /// animation with two, `Bateau_CA/0002`'s `mla2`; which one the
    /// original shows is not read yet).
    pub markers: Vec<Point>,
}

/// An animation of the scene: a warp package played at `fps`.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneAnimation {
    pub name: String,
    pub fps: f32,
    /// Every `marker` child, in file order (one in the data, except one
    /// animation with two, `Bateau_CA/0002`'s `mla2`; which one the
    /// original shows is not read yet).
    pub markers: Vec<Point>,
}

/// A cell of a cube face's 16 × 16 click grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub face: u32,
    pub x: u32,
    pub y: u32,
}

/// A way out of the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Exit {
    /// `<zone>\<node>.wp`, or a name that is no scene.
    pub linked_warp: String,
    /// 16 in all 843 exits of the data, whatever their block count: the
    /// side of the click grid (offsets run 0..15) [INFERRED].
    pub nb_warp_block: u32,
    /// `angleX`, `angleY`.
    pub camera: Option<(f32, f32)>,
    /// Every `marker` child, in file order (one in the data, except one
    /// animation with two, `Bateau_CA/0002`'s `mla2`; which one the
    /// original shows is not read yet).
    pub markers: Vec<Point>,
    pub blocks: Vec<Block>,
}

/// A scene description.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// `major`, `minor` of `FileFormatVersion`.
    pub version: Option<(u32, u32)>,
    pub objects: Vec<SceneObject>,
    pub animations: Vec<SceneAnimation>,
    pub exits: Vec<Exit>,
    /// Top-level elements of other names (none in the data).
    pub unknown: Vec<String>,
}

/// Why a scene cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneError {
    Xml(XmlError),
    /// An attribute missing or not a number: element, attribute.
    Attribute(String, &'static str),
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SceneError::Xml(e) => e.fmt(f),
            SceneError::Attribute(el, a) => write!(f, "{el}: attribute {a} missing or wrong"),
        }
    }
}

impl std::error::Error for SceneError {}

fn number<T: std::str::FromStr>(el: &Element, a: &'static str) -> Result<T, SceneError> {
    el.attr(a)
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(|| SceneError::Attribute(el.name.clone(), a))
}

fn float(el: &Element, a: &'static str) -> Result<f32, SceneError> {
    number::<f64>(el, a).map(|v| v as f32)
}

fn text(el: &Element, a: &'static str) -> Result<String, SceneError> {
    el.attr(a)
        .map(str::to_string)
        .ok_or_else(|| SceneError::Attribute(el.name.clone(), a))
}

fn markers(el: &Element) -> Result<Vec<Point>, SceneError> {
    el.children_named("marker")
        .map(|m| Ok([float(m, "x")?, float(m, "y")?, float(m, "z")?]))
        .collect()
}

impl Scene {
    pub fn parse(data: &[u8]) -> Result<Scene, SceneError> {
        let mut scene = Scene::default();
        for el in xml::parse(data).map_err(SceneError::Xml)? {
            match el.name.as_str() {
                "FileFormatVersion" => {
                    scene.version = Some((number(&el, "major")?, number(&el, "minor")?));
                }
                "object" => scene.objects.push(SceneObject {
                    name: text(&el, "name")?,
                    markers: markers(&el)?,
                }),
                "animation" => scene.animations.push(SceneAnimation {
                    name: text(&el, "name")?,
                    fps: float(&el, "fps")?,
                    markers: markers(&el)?,
                }),
                "exit" => {
                    let camera = el
                        .child("camera")
                        .map(|c| Ok((float(c, "angleX")?, float(c, "angleY")?)))
                        .transpose()?;
                    let blocks = el
                        .children_named("block")
                        .map(|b| {
                            Ok(Block {
                                face: number(b, "face")?,
                                x: number(b, "offsetX")?,
                                y: number(b, "offsetY")?,
                            })
                        })
                        .collect::<Result<_, SceneError>>()?;
                    scene.exits.push(Exit {
                        linked_warp: text(&el, "linkedWarp")?,
                        nb_warp_block: number(&el, "nbWarpBlock")?,
                        camera,
                        markers: markers(&el)?,
                        blocks,
                    });
                }
                _ => scene.unknown.push(el.name),
            }
        }
        Ok(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_reads_its_objects_animations_and_exits() {
        let x = br#"<?xml version="1.0" ?>
<FileFormatVersion major="1" minor="1" />
<object name="o1">
    <marker x="1.5" y="-2" z="500" />
</object>
<animation name="background" fps="15.000000" />
<exit linkedWarp="Zone\0002.wp" nbWarpBlock="2">
    <camera angleX="0.000000" angleY="90.0" />
    <marker x="0" y="0" z="-500" />
    <block face="2" offsetX="4" offsetY="15" />
    <block face="2" offsetX="5" offsetY="15" />
</exit>
"#;
        let s = Scene::parse(x).unwrap();
        assert_eq!(s.version, Some((1, 1)));
        assert_eq!(s.objects[0].markers, vec![[1.5, -2.0, 500.0]]);
        assert_eq!(s.animations[0].fps, 15.0);
        assert!(s.animations[0].markers.is_empty());
        let e = &s.exits[0];
        assert_eq!(e.linked_warp, "Zone\\0002.wp");
        assert_eq!(e.camera, Some((0.0, 90.0)));
        assert_eq!(
            e.blocks[1],
            Block {
                face: 2,
                x: 5,
                y: 15
            }
        );
        assert!(s.unknown.is_empty());
    }
}
