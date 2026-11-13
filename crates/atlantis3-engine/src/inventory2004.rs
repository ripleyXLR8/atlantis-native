//! *Atlantis Evolution*'s 2004 CD program (atlantis4-cd-en-3022848): the
//! bag, the inventory screen, the object in the hand, the compositions
//! and the followers. Every address below is of that program.
//!
//! The bag is a circular list (head 0x006e7d4c, count 0x006e7d50) of
//! nodes (id +8, sprite +0xc, name +0x10) in the order the objects came
//! (`AtlInventaire_AddObjet` FUN_0042a980: appended, a duplicate refused
//! with a log; `RemoveObjet` FUN_0042aaf0). Unlike *Atlantis III*'s it has
//! no world and no mirror in the flags (the node's constructor FUN_0042c360
//! stores id, sprite and name only).
//!
//! The inventory (loop mode 8, opened by the right button in the warp,
//! FUN_00428690) is drawn by FUN_00428940: the warp, the followers
//! (FUN_0043d000), the items (FUN_0042af70), the bar `Interface.spr` #0
//! centred on (W/2, H − 45), then the object in the hand at the pointer
//! (FUN_004298d0) or the pointer (FUN_00429460). The left button takes an
//! item into the hand, puts it back, or combines the hand's object with
//! the item (`#COMPOSITIONS#`); the right button on an item shows it big
//! (`inventbig.jpg` and the object's `big_*.spr`), elsewhere it closes the
//! inventory with the object kept in the hand.

use atlantis2_engine::script::BagEntry;
pub use atlantis2_engine::script::FollowerOp;

/// The bar's layout by screen width (FUN_0042a3e0, from the inventory's
/// init FUN_0042a230): the visible slots (0x006e9d54), the slot width
/// (0x006ae548), the bar's height (0x006ae54c) and the left margin
/// (0x006e9d58). At 1024 the slots are 84 pixels wide, 11 of them, after
/// 48 pixels; another width keeps the values (the data's 80 and 78, no
/// slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub slots: i32,
    pub slot: i32,
    pub bar: i32,
    pub margin: i32,
}

impl Layout {
    pub fn for_width(width: usize) -> Layout {
        match width {
            640 => Layout {
                slots: 8,
                slot: 80,
                bar: 78,
                margin: 0,
            },
            800 => Layout {
                slots: 10,
                slot: 80,
                bar: 78,
                margin: 0,
            },
            1024 => Layout {
                slots: 11,
                slot: 84,
                bar: 78,
                margin: 48,
            },
            _ => Layout {
                slots: 0,
                slot: 80,
                bar: 78,
                margin: 0,
            },
        }
    }

    /// The centre of item slot `j` (FUN_0042af70 @ 0x0042b004..0x0042b034):
    /// x = margin + slot · j + slot / 2, y = H − 5 − slot / 2 (the halves
    /// by `cdq; sub; sar 1`).
    pub fn item_centre(&self, j: i32, height: usize) -> (i32, i32) {
        let half = self.slot / 2;
        (self.margin + self.slot * j + half, height as i32 - 5 - half)
    }

    /// The bag's index under the pointer (FUN_0042b170
    /// @ 0x0042b2fb..0x0042b352): py > H − bar, 0 < px − margin < slot ·
    /// slots; (px − margin) / slot + scroll.
    pub fn index_at(&self, height: usize, pointer: (i32, i32), scroll: i32) -> Option<i32> {
        let (px, py) = pointer;
        let d = px - self.margin;
        (py > height as i32 - self.bar && d > 0 && d < self.slot * self.slots)
            .then(|| d / self.slot + scroll)
    }
}

/// The bar's y centre (FUN_00428940 @ 0x004289c2..0x004289ec: H − 0x2d).
pub fn bar_centre_y(height: usize) -> i32 {
    height as i32 - 45
}

/// The items drawn (FUN_0042af70): walking the bag in order, entry i is
/// in slot j = i − scroll while scroll ≤ i and fewer than `slots` slots
/// were used; the object in the hand (`OBJETENMAIN`) keeps its slot but
/// is not drawn. (entry, slot) pairs.
pub fn drawn_items(bag: &[BagEntry], lay: &Layout, scroll: i32, hand: i32) -> Vec<(BagEntry, i32)> {
    let mut out = Vec::new();
    let mut j = 0;
    for (i, e) in bag.iter().enumerate() {
        if scroll <= i as i32 && j < lay.slots {
            if e.id != hand {
                out.push((*e, j));
            }
            j += 1;
        }
    }
    out
}

/// The arrows (FUN_0042af70 @ 0x0042b077..0x0042b0ea, `fleche.SPR`,
/// top-left): #0 at (0, H − 110) when scrolled, #1 at (W − 16, H − 110)
/// when scroll < count − slots.
pub fn arrows(
    lay: &Layout,
    scroll: i32,
    count: usize,
    width: usize,
    height: usize,
) -> Vec<(usize, i32, i32)> {
    let mut out = Vec::new();
    let y = height as i32 - 110;
    if scroll > 0 {
        out.push((0, 0, y));
    }
    if scroll < count as i32 - lay.slots {
        out.push((1, width as i32 - 16, y));
    }
    out
}

/// What a frame's buttons did in the inventory (FUN_00428940
/// @ 0x00428b60..0x00428f84), the pick FUN_0042b170's answer `pick` (an
/// object id, −1 nothing, −2 a follower chosen or the right button above
/// the bar), the object in the hand `hand` (`OBJETENMAIN`, 0 none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The right button with −2: the inventory closed (FUN_00428750), the
    /// hand's object put back in the hand (FUN_00429600) when there was
    /// one, `CtrlScenarIN` (FUN_00450d30).
    Close,
    /// The right button on an item: shown big (0x006ae550 = id).
    Examine(i32),
    /// The left button while an object is shown big: the view ended
    /// (@ 0x00428d6c..0x00428e06); the click goes no further.
    EndExamine,
    /// The left button with an empty hand on an item: in the hand
    /// (FUN_00429600).
    Hold(i32),
    /// The left button with an object in the hand on nothing, or on the
    /// same object: the hand emptied (FUN_00429a20).
    EmptyHand,
    /// The left button with an object in the hand on another item: the
    /// composition of the two (FUN_0042ab80), when there is one.
    Combine(i32, i32),
}

/// The buttons' outcome for a frame (FUN_00428940): the right button
/// first, then the left one.
pub fn action(right: bool, left: bool, examining: bool, pick: i32, hand: i32) -> Vec<Action> {
    let mut out = Vec::new();
    if right {
        if pick == -2 {
            out.push(Action::Close);
        } else if pick > -1 {
            out.push(Action::Examine(pick));
            // 0x006ae550 is set: the left button below ends it again.
            return finish_left(out, left, true, pick, hand);
        }
    }
    finish_left(out, left, examining, pick, hand)
}

fn finish_left(
    mut out: Vec<Action>,
    left: bool,
    examining: bool,
    pick: i32,
    hand: i32,
) -> Vec<Action> {
    if !left {
        return out;
    }
    if examining {
        out.push(Action::EndExamine);
        return out;
    }
    if hand == 0 {
        if pick > -1 {
            out.push(Action::Hold(pick));
        }
    } else if hand > 0 {
        if pick == -1 || pick == hand {
            out.push(Action::EmptyHand);
        } else if pick > -1 {
            out.push(Action::Combine(hand, pick));
        }
    }
    out
}

/// One line of `#COMPOSITIONS#` (section 8 of `defnames.con`): the two
/// objects combined (in either order) and the objects they give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub a: Vec<u8>,
    pub b: Vec<u8>,
    pub products: Vec<Vec<u8>>,
}

/// The compositions of `defnames.con`'s text: each line between
/// `#COMPOSITIONS#` and the next tag that is not empty nor a `//`
/// comment, its comma-separated names (blanks before a name skipped, a
/// name ends at a blank, a comma, a `/` or a `)`, as FUN_00467620 copies
/// it).
pub fn compositions(defnames: &[u8]) -> Vec<Composition> {
    let text = String::from_utf8_lossy(defnames);
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let l = line.trim_start_matches([' ', '\t']).trim_end_matches('\r');
        if l.starts_with('#') {
            inside = l.starts_with("#COMPOSITIONS#");
            continue;
        }
        if !inside || l.is_empty() || l.starts_with("//") {
            continue;
        }
        let names: Vec<Vec<u8>> = l
            .split(',')
            .map(|f| {
                f.trim_start_matches([' ', '\t'])
                    .bytes()
                    .take_while(|&c| {
                        !matches!(c, b'\r' | b'\n' | 0 | b' ' | b',' | b'\t' | b'/' | b')')
                    })
                    .collect::<Vec<u8>>()
            })
            .collect();
        if names.len() < 2 {
            continue;
        }
        out.push(Composition {
            a: names[0].clone(),
            b: names[1].clone(),
            products: names[2..]
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect(),
        });
    }
    out
}

/// FUN_0042ab80(a, b) and FUN_0042ad40: the first line whose first name
/// is `a` and second `b`, or first `b` and second `a` (FUN_00467db0: the
/// line's leading name of the same length, equal by `_strnicmp`); its
/// products (FUN_0042ab80 counts the parameters from the third on,
/// FUN_0042ad40 gives the i-th). `None` without a line.
pub fn combine<'a>(list: &'a [Composition], a: &[u8], b: &[u8]) -> Option<&'a [Vec<u8>]> {
    let eq = |x: &[u8], y: &[u8]| x.eq_ignore_ascii_case(y);
    list.iter()
        .find(|c| (eq(&c.a, a) && eq(&c.b, b)) || (eq(&c.a, b) && eq(&c.b, a)))
        .map(|c| c.products.as_slice())
}

/// A follower (`ADDSUIVEUR=NAME`, FUN_0043d240): a node of the list
/// 0x006ffc4c (count 0x006ffc50) with the name, the `#PERSONNAGES#`
/// parameters 1 (+0x14, the frame when disabled) and 2 (+0x18, enabled)
/// of `suiveurs.spr`, and the enabled byte (+0x24 of the node, 0 at the
/// add).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Follower {
    pub name: String,
    pub frame_off: i32,
    pub frame_on: i32,
    pub enabled: bool,
}

/// Where follower k is drawn (FUN_0043d000 @ 0x0043d014..0x0043d06e:
/// `AtlSpr_Aff(suiveurs.spr, frame, 30 + 70 k, 30, 1.0)`, top-left) and
/// its frame (+0x18 when enabled, else +0x14).
pub fn follower_draws(list: &[Follower]) -> Vec<(i32, i32, i32)> {
    list.iter()
        .enumerate()
        .map(|(k, f)| {
            let frame = if f.enabled { f.frame_on } else { f.frame_off };
            (frame, 30 + 70 * k as i32, 30)
        })
        .collect()
}

/// The follower clicked (FUN_0043d080): the first enabled one whose
/// sprite at its place (30 + 70 k, 30) is opaque under the pointer,
/// tested with its frame +0x14 (parameter 1, @ 0x0043d0c4: the drawn
/// +0x18 of an enabled follower is not the one tested). `opaque(frame, x,
/// y)` is the sprite test at the pointer.
pub fn follower_hit(list: &[Follower], opaque: impl Fn(i32, i32, i32) -> bool) -> Option<usize> {
    list.iter()
        .zip(follower_draws(list))
        .position(|(f, (_, x, y))| f.enabled && opaque(f.frame_off, x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FUN_0043d080 tests an enabled follower with its parameter 1
    /// frame (+0x14), though it is drawn with parameter 2 (+0x18).
    #[test]
    fn follower_hit_uses_the_first_frame() {
        let f = |enabled| Follower {
            name: "MIRANDA".into(),
            frame_off: 1,
            frame_on: 2,
            enabled,
        };
        let list = [f(false), f(true)];
        assert_eq!(follower_draws(&list)[1], (2, 100, 30));
        // Only frame 1 is opaque at the pointer: the enabled follower 1.
        assert_eq!(follower_hit(&list, |frame, _, _| frame == 1), Some(1));
        assert_eq!(follower_hit(&list, |frame, _, _| frame == 2), None);
        // Follower 0 (disabled) is never hit; 1 only at its own place.
        assert_eq!(follower_hit(&list, |_, x, _| x == 30), None);
        assert_eq!(follower_hit(&list, |_, x, _| x == 100), Some(1));
    }

    fn e(id: i32) -> BagEntry {
        BagEntry {
            sprite: id as u8,
            id,
            world: 0,
        }
    }

    #[test]
    fn layout_1024() {
        let l = Layout::for_width(1024);
        assert_eq!(l.item_centre(0, 768), (48 + 42, 768 - 47));
        assert_eq!(l.item_centre(2, 768), (48 + 168 + 42, 721));
        assert_eq!(l.index_at(768, (49, 700), 0), Some(0));
        assert_eq!(l.index_at(768, (48, 700), 0), None);
        assert_eq!(l.index_at(768, (49, 690), 0), None);
        assert_eq!(l.index_at(768, (48 + 84, 700), 1), Some(2));
        assert_eq!(l.index_at(768, (48 + 84 * 11, 700), 0), None);
        assert_eq!(bar_centre_y(768), 723);
    }

    #[test]
    fn the_hand_keeps_its_slot() {
        let l = Layout::for_width(1024);
        let bag = [e(5), e(11), e(9)];
        assert_eq!(drawn_items(&bag, &l, 0, 11), vec![(e(5), 0), (e(9), 2)]);
        assert_eq!(drawn_items(&bag, &l, 1, 0), vec![(e(11), 0), (e(9), 1)]);
    }

    #[test]
    fn buttons() {
        use Action::*;
        assert_eq!(action(false, true, false, 7, 0), vec![Hold(7)]);
        assert_eq!(action(false, true, false, -1, 0), vec![]);
        assert_eq!(action(false, true, false, -1, 7), vec![EmptyHand]);
        assert_eq!(action(false, true, false, 7, 7), vec![EmptyHand]);
        assert_eq!(action(false, true, false, 8, 7), vec![Combine(7, 8)]);
        assert_eq!(action(false, true, false, -2, 7), vec![]);
        assert_eq!(action(true, false, false, -2, 7), vec![Close]);
        assert_eq!(action(true, false, false, 8, 0), vec![Examine(8)]);
        assert_eq!(action(true, false, false, -1, 0), vec![]);
        assert_eq!(action(false, true, true, 8, 0), vec![EndExamine]);
    }

    #[test]
    fn the_cd_compositions() {
        let text = b"#ZONES_CLIC#\r\nX\r\n#COMPOSITIONS#\r\n//c\r\nALLUMETTES,LANTERNE,ALLUMETTES,LANTERNE_ALLUMEE\r\nCORDE,SEAU2,CORDE_SEAU\r\n\r\n#END_OF_FILE#\r\n";
        let c = compositions(text);
        assert_eq!(c.len(), 2);
        let p = combine(&c, b"lanterne", b"ALLUMETTES").unwrap();
        assert_eq!(p, [b"ALLUMETTES".to_vec(), b"LANTERNE_ALLUMEE".to_vec()]);
        assert!(combine(&c, b"LANTERNE", b"CORDE").is_none());
    }
}
