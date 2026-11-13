//! *Atlantis Evolution*'s 2004 program's films in the game (atlantis4-cd-en-3022848):
//! the script's `CINEMA=` (FUN_0044c850 @ 0x0044ccc6), FUN_00402a70 and
//! `AtlBink_Start` (FUN_00401d30), the loop mode 5 handler
//! (`AtlMain_loop_BINK` @ 0x004a1560: FUN_00402fb0 while it returns 1, then
//! FUN_00403340) and the film's close (FUN_00403230).
//!
//! With the travel films (`traj<n>` and the moves' `binktraj\` films,
//! the move's transition sprites drawn over each frame: FUN_00435c80,
//! FUN_004359b0, FUN_004360f0), the cine-in and cine-out letterbox
//! animations (FUN_00444870 over the panorama), `TESTTIMERVIDEO`
//! (FUN_00450e60) and the inventory's and subject menu's close
//! (@ 0x00402ab6..0x00402ade).
//!
//! The RE note is atlantis4-re `subsystems/cd2004-films.md`.

use std::sync::Arc;

use atlantis3_data::cine_txt::{Caption, CueFile};
use atlantis3_data::evolution2004_film::{txt_name, FilmFiles, FilmOptions};
use atlantis3_data::spr::{Bank, Pixel};

use super::{stretch_video, Game, Mode, Subtitle};
use crate::audio::CHANNEL_CINEMA;
use crate::caption::{CaptionFade, CaptionOp};
use crate::film2004::{sound_gain, Film, FilmSound};
use crate::px::Surface;

/// The cine-in's and cine-out's band at full progress, in screen pixels
/// (76.0 at 0x00699344 = (600 − 448) / 2).
pub(crate) const CINE_BAND: f32 = 76.0;

/// A film playing (the globals 0x006e01fc.. of `AtlBink`).
pub(crate) struct FilmPlaying {
    pub film: Film,
    /// The loop mode `AtlBink` replaced (0x006e018c, FUN_004a2620(5)
    /// @ 0x00402ae6).
    pub back: Mode,
    /// The cue lines of `bink\<name>.txt` (the list 0x006e020c): the line,
    /// shown (+0x2d), done (+0x2c).
    pub cues: Vec<(atlantis3_data::cine_txt::Cue, bool, bool)>,
    /// The caption of `loc\text\<SousTitre>\<name>.txt` (FUN_00401120).
    pub caption: Option<CaptionFade>,
    /// A travel film (0x006e01e0): no caption, no cine-out, the move's
    /// transition sprites drawn over its frames.
    pub traj: bool,
    /// The transition sprites (the list 0x006ea34c..0x006ea350 that
    /// `AtlSpr_InitSpf` FUN_004359b0 / FUN_00435c80 fills).
    pub spf: Vec<Bank>,
}

/// The cine-in or cine-out running (0x006e01f0 / 0x006e01f4) and its
/// progress (0x006e01f8, 0 .. 1).
pub(crate) struct Cine {
    pub out: bool,
    pub p: f32,
    /// The time of the previous pass ([`Game::cine_step`]).
    pub last: f64,
    /// The cine-in's film, kept for its start (0x006e0168, 0x006e01b0,
    /// 0x006e01a4): file, cue name, fps.
    pub start: Option<(String, String, i32)>,
    /// The loop mode to go back to (0x006e018c).
    pub back: Mode,
    /// The cine-out's film stopped by the player (0x006e01e3).
    pub stopped: bool,
}

impl Game {
    /// The 2004 `CINEMA=` line from its `=` (@ 0x0044ccc6..0x0044d6c8):
    /// the options read, the `%` stored (0x006ac080, whether the film
    /// starts or not), the name looked up in `#CINEMATIQUES#`; a travel
    /// film's path `binktraj\<file>` (FUN_00418740 @ 0x0044d539) and its
    /// transition sprites (FUN_00435c80(current view, traj)
    /// @ 0x0044d5b8); the cine-in and cine-out wanted (@ 0x0044d5c0..
    /// 0x0044d5e9); then FUN_00402a70(file, txt, fps, traj ≠ 0, 0).
    /// Whether the film (or its cine-in) started.
    pub(crate) fn cinema_2004(&mut self, line: &[u8]) -> bool {
        let o = FilmOptions::parse(line);
        self.film_percent = o.dial_percent();
        let Some(file) = self.cinematiques.file_of(o.name()) else {
            log::warn!(
                "DefName_GetIdName : Nom de #CINEMATIQUES# inconnu:{}",
                String::from_utf8_lossy(&o.text)
            );
            self.not_ported("a CINEMA= name not in #CINEMATIQUES#");
            return false;
        };
        let file = String::from_utf8_lossy(file).into_owned();
        let traj = o.traj != 0;
        let (path, spf) = if traj {
            let spf = match (self.place.view, u32::try_from(o.traj)) {
                (Some(view), Ok(to)) => self.transition_banks_2004(view, to),
                _ => Vec::new(),
            };
            (format!("binktraj/{file}"), spf)
        } else {
            (file.clone(), Vec::new())
        };
        self.film_cine_in = !o.nocinein;
        self.film_cine_out = o.cine_out();
        self.film_begin_2004(&path, &txt_name(&file), o.fps, traj, false, spf)
    }

    /// `AtlSpr_InitSpf` (FUN_004359b0 for a move @ 0x00443fa9,
    /// FUN_00435c80 for `CINEMA=..,traj<n>`): the destination `to` of view
    /// `from` (its `+0x24` list), and per sprite name of it (`+0x2c`)
    /// that the hide table does not hide (FUN_004522b0: the name's
    /// `#PERSONNAGES#` or `#OBJETS#` id, FUN_004664f0 < 1), the WAM
    /// entity of that name (FUN_004695b0, `strcmp`), `%s%03d%03d.spr`
    /// (0x006aeaa8) of its prefix, `from` and `to`, under `spf\`
    /// (FUN_00418f10, the current disc's).
    pub(crate) fn transition_banks_2004(&self, from: u32, to: u32) -> Vec<Bank> {
        let Some(d) = crate::travel::destination(&self.place.wam, from, to) else {
            return Vec::new();
        };
        d.sprites
            .iter()
            .filter(|name| !self.transition_hidden(name))
            .filter_map(|name| {
                let e = self
                    .place
                    .wam
                    .entities
                    .iter()
                    .find(|e| e.con_name == **name)?;
                let path = format!("spf/{}{from:03}{to:03}.spr", e.prefix);
                let bank = self
                    .files
                    .read(&path)
                    .and_then(|d| Bank::parse(&d, self.screen.depth()).ok());
                if bank.is_none() {
                    log::warn!("AtlSpr_InitSpf : pb lors du chargement de {path}");
                }
                bank
            })
            .collect()
    }

    /// FUN_00402a70: 0x006e01e2 := 1, 0x006e9e70, 0x006e9fe8, 0x006e9feb
    /// := 0; loop mode 5 already → refused; the inventory (FUN_00428870 →
    /// FUN_00428750) and the subject menu (FUN_00428930 → FUN_004288e0)
    /// closed; mode 5, the old one kept. No cine-in wanted (0x006e01e8)
    /// or a travel film → `AtlBink_Start` at once; else the film's test,
    /// its arguments kept and the cine-in started: the film starts at its
    /// end ([`Game::cine_step`]).
    pub(crate) fn film_begin_2004(
        &mut self,
        file: &str,
        txt: &str,
        fps: i32,
        traj: bool,
        loc: bool,
        spf: Vec<Bank>,
    ) -> bool {
        self.film_skip_key = false;
        self.click = false;
        self.double_click = false;
        if self.film.is_some() || self.film_cine.is_some() {
            log::warn!("AtlBink_start({file}): deja en AtlMain_loop_BINK!!");
            return false;
        }
        // FUN_00428750 (@ 0x00402abf) and FUN_004288e0 (@ 0x00402ad7):
        // the mode back first.
        if self.inventory.open {
            self.close_inventory();
        }
        if self.mode == Mode::Subjects {
            self.close_subjects();
        }
        let back = self.mode;
        if !self.film_cine_in || traj {
            return self.atl_bink_start(file, txt, fps, traj, loc, spf, back);
        }
        // @ 0x00402b05..0x00402be3: the cue's and the film's test as
        // `AtlBink_Start`'s.
        let langue = self.startup.voice_language();
        let sous_titre = self.startup.text_language();
        let files = &self.files;
        let paths = FilmFiles::resolve_bink(file, txt, &langue, &sous_titre, false, false, |p| {
            files.size(p).is_some()
        });
        if self.files.size(&paths.video).is_none() {
            log::warn!("AtlBink_start({}): cinematique introuvable", paths.video);
            self.film_cine_in = false;
            return false;
        }
        self.film_cine = Some(Cine {
            out: false,
            p: 0.0,
            last: self.now,
            start: Some((file.to_string(), txt.to_string(), fps)),
            back,
            stopped: false,
        });
        self.mode = Mode::Video;
        true
    }

    /// The start-up's logos: FUN_00402a70(name, "", 25, 0, 1)
    /// (FUN_0045e970), `loc` taking the film from `loc\bink\<Langue>\`.
    pub(super) fn film_start_2004(&mut self, file: &str, fps: i32, loc: bool) -> bool {
        self.film_begin_2004(file, &txt_name(file), fps, false, loc, Vec::new())
    }

    /// `AtlBink_Start(file, txt, fps, traj, loc)` (FUN_00401d30): `loc`,
    /// the fifth argument (1 for the start-up's logos, FUN_0045e970),
    /// takes the film from `loc\bink\<Langue>\` even without a cue file
    /// (§3 of the note); `traj` takes `file` as the film's path.
    #[allow(clippy::too_many_arguments)]
    fn atl_bink_start(
        &mut self,
        file: &str,
        txt: &str,
        fps: i32,
        traj: bool,
        loc: bool,
        spf: Vec<Bank>,
        back: Mode,
    ) -> bool {
        let langue = self.startup.voice_language();
        let sous_titre = self.startup.text_language();
        let files = &self.files;
        let paths = FilmFiles::resolve_bink(file, txt, &langue, &sous_titre, traj, loc, |p| {
            files.size(p).is_some()
        });
        // AtlBink_LoadUnderTitleFile (FUN_00401590), the cue file.
        let cues = paths
            .cue
            .as_deref()
            .and_then(|p| self.files.read(p))
            .map(|d| CueFile::parse(&d).cues)
            .unwrap_or_default();
        // The film's test (@ 0x0040235d): `cinematique introuvable`, the
        // mode back, 0.
        let Some(data) = self.files.read(&paths.video) else {
            log::warn!("AtlBink_start({}): cinematique introuvable", paths.video);
            return false;
        };
        // @ 0x004023df: only 15 and 25 (`frame rate incorrect`). The rate
        // goes to `BinkSetFrameRate(fps, 1)`, which the open with flags 0
        // (@ 0x0040247e) does not use [INFERRED, RAD's API]: the film plays
        // at its file's rate.
        if fps != 15 && fps != 25 {
            log::warn!("AtlBink_start({file}): frame rate incorrect (p_Fps={fps}).");
            return false;
        }
        let name = paths.video.rsplit('/').next().unwrap_or(file);
        let film = match Film::open(name, Arc::from(data), self.depth(), self.now) {
            Ok(f) => f,
            Err(e) => {
                if traj {
                    log::warn!(
                        "AtlBink_start - Trajectoire non ouverte : {} ({e:?})",
                        paths.video
                    );
                } else {
                    log::warn!(
                        "AtlBink_start - Cinematique non ouverte : {} ({e:?})",
                        paths.video
                    );
                }
                return false;
            }
        };
        // The caption (FUN_00401120 @ 0x0040232e), not for a travel film.
        let caption = if traj {
            None
        } else {
            self.files
                .read(&paths.caption)
                .and_then(|d| Caption::parse(&d))
                .map(CaptionFade::new_2004)
        };
        // The music's share (@ 0x0040267f): over 0, the music faded to
        // (100 − x) % in 1 s (FUN_00430c50).
        if self.film_percent > 0.0 {
            self.music2004
                .duck((100.0 - self.film_percent) * 0.01, 1000);
        }
        // The place animations' sounds stopped (FUN_00439dc0 @ 0x004026a2).
        self.anim_sounds_stop_2004();
        // The sound track (@ 0x004026a7..0x004028c1): a previous film's
        // stream stopped and closed first, then the new one played.
        if let Some(s) = self.film_sound.take() {
            s.stop(&self.audio);
        }
        if let Some(path) = &paths.sound {
            let gain = sound_gain(
                self.startup.config.global_volume as f32,
                self.startup.config.music_volume as f32,
            );
            match self.files.read(path) {
                Some(d) => {
                    self.film_sound = FilmSound::open(&self.audio, CHANNEL_CINEMA, d, gain);
                    if self.film_sound.is_none() {
                        log::warn!("AtlBink_PlayMusic({path}) : Stream_Open() Not OK");
                    }
                }
                None => log::warn!("AtlBink_PlayMusic({path}) : Stream_Open() Not OK"),
            }
        }
        // @ 0x004028cd..0x004028e9: `NUMFRAMEVIDEO` := 0 (ebp, 0 since
        // @ 0x00401d58), at the film's start: after a cine-in, and for the
        // travel films too. A `CINEMA=` without cine-in calls this from the
        // script (out of the game then): written when it is back.
        if self.script.is_some() {
            self.set_var("NUMFRAMEVIDEO", 0);
        } else {
            self.numframevideo_reset = true;
        }
        // @ 0x004028f0..0x00402902: the subtitle cleared, the clicks
        // cleared.
        self.subtitle = None;
        self.click = false;
        self.double_click = false;
        log::info!("AtlBink_start({}) {:?}", paths.video, film.size());
        self.film = Some(FilmPlaying {
            film,
            back,
            cues: cues.into_iter().map(|c| (c, false, false)).collect(),
            caption,
            traj,
            spf,
        });
        self.mode = Mode::Video;
        true
    }

    /// One pass of loop mode 5 (FUN_00402fb0): the cine-in or cine-out
    /// when one runs, else the film's frame.
    pub(crate) fn film_pass_2004(&mut self) {
        if self.film_cine.is_some() {
            self.cine_step();
        } else {
            self.film_step();
        }
    }

    /// FUN_00402fb0's cine-in (0x006e01f0) and cine-out (0x006e01f4)
    /// branches. A cine-in at progress 1.0 ends: `AtlBink_Start` of the
    /// kept film (@ 0x00402fef, `traj` and `loc` 0). Else the screen
    /// cleared to black (FUN_00412be0 → `CDirectDrawWindow::Clear`, colour
    /// 0), the panorama drawn by FUN_00444870(screen, ftol(p · 76)) — the
    /// renderer's rows limited to band .. h − band − 1 (its rectangle
    /// {band, h − band − 1, w − 1, 0} @ 0x004448a8..0x004448cc, the warp's
    /// own draw passing {0, h − 1, w − 1, 0} @ 0x00442a36), the place
    /// animations' sounds, sprites and step as the warp's — then the flip
    /// (FUN_004127a0(0): the stretch and the texts); p += 2 · the time
    /// the draw and flip took, capped at 1 (cine-out: −, floored at 0).
    /// A cine-out at 0 ends: 0x006e01e8, 0x006e01ec := 0, then
    /// FUN_00403340 again: the film's close.
    ///
    /// The time: the program reads the performance counter before the
    /// draw (@ 0x0040300e) and after the flip (@ 0x00403049); this port's
    /// flip is the frontend's present, so the port adds the time since
    /// the previous pass [INFERRED: the draw and flip are the whole pass
    /// of loop mode 5, the flip waiting for the display].
    fn cine_step(&mut self) {
        let Some(c) = self.film_cine.as_mut() else {
            return;
        };
        let done = if c.out { c.p == 0.0 } else { c.p == 1.0 };
        if done {
            let c = self.film_cine.take().expect("the cine");
            if c.out {
                self.film_cine_in = false;
                self.film_cine_out = false;
                self.film_close(c.stopped);
            } else if let Some((file, txt, fps)) = c.start {
                if !self.atl_bink_start(&file, &txt, fps, false, false, Vec::new(), c.back) {
                    // `AtlBink_Start` failed (mode back), then FUN_00403340
                    // → FUN_00403230 with no film open.
                    self.mode = c.back;
                    self.film_close(false);
                }
            }
            return;
        }
        let dt = (self.now - c.last).max(0.0);
        c.last = self.now;
        // ftol (0x00667538): truncation.
        let band = (c.p * CINE_BAND) as usize;
        self.draw_letterbox_2004(band);
        self.flip_texts();
        let Some(c) = self.film_cine.as_mut() else {
            return;
        };
        let step = (2.0 * dt) as f32;
        if c.out {
            c.p -= step;
            if c.p <= 0.0 {
                c.p = 0.0;
            }
        } else {
            c.p += step;
            if c.p >= 1.0 {
                c.p = 1.0;
            }
        }
    }

    /// FUN_00412be0 then FUN_00444870(screen, band): the screen black, the
    /// panorama in rows band .. h − band − 1 (no cursor: the mode is 5).
    pub(crate) fn draw_letterbox_2004(&mut self, band: usize) {
        // FUN_00444870 draws no full-screen sprite (FUN_00444670 does).
        let spr = self.spr_plein_ecran.take();
        let drawn = self.draw_warp_only();
        self.spr_plein_ecran = spr;
        if !drawn {
            self.screen.clear();
            return;
        }
        let (w, h) = (self.width, self.height);
        let band = band.min(h / 2);
        let (top, bottom) = (band * w, (h - band) * w);
        match &mut self.screen {
            Surface::Rgb565(s) => {
                s[..top].fill(0);
                s[bottom..].fill(0);
            }
            Surface::Xrgb8888(s) => {
                s[..top].fill(0);
                s[bottom..].fill(0);
            }
        }
    }

    /// FUN_00402fb0, one pass of loop mode 5 (no cine-in or cine-out
    /// running): `TESTTIMERVIDEO` = 1 → the timer scans (FUN_00450e60, the
    /// warp's own: `/tim1` while `TESTTIME1`, `/tim2` while `TESTTIME2`);
    /// the frame drawn when due (FUN_00402e20; a travel film's transition
    /// sprites over it, FUN_004360f0(`FrameNum` − 1)), the flip after a
    /// drawn frame, then the stops: a click or a double click (0x006e9fe8,
    /// 0x006e9feb) or Escape / Space (0x006e9e70) end the film with
    /// `NUMFRAMEVIDEO` := `FrameNum`; else, after a drawn frame,
    /// FUN_00402f60: the last frame ends it, or `BinkNextFrame`.
    pub(crate) fn film_step(&mut self) {
        if self.film.is_none() {
            return;
        }
        // @ 0x0040313b..0x0040314d.
        if self.script.as_ref().and_then(|s| s.var("TESTTIMERVIDEO")) == Some(1) {
            self.with_script(|s, h| s.timer_scans(h));
        }
        let Some(f) = self.film.as_mut() else {
            return;
        };
        let mut drew = false;
        if f.film.due(self.now) {
            if f.film.do_frame() {
                drew = true;
                let (w, h) = f.film.size();
                let (sw, sh) = (self.width, self.height);
                // FUN_004360f0(`FrameNum` − 1) @ 0x00402eb9: the sprites'
                // own positions in the stretch surface.
                let n = f.film.frame_num;
                match &mut self.screen {
                    Surface::Rgb565(out) => {
                        film_frame(&f.film.rgb565, w, h, &f.spf, n, out, sw, sh)
                    }
                    Surface::Xrgb8888(out) => {
                        film_frame(&f.film.xrgb, w, h, &f.spf, n, out, sw, sh)
                    }
                }
                // FUN_00403390 with the frame drawn (`FrameNum`).
                let frame = f.film.frame_num as i32;
                let fpu = self.camera.fpu;
                if let Some(c) = f.caption.as_mut() {
                    for op in c.frame(frame, fpu) {
                        self.subtitle = match op {
                            CaptionOp::Set(rgb) => Some(Subtitle {
                                text: c.caption.text.clone(),
                                rgb,
                                caption: true,
                            }),
                            CaptionOp::Clear => None,
                        };
                    }
                }
                for (cue, shown, done) in f.cues.iter_mut() {
                    if *done {
                        continue;
                    }
                    if !*shown {
                        if frame >= cue.from_frame {
                            *shown = true;
                            let text = self
                                .texts
                                .get_or_insert_with(|| {
                                    crate::text::Texts::load(
                                        &self.files,
                                        &self.startup.text_language(),
                                    )
                                })
                                .dial(cue.dialog, cue.line)
                                .map(<[u8]>::to_vec)
                                .unwrap_or_default();
                            self.subtitle = Some(Subtitle {
                                text,
                                rgb: cue.rgb(),
                                caption: false,
                            });
                        }
                    } else if frame >= cue.to_frame {
                        *done = true;
                        self.subtitle = None;
                    }
                }
            } else {
                // A decoding error: the port ends the film.
                self.film_end(true);
                return;
            }
        }
        if drew {
            // FUN_004127a0: the stretch to the screen, then the texts.
            self.flip_texts();
        }
        let Some(f) = self.film.as_mut() else {
            return;
        };
        let frame_num = f.film.frame_num as i32;
        if self.click || self.double_click || self.film_skip_key {
            // @ 0x00403197..0x0040321e: the three flags cleared, 0x006e01e3
            // := 1 (the sound track stopped at the close).
            if let Some(s) = self.script.as_mut() {
                s.set_var("NUMFRAMEVIDEO", frame_num);
            }
            self.click = false;
            self.double_click = false;
            self.film_skip_key = false;
            if let Some(s) = self.script.as_mut() {
                s.click = false;
                s.double_click = false;
            }
            self.film_end(true);
            return;
        }
        if drew && !f.film.next_frame() {
            // FUN_00402f60 @ 0x00402f6f: the last frame shown.
            if let Some(s) = self.script.as_mut() {
                s.set_var("NUMFRAMEVIDEO", frame_num);
            }
            self.film_end(false);
        }
    }

    /// FUN_00403340: a cine-out wanted (0x006e01ec) and not a travel film
    /// → the cine-out starts at progress 1.0 (the stretch surface closed;
    /// the film, its sound track and the subtitle stay until the close);
    /// else the film's close ([`Game::film_close`]).
    fn film_end(&mut self, stopped: bool) {
        let Some(f) = self.film.as_ref() else {
            return;
        };
        if self.film_cine_out && !f.traj {
            self.film_cine = Some(Cine {
                out: true,
                p: 1.0,
                last: self.now,
                start: None,
                back: f.back,
                stopped,
            });
            return;
        }
        self.film_close(stopped);
    }

    /// FUN_00403340's end then FUN_00403230: the loop mode back,
    /// `BinkClose`; the sound track stopped when the film was stopped
    /// (0x006e01e3) or the track already ended (0x006e0208 = 0), else it
    /// plays on to its end; the subtitle cleared, the cue list freed; the
    /// music back to full in 2 s when `%` is over 0 (FUN_00430c50(1.0,
    /// 2000) @ 0x0040332e); `Interp_CloseInterrupt(1)` (FUN_00452ef0): the
    /// waiting section goes on. The transition sprites stay loaded until
    /// the next `AtlSpr_InitSpf` (FUN_00403230 does not free them); the
    /// port drops them with the film, as only a travel film draws them.
    fn film_close(&mut self, stopped: bool) {
        let f = self.film.take();
        if let Some(f) = &f {
            self.mode = f.back;
        }
        if stopped {
            if let Some(s) = self.film_sound.take() {
                s.stop(&self.audio);
            }
        }
        self.subtitle = None;
        if self.film_percent > 0.0 {
            self.music2004.duck(1.0, 2000);
        }
        if let Some(f) = &f {
            log::info!("AtlBink end ({})", f.film.name());
        }
        self.with_script(|s, h| s.close_interrupt(h));
        self.look_last = None;
    }

    /// Whether loop mode 5 runs: a film, or its cine-in or cine-out.
    pub(crate) fn film_running_2004(&self) -> bool {
        self.film.is_some() || self.film_cine.is_some()
    }

    /// The film's sound track decoded ahead of the output; its end
    /// (FMOD's end callback LAB_00401d10: 0x006e0208 := 0) drops it.
    pub(crate) fn film_sound_frame(&mut self) {
        if let Some(s) = self.film_sound.as_mut() {
            if !s.pump(&self.audio) {
                self.film_sound = None;
            }
        }
    }

    /// Tests: the film playing (name, `FrameNum`, frames) and whether its
    /// sound track plays.
    #[doc(hidden)]
    pub fn film_2004(&self) -> Option<(&str, u32, u32)> {
        self.film
            .as_ref()
            .map(|f| (f.film.name(), f.film.frame_num, f.film.frames()))
    }

    #[doc(hidden)]
    pub fn film_sound_playing(&self) -> bool {
        self.film_sound.is_some()
    }

    /// Tests: the cine-in or cine-out running (whether it is the
    /// cine-out, its progress).
    #[doc(hidden)]
    pub fn film_cine_2004(&self) -> Option<(bool, f32)> {
        self.film_cine.as_ref().map(|c| (c.out, c.p))
    }

    /// Tests: the travel film's transition banks loaded.
    #[doc(hidden)]
    pub fn film_spf_2004(&self) -> usize {
        self.film.as_ref().map_or(0, |f| f.spf.len())
    }
}

/// A film frame to the screen: with transition sprites, the frame copied
/// into a stretch surface of W × ftol(W · 0.75) at y = (surface height −
/// film height) >> 1 (`BinkCopyToBuffer` @ 0x00402e97), the sprites drawn
/// there (`AtlSpr_Aff` at their own positions, frame `n` − 1, alpha 1,
/// FUN_004360f0), then the stretch ([`stretch_video`]).
#[allow(clippy::too_many_arguments)]
fn film_frame<P: Pixel + Default>(
    frame: &[P],
    w: usize,
    h: usize,
    spf: &[Bank],
    n: u32,
    out: &mut [P],
    sw: usize,
    sh: usize,
) {
    if spf.is_empty() {
        stretch_video(frame, w, h, out, sw, sh);
        return;
    }
    let surf_h = (w as f32 * 0.75) as usize;
    let mut surf = vec![P::default(); w * surf_h];
    let top = surf_h.saturating_sub(h) >> 1;
    for y in 0..h.min(surf_h - top) {
        surf[(top + y) * w..(top + y + 1) * w].copy_from_slice(&frame[y * w..(y + 1) * w]);
    }
    crate::travel::draw_transition_sprites(&mut surf, w, surf_h, spf, n);
    stretch_video(&surf, w, surf_h, out, sw, sh);
}
