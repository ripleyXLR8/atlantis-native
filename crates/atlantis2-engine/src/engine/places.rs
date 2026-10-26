//! The place sounds as the engine runs them ([`crate::place_sound`]): the
//! view entry, the panorama frame, a video's frames and its end, a place
//! file loaded.

use super::{Engine, Event};
use crate::place_sound::{self, Entry, Slot, FIRST_VOICE, SLOTS, TABLE};
use crate::sound::Pcm;

impl Engine {
    /// FUN_0047afc0 (from `AtlWam_Load`): every slot's sounds stopped and
    /// freed, the slots free (their timers kept); the new file's sources.
    pub(super) fn place_sounds_reset(&mut self) {
        for k in 0..SLOTS {
            for i in 0..4 {
                let id = self.place.slots[k].sounds[i];
                if id != -1 {
                    self.voices.stop(FIRST_VOICE + k);
                    self.place.slots[k].voice_on = false;
                    self.place_free(id);
                    self.place.slots[k].sounds[i] = -1;
                }
            }
            self.place.slots[k].playing = -1;
            self.place.slots[k].source = None;
        }
        self.place.sources = place_sound::sources(&self.wam);
        self.place.gains = vec![(0, 0); self.place.sources.len()];
    }

    /// FUN_0047ad30 (from the view entry): the listener moves to the
    /// view's position; each source the view hears and no slot holds
    /// takes the first free slot, its files loaded (none free: the rest
    /// is skipped, as the original's warning says).
    pub(super) fn place_sounds_enter(&mut self) {
        let p = &mut self.place;
        p.listener = p.target;
        p.target = [0.0; 3];
        let Some(place) = p.listeners.place(&self.wam_name) else {
            return;
        };
        if let Some(at) = p.listeners.position(place, self.view) {
            p.target = at;
        }
        let heard: Vec<(u32, [f32; 3])> = self
            .current_view()
            .map(|v| {
                v.sounds
                    .iter()
                    .map(|b| (b.ambient_id, b.position))
                    .collect()
            })
            .unwrap_or_default();
        for (id, q) in heard {
            let Some(source) = self.place.source_of(id) else {
                continue;
            };
            if self.place.slots.iter().any(|s| s.source == Some(source)) {
                continue;
            }
            let Some(k) = self.place.slots.iter().position(|s| s.source.is_none()) else {
                log::warn!("AtlSound_InitBruitagesWarp vue {}: no free slot", self.view);
                return;
            };
            let files = self.place.sources[source].files.clone();
            for (i, name) in files.iter().enumerate() {
                let id = self.place_load(name, source);
                self.place.slots[k].sounds[i] = id;
                if let Some(Some(e)) = usize::try_from(id)
                    .ok()
                    .and_then(|i| self.place.table.get_mut(i))
                {
                    e.looping = false;
                }
            }
            let s = &mut self.place.slots[k];
            s.source = Some(source);
            s.q = q;
            s.timer = 0;
        }
    }

    /// FUN_0047a530 for a place sound: the sound table's load
    /// ([`Engine::table_load`]) with the source's position.
    fn place_load(&mut self, name: &str, owner: usize) -> i32 {
        self.table_load(name, Some(owner))
    }

    /// FUN_0047a530(name, position), the one sound table of 50 entries
    /// every sound but the music and the dialog voices goes through (the
    /// place sounds, `NEWSOUND`, the films' and moves' tracks, the
    /// puzzles'): the entry of that name if one is loaded (exact bytes;
    /// its position stays the first loader's), else the first free entry
    /// (@ 0x0047a610; none: `find_free_sound: Sound overflow`, a fatal
    /// error, and entry 0) with the file decoded into it (@ 0x0047aad0,
    /// which first empties the entry); −1 when the file cannot be read.
    pub(crate) fn table_load(&mut self, name: &str, owner: Option<usize>) -> i32 {
        if let Some(i) = self.table_find(name) {
            return i;
        }
        let free = match self.place.table.iter().position(Option::is_none) {
            Some(i) => i,
            None => {
                self.original_fatal("find_free_sound: Sound overflow");
                0
            }
        };
        self.place.table[free] = None;
        self.place_pcm.remove(&free);
        let Some((_, pcm)) = self.load_sound(name) else {
            if owner.is_none() {
                log::warn!("### ERREUR lecture impossible fichier : {name}");
            }
            return -1;
        };
        self.place.table[free] = Some(Entry {
            name: name.to_string(),
            owner,
            looping: false,
        });
        self.place_pcm.insert(free, pcm);
        free as i32
    }

    /// The sound table's loaded entry of that name (exact bytes), if any.
    pub(crate) fn table_find(&self, name: &str) -> Option<i32> {
        self.place
            .table
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| e.name == name))
            .map(|i| i as i32)
    }

    /// FUN_0047aa30 on a place sound.
    fn place_free(&mut self, id: i32) {
        if let Ok(i) = usize::try_from(id) {
            if i < TABLE {
                self.place.table[i] = None;
                self.place_pcm.remove(&i);
            }
        }
    }

    /// FUN_0047b390 (each panorama frame that is not turning, after the
    /// click scan): each slot's voice levels from the view's listener, then
    /// its scheduler.
    pub(super) fn place_sounds_frame(&mut self) {
        let listener = self.place.target;
        for k in 0..SLOTS {
            self.place_levels(k, listener, None);
            self.place_schedule(k);
        }
    }

    /// FUN_0047b030 (`AtlHnm_start`): the listener's steps over the
    /// video's frames.
    pub(super) fn place_sounds_video_start(&mut self, frames: u32) {
        let f = self.camera.fpu;
        let p = &mut self.place;
        p.count = 0.0;
        let n = frames as f32;
        for i in 0..3 {
            p.step[i] = f.f32(f.div(
                f.sub(f64::from(p.target[i]), f64::from(p.listener[i])),
                f64::from(n),
            ));
        }
        p.frames = n;
    }

    /// FUN_0047b090 (each frame of a video that goes on): the levels from
    /// the moving listener, the sources the new view does not hear fading
    /// out; the schedulers; then the listener one step on.
    pub(super) fn place_sounds_video_frame(&mut self) {
        let listener = self.place.listener;
        for k in 0..SLOTS {
            let fade = self.place.slots[k].source.map(|src| {
                let heard = self
                    .current_view()
                    .is_some_and(|v| self.place.heard_in(v, src));
                if heard {
                    2.0
                } else {
                    place_sound::transition_factor(
                        self.camera.fpu,
                        f64::from(self.place.frames),
                        f64::from(self.place.count),
                    )
                }
            });
            self.place_levels(k, listener, Some(fade.unwrap_or(2.0)));
            self.place_schedule(k);
        }
        let f = self.camera.fpu;
        let p = &mut self.place;
        for i in 0..3 {
            p.listener[i] = f.f32(f.add(f64::from(p.listener[i]), f64::from(p.step[i])));
        }
        p.count = f.f32(f.add(f64::from(p.count), 1.0));
    }

    /// FUN_0047b280 (the end of a video, or a transition that could not
    /// start): the slots of sources the view does not hear stopped and
    /// freed, their sounds freed unless another slot uses them.
    pub(super) fn place_sounds_after_video(&mut self) {
        for k in 0..SLOTS {
            let Some(src) = self.place.slots[k].source else {
                continue;
            };
            let heard = self
                .current_view()
                .is_some_and(|v| self.place.heard_in(v, src));
            if heard {
                continue;
            }
            for i in 0..4 {
                let id = self.place.slots[k].sounds[i];
                if id != -1 {
                    self.voices.stop(FIRST_VOICE + k);
                    self.place.slots[k].voice_on = false;
                    if !self.place.shared(k, id) {
                        self.place_free(id);
                    }
                    self.place.slots[k].sounds[i] = -1;
                }
            }
            self.place.slots[k] = Slot {
                sounds: self.place.slots[k].sounds,
                ..Slot::default()
            };
        }
    }

    /// One slot's levels (FUN_0047b390 / FUN_0047b090's loop body): a
    /// started voice that ended is noted; one that plays gets the ears'
    /// gains of its source and the volume and pan from them.
    fn place_levels(&mut self, k: usize, listener: [f32; 3], fade: Option<f64>) {
        let voice = FIRST_VOICE + k;
        if !self.place.slots[k].voice_on {
            return;
        }
        if !self.voices.is_playing(voice) {
            self.place.slots[k].voice_on = false;
            self.place.slots[k].playing = -1;
            return;
        }
        let Some(src) = self.place.slots[k].source else {
            return;
        };
        let (yaw, pitch) = self.camera.angles();
        let s = &self.place.sources[src];
        let (g0, g1, _) = place_sound::gains(
            self.camera.fpu,
            s.pos,
            s.level,
            listener,
            self.place.slots[k].q,
            yaw,
            pitch,
        );
        self.place.gains[src] = (g0, g1);
        let (att, pan) = place_sound::pan_volume(
            self.camera.fpu,
            &crate::sound::attenuation_table(),
            self.music.master,
            self.fx_volume,
            g0,
            g1,
            fade,
            true,
        );
        self.voices.set_attenuation(voice, att);
        self.voices.set_pan(voice, pan);
    }

    /// FUN_0047b4d0 on slot `k`: a held, silent slot whose timer is due
    /// draws a file and starts it silent (looping when the source's first
    /// probability is 100), or waits; a timer past is due again.
    fn place_schedule(&mut self, k: usize) {
        let slot = &self.place.slots[k];
        let Some(src) = slot.source else {
            return;
        };
        if slot.voice_on {
            return;
        }
        let tick = self.ticks as i32;
        if slot.timer != 0 {
            if slot.timer < tick {
                self.place.slots[k].timer = 0;
            }
            return;
        }
        let probs = self.place.sources[src].probs;
        let loaded = slot.sounds.map(|id| id != -1);
        let (pick, timer) = place_sound::draw(&mut self.rand, probs, loaded, tick);
        self.place.slots[k].timer = timer;
        let Some(i) = pick else {
            return;
        };
        let id = self.place.slots[k].sounds[i];
        self.place.slots[k].playing = i as i32;
        let Some(Some(entry)) = self.place.table.get(id as usize).cloned() else {
            return;
        };
        let mut looping = entry.looping;
        if probs[0] == 100 {
            if i != 0 {
                log::warn!("tst start new bruitage (100%) : not the first file");
            }
            looping = true;
            if let Some(Some(e)) = self.place.table.get_mut(id as usize) {
                e.looping = true;
            }
        }
        // The position's gains cleared: the sound starts silent
        // (FUN_0047a650 from those gains); an entry with no position plays
        // at the effects volume, centred.
        let (att, pan) = match entry.owner {
            Some(owner) => {
                self.place.gains[owner] = (0, 0);
                let (g0, g1) = self.place.gains[owner];
                place_sound::pan_volume(
                    self.camera.fpu,
                    &crate::sound::attenuation_table(),
                    self.music.master,
                    self.fx_volume,
                    g0,
                    g1,
                    None,
                    false,
                )
            }
            None => (self.fx_attenuation(), 0),
        };
        let Some(pcm) = self.place_pcm.get(&(id as usize)).cloned() else {
            return;
        };
        let voice = FIRST_VOICE + k;
        self.voices.play(voice, pcm, looping, att, pan);
        self.place.slots[k].voice_on = true;
        self.event(Event::PlaceSound {
            slot: k,
            name: entry.name,
        });
    }
}

/// The decoded place sounds, by table entry.
pub(super) type PlacePcm = std::collections::HashMap<usize, Pcm>;
