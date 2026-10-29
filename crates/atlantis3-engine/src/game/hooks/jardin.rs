//! Bagdad's garden (`jardin.wam`, world 3): the pictures restored from the
//! soldiers' flags, the monkey's moves, the banana bunch (view 19), the
//! soldiers' pot and stones shown at the end of their animations. The
//! puzzles themselves are scripted (chapters 31, 32, 36..41); the jewel
//! case's start is in the zone hook (hooks.rs).

use super::Hx;

#[derive(Debug, Clone)]
pub struct State {
    /// The monkey's reload byte (@ 0x0071f0c0, 1 at start and after a
    /// load) and his place (@ 0x0071f0c4: view 20 or 100, 20 at start).
    pub monkey_reload: bool,
    pub monkey_at: i32,
    /// The banana bunch: its reload byte (@ 0x0071d3d8), done
    /// (@ 0x00785fd1) and playing (@ 0x00785fd2).
    pub banana_reload: bool,
    pub banana_done: bool,
    pub banana_playing: bool,
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

impl State {
    pub fn new() -> State {
        State {
            monkey_reload: true,
            monkey_at: 20,
            banana_reload: true,
            banana_done: false,
            banana_playing: false,
        }
    }

    /// @ 0x00459e70: 0x0071d3d8 and 0x0071f0c0 := 1.
    pub fn after_load(&mut self) {
        self.banana_reload = true;
        self.monkey_reload = true;
    }
}

/// @ 0x0045fa90(id, anim, kind, mode): mode 1 (`PLAYCUTANIM`): every slot
/// of (id, kind) hidden, then the one of animation `anim` shown; mode 2
/// (`PLAYANIM`): the slot of animation `anim` queued after the shown slot
/// of (id, kind), which plays on to its file's end first.
pub(super) fn play_anim_c(h: &mut Hx, id: i32, anim: i32, kind: i32, mode: i32) {
    let now = h.now();
    let slots = &mut h.g.slots;
    if mode == 1 {
        slots.hide(id, kind, now);
        slots.show(id, kind, anim, now);
        return;
    }
    if mode != 2 {
        return;
    }
    let target = slots
        .slots
        .iter()
        .position(|s| s.id == id && s.kind == kind && s.state.anim as i32 == anim);
    let Some(cur) = slots
        .slots
        .iter()
        .position(|s| s.id == id && s.kind == kind && s.shown)
    else {
        return;
    };
    if let Some(t) = target {
        let s = &mut slots.slots[cur];
        s.last = s.count;
        s.queued = true;
        s.next = Some(t);
        let t = &mut slots.slots[t];
        t.queued = false;
        t.frame = 0;
        t.last = if t.state.end < 1 {
            t.count
        } else {
            t.state.end - 1
        };
    }
}

/// The `jardin.wam` part of the view entry (@ 0x004c2010, 0x004c2370..
/// 0x004c246c).
pub(super) fn view(h: &mut Hx, vue: i32) {
    if h.flag(128) != 0 {
        h.run("(HidePerso=BAG_SOLDAT_BOIS)");
    }
    if h.flag(134) != 0 {
        h.run("(ShowObjet=BAG_POT_TERRE)");
        h.run("(HidePerso=BAG_SOLDAT_TERRE)");
    }
    if h.flag(139) != 0 {
        h.run("(HidePerso=BAG_SOLDAT_PIERRE)");
        h.run("(HideObjet=BAG_PIERRES_GARDE)");
    }
    if h.flag(829) == 0 {
        h.set_flag(829, 1);
        h.play_voice(199, 1, 200, 255, 255);
    }
    monkey(h);
    // The views' jump table (@ 0x004c272c).
    match vue {
        19 => banana_setup(h),
        31 => {
            h.run("(showperso=BAG_HOURISBLEUE_DEDANS)");
            h.run("(hideperso=BAG_HOURISBLEUE_DEHORS)");
        }
        32 => {
            h.run("(showperso=BAG_HOURISVERTE_DEDANS)");
            h.run("(hideperso=BAG_HOURISVERTE_DEHORS)");
        }
        33 => {
            h.run("(showperso=BAG_HOURISROUGE_DEDANS)");
            h.run("(hideperso=BAG_HOURISROUGE_DEHORS)");
        }
        _ => {}
    }
}

/// Whether slot (`name`, n) of this view (a character) has ended
/// (@ 0x0043f7f0 then byte +0x46d). A missing slot is −1 in the original,
/// which then reads the byte before slot 0's (@ 0x007594a9): false here.
fn perso_slot_ended(h: &Hx, name: &str, n: i32) -> bool {
    let id = h.perso(name);
    h.slot_of(id, n).is_some_and(|k| h.g.slots.slots[k].ended)
}

/// The `jardin.wam` part of the frame hook (@ 0x004c27d0, 0x004c2dae..
/// 0x004c2ec0); no zone clear here.
pub(super) fn frame(h: &mut Hx, vue: i32) {
    match vue {
        // The earth soldier beaten (anim 4 = his view-3 state 3).
        3 => {
            if perso_slot_ended(h, "BAG_SOLDAT_TERRE", 3) && h.flag(827) == 0 {
                h.run("(ShowObjet=BAG_POT_TERRE)");
                h.set_flag(827, 1);
            }
        }
        // The stone soldier appeared (his view-6 state 2).
        6 => {
            if perso_slot_ended(h, "BAG_SOLDAT_PIERRE", 2) && h.flag(828) == 0 {
                h.run("(ShowObjet=BAG_PIERRES_GARDE)");
                h.set_flag(828, 1);
            }
        }
        19 => banana_step(h),
        _ => {}
    }
}

/// The monkey (@ 0x004cfaa0, every `jardin.wam` view entry): pictures
/// only, until `BAG_PUZZLESINGEOK` = 1.
fn monkey(h: &mut Hx) {
    let vue = h.vue();
    let id = h.perso("BAG_SINGE");
    if h.flag(912) == 0 {
        h.set_flag(912, 1);
    }
    if h.g.puzzles.jardin.monkey_reload {
        let v = h.flag(804);
        h.set_var("BAG_PUZZLESINGEOK", v);
    }
    if h.var("BAG_PUZZLESINGEOK") == 1 {
        return;
    }
    let at = h.g.puzzles.jardin.monkey_at;
    if vue == 20 && at == 20 {
        play_anim_c(h, id, 2, 1, 1);
        play_anim_c(h, id, 3, 1, 2);
        h.g.puzzles.jardin.monkey_at = 100;
        h.g.puzzles.jardin.monkey_reload = false;
        return;
    }
    if vue == 100 && at == 100 {
        play_anim_c(h, id, 4, 1, 1);
        play_anim_c(h, id, 1, 1, 2);
        h.g.puzzles.jardin.monkey_at = 20;
        h.g.puzzles.jardin.monkey_reload = false;
        return;
    }
    // @ 0x0045f770(id, 1): a slot of the monkey in this view.
    if h.g.slots.slots.iter().any(|s| s.id == id && s.kind == 1) {
        if at == 20 {
            play_anim_c(h, id, 1, 1, 1);
        } else if at == 100 {
            play_anim_c(h, id, 3, 1, 1);
        }
    }
    h.g.puzzles.jardin.monkey_reload = false;
}

/// The banana bunch's set-up (@ 0x004c3f80, view 19's entry).
fn banana_setup(h: &mut Hx) {
    if h.flag(911) == 0 {
        let j = &mut h.g.puzzles.jardin;
        j.banana_done = false;
        j.banana_playing = false;
        j.banana_reload = false;
        h.set_flag(911, 1);
        return;
    }
    if h.g.puzzles.jardin.banana_reload {
        h.g.puzzles.jardin.banana_done = h.flag(802) != 0;
        if h.flag(803) == 1 {
            h.run("(showobjet=BAG_BANANE)");
        }
    }
    h.g.puzzles.jardin.banana_playing = false;
    h.g.puzzles.jardin.banana_reload = false;
}

/// The banana bunch's step (@ 0x004c3ff0, view 19's frame): the pole in
/// hand on the bunch knocks it down (flags 802, 803); at the end of the
/// bunch's anim 1 the banana is shown.
fn banana_step(h: &mut Hx) {
    let bunch = h.obj("BAG_REGIMEBANANE");
    let pole = h.obj("BAG_PERCHE");
    if h.g.puzzles.jardin.banana_done {
        return;
    }
    if !h.g.puzzles.jardin.banana_playing && h.var("CLICOBJET") == bunch && h.hand() == pole {
        play_anim_c(h, bunch, 1, 2, 1);
        play_anim_c(h, bunch, 2, 2, 2);
        h.g.puzzles.jardin.banana_playing = true;
        h.set_flag(802, 1);
        h.set_flag(803, 1);
    } else if !h.g.puzzles.jardin.banana_playing {
        return;
    }
    // @ 0x0045f7c0(id, 2, 1): the bunch's slot of anim 1, its end byte.
    let ended =
        h.g.slots
            .slots
            .iter()
            .find(|s| s.id == bunch && s.kind == 2 && s.state.anim as i32 == 1)
            .is_some_and(|s| s.ended);
    if ended {
        h.run("(showobjet=BAG_BANANE)");
        h.g.puzzles.jardin.banana_playing = false;
        h.g.puzzles.jardin.banana_done = true;
    }
}
