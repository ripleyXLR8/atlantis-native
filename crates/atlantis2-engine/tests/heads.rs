//! The dialog heads and the lip-sync against the original's own results
//! (`tests/golden/heads.txt`, `tests/golden/heads_pii.txt`,
//! `tests/golden/lipsync_tibet.txt`): every golden frame of three heads is
//! drawn again with each variant (the Pentium II's under 0x0e7f, the C one
//! under 0x027f) and compared by MD5 (frame and Z buffer), the C pipeline's
//! intermediate buffers of three frames likewise, and the lip-sync weights
//! and mouth meshes of every Tibet line under three x87 control words.
//!
//! They need the game: `ATLANTIS2_DATA` = the installed folder,
//! `ATLANTIS2_DISCS` = a folder holding the four CD images (or copies of
//! the CDs); without them the tests are skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atlantis2_data::db::Scene;
use atlantis2_data::dialog::{parse_com, parse_info, parse_info_rounding};
use atlantis2_data::tga::Tga;
use atlantis2_data::{A2Files, Folder};
use atlantis2_engine::crt::Rand;
use atlantis2_engine::head::Head;
use atlantis2_engine::lipsync::LipSync;
use atlantis2_engine::raster3d::{HEIGHT, WIDTH};
use atlantis2_engine::{Fpu, Variant};
use md5::{Digest, Md5};

const HEADS: &str = include_str!("golden/heads.txt");
const HEADS_PII: &str = include_str!("golden/heads_pii.txt");
const HEADS_PII_MORE: &str = include_str!("golden/heads_pii_more.txt");
const LIPSYNC: &str = include_str!("golden/lipsync_tibet.txt");

fn hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn game() -> Option<A2Files> {
    let (Some(dir), Some(discs)) = (
        std::env::var_os("ATLANTIS2_DATA"),
        std::env::var_os("ATLANTIS2_DISCS"),
    ) else {
        eprintln!("ATLANTIS2_DATA or ATLANTIS2_DISCS not set: test skipped");
        return None;
    };
    let (files, _) =
        A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)]).expect("game folder");
    Some(files)
}

fn load_head(files: &A2Files, db: &str, variant: Variant, fpu: Fpu) -> (Head, Vec<u8>) {
    let data = files.read(Folder::ThreeDDial, db).expect(db);
    let scene = Scene::parse(&data).expect(db);
    let map = Head::texture_name(&scene).expect("texture").to_string();
    let tga = |name: &str| {
        files
            .read(Folder::ThreeDDial, name)
            .map(|d| Tga::parse(&d).unwrap())
    };
    let (yf, ymf) = Head::eye_map_names(&map);
    let head = Head::new(
        fpu,
        variant,
        &scene,
        tga(&map).expect("map"),
        tga(&yf),
        tga(&ymf),
    )
    .unwrap();
    (head, data)
}

struct Frame {
    md5: String,
    zmd5: String,
    drawn: usize,
    bbox: [i32; 4],
}

fn render(head: &mut Head, cam: usize, rand: &mut Rand) -> (Frame, atlantis2_engine::head::Stages) {
    let mut px = vec![0xf81fu16; WIDTH * HEIGHT];
    let mut z = vec![0u32; WIDTH * HEIGHT];
    let st = head.display(cam, rand, &mut px, &mut z);
    let mut bbox = [i32::MAX, i32::MAX, -1, -1];
    let mut drawn = 0;
    for (i, &p) in px.iter().enumerate() {
        if p != 0xf81f {
            drawn += 1;
            let (x, y) = ((i % WIDTH) as i32, (i / WIDTH) as i32);
            bbox = [
                bbox[0].min(x),
                bbox[1].min(y),
                bbox[2].max(x),
                bbox[3].max(y),
            ];
        }
    }
    if drawn == 0 {
        bbox = [-1; 4];
    }
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    let zbytes: Vec<u8> = z.iter().flat_map(|p| p.to_le_bytes()).collect();
    (
        Frame {
            md5: hex(&bytes),
            zmd5: hex(&zbytes),
            drawn,
            bbox,
        },
        st,
    )
}

fn verts_md5(head: &Head) -> String {
    let b: Vec<u8> = head
        .vertices()
        .iter()
        .flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes()))
        .collect();
    hex(&b)
}

/// The weights of a golden mouth tag.
fn weights(tag: &str) -> Option<[f32; 7]> {
    let mut w = [0.0f32; 7];
    match tag {
        "a+o" => {
            w[2] = 0.5;
            w[3] = 0.5;
        }
        "d+b+2s" => {
            w[0] = 0.25;
            w[4] = 0.25;
            w[5] = 0.5;
        }
        t if t.starts_with("blink") => w[2] = 1.0,
        t if t.len() == 1 => {
            let i = b"diaobsv".iter().position(|&c| c == t.as_bytes()[0])?;
            w[i] = 1.0;
        }
        _ => return None,
    }
    Some(w)
}

/// Every golden frame of the three heads drawn with the C routines under
/// 0x027f, in the order the original drew them.
#[test]
fn heads_match_the_original() {
    let Some(files) = game() else { return };
    check_frames(&files, HEADS, Variant::C, Fpu::Double);
}

/// The same frames drawn with the Pentium II's routines (the II
/// rasteriser and the x87 assembly transform), every step under 0x0e7f:
/// the reference (`heads_pii.txt`).
#[test]
fn pii_heads_match_the_original() {
    let Some(files) = game() else { return };
    check_frames(&files, HEADS_PII, Variant::PII, Fpu::Chop);
}

/// The 25 other heads of the game (every `.db` of the four discs'
/// `datas_a2\3d\Dial`) against the original's Pentium II path
/// (`heads_pii_more.txt`, a shorter frame sequence): the drawer the three
/// heads above settle is the same for all 28.
#[test]
fn every_other_head_matches_the_original() {
    let Some(files) = game() else { return };
    check_frames(&files, HEADS_PII_MORE, Variant::PII, Fpu::Chop);
}

fn check_frames(files: &A2Files, golden: &str, variant: Variant, fpu: Fpu) {
    let mut by_db: BTreeMap<&str, Vec<Vec<&str>>> = BTreeMap::new();
    let mut order = Vec::new();
    for line in golden.lines().filter(|l| l.starts_with("frame ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if !by_db.contains_key(f[1]) {
            order.push(f[1]);
        }
        by_db.entry(f[1]).or_default().push(f);
    }
    let mut total = 0;
    for db in order {
        let (mut head, data) = load_head(files, db, variant, fpu);
        let info = golden
            .lines()
            .find(|l| l.starts_with(&format!("file {db} ")))
            .unwrap();
        let info: Vec<&str> = info.split_whitespace().collect();
        assert_eq!(hex(&data), info[2], "{db}");
        assert_eq!(head.camera_count().to_string(), info[3], "{db}");
        assert_eq!(verts_md5(&head), info[5], "{db} base mesh");
        let mut rand = Rand::new(0);
        let mut bad = Vec::new();
        for f in &by_db[db] {
            let (cam, mood, tag, frame): (usize, u32, &str, i32) = (
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
                f[4],
                f[5].parse().unwrap(),
            );
            head.pose(frame);
            assert_eq!(head.fpu(), fpu, "{db}: the word the heads leave");
            if tag == "idle" {
                head.idle_mouth(mood, &mut rand);
            } else {
                if tag == "blink0" {
                    head.set_blink_state(3);
                }
                head.mouth(mood, &weights(tag).expect(tag));
            }
            let (got, _) = render(&mut head, cam, &mut rand);
            let bbox: Vec<i32> = f[9..13].iter().map(|v| v.parse().unwrap()).collect();
            let mut ok = got.md5 == f[6]
                && got.zmd5 == f[7]
                && got.drawn.to_string() == f[8]
                && got.bbox[..] == bbox[..];
            let mut i = 13;
            while i + 1 < f.len() {
                match f[i] {
                    "target" => ok &= head.target_name() == f[i + 1],
                    "verts" => ok &= verts_md5(&head) == f[i + 1],
                    _ => {}
                }
                i += 2;
            }
            if !ok {
                bad.push(format!(
                    "{} | got {} {} {} {:?} target {}",
                    f[1..6].join(" "),
                    got.md5,
                    got.zmd5,
                    got.drawn,
                    got.bbox,
                    head.target_name()
                ));
            }
            total += 1;
        }
        assert!(
            bad.is_empty(),
            "{db}: {} frames differ, first: {:#?}",
            bad.len(),
            &bad[..bad.len().min(4)]
        );
    }
    assert_eq!(
        total,
        golden.lines().filter(|l| l.starts_with("frame ")).count()
    );
}

fn floats(v: &[&str]) -> Vec<u32> {
    v.iter()
        .map(|s| (s.parse::<f64>().unwrap() as f32).to_bits())
        .collect()
}

/// One frame of shs1.db (frames 1 and 20) and H5s1.db (frame 1) step by
/// step: the pose, the mouth, the camera, the transform, the culling, the
/// projection and the pixels; the C routines under 0x027f.
#[test]
fn head_stages_match_the_original() {
    let Some(files) = game() else { return };
    check_stages(&files, HEADS, Variant::C, Fpu::Double);
}

/// The same frames step by step with the Pentium II's routines, every
/// step under 0x0e7f.
#[test]
fn pii_head_stages_match_the_original() {
    let Some(files) = game() else { return };
    check_stages(&files, HEADS_PII, Variant::PII, Fpu::Chop);
}

fn check_stages(files: &A2Files, golden: &str, variant: Variant, fpu: Fpu) {
    let mut keys: Vec<(String, i32)> = Vec::new();
    for l in golden.lines().filter(|l| l.starts_with("stage ")) {
        let f: Vec<&str> = l.split_whitespace().collect();
        let k = (f[1].to_string(), f[2].parse().unwrap());
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    assert_eq!(keys.len(), 3);
    for (db, frame) in keys {
        let want: BTreeMap<&str, Vec<&str>> = golden
            .lines()
            .filter(|l| l.starts_with(&format!("stage {db} {frame} ")))
            .map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                (f[3], f[4..].to_vec())
            })
            .collect();
        let (mut head, _) = load_head(files, &db, variant, fpu);
        let mut rand = Rand::new(0);
        head.pose(frame);
        let (q, p, s) = head.head_world();
        let tag = format!("{db} frame {frame}");
        assert_eq!(
            q.map(f32::to_bits).to_vec(),
            floats(&want["head_q"]),
            "{tag} head rotation"
        );
        assert_eq!(
            p.map(f32::to_bits).to_vec(),
            floats(&want["head_p"]),
            "{tag} head position"
        );
        assert_eq!(
            vec![s.to_bits()],
            floats(&want["head_s"]),
            "{tag} head scale"
        );
        head.mouth(1, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(verts_md5(&head), want["verts"][0], "{tag} mouth");
        let (fxy, slopes, ab) = head.camera_values(0);
        let mut cam: Vec<u32> = fxy.map(f32::to_bits).to_vec();
        cam.extend(slopes.map(f32::to_bits));
        cam.extend(ab.map(f32::to_bits));
        assert_eq!(cam, floats(&want["cam"]), "{tag} camera");
        let (got, st) = render(&mut head, 0, &mut rand);
        let cs: Vec<u8> = st
            .camera
            .iter()
            .flat_map(|v| v.iter().chain(&[0.0]).flat_map(|x| x.to_le_bytes()))
            .collect();
        let pr: Vec<u8> = st
            .projected
            .iter()
            .flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes()))
            .collect();
        let md5 = &want["md5"];
        assert_eq!(hex(&cs), md5[0], "{tag} camera-space vertices");
        assert_eq!(hex(&st.codes), md5[2], "{tag} clip codes");
        assert_eq!(hex(&st.faces), md5[3], "{tag} face bytes");
        assert_eq!(
            st.faces.iter().filter(|&&b| b == 0).count().to_string(),
            want["front"][0],
            "{tag} front faces"
        );
        assert_eq!(hex(&pr), md5[1], "{tag} projected vertices");
        let fr = &want["frame"];
        assert_eq!(
            (got.md5.as_str(), got.zmd5.as_str(), got.drawn.to_string()),
            (fr[0], fr[1], fr[2].to_string()),
            "{tag} frame"
        );
    }
}

/// The lip-sync of every Tibet line: the speakers' numbers, the decoded
/// voices, the analysis, the weights of every frame and the mouth meshes,
/// under 0x027f, 0x007f and 0x0e7f.
#[test]
fn tibet_lipsync_matches_the_original() {
    let Some(files) = game() else { return };
    let want: BTreeMap<&str, &str> = LIPSYNC
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .collect();
    let info_txt = files.read(Folder::Scenar, "infoTI.txt").unwrap();
    let info = parse_info(&info_txt);
    // Read under 0x0e7f, the numbers are stored toward zero (the
    // original's atof then `fstp dword`).
    let info_chop = parse_info_rounding(&info_txt, true);
    let com = parse_com(&files.read(Folder::Scenar, "comTI.txt").unwrap());
    assert_eq!(com.len(), 82);
    let perso = |name: &str| info.iter().find(|p| p.name == name).unwrap();
    for name in ["Heros", "Chamane"] {
        let p = perso(name);
        let k = |s: &str| want[format!("perso.{name}.{s}").as_str()];
        assert_eq!(p.mesh, k("mesh"));
        assert_eq!(format!("{:08x}", p.smooth.to_bits()), k("smooth"));
        assert_eq!(format!("{:08x}", p.offset.to_bits()), k("offset"));
        let m: Vec<u8> = p
            .matrix
            .unwrap()
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        assert_eq!(hex(&m), k("matrix_md5"), "{name} matrix");
    }
    let mut heads: BTreeMap<String, Head> = BTreeMap::new();
    let mut checked = 0;
    for fpu in [Fpu::Double, Fpu::Single, Fpu::Chop] {
        let cw = format!("{:04x}", fpu.control_word());
        let mut bad = Vec::new();
        for line in &com {
            let id = line.id.as_str();
            let p = if fpu == Fpu::Chop {
                info_chop.iter().find(|p| p.name == line.speaker).unwrap()
            } else {
                perso(&line.speaker)
            };
            let k = |s: &str| want[format!("line.{id}.{s}").as_str()];
            let apc = files.read(Folder::ApcDial, &format!("{id}.apc")).expect(id);
            assert_eq!(apc.len().to_string(), k("apc_bytes"), "{id}");
            let voice = atlantis_codecs::decode_apc(&apc).unwrap();
            assert_eq!(
                (voice.samples.len() * 2).to_string(),
                k("pcm_bytes"),
                "{id}"
            );
            let pcm: Vec<u8> = voice.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            assert_eq!(hex(&pcm), k("pcm_md5"), "{id} voice");
            let n = voice.samples.len() as u32;
            let mut lip = LipSync::new(fpu, &p.matrix.unwrap(), &voice.samples, n, p.smooth);
            assert_eq!(lip.blocks().to_string(), k("blocks"), "{id}");
            assert_eq!(lip.max_energy().to_string(), k("energy_max"), "{id}");
            let e: Vec<u8> = lip
                .energies()
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect();
            assert_eq!(hex(&e), k("energy_md5"), "{id} energies");
            let h: Vec<u8> = lip.histograms().iter().flatten().copied().collect();
            assert_eq!(hex(&h), k("hist_md5"), "{id} histograms");
            let db = format!("{}s1.db", p.mesh);
            let head = heads
                .entry(format!("{db} {cw}"))
                .or_insert_with(|| load_head(&files, &db, Variant::C, fpu).0);
            let frames = (f64::from(n) / 22050.0 / (1.0 / 15.0)) as usize + 4;
            assert_eq!(
                frames.to_string(),
                want[format!("cw{cw}.{id}.frames").as_str()]
            );
            let (mut wbytes, mut vhex) = (Vec::new(), String::new());
            for i in 0..frames {
                let t = (i as f64 * (1.0 / 15.0) + f64::from(p.offset)) as f32;
                let w = lip.eval(t);
                for x in w {
                    let b = if x.is_nan() { 0xffc0_0000 } else { x.to_bits() };
                    wbytes.extend(b.to_le_bytes());
                }
                head.mouth(line.mood, &w);
                vhex.push_str(&verts_md5(head));
            }
            if hex(&wbytes) != want[format!("cw{cw}.{id}.weights_md5").as_str()] {
                bad.push(format!("{cw} {id} weights"));
            }
            if hex(vhex.as_bytes()) != want[format!("cw{cw}.{id}.vertices_md5").as_str()] {
                bad.push(format!("{cw} {id} vertices"));
            }
            checked += 1;
        }
        assert!(
            bad.is_empty(),
            "{} differ: {:?}",
            bad.len(),
            &bad[..bad.len().min(6)]
        );
    }
    assert_eq!(checked, 3 * 82);
}
