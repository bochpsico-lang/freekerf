//! Bitmap tracing: a port of CsPotrace (`CsPotrace/CsPotrace.cs` in LaserGRBL), the
//! C# translation of potrace.js by Wolfgang Nagl, after Peter Selinger's Potrace
//! (GPL-2.0-or-later). Every step, rounding and order is kept so the traced paths
//! are the ones LaserGRBL engraves.

use super::{Curve, CurveKind, DPoint, TraceOptions};
use crate::image::{Image, split};

const POTRACE_CORNER: u8 = 1;
const POTRACE_CURVETO: u8 = 2;

/// The black and white bitmap (`Bitmap_p`): `true` is black.
#[derive(Clone)]
struct Bits {
    w: i32,
    h: i32,
    data: Vec<bool>,
}

impl Bits {
    fn at(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.w && y >= 0 && y < self.h && self.data[(self.w * y + x) as usize]
    }

    fn flip(&mut self, x: i32, y: i32) {
        let i = (self.w * y + x) as usize;
        self.data[i] = !self.at(x, y);
    }

    fn size(&self) -> i32 {
        self.w * self.h
    }
}

#[derive(Clone, Copy, Default)]
struct Sum {
    x: f64,
    y: f64,
    xy: f64,
    x2: f64,
    y2: f64,
}

#[derive(Clone)]
struct PrivCurve {
    n: usize,
    tag: Vec<u8>,
    vertex: Vec<DPoint>,
    c: Vec<DPoint>,
    alpha: Vec<f64>,
    beta: Vec<f64>,
}

impl PrivCurve {
    fn new(n: usize) -> Self {
        Self {
            n,
            tag: vec![0; n],
            vertex: vec![DPoint::default(); n],
            c: vec![DPoint::default(); n * 3],
            alpha: vec![0.0; n],
            beta: vec![0.0; n],
        }
    }
}

struct TracePath {
    area: i32,
    len: i32,
    positive: bool,
    pt: Vec<(i32, i32)>,
    max_x: i32,
    x0: f64,
    y0: f64,
    m: usize,
    po: Vec<i32>,
    lon: Vec<i32>,
    sums: Vec<Sum>,
    curve: PrivCurve,
}

fn interval(lambda: f64, a: DPoint, b: DPoint) -> DPoint {
    DPoint::new(a.x + lambda * (b.x - a.x), a.y + lambda * (b.y - a.y))
}

fn sign(i: f64) -> i32 {
    if i > 0.0 {
        1
    } else if i < 0.0 {
        -1
    } else {
        0
    }
}

fn isign(i: i32) -> i32 {
    i.signum()
}

fn dorth_infty(p0: DPoint, p2: DPoint) -> DPoint {
    DPoint::new(f64::from(-sign(p2.y - p0.y)), f64::from(sign(p2.x - p0.x)))
}

fn ddenom(p0: DPoint, p2: DPoint) -> f64 {
    let r = dorth_infty(p0, p2);
    r.y * (p2.x - p0.x) - r.x * (p2.y - p0.y)
}

fn dpara(p0: DPoint, p1: DPoint, p2: DPoint) -> f64 {
    let (x1, y1) = (p1.x - p0.x, p1.y - p0.y);
    let (x2, y2) = (p2.x - p0.x, p2.y - p0.y);
    x1 * y2 - x2 * y1
}

fn cprod(p0: DPoint, p1: DPoint, p2: DPoint, p3: DPoint) -> f64 {
    let (x1, y1) = (p1.x - p0.x, p1.y - p0.y);
    let (x2, y2) = (p3.x - p2.x, p3.y - p2.y);
    x1 * y2 - x2 * y1
}

fn iprod(p0: DPoint, p1: DPoint, p2: DPoint) -> f64 {
    let (x1, y1) = (p1.x - p0.x, p1.y - p0.y);
    let (x2, y2) = (p2.x - p0.x, p2.y - p0.y);
    x1 * x2 + y1 * y2
}

fn iprod1(p0: DPoint, p1: DPoint, p2: DPoint, p3: DPoint) -> f64 {
    let (x1, y1) = (p1.x - p0.x, p1.y - p0.y);
    let (x2, y2) = (p3.x - p2.x, p3.y - p2.y);
    x1 * x2 + y1 * y2
}

fn ddist(p: DPoint, q: DPoint) -> f64 {
    ((p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y)).sqrt()
}

fn xprodi(p1: (i32, i32), p2: (i32, i32)) -> i32 {
    p1.0 * p2.1 - p1.1 * p2.0
}

fn cyclic<T: PartialOrd + Copy>(a: T, b: T, c: T) -> bool {
    if a <= c { a <= b && b < c } else { a <= b || b < c }
}

fn quadform(q: &[f64; 9], w: DPoint) -> f64 {
    let v = [w.x, w.y, 1.0];
    let mut sum = 0.0;
    for i in 0..3 {
        for j in 0..3 {
            sum += v[i] * q[i * 3 + j] * v[j];
        }
    }
    sum
}

fn bezier(t: f64, p0: DPoint, p1: DPoint, p2: DPoint, p3: DPoint) -> DPoint {
    let s = 1.0 - t;
    DPoint::new(
        s * s * s * p0.x + 3.0 * (s * s * t) * p1.x + 3.0 * (t * t * s) * p2.x + t * t * t * p3.x,
        s * s * s * p0.y + 3.0 * (s * s * t) * p1.y + 3.0 * (t * t * s) * p2.y + t * t * t * p3.y,
    )
}

fn tangent(p0: DPoint, p1: DPoint, p2: DPoint, p3: DPoint, q0: DPoint, q1: DPoint) -> f64 {
    let a_ = cprod(p0, p1, q0, q1);
    let b_ = cprod(p1, p2, q0, q1);
    let c_ = cprod(p2, p3, q0, q1);
    let a = a_ - 2.0 * b_ + c_;
    let b = -2.0 * a_ + 2.0 * b_;
    let c = a_;
    let d = b * b - 4.0 * a * c;
    if a == 0.0 || d < 0.0 {
        return -1.0;
    }
    let s = d.sqrt();
    let r1 = (-b + s) / (2.0 * a);
    let r2 = (-b - s) / (2.0 * a);
    if (0.0..=1.0).contains(&r1) {
        r1
    } else if (0.0..=1.0).contains(&r2) {
        r2
    } else {
        -1.0
    }
}

fn modi(a: i32, n: i32) -> i32 {
    if a >= n {
        a % n
    } else if a >= 0 {
        a
    } else {
        n - 1 - (-1 - a) % n
    }
}

/// `ConvertBitmap`: black where `R + G + B < 0.5 · 255 · 3` (alpha ignored).
fn convert(img: &Image) -> Bits {
    let data = img
        .pixels()
        .iter()
        .map(|&p| {
            let (_, r, g, b) = split(p);
            f64::from(r) + f64::from(g) + f64::from(b) < 0.5 * 255.0 * 3.0
        })
        .collect();
    #[allow(clippy::cast_possible_wrap)]
    Bits {
        w: img.width() as i32,
        h: img.height() as i32,
        data,
    }
}

fn find_next(bm: &Bits, from: (i32, i32)) -> Option<(i32, i32)> {
    let mut i = bm.w * from.1 + from.0;
    while i < bm.size() && !bm.data[i as usize] {
        i += 1;
    }
    if i >= bm.size() {
        return None;
    }
    let y = i / bm.w;
    Some((i - y * bm.w, y))
}

fn majority(bm: &Bits, x: i32, y: i32) -> bool {
    for i in 2..5 {
        let mut ct = 0;
        for a in (-i + 1)..i {
            ct += if bm.at(x + a, y + i - 1) { 1 } else { -1 };
            ct += if bm.at(x + i - 1, y + a - 1) { 1 } else { -1 };
            ct += if bm.at(x + a - 1, y - i) { 1 } else { -1 };
            ct += if bm.at(x - i, y + a) { 1 } else { -1 };
        }
        if ct > 0 {
            return true;
        } else if ct < 0 {
            return false;
        }
    }
    false
}

/// `findPath` with the turn policy `minority` (LaserGRBL's).
fn find_path(original: &Bits, bm: &Bits, start: (i32, i32)) -> TracePath {
    let (mut x, mut y) = start;
    let (mut dirx, mut diry) = (0, 1);
    let mut path = TracePath {
        area: 0,
        len: 0,
        positive: original.at(x, y),
        pt: Vec::new(),
        max_x: -1,
        x0: 0.0,
        y0: 0.0,
        m: 0,
        po: Vec::new(),
        lon: Vec::new(),
        sums: Vec::new(),
        curve: PrivCurve::new(0),
    };
    loop {
        path.pt.push((x, y));
        path.max_x = path.max_x.max(x);
        path.len += 1;
        x += dirx;
        y += diry;
        path.area -= x * diry;
        if (x, y) == start {
            break;
        }
        let l = bm.at(x + (dirx + diry - 1) / 2, y + (diry - dirx - 1) / 2);
        let r = bm.at(x + (dirx - diry - 1) / 2, y + (diry + dirx - 1) / 2);
        if r && !l {
            // turn policy "minority": turn right unless the majority is black
            if majority(bm, x, y) {
                (dirx, diry) = (diry, -dirx);
            } else {
                (dirx, diry) = (-diry, dirx);
            }
        } else if r {
            (dirx, diry) = (-diry, dirx);
        } else if !l {
            (dirx, diry) = (diry, -dirx);
        }
    }
    path
}

fn xor_path(bm: &mut Bits, path: &TracePath) {
    let mut y1 = path.pt[0].1;
    for &(x, y) in path.pt.iter().skip(1) {
        if y != y1 {
            let min_y = y1.min(y);
            for j in x..path.max_x {
                bm.flip(j, min_y);
            }
            y1 = y;
        }
    }
}

fn calc_sums(path: &mut TracePath) {
    path.x0 = f64::from(path.pt[0].0);
    path.y0 = f64::from(path.pt[0].1);
    path.sums.push(Sum::default());
    for i in 0..path.pt.len() {
        let x = f64::from(path.pt[i].0) - path.x0;
        let y = f64::from(path.pt[i].1) - path.y0;
        let s = path.sums[i];
        path.sums.push(Sum {
            x: s.x + x,
            y: s.y + y,
            xy: s.xy + x * y,
            x2: s.x2 + x * x,
            y2: s.y2 + y * y,
        });
    }
}

#[allow(clippy::many_single_char_names)]
fn calc_lon(path: &mut TracePath) {
    let n = path.len;
    let pt = &path.pt;
    let nu = n as usize;
    let mut pivk = vec![0i32; nu];
    let mut nc = vec![0i32; nu];
    path.lon = vec![0; nu];
    let at = |i: i32| pt[i as usize];

    let mut k = 0;
    for i in (0..n).rev() {
        if at(i).0 != at(k).0 && at(i).1 != at(k).1 {
            k = i + 1;
        }
        nc[i as usize] = k;
    }

    for i in (0..n).rev() {
        let mut ct = [0i32; 4];
        let nx = at(modi(i + 1, n));
        let dir = (3 + 3 * (nx.0 - at(i).0) + (nx.1 - at(i).1)) / 2;
        ct[dir as usize] += 1;
        let mut constraint = [(0i32, 0i32); 2];
        let mut k = nc[i as usize];
        let mut k1 = i;
        let mut found = false;
        loop {
            let dir = (3 + 3 * isign(at(k).0 - at(k1).0) + isign(at(k).1 - at(k1).1)) / 2;
            ct[dir as usize] += 1;
            if ct == [1, 1, 1, 1] {
                pivk[i as usize] = k1;
                found = true;
                break;
            }
            let cur = (at(k).0 - at(i).0, at(k).1 - at(i).1);
            if xprodi(constraint[0], cur) < 0 || xprodi(constraint[1], cur) > 0 {
                break;
            }
            if cur.0.abs() <= 1 && cur.1.abs() <= 1 {
                // nothing
            } else {
                let off = (
                    cur.0
                        + if cur.1 >= 0 && (cur.1 > 0 || cur.0 < 0) {
                            1
                        } else {
                            -1
                        },
                    cur.1
                        + if cur.0 <= 0 && (cur.0 < 0 || cur.1 < 0) {
                            1
                        } else {
                            -1
                        },
                );
                if xprodi(constraint[0], off) >= 0 {
                    constraint[0] = off;
                }
                let off = (
                    cur.0
                        + if cur.1 <= 0 && (cur.1 < 0 || cur.0 < 0) {
                            1
                        } else {
                            -1
                        },
                    cur.1
                        + if cur.0 >= 0 && (cur.0 > 0 || cur.1 < 0) {
                            1
                        } else {
                            -1
                        },
                );
                if xprodi(constraint[1], off) <= 0 {
                    constraint[1] = off;
                }
            }
            k1 = k;
            k = nc[k1 as usize];
            if !cyclic(k, i, k1) {
                break;
            }
        }
        if !found {
            let dk = (isign(at(k).0 - at(k1).0), isign(at(k).1 - at(k1).1));
            let cur = (at(k1).0 - at(i).0, at(k1).1 - at(i).1);
            let a = xprodi(constraint[0], cur);
            let b = xprodi(constraint[0], dk);
            let c = xprodi(constraint[1], cur);
            let d = xprodi(constraint[1], dk);
            let mut j = 10_000_000;
            if b < 0 {
                j = a / -b;
            }
            if d > 0 {
                j = j.min(-c / d);
            }
            pivk[i as usize] = modi(k1 + j, n);
        }
    }

    let mut j = pivk[nu - 1];
    path.lon[nu - 1] = j;
    for i in (0..n - 1).rev() {
        if cyclic(i + 1, pivk[i as usize], j) {
            j = pivk[i as usize];
        }
        path.lon[i as usize] = j;
    }
    let mut i = n - 1;
    while i >= 0 && cyclic(modi(i + 1, n), j, path.lon[i as usize]) {
        path.lon[i as usize] = j;
        i -= 1;
    }
}

fn penalty3(path: &TracePath, i: i32, j: i32) -> f64 {
    let n = path.len;
    let pt = &path.pt;
    let sums = &path.sums;
    let (j, r) = if j >= n { (j - n, true) } else { (j, false) };
    let (iu, ju, nu) = (i as usize, j as usize, n as usize);
    let (x, y, x2, xy, y2, k);
    if r {
        x = sums[ju + 1].x - sums[iu].x + sums[nu].x;
        y = sums[ju + 1].y - sums[iu].y + sums[nu].y;
        x2 = sums[ju + 1].x2 - sums[iu].x2 + sums[nu].x2;
        xy = sums[ju + 1].xy - sums[iu].xy + sums[nu].xy;
        y2 = sums[ju + 1].y2 - sums[iu].y2 + sums[nu].y2;
        k = j + 1 - i + n;
    } else {
        x = sums[ju + 1].x - sums[iu].x;
        y = sums[ju + 1].y - sums[iu].y;
        x2 = sums[ju + 1].x2 - sums[iu].x2;
        xy = sums[ju + 1].xy - sums[iu].xy;
        y2 = sums[ju + 1].y2 - sums[iu].y2;
        k = j + 1 - i;
    }
    let (pi, pj, p0) = (pt[iu], pt[j as usize], pt[0]);
    let px = f64::from(pi.0 + pj.0) / 2.0 - f64::from(p0.0);
    let py = f64::from(pi.1 + pj.1) / 2.0 - f64::from(p0.1);
    let ey = f64::from(pj.0 - pi.0);
    let ex = -f64::from(pj.1 - pi.1);
    let k = f64::from(k);
    let a = (x2 - 2.0 * x * px) / k + px * px;
    let b = (xy - x * py - y * px) / k + px * py;
    let c = (y2 - 2.0 * y * py) / k + py * py;
    let s = ex * ex * a + 2.0 * ex * ey * b + ey * ey * c;
    s.sqrt()
}

fn best_polygon(path: &mut TracePath) {
    let n = path.len;
    let nu = n as usize;
    let mut clip0 = vec![0i32; nu];
    let mut pen = vec![0.0f64; nu + 1];
    let mut prev = vec![0i32; nu + 1];
    let mut clip1 = vec![0i32; nu + 1];
    let mut seg0 = vec![0i32; nu + 1];
    let mut seg1 = vec![0i32; nu + 1];

    for i in 0..n {
        let mut c = modi(path.lon[modi(i - 1, n) as usize] - 1, n);
        if c == i {
            c = modi(i + 1, n);
        }
        clip0[i as usize] = if c < i { n } else { c };
    }
    let mut j = 1;
    for i in 0..n {
        while j <= clip0[i as usize] {
            clip1[j as usize] = i;
            j += 1;
        }
    }
    let mut i = 0;
    j = 0;
    while i < n {
        seg0[j as usize] = i;
        i = clip0[i as usize];
        j += 1;
    }
    seg0[j as usize] = n;
    let m = j;
    i = n;
    for j in (1..=m).rev() {
        seg1[j as usize] = i;
        i = clip1[i as usize];
    }
    seg1[0] = 0;
    pen[0] = 0.0;
    for j in 1..=m {
        for i in seg1[j as usize]..=seg0[j as usize] {
            let mut best = -1.0;
            let mut k = seg0[j as usize - 1];
            while k >= clip1[i as usize] {
                let thispen = penalty3(path, k, i) + pen[k as usize];
                if best < 0.0 || thispen < best {
                    prev[i as usize] = k;
                    best = thispen;
                }
                k -= 1;
            }
            pen[i as usize] = best;
        }
    }
    path.m = m as usize;
    path.po = vec![0; m as usize];
    let mut i = n;
    let mut j = m - 1;
    while i > 0 {
        i = prev[i as usize];
        path.po[j as usize] = i;
        j -= 1;
    }
}

fn pointslope(path: &TracePath, mut i: i32, mut j: i32) -> (DPoint, DPoint) {
    let n = path.len;
    let sums = &path.sums;
    let mut r = 0;
    while j >= n {
        j -= n;
        r += 1;
    }
    while i >= n {
        i -= n;
        r -= 1;
    }
    while j < 0 {
        j += n;
        r -= 1;
    }
    while i < 0 {
        i += n;
        r += 1;
    }
    let (iu, ju, nu) = (i as usize, j as usize, n as usize);
    let rf = f64::from(r);
    let x = sums[ju + 1].x - sums[iu].x + rf * sums[nu].x;
    let y = sums[ju + 1].y - sums[iu].y + rf * sums[nu].y;
    let x2 = sums[ju + 1].x2 - sums[iu].x2 + rf * sums[nu].x2;
    let xy = sums[ju + 1].xy - sums[iu].xy + rf * sums[nu].xy;
    let y2 = sums[ju + 1].y2 - sums[iu].y2 + rf * sums[nu].y2;
    let k = f64::from(j + 1 - i + r * n);
    let ctr = DPoint::new(x / k, y / k);
    let mut a = (x2 - x * x / k) / k;
    let b = (xy - x * y / k) / k;
    let mut c = (y2 - y * y / k) / k;
    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) / 2.0;
    a -= lambda2;
    c -= lambda2;
    let mut dir = DPoint::default();
    let l;
    if a.abs() >= c.abs() {
        l = (a * a + b * b).sqrt();
        if l != 0.0 {
            dir = DPoint::new(-b / l, a / l);
        }
    } else {
        l = (c * c + b * b).sqrt();
        if l != 0.0 {
            dir = DPoint::new(-c / l, b / l);
        }
    }
    if l == 0.0 {
        dir = DPoint::new(0.0, 0.0);
    }
    (ctr, dir)
}

#[allow(clippy::too_many_lines)]
fn adjust_vertices(path: &mut TracePath) {
    let m = path.m;
    let n = path.len;
    let (x0, y0) = (path.x0, path.y0);
    let mi = m as i32;
    let mut ctr = Vec::with_capacity(m);
    let mut dir = Vec::with_capacity(m);
    for i in 0..m {
        let j = path.po[modi(i as i32 + 1, mi) as usize];
        let j = modi(j - path.po[i], n) + path.po[i];
        let (c, d) = pointslope(path, path.po[i], j);
        ctr.push(c);
        dir.push(d);
    }
    let mut q = vec![[0.0f64; 9]; m];
    let mut v = [0.0f64; 3];
    for i in 0..m {
        let d = dir[i].x * dir[i].x + dir[i].y * dir[i].y;
        if d == 0.0 {
            q[i] = [0.0; 9];
        } else {
            v[0] = dir[i].y;
            v[1] = -dir[i].x;
            v[2] = -v[1] * ctr[i].y - v[0] * ctr[i].x;
            for l in 0..3 {
                for k in 0..3 {
                    q[i][l * 3 + k] = v[l] * v[k] / d;
                }
            }
        }
    }
    path.curve = PrivCurve::new(m);
    for i in 0..m {
        let p = path.pt[path.po[i] as usize];
        let s = DPoint::new(f64::from(p.0) - x0, f64::from(p.1) - y0);
        let j = modi(i as i32 - 1, mi) as usize;
        let mut qq = [0.0f64; 9];
        for l in 0..3 {
            for k in 0..3 {
                qq[l * 3 + k] = q[j][l * 3 + k] + q[i][l * 3 + k];
            }
        }
        let at = |qq: &[f64; 9], a: usize, b: usize| qq[a * 3 + b];
        let mut w = DPoint::default();
        loop {
            let det = at(&qq, 0, 0) * at(&qq, 1, 1) - at(&qq, 0, 1) * at(&qq, 1, 0);
            if det != 0.0 {
                w.x = (-at(&qq, 0, 2) * at(&qq, 1, 1) + at(&qq, 1, 2) * at(&qq, 0, 1)) / det;
                w.y = (at(&qq, 0, 2) * at(&qq, 1, 0) - at(&qq, 1, 2) * at(&qq, 0, 0)) / det;
                break;
            }
            if at(&qq, 0, 0) > at(&qq, 1, 1) {
                v[0] = -at(&qq, 0, 1);
                v[1] = at(&qq, 0, 0);
            } else if at(&qq, 1, 1) != 0.0 {
                v[0] = -at(&qq, 1, 1);
                v[1] = at(&qq, 1, 0);
            } else {
                v[0] = 1.0;
                v[1] = 0.0;
            }
            let d = v[0] * v[0] + v[1] * v[1];
            v[2] = -v[1] * s.y - v[0] * s.x;
            for l in 0..3 {
                for k in 0..3 {
                    qq[l * 3 + k] += v[l] * v[k] / d;
                }
            }
        }
        let dx = (w.x - s.x).abs();
        let dy = (w.y - s.y).abs();
        if dx <= 0.5 && dy <= 0.5 {
            path.curve.vertex[i] = DPoint::new(w.x + x0, w.y + y0);
            continue;
        }
        let mut min = quadform(&qq, s);
        let mut xmin = s.x;
        let mut ymin = s.y;
        if at(&qq, 0, 0) != 0.0 {
            for z in 0..2 {
                w.y = s.y - 0.5 + f64::from(z);
                w.x = -(at(&qq, 0, 1) * w.y + at(&qq, 0, 2)) / at(&qq, 0, 0);
                let dx = (w.x - s.x).abs();
                let cand = quadform(&qq, w);
                if dx <= 0.5 && cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }
        if at(&qq, 1, 1) != 0.0 {
            for z in 0..2 {
                w.x = s.x - 0.5 + f64::from(z);
                w.y = -(at(&qq, 1, 0) * w.x + at(&qq, 1, 2)) / at(&qq, 1, 1);
                let dy = (w.y - s.y).abs();
                let cand = quadform(&qq, w);
                if dy <= 0.5 && cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }
        for l in 0..2 {
            for k in 0..2 {
                w.x = s.x - 0.5 + f64::from(l);
                w.y = s.y - 0.5 + f64::from(k);
                let cand = quadform(&qq, w);
                if cand < min {
                    min = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }
        path.curve.vertex[i] = DPoint::new(xmin + x0, ymin + y0);
    }
}

fn smooth(path: &mut TracePath, alphamax: f64) {
    let curve = &mut path.curve;
    let m = curve.n;
    if !path.positive {
        curve.vertex.reverse();
    }
    let mi = m as i32;
    for i in 0..m {
        let j = modi(i as i32 + 1, mi) as usize;
        let k = modi(i as i32 + 2, mi) as usize;
        let p4 = interval(1.0 / 2.0, curve.vertex[k], curve.vertex[j]);
        let denom = ddenom(curve.vertex[i], curve.vertex[k]);
        let mut alpha = if denom == 0.0 {
            4.0 / 3.0
        } else {
            let dd = (dpara(curve.vertex[i], curve.vertex[j], curve.vertex[k]) / denom).abs();
            let a = if dd > 1.0 { 1.0 - 1.0 / dd } else { 0.0 };
            a / 0.75
        };
        if alpha >= alphamax {
            curve.tag[j] = POTRACE_CORNER;
            curve.c[3 * j + 1] = curve.vertex[j];
            curve.c[3 * j + 2] = p4;
        } else {
            alpha = alpha.clamp(0.55, 1.0);
            let p2 = interval(0.5 + 0.5 * alpha, curve.vertex[i], curve.vertex[j]);
            let p3 = interval(0.5 + 0.5 * alpha, curve.vertex[k], curve.vertex[j]);
            curve.tag[j] = POTRACE_CURVETO;
            curve.c[3 * j] = p2;
            curve.c[3 * j + 1] = p3;
            curve.c[3 * j + 2] = p4;
        }
        curve.alpha[j] = alpha;
        curve.beta[j] = 0.5;
    }
}

#[derive(Clone, Copy, Default)]
struct Opti {
    pen: f64,
    c: [DPoint; 2],
    t: f64,
    s: f64,
    alpha: f64,
}

#[allow(clippy::too_many_lines, clippy::similar_names)]
fn opti_penalty(
    curve: &PrivCurve,
    i: i32,
    j: i32,
    opttolerance: f64,
    convc: &[i32],
    areac: &[f64],
) -> Option<Opti> {
    let m = curve.n as i32;
    let vertex = &curve.vertex;
    let v = |k: i32| vertex[k as usize];
    let cc = |k: i32| curve.c[k as usize];
    if i == j {
        return None;
    }
    let k = i;
    let i1 = modi(i + 1, m);
    let mut k1 = modi(k + 1, m);
    let conv = convc[k1 as usize];
    if conv == 0 {
        return None;
    }
    let d = ddist(v(i), v(i1));
    let mut k = k1;
    while k != j {
        k1 = modi(k + 1, m);
        let k2 = modi(k + 2, m);
        if convc[k1 as usize] != conv {
            return None;
        }
        if sign(cprod(v(i), v(i1), v(k1), v(k2))) != conv {
            return None;
        }
        if iprod1(v(i), v(i1), v(k1), v(k2)) < d * ddist(v(k1), v(k2)) * -0.999_847_695_156 {
            return None;
        }
        k = k1;
    }
    let p0 = cc(modi(i, m) * 3 + 2);
    let mut p1 = v(modi(i + 1, m));
    let mut p2 = v(modi(j, m));
    let p3 = cc(modi(j, m) * 3 + 2);
    let mut area = areac[j as usize] - areac[i as usize];
    area -= dpara(v(0), cc(i * 3 + 2), cc(j * 3 + 2)) / 2.0;
    if i >= j {
        area += areac[m as usize];
    }
    let a1 = dpara(p0, p1, p2);
    let a2 = dpara(p0, p1, p3);
    let a3 = dpara(p0, p2, p3);
    let a4 = a1 + a3 - a2;
    if a2 == a1 {
        return None;
    }
    let mut t = a3 / (a3 - a4);
    let s = a2 / (a2 - a1);
    let a = a2 * t / 2.0;
    if a == 0.0 {
        return None;
    }
    let r = area / a;
    let alpha = 2.0 - (4.0 - r / 0.3).sqrt();
    let mut res = Opti {
        pen: 0.0,
        c: [interval(t * alpha, p0, p1), interval(s * alpha, p3, p2)],
        alpha,
        t,
        s,
    };
    p1 = res.c[0];
    p2 = res.c[1];
    let mut k = modi(i + 1, m);
    while k != j {
        let k1 = modi(k + 1, m);
        t = tangent(p0, p1, p2, p3, v(k), v(k1));
        if t < -0.5 {
            return None;
        }
        let pt = bezier(t, p0, p1, p2, p3);
        let d = ddist(v(k), v(k1));
        if d == 0.0 {
            return None;
        }
        let d1 = dpara(v(k), v(k1), pt) / d;
        if d1.abs() > opttolerance {
            return None;
        }
        if iprod(v(k), v(k1), pt) < 0.0 || iprod(v(k1), v(k), pt) < 0.0 {
            return None;
        }
        res.pen += d1 * d1;
        k = k1;
    }
    let mut k = i;
    while k != j {
        let k1 = modi(k + 1, m);
        t = tangent(p0, p1, p2, p3, cc(k * 3 + 2), cc(k1 * 3 + 2));
        if t < -0.5 {
            return None;
        }
        let pt = bezier(t, p0, p1, p2, p3);
        let d = ddist(cc(k * 3 + 2), cc(k1 * 3 + 2));
        if d == 0.0 {
            return None;
        }
        let mut d1 = dpara(cc(k * 3 + 2), cc(k1 * 3 + 2), pt) / d;
        let mut d2 = dpara(cc(k * 3 + 2), cc(k1 * 3 + 2), v(k1)) / d;
        d2 *= 0.75 * curve.alpha[k1 as usize];
        if d2 < 0.0 {
            d1 = -d1;
            d2 = -d2;
        }
        if d1 < d2 - opttolerance {
            return None;
        }
        if d1 < d2 {
            res.pen += (d1 - d2) * (d1 - d2);
        }
        k = k1;
    }
    Some(res)
}

#[allow(clippy::too_many_lines, clippy::needless_range_loop)]
fn opti_curve(path: &mut TracePath, opttolerance: f64) {
    let curve = &path.curve;
    let m = curve.n;
    let mi = m as i32;
    let vert = &curve.vertex;
    let mut pt = vec![0i32; m + 1];
    let mut pen = vec![0.0f64; m + 1];
    let mut len = vec![0i32; m + 1];
    let mut opt = vec![Opti::default(); m + 1];
    let mut convc = vec![0i32; m];
    let mut areac = vec![0.0f64; m + 1];
    for i in 0..m {
        convc[i] = if curve.tag[i] == POTRACE_CURVETO {
            sign(dpara(
                vert[modi(i as i32 - 1, mi) as usize],
                vert[i],
                vert[modi(i as i32 + 1, mi) as usize],
            ))
        } else {
            0
        };
    }
    let mut area = 0.0;
    areac[0] = 0.0;
    let p0 = curve.vertex[0];
    for i in 0..m {
        let i1 = modi(i as i32 + 1, mi) as usize;
        if curve.tag[i1] == POTRACE_CURVETO {
            let alpha = curve.alpha[i1];
            area +=
                0.3 * alpha * (4.0 - alpha) * dpara(curve.c[i * 3 + 2], vert[i1], curve.c[i1 * 3 + 2]) / 2.0;
            area += dpara(p0, curve.c[i * 3 + 2], curve.c[i1 * 3 + 2]) / 2.0;
        }
        areac[i + 1] = area;
    }
    pt[0] = -1;
    pen[0] = 0.0;
    len[0] = 0;
    for j in 1..=m {
        pt[j] = j as i32 - 1;
        pen[j] = pen[j - 1];
        len[j] = len[j - 1] + 1;
        let mut i = j as i32 - 2;
        while i >= 0 {
            let Some(o) = opti_penalty(curve, i, modi(j as i32, mi), opttolerance, &convc, &areac) else {
                break;
            };
            let iu = i as usize;
            if len[j] > len[iu] + 1 || (len[j] == len[iu] + 1 && pen[j] > pen[iu] + o.pen) {
                pt[j] = i;
                pen[j] = pen[iu] + o.pen;
                len[j] = len[iu] + 1;
                opt[j] = o;
            }
            i -= 1;
        }
    }
    let om = len[m] as usize;
    let mut ocurve = PrivCurve::new(om);
    let mut s = vec![0.0f64; om];
    let mut t = vec![0.0f64; om];
    let mut j = m as i32;
    for i in (0..om).rev() {
        let jm = modi(j, mi) as usize;
        if pt[j as usize] == j - 1 {
            ocurve.tag[i] = curve.tag[jm];
            ocurve.c[i * 3] = curve.c[jm * 3];
            ocurve.c[i * 3 + 1] = curve.c[jm * 3 + 1];
            ocurve.c[i * 3 + 2] = curve.c[jm * 3 + 2];
            ocurve.vertex[i] = curve.vertex[jm];
            ocurve.alpha[i] = curve.alpha[jm];
            ocurve.beta[i] = curve.beta[jm];
            s[i] = 1.0;
            t[i] = 1.0;
        } else {
            let o = opt[j as usize];
            ocurve.tag[i] = POTRACE_CURVETO;
            ocurve.c[i * 3] = o.c[0];
            ocurve.c[i * 3 + 1] = o.c[1];
            ocurve.c[i * 3 + 2] = curve.c[jm * 3 + 2];
            ocurve.vertex[i] = interval(o.s, curve.c[jm * 3 + 2], vert[jm]);
            ocurve.alpha[i] = o.alpha;
            s[i] = o.s;
            t[i] = o.t;
        }
        j = pt[j as usize];
    }
    for i in 0..om {
        let i1 = modi(i as i32 + 1, om as i32) as usize;
        ocurve.beta[i] = s[i] / (s[i] + t[i1]);
    }
    path.curve = ocurve;
}

/// `tracetoList`: every path as a list of lines and Béziers.
fn to_curves(path: &TracePath) -> Vec<Curve> {
    let c = &path.curve;
    let mut out = Vec::new();
    if c.n == 0 {
        return out;
    }
    let mut l = c.c[(c.n - 1) * 3 + 2];
    for j in 0..c.n {
        let a = c.c[j * 3 + 1];
        let b = c.c[j * 3 + 2];
        if c.tag[j] == POTRACE_CORNER {
            out.push(Curve::line(l, a));
            out.push(Curve::line(a, b));
        } else {
            out.push(Curve {
                kind: CurveKind::Bezier,
                a: l,
                control_a: c.c[j * 3],
                control_b: a,
                b,
            });
        }
        l = b;
    }
    out
}

/// `Potrace.PotraceTrace`: the outlines of the black areas of `img` (row 0 at
/// `y = 0`), outer outlines before the holes inside them.
#[must_use]
pub fn trace(img: &Image, options: &TraceOptions) -> Vec<Vec<Curve>> {
    let original = convert(img);
    let mut bm = original.clone();
    let mut paths = Vec::new();
    let mut current = (0, 0);
    while let Some(p) = find_next(&bm, current) {
        current = p;
        let path = find_path(&original, &bm, current);
        xor_path(&mut bm, &path);
        if path.area > options.turdsize {
            paths.push(path);
        }
    }
    for path in &mut paths {
        calc_sums(path);
        calc_lon(path);
        best_polygon(path);
        adjust_vertices(path);
        smooth(path, options.alphamax);
        if options.curve_optimizing {
            opti_curve(path, options.opttolerance);
        }
    }
    paths.iter().map(to_curves).collect()
}
