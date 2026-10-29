//! Rhea's garden (`rhea.wam`, world 5, chapter 54): the old couple's
//! subjects asked in order (flag 875 = the step, 0..8; flag 877 = the
//! subject chapi054 noted: 11..16 the old man, 21..26 the old woman;
//! flag 876 = a reset asked; flag 874 = the subjects set up).

use super::{Hx, ZoneRec};

/// The garden's globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// A byte the entry clears (@ 0x0071eb18; set to 1 by the load,
    /// read by no code).
    pub b71eb18: bool,
    /// The dialog's two camera values per view (@ 0x0075577c,
    /// 0x00755780 through @ 0x0040d280 / 0x0040d290) and the two of
    /// @ 0x00714f14 / 0x00714f18, read by the dialog code (@ 0x0040b7cd,
    /// 0x0040b8ee).
    pub dialog_camera: Option<[f32; 4]>,
}

impl State {
    pub fn new() -> State {
        State {
            b71eb18: true,
            dialog_camera: None,
        }
    }

    /// @ 0x00459e70.
    pub fn after_load(&mut self) {
        self.b71eb18 = true;
    }
}

/// The original's fatal message (@ 0x00446460: a message box, then the
/// exit @ 0x004d6aa0).
fn fatal(h: &mut Hx, msg: String) {
    log::error!("{msg}");
    if h.s.fatal.is_none() {
        h.s.fatal = Some(msg);
    }
}

/// The entry (@ 0x004cd300): `Init Puzzle Rhéa`, flag 919 := 1 (read by
/// nothing), the dialog camera of the view.
pub(super) fn view(h: &mut Hx) {
    log::info!("Init Puzzle Rhéa");
    if h.flag(919) == 0 {
        h.set_flag(919, 1);
    }
    let cam = match h.vue() {
        10 => Some([485.0, 100.0, 50.0, 50.0]),
        11 => Some([165.0, 100.0, 60.0, 80.0]),
        12 => Some([115.0, 15.0, 80.0, 60.0]),
        13 => Some([90.0, 20.0, 40.0, 40.0]),
        v => {
            log::warn!("PuzzleRhea_Init : Vue invalide !!! ({v})");
            None
        }
    };
    let r = &mut h.g.puzzles.rhea;
    if cam.is_some() {
        // visual: the dialog's camera values (M4).
        r.dialog_camera = cam;
    }
    r.b71eb18 = false;
}

/// Every frame (@ 0x004cd450). The zone record is not read.
pub(super) fn frame(h: &mut Hx, _z: &ZoneRec) {
    let vue = h.vue();
    if h.flag(876) != 0 {
        h.set_flag(876, 0);
        h.set_flag(874, 0);
        h.run("(Vue=10)");
    }
    if h.flag(874) == 0 {
        log::info!("** Reset Puzzle Rhéa **");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_OMEGA)");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_VIEUX)");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_HUMANITE)");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_EXPAND)");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_CRANE)");
        h.run("(EnableSuj,INT_VIEUX,DIAL_INT_HEROINE)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_OMEGA)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_VIEUX)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_HUMANITE)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_EXPAND)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_CRANE)");
        h.run("(EnableSuj,INT_VIEILLE,DIAL_INT_HEROINE)");
        h.set_flag(874, 1);
        h.set_flag(875, 0);
        h.run("(AngleWarpXNow=100.0f)");
        h.run("(Dial=242)");
        return;
    }
    if h.flag(875) == 1 && vue == 11 {
        h.set_flag(875, 2);
        h.run("(Dial=252)");
        return;
    }
    let asked = h.flag(877);
    if asked == 0 {
        return;
    }
    // The right subject at the right step: (subject, step, dialog).
    let step = match asked {
        13 => Some((4, "(Dial=261)")),
        15 => Some((2, "(Dial=259)")),
        16 => Some((3, "(Dial=260)")),
        21 => Some((7, "(Dial=263)")),
        22 => Some((0, "(Dial=251)")),
        24 => Some((6, "(Dial=262)")),
        11 | 12 | 14 | 23 | 25 | 26 => None,
        _ => {
            fatal(
                h,
                format!("PuzzleRhea_Continue : flag877 possède une valeur non autorisée : {asked}"),
            );
            None
        }
    };
    if let Some((at, dial)) = step {
        if h.flag(875) == at {
            h.set_flag(875, at + 1);
            h.run(dial);
            h.set_flag(877, 0);
            return;
        }
    }
    // A wrong subject: the failure dialog by step, then a reset.
    let man = (11..=16).contains(&asked);
    let woman = (21..=26).contains(&asked);
    if man || woman {
        let dial = match h.flag(875) {
            0 | 6 | 7 => Some(if man { "(Dial=249)" } else { "(Dial=250)" }),
            1 | 5 | 8 => Some(if man { "(Dial=257)" } else { "(Dial=256)" }),
            2..=4 => Some(if man { "(Dial=264)" } else { "(Dial=265)" }),
            n => {
                fatal(
                    h,
                    format!("PuzzleRhea_Continue : flag875 possède une valeur non autorisée : {n}"),
                );
                None
            }
        };
        if let Some(dial) = dial {
            h.run(dial);
        }
        h.set_flag(876, 1);
    }
    h.set_flag(877, 0);
}
