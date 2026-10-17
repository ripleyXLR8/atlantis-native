//! The presentation of the dialogs: the talking heads over the panorama,
//! the voices, the subtitles, the subject menu and the game-over screen.
//!
//! Translated from ScummVM's CryOmni3D Atlantis engine (`dialog.cpp`
//! `playConSection` (its presentation of each line), `playSingleConLine`,
//! `showSubjectMenu`, `drawGivenObject`; `menus.cpp` `showGameOver`; GPLv3,
//! the ScummVM team), which follow `atlantis.exe`: the section executor
//! `FUN_0042844c`, the line player `FUN_0042ee88`, the voice-only players
//! `FUN_00430258` (`off`, or a line without its `.syc`) and `FUN_004305e8`
//! (`off0`), the subject menu `FUN_004261a0` / `FUN_00425828` and
//! GereGameOver `FUN_00426590`.
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
use atlantis_data::sprite_blend::{blend_ubb_pixel565, compose_ubb_ia};
use atlantis_data::SprSprite;

use crate::audio::VoiceId;
use crate::con::script::latin1_bytes;
use crate::engine::{Engine, Event};
use crate::head::{head_paths, texture_paths, DialogHead};
use crate::hooks::HookFuture;
use crate::music::apc_pcm;
use crate::scripts::{DialogPresenter, GameOverRequest, LineOutcome, LineRequest};
use crate::subtitles::{split_subtitle, SubtitleFont};
use crate::sys::{Input, Key, MouseButton};
use crate::video_skip::VideoSkipState;
use crate::warp::{DIALOG_CAM_FOV_DEG, SCREEN_H, SCREEN_W};

use game_over::{
    game_over_fades_new_sound, game_over_image_name, game_over_subtitle_drawn,
    game_over_voice_name, game_over_wait_ends,
};
use rules::{
    dial_row, head_open_fails, line_view, pick_dial_cam, Angles, DialInit, LineHeader,
    HEAD_OPEN_FAIL_TEXT,
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

/// What a line hands to the line player (`playSingleConLine`'s
/// arguments).
struct SingleLine<'a> {
    section: i32,
    letter: char,
    text: &'a str,
    /// The UBB (`DIALOG\PERSO\<name>.UBB`), none for a voice-only line.
    video: Option<String>,
    /// The camera's SPF overlay (`DIALOG\PERSO\<name>.SPF`).
    spf: Option<String>,
    /// 2..5 for a talking head, 0 for a voice-only line.
    cam: i32,
    /// `off0`: the subtitle over the screen as it stands.
    keep_screen: bool,
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

    fn font(&mut self) -> Rc<SubtitleFont> {
        self.font
            .get_or_insert_with(|| Rc::new(SubtitleFont::new()))
            .clone()
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
        // (FUN_00430884) is used by no shipped CON file.
        let off = header.off;
        let keep_screen = header.off0;
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
        if !voice_only {
            let rec = init.rows.get(&row);
            head_fails = head_open_fails(voice_only, rec.is_some());
            if let Some(r) = rec {
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
            // enhancements.
            log::warn!(
                "section {} line {}: no INITDIAL row {row}",
                req.section,
                req.letter
            );
            if !e.enhancements.ui_ux {
                self.error_box(e, HEAD_OPEN_FAIL_TEXT).await;
            }
        } else {
            video_name.clone_from(&video);
            let single = SingleLine {
                section: req.section,
                letter: req.letter,
                text: &req.line.text,
                dial_row: (!voice_only && video.is_some()).then_some(row),
                video,
                spf,
                cam: if voice_only { 0 } else { cam },
                keep_screen,
                voice_off: voice_only && !keep_screen,
            };
            played = self.play_single(e, &single).await;
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
    /// panorama at the camera's field of view, the voice
    /// `WAV\ATA<section><letter>.APC` and the subtitle, until the voice ends
    /// or a left click (or Escape with a talking head).
    async fn play_single(&mut self, e: &mut Engine, l: &SingleLine<'_>) -> Played {
        let mut out = Played::default();
        let font = self.font();
        let objects = Self::sprites(e, &mut self.object_sprites, "OBJETS1.SPR").await;

        // The camera's FOV (0x00428a0e / 0x00428ad4), put back after.
        let saved_fov = e.warp.hfov();
        let mut fov_changed = false;
        if e.warp_loaded && (2..=5).contains(&l.cam) {
            e.warp.set_hfov(DIALOG_CAM_FOV_DEG[(l.cam - 2) as usize]);
            fov_changed = true;
        }

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

        let sub_lines = if e.subtitles && !l.text.is_empty() {
            split_subtitle(&latin1_bytes(l.text))
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
                let head = self.load_head(e, row, l.cam, &syc).await;
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
        };

        // The line lasts as long as its voice; a talking head (0x00496188
        // set) also ends on Escape (FUN_0041a298 @ 0x0041a3c0), which sets
        // the skip flag 0x00496134.
        let escape_skips = l.cam != 0;
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
                    player.draw_frame(e, &font, &objects, &sub_lines);
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
                    let escape = escape_skips && fi.escape;
                    if escape || fi.left_press || e.sys.buttons().0 {
                        out.escape = escape;
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
        if fov_changed {
            e.warp.set_hfov(saved_fov);
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

    /// The error box of a head that cannot be opened. The original shows a
    /// modal Windows dialog with an OK button; the port draws its own box
    /// over the screen and waits for OK (a click on it, Enter, Space or
    /// Escape).
    async fn error_box(&mut self, e: &Engine, text: &str) {
        let font = self.font();
        let base = e.sys.screen.borrow().clone();
        let bytes = latin1_bytes(text);
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
                Self::draw_arrow(e, &mut s);
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
                          // @ 0x00426616..0x00426647: NEWSOUND set, the NewSound sample fades.
        if game_over_fades_new_sound(e.vars.get("newsound")) {
            e.music.fade_out_new_sound(e.effects_volume());
        }
        let name = format!("IMAGES\\{}", game_over_image_name(req.n));
        let Some(img) = e.read(&name).await.and_then(|d| read_tga15(&d)) else {
            log::warn!("game over: cannot load {name}");
            return;
        };
        let mut frame = vec![0u16; N];
        for y in 0..img.height.min(SCREEN_H) {
            let n = img.width.min(SCREEN_W);
            frame[y * SCREEN_W..y * SCREEN_W + n]
                .copy_from_slice(&img.pixels[y * img.width..y * img.width + n]);
        }
        let narrator = req.narrator.as_ref().filter(|(t, _)| !t.is_empty());
        // The narrator's subtitle (FUN_0042ffe8 / FUN_00430178, as a line's).
        if let (Some((text, _)), true) = (narrator, game_over_subtitle_drawn(e.subtitles)) {
            let font = self.font();
            font.draw_subtitles(
                &mut frame,
                SCREEN_W,
                SCREEN_H,
                &split_subtitle(&latin1_bytes(text)),
                0,
            );
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
        let pcm = match narrator {
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
        if let Some((pcm, apc)) = pcm {
            let v = e.sys.mixer.play(pcm, false, e.voice_volume, 0);
            e.emit(Event::Voice(apc));
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

/// The frame loop state of a line (`playSingleConLine`'s `drawFrame`).
struct LinePlayer {
    talking_head: bool,
    voice_off_step: bool,
    /// off0: the screen as it stood, the subtitle's background.
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

    fn draw_frame(
        &mut self,
        e: &mut Engine,
        font: &SubtitleFont,
        objects: &[SprSprite],
        sub_lines: &[Vec<u8>],
    ) {
        if self.talking_head {
            e.animate_sprites();
        } else if self.voice_off_step {
            e.animate_sprites();
            self.voice_off_step = false;
        }
        let have_view = self.kept_screen.is_some() || e.warp_loaded;
        let due = e.sys.millis().saturating_sub(self.start) >= UBB_FRAME_MS * u64::from(self.shown);
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
            if let (Some(head), true) = (self.head.as_mut(), w == SCREEN_W && h == SCREEN_H) {
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
                    if head.lipsync(i32::try_from(t).unwrap_or(i32::MAX)) < 0 {
                        self.head_ended = true;
                    }
                }
                head.transform();
                head.render(&mut self.composite, w);
            }

            // The SPF overlay: the original draws it into the locked
            // surface, which the full copy of the back buffer (FUN_00410f36
            // @ 0x0042f57e) then overwrites, so it is never seen; drawn over
            // the head with the minor bug fixes. The modulo loops the SPF
            // when the UBB outlives it (every shipped pair has 31 frames).
            if let (Some(spf), true) = (&self.spf, e.enhancements.minor_bug_fixes) {
                let k = if self.spf_count > 0 {
                    self.spf_frame % self.spf_count
                } else {
                    self.spf_frame
                };
                composite_spf_frame(spf, k, &mut self.composite, w, h);
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
                let mut compose = self.composite.clone();
                // The original draws on the 640x480 screen (FUN_00430178).
                let off = (h as i32 - SCREEN_H as i32) / 2;
                font.draw_subtitles(&mut compose, w, h, sub_lines, off);
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
        } else {
            bg.fill(0);
        }
        font.draw_subtitles(&mut bg, SCREEN_W, SCREEN_H, sub_lines, 0);
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
