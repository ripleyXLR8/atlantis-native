//! Sound output: a cpal stream pulling samples from the engine's mixer.

use atlantis_engine::audio::Mixer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Opens the default output device; `None` (logged) when there is none.
/// The stream plays while it is kept alive.
pub fn start(mixer: Mixer) -> Option<cpal::Stream> {
    start_with(move |out, rate, channels| mixer.fill(out, rate, channels))
}

/// The same for any mixer: `fill(out, rate, channels)` writes interleaved
/// samples.
pub fn start_with<F>(mut fill: F) -> Option<cpal::Stream>
where
    F: FnMut(&mut [f32], u32, usize) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host.default_output_device().or_else(|| {
        log::warn!("no audio output device");
        None
    })?;
    let config = device
        .default_output_config()
        .map_err(|e| log::warn!("audio output: {e}"))
        .ok()?;
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |out: &mut [f32], _| {
                #[cfg(not(target_arch = "wasm32"))]
                crate::watchdog::audio_beat();
                fill(out, rate, channels)
            },
            |e| log::warn!("audio stream: {e}"),
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut buf = Vec::new();
            device.build_output_stream(
                &config.into(),
                move |out: &mut [i16], _| {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::watchdog::audio_beat();
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
