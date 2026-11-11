//! Videos on the GUI layer: the `video` sprite of `GUI/InGame.lua` that
//! `PlayMovie` fills, its black background and its skip button, and the
//! video sprites of the other screens (the dialogs' talking heads), as
//! the original plays them (atlantis4-gog-en-2363910).
//!
//! **A video sprite** (`TeSpriteLayout` whose surface, at `+0x550`, holds
//! a codec at `+0x204` and a `TeFrameAnim` at `+0x218`):
//!
//! - `play()` (sprite slot `+0x34` → surface @ 0x004e6170) takes the
//!   codec's frame count and frame rate into the frame animation, then
//!   plays it (@ 0x004f58f0): if it is not playing it starts at the clock's
//!   time (µs / 1000.0) and is updated at once, so frame 0 shows the same
//!   frame. The animation keeps the base's defaults: **1 loop** (the
//!   `TeAnimation` constructor @ 0x004f5c5a), first frame 0, count −1
//!   (`TeFrameAnim`'s @ 0x0056ea4b), no reverse.
//! - Every frame the global animation update (@ 0x004f5950) gives each
//!   playing animation the same `now` (µs / 1000.0); `TeFrameAnim::update`
//!   (@ 0x0056e850, [`FrameAnim`]) computes `k = trunc((now − start) /
//!   1000 · fps)` in doubles: the frame is `k` until `k ≥ N`, when the
//!   animation stops (its "stopped" signal, surface `+0x22c`) and
//!   finishes. The end is time-driven: a slow machine skips frames,
//!   it does not slow the video.
//! - A new frame index ("frame changed", connected @ 0x004e7d91 to
//!   0x004e61b0) makes the codec bring that frame into the texture
//!   (`TeTheora` slot `+0x40` @ 0x004ccdb0): it decodes forward from where
//!   it stands; going back, or jumping 20 frames or more ahead, it seeks
//!   by granule (@ 0x004cb9d0: bisection over the file for a page between
//!   20 and 0 frames before `frame − 2^kfgshift`, or a rewind for files of
//!   4 KiB or less) and drops packets until a key frame within the last
//!   `2^kfgshift` frames before the target. A Theora key frame is at most
//!   `2^kfgshift` frames behind any frame (the granule's offset field), so
//!   the picture is always the one decoding every packet in order gives;
//!   here the decoder simply decodes forward, or rewinds and decodes
//!   forward ([`atlantis4_data::video::Video::seek_rgba`]).
//! - `stop()` (sprite slot `+0x38` → @ 0x004f5ab0): if playing, stops and
//!   emits "stopped".
//!
//! **`PlayMovie(video, sound)`** (binding @ 0x0057e160, worker
//! @ 0x005232b0, in this order): `videoBackgroundButton` shown,
//! `skipVideoButton` **hidden**, the music stopped, the sound started on
//! the `video` channel, the `video` sprite loaded with the video path,
//! shown and played; the game's "not in a movie" flag (`game+0x4038`)
//! cleared. While it is clear the per-frame view update (@ 0x0052ab40:
//! mouse look, edge turning) does not run.
//!
//! **Input** (`Game::enter` connects @ 0x00529177..0x005293a5): the
//! background button (`1.01 × 1.01` of the window, under the video) and
//! the skip button are `TeButtonLayout`s, which take every press and
//! release that hits them, so no click reaches the warp. A validated
//! click on the background (@ 0x00522180) **toggles** the skip button's
//! visibility; one on the skip button (@ 0x005220b0) stops the video. No
//! key skips: the skip function's only reference is that connection.
//!
//! **The end** (@ 0x00524740, on the sprite's "stopped" signal: at the
//! last frame's end or on a skip): the background, the skip button and the
//! video hidden, the `video` channel stopped, the music played again (from
//! its start), the flag set, then `OnMovieFinished(<the surface's path>)`
//! (`"Videos/intro.ogv"` for the intro in the original's live trace).
//!
//! **Geometry** (`GUI/HD/InGame.lua`, inside `inGame`, `Percent` 1 × 1 of
//! the window): `videoBackgroundButton` `Percent` 1.01 × 1.01 with
//! `2D/BlackScreen.png` stretched over it; `video` `Percent` 1.01 × 1.01,
//! `LetterBox` with the picture's ratio (the load sets `ratio = w / h`);
//! both centred. `skipVideoButton` anchor (0, 0) at (0.90, 0.01), as large
//! as its `upLayout` `2D/btn_back.png` (75 × 75, a `Pixel` sprite: drawn
//! with its translation truncated), `downLayout` `2D/btn_backOver.png`
//! while pressed. Children draw in declaration order: background, video,
//! skip button, all over the warp and the markers, under the cursor.

use atlantis4_data::video::{alpha_name, RgbTable, Video, VideoError};
use atlantis4_data::A4Files;

use crate::frame_anim::{Events, FrameAnim};
use crate::layout::{
    percent_position, percent_size, pixel_snap, window_rect, RatioMode, Rect, SAFE_AREA_RATIO,
};
use crate::package::Picture;

/// `videoBackgroundButton`'s `upLayout` picture.
pub const BACKGROUND_PICTURE: &str = "2D/BlackScreen.png";
/// `skipVideoButton`'s `upLayout` picture.
pub const SKIP_PICTURE: &str = "2D/btn_back.png";
/// `skipVideoButton`'s `downLayout` picture.
pub const SKIP_DOWN_PICTURE: &str = "2D/btn_backOver.png";
/// The size of the background and of the video's box (`Percent`).
pub const LAYER_SIZE: (f32, f32) = (1.01, 1.01);
/// The skip button's position (`Percent`) and anchor.
pub const SKIP_POSITION: (f32, f32) = (0.90, 0.01);
pub const SKIP_ANCHOR: (f32, f32) = (0.0, 0.0);

/// The folders the install keeps a file's platform or definition variant
/// in, tried in this order when the path as written is missing.
///
/// `[UNKNOWN]` the resolver's rule (@ 0x00552bd0, virtual calls through
/// the version "definitions"); this is the narrowest reading of the
/// data: every `PlayMovie` video of the scripts is found in
/// `Videos/PC-MacOSX/` (68 of 70; the 2 others exist nowhere), the skip
/// button's picture only in `2D/HD/`.
pub const VARIANT_DIRS: [&str; 2] = ["PC-MacOSX", "HD"];

/// The file a logical path stands for: as written, else with
/// `PC-MacOSX/` or `HD/` before the file name ([`VARIANT_DIRS`]).
pub fn resolve(files: &A4Files, path: &str) -> Option<String> {
    if files.source().size(path).is_some() {
        return Some(path.to_string());
    }
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    VARIANT_DIRS.iter().find_map(|v| {
        let p = if dir.is_empty() {
            format!("{v}/{file}")
        } else {
            format!("{dir}/{v}/{file}")
        };
        files.source().size(&p).is_some().then_some(p)
    })
}

/// Why a video sprite has nothing to play.
#[derive(Debug)]
pub enum SpriteError {
    /// No such file (after [`resolve`]).
    Missing(String),
    /// The file is not a video the codec reads.
    Video(String, VideoError),
}

impl std::fmt::Display for SpriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpriteError::Missing(p) => write!(f, "no video {p}"),
            SpriteError::Video(p, e) => write!(f, "video {p}: {e}"),
        }
    }
}

impl std::error::Error for SpriteError {}

/// A sprite's video: its codec, its frame animation and the texture.
pub struct VideoSprite {
    /// The path the sprite was given (what `OnMovieFinished` receives).
    pub path: String,
    /// Boxed: the decoder's state is large.
    video: Box<Video>,
    /// `surface+0x218`.
    pub anim: FrameAnim,
    /// The texture's picture: the last frame brought in (RGBA, rows
    /// top-down), or nothing before the first.
    picture: Option<Picture>,
    /// The texture's margin right of and below the picture: the first
    /// frame's last column and last row (the texture is made with the
    /// first frame and a 4-texel margin repeating its edge,
    /// @ 0x004e9210..0x004e95e7; later frames replace the picture's
    /// rectangle only, `glTexSubImage2D` of the picture's size
    /// @ 0x004e9fbf; `[INFERRED]` from those arguments).
    margin: Option<Margin>,
}

/// A texture margin: the column right of the picture, the row below it.
type Margin = (Vec<[u8; 4]>, Vec<[u8; 4]>);

impl std::fmt::Debug for VideoSprite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoSprite")
            .field("path", &self.path)
            .field("video", &self.video)
            .field("anim", &self.anim)
            .finish()
    }
}

impl VideoSprite {
    /// The sprite loaded with `path` (`X.ogv`, paired with `X.alpha.ogv`
    /// when that file exists, the video factory @ 0x00551de0).
    pub fn open(files: &A4Files, path: &str) -> Result<VideoSprite, SpriteError> {
        let file = resolve(files, path).ok_or_else(|| SpriteError::Missing(path.into()))?;
        let colour = files
            .read(&file)
            .ok_or_else(|| SpriteError::Missing(file.clone()))?;
        let alpha = alpha_name(&file).and_then(|a| files.read(&a));
        let video = Box::new(
            Video::open(&colour, alpha.as_deref()).map_err(|e| SpriteError::Video(file, e))?,
        );
        let (num, den) = video.fps();
        let anim = FrameAnim {
            playing: false,
            start_ms: 0.0,
            loops: 1,
            reverse: false,
            n: video.frame_count() as u32,
            fps: codec_fps(num, den),
            first: 0,
            count: u32::MAX,
            frame: -1,
        };
        Ok(VideoSprite {
            path: path.to_string(),
            video,
            anim,
            picture: None,
            margin: None,
        })
    }

    /// The picture's width and height.
    pub fn size(&self) -> (u32, u32) {
        self.video.size()
    }

    /// The layout's `ratio` after the load: width / height as a float
    /// (1.0 for a height of 0).
    pub fn ratio(&self) -> f32 {
        let (w, h) = self.size();
        if h == 0 {
            1.0
        } else {
            (f64::from(w) / f64::from(h)) as f32
        }
    }

    pub fn has_alpha(&self) -> bool {
        self.video.has_alpha()
    }

    /// `play()` at `now_ms` (the clock's µs / 1000.0): a stopped
    /// animation starts there and shows its frame for that time.
    pub fn play(&mut self, now_ms: f64, table: Option<&RgbTable>) -> Events {
        self.anim.n = self.video.frame_count() as u32;
        if self.anim.playing {
            return Events::default();
        }
        self.anim.playing = true;
        self.anim.start_ms = now_ms;
        self.update(now_ms, table)
    }

    /// `stop()`: `true` if it was playing (the "stopped" signal).
    pub fn stop(&mut self) -> bool {
        std::mem::replace(&mut self.anim.playing, false)
    }

    /// The frame animation at `now_ms`, and the new frame brought into
    /// the texture.
    pub fn update(&mut self, now_ms: f64, table: Option<&RgbTable>) -> Events {
        if !self.anim.playing {
            return Events::default();
        }
        let ev = self.anim.update(now_ms);
        if ev.changed {
            self.bring(self.anim.frame, table);
        }
        ev
    }

    /// `TeTheora`'s update to `frame`: nothing for a frame past the end.
    fn bring(&mut self, frame: i32, table: Option<&RgbTable>) {
        let Some(table) = table else {
            log::warn!(
                "{}: no video colour table, frame {frame} not shown",
                self.path
            );
            return;
        };
        let Ok(frame) = usize::try_from(frame) else {
            return;
        };
        let (w, h) = self.size();
        let mut rgba = vec![0u8; 4 * w as usize * h as usize];
        match self.video.seek_rgba(table, frame, &mut rgba) {
            Ok(true) => {
                let pic = Picture {
                    width: w,
                    height: h,
                    rgba,
                };
                if self.margin.is_none() && w > 0 && h > 0 {
                    let col = (0..h)
                        .map(|y| pic.texel(i64::from(w) - 1, i64::from(y)))
                        .collect();
                    let row = (0..w)
                        .map(|x| pic.texel(i64::from(x), i64::from(h) - 1))
                        .collect();
                    self.margin = Some((col, row));
                }
                self.picture = Some(pic);
            }
            Ok(false) => {}
            Err(e) => log::warn!("{}: frame {frame}: {e}", self.path),
        }
    }

    /// The texture's picture, if a frame was brought in.
    pub fn picture(&self) -> Option<&Picture> {
        self.picture.as_ref()
    }

    /// A tap of the texture at (`x`, `y`) of the picture: clamped at the
    /// left and top edges (the texture's own), the first frame's edge in
    /// the margin right and below.
    fn tap(&self, pic: &Picture, x: i64, y: i64) -> [u8; 4] {
        let (w, h) = (i64::from(pic.width), i64::from(pic.height));
        let (x, y) = (x.max(0), y.max(0));
        match &self.margin {
            Some((col, row)) if x >= w || y >= h => {
                if x >= w && y >= h {
                    col[col.len() - 1]
                } else if x >= w {
                    col[y as usize]
                } else {
                    row[x as usize]
                }
            }
            _ => pic.texel_clamped(x, y),
        }
    }
}

/// `TeTheora`'s frame rate (@ 0x004cb4b0): numerator / denominator, as
/// unsigned integers, divided on the x87 and stored as a float.
pub fn codec_fps(num: u32, den: u32) -> f32 {
    (f64::from(num) / f64::from(den)) as f32
}

/// GL's `GL_LINEAR` of four taps at fractions (`a`, `b`), rounded to 8
/// bits per channel.
fn bilinear(t: [[u8; 4]; 4], a: f64, b: f64) -> [u8; 4] {
    let w = [(1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b];
    std::array::from_fn(|c| {
        let v: f64 = t.iter().zip(&w).map(|(p, w)| f64::from(p[c]) * w).sum();
        (v + 0.5).floor().clamp(0.0, 255.0) as u8
    })
}

/// How a quad's colour reaches the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Material mode 0 (`GL_COMBINE`, alpha 1): the full-screen pictures
    /// and videos (`GL_RGB` textures) cover what is under them.
    Opaque,
    /// Mode 1 (`GL_MODULATE`, white) with `GL_SRC_ALPHA,
    /// GL_ONE_MINUS_SRC_ALPHA`: the GUI's sprites, the dialogs' videos.
    Blend,
}

/// Draws a picture stretched over `rect` (window pixels) into `out`
/// (`width × height` XRGB, rows top-down): every pixel whose centre is
/// inside samples the picture at the matching point (texel centres at
/// half-integers of the picture), `GL_LINEAR`, with `tap` giving the
/// texture beyond the picture.
#[allow(clippy::too_many_arguments)]
pub fn draw_stretched(
    out: &mut [u32],
    width: u32,
    height: u32,
    pic: &Picture,
    rect: Rect,
    mode: Mode,
    tap: &dyn Fn(i64, i64) -> [u8; 4],
) {
    if pic.width == 0 || pic.height == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let (sx, sy) = (
        f64::from(pic.width) / rect.width,
        f64::from(pic.height) / rect.height,
    );
    let i0 = (rect.left - 0.5).ceil().max(0.0) as i64;
    let i1 = ((rect.left + rect.width - 0.5).ceil() as i64).min(i64::from(width));
    let j0 = (rect.top - 0.5).ceil().max(0.0) as i64;
    let j1 = ((rect.top + rect.height - 0.5).ceil() as i64).min(i64::from(height));
    for j in j0..j1 {
        let ty = (j as f64 + 0.5 - rect.top) * sy - 0.5;
        let (fy, b) = (ty.floor(), ty - ty.floor());
        let y0 = fy as i64;
        for i in i0..i1 {
            let tx = (i as f64 + 0.5 - rect.left) * sx - 0.5;
            let (fx, a) = (tx.floor(), tx - tx.floor());
            let x0 = fx as i64;
            let c = bilinear(
                [
                    tap(x0, y0),
                    tap(x0 + 1, y0),
                    tap(x0, y0 + 1),
                    tap(x0 + 1, y0 + 1),
                ],
                a,
                b,
            );
            let px = &mut out[j as usize * width as usize + i as usize];
            *px = match mode {
                Mode::Opaque => u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]),
                Mode::Blend => crate::sprite::blend(*px, c),
            };
        }
    }
}

/// Where the movie's layouts land in a `width × height` window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovieLayout {
    pub background: Rect,
    pub video: Rect,
    /// The skip button's hit rectangle (its layout's).
    pub skip: Rect,
    /// Where its picture is drawn (translation truncated).
    pub skip_drawn: Rect,
}

impl MovieLayout {
    /// The layouts for a picture `video` (w, h) and a skip picture
    /// `skip` (w, h) in a `width × height` window.
    pub fn new(width: u32, height: u32, video: (u32, u32), skip: (u32, u32)) -> MovieLayout {
        let win = (width as f32, height as f32);
        let bg = percent_size(LAYER_SIZE, win, 1.0, RatioMode::None, SAFE_AREA_RATIO);
        let centre = percent_position((0.5, 0.5), (0.5, 0.5), bg, win);
        let ratio = if video.1 == 0 {
            1.0
        } else {
            (f64::from(video.0) / f64::from(video.1)) as f32
        };
        let vs = percent_size(
            LAYER_SIZE,
            win,
            ratio,
            RatioMode::LetterBox,
            SAFE_AREA_RATIO,
        );
        let vc = percent_position((0.5, 0.5), (0.5, 0.5), vs, win);
        let ss = (skip.0 as f32, skip.1 as f32);
        let sc = percent_position(SKIP_POSITION, SKIP_ANCHOR, ss, win);
        MovieLayout {
            background: window_rect(centre, bg, width, height),
            video: window_rect(vc, vs, width, height),
            skip: window_rect(sc, ss, width, height),
            skip_drawn: window_rect(pixel_snap(sc), ss, width, height),
        }
    }
}

/// What the original's audio does at a movie's start and end, for the
/// sound side to play (`TeMusic` on `Application+0` and `game+0x3f8c`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MovieCue {
    /// The music stopped, `sound` played once on the `video` channel at
    /// volume 1.0 (path as the script gives it).
    Start { sound: String },
    /// The `video` channel stopped, the music played again from its
    /// start.
    End,
}

/// A `PlayMovie` in progress.
#[derive(Debug)]
pub struct Movie {
    /// The `video` sprite (`None`: the file could not be loaded; the
    /// original then has no codec, `play()` does nothing and the movie
    /// never ends by itself, `[INFERRED]` from 0x004e6170's null test).
    pub sprite: Option<VideoSprite>,
    /// The path given to `PlayMovie`.
    pub path: String,
    /// `skipVideoButton` shown.
    pub skip_shown: bool,
    /// A press is held on the skip button (its down picture).
    pub skip_pressed: bool,
    /// Where the press went: the skip button, the background.
    press: Option<Press>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Skip,
    Background,
}

/// What a click did to the movie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Nothing validated.
    None,
    /// The skip button was shown or hidden.
    Toggled,
    /// The skip button stopped the video: the movie ends.
    Skipped,
}

impl Movie {
    /// The worker's sprite part: load `path`, play at `now_ms`.
    pub fn start(files: &A4Files, path: &str, now_ms: f64, table: Option<&RgbTable>) -> Movie {
        let sprite = match VideoSprite::open(files, path) {
            Ok(mut s) => {
                s.play(now_ms, table);
                Some(s)
            }
            Err(e) => {
                log::warn!("PlayMovie: {e}: nothing plays and the movie does not end by itself");
                None
            }
        };
        Movie {
            sprite,
            path: path.to_string(),
            skip_shown: false,
            skip_pressed: false,
            press: None,
        }
    }

    /// The frame at `now_ms`; `true` when the video stopped (its end).
    pub fn update(&mut self, now_ms: f64, table: Option<&RgbTable>) -> bool {
        self.sprite
            .as_mut()
            .is_some_and(|s| s.update(now_ms, table).stopped)
    }

    /// The surface's path, for `OnMovieFinished`.
    pub fn finished_name(&self) -> String {
        self.sprite
            .as_ref()
            .map_or_else(|| self.path.clone(), |s| s.path.clone())
    }

    /// A press at window point (`x`, `y`): the topmost visible button hit
    /// takes it (the skip button above the background).
    pub fn press(&mut self, layout: &MovieLayout, x: f64, y: f64) {
        self.press = if self.skip_shown && layout.skip.contains(x, y) {
            self.skip_pressed = true;
            Some(Press::Skip)
        } else if layout.background.contains(x, y) {
            Some(Press::Background)
        } else {
            None
        };
    }

    /// The pointer moved while pressed: the skip button shows its down
    /// picture while the pointer is inside.
    pub fn drag(&mut self, layout: &MovieLayout, x: f64, y: f64) {
        if self.press == Some(Press::Skip) {
            self.skip_pressed = layout.skip.contains(x, y);
        }
    }

    /// A release at (`x`, `y`): a press and a release on the same button
    /// validate its click.
    pub fn release(&mut self, layout: &MovieLayout, x: f64, y: f64) -> Click {
        let press = self.press.take();
        self.skip_pressed = false;
        match press {
            Some(Press::Skip) if self.skip_shown && layout.skip.contains(x, y) => {
                let stopped = self.sprite.as_mut().is_some_and(VideoSprite::stop);
                if stopped {
                    Click::Skipped
                } else {
                    Click::None
                }
            }
            Some(Press::Background) if layout.background.contains(x, y) => {
                // The skip button was not hit by this release while
                // shown, or it is hidden: the background takes it.
                if self.skip_shown && layout.skip.contains(x, y) {
                    return Click::None;
                }
                self.skip_shown = !self.skip_shown;
                Click::Toggled
            }
            _ => Click::None,
        }
    }

    /// Whether the opaque background covers every pixel of a `width ×
    /// height` window (then nothing under it shows).
    pub fn covers(&self, width: u32, height: u32) -> bool {
        let b = self.layout(width, height, (75, 75)).background;
        b.left <= 0.5
            && b.top <= 0.5
            && b.left + b.width > f64::from(width) - 0.5
            && b.top + b.height > f64::from(height) - 0.5
    }

    /// The movie's layouts in a `width × height` window.
    pub fn layout(&self, width: u32, height: u32, skip: (u32, u32)) -> MovieLayout {
        let size = self.sprite.as_ref().map_or((1, 1), VideoSprite::size);
        MovieLayout::new(width, height, size, skip)
    }

    /// Draws the background, the video and, when shown, the skip button
    /// (`skip_up` / `skip_down`), in that order.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        out: &mut [u32],
        width: u32,
        height: u32,
        background: Option<&Picture>,
        skip_up: Option<&Picture>,
        skip_down: Option<&Picture>,
    ) {
        let skip_size = skip_up.map_or((75, 75), |p| (p.width, p.height));
        let l = self.layout(width, height, skip_size);
        match background {
            Some(bg) => draw_stretched(
                out,
                width,
                height,
                bg,
                l.background,
                Mode::Opaque,
                &|x, y| bg.texel_clamped(x, y),
            ),
            None => fill(out, width, height, l.background, 0),
        }
        if let Some(s) = &self.sprite {
            if let Some(pic) = s.picture() {
                let mode = if s.has_alpha() {
                    Mode::Blend
                } else {
                    Mode::Opaque
                };
                draw_stretched(out, width, height, pic, l.video, mode, &|x, y| {
                    s.tap(pic, x, y)
                });
            }
        }
        if self.skip_shown {
            let pic = if self.skip_pressed {
                skip_down.or(skip_up)
            } else {
                skip_up
            };
            if let Some(p) = pic {
                draw_stretched(out, width, height, p, l.skip_drawn, Mode::Blend, &|x, y| {
                    p.texel_clamped(x, y)
                });
            }
        }
    }
}

/// Fills the pixels whose centre is in `rect` with `colour`.
fn fill(out: &mut [u32], width: u32, height: u32, rect: Rect, colour: u32) {
    let i0 = (rect.left - 0.5).ceil().max(0.0) as i64;
    let i1 = ((rect.left + rect.width - 0.5).ceil() as i64).min(i64::from(width));
    let j0 = (rect.top - 0.5).ceil().max(0.0) as i64;
    let j1 = ((rect.top + rect.height - 0.5).ceil() as i64).min(i64::from(height));
    for j in j0..j1 {
        for i in i0..i1 {
            out[j as usize * width as usize + i as usize] = colour;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_intro_covers_the_window_and_the_skip_button_sits_top_right() {
        let l = MovieLayout::new(1365, 768, (800, 448), (75, 75));
        // 1.01 × 1365 wide, letter-boxed to 800 / 448.
        assert!((l.video.width - 1_378.650_02).abs() < 1e-4);
        assert!((l.video.height - 772.044_006).abs() < 1e-4);
        assert!(l.video.left < 0.0 && l.video.top < 0.0);
        assert!(l.background.contains(0.5, 0.5) && l.background.contains(1364.5, 767.5));
        // Centre (583.499939, −338.820007) truncated to (583, −338).
        assert_eq!((l.skip_drawn.left, l.skip_drawn.top), (1228.0, 8.5));
        assert!(l.skip.contains(1266.0, 37.0));
    }

    #[test]
    fn a_click_on_the_background_shows_the_skip_button_and_a_second_hides_it() {
        let l = MovieLayout::new(1365, 768, (800, 448), (75, 75));
        let mut m = Movie {
            sprite: None,
            path: "Videos/x.ogv".into(),
            skip_shown: false,
            skip_pressed: false,
            press: None,
        };
        // On the hidden skip button: the background takes it.
        m.press(&l, 1266.0, 37.0);
        assert_eq!(m.release(&l, 1266.0, 37.0), Click::Toggled);
        assert!(m.skip_shown);
        m.press(&l, 100.0, 100.0);
        assert_eq!(m.release(&l, 100.0, 100.0), Click::Toggled);
        assert!(!m.skip_shown);
        // A press inside and a release outside the skip button: nothing.
        m.skip_shown = true;
        m.press(&l, 1266.0, 37.0);
        assert!(m.skip_pressed);
        assert_eq!(m.release(&l, 600.0, 400.0), Click::None);
        assert!(m.skip_shown);
    }

    #[test]
    fn codec_rates_are_the_quotient_as_a_float() {
        assert_eq!(codec_fps(25, 1), 25.0);
        assert_eq!(codec_fps(30_000, 1_001), (30_000.0f64 / 1_001.0) as f32);
    }
}
