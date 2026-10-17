//! A watchdog for hangs (desktop): each event handler records what it is
//! doing; a thread logs it when the window has not come back to the
//! system for 2 s (Windows closes a window that does not answer for ~5 s,
//! and the log is then the only trace).

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Idle = 0,
    EngineStep,
    Redraw,
    WindowEvent,
    DeviceEvent,
    AboutToWait,
    Resumed,
}

const PHASES: [Phase; 7] = [
    Phase::Idle,
    Phase::EngineStep,
    Phase::Redraw,
    Phase::WindowEvent,
    Phase::DeviceEvent,
    Phase::AboutToWait,
    Phase::Resumed,
];

static START: OnceLock<Instant> = OnceLock::new();
static BEAT_MS: AtomicU64 = AtomicU64::new(0);
static PHASE: AtomicU8 = AtomicU8::new(0);
static AUDIO_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Marks the start of `phase` on the event-loop thread.
pub fn enter(phase: Phase) {
    BEAT_MS.store(now_ms(), Ordering::Relaxed);
    PHASE.store(phase as u8, Ordering::Relaxed);
}

/// Marks that the event-loop thread is back with the system.
pub fn leave() {
    enter(Phase::Idle);
}

/// Marks one audio callback.
pub fn audio_beat() {
    AUDIO_MS.store(now_ms(), Ordering::Relaxed);
}

/// Starts the watchdog thread.
pub fn start() {
    leave();
    std::thread::spawn(|| {
        let mut reported = false;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let now = now_ms();
            let beat = BEAT_MS.load(Ordering::Relaxed);
            let phase = PHASES[usize::from(PHASE.load(Ordering::Relaxed)).min(PHASES.len() - 1)];
            // Idle with no beat is the window waiting for events: fine.
            let stuck = phase != Phase::Idle && now.saturating_sub(beat) > 2000;
            if stuck && !reported {
                let audio = AUDIO_MS.load(Ordering::Relaxed);
                log::error!(
                    "hang: the window has been in {phase:?} for {} ms (last audio callback {} ms ago)",
                    now - beat,
                    now.saturating_sub(audio)
                );
                reported = true;
            } else if !stuck && reported {
                log::warn!("hang over: back after {phase:?}");
                reported = false;
            }
        }
    });
}
