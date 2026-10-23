//! The engine as the scripts' [`Host`]: what a `/set` line does outside
//! the script tables, and the engine's side of the run model (the scans of
//! the warp frame, the ends of the video and dialog loops, the music
//! player's states).
//!
//! The interpreter is taken out of the engine while it runs
//! ([`Engine::with_script`]), so that the engine can be its host; what both
//! keep (the click flags, the start angles of the next view entry, the
//! object in hand) is handed over around each call.

use std::sync::Arc;

use atlantis2_data::defnames;
use atlantis2_data::Folder;
use atlantis_codecs::decode_apc;

use super::{load_objects_bank, Engine, Event};
use crate::crt::Rand;
use crate::dialog::{Dialog, FrameContext, TickContext};
use crate::music;
use crate::script::{AnimOp, DialStart, Host, Keys, Script};
use crate::sound::{volume_to_attenuation, Pcm, VOICE_EFFECTS, VOICE_MUSIC};

/// The music's start volume during a dialog (@ 0x00477470 with the flag
/// @ 0x006725c8).
const DIALOG_START_VOLUME: i32 = 14;

/// The type a disabled click zone gains (@ 0x00410060, 0x15b38).
const DISABLED: i32 = 88888;

impl Engine {
    /// Runs `f` on the interpreter with the engine as its host; `None`
    /// when the scripts do not run this game. The click flags and the
    /// start angles of the next view entry are the engine's, given to the
    /// script before and taken back after; the object in hand is the
    /// script's; a cursor pair the script cleared (`objetenmain=0`,
    /// @ 0x0047bf40) is cleared in the engine too.
    pub(crate) fn with_script<R>(
        &mut self,
        f: impl FnOnce(&mut Script, &mut dyn Host) -> R,
    ) -> Option<R> {
        let mut s = self.con.take()?;
        s.click = self.click;
        s.click_aux = self.click_aux;
        s.double_click = self.double;
        let cursor_pair = (s.hand_drawn, s.hand_x);
        (s.start_x, s.start_y) = self.arrival;
        s.set_fpu(self.camera.fpu);
        if self.trace && s.lines.is_none() {
            s.lines = Some(Vec::new());
        }
        let r = f(&mut s, self);
        self.click = s.click;
        self.click_aux = s.click_aux;
        self.double = s.double_click;
        if (s.hand_drawn, s.hand_x) != cursor_pair && !s.hand_drawn {
            self.cursor.shown = None;
        }
        self.arrival = (s.start_x, s.start_y);
        let k = &s.keys;
        self.hand = s
            .cursor_objects
            .then(|| (s.vars[k.objetenmain], s.cursor_sprite));
        self.vars.objet_en_main = s.vars[k.objetenmain];
        self.vars.cliczone = s.vars[k.cliczone];
        self.vars.clicperso = s.vars[k.clicperso];
        self.vars.clicobjet = s.vars[k.clicobjet];
        let lines = s.lines.as_mut().map(std::mem::take).unwrap_or_default();
        self.con = Some(s);
        for (chapter, line) in lines {
            self.event(Event::Set { chapter, line });
        }
        self.after_refused_place();
        Some(r)
    }

    /// A script variable by name (`ReadVar` @ 0x00484ff0), from the
    /// interpreter or, while it runs a `vue=` line, from the variables it
    /// lent ([`Host::enter_view_with_vars`]); `None` without the scripts or
    /// for a name not in `#VARIABLES#`.
    pub(crate) fn script_var_or_lent(&self, name: &str) -> Option<i32> {
        if let Some(s) = self.con.as_deref() {
            return s.var(name);
        }
        let lent = self.lent_vars.as_ref()?;
        let i = self.names.id(defnames::VARIABLES, name.as_bytes()).ok()?;
        lent.get(i).copied()
    }

    /// Writes a script variable (when the scripts run).
    pub(crate) fn set_script_var(&mut self, key: impl FnOnce(&Keys) -> usize, value: i32) {
        if let Some(s) = self.con.as_mut() {
            let k = key(&s.keys);
            s.vars[k] = value;
        }
    }

    /// The CON interpreter, when the scripts run the game.
    pub fn script_state(&self) -> Option<&Script> {
        self.con.as_deref()
    }

    pub fn script_state_mut(&mut self) -> Option<&mut Script> {
        self.con.as_deref_mut()
    }

    /// The music file the player streams, if any.
    pub fn music_playing(&self) -> Option<i32> {
        self.player.stream.music()
    }

    /// Whether the game quits: the scripts' `EXIT`, or the menus.
    pub fn quit(&self) -> bool {
        self.con.as_ref().is_some_and(|s| s.quit) || self.quit_requested
    }

    /// `Interp_GereFrame` (@ 0x00484b70) in the warp frame: on an arrival,
    /// the per-view reset (which drops this frame's click) before the
    /// arrival scan; then the angles and the timer scans.
    pub(super) fn gere_frame(&mut self) {
        let arrival = self.con.as_ref().map_or(0, |s| s.vars[s.keys.arriveevue]);
        if arrival != 0 {
            self.per_view_reset();
            self.vars.arrivee_vue = arrival as u32;
            let vars = self.scan_vars(0);
            self.event(Event::ArrivalScan(vars));
        }
        let (yaw, pitch) = self.camera.angles();
        self.with_script(|s, h| s.gere_frame(h, yaw, pitch));
        self.vars.arrivee_vue = 0;
    }

    /// The music state machine (@ 0x00476fc0), on every dispatcher pass:
    /// the opens pending (1 from the start, 2 at a mark), the fade (3) and
    /// then, as in 4, the ring's refill, the stop (5). True when the
    /// refill wrote the ring.
    pub(super) fn music_frame(&mut self) -> bool {
        match self.music.state {
            1 => self.music_open(false),
            2 => self.music_open(true),
            3 => {
                self.fade_step();
                return self.music_refill();
            }
            4 => return self.music_refill(),
            5 => {
                // FUN_004778f0: the voice stopped, the file closed.
                self.voices.stop(VOICE_MUSIC);
                self.player.stream.close();
                self.music.state = 0;
                self.event(Event::MusicStop);
            }
            _ => {}
        }
        false
    }

    /// Play music `n` (`FUN_0047a200(n, mark, loops)`): nothing when it is
    /// the current one; else it becomes current and opens at once, or
    /// after the playing one has faded out (FUN_00477dc0).
    pub(super) fn play_music(&mut self, n: i32, mark: i32, loops: i32) {
        if n == self.player.current {
            return;
        }
        self.player.current = n;
        self.player.mark = mark;
        self.player.stream.loops = loops;
        let next = if mark == 0 { 1 } else { 2 };
        if self.music.state != 0 {
            self.music.switch(next);
        } else {
            self.music.state = next;
        }
    }

    /// States 1 and 2: the current music opened (two tries), from its
    /// start (FUN_00477410: the first 0x80000 bytes decoded into the music
    /// buffer, the volume 0, `srand` of the tick count; FUN_00477470: voice
    /// 0 created from the buffer, looping, then the start volume) or at
    /// its mark (FUN_004774f0: voice 0 created from the music buffer as it
    /// is; `GotoMark`; FUN_00477540: played at the start volume). The start
    /// volume is 14 in a dialog, else the music volume.
    fn music_open(&mut self, at_mark: bool) {
        let n = self.player.current;
        if !(self.music_file_open(n) || self.music_file_open(n)) {
            self.music.state = 0;
            self.stream_events();
            return;
        }
        if !at_mark {
            self.player.stream.decode_start();
            self.music.volume = 0;
            // `srand` of the 15 Hz count (@ 0x0047743f reads the counter
            // the 15 Hz task increments, @ 0x006569d8).
            self.rand = Rand::new(self.ticks);
        }
        let s = &self.player.stream;
        let pcm = Pcm {
            rate: s.rate,
            channels: s.channels,
            samples: Arc::new(s.buffer.clone()),
        };
        let att = if at_mark {
            -10000
        } else {
            self.music.attenuation()
        };
        self.voices.play(VOICE_MUSIC, pcm, true, att, 0);
        if at_mark {
            let mut ring = self.voices.music_ring();
            self.player
                .stream
                .goto_mark(self.player.mark, &mut self.rand, &mut ring);
        }
        self.music.volume = self.start_volume();
        self.voices
            .set_attenuation(VOICE_MUSIC, self.music.attenuation());
        self.music.state = 4;
        // The mark played: `GotoMark`'s, 0 from the start (where the
        // original leaves @ 0x006725a4 as it was).
        let mark = if at_mark {
            self.player.stream.current_mark
        } else {
            0
        };
        self.player.stream.events.clear();
        self.event(Event::Music { music: n, mark });
    }

    /// The music's start volume: 14 in a dialog, else the music volume.
    fn start_volume(&self) -> i32 {
        if self.music.in_dialog {
            DIALOG_START_VOLUME
        } else {
            self.music.max
        }
    }

    /// FUN_00477160 on music `n`: false when the file is missing or fails
    /// its check.
    fn music_file_open(&mut self, n: i32) -> bool {
        let Some(file) = self.music_data(n) else {
            return false;
        };
        self.player.stream.open(file)
    }

    /// Music `n`'s file and marks, read from the discs (the install's
    /// `music\` copies are empty until `LOADMUSIK`, which, like `delmusik`,
    /// does nothing here).
    fn music_data(&self, n: i32) -> Option<music::MusicData> {
        load_music(&self.files, n)
    }

    /// The ring's refill (`check_buffer_music` @ 0x00477590), the end of a
    /// track drawing the next one from the `startmusik` list; the minor
    /// bug fixes play every track to its end.
    fn music_refill(&mut self) -> bool {
        let files = &self.files;
        let mut load = |n: i32| load_music(files, n);
        let mut ring = self.voices.music_ring();
        let wrote = self.player.stream.refill(
            &mut ring,
            &self.player.list,
            self.player.current,
            &mut self.rand,
            &mut load,
            self.enhancements.minor_bug_fixes,
        );
        self.stream_events();
        wrote
    }

    /// The streamer's events as the engine's.
    fn stream_events(&mut self) {
        for e in std::mem::take(&mut self.player.stream.events) {
            match e {
                music::StreamEvent::Open(n) => self.event(Event::Music {
                    music: n,
                    mark: self.player.stream.current_mark,
                }),
                music::StreamEvent::Recopy(n) => {
                    log::info!("{}: rewind refused, the ring loops", music::music_name(n))
                }
                music::StreamEvent::Cursor(..) => {}
            }
        }
    }

    /// A sound of `datas_a2\apc\` decoded whole (FUN_0047a530).
    pub(super) fn load_sound(&self, name: &str) -> Option<(String, Pcm)> {
        let data = self.files.read(Folder::Apc, name)?;
        let apc = decode_apc(&data)
            .map_err(|e| log::warn!("{name}: {e}"))
            .ok()?;
        Some((
            name.to_string(),
            Pcm {
                rate: apc.sample_rate,
                channels: apc.channels,
                samples: Arc::new(apc.samples),
            },
        ))
    }

    /// The effects volume's attenuation (FUN_0047acf0).
    pub(super) fn fx_attenuation(&self) -> i32 {
        volume_to_attenuation(
            &crate::sound::attenuation_table(),
            self.music.master,
            self.fx_volume,
        )
    }

    /// The speaker's place animation hidden while it speaks
    /// (FUN_00461870: the first active slot whose id is one of its
    /// `[IdText]` names, inactive; shown again when the speaker changes and
    /// at the end).
    pub(super) fn sync_hidden(&mut self) {
        let want = self.dialog.as_ref().and_then(|d| d.hidden.clone());
        let have = self.hidden.as_ref().map(|(_, n)| n.clone());
        if want == have {
            return;
        }
        if let Some((Some(slot), _)) = self.hidden.take() {
            if let Some(s) = self.anim.slots.get_mut(slot) {
                s.active = true;
            }
        }
        if let Some(names) = want {
            let ids: Vec<i32> = names
                .iter()
                .filter_map(|n| self.names.id(defnames::PERSONNAGES, n.as_bytes()).ok())
                .map(|i| i as i32)
                .collect();
            let slot = self
                .anim
                .slots
                .iter()
                .position(|s| s.active && ids.contains(&s.id));
            if let Some(i) = slot {
                self.anim.slots[i].active = false;
            }
            self.hidden = Some((slot, names));
        }
    }

    /// A dialog over a film (`cinema=N,M`), on the video loop's new ticks
    /// only (@ 0x0040c068): its step (@ 0x00480830, the line sequencer
    /// alone, its result ignored), then its draw over the frame
    /// (@ 0x00480840). The film's end ends it.
    pub(super) fn film_dialog_step(&mut self, new_tick: bool, out: Option<&mut [u16]>) {
        let (w, h) = (self.renderer.width(), self.renderer.height());
        let Some(d) = self.film_dialog.as_mut() else {
            return;
        };
        if !new_tick {
            return;
        }
        let mut ctx = TickContext {
            files: &self.files,
            camera: &mut self.camera,
            voices: &self.voices,
            voice_volume: self.voice_volume,
            master: self.music.master,
            clock: self.clock,
            screen: (w as u32, h as u32),
        };
        d.film_step(&mut ctx);
        if let Some(out) = out {
            let mut frame = FrameContext {
                clock: self.clock,
                rand: &mut self.rand,
                font: self.font.as_ref(),
                subtitles: self.subtitles,
                screen: (w, h),
                fpu: self.camera.fpu,
            };
            d.draw(out, &mut self.zbuf, &mut frame);
            self.camera.fpu = d.fpu();
            self.renderer.fpu = d.fpu();
        }
    }

    /// The slots an animation key acts on: those of `id` (none of the
    /// keys checks the kind: a character and an object of the same index
    /// share it), first first.
    fn slots_of(&self, id: i32) -> impl Iterator<Item = usize> + '_ {
        self.anim
            .slots
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.id == id)
            .map(|(i, _)| i)
    }

    fn first_active(&self, id: i32) -> Option<usize> {
        self.slots_of(id).find(|&i| self.anim.slots[i].active)
    }
}

impl Host for Engine {
    fn read_chapter(&mut self, chapter: i32) -> Option<Vec<u8>> {
        self.files
            .read(Folder::Scenar, &format!("chapi{chapter:03}.con"))
    }

    fn load_wam(&mut self, name: &str) -> bool {
        if super::puzzles::ported_place(name).is_none() {
            self.refuse_place(name);
            return false;
        }
        self.load_place(name);
        true
    }

    fn has_view(&mut self, view: i32) -> bool {
        u32::try_from(view).is_ok_and(|v| self.wam.view(v).is_some())
    }

    fn enter_view_with_vars(&mut self, view: i32, yaw: f32, pitch: f32, vars: &mut [i32]) {
        // The interpreter is out of the engine while it runs this: its
        // variables are lent for the entry (copied back after).
        self.lent_vars = Some(vars.to_vec());
        Host::enter_view(self, view, yaw, pitch);
        if let Some(v) = self.lent_vars.take() {
            vars.copy_from_slice(&v);
        }
    }

    fn enter_view(&mut self, view: i32, yaw: f32, pitch: f32) {
        self.vars.last_vue = self.view;
        if let Err(e) = Engine::enter_view(self, view as u32, yaw, pitch) {
            log::error!("{e}");
        }
        // `vue=` ends with the per-view reset.
        self.per_view_reset();
    }

    fn turn_now(&mut self, yaw: bool, angle: f32) {
        // FUN_0045ad40 / FUN_0045ada0 set the rotation of the scene's root
        // or camera node [INFERRED: absolute, from "set rotation"].
        let (y, p) = self.camera.angles();
        if yaw {
            self.camera.enter(angle, p);
        } else {
            self.camera.enter(y, angle);
        }
    }

    fn zoom_start(&mut self, zoom: f32, fov: f32) {
        // FUN_0045b350: the next move's turn narrows the field of view.
        self.zoom.start(self.camera.fpu, zoom, fov);
    }

    fn zoom_psycho_start(&mut self, a: f32, b: f32) {
        // FUN_0045b3b0 (Shamballa's `voiles.wam`): the zoom with a roll of
        // the camera, which the port's turn does not have yet.
        log::info!("ZoomPsychoStart({a}, {b}): the roll is not ported");
    }

    fn enable_zone(&mut self, view: i32, zone: i32, on: bool) {
        // FUN_00410060: the first view numbered V of the current place that
        // has a zone of value Z; each such zone's type ± 88888.
        let current = self.view;
        let place = self
            .wam
            .places
            .iter()
            .position(|p| p.views.iter().any(|v| v.id == current))
            .unwrap_or(0);
        let Some(p) = self.wam.places.get_mut(place) else {
            return;
        };
        let Some(v) = p
            .views
            .iter_mut()
            .find(|v| v.id as i32 == view && v.zones.iter().any(|z| z.value == zone))
        else {
            return;
        };
        for z in v.zones.iter_mut().filter(|z| z.value == zone) {
            if on && z.kind >= DISABLED {
                z.kind -= DISABLED;
            } else if !on && z.kind < DISABLED {
                z.kind += DISABLED;
            }
        }
    }

    fn abort_departure(&mut self) {
        self.sprite_hit = None;
        self.move_pending = false;
    }

    fn anim(&mut self, op: AnimOp) {
        let slots = &mut self.anim.slots;
        match op {
            // Both stop at the first matching record (FUN_0048c6b0 jumps
            // to the table update on its first hit, FUN_0048c610 too).
            AnimOp::Hide { kind, id } => {
                if let Some(s) = slots.iter_mut().find(|s| s.id == id && s.kind == kind) {
                    s.active = false;
                    s.frame = 0;
                }
            }
            AnimOp::Show { kind, id, number } => {
                if let Some(s) = slots.iter_mut().find(|s| {
                    s.id == id && s.kind == kind && (number == 0 || s.state.anim as i32 == number)
                }) {
                    s.active = true;
                }
            }
            AnimOp::Play { id, number, .. } => {
                let target = self
                    .slots_of(id)
                    .find(|&i| self.anim.slots[i].state.anim as i32 == number);
                let Some(a) = self.first_active(id) else {
                    return;
                };
                let slots = &mut self.anim.slots;
                slots[a].end = slots[a].count;
                if let Some(t) = target {
                    slots[a].handing = true;
                    slots[a].chain = t as i32;
                    let s = &mut slots[t];
                    s.handing = false;
                    s.frame = 0;
                    s.end = if s.state.end <= 0 {
                        s.count
                    } else {
                        s.state.end - 1
                    };
                }
            }
            AnimOp::Continue { id, .. } => {
                if let Some(a) = self.first_active(id) {
                    let s = &mut self.anim.slots[a];
                    s.end = s.count;
                }
            }
            AnimOp::Stop { id, number } => {
                for s in slots
                    .iter_mut()
                    .filter(|s| s.id == id && s.state.anim as i32 == number)
                {
                    s.active = false;
                    s.frame = 0;
                }
            }
            AnimOp::GotoFrame { id, frame, .. } => {
                if let Some(s) = slots.iter_mut().find(|s| s.id == id) {
                    s.frame = frame;
                    if s.kind == 2 && frame > s.count {
                        log::error!("SetVariables(): GOTOFRAMEOBJET! {frame}>{}", s.count);
                    }
                }
            }
            AnimOp::AddFrame { kind, id, frames } => {
                let horse = kind == 2
                    && self
                        .names
                        .name(defnames::OBJETS, id as usize)
                        .is_some_and(|n| n == b"CHEVAL");
                if let Some(s) = self.anim.slots.iter_mut().find(|s| s.id == id) {
                    s.frame += frames;
                    if s.frame >= s.end {
                        if horse {
                            s.state.first = 0;
                            s.state.end = 0;
                            s.end = s.count;
                        } else {
                            s.frame = s.end;
                        }
                    }
                }
            }
            AnimOp::ChangeAnimGotoPerso { id, a, b } => {
                let find = |n: i32| {
                    self.slots_of(id)
                        .find(|&i| self.anim.slots[i].state.anim as i32 == n)
                };
                match (find(a), find(b)) {
                    (Some(sa), Some(sb)) => self.anim.slots[sa].chain = sb as i32,
                    _ => log::error!("AtlSpw_find_index_anum({id}, {a}, {b})"),
                }
            }
            AnimOp::TheFrame { id, on, .. } => {
                log::info!("ANIM_THE_FRAME({id}, {on}): the playing byte is not ported");
            }
        }
    }

    fn show_table(&mut self, id: i32, shown: bool, kind: i32) {
        self.anim.table.set(id, i32::from(shown), kind);
    }

    fn anim_frame(&mut self, id: i32) -> Option<i32> {
        self.anim.slots.iter().find(|s| s.id == id).map(|s| s.frame)
    }

    fn view_id(&mut self) -> i32 {
        self.view as i32
    }

    fn slot_frame(&mut self, id: i32, anim: Option<i32>) -> Option<i32> {
        self.anim
            .slots
            .iter()
            .find(|s| s.id == id && anim.is_none_or(|a| s.state.anim as i32 == a))
            .map(|s| s.frame)
    }

    fn slot_rewind(&mut self, id: i32) {
        if let Some(s) = self.anim.slots.iter_mut().find(|s| s.id == id) {
            s.frame = 0;
            s.end = s.state.end;
        }
    }

    fn voice_busy(&mut self, voice: i32) -> bool {
        usize::try_from(voice).is_ok_and(|v| self.voices.is_playing(v))
    }

    fn music_step(&mut self) {
        self.music_frame();
    }

    fn cinema_sound(&mut self, apc: &str) {
        self.pending_sound = self.load_sound(apc);
        if self.pending_sound.is_none() {
            log::info!("{apc}: no sound track");
        }
    }

    fn play_video(&mut self, hnm: &str, dialog: Option<i32>) -> bool {
        let started = self.start_video(hnm);
        // FUN_0040c030: the film, then dialog M set up over it
        // (FUN_004801a0 → FUN_0045ebc0; a failure removes it).
        if let (true, Some(m)) = (started, dialog) {
            let d = match self.dial_script.as_ref() {
                Some(script) => Dialog::initialise(
                    m as u32,
                    script,
                    &self.files,
                    &mut self.rand,
                    self.renderer.variant,
                    self.camera.fpu,
                )
                .ok(),
                None => None,
            };
            if d.is_none() {
                log::warn!("cinema: dialog {m} cannot be played over the film");
            }
            self.film_dialog = d;
        }
        started
    }

    fn dialog_running(&self) -> bool {
        self.dialog.is_some()
    }

    fn dial_stop(&mut self) {
        self.stop_dialog();
    }

    fn dial_start(&mut self, n: i32) -> DialStart {
        match self.dial_open(n as u32) {
            Ok(()) => DialStart::Started,
            Err(e) => {
                log::warn!("AtlDial_start({n}): {e}");
                DialStart::Failed
            }
        }
    }

    fn music_volume(&mut self, percent: i32) {
        self.music.set_percent(percent);
    }

    fn new_sound(&mut self, apc: &str) {
        // FUN_0047a290: only when the file opens; what voice 2 played is
        // stopped.
        if let Some((name, pcm)) = self.load_sound(apc) {
            let att = self.fx_attenuation();
            self.voices.play(VOICE_EFFECTS, pcm, false, att, 0);
            self.event(Event::Sound(name));
        }
    }

    fn traj_sound(&mut self, apc: &str) {
        self.traj_sound = Some(apc.to_string());
    }

    fn start_music(&mut self, list: &str) {
        self.player.list = list.bytes().take(0xff).collect();
        let list = self.player.list.clone();
        if let Some((e, _)) = music::select(
            &mut self.rand,
            &list,
            self.player.current,
            self.player.stream.current_mark,
        ) {
            self.play_music(e.music, e.mark, e.loops);
        }
    }

    fn stop_music(&mut self) {
        self.music.stop();
    }

    fn set_monde(&mut self, monde: i32) -> bool {
        self.event(Event::Monde(monde));
        self.monde = monde;
        self.files.set_monde(monde);
        self.objects_bank = load_objects_bank(&self.files, monde);
        self.load_subjects_bank(monde);
        self.load_world_texts(monde);
        // Every disc is read where it is: the disc asked for is there.
        true
    }

    fn bag_layout(&mut self, visible: i32) {
        self.bar.taken(visible);
    }

    fn message_clic(&mut self, text: &str) {
        log::info!("MESSAGE_CLIC={text}: the message viewer is not ported");
    }

    fn view_big_sprite(&mut self, bank: &str) {
        self.open_picture(bank);
    }

    fn credits(&mut self) {
        // FUN_004818f0 → `AtlMenu_start_Credits` (@ 0x00486e00).
        self.open_credits();
    }
}

/// Music `n`'s file and marks from the discs (`muzik%02d.apc`, `.spp`).
fn load_music(files: &atlantis2_data::A2Files, n: i32) -> Option<music::MusicData> {
    let name = music::music_name(n);
    let Some(data) = files.read(Folder::ApcMusic, &name) else {
        log::warn!("{name}: not found, no music");
        return None;
    };
    let marks = files
        .read(Folder::ApcMusic, &music::spp_name(n))
        .or_else(|| files.read(Folder::Music, &music::spp_name(n)))
        .and_then(|d| music::parse_spp(&d))
        .unwrap_or_default();
    Some(music::MusicData {
        music: n,
        data: Arc::new(data),
        marks,
    })
}
