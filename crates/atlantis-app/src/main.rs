//! Desktop entry point: `atlantis-native <game dir> [panorama]`, panorama a
//! CYCLO\\ name such as ATL16001 (default).

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::sync::Arc;

    use atlantis_app::{open_viewer, App};
    use atlantis_data::{Game, StdDir};
    use winit::event_loop::EventLoop;

    env_logger::init();
    let mut args = std::env::args().skip(1);
    let viewer = args.next().map(|dir| {
        let name = args.next().unwrap_or_else(|| "ATL16001".into());
        Game::open(Arc::new(StdDir::new(dir)))
            .map_err(|e| e.to_string())
            .and_then(|g| open_viewer(&g, &name))
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            })
    });
    let el = EventLoop::new().unwrap();
    el.run_app(&mut App::new(viewer)).unwrap();
}

#[cfg(target_arch = "wasm32")]
fn main() {}
