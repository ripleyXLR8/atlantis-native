//! A headless OpenGL context for the tests: EGL on a device (a graphics
//! card's driver or Mesa's llvmpipe), surfaceless, no window.
//!
//! On Linux with Mesa's EGL (`libEGL.so.1`) this needs nothing else; under
//! WSLg the device is llvmpipe or Mesa's D3D12 driver over the card. Where
//! no context can be made, a test says `SKIPPED` and passes, unless
//! `ATLANTIS4_REQUIRE_GPU=1` asks for a failure.

#![allow(dead_code)]

use std::io::Write;

/// A current context and the objects that keep it alive.
pub struct Headless {
    pub gl: Option<glow::Context>,
    #[cfg(target_os = "linux")]
    _ctx: glutin::api::egl::context::PossiblyCurrentContext,
    #[cfg(target_os = "linux")]
    _display: glutin::api::egl::display::Display,
    pub device: String,
}

/// The variable that turns a missing context into a failure.
pub const REQUIRE_VAR: &str = "ATLANTIS4_REQUIRE_GPU";

/// A context, or `None` after saying why (or a panic with
/// `ATLANTIS4_REQUIRE_GPU=1`).
pub fn context() -> Option<Headless> {
    match make() {
        Ok(h) => Some(h),
        Err(e) => {
            if std::env::var_os(REQUIRE_VAR).is_some_and(|v| v == "1") {
                panic!("no headless OpenGL context: {e}");
            }
            let thread = std::thread::current();
            let _ = writeln!(
                std::io::stderr(),
                "SKIPPED {}: no headless OpenGL context ({e})",
                thread.name().unwrap_or("?")
            );
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn make() -> Result<Headless, String> {
    Err("headless contexts are made on Linux (EGL) only".into())
}

#[cfg(target_os = "linux")]
fn make() -> Result<Headless, String> {
    use glutin::api::egl::{device::Device, display::Display};
    use glutin::config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder};
    use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
    use glutin::display::GlDisplay;

    let devices = Device::query_devices().map_err(|e| e.to_string())?;
    let mut why = String::from("no EGL device");
    // One device when `ATLANTIS4_GPU_DEVICE` names it (its index in
    // EGL's list, or part of its name or vendor), else the first that
    // works in EGL's order.
    let want = std::env::var("ATLANTIS4_GPU_DEVICE").ok();
    let mut devices: Vec<(usize, Device)> = devices.enumerate().collect();
    if let Some(w) = &want {
        devices.retain(|(i, d)| {
            w.parse::<usize>().ok() == Some(*i)
                || d.name().is_some_and(|n| n.contains(w.as_str()))
                || d.vendor().is_some_and(|n| n.contains(w.as_str()))
        });
    }
    for (index, device) in devices {
        let name = format!(
            "#{index} {} {}",
            device.vendor().unwrap_or("?"),
            device.name().unwrap_or("?")
        );
        // SAFETY: no DRM handle is passed.
        let display = match unsafe { Display::with_device(&device, None) } {
            Ok(d) => d,
            Err(e) => {
                why = format!("{name}: {e}");
                continue;
            }
        };
        for (api, ctx_api) in [
            (Api::OPENGL, ContextApi::OpenGl(Some(Version::new(3, 3)))),
            (Api::GLES3, ContextApi::Gles(Some(Version::new(3, 0)))),
        ] {
            let template = ConfigTemplateBuilder::new()
                .with_surface_type(ConfigSurfaceTypes::empty())
                .with_api(api)
                .build();
            // SAFETY: the display is alive.
            let config = match unsafe { display.find_configs(template) }
                .ok()
                .and_then(|mut c| c.next())
            {
                Some(c) => c,
                None => {
                    why = format!("{name}: no {api:?} configuration");
                    continue;
                }
            };
            let attrs = ContextAttributesBuilder::new()
                .with_context_api(ctx_api)
                .build(None);
            // SAFETY: the configuration is the display's.
            let ctx = match unsafe { display.create_context(&config, &attrs) } {
                Ok(c) => c,
                Err(e) => {
                    why = format!("{name}: {e}");
                    continue;
                }
            };
            let ctx = match ctx.make_current_surfaceless() {
                Ok(c) => c,
                Err(e) => {
                    why = format!("{name}: {e}");
                    continue;
                }
            };
            // SAFETY: the context is current on this thread.
            let gl = unsafe {
                glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s).cast())
            };
            return Ok(Headless {
                gl: Some(gl),
                _ctx: ctx,
                _display: display,
                device: name,
            });
        }
    }
    Err(why)
}
