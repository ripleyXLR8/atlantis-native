//! `TeFont2`'s kerning map, as atlantis4-gog-en-2363910 builds and reads
//! it: an MSVC 2010 `std::map<(u32 c1, u32 c2), vec3>` whose comparator
//! (@ 0x00441790, inlined in 0x00441680 / 0x00441720 / 0x00441b60) is
//!
//! ```text
//! less(a, b) = a.c1 <= b.c1 && a.c2 < b.c2
//! ```
//!
//! which is **not** a lexicographic order. `TeFont2::load` (@ 0x004a0350)
//! fills it in file order with `map[key] = (dx, dy, 0)` (`operator[]` @
//! 0x00441680: a lower bound, then the hinted insert @ 0x00441b60 /
//! 0x00441ce0 and the red-black fix-up @ 0x00441dc0 unless the key is
//! "equal"; a later entry overwrites the value of an equal one). The line
//! builder and the drawer read it with `find` (@ 0x00441720): the lower
//! bound, "found" unless `less(key, node)`. With this comparator most
//! pairs that are not in the file find some stored pair, so nearly every
//! pair of bytes is kerned (mostly by −1).
//!
//! [`KernMap`] is that tree, transcribed with its insertion order and
//! rotations, since the shape decides what `find` returns.

/// `less` @ 0x00441790.
fn less(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0 <= b.0 && a.1 < b.1
}

#[derive(Clone, Copy, Debug)]
struct Node {
    left: usize,
    parent: usize,
    right: usize,
    key: (u32, u32),
    value: (f32, f32),
    red: bool,
}

/// The head node (`_Myhead`): its parent is the root, its left the
/// leftmost node, its right the rightmost; it is also every leaf's nil.
const HEAD: usize = 0;

/// The kerning `std::map` of a font.
#[derive(Clone, Debug)]
pub struct KernMap {
    nodes: Vec<Node>,
}

impl Default for KernMap {
    fn default() -> Self {
        KernMap {
            nodes: vec![Node {
                left: HEAD,
                parent: HEAD,
                right: HEAD,
                key: (0, 0),
                value: (0.0, 0.0),
                red: false,
            }],
        }
    }
}

impl KernMap {
    /// The map filled as `TeFont2::load` does: `map[key] = value` for each
    /// pair in file order.
    pub fn from_pairs(pairs: impl IntoIterator<Item = ((u32, u32), (f32, f32))>) -> KernMap {
        let mut m = KernMap::default();
        for (k, v) in pairs {
            m.set(k, v);
        }
        m
    }

    /// The number of nodes (`_Mysize`).
    pub fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    /// Whether the map has no node.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn root(&self) -> usize {
        self.nodes[HEAD].parent
    }

    fn key(&self, x: usize) -> (u32, u32) {
        self.nodes[x].key
    }

    /// `++iterator`.
    fn next(&self, mut x: usize) -> usize {
        let n = &self.nodes;
        if n[x].right != HEAD {
            x = n[x].right;
            while n[x].left != HEAD {
                x = n[x].left;
            }
            return x;
        }
        let mut p = n[x].parent;
        while p != HEAD && x == n[p].right {
            x = p;
            p = n[p].parent;
        }
        p
    }

    /// `--iterator` (from the head: the rightmost node).
    fn prev(&self, mut x: usize) -> usize {
        let n = &self.nodes;
        if x == HEAD {
            return n[HEAD].right;
        }
        if n[x].left != HEAD {
            x = n[x].left;
            while n[x].right != HEAD {
                x = n[x].right;
            }
            return x;
        }
        let mut p = n[x].parent;
        while p != HEAD && x == n[p].left {
            x = p;
            p = n[p].parent;
        }
        if p == HEAD {
            x
        } else {
            p
        }
    }

    /// `_Lbound`.
    fn lower_bound(&self, k: (u32, u32)) -> usize {
        let mut w = HEAD;
        let mut t = self.root();
        while t != HEAD {
            if less(self.key(t), k) {
                t = self.nodes[t].right;
            } else {
                w = t;
                t = self.nodes[t].left;
            }
        }
        w
    }

    /// `find` @ 0x00441720: the node's index, `None` for `end()`.
    fn find(&self, k: (u32, u32)) -> Option<usize> {
        let w = self.lower_bound(k);
        (w != HEAD && !less(k, self.key(w))).then_some(w)
    }

    /// The `(dx, dy)` that `find` gives for (`c1`, `c2`), zero for `end()`.
    pub fn kern(&self, c1: u32, c2: u32) -> (f32, f32) {
        self.find((c1, c2))
            .map_or((0.0, 0.0), |x| self.nodes[x].value)
    }

    fn rotate_left(&mut self, w: usize) {
        let p = self.nodes[w].right;
        let pl = self.nodes[p].left;
        self.nodes[w].right = pl;
        if pl != HEAD {
            self.nodes[pl].parent = w;
        }
        let wp = self.nodes[w].parent;
        self.nodes[p].parent = wp;
        if w == self.root() {
            self.nodes[HEAD].parent = p;
        } else if w == self.nodes[wp].left {
            self.nodes[wp].left = p;
        } else {
            self.nodes[wp].right = p;
        }
        self.nodes[p].left = w;
        self.nodes[w].parent = p;
    }

    fn rotate_right(&mut self, w: usize) {
        let p = self.nodes[w].left;
        let pr = self.nodes[p].right;
        self.nodes[w].left = pr;
        if pr != HEAD {
            self.nodes[pr].parent = w;
        }
        let wp = self.nodes[w].parent;
        self.nodes[p].parent = wp;
        if w == self.root() {
            self.nodes[HEAD].parent = p;
        } else if w == self.nodes[wp].right {
            self.nodes[wp].right = p;
        } else {
            self.nodes[wp].left = p;
        }
        self.nodes[p].right = w;
        self.nodes[w].parent = p;
    }

    /// `_Insert` @ 0x00441dc0: a red node as `wh`'s left or right child,
    /// then the red-black fix-up.
    fn link(&mut self, left: bool, wh: usize, k: (u32, u32)) -> usize {
        let x = self.nodes.len();
        self.nodes.push(Node {
            left: HEAD,
            parent: wh,
            right: HEAD,
            key: k,
            value: (0.0, 0.0),
            red: true,
        });
        if wh == HEAD {
            let h = &mut self.nodes[HEAD];
            h.parent = x;
            h.left = x;
            h.right = x;
        } else if left {
            self.nodes[wh].left = x;
            if wh == self.nodes[HEAD].left {
                self.nodes[HEAD].left = x;
            }
        } else {
            self.nodes[wh].right = x;
            if wh == self.nodes[HEAD].right {
                self.nodes[HEAD].right = x;
            }
        }
        let mut p = x;
        loop {
            let pp = self.nodes[p].parent;
            if !self.nodes[pp].red {
                break;
            }
            let ppp = self.nodes[pp].parent;
            if pp == self.nodes[ppp].left {
                let w = self.nodes[ppp].right;
                if self.nodes[w].red {
                    self.nodes[pp].red = false;
                    self.nodes[w].red = false;
                    self.nodes[ppp].red = true;
                    p = ppp;
                } else {
                    if p == self.nodes[pp].right {
                        p = pp;
                        self.rotate_left(p);
                    }
                    let pp = self.nodes[p].parent;
                    let ppp = self.nodes[pp].parent;
                    self.nodes[pp].red = false;
                    self.nodes[ppp].red = true;
                    self.rotate_right(ppp);
                }
            } else {
                let w = self.nodes[ppp].left;
                if self.nodes[w].red {
                    self.nodes[pp].red = false;
                    self.nodes[w].red = false;
                    self.nodes[ppp].red = true;
                    p = ppp;
                } else {
                    if p == self.nodes[pp].left {
                        p = pp;
                        self.rotate_right(p);
                    }
                    let pp = self.nodes[p].parent;
                    let ppp = self.nodes[pp].parent;
                    self.nodes[pp].red = false;
                    self.nodes[ppp].red = true;
                    self.rotate_left(ppp);
                }
            }
        }
        let r = self.root();
        self.nodes[r].red = false;
        x
    }

    /// `insert(value)` @ 0x00441ce0 (unhinted).
    fn insert(&mut self, k: (u32, u32)) -> usize {
        let mut t = self.root();
        let mut wh = HEAD;
        let mut left = true;
        while t != HEAD {
            wh = t;
            left = less(k, self.key(t));
            t = if left {
                self.nodes[t].left
            } else {
                self.nodes[t].right
            };
        }
        let mut w = wh;
        if left {
            if w == self.nodes[HEAD].left {
                return self.link(true, wh, k);
            }
            w = self.prev(w);
        }
        if less(self.key(w), k) {
            return self.link(left, wh, k);
        }
        w
    }

    /// `insert(hint, value)` @ 0x00441b60.
    fn insert_hint(&mut self, w: usize, k: (u32, u32)) -> usize {
        if self.is_empty() {
            return self.link(true, HEAD, k);
        }
        let h = self.nodes[HEAD];
        if w == h.left {
            if less(k, self.key(w)) {
                return self.link(true, w, k);
            }
        } else if w == HEAD {
            if less(self.key(h.right), k) {
                return self.link(false, h.right, k);
            }
        } else {
            if less(k, self.key(w)) {
                let p = self.prev(w);
                if less(self.key(p), k) {
                    return if self.nodes[p].right == HEAD {
                        self.link(false, p, k)
                    } else {
                        self.link(true, w, k)
                    };
                }
            }
            if less(self.key(w), k) {
                let n = self.next(w);
                if n == HEAD || less(k, self.key(n)) {
                    return if self.nodes[w].right == HEAD {
                        self.link(false, w, k)
                    } else {
                        self.link(true, n, k)
                    };
                }
            }
        }
        self.insert(k)
    }

    /// `map[k] = v` (`operator[]` @ 0x00441680, then the value written).
    pub fn set(&mut self, k: (u32, u32), v: (f32, f32)) {
        let mut w = self.lower_bound(k);
        if w == HEAD || less(k, self.key(w)) {
            w = self.insert_hint(w, k);
        }
        self.nodes[w].value = v;
    }
}

/// The map's lookups for every pair of bytes, `[c1 * 256 + c2]`, made
/// once at load by [`KernMap::kern`].
#[derive(Clone, Debug)]
pub struct KernTable {
    map: KernMap,
    table: Vec<(f32, f32)>,
}

impl KernTable {
    pub fn new(map: KernMap) -> KernTable {
        let mut table = Vec::with_capacity(65536);
        for c1 in 0..256 {
            for c2 in 0..256 {
                table.push(map.kern(c1, c2));
            }
        }
        KernTable { map, table }
    }

    /// What `find` gives for (`c1`, `c2`).
    pub fn kern(&self, c1: u32, c2: u32) -> (f32, f32) {
        if c1 < 256 && c2 < 256 {
            self.table[(c1 * 256 + c2) as usize]
        } else {
            self.map.kern(c1, c2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pair_kerns_the_keys_it_does_not_order() {
        // One node (A, V): `find` returns it unless the key is "less" on
        // either side.
        let m = KernMap::from_pairs([((65, 86), (-2.0, 0.0))]);
        assert_eq!(m.kern(65, 86), (-2.0, 0.0));
        // (A, B): less((A, B), (A, V)): end().
        assert_eq!(m.kern(65, 66).0, 0.0);
        // (B, A): neither is less: found.
        assert_eq!(m.kern(66, 65).0, -2.0);
        // (B, W): less((A, V), (B, W)): end().
        assert_eq!(m.kern(66, 87).0, 0.0);
    }

    #[test]
    fn a_repeated_pair_keeps_the_last_value() {
        let m = KernMap::from_pairs([((1, 2), (-1.0, 0.0)), ((1, 2), (-3.0, 0.0))]);
        assert_eq!(m.len(), 1);
        assert_eq!(m.kern(1, 2).0, -3.0);
    }
}
