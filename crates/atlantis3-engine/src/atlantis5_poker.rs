//! *Atlantis V*'s poker board: the menu of mode 12 (`CMenu_Poker`, ctor
//! @ 0x00421e00, vtable 0x00655668: process @ 0x004240b0, draw
//! @ 0x00425720, init @ 0x00425890, close @ 0x004258a0; the buttons'
//! handler @ 0x00426840 → @ 0x004258b0). The board is C, the rules are the
//! scripts' (`Atl_PuzzlePoker_*`, run as the program's one-shot strings,
//! @ 0x00454220): the board asks the scripts at each step and shows the
//! choice panel, the cash, the pot, the players' cards and the winner.
//!
//! Addresses are atlantis5-dvd-int-3891200's (the international DVD's
//! program; the French DVD's is encrypted and not read).
//!
//! The original runs each step's Lua calls inside its process; a call
//! whose script plays a dialog or a film (`Atl_Dialog_Play(275)`,
//! `Atl_PlayActionVideo("ma0013")`, `Atl_PuzzlePoker_BetBasRelief()`,
//! `OnWin`, `OnLose`, `LoadNextGame`) runs that loop before it returns.
//! Native runs those as script runs ([`Host::run`]) and holds the rest of
//! the step (the queued [`Op`]s) until the run ends; the getters and
//! setters run at once ([`Host::call`]).

use std::collections::VecDeque;
use std::sync::Arc;

use atlantis3_data::spr::{Bank, Depth, Target};
use atlantis5_script::Value;
use atlantis_data::FileSource;

use crate::atlantis5::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::atlantis5_dialog::Fonts;
use crate::atlantis5_menu::{halign, rgb};
use crate::text::Zone;

/// The screen (`FUN_0044d1b0` +0xc, +0x10; Native's menus' screen).
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const W: i32 = VIEW_WIDTH as i32;
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const H: i32 = VIEW_HEIGHT as i32;

/// The board's folder (path 27, `sprites2d/menu/poker/`).
const DIR: &str = "datas/sprites2d/menu/poker";

/// What the board asks of the game.
pub trait Host {
    /// A one-shot string run at once (@ 0x00454220), with `args` the
    /// function it returns called with them; `nresults` values back (nil
    /// when the call fails, which is logged).
    fn call(&mut self, source: &str, args: Option<&[Value]>, nresults: usize) -> Vec<Value>;
    /// A one-shot string run as a script run: its dialog or film plays
    /// before the board goes on.
    fn run(&mut self, source: &str, args: Option<Vec<Value>>);
    /// Whether a script run is going on.
    fn busy(&self) -> bool;
    /// The program's CRT `rand` (@ 0x0062b352).
    fn rand(&mut self) -> u32;
    /// A `string_resources.xml` text.
    fn string(&self, key: &str) -> String;
}

// The elements (the ctor's fields).
const ROOT: usize = 0; // +0x04 menu_root
const BUBBLE: [usize; 3] = [1, 2, 3]; // +0x14.. card_bubble_1..3
const CARD: [[usize; 2]; 4] = [[4, 5], [6, 7], [8, 9], [10, 11]]; // card_set_1_1 .. 4_2
const WINNER_BG: usize = 12; // +0x44
const PIC_WINNER: usize = 13; // +0x40
const NEXT_GAME: usize = 14; // +0x60
const TEXT_WINNER: usize = 15; // +0x58
const CASH: [usize; 4] = [16, 17, 18, 19]; // +0x48 howard, elliot, fisher, punter
const CHOICE_BG: usize = 20; // +0x64
const INCREASE: usize = 21; // +0x68
const FOLLOW: usize = 22; // +0x6c
const STOP: usize = 23; // +0x70
const POT: usize = 24; // +0x5c

/// The draw's order (@ 0x00425720): each shown with its children.
const DRAW: [usize; 12] = [
    CHOICE_BG, CASH[0], CASH[1], CASH[2], CASH[3], BUBBLE[0], BUBBLE[1], BUBBLE[2], WINNER_BG,
    CARD[3][0], CARD[3][1], POT,
];

enum Kind {
    Root,
    /// `CIE_Sprite` (@ 0x0044f800): its file, its frame (+0x5c,
    /// @ 0x0044fa90).
    Sprite(Option<Bank>, usize),
    /// `CIE_Text` (@ 0x00451170) or `CIE_TextButton` (@ 0x0044e9d0, the
    /// colour under the mouse; `over` its +0x8c).
    Text {
        font: &'static str,
        text: String,
        width: i32,
        color: u32,
        hover: Option<u32>,
        align: u32,
        over: bool,
    },
}

struct El {
    parent: Option<usize>,
    children: Vec<usize>,
    x: i32,
    y: i32,
    /// +0x41: 0 from the base ctor (@ 0x00451b40), which none of the
    /// element ctors changes.
    visible: bool,
    kind: Kind,
}

/// The steps a process left to do after a call that runs a loop.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    /// A string run as a script run (it may wait).
    Run(String, Option<Vec<Value>>),
    /// State 1's end: the cards, the cash and the pot shown.
    Cards,
    /// `FUN_00426050(p)`: `return Atl_PuzzlePoker_PayBet` (p), then the
    /// cash of p and the pot shown.
    PayBet(i32),
    /// State 8's end: 0xa when Howard has not stopped, else 0xc.
    AfterPunter,
    State(u32),
}

/// The poker board.
pub struct Poker {
    els: Vec<El>,
    /// +0x74.
    pub state: u32,
    /// The menu's cursor (+0x08) shown.
    pub cursor: bool,
    ops: VecDeque<Op>,
    winners: [Option<Bank>; 4],
    /// The mouse at the last frame: the buttons' hover is tested only on
    /// a move (a mouse event of kind 2, @ 0x004523e0).
    last_mouse: Option<(i32, i32)>,
    /// The cursor's place (+0x08, put at the menu's mouse by @ 0x0040d920
    /// on the move events of the frames the process runs). A run's dialog
    /// or film blocks the original's process: the moves made meanwhile
    /// reach the global mouse, not the cursor, until a move event of a
    /// frame the process runs.
    place: Option<(i32, i32)>,
    /// The mouse at the last frame, held by a run or not.
    seen: Option<(i32, i32)>,
    /// The mouse to put back at the cursor's place (states 2, 0xa, 0xc),
    /// for the host to take ([`Poker::take_mouse`]).
    mouse_back: Option<(i32, i32)>,
}

fn num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Num(x)) => *x,
        Some(Value::Str(s)) => String::from_utf8_lossy(s).trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `__ftol` of `lua_tonumber`.
#[allow(clippy::cast_possible_truncation)]
fn ftol(v: Option<&Value>) -> i32 {
    num(v) as i32
}

/// `_snwprintf(fmt, n)` of a resource text + `" (%d$)"` (0x0066c2d8) or
/// of `"%d$"` (0x0066c6b4).
fn money(prefix: &str, n: i32) -> String {
    format!("{prefix}{n}$")
}

/// The action strings (@ 0x00425de0, @ 0x004261b0: compared in this
/// order, anything else 3).
fn action_code(v: Option<&Value>) -> u32 {
    match v {
        Some(Value::Str(s)) if s == b"increase_bet" => 0,
        Some(Value::Str(s)) if s == b"follow_bet" => 1,
        Some(Value::Str(s)) if s == b"stop_bet" => 2,
        _ => 3,
    }
}

impl Poker {
    /// The board (@ 0x00421e00), its elements hidden, state 0 (the init
    /// @ 0x00425890).
    pub fn new(src: &Arc<dyn FileSource>, host: &dyn Host) -> Poker {
        let spr = |f: &str| {
            src.read(&format!("{DIR}/{f}"))
                .and_then(|d| Bank::parse(&d, Depth::Xrgb8888).ok())
        };
        let mut p = Poker {
            els: Vec::new(),
            state: 0,
            cursor: false,
            ops: VecDeque::new(),
            winners: [
                spr("howard.spr"),
                spr("elliot.spr"),
                spr("fisher.spr"),
                spr("punter.spr"),
            ],
            last_mouse: None,
            place: None,
            seen: None,
            mouse_back: None,
        };
        p.add(None, 0, 0, Kind::Root);
        // card_bubble_1..3 (bubble.spr) at (0, 10).
        for _ in 0..3 {
            p.add(Some(ROOT), 0, 10, Kind::Sprite(spr("bubble.spr"), 0));
        }
        // card_set_1_1 .. 3_2 in the bubbles at (0, 20); 4_1, 4_2 on the
        // root at (724, 470), (800, 470).
        for b in BUBBLE {
            for _ in 0..2 {
                p.add(Some(b), 0, 0x14, Kind::Sprite(spr("card_set.spr"), 0));
            }
        }
        p.add(
            Some(ROOT),
            0x2d4,
            0x1d6,
            Kind::Sprite(spr("card_set.spr"), 0),
        );
        p.add(
            Some(ROOT),
            0x320,
            0x1d6,
            Kind::Sprite(spr("card_set.spr"), 0),
        );
        // winner_background at (512, 300); pic_winner (howard.spr) in it.
        p.add(
            Some(ROOT),
            0x200,
            0x12c,
            Kind::Sprite(spr("winner_background.spr"), 0),
        );
        p.add(Some(WINNER_BG), 0, 0, Kind::Sprite(spr("howard.spr"), 0));
        // next_game: `back_to_game`, AtlFont_PokerContinue, 400 wide,
        // black, red under the mouse; text_winner: `winner`,
        // AtlFont_PokerChoice, black, (0, -10), 100 wide.
        p.add(
            Some(WINNER_BG),
            0,
            0,
            Kind::Text {
                font: "AtlFont_PokerContinue",
                text: host.string("back_to_game"),
                width: 0x190,
                color: 0,
                hover: Some(0xff),
                align: 1,
                over: false,
            },
        );
        p.add(
            Some(WINNER_BG),
            0,
            -10,
            Kind::Text {
                font: "AtlFont_PokerChoice",
                text: host.string("winner"),
                width: 100,
                color: 0,
                hover: None,
                align: 0,
                over: false,
            },
        );
        // cash_howard (90 wide, right), cash_elliot, cash_fisher,
        // cash_punter (100 wide, centred): white "-".
        for (w, a) in [(0x5a, 2), (100, 1), (100, 1), (100, 1)] {
            p.add(
                Some(ROOT),
                0,
                0,
                Kind::Text {
                    font: "AtlFont_PokerChoice",
                    text: "-".into(),
                    width: w,
                    color: 0x00ff_ffff,
                    hover: None,
                    align: a,
                    over: false,
                },
            );
        }
        // choice_background at (0, 300) and its three buttons, 400 wide.
        p.add(
            Some(ROOT),
            0,
            0x12c,
            Kind::Sprite(spr("choice_background.spr"), 0),
        );
        for key in ["increase_bet", "follow_bet", "stop_bet"] {
            p.add(
                Some(CHOICE_BG),
                0,
                0,
                Kind::Text {
                    font: "AtlFont_PokerChoice",
                    text: host.string(key),
                    width: 0x190,
                    color: 0,
                    hover: Some(0xff),
                    align: 0,
                    over: false,
                },
            );
        }
        // pot: white "-", 100 wide, left.
        p.add(
            Some(ROOT),
            0,
            0,
            Kind::Text {
                font: "AtlFont_PokerChoice",
                text: "-".into(),
                width: 100,
                color: 0x00ff_ffff,
                hover: None,
                align: 0,
                over: false,
            },
        );
        debug_assert_eq!(p.els.len(), POT + 1);
        p
    }

    fn add(&mut self, parent: Option<usize>, x: i32, y: i32, kind: Kind) {
        let i = self.els.len();
        self.els.push(El {
            parent,
            children: Vec::new(),
            x,
            y,
            visible: false,
            kind,
        });
        if let Some(p) = parent {
            self.els[p].children.push(i);
        }
    }

    fn place(&mut self, i: usize, x: i32, y: i32) {
        self.els[i].x = x;
        self.els[i].y = y;
    }

    /// Show (@ 0x00452530): shown, and each hidden child shown. A show
    /// resets the hit flag (+0x40 = 0 @ 0x00452530) and a text button's
    /// state (+0x8c = 0 @ 0x0044ec50): drawn in its normal colour, and
    /// not clickable until the mouse moves over it again.
    fn show(&mut self, i: usize) {
        self.els[i].visible = true;
        if let Kind::Text { over, .. } = &mut self.els[i].kind {
            *over = false;
        }
        for c in self.els[i].children.clone() {
            if !self.els[c].visible {
                self.show(c);
            }
        }
    }

    /// Hide (@ 0x004525d0): hidden, and each shown child hidden.
    fn hide(&mut self, i: usize) {
        self.els[i].visible = false;
        for c in self.els[i].children.clone() {
            if self.els[c].visible {
                self.hide(c);
            }
        }
    }

    fn set_text(&mut self, i: usize, s: String) {
        if let Kind::Text { text, .. } = &mut self.els[i].kind {
            *text = s;
        }
    }

    fn text(&self, i: usize) -> (&'static str, String, i32) {
        match &self.els[i].kind {
            Kind::Text {
                font, text, width, ..
            } => (font, text.clone(), *width),
            _ => ("", String::new(), 0),
        }
    }

    /// The place on the screen (@ 0x004520a0).
    fn abs(&self, i: usize) -> (i32, i32) {
        let e = &self.els[i];
        let (px, py) = e.parent.map_or((0, 0), |p| self.abs(p));
        (px + e.x, py + e.y)
    }

    /// The place on the screen of element `name` (tests).
    #[doc(hidden)]
    pub fn place_of(&self, name: &str) -> Option<(i32, i32)> {
        self.index(name).map(|i| self.abs(i))
    }

    /// Whether element `name` is shown (tests): `increase_bet`,
    /// `follow_bet`, `stop_bet`, `next_game`, `winner_background`,
    /// `choice_background`.
    pub fn shown(&self, name: &str) -> bool {
        self.index(name).is_some_and(|i| self.els[i].visible)
    }

    fn index(&self, name: &str) -> Option<usize> {
        Some(match name {
            "increase_bet" => INCREASE,
            "follow_bet" => FOLLOW,
            "stop_bet" => STOP,
            "next_game" => NEXT_GAME,
            "winner_background" => WINNER_BG,
            "pic_winner" => PIC_WINNER,
            "choice_background" => CHOICE_BG,
            "pot" => POT,
            "cash_howard" => CASH[0],
            "cash_elliot" => CASH[1],
            "cash_fisher" => CASH[2],
            "cash_punter" => CASH[3],
            _ => return None,
        })
    }

    /// A text element's text (tests).
    pub fn text_of(&self, name: &str) -> Option<String> {
        self.index(name).map(|i| self.text(i).1)
    }

    /// The frames of the players' two cards (player 1 Howard .. 4).
    pub fn cards(&self, player: usize) -> (usize, usize) {
        let f = |i: usize| match &self.els[i].kind {
            Kind::Sprite(_, f) => *f,
            _ => 0,
        };
        let c = match player {
            1 => CARD[3],
            2 => CARD[0],
            3 => CARD[1],
            _ => CARD[2],
        };
        (f(c[0]), f(c[1]))
    }

    /// The middle of shown element `name`'s box (tests and tools).
    pub fn point_of(&self, name: &str, fonts: &Fonts) -> Option<(i32, i32)> {
        let i = self.index(name)?;
        if !self.els[i].visible {
            return None;
        }
        let (x, y) = self.abs(i);
        let (_, t, w) = self.text(i);
        let h = fonts.text_height(self.text(i).0, &t, w);
        Some((x + w / 2, y + h / 2))
    }

    // -- the scripts ---------------------------------------------------------

    fn get(host: &mut dyn Host, source: &str) -> i32 {
        ftol(host.call(source, None, 1).first())
    }

    fn get_of(host: &mut dyn Host, source: &str, p: i32) -> Vec<Value> {
        host.call(source, Some(&[Value::Num(f64::from(p))]), 1)
    }

    fn truthy(v: &[Value]) -> bool {
        !matches!(v.first(), None | Some(Value::Nil | Value::Bool(false)))
    }

    /// @ 0x00425de0.
    fn action(host: &mut dyn Host, p: i32) -> u32 {
        action_code(Self::get_of(host, "return Atl_PuzzlePoker_GetPlayerAction", p).first())
    }

    /// @ 0x004261b0.
    fn scenario_action(host: &mut dyn Host, p: i32) -> u32 {
        action_code(Self::get_of(host, "return Atl_PuzzlePokerScenario_GetPlayerAction", p).first())
    }

    /// @ 0x00425d50: 0 `increase_bet`, 1 `follow_bet`, 2 and 3
    /// `stop_bet`.
    fn set_action(host: &mut dyn Host, p: i32, code: u32) {
        let mut args = vec![Value::Num(f64::from(p))];
        match code {
            0 => args.push(Value::Str(b"increase_bet".to_vec())),
            1 => args.push(Value::Str(b"follow_bet".to_vec())),
            2 | 3 => args.push(Value::Str(b"stop_bet".to_vec())),
            _ => {}
        }
        host.call("return Atl_PuzzlePoker_SetPlayerAction", Some(&args), 0);
    }

    fn cash(host: &mut dyn Host, p: i32) -> i32 {
        ftol(Self::get_of(host, "return Atl_PuzzlePoker_GetPlayerCash", p).first())
    }

    fn bet(host: &mut dyn Host) -> i32 {
        Self::get(host, "return Atl_PuzzlePoker_GetBet()")
    }

    fn min_bet(host: &mut dyn Host) -> i32 {
        Self::get(host, "return Atl_PuzzlePoker_GetMinBet()")
    }

    fn flag(host: &mut dyn Host, source: &str) -> bool {
        Self::truthy(&host.call(source, None, 1))
    }

    /// @ 0x00426570: `"%d$"` of player p's cash on its text.
    fn show_cash(&mut self, host: &mut dyn Host, p: i32) {
        let c = Self::cash(host, p);
        if let Ok(k) = usize::try_from(p - 1) {
            if let Some(&i) = CASH.get(k) {
                self.set_text(i, money("", c));
            }
        }
    }

    /// @ 0x00426670: the pot's text.
    fn show_pot(&mut self, host: &mut dyn Host) {
        let pot = Self::get(host, "return Atl_PuzzlePoker_GetPot()");
        self.set_text(POT, money("", pot));
    }

    fn pay_bet(&mut self, host: &mut dyn Host, p: i32) {
        host.call(
            "return Atl_PuzzlePoker_PayBet",
            Some(&[Value::Num(f64::from(p))]),
            0,
        );
        self.show_cash(host, p);
        self.show_pot(host);
    }

    /// `__ftol(rand() × 3/32767)` (0x00655678, a float): 0, 1 or 2.
    #[allow(clippy::cast_possible_truncation)]
    fn third(host: &mut dyn Host) -> u32 {
        let r = f64::from(host.rand());
        (r * f64::from(f32::from_bits(0x38c0_0180))) as u32
    }

    fn dialog(&mut self, id: u32) {
        self.ops
            .push_back(Op::Run(format!("Atl_Dialog_Play({id})"), None));
    }

    /// The panel's mouse put back at the cursor's place (@ 0x004520a0 on
    /// the cursor, stored in the global mouse 0x006b0b38 / 0x006b0b3c and
    /// given to @ 0x0040d890), so the moves made during the blocking film
    /// or dialogs are dropped; the cursor shown. No hover is tested for
    /// that place (no move).
    fn show_cursor(&mut self) {
        self.cursor = true;
        if let Some(p) = self.place {
            self.mouse_back = Some(p);
            self.last_mouse = Some(p);
        }
    }

    /// A frame held by a script run (a film or dialog of the board's): its
    /// moves reach the mouse, not the cursor.
    pub fn held_frame(&mut self, mouse: (i32, i32)) {
        self.seen = Some(mouse);
    }

    /// Where the board's cursor is drawn: its place (+0x08), which only a
    /// move event of a frame the process runs changes (@ 0x0040d920).
    pub fn cursor_place(&self) -> Option<(i32, i32)> {
        self.place
    }

    /// The mouse the board put back at the cursor's place this frame, if
    /// any: the host's menus' mouse is to be set to it.
    pub fn take_mouse(&mut self) -> Option<(i32, i32)> {
        self.mouse_back.take()
    }

    // -- a frame -------------------------------------------------------------

    /// Runs the queued steps; false while a run holds them.
    fn drain(&mut self, host: &mut dyn Host) -> bool {
        while !host.busy() {
            let Some(op) = self.ops.pop_front() else {
                return true;
            };
            match op {
                Op::Run(s, a) => host.run(&s, a),
                Op::Cards => self.cards_shown(host),
                Op::PayBet(p) => self.pay_bet(host, p),
                Op::AfterPunter => {
                    self.state = if Self::action(host, 1) == 2 { 0xc } else { 0xa };
                }
                Op::State(s) => self.state = s,
            }
        }
        false
    }

    /// One frame (the menu manager's state 3 with mode 12): the mouse's
    /// move over the buttons (only when it moved since the last frame:
    /// the base handler @ 0x004523e0 hit-tests on a move event only), the
    /// clicks (@ 0x0040d920 → the buttons' handler), then the process
    /// (@ 0x004240b0). True when the board ends (its process returned 1).
    pub fn frame(
        &mut self,
        host: &mut dyn Host,
        mouse: (i32, i32),
        clicks: usize,
        measure: &dyn Fn(&str, &str, i32, bool) -> i32,
    ) -> bool {
        let moved = self.seen != Some(mouse);
        self.seen = Some(mouse);
        if !self.drain(host) {
            return false;
        }
        // The process runs: the cursor at the mouse when this frame moved
        // it (the moves of the frames held by a run are not events here).
        if moved || self.place.is_none() {
            self.place = Some(mouse);
        }
        // The hover on a move event of this frame only: not for the moves
        // made while a run held the process (the showdown's dialogs, after
        // state 0xc's put-back), nor for a put-back's place.
        if moved && self.last_mouse.is_some_and(|m| m != mouse) {
            self.hover(mouse, measure);
        }
        self.last_mouse = Some(mouse);
        for _ in 0..clicks {
            let hit = [INCREASE, FOLLOW, STOP, NEXT_GAME].into_iter().find(|&i| {
                self.els[i].visible && matches!(self.els[i].kind, Kind::Text { over: true, .. })
            });
            if let Some(i) = hit {
                self.clicked(host, i);
            }
        }
        if !self.drain(host) {
            return false;
        }
        let closed = self.process(host, measure);
        let _ = self.drain(host);
        closed
    }

    /// The text buttons' +0x8c (@ 0x0044eb60): 1 with the mouse in the
    /// box (both ends included, @ 0x00452020), else 0; a hidden button
    /// takes no event (@ 0x004523e0).
    fn hover(&mut self, (mx, my): (i32, i32), measure: &dyn Fn(&str, &str, i32, bool) -> i32) {
        for i in [INCREASE, FOLLOW, STOP, NEXT_GAME] {
            let shown = self.visible_path(i);
            let (x, y) = self.abs(i);
            let (font, text, w) = self.text(i);
            let h = measure(font, &text, w, true);
            let inside = (x..=x + w).contains(&mx) && (y..=y + h).contains(&my);
            if let Kind::Text { over, .. } = &mut self.els[i].kind {
                if shown {
                    *over = inside;
                }
            }
        }
    }

    fn visible_path(&self, i: usize) -> bool {
        let e = &self.els[i];
        e.visible && e.parent.is_none_or(|p| p == ROOT || self.visible_path(p))
    }

    /// The buttons' handler (@ 0x004258b0, the clicked element's name).
    fn clicked(&mut self, host: &mut dyn Host, i: usize) {
        match i {
            INCREASE => {
                host.call("Atl_PuzzlePoker_IncreaseBet()", None, 0);
                self.pay_bet(host, 1);
                Self::set_action(host, 1, 0);
                self.state = 4;
            }
            FOLLOW => {
                Self::set_action(host, 1, 1);
                if self.state == 3 {
                    self.pay_bet(host, 1);
                    self.state = 4;
                } else if self.state == 0xb {
                    host.call("Atl_PuzzlePoker_PayMinBet()", None, 0);
                    self.show_cash(host, 1);
                    self.state = 0xc;
                }
            }
            STOP => {
                Self::set_action(host, 1, 2);
                self.state = 4;
            }
            NEXT_GAME => {
                if Self::get(host, "return Atl_PuzzlePoker_GetGame()") == 5 {
                    self.state = 0xf;
                    self.ops
                        .push_back(Op::Run("Atl_PuzzlePoker_OnWin()".into(), None));
                } else {
                    self.state = 1;
                }
            }
            _ => return,
        }
        self.hide(ROOT);
        self.cursor = false;
    }

    /// State 1's end (@ 0x00424480..0x00424542): each player's two cards
    /// (`return Atl_PuzzlePokerScenario_GetPlayerCards`, @ 0x004263c0) as
    /// the frames of their card sprites, the cash and the pot.
    fn cards_shown(&mut self, host: &mut dyn Host) {
        for (p, set) in [(1, CARD[3]), (2, CARD[0]), (3, CARD[1]), (4, CARD[2])] {
            let v = host.call(
                "return Atl_PuzzlePokerScenario_GetPlayerCards",
                Some(&[Value::Num(f64::from(p))]),
                2,
            );
            for (k, &i) in set.iter().enumerate() {
                let f = usize::try_from(ftol(v.get(k))).unwrap_or(0);
                if let Kind::Sprite(_, frame) = &mut self.els[i].kind {
                    *frame = f;
                }
            }
        }
        for p in 1..=4 {
            self.show_cash(host, p);
        }
        self.show_pot(host);
        self.state = 2;
    }

    /// The process (@ 0x004240b0) for the state +0x74.
    #[allow(clippy::too_many_lines)]
    fn process(
        &mut self,
        host: &mut dyn Host,
        measure: &dyn Fn(&str, &str, i32, bool) -> i32,
    ) -> bool {
        match self.state {
            0 => {
                self.layout(measure);
                host.call("Atl_PuzzlePoker_InitGame()", None, 0);
                self.state = 1;
            }
            1 => {
                self.ops
                    .push_back(Op::Run("Atl_PuzzlePoker_LoadNextGame()".into(), None));
                self.ops
                    .push_back(Op::Run("Atl_PlayActionVideo(\"ma0013\")".into(), None));
                self.ops.push_back(Op::Cards);
            }
            2 => {
                if Self::action(host, 1) == 2 {
                    self.state = 4;
                    return false;
                }
                self.show_cursor();
                for s in [
                    "Atl_PuzzlePoker_DisplayPlayers()",
                    "Atl_PuzzlePoker_DisplayCards()",
                    "Atl_PuzzlePoker_DisplayTokens()",
                ] {
                    host.call(s, None, 0);
                }
                let (bet, min) = (Self::bet(host), Self::min_bet(host));
                let t = host.string("increase_bet");
                self.set_text(INCREASE, money(&format!("{t} ("), bet + min) + ")");
                let t = host.string("follow_bet");
                self.set_text(FOLLOW, money(&format!("{t} ("), bet) + ")");
                let cash = Self::cash(host, 1);
                self.hide(ROOT);
                self.show(CHOICE_BG);
                if cash < bet + min {
                    self.hide(INCREASE);
                    if bet > cash {
                        self.hide(FOLLOW);
                    }
                } else if !Self::flag(host, "return Atl_PuzzlePoker_IsBetIncreaseAllowed()") {
                    self.hide(INCREASE);
                } else {
                    for i in [INCREASE, FOLLOW, STOP] {
                        self.show(i);
                    }
                }
                for i in [
                    CASH[0], CASH[1], CASH[2], CASH[3], POT, CARD[3][0], CARD[3][1],
                ] {
                    self.show(i);
                }
                self.state = 3;
            }
            4 => self.elliot(host),
            6 => self.fisher(host),
            8 => self.punter(host),
            0xa => {
                if Self::action(host, 2) == 0 {
                    let min = Self::min_bet(host);
                    let t = host.string("follow_elliot_bet");
                    self.set_text(FOLLOW, money(&format!("{t} ("), min) + ")");
                    self.hide(ROOT);
                    self.show_cursor();
                    self.show(CHOICE_BG);
                    if Self::cash(host, 1) < min {
                        self.hide(FOLLOW);
                    }
                    self.hide(INCREASE);
                    for i in [
                        CASH[0], CASH[1], CASH[2], CASH[3], POT, CARD[3][0], CARD[3][1],
                    ] {
                        self.show(i);
                    }
                    self.state = 0xb;
                } else {
                    self.state = 0xc;
                }
            }
            0xc => {
                let round = Self::get(host, "return Atl_PuzzlePoker_GetRound()");
                if round > 3 {
                    self.state = 0xe;
                    self.show_cursor();
                    self.showdown(host);
                } else {
                    host.call(
                        "return Atl_PuzzlePoker_SetRound",
                        Some(&[Value::Num(f64::from(round + 1))]),
                        0,
                    );
                    self.state = 2;
                }
            }
            0xe => self.showdown(host),
            0xf => {
                self.cursor = false;
                self.hide(ROOT);
                return true;
            }
            _ => {}
        }
        false
    }

    /// State 0's places (@ 0x00424109..0x0042443f).
    fn layout(&mut self, measure: &dyn Fn(&str, &str, i32, bool) -> i32) {
        self.place(BUBBLE[0], 0xbe, 10);
        self.place(BUBBLE[1], 0x1d6, 10);
        self.place(BUBBLE[2], 0x2f8, 10);
        for set in &CARD[..3] {
            self.place(set[0], 5, 0x14);
            self.place(set[1], 0x55, 0x14);
        }
        self.place(CARD[3][0], W - 0xfa, H - 100);
        self.place(CARD[3][1], W - 0xaa, H - 100);
        self.place(WINNER_BG, W / 2, H / 2);
        self.place(PIC_WINNER, -150, -15);
        // @ 0x00461800(400, ..., 0) and (0x17c, ..., 0): not partitioned.
        let (f, t, _) = self.text(NEXT_GAME);
        let h = measure(f, &t, 400, false);
        self.place(NEXT_GAME, -200, -(h / 2));
        self.place(TEXT_WINNER, -180, 20);
        self.place(CASH[0], W - 100, H - 0x20);
        self.place(CASH[1], 0xbe, 10);
        self.place(CASH[2], 0x1d6, 10);
        self.place(CASH[3], 0x2f8, 10);
        self.place(CHOICE_BG, 10, H - 0x82);
        self.place(INCREASE, 0x14, 0x14);
        // The second measure reads `increase_bet`'s text again (+0x68,
        // 0x0042439c).
        let (f, t, _) = self.text(INCREASE);
        let h1 = measure(f, &t, 0x17c, false);
        self.place(FOLLOW, 0x14, h1 + 0x1e);
        self.place(STOP, 0x14, h1 + 0x1e + h1 + 10);
        self.place(POT, W / 2, H / 2 + 0x78);
    }

    /// Elliot's turn (state 4).
    fn elliot(&mut self, host: &mut dyn Host) {
        if Self::action(host, 2) == 2 {
            self.state = 6;
            return;
        }
        let a = Self::scenario_action(host, 1);
        Self::set_action(host, 2, a);
        let x = Self::action(host, 2);
        if x == 0 || Self::flag(host, "return Atl_PuzzlePoker_IsElliotBetIncreaseDeffered()") {
            if Self::flag(host, "return Atl_PuzzlePoker_IsBetIncreaseAllowed()") {
                host.call("Atl_PuzzlePoker_IncreaseBet()", None, 0);
                if Self::action(host, 1) != 2 {
                    self.dialog(276);
                }
            } else {
                Self::set_action(host, 2, 1);
                host.call(
                    "return Atl_PuzzlePoker_ElliotBetIncreaseDeffered",
                    Some(&[Value::Bool(true)]),
                    0,
                );
                if Self::action(host, 1) != 2 {
                    let id = [275, 176, 177][Self::third(host).min(2) as usize];
                    self.dialog(id);
                }
            }
            self.ops.push_back(Op::PayBet(2));
        } else if x == 1 {
            self.pay_bet(host, 2);
            if Self::action(host, 1) != 2 {
                let id = [275, 176, 177][Self::third(host).min(2) as usize];
                self.dialog(id);
            }
        } else if Self::action(host, 1) != 2 {
            self.dialog(274);
        }
        self.ops.push_back(Op::State(6));
    }

    /// The fisherman's turn (state 6).
    fn fisher(&mut self, host: &mut dyn Host) {
        if Self::action(host, 3) != 2 {
            let a = Self::scenario_action(host, 2);
            Self::set_action(host, 3, a);
            match Self::action(host, 3) {
                1 => {
                    let rand_dialog = if Self::cash(host, 3) < Self::bet(host) {
                        if Self::flag(host, "return Atl_PuzzlePoker_IsBasReliefBidden()") {
                            Self::set_action(host, 3, 1);
                            true
                        } else {
                            self.ops
                                .push_back(Op::Run("Atl_PuzzlePoker_BetBasRelief()".into(), None));
                            false
                        }
                    } else {
                        self.pay_bet(host, 3);
                        true
                    };
                    if rand_dialog && Self::action(host, 1) != 2 {
                        let id = [283, 181, 182][Self::third(host).min(2) as usize];
                        self.dialog(id);
                    }
                }
                2 if Self::action(host, 1) != 2 => self.dialog(282),
                _ => {}
            }
        }
        self.ops.push_back(Op::State(8));
    }

    /// The punter's turn (state 8).
    fn punter(&mut self, host: &mut dyn Host) {
        if Self::action(host, 4) != 2 {
            let a = Self::scenario_action(host, 3);
            Self::set_action(host, 4, a);
            match Self::action(host, 4) {
                1 => {
                    self.pay_bet(host, 4);
                    if Self::action(host, 1) != 2 {
                        let id = [280, 178, 179][Self::third(host).min(2) as usize];
                        self.dialog(id);
                    }
                }
                2 if Self::action(host, 1) != 2 => self.dialog(279),
                _ => {}
            }
        }
        self.ops.push_back(Op::AfterPunter);
    }

    /// The showdown (state 0xe, @ 0x004253e3..0x00425687).
    fn showdown(&mut self, host: &mut dyn Host) {
        let winner = Self::get(host, "return Atl_PuzzlePoker_GetWinner()");
        let game = Self::get(host, "return Atl_PuzzlePoker_GetGame()");
        self.hide(ROOT);
        for i in [BUBBLE[0], BUBBLE[1], BUBBLE[2], CARD[3][0], CARD[3][1]] {
            self.show(i);
        }
        // pic_winner's file (+0x48): the winner's of howard, elliot,
        // fisher, punter (the table at 0x0066c010..).
        if let Some(b) = usize::try_from(winner - 1)
            .ok()
            .and_then(|k| self.winners.get(k))
        {
            let bank = b.clone();
            if let Kind::Sprite(file, _) = &mut self.els[PIC_WINNER].kind {
                *file = bank;
            }
        } else {
            log::warn!("poker: winner {winner}, no picture");
        }
        self.show(WINNER_BG);
        self.show(NEXT_GAME);
        host.call(
            "return Atl_PuzzlePoker_GivePotTo",
            Some(&[Value::Num(f64::from(winner))]),
            0,
        );
        self.show_pot(host);
        self.show_cash(host, winner);
        host.call("Atl_PuzzlePoker_DisplayCards()", None, 0);
        let lose = |p: &mut Poker, cause: i32| {
            p.ops.push_back(Op::Run(
                "return Atl_PuzzlePoker_OnLose".into(),
                Some(vec![Value::Num(f64::from(cause))]),
            ));
            p.ops.push_back(Op::State(0xf));
        };
        if game == 5 {
            if winner == 1 {
                self.state = 0xd;
            } else {
                lose(self, 1);
            }
            return;
        }
        if Self::cash(host, 1) == 0 {
            lose(self, 2);
            return;
        }
        match (game, winner) {
            (4, 3) => {
                self.dialog(285);
                lose(self, 3);
                return;
            }
            (2, 4) => self.dialog(281),
            (3, 2) => self.dialog(277),
            _ => {}
        }
        self.ops.push_back(Op::State(0xd));
    }

    /// The draw (@ 0x00425720): nothing in state 0xf; else each element
    /// of the list that is shown, with its shown children.
    pub fn draw(&self, out: &mut [u32], fonts: &Fonts) {
        if self.state == 0xf {
            return;
        }
        for i in DRAW {
            self.draw_el(i, out, fonts);
        }
    }

    fn draw_el(&self, i: usize, out: &mut [u32], fonts: &Fonts) {
        let e = &self.els[i];
        if !e.visible {
            return;
        }
        let (x, y) = self.abs(i);
        match &e.kind {
            Kind::Root => {}
            Kind::Sprite(Some(b), f) => {
                let mut t = Target::screen(out, VIEW_WIDTH, VIEW_HEIGHT);
                b.draw(&mut t, Some(x), y, *f);
            }
            Kind::Sprite(None, _) => {}
            Kind::Text {
                font,
                text,
                width,
                color,
                hover,
                align,
                over,
            } => {
                let c = if *over {
                    hover.unwrap_or(*color)
                } else {
                    *color
                };
                let h = fonts.text_height(font, text, *width);
                let zone = Zone::new(0, 0, 0, 0, halign(*align), 0);
                let lines = fonts.lines(font, text, *width, h.max(1), zone);
                fonts.draw(font, &lines, rgb(c), out, VIEW_WIDTH, VIEW_HEIGHT, x, y);
            }
        }
        for &c in &e.children {
            self.draw_el(c, out, fonts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants() {
        // 3/32767 as a float: rand 32767 gives 2.
        let k = f64::from(f32::from_bits(0x38c0_0180));
        assert!((k - 3.0 / 32767.0).abs() < 1e-9);
        #[allow(clippy::cast_possible_truncation)]
        let top = (32767.0 * k) as u32;
        assert_eq!(top, 2);
        assert_eq!(money("", 80), "80$");
        assert_eq!(action_code(Some(&Value::Str(b"follow_bet".to_vec()))), 1);
        assert_eq!(action_code(Some(&Value::Str(b"none".to_vec()))), 3);
        assert_eq!(action_code(None), 3);
    }
}
