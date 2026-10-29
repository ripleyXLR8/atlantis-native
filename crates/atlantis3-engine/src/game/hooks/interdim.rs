//! The interdimension (`interdim.wam`, world 5, chapter 52): the dolphin's
//! cut animation, the bubbles shown sharp or blurred from the worlds
//! visited, the dolphin's subjects, and flag 25 once the three clues
//! (flags 159, 866, 867) are heard.

use super::{Hx, ZoneRec};

/// The interdimension's globals.
#[derive(Debug, Default, Clone)]
pub struct State {
    /// In `interdim.wam` (@ 0x007592ac, written by the view hook).
    pub here: bool,
    /// A byte the entry clears (@ 0x0071d810; set to 1 by the load, read
    /// by no code).
    pub b71d810: bool,
}

impl State {
    pub fn new() -> State {
        State {
            here: false,
            b71d810: true,
        }
    }

    /// @ 0x00459e70.
    pub fn after_load(&mut self) {
        self.b71d810 = true;
    }
}

/// The entry (@ 0x004c79d0): the dolphin's animation in view 8; at the
/// first view after `CHANGEWAM=interdim.wam` (flag 897 = 1) the subjects
/// and the bubbles.
pub(super) fn view(h: &mut Hx) {
    if h.vue() == 8 {
        if h.flag(895) == 0 {
            h.set_flag(895, 1);
            h.run("(PlayCutAnimP=INT_DAUPHIN,1)");
        } else {
            h.run("(PlayCutAnimP=INT_DAUPHIN,2)");
        }
        h.run("(PlayAnimP=INT_DAUPHIN,2)");
    }
    if h.flag(897) != 1 {
        h.g.puzzles.interdim.b71d810 = false;
        return;
    }
    log::info!("PuzzleInterdim_Init : 1er Warp de interdim.wam");
    log::info!("-- Interdimension init de depart");
    h.set_flag(897, 0);
    dolphin_subjects(h);
    h.run("(HideObjet=INT_BULLESNETTES)");
    h.run("(HideObjet=INT_BULLESMOYENNES)");
    h.run("(HideObjet=INT_BULLESFLOUES)");
    h.run("(HideOBJET=INT_BULLENETFIN)");
    h.run("(HideOBJET=INT_BULLEF2FIN)");
    h.run("(HideOBJET=INT_BULLEF3FIN)");
    match h.flag(892) {
        1 => {
            h.run("(ShowObjet=INT_BULLESFLOUES)");
            h.run("(ShowOBJET=INT_BULLEF3FIN)");
        }
        2 => {
            h.run("(ShowObjet=INT_BULLESMOYENNES)");
            h.run("(ShowOBJET=INT_BULLEF3FIN)");
        }
        _ => {
            h.run("(ShowObjet=INT_BULLESNETTES)");
            if h.flag(26) == 0 {
                h.run("(ShowOBJET=INT_BULLEF2FIN)");
            } else {
                h.run("(ShowOBJET=INT_BULLENETFIN)");
            }
        }
    }
    h.run("(HideOBJET=INT_BULLENETCOMPLEXE)");
    h.run("(HideOBJET=INT_BULLEFLOUCOMPLEXE)");
    h.run("(HideOBJET=INT_BULLENETMARAIS)");
    h.run("(HideOBJET=INT_BULLENETMAGICIEN)");
    h.run("(HideOBJET=INT_BULLEF2MARAIS)");
    h.run("(HideOBJET=INT_BULLEF2MAGICIEN)");
    h.run("(HideOBJET=INT_BULLEF3MARAIS)");
    h.run("(HideOBJET=INT_BULLEF3MAGICIEN)");
    if h.flag(890) == 1 {
        h.run("(ShowOBJET=INT_BULLENETCOMPLEXE)");
    } else {
        h.run("(ShowOBJET=INT_BULLEFLOUCOMPLEXE)");
    }
    if h.flag(890) == 3 {
        h.run("(ShowOBJET=INT_BULLENETMAGICIEN)");
    } else if h.flag(893) == 1 {
        h.run("(ShowOBJET=INT_BULLEF2MAGICIEN)");
    } else {
        h.run("(ShowOBJET=INT_BULLEF3MAGICIEN)");
    }
    if h.flag(890) == 4 {
        h.run("(ShowOBJET=INT_BULLENETMARAIS)");
    } else if h.flag(894) == 1 {
        h.run("(ShowOBJET=INT_BULLEF2MARAIS)");
    } else {
        h.run("(ShowOBJET=INT_BULLEF3MARAIS)");
    }
    h.run("(HideOBJET=INT_BULLENETCOULOIR)");
    h.run("(HideOBJET=INT_BULLEFLOUCOULOIR)");
    if h.flag(890) == 1 && h.flag(873) == 0 {
        h.run("(ShowOBJET=INT_BULLENETCOULOIR)");
    } else {
        h.run("(ShowOBJET=INT_BULLEFLOUCOULOIR)");
    }
    h.g.puzzles.interdim.b71d810 = false;
}

/// Every frame (@ 0x004c7c30; the zone record is not read): the subjects
/// rebuilt when the scripts ask (flag 868), flag 25 once the three clues
/// are heard.
pub(super) fn frame(h: &mut Hx, _z: &ZoneRec) {
    if h.flag(868) != 0 {
        h.set_flag(868, 0);
        dolphin_subjects(h);
    }
    if h.flag(159) != 0 && h.flag(866) != 0 && h.flag(867) != 0 {
        h.set_flag(25, 1);
    }
}

/// `(EnableSuj,INT_DAUPHIN,s)` then `(DisableSuj,...)` when the subject's
/// "heard" flag is set.
fn offer(h: &mut Hx, subject: &str, heard: usize) {
    h.run(&format!("(EnableSuj,INT_DAUPHIN,{subject})"));
    if h.flag(heard) != 0 {
        h.run(&format!("(DisableSuj,INT_DAUPHIN,{subject})"));
    }
}

/// The dolphin's subjects (@ 0x004c7570, `InitDialDauphin`), by the world
/// left (flag 890).
fn dolphin_subjects(h: &mut Hx) {
    for s in [
        "HOGGAR", "BAGDAD", "PALEO", "COULOIR", "DAUPHIN", "CRANE", "VIEUX", "TARGUI", "OMEGA",
        "HUMANITE", "HEROINE", "EXPAND", "BOSS",
    ] {
        h.run(&format!("(KillSuj,INT_DAUPHIN,DIAL_INT_{s})"));
    }
    match h.flag(890) {
        1 => {
            if h.flag(26) == 0 {
                if h.flag(159) != 0 {
                    offer(h, "DIAL_INT_BOSS", 982);
                    offer(h, "DIAL_INT_DAUPHIN", 983);
                    offer(h, "DIAL_INT_CRANE", 984);
                    offer(h, "DIAL_INT_HEROINE", 985);
                    offer(h, "DIAL_INT_COULOIR", 986);
                    offer(h, "DIAL_INT_OMEGA", 994);
                    if h.flag(893) != 0 {
                        offer(h, "DIAL_INT_BAGDAD", 987);
                    }
                    if h.flag(894) != 0 {
                        offer(h, "DIAL_INT_PALEO", 988);
                    }
                }
            } else {
                offer(h, "DIAL_INT_BOSS", 990);
                offer(h, "DIAL_INT_TARGUI", 991);
                offer(h, "DIAL_INT_OMEGA", 992);
                offer(h, "DIAL_INT_HEROINE", 993);
            }
        }
        3 => {
            if h.flag(866) != 0 {
                if h.flag(891) != 0 {
                    offer(h, "DIAL_INT_BAGDAD", 987);
                    offer(h, "DIAL_INT_HOGGAR", 986);
                }
                if h.flag(894) != 0 {
                    offer(h, "DIAL_INT_BAGDAD", 987);
                    offer(h, "DIAL_INT_PALEO", 988);
                }
            }
        }
        4 => {
            if h.flag(867) != 0 {
                if h.flag(891) != 0 {
                    offer(h, "DIAL_INT_PALEO", 988);
                    offer(h, "DIAL_INT_HOGGAR", 986);
                }
                if h.flag(893) != 0 {
                    offer(h, "DIAL_INT_PALEO", 988);
                    offer(h, "DIAL_INT_BAGDAD", 987);
                }
            }
        }
        w => log::warn!(
            "InitDialDauphin : Monde de provenance pour l'interdimension mal initialisé ({w})"
        ),
    }
}
