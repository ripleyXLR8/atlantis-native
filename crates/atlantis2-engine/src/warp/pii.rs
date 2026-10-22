//! `warpPII`: the polygon set-up @ 0x0057b3c0, the scan conversion
//! @ 0x0057d7c0 and the span @ 0x0057dd00 (which `warpMMX` shares), hand
//! written x87 and MMX code, transcribed instruction by instruction.
//!
//! Everything runs under the control word 0x0e7f the set-up loads
//! ([`Fpu::Chop`]: 53-bit precision, rounding toward zero, `fistp`
//! truncating). Values the code keeps on the x87 stack stay unrounded to
//! float; the ones it stores with `fst dword` are rounded toward zero.
//!
//! Against `warpC`: the triangle's vertices are ordered by the integer
//! part of y (each order of the three rows has its own copy of the code,
//! all with the same edge set-up), edge and span start values keep the stack's precision, the
//! texture coordinates are 1.31 fixed point (the face's 512 texels times
//! 2^22, `FUN_004d8d40` for this class), and the pixels are stepped in
//! 16-bit words (MMX `paddw`), so the texel index wraps within the face
//! instead of leaving it. The last pixels of a span are interpolated
//! towards the last pixel (a table of t − 1 and 1 / (t − 1)), not one past.

use super::{Renderer, Vertex, FACE_SIZE};
use crate::fpu::Fpu;

/// The control word 0x0e7f (`fldcw` @ 0x0057b3c1 of the word @ 0x005edc78).
const X87: Fpu = Fpu::Chop;

/// The texture scale (@ 0x006e1448 / 0x006e144c, set by FUN_004d8d40 for a
/// 512 x 512 face in this class): (511 + 1) · 2^(31 − 9) = 2^31, so a texel
/// coordinate u ∈ [0, 1) becomes u · 2^31.
const TEX_SCALE: f64 = 2_147_483_648.0;

/// The texture's row length in texels (the face surface's pitch, the high
/// word of the `pmaddwd` factor @ 0x006e14fc).
const PITCH: usize = FACE_SIZE;

/// The right shift from the 16-bit coordinate words to texels (@ 0x006e14d4:
/// 16 − 9).
const TEXEL_SHIFT: u32 = 7;

/// The span's integer (u, v) pairs (@ 0x005ed7c0, 8 bytes each up to the
/// masks @ 0x005edbf8): the first pixel's and one per 32-pixel block.
const ENDS: usize = 135;

/// The widest screen `warpPII` draws: a span of n pixels takes
/// ceil(n / 32) − 1 blocks besides its first pixel, at most `ENDS` − 1. A
/// wider span would write over the masks after the pairs.
pub const MAX_WIDTH: usize = ENDS * 32;

/// 1 / (t − 1), the table @ 0x005ee018 (stride 8) for the last t pixels of
/// a span (0 for t = 1); the table @ 0x005ede18 holds t − 1 itself.
fn inv_tail(t: u32) -> f64 {
    if t <= 1 {
        0.0
    } else {
        f64::from(1.0f32 / (t - 1) as f32)
    }
}

/// An edge of a triangle (0x48 bytes @ 0x005edcf0, 0x005edd38, 0x005edd80):
/// first and end rows, x, w, their steps per row, u, v and their steps.
#[derive(Clone, Copy, Debug, Default)]
struct Edge {
    ytop: i32,
    ybot: i32,
    x: f64,
    w: f64,
    dx: f64,
    dw: f64,
    u: f64,
    v: f64,
    du: f64,
    dv: f64,
}

/// The three edge slots.
const A: usize = 0; // @ 0x005edcf0
const B: usize = 1; // @ 0x005edd38
const C: usize = 2; // @ 0x005edd80

/// What the set-up hands the scan conversion (@ 0x0057d734): the left and
/// right edges (esi, edi), the row the first part ends at (ebp), and the
/// edges of the second part (@ 0x005eddc8, 0x005eddcc), if any.
struct Plan {
    left: usize,
    right: usize,
    yend: i32,
    second: Option<(usize, usize)>,
}

/// The gradients across a triangle (@ 0x006e14ac, 0x006e142c, 0x006e1508,
/// 0x006e1458, 0x006e14a8, 0x006e1428): d/dx and 32 · d/dx of v, w, u.
struct Gradients {
    dvdx: f64,
    dvdx32: f64,
    dwdx: f64,
    dwdx32: f64,
    dudx: f64,
    dudx32: f64,
}

/// Vertex k of a polygon made ready (@ 0x0057b3c7..0x0057b484 for the first
/// two, @ 0x0057b460 for each next one): x + 0.5 and y + 0.5 stored, the
/// integer row of y + 0.5 (`fistp` of the unrounded sum: truncation), u and
/// v scaled and multiplied by w.
fn prepare(v: &mut Vertex) -> i32 {
    let x = X87;
    let y = x.add(0.5, v[1]);
    v[0] = x.f(x.add(0.5, v[0]));
    v[1] = x.f(y);
    v[3] = x.f(x.mul(x.mul(TEX_SCALE, v[3]), v[2]));
    v[4] = x.f(x.mul(x.mul(TEX_SCALE, v[4]), v[2]));
    x.fistp(y)
}

/// The edge from `p` (top) to `q` from row `ytop` (clamped) to `ybot`
/// (@ 0x0057b4ff..0x0057b577, the same code in every case): the slope and
/// start values at the centre of row `ytop`, with u, v, w unless `xonly`.
fn edge(p: &Vertex, q: &Vertex, ytop: i32, ybot: i32, xonly: bool) -> Edge {
    let x = X87;
    let inv = x.div(1.0, x.sub(q[1], p[1]));
    let fy = x.sub(f64::from(ytop.wrapping_add(1)), p[1]);
    let slope = x.mul(x.sub(q[0], p[0]), inv);
    let mut e = Edge {
        ytop,
        ybot,
        dx: x.f(slope),
        x: x.f(x.add(p[0], x.mul(slope, fy))),
        ..Edge::default()
    };
    if xonly {
        return e;
    }
    let dus = x.mul(x.sub(q[3], p[3]), inv);
    let dvs = x.mul(x.sub(q[4], p[4]), inv);
    e.dv = x.f(dvs);
    e.v = x.f(x.add(p[4], x.mul(dvs, fy)));
    e.du = x.f(dus);
    e.u = x.f(x.add(p[3], x.mul(dus, fy)));
    let dws = x.mul(inv, x.sub(q[2], p[2]));
    e.dw = x.f(dws);
    e.w = x.f(x.add(p[2], x.mul(fy, dws)));
    e
}

/// `fcomi` then `ja`: false when unordered.
#[inline(always)]
fn above(a: f64, b: f64) -> bool {
    a > b
}

/// `fcomi` then `jb`: true when unordered.
#[inline(always)]
fn below(a: f64, b: f64) -> bool {
    !matches!(
        a.partial_cmp(&b),
        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
    )
}

impl Renderer {
    /// 0x0057b3c0: loads the control word 0x0e7f, then draws the polygon
    /// as a fan (v₀, vₖ₋₁, vₖ), each vertex made ready once.
    pub(super) fn polygon_pii(&mut self, poly: &mut [Vertex], tex: &[u16], out: &mut [u16]) {
        self.fpu = X87;
        let iy0 = prepare(&mut poly[0]);
        let mut iy_prev = prepare(&mut poly[1]);
        for k in 2..poly.len() {
            let iy = prepare(&mut poly[k]);
            let tri = [poly[0], poly[k - 1], poly[k]];
            self.triangle_pii(&tri, [iy0, iy_prev, iy], tex, out);
            iy_prev = iy;
        }
    }

    /// The triangle (v₀ eax, vₖ₋₁ edx, vₖ ebx) with the integer rows of
    /// its vertices (ebp, ecx, edi): one case per order of the rows
    /// (@ 0x0057b49f..0x0057d20b).
    fn triangle_pii(&mut self, v: &[Vertex; 3], iy: [i32; 3], tex: &[u16], out: &mut [u16]) {
        let [a, b, c] = iy;
        let plan = if a < b {
            if b < c {
                self.strict(v, iy, 0, 1, 2) // @ 0x0057ced4
            } else if b == c {
                self.flat_bottom(v, iy, 0, 1, 2) // @ 0x0057cc5d
            } else if a < c {
                self.strict(v, iy, 0, 2, 1) // @ 0x0057c6af
            } else if a == c {
                self.flat_top(v, iy, 2, 0, 1) // @ 0x0057c9e6
            } else {
                self.strict(v, iy, 2, 0, 1) // @ 0x0057c378
            }
        } else if a == b {
            if b < c {
                self.flat_top(v, iy, 1, 0, 2) // @ 0x0057d490
            } else if b == c {
                None
            } else {
                self.flat_bottom(v, iy, 2, 1, 0) // @ 0x0057d219
            }
        } else if b < c {
            if a < c {
                self.strict(v, iy, 1, 0, 2) // @ 0x0057bb37
            } else if a == c {
                self.flat_bottom(v, iy, 1, 0, 2) // @ 0x0057be6e
            } else {
                self.strict(v, iy, 1, 2, 0) // @ 0x0057b800
            }
        } else if b == c {
            self.flat_top(v, iy, 2, 1, 0) // @ 0x0057c0e5
        } else {
            self.strict(v, iy, 2, 1, 0) // @ 0x0057b4bb
        };
        if let Some((mut e, plan)) = plan {
            self.scan_pii(&mut e, plan, tex, out);
        }
    }

    /// The clip rows (@ 0x006e13d8, 0x006e1440 from FUN_004d8b30 with the
    /// rectangle of FUN_00402b80): the first row and one past the last.
    fn rows(&self) -> (i32, i32) {
        (0, self.height as i32)
    }

    /// Three different rows, top `t`, middle `m`, bottom `bo` (indices into
    /// `v`): the long edge A top→bottom; when the middle row is below the
    /// first screen row, the upper edge B top→middle, left of A unless its
    /// slope is greater, and the lower edge C middle→bottom (x only when it
    /// is on the right) unless the middle row is the screen's end; else
    /// the lower edge alone, in slot B (@ 0x0057b4bb and its five copies).
    fn strict(
        &self,
        v: &[Vertex; 3],
        iy: [i32; 3],
        t: usize,
        m: usize,
        bo: usize,
    ) -> Option<([Edge; 3], Plan)> {
        let (ymin, ymax) = self.rows();
        let (mut yt, mut ym, mut yb) = (iy[t], iy[m], iy[bo]);
        if yt >= ymax || yb <= ymin {
            return None;
        }
        yt = yt.max(ymin);
        yb = yb.min(ymax);
        ym = ym.min(ymax);
        let mut e = [Edge::default(); 3];
        e[A] = edge(&v[t], &v[bo], yt, yb, false);
        if ym > ymin {
            e[B] = edge(&v[t], &v[m], yt, ym, false);
            let plan = if !above(e[B].dx, e[A].dx) {
                // B on the left.
                if ym == ymax {
                    // @ 0x0057d704
                    Plan {
                        left: B,
                        right: A,
                        yend: e[B].ybot,
                        second: None,
                    }
                } else {
                    e[C] = edge(&v[m], &v[bo], ym, yb, false);
                    Plan {
                        left: B,
                        right: A,
                        yend: e[B].ybot,
                        second: Some((C, A)),
                    }
                }
            } else if ym == ymax {
                // @ 0x0057d71d
                Plan {
                    left: A,
                    right: B,
                    yend: e[B].ybot,
                    second: None,
                }
            } else {
                e[C] = edge(&v[m], &v[bo], ym, yb, true);
                Plan {
                    left: A,
                    right: B,
                    yend: e[B].ybot,
                    second: Some((A, C)),
                }
            };
            Some((e, plan))
        } else {
            ym = ym.max(ymin);
            e[B] = edge(&v[m], &v[bo], ym, yb, false);
            let (left, right) = if above(e[B].dx, e[A].dx) {
                (B, A)
            } else {
                (A, B)
            };
            Some((
                e,
                Plan {
                    left,
                    right,
                    yend: e[A].ybot,
                    second: None,
                },
            ))
        }
    }

    /// A top `t` and two bottoms on the same row; `x` is the bottom whose x
    /// the code loads second (compared first): its edge is the left one
    /// when its x is smaller (or unordered), else the other's.
    fn flat_bottom(
        &self,
        v: &[Vertex; 3],
        iy: [i32; 3],
        t: usize,
        x: usize,
        y: usize,
    ) -> Option<([Edge; 3], Plan)> {
        let (ymin, ymax) = self.rows();
        let (yt, yb) = (iy[t], iy[x]);
        if yt >= ymax || yb <= ymin {
            return None;
        }
        let (yt, yb) = (yt.max(ymin), yb.min(ymax));
        let (l, r) = if below(v[x][0], v[y][0]) {
            (x, y)
        } else {
            (y, x)
        };
        let mut e = [Edge::default(); 3];
        e[B] = edge(&v[t], &v[r], yt, yb, false);
        e[A] = edge(&v[t], &v[l], yt, yb, false);
        Some((
            e,
            Plan {
                left: A,
                right: B,
                yend: e[A].ybot,
                second: None,
            },
        ))
    }

    /// Two tops on the same row and a bottom `bo`; `x` is the top whose x
    /// the code loads second: its edge is the left one when its x is
    /// smaller (or unordered).
    fn flat_top(
        &self,
        v: &[Vertex; 3],
        iy: [i32; 3],
        x: usize,
        y: usize,
        bo: usize,
    ) -> Option<([Edge; 3], Plan)> {
        let (ymin, ymax) = self.rows();
        let (yt, yb) = (iy[x], iy[bo]);
        if yt >= ymax || yb <= ymin {
            return None;
        }
        let (yt, yb) = (yt.max(ymin), yb.min(ymax));
        let (l, r) = if below(v[x][0], v[y][0]) {
            (x, y)
        } else {
            (y, x)
        };
        let mut e = [Edge::default(); 3];
        e[B] = edge(&v[r], &v[bo], yt, yb, false);
        e[A] = edge(&v[l], &v[bo], yt, yb, false);
        Some((
            e,
            Plan {
                left: A,
                right: B,
                yend: e[A].ybot,
                second: None,
            },
        ))
    }

    /// 0x0057d7c0: the gradients from the first part's two edges, then the
    /// rows of both parts. After the first part the right edge has taken
    /// one more step, and the left one too when it goes on.
    fn scan_pii(&mut self, e: &mut [Edge; 3], plan: Plan, tex: &[u16], out: &mut [u16]) {
        let x = X87;
        let (mut l, mut r) = (plan.left, plan.right);
        let inv = x.div(1.0, x.sub(e[r].dx, e[l].dx));
        let dv = x.mul(x.sub(e[r].dv, e[l].dv), inv);
        let dw = x.mul(x.sub(e[r].dw, e[l].dw), inv);
        let du = x.mul(inv, x.sub(e[r].du, e[l].du));
        let g = Gradients {
            dvdx: x.f(dv),
            dvdx32: x.f(x.mul(dv, 32.0)),
            dwdx: x.f(dw),
            dwdx32: x.f(x.mul(dw, 32.0)),
            dudx: x.f(du),
            dudx32: x.f(x.mul(du, 32.0)),
        };
        let mut y = e[l].ytop;
        let mut count = plan.yend.wrapping_sub(y);
        let mut second = plan.second;
        loop {
            // The set-up never hands an empty part (the asm's `dec`/`jne`
            // would run 2^32 rows).
            if count <= 0 {
                return;
            }
            loop {
                self.row_pii(y, &e[l], &e[r], &g, tex, out);
                y += 1;
                count -= 1;
                if count == 0 {
                    break;
                }
                step_right(&mut e[r]);
                step_left(&mut e[l]);
            }
            step_right(&mut e[r]);
            let Some((l2, r2)) = second.take() else {
                return;
            };
            if l2 == l {
                step_left(&mut e[l]);
            }
            (l, r) = (l2, r2);
            count = e[r].ybot.wrapping_sub(plan.yend);
        }
    }

    /// A row (@ 0x0057d877): from trunc(max(0, x_left)) to
    /// min(W, trunc(x_right)), exclusive.
    fn row_pii(&mut self, y: i32, l: &Edge, r: &Edge, g: &Gradients, tex: &[u16], out: &mut [u16]) {
        let x = X87;
        let xr = x.fistp(r.x);
        // fcomi of the clip's left float (@ 0x006e1418) with x_left, then
        // fcmovb: x_left when the clip is below it or unordered.
        let xmin = 0.0;
        let xl = x.fistp(if below(xmin, l.x) { l.x } else { xmin });
        let width = self.width as i32; // @ 0x006e1410
        let xend = if width >= xr { xr } else { width };
        if xl < xend {
            if xl < 0 {
                // Only a NaN x_left gets here; the original would write
                // before the screen.
                return;
            }
            self.span_pii(y, xl, xend, l, g, tex, out);
        }
    }

    /// 0x0057dd00: the perspective division at the span's first pixel and
    /// every 32nd one after it, kept as 1.31 integers; the pixels between in
    /// 16-bit words with a step truncated to 16 bits; the last pixels from
    /// the last division to a division at the span's last pixel.
    #[allow(clippy::too_many_arguments)]
    fn span_pii(
        &mut self,
        y: i32,
        xl: i32,
        xend: i32,
        l: &Edge,
        g: &Gradients,
        tex: &[u16],
        out: &mut [u16],
    ) {
        let x = X87;
        let n = (xend - xl) as u32;
        let d = x.sub(f64::from(xl + 1), l.x);
        let mut w = x.add(l.w, x.mul(g.dwdx, d));
        let mut rw = x.div(1.0, w);
        let mut vv = x.add(l.v, x.mul(g.dvdx, d));
        let mut uu = x.add(x.mul(g.dudx, d), l.u);
        // The integer (u, v) at the first pixel and at each 32nd one after
        // it (@ 0x005ed7c0), and the float of the last one (@ 0x005edc88,
        // 0x005edc80).
        let mut ends = [(0i32, 0i32); ENDS];
        let u0 = x.mul(rw, uu);
        let v0 = x.mul(rw, vv);
        ends[0] = (x.fistp(u0), x.fistp(v0));
        let (mut ulast, mut vlast) = (x.f(u0), x.f(v0));
        let (blocks, tail) = if n <= 32 {
            (0, n)
        } else if n & 31 == 0 {
            ((n >> 5) - 1, 32)
        } else {
            (n >> 5, n & 31)
        };
        if blocks > 0 {
            w = x.add(w, g.dwdx32);
            rw = x.div(1.0, w);
            vv = x.add(vv, g.dvdx32);
            uu = x.add(uu, g.dudx32);
            for (j, end) in ends[1..=blocks as usize].iter_mut().enumerate() {
                let vj = x.mul(vv, rw);
                let uj = x.mul(rw, uu);
                *end = (x.fistp(uj), x.fistp(vj));
                if j + 1 < blocks as usize {
                    w = x.add(w, g.dwdx32);
                    rw = x.div(1.0, w);
                    vv = x.add(vv, g.dvdx32);
                    uu = x.add(uu, g.dudx32);
                } else {
                    (ulast, vlast) = (x.f(uj), x.f(vj));
                }
            }
        }
        // The last `tail` pixels (@ 0x0057de50): the division at the last
        // pixel, the step (end − last) / (tail − 1) truncated.
        let t1 = f64::from(tail - 1);
        let we = x.add(x.mul(g.dwdx, t1), w);
        let rwe = x.div(1.0, we);
        let ue = x.add(uu, x.mul(g.dudx, t1));
        let ve = x.add(vv, x.mul(t1, g.dvdx));
        let vend = x.mul(ve, rwe);
        let uend = x.mul(ue, rwe);
        let t2 = inv_tail(tail);
        let dv_tail = x.fistp(x.mul(x.sub(vend, vlast), t2));
        let du_tail = x.fistp(x.mul(x.sub(uend, ulast), t2));

        let mut base = y as usize * self.width + xl as usize;
        for j in 0..blocks as usize {
            let (u0, v0) = ends[j];
            let (u1, v1) = ends[j + 1];
            run(
                tex,
                &mut out[base..base + 32],
                word(u0),
                word(v0),
                block_step(u0, u1),
                block_step(v0, v1),
            );
            base += 32;
        }
        let (u0, v0) = ends[blocks as usize];
        if tail == 1 {
            // @ 0x0057e0c0: one pixel.
            out[base] = texel(tex, word(u0), word(v0));
        } else {
            run(
                tex,
                &mut out[base..base + tail as usize],
                word(u0),
                word(v0),
                tail_step(du_tail),
                tail_step(dv_tail),
            );
        }
    }
}

/// x += dx of a right edge, stored (@ 0x0057d840, 0x0057d8d8).
fn step_right(e: &mut Edge) {
    let x = X87;
    e.x = x.f(x.add(e.dx, e.x));
}

/// v, u, w, x += their steps, stored (@ 0x0057d84b, 0x0057d8f2).
fn step_left(e: &mut Edge) {
    let x = X87;
    e.v = x.f(x.add(e.dv, e.v));
    e.u = x.f(x.add(e.du, e.u));
    e.w = x.f(x.add(e.dw, e.w));
    e.x = x.f(x.add(e.dx, e.x));
}

/// The 16-bit word of a 1.31 coordinate: the high word of 2u (`paddd`,
/// `psrld 16`, or the mask 0xffff0000 for v).
#[inline(always)]
fn word(c: i32) -> u16 {
    ((c as u32).wrapping_add(c as u32) >> 16) as u16
}

/// The per-pixel step over a 32-pixel block: (2·c₁ − 2·c₀) >> 5
/// (`psubd`, `psrad 5`), of which the high word is kept (`psllq 16` and
/// the masks @ 0x005edc00, 0x005edbf8).
#[inline(always)]
fn block_step(c0: i32, c1: i32) -> u16 {
    let d = c1.wrapping_add(c1).wrapping_sub(c0.wrapping_add(c0));
    ((d >> 5) as u32 >> 16) as u16
}

/// The per-pixel step of the last pixels: the high word of 2·d.
#[inline(always)]
fn tail_step(d: i32) -> u16 {
    (d.wrapping_add(d) as u32 >> 16) as u16
}

/// The texel at the coordinate words (u, v): `psrlw 7`, `pmaddwd` by
/// (1, pitch).
#[inline(always)]
fn texel(tex: &[u16], u: u16, v: u16) -> u16 {
    tex[usize::from(v >> TEXEL_SHIFT) * PITCH + usize::from(u >> TEXEL_SHIFT)]
}

/// Pixels from the words (u, v) by (du, dv) each, wrapping at 16 bits
/// (`paddw`).
#[inline(always)]
fn run(tex: &[u16], out: &mut [u16], u: u16, v: u16, du: u16, dv: u16) {
    let (mut u, mut v) = (u, v);
    for px in out {
        *px = texel(tex, u, v);
        u = u.wrapping_add(du);
        v = v.wrapping_add(dv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_table_holds_the_inverses() {
        // A few entries of the table @ 0x005ee018 (float bits).
        let bits = |t| (inv_tail(t) as f32).to_bits();
        assert_eq!(bits(1), 0);
        assert_eq!(bits(2), 0x3f80_0000);
        assert_eq!(bits(4), 0x3eaa_aaab);
        assert_eq!(bits(8), 0x3e12_4925);
        assert_eq!(bits(12), 0x3dba_2e8c);
        assert_eq!(bits(24), 0x3d32_1643);
        assert_eq!(bits(32), 0x3d04_2108);
    }

    #[test]
    fn fcomi_branches_follow_the_flags() {
        // ja: CF = 0 and ZF = 0, false when equal or unordered.
        assert!(above(2.0, 1.0));
        assert!(!above(1.0, 1.0));
        assert!(!above(f64::NAN, 1.0));
        // jb: CF = 1, set when below or unordered, clear when equal.
        assert!(below(1.0, 2.0));
        assert!(!below(1.0, 1.0));
        assert!(!below(2.0, 1.0));
        assert!(below(f64::NAN, 1.0));
        assert!(below(1.0, f64::NAN));
    }

    #[test]
    fn edge_sums_round_toward_zero() {
        // Every start value is s + ds · fy with ds = −2^-60 and fy = 1: the
        // fadd under 0x0e7f gives 1 − 2^-53, which the fst dword stores as
        // the float below 1 (to nearest, the sum would be 1).
        let p: Vertex = [1.0, 0.0, 1.0, 1.0, 1.0];
        let q: Vertex = [0.0, 2f64.powi(60), 0.0, 0.0, 0.0];
        let e = edge(&p, &q, 0, 1, false);
        assert_eq!(e.dx, -(2f64.powi(-60)));
        let below_one = f64::from(f32::from_bits(0x3f7f_ffff));
        assert_eq!([e.x, e.w, e.u, e.v], [below_one; 4]);
    }

    #[test]
    fn the_longest_span_fills_the_buffer() {
        // MAX_WIDTH pixels: the first pixel's pair and 134 blocks, the last
        // 32 pixels as the tail (a 64-pair buffer would overflow). u = (i + 1) / 16 texel at pixel i (the
        // left edge half a pixel before the first centre).
        use crate::warp::Variant;
        let n = MAX_WIDTH;
        let mut r = Renderer::new(Variant::PII, Fpu::Chop, n, 1);
        let du = TEX_SCALE / 512.0 / 16.0;
        let l = Edge {
            x: 0.5,
            w: 1.0,
            u: 0.5 * du,
            ..Edge::default()
        };
        let g = Gradients {
            dvdx: 0.0,
            dvdx32: 0.0,
            dwdx: 0.0,
            dwdx32: 0.0,
            dudx: du,
            dudx32: 32.0 * du,
        };
        let tex: Vec<u16> = (0..512 * 512).map(|i| (i % 512) as u16).collect();
        let mut out = vec![0xffffu16; n];
        r.span_pii(0, 0, n as i32, &l, &g, &tex, &mut out);
        for (i, &t) in out.iter().enumerate() {
            let want = (i + 1) >> 4;
            if i < n - 32 {
                assert_eq!(usize::from(t), want, "pixel {i}");
            } else {
                // The tail's step, (end − last) · float(1/31) truncated,
                // falls just short of du.
                assert!(want - usize::from(t) <= 1, "pixel {i}: {t}");
            }
        }
    }

    #[test]
    fn words_wrap_within_the_face() {
        // u = 0.5 of the face: 2^30 → word 0x8000 → texel 256.
        assert_eq!(word(1 << 30), 0x8000);
        // A coordinate just below 0 wraps to the face's last texel.
        assert_eq!(word(-1) >> TEXEL_SHIFT, 511);
        // The integer indefinite of an out-of-range fistp reads texel 0.
        assert_eq!(word(i32::MIN), 0);
        // One texel per pixel over a block: 32 texels in 1.31.
        assert_eq!(block_step(0, 32 << 22), 1 << 7);
        assert_eq!(block_step(32 << 22, 0), (-(1i32 << 7)) as u16);
        // The step keeps the high word: a fraction below 2^-16 is lost.
        assert_eq!(block_step(0, 31), 0);
        assert_eq!(block_step(0, -31), 0xffff);
    }
}
