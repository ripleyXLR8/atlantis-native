//! Place files (WAM\\*.WAM): the places of an area, their transitions,
//! clickable zones and ambient sounds, the links to other areas, the sprite
//! lists and the NPCs.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine
//! (engines/cryomni3d/atlantis/wam_parser.cpp, GPLv3, the ScummVM team),
//! which follows atlantis.exe's loader FUN_00421ebc.
//!
//! Every record has fixed-size fields; names sit in NUL-padded 13-byte
//! slots.
//!
//! ```text
//! header        14 bytes, u16 place count
//! place         name[13], u16 id, u8 kind, u8 nTransitions,
//!               nTransitions x 32-byte transition
//!                 (name[13], u16 dst, u8 flags, f32 srcAlpha, srcBeta,
//!                  dstAlpha, dstBeta),
//!               u8 nZones, nZones x (i16 x, y, w, h, action),
//!               u8 nAmbient, nAmbient x 17-byte ambient reference
//! sub-WAM       exactly 8 slots of (u8, u16 src, u16 dst, name[13]);
//!               unused slots are zero (src 0)
//! file refs     u8 n, n x (u8 type, name[13])  (1 SPW list, 2 SPF list,
//!               3 centre .vue)
//! NPCs          u8 n, n x 108-byte block
//! 3D sounds     u8 n, n x 100-byte object
//! ```
//!
//! The place count of the header is authoritative: names whose slot starts
//! with NUL are empty stub places (all with id 0), not end markers.

use std::collections::BTreeMap;

use crate::reader::{atoi, strtod, Reader};

/// A clickable zone of a panorama, in panorama coordinates (x 0..2047,
/// y 0..767). `action` is the place to go to, or a script id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zone {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
    pub action: i16,
}

/// A transition video (UBB) from a place to `dst_place`, with the camera
/// angles when leaving and when arriving.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transition {
    pub video: String,
    pub dst_place: u16,
    /// 0x01: a normal transition.
    pub flags: u8,
    pub src_alpha: f32,
    pub src_beta: f32,
    pub dst_alpha: f32,
    pub dst_beta: f32,
}

/// A link to a place of another area (another WAM).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubWam {
    pub src_place: u16,
    pub dst_place: u16,
    pub wam: String,
}

/// An NPC of the area (108-byte block of the WAM tail).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcEntry {
    /// The character id the CON scripts name (ClicPerso).
    pub perso_id: u32,
    /// The canonical SPW file (the others come from the SPW list).
    pub sprite_file: String,
    /// The click rectangle at block +0x62 (x, y, w, h), as stored;
    /// FUN_00424a20 overwrites it from the sprite list (see
    /// [`crate::sprite_list::npc_click_rects`]).
    pub rect_x: i16,
    pub rect_y: i16,
    pub rect_w: i16,
    pub rect_h: i16,
    /// The views the NPC appears in: block +0x1e, 17 dwords read as a
    /// bitmap (bit b of dword k = view 32 * k + b), in the order
    /// FUN_0045240c enumerates them. View 0 is left out: in the original a
    /// set view-0 bit ends the list at once (its low word 0 reads as the
    /// end) and the enumerator's state then makes the next entry continue
    /// this bitmap; no block of the shipped WAMs sets that bit, so neither
    /// effect is reproduced.
    pub vues: Vec<u16>,
}

/// A place's reference to a 3D ambient sound object (17 bytes after the
/// zones; FUN_0043187c binds it by id, FUN_00450ed8 reads `t` and `m`, `c`
/// goes to the object's +0x26).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AmbientRef {
    /// 0xff: empty.
    pub object_id: u8,
    /// +5: the non-directional part of the gains.
    pub t: f32,
    /// +9: loudness, times the object's.
    pub m: f32,
    /// +0xd.
    pub c: f32,
}

/// A 3D ambient sound object (100 bytes after the NPC blocks). The position
/// is the file's; the original rotates it at load (x' = -y, y' = x,
/// z' = -z).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbientObject {
    pub id: u8,
    pub name: String,
    /// +0x0e.
    pub pos: [f32; 3],
    /// +0x1a.
    pub loudness: f32,
    /// +0x2c: out of 101; above 100, weight - 100 and looped.
    pub weights: [u8; 4],
    /// +0x30: `wav\<name>`; empty = absent.
    pub variants: [String; 4],
}

/// The listener position of a place: a `frame N` / `camera v1 v2 v3` entry
/// of the area's centre .vue file, stored as (-v2, v1, -v3)
/// (FUN_004315b4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CentrePos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A place: a panorama, the transitions leaving it, its zones and its
/// ambient sounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub id: u16,
    /// The panorama (CYCLO\\*.HNM), e.g. `atl16001.hnm`; empty for stubs.
    pub cyclo: String,
    /// The byte after the id: the place type (0 panorama, 1 ubb\\vue,
    /// 2 images). Every place of the shipped WAMs has 0.
    pub kind: u8,
    pub transitions: Vec<Transition>,
    pub zones: Vec<Zone>,
    pub ambient_refs: Vec<AmbientRef>,
}

impl Place {
    /// The transition to `dst_place`, if any (the first one).
    pub fn find_transition(&self, dst_place: u16) -> Option<&Transition> {
        self.transitions.iter().find(|t| t.dst_place == dst_place)
    }

    /// The action of the zones under a panorama point, 0 if none, as
    /// FUN_00422bb0 computes it: inclusive bounds, the zone fields sign
    /// extended and then every compare unsigned (JC/JNC), so a zone with a
    /// negative x or y never matches; the largest action of all the zones
    /// that hold the point.
    pub fn hit_test(&self, x: i16, y: i16) -> u16 {
        let px = i32::from(x) as u32;
        let py = i32::from(y) as u32;
        let mut best = 0u16;
        for z in &self.zones {
            let (zx, zy) = (i32::from(z.x), i32::from(z.y));
            if px >= zx as u32
                && (zx + i32::from(z.w)) as u32 >= px
                && py >= zy as u32
                && (zy + i32::from(z.h)) as u32 >= py
                && z.action as u16 > best
            {
                best = z.action as u16;
            }
        }
        best
    }
}

/// The panorama point the original hit-tests (zones and NPCs) for a
/// panorama position from the mouse mapping: FUN_0045166f returns
/// y = 0x2ff - v.
pub fn hit_test_point(x: i16, y: i16) -> (i16, i16) {
    (x, 767 - y)
}

/// A parsed WAM file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wam {
    pub places: Vec<Place>,
    /// The used sub-WAM slots (source place not 0), in slot order.
    pub sub_wams: Vec<SubWam>,
    pub npcs: Vec<NpcEntry>,
    pub ambient_objects: Vec<AmbientObject>,
    /// The file references by position (the original reads the centre .vue
    /// as entry 2 whatever its type, FUN_004315b4 @ 0x00431601).
    pub file_refs: Vec<String>,
    /// The SPW sprite list (type 1), e.g. `spwatl1.txt`; empty if none.
    pub spw_list: String,
    /// The SPF transition sprite list (type 2), e.g. `spfatl1.txt`.
    pub spf_list: String,
    /// The centre .vue file (type 3), e.g. `atl1cent.vue`.
    pub centre_vue: String,
}

impl Wam {
    /// Parses a WAM file. A truncated file gives the records read so far.
    pub fn parse(data: &[u8]) -> Wam {
        let mut w = Wam::default();
        let mut s = Reader::new(data);
        s.discard(14);
        let n_places = s.u16();

        let mut p = 0;
        while p < n_places && !s.eos() {
            let mut place = Place {
                cyclo: s.name13(),
                id: s.u16(),
                kind: s.u8(),
                ..Place::default()
            };
            let n_transitions = s.u8();
            for _ in 0..n_transitions {
                place.transitions.push(Transition {
                    video: s.name13(),
                    dst_place: s.u16(),
                    flags: s.u8(),
                    src_alpha: s.f32(),
                    src_beta: s.f32(),
                    dst_alpha: s.f32(),
                    dst_beta: s.f32(),
                });
            }
            let n_zones = s.u8();
            for _ in 0..n_zones {
                place.zones.push(Zone {
                    x: s.i16(),
                    y: s.i16(),
                    w: s.i16(),
                    h: s.i16(),
                    action: s.i16(),
                });
            }
            // The ambient sound references (place +0x2b2 count, +0x2b3
            // records).
            let n_ambient = s.u8();
            for _ in 0..n_ambient {
                let object_id = s.u8();
                // The editor's pointer, replaced by FUN_0043187c.
                s.discard(4);
                place.ambient_refs.push(AmbientRef {
                    object_id,
                    t: s.f32(),
                    m: s.f32(),
                    c: s.f32(),
                });
            }
            log::trace!(
                "place {} '{}' trans={} zones={}",
                place.id,
                place.cyclo,
                n_transitions,
                n_zones
            );
            w.places.push(place);
            p += 1;
        }

        // Always 8 sub-WAM slots; unused ones are zero (src 0).
        let mut slot = 0;
        while slot < 8 && !s.eos() {
            s.discard(1);
            let src_place = s.u16();
            let dst_place = s.u16();
            let wam = s.name13();
            if src_place != 0 {
                w.sub_wams.push(SubWam {
                    src_place,
                    dst_place,
                    wam,
                });
            }
            slot += 1;
        }
        if s.eos() {
            return w;
        }

        let n_refs = s.u8();
        for _ in 0..n_refs {
            let kind = s.u8();
            let name = s.name13();
            match kind {
                1 => w.spw_list = name.clone(),
                2 => w.spf_list = name.clone(),
                3 => w.centre_vue = name.clone(),
                _ => {}
            }
            w.file_refs.push(name);
        }
        if s.eos() {
            return w;
        }

        // The NPC blocks; the port reads the id, the sprite, the views and
        // the click rectangle.
        let n_npcs = s.u8();
        for _ in 0..n_npcs {
            s.discard(2); // index
            let perso_id = s.u32();
            let sprite_file = s.name13();
            s.discard(0x1e - (2 + 4 + 13));
            let mut vues = Vec::new();
            for k in 0..17u16 {
                let bits = s.u32();
                let first = if k == 0 { 1 } else { 0 };
                for b in first..32u16 {
                    if bits >> b & 1 != 0 {
                        vues.push(32 * k + b);
                    }
                }
            }
            let npc = NpcEntry {
                perso_id,
                sprite_file,
                rect_x: s.i16(),
                rect_y: s.i16(),
                rect_w: s.i16(),
                rect_h: s.i16(),
                vues,
            };
            s.discard(108 - 0x6a);
            log::trace!("NPC perso={} sprite='{}'", npc.perso_id, npc.sprite_file);
            w.npcs.push(npc);
        }
        if s.eos() {
            return w;
        }

        // The 3D ambient sound objects: count byte + 100-byte records
        // (0x0076f6c6).
        let n_objects = s.u8();
        let mut n = 0;
        while n < n_objects && !s.eos() {
            let id = s.u8();
            let name = s.name13();
            let pos = [s.f32(), s.f32(), s.f32()];
            let loudness = s.f32();
            // The gains (outputs) and two zero bytes.
            s.discard(0x2c - 0x1e);
            let weights = [s.u8(), s.u8(), s.u8(), s.u8()];
            let variants = [s.name13(), s.name13(), s.name13(), s.name13()];
            w.ambient_objects.push(AmbientObject {
                id,
                name,
                pos,
                loudness,
                weights,
                variants,
            });
            n += 1;
        }
        w
    }

    /// The place with this id (the first one).
    pub fn find_place(&self, id: u16) -> Option<&Place> {
        self.places.iter().find(|p| p.id == id)
    }
}

/// FUN_004315b4's walk of a centre .vue file (SPRLIST\\*.VUE) into `table`.
/// The original never clears the table: entries of earlier files stay.
pub fn parse_centre_vue(buf: &[u8], table: &mut BTreeMap<i32, CentrePos>) {
    // @ 0x00431721..0x0043184a: atoi / strtod on a NUL-ended copy.
    let size = buf.len();
    let mut text = Vec::with_capacity(size + 1);
    text.extend_from_slice(buf);
    text.push(0);
    let at = |p: usize, word: &[u8]| p + word.len() <= size && &text[p..p + word.len()] == word;
    let from = |p: usize| &text[p.min(size)..];
    if size == 0 {
        return;
    }
    let mut p = 0usize;
    loop {
        if at(p, b"frame") {
            p += 6;
            let n = atoi(from(p));
            loop {
                if at(p, b"camera") {
                    p += 7;
                    let v1 = strtod(from(p)) as f32;
                    let mut v2 = 0.0f32;
                    loop {
                        if at(p, b" ") {
                            p += 1;
                            v2 = strtod(from(p)) as f32;
                            break;
                        }
                        p += 1;
                        if size <= p {
                            break;
                        }
                    }
                    loop {
                        if at(p, b" ") {
                            p += 1;
                            let v3 = strtod(from(p)) as f32;
                            table.insert(
                                n,
                                CentrePos {
                                    x: -v2,
                                    y: v1,
                                    z: -v3,
                                },
                            );
                            break;
                        }
                        p += 1;
                        if size <= p {
                            break;
                        }
                    }
                    break;
                }
                p += 1;
                if size <= p {
                    break;
                }
            }
        }
        p += 1;
        if size <= p {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FUN_00422bb0 on zones {100,100,50,50,5} {120,120,50,50,3}
    // {300,300,20,20,2} {310,310,20,20,9} {-1,400,50,50,7} {0,1,-1,-1,0}
    // under emulation: inclusive bounds, the largest action of all the
    // zones holding the point, unsigned compares (x = -1 never matches).
    #[test]
    fn hit_test_matches_original() {
        let zones = [
            (100, 100, 50, 50, 5),
            (120, 120, 50, 50, 3),
            (300, 300, 20, 20, 2),
            (310, 310, 20, 20, 9),
            (-1, 400, 50, 50, 7),
            (0, 1, -1, -1, 0),
        ];
        let place = Place {
            zones: zones
                .iter()
                .map(|&(x, y, w, h, action)| Zone { x, y, w, h, action })
                .collect(),
            ..Place::default()
        };
        let expected = [
            (150, 100, 5),
            (149, 149, 5),
            (150, 150, 5),
            (99, 100, 0),
            (100, 151, 0),
            (130, 130, 5),
            (165, 165, 3),
            (170, 170, 3),
            (315, 315, 9),
            (305, 305, 2),
            (325, 325, 9),
            (0, 410, 0),
            (10, 410, 0),
            (48, 449, 0),
            (500, 500, 0),
            (0, 0, 0),
            (2047, 767, 0),
        ];
        for (x, y, want) in expected {
            assert_eq!(place.hit_test(x, y), want, "point ({x},{y})");
        }
    }

    #[test]
    fn hit_test_point_flips_y() {
        assert_eq!(hit_test_point(10, 0), (10, 767));
        assert_eq!(hit_test_point(2047, 767), (2047, 0));
    }

    #[test]
    fn centre_vue_entries() {
        let text = b"frame 3\ncamera 10.5 -20 30\nframe 4\ncamera 1 2 3";
        let mut t = BTreeMap::new();
        parse_centre_vue(text, &mut t);
        assert_eq!(
            t[&3],
            CentrePos {
                x: 20.0,
                y: 10.5,
                z: -30.0
            }
        );
        assert_eq!(
            t[&4],
            CentrePos {
                x: -2.0,
                y: 1.0,
                z: -3.0
            }
        );
    }
}
