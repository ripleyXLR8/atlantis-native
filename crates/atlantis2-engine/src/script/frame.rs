//! When scripts run.
//!
//! Only the panorama (warp) loop runs scans. Each warp frame
//! (`AtlWarp_continue` @ 0x0045a3b0), when not paused and no transition
//! runs, does in this order:
//!
//! 1. a hit animation record (@ 0x0045a650): [`Script::take_object`] for
//!    an object to take (kind 1), `CLICOBJET` (kind 3) or `CLICPERSO`
//!    (kind 4) through [`Script::vars`];
//! 2. a character with subjects clicked with an empty hand (@ 0x0045a7c0):
//!    `CLICPERSO` and the subject menu;
//! 3. a click (@ 0x0049ca80): on a move zone [`Script::departure_scan`],
//!    then the move unless `ABORTDEPART`; on an enabled zone of type > 2,
//!    `CLICZONE` = its id;
//! 4. [`Script::gere_frame`] (`Interp_GereFrame` @ 0x00484b70): the queued
//!    command, the arrival scan, the angles, the timer scans;
//! 5. [`Script::click_scan`] (@ 0x0045a870) on a click with no subject menu
//!    and no overlay open;
//! 6. the place sounds;
//!
//! then a pending move completes ([`Script::arrive`]: `ARRIVEEVUE` = the new
//! view, whose arrival scan runs in the next frame), and `CLICZONE` = 0.
//! The other loops (video, dialog, menus, viewers) run no scan; a section
//! that waits for one of them goes on when it ends
//! ([`Script::video_ended`], [`Script::dialog_ended`],
//! [`Script::disc_inserted`]).
//!
//! The clocks `TIME1`, `TIME2` and `RANDOM` change once per second of real
//! time, from the 15 Hz tick ([`Script::tick`]), in every loop.

use super::{Host, Script};
use crate::fpu::{ftol, Fpu};

impl Script {
    /// Start-up (@ 0x0049d680, the script part): the tables as
    /// `LoadDefNames` and @ 0x004846f0 leave them, `LIMITEANGLEWARPY` = 70,
    /// `LIMITEANGLEWARPX` = −1, `NEARW77` = 6, no click (@ 0x0049d970),
    /// `CHAPTER` = the current chapter (0) (@ 0x00483bc0), and that chapter
    /// loaded. False when it is missing (fatal in the original).
    pub fn boot(&mut self, host: &mut dyn Host) -> bool {
        self.flags.iter_mut().for_each(|f| *f = 0);
        let k = self.keys.clone();
        self.vars[k.limiteanglewarpy] = 70;
        self.vars[k.limiteanglewarpx] = -1;
        self.vars[k.nearw77] = 6;
        self.click = false;
        self.click_aux = 0;
        self.vars[k.chapter] = self.chapter;
        if !self.load_chapter(host, self.chapter & 0xff) {
            self.fail(format!("chapitre {} introuvable", self.chapter));
            return false;
        }
        true
    }

    /// A new game for a player without a save (menu @ 0x00488f12): the
    /// `/INIT` of the chapter loaded at start-up. How the menu resets the
    /// variables before (FUN_00489040) is not read.
    pub fn new_game(&mut self, host: &mut dyn Host) -> bool {
        self.run_init(host)
    }

    /// The per-view reset (@ 0x00483ce0), on every arrival and by `vue=`:
    /// the click variables, `VUE`, `ABORTDEPART`, the limits back to 70
    /// and −1, `SACADOS`, `FLAG`, the object in hand and the cursor.
    pub fn view_reset(&mut self) {
        let k = self.keys.clone();
        self.view_number = self.current_view.unwrap_or(0);
        self.chapter = self.vars[k.chapter];
        for v in [k.clicperso, k.cliczone, k.clicobjet] {
            self.vars[v] = 0;
        }
        self.vars[k.vue] = self.view_number;
        self.vars[k.abortdepart] = 0;
        self.vars[k.limiteanglewarpy] = 70;
        self.vars[k.limiteanglewarpx] = -1;
        self.vars[k.sacados] = 0;
        self.vars[k.flag] = 0;
        self.vars[k.objetenmain] = 0;
        self.set_cursor(0, false);
        self.click = false;
        self.click_aux = 0;
    }

    /// A move completed (`AtlWarp_continue`, @ 0x0045a3b0 step 5):
    /// `LASTVUE`, `VUE`, and `ARRIVEEVUE` = the new view, for the arrival
    /// scan of the next frame. The host enters the view.
    pub fn arrive(&mut self, view: i32) {
        self.set_current_view(view);
        self.vars[self.keys.arriveevue] = view;
    }

    /// `Interp_GereFrame` (@ 0x00484b70), with the current view angles in
    /// radians (`FUN_0045adf0`).
    pub fn gere_frame(&mut self, host: &mut dyn Host, yaw: f32, pitch: f32) {
        if self.fatal.is_some() {
            return;
        }
        let k = self.keys.clone();
        if let Some(cmd) = self.queued.take() {
            self.run_command(host, &cmd);
        }
        let event = self.vars[k.testspecialevent];
        if event != 0 {
            host.special_event(event);
        }
        if self.vars[k.exit] != 0 {
            log::info!("Interp_GereFrame: EXIT, quitat2=1");
            self.quit = true;
        }
        if self.vars[k.arriveevue] != 0 {
            self.view_reset();
            let next = self.vars[k.objetenmain_next_warp];
            if next != 0 {
                self.hold(next);
                self.vars[k.objetenmain_next_warp] = 0;
            }
            log::debug!("# CtrlScenarIN (arrivéevue in interp_gereframe) #");
            self.ctrl_scenar_in(host);
            self.vars[k.flag] = 0;
            self.vars[k.arriveevue] = 0;
            self.vars[k.cliczone] = 0;
        }
        if self.vars[k.abortdepart] != 0 {
            self.vars[k.arriveevue] = 0;
            self.vars[k.abortdepart] = 0;
            self.vars[k.departvue] = 0;
            self.click = false;
            host.abort_departure();
        }
        self.vars[k.anglewarpx] = self.degrees(yaw);
        self.vars[k.anglewarpy] = self.degrees(pitch);
        self.timer_scans(host);
    }

    /// An angle in whole degrees, 0..359 (@ 0x00484d14: × 180, ÷ π, `_ftol`,
    /// + 360 when negative).
    pub fn degrees(&self, rad: f32) -> i32 {
        let fpu = self.fpu;
        let pi = f64::from(f32::from_bits(0x4049_0fdb));
        let d = ftol(fpu.div(fpu.mul(f64::from(rad), 180.0), pi));
        if d < 0 {
            d + 360
        } else {
            d
        }
    }

    /// The click scan (@ 0x0045a870), on a click when neither the subject
    /// menu (the caller's to check, @ 0x006617b0) nor the overlay is open:
    /// `CLICPERSO` = the character hit (a kind 2 record) if any, the scan,
    /// the timer scans, then the click forgotten (`CLICPERSO` = `CLICOBJET`
    /// = 0, `SUJET` = −2).
    pub fn click_scan(&mut self, host: &mut dyn Host, character: Option<i32>) {
        if !self.click || self.overlay {
            return;
        }
        let k = self.keys.clone();
        if let Some(id) = character {
            self.vars[k.clicperso] = id;
        }
        self.ctrl_scenar_in(host);
        self.timer_scans(host);
        self.vars[k.clicperso] = 0;
        self.vars[k.clicobjet] = 0;
        self.vars[k.sujet] = -2;
        self.click = false;
        self.click_aux = 0;
    }

    /// The departure scan of a click on a zone (@ 0x0049ca80), which the
    /// caller runs on a click with no subject menu, overlay, dialog or hit
    /// record: `DEPARTVUE` = the target view (zones of type 1 and 2), the
    /// scan (`CtrlScenarOUT` @ 0x00483cc0), `DEPARTVUE` = 0. Whether the
    /// click goes on (to the move, or `CLICZONE` for a zone of type > 2):
    /// no `ABORTDEPART` and the click still there.
    pub fn departure_scan(&mut self, host: &mut dyn Host, target: Option<i32>) -> bool {
        let k = self.keys.clone();
        if let Some(t) = target {
            self.vars[k.departvue] = t;
        }
        log::debug!("# CtrlScenarOUT #");
        self.ctrl_scenar_in(host);
        self.vars[k.departvue] = 0;
        self.vars[k.abortdepart] == 0 && self.click
    }

    /// An object taken from the place (@ 0x0045a650, a kind 1 record):
    /// `CLICOBJET`, hidden, in the bag with its sprite, in the hand.
    pub fn take_object(&mut self, host: &mut dyn Host, id: i32, sprite: u8) {
        let k = self.keys.clone();
        self.vars[k.clicobjet] = id;
        self.hide(host, id, 2, false);
        self.bag_add(host, id, sprite);
        self.vars[k.objetenmain] = id;
        self.set_cursor(i32::from(sprite), true);
    }

    /// The subject menu's answer (@ 0x0045a9f0): `SUJET` = the chosen
    /// subject's value, or, the menu closed, `CLICPERSO` = 0 and `SUJET` =
    /// −2.
    pub fn subject_chosen(&mut self, value: Option<i32>) {
        match value {
            Some(v) => self.vars[self.keys.sujet] = v,
            None => self.dialog_reset(),
        }
    }

    /// `CLICPERSO` = 0, `SUJET` = −2 (the end of a dialog, a dialog that
    /// cannot start, a closed subject menu).
    pub(crate) fn dialog_reset(&mut self) {
        self.vars[self.keys.clicperso] = 0;
        self.vars[self.keys.sujet] = -2;
    }

    /// Goes on with a waiting section (`FUN_00483f90(resume point)`).
    pub(crate) fn resume_if_pending(&mut self, host: &mut dyn Host) {
        if self.resume_pending {
            self.resume_pending = false;
            let at = self.resume_at;
            self.run_section(host, at);
        }
    }

    /// The end of a cinematic (@ 0x0040c060): the click forgotten, then the
    /// waiting section goes on.
    pub fn video_ended(&mut self, host: &mut dyn Host) {
        self.click = false;
        self.resume_if_pending(host);
    }

    /// The end of a dialog (@ 0x004808a0): `CLICPERSO` = 0, `SUJET` = −2,
    /// then the waiting section goes on.
    pub fn dialog_ended(&mut self, host: &mut dyn Host) {
        self.dialog_reset();
        self.resume_if_pending(host);
    }

    /// The disc asked for is in (@ 0x0040dff0, the CD menu @ 0x00486eb0).
    pub fn disc_inserted(&mut self, host: &mut dyn Host) {
        self.resume_if_pending(host);
    }

    /// The 15 Hz tick (@ 0x0040caa0), when the window is active and the
    /// game not paused: one more tick and the four tick flags raised
    /// (@ 0x0040cabd..0x0040cad8); the seconds accumulator gains the
    /// scheduler time since this task's last run (`now - last`, seconds);
    /// over 10 s it is dropped, from 1 s one second is taken and `RANDOM` =
    /// the tick count & 0xff, `TIME1` and `TIME2` + 1. So the clocks count
    /// seconds of real time, at most one per tick.
    pub fn tick(&mut self, now: f64, last: f64) {
        let fpu = self.fpu;
        self.ticks = self.ticks.wrapping_add(1);
        self.tick_flags = [true; 4];
        let sum = fpu.add(fpu.sub(now, last), f64::from(self.seconds));
        self.seconds = fpu.f32(sum);
        if sum > 10.0 {
            self.seconds = 0.0;
            return;
        }
        if f64::from(self.seconds) >= 1.0 {
            self.seconds = fpu.f32(fpu.sub(f64::from(self.seconds), 1.0));
            let k = self.keys.clone();
            self.vars[k.random] = self.ticks & 0xff;
            self.vars[k.time1] = self.vars[k.time1].wrapping_add(1);
            self.vars[k.time2] = self.vars[k.time2].wrapping_add(1);
        }
    }

    /// The text of a variable's name, for messages.
    pub fn var_name(&self, index: usize) -> String {
        self.names
            .name(atlantis2_data::defnames::VARIABLES, index)
            .map(|n| n.iter().map(|&c| char::from(c)).collect())
            .unwrap_or_default()
    }

    /// The control word the script's float code runs under (the warp's
    /// renderer may change it after the first frame).
    pub fn set_fpu(&mut self, fpu: Fpu) {
        self.fpu = fpu;
    }
}
