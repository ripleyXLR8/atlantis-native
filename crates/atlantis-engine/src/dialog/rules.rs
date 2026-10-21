//! The rules of a dialog line that do not draw: the INITDIAL table, the
//! header flags, the camera, the INITDIAL row and the view angles.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`dialog.cpp`
//! `loadDialInitData`, `pickDialCam` and the line set-up of
//! `playConSection`; `dialog_view.h`; GPLv3, the ScummVM team), which
//! follow the section executor `FUN_0042844c` and the INITDIAL parser
//! `FUN_0042ea18` of `atlantis.exe`.

use std::collections::HashMap;

use crate::con_script::script::latin1_bytes;
use crate::con_script::ConLine;

/// One row of `DIALOG\INITDIAL.TXT`: the files of a character's talking
/// head, per camera 2..5. "none" is stored as an empty name, except for the
/// camera UBBs, which keep it (an explicit camera may name it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialInitEntry {
    /// Columns 6..9: the UBB base names of cameras 2..5.
    pub cam_ubb: [String; 4],
    /// Columns 10..13 (optional): the SPF overlay base names.
    pub cam_spf: [String; 4],
    /// Column 4: the mouth animation (`DIALOG\PERSO\<base>.3DA`).
    pub anim_m: String,
    /// Column 5: the lip-sync and camera set-up
    /// (`DIALOG\PERSO\<base>.S3D`).
    pub s3d: String,
    /// Column 2: the head mesh (a type-1 F3DC, `.3DC`).
    pub anim_3dc: String,
    /// Column 3: the head animation (`.3DA`).
    pub anim_3da: String,
}

/// The INITDIAL table by character id, and the player character's id
/// (the row whose s3d field is "hero").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialInit {
    pub rows: HashMap<i32, DialInitEntry>,
    pub hero: i32,
}

impl DialInit {
    /// Parses `DIALOG\INITDIAL.TXT`: whitespace-separated rows
    /// `persoId unk anim3DC anim3DA animM s3d cam2 cam3 cam4 cam5
    /// [spf2..spf5]`, up to "FIN".
    pub fn parse(data: &[u8]) -> DialInit {
        let mut out = DialInit::default();
        let none = |s: &str| s.eq_ignore_ascii_case("none");
        let keep = |s: &str| {
            if none(s) {
                String::new()
            } else {
                s.to_string()
            }
        };
        for line in data.split(|&c| c == b'\r' || c == b'\n') {
            let tok: Vec<String> = line
                .split(|&c| c == b' ' || c == b'\t')
                .filter(|t| !t.is_empty())
                .map(crate::con_script::script::latin1)
                .collect();
            if tok.is_empty() {
                continue;
            }
            if tok[0].eq_ignore_ascii_case("FIN") {
                break;
            }
            if tok.len() < 10 {
                continue;
            }
            let id = crate::con_script::script::ConText(tok[0].as_bytes()).atoi(0);
            if id <= 0 {
                continue;
            }
            if tok[5].eq_ignore_ascii_case("hero") {
                out.hero = id;
            }
            let mut e = DialInitEntry {
                anim_m: keep(&tok[4]),
                s3d: keep(&tok[5]),
                anim_3dc: keep(&tok[2]),
                anim_3da: keep(&tok[3]),
                ..DialInitEntry::default()
            };
            for i in 0..4 {
                e.cam_ubb[i] = tok[6 + i].clone();
                if let Some(s) = tok.get(10 + i) {
                    e.cam_spf[i] = keep(s);
                }
            }
            out.rows.insert(id, e);
        }
        out
    }
}

/// What the end of a line's header says (`FUN_0042844c` @
/// 0x004286a6..0x0042877e, tested case-sensitively just before ']').
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineHeader {
    /// "cam"/"Cam" + one character: the camera (2 when out of 2..5); 0 for
    /// none.
    pub explicit_cam: i32,
    /// The speaker starts with "Seth" (0x00428608).
    pub seth: bool,
    /// "off", "off0" or "off1": no camera choice, no talking head.
    pub off: bool,
    /// "off0" (`FUN_004305e8`): the subtitle over the screen as it stands.
    pub off0: bool,
    /// "off1" (`FUN_00430884`; atlantis-cd-zh-tw-696832 `FUN_0042fe7c`,
    /// flag 0x004860bc set @ 0x00427f6a..0x00427f8d): the screen buffer
    /// under the subtitle. The 1997 executables hold no "off1" string and
    /// their scripts no such header; the 2001, GOG.com and Chinese builds
    /// test it.
    pub off1: bool,
}

impl LineHeader {
    pub fn parse(line: &ConLine) -> LineHeader {
        let tail = latin1_bytes(if line.params.is_empty() {
            &line.speaker
        } else {
            &line.params
        });
        let tl = tail.len();
        let tail_is =
            |lit: &[u8], back: usize| tl >= back && tail[tl - back..].get(..lit.len()) == Some(lit);
        let mut explicit_cam = 0;
        if tail_is(b"cam", 4) || tail_is(b"Cam", 4) {
            let d = tail[tl - 1];
            explicit_cam = if d.is_ascii_digit() {
                i32::from(d - b'0')
            } else {
                0
            };
            if !(2..=5).contains(&explicit_cam) {
                explicit_cam = 2;
            }
        }
        LineHeader {
            explicit_cam,
            seth: line.speaker.starts_with("Seth"),
            off: tail_is(b"off", 3) || tail_is(b"off0", 4) || tail_is(b"off1", 4),
            off0: tail_is(b"off0", 4),
            off1: tail_is(b"off1", 4),
        }
    }
}

/// The camera of a line (`FUN_0042844c` @ 0x004287c8..0x004289ea):
/// - "camN" at the end of the header: N, used even when that camera's
///   INITDIAL UBB is "none";
/// - an off / off0 / off1 line draws nothing: the camera stays the last
///   one (0x0077290a);
/// - otherwise draw (clock & 3) + 2 until the camera differs from the last
///   one and its UBB is not "none" (0x004287f6), then draw a second time
///   against the first draw (0x00428896; a Seth line after the 0x230 check,
///   0x00428979..0x004289e3);
/// - perso 0x230 is forced to camera 2 in the 2001 build (0x0042892e).
///
/// The draws read the millisecond clock (`FUN_00451d0c`, timeGetTime), not
/// rand, as the original's: its loop spins until the clock gives a usable
/// camera. `clock` is that clock, advanced by a millisecond for each
/// rejected draw (a spin of a few milliseconds, shorter than a frame). With
/// fewer than two usable cameras the original would loop forever; the port
/// stops after 64 draws and keeps the last drawn camera (a hang guard,
/// unreachable with the shipped INITDIAL, whose rows all have two usable
/// cameras or more).
pub fn pick_dial_cam(
    table: &DialInit,
    perso: i32,
    header: &LineHeader,
    last_cam: i32,
    rhea_camera2: bool,
    clock: &mut u64,
) -> i32 {
    let row = table.rows.get(&perso);
    let usable = |cam: i32| match row {
        None => true,
        Some(r) => {
            let s = &r.cam_ubb[(cam - 2) as usize];
            !s.is_empty() && !s.starts_with("none")
        }
    };
    let mut draw = |last: i32| {
        let mut cam = 2;
        for _ in 0..64 {
            cam = (*clock & 3) as i32 + 2;
            if cam != last && usable(cam) {
                return cam;
            }
            *clock += 1;
        }
        cam
    };
    let mut cam = if (2..=5).contains(&last_cam) {
        last_cam
    } else {
        2
    };
    let drawn = header.explicit_cam == 0 && !header.off;
    if header.explicit_cam != 0 {
        cam = if (2..=5).contains(&header.explicit_cam) {
            header.explicit_cam
        } else {
            2
        };
    } else if drawn {
        cam = draw(last_cam);
        if !header.seth {
            cam = draw(cam);
        }
    }
    if perso == 0x230 && rhea_camera2 {
        cam = 2;
    }
    if drawn && header.seth {
        cam = draw(cam);
    }
    cam
}

/// The INITDIAL row (0x0077290c) of a line: the IncPerso count for a Seth
/// line (0x00428a91); otherwise ClicPerso's low byte (0x00428924), so a
/// ClicPerso below 0x200 still picks row 0x200 + (ClicPerso & 0xff).
pub fn dial_row(seth_path: bool, clic_perso: i32, inc_perso: u8) -> i32 {
    0x200
        + if seth_path {
            i32::from(inc_perso)
        } else {
            clic_perso & 0xff
        }
}

/// A talking-head line (not voice-only) whose UBB does not open. A row
/// with no INITDIAL record has an empty UBB name, so `FUN_0042ee88` opens
/// "dialog\perso\" alone (`FUN_00434814` @ 0x0042f005); an explicit camN
/// whose INITDIAL UBB is "none" (`FUN_0042844c` keeps it: the draw loops
/// are skipped, 0x004287cf) has the name "none.ubb", which no archive
/// holds. The open fails, an error box is shown (dialog 0x65, code 0xc) and
/// the line returns before its subtitle and voice (0x0042f00f..0x0042f056);
/// the letter still counts (0x00428b21). `cam_ubb`: the record's name for
/// the line's camera ("none" kept). No line of the shipped CON files lands
/// an explicit camera on a "none" one.
pub fn head_open_fails(voice_only: bool, have_record: bool, cam_ubb: &str) -> bool {
    !voice_only && (!have_record || cam_ubb.is_empty() || cam_ubb.eq_ignore_ascii_case("none"))
}

/// The text of that error box: "Fichier introuvable : " (0x004bbf18) and
/// the path, "dialog\perso\" (0x004bbbfc) and the UBB name
/// (0x0042ef9d..0x0042f031).
pub const HEAD_OPEN_FAIL_TEXT: &str = "Fichier introuvable : dialog\\perso\\";

/// The error box's text for the UBB name the original appends: "" without
/// a record, "none.ubb" for a "none" camera (the parser appends ".ubb").
pub fn head_open_fail_text(ubb_name: &str) -> String {
    format!("{HEAD_OPEN_FAIL_TEXT}{ubb_name}")
}

/// View angles, radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Angles {
    pub alpha: f64,
    pub beta: f64,
}

/// The view behind a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineView {
    /// The panorama behind the talking head.
    pub view: Angles,
    /// The view angles 0x00bcb59c / 0x00bcb598 during and after the line.
    pub kept: Angles,
    /// The walk's end angles become `kept` before the line.
    pub entry_update: bool,
    /// The camera's FOV is set (`FUN_0045179a(FOV[cam])`); else no FOV
    /// call, the FOV in effect stays (see [`line_fov`]).
    pub cam_fov: bool,
}

/// The panorama's FOV over a talking-head line: the camera's
/// ([`DIALOG_CAM_FOV_DEG`], the table 0x00496080) when the line sets one
/// ([`LineView::cam_fov`]: `FUN_0042844c` @ 0x00428a14 / 0x00428ada), not
/// for a voice-only line (`FUN_00430258` draws at 75.137 whatever was set,
/// 0x0043044c; off0 / off1 do not redraw the view) nor without a panorama;
/// `None`: the FOV in effect stays. The original resets 75.137 after every
/// line (0x00428b43..0x00428b4d); the port puts back the FOV it had before
/// the line, which is [`WARP_FOV_DEG`]. For the four shipped talking heads
/// with no FOV call (ClicPerso below 0x200, each the first line of its
/// walk) the original's FOV is the one the walk started with, 75.137.
///
/// [`DIALOG_CAM_FOV_DEG`]: crate::warp::DIALOG_CAM_FOV_DEG
/// [`WARP_FOV_DEG`]: crate::warp::WARP_FOV_DEG
pub fn line_fov(warp_loaded: bool, view: &LineView, voice_only: bool, cam: i32) -> Option<f32> {
    if !warp_loaded || voice_only || !view.cam_fov {
        return None;
    }
    let i = usize::try_from(cam - 2).ok()?;
    crate::warp::DIALOG_CAM_FOV_DEG.get(i).copied()
}

/// `FUN_00451915`: beta kept within +-max, else the limit.
fn clamp_beta(b: f64, max: f64) -> f64 {
    if b > max {
        max
    } else if b < -max {
        -max
    } else {
        b
    }
}

/// `FUN_00451915`: alpha >= 2 pi -> alpha - 2 pi, < 0 -> + 2 pi.
fn wrap_alpha(a: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    if a >= two_pi {
        a - two_pi
    } else if a < 0.0 {
        a + two_pi
    } else {
        a
    }
}

/// The view behind a dialog line (`FUN_0042844c` @ 0x00428949..
/// 0x00428b5e). The view angles only move on a line with explicit header
/// angles; the turn of a talking head is drawn into the projection grid
/// alone, so each line turns from the same kept angles.
///
/// `seth_path`: the header starts with "Seth" and has no off flag;
/// `angle`: the explicit header angles (radians); `beta_max`:
/// `FUN_00451915`'s pitch limit.
pub fn line_view(
    kept: Angles,
    seth_path: bool,
    clic_perso: i32,
    angle: Option<(f64, f64)>,
    beta_max: f64,
) -> LineView {
    let mut l = LineView {
        view: kept,
        kept,
        entry_update: false,
        cam_fov: false,
    };
    if seth_path {
        // 0x004289ef..0x00428a75: entry angles := kept; FOV of the camera
        // (0x004289ff..0x00428a14); the grid at (alpha + pi, 0).
        l.entry_update = true;
        l.cam_fov = true;
        l.view.alpha = wrap_alpha(kept.alpha + std::f64::consts::PI);
        l.view.beta = 0.0;
    } else if clic_perso >= 0x200 {
        // 0x00428a9d..0x00428ae7 (off lines too): FOV 75.137 then the
        // camera's (0x00428ab6..0x00428ada); the grid at (alpha, 0). Below
        // 0x200 (JL 0x00428afc) none of this runs: no FOV call.
        l.entry_update = true;
        l.cam_fov = true;
        l.view.beta = 0.0;
    }
    if let Some((x, y)) = angle {
        // 0x00428afc..0x00428b0e: FUN_00451915(X, Y) writes the globals.
        l.view.alpha = wrap_alpha(x);
        l.view.beta = clamp_beta(y, beta_max);
        l.kept = l.view;
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rad(d: f64) -> f64 {
        d.to_radians()
    }

    fn deg(r: f64) -> i32 {
        let mut d = (r.to_degrees()) % 360.0;
        if d < 0.0 {
            d += 360.0;
        }
        ((d + 0.5).floor() as i32) % 360
    }

    /// (seth, off, perso, angle, view degrees) of a line.
    type L = (bool, bool, i32, Option<(i32, i32)>, (i32, i32));

    /// The lines of a walk, as the executor drives them: the kept angles
    /// carry from line to line, the entry angles follow `entry_update`.
    fn walk(lines: &[L], end: (i32, i32)) {
        let mut kept = Angles {
            alpha: rad(30.0),
            beta: rad(5.0),
        };
        let mut entry = kept;
        for (i, &(seth, off, perso, angle, (vx, vy))) in lines.iter().enumerate() {
            let v = line_view(
                kept,
                seth && !off,
                perso,
                angle.map(|(x, y)| (rad(f64::from(x)), rad(f64::from(y)))),
                rad(50.0) * 0.9,
            );
            if v.entry_update {
                entry = kept;
            }
            let shown = if off { v.kept } else { v.view };
            assert_eq!(
                (deg(shown.alpha), deg(shown.beta)),
                (vx, vy),
                "line {}",
                char::from(b'A' + i as u8)
            );
            kept = v.kept;
        }
        assert_eq!((deg(entry.alpha), deg(entry.beta)), end);
    }

    // The views of the original FUN_0042844c run under emulation, the
    // player at 30 / 5 degrees (ScummVM's video_dialog_view.h cases).

    #[test]
    fn chapi003_35() {
        walk(
            &[
                (false, false, 520, None, (30, 0)),
                (true, false, 514, None, (210, 0)),
                (false, false, 520, None, (30, 0)),
                (true, false, 514, None, (210, 0)),
                (false, false, 520, None, (30, 0)),
            ],
            (30, 5),
        );
    }

    #[test]
    fn chapi003_43() {
        walk(
            &[
                (true, false, 514, None, (210, 0)),
                (false, false, 522, Some((122, 0)), (122, 0)),
                (true, false, 514, None, (302, 0)),
                (false, false, 520, Some((325, 0)), (325, 0)),
                (false, false, 522, Some((122, 0)), (122, 0)),
                (false, false, 522, Some((122, 0)), (122, 0)),
                (false, false, 520, Some((325, 0)), (325, 0)),
            ],
            (122, 0),
        );
    }

    #[test]
    fn chapi016_206() {
        walk(
            &[
                (false, false, 520, None, (30, 0)),
                (false, true, 527, None, (30, 5)),
                (false, false, 520, None, (30, 0)),
            ],
            (30, 5),
        );
    }

    /// The camera's FOV is set only on the Seth path (0x004289ff) and for
    /// ClicPerso >= 0x200 (0x00428a9d; JL 0x00428afc skips the FOV calls).
    /// The FOV in effect at the call of FUN_0042ee88 (0x00428b1c), the
    /// original run under emulation: CHAPI056 /744 A (ClicPerso 329) and
    /// CHAPI065 /901 A (323): no FOV call; CHAPI003 /40 A (521, explicit
    /// angles and cam2): cam 2's; CHAPI056 /747 A (Seth, 514) and /747 B
    /// (663): the camera's.
    #[test]
    fn cam_fov() {
        let kept = Angles {
            alpha: rad(30.0),
            beta: rad(5.0),
        };
        let v = |seth: bool, perso: i32, angle: Option<(f64, f64)>| {
            line_view(kept, seth, perso, angle, rad(50.0) * 0.9)
        };
        assert!(!v(false, 329, None).cam_fov);
        assert!(!v(false, 323, None).cam_fov);
        let lascoyt = v(false, 521, Some((rad(269.0), 0.0)));
        assert!(lascoyt.cam_fov);
        assert_eq!(line_fov(true, &lascoyt, false, 2), Some(20.68));
        assert!(v(true, 514, None).cam_fov);
        assert!(v(false, 663, None).cam_fov);
        // Not over a voice-only line, nor without a panorama.
        assert_eq!(line_fov(true, &lascoyt, true, 2), None);
        assert_eq!(line_fov(false, &lascoyt, false, 2), None);
        assert_eq!(line_fov(true, &v(false, 329, None), false, 4), None);
        assert_eq!(line_fov(true, &v(true, 514, None), false, 4), Some(48.0));
    }

    #[test]
    fn chapi033_450_pitch_clamp() {
        walk(&[(false, false, 604, Some((0, 128)), (0, 45))], (30, 5));
    }

    #[test]
    fn dial_rows() {
        assert_eq!(dial_row(false, 329, 0), 585);
        assert_eq!(dial_row(false, 323, 0), 579);
        assert_eq!(dial_row(false, 539, 0), 539);
        assert_eq!(dial_row(true, 520, 2), 514);
        assert_eq!(dial_row(false, 0, 0), 512);
    }

    fn table(id: i32, cams: [&str; 4]) -> DialInit {
        let mut t = DialInit::default();
        let e = DialInitEntry {
            cam_ubb: cams.map(str::to_string),
            ..DialInitEntry::default()
        };
        t.rows.insert(id, e);
        t
    }

    fn header(explicit_cam: i32, seth: bool, off: bool) -> LineHeader {
        LineHeader {
            explicit_cam,
            seth,
            off,
            off0: false,
            off1: false,
        }
    }

    #[test]
    fn explicit_cam_on_unavailable_cam() {
        let t = table(524, ["none", "pecan2c3", "pecan2c4", "pecan2c5"]);
        let mut clock = 7;
        assert_eq!(
            pick_dial_cam(&t, 524, &header(2, false, false), 0, true, &mut clock),
            2
        );
    }

    #[test]
    fn rhea_forced_cam2() {
        let t = table(560, ["rhean1c2", "rhean1c3", "rhean1c4", "rhean1c5"]);
        for start in 0..32u64 {
            let mut clock = start;
            assert_eq!(
                pick_dial_cam(&t, 560, &header(0, false, false), 0, true, &mut clock),
                2
            );
            assert_eq!(
                pick_dial_cam(&t, 560, &header(4, false, false), 0, true, &mut clock),
                2
            );
        }
    }

    #[test]
    fn off_line_keeps_cam() {
        let t = table(560, ["rhean1c2", "rhean1c3", "rhean1c4", "rhean1c5"]);
        let mut clock = 3;
        assert_eq!(
            pick_dial_cam(&t, 560, &header(0, false, true), 4, true, &mut clock),
            2
        );
        assert_eq!(
            pick_dial_cam(&t, 524, &header(0, false, true), 5, true, &mut clock),
            5
        );
        // The 1999 build: no forced camera for 0x230.
        assert_eq!(
            pick_dial_cam(&t, 560, &header(0, false, true), 4, false, &mut clock),
            4
        );
    }

    /// A drawn camera is never one whose UBB is "none", whatever the clock
    /// (garde17, row 604: camera 2 is "none").
    #[test]
    fn drawn_cam_never_none() {
        let t = table(604, ["none", "grdan1c3", "grdan1c4", "grdan1c5"]);
        for pass in 0..200u64 {
            let mut clock = pass * 16;
            let last = (pass % 5) as i32 + 1;
            for seth in [false, true] {
                let cam = pick_dial_cam(&t, 604, &header(0, seth, false), last, true, &mut clock);
                assert_ne!(cam, 2);
                assert!((3..=5).contains(&cam));
            }
        }
    }

    #[test]
    fn header_flags() {
        let line = |speaker: &str, params: &str| ConLine {
            speaker: speaker.into(),
            params: params.into(),
            ..ConLine::default()
        };
        assert_eq!(LineHeader::parse(&line("Garde13", "cam3")).explicit_cam, 3);
        assert_eq!(LineHeader::parse(&line("Agathe", "Cam9")).explicit_cam, 2);
        assert_eq!(
            LineHeader::parse(&line("EnnemiPilot", "120,0,cam3")).explicit_cam,
            3
        );
        let h = LineHeader::parse(&line("Seth", "off"));
        assert!(h.seth && h.off && !h.off0);
        let h = LineHeader::parse(&line("Gimbas", "off0"));
        assert!(h.off && h.off0 && !h.off1);
        let h = LineHeader::parse(&line("Commander", "off1"));
        assert!(h.off && h.off1 && !h.off0);
        let h = LineHeader::parse(&line("Seth", "off1"));
        assert!(h.seth && h.off && h.off1);
        assert!(LineHeader::parse(&line("Seth", "")).seth);
        assert!(!LineHeader::parse(&line("seth", "")).seth);
        assert_eq!(LineHeader::parse(&line("Seth", "")), header(0, true, false));
    }
}
