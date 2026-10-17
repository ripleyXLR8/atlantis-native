//! The WAM loader, the sprite lists and the click areas against the
//! original atlantis.exe: synthetic WAMs, then every WAM of the game
//! (loader FUN_00421ebc, zone hit-test FUN_00422bb0, list load FUN_00424a20,
//! NPC hit-test FUN_00422d5c, pick-up FUN_004278a0, the active flag of the
//! place sprite table), run under emulation for the golden values.
//!
//! The data tests need the game: set `ATLANTIS_DATA` to the game directory
//! (atlantis.exe and BIGCD1..4.BIG); without it they are skipped. Run them
//! with `--release`.

mod common;
#[path = "data/wam_synthetic.rs"]
mod synthetic;

use std::collections::{BTreeMap, HashMap};

use atlantis_data::sprite_anim::{
    animate_place_sprites, apply_hidden, enter_place, set_perso_active, start_place_sprite_anim,
    stop_place_sprite_anim, SPRITE_ANIM_SLOTS,
};
use atlantis_data::sprite_list::{
    npc_click_rects, npc_hit_test, object_pickup_rows, parse_sprite_list,
};
use atlantis_data::wam::{parse_centre_vue, Place, Wam};
use atlantis_data::{BigArchive, SpritePlaceEntry};
use common::{g9, game, golden, golden_lines, golden_map, md5_hex};

// The canonical text of each section, as the golden generator writes it.

fn canon_places(w: &Wam) -> String {
    w.places
        .iter()
        .map(|p| {
            format!(
                "P {} {} {} {}\n",
                p.id,
                p.cyclo,
                p.transitions.len(),
                p.zones.len()
            )
        })
        .collect()
}

fn canon_transitions(w: &Wam) -> String {
    let mut s = String::new();
    for p in &w.places {
        for t in &p.transitions {
            s += &format!(
                "T {} {} {} {} {:08x} {:08x} {:08x} {:08x}\n",
                p.id,
                t.video,
                t.dst_place,
                t.flags,
                t.src_alpha.to_bits(),
                t.src_beta.to_bits(),
                t.dst_alpha.to_bits(),
                t.dst_beta.to_bits()
            );
        }
    }
    s
}

fn canon_zones(w: &Wam) -> String {
    let mut s = String::new();
    for p in &w.places {
        for z in &p.zones {
            s += &format!("Z {} {} {} {} {} {}\n", p.id, z.x, z.y, z.w, z.h, z.action);
        }
    }
    s
}

fn canon_subwams(w: &Wam) -> String {
    w.sub_wams
        .iter()
        .map(|s| format!("S {} {} {}\n", s.src_place, s.dst_place, s.wam))
        .collect()
}

fn canon_refs(w: &Wam) -> String {
    let mut s = String::new();
    for (k, name) in [(1, &w.spw_list), (2, &w.spf_list), (3, &w.centre_vue)] {
        if !name.is_empty() {
            s += &format!("R {k} {name}\n");
        }
    }
    s
}

fn canon_npcs(w: &Wam) -> String {
    w.npcs
        .iter()
        .map(|n| format!("N {} {}\n", n.perso_id, n.sprite_file))
        .collect()
}

fn canon_all(w: &Wam) -> String {
    canon_places(w)
        + &canon_transitions(w)
        + &canon_zones(w)
        + &canon_subwams(w)
        + &canon_refs(w)
        + &canon_npcs(w)
}

/// The golden generator's hit points: a 16-px grid over the 2048x768
/// panorama, then each zone's edges and the pixels just outside, clipped.
fn append_hits(p: &Place, out: &mut Vec<u8>) {
    let mut pts = Vec::new();
    for y in (0..768).step_by(16) {
        for x in (0..2048).step_by(16) {
            pts.push((x, y));
        }
    }
    for z in &p.zones {
        let (x, y, w, h) = (
            i32::from(z.x),
            i32::from(z.y),
            i32::from(z.w),
            i32::from(z.h),
        );
        let xs = [x - 1, x, x + w, x + w + 1];
        let ys = [y - 1, y, y + h, y + h + 1];
        for &px in &xs {
            for &py in &ys {
                if (0..2048).contains(&px) && (0..768).contains(&py) {
                    pts.push((px, py));
                }
            }
        }
    }
    for (x, y) in pts {
        out.extend_from_slice(&p.hit_test(x as i16, y as i16).to_le_bytes());
    }
}

#[test]
fn synthetic_parse_standard() {
    let w = Wam::parse(synthetic::K_WAM_STANDARD);
    assert_eq!(canon_all(&w), synthetic::K_CANON_STANDARD);
}

// A place whose name slot starts with NUL: the original keeps an empty
// name ("P 0  0 0").
#[test]
fn synthetic_parse_leading_nul_name() {
    let w = Wam::parse(synthetic::K_WAM_LEADING_NUL);
    assert_eq!(canon_all(&w), synthetic::K_CANON_LEADING_NUL);
}

fn read_wam(big: &BigArchive, name: &str) -> Wam {
    let data = big
        .read(&format!("WAM\\{name}"))
        .unwrap_or_else(|| panic!("WAM\\{name} missing"));
    Wam::parse(&data)
}

fn sprite_list(big: &BigArchive, list: &str) -> Vec<SpritePlaceEntry> {
    big.read(&format!("SPRLIST\\{list}"))
        .map(|d| parse_sprite_list(&d))
        .unwrap_or_default()
}

/// Every WAM: each section of the tables and the zone hit-test results.
#[test]
fn all_wams_match_original() {
    let Some(game) = game("WAM test") else { return };
    let golden = golden_map(include_str!("golden/archive_wam.txt"));
    let mut wams = 0;
    let mut bad = Vec::new();
    for key in golden.keys().filter(|k| k.ends_with("|places")) {
        let name = &key[..key.len() - "|places".len()];
        wams += 1;
        let w = read_wam(&game.big, name);
        let sections = [
            ("places", canon_places(&w)),
            ("transitions", canon_transitions(&w)),
            ("zones", canon_zones(&w)),
            ("subwams", canon_subwams(&w)),
            ("refs", canon_refs(&w)),
            ("npcs", canon_npcs(&w)),
        ];
        for (sec, text) in sections {
            if md5_hex(text.as_bytes()) != golden[format!("{name}|{sec}").as_str()] {
                bad.push(format!("{name}: section {sec}"));
            }
        }
        let nonzero_kind = w.places.iter().filter(|p| p.kind != 0).count();
        if nonzero_kind.to_string() != golden[format!("{name}|nonzero_type_places").as_str()] {
            bad.push(format!(
                "{name}: {nonzero_kind} places with a non-zero type"
            ));
        }
        let refs: usize = w.places.iter().map(|p| p.ambient_refs.len()).sum();
        if refs.to_string() != golden[format!("{name}|ambient_records").as_str()] {
            bad.push(format!("{name}: {refs} ambient references"));
        }
        if w.ambient_objects.len().to_string() != golden[format!("{name}|objects3d").as_str()] {
            bad.push(format!(
                "{name}: {} ambient objects",
                w.ambient_objects.len()
            ));
        }
        let mut hits = Vec::new();
        for p in &w.places {
            append_hits(p, &mut hits);
        }
        if (hits.len() / 2).to_string() != golden[format!("{name}|hitcount").as_str()] {
            bad.push(format!("{name}: hit-test point count {}", hits.len() / 2));
        }
        if md5_hex(&hits) != golden[format!("{name}|hit").as_str()] {
            bad.push(format!("{name}: hit-test results"));
        }
    }
    eprintln!("WAM: {wams} files compared, {} differences", bad.len());
    assert_eq!(wams, 38);
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The ambient sounds: objects, per-place references, file reference 2 and
/// the centre .vue tables (FUN_004315b4), as digests.
#[test]
fn ambient_sounds_match_original() {
    let Some(game) = game("ambient sound test") else {
        return;
    };
    let golden = golden_map(include_str!("golden/wam_ambient.txt"));
    let mut checked = 0;
    for key in golden.keys().copied() {
        if let Some(name) = key.strip_suffix("|ambient") {
            let w = read_wam(&game.big, name);
            let path = format!("WAM\\{name}");
            let mut lines = vec![format!(
                "fileref2 {path} {}",
                w.file_refs.get(2).map_or("-", String::as_str)
            )];
            for o in &w.ambient_objects {
                let dash = |s: &str| {
                    if s.is_empty() {
                        "-".to_string()
                    } else {
                        s.to_string()
                    }
                };
                let variants: Vec<String> = o.variants.iter().map(|v| dash(v)).collect();
                lines.push(format!(
                    "obj {path} {} {} {} {} {} {} {} {} {} {} {}",
                    o.id,
                    dash(&o.name),
                    g9(o.pos[0]),
                    g9(o.pos[1]),
                    g9(o.pos[2]),
                    g9(o.loudness),
                    o.weights[0],
                    o.weights[1],
                    o.weights[2],
                    o.weights[3],
                    variants.join(",")
                ));
            }
            for p in &w.places {
                for (k, r) in p.ambient_refs.iter().enumerate() {
                    lines.push(format!(
                        "ref {path} {} {k} {} {} {} {}",
                        p.id,
                        r.object_id,
                        g9(r.t),
                        g9(r.m),
                        g9(r.c)
                    ));
                }
            }
            let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
            assert_eq!(
                lines.len().to_string(),
                golden[format!("{name}|lines").as_str()],
                "{name}"
            );
            assert_eq!(
                md5_hex(text.as_bytes()),
                golden[key],
                "{name}: ambient sounds differ"
            );
            checked += 1;
        } else if let Some(vue) = key.strip_suffix("|centre") {
            let data = game
                .big
                .read(&format!("SPRLIST\\{vue}"))
                .expect("centre .vue missing");
            let mut table = BTreeMap::new();
            parse_centre_vue(&data, &mut table);
            let mut text = String::new();
            for n in golden[format!("{vue}|frames").as_str()].split(',') {
                let c = table[&n.parse::<i32>().unwrap()];
                text += &format!("centre {vue} {n} {} {} {}\n", g9(c.x), g9(c.y), g9(c.z));
            }
            assert_eq!(
                md5_hex(text.as_bytes()),
                golden[key],
                "{vue}: centre table differs"
            );
            checked += 1;
        }
    }
    eprintln!("ambient: {checked} WAM / centre digests compared");
    assert_eq!(checked, 11);
}

/// NPC click areas: the rectangles after the list load and the hit-test at
/// a 48-px grid plus every rectangle edge +-1, in each view with sprites.
#[test]
fn npc_click_areas_match_original() {
    let Some(game) = game("NPC click test") else {
        return;
    };
    let golden = golden_map(include_str!("golden/graphics_npc_click.txt"));
    let mut wams = 0;
    for key in golden.keys().filter(|k| k.ends_with("|rects")) {
        let name = &key[..key.len() - "|rects".len()];
        wams += 1;
        let w = read_wam(&game.big, name);
        let entries = sprite_list(&game.big, &w.spw_list.to_ascii_uppercase());
        let rects = npc_click_rects(&w.npcs, &entries, |f| {
            game.big
                .read(&format!("SPRITE\\CYCLO\\{}", f.to_ascii_uppercase()))
        });
        let rtext: String = w
            .npcs
            .iter()
            .zip(&rects)
            .enumerate()
            .map(|(i, (n, r))| {
                format!(
                    "{i} {} {} {} {} {} {}\n",
                    n.perso_id, n.sprite_file, r.x, r.y, r.w, r.h
                )
            })
            .collect();
        assert_eq!(
            md5_hex(rtext.as_bytes()),
            golden[*key],
            "{name}: NPC rectangles"
        );

        let mut pts = Vec::new();
        for y in (0..768).step_by(48) {
            for x in (0..2048).step_by(48) {
                pts.push((x, y));
            }
        }
        for r in &rects {
            let cxs = [i32::from(r.x), i32::from(r.x) + i32::from(r.w)];
            let cys = [i32::from(r.y), i32::from(r.y) + i32::from(r.h)];
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for cx in cxs {
                        for cy in cys {
                            // Common::Point holds int16.
                            pts.push((i32::from((cx + dx) as i16), i32::from((cy + dy) as i16)));
                        }
                    }
                }
            }
        }
        let mut res = Vec::new();
        let vues = golden[format!("{name}|vues").as_str()];
        for v in vues.split(',').take_while(|v| !v.starts_with('-')) {
            let vue: u16 = v.parse().unwrap();
            for &(x, y) in &pts {
                res.extend_from_slice(
                    &npc_hit_test(&w.npcs, &rects, &entries, vue, x, y).to_le_bytes(),
                );
            }
        }
        assert_eq!(
            md5_hex(&res),
            golden[format!("{name}|hits").as_str()],
            "{name}: hit-test results"
        );
    }
    eprintln!("NPC click: {wams} WAMs compared");
    assert_eq!(wams, 38);
}

/// Object pick-up rows: for each view, the rows FUN_004278a0's loop takes
/// (object id, type != 3) and their icon byte.
#[test]
fn object_pickup_rows_match_original() {
    let Some(game) = game("object pick-up test") else {
        return;
    };
    let golden = golden_map(include_str!("golden/scripts_object_pickup.txt"));
    let mut wams = 0;
    let mut total = 0;
    for key in golden.keys().filter(|k| k.ends_with("|rows")) {
        let name = &key[..key.len() - "|rows".len()];
        wams += 1;
        let w = read_wam(&game.big, name);
        let entries = sprite_list(&game.big, &w.spw_list.to_ascii_uppercase());
        let mut text = String::new();
        let mut n = 0;
        for vue in 0..512u16 {
            for (i, e) in entries.iter().enumerate() {
                if e.place_id != vue || e.perso_id < 0x100 || e.perso_id >= 0x200 {
                    continue;
                }
                if !object_pickup_rows(&entries, vue, e.perso_id).contains(&i) {
                    continue;
                }
                text += &format!("{vue} {} {} {}\n", e.perso_id, e.raw_file, e.icon);
                n += 1;
            }
        }
        total += n;
        assert_eq!(
            n.to_string(),
            golden[format!("{name}|count").as_str()],
            "{name}: row count"
        );
        assert_eq!(
            md5_hex(text.as_bytes()),
            golden[*key],
            "{name}: pick-up rows"
        );
    }
    eprintln!("pick-up: {wams} WAMs, {total} rows compared");
    assert_eq!(wams, 38);
}

/// The leading decimal integer of `s` and the rest.
fn leading_int(s: &str) -> Option<(i32, &str)> {
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
        .map_or(s.len(), |(i, _)| i);
    Some((s[..end].parse().ok()?, &s[end..]))
}

fn scan_row(item: &str) -> Option<(i32, i32, i32)> {
    let (v, rest) = leading_int(item)?;
    let (k, rest) = leading_int(rest.strip_prefix('.')?)?;
    let (a, _) = leading_int(rest.strip_prefix('=')?)?;
    Some((v, k, a))
}

fn view_row(es: &mut [SpritePlaceEntry], v: u16, k: usize) -> Option<&mut SpritePlaceEntry> {
    es.iter_mut().filter(|e| e.place_id == v).nth(k)
}

/// The place-sprite active flag over ShowPerso / HidePerso / NEWWSPRANIM /
/// STOPWSPRANIM / type-2 cycles and place re-entries, in every WAM.
#[test]
fn sprite_active_flag_matches_original() {
    let Some(game) = game("sprite active test") else {
        return;
    };
    let mut parsed: Vec<SpritePlaceEntry> = Vec::new();
    let (mut rows, mut scen) = (0, 0);
    let mut bad = Vec::new();
    let mut lists: HashMap<String, Vec<SpritePlaceEntry>> = HashMap::new();
    for l in golden_lines(include_str!("golden/scripts_sprite_active.txt")) {
        let t: Vec<&str> = l.split_whitespace().collect();
        match t[0] {
            "L" => {
                parsed = lists
                    .entry(t[2].to_string())
                    .or_insert_with(|| sprite_list(&game.big, &t[2].to_ascii_uppercase()))
                    .clone();
            }
            "R" => {
                let n: Vec<i32> = t[2..7].iter().map(|v| v.parse().unwrap()).collect();
                let (v, k, id, act, typ) = (n[0], n[1], n[2], n[3], n[4]);
                rows += 1;
                let ok = view_row(&mut parsed, v as u16, k as usize).is_some_and(|e| {
                    e.active == (act != 0) && e.perso_id == id.max(0) && e.kind == typ
                });
                if !ok {
                    bad.push(format!("{}: list view {v} row {k}", t[1]));
                }
            }
            "S" => {
                scen += 1;
                let (head, tail) = l.split_once(':').unwrap();
                let h: Vec<&str> = head.split_whitespace().collect();
                let kind = h[2];
                let arg1: i32 = h[3].parse().unwrap();
                let arg2: i32 = h.get(4).map_or(0, |v| v.parse().unwrap());
                let mut want = Vec::new();
                // sscanf "%d.%d=%d": "1.0=0(before 1)" is row 1.0, "1)" no row.
                for item in tail.split_whitespace() {
                    if let Some((v, k, a)) = scan_row(item) {
                        want.push((v as u16, k as usize, a));
                    }
                }
                let mut es = parsed.clone();
                if kind == "HLOAD" || kind == "HSHOW" {
                    apply_hidden(&mut es, &[arg1]); // the list load
                }
                for &(v, _, _) in &want {
                    enter_place(&mut es, v);
                }
                let mut current = 0;
                match kind {
                    "SHOW" | "HSHOW" => set_perso_active(&mut es, arg1, true),
                    "HIDE" => set_perso_active(&mut es, arg1, false),
                    "NEW" => {
                        start_place_sprite_anim(&mut es, arg1 as u16, arg2 + 1, &mut current);
                    }
                    "STOP" => {
                        stop_place_sprite_anim(&mut es, arg1 as u16, arg2 + 1);
                    }
                    "T2" => {
                        let inactive =
                            view_row(&mut es, arg1 as u16, arg2 as usize).map(|e| !e.active);
                        if inactive == Some(true) {
                            start_place_sprite_anim(&mut es, arg1 as u16, arg2 + 1, &mut current);
                        }
                        if let Some(e) = view_row(&mut es, arg1 as u16, arg2 as usize) {
                            e.frame_count = 3; // any count: the cycle ends at it
                            let mut last = [0u32; SPRITE_ANIM_SLOTS];
                            let mut wspr = 0;
                            let mut t = 1u32;
                            while t < 200
                                && view_row(&mut es, arg1 as u16, arg2 as usize)
                                    .is_some_and(|e| e.active)
                            {
                                animate_place_sprites(
                                    &mut es,
                                    arg1 as u16,
                                    current,
                                    1000 + 14 * t,
                                    &mut last,
                                    &mut wspr,
                                    false,
                                );
                                t += 1;
                            }
                        }
                    }
                    _ => {} // HLOAD: the list load only
                }
                for &(v, _, _) in &want {
                    enter_place(&mut es, v);
                }
                for &(v, k, a) in &want {
                    let got = view_row(&mut es, v, k).map(|e| i32::from(e.active));
                    if got != Some(a) {
                        bad.push(format!(
                            "{} {kind} {arg1}: row {v}.{k} port {got:?} original {a}",
                            h[1]
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    eprintln!(
        "sprite active: {rows} rows, {scen} scenarios compared, {} differ",
        bad.len()
    );
    assert_eq!(rows, 1393);
    assert_eq!(scen, 3463);
    assert!(bad.is_empty(), "{:#?}", &bad[..bad.len().min(10)]);
}

#[test]
fn golden_files_parse() {
    assert!(golden(include_str!("golden/archive_wam.txt")).len() > 400);
}
