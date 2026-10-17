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
    MouseMove { x: i32, y: i32 },
    MouseDown { button: MouseButton, x: i32, y: i32 },
    MouseUp { button: MouseButton, x: i32, y: i32 },
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
        })
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
        FrameFuture { sys: self, started: false }
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
        match input {
            Input::MouseMove { x, y } => self.mouse.set((x, y)),
            Input::MouseDown { button, x, y } | Input::MouseUp { button, x, y } => {
                self.mouse.set((x, y));
                let down = matches!(input, Input::MouseDown { .. });
                let (l, r) = self.buttons.get();
                self.buttons.set(match button {
                    MouseButton::Left => (down, r),
                    MouseButton::Right => (l, down),
                });
            }
            Input::Quit => self.quit.set(true),
            _ => {}
        }
        self.events.borrow_mut().push_back(input);
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
        Runner { sys, task: Some(Box::pin(task)) }
    }

    /// Advances the engine to its next frame at time `now_ms`. False once
    /// the engine has returned.
    pub fn step(&mut self, now_ms: u64) -> bool {
        let Some(task) = &mut self.task else { return false };
        self.sys.now_ms.set(now_ms);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            self.sys.yielded.set(false);
            match task.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {
                    self.task = None;
                    return false;
                }
                // A frame was presented: show it.
                Poll::Pending if self.sys.yielded.get() => return true,
                // Pending without a frame cannot happen (the engine awaits
                // nothing else); poll again rather than stall.
                Poll::Pending => continue,
            }
        }
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
