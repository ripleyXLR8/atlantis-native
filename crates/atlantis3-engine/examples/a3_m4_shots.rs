//! Developers' aid: screenshots (PNG) of the M4 boards, headless, played
//! through the board API: the merchant's house guard with the stone in
//! flight, the garden's jewel case with a gem glowing under the pointer;
//! the Hoggar's spinning top mid-throw with stars lit, the funeral
//! chamber's star, the Isis tiles mid-play (a KS tile held: its cursor and
//! caption), the scales with a feather on each pan, the Ankh coffer open
//! (the place boards are played as their goldens' harness does, then
//! drawn by real warp frames aimed at a zone).
//!
//! `cargo run --release -p atlantis3-engine --example a3_m4_shots -- <game folder> <out folder>`

use std::path::{Path, PathBuf};

use atlantis3_data::A3Files;
use atlantis3_engine::{BoardInput, Game};

/// Frame n of the place harnesses' clock.
fn at(n: u32) -> f64 {
    100.0 + f64::from(n) * 1.0025 / 15.0
}

/// A place board of worlds 1 and 2 begun as its golden's harness does:
/// the commands before the set-up, the place's set-up, the view entered
/// at frame 0, the commands after.
fn place_board(
    files: A3Files,
    wam: &str,
    world: i32,
    view: u32,
    pre: &[&str],
    post: &[&str],
) -> Game {
    let mut g = Game::new(files).expect("the game starts");
    assert!(g.board_begin(wam, world));
    for c in pre {
        g.run_command(c);
    }
    g.board_wam_setup();
    g.seed(1);
    g.board_enter(view, at(0));
    for c in post {
        g.run_command(c);
    }
    g
}

/// Frames `from + 1 ..= to` of a place board.
fn frames(g: &mut Game, from: u32, to: u32) -> u32 {
    for n in from + 1..=to {
        g.board_place_frame(at(n));
    }
    to
}

/// A click on a zone between frames n and n + 1, then frame n + 1.
fn click(g: &mut Game, n: u32, zone: &str) -> u32 {
    assert!(g.board_zone_hook(zone, at(n)), "{zone}");
    frames(g, n, n + 1)
}

/// The camera aimed at a zone, then a few real warp frames drawn (the
/// game's own frame: the place animations, the hooks, the cursor).
fn aimed_shot(g: &mut Game, n: u32, zone: &str, out: &Path, name: &str) {
    g.aim_at_zone(|z| z.name.as_deref() == Some(zone));
    for k in 1..=2 {
        g.step(at(n) + f64::from(k) / 30.0);
    }
    shot(g, out, name);
}

fn shot(g: &Game, out: &Path, name: &str) {
    let png = atlantis2_engine::png::encode_rgb565(g.screen(), g.width(), g.height());
    std::fs::write(out.join(format!("{name}.png")), png).unwrap();
    println!("{name}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("game folder"));
    let out = PathBuf::from(args.next().expect("output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let files = || A3Files::open_dir(&dir).expect("an Atlantis III install").0;

    // The guard: the cat freed, view 1, the stone thrown on its zone, 20
    // frames of its flight (the hook draws the panorama and the stone).
    let mut g = Game::new(files()).expect("the game starts");
    for f in [118, 119] {
        g.script_mut().unwrap().flags[f] = 1;
    }
    assert!(g.board_begin("MMarchd.wam", 3));
    g.board_enter(1, at(0));
    g.run_command("(sacados=MMA_PIERRE)");
    g.run_command("(objetenmain=MMA_PIERRE)");
    let mut n = 1;
    assert!(g.board_frame(at(n), BoardInput::Zone("MMA_ZONE_PIERRE")));
    for _ in 0..20 {
        n += 1;
        g.board_frame(at(n), BoardInput::None);
    }
    shot(&g, &out, "guard");

    // The jewel case: view 114, the case opened by its zone, the pointer
    // on the first gem for a few ticks.
    let mut g = Game::new(files()).expect("the game starts");
    for f in [829, 144] {
        g.script_mut().unwrap().flags[f] = 1;
    }
    assert!(g.board_begin("jardin.wam", 3));
    g.board_enter(114, 100.0);
    assert!(g.board_zone_hook("BAG_ZONE_ECRIN", 200.0));
    let gem = (0..600)
        .step_by(8)
        .flat_map(|y| (0..800).step_by(8).map(move |x| (x, y)))
        .find(|&p| g.board_ecrin_hit(0, p))
        .unwrap_or((400, 300));
    for k in 0..20 {
        let t = 200.0 + f64::from(k + 1) / 32.0;
        g.board_screen_frame(t, gem, false, false);
    }
    shot(&g, &out, "ecrin");

    // The spinning top: zones 4 and 1 thrown to their ends (stars 4 and 1
    // lit), then zone 3 thrown and 100 frames of its spin (star 3 lit).
    let mut g = place_board(
        files(),
        "hoggar.wam",
        1,
        1,
        &["(sacados=HOG_OBJ_DISQUE)"],
        &["(sacados=HOG_OBJ_TOUPIE)", "(objetenmain=HOG_OBJ_TOUPIE)"],
    );
    let idle = |g: &Game| {
        g.board_st("toupie")
            .is_some_and(|s| s.starts_with("state=0 "))
    };
    let mut n = frames(&mut g, 0, 1);
    for zone in [
        "HOG_TOUPIE_ZONE04",
        "HOG_TOUPIE_ZONE01",
        "HOG_TOUPIE_ZONE03",
    ] {
        while !idle(&g) {
            n = frames(&mut g, n, n + 1);
        }
        n = click(&mut g, n, zone);
    }
    n = frames(&mut g, n, n + 100);
    aimed_shot(&mut g, n, "HOG_TOUPIE_ZONE03", &out, "toupie");

    // The funeral chamber's star: two clicks to the right.
    let mut g = place_board(files(), "CFuner.wam", 2, 1, &[], &[]);
    let mut n = frames(&mut g, 0, 2);
    for _ in 0..2 {
        n = click(&mut g, n, "CHF_ETOILEZONED");
        n = frames(&mut g, n, n + 2);
    }
    aimed_shot(&mut g, n, "CHF_ETOILEZONED", &out, "cfuner_stars");

    // The Isis tiles: KI tiles 0..2 put in place, then KS tile 0 taken
    // (its in-hand cursor and its caption).
    let mut g = place_board(files(), "LDMorts.wam", 2, 10, &[], &[]);
    let pos = |g: &Game, kind: &str, i: usize| -> i32 {
        let st = g.board_st("isis").unwrap();
        let list = st
            .split_whitespace()
            .find_map(|w| w.strip_prefix(&format!("{kind}=")))
            .unwrap()
            .to_string();
        list.split(',').nth(i).unwrap().parse().unwrap()
    };
    let mut n = frames(&mut g, 0, 2);
    for i in 0..3 {
        let p = pos(&g, "ki", i);
        if p == i as i32 {
            continue;
        }
        n = click(&mut g, n, &format!("LDM_ICARO{p}"));
        n = frames(&mut g, n, n + 1);
        n = click(&mut g, n, &format!("LDM_ICARO{i}"));
        n = frames(&mut g, n, n + 1);
    }
    let p = pos(&g, "ks", 0);
    n = click(&mut g, n, &format!("LDM_MCARO{p}"));
    n = frames(&mut g, n, n + 2);
    aimed_shot(&mut g, n, &format!("LDM_MCARO{p}"), &out, "isis_tiles");

    // The scales: hung, a feather of weight 3 on the left pan, one of
    // weight 7 on the right (the beam leans right).
    let mut g = Game::new(files()).expect("the game starts");
    assert!(g.board_begin("LDMorts.wam", 2));
    {
        let f = &mut g.script_mut().unwrap().flags;
        f[76] = 1;
        f[85] = 1;
        f[79] = 0;
        for (k, w) in [3, 7, 0, 9, 1, 5, 8, 2, 6, 4].into_iter().enumerate() {
            f[545 + k] = w;
            f[535 + k] = 1;
        }
    }
    g.board_wam_setup();
    g.seed(1);
    g.board_enter(4, at(0));
    g.run_command("(sacados=LDM_BALANCE_PR)");
    for k in 0..10 {
        g.run_command(&format!("(sacados=LDM_PLUME{k})"));
    }
    let mut n = frames(&mut g, 0, 2);
    g.board_hand(Some("LDM_BALANCE_PR"));
    n = click(&mut g, n, "LDM_ACCROCHEBALANCE");
    for (feather, pan) in [
        ("LDM_PLUME0", "LDM_BALANCEG"),
        ("LDM_PLUME1", "LDM_BALANCED"),
    ] {
        n = frames(&mut g, n, n + 2);
        g.board_hand(Some(feather));
        n = click(&mut g, n, pan);
    }
    n = frames(&mut g, n, n + 2);
    aimed_shot(&mut g, n, "LDM_ACCROCHEBALANCE", &out, "scales");

    // The Ankh coffer: clicked, its lid's play to its end.
    let mut g = place_board(files(), "LDMorts.wam", 2, 30, &[], &[]);
    let mut n = frames(&mut g, 0, 2);
    n = click(&mut g, n, "LDM_COFFRE_ANKH");
    n = frames(&mut g, n, n + 12);
    aimed_shot(&mut g, n, "LDM_COFFRE_ANKH", &out, "ankh_coffer");
}
