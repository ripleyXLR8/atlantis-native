//! Dialogs: `Dialog2`, the spoken lines in the dialog bar and the
//! conversation view, and `Question2`, the answers, as
//! atlantis4-gog-en-2363910 runs them.
//!
//! **A line** (`DrawText` @ 0x005801d0 → 0x0052bb10 → `pushDialog`
//! @ 0x0052e490): an id missing from the language's text table does
//! nothing; otherwise the line is queued, and shown at once when the voice
//! channel is idle. `showNext` (@ 0x0052de80) with the bar not up shows
//! `dialogLockButton`, puts `sprintf(textFormat, text)` in `text`, plays
//! the line's talking head (`Videos/Dialogs/<v>.ogv`, else a still
//! `2D/Dialogs/<v>.png` / `.jpg`; no video: the `video` sprite is only
//! stopped), plays the voice on the `dialog` channel (`silence5s.ogg` when
//! missing), shows the line's background or the conversation's
//! (`2D/<bg>.png`, else `.jpg`), loads `2D/Dialogs/Character_<n>.png` into
//! `avatar` (empty when missing: the sprite is unloaded first), slides the
//! bar up (`dialogAnimationUp`, 500 ms) and hides `quit`. With the bar up
//! (anchor y ≥ 1) it only slides it down. The voice's end
//! (`onVoiceStopped` @ 0x0052c220) slides the bar down; the down
//! animation's end (@ 0x0052f4e0) stops the video, shows the next line
//! **first**, then calls `OnDialogFinished(id)`. A click on the bar when
//! no bar animation runs stops the voice and the video (@ 0x0052c390).
//! An empty queue shows `quit` in a conversation, else closes the dialog
//! (`HideDialog`, which calls `OnDialogClosed()`); then the
//! conversation's idle video, if any, plays again.
//!
//! **A conversation** (`InitDialog(bg, video)` @ 0x0052e800): background
//! and idle video stored, the queue emptied, `dialogLockButton` and `quit`
//! shown, the background and the idle video put up, then `showNext` if
//! the voice is idle and the bar is not going down. `quit` and
//! `HideDialog` (@ 0x0052d850) stop both bar animations, hide `video`,
//! `background` and `dialogLockButton`, call `OnDialogClosed()` and
//! forget the conversation (the queue stays).
//!
//! **The talking head's end** (`onVideoFinished` @ 0x0052f6f0): `quit`
//! shown; if the voice is idle and the bar is not going down, `showNext`;
//! otherwise, with lines queued or the voice playing, `quit` hidden again;
//! in every case `play()`, which restarts a finished video from its first
//! frame: a talking head shorter than its voice loops until the voice
//! ends, and the idle video loops.
//!
//! **Answers** (`PushAnswer(id, layout)` → `Question2::pushAnswer`
//! @ 0x00567c20): the layout file (`GUI/sujetN.lua`) loaded into its own
//! GUI, its `text` set to `sprintf(textFormat, text)` (an unknown id gives
//! an empty text, the answer is still pushed), its `answer` button added
//! to `questionList` (a list stacked downward, gap at most 0.045 of the
//! window's height, negative when the answers do not fit: they overlap),
//! `lockButton` shown. A validated click on an answer's 110 × 110 icon
//! plays its `validationSound`, calls `OnAnswered(id)`, then removes every
//! answer (`HideAnswers` @ 0x00567550).
//!
//! **Draw order** (`Game::load` @ 0x005294bf..0x00529712): the HUD, the
//! inventory menu, the dialog, the answers, the notification.
//!
//! `[UNKNOWN]` kept narrow here: whether stopping the `video` sprite
//! (a line without a video, a click on the bar, the bar's down end) also
//! raises `onVideoFinished` (taken as no: only a video's own end does);
//! the parent size of `dialogLockButton` (taken as the window, as the
//! emulated geometry golden assumes).

use std::collections::VecDeque;

use atlantis4_data::texts::{texts_path, DialogTable};
use atlantis4_data::video::RgbTable;
use atlantis4_data::xml::to_cp1252;
use atlantis4_data::A4Files;
use atlantis4_script::Value;

use crate::canvas::Canvas;
use crate::gui::text::FontCache;
use crate::gui::{self, Gui, Id, Painter};
use crate::layout::Rect;
use crate::movie::VideoSprite;
use crate::package::Picture;
use crate::sound::Sounds;

/// The dialog screen's layout file.
pub const DIALOG_LUA: &str = "GUI/dialog.lua";
/// The answers' list.
pub const QUESTIONS_LUA: &str = "GUI/questions.lua";
/// `PushAnswer`'s default layout (@ 0x005b5d1c).
pub const ANSWER_LUA: &str = "GUI/answer.lua";

/// A queued line (`pushDialog`'s record).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub id: String,
    /// The text, Windows-1252 bytes.
    pub text: Vec<u8>,
    /// The voice's file name (`Sounds/Dialogs/<voice>`).
    pub voice: String,
    /// The speaker picture's number.
    pub n: i32,
    pub video: String,
    pub background: String,
}

/// What the player's click led to, for the caller to run in this order.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A Lua callback.
    Lua(String, Vec<Value>),
    /// `HideAnswers` after `OnAnswered` (@ 0x00567810).
    HideAnswers,
}

/// The `dialog.lua` objects `Dialog2::load` looks up.
#[derive(Clone, Copy, Debug)]
struct Ids {
    lock: Id,
    background: Id,
    video: Id,
    bar: Id,
    avatar: Id,
    text: Id,
    quit: Id,
    up: Id,
    down: Id,
}

/// One pushed answer.
#[derive(Clone, Debug)]
pub struct Answer {
    pub id: String,
    pub gui: Gui,
    pub button: Id,
}

/// The dialog and answers screens.
#[derive(Default)]
pub struct Dialogs {
    /// `GUI/dialog.lua` (loaded at the first use).
    pub gui: Option<Gui>,
    ids: Option<Ids>,
    /// `GUI/questions.lua`: `lockButton`, `questionList`.
    pub questions: Option<Gui>,
    q: Option<(Id, Id)>,
    pub answers: Vec<Answer>,
    loaded: bool,
    texts: Option<DialogTable>,
    pub queue: VecDeque<Line>,
    pub current: Option<Line>,
    /// Lines whose voice ended (`+0x848`).
    pub finished: Vec<Line>,
    /// `+0x844`.
    pub in_conversation: bool,
    conv_background: String,
    conv_video: String,
    /// The `video` sprite's video.
    pub video: Option<VideoSprite>,
    fonts: FontCache,
    pictures: std::collections::HashMap<String, Option<Picture>>,
    /// Lua callbacks raised, to run in this order.
    callbacks: Vec<(String, Vec<Value>)>,
}

/// What the dialogs act on.
pub struct Ctx<'a> {
    pub files: &'a A4Files,
    pub sounds: &'a mut Sounds,
    pub table: Option<&'a RgbTable>,
    /// The clock, ms.
    pub now: f64,
}

fn s(id: &str) -> Vec<Value> {
    vec![Value::Str(id.as_bytes().to_vec())]
}

impl Dialogs {
    /// Loads the two screens and the text table once (`Dialog2::load`
    /// @ 0x0052fd30, `Question2::load` @ 0x00567d70).
    fn ensure(&mut self, files: &A4Files) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let lang = files.language();
        self.texts = files
            .read(&texts_path(&lang))
            .and_then(|d| DialogTable::parse(&d).ok());
        if self.texts.is_none() {
            log::warn!("no text table {}", texts_path(&lang));
        }
        if let Some(mut g) = Gui::load(files, DIALOG_LUA) {
            let f = |g: &Gui, n: &str| g.find(n);
            let ids = (|| {
                Some(Ids {
                    lock: f(&g, "dialogLockButton")?,
                    background: f(&g, "background")?,
                    video: f(&g, "video")?,
                    bar: f(&g, "dialog")?,
                    avatar: f(&g, "avatar")?,
                    text: f(&g, "text")?,
                    quit: f(&g, "quit")?,
                    up: f(&g, "dialogAnimationUp")?,
                    down: f(&g, "dialogAnimationDown")?,
                })
            })();
            match ids {
                Some(ids) => {
                    // Both animations drive the bar's anchor
                    // (@ 0x005302be..0x005306e2); the lock button starts
                    // hidden (@ 0x0053006a).
                    g.bind(ids.up, ids.bar);
                    g.bind(ids.down, ids.bar);
                    g.node_mut(ids.lock).visible = false;
                    self.ids = Some(ids);
                    self.gui = Some(g);
                }
                None => log::warn!("{DIALOG_LUA}: an object is missing"),
            }
        }
        if let Some(mut g) = Gui::load(files, QUESTIONS_LUA) {
            if let (Some(lock), Some(list)) = (g.find("lockButton"), g.find("questionList")) {
                // Hidden (@ 0x00567e66), and made click-through
                // (@ 0x00567ef5, `+0x580 = 1`): the dialog's `quit`
                // under it can be clicked while topics show.
                g.node_mut(lock).visible = false;
                g.node_mut(lock).click_pass_through = true;
                self.q = Some((lock, list));
                self.questions = Some(g);
            }
        }
    }

    /// The text of a line id (`0x0056a600` on the text table).
    pub fn text_of(&mut self, files: &A4Files, id: &str) -> Option<Vec<u8>> {
        self.ensure(files);
        let t = self.texts.as_ref()?.get(id)?;
        Some(to_cp1252(&t.text))
    }

    /// Whether the dialog view (`dialogLockButton`) is shown.
    pub fn shown(&self) -> bool {
        match (&self.gui, self.ids) {
            (Some(g), Some(ids)) => g.node(ids.lock).visible,
            _ => false,
        }
    }

    /// Whether the answers' `lockButton` is shown.
    pub fn answers_shown(&self) -> bool {
        match (&self.questions, self.q) {
            (Some(g), Some((lock, _))) => g.node(lock).visible,
            _ => false,
        }
    }

    /// The callbacks raised so far, to run in this order.
    pub fn take_callbacks(&mut self) -> Vec<(String, Vec<Value>)> {
        std::mem::take(&mut self.callbacks)
    }

    fn bar_anchor_y(&self) -> f32 {
        match (&self.gui, self.ids) {
            (Some(g), Some(ids)) => g.node(ids.bar).anchor[1],
            _ => -0.1,
        }
    }

    fn down_playing(&self) -> bool {
        match (&self.gui, self.ids) {
            (Some(g), Some(ids)) => g.playing(ids.down),
            _ => false,
        }
    }

    fn up_playing(&self) -> bool {
        match (&self.gui, self.ids) {
            (Some(g), Some(ids)) => g.playing(ids.up),
            _ => false,
        }
    }

    fn set_visible(&mut self, which: fn(&Ids) -> Id, v: bool) {
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            g.node_mut(which(&ids)).visible = v;
        }
    }

    fn play(&mut self, which: fn(&Ids) -> Id, now: f64) {
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            g.play(which(&ids), now);
        }
    }

    /// `DrawText(id, voice, n, video, background)` (@ 0x0052bb10).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        c: &mut Ctx<'_>,
        id: &str,
        voice: &str,
        n: i32,
        video: &str,
        background: &str,
    ) {
        let Some(text) = self.text_of(c.files, id) else {
            log::info!("DrawText({id}): no such line");
            return;
        };
        log::info!("[Dialog2] pushDialog - id: {id}");
        self.queue.push_back(Line {
            id: id.to_string(),
            text,
            voice: voice.to_string(),
            n,
            video: video.to_string(),
            background: background.to_string(),
        });
        if !c.sounds.voice.is_playing() {
            self.show_next(c);
        }
    }

    /// `showNext` @ 0x0052de80.
    fn show_next(&mut self, c: &mut Ctx<'_>) {
        if self.queue.is_empty() {
            if self.in_conversation {
                self.set_visible(|i| i.quit, true);
            } else {
                self.hide_dialog(c);
            }
            if !self.conv_video.is_empty() {
                let v = self.conv_video.clone();
                self.set_line_video(c, &v);
            }
            return;
        }
        // `!(1.0 > anchor.y)` @ 0x0052df15: the bar is up.
        if self.bar_anchor_y() >= 1.0 {
            self.play(|i| i.down, c.now);
            return;
        }
        self.set_visible(|i| i.lock, true);
        let line = self.queue.pop_front().expect("a queued line");
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            let format = g.text_format.clone().unwrap_or_else(|| b"%s".to_vec());
            g.set_text(ids.text, gui::text::format(&format, &line.text), c.now);
        }
        if line.video.is_empty() {
            self.stop_video();
        } else {
            self.set_line_video(c, &line.video);
        }
        c.sounds.play_voice(c.files, &line.voice);
        if !self.set_background(c.files, &line.background) {
            let bg = self.conv_background.clone();
            self.set_background(c.files, &bg);
        }
        // The avatar: unloaded, then the picture if it exists
        // (@ 0x0052e25f..0x0052e2c2, 0x0054c940).
        let avatar = format!("2D/Dialogs/Character_{}.png", line.n);
        let found = gui::resolve(c.files, &avatar).is_some();
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            let a = g.node_mut(ids.avatar);
            a.image = found.then_some(avatar);
            if found {
                if let Some((w, h)) = gui::image_size(c.files, a.image.as_deref().unwrap_or("")) {
                    a.ratio = (f64::from(w) / f64::from(h.max(1))) as f32;
                }
            }
        }
        self.current = Some(line);
        self.play(|i| i.up, c.now);
        self.set_visible(|i| i.quit, false);
    }

    /// The `video` sprite stopped, its picture kept (sprite slot `+0x38`).
    fn stop_video(&mut self) {
        if let Some(v) = self.video.as_mut() {
            v.stop();
        }
    }

    /// `setLineVideo` (@ 0x0052c6c0): `Videos/Dialogs/<v>.ogv` played from
    /// its start, else the still `2D/Dialogs/<v>.png` or `.jpg`.
    fn set_line_video(&mut self, c: &mut Ctx<'_>, name: &str) {
        let Some(ids) = self.ids else { return };
        let path = format!("Videos/Dialogs/{name}.ogv");
        match VideoSprite::open(c.files, &path) {
            Ok(mut v) => {
                v.play(c.now, c.table);
                if let Some(g) = self.gui.as_mut() {
                    let n = g.node_mut(ids.video);
                    n.ratio = v.ratio();
                    n.image = None;
                    n.visible = true;
                }
                self.video = Some(v);
            }
            Err(_) => {
                let still = [".png", ".jpg"]
                    .iter()
                    .map(|e| format!("2D/Dialogs/{name}{e}"))
                    .find(|p| gui::resolve(c.files, p).is_some());
                match still {
                    Some(p) => {
                        self.video = None;
                        if let Some(g) = self.gui.as_mut() {
                            let size = gui::image_size(c.files, &p);
                            let n = g.node_mut(ids.video);
                            if let Some((w, h)) = size {
                                n.ratio = (f64::from(w) / f64::from(h.max(1))) as f32;
                            }
                            n.image = Some(p);
                            n.visible = true;
                        }
                    }
                    None => log::warn!("[Dialog2] no video or picture {name}"),
                }
            }
        }
    }

    /// `setBackground` (@ 0x0052d110): `2D/<bg>.png`, else `.jpg`;
    /// `false` when neither exists.
    fn set_background(&mut self, files: &A4Files, name: &str) -> bool {
        let Some(ids) = self.ids else { return false };
        let found = [".png", ".jpg"]
            .iter()
            .map(|e| format!("2D/{name}{e}"))
            .find(|p| gui::resolve(files, p).is_some());
        let Some(p) = found else {
            if !name.is_empty() {
                log::info!("[Dialog2] can't find bg: {name} (.png or .jpg)");
            }
            return false;
        };
        if let Some(g) = self.gui.as_mut() {
            let n = g.node_mut(ids.background);
            n.image = Some(p);
            n.visible = true;
        }
        true
    }

    /// `InitDialog(background, video)` (@ 0x0052e800).
    pub fn init_dialog(&mut self, c: &mut Ctx<'_>, background: &str, video: &str) {
        self.ensure(c.files);
        self.conv_background = background.to_string();
        self.conv_video = video.to_string();
        self.queue.clear();
        self.in_conversation = true;
        self.set_visible(|i| i.lock, true);
        self.set_visible(|i| i.quit, true);
        self.set_background(c.files, background);
        log::info!("[Dialog2] showDialog - video: {video}");
        if !video.is_empty() {
            self.set_line_video(c, video);
        }
        if !c.sounds.voice.is_playing() && !self.down_playing() {
            self.show_next(c);
        }
    }

    /// `HideDialog` (@ 0x0052d850): `OnDialogClosed()` is raised.
    pub fn hide_dialog(&mut self, _c: &mut Ctx<'_>) {
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            g.stop(ids.up);
            g.stop(ids.down);
            g.node_mut(ids.video).visible = false;
            g.node_mut(ids.background).visible = false;
            g.node_mut(ids.lock).visible = false;
            g.reset_input();
        }
        self.stop_video();
        self.callbacks
            .push(("OnDialogClosed".to_string(), Vec::new()));
        self.in_conversation = false;
        self.conv_background.clear();
        self.conv_video.clear();
    }

    /// The voice channel stopped (`onVoiceStopped` @ 0x0052c220).
    pub fn voice_stopped(&mut self, c: &mut Ctx<'_>) {
        if let Some(l) = self.current.clone() {
            self.finished.push(l);
        }
        self.play(|i| i.down, c.now);
    }

    /// A validated click on the bar (`onBarClicked` @ 0x0052c390): the
    /// voice and the video stopped when no bar animation runs.
    fn bar_clicked(&mut self, c: &mut Ctx<'_>) {
        if self.up_playing() || self.down_playing() {
            return;
        }
        c.sounds.stop_voice();
        self.stop_video();
    }

    /// `onAnimationDownFinished` @ 0x0052f4e0.
    fn down_finished(&mut self, c: &mut Ctx<'_>) {
        let id = self.current.as_ref().map(|l| l.id.clone());
        log::info!(
            "[Dialog2] onAnimationDownFinished - id: {}",
            id.as_deref().unwrap_or("")
        );
        self.stop_video();
        self.show_next(c);
        if let Some(id) = id {
            self.callbacks
                .push(("OnDialogFinished".to_string(), s(&id)));
        }
    }

    /// `onVideoFinished` @ 0x0052f6f0.
    fn video_finished(&mut self, c: &mut Ctx<'_>) {
        self.set_visible(|i| i.quit, true);
        if !c.sounds.voice.is_playing() && !self.down_playing() {
            self.show_next(c);
        } else if !self.queue.is_empty() || c.sounds.voice.is_playing() {
            self.set_visible(|i| i.quit, false);
        }
        if let Some(v) = self.video.as_mut() {
            v.play(c.now, c.table);
        }
    }

    /// `PushAnswer(id, layout)` (@ 0x0052bd70 → 0x00567c20).
    pub fn push_answer(&mut self, c: &mut Ctx<'_>, id: &str, layout: &str) {
        self.ensure(c.files);
        let text = self.text_of(c.files, id).unwrap_or_default();
        let Some(mut g) = Gui::load(c.files, layout) else {
            return;
        };
        let Some(button) = g.find("answer") else {
            log::warn!("{layout}: no answer button");
            return;
        };
        if let Some(t) = g.find("text") {
            let format = g.text_format.clone().unwrap_or_else(|| b"%s".to_vec());
            g.set_text(t, gui::text::format(&format, &text), c.now);
        }
        self.answers.push(Answer {
            id: id.to_string(),
            gui: g,
            button,
        });
        if let (Some(q), Some((lock, _))) = (self.questions.as_mut(), self.q) {
            q.node_mut(lock).visible = true;
        }
    }

    /// `HideAnswers()` (@ 0x00567550).
    pub fn hide_answers(&mut self) {
        if let (Some(q), Some((lock, _))) = (self.questions.as_mut(), self.q) {
            q.node_mut(lock).visible = false;
            q.reset_input();
        }
        self.answers.clear();
    }

    /// The frame's update at `c.now`: the talking head, the bar's
    /// animations and the text's scroll.
    pub fn update(&mut self, c: &mut Ctx<'_>) {
        if let Some(v) = self.video.as_mut() {
            if v.update(c.now, c.table).stopped {
                self.video_finished(c);
            }
        }
        let finished = match self.gui.as_mut() {
            Some(g) => g.tick(c.now),
            None => Vec::new(),
        };
        if let Some(ids) = self.ids {
            if finished.contains(&ids.up) {
                log::debug!("[Dialog2] onAnimationUpFinished");
            }
            if finished.contains(&ids.down) {
                self.down_finished(c);
            }
        }
        for a in &mut self.answers {
            a.gui.tick(c.now);
        }
    }

    /// Geometry and texts for a `width × height` window.
    pub fn layout(&mut self, files: &A4Files, width: u32, height: u32) {
        let win = (width as f32, height as f32);
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            g.update(ids.lock, win, (0.0, 0.0), None);
            g.layout_texts(files, &mut self.fonts);
            // A text's size follows its block: placed again.
            g.update(ids.lock, win, (0.0, 0.0), None);
        }
        if let (Some(q), Some((lock, list))) = (self.questions.as_mut(), self.q) {
            q.update(lock, win, (0.0, 0.0), None);
            let lock_size = q.node(lock).computed_size;
            let s = q.own_size(list, lock_size);
            let n = q.node(list);
            let sizes: Vec<_> = self
                .answers
                .iter()
                .map(|a| a.gui.own_size(a.button, s))
                .collect();
            let (fitted, centres) =
                gui::list_layout(s, &sizes, n.direction, n.minimum_margin, n.maximum_margin);
            let origin = n.world;
            for (a, at) in self.answers.iter_mut().zip(centres) {
                a.gui.update(a.button, fitted, origin, Some(at));
                a.gui.layout_texts(files, &mut self.fonts);
                a.gui.update(a.button, fitted, origin, Some(at));
            }
        }
    }

    fn load_pictures(&mut self, files: &A4Files) {
        let mut paths = Vec::new();
        if let (Some(g), Some(ids)) = (&self.gui, self.ids) {
            g.pictures(ids.lock, &mut paths);
        }
        if self.answers_shown() {
            for a in &self.answers {
                a.gui.pictures(a.button, &mut paths);
            }
        }
        for p in paths {
            self.pictures.entry(p.clone()).or_insert_with(|| {
                let pic = gui::load_picture(files, &p);
                if pic.is_none() {
                    log::warn!("no picture {p}");
                }
                pic
            });
        }
    }

    /// Draws the dialog, then the answers, on `c`.
    pub fn draw_on(&mut self, files: &A4Files, c: &mut dyn Canvas) {
        let (width, height) = c.size();
        if !self.shown() && !self.answers_shown() {
            return;
        }
        self.layout(files, width, height);
        self.load_pictures(files);
        let painter = Paint {
            pictures: &self.pictures,
            video: self.video.as_ref(),
            video_node: self.ids.map(|i| i.video),
        };
        if let (Some(g), Some(ids)) = (&self.gui, self.ids) {
            if g.node(ids.lock).visible {
                g.draw_on(ids.lock, c, &painter);
            }
        }
        if self.answers_shown() {
            let none = Paint {
                pictures: &self.pictures,
                video: None,
                video_node: None,
            };
            for a in &self.answers {
                a.gui.draw_on(a.button, c, &none);
            }
        }
    }

    /// A digest of what [`Dialogs::draw`] shows.
    pub fn hash_into(&self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        self.shown().hash(h);
        self.answers_shown().hash(h);
        if let (Some(g), Some(ids)) = (&self.gui, self.ids) {
            if g.node(ids.lock).visible {
                g.hash_into(ids.lock, h);
                g.node(ids.bar).anchor.map(f32::to_bits).hash(h);
            }
        }
        if let Some(v) = &self.video {
            (&v.path, v.anim.frame, v.picture().is_some()).hash(h);
        }
        if self.answers_shown() {
            for a in &self.answers {
                a.gui.hash_into(a.button, h);
            }
        }
    }

    /// A press at window point (`x`, `y`): the answers, then the dialog,
    /// take it; `true` when consumed.
    pub fn press(&mut self, x: f64, y: f64, width: u32, height: u32) -> bool {
        if self.answers_shown() {
            for a in self.answers.iter_mut().rev() {
                if a.gui.press(a.button, x, y, width, height) {
                    return true;
                }
            }
            if let (Some(q), Some((lock, _))) = (self.questions.as_mut(), self.q) {
                if q.press(lock, x, y, width, height) {
                    return true;
                }
            }
        }
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            if g.node(ids.lock).visible {
                return g.press(ids.lock, x, y, width, height);
            }
        }
        false
    }

    /// The pointer moved: presses and roll-overs.
    pub fn hover(&mut self, x: f64, y: f64, width: u32, height: u32) {
        let mut taken = false;
        if self.answers_shown() {
            for a in self.answers.iter_mut().rev() {
                a.gui.hover(a.button, x, y, width, height, &mut taken);
            }
        }
        if let (Some(g), Some(ids)) = (self.gui.as_mut(), self.ids) {
            if g.node(ids.lock).visible {
                g.hover(ids.lock, x, y, width, height, &mut taken);
            }
        }
    }

    /// A release at (`x`, `y`): `None` when no layer of the dialogs took
    /// it, else what the validated click does, in order.
    pub fn release(
        &mut self,
        c: &mut Ctx<'_>,
        x: f64,
        y: f64,
        width: u32,
        height: u32,
    ) -> Option<Vec<Action>> {
        if self.answers_shown() {
            let mut hit = None;
            for (k, a) in self.answers.iter_mut().enumerate().rev() {
                let (consumed, valid) = a.gui.release(a.button, x, y, width, height);
                if consumed {
                    hit = Some((k, valid));
                    break;
                }
            }
            if let Some((k, valid)) = hit {
                let mut out = Vec::new();
                if valid.is_some() {
                    let a = &self.answers[k];
                    // The button's validation sound first (@ 0x004e52c0).
                    if let Some(snd) = &a.gui.node(a.button).validation_sound {
                        let vol = a.gui.node(a.button).volume_sound;
                        c.sounds.play_free_sound(c.files, snd, vol);
                    }
                    log::info!("answer {}", a.id);
                    out.push(Action::Lua("OnAnswered".to_string(), s(&a.id)));
                    out.push(Action::HideAnswers);
                }
                for a in &mut self.answers {
                    a.gui.reset_input();
                }
                return Some(out);
            }
            if let (Some(q), Some((lock, _))) = (self.questions.as_mut(), self.q) {
                if q.release(lock, x, y, width, height).0 {
                    return Some(Vec::new());
                }
            }
        }
        let ids = self.ids?;
        let g = self.gui.as_mut()?;
        if !g.node(ids.lock).visible {
            return None;
        }
        let (consumed, valid) = g.release(ids.lock, x, y, width, height);
        if let Some(b) = valid {
            if let Some(snd) = g.node(b).validation_sound.clone() {
                let vol = g.node(b).volume_sound;
                c.sounds.play_free_sound(c.files, &snd, vol);
            }
            if b == ids.bar {
                self.bar_clicked(c);
            } else if b == ids.quit {
                self.hide_dialog(c);
            }
        }
        let out = self
            .take_callbacks()
            .into_iter()
            .map(|(n, a)| Action::Lua(n, a))
            .collect();
        consumed.then_some(out)
    }
}

/// Draws the dialog's sprites: pictures by path, the `video` sprite's
/// talking head.
struct Paint<'a> {
    pictures: &'a std::collections::HashMap<String, Option<Picture>>,
    video: Option<&'a VideoSprite>,
    video_node: Option<Id>,
}

impl Painter for Paint<'_> {
    fn picture(&self, path: &str) -> Option<&Picture> {
        self.pictures.get(path).and_then(Option::as_ref)
    }

    fn video(&self, _gui: &Gui, id: Id, c: &mut dyn Canvas, rect: Rect) -> bool {
        if Some(id) != self.video_node {
            return false;
        }
        match self.video {
            Some(v) => {
                v.draw_on(c, rect);
                true
            }
            None => false,
        }
    }
}
