//! A node ("warp") as `ChangeWarp` (atlantis4-gog-en-2363910
//! @ 0x00526c70) builds it for the view: its `background` package, its
//! description (exits, markers), and the view's camera.
//!
//! Not done yet (later steps of M1 and M2): the node's Lua (`OnWarpEnter`,
//! `PutObject`, `StartAnimation`, `SetMarker`, `LookAt` …), the objects and
//! Theora animations drawn over the background, sounds, the cursor.

use atlantis4_data::anim::Anim;
use atlantis4_data::image::decode_jpeg;
use atlantis4_data::scene::Scene;
use atlantis4_data::wpa::{Package, TileSource};
use atlantis4_data::{A4Files, WarpPath};

use crate::cube::{face_pictures, Atlas, Background};
use crate::markers::Marker;
use crate::pick::ExitMesh;

/// The first node of a new game (`Texts/Full/FirstWarpPath.lua`'s
/// `firstWarpPath`).
pub const FIRST_WARP_SCRIPT: &str = "Texts/Full/FirstWarpPath.lua";

/// The value of `firstWarpPath = "…"` in that script (its first line not
/// commented out). The script is Lua; until the interpreter runs it, its
/// one assignment is read here.
pub fn first_warp_path(script: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(script);
    text.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("firstWarpPath")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim();
        let q = rest.chars().next().filter(|&c| c == '"' || c == '\'')?;
        let body = &rest[1..];
        Some(body[..body.find(q)?].to_string())
    })
}

/// Why a node cannot be loaded.
#[derive(Debug)]
pub enum NodeError {
    Missing(String),
    Data(String),
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeError::Missing(s) => write!(f, "missing: {s}"),
            NodeError::Data(s) => write!(f, "bad data: {s}"),
        }
    }
}

impl std::error::Error for NodeError {}

/// A node ready to draw.
pub struct Node {
    pub path: WarpPath,
    pub scene: Scene,
    pub background: Background,
    /// The six face pictures (see [`crate::cube::face_pictures`]).
    pub faces: [Vec<u32>; 6],
    /// The exits' click meshes, in XML order.
    pub exits: Vec<ExitMesh>,
    /// The markers: the objects', the animations', then the exits' (the
    /// original's array follows the XML's element order).
    pub markers: Vec<Marker>,
}

fn data<E: std::fmt::Display>(what: &str) -> impl FnOnce(E) -> NodeError + '_ {
    move |e| NodeError::Data(format!("{what}: {e}"))
}

impl Node {
    /// The node `path` of the install.
    pub fn load(files: &A4Files, path: &WarpPath) -> Result<Node, NodeError> {
        let xml_name = path.xml();
        let xml = files
            .read(&xml_name)
            .ok_or_else(|| NodeError::Missing(xml_name.clone()))?;
        let scene = Scene::parse(&xml).map_err(data(&xml_name))?;
        let pkg_name = path.package("background");
        let src = files.source();
        let pkg = Package::open(src, &pkg_name)
            .ok_or_else(|| NodeError::Missing(pkg_name.clone()))?
            .map_err(data(&pkg_name))?;
        let read = |n: &str| -> Result<Vec<u8>, NodeError> {
            let e = pkg
                .entry(n)
                .ok_or_else(|| NodeError::Missing(format!("{pkg_name}:{n}")))?;
            pkg.read(src, &pkg_name, e)
                .ok_or_else(|| NodeError::Missing(format!("{pkg_name}:{n}")))
        };
        let anim = Anim::parse(&read("anim.anim")?).map_err(data(&pkg_name))?;
        let mut pictures = Vec::with_capacity(anim.tiles.len());
        for t in &anim.tiles {
            let Some(TileSource::Jpeg(name)) = TileSource::resolve(&pkg, &t.name) else {
                return Err(NodeError::Data(format!(
                    "{pkg_name}: tile {} not a JPEG",
                    t.name
                )));
            };
            let img = decode_jpeg(&read(&name)?).map_err(data(&name))?;
            if img.channels != 3 {
                return Err(NodeError::Data(format!("{name}: not RGB")));
            }
            pictures.push(img);
        }
        let sizes: Vec<(u32, u32)> = pictures.iter().map(|p| (p.width, p.height)).collect();
        let background = Background::from_anim(&anim, &sizes).map_err(data(&pkg_name))?;
        let atlases: Vec<Atlas<'_>> = pictures
            .iter()
            .map(|p| Atlas {
                width: p.width as usize,
                height: p.height as usize,
                rgb: &p.pixels,
            })
            .collect();
        let faces = face_pictures(&background, &atlases);
        let exits = scene.exits.iter().map(ExitMesh::new).collect();
        let markers = scene
            .objects
            .iter()
            .flat_map(|o| &o.markers)
            .chain(scene.animations.iter().flat_map(|a| &a.markers))
            .chain(scene.exits.iter().flat_map(|e| &e.markers))
            .map(|&p| Marker::new(p))
            .collect();
        Ok(Node {
            path: path.clone(),
            scene,
            background,
            faces,
            exits,
            markers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_warp_is_the_line_not_commented_out() {
        let s = b"firstWarpPath = \"3D/A/0001.wp\"\n--firstWarpPath = \"3D/B/0001.wp\"\n";
        assert_eq!(first_warp_path(s).as_deref(), Some("3D/A/0001.wp"));
        assert_eq!(first_warp_path(b"--firstWarpPath = \"x\""), None);
    }
}
