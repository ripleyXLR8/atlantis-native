//! `WAM V2.8` place files (`wam\*.wam`): the views of a place, the moves
//! between them, their 3D click zones and the animated entities.
//!
//! Unlike the first game's fixed records, the file is a stream of chunks
//! written by an editor: a 0x20-byte banner, then chunks `u32 tag
//! 0xDEC0DEnn, u32 payload size, payload`, read by @ 0x00411070 into a
//! linked tree (one handler per tag; unknown tags are skipped by size, tag
//! 0xDEC0DE01 ends the walk). Child records attach to the last record of
//! their parent kind: views to the last place, destinations, zones, sounds
//! and links to the last view (links to the last place), points to the
//! last zone, `id_spf` names to the last destination, entity states to the
//! last entity. Lists keep the file order, which is the zone pick order.

use std::fmt;

/// A WAM file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wam {
    /// `atoi(banner + 5) * 100 + atoi(after the '.')` (@ 0x00412490): 208
    /// for `WAM V2.8`. A newer version is only logged by the original.
    pub version: u32,
    /// The two file prefixes of tag 0x0c (`ti`, `ti`); the game never
    /// reads them.
    pub prefixes: [String; 2],
    pub places: Vec<Place>,
    pub entities: Vec<Entity>,
    pub ambient_objects: Vec<AmbientObject>,
    /// Chunks the reader skipped: an unknown tag, or a known one with a
    /// payload size its handler refuses (logged as `err taille`).
    pub skipped: usize,
}

/// A place (tag 0x02, @ 0x004117e0); every shipped file has one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub name: String,
    /// Place +0x10 (1 in every file; no reader found).
    pub unknown: u32,
    pub views: Vec<View>,
    /// Links to other WAMs (tag 0x07), used by type-2 zones.
    pub links: Vec<Link>,
}

/// A view (tag 0x03, @ 0x00411840): a cube panorama the player stands in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// Its warp file, `datas_a2\warp\<name>`.
    pub name: String,
    pub id: u32,
    /// View +0x14, only in the 32-byte form; +0x18, +0x1c (editor map
    /// positions, it seems: no reader found).
    pub extra: Option<u32>,
    pub map: [u32; 2],
    pub destinations: Vec<Destination>,
    pub zones: Vec<Zone>,
    pub sounds: Vec<Sound>,
}

/// A move from the view to another (tag 0x04, @ 0x00411930).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Destination {
    /// The transition video, `datas_a2\hnm\<video>`.
    pub video: String,
    pub target: u32,
    /// Destination +0x14 (no reader found).
    pub unknown: u32,
    /// +0x18: the angles are taken as given (otherwise 999 means unset).
    pub angles_as_given: u32,
    /// Radians; 999.0 = unset.
    pub start_yaw: f32,
    pub start_pitch: f32,
    pub end_yaw: f32,
    pub end_pitch: f32,
    /// Transition sprites (tag 0x10): CON names of characters or objects.
    pub sprites: Vec<String>,
}

/// A click zone (tag 0x05, @ 0x00411a50) and its corners (tag 0x0d).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zone {
    /// 1 move, 2 other WAM, 3 named zone, 4 / 10 / 50 / 1000 `CLICZONE`;
    /// + 88888 when disabled.
    pub kind: i32,
    /// The target view (types 1, 2) or the `CLICZONE` value. Type 3 zones
    /// get their name's index in `defnames.con` at load (@ 0x00410020).
    pub value: i32,
    /// Only in the 40-byte form.
    pub name: Option<String>,
    /// Directions from the viewer, x right, y down, z forward (`ft`).
    pub points: Vec<[f32; 3]>,
}

/// A positioned sound of a view (tag 0x06, `bruitage`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sound {
    pub ambient_id: u32,
    pub position: [f32; 3],
}

/// A link to another WAM (tag 0x07, `lien`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    /// Not read by the game.
    pub unknown: u32,
    pub from_view: u32,
    pub to_view: u32,
    pub wam: String,
}

/// An ambient sound object (tag 0x0b), kept as read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbientObject {
    pub id: u32,
    pub name: String,
    pub payload: Vec<u8>,
}

/// A character or object drawn as SPW animations (tag 0x0e).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entity {
    pub name: String,
    /// The 2-letter SPW prefix (a u32 in the file).
    pub prefix: String,
    /// Entity +0x14 (overwritten by the `defnames.con` index at load).
    pub index: u32,
    /// The CON name (32 bytes; 16 in the 44-byte form; none in the 28-byte
    /// form).
    pub con_name: String,
    /// 1 character, 2 object.
    pub kind: u32,
    /// Its states (tag 0x0f): 12, 10 or 8 u32 as in the file.
    pub states: Vec<Vec<u32>>,
}

/// One state of an entity (tag 0x0f): where and how it is shown. The
/// shorter forms of the record leave the last fields 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityState {
    /// +0x00: the state letter (`A`), first character of the SPW name.
    pub letter: u32,
    /// +0x04: the animation number.
    pub anim: u32,
    /// +0x08: the view where it is shown (@ 0x00459fe0 loads the states of
    /// the entered view).
    pub view: u32,
    /// +0x0c: the view it was made for (part of the SPW name).
    pub made_for: u32,
    /// +0x10: the weight of the random choice among the slots of the same
    /// entity (@ 0x0047d9a0).
    pub weight: i32,
    /// +0x14: the random pause between plays (@ 0x0047d7f0).
    pub delay: i32,
    /// +0x18: the click kind: 1 object to take, 2 character to talk to, 3
    /// `CLICOBJET`, 4 `CLICPERSO`; 0 not clickable.
    pub click: i32,
    /// +0x1c: the next state, as text (`B2`); 0 = none.
    pub next: u32,
    /// +0x20: the first frame.
    pub first: i32,
    /// +0x24: the frame count + 1 (0 = the file's count); −1 hides the
    /// slot after a play.
    pub end: i32,
    /// +0x28: shown at load.
    pub shown: i32,
    /// +0x2c: 1 = an animated frame, otherwise a list of frames.
    pub single: i32,
}

impl EntityState {
    /// The state from the record's words.
    pub fn from_words(w: &[u32]) -> EntityState {
        let g = |i: usize| w.get(i).copied().unwrap_or(0);
        EntityState {
            letter: g(0),
            anim: g(1),
            view: g(2),
            made_for: g(3),
            weight: g(4) as i32,
            delay: g(5) as i32,
            click: g(6) as i32,
            next: g(7),
            first: g(8) as i32,
            end: g(9) as i32,
            shown: g(10) as i32,
            single: g(11) as i32,
        }
    }
}

impl Entity {
    /// Its states, decoded.
    pub fn state_records(&self) -> impl Iterator<Item = EntityState> + '_ {
        self.states.iter().map(|w| EntityState::from_words(w))
    }

    /// The SPW file of one of its states (@ 0x00412230, format
    /// `%s%s%s%s%s.SPW`: prefix, letter, animation, `%03d` of the view
    /// shown in and of the view made for).
    pub fn spw_name(&self, s: &EntityState) -> String {
        format!(
            "{}{}{}{:03}{:03}.SPW",
            self.prefix,
            char::from(s.letter as u8),
            s.anim,
            s.view,
            s.made_for
        )
    }
}

/// Why a file is not a WAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WamError {
    /// Shorter than its banner.
    Truncated,
    /// No `WAM` banner.
    NotWam,
}

impl fmt::Display for WamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WamError::Truncated => write!(f, "truncated WAM file"),
            WamError::NotWam => write!(f, "not a WAM file"),
        }
    }
}

impl std::error::Error for WamError {}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| char::from(c)).collect()
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// C `atoi`: optional spaces and sign, then digits.
fn atoi(b: &[u8]) -> i64 {
    let mut i = 0;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let neg = b.get(i) == Some(&b'-');
    if matches!(b.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let mut v: i64 = 0;
    while let Some(d) = b.get(i).filter(|c| c.is_ascii_digit()) {
        v = v.wrapping_mul(10).wrapping_add(i64::from(d - b'0'));
        i += 1;
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

const TAG_BASE: u32 = 0xDEC0_DE00;

impl Wam {
    /// Reads a WAM file. Chunks run to the end of the data (or tag
    /// 0xDEC0DE01); a chunk cut short ends the walk.
    pub fn parse(data: &[u8]) -> Result<Wam, WamError> {
        let banner = data.get(..0x20).ok_or(WamError::Truncated)?;
        if !banner[..3].eq_ignore_ascii_case(b"WAM") {
            return Err(WamError::NotWam);
        }
        let dot = banner.iter().position(|&c| c == b'.').unwrap_or(5);
        let version = atoi(&banner[5..])
            .wrapping_mul(100)
            .wrapping_add(atoi(&banner[dot + 1..])) as u32;
        let mut wam = Wam {
            version,
            ..Wam::default()
        };
        let mut pos = 0x20;
        while pos + 8 <= data.len() {
            let tag = u32_at(data, pos);
            let size = u32_at(data, pos + 4) as usize;
            let Some(p) = size
                .checked_add(pos + 8)
                .and_then(|end| data.get(pos + 8..end))
            else {
                log::warn!("WAM chunk {tag:#x} at {pos:#x} cut short");
                break;
            };
            pos += 8 + size; // within data.len(): checked above
            if tag & 0xffff_ff00 != TAG_BASE {
                wam.skipped += 1;
                continue;
            }
            if !wam.chunk(tag & 0xff, p) {
                wam.skipped += 1;
                log::debug!("WAM chunk {tag:#x} of {size} bytes skipped");
            }
            if tag == TAG_BASE | 1 {
                break;
            }
        }
        Ok(wam)
    }

    /// One chunk; false when it is unknown or of a size its handler
    /// refuses (or has no parent record).
    fn chunk(&mut self, tag: u32, p: &[u8]) -> bool {
        let n = p.len();
        match (tag, n) {
            (0x01, _) => true,
            // File prefixes; the 1052-byte form has four paths first.
            (0x0c, 12) | (0x0c, 1052) => {
                let b = &p[n - 12..];
                self.prefixes = [cstr(&b[..6]), cstr(&b[6..])];
                true
            }
            // The editor's folders.
            (0x11..=0x17, 260) => true,
            (0x02, 20) => {
                self.places.push(Place {
                    name: cstr(&p[..16]),
                    unknown: u32_at(p, 16),
                    ..Place::default()
                });
                true
            }
            (0x03, 28) | (0x03, 32) => {
                let Some(place) = self.places.last_mut() else {
                    return false;
                };
                let (extra, map_at) = if n == 32 {
                    (Some(u32_at(p, 20)), 24)
                } else {
                    (None, 20)
                };
                place.views.push(View {
                    name: cstr(&p[..16]),
                    id: u32_at(p, 16),
                    extra,
                    map: [u32_at(p, map_at), u32_at(p, map_at + 4)],
                    ..View::default()
                });
                true
            }
            (0x04, 44) => {
                let Some(view) = self.last_view() else {
                    return false;
                };
                view.destinations.push(Destination {
                    video: cstr(&p[..16]),
                    target: u32_at(p, 16),
                    unknown: u32_at(p, 20),
                    angles_as_given: u32_at(p, 24),
                    start_yaw: f32_at(p, 28),
                    start_pitch: f32_at(p, 32),
                    end_yaw: f32_at(p, 36),
                    end_pitch: f32_at(p, 40),
                    sprites: Vec::new(),
                });
                true
            }
            (0x10, 32) => {
                let Some(d) = self.last_view().and_then(|v| v.destinations.last_mut()) else {
                    return false;
                };
                d.sprites.push(cstr(p));
                true
            }
            (0x05, 8) | (0x05, 40) => {
                let Some(view) = self.last_view() else {
                    return false;
                };
                view.zones.push(Zone {
                    kind: u32_at(p, 0) as i32,
                    value: u32_at(p, 4) as i32,
                    name: (n == 40).then(|| cstr(&p[8..40])),
                    points: Vec::new(),
                });
                true
            }
            (0x0d, 12) => {
                let Some(z) = self.last_view().and_then(|v| v.zones.last_mut()) else {
                    return false;
                };
                z.points.push([f32_at(p, 0), f32_at(p, 4), f32_at(p, 8)]);
                true
            }
            (0x06, 16) => {
                let Some(view) = self.last_view() else {
                    return false;
                };
                view.sounds.push(Sound {
                    ambient_id: u32_at(p, 0),
                    position: [f32_at(p, 4), f32_at(p, 8), f32_at(p, 12)],
                });
                true
            }
            (0x07, 28) => {
                let Some(place) = self.places.last_mut() else {
                    return false;
                };
                place.links.push(Link {
                    unknown: u32_at(p, 0),
                    from_view: u32_at(p, 4),
                    to_view: u32_at(p, 8),
                    wam: cstr(&p[12..28]),
                });
                true
            }
            (0x0b, 132) | (0x0b, 124) => {
                self.ambient_objects.push(AmbientObject {
                    id: u32_at(p, 0),
                    name: cstr(&p[4..20]),
                    payload: p.to_vec(),
                });
                true
            }
            (0x0e, 60) | (0x0e, 44) | (0x0e, 28) => {
                let prefix: String = p[16..20]
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| char::from(c))
                    .collect();
                let (con_name, kind_at) = match n {
                    60 => (cstr(&p[24..56]), 56),
                    44 => (cstr(&p[24..40]), 40),
                    _ => (String::new(), 24),
                };
                self.entities.push(Entity {
                    name: cstr(&p[..16]),
                    prefix,
                    index: u32_at(p, 20),
                    con_name,
                    kind: u32_at(p, kind_at),
                    states: Vec::new(),
                });
                true
            }
            (0x0f, 48) | (0x0f, 40) | (0x0f, 32) => {
                let Some(e) = self.entities.last_mut() else {
                    return false;
                };
                e.states.push(
                    p.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|&c| u32::from_le_bytes(c))
                        .collect(),
                );
                true
            }
            _ => false,
        }
    }

    fn last_view(&mut self) -> Option<&mut View> {
        self.places.last_mut()?.views.last_mut()
    }

    /// The fix-ups of `AtlWam_Load` (@ 0x0040fee0) that need
    /// `defnames.con`: every entity's id becomes the index of its CON name
    /// in the section of its kind (@ 0x0040ffc0: 1 characters, 2 objects,
    /// 3 click zones), every zone of type 3 the index of its name in
    /// `#ZONES_CLIC#` (@ 0x00410020). A name not in the file stops the
    /// original; here it is logged and the file's value kept.
    pub fn apply_defnames(&mut self, names: &crate::defnames::DefNames) {
        use crate::defnames::{OBJETS, PERSONNAGES, ZONES_CLIC};
        for e in &mut self.entities {
            let section = match e.kind {
                1 => PERSONNAGES,
                2 => OBJETS,
                3 => ZONES_CLIC,
                _ => continue,
            };
            match names.id(section, e.con_name.as_bytes()) {
                Ok(i) => e.index = i as u32,
                Err(err) => log::warn!("defnames.con: {err}"),
            }
        }
        for z in self
            .places
            .iter_mut()
            .flat_map(|p| p.views.iter_mut())
            .flat_map(|v| v.zones.iter_mut())
            .filter(|z| z.kind == 3)
        {
            let name = z.name.clone().unwrap_or_default();
            match names.id(ZONES_CLIC, name.as_bytes()) {
                Ok(i) => z.value = i as i32,
                Err(err) => log::warn!("defnames.con: {err}"),
            }
        }
    }

    /// The view with id `id` in the first place that has one.
    pub fn view(&self, id: u32) -> Option<&View> {
        self.places
            .iter()
            .flat_map(|p| p.views.iter())
            .find(|v| v.id == id)
    }
}

impl Zone {
    /// Whether the direction `d` is inside the zone (@ 0x00458210): for
    /// every edge (Pᵢ, Pᵢ₊₁), the last point closing to the first,
    /// d · (Pᵢ × Pᵢ₊₁) >= 0 (a convex polygon seen from the viewer).
    pub fn contains(&self, d: [f64; 3]) -> bool {
        let n = self.points.len();
        (0..n).all(|k| {
            let a = self.points[k].map(f64::from);
            let b = self.points[(k + 1) % n].map(f64::from);
            let c = [
                a[1] * b[2] - b[1] * a[2],
                b[0] * a[2] - a[0] * b[2],
                a[0] * b[1] - b[0] * a[1],
            ];
            d[0] * c[0] + d[1] * c[1] + d[2] * c[2] >= 0.0
        })
    }

    /// The zone's aim (@ 0x00459300): the yaw and pitch (radians, as
    /// @ 0x004581d0 measures them) of the sum of its points, where the view
    /// turns when a move gives no angle.
    pub fn aim(&self) -> (f64, f64) {
        let mut s = [0f64; 3];
        for p in &self.points {
            for i in 0..3 {
                s[i] += f64::from(p[i]);
            }
        }
        (
            s[2].atan2(s[0]),
            s[1].atan2((s[0] * s[0] + s[2] * s[2]).sqrt()),
        )
    }
}

/// The first zone, in file order, holding the direction `d` (@ 0x00458fe0).
/// A disabled zone (type + 88888) is still picked: it hides the zones
/// after it.
pub fn pick(zones: &[Zone], d: [f64; 3]) -> Option<usize> {
    zones.iter().position(|z| z.contains(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut c = (TAG_BASE | u32::from(tag)).to_le_bytes().to_vec();
        c.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        c.extend_from_slice(payload);
        c
    }

    fn name(s: &str, n: usize) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        b.resize(n, 0);
        b
    }

    #[test]
    fn a_small_place() {
        let mut d = name("WAM V2.8  (c)Cryo 1999", 0x20);
        let mut pre = name("ti", 6);
        pre.extend(name("ti", 6));
        d.extend(chunk(0x0c, &pre));
        let mut place = name("", 16);
        place.extend(1u32.to_le_bytes());
        d.extend(chunk(0x02, &place));
        let mut view = name("ti0002.hnm", 16);
        for v in [2u32, 296, 616] {
            view.extend(v.to_le_bytes());
        }
        d.extend(chunk(0x03, &view));
        let mut dest = name("ti002003.HNM", 16);
        for v in [3u32, 1, 1] {
            dest.extend(v.to_le_bytes());
        }
        for f in [4.7124f32, 0.0, 4.7124, 0.0] {
            dest.extend(f.to_le_bytes());
        }
        d.extend(chunk(0x04, &dest));
        let mut zone = 1u32.to_le_bytes().to_vec();
        zone.extend(3i32.to_le_bytes());
        d.extend(chunk(0x05, &zone));
        // A square in front (z = 1), listed so that inside is positive.
        for (x, y) in [(-1f32, -1f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let mut pt = x.to_le_bytes().to_vec();
            pt.extend(y.to_le_bytes());
            pt.extend(1f32.to_le_bytes());
            d.extend(chunk(0x0d, &pt));
        }
        d.extend(chunk(0x99, &[1, 2, 3]));
        let w = Wam::parse(&d).unwrap();
        assert_eq!(w.version, 208);
        assert_eq!(w.prefixes, ["ti".to_string(), "ti".to_string()]);
        assert_eq!(w.skipped, 1);
        let v = w.view(2).unwrap();
        assert_eq!(v.name, "ti0002.hnm");
        assert_eq!((v.extra, v.map), (None, [296, 616]));
        assert_eq!(v.destinations[0].target, 3);
        assert_eq!(v.destinations[0].start_yaw, 4.7124);
        let z = &v.zones[0];
        assert_eq!((z.kind, z.value, z.points.len()), (1, 3, 4));
        // Which way round is inside depends on the winding: the shipped
        // zones are wound so that their inside is positive.
        let inside = z.contains([0.0, 0.0, 1.0]);
        assert_eq!(z.contains([0.0, 0.0, -1.0]), !inside);
        assert!(!z.contains([5.0, 0.0, 1.0]));
        let (yaw, pitch) = z.aim();
        assert!((yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-12 && pitch.abs() < 1e-12);
        assert_eq!(Wam::parse(b"WAM"), Err(WamError::Truncated));
        assert_eq!(Wam::parse(&[0u8; 0x20]), Err(WamError::NotWam));
    }

    #[test]
    fn old_banner_version() {
        let d = name("WAM v2.01 (c)Cryo 1998", 0x20);
        assert_eq!(Wam::parse(&d).unwrap().version, 201);
    }

    #[test]
    fn hostile_banner_and_chunk_sizes() {
        // 27 digits overflow an i64: wrapped, not a panic.
        let d = name("WAM v999999999999999999999999999.9", 0x20);
        let _ = Wam::parse(&d).unwrap().version;
        // A chunk size at the top of the range ends the walk.
        let mut d = name("WAM v2.8", 0x20);
        d.extend_from_slice(&0xDEC0_DE0Eu32.to_le_bytes());
        d.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Wam::parse(&d).is_ok());
    }
}
