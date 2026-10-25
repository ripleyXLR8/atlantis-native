//! Draws the menus of Atlantis II as the original lays them out and writes
//! them to PNG files: the player menu, the main menu, the options, the
//! sound page and the load page with the saves of a game played headless.
//!
//! ```text
//! ATLANTIS2_DATA=<install> ATLANTIS2_DISCS=<disc images> \
//!     cargo run --release --example menu_png -- <data folder> <prefix>
//! ```
//!
//! A new game runs for a few seconds, three saves are made in
//! `<data folder>\save\` (a scratch folder: player 0 must have no save in
//! it), then each page is drawn with the pointer over a widget
//! long enough for its tooltip: `<prefix>player.png`, `<prefix>main.png`,
//! `<prefix>options.png`, `<prefix>sound.png`, `<prefix>load.png`.

use std::path::PathBuf;

use atlantis2_data::save::SystemTime;
use atlantis2_data::A2Files;
use atlantis2_engine::engine::PASS;
use atlantis2_engine::menu::PageKind;
use atlantis2_engine::png::encode_rgb565;
use atlantis2_engine::{Engine, Fpu, Variant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = PathBuf::from(args.first().ok_or("a data folder is needed")?);
    let prefix = args.get(1).cloned().unwrap_or_else(|| "menu-".into());
    let dir = std::env::var_os("ATLANTIS2_DATA").ok_or("ATLANTIS2_DATA not set")?;
    let discs = std::env::var_os("ATLANTIS2_DISCS").ok_or("ATLANTIS2_DISCS not set")?;
    let (files, _) = A2Files::open_dir(&PathBuf::from(dir), &[PathBuf::from(discs)])?;
    let mut e = Engine::boot(files, Variant::default(), Fpu::default(), 640, 480)?;
    e.set_data_dir(Some(data));
    let mut clock = 0.0;
    let mut screen = vec![0u16; 640 * 480];
    let mut run = |e: &mut Engine, frames: u32, screen: &mut Vec<u16>| {
        for _ in 0..frames {
            clock += PASS;
            e.set_clock(clock);
            e.pass(Some(screen));
        }
    };
    let write = |name: &str, screen: &[u16]| -> std::io::Result<()> {
        let path = format!("{prefix}{name}.png");
        std::fs::write(&path, encode_rgb565(screen, 640, 480))?;
        println!("{path}");
        Ok(())
    };

    // The player menu, the pointer on the third symbol.
    e.open_player_menu();
    e.pointer(395, 129);
    run(&mut e, 60, &mut screen);
    write("player", &screen)?;

    // A click on the first symbol: player 0 has no save, a new game
    // starts; the opening film skipped, three saves a minute apart.
    e.pointer(277, 146);
    e.click(e.clock());
    e.release();
    run(&mut e, 2, &mut screen);
    e.end_video();
    run(&mut e, 30, &mut screen);
    for (i, minute) in [(0u8, 10u16), (1, 11), (2, 12)] {
        e.wall_clock = match minute {
            10 => || stamp(10),
            11 => || stamp(11),
            _ => || stamp(12),
        };
        e.save_slot(i);
        e.mouse_motion(200, 0);
        run(&mut e, 20, &mut screen);
    }

    // The main menu, the pointer on the options' tool box.
    e.open_main_menu();
    e.pointer(550, 80);
    run(&mut e, 60, &mut screen);
    write("main", &screen)?;
    for (page, name, at) in [
        (PageKind::Options, "options", (230, 200)),
        (PageKind::Sound, "sound", (300, 215)),
        (PageKind::Load, "load", (138, 159)),
    ] {
        e.open_menu_page(page);
        e.pointer(at.0, at.1);
        run(&mut e, 60, &mut screen);
        write(name, &screen)?;
    }
    Ok(())
}

/// 22 October 2026, 14:`minute`.
fn stamp(minute: u16) -> SystemTime {
    SystemTime {
        year: 2026,
        month: 10,
        day_of_week: 4,
        day: 22,
        hour: 14,
        minute,
        second: 0,
        milliseconds: 0,
    }
}
