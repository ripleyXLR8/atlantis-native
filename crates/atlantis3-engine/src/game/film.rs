//! *Atlantis Evolution*'s 2004 program's films in the game (atlantis4-cd-en-3022848):
//! the script's `CINEMA=` (FUN_0044c850 @ 0x0044ccc6), FUN_00402a70 and
//! `AtlBink_Start` (FUN_00401d30), the loop mode 5 handler
//! (`AtlMain_loop_BINK` @ 0x004a1560: FUN_00402fb0 while it returns 1, then
//! FUN_00403340) and the film's close (FUN_00403230).
//!
//! The RE note is atlantis4-re `subsystems/cd2004-films.md`. Not ported (the
//! note's open items): the travel films (`traj<n>`, `binktraj\`, drawn over
//! the panorama), the 0.5 s letterbox animations of the cine-in (without
//! `nocinein`) and of `cineout`, the place sounds' stop at the start
//! (FUN_00439dc0), the inventory's and subject menu's close
//! (@ 0x00402ab6..0x00402ade) and `TESTTIMERVIDEO`.

use std::sync::Arc;

use atlantis3_data::cine_txt::{Caption, CueFile};
use atlantis3_data::evolution2004_film::{FilmFiles, FilmOptions};

use super::{stretch_video, Game, Mode, Subtitle};
use crate::audio::CHANNEL_CINEMA;
use crate::caption::{CaptionFade, CaptionOp};
use crate::film2004::{sound_gain, Film, FilmSound};
use crate::px::Surface;

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
    /// 0x006e9e70: Escape or Space in loop mode 5 ([`Game::skip`]).
    pub skip_key: bool,
}

impl Game {
    /// The 2004 `CINEMA=` line from its `=` (@ 0x0044ccc6..0x0044d6c8):
    /// the options read, the `%` stored (0x006ac080, whether the film
    /// starts or not), the name looked up in `#CINEMATIQUES#`, then
    /// FUN_00402a70(file, txt, fps, traj ≠ 0, 0). Whether the film started.
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
        if o.traj != 0 {
            // `binktraj\` + name (FUN_00418740 @ 0x0044d539), FUN_00435c80
            // and the film drawn over the panorama (FUN_004360f0).
            self.not_ported("the 2004 travel films (CINEMA=..,traj<n>)");
            return false;
        }
        if !o.nocinein {
            // FUN_00402a70 @ 0x00402b05..: the film starts after a 0.5 s
            // animation of the view (FUN_00402fb0 @ 0x00402ffd..0x00403077)
            // that is not ported: it starts at once.
            self.not_ported("the 2004 films' cine-in animation");
        }
        if o.cine_out() {
            self.not_ported("the 2004 films' cine-out animation");
        }
        self.film_start(&file, o.fps)
    }

    /// FUN_00402a70 then `AtlBink_Start(file, txt, fps, 0, 0)`.
    fn film_start(&mut self, file: &str, fps: i32) -> bool {
        // FUN_00402a70: 0x006e9e70, 0x006e9fe8, 0x006e9feb := 0
        // (@ 0x00402a94..0x00402aa0); loop mode 5 already: refused
        // (`AtlBink_start(%s): deja en AtlMain_loop_BINK!!` @ 0x00402aba).
        if self.film.is_some() {
            log::warn!("AtlBink_start({file}): deja en AtlMain_loop_BINK!!");
            return false;
        }
        self.click = false;
        self.double_click = false;
        let langue = self.startup.voice_language();
        let sous_titre = self.startup.text_language();
        let files = &self.files;
        let paths = FilmFiles::resolve(file, &langue, &sous_titre, |p| files.size(p).is_some());
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
        let film = match Film::open(file, Arc::from(data), self.depth(), self.now) {
            Ok(f) => f,
            Err(e) => {
                log::warn!(
                    "AtlBink_start - Cinematique non ouverte : {} ({e:?})",
                    paths.video
                );
                return false;
            }
        };
        // The caption (FUN_00401120 @ 0x0040232e).
        let caption = self
            .files
            .read(&paths.caption)
            .and_then(|d| Caption::parse(&d))
            .map(CaptionFade::new_2004);
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
        // @ 0x004028f0..0x00402902: the subtitle cleared, the clicks
        // cleared (`NUMFRAMEVIDEO` := 0 by the script, which is the
        // caller here: `Script::cinema_2004`).
        self.subtitle = None;
        self.click = false;
        self.double_click = false;
        log::info!("AtlBink_start({}) {:?}", paths.video, film.size());
        self.film = Some(FilmPlaying {
            film,
            back: self.mode,
            cues: cues.into_iter().map(|c| (c, false, false)).collect(),
            caption,
            skip_key: false,
        });
        self.mode = Mode::Video;
        true
    }

    /// FUN_00402fb0, one pass of loop mode 5 (no cine-in or cine-out
    /// running): the frame drawn when due (FUN_00402e20), the flip after a
    /// drawn frame, then the stops: a click or a double click (0x006e9fe8,
    /// 0x006e9feb) or Escape / Space (0x006e9e70) end the film with
    /// `NUMFRAMEVIDEO` := `FrameNum`; else, after a drawn frame,
    /// FUN_00402f60: the last frame ends it, or `BinkNextFrame`.
    pub(crate) fn film_step(&mut self) {
        let Some(f) = self.film.as_mut() else {
            return;
        };
        let mut drew = false;
        if f.film.due(self.now) {
            if f.film.do_frame() {
                drew = true;
                let (w, h) = f.film.size();
                let (sw, sh) = (self.width, self.height);
                match &mut self.screen {
                    Surface::Rgb565(out) => stretch_video(&f.film.rgb565, w, h, out, sw, sh),
                    Surface::Xrgb8888(out) => stretch_video(&f.film.xrgb, w, h, out, sw, sh),
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
        if self.click || self.double_click || f.skip_key {
            // @ 0x00403197..0x0040321e: the three flags cleared, 0x006e01e3
            // := 1 (the sound track stopped at the close).
            if let Some(s) = self.script.as_mut() {
                s.set_var("NUMFRAMEVIDEO", frame_num);
            }
            self.click = false;
            self.double_click = false;
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

    /// FUN_00403340 (no `cineout`) then FUN_00403230: the loop mode back,
    /// `BinkClose`; the sound track stopped when the film was stopped
    /// (0x006e01e3) or the track already ended (0x006e0208 = 0), else it
    /// plays on to its end; the subtitle cleared, the cue list freed; the
    /// music back to full in 2 s when `%` is over 0 (FUN_00430c50(1.0,
    /// 2000) @ 0x0040332e); `Interp_CloseInterrupt(1)` (FUN_00452ef0): the
    /// waiting section goes on.
    fn film_end(&mut self, stopped: bool) {
        let Some(f) = self.film.take() else {
            return;
        };
        self.mode = f.back;
        if stopped {
            if let Some(s) = self.film_sound.take() {
                s.stop(&self.audio);
            }
        }
        self.subtitle = None;
        if self.film_percent > 0.0 {
            self.music2004.duck(1.0, 2000);
        }
        log::info!("AtlBink end ({})", f.film.name());
        self.with_script(|s, h| s.close_interrupt(h));
        self.look_last = None;
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
}
