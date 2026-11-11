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
    start_config(fill, None, None)
}

/// The same, asking the device for `rate` when it offers it (in any of
/// its supported ranges), else its default rate.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn start_with_rate<F>(fill: F, rate: u32) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    start_config(fill, Some(rate), None)
}

/// The same, with output buffers of `frames` frames (the browser's: a
/// buffer is filled when the one before it starts playing, so the page's
/// thread may be busy that long without a gap in the sound; cpal's
/// default is 2,048 frames, 46 ms at 44,100 Hz).
#[cfg(target_arch = "wasm32")]
pub fn start_with_rate_buffer<F>(fill: F, rate: u32, frames: u32) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    start_config(fill, Some(rate), Some(frames))
}

fn start_config<F>(mut fill: F, want: Option<u32>, frames: Option<u32>) -> Option<cpal::Stream>
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
    let mut stream_config: cpal::StreamConfig = config.clone().into();
    if let Some(n) = frames {
        stream_config.buffer_size = cpal::BufferSize::Fixed(n);
    }
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
pub fn underruns() -> u32 {
    UNDERRUNS.load(std::sync::atomic::Ordering::Relaxed)
}
