//! Developers' aid: every place of the game entered view by view as the
//! scripts do it (`(monde=n)`, `(changewam=...)`, `(vue=...)`), headless:
//! prints per place the views whose cube and place animations load, and
//! writes one screenshot (PNG) per place.
//!
//! `cargo run --release -p atlantis3-engine --example a3_world_shots -- <game folder> <out folder>`

use std::path::PathBuf;

use atlantis3_data::A3Files;
use atlantis3_engine::Game;

/// The places and the world (`MONDE`) whose folders hold their cubes, as
/// the chapters' `/INIT`s pair them, with the view to photograph.
const PLACES: [(&str, i32, u32); 10] = [
    ("hoggar.wam", 1, 4),
    ("CFuner.wam", 2, 4),
    ("LDMorts.wam", 2, 22),
    ("jardin.wam", 3, 30),
    ("MMarchd.wam", 3, 12),
    ("PMagi.wam", 3, 7),
    ("TourMagi.wam", 3, 1),
    ("paleo.wam", 4, 1),
    ("Rhea.wam", 5, 10),
    ("Interdim.wam", 5, 1),
];

/// Warnings and errors to stderr (`A3_SHOTS_DEBUG`).
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    if std::env::var_os("A3_SHOTS_DEBUG").is_some() {
        log::set_logger(&Stderr).unwrap();
        log::set_max_level(log::LevelFilter::Warn);
    }
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = A3Files::open_dir(&dir).expect("an Atlantis III install").0;
    let mut g = Game::new(files).expect("the game starts");
    g.seed(1);
    let mut t = 0.0;
    // A new game (the Hoggar's arrival, its cinematics skipped): the warp
    // runs from then on.
    g.new_game();
    for _ in 0..300 {
        t += 1.0 / 30.0;
        g.step(t);
        if g.video().is_some() {
            g.finish_video();
        }
        if g.dialog_running() {
            g.finish_dialog();
        }
    }
    for (wam, world, shot) in PLACES {
        g.run_command(&format!("(monde={world})"));
        g.run_command(&format!("(changewam={wam})"));
        if let Some(e) = g.script().and_then(|s| s.fatal.as_ref()) {
            println!("{wam}: script stopped: {e}");
        }
        let ids: Vec<u32> = g
            .wam()
            .places
            .iter()
            .flat_map(|p| &p.views)
            .map(|v| v.id)
            .collect();
        let (mut cubes, mut sprites) = (0, 0);
        for &id in &ids {
            g.run_command(&format!("(vue={id})"));
            if std::env::var_os("A3_SHOTS_DEBUG").is_some() {
                println!(
                    "  {:?} world {:?}",
                    g.place(),
                    g.script().map(|s| s.a3.world)
                );
            }
            if g.place().1 == Some(id) && g.cube_loaded() {
                cubes += 1;
            }
            sprites += g.slots().slots.iter().filter(|s| s.count > 0).count();
        }
        let shot = if ids.contains(&shot) { shot } else { ids[0] };
        g.run_command(&format!("(vue={shot})"));
        for _ in 0..4 {
            t += 1.0 / 30.0;
            g.step(t);
            if g.video().is_some() {
                g.finish_video();
            }
            if g.dialog_running() {
                g.finish_dialog();
            }
        }
        let name = wam.trim_end_matches(".wam").to_ascii_lowercase();
        let png = atlantis2_engine::png::encode_rgb565(g.screen(), g.width(), g.height());
        std::fs::write(out.join(format!("world-{name}.png")), png).unwrap();
        println!(
            "{wam} world {world}: {} views, {cubes} cubes loaded, {sprites} place animations, shot view {shot}",
            ids.len()
        );
    }
}
