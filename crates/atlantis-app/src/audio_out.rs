//! Sound output: a cpal stream pulling samples from the engine's mixer.

#[cfg(not(target_arch = "wasm32"))]
use atlantis_engine::audio::Mixer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Opens the default output device; `None` (logged) when there is none.
/// The stream plays while it is kept alive. (The browser's goes through
/// [`start_with`], silent while the page is hidden.)
#[cfg(not(target_arch = "wasm32"))]
pub fn start(mixer: Mixer) -> Option<cpal::Stream> {
    start_with(move |out, rate, channels| mixer.fill(out, rate, channels))
}

/// The same for any mixer: `fill(out, rate, channels)` writes interleaved
/// samples.
pub fn start_with<F>(fill: F) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    start_config(fill, None)
}

/// The same, asking the device for `rate` when it offers it (in any of
/// its supported ranges), else its default rate.
#[cfg_attr(
    any(not(feature = "atlantis4"), target_arch = "wasm32"),
    allow(dead_code)
)]
pub fn start_with_rate<F>(fill: F, rate: u32) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    start_config(fill, Some(rate))
}

fn start_config<F>(mut fill: F, want: Option<u32>) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host.default_output_device().or_else(|| {
        log::warn!("no audio output device");
        None
    })?;
    let default = device
        .default_output_config()
        .map_err(|e| log::warn!("audio output: {e}"))
        .ok()?;
    let config = want
        .and_then(|rate| {
            device.supported_output_configs().ok()?.find_map(|c| {
                let ok = c.channels() == default.channels()
                    && c.sample_format() == default.sample_format()
                    && c.min_sample_rate().0 <= rate
                    && rate <= c.max_sample_rate().0;
                ok.then(|| c.with_sample_rate(cpal::SampleRate(rate)))
            })
        })
        .unwrap_or(default);
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                #[cfg(not(target_arch = "wasm32"))]
                crate::watchdog::audio_beat();
                #[cfg(target_arch = "wasm32")]
                note_underrun(info);
                #[cfg(not(target_arch = "wasm32"))]
                let _ = info;
                fill(out, rate, channels)
            },
            |e| log::warn!("audio stream: {e}"),
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut buf = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], info: &cpal::OutputCallbackInfo| {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::watchdog::audio_beat();
                    #[cfg(target_arch = "wasm32")]
                    note_underrun(info);
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = info;
                    buf.resize(out.len(), 0.0);
                    fill(&mut buf, rate, channels);
                    for (o, s) in out.iter_mut().zip(&buf) {
                        *o = (s * 32767.0) as i16;
                    }
                },
                |e| log::warn!("audio stream: {e}"),
                None,
            )
        }
        f => {
            log::warn!("audio output: sample format {f:?} not supported");
            return None;
        }
    }
    .map_err(|e| log::warn!("audio output: {e}"))
    .ok()?;
    stream
        .play()
        .map_err(|e| log::warn!("audio output: {e}"))
        .ok()?;
    log::info!("audio output: {rate} Hz, {channels} channels");
    Some(stream)
}

/// The browser's output buffers that were due before they were filled
/// (the page's thread busy longer than the sound scheduled ahead): each is
/// a gap or a click in the sound. A testing aid of the end-to-end runs.
#[cfg(target_arch = "wasm32")]
static UNDERRUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// cpal's WebAudio output schedules each buffer at the end of the last;
/// one whose start is already past when it is filled came too late.
#[cfg(target_arch = "wasm32")]
fn note_underrun(info: &cpal::OutputCallbackInfo) {
    let t = info.timestamp();
    if t.playback.duration_since(&t.callback).is_none() {
        UNDERRUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The count of late output buffers since the page opened.
#[cfg(target_arch = "wasm32")]
#[cfg_attr(not(feature = "atlantis4"), allow(dead_code))]
pub fn underruns() -> u32 {
    UNDERRUNS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Atlantis Evolution's sound output in the browser: the page's thread
/// mixes ahead and schedules each part on the page's audio clock.
///
/// cpal's output keeps a single buffer ahead (it fills one when the one
/// before starts playing): the page's thread may then be busy one
/// buffer's time, and a late buffer starts at once over the next one.
/// Here the parts are kept [`LEAD`] ahead of the audio clock, filled at
/// each redraw, at each part's end and on a timer, and before work known
/// to hold the page's thread longer ([`cover`]: a scene's loading, a
/// movie's opening, a file read at once) the mix runs that much further
/// ahead. The mix is the same: the same samples in the same order,
/// computed earlier. After a gap, the next part starts at once and is
/// counted ([`underruns`]: one per 4,096 frames of the gap, cpal's
/// count of late buffers then).
#[cfg(all(target_arch = "wasm32", feature = "atlantis4"))]
pub mod web_out {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{AudioContext, AudioContextOptions};

    /// The frames of a part (46 ms at 44,100 Hz).
    const PART: usize = 2048;
    /// How far ahead of the audio clock the mix is kept: the sound's
    /// latency, and the page's thread's longest busy time without a gap.
    pub const LEAD: f64 = 0.25;
    /// The furthest [`cover`] mixes ahead: the music's queue (four
    /// 250 ms buffers ahead of the mix, refilled at each update) must not
    /// run dry meanwhile.
    const MAX_COVER: f64 = 0.9;
    /// The first part's delay after the output starts.
    const START: f64 = 0.05;
    /// The timer's period (ms) for the parts while no redraw runs.
    const TIMER_MS: i32 = 40;
    /// cpal's late buffer in the count of gaps (4,096 frames).
    const LATE_BUFFER: f64 = 4096.0 / 44_100.0;

    type Fill = Box<dyn FnMut(&mut [f32], u32, usize)>;

    struct Inner {
        ctx: AudioContext,
        rate: u32,
        channels: usize,
        fill: Fill,
        /// The audio clock's time where the next part starts (0: none yet).
        next: f64,
        buf: Vec<f32>,
        plane: Vec<f32>,
        /// Called at each part's end: more parts.
        ended: Option<Closure<dyn FnMut()>>,
    }

    thread_local! {
        static OUT: RefCell<Option<Rc<RefCell<Inner>>>> = const { RefCell::new(None) };
    }

    /// The output while it is kept; it stops when dropped.
    pub struct WebOutput {
        inner: Rc<RefCell<Inner>>,
        timer: i32,
        _tick: Closure<dyn FnMut()>,
    }

    impl Drop for WebOutput {
        fn drop(&mut self) {
            if let Some(w) = web_sys::window() {
                w.clear_interval_with_handle(self.timer);
            }
            OUT.with(|o| *o.borrow_mut() = None);
            let mut i = self.inner.borrow_mut();
            i.ended = None;
            let _ = i.ctx.close();
        }
    }

    /// The output at `rate` (the page's audio clock at that rate when the
    /// browser allows it), stereo; `None` (logged) when the page has no
    /// audio.
    pub fn start<F>(fill: F, rate: u32) -> Option<WebOutput>
    where
        F: FnMut(&mut [f32], u32, usize) + 'static,
    {
        let opts = AudioContextOptions::new();
        opts.set_sample_rate(rate as f32);
        let ctx = AudioContext::new_with_context_options(&opts)
            .or_else(|_| AudioContext::new())
            .map_err(|e| log::warn!("audio output: {e:?}"))
            .ok()?;
        let rate = ctx.sample_rate() as u32;
        let channels = 2;
        let dest = ctx.destination();
        if dest.max_channel_count() >= 2 {
            dest.set_channel_count(2);
        }
        let _ = ctx.resume();
        let inner = Rc::new(RefCell::new(Inner {
            ctx,
            rate,
            channels,
            fill: Box::new(fill),
            next: 0.0,
            buf: vec![0.0; PART * channels],
            plane: vec![0.0; PART],
            ended: None,
        }));
        let ended = Closure::<dyn FnMut()>::new(|| top_up(LEAD));
        inner.borrow_mut().ended = Some(ended);
        OUT.with(|o| *o.borrow_mut() = Some(inner.clone()));
        let tick = Closure::<dyn FnMut()>::new(|| top_up(LEAD));
        let timer = web_sys::window()?
            .set_interval_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                TIMER_MS,
            )
            .ok()?;
        top_up(LEAD);
        log::info!(
            "audio output: {rate} Hz, {channels} channels, mixed {:.0} ms ahead",
            LEAD * 1e3
        );
        Some(WebOutput {
            inner,
            timer,
            _tick: tick,
        })
    }

    /// The parts scheduled up to [`LEAD`] ahead (a redraw's call).
    pub fn keep_ahead() {
        top_up(LEAD);
    }

    /// The mix run `secs` ahead of the audio clock (at most
    /// [`MAX_COVER`]), before work that holds the page's thread about that
    /// long.
    pub fn cover(secs: f64) {
        top_up(secs.clamp(LEAD, MAX_COVER));
    }

    fn top_up(ahead: f64) {
        let Some(out) = OUT.with(|o| o.borrow().clone()) else {
            return;
        };
        // (a part's end during a fill started by another call: that one
        // continues)
        let Ok(mut i) = out.try_borrow_mut() else {
            return;
        };
        i.top_up(ahead);
    }

    impl Inner {
        fn top_up(&mut self, ahead: f64) {
            let now = self.ctx.current_time();
            let step = PART as f64 / f64::from(self.rate);
            if self.next == 0.0 {
                self.next = now + START;
            } else if self.next < now {
                // The parts ran out: a gap.
                let gap = now - self.next;
                let late = 1 + (gap / LATE_BUFFER) as u32;
                super::UNDERRUNS.fetch_add(late, std::sync::atomic::Ordering::Relaxed);
                super::note_gap(gap);
                self.next = now + 0.01;
            }
            while self.next < now + ahead {
                if self.schedule().is_err() {
                    return;
                }
                self.next += step;
            }
        }

        /// One part mixed and scheduled at `next`.
        fn schedule(&mut self) -> Result<(), JsValue> {
            let Inner {
                ctx,
                rate,
                channels,
                fill,
                next,
                buf,
                plane,
                ended,
            } = self;
            fill(buf, *rate, *channels);
            let part = ctx.create_buffer(*channels as u32, PART as u32, *rate as f32)?;
            for c in 0..*channels {
                for (p, f) in plane.iter_mut().zip(buf.chunks_exact(*channels)) {
                    *p = f[c];
                }
                part.copy_to_channel(plane, c as i32)?;
            }
            let source = ctx.create_buffer_source()?;
            source.set_buffer(Some(&part));
            source.connect_with_audio_node(&ctx.destination())?;
            if let Some(e) = ended.as_ref() {
                let node: &web_sys::AudioScheduledSourceNode = source.as_ref();
                node.set_onended(Some(e.as_ref().unchecked_ref()));
            }
            source.start_with_when(*next)?;
            Ok(())
        }
    }
}

/// The length of the output's gaps since the page opened (s), a testing
/// aid as [`underruns`].
#[cfg(all(target_arch = "wasm32", feature = "atlantis4"))]
static GAP_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(all(target_arch = "wasm32", feature = "atlantis4"))]
fn note_gap(secs: f64) {
    GAP_US.fetch_add((secs * 1e6) as u64, std::sync::atomic::Ordering::Relaxed);
}

/// The output's gaps since the page opened, in ms.
#[cfg(all(target_arch = "wasm32", feature = "atlantis4"))]
pub fn gap_ms() -> f64 {
    GAP_US.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e3
}
