//! The rasteriser of the talking heads: the face set-ups `FUN_00456d88`
//! (affine texture, group type 2), `FUN_004582f8` (perspective texture,
//! type 9) and `FUN_00455cc0` (flat colour, type 1), the row lists
//! `FUN_004567b4`, the row loop `FUN_0045c104` with its front-to-back
//! stack (`FUN_0045b824`) and visible spans (`FUN_0045baa4`), and the span
//! fills of the table 0x004bad3c.
//!
//! The edge records and stack entries of the original are linked by
//! pointers; here they are indices into `DialogHead::edges` and
//! `RasterState::entries`, with [`NONE`] for the null pointer.

use super::{f32r, truncd, wrap32, DialogHead, Edge, NONE, SENTINELS};

/// float 0x004bf044
const EPS: f64 = 100.0;
/// float 0x004baee0 = 1.5 * 2^52
const MAGIC: f64 = 6_755_399_441_055_744.0;

/// `x + 1.5 * 2^52` stored as a double, its low dword read back: `x`
/// rounded to the nearest integer.
#[inline]
fn rnd_magic(x: f64) -> i32 {
    let y = x + MAGIC;
    wrap32((y - MAGIC) as i64)
}

/// A stack entry of the row loop: a run of linked edges (a face's left
/// edges, or a sentinel).
#[derive(Clone, Copy, Debug)]
struct Entry {
    ty: u32,
    xl: i32,
    xr: i32,
    first: usize,
    cur: usize,
    behind: usize,
    next: usize,
    prev: usize,
}

impl Entry {
    fn new(ty: u32) -> Entry {
        Entry {
            ty,
            xl: 0,
            xr: 0,
            first: NONE,
            cur: NONE,
            behind: NONE,
            next: NONE,
            prev: NONE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Span {
    xs: i32,
    xe: i32,
    e: usize,
    ty: u32,
}

/// The rasteriser's working lists, kept between frames for their storage.
#[derive(Clone, Default)]
pub(super) struct RasterState {
    /// `FUN_004567b4`'s row lists (0x00be473c start, 0x00be373c end,
    /// 0x00be573c replacements); each row's edges in the reverse of the
    /// original's (prepended) order.
    start_rows: Vec<Vec<usize>>,
    end_rows: Vec<Vec<usize>>,
    repl_rows: Vec<Vec<usize>>,
    entries: Vec<Entry>,
    spans: Vec<Span>,
}

/// The values of one face's set-up shared by its edges.
struct FaceSetup {
    gtype: u32,
    x: [i32; 3],
    y: [i32; 3],
    iz: [f64; 3],
    cf: [u32; 3],
    u: [i32; 3],
    v: [i32; 3],
    dizdx: f64,
    dizdy: f64,
    dudx: i32,
    dvdx: i32,
    pu: [f64; 3],
    pv: [f64; 3],
    /// (0, 1), (1, 2), (0, 2)
    dpu: [f64; 3],
    dpv: [f64; 3],
    dpudx: f64,
    dpvdx: f64,
    tex: usize,
    shade: i32,
    colour: u32,
}

impl FaceSetup {
    /// x, dx, 1/z and its step of a record set up for the first time this
    /// frame.
    fn geom(&self, e: &mut Edge, p: usize, sl: f64, cf: u32) {
        if e.flags != 0 {
            return;
        }
        e.dx = truncd(sl * 65536.0);
        e.x = truncd((i64::from(self.x[p]) << 16) as f64 - f64::from(e.dx) * 0.5);
        e.flags |= cf;
        e.gtype = self.gtype;
        e.diz = (sl * self.dizdx + self.dizdy) as f32;
        e.iz = (self.iz[p] - 0.5 * f64::from(e.diz)) as f32;
    }

    /// A left edge from its top corner `p` (inverse slope; type 1
    /// divides).
    fn left(&self, e: &mut Edge, p: usize, q: usize, inv: f64) {
        let dxe = self.x[q].wrapping_sub(self.x[p]);
        let sl = if self.gtype == 1 {
            f64::from(dxe) / f64::from(self.y[q].wrapping_sub(self.y[p]))
        } else {
            f64::from(dxe) * inv
        };
        self.geom(e, p, sl, self.cf[p] | (self.cf[q] << 2));
        e.gtype = self.gtype;
        if self.gtype == 1 {
            // @ 0x00455ef4
            e.colour = self.colour as u16;
            e.flags |= 2;
            e.dizdx = self.dizdx as f32;
            return;
        }
        e.tex = self.tex;
        e.shade = self.shade;
        if self.gtype == 9 {
            // @ 0x00458a07; the stored corner differences, sign-symmetric
            // under rounding
            let sg = if p < q { 1.0 } else { -1.0 };
            let (lo, hi) = (p.min(q), p.max(q));
            let k = match (lo, hi) {
                (0, 1) => 0,
                (1, 2) => 1,
                _ => 2,
            };
            e.a58 = (16.0 * self.dizdx) as f32;
            e.dpu = (sg * self.dpu[k] * inv) as f32;
            e.dpv = (sg * self.dpv[k] * inv) as f32;
            e.pu = (self.pu[p] - f64::from(e.dpu) * 0.5) as f32;
            e.pv = (self.pv[p] - f64::from(e.dpv) * 0.5) as f32;
            e.dpudx = self.dpudx as f32;
            e.dpvdx = self.dpvdx as f32;
            e.a5c = (16.0 * self.dpudx) as f32;
            e.a60 = (16.0 * self.dpvdx) as f32;
            e.flags |= 2;
            e.dizdx = self.dizdx as f32;
            return;
        }
        e.du = truncd(f32r(f64::from(self.u[q].wrapping_sub(self.u[p]))) * inv);
        e.dv = truncd(f32r(f64::from(self.v[q].wrapping_sub(self.v[p]))) * inv);
        e.u = truncd(f64::from(self.u[p]) - f64::from(e.du) * 0.5);
        e.v = truncd(f64::from(self.v[p]) - f64::from(e.dv) * 0.5);
        e.flags |= 2;
        e.dizdx = self.dizdx as f32;
        e.dudx = self.dudx;
        e.dvdx = self.dvdx;
    }

    /// A right edge from its top corner `p` (direct division).
    fn right(&self, e: &mut Edge, p: usize, q: usize, bits: u32) {
        let sl = f64::from(self.x[q].wrapping_sub(self.x[p]))
            / f64::from(self.y[q].wrapping_sub(self.y[p]));
        self.geom(e, p, sl, self.cf[p] | (self.cf[q] << 2));
        e.flags |= bits;
    }
}

impl DialogHead {
    /// `FUN_00456d88` (type 2), `FUN_004582f8` (type 9), `FUN_00455cc0`
    /// (type 1): the edge records of the group's visible faces.
    pub(super) fn setup_group(&mut self, ni: usize, group: usize) {
        let n = &self.nodes[ni];
        let gtype = n.d.groups[group].gtype;
        let material = &n.d.groups[group].material;
        let tex = self.texture_index.get(material).copied().unwrap_or(NONE);
        // FUN_0044c1a0(3dc, 15) from FUN_00418b68
        let shade = 15;
        // FUN_00455cc0 @ 0x00455ccc / 0x00455cda: the group's +8 word (the
        // material record's +0x20), plus 0x1388 per face, culled faces
        // included
        let mut colour = self.colours.get(material).copied().unwrap_or(0) & 0xffff;
        for fi in 0..n.d.faces.len() {
            let fc = &n.d.faces[fi];
            if fc.group != group {
                continue;
            }
            colour = (colour + 0x1388) & 0xffff;
            if fc.flags & 1 != 0 {
                continue;
            }
            if fc.flags & 2 != 0 {
                continue; // +0x34 of the three records cleared (not used by the heads)
            }
            let [r01, r12, r20] = n.face_edges[fi];
            let mut x = [0; 3];
            let mut y = [0; 3];
            let mut iz = [0.0; 3];
            let mut cf = [0; 3];
            for k in 0..3 {
                let pv = n.proj[fc.vi[k]];
                x[k] = pv.x;
                y[k] = pv.y;
                iz[k] = f64::from(pv.iz);
                cf[k] = pv.flags & 0xc;
            }
            let dx01 = x[1].wrapping_sub(x[0]);
            let dx12 = x[2].wrapping_sub(x[1]);
            let dy01 = y[1].wrapping_sub(y[0]);
            let dy12 = y[2].wrapping_sub(y[1]);
            let dy02 = y[2].wrapping_sub(y[0]);
            let area = f32r(f64::from(
                dx01.wrapping_mul(dy12)
                    .wrapping_sub(dx12.wrapping_mul(dy01)),
            ));
            if area >= 0.0 || area.is_nan() {
                continue;
            }
            let inva = if gtype == 1 {
                1.0 / area
            } else {
                f32r(1.0 / area)
            };
            let (fdy01, fdy12) = (f64::from(dy01), f64::from(dy12));
            let a = iz[2] - iz[1];
            let b = iz[1] - iz[0];
            let dizdx = f32r((b * fdy12 - a * fdy01) * inva);
            let dizdy = f32r(-((b * f64::from(dx12) - a * f64::from(dx01)) * inva));
            let mut u = [0; 3];
            let mut v = [0; 3];
            if gtype != 1 {
                for k in 0..3 {
                    u[k] = fc.uv[k][0];
                    v[k] = fc.uv[k][1];
                }
            }
            let du01 = f32r(f64::from(u[1].wrapping_sub(u[0])));
            let du12 = f32r(f64::from(u[2].wrapping_sub(u[1])));
            let dv01 = f32r(f64::from(v[1].wrapping_sub(v[0])));
            let dv12 = f32r(f64::from(v[2].wrapping_sub(v[1])));
            let dudx = truncd(f32r((du01 * fdy12 - du12 * fdy01) * inva));
            let dvdx = truncd(f32r((dv01 * fdy12 - dv12 * fdy01) * inva));
            // FUN_004582f8 @ 0x00458526..0x004587a9: u * iz, v * iz per
            // corner, as float
            let mut pu = [0.0; 3];
            let mut pv = [0.0; 3];
            let mut dpu = [0.0; 3];
            let mut dpv = [0.0; 3];
            let (mut dpudx, mut dpvdx) = (0.0, 0.0);
            if gtype == 9 {
                for k in 0..3 {
                    pu[k] = f32r(f64::from(u[k]) * iz[k]);
                    pv[k] = f32r(f64::from(v[k]) * iz[k]);
                }
                dpu = [
                    f32r(pu[1] - pu[0]),
                    f32r(pu[2] - pu[1]),
                    f32r(pu[2] - pu[0]),
                ];
                dpv = [
                    f32r(pv[1] - pv[0]),
                    f32r(pv[2] - pv[1]),
                    f32r(pv[2] - pv[0]),
                ];
                dpudx = f32r((dpu[0] * fdy12 - dpu[1] * fdy01) * inva);
                // V1 - V0 kept unrounded (FST)
                dpvdx = f32r((fdy12 * (pv[1] - pv[0]) - fdy01 * dpv[1]) * inva);
            }
            let s = FaceSetup {
                gtype,
                x,
                y,
                iz,
                cf,
                u,
                v,
                dizdx,
                dizdy,
                dudx,
                dvdx,
                pu,
                pv,
                dpu,
                dpv,
                dpudx,
                dpvdx,
                tex,
                shade,
                colour,
            };
            let inv = |d: i32| f32r(1.0 / f64::from(d));
            let edges = &mut self.edges;
            if y[0] < y[1] {
                if y[0] > y[2] {
                    // V2 top, V0 mid, V1 bottom (short edges left)
                    s.left(&mut edges[r01], 0, 1, inv(dy01));
                    edges[r01].next = r12;
                    s.right(&mut edges[r12], 2, 1, 1);
                    s.left(&mut edges[r20], 2, 0, inv(dy02.wrapping_neg()));
                    edges[r20].next = r12;
                } else if y[1] < y[2] {
                    // V0 top, V1 mid, V2 bottom (short edges left)
                    s.left(&mut edges[r01], 0, 1, inv(dy01));
                    edges[r01].next = r20;
                    s.left(&mut edges[r12], 1, 2, inv(dy12));
                    edges[r12].next = r20;
                    s.right(&mut edges[r20], 0, 2, 1);
                } else {
                    // V0 top, V2 mid, V1 bottom (short edges right)
                    s.left(&mut edges[r01], 0, 1, inv(dy01));
                    edges[r01].next = r20; // written even when r20 is horizontal
                    if dy12 != 0 {
                        s.right(&mut edges[r12], 2, 1, 0x41);
                        edges[r12].pred = r01;
                    }
                    if dy02 != 0 {
                        s.right(&mut edges[r20], 0, 2, 1);
                    }
                }
            } else if y[1] > y[2] {
                // V2 top, V1 mid, V0 bottom (short edges right)
                if dy01 != 0 {
                    s.right(&mut edges[r01], 1, 0, 0x41);
                    edges[r01].pred = r20;
                }
                s.right(&mut edges[r12], 2, 1, 1);
                s.left(&mut edges[r20], 2, 0, inv(dy02.wrapping_neg()));
                edges[r20].next = r12;
            } else if y[0] < y[2] {
                // V1 top, V0 mid, V2 bottom (short edges right)
                if dy01 != 0 {
                    s.right(&mut edges[r01], 1, 0, 1);
                }
                s.left(&mut edges[r12], 1, 2, inv(dy12));
                edges[r12].next = r01; // written even when r01 is horizontal
                s.right(&mut edges[r20], 0, 2, 0x41);
                edges[r20].pred = r12;
            } else {
                // V1 top, V2 mid, V0 bottom (short edges left)
                if dy01 != 0 {
                    s.right(&mut edges[r01], 1, 0, 1);
                }
                if dy12 != 0 {
                    s.left(&mut edges[r12], 1, 2, inv(dy12));
                    edges[r12].next = r01;
                }
                if dy02 != 0 {
                    s.left(&mut edges[r20], 2, 0, inv(dy02.wrapping_neg()));
                    edges[r20].next = r01;
                }
            }
        }
    }

    /// `FUN_004567b4`: the row lists, walking the records in block order
    /// (dispatch at 0x004567ca). Kind bits 4 / 8: top corner above / below
    /// the screen; 0x10 / 0x20: bottom corner above / below.
    pub(super) fn build_lists(&mut self) {
        let st = &mut self.raster;
        for rows in [&mut st.start_rows, &mut st.end_rows, &mut st.repl_rows] {
            rows.resize_with(self.out_h as usize, Vec::new);
            rows.iter_mut().for_each(Vec::clear);
        }
        let mut warned = false;
        let real = self.edges.len() - SENTINELS;
        for i in 0..real {
            let k = self.edges[i].flags;
            if k == 0 {
                continue;
            }
            let e = self.edges[i];
            let pv = &self.nodes[e.node].proj;
            let (ya, yb) = (pv[e.va].y, pv[e.vb].y);
            let (ymin, ymax) = (ya.min(yb), ya.max(yb));
            // rows off the screen are never reached
            let push = |rows: &mut Vec<Vec<usize>>, y: i32| {
                if let Some(r) = usize::try_from(y).ok().and_then(|y| rows.get_mut(y)) {
                    r.push(i);
                }
            };
            let e = &mut self.edges[i];
            match k {
                // 0x00456b8a
                1 | 9 | 0x21 => e.next = NONE,
                // 0x00456951
                2 => {
                    e.pred = NONE;
                    push(&mut st.start_rows, ymin);
                    push(&mut st.end_rows, ymax);
                }
                // 0x00456a72: runs off the bottom
                0xa | 0x22 => {
                    e.pred = NONE;
                    push(&mut st.start_rows, ymin);
                }
                // 0x00456b67
                0x41 | 0x49 | 0x61 => {
                    push(&mut st.repl_rows, ymin);
                    e.next = NONE;
                }
                // 0x00456d48
                0x43 | 0x4b | 0x63 => push(&mut st.repl_rows, ymin),
                5 | 6 | 7 | 0x11 | 0x12 | 0x13 | 0x19 | 0x1a | 0x1b | 0x25 | 0x26 | 0x27 | 0x45
                | 0x47 | 0x51 | 0x53 | 0x59 | 0x5b | 0x65 | 0x67 => {
                    // Top clipping (0x004569bd, 0x00456ac9, 0x00456b96,
                    // 0x00456c0b, 0x00456c80, 0x00456cee) is not ported: no
                    // shipped head and camera reaches it.
                    if !warned {
                        log::warn!(
                            "dialog head: top-clipped edge kind {k:#x} not ported; edge dropped"
                        );
                    }
                    warned = true;
                }
                // 0x00456d6b (3 and the rest)
                _ => {}
            }
            e.flags = 0;
        }
    }

    fn new_entry(&mut self, ty: u32) -> usize {
        self.raster.entries.push(Entry::new(ty));
        self.raster.entries.len() - 1
    }

    /// The depth of edge `ei` at `x`, as the x87 holds it before a store.
    fn depth_at_x87(&self, ei: usize, x: i32) -> f64 {
        let e = &self.edges[ei];
        ((i64::from(x) - i64::from(e.x)) >> 16) as f64 * f64::from(e.dizdx) + f64::from(e.iz)
    }

    /// The depth of edge `ei` at `x`, stored as a float.
    fn depth_at(&self, ei: usize, x: i32) -> f64 {
        f32r(self.depth_at_x87(ei, x))
    }

    /// `FUN_0045b824`: inserts entry `nw` in the front-to-back stack whose
    /// top is `top`, at `x`; the new top.
    fn depth_insert(&mut self, top: usize, nw: usize, x: i32) -> usize {
        let ztop = self.depth_at(self.raster.entries[top].cur, x);
        let nf = &self.edges[self.raster.entries[nw].first];
        let znew = f64::from(nf.iz);
        let dznew = f64::from(nf.dizdx);
        // 0x0045b86a: znew + 100 stays on the x87 stack (not rounded to
        // float); 0x0045b8b7: the inner loop's limit is stored as a float.
        let lim_top = znew + EPS;
        let lim = f32r(znew + EPS);
        let ent = |s: &Self, i: usize| s.raster.entries[i];
        let edge_top = &self.edges[ent(self, top).cur];
        if ztop < znew || (ztop < lim_top && f64::from(edge_top.dizdx) < dznew) {
            let mut e = top;
            while ent(self, e).xr <= ent(self, nw).xr {
                e = ent(self, e).behind;
            }
            self.raster.entries[nw].behind = e;
            return nw;
        }
        if ent(self, top).behind == NONE {
            return top;
        }
        let mut e6 = top;
        loop {
            let mut e7 = ent(self, e6).behind;
            if ent(self, e6).xr < ent(self, nw).xr {
                if x < ent(self, e7).xr {
                    let mut e3 = ent(self, e7).cur;
                    while x > self.edges[self.edges[e3].next].x {
                        let nx = self.edges[e3].next;
                        self.raster.entries[e7].cur = nx;
                        e3 = nx;
                    }
                    // 0x0045b8f7: FST keeps the unrounded value for the
                    // compare with znew; the limit test reloads the float
                    // store (0x0045b904).
                    let z7x = self.depth_at_x87(e3, x);
                    let z7 = f32r(z7x);
                    if z7x < znew || (z7 < lim && f64::from(self.edges[e3].dizdx) < dznew) {
                        self.raster.entries[e6].behind = nw;
                        self.raster.entries[nw].behind = e7;
                        return top;
                    }
                } else {
                    self.raster.entries[e6].behind = ent(self, e7).behind;
                    e7 = e6;
                }
            }
            e6 = e7;
            if ent(self, e7).behind == NONE {
                break;
            }
        }
        top
    }

    /// `FUN_0045baa4`: the visible spans of a row, left to right.
    fn build_spans(&mut self, head: usize, tail: usize) {
        self.raster.spans.clear();
        let ent = |s: &Self, i: usize| s.raster.entries[i];
        let mut esi = ent(self, head).next;
        let mut edi = ent(self, head).xr >> 16;
        let mut ecx = ent(self, head).cur;
        let mut top = head;
        while ent(self, esi).xl < ent(self, head).xr {
            top = self.depth_insert(top, esi, ent(self, esi).xl);
            esi = ent(self, esi).next;
        }
        while top != tail {
            let nx = self.edges[self.edges[ecx].next].x;
            if ((i64::from(nx) - i64::from(ent(self, esi).xl)) >> 16) <= 0 {
                self.raster.entries[top].cur = ecx;
                self.raster.spans.push(Span {
                    xs: edi,
                    xe: nx >> 16,
                    e: ecx,
                    ty: ent(self, top).ty,
                });
                ecx = self.edges[ecx].next;
                edi = self.edges[ecx].x >> 16;
                if self.edges[ecx].next == NONE {
                    let old = top;
                    let ebx = i64::from(edi) << 16;
                    let mut e = ent(self, top).behind;
                    while ebx > i64::from(ent(self, e).xr) {
                        e = ent(self, e).behind;
                    }
                    self.raster.entries[old].behind = ent(self, e).behind;
                    top = e;
                    let mut c = ent(self, top).cur;
                    while ebx > i64::from(self.edges[self.edges[c].next].x) {
                        c = self.edges[c].next;
                    }
                    self.raster.entries[top].cur = c;
                    ecx = c;
                }
            } else {
                self.raster.entries[top].cur = ecx;
                let xl = ent(self, esi).xl;
                let newtop = self.depth_insert(top, esi, xl);
                if newtop != top {
                    self.raster.spans.push(Span {
                        xs: edi,
                        xe: xl >> 16,
                        e: ecx,
                        ty: ent(self, top).ty,
                    });
                    edi = xl >> 16;
                }
                top = newtop;
                esi = ent(self, esi).next;
                ecx = ent(self, top).cur;
            }
        }
    }

    /// The span fills of the table 0x004bad3c, called by `FUN_00472ad0`
    /// (ebx = xs, ecx = n).
    fn fill_span(
        &self,
        dst: &mut [u16],
        pitch: usize,
        row: usize,
        sp: &Span,
        mask: Option<&mut [u8]>,
    ) {
        let (xs, xe) = (sp.xs, sp.xe);
        let e = &self.edges[sp.e];
        let n = xe.wrapping_sub(xs);
        let Some(start) = usize::try_from(xs)
            .ok()
            .map(|x| row * pitch + x)
            .filter(|&s| s + n as usize <= dst.len())
        else {
            return;
        };
        if let Some(m) = mask.and_then(|m| m.get_mut(start..start + n as usize)) {
            m.fill(1);
        }
        let out = &mut dst[start..start + n as usize];
        if sp.ty == 1 {
            // 0x00474380: flat colour
            out.fill(e.colour);
            return;
        }
        let Some(t) = self.textures.get(e.tex) else {
            return;
        };
        let shade = (e.shade as u32) << 8;
        let texel = |idx: u32| {
            (t.table[(shade | u32::from(t.image[idx as usize])) as usize & 0x1fff] >> 16) as u16
        };
        if sp.ty == 2 {
            // 0x004743c0: affine
            let k = xs.wrapping_sub(e.x >> 16);
            let (mut u, mut v) = (e.u, e.v);
            if k > 0 {
                u = wrap32(i64::from(e.u) + i64::from(k) * i64::from(e.dudx));
                v = wrap32(i64::from(e.v) + i64::from(k) * i64::from(e.dvdx));
            }
            for o in out.iter_mut() {
                let idx = (i64::from(v >> 16) * 256 + i64::from(u >> 16)) as u32;
                // the original reads past the image for an index >= 0x10000;
                // no shipped line reaches it
                *o = if idx < 0x10000 {
                    texel(idx)
                } else {
                    (t.table[shade as usize & 0x1fff] >> 16) as u16
                };
                u = u.wrapping_add(e.dudx);
                v = v.wrapping_add(e.dvdx);
            }
            return;
        }
        // 0x00475680: perspective, one division every 16 pixels (K = 1.0 at
        // 0x004baee4); the last r pixels end on a point (r - 1) / 16 of a
        // block further with a step of 1 / r of the way (tables 0x004baf04,
        // 0x004baf4c)
        let mut i = 0usize;
        let mut run = |out: &mut [u16], pu: i32, pv: i32, su: i32, sv: i32, cnt: i32| {
            let (mut cu, mut cv) = (pu >> 8, pv >> 8);
            for _ in 0..cnt {
                let idx = (((cv >> 8) as u32 & 0xff) << 8) | ((cu >> 8) as u32 & 0xff);
                out[i] = texel(idx);
                i += 1;
                cu = cu.wrapping_add(su);
                cv = cv.wrapping_add(sv);
            }
        };
        const T1: [f64; 17] = [
            0.0, 0.0, 0.0625, 0.125, 0.1875, 0.25, 0.3125, 0.375, 0.4375, 0.5, 0.5625, 0.625,
            0.6875, 0.75, 0.8125, 0.875, 0.9375,
        ];
        let t2 = |r: i32| f32r(1.0 / f64::from(r));
        let f = f64::from(wrap32(((i64::from(xs) + 1) << 16) - i64::from(e.x))) * (1.0 / 65536.0); // float 0x004baef0 = 2^-16
        let (dizdx, dpudx, dpvdx) = (f64::from(e.dizdx), f64::from(e.dpudx), f64::from(e.dpvdx));
        let (a58, a5c, a60) = (f64::from(e.a58), f64::from(e.a5c), f64::from(e.a60));
        let mut iz = dizdx * f + f64::from(e.iz);
        let mut uu = dpudx * f + f64::from(e.pu);
        let mut vv = dpvdx * f + f64::from(e.pv);
        if n == 1 {
            // @ 0x00475ece
            let w = 1.0 / iz;
            run(out, rnd_magic(uu * w), rnd_magic(vv * w), 0, 0, 1);
            return;
        }
        if n == 2 {
            // @ 0x00475e21
            let mut w = 1.0 / iz;
            run(out, rnd_magic(uu * w), rnd_magic(vv * w), 0, 0, 1);
            vv += dpvdx;
            iz += dizdx;
            uu += dpudx;
            w = 1.0 / iz;
            run(out, rnd_magic(uu * w), rnd_magic(vv * w), 0, 0, 1);
            return;
        }
        if n <= 16 {
            // @ 0x00475d56: the start point kept as float
            let mut w = 1.0 / iz;
            let (a, b) = (uu * w, vv * w);
            let (pu, pv) = (rnd_magic(a), rnd_magic(b));
            let tn = T1[n as usize];
            iz += a58 * tn;
            uu += a5c * tn;
            vv += a60 * tn;
            w = 1.0 / iz;
            run(
                out,
                pu,
                pv,
                rnd_magic((uu * w - f32r(a)) * t2(n)) >> 8,
                rnd_magic((vv * w - f32r(b)) * t2(n)) >> 8,
                n,
            );
            return;
        }
        let mut w = 1.0 / iz;
        let mut p0 = rnd_magic(uu * w);
        let mut p1 = rnd_magic(vv * w);
        let mut rem = n - 16;
        loop {
            // @ 0x0047577c
            iz += a58;
            uu += a5c;
            vv += a60;
            w = 1.0 / iz;
            let (q0, q1) = (rnd_magic(uu * w), rnd_magic(vv * w));
            run(
                out,
                p0,
                p1,
                wrap32(i64::from(q0) - i64::from(p0)) >> 12,
                wrap32(i64::from(q1) - i64::from(p1)) >> 12,
                16,
            );
            p0 = q0;
            p1 = q1;
            if rem - 16 > 0 {
                rem -= 16;
                continue;
            }
            // @ 0x004759e1
            let r = rem;
            let tr = T1[r as usize];
            iz += a58 * tr;
            uu += a5c * tr;
            vv += a60 * tr;
            w = 1.0 / iz;
            run(
                out,
                p0,
                p1,
                rnd_magic((uu * w - f64::from(p0)) * t2(r)) >> 8,
                rnd_magic((vv * w - f64::from(p1)) * t2(r)) >> 8,
                r,
            );
            return;
        }
    }

    /// `FUN_0045c104`: the row loop.
    pub(super) fn rasterise(&mut self, dst: &mut [u16], pitch: usize, mut mask: Option<&mut [u8]>) {
        let (w, l) = (self.out_w, 0i32);
        self.raster.entries.clear();
        let s0 = self.edges.len() - SENTINELS;
        for s in &mut self.edges[s0..] {
            *s = Edge::default();
        }
        let sentinel =
            |h: &mut Self, k: usize, ty: u32, x0: i32, x1: i32, iz: f32, xl: i32, xr: i32| {
                let en = h.new_entry(ty);
                let (a, b) = (s0 + 2 * k, s0 + 2 * k + 1);
                h.edges[a].x = x0;
                h.edges[a].iz = iz;
                h.edges[b].x = x1;
                h.edges[b].iz = iz;
                h.edges[a].next = b;
                let e = &mut h.raster.entries[en];
                e.first = a;
                e.cur = a;
                e.xl = xl;
                e.xr = xr;
                en
            };
        let big = 2_147_483_648.0f32;
        let head = sentinel(
            self,
            0,
            4,
            -0x7fff_ffff,
            l << 16,
            big,
            -0x7fff_ffff,
            l << 16,
        );
        let tail = sentinel(
            self,
            1,
            4,
            (l + w) << 16,
            0x7fff_fffe,
            big,
            (l + w) << 16,
            0x7fff_fffe,
        );
        let bottom = sentinel(
            self,
            2,
            0,
            -0x7fff_ffff,
            0x7fff_ffff,
            0.0,
            -0x7fff_ffff,
            0x7fff_ffff,
        );
        let endl = self.new_entry(0x10);
        {
            let en = &mut self.raster.entries;
            en[endl].xl = 0x7fff_fffe;
            en[endl].xr = 0x7fff_ffff;
            en[bottom].next = head;
            en[head].prev = bottom;
            en[head].next = tail;
            en[tail].prev = head;
            en[tail].next = endl;
            en[endl].prev = tail;
        }
        let rows = std::mem::take(&mut self.raster.start_rows);
        let ends = std::mem::take(&mut self.raster.end_rows);
        let repls = std::mem::take(&mut self.raster.repl_rows);
        for row in 0..self.out_h as usize {
            self.raster.entries[tail].behind = NONE;
            self.raster.entries[head].behind = bottom;
            for &ei in ends[row].iter().rev() {
                let en = self.edges[ei].entry;
                if en == NONE {
                    continue;
                }
                let (p, nx) = (self.raster.entries[en].prev, self.raster.entries[en].next);
                self.raster.entries[p].next = nx;
                self.raster.entries[nx].prev = p;
            }
            for &ei in rows[row].iter().rev() {
                let gt = self.edges[ei].gtype;
                let en = self.new_entry(gt);
                let x = self.edges[ei].x;
                self.edges[ei].entry = en;
                let ents = &mut self.raster.entries;
                ents[en].xl = x;
                ents[en].first = ei;
                let tn = ents[tail].next;
                ents[en].next = tn;
                ents[tn].prev = en;
                ents[tail].next = en;
                ents[en].prev = tail;
            }
            for &ei in repls[row].iter().rev() {
                let p = self.edges[ei].pred;
                self.edges[p].next = ei;
            }
            let mut en = self.raster.entries[head].next;
            while en != endl {
                let ty = self.raster.entries[en].ty;
                let first = self.raster.entries[en].first;
                let mut e = first;
                loop {
                    let ed = &mut self.edges[e];
                    ed.x = ed.x.wrapping_add(ed.dx);
                    ed.iz = (f64::from(ed.iz) + f64::from(ed.diz)) as f32;
                    if ty == 2 {
                        ed.u = ed.u.wrapping_add(ed.du);
                        ed.v = ed.v.wrapping_add(ed.dv);
                    } else if ty == 9 {
                        ed.pu = (f64::from(ed.pu) + f64::from(ed.dpu)) as f32;
                        ed.pv = (f64::from(ed.pv) + f64::from(ed.dpv)) as f32;
                    }
                    if ed.next == NONE {
                        break;
                    }
                    e = ed.next;
                }
                let (xl, xr) = (self.edges[first].x, self.edges[e].x);
                let ent = &mut self.raster.entries[en];
                ent.xl = xl;
                ent.xr = xr;
                ent.cur = first;
                en = ent.next;
            }
            // insertion sort by xl
            let ents = &mut self.raster.entries;
            let mut en = ents[head].next;
            while en != endl {
                let nxt = ents[en].next;
                if ents[en].xl < ents[ents[en].prev].xl {
                    let mut p = ents[en].prev;
                    ents[p].next = nxt;
                    ents[nxt].prev = p;
                    p = ents[p].prev;
                    while ents[en].xl < ents[p].xl {
                        p = ents[p].prev;
                    }
                    let q = ents[p].next;
                    ents[en].next = q;
                    ents[q].prev = en;
                    ents[p].next = en;
                    ents[en].prev = p;
                }
                en = nxt;
            }
            self.build_spans(head, tail);
            for sp in self.raster.spans.iter().rev() {
                if sp.xe - sp.xs > 0 && matches!(sp.ty, 1 | 2 | 9) {
                    self.fill_span(dst, pitch, row, sp, mask.as_deref_mut());
                }
            }
        }
        self.raster.start_rows = rows;
        self.raster.end_rows = ends;
        self.raster.repl_rows = repls;
        for e in &mut self.edges {
            e.entry = NONE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(x: i32, iz: f32, dizdx: f32, next: usize) -> Edge {
        Edge {
            x,
            iz,
            dizdx,
            next,
            ..Edge::default()
        }
    }

    fn entry(xr: i32, first: usize, behind: usize) -> Entry {
        Entry {
            xr,
            first,
            cur: first,
            behind,
            ..Entry::new(0)
        }
    }

    /// `FUN_0045b824` compares the unrounded x87 depth (FST 0x0045b8f7,
    /// FCOMP 0x0045b8fb) with the new entry's 1/z: a depth just under 1/z
    /// that rounds to it as a float puts the new entry in front of the
    /// stack entry. Expected links from the original run under emulation.
    #[test]
    fn depth_insert_inner_near_tie() {
        let mut h = DialogHead::default();
        let x = 10 << 16;
        // edges: 0 the end, 1 the top's, 2 the inner entry's, 3 the new one's
        h.edges = vec![
            edge(0x7fff_ffff, 0.0, 0.0, NONE),
            edge(x, 1000.0, 0.0, 0),
            edge(
                x - (1 << 16),
                f32::from_bits(0x3f7f_ffff),
                (3.0 / 67_108_864.0) as f32,
                0,
            ),
            edge(x, 1.0, 0.0, 0),
        ];
        // entries: 0 the inner one (e7), 1 the top, 2 the new one
        h.raster.entries = vec![
            entry(100 << 16, 2, NONE),
            entry(20 << 16, 1, 0),
            entry(50 << 16, 3, NONE),
        ];
        let ret = h.depth_insert(1, 2, x);
        assert_eq!(ret, 1, "the top is kept");
        assert_eq!(
            h.raster.entries[1].behind, 2,
            "the new entry behind the top"
        );
        assert_eq!(
            h.raster.entries[2].behind, 0,
            "the inner entry behind the new one"
        );
    }

    /// `FUN_0045b824` compares the top's depth with the unrounded
    /// znew + 100 (0x0045b86a..0x0045b874): 2^26 + 96 against 2^26 + 100,
    /// which a float store would round to 2^26 + 96. Expected links from
    /// the original run under emulation.
    #[test]
    fn depth_insert_top_near_tie() {
        let mut h = DialogHead::default();
        let x = 10 << 16;
        h.edges = vec![
            edge(0x7fff_ffff, 0.0, 0.0, NONE),
            edge(x, 67_108_960.0, 0.0, 0),
            edge(x, 67_108_864.0, 1.0, 0),
        ];
        h.raster.entries = vec![entry(60 << 16, 1, NONE), entry(50 << 16, 2, NONE)];
        let ret = h.depth_insert(0, 1, x);
        assert_eq!(ret, 1, "the new entry is the top");
        assert_eq!(h.raster.entries[1].behind, 0, "the old top behind it");
    }
}
