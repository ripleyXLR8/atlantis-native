//! What the engine sees of the platform: the clock, input, the 640x480
//! frame, the cursor and the mixer.
//!
//! The C++ engine is written as blocking loops (play a video until it ends,
//! wait for a click). A browser cannot block, so the engine runs as one
//! async task: where the C++ shows a frame and polls events, the Rust code
//! calls [`Sys::frame`] and awaits it. The front end polls the task once per
//! displayed frame (see [`Runner`]): the loops translate line for line and
//! run the same on desktop and web.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::audio::Mixer;
use crate::warp::{SCREEN_H, SCREEN_W};

/// Input, in 640x480 screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The pointer moved to (x, y) (ignored while the mouse is locked).
    MouseMove {
        x: i32,
        y: i32,
    },
    /// Relative motion of the mouse, in screen pixels (x right, y down):
    /// what the panoramic view turns with (see [`Sys::set_mouse_locked`]).
    MouseDelta {
        dx: i32,
        dy: i32,
    },
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    MouseUp {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    KeyDown(Key),
    KeyUp(Key),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Space,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Char(char),
    F(u8),
}

/// The cursor image: RGB565 pixels with a transparent key, and its hotspot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u16>,
    pub transparent: u16,
    pub hot_x: i32,
    pub hot_y: i32,
}

/// A callback run before each engine step with the time (see
/// [`Sys::add_timer`]).
pub type Timer = Box<dyn FnMut(u64)>;

pub struct Sys {
    /// The frame shown at the next present, RGB565 640x480.
    pub screen: RefCell<Vec<u16>>,
    pub cursor: RefCell<Option<Cursor>>,
    pub mixer: Mixer,
    now_ms: Cell<u64>,
    mouse: Cell<(i32, i32)>,
    buttons: Cell<(bool, bool)>,
    events: RefCell<VecDeque<Input>>,
    quit: Cell<bool>,
    yielded: Cell<bool>,
    rel: Cell<(i32, i32)>,
    locked: Cell<bool>,
    warp_request: Cell<Option<(i32, i32)>>,
    keys_held: RefCell<Vec<Key>>,
    timers: RefCell<Vec<Timer>>,
}

impl Sys {
    pub fn new(mixer: Mixer) -> Rc<Sys> {
        Rc::new(Sys {
            screen: RefCell::new(vec![0; SCREEN_W * SCREEN_H]),
            cursor: RefCell::new(None),
            mixer,
            now_ms: Cell::new(0),
            mouse: Cell::new((SCREEN_W as i32 / 2, SCREEN_H as i32 / 2)),
            buttons: Cell::new((false, false)),
            events: RefCell::new(VecDeque::new()),
            quit: Cell::new(false),
            yielded: Cell::new(false),
            rel: Cell::new((0, 0)),
            locked: Cell::new(false),
            warp_request: Cell::new(None),
            keys_held: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
        })
    }

    /// Runs `f` with the time before each engine step, whatever loop the
    /// engine is in: the original's 200 Hz timer callback (`FUN_00431150`)
    /// runs that way, beside the game's loops.
    pub fn add_timer(&self, f: Timer) {
        self.timers.borrow_mut().push(f);
    }

    /// Milliseconds since the start (the original reads timeGetTime).
    pub fn millis(&self) -> u64 {
        self.now_ms.get()
    }

    pub fn mouse(&self) -> (i32, i32) {
        self.mouse.get()
    }

    /// (left, right) buttons held.
    pub fn buttons(&self) -> (bool, bool) {
        self.buttons.get()
    }

    /// Whether a key is held down (the original reads F5..F8 that way).
    pub fn key_held(&self, key: Key) -> bool {
        self.keys_held.borrow().contains(&key)
    }

    /// The relative mouse motion since the last call (x right, y down).
    pub fn take_mouse_relative(&self) -> (i32, i32) {
        self.rel.replace((0, 0))
    }

    /// Relative mouse mode, for the panoramic view: the front end hides
    /// the system pointer, keeps it in the window and reports
    /// [`Input::MouseDelta`]. The original reads the motion of a pointer it
    /// puts back at the screen centre every frame (SetCursorPos(320, 240));
    /// a locked pointer gives the same deltas without moving the system
    /// pointer. While locked, [`Sys::mouse`] follows the deltas, clamped to
    /// the screen, as the original's pointer would.
    pub fn set_mouse_locked(&self, locked: bool) {
        self.locked.set(locked);
    }

    pub fn mouse_locked(&self) -> bool {
        self.locked.get()
    }

    /// Moves the pointer (the original's SetCursorPos): [`Sys::mouse`] at
    /// once, the system pointer when the front end can move it.
    pub fn set_mouse_pos(&self, x: i32, y: i32) {
        self.mouse.set((x, y));
        self.warp_request.set(Some((x, y)));
    }

    pub fn should_quit(&self) -> bool {
        self.quit.get()
    }

    /// The next input event, if any (mouse position and buttons are
    /// already up to date when the frame resumes).
    pub fn poll_event(&self) -> Option<Input> {
        self.events.borrow_mut().pop_front()
    }

    /// Shows `screen` and waits for the next frame: the engine's
    /// "present and pump events".
    pub fn frame(&self) -> FrameFuture<'_> {
        FrameFuture {
            sys: self,
            started: false,
        }
    }

    /// Waits until `ms` milliseconds have passed, frame by frame.
    pub async fn delay(&self, ms: u64) {
        let end = self.millis() + ms;
        while self.millis() < end && !self.should_quit() {
            self.frame().await;
        }
    }

    // Front-end side.

    pub fn push_input(&self, input: Input) {
        let locked = self.locked.get();
        match input {
            Input::MouseMove { x, y } => {
                if locked {
                    return;
                }
                self.mouse.set((x, y));
            }
            Input::MouseDelta { dx, dy } => {
                let (rx, ry) = self.rel.get();
                self.rel.set((rx.saturating_add(dx), ry.saturating_add(dy)));
                if locked {
                    let (x, y) = self.mouse.get();
                    self.mouse.set((
                        x.saturating_add(dx).clamp(0, SCREEN_W as i32 - 1),
                        y.saturating_add(dy).clamp(0, SCREEN_H as i32 - 1),
                    ));
                }
            }
            Input::MouseDown { button, x, y } | Input::MouseUp { button, x, y } => {
                if !locked {
                    self.mouse.set((x, y));
                }
                let down = matches!(input, Input::MouseDown { .. });
                let (l, r) = self.buttons.get();
                self.buttons.set(match button {
                    MouseButton::Left => (down, r),
                    MouseButton::Right => (l, down),
                });
            }
            Input::Quit => self.quit.set(true),
            Input::KeyDown(k) => {
                let mut held = self.keys_held.borrow_mut();
                if !held.contains(&k) {
                    held.push(k);
                }
            }
            Input::KeyUp(k) => self.keys_held.borrow_mut().retain(|&h| h != k),
        }
        self.events.borrow_mut().push_back(input);
    }

    /// The pointer position the engine asked for since the last call, for
    /// the front end to move the system pointer to.
    pub fn take_warp_request(&self) -> Option<(i32, i32)> {
        self.warp_request.take()
    }
}

pub struct FrameFuture<'a> {
    sys: &'a Sys,
    started: bool,
}

impl Future for FrameFuture<'_> {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.started {
            Poll::Ready(())
        } else {
            self.started = true;
            self.sys.yielded.set(true);
            Poll::Pending
        }
    }
}

/// Drives the engine task: the front end calls [`Runner::step`] once per
/// displayed frame, then shows `sys.screen`.
pub struct Runner {
    pub sys: Rc<Sys>,
    task: Option<Pin<Box<dyn Future<Output = ()>>>>,
}

impl Runner {
    pub fn new(sys: Rc<Sys>, task: impl Future<Output = ()> + 'static) -> Runner {
        Runner {
            sys,
            task: Some(Box::pin(task)),
        }
    }

    /// Advances the engine to its next frame at time `now_ms`. False once
    /// the engine has returned.
    pub fn step(&mut self, now_ms: u64) -> bool {
        let Some(task) = &mut self.task else {
            return false;
        };
        self.sys.now_ms.set(now_ms);
        for t in self.sys.timers.borrow_mut().iter_mut() {
            t(now_ms);
        }
        let mut cx = Context::from_waker(Waker::noop());
        self.sys.yielded.set(false);
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                self.task = None;
                false
            }
            // A frame was presented, or the engine waits for something else
            // (the web build's file reads): show the screen as it is and
            // poll again at the next frame.
            Poll::Pending => true,
        }
    }

    /// Whether the last step presented a new frame (false while the engine
    /// waits for something else than a frame).
    pub fn presented(&self) -> bool {
        self.sys.yielded.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_advances_one_frame_per_step() {
        let sys = Sys::new(Mixer::new());
        let s = sys.clone();
        let mut r = Runner::new(sys.clone(), async move {
            for i in 0..3u16 {
                s.screen.borrow_mut()[0] = i;
                s.frame().await;
            }
        });
        assert!(r.step(0));
        assert_eq!(sys.screen.borrow()[0], 0);
        assert!(r.step(16));
        assert_eq!(sys.screen.borrow()[0], 1);
        assert!(r.step(33));
        assert!(!r.step(50));
    }
}
