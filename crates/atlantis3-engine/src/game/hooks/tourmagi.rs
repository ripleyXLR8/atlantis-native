//! The magician's tower (`tourmagi.wam`, world 3, chapter 47): the race of
//! the thief (flag 800) against the demon (flag 801) on the squares 39..99,
//! each square a view (entry @ 0x004d4f90, frame @ 0x004d5170). The
//! player's die is the `GBOY` screen's result (@ 0x00756060). Player on 99
//! → `Flag151` = 1 + `CtrlScenarIN` (@ 0x004d56b0); demon on 99 →
//! `Flag914` := 0, `Flag981` := 1 + `CtrlScenarIN`.

use super::{Hx, Screen, ZoneRec};

/// The first and last squares (@ 0x0071f96c, 0x0071f968).
const FIRST: i32 = 39;
const LAST: i32 = 99;

/// The square table (@ 0x007899e0[square − 39], filled once, byte
/// 0x0071f964, @ 0x004d4f90): 1 lion (+10), 2 snake (−10), 3 well (−20),
/// 4 moon (+20), 0 plain.
fn square(n: i32) -> i32 {
    match n {
        46 | 51 | 64 | 89 => 1,
        50 | 62 | 73 | 86 | 97 => 2,
        92 => 3,
        58 => 4,
        _ => 0,
    }
}

#[derive(Debug, Clone)]
pub struct State {
    /// In `tourmagi.wam` at the last frame hook (@ 0x00789ae3; read by the
    /// tower map's screen).
    pub here: bool,
    /// The reload byte (@ 0x0071f954): 1 at start and after a load.
    pub reload: bool,
    /// The player's and the demon's squares (@ 0x00789ad4, 0x00789ad8;
    /// flags 800, 801).
    pub player: i32,
    pub demon: i32,
    /// The demon's pending square (@ 0x0071f958), the player's square to
    /// enter at the next frame (@ 0x0071f95c) and his pending move
    /// (@ 0x0071f960); −1 = none.
    pub demon_to: i32,
    pub player_at: i32,
    pub player_to: i32,
    /// The player has won (@ 0x00789ae1, @ 0x00789ae2).
    pub won: bool,
    pub won_seen: bool,
    /// The cinematic to play after the player's move (@ 0x00789ae4).
    pub cinema: i32,
    /// The demon's turn ran (@ 0x00789ae8): the die screen comes next.
    pub demon_turn: bool,
    /// The `GBOY` screen's result (@ 0x00756060): 1..6, 0 = none. The
    /// screen's start sets it to 0; its play writes it.
    pub dice: i32,
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

impl State {
    pub fn new() -> State {
        State {
            here: false,
            reload: true,
            player: 0,
            demon: 0,
            demon_to: -1,
            player_at: -1,
            player_to: -1,
            won: false,
            won_seen: false,
            cinema: 0,
            demon_turn: false,
            dice: 0,
        }
    }

    /// @ 0x00459e70: 0x0071f954 := 1.
    pub fn after_load(&mut self) {
        self.reload = true;
    }
}

/// The view entry (@ 0x004d4f90): the board set up at the first entry
/// (`Flag914` = 0), the squares read back after a load.
pub(super) fn view(h: &mut Hx) {
    if h.flag(914) == 0 {
        h.set_flag(914, 1);
        h.set_flag(801, FIRST + 1);
        h.set_flag(800, FIRST);
        let t = &mut h.g.puzzles.tourmagi;
        t.demon_to = -1;
        t.player_at = -1;
        t.player_to = -1;
        t.cinema = 0;
        t.player = FIRST;
        t.won_seen = false;
        t.won = false;
        t.dice = 0;
        t.demon = FIRST + 1;
    }
    if h.g.puzzles.tourmagi.reload {
        let (p, d, w) = (h.flag(800), h.flag(801), h.flag(151) != 0);
        let t = &mut h.g.puzzles.tourmagi;
        t.player = p;
        t.won = w;
        t.demon = d;
        t.dice = 0;
        t.won_seen = w;
    }
    h.g.puzzles.tourmagi.demon_turn = false;
    // visual: the demon's pawn next to the player (`TAS_DEMONPLUS1/2`,
    // `TAS_DEMONMOINS1/2` slots, @ 0x004d46e0) (M4).
    h.g.puzzles.tourmagi.reload = false;
}

/// The demon's die (@ 0x004d50d0): 6 below 90; at 90, 91 the exact throw
/// to the well (92) when `rand() % 100 < 40`; from 92 the exact throw to
/// 99 when within 6.
fn demon_die(h: &mut Hx) -> i32 {
    let d = h.g.puzzles.tourmagi.demon;
    if d < 90 {
        return 6;
    }
    if d < 92 {
        if h.rand() % 100 < 40 {
            return 92 - d;
        }
        return 6;
    }
    if LAST <= d + 6 {
        return LAST - d;
    }
    log::error!("DemonLancerDe n'a pas trouve un lancer correct");
    6
}

/// A target past 99 bounced back.
fn bounce(n: i32) -> i32 {
    if n > LAST {
        LAST * 2 - n
    } else {
        n
    }
}

/// The frame (@ 0x004d5170), with the zone record.
pub(super) fn frame(h: &mut Hx, z: &ZoneRec) {
    // @ 0x004d5170..0x004d51b3: no demon move pending and the demon's turn
    // not running: the player's part, after clearing a freeze still set
    // (logged).
    let t = &h.g.puzzles.tourmagi;
    if !t.demon_turn && t.demon_to == -1 && !h.g.puzzles.freeze {
        return player_part(h, z);
    }
    if !h.g.puzzles.tourmagi.demon_turn && h.g.puzzles.tourmagi.demon_to == -1 {
        log::warn!("freeze still set in the tower: cleared");
        h.g.puzzles.freeze = false;
    }
    if h.g.puzzles.tourmagi.demon_to == -1 {
        if h.g.puzzles.tourmagi.demon_turn {
            // 2. The demon has moved: the player's die.
            h.g.puzzles.freeze = false;
            h.g.puzzles.tourmagi.demon_turn = false;
            h.run("(StopMusik=1)");
            h.run("(StartMusik=40)");
            log::debug!("-------- Lancement Game Boy");
            // sound: the music volume saved (0x007899d8 := 0x007178e8) and
            // set to 99 (@ 0x00434100) (host, M4).
            h.start_screen(Screen::GBoy);
            return;
        }
        return player_part(h, z);
    }
    // 1. The demon's move.
    let to = h.g.puzzles.tourmagi.demon_to;
    log::debug!("---------------- DemonGotoCase : {to}");
    h.g.puzzles.tourmagi.demon = to;
    h.set_flag(801, to);
    h.g.puzzles.tourmagi.demon_to = -1;
    // visual: the demon's pawn redrawn (@ 0x004d46e0) (M4).
    let (d, p) = (h.g.puzzles.tourmagi.demon, h.g.puzzles.tourmagi.player);
    if d == p {
        h.g.puzzles.tourmagi.demon_to = d - 1;
        return;
    }
    if d == LAST {
        h.g.puzzles.freeze = false;
        h.g.puzzles.tourmagi.demon_turn = false;
        log::info!("Demon a Gagne !!!");
        // sound: every sound stopped (@ 0x00433580) (M4).
        h.set_flag(914, 0);
        h.set_flag(981, 1);
        h.ctrl();
        return;
    }
    match square(d) {
        1 | 4 => {
            h.g.puzzles.tourmagi.demon_to = d + if square(d) == 1 { 10 } else { 20 };
            // sound: every sound stopped (@ 0x00433580) (M4).
            h.run("(Cinema=69,fps=25)");
        }
        2 | 3 => {
            h.g.puzzles.tourmagi.demon_to = d - if square(d) == 2 { 10 } else { 20 };
            // sound: every sound stopped (@ 0x00433580) (M4).
            h.run("(Cinema=70,fps=25)");
        }
        // A plain square: the player's part in the same frame.
        _ => player_part(h, z),
    }
}

/// The player's part (@ 0x004d5352): his pending move, the square entered,
/// or (nothing pending, not won) a click on `TAS_ZONE_CASESUIV` starts the
/// demon's turn and a die result moves him.
fn player_part(h: &mut Hx, z: &ZoneRec) {
    let to = h.g.puzzles.tourmagi.player_to;
    if to != -1 {
        // 3. The move: the view of the square.
        log::debug!("---------------- JoueurChangeCase : {to}");
        let mut at = to;
        h.g.puzzles.tourmagi.player_to = -1;
        if to == h.g.puzzles.tourmagi.demon {
            at -= 1;
        }
        h.g.puzzles.tourmagi.player_at = at;
        let x = if at == LAST {
            "(AngleWarpX=-90)"
        } else {
            "(AngleWarpX=-156)"
        };
        h.set_flag(800, at);
        h.g.puzzles.tourmagi.player = at;
        h.run(x);
        let y = if square(at) == 0 {
            "(AngleWarpY=20)"
        } else {
            "(AngleWarpY=60)"
        };
        h.run(y);
        h.run(&format!("(Vue={at})"));
        let c = h.g.puzzles.tourmagi.cinema;
        if c != 0 {
            // sound: every sound stopped (@ 0x00433580) (M4).
            h.run(&format!("(Cinema={c},fps=25)"));
            h.g.puzzles.tourmagi.cinema = 0;
        }
        return;
    }
    let at = h.g.puzzles.tourmagi.player_at;
    if at != -1 {
        // The square entered at the last frame.
        log::debug!("---------------- JoueurGotoCase : {at}");
        h.g.puzzles.tourmagi.player = at;
        h.set_flag(800, at);
        h.g.puzzles.tourmagi.player_at = -1;
        if at == LAST {
            log::info!("Joueur a Gagne !!!");
            h.g.puzzles.tourmagi.won_seen = true;
            // @ 0x004d56b0.
            h.g.puzzles.tourmagi.won = true;
            h.set_flag(151, 1);
            h.ctrl();
            return;
        }
        let t = &mut h.g.puzzles.tourmagi;
        match square(at) {
            1 => {
                t.cinema = 67;
                t.player_to = at + 10;
                return;
            }
            2 => {
                t.cinema = 68;
                t.player_to = at - 10;
                return;
            }
            3 => {
                t.cinema = 68;
                t.player_to = at - 20;
                return;
            }
            4 => {
                t.cinema = 67;
                t.player_to = at + 20;
                return;
            }
            _ => {}
        }
    }
    if h.g.puzzles.tourmagi.won_seen {
        return;
    }
    // 4. The click: the demon throws.
    if z.is("TAS_ZONE_CASESUIV") {
        h.g.puzzles.freeze = true;
        h.g.puzzles.tourmagi.demon_turn = true;
        let die = demon_die(h);
        let t = &mut h.g.puzzles.tourmagi;
        t.demon_to = bounce(die + t.demon);
        log::debug!("########################## CaseDemon : {}", t.demon);
    }
    // The die's result moves the player.
    let dice = h.g.puzzles.tourmagi.dice;
    if dice > 0 {
        log::debug!("########################## Resultat GameBoy : {dice}");
        let t = &mut h.g.puzzles.tourmagi;
        t.player_to = bounce(t.player + dice);
        log::debug!("########################## CaseJoueur : {}", t.player_to);
        t.dice = 0;
    }
}

/// The player's win (@ 0x004d56b0): the won byte, `Flag151` := 1,
/// `CtrlScenarIN`.
pub(super) fn player_won(h: &mut Hx) {
    h.g.puzzles.tourmagi.won = true;
    h.set_flag(151, 1);
    h.ctrl();
}
