//! The presentation of the dialogs: the talking heads over the panorama,
//! the voices, the subtitles, the subject menu and the game-over screen.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`dialog.cpp`
//! `playConSection` (its presentation of each line), `playSingleConLine`,
//! `showSubjectMenu`, `drawGivenObject`; `menus.cpp` `showGameOver`; GPLv3,
//! the ScummVM team), which follow `atlantis.exe`: the section executor
//! `FUN_0042844c`, the line player `FUN_0042ee88`, the voice-only players
//! `FUN_00430258` (`off`, or a line without its `.syc`), `FUN_004305e8`
//! (`off0`) and `FUN_00430884` (`off1`), the subject menu `FUN_004261a0` /
//! `FUN_00425828` and GereGameOver `FUN_00426590`.
//!
//! [`Dialogs`] implements [`DialogPresenter`]; the section walk itself is in
//! [`crate::scripts`].
//!
//! # Pace
//!
//! The original's line loop is not paced: it shows the next UBB frame as
//! fast as the machine allows. The port shows one every 80 ms, the pace the
//! ScummVM port's HNM decoder gives the UBB2 videos.

pub mod game_over;
pub mod rules;
pub mod subject_menu;

use std::rc::Rc;

use atlantis_codecs::UbbVideo;
use atlantis_data::sprite::{composite_spf_frame, read_spr_file, read_tga15, spw_frame_count};
use atlantis_data::sprite_blend::{blend_ubb_pixel565, compose_ubb_ia, ubb_ia_coverage};
use atlantis_data::SprSprite;

use crate::audio::VoiceId;
use crate::con_script::script::latin1_bytes;
use crate::engine::{Engine, Event};
use crate::head::{head_paths, texture_paths, DialogHead};
use crate::hooks::HookFuture;
use crate::music::apc_pcm;
use crate::scripts::{DialogPresenter, GameOverRequest, LineOutcome, LineRequest};
use crate::subtitles::{
    player_font, system_font, BitmapBreaker, BitmapFont, FontFace, SubtitleFont, Subtitler,
};
use crate::sys::{Input, Key, MouseButton};
use crate::video_skip::VideoSkipState;
use crate::warp::{SCREEN_H, SCREEN_W};

use game_over::{
    game_over_fades_new_sound, game_over_image_name, game_over_subtitle_drawn,
    game_over_voice_name, game_over_wait_ends,
};
use rules::{
    dial_row, head_open_fail_text, head_open_fails, line_fov, line_view, pick_dial_cam, Angles,
    DialInit, LineHeader,
};
use subject_menu::{menu_draws, menu_hit, menu_subject};

/// The pace of the UBB frames (the ScummVM port's HNM decoder for UBB2).
pub const UBB_FRAME_MS: u64 = 80;

/// Where the given object's icon is drawn: its hotspot at (0x136, 0x1a4)
/// (`FUN_0042ee88` @ 0x0042f52b..0x0042f555).
const GIVEN_OBJECT_POS: (i32, i32) = (0x136, 0x1a4);

/// The original's normal pointer: CURSEURS.SPR frame 6 (`FUN_0042d558` @
/// 0x0042d5b9), for the subject menu and the error box.
const ARROW_CURSOR: usize = 6;

const N: usize = SCREEN_W * SCREEN_H;

/// The input of one frame of a dialog loop.
#[derive(Default)]
struct FrameInput {
    left_press: bool,
    right_press: bool,
    escape: bool,
    enter: bool,
}

fn poll_input(e: &Engine) -> FrameInput {
    let mut fi = FrameInput::default();
    while let Some(ev) = e.sys.poll_event() {
        match ev {
            Input::MouseDown {
                button: MouseButton::Left,
                ..
            } => fi.left_press = true,
            Input::MouseDown {
                button: MouseButton::Right,
                ..
            } => fi.right_press = true,
            Input::KeyDown(Key::Escape) => fi.escape = true,
            Input::KeyDown(Key::Enter | Key::Space) => fi.enter = true,
            _ => {}
        }
    }
    fi
}

/// The buttons released and the pending events dropped (the loops drain
/// the press that opened them).
async fn drain(e: &Engine) {
    e.wait_buttons_up().await;
    while e.sys.poll_event().is_some() {}
}

/// The original's error box (dialog 0x65 of the executable's resources,
/// its procedure's text for the error code): a modal Windows dialog with
/// an OK button; the port draws its own box over the screen in `font` and
/// waits for OK (a click on it, Enter, Space or Escape). (The original
/// also switches the display to a window first, FUN_0041a1d4; not done.)
pub(crate) async fn error_box(e: &Engine, font: &SubtitleFont, text: &str) {
    e.emit(Event::ErrorBox(text.to_string()));
    let bytes = latin1_bytes(text);
    let base = e.sys.screen.borrow().clone();
    let w = (font.width(&bytes) + 40).clamp(160, SCREEN_W as i32 - 20);
    let h = 76;
    let (bx, by) = ((SCREEN_W as i32 - w) / 2, (SCREEN_H as i32 - h) / 2);
    let ok = (bx + w / 2 - 36, by + h - 32, 72, 22);
    let fill = |s: &mut [u16], x: i32, y: i32, w: i32, h: i32, c: u16| {
        for yy in y.max(0)..(y + h).min(SCREEN_H as i32) {
            for xx in x.max(0)..(x + w).min(SCREEN_W as i32) {
                s[yy as usize * SCREEN_W + xx as usize] = c;
            }
        }
    };
    drain(e).await;
    loop {
        if e.sys.should_quit() {
            break;
        }
        {
            let mut s = e.sys.screen.borrow_mut();
            s.copy_from_slice(&base);
            fill(&mut s, bx, by, w, h, 0x0000);
            fill(&mut s, bx + 1, by + 1, w - 2, h - 2, 0xc618);
            font.draw(
                &mut s,
                SCREEN_W,
                SCREEN_H,
                &bytes,
                bx + 20,
                by + 14,
                w - 30,
                0,
            );
            fill(&mut s, ok.0, ok.1, ok.2, ok.3, 0x0000);
            fill(&mut s, ok.0 + 1, ok.1 + 1, ok.2 - 2, ok.3 - 2, 0xdefb);
            let tw = font.width(b"OK");
            font.draw(
                &mut s,
                SCREEN_W,
                SCREEN_H,
                b"OK",
                ok.0 + (ok.2 - tw) / 2,
                ok.1 + 4,
                ok.2,
                0,
            );
            Dialogs::draw_arrow(e, &mut s);
        }
        e.sys.frame().await;
        let fi = poll_input(e);
        let (mx, my) = e.sys.mouse();
        let on_ok = (ok.0..ok.0 + ok.2).contains(&mx) && (ok.1..ok.1 + ok.3).contains(&my);
        if (fi.left_press && on_ok) || fi.enter || fi.escape {
            break;
        }
    }
    e.sys.screen.borrow_mut().copy_from_slice(&base);
    drain(e).await;
}

/// Copies a `w` x `h` picture onto the 640x480 screen, centred, clipped.
fn blit_centred(screen: &mut [u16], src: &[u16], w: usize, h: usize) {
    let bx = (SCREEN_W as i64 - w as i64) / 2;
    let by = (SCREEN_H as i64 - h as i64) / 2;
    let (bx, by) = (bx.max(0) as usize, by.max(0) as usize);
    for y in 0..h.min(SCREEN_H - by) {
        let n = w.min(SCREEN_W - bx);
        screen[(by + y) * SCREEN_W + bx..(by + y) * SCREEN_W + bx + n]
            .copy_from_slice(&src[y * w..y * w + n]);
    }
}

/// HD: the subtitle `lines` just drawn on the screen (`under` before,
/// `after` with them; moved by (`dx`, `dy`)), for the front end to draw
/// sharp (see [`crate::hd::HdText`]).
fn record_subtitles(
    e: &Engine,
    font: &Subtitler,
    under: &[u16],
    after: &[u16],
    lines: &[Vec<u8>],
    dx: i32,
    dy: i32,
) {
    let items = font.items(lines, dx, dy);
    e.sys.hd_text.borrow_mut().set(items, under, after);
    e.sys.hd_text_mark();
}

/// What a line hands to the line player (`playSingleConLine`'s
/// arguments).
struct SingleLine<'a> {
    section: i32,
    letter: char,
    text: &'a str,
    /// The text's final blanks ([`crate::con_script::ConLine::trail`]).
    trail: &'a str,
    /// The UBB (`DIALOG\PERSO\<name>.UBB`), none for a voice-only line.
    video: Option<String>,
    /// The camera's SPF overlay (`DIALOG\PERSO\<name>.SPF`).
    spf: Option<String>,
    /// 2..5 for a talking head, 0 for a voice-only line.
    cam: i32,
    /// `off0` / `off1`: the subtitle over the screen as it stands.
    keep_screen: bool,
    /// `off1`: the screen buffer becomes the screen once the voice loads.
    screen_buffer: bool,
    /// A voice-only line that redraws the view (`FUN_00430258`).
    voice_off: bool,
    /// The INITDIAL row of the head, for the 3D head.
    dial_row: Option<i32>,
}

/// What the line player did.
#[derive(Default)]
struct Played {
    escape: bool,
    frames: u32,
    voice: bool,
    subtitles: usize,
}

/// The dialog presenter of the game.
#[derive(Default)]
pub struct Dialogs {
    /// `DIALOG\INITDIAL.TXT`, read at the first line.
    init: Option<Rc<DialInit>>,
    /// The camera of the last line (0x0077290a), 0 for none yet.
    last_cam: i32,
    /// SUJETS.SPR: the subject menu's border (frame 0) and icons.
    subject_sprites: Option<Rc<Vec<SprSprite>>>,
    /// OBJETS1.SPR: the object icons, for the given object.
    object_sprites: Option<Rc<Vec<SprSprite>>>,
    font: Option<Rc<SubtitleFont>>,
    /// The subtitles of the build (the 1997 build's FONTS.BIN, read at
    /// the first line).
    subtitler: Option<Subtitler>,
}

impl Dialogs {
    pub fn new() -> Dialogs {
        Dialogs::default()
    }

    /// The INITDIAL table (`loadDialInitData`).
    pub async fn dial_init(&mut self, e: &Engine) -> Rc<DialInit> {
        if self.init.is_none() {
            let t = match e.read("DIALOG\\INITDIAL.TXT").await {
                Some(d) => DialInit::parse(&d),
                None => DialInit::default(),
            };
            log::info!("INITDIAL.TXT: {} rows, hero {}", t.rows.len(), t.hero);
            self.init = Some(Rc::new(t));
        }
        self.init.clone().unwrap_or_default()
    }

    async fn sprites(
        e: &Engine,
        slot: &mut Option<Rc<Vec<SprSprite>>>,
        name: &str,
    ) -> Rc<Vec<SprSprite>> {
        if slot.is_none() {
            let (w, h) = atlantis_data::sprite::limits::DEFAULT;
            let s = e
                .read(&format!("SPRITE\\2D\\{name}"))
                .await
                .and_then(|d| read_spr_file(&d, w, h))
                .unwrap_or_default();
            *slot = Some(Rc::new(s));
        }
        slot.clone().unwrap_or_default()
    }

    /// The outline font, reading the bytes in the release's code page
    /// (the DOS code page texts of atlantis-cd-fr-688640 as code page 850);
    /// for the Chinese builds the player's own font beside the game's
    /// files, else the port's ([`FontFace::for_release`]), the rows of the
    /// build's step.
    async fn font(&mut self, e: &Engine) -> Rc<SubtitleFont> {
        // The code page of the moment (the Dutch and Swedish DVDs' DOS
        // texts: [`crate::enhancements::BugFix::DosLetters`]).
        let code_page = e.subtitle_code_page();
        if let Some(f) = self.font.as_ref().filter(|f| f.code_page() == code_page) {
            return f.clone();
        }
        let build = &e.game.exe.build;
        let file = match player_font(code_page) {
            // The Japanese release's MS PGothic also from the system's
            // fonts (`system_font`).
            Some((name, _)) => match e.read_beside(name).await {
                Some(f) => Some(f),
                None => system_font(code_page),
            },
            None => None,
        };
        let face = FontFace::for_release(code_page, file);
        let mut f = SubtitleFont::with_face(code_page, face).with_row_step(build.subtitle_row_step);
        if build.subtitle_languages {
            f = f.with_rows_to_the_edge();
        }
        let f = Rc::new(f);
        self.font = Some(f.clone());
        f
    }

    /// The build's subtitles: the GDI Arial layout, or the 1997 build's
    /// `DIALOG\FONTS.BIN` (atlantis-cd-en-688128 0x004191c2..0x004191f9
    /// loads it at start-up; the reference's layout when it is missing);
    /// with the breaker of the moment ([`Engine::subtitle_split`]: the
    /// Chinese rows' fix, [`crate::enhancements::BugFix::ChineseRows`]; the
    /// Japanese DVD build's language).
    async fn subtitler(&mut self, e: &Engine) -> Subtitler {
        let split = e.subtitle_split();
        let code_page = e.subtitle_code_page();
        if let Some(s) = &self.subtitler {
            match s {
                Subtitler::Gdi(font, _) if font.code_page() != code_page => {}
                Subtitler::Gdi(font, _) => return Subtitler::Gdi(font.clone(), split),
                _ => return s.clone(),
            }
        }
        let mut s = None;
        if e.game.exe.build.bitmap_subtitles {
            match e
                .read("DIALOG\\FONTS.BIN")
                .await
                .and_then(|d| BitmapFont::new(&d))
            {
                Some(f) => {
                    // atlantis-cd-sv-688128 FUN_0042ea7c: the back space only.
                    let breaker = if e.game.exe.build.subtitle_cut_back_only {
                        BitmapBreaker::Back
                    } else {
                        BitmapBreaker::Nearest
                    };
                    s = Some(Subtitler::Bitmap(std::sync::Arc::new(f), breaker))
                }
                None => log::warn!("DIALOG\\FONTS.BIN missing: the subtitles in Arial"),
            }
        }
        // The Chinese builds' breaker (FUN_0042f424), the Japanese one's
        // (FUN_0042f618).
        let s = match s {
            Some(s) => s,
            None => Subtitler::Gdi(self.font(e).await, split),
        };
        self.subtitler = Some(s.clone());
        s
    }

    /// Draws the arrow cursor at the mouse.
    fn draw_arrow(e: &Engine, screen: &mut [u16]) {
        if let Some(spr) = e.cursor_sprites.get(ARROW_CURSOR) {
            let (x, y) = e.sys.mouse();
            spr.draw(screen, SCREEN_W, SCREEN_H, x, y);
        }
    }

    /// The presentation of one line of a section walk (`FUN_0042844c` @
    /// 0x00428689..0x00428b5e, then `FUN_0042ee88`).
    async fn line(&mut self, e: &mut Engine, req: &LineRequest) -> LineOutcome {
        let init = self.dial_init(e).await;
        let perso = req.perso;
        let header = LineHeader::parse(&req.line);
        // The camera (pickDialCam).
        let mut clock = e.sys.millis();
        let cam = pick_dial_cam(
            &init,
            perso,
            &header,
            self.last_cam,
            e.game.exe.build.rhea_camera2,
            &mut clock,
        );
        // off / off0 / off1: no talking head; off0 (FUN_004305e8) draws the
        // subtitle over the screen as it stands, with no view redraw. off1
        // (FUN_00430884; the GOG.com build's FUN_004307d0;
        // atlantis-cd-zh-tw-696832 FUN_0042fe7c, called @ 0x0042e3a6 when
        // off0's flag 0x004860b8 is clear and 0x004860bc set) is off0's
        // code with the screen buffer copied to the screen once the voice
        // has loaded (play_single), with no sprite pass (FUN_00425304), no
        // panorama redraw (FUN_00451d8b) and no FOV call (FUN_0045179a).
        // The GOG.com scripts have 30 off1 headers, the Chinese ones 29, the
        // others none. The 1997 executables have no such player
        // (`ExeBuild::off1_player`): an off line there.
        let off = header.off;
        let off1 = header.off1 && e.game.exe.build.off1_player;
        let keep_screen = header.off0 || off1;
        let row = dial_row(header.seth && !off, perso, req.inc_perso as u8);
        // FUN_0042ee88 step 3: a line whose wav\ata<NNNN><letter>.syc is
        // missing (FUN_0042f9e8 fails) is played voice-only by FUN_00430258,
        // as an off line: FOV 75.137, the view at the kept angles, no UBB.
        let no_syc = !off
            && !e
                .game
                .big
                .contains(&format!("WAV\\ATA{:04}{}.SYC", req.section, req.letter));
        let voice_only = off || no_syc;
        let mut head_fails = false;
        let mut video = None;
        let mut spf = None;
        // The name FUN_0042ee88 appends to "dialog\perso\" (0x0042ef9d..
        // 0x0042effb): the parser's name + ".ubb", empty without a record.
        let mut ubb_name = String::new();
        if !voice_only {
            let rec = init.rows.get(&row);
            let cam_ubb = rec.map_or("", |r| r.cam_ubb[(cam - 2) as usize].as_str());
            head_fails = head_open_fails(voice_only, rec.is_some(), cam_ubb);
            if let Some(r) = rec {
                ubb_name = format!("{cam_ubb}.ubb");
                let ubb = &r.cam_ubb[(cam - 2) as usize];
                if !ubb.is_empty() && !ubb.eq_ignore_ascii_case("none") {
                    video = Some(format!("DIALOG\\PERSO\\{}.UBB", ubb.to_ascii_uppercase()));
                }
                let s = &r.cam_spf[(cam - 2) as usize];
                if !s.is_empty() {
                    spf = Some(format!("DIALOG\\PERSO\\{}.SPF", s.to_ascii_uppercase()));
                }
            }
        }
        self.last_cam = cam;

        // The view behind the line (rules::line_view): a talking head turns
        // from the kept angles without moving them; explicit header angles
        // move them; a voice-only line redraws at them; after the line the
        // view goes back to them.
        let kept_f = (e.warp.alpha(), e.warp.beta());
        let kept = Angles {
            alpha: f64::from(kept_f.0),
            beta: f64::from(kept_f.1),
        };
        let angle = req.line.angle.map(|(x, y)| {
            (
                f64::from(x) * std::f64::consts::PI / 180.0,
                f64::from(y) * std::f64::consts::PI / 180.0,
            )
        });
        let view = line_view(kept, header.seth && !off, perso, angle, e.warp.beta_limit());
        let set_view = |e: &mut Engine, a: Angles| {
            if e.warp_loaded {
                e.warp.set_alpha(a.alpha as f32);
                e.warp.set_beta(a.beta as f32);
            }
        };
        set_view(e, if voice_only { view.kept } else { view.view });

        let mut played = Played::default();
        let mut video_name = None;
        if head_fails {
            // FUN_0042ee88 @ 0x0042f036..0x0042f048: the modal error box
            // (dialog 0x65, code 0xc), then nothing of the line; the letter
            // still counts (0x00428b21). Not shown with the UI/UX
            // enhancements. atlantis-cd-fr-688640's own CRYO.DLL returns 0,
            // not -1, for the failed open, so its FUN_00433038 does not see
            // the failure and reads a header from handle 0 (FUN_00432f10);
            // what the player gets then is not established, and the port
            // does as here for that build too.
            log::warn!(
                "section {} line {}: row {row} cam {cam}, no UBB '{ubb_name}'",
                req.section,
                req.letter
            );
            if !e.enhancements().ui_ux {
                self.error_box(e, &head_open_fail_text(&ubb_name)).await;
            }
        } else {
            video_name.clone_from(&video);
            let single = SingleLine {
                section: req.section,
                letter: req.letter,
                text: &req.line.text,
                trail: &req.line.trail,
                dial_row: (!voice_only && video.is_some()).then_some(row),
                video,
                spf,
                cam: if voice_only { 0 } else { cam },
                keep_screen,
                screen_buffer: off1,
                voice_off: voice_only && !keep_screen,
            };
            // The camera's FOV over the line (rules::line_fov): set only on
            // the Seth path (0x004289ff..0x00428a14) and for ClicPerso >=
            // 0x200 (0x00428a9d..0x00428ada); a talking head with ClicPerso
            // below 0x200 (CHAPI056 /744 A, the first line of its walk) sets
            // none and keeps the FOV the walk started with, 75.137. The
            // voice-only players draw at 75.137 whatever was set
            // (FUN_00430258 @ 0x0043044c) or do not redraw the view (off0,
            // off1). Put back after the line.
            let fov = line_fov(e.warp_loaded, &view, voice_only, cam);
            let saved_fov = e.warp.hfov();
            if let Some(f) = fov {
                e.warp.set_hfov(f);
            }
            played = self.play_single(e, &single).await;
            if fov.is_some() {
                e.warp.set_hfov(saved_fov);
            }
        }
        // FUN_00451915(0x00bcb59c, 0x00bcb598) after every line (0x00428b5e).
        set_view(e, view.kept);
        e.emit(Event::LinePlayed {
            section: req.section,
            letter: req.letter,
            video: video_name,
            frames: played.frames,
            voice: played.voice,
            subtitles: played.subtitles,
        });
        LineOutcome {
            escape_skip: played.escape,
            entry_angles: view.entry_update.then_some(kept_f),
        }
    }

    /// One dialog line (`playSingleConLine`): the UBB video looped over the
    /// panorama (at the camera's field of view, which the caller sets), the
    /// voice `WAV\ATA<section><letter>.APC` and the subtitle, until the
    /// voice ends, a left click or Escape.
    async fn play_single(&mut self, e: &mut Engine, l: &SingleLine<'_>) -> Played {
        let mut out = Played::default();
        let font = self.subtitler(e).await;
        let objects = Self::sprites(e, &mut self.object_sprites, "OBJETS1.SPR").await;

        let mut ubb = None;
        if let Some(name) = &l.video {
            if let Some(d) = e.read(name).await {
                match UbbVideo::new(d) {
                    Ok(v) => ubb = Some(v),
                    Err(err) => log::warn!("{name}: {err}"),
                }
            }
        }
        // The camera's SPF overlay (INITDIAL columns 10..13: the guards'
        // mail coif, the pilot's scarf), loaded by FUN_0042fc90 and drawn by
        // FUN_0042ff90 after the head.
        let mut spf = None;
        if let Some(name) = l.spf.as_ref().filter(|n| e.game.big.contains(n)) {
            spf = e.read(name).await;
        }

        let sub_lines = if e.subtitles_shown() && !l.text.is_empty() {
            font.split_line(&latin1_bytes(&[l.text, l.trail].concat()), &e.subtitle_text)
        } else {
            Vec::new()
        };
        out.subtitles = sub_lines.len();

        // A talking-head line (FUN_0042ee88) runs with 0x00496188 = 1 and
        // calls FUN_00425304 every frame: the place sprites animate, the
        // persos are not drawn. FUN_00430258 calls it once (0x00430467,
        // persos shown) before its wait loop; FUN_004305e8 never does.
        let talking_head = l.cam != 0 && !l.keep_screen;
        // The 3D head drawn over the UBB, the animated mouth (FUN_0042ee88
        // step 4: the speaker's INITDIAL row, its first sub-record, the
        // camera, and the line's lip-sync track wav\ata<NNNN><letter>.syc
        // read by FUN_0042f9e8).
        let full_screen_ubb = ubb
            .as_ref()
            .is_some_and(|u| u.width() == SCREEN_W && u.height() == SCREEN_H);
        let head = match l.dial_row {
            Some(row) if talking_head && full_screen_ubb && (2..=5).contains(&l.cam) => {
                let syc = format!("ATA{:04}{}.SYC", l.section, l.letter);
                let mut head = self.load_head(e, row, l.cam, &syc).await;
                if let Some(h) = head.as_mut() {
                    // The rest pose of some head animations at frame 0
                    // (an original bug, fixed as an enhancement).
                    h.set_rest_key_fix(e.head_rest_key_fix());
                }
                if head.is_none() {
                    // A head file is missing (lines without a .SYC never get
                    // here: they are played voice-only).
                    log::info!("no head for INITDIAL row {row} ({syc})");
                }
                head
            }
            _ => None,
        };
        if talking_head {
            e.sprites_hide_npcs = true;
            e.recomposite_sprite_layer();
        }
        let mut player = LinePlayer {
            talking_head,
            voice_off_step: l.voice_off && !talking_head,
            kept_screen: l.keep_screen.then(|| e.sys.screen.borrow().clone()),
            kept_hd: l.keep_screen && e.sys.hd_shown(),
            hd: LineHd::default(),
            ubb,
            spf_count: spf.as_deref().map_or(0, spw_frame_count) as usize,
            spf,
            composite: Vec::new(),
            bg: vec![0; N],
            start: e.sys.millis(),
            shown: 0,
            spf_frame: 0,
            head,
            head_time0: None,
            head_ended: false,
            head_cut: false,
        };

        // The line lasts as long as its voice (FUN_0042ee88 / FUN_00430258):
        // it ends when the sample is done or on a left click (0x0049611c).
        // Escape is read by FUN_0041a298 (VideoSkipState::poll): a line with
        // a talking head runs with 0x00496188 set, so Escape ends it and
        // sets the dialog-skip flag 0x00496134 (0x0041a3d2); the voice-only
        // players (off, off0, off1, no .syc) clear 0x00496188 first, so
        // Escape ends the line only when bow mode, the bar request and
        // OBJETENMAIN are all 0 (0x00496124 = 0, 0x0041a419; their loops
        // test it, e.g. 0x00430586), without the skip flag: the following
        // lines play, and the players put 0x00496124 and 0x00496138 back
        // (0x004305b7, 0x004305c1), so no menu opens.
        let mut escape_poll = VideoSkipState {
            talking_head: l.cam != 0,
            bow_mode: e.bow_mode,
            object_in_hand: e.vars.get("objetenmain") != 0,
            section_play: e.section_play,
            ..VideoSkipState::default()
        };
        drain(e).await;
        // FUN_00430258 lowers the music when no fade runs (@ 0x0043027f) and
        // raises it once the voice has played (@ 0x004305a6); without the
        // voice it returns before, leaving the music low.
        if l.cam == 0 {
            e.music.duck(true, None);
        }
        let apc = format!("WAV\\ATA{:04}{}.APC", l.section, l.letter);
        let pcm = if e.game.big.contains(&apc) {
            e.read(&apc).await.and_then(|d| apc_pcm(&d))
        } else {
            None
        };
        match pcm {
            None => log::info!("no {apc}: line skipped"),
            Some(pcm) => {
                // off1: with the voice loaded (a CRYO_APC 1.20 header; a
                // missing file or another header returns before anything is
                // drawn), the screen buffer becomes the screen, before the
                // voice starts and the subtitle goes over it.
                if l.screen_buffer {
                    player.kept_screen = Some(off1_screen(&e.ix_buffers));
                    player.kept_hd = false;
                }
                let mixer = e.sys.mixer.clone();
                let mut voice: Option<VoiceId> = None;
                let start_voice = |e: &Engine, voice: &mut Option<VoiceId>| {
                    *voice = Some(mixer.play(pcm.clone(), false, e.voice_volume, 0));
                    e.emit(Event::Voice(apc.clone()));
                };
                // With a talking head the voice starts after the second UBB
                // frame (0x0042f678..0x0042f6a7); the voice-only players
                // start it at once.
                if !(player.ubb.is_some() && talking_head) {
                    start_voice(e, &mut voice);
                }
                loop {
                    if e.should_abort() {
                        break;
                    }
                    if let Some(v) = voice {
                        if !e.sys.mixer.is_playing(v) {
                            break;
                        }
                    }
                    player.draw_frame(e, &font, &objects, &sub_lines, voice.is_some());
                    if player.head_cut {
                        break;
                    }
                    if voice.is_none() && (player.shown >= 2 || player.ubb.is_none()) {
                        start_voice(e, &mut voice);
                    }
                    e.sys.frame().await;
                    let fi = poll_input(e);
                    // FUN_0041a298 in both line loops: a right press sets
                    // 0x00496120 and toggles the bar request.
                    if fi.right_press {
                        e.bar.right_pressed(e.section_play, false);
                    }
                    // The bar request as the right presses left it.
                    escape_poll.bar_request = e.bar.request;
                    let escape = fi.escape && escape_poll.poll(false, false, true);
                    if escape || fi.left_press || e.sys.buttons().0 {
                        // With a talking head Escape also sets the dialog-skip
                        // flag 0x00496134 that FUN_0042844c reads after the
                        // line.
                        out.escape = escape && escape_poll.talking_head;
                        break;
                    }
                }
                if let Some(v) = voice {
                    e.sys.mixer.stop(v);
                    out.voice = true;
                    if l.cam == 0 {
                        e.music.unduck(None);
                        e.bar.voice_line_ended(); // 0x004305cb
                    }
                }
            }
        }
        out.frames = player.shown;
        if talking_head {
            e.sprites_hide_npcs = false;
            e.recomposite_sprite_layer();
        }
        out
    }

    /// The 3D head of INITDIAL row `row` at camera `cam` with the lip-sync
    /// track `syc` (`WAV\<syc>`); `None` when a file is missing.
    async fn load_head(&mut self, e: &Engine, row: i32, cam: i32, syc: &str) -> Option<DialogHead> {
        let init = self.dial_init(e).await;
        let r = init.rows.get(&row)?;
        if r.s3d.is_empty() || r.anim_3dc.is_empty() || r.anim_3da.is_empty() || r.anim_m.is_empty()
        {
            return None;
        }
        let paths = head_paths(&r.s3d, &r.anim_3dc, &r.anim_3da, &r.anim_m, syc);
        let mut files = std::collections::HashMap::new();
        for p in &paths {
            let d = e.read(p).await?;
            files.insert(p.clone(), d);
        }
        // the textures the mesh names
        let mesh = atlantis_data::head::F3dc::parse(&files[&paths[0]])?;
        for p in texture_paths(&mesh) {
            let d = e.read(&p).await?;
            files.insert(p, d);
        }
        DialogHead::load(
            |name| files.get(name).cloned(),
            &r.s3d,
            &r.anim_3dc,
            &r.anim_3da,
            &r.anim_m,
            cam,
            syc,
        )
    }

    /// The error box of a head that cannot be opened ([`error_box`]).
    async fn error_box(&mut self, e: &Engine, text: &str) {
        let font = self.font(e).await;
        error_box(e, &font, text).await;
    }

    /// The subject menu (`showSubjectMenu`: draw `FUN_004261a0`, hit test
    /// `FUN_004211f0`): the subjects over the panorama, then a left click
    /// (`FUN_00425828` waits for a left click only, 0x004259b6..0x004259cc;
    /// no hover highlight).
    async fn subject_menu_impl(
        &mut self,
        e: &mut Engine,
        perso: i32,
        subjects: &[u8],
    ) -> Option<i32> {
        if subjects.is_empty() {
            return None;
        }
        let sprites = Self::sprites(e, &mut self.subject_sprites, "SUJETS.SPR").await;
        let ids: Vec<i32> = subjects.iter().map(|&s| i32::from(s)).collect();
        let (draws, gx, gy) = menu_draws(&ids);
        drain(e).await;
        loop {
            if e.should_abort() {
                return None;
            }
            {
                let mut frame = vec![0u16; N];
                if e.warp_loaded {
                    e.warp.render(&e.warp_surface, &mut frame);
                    e.hd_show_view_over(Some(&frame));
                }
                for (i, &id) in ids.iter().enumerate() {
                    let border = draws[2 * i];
                    let icon = draws[2 * i + 1];
                    if let Some(s) = sprites.first() {
                        s.draw(&mut frame, SCREEN_W, SCREEN_H, border.x, border.y);
                    }
                    let k = if id > 0 && (id as usize) < sprites.len() {
                        id as usize
                    } else {
                        0
                    };
                    if let Some(s) = sprites.get(k) {
                        s.draw(&mut frame, SCREEN_W, SCREEN_H, icon.x, icon.y);
                    }
                }
                // FUN_004261a0 @ 0x00426393: FUN_0042bc58, the guests not
                // hidden (FUN_00425828 hides all but the clicked one for a
                // follower).
                e.draw_guests(&mut frame, None);
                Self::draw_arrow(e, &mut frame);
                e.sys.screen.borrow_mut().copy_from_slice(&frame);
            }
            e.sys.frame().await;
            let fi = poll_input(e);
            if fi.left_press || e.sys.buttons().0 {
                let (mx, my) = e.sys.mouse();
                drain(e).await;
                let idx = menu_hit(ids.len() as i32, gx, gy, mx, my);
                let s = menu_subject(&ids, idx);
                let chosen = (s >= 0).then_some(s);
                e.emit(Event::SubjectMenu {
                    perso,
                    subjects: subjects.to_vec(),
                    chosen,
                });
                return chosen;
            }
        }
    }

    /// GereGameOver (`showGameOver`, `FUN_00426590`).
    async fn game_over_impl(&mut self, e: &mut Engine, req: &GameOverRequest) {
        e.stop_ambient(); // FUN_00426590 @ 0x004265d5
        let narrator = req.narrator.as_ref().filter(|(t, _)| !t.is_empty());
        // The narrator's voice, WAV\ATA<section><letter>.APC.
        let voice = match narrator {
            Some((_, letter)) => {
                let apc = format!("WAV\\{}", game_over_voice_name(req.section, *letter));
                let pcm = if e.game.big.contains(&apc) {
                    e.read(&apc).await.and_then(|d| apc_pcm(&d))
                } else {
                    None
                };
                pcm.map(|p| (p, apc))
            }
            None => None,
        };
        // The 1997 build starts it here, before the image
        // (atlantis-cd-en-688128 FUN_004256a4 @ 0x004256ee..0x0042587b). It
        // starts it with or without a narrator line and leaves it playing
        // when there is none (0x004258c1, 0x00425a2b), which none of the
        // data's 144 game overs meets: it is started only with the line.
        let mut started = None;
        if e.game.exe.build.game_over_voice_at_entry {
            if let Some((pcm, apc)) = &voice {
                started = Some(e.sys.mixer.play(pcm.clone(), false, e.voice_volume, 0));
                e.emit(Event::Voice(apc.clone()));
            }
        }
        // @ 0x00426616..0x00426647: NEWSOUND set, the NewSound sample fades.
        if game_over_fades_new_sound(e.vars.get("newsound")) {
            e.music.fade_out_new_sound(e.effects_volume());
        }
        let name = format!("IMAGES\\{}", game_over_image_name(req.n));
        let Some(img) = e.read(&name).await.and_then(|d| read_tga15(&d)) else {
            log::warn!("game over: cannot load {name}");
            if let Some(v) = started {
                e.sys.mixer.stop(v);
            }
            return;
        };
        let mut frame = vec![0u16; N];
        for y in 0..img.height.min(SCREEN_H) {
            let n = img.width.min(SCREEN_W);
            frame[y * SCREEN_W..y * SCREEN_W + n]
                .copy_from_slice(&img.pixels[y * img.width..y * img.width + n]);
        }
        // The narrator's subtitle (FUN_0042ffe8 / FUN_00430178, as a line's).
        if let (Some((text, _)), true) = (narrator, game_over_subtitle_drawn(e.subtitles_shown())) {
            let font = self.subtitler(e).await;
            let lines = font.split_line(&latin1_bytes(text), &e.subtitle_text);
            let under = e.sys.hd_tile().is_some().then(|| frame.clone());
            font.draw_subtitles(&mut frame, SCREEN_W, SCREEN_H, &lines, 0);
            if let Some(under) = under {
                // (it stands while the screen shows it, see Sys::hd_text)
                record_subtitles(e, &font, &under, &frame, &lines, 0, 0);
            }
        }
        e.sys.screen.borrow_mut().copy_from_slice(&frame);
        e.sys.frame().await;
        drain(e).await;

        // The narrator's voice: wait for it, or hold the image 5 s without
        // it (game_over_wait_ends). FUN_0041a298 polls with the flags of the
        // moment (0x00496188, the talking head, is 0 here).
        let mut skip = VideoSkipState {
            object_in_hand: e.vars.get("objetenmain") != 0,
            section_play: e.section_play,
            ..VideoSkipState::default()
        };
        if let Some((pcm, apc)) = voice {
            let v = match started {
                Some(v) => v,
                None => {
                    let v = e.sys.mixer.play(pcm, false, e.voice_volume, 0);
                    e.emit(Event::Voice(apc));
                    v
                }
            };
            // @ 0x00426b0c: until the sample ends, a left press or Escape
            // (when it would open the menu).
            while !e.should_abort() {
                let fi = poll_input(e);
                let done = !e.sys.mixer.is_playing(v);
                if game_over_wait_ends(
                    &mut skip,
                    true,
                    done,
                    0,
                    fi.left_press,
                    fi.right_press,
                    fi.escape,
                ) {
                    break;
                }
                e.sys.frame().await;
            }
            e.sys.mixer.stop(v);
        } else if narrator.is_some() {
            // @ 0x00426ae0: a narrator line whose voice is missing holds the
            // image for 5000 ms or until a left press.
            let start = e.sys.millis();
            while !e.should_abort() {
                let fi = poll_input(e);
                let elapsed = e.sys.millis() - start;
                if game_over_wait_ends(
                    &mut skip,
                    false,
                    false,
                    elapsed,
                    fi.left_press,
                    fi.right_press,
                    fi.escape,
                ) {
                    break;
                }
                e.sys.frame().await;
            }
        }
        // @ 0x00426bc2: the screen is cleared to black.
        e.sys.screen.borrow_mut().fill(0);
        e.sys.frame().await;
        drain(e).await;
    }
}

/// The screen of an `off1` line (`FUN_00430884`; atlantis-cd-zh-tw-696832
/// `FUN_0042fe7c`): the screen buffer 0x004860d4 (0x004960d4 in the 2001
/// build), the first of the two IX buffers that start-up allocates
/// (malloc(0x12c000) stored @ 0x00418f9d) and nothing clears, so it holds
/// what the last video or loadmusik copy left (`ix_buffers`; zeros before
/// any). Once the voice has loaded (`FUN_00417cc4`'s "CRYO_APC" "1.20"
/// test @ 0x00417d33..0x00417d6c passed, 0x0042ff95; the file freed @
/// 0x00430025), the surface is locked (0x0043002a..0x0043004e) and
/// FUN_00410f36 copies 0x1e0 x 0x500 bytes of it to the surface
/// (0x00430074..0x0043008a; in this build a flat copy of rows * 0x500
/// bytes, 0x00410f40..0x00410f6d), then the surface is unlocked
/// (0x00430092..0x004300a5), the voice started (0x004300ce..0x004300ed),
/// the subtitle drawn and presented (0x00430108..0x00430124). The 2001
/// build copies it to the back buffer 0x004960d8 first, then the back
/// buffer to the surface, 0x500 bytes a row at the surface's pitch
/// (0x00430a86..0x00430abb): on the port's 640-pixel screen the same
/// picture. The wait and what follows are off0's (`FUN_0042fbe0`, the same
/// code but the copy).
pub fn off1_screen(ix_buffers: &[u16]) -> Vec<u16> {
    let mut out = vec![0; N];
    let n = ix_buffers.len().min(N);
    out[..n].copy_from_slice(&ix_buffers[..n]);
    out
}

/// The frame loop state of a line (`playSingleConLine`'s `drawFrame`).
struct LinePlayer {
    talking_head: bool,
    voice_off_step: bool,
    /// off0: the screen as it stood, the subtitle's background; off1: the
    /// screen buffer, once the voice has loaded.
    kept_screen: Option<Vec<u16>>,
    ubb: Option<UbbVideo<Vec<u8>>>,
    spf: Option<Vec<u8>>,
    spf_count: usize,
    /// The UBB frame composited over the panorama (the UBB's size).
    composite: Vec<u16>,
    /// The background of a line without video.
    bg: Vec<u16>,
    start: u64,
    /// UBB frames shown.
    shown: u32,
    /// The SPF frame (0x00496104).
    spf_frame: usize,
    /// The 3D head over the UBB.
    head: Option<DialogHead>,
    /// The clock at the head's first frame.
    head_time0: Option<u64>,
    /// The lip-sync track has ended: the neutral mouth.
    head_ended: bool,
    /// The lip-sync ended before the voice started: `FUN_0042ee88` leaves
    /// its loop before drawing (0x0042f47b..0x0042f48c -> 0x0042f6e3).
    head_cut: bool,
    /// The kept screen had an HD layer, which still stands for it.
    kept_hd: bool,
    /// The HD picture of the line (the port's HD mode).
    hd: LineHd,
}

/// The HD picture of a line: the head drawn at the HD size, and whether
/// [`Sys::hd`](crate::sys::Sys::hd) holds the HD version of the composite.
#[derive(Default)]
struct LineHd {
    head: Option<DialogHead>,
    head_pixels: Vec<u16>,
    head_mask: Vec<u8>,
    /// The 640x480 view under this frame's composite, and the composite
    /// before the head.
    pano: Vec<u16>,
    pre_head: Vec<u16>,
    /// The composite before the SPF overlay (the HD layer draws the
    /// overlay over the head, with the character).
    pre_overlay: Vec<u16>,
    /// The pixels the body video drew in this frame (its runs, see
    /// `ubb_ia_coverage`): the character, which a wide picture may move.
    body: Vec<u8>,
    valid: bool,
}

impl LinePlayer {
    /// The view behind the line into `out` (640x480): the kept screen, the
    /// panorama, or black.
    fn background(&self, e: &mut Engine, out: &mut [u16]) {
        if let Some(k) = &self.kept_screen {
            out.copy_from_slice(k);
        } else if e.warp_loaded {
            e.warp.render(&e.warp_surface, out);
        } else {
            out.fill(0);
        }
    }

    /// The HD picture of the composite just drawn (the port's HD mode):
    /// the HD view, the UBB body upscaled, the head drawn again at the HD
    /// size by its copy, animated with the same calls as the head (frame
    /// `frame`, the neutral mouth when `ended`, else the lip-sync at
    /// `t`).
    fn draw_hd(&mut self, e: &mut Engine, tile: usize, frame: i32, ended: bool, t: Option<i32>) {
        let (w, h) = crate::warp::hd_size(tile);
        let mut drawn = false;
        if let (Some(head), true) = (&self.head, self.hd.pre_head.len() == N) {
            let fresh = self
                .hd
                .head
                .as_ref()
                .is_none_or(|hh| hh.output_size() != (w, h));
            if fresh {
                // A copy of the head as it stands after this frame's calls.
                self.hd.head = Some(head.scaled(tile));
            }
            let hh = self.hd.head.as_mut().expect("HD head");
            if !fresh {
                hh.set_frame(frame);
                if ended {
                    hh.neutral();
                } else if let Some(t) = t {
                    hh.lipsync(t);
                }
            }
            hh.transform();
            self.hd.head_pixels.resize(w * h, 0);
            self.hd.head_mask.clear();
            self.hd.head_mask.resize(w * h, 0);
            hh.render_masked(&mut self.hd.head_pixels, w, &mut self.hd.head_mask);
            drawn = true;
        }
        e.hd_view_on_cpu(tile);
        let mut layer = e.sys.hd.borrow_mut();
        crate::hd::compose_dialog(
            &mut layer,
            tile,
            e.warp.side_tiles(),
            &e.hd.view,
            &self.hd.pano,
            &self.hd.pre_head,
            &self.hd.pre_overlay,
            &self.composite,
            &self.hd.body,
            drawn.then_some((&self.hd.head_pixels[..], &self.hd.head_mask[..])),
            e.sys.hd_options().smooth_interface,
        );
        layer.base.copy_from_slice(&self.composite);
        drop(layer);
        e.hd_layer_replaced();
        self.hd.valid = true;
    }

    fn draw_frame(
        &mut self,
        e: &mut Engine,
        font: &Subtitler,
        objects: &[SprSprite],
        sub_lines: &[Vec<u8>],
        voice_started: bool,
    ) {
        if self.talking_head {
            e.animate_sprites();
        } else if self.voice_off_step {
            e.animate_sprites();
            self.voice_off_step = false;
        }
        let have_view = self.kept_screen.is_some() || e.warp_loaded;
        let due = e.sys.millis().saturating_sub(self.start) >= UBB_FRAME_MS * u64::from(self.shown);
        // HD: a full-screen video over the panorama.
        let hd_tile = e
            .sys
            .hd_tile()
            .filter(|_| self.kept_screen.is_none() && e.warp_loaded);
        if let (Some(ubb), true) = (self.ubb.as_ref(), due) {
            let (w, h) = (ubb.width(), ubb.height());
            if self.composite.len() != w * h {
                self.composite = vec![0; w * h];
            }
            // The panorama under the frame, so that the SKIP runs show it
            // (FUN_00451d8b); black when the sizes differ.
            let mut comp = std::mem::take(&mut self.composite);
            if have_view && w == SCREEN_W && h == SCREEN_H {
                self.background(e, &mut comp);
            } else {
                comp.fill(0);
            }
            self.hd.valid = false;
            if let (Some(tile), true) = (hd_tile, w == SCREEN_W && h == SCREEN_H) {
                e.hd_render_view(tile);
                self.hd.pano.clone_from(&comp);
            }
            self.composite = comp;
        }
        let mut decoded = false;
        let mut frame_index = 0;
        if let (Some(ubb), true) = (self.ubb.as_mut(), due) {
            match ubb.next_frame() {
                Ok(f) => {
                    // the frame index in the loop (the original rewinds at
                    // the header's frame count, and the head animation's
                    // index restarts with it)
                    frame_index = f.index;
                    // FUN_00450b08.
                    compose_ubb_ia(
                        &mut self.composite,
                        f.pixels,
                        f.palette,
                        f.ia,
                        blend_ubb_pixel565,
                    );
                    decoded = true;
                    if hd_tile.is_some() {
                        self.hd.body = ubb_ia_coverage(f.ia, self.composite.len());
                    }
                    if hd_tile.is_some() && self.head.is_some() && !self.hd.pano.is_empty() {
                        self.hd.pre_head.clone_from(&self.composite);
                    }
                }
                Err(err) => {
                    log::warn!("dialog video: {err}");
                    self.ubb = None;
                }
            }
        }
        if decoded {
            let (w, h) = self
                .ubb
                .as_ref()
                .map_or((0, 0), |u| (u.width(), u.height()));
            // The 3D head (FUN_0044dd10 with the frame index of the UBB
            // loop, FUN_0044e860 at the ms elapsed since the first frame, or
            // FUN_0044e2e0 once it has returned a negative value, then
            // FUN_0044c9c0), drawn after the UBB composite and before the
            // SPF frame as in FUN_0042ee88's loop.
            let full = w == SCREEN_W && h == SCREEN_H;
            let mut head_t = None;
            let ended_before = self.head_ended;
            if let (Some(head), true) = (self.head.as_mut(), full) {
                head.set_frame(frame_index as i32);
                let now = e.sys.millis();
                let t0 = *self.head_time0.get_or_insert(now);
                if self.head_ended {
                    head.neutral();
                } else {
                    // 0x006ce0e0 is a millisecond clock advanced by 5 at
                    // 200 Hz (FUN_00431150, AIL_set_timer_frequency(200) in
                    // FUN_004310ac): the elapsed time is a multiple of 5
                    let t = (now - t0) / 5 * 5;
                    let t = i32::try_from(t).unwrap_or(i32::MAX);
                    head_t = Some(t);
                    if head.lipsync(t) < 0 {
                        self.head_ended = true;
                    }
                }
                // A negative lip-sync result (kept once it went negative)
                // while the sample status 0x006ce108 is still 0, i.e. before
                // the voice starts, ends the line with nothing drawn
                // (0x0042f47b..0x0042f48c). No shipped .syc ends that early.
                if self.head_ended && !voice_started {
                    self.head_cut = true;
                    return;
                }
                head.transform();
                head.render(&mut self.composite, w);
            }
            // HD: the layer is made once the SPF overlay is drawn (it belongs
            // to the character, which a wide picture may move), from the
            // composite before it and after it.
            let hd_now = hd_tile.filter(|_| full && self.hd.pano.len() == N);
            if hd_now.is_some() {
                self.hd.pre_overlay.clone_from(&self.composite);
            }

            // The SPF overlay: the original draws it into the locked
            // surface, which the full copy of the back buffer (FUN_00410f36
            // @ 0x0042f57e) then overwrites, so it is never seen; drawn over
            // the head with its fix (BugFix::HeadOverlay). The modulo loops the SPF
            // when the UBB outlives it (every shipped pair has 31 frames).
            if let (Some(spf), true) = (&self.spf, e.head_overlay_drawn()) {
                let k = if self.spf_count > 0 {
                    self.spf_frame % self.spf_count
                } else {
                    self.spf_frame
                };
                composite_spf_frame(spf, k, &mut self.composite, w, h);
            }
            if let Some(tile) = hd_now {
                self.draw_hd(e, tile, frame_index as i32, ended_before, head_t);
            }
            // The given object (0x0049619c): its OBJETS1.SPR icon, drawn by
            // the talking-head loop only (0x0042f52d).
            if let Some(spr) = e.given_object.and_then(|i| objects.get(i)) {
                spr.draw(
                    &mut self.composite,
                    w,
                    h,
                    GIVEN_OBJECT_POS.0,
                    GIVEN_OBJECT_POS.1,
                );
            }
            self.spf_frame += 1;
            self.shown += 1;
        }

        let mut screen = e.sys.screen.borrow_mut();
        if let Some(ubb) = &self.ubb {
            let (w, h) = (ubb.width(), ubb.height());
            if self.composite.len() == w * h {
                if self.hd.valid && hd_tile.is_some() {
                    e.sys.hd_mark();
                }
                let mut compose = self.composite.clone();
                // The original draws on the 640x480 screen (FUN_00430178).
                let off = (h as i32 - SCREEN_H as i32) / 2;
                font.draw_subtitles(&mut compose, w, h, sub_lines, off);
                if e.sys.hd_tile().is_some() && !sub_lines.is_empty() {
                    // HD: the screen without the subtitle, for the sharp
                    // one; the lines where blit_centred puts them.
                    blit_centred(&mut screen, &self.composite, w, h);
                    let under = screen.clone();
                    blit_centred(&mut screen, &compose, w, h);
                    let bx = ((SCREEN_W as i32 - w as i32) / 2).max(0);
                    let by = ((SCREEN_H as i32 - h as i32) / 2).max(0);
                    record_subtitles(e, font, &under, &screen, sub_lines, bx, off + by);
                    return;
                }
                blit_centred(&mut screen, &compose, w, h);
                return;
            }
        }
        // No video: the view (or the kept screen, or black) and the
        // subtitle. The voice-only players never draw the given object.
        drop(screen);
        let mut bg = std::mem::take(&mut self.bg);
        if have_view {
            self.background(e, &mut bg);
            if self.kept_screen.is_none() {
                e.hd_show_view_over(Some(&bg));
            } else if self.kept_hd {
                e.sys.hd_mark();
            }
        } else {
            bg.fill(0);
        }
        let under = (e.sys.hd_tile().is_some() && !sub_lines.is_empty()).then(|| bg.clone());
        font.draw_subtitles(&mut bg, SCREEN_W, SCREEN_H, sub_lines, 0);
        if let Some(under) = under {
            record_subtitles(e, font, &under, &bg, sub_lines, 0, 0);
        }
        e.sys.screen.borrow_mut().copy_from_slice(&bg);
        self.bg = bg;
    }
}

impl DialogPresenter for Dialogs {
    fn play_line<'a>(
        &'a mut self,
        e: &'a mut Engine,
        req: &'a LineRequest,
    ) -> HookFuture<'a, LineOutcome> {
        Box::pin(self.line(e, req))
    }

    fn subject_menu<'a>(
        &'a mut self,
        e: &'a mut Engine,
        perso: i32,
        subjects: &'a [u8],
    ) -> HookFuture<'a, Option<i32>> {
        Box::pin(self.subject_menu_impl(e, perso, subjects))
    }

    fn new_conversation(&mut self) {
        self.last_cam = 0;
    }

    fn game_over<'a>(
        &'a mut self,
        e: &'a mut Engine,
        req: &'a GameOverRequest,
    ) -> HookFuture<'a, ()> {
        Box::pin(self.game_over_impl(e, req))
    }
}

#[cfg(test)]
mod off1_tests {
    use std::cell::RefCell;
    use std::future::Future;
    use std::rc::Rc;
    use std::sync::Arc;

    use atlantis_data::disc_image::DiscSet;
    use atlantis_data::{Game, StdDir};
    use md5::{Digest, Md5};

    use super::*;
    use crate::audio::Mixer;
    use crate::con_script::{con_track_letter, ConItem, ConLine, ConScript};
    use crate::hooks::DirectLoader;
    use crate::sys::{Runner, Sys};
    use crate::test_support::button;

    /// The screen buffer FUN_00430884 / FUN_004307d0 were run on under
    /// emulation: byte i = (i * 7 + (i >> 11)) & 0xff over 640 x 480 x 2
    /// bytes; the second IX buffer 0xaa.
    fn pattern_block() -> Vec<u16> {
        let byte = |i: usize| ((i * 7 + (i >> 11)) & 0xff) as u16;
        let mut block: Vec<u16> = (0..N).map(|j| byte(2 * j) | byte(2 * j + 1) << 8).collect();
        block.resize(2 * N, 0xaaaa);
        block
    }

    fn md5_565(px: &[u16]) -> String {
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
        Md5::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The copy of FUN_00430884 / FUN_004307d0 (run under emulation: the
    /// surface at pitch 0x500 after a voice that loads, md5 df9e2805...)
    /// and of atlantis-cd-zh-tw-696832 FUN_0042fe7c (its flat 0x96000-byte
    /// copy gives the same 640-pixel screen).
    #[test]
    fn off1_screen_is_the_first_ix_buffer() {
        let block = pattern_block();
        let screen = off1_screen(&block);
        assert_eq!(md5_565(&block[..N]), "df9e28051d159698bcf01dedcd011fa4");
        assert_eq!(md5_565(&screen), "df9e28051d159698bcf01dedcd011fa4");
        // Before any video: the buffer the port has not filled is zero.
        assert!(off1_screen(&[]).iter().all(|&p| p == 0));
        let short = off1_screen(&block[..100]);
        assert_eq!(&short[..100], &block[..100]);
        assert!(short[100..].iter().all(|&p| p == 0));
    }

    /// The voice gate (FUN_00417cc4 @ 0x00417d33..0x00417d6c): "CRYO_APC"
    /// then "1.20", else no voice and nothing drawn (the emulated cases of
    /// a bad magic and a bad version).
    #[test]
    fn off1_voice_gate() {
        let mut apc = b"CRYO_APC1.20".to_vec();
        for v in [0u32, 22050, 0, 0, 0] {
            apc.extend(v.to_le_bytes());
        }
        assert!(apc_pcm(&apc).is_some());
        let mut magic = apc.clone();
        magic[7] = b'X';
        assert!(apc_pcm(&magic).is_none());
        let mut version = apc.clone();
        version[11] = b'1';
        assert!(apc_pcm(&version).is_none());
    }

    struct Seen {
        frames: usize,
        /// Frames whose pixels outside the subtitle rows were the screen
        /// buffer.
        buffer: usize,
        /// Frames left as the screen stood before the line.
        untouched: usize,
        /// Frames with pixels in the subtitle rows not the screen buffer.
        subtitled: usize,
    }

    /// Runs `task` (a [`play_line`]) frame by frame, a left click after 20
    /// frames: what the frames before it showed, and the LinePlayed
    /// events. `whole_screen`: the screen buffer is looked for on every
    /// row, else outside the subtitle's rows 400..460.
    fn play<Fut: Future<Output = ()> + 'static>(
        sys: Rc<Sys>,
        task: Fut,
        trace: &Rc<RefCell<Vec<Event>>>,
        whole_screen: bool,
    ) -> (Seen, Vec<(bool, usize)>) {
        let block = pattern_block();
        let mut seen = Seen {
            frames: 0,
            buffer: 0,
            untouched: 0,
            subtitled: 0,
        };
        let mut runner = Runner::new(sys.clone(), task);
        let mut frame = 0u64;
        while runner.step(frame * 16) {
            frame += 1;
            assert!(frame < 10_000, "the line did not end");
            if frame == 20 {
                button(&sys, MouseButton::Left, true);
            }
            if frame == 22 {
                button(&sys, MouseButton::Left, false);
            }
            if frame >= 20 {
                continue;
            }
            let s = sys.screen.borrow();
            let rows = |r: std::ops::Range<usize>| r.start * SCREEN_W..r.end * SCREEN_W;
            let outside = |want: &dyn Fn(usize) -> u16| {
                if whole_screen {
                    (0..N).all(|i| s[i] == want(i))
                } else {
                    rows(0..400)
                        .chain(rows(460..SCREEN_H))
                        .all(|i| s[i] == want(i))
                }
            };
            seen.frames += 1;
            if outside(&|i| block[i]) {
                seen.buffer += 1;
                if rows(400..460).any(|i| s[i] != block[i]) {
                    seen.subtitled += 1;
                }
            }
            if s.iter().all(|&p| p == 0x1234) {
                seen.untouched += 1;
            }
        }
        let played = trace
            .borrow()
            .iter()
            .filter_map(|e| match e {
                Event::LinePlayed {
                    voice, subtitles, ..
                } => Some((*voice, *subtitles)),
                _ => None,
            })
            .collect();
        (seen, played)
    }

    /// An engine over `g`, its events traced.
    fn engine(g: Game) -> (Rc<Sys>, Engine, Rc<RefCell<Vec<Event>>>) {
        let sys = Sys::new(Mixer::new());
        let mut e = Engine::new(sys.clone(), g, Rc::new(DirectLoader));
        let trace = Rc::new(RefCell::new(Vec::new()));
        e.trace = Some(trace.clone());
        (sys, e, trace)
    }

    fn request(section: i32, letter: char, line: ConLine) -> LineRequest {
        LineRequest {
            section,
            letter,
            line,
            perso: 514,
            interactive: false,
            inc_perso: 0,
        }
    }

    /// Line `req` over a screen filled with 0x1234, `block` as the IX
    /// buffers.
    async fn play_line(mut e: Engine, req: LineRequest, block: Vec<u16>, subtitles: bool) {
        e.ix_buffers = block;
        e.subtitles = subtitles;
        e.sys.screen.borrow_mut().fill(0x1234);
        let mut d = Dialogs::new();
        d.line(&mut e, &req).await;
    }

    /// An off1 line played by the off1 player on the 2001 build's data
    /// (FUN_00430884; the reference's scripts have no off1 header, the line
    /// is made up): with its voice (CHAPI001 /2 A's file), the screen
    /// buffer under the subtitle until the click; without one (no
    /// ATA9999A.APC), nothing drawn.
    #[test]
    fn off1_line_shows_the_screen_buffer_once_the_voice_loads() {
        for (section, voice) in [(2, true), (9999, false)] {
            let line = ConLine {
                speaker: "Seth".into(),
                params: "off1".into(),
                text: "Off one".into(),
                ..ConLine::default()
            };
            let Some(dir) = std::env::var_os("ATLANTIS_DATA") else {
                eprintln!("ATLANTIS_DATA not set: skipped");
                return;
            };
            let g = Game::open(Arc::new(StdDir::new(dir))).expect("the game directory");
            assert!(g.exe.build.off1_player);
            let (sys, e, trace) = engine(g);
            let task = play_line(e, request(section, 'A', line), pattern_block(), true);
            let (seen, played) = play(sys, task, &trace, false);
            if voice {
                assert_eq!(played, vec![(true, 1)]);
                assert_eq!(seen.buffer, seen.frames, "every frame on the buffer");
                assert_eq!(seen.subtitled, seen.frames, "and the subtitle");
                assert!(seen.frames > 10);
            } else {
                assert_eq!(played, vec![(false, 1)]);
                assert_eq!(seen.untouched, seen.frames);
            }
        }
    }

    /// The off1 lines of a release's scripts, with the section and the
    /// letter of each.
    fn off1_lines(g: &Game) -> Vec<(i32, char, ConLine)> {
        let mut out = Vec::new();
        // The chapters' scripts, in the archives or loose (GOG.com).
        for n in 0..1000 {
            let Some(data) = g.big.read(&format!("SCENAR\\CHAPI{n:03}.CON")) else {
                continue;
            };
            let script = ConScript::parse(&data);
            for sec in script.sections() {
                let lines = sec.items.iter().filter_map(|it| match it {
                    ConItem::Line(l) => Some(l),
                    _ => None,
                });
                for (i, l) in lines.enumerate() {
                    if LineHeader::parse(l).off1 {
                        out.push((sec.id, con_track_letter(i), l.clone()));
                    }
                }
            }
        }
        out
    }

    /// A release's off1 headers (`count` of them); the first one whose
    /// voice is there (or `want`), played with the subtitles off by the
    /// build's off1 player (`ExeBuild::off1_player`), shows the screen
    /// buffer, the whole screen.
    fn release_off1(g: Game, build: &str, count: usize, want: Option<(i32, char)>) {
        assert_eq!(g.exe.build.name, build);
        assert!(g.exe.build.off1_player);
        let lines = off1_lines(&g);
        assert_eq!(lines.len(), count);
        let (section, letter, line) = lines
            .into_iter()
            .find(|(s, l, _)| match want {
                Some(w) => w == (*s, *l),
                None => g.big.contains(&format!("WAV\\ATA{s:04}{l}.APC")),
            })
            .expect("an off1 line with its voice");
        let (sys, e, trace) = engine(g);
        let task = play_line(e, request(section, letter, line), pattern_block(), false);
        let (seen, played) = play(sys, task, &trace, true);
        eprintln!(
            "{build}: /{section} {letter} {} frames, {} on the buffer",
            seen.frames, seen.buffer
        );
        assert_eq!(played, vec![(true, 0)]);
        assert!(seen.frames > 10);
        assert_eq!(seen.buffer, seen.frames);
    }

    /// The Chinese CDs (FUN_0042fe7c): 29 off1 headers each.
    fn chinese_off1(var: &str, build: &str) {
        let Some(dir) = std::env::var_os(var) else {
            eprintln!("{var} not set: skipped");
            return;
        };
        let set = DiscSet::open_folder(dir.as_ref()).expect("CD images");
        let g = Game::open(Arc::new(set)).expect("the game from its images");
        release_off1(g, build, 29, None);
    }

    #[test]
    fn traditional_chinese_off1_line_shows_the_screen_buffer() {
        chinese_off1("ATLANTIS_ISO_ZH_TW", "atlantis-cd-zh-tw-696832");
    }

    #[test]
    fn simplified_chinese_off1_line_shows_the_screen_buffer() {
        chinese_off1("ATLANTIS_BIN_ZH_CN", "atlantis-cd-zh-cn-692736");
    }

    /// The GOG.com release: 30 off1 headers; CHAPI031 /377 A, the line its
    /// executable's off1 player FUN_004307d0 was emulated on, shows the
    /// screen buffer.
    #[test]
    fn gog_off1_line_shows_the_screen_buffer() {
        let Some(dir) = std::env::var_os("ATLANTIS_GOG_FR") else {
            eprintln!("ATLANTIS_GOG_FR not set: skipped");
            return;
        };
        let g = Game::open(Arc::new(StdDir::new(dir))).expect("the game directory");
        release_off1(g, "atlantis-gog-714752", 30, Some((377, 'A')));
    }
}

#[cfg(test)]
mod dos_letters_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use atlantis_data::dvd::{DvdFiles, LANGUAGES};
    use atlantis_data::{Game, StdDir, TextCodePage};

    use super::*;
    use crate::audio::Mixer;
    use crate::hooks::DirectLoader;
    use crate::sys::{Runner, Sys};
    use crate::{BugFix, Enhancements};

    /// The rows "äåö ÄÅÖ" as the Dutch and Swedish DVDs' DOS texts hold
    /// them, drawn by the subtitles of the moment over a black screen: the
    /// code page the font reads them in and the pixels.
    async fn drawn(d: &mut Dialogs, e: &Engine) -> (TextCodePage, TextCodePage, Vec<u16>) {
        let rows = vec![vec![0x84, 0x86, 0x94, b' ', 0x8e, 0x8f, 0x99]];
        let s = d.subtitler(e).await;
        let mut screen = vec![0u16; SCREEN_W * SCREEN_H];
        s.draw_subtitles(&mut screen, SCREEN_W, SCREEN_H, &rows, 0);
        let Subtitler::Gdi(font, _) = &s else {
            panic!("the DVD's subtitles are GDI text")
        };
        (font.code_page(), d.font(e).await.code_page(), screen)
    }

    /// The fix switched on and off on one dialog state: the cached font
    /// and subtitler follow it, the letters drawn change with it and come
    /// back the same.
    #[test]
    fn dos_letters_reach_the_drawn_subtitles() {
        let mut tested = 0;
        for (folder, var) in [
            ("HOL", "ATLANTIS_DVD_A_FILES"),
            ("SUE", "ATLANTIS_DVD_B_FILES"),
        ] {
            let Some(dir) = std::env::var_os(var) else {
                eprintln!("{var} not set: {folder} skipped");
                continue;
            };
            let l = LANGUAGES.iter().find(|l| l.folder == folder).unwrap();
            let files = DvdFiles::new(Arc::new(StdDir::new(dir)), l).expect("the side");
            let g = Game::open(Arc::new(files)).expect("the language");
            let sys = Sys::new(Mixer::new());
            let out = Rc::new(RefCell::new(Vec::new()));
            let o = out.clone();
            let s2 = sys.clone();
            let mut r = Runner::new(sys.clone(), async move {
                let mut e = Engine::new(s2, g, Rc::new(DirectLoader));
                let mut d = Dialogs::new();
                e.set_enhancements(Enhancements::original());
                let a = drawn(&mut d, &e).await;
                e.set_enhancements(Enhancements::only_fix(BugFix::DosLetters));
                let b = drawn(&mut d, &e).await;
                e.set_enhancements(Enhancements::original());
                let c = drawn(&mut d, &e).await;
                o.borrow_mut().extend([a, b, c]);
            });
            let mut now = 0;
            while r.step(now) {
                now += 16;
                assert!(now < 16 * 1000, "{folder}: not drawn");
            }
            let out = out.borrow();
            let [a, b, c] = &out[..] else {
                panic!("{folder}: not drawn")
            };
            assert_eq!(
                (a.0, a.1),
                (TextCodePage::Windows1252, TextCodePage::Windows1252)
            );
            assert_eq!((b.0, b.1), (TextCodePage::Dos850, TextCodePage::Dos850));
            assert_eq!(
                (c.0, c.1),
                (TextCodePage::Windows1252, TextCodePage::Windows1252)
            );
            assert!(a.2.iter().any(|&p| p != 0), "{folder}: nothing drawn");
            assert!(
                b.2.iter().any(|&p| p != 0),
                "{folder}: nothing drawn with the fix"
            );
            assert_ne!(
                a.2, b.2,
                "{folder}: the same letters with and without the fix"
            );
            assert_eq!(
                a.2, c.2,
                "{folder}: the fix switched off draws other letters"
            );
            tested += 1;
        }
        if tested == 0 {
            eprintln!("no DVD side: the DOS letters' drawing not checked");
        }
    }
}

#[cfg(test)]
mod japanese_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use atlantis_data::dvd::{DvdFiles, LANGUAGES};
    use atlantis_data::{Game, StdDir};

    use super::*;
    use crate::audio::Mixer;
    use crate::con_script::{con_track_letter, ConItem, ConLine, ConScript};
    use crate::hooks::DirectLoader;
    use crate::subtitles::split_subtitle;
    use crate::sys::{Runner, Sys};
    use crate::test_support::button;

    fn japanese() -> Option<Game> {
        let Some(dir) = std::env::var_os("ATLANTIS_DVD_B_FILES") else {
            eprintln!("ATLANTIS_DVD_B_FILES not set: skipped");
            return None;
        };
        let l = LANGUAGES.iter().find(|l| l.folder == "JAP").unwrap();
        let files = DvdFiles::new(Arc::new(StdDir::new(dir)), l).expect("side B");
        Some(Game::open(Arc::new(files)).expect("the Japanese DVD"))
    }

    /// The first dialog line of a CON file with a text: its section,
    /// letter and line.
    fn first_line(g: &Game, file: &str) -> (i32, char, ConLine) {
        let con = g.big.read(&format!("SCENAR\\{file}")).unwrap();
        let script = ConScript::parse(&con);
        for sec in script.sections() {
            let lines = sec.items.iter().filter_map(|i| match i {
                ConItem::Line(l) => Some(l),
                _ => None,
            });
            for (k, l) in lines.enumerate() {
                if !l.text.trim().is_empty() {
                    return (sec.id, con_track_letter(k), l.clone());
                }
            }
        }
        panic!("{file}: no dialog line")
    }

    /// The white pixels of the 18-px band from y, x 5..635.
    fn white(s: &[u16], y: i32) -> usize {
        (y.max(0)..(y + 18).min(SCREEN_H as i32))
            .flat_map(|y| (5..635).map(move |x| (x, y)))
            .filter(|&(x, y)| s[y as usize * SCREEN_W + x as usize] == 0xffff)
            .count()
    }

    /// The first line of `file` played by the Japanese build with the
    /// language counter at `clicks` and SUBTITLES `subtitles`: the
    /// subtitle rows it counted, and the most white pixels each band of
    /// `ys` held in a frame.
    fn played(clicks: u8, subtitles: bool, file: &str, ys: &[i32]) -> (usize, Vec<usize>) {
        let g = japanese().unwrap();
        let (section, letter, line) = first_line(&g, file);
        let sys = Sys::new(Mixer::new());
        let mut e = Engine::new(sys.clone(), g, Rc::new(DirectLoader));
        let trace = Rc::new(RefCell::new(Vec::new()));
        e.trace = Some(trace.clone());
        e.subtitle_clicks = clicks;
        e.subtitles = subtitles;
        let req = LineRequest {
            section,
            letter,
            line,
            perso: 514,
            interactive: false,
            inc_perso: 0,
        };
        let task = async move {
            let mut d = Dialogs::new();
            d.line(&mut e, &req).await;
        };
        let mut runner = Runner::new(sys.clone(), task);
        let mut most = vec![0; ys.len()];
        let mut frame = 0u64;
        while runner.step(frame * 16) {
            frame += 1;
            assert!(frame < 20_000, "the line did not end");
            if frame == 200 {
                button(&sys, MouseButton::Left, true);
            }
            if frame == 202 {
                button(&sys, MouseButton::Left, false);
            }
            let s = sys.screen.borrow();
            for (m, &y) in most.iter_mut().zip(ys) {
                *m = (*m).max(white(&s, y));
            }
        }
        let rows = trace
            .borrow()
            .iter()
            .find_map(|e| match e {
                Event::LinePlayed { subtitles, .. } => Some(*subtitles),
                _ => None,
            })
            .expect("the line played");
        (rows, most)
    }

    /// CHAPJ001's first line (rows 66, 42 and an empty one: the golden's)
    /// with the counter at 0 and SUBTITLES off: shown, rows 18 px apart
    /// from the base 0x1f2 (row 0 at 426, row 1 at 444); CHAPI001's first
    /// (one row) at 1: at the reference's base, 0x1ce - 2 * 18; at 2,
    /// SUBTITLES off: nothing.
    #[test]
    fn japanese_and_english_rows_on_the_screen() {
        if japanese().is_none() {
            return;
        }
        let ja = [0x1f2 - 4 * 18, 0x1f2 - 3 * 18, 0x1f2 - 5 * 18];
        let (n, most) = played(0, false, "CHAPJ001.CON", &ja);
        assert_eq!(n, 3);
        assert!(most[0] >= 15 && most[1] >= 15, "{most:?}");
        assert!(most[2] < 15, "a row above: {most:?}");

        let g = japanese().unwrap();
        let (_, _, l) = first_line(&g, "CHAPI001.CON");
        let rows = split_subtitle(latin1_bytes(&l.text).trim_ascii_end());
        assert_eq!(rows.len(), 1);
        let en = [0x1ce - 2 * 18, 0x1ce - 3 * 18];
        let (n, most) = played(1, false, "CHAPI001.CON", &en);
        assert_eq!(n, 1);
        assert!(most[0] >= 15 && most[1] < 15, "{most:?}");

        let (n, most) = played(2, false, "CHAPI001.CON", &en);
        assert_eq!(n, 0);
        assert!(most[0] < 15, "{most:?}");
    }
}
