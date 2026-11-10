//! Растровые буферы и примитивы: изображение RGBA (с предумноженной альфой), заливка многоугольников,
//! размытие, заливка области, аффинная выборка.

use crate::util::Rect;
use rayon::prelude::*;

/// Изображение RGBA8 с предумноженной альфой (как в Canvas).
#[derive(Clone)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Img {
    pub fn new(w: usize, h: usize) -> Img {
        Img { w, h, px: vec![0; w * h * 4] }
    }
    pub fn filled(w: usize, h: usize, c: [u8; 4]) -> Img {
        let mut px = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            px.extend_from_slice(&c);
        }
        Img { w, h, px }
    }
    pub fn full(&self) -> Rect {
        Rect::new(0, 0, self.w as i32, self.h as i32)
    }
    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        (y * self.w + x) * 4
    }
    pub fn crop(&self, r: Rect) -> Img {
        let mut out = Img::new(r.w as usize, r.h as usize);
        let Some(c) = r.clip(self.w, self.h) else { return out };
        for y in 0..c.h as usize {
            let sy = c.y as usize + y;
            let dy = (c.y - r.y) as usize + y;
            let s = self.idx(c.x as usize, sy);
            let d = out.idx((c.x - r.x) as usize, dy);
            out.px[d..d + c.w as usize * 4].copy_from_slice(&self.px[s..s + c.w as usize * 4]);
        }
        out
    }
    /// Скопировать src в позицию (x, y) с заменой пикселей.
    pub fn put(&mut self, x: i32, y: i32, src: &Img) {
        let r = Rect::new(x, y, src.w as i32, src.h as i32);
        let Some(c) = r.clip(self.w, self.h) else { return };
        for yy in 0..c.h as usize {
            let s = src.idx((c.x - x) as usize, (c.y - y) as usize + yy);
            let d = self.idx(c.x as usize, c.y as usize + yy);
            self.px[d..d + c.w as usize * 4].copy_from_slice(&src.px[s..s + c.w as usize * 4]);
        }
    }
    pub fn copy_rect_from(&mut self, src: &Img, r: Rect) {
        let Some(c) = r.clip(self.w.min(src.w), self.h.min(src.h)) else { return };
        for y in c.y as usize..c.bottom() as usize {
            let a = self.idx(c.x as usize, y);
            self.px[a..a + c.w as usize * 4].copy_from_slice(&src.px[a..a + c.w as usize * 4]);
        }
    }
    pub fn clear_rect(&mut self, r: Rect) {
        let Some(c) = r.clip(self.w, self.h) else { return };
        for y in c.y as usize..c.bottom() as usize {
            let a = self.idx(c.x as usize, y);
            self.px[a..a + c.w as usize * 4].fill(0);
        }
    }
    pub fn clear(&mut self) {
        self.px.fill(0);
    }
    /// Нанести src поверх (source-over) в точке (x, y) с непрозрачностью.
    pub fn draw_over(&mut self, x: i32, y: i32, src: &Img, alpha: f32) {
        let r = Rect::new(x, y, src.w as i32, src.h as i32);
        let Some(c) = r.clip(self.w, self.h) else { return };
        for yy in 0..c.h as usize {
            for xx in 0..c.w as usize {
                let s = src.idx((c.x - x) as usize + xx, (c.y - y) as usize + yy);
                let d = self.idx(c.x as usize + xx, c.y as usize + yy);
                let sa = src.px[s + 3] as f32 / 255.0 * alpha;
                if sa <= 0.0 {
                    continue;
                }
                for k in 0..4 {
                    let sv = src.px[s + k] as f32 * alpha;
                    let dv = self.px[d + k] as f32;
                    self.px[d + k] = (sv + dv * (1.0 - sa)).round().min(255.0) as u8;
                }
            }
        }
    }
    /// Непрозрачные границы содержимого.
    pub fn content_bounds(&self) -> Option<Rect> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0usize, 0usize);
        let mut any = false;
        for y in 0..self.h {
            let row = &self.px[y * self.w * 4..(y + 1) * self.w * 4];
            let mut first = None;
            let mut last = 0;
            for x in 0..self.w {
                if row[x * 4 + 3] != 0 {
                    if first.is_none() {
                        first = Some(x);
                    }
                    last = x;
                }
            }
            if let Some(f) = first {
                any = true;
                x0 = x0.min(f);
                x1 = x1.max(last);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        if any {
            Some(Rect::new(x0 as i32, y0 as i32, (x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32))
        } else {
            None
        }
    }
    /// Перевод в RGBA без предумножения (для PNG).
    pub fn to_straight(&self) -> Vec<u8> {
        let mut out = self.px.clone();
        for p in out.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for k in 0..3 {
                    p[k] = ((p[k] as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }
    pub fn from_straight(w: usize, h: usize, rgba: &[u8]) -> Img {
        let mut px = rgba.to_vec();
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a < 255 {
                for k in 0..3 {
                    p[k] = ((p[k] as u32 * a + 127) / 255) as u8;
                }
            }
        }
        Img { w, h, px }
    }
    /// Уменьшенная копия (усреднение блоков) — для миниатюр.
    pub fn thumbnail(&self, max: usize) -> Img {
        let k = (max as f32 / self.w.max(self.h) as f32).min(1.0);
        let tw = ((self.w as f32 * k).round() as usize).max(1);
        let th = ((self.h as f32 * k).round() as usize).max(1);
        self.resize(tw, th)
    }
    /// Масштабирование с усреднением (уменьшение) или билинейно (увеличение).
    pub fn resize(&self, tw: usize, th: usize) -> Img {
        let mut out = Img::new(tw, th);
        let sx = self.w as f32 / tw as f32;
        let sy = self.h as f32 / th as f32;
        out.px.par_chunks_mut(tw * 4).enumerate().for_each(|(y, row)| {
            for x in 0..tw {
                let fx0 = x as f32 * sx;
                let fy0 = y as f32 * sy;
                let mut acc = [0f32; 4];
                if sx <= 1.0 && sy <= 1.0 {
                    let v = self.sample(fx0 + sx * 0.5 - 0.5, fy0 + sy * 0.5 - 0.5);
                    acc = v;
                } else {
                    let x0 = fx0 as usize;
                    let y0 = fy0 as usize;
                    let x1 = ((fx0 + sx).ceil() as usize).min(self.w).max(x0 + 1);
                    let y1 = ((fy0 + sy).ceil() as usize).min(self.h).max(y0 + 1);
                    let mut n = 0.0;
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            let i = self.idx(xx, yy);
                            for k in 0..4 {
                                acc[k] += self.px[i + k] as f32;
                            }
                            n += 1.0;
                        }
                    }
                    for k in 0..4 {
                        acc[k] /= n;
                    }
                }
                for k in 0..4 {
                    row[x * 4 + k] = acc[k].round().clamp(0.0, 255.0) as u8;
                }
            }
        });
        out
    }
    /// Билинейная выборка (предумноженные значения), за краем — прозрачно.
    #[inline]
    pub fn sample(&self, fx: f32, fy: f32) -> [f32; 4] {
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let x0 = x0 as i64;
        let y0 = y0 as i64;
        let mut out = [0f32; 4];
        let w = self.w as i64;
        let h = self.h as i64;
        for (dx, dy, wt) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let x = x0 + dx;
            let y = y0 + dy;
            if x < 0 || y < 0 || x >= w || y >= h || wt <= 0.0 {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            for k in 0..4 {
                out[k] += self.px[i + k] as f32 * wt;
            }
        }
        out
    }
    pub fn flip(&mut self, horiz: bool) {
        let (w, h) = (self.w, self.h);
        let mut out = vec![0u8; self.px.len()];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = if horiz { (w - 1 - x, y) } else { (x, h - 1 - y) };
                let s = (sy * w + sx) * 4;
                let d = (y * w + x) * 4;
                out[d..d + 4].copy_from_slice(&self.px[s..s + 4]);
            }
        }
        self.px = out;
    }
    pub fn rotate90(&self, cw: bool) -> Img {
        let mut out = Img::new(self.h, self.w);
        for y in 0..self.h {
            for x in 0..self.w {
                let (nx, ny) = if cw { (self.h - 1 - y, x) } else { (y, self.w - 1 - x) };
                let s = self.idx(x, y);
                let d = out.idx(nx, ny);
                out.px[d..d + 4].copy_from_slice(&self.px[s..s + 4]);
            }
        }
        out
    }
}

/* ---------- заливка многоугольников со сглаживанием ---------- */

/// Покрытие многоугольников: вызывает emit(x, y, coverage) для каждого задетого пикселя.
pub fn fill_polys(w: usize, h: usize, polys: &[Vec<(f32, f32)>], even_odd: bool, mut emit: impl FnMut(usize, usize, f32)) {
    const SS: usize = 5;
    let mut miny = f32::MAX;
    let mut maxy = f32::MIN;
    let mut minx = f32::MAX;
    let mut maxx = f32::MIN;
    for p in polys {
        for &(x, y) in p {
            miny = miny.min(y);
            maxy = maxy.max(y);
            minx = minx.min(x);
            maxx = maxx.max(x);
        }
    }
    if miny > maxy {
        return;
    }
    let y0 = miny.floor().max(0.0) as usize;
    let y1 = (maxy.ceil() as i64).clamp(0, h as i64) as usize;
    let x0 = minx.floor().max(0.0) as usize;
    let x1 = (maxx.ceil() as i64 + 1).clamp(0, w as i64) as usize;
    if x1 <= x0 {
        return;
    }
    let mut acc = vec![0f32; x1 - x0 + 1];
    let mut xs: Vec<(f32, i32)> = Vec::new();
    for py in y0..y1 {
        acc.iter_mut().for_each(|v| *v = 0.0);
        let mut touched = false;
        for s in 0..SS {
            let sy = py as f32 + (s as f32 + 0.5) / SS as f32;
            xs.clear();
            for p in polys {
                let n = p.len();
                if n < 2 {
                    continue;
                }
                for i in 0..n {
                    let a = p[i];
                    let b = p[(i + 1) % n];
                    if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                        let x = a.0 + (sy - a.1) * (b.0 - a.0) / (b.1 - a.1);
                        xs.push((x, if b.1 > a.1 { 1 } else { -1 }));
                    }
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut wind = 0;
            for i in 0..xs.len() - 1 {
                wind += xs[i].1;
                let inside = if even_odd { wind % 2 != 0 } else { wind != 0 };
                if inside {
                    let xa = (xs[i].0 - x0 as f32).max(0.0);
                    let xb = (xs[i + 1].0 - x0 as f32).min((x1 - x0) as f32);
                    if xb > xa {
                        touched = true;
                        let wgt = 1.0 / SS as f32;
                        let ia = xa.floor() as usize;
                        let ib = xb.floor() as usize;
                        if ia == ib {
                            acc[ia] += (xb - xa) * wgt;
                        } else {
                            acc[ia] += (ia as f32 + 1.0 - xa) * wgt;
                            for v in acc.iter_mut().take(ib).skip(ia + 1) {
                                *v += wgt;
                            }
                            if ib < acc.len() {
                                acc[ib] += (xb - ib as f32) * wgt;
                            }
                        }
                    }
                }
            }
        }
        if touched {
            for (i, &v) in acc.iter().enumerate() {
                if v > 0.001 && x0 + i < w {
                    emit(x0 + i, py, v.min(1.0));
                }
            }
        }
    }
}

pub fn flatten_quad(out: &mut Vec<(f32, f32)>, a: (f32, f32), c: (f32, f32), b: (f32, f32), n: usize) {
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let u = 1.0 - t;
        out.push((u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1));
    }
}

pub fn flatten_cubic(out: &mut Vec<(f32, f32)>, a: (f32, f32), c1: (f32, f32), c2: (f32, f32), b: (f32, f32), n: usize) {
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let u = 1.0 - t;
        let w0 = u * u * u;
        let w1 = 3.0 * u * u * t;
        let w2 = 3.0 * u * t * t;
        let w3 = t * t * t;
        out.push((w0 * a.0 + w1 * c1.0 + w2 * c2.0 + w3 * b.0, w0 * a.1 + w1 * c1.1 + w2 * c2.1 + w3 * b.1));
    }
}

pub fn ellipse_poly(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    let n = ((rx.abs() + ry.abs()) * 0.6).clamp(24.0, 720.0) as usize;
    (0..n).map(|i| {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        (cx + t.cos() * rx, cy + t.sin() * ry)
    }).collect()
}

/// Расстояние от точки до отрезка.
#[inline]
pub fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let qx = ax + dx * t - px;
    let qy = ay + dy * t - py;
    (qx * qx + qy * qy).sqrt()
}

/* ---------- размытие ---------- */

/// Трёхпроходное размытие прямоугольником (≈ гауссово) по RGBA с предумножением.
pub fn blur(img: &Img, radius: f32) -> Img {
    if radius < 0.5 {
        return img.clone();
    }
    let (w, h) = (img.w, img.h);
    let r = ((radius * 0.85).round() as usize).max(1);
    let mut a: Vec<f32> = img.px.iter().map(|&v| v as f32).collect();
    for _ in 0..3 {
        let mut b = vec![0f32; a.len()];
        b.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| box_line(&a[y * w * 4..(y + 1) * w * 4], row, w, r));
        let cols: Vec<Vec<f32>> = (0..w).into_par_iter().map(|x| {
            let src: Vec<f32> = (0..h).flat_map(|y| { let i = (y * w + x) * 4; [b[i], b[i + 1], b[i + 2], b[i + 3]] }).collect();
            let mut dst = vec![0f32; h * 4];
            box_line(&src, &mut dst, h, r);
            dst
        }).collect();
        for (x, col) in cols.iter().enumerate() {
            for y in 0..h {
                let i = (y * w + x) * 4;
                a[i..i + 4].copy_from_slice(&col[y * 4..y * 4 + 4]);
            }
        }
    }
    Img { w, h, px: a.iter().map(|&v| v.round().clamp(0.0, 255.0) as u8).collect() }
}

/// Скользящее среднее по линии из n пикселей RGBA с радиусом r (края повторяются).
fn box_line(src: &[f32], dst: &mut [f32], n: usize, r: usize) {
    let div = (2 * r + 1) as f32;
    for ch in 0..4 {
        let get = |i: i64| src[(i.clamp(0, n as i64 - 1) as usize) * 4 + ch];
        let mut acc = 0.0;
        for i in -(r as i64)..=(r as i64) {
            acc += get(i);
        }
        for i in 0..n {
            dst[i * 4 + ch] = acc / div;
            acc += get(i as i64 + r as i64 + 1) - get(i as i64 - r as i64);
        }
    }
}

/* ---------- заливка области ---------- */

/// Маска заливки от точки (sx, sy) с допуском tol (0..100). Возвращает маску и границы.
pub fn flood(src: &Img, sx: usize, sy: usize, tol: f32, expand: usize) -> (Vec<u8>, Rect) {
    let (w, h) = (src.w, src.h);
    let d = &src.px;
    let i0 = (sy * w + sx) * 4;
    let t = (tol * 2.55) as i32;
    let r0 = [d[i0] as i32, d[i0 + 1] as i32, d[i0 + 2] as i32, d[i0 + 3] as i32];
    let close = |i: usize| -> bool {
        let j = i * 4;
        if r0[3] == 0 {
            return (d[j + 3] as i32) <= t;
        }
        (d[j] as i32 - r0[0]).abs() <= t && (d[j + 1] as i32 - r0[1]).abs() <= t && (d[j + 2] as i32 - r0[2]).abs() <= t && (d[j + 3] as i32 - r0[3]).abs() <= t
    };
    let mut mask = vec![0u8; w * h];
    let (mut minx, mut maxx, mut miny, mut maxy) = (sx, sx, sy, sy);
    let mut stack = vec![(sx, sy)];
    while let Some((x0, y)) = stack.pop() {
        let mut x = x0;
        let mut i = y * w + x;
        if mask[i] != 0 || !close(i) {
            continue;
        }
        while x > 0 && mask[i - 1] == 0 && close(i - 1) {
            x -= 1;
            i -= 1;
        }
        let (mut up, mut dn) = (false, false);
        minx = minx.min(x);
        miny = miny.min(y);
        maxy = maxy.max(y);
        while x < w && mask[i] == 0 && close(i) {
            mask[i] = 255;
            if y > 0 {
                let u = i - w;
                if mask[u] == 0 && close(u) {
                    if !up {
                        stack.push((x, y - 1));
                        up = true;
                    }
                } else {
                    up = false;
                }
            }
            if y + 1 < h {
                let v = i + w;
                if mask[v] == 0 && close(v) {
                    if !dn {
                        stack.push((x, y + 1));
                        dn = true;
                    }
                } else {
                    dn = false;
                }
            }
            x += 1;
            i += 1;
        }
        maxx = maxx.max(x - 1);
    }
    let mut b = Rect::new(minx as i32, miny as i32, (maxx - minx + 1) as i32, (maxy - miny + 1) as i32);
    if expand > 0 {
        let e = expand as i32;
        b = b.inflate(e).clip(w, h).unwrap_or(b);
        let mut tmp = vec![0u8; w * h];
        for y in b.y as usize..b.bottom() as usize {
            for x in b.x as usize..b.right() as usize {
                let mut v = 0;
                for k in -e..=e {
                    let xx = x as i32 + k;
                    if xx >= 0 && (xx as usize) < w && mask[y * w + xx as usize] != 0 {
                        v = 255;
                        break;
                    }
                }
                tmp[y * w + x] = v;
            }
        }
        for y in b.y as usize..b.bottom() as usize {
            for x in b.x as usize..b.right() as usize {
                let mut v = 0;
                for k in -e..=e {
                    let yy = y as i32 + k;
                    if yy >= 0 && (yy as usize) < h && tmp[yy as usize * w + x] != 0 {
                        v = 255;
                        break;
                    }
                }
                mask[y * w + x] = v;
            }
        }
    }
    (mask, b)
}

/* ---------- аффинная отрисовка ---------- */

/// Матрица 2×3: x' = a*x + c*y + e; y' = b*x + d*y + f.
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-9 {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) })
    }
}

/// Нарисовать src с преобразованием m поверх dst (source-over), билинейно или ближайшим соседом.
pub fn draw_affine(dst: &mut Img, src: &Img, m: Affine, smooth: bool) {
    let Some(inv) = m.invert() else { return };
    let corners = [(0.0, 0.0), (src.w as f32, 0.0), (src.w as f32, src.h as f32), (0.0, src.h as f32)].map(|(x, y)| m.apply(x, y));
    let minx = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let maxx = corners.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let miny = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let maxy = corners.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let Some(r) = Rect::from_f(minx, miny, maxx, maxy).inflate(1).clip(dst.w, dst.h) else { return };
    let dw = dst.w;
    dst.px.par_chunks_mut(dw * 4).enumerate().skip(r.y as usize).take(r.h as usize).for_each(|(y, row)| {
        for x in r.x as usize..r.right() as usize {
            let (sx, sy) = inv.apply(x as f32 + 0.5, y as f32 + 0.5);
            let v = if smooth {
                if sx < -1.0 || sy < -1.0 || sx > src.w as f32 + 1.0 || sy > src.h as f32 + 1.0 {
                    continue;
                }
                src.sample(sx - 0.5, sy - 0.5)
            } else {
                if sx < 0.0 || sy < 0.0 || sx >= src.w as f32 || sy >= src.h as f32 {
                    continue;
                }
                let i = src.idx(sx as usize, sy as usize);
                [src.px[i] as f32, src.px[i + 1] as f32, src.px[i + 2] as f32, src.px[i + 3] as f32]
            };
            let sa = v[3] / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let d = x * 4;
            for k in 0..4 {
                row[d + k] = (v[k] + row[d + k] as f32 * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}
