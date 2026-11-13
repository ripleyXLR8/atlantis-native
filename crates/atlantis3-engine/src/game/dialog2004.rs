//! *Atlantis Evolution*'s 2004 program's dialogs in the game
//! (atlantis4-cd-en-3022848): the script's `DIAL=` (FUN_0044c850
//! @ 0x0044e6a2), `AtlDial_Start` (FUN_00410110), the loop mode 6 handler
//! (`AtlDial_Continue` FUN_00410600 @ 0x004a1570, or from the panorama's
//! frame for a light dialog, FUN_00476320 @ 0x00476369), the line's text at
//! each flip (`AtlDial_DisplayText` FUN_00410c40 @ 0x0041299b) and
//! `AtlDial_Close` (FUN_004111e0).
//!
//! `CDialogue` (FUN_0046cab0, 0x1a8 bytes): `Open` FUN_0046cd90,
//! `NextReplique` FUN_0046d790, `Perform` FUN_0046dfa0, `Render`
//! FUN_0046ea60, `Display` FUN_0046f630. Its data are
//! [`atlantis3_data::evolution2004_dialog`].
//!
//! The RE note is atlantis4-re `subsystems/cd2004-dialogs.md`. With the
//! speakers shown by a place animation (`[Spw]`, FUN_0046d3f0, and the
//! panorama drawn letterboxed by FUN_00444870), the head and background
//! picked from a place animation that shows the speaker (`[IdText]`:
//! FUN_0046f980's and FUN_004700d0's lookups of the animation table
//! 0x006ea3ac) and the debug fill of a head that failed to open
//! (FUN_004c2bb0 with 0x7e7eff).

use std::sync::Arc;

use atlantis3_data::bink::BinkVideo;
use atlantis3_data::evolution2004_dialog::{
    background_names, background_names_anim, head_name, is_player, parse_com_2004, parse_info_2004,
    voice_name, DialOptions, Dialogs2004,
};

use super::{stretch_video, Game, Mode};
use crate::audio::CHANNEL_VOICE;
use crate::film2004::FilmSound;
use crate::menu::PagePixel;
use crate::px::Surface;

/// The 3D dialogs' surface (W, H) of `AtlDial_Start`'s `new CDialogue`
/// (0x006acfa0, 0x006acfa4: 800 × 600); a voice-over's is the back
/// buffer's (FUN_00411a30 / FUN_00411a50).
pub const SURFACE: (usize, usize) = (800, 600);

/// The rows of the dialog surface left out of the head's blend at the top
/// and at the bottom (`0x4c * pitch`, `height − 0x98`, @ 0x0046e1f1..).
pub const LETTERBOX: usize = 0x4c;

/// A head film (`BinkOpen(path, 0x100000)` @ 0x0046d30e, the handle at
/// CDialogue +0x2c).
pub(crate) struct Head {
    pub path: String,
    video: BinkVideo<Arc<[u8]>>,
    /// The time the next frame is due ([INFERRED]: `BinkWait` paces at the
    /// file's rate, as the films' clock in [`crate::film2004`]).
    due: f64,
    /// The last frame, a8r8g8b8 (`BinkCopyToBuffer(.., 5)`,
    /// `BINKSURFACE32A`).
    pub argb: Vec<u32>,
    /// `FrameNum` of the frame drawn last (CDialogue +0x10).
    pub frame_num: u32,
}

impl Head {
    /// FUN_0046d2e0: the film opened, then `BinkGoto(BinkGetKeyFrame(rand()
    /// % Frames + 1, 2))` (@ 0x0046d36a..0x0046d384): it starts at the key
    /// frame closest to a random frame.
    fn open(path: &str, data: Vec<u8>, r: i32, now: f64) -> Option<Head> {
        let mut video = BinkVideo::open(Arc::<[u8]>::from(data))
            .map_err(|e| log::warn!("Probleme d'ouverture du Bink : {path} ({e:?})"))
            .ok()?;
        let frames = video.frame_count().max(1);
        let target = r as u32 % frames; // 0-based of `rand() % Frames + 1`
        let key = closest_key_frame(&video, target);
        video.seek_key_frame(key);
        let n = video.width() as usize * video.height() as usize;
        Some(Head {
            path: path.to_string(),
            video,
            due: now,
            argb: vec![0; n],
            frame_num: key + 1,
        })
    }

    /// `BinkWait` = 0 then `BinkDoFrame` and `BinkCopyToBuffer`; after the
    /// last frame `BinkNextFrame` goes back to the first ([INFERRED],
    /// RAD's Bink loops). Whether a frame was decoded.
    fn step(&mut self, now: f64) -> bool {
        if now < self.due {
            return false;
        }
        if self.video.frames_decoded() >= self.video.frame_count() {
            self.video.rewind();
        }
        self.frame_num = self.video.frames_decoded() + 1;
        match self.video.next_frame() {
            Some(Ok(f)) => f.to_argb8888(&mut self.argb),
            Some(Err(e)) => {
                log::warn!("{}: {e:?}", self.path);
                return false;
            }
            None => return false,
        }
        let fps = self.video.fps();
        self.due += if fps > 0.0 { 1.0 / fps } else { 1.0 / 15.0 };
        if self.due < now {
            self.due = now;
        }
        true
    }

    fn size(&self) -> (usize, usize) {
        (self.video.width() as usize, self.video.height() as usize)
    }
}

/// `BinkGetKeyFrame(frame, BINKGETKEYCLOSEST)`: the key frame nearest
/// `target` (0-based); the earlier one on a tie [INFERRED].
fn closest_key_frame<D: AsRef<[u8]>>(v: &BinkVideo<D>, target: u32) -> u32 {
    let before = (0..=target).rev().find(|&f| v.is_key_frame(f));
    let after = (target..v.frame_count()).find(|&f| v.is_key_frame(f));
    match (before, after) {
        (Some(b), Some(a)) if a - target < target - b => a,
        (Some(b), _) => b,
        (None, Some(a)) => a,
        (None, None) => 0,
    }
}

/// A line's voice: the Ogg file streamed to [`CHANNEL_VOICE`], playing
/// until its length (the last page's granule position over its rate) has
/// passed on the game's clock (FMOD's `IsPlaying`, FUN_0042ffd0, taken as
/// "not at the end yet" [INFERRED]).
pub(crate) struct Voice {
    sound: FilmSound,
    end: f64,
}

impl Voice {
    fn open(audio: &crate::audio::Audio, data: Vec<u8>, gain: f32, now: f64) -> Option<Voice> {
        let frames = crate::music2004::ogg_frames(&data);
        let rate = atlantis4_data::vorbis::VorbisStream::new(data.clone())
            .ok()?
            .rate()
            .max(1);
        let sound = FilmSound::open(audio, CHANNEL_VOICE, data, gain)?;
        Some(Voice {
            sound,
            end: now + frames as f64 / f64::from(rate),
        })
    }

    /// Streams ahead; whether it still plays at `now`.
    fn playing(&mut self, audio: &crate::audio::Audio, now: f64) -> bool {
        self.sound.pump(audio);
        now < self.end
    }

    fn stop(self, audio: &crate::audio::Audio) {
        self.sound.stop(audio);
    }
}

/// The running dialog (`AtlDial`'s globals and the `CDialogue` at
/// 0x006e6f5c).
pub(crate) struct Dialog2004 {
    /// CDialogue +0x08, +0x0c.
    pub dialog: i32,
    pub line: i32,
    /// +0x14, +0x15.
    pub voixoff: bool,
    pub light: bool,
    /// The loop mode replaced (0x006e0b3c, FUN_004a2620(6) @ 0x0041022f).
    pub back: Mode,
    /// The last speaker (0x006e0b08).
    pub speaker: String,
    /// The line's voice (`LoadSfx(path, 1, 2, 0)` @ 0x0046def3) and its
    /// path.
    pub voice: Option<Voice>,
    pub voice_path: String,
    pub head: Option<Head>,
    /// The background (0x00716c50, made black by each `CDialogue`,
    /// @ 0x0046cc35) and its file (0x00716cf8), a8r8g8b8 at the surface's
    /// size.
    pub background: Vec<u32>,
    pub background_path: String,
    /// The dialog surface (CDialogue +0x30), in the display's format.
    pub surface: Surface,
    pub size: (usize, usize),
    /// The line's speaker has `[Spw]` (FUN_004758f0): shown by a place
    /// animation, no head film.
    pub spw: bool,
}

impl Game {
    /// The 2004 dialog data, read at the first `DIAL` (`AtlDial_MainInit`
    /// FUN_0040ed20: `scenar\info.txt`, `loc\text\<SousTitre>\dial.txt`,
    /// `scenar\com.txt`).
    fn dialogs_2004(&mut self) -> &mut Dialogs2004 {
        if self.dialogs2004.is_none() {
            let read = |p: &str| self.files.read(p).unwrap_or_default();
            let info = parse_info_2004(&read("scenar/info.txt"));
            let lang = self.startup.text_language();
            let dial = atlantis3_data::dialog::parse_dial_2004(&read(&format!(
                "loc/text/{lang}/dial.txt"
            )));
            let com = parse_com_2004(&read("scenar/com.txt"));
            let d = Dialogs2004::build(info, &dial, &com);
            log::info!(
                "AtlDial_MainInit: {} persos, {} dialogs, {} issues",
                d.persos.len(),
                d.entries.len(),
                d.issues.len()
            );
            self.dialogs2004 = Some(d);
        }
        let player = self.player_2004.clone();
        let d = self.dialogs2004.as_mut().unwrap();
        d.player = player;
        d
    }

    /// `PLAYERNAME=` (@ 0x0044df59): kept when it is a player perso
    /// (FUN_00410520), then FUN_0040eca0: the name set and its index in
    /// the list (FUN_00410590) given back for `PLAYERNAME`.
    pub(crate) fn player_name_2004(&mut self, name: &str) -> Option<i32> {
        if is_player(name) {
            self.player_2004 = name.to_string();
            if let Some(d) = self.dialogs2004.as_mut() {
                d.player = name.to_string();
            }
            atlantis3_data::evolution2004_dialog::PLAYERS
                .iter()
                .position(|&p| p == name)
                .map(|i| i as i32)
        } else {
            log::warn!("Le Perso {name} n'existe pas pour les dialogues");
            None
        }
    }

    /// The script's `DIAL=` (@ 0x0044e6b7..0x0044e97e): the options, the
    /// dialog drawn (`std::random_shuffle` then `rand() % n`), then
    /// `AtlDial_Start(n, off, light, %)`.
    pub(crate) fn dial_2004(&mut self, line: &[u8]) {
        let o = DialOptions::parse(line);
        let mut rand = std::mem::replace(&mut self.rand, atlantis2_engine::crt::Rand::new(0));
        let n = o.pick(&mut || rand.rand());
        self.rand = rand;
        let Some(n) = n else {
            log::warn!("DIAL= without a dialog number");
            return;
        };
        self.start_dialog_2004(n, o.voixoff, o.light, o.percent);
    }

    /// `AtlDial_Start(n, voixoff, light, percent)` (FUN_00410110).
    pub(crate) fn start_dialog_2004(&mut self, n: i32, voixoff: bool, light: bool, percent: i32) {
        // The subtitle voice slots stopped and freed (FUN_00410b10).
        self.voices_clear();
        if self.dialog2004.is_some() {
            log::warn!("AtlDial_Start(long p_NoDial) : mode dialogue deja initialise.");
            return;
        }
        // `new CDialogue(w, h, voixoff, light)`: 800 × 600, or the back
        // buffer's size for a voice-over.
        let size = if voixoff {
            (self.width, self.height)
        } else {
            SURFACE
        };
        let depth = self.depth();
        self.dialog2004 = Some(Dialog2004 {
            dialog: n,
            line: 0,
            voixoff,
            light,
            back: self.mode,
            speaker: String::new(),
            voice: None,
            voice_path: String::new(),
            head: None,
            background: vec![0; size.0 * size.1],
            background_path: String::new(),
            surface: Surface::new(depth, size.0 * size.1),
            size,
            spw: false,
        });
        // `CDialogue::Open(n)`: the fallback of a missing dialog, line 0,
        // `NextReplique`, `Perform` (their results ignored).
        self.dialogs_2004().open(n);
        self.next_line_2004();
        self.perform_2004();
        // The music to (100 − %) % in 1 s (FUN_00430bf0 @ 0x0041020b).
        self.music2004.duck((100.0 - percent as f32) * 0.01, 1000);
        if !light {
            // FUN_004a2620(6) @ 0x0041022f: the dialog's loop mode.
            self.mode = Mode::Dialog;
        }
        // 0x006e0b08 := the line's speaker (@ 0x004103a9).
        let s = self.speaker_2004();
        if let Some(d) = self.dialog2004.as_mut() {
            d.speaker = s;
        }
        log::info!("---> Dialogue en cours : {n} (voixoff {voixoff}, light {light})");
    }

    /// The current line's speaker (`GetSpeakerName` FUN_0046f0b0, `HERO`
    /// as the player perso).
    fn speaker_2004(&mut self) -> String {
        let Some((n, l)) = self.dialog2004.as_ref().map(|d| (d.dialog, d.line)) else {
            return String::new();
        };
        let d = self.dialogs_2004();
        d.entry(n, l)
            .map(|e| d.speaker(e).to_string())
            .unwrap_or_default()
    }

    /// `CDialogue::NextReplique` (FUN_0046d790): the voice freed, the next
    /// line, its head film and background (not a voice-over), its voice
    /// loaded and played; false past the last line.
    fn next_line_2004(&mut self) -> bool {
        let Some(mut run) = self.dialog2004.take() else {
            return false;
        };
        let ok = self.next_line_in(&mut run);
        self.dialog2004 = Some(run);
        ok
    }

    fn next_line_in(&mut self, run: &mut Dialog2004) -> bool {
        // The voice of the line ending stopped and freed (FUN_0042fdd0
        // @ 0x0046d96c).
        if let Some(v) = run.voice.take() {
            v.stop(&self.audio);
        }
        run.line += 1;
        let (n, line) = (run.dialog, run.line);
        if line > self.dialogs_2004().count(n) {
            return false;
        }
        let speaker = self.speaker_2004_of(n, line);
        let perso = self.dialogs_2004().perso(&speaker);
        run.spw = perso.spw;
        if !run.voixoff {
            if perso.spw {
                // @ 0x0046de36..0x0046ded2: the stretch surface closed, the
                // head closed and its path emptied, then FUN_0046d3f0.
                run.head = None;
                self.spw_speaker_2004(&perso);
            } else {
                self.line_head_2004(run, n, line, &speaker);
            }
        } else if !run.light && perso.spw {
            // @ 0x0046ded2 from the voice-over test: a voice-over line of
            // a `[Spw]` speaker (not light) shows its animation too.
            self.spw_speaker_2004(&perso);
        }
        // The voice: `loc\voix\<Langue>\AT<dddd><ll>.ogg` (FUN_004710f0),
        // `LoadSfx(.., 1, 2, 0)` then, loaded, `PlaySfx` (@ 0x0046df1d,
        // @ 0x0046df44): FMOD channel 7, volume `ftol(ftol(G · 0.01 · V) ·
        // 2.55)` (@ 0x0042f7e6..0x0042f813, `VoicesVolume` 0x006ae75c),
        // taken as the gain `v / 255` [INFERRED, as the music's].
        let lang = self.startup.voice_language();
        let path = format!("loc/voix/{lang}/{}", voice_name(n, line));
        let c = &self.startup.config;
        let v = (f64::from(c.global_volume as f32)
            * f64::from(0.01f32)
            * f64::from(c.voices_volume)) as i32;
        let v = (f64::from(v) * f64::from(2.55f32)) as i32;
        let gain = v.clamp(0, 255) as f32 / 255.0;
        let now = self.now;
        run.voice = self
            .files
            .read(&path)
            .and_then(|data| Voice::open(&self.audio, data, gain, now));
        if run.voice.is_none() {
            log::warn!("AtlSound_LoadSfx({path}) : Not OK");
        }
        run.voice_path = path;
        true
    }

    /// The speaker of line (n, line), `HERO` as the player perso.
    fn speaker_2004_of(&mut self, n: i32, line: i32) -> String {
        let d = self.dialogs_2004();
        d.entry(n, line)
            .map(|e| d.speaker(e).to_string())
            .unwrap_or_default()
    }

    /// @ 0x0046db4a..0x0046de2e: the head number (FUN_0046d1c0), the head
    /// film `binkdial\<Nom><nnnn>.bik` (FUN_0046f980), the background
    /// (FUN_004700d0, loaded by FUN_0046d390 when it changed) and the head
    /// opened when its path changed (FUN_0046d2e0).
    fn line_head_2004(&mut self, run: &mut Dialog2004, n: i32, line: i32, speaker: &str) {
        let mut rand = std::mem::replace(&mut self.rand, atlantis2_engine::crt::Rand::new(0));
        let number = self
            .dialogs_2004()
            .head_number(n, line, &mut || rand.rand());
        self.rand = rand;
        let perso = self.dialogs_2004().perso(speaker);
        // FUN_0046f980 and FUN_004700d0: a speaker that is not the player
        // (FUN_00410520) with a shown place animation of one of its
        // `[IdText]`s takes `Nom[k]` / `Fond[k]` of that IdText and the
        // animation state's view in the background's name.
        let anim = if is_player(speaker) {
            None
        } else {
            self.shown_id_text_2004(&perso)
        };
        let k = anim.map_or(0, |(k, _)| k);
        let head = head_name(perso.nom(k), number);
        let warp = self
            .place
            .cube
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        let [bg1, bg2] = match anim {
            Some((_, made_for)) => background_names_anim(perso.fond(k), &warp, made_for, number),
            None => background_names(perso.fond(k), &warp, number),
        };
        let bg = if self.files.size(&bg1).is_some() {
            bg1
        } else {
            bg2
        };
        if bg != run.background_path {
            match self
                .files
                .read(&bg)
                .and_then(|d| atlantis4_data::jpeg::decode(&d).ok())
            {
                Some(img) => {
                    load_background(&mut run.background, run.size, &img);
                    run.background_path = bg;
                }
                None => log::warn!("Impossible de trouver le fichier {bg}"),
            }
        }
        if run.head.as_ref().is_none_or(|h| h.path != head) {
            run.head = None;
            match self.files.read(&head) {
                Some(data) => {
                    let r = self.rand.rand();
                    run.head = Head::open(&head, data, r, self.now);
                }
                None => log::warn!("Probleme d'ouverture du Bink : {head}"),
            }
        }
    }

    /// The lookup of FUN_0046f980 and FUN_004700d0: the index k of the
    /// first `[IdText]` that the hide table does not hide (FUN_004522b0)
    /// with a place-animation slot (table 0x006ea3ac) of kind 1, its
    /// `#PERSONNAGES#` id (FUN_00466f70(0, ..)) and shown (+0x28 = 1); and
    /// that slot's state `+0xc` (the view the animation was made for).
    fn shown_id_text_2004(
        &self,
        perso: &atlantis3_data::evolution2004_dialog::Perso2004,
    ) -> Option<(usize, u32)> {
        perso.id_texts.iter().enumerate().find_map(|(k, name)| {
            if self.transition_hidden(name) {
                return None;
            }
            let id = self
                .names
                .id(atlantis2_data::defnames::PERSONNAGES, name.as_bytes())
                .ok()? as i32;
            self.slots
                .slots
                .iter()
                .find(|s| s.kind == 1 && s.id == id && s.shown)
                .map(|s| (k, s.state.made_for))
        })
    }

    /// FUN_0046d3f0, a `[Spw]` speaker's line: a head number drawn from
    /// its `[Neutre]` (FUN_0046d290: `Neutre[rand() % n]`; the original
    /// divides by zero without one); the first `[IdText]` not hidden
    /// (FUN_004522b0) whose `#PERSONNAGES#` id has a slot of kind 1
    /// playing that animation number (state `+0x04`, shown or not) →
    /// FUN_00466540(id, number, 1, 1, 0): every slot of (id, 1) hidden
    /// (FUN_004663d0), the one of that number shown (FUN_004660d0), and
    /// the hide table's row of (id, 1) shown (FUN_00466460).
    fn spw_speaker_2004(&mut self, perso: &atlantis3_data::evolution2004_dialog::Perso2004) {
        if perso.neutre.is_empty() {
            log::warn!("[Spw] speaker without [Neutre]: the original divides by zero");
            return;
        }
        let r = self.rand.rand() as usize;
        let number = perso.neutre[r % perso.neutre.len()];
        let found = perso.id_texts.iter().find_map(|name| {
            if self.transition_hidden(name) {
                return None;
            }
            let id = self
                .names
                .id(atlantis2_data::defnames::PERSONNAGES, name.as_bytes())
                .ok()? as i32;
            self.slots
                .slots
                .iter()
                .any(|s| s.kind == 1 && s.id == id && s.state.anim as i32 == number)
                .then_some(id)
        });
        let Some(id) = found else {
            return;
        };
        log::debug!("[Spw] speaker: perso {id} animation {number}");
        self.slots.hide(id, 1, self.now);
        self.slots.show(id, 1, number, self.now);
        if let Some(s) = self.script.as_mut() {
            s.show_row(id, true, 1);
        }
    }

    /// Tests: the current 2004 line's speaker has `[Spw]`.
    #[doc(hidden)]
    pub fn dialog_2004_spw(&self) -> bool {
        self.dialog2004.as_ref().is_some_and(|d| d.spw)
    }

    /// `CDialogue::Perform` (FUN_0046dfa0): the voice's end goes to the
    /// next line (none: false); a voice-over draws the panorama; a 3D
    /// dialog decodes the head's frame when due and blends it over the
    /// background.
    fn perform_2004(&mut self) -> bool {
        let Some(mut run) = self.dialog2004.take() else {
            return false;
        };
        let ok = self.perform_in(&mut run);
        self.dialog2004 = Some(run);
        ok
    }

    fn perform_in(&mut self, run: &mut Dialog2004) -> bool {
        // @ 0x0046e2f7..0x0046e33d: a loaded voice that stopped →
        // `NextReplique`; no voice: a light dialog ends.
        match run.voice.as_mut() {
            Some(v) => {
                if !v.playing(&self.audio, self.now) && !self.next_line_in(run) {
                    return false;
                }
            }
            None if run.light => return false,
            None => {}
        }
        if run.light {
            return true;
        }
        if run.voixoff {
            // FUN_00444670 into the dialog surface: the panorama's frame.
            self.draw_warp_only();
            return true;
        }
        let Some(h) = run.head.as_mut() else {
            // @ 0x0046e33d..: no head. `[Spw]` → the stretch surface is
            // closed, so FUN_00411c30 is the back buffer: cleared to 0
            // (FUN_004a58d0) and the panorama drawn in rows 0x4c ..
            // h − 0x4c − 1 (FUN_00444870); `Render` then draws nothing.
            // Else the dialog surface filled with 0x7e7eff (FUN_004c2bb0, a
            // head that failed to open).
            if run.spw {
                self.draw_letterbox_2004(LETTERBOX);
            } else {
                match &mut run.surface {
                    Surface::Xrgb8888(s) => s.fill(0x007e_7eff),
                    Surface::Rgb565(s) => s.fill(<u16 as PagePixel>::from_argb(0x007e_7eff)),
                }
            }
            return true;
        };
        if !h.step(self.now) {
            return true;
        }
        let (w, hh) = run.size;
        let (bw, bh) = h.size();
        // `BinkCopyToBuffer` centred into the head surface (+0xb0).
        let mut head = vec![0u32; w * hh];
        let (x0, y0) = (w.saturating_sub(bw) / 2, hh.saturating_sub(bh) / 2);
        for y in 0..bh.min(hh) {
            let n = bw.min(w);
            head[(y0 + y) * w + x0..(y0 + y) * w + x0 + n]
                .copy_from_slice(&h.argb[y * bw..y * bw + n]);
        }
        // The background into the dialog surface (FUN_004b5120), then the
        // head blended over rows 0x4c .. H − 0x4c.
        let rows = LETTERBOX..hh.saturating_sub(LETTERBOX);
        match &mut run.surface {
            Surface::Xrgb8888(s) => {
                for (d, &b) in s.iter_mut().zip(&run.background) {
                    *d = b & 0x00ff_ffff;
                }
                for y in rows {
                    for x in 0..w {
                        blend_8888(&mut s[y * w + x], head[y * w + x]);
                    }
                }
            }
            Surface::Rgb565(s) => {
                for (d, &b) in s.iter_mut().zip(&run.background) {
                    *d = <u16 as PagePixel>::from_argb(b);
                }
                for y in rows {
                    for x in 0..w {
                        blend_565(&mut s[y * w + x], head[y * w + x]);
                    }
                }
            }
        }
        true
    }

    /// `AtlDial_Continue` (FUN_00410600); false: the caller closes the
    /// dialog.
    fn continue_2004(&mut self) -> bool {
        let Some((light, voixoff)) = self.dialog2004.as_ref().map(|d| (d.light, d.voixoff)) else {
            return false;
        };
        // @ 0x004106a7..0x004106d6: a click (0x006e9fe8, not for a light
        // dialog) goes to the next line.
        if !light && self.click {
            self.click = false;
            self.double_click = false;
            if let Some(s) = self.script.as_mut() {
                s.click = false;
                s.double_click = false;
            }
            if !self.next_line_2004() {
                return false;
            }
        }
        if !self.perform_2004() {
            return false;
        }
        if !voixoff {
            // @ 0x00410708..0x00410816: a new speaker is remembered and the
            // pass ends with one more `Perform`, without a flip.
            let s = self.speaker_2004();
            if self.dialog2004.as_ref().is_some_and(|d| d.speaker != s) {
                if let Some(d) = self.dialog2004.as_mut() {
                    d.speaker = s;
                }
                self.perform_2004();
                return true;
            }
        } else {
            let s = self.speaker_2004();
            if let Some(d) = self.dialog2004.as_mut() {
                d.speaker = s;
            }
        }
        if !light {
            self.render_2004();
            self.flip_texts();
        }
        true
    }

    /// `CDialogue::Render` (FUN_0046ea60) into the stretch surface (3D
    /// dialog) or the back buffer (voice-over), then the flip's stretch.
    fn render_2004(&mut self) {
        let Some(run) = self.dialog2004.as_ref() else {
            return;
        };
        if run.voixoff || (run.spw && run.head.is_none()) {
            // The panorama already is on the screen (Perform's draw);
            // `Render` (FUN_0046ea60) draws nothing for a `[Spw]` speaker
            // without a head.
            return;
        }
        let (w, h) = run.size;
        let (sw, sh) = (self.width, self.height);
        match (&run.surface, &mut self.screen) {
            (Surface::Rgb565(s), Surface::Rgb565(o)) => stretch_video(s, w, h, o, sw, sh),
            (Surface::Xrgb8888(s), Surface::Xrgb8888(o)) => stretch_video(s, w, h, o, sw, sh),
            _ => {}
        }
    }

    /// `AtlDial_DisplayText` at a flip (@ 0x00412980..0x0041299b, when
    /// `SubTitleOn` and a dialog runs): `CDialogue::Display`
    /// (FUN_0046f630), the line's text in `AtlFont_Default`, black at
    /// layout (9, 11, 21, 19) then white at (10, 10, 20, 20), both with the
    /// arguments (4, 1) (@ 0x0046f663, @ 0x0046f7ce). The 2004 program
    /// draws with GDI's `DrawTextA` (FUN_004213a0, FUN_004211d0; the last
    /// argument 1 aligns to the bottom, @ 0x0042127a; the 4 is
    /// [`crate::text::HALIGN_2004`]: left, right-to-left for a Hebrew or
    /// Arabic font): *Atlantis III*'s dialog layouts with that code.
    pub(crate) fn dialog_text_2004(&mut self) {
        if !self.startup.config.subtitle_on {
            return;
        }
        let Some((n, l)) = self.dialog2004.as_ref().map(|d| (d.dialog, d.line)) else {
            return;
        };
        let Some(text) = self.dialogs_2004().entry(n, l).map(|e| e.text.clone()) else {
            return;
        };
        let (w, h) = (self.width, self.height);
        self.texts();
        let t = self.texts.as_ref().unwrap();
        crate::with_px!(&mut self.screen, s => t.print_shadowed(
            s,
            w,
            h,
            &text,
            crate::text::Zone {
                halign: crate::text::HALIGN_2004,
                ..crate::text::DIALOG_SHADOW
            },
            crate::text::Zone {
                halign: crate::text::HALIGN_2004,
                ..crate::text::DIALOG_TEXT
            },
            0xffffff,
        ));
    }

    /// `AtlDial_Close` (FUN_004111e0): the loop mode back (not light), the
    /// music back to full in 1 s (FUN_00430bf0(1.0)), the dialog freed
    /// (its head closed), then `Interp_CloseInterrupt` (FUN_00452ef0(0)):
    /// the waiting section goes on.
    pub(crate) fn close_dialog_2004(&mut self) {
        let Some(run) = self.dialog2004.take() else {
            return;
        };
        if let Some(v) = run.voice {
            v.stop(&self.audio);
        }
        if !run.light {
            self.mode = run.back;
        }
        self.music2004.duck(1.0, 1000);
        log::info!("AtlDial_Close ({})", run.dialog);
        self.with_script(|s, h| s.close_interrupt(h));
    }

    /// One pass of loop mode 6 (@ 0x004a1570): `AtlDial_Continue`, closed
    /// when it returns 0.
    pub(crate) fn dialog_frame_2004(&mut self) {
        if !self.continue_2004() {
            self.close_dialog_2004();
        }
    }

    /// A light dialog's pass in the panorama's frame (FUN_00476320
    /// @ 0x00476369, called @ 0x00443db6 by the loop mode 3 handler).
    pub(crate) fn light_dialog_2004(&mut self) {
        if self.dialog2004.as_ref().is_some_and(|d| d.light) {
            self.dialog_frame_2004();
        }
    }

    /// Tests: the dialog running (dialog, line, voice-over, light).
    #[doc(hidden)]
    pub fn dialog_2004(&self) -> Option<(i32, i32, bool, bool)> {
        self.dialog2004
            .as_ref()
            .map(|d| (d.dialog, d.line, d.voixoff, d.light))
    }

    /// Tests: the line's voice path and whether it plays, its head film
    /// and `FrameNum`, its background and its text.
    #[doc(hidden)]
    pub fn dialog_2004_media(&mut self) -> Option<DialogMedia2004> {
        let (n, l) = self.dialog2004.as_ref().map(|d| (d.dialog, d.line))?;
        let text = self.dialogs_2004().entry(n, l).map(|e| e.text.clone());
        let d = self.dialog2004.as_ref()?;
        Some(DialogMedia2004 {
            voice: d.voice_path.clone(),
            voice_playing: d.voice.is_some(),
            head: d.head.as_ref().map(|h| (h.path.clone(), h.frame_num)),
            background: d.background_path.clone(),
            text: text.unwrap_or_default(),
        })
    }

    /// Tests: the player perso.
    #[doc(hidden)]
    pub fn player_2004(&self) -> &str {
        &self.player_2004
    }
}

/// What a 2004 dialog line plays (tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogMedia2004 {
    pub voice: String,
    pub voice_playing: bool,
    pub head: Option<(String, u32)>,
    pub background: String,
    pub text: Vec<u8>,
}

/// The background picture into its surface (FUN_00453df0): rows centred
/// vertically (`(H − h) / 2` when the surface is taller, @ 0x00453ea0..),
/// from x = 0; the rest kept.
pub(super) fn load_background(
    bg: &mut [u32],
    (w, h): (usize, usize),
    img: &atlantis4_data::jpeg::Decoded,
) {
    let (iw, ih, c) = (
        img.width as usize,
        img.height as usize,
        img.channels as usize,
    );
    if c < 3 {
        return;
    }
    let top = h.saturating_sub(ih) / 2;
    for y in 0..ih.min(h.saturating_sub(top)) {
        for x in 0..iw.min(w) {
            let p = &img.pixels[(y * iw + x) * c..];
            bg[(top + y) * w + x] =
                0xff00_0000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
        }
    }
}

/// The 32-bit blend (FUN_00666b07, MMX): per byte `(s · a + d · (256 −
/// a)) >> 8` with the destination's alpha byte cleared first (mask
/// 0x00ffffff00ffffff at 0x006dba68), `a` the source's alpha.
pub fn blend_8888(d: &mut u32, s: u32) {
    let a = s >> 24;
    let mut out = 0;
    for sh in [0, 8, 16, 24] {
        let sc = (s >> sh) & 0xff;
        let dc = if sh == 24 { 0 } else { (*d >> sh) & 0xff };
        let v = ((sc * a + dc * (256 - a)) >> 8).min(255);
        out |= v << sh;
    }
    *d = out;
}

/// The 16-bit blend (@ 0x0046e2a7..0x0046e33d, RGB565): nothing for alpha
/// 0; else, with `i = 255 − a` and the source bytes B, G, R:
/// `((d.g · i) >> 3) + ((G · a) >> 5)) & 0x7e0`, `(((d.b · i) >> 8) +
/// ((B · a) >> 11)) & 0x1f`, `(a · R + d.r · i · 8) & 0xf800`.
pub fn blend_565(d: &mut u16, s: u32) {
    let a = s >> 24;
    if a == 0 {
        return;
    }
    let i = 255 - a;
    let (b, g, r) = (s & 0xff, (s >> 8) & 0xff, (s >> 16) & 0xff);
    let dv = u32::from(*d);
    let gg = ((((dv & 0x7e0) >> 5) * i) >> 3).wrapping_add((g * a) >> 5) & 0x7e0;
    let bb = ((((dv & 0x1f) * i) >> 8) as u8).wrapping_add(((b * a) >> 11) as u8) as u32 & 0x1f;
    let rr = (a * r + (dv >> 11) * i * 8) & 0xf800;
    *d = (gg + bb + rr) as u16;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends() {
        // Opaque source: the source (alpha byte a·a >> 8).
        // (s · 255 + d) >> 8 per byte.
        let mut d = 0x0012_3456;
        blend_8888(&mut d, 0xff80_4020);
        assert_eq!(d, 0xfe7f_3f20);
        // Transparent source: the destination, alpha cleared.
        let mut d = 0xff12_3456;
        blend_8888(&mut d, 0x00ff_ffff);
        assert_eq!(d, 0x0012_3456);
        // Half: (s·128 + d·128) >> 8.
        let mut d = 0x0000_0000;
        blend_8888(&mut d, 0x80ff_ffff);
        assert_eq!(d & 0x00ff_ffff, 0x007f_7f7f);
        // 565: alpha 0 keeps; alpha 255 takes the source's top bits.
        let mut d = 0x1234;
        blend_565(&mut d, 0x00ff_ffff);
        assert_eq!(d, 0x1234);
        let mut d = 0;
        blend_565(&mut d, 0xffff_ffff);
        assert_eq!(d, 0xffff);
    }
}
