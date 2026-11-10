//! Векторные слои: штрихи хранятся как линии (точки + параметры кисти) и перерисовываются без потерь.
//! Векторный ластик: касанием, всю линию или до пересечения с другими линиями.

use crate::brush::{self, Brush, Pt, Sink, Stamp, Stroke, Tip};
use crate::editor::{cover_clip, over16, tip_for};
use crate::raster::{Affine, Img};
use crate::util::Rect;
use rayon::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct VStroke {
    pub pts: Vec<Pt>,
    pub brush: Arc<Brush>,
    pub size: f32,
    pub color: [u8; 3],
    pub opacity: f32,
    pub seed: u32,
    pub gamma: f32,
}

pub type VList = Arc<Vec<VStroke>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EraseMode { Touch, Whole, Intersect }

impl EraseMode {
    pub fn id(self) -> &'static str {
        match self { EraseMode::Touch => "touch", EraseMode::Whole => "whole", EraseMode::Intersect => "intersect" }
    }
    pub fn from_id(s: &str) -> EraseMode {
        match s { "whole" => EraseMode::Whole, "intersect" => EraseMode::Intersect, _ => EraseMode::Touch }
    }
    pub fn name(self) -> &'static str {
        match self { EraseMode::Touch => "Касанием", EraseMode::Whole => "Всю линию", EraseMode::Intersect => "До пересечения" }
    }
}

impl VStroke {
    /// Запас вокруг точек, куда может попасть краска.
    pub fn margin(&self) -> f32 {
        let b = &self.brush;
        self.size * (0.8 + b.scatter + b.tilt_size) + 3.0
    }
    pub fn bbox(&self) -> Rect {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &self.pts {
            x0 = x0.min(p.x); y0 = y0.min(p.y); x1 = x1.max(p.x); y1 = y1.max(p.y);
        }
        if self.pts.is_empty() { return Rect::new(0, 0, 0, 0); }
        let m = self.margin();
        Rect::from_f(x0 - m, y0 - m, x1 + m, y1 + m)
    }
    pub fn len(&self) -> f32 {
        self.pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum()
    }
    fn piece(&self, pts: Vec<Pt>, cut_start: bool, cut_end: bool) -> VStroke {
        let mut b = (*self.brush).clone();
        if cut_start { b.taper_start = 0.0; }
        if cut_end { b.taper_end = 0.0; }
        VStroke { pts, brush: Arc::new(b), ..self.clone() }
    }
}

/* ---------- отрисовка ---------- */

struct RectSink<'a> {
    buf: &'a mut [u16],
    r: Rect,
    shape: String,
    hard: f32,
    round: f32,
    pixel: bool,
    tips: [Option<Arc<Tip>>; 7],
}

impl<'a> Sink for RectSink<'a> {
    fn stamp(&mut self, s: &Stamp) {
        let r = self.r;
        let rw = r.w as usize;
        let a = s.alpha.min(1.0);
        if self.pixel {
            let q = s.size.round().max(1.0) as i32;
            let x0 = (s.x - q as f32 / 2.0).round() as i32;
            let y0 = (s.y - q as f32 / 2.0).round() as i32;
            for y in y0.max(r.y)..(y0 + q).min(r.bottom()) {
                for x in x0.max(r.x)..(x0 + q).min(r.right()) {
                    over16(self.buf, (((y - r.y) as usize) * rw + (x - r.x) as usize) * 4, s.color, a);
                }
            }
            return;
        }
        let tip = tip_for(&mut self.tips, &self.shape, self.hard, s.size);
        let buf = &mut *self.buf;
        cover_clip(&tip, s, self.round, r, |x, y, c| {
            over16(buf, ((y - r.y as usize) * rw + (x - r.x as usize)) * 4, s.color, c * a);
        });
    }
}

/// Нарисовать один штрих в буфер прямоугольника r (u16, premultiplied).
fn stroke_to_buffer(s: &VStroke, r: Rect, buf: &mut [u16]) {
    let b = &s.brush;
    let mut st = Stroke::new((**b).clone(), s.size, s.color, vec![], s.gamma, 1.0);
    st.set_seed(s.seed);
    st.replace(s.pts.clone());
    let mut sink = RectSink { buf, r, shape: b.shape.clone(), hard: b.hardness, round: b.roundness, pixel: b.pixel, tips: Default::default() };
    st.rerender(&mut sink);
}

/// Перерисовать прямоугольник r слоя из векторных штрихов.
pub fn render(img: &mut Img, strokes: &[VStroke], r: Rect) {
    let Some(r) = r.clip(img.w, img.h) else { return };
    img.clear_rect(r);
    let w = img.w;
    // Штрихи рисуются в свои буферы параллельно (порциями, чтобы не занять много памяти),
    // затем накладываются строго по порядку.
    let todo: Vec<(&VStroke, Rect)> = strokes.iter().filter(|s| !s.pts.is_empty()).filter_map(|s| s.bbox().intersect(&r).map(|ir| (s, ir))).collect();
    let budget: i64 = 48 << 20;
    let mut start = 0;
    while start < todo.len() {
        let mut end = start;
        let mut acc: i64 = 0;
        while end < todo.len() && (end == start || acc + todo[end].1.w as i64 * todo[end].1.h as i64 * 8 <= budget) {
            acc += todo[end].1.w as i64 * todo[end].1.h as i64 * 8;
            end += 1;
        }
        let layers: Vec<(Rect, Vec<u16>, f32, Option<(Arc<brush::Grain>, f32, f32)>)> = todo[start..end]
            .par_iter()
            .map(|(s, ir)| {
                let mut buf = vec![0u16; (ir.w * ir.h * 4) as usize];
                stroke_to_buffer(s, *ir, &mut buf);
                let g = s.brush.grain.as_ref().filter(|_| s.brush.grain_strength > 0.0).map(|g| (brush::grain(g), s.brush.grain_scale, s.brush.grain_strength));
                (*ir, buf, s.opacity, g)
            })
            .collect();
        start = end;
        composite_layers(img, w, layers);
    }
}

fn composite_layers(img: &mut Img, w: usize, layers: Vec<(Rect, Vec<u16>, f32, Option<(Arc<brush::Grain>, f32, f32)>)>) {
    for (ir, buf, op, g) in layers {
        let rw = ir.w as usize;
        for y in 0..ir.h as usize {
            let gy = ir.y as usize + y;
            for x in 0..rw {
                let i = (y * rw + x) * 4;
                let ba = buf[i + 3] as f32 / 65535.0;
                if ba <= 0.0 { continue; }
                let gx = ir.x as usize + x;
                let mut k = op;
                if let Some((g, sc, st)) = &g { k *= g.at(gx as i32, gy as i32, *sc, *st); }
                let sa = ba * k;
                let o = (gy * w + gx) * 4;
                for c in 0..3 {
                    let sv = buf[i + c] as f32 / 65535.0 * k;
                    img.px[o + c] = (sv * 255.0 + img.px[o + c] as f32 * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
                }
                img.px[o + 3] = (sa * 255.0 + img.px[o + 3] as f32 * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Полная перерисовка слоя.
pub fn render_full(w: usize, h: usize, strokes: &[VStroke]) -> Img {
    let mut img = Img::new(w, h);
    render(&mut img, strokes, Rect::new(0, 0, w as i32, h as i32));
    img
}

/* ---------- геометрия ---------- */

fn seg_point_dist(px: f32, py: f32, a: Pt, b: Pt) -> (f32, f32) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((px - a.x) * dx + (py - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((a.x + dx * t - px).hypot(a.y + dy * t - py), t)
}

/// Расстояние от точки до отрезка c-d (ластик движется отрезком).
fn capsule_hit(p: Pt, c: (f32, f32), d: (f32, f32), r: f32) -> bool {
    let a = Pt { x: c.0, y: c.1, ..Default::default() };
    let b = Pt { x: d.0, y: d.1, ..Default::default() };
    seg_point_dist(p.x, p.y, a, b).0 <= r
}

fn lerp_pt(a: Pt, b: Pt, t: f32) -> Pt {
    Pt { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, p: a.p + (b.p - a.p) * t, tilt: b.tilt }
}

/// Уплотнить точки так, чтобы шаг был не больше step.
fn densify(pts: &[Pt], step: f32) -> Vec<Pt> {
    let mut out = Vec::with_capacity(pts.len() * 2);
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            let a = pts[i - 1];
            let d = (p.x - a.x).hypot(p.y - a.y);
            let n = (d / step).ceil() as usize;
            for k in 1..n { out.push(lerp_pt(a, *p, k as f32 / n as f32)); }
        }
        out.push(*p);
    }
    out
}

fn seg_bbox_hit(s: &VStroke, c: (f32, f32), d: (f32, f32), r: f32) -> bool {
    let bb = Rect::from_f(c.0.min(d.0) - r, c.1.min(d.1) - r, c.0.max(d.0) + r, c.1.max(d.1) + r);
    let m = s.margin() as i32;
    s.bbox().inflate(-m + 1).overlaps(&bb)
}

/// Задевает ли ластик (отрезок c-d радиуса r) штрих. Возвращает (индекс сегмента, t).
fn touch_point(s: &VStroke, c: (f32, f32), d: (f32, f32), r: f32) -> Option<(usize, f32)> {
    if !seg_bbox_hit(s, c, d, r) { return None; }
    let n = s.pts.len();
    if n == 1 {
        return if capsule_hit(s.pts[0], c, d, r) { Some((0, 0.0)) } else { None };
    }
    let steps = (((d.0 - c.0).hypot(d.1 - c.1) / (r * 0.5).max(0.5)).ceil() as usize).clamp(1, 64);
    let mut best: Option<(f32, usize, f32)> = None;
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let (px, py) = (c.0 + (d.0 - c.0) * t, c.1 + (d.1 - c.1) * t);
        for i in 0..n - 1 {
            let (dist, tt) = seg_point_dist(px, py, s.pts[i], s.pts[i + 1]);
            if dist <= r && best.map_or(true, |b| dist < b.0) { best = Some((dist, i, tt)); }
        }
    }
    best.map(|b| (b.1, b.2))
}

/// Пересечение отрезков; параметр на первом.
fn seg_x(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f32> {
    let r = (b.x - a.x, b.y - a.y);
    let s = (d.x - c.x, d.y - c.y);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-9 { return None; }
    let q = (c.x - a.x, c.y - a.y);
    let t = (q.0 * s.1 - q.1 * s.0) / den;
    let u = (q.0 * r.1 - q.1 * r.0) / den;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) { Some(t) } else { None }
}

/// Все пересечения штриха `si` с остальными (и с самим собой) — в виде «положений» i + t.
fn intersections(list: &[VStroke], si: usize) -> Vec<f32> {
    let s = &list[si];
    let n = s.pts.len();
    let mut out = vec![];
    if n < 2 { return out; }
    let core = |v: &VStroke| { let m = v.margin() as i32; v.bbox().inflate(-m + 1) };
    let sb = core(s);
    for (oi, o) in list.iter().enumerate() {
        if o.pts.len() < 2 { continue; }
        if oi != si && !core(o).inflate(1).overlaps(&sb.inflate(1)) { continue; }
        for j in 0..o.pts.len() - 1 {
            let (c, d) = (o.pts[j], o.pts[j + 1]);
            let jb = Rect::from_f(c.x.min(d.x) - 1.0, c.y.min(d.y) - 1.0, c.x.max(d.x) + 1.0, c.y.max(d.y) + 1.0);
            if !jb.overlaps(&sb.inflate(1)) { continue; }
            for i in 0..n - 1 {
                if oi == si && (i as i64 - j as i64).abs() <= 1 { continue; }
                if let Some(t) = seg_x(s.pts[i], s.pts[i + 1], c, d) { out.push(i as f32 + t); }
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn at_pos(pts: &[Pt], pos: f32) -> Pt {
    let i = (pos.floor() as usize).min(pts.len().saturating_sub(2));
    lerp_pt(pts[i], pts[(i + 1).min(pts.len() - 1)], pos - i as f32)
}

/// Часть штриха между положениями a..b.
fn sub_pts(pts: &[Pt], a: f32, b: f32) -> Vec<Pt> {
    let mut out = vec![at_pos(pts, a)];
    let i0 = a.floor() as usize + 1;
    let i1 = b.ceil() as usize;
    for i in i0..i1.min(pts.len()) {
        if (i as f32) > a && (i as f32) < b { out.push(pts[i]); }
    }
    out.push(at_pos(pts, b));
    out
}

/// Стереть векторным ластиком: отрезок c-d (координаты холста), радиус r.
/// Возвращает прямоугольник, который нужно перерисовать.
pub fn erase(list: &mut Vec<VStroke>, c: (f32, f32), d: (f32, f32), r: f32, mode: EraseMode) -> Option<Rect> {
    let mut dirty: Option<Rect> = None;
    let mut out: Vec<VStroke> = Vec::with_capacity(list.len());
    let mut i = 0;
    while i < list.len() {
        let s = &list[i];
        match mode {
            EraseMode::Whole => {
                if touch_point(s, c, d, r).is_some() {
                    dirty = crate::util::union_opt(dirty, s.bbox());
                } else {
                    out.push(s.clone());
                }
            }
            EraseMode::Touch => {
                if !seg_bbox_hit(s, c, d, r) || touch_point(s, c, d, r).is_none() { out.push(s.clone()); i += 1; continue; }
                dirty = crate::util::union_opt(dirty, s.bbox());
                let dense = densify(&s.pts, (r / 3.0).clamp(0.5, 4.0));
                let mut cur: Vec<Pt> = vec![];
                let mut cut_before = false;
                for p in dense.iter() {
                    if capsule_hit(*p, c, d, r) {
                        if cur.len() >= 2 {
                            out.push(s.piece(std::mem::take(&mut cur), cut_before, true));
                        }
                        cur.clear();
                        cut_before = true;
                    } else {
                        cur.push(*p);
                    }
                }
                if cur.len() >= 2 { out.push(s.piece(cur, cut_before, false)); }
            }
            EraseMode::Intersect => {
                let Some((seg, t)) = touch_point(s, c, d, r) else { out.push(s.clone()); i += 1; continue; };
                dirty = crate::util::union_opt(dirty, s.bbox());
                let pos = seg as f32 + t;
                let xs = intersections(list, i);
                let last = (s.pts.len() - 1) as f32;
                let a = xs.iter().copied().filter(|&x| x < pos - 1e-4).fold(None, |m: Option<f32>, x| Some(m.map_or(x, |m| m.max(x))));
                let b = xs.iter().copied().filter(|&x| x > pos + 1e-4).fold(None, |m: Option<f32>, x| Some(m.map_or(x, |m| m.min(x))));
                if s.pts.len() >= 2 {
                    if let Some(a) = a {
                        if a > 1e-3 { out.push(s.piece(sub_pts(&s.pts, 0.0, a), false, true)); }
                    }
                    if let Some(b) = b {
                        if b < last - 1e-3 { out.push(s.piece(sub_pts(&s.pts, b, last), true, false)); }
                    }
                }
            }
        }
        i += 1;
    }
    if dirty.is_some() { *list = out; }
    dirty
}

/// Применить аффинное преобразование ко всем штрихам (толщина масштабируется).
pub fn transform(list: &[VStroke], m: Affine) -> Vec<VStroke> {
    let k = (m.a * m.d - m.b * m.c).abs().sqrt().max(0.01);
    list.iter().map(|s| {
        let pts = s.pts.iter().map(|p| { let (x, y) = m.apply(p.x, p.y); Pt { x, y, ..*p } }).collect();
        VStroke { pts, size: s.size * k, ..s.clone() }
    }).collect()
}

/// Какие штрихи задевает выделение (маска w×h).
pub fn touched_by_mask(list: &[VStroke], mask: &[u8], w: usize, h: usize) -> Vec<bool> {
    list.iter().map(|s| s.pts.iter().any(|p| {
        let (x, y) = (p.x as i32, p.y as i32);
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && mask[y as usize * w + x as usize] > 127
    })).collect()
}

/* ---------- сохранение ---------- */

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct VFile {
    version: u32,
    brushes: Vec<Brush>,
    strokes: Vec<VFileStroke>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct VFileStroke {
    b: usize,
    s: f32,
    c: String,
    o: f32,
    seed: u32,
    g: f32,
    p: Vec<f32>,
}

pub fn to_json(list: &[VStroke]) -> Vec<u8> {
    let mut brushes: Vec<Brush> = vec![];
    let mut ptrs: Vec<*const Brush> = vec![];
    let mut strokes = vec![];
    for s in list {
        let ptr = Arc::as_ptr(&s.brush);
        let bi = match ptrs.iter().position(|p| *p == ptr) {
            Some(i) => i,
            None => match brushes.iter().position(|b| *b == *s.brush) {
                Some(i) => i,
                None => { brushes.push((*s.brush).clone()); ptrs.push(ptr); brushes.len() - 1 }
            },
        };
        let mut p = Vec::with_capacity(s.pts.len() * 3);
        for q in &s.pts { p.push((q.x * 100.0).round() / 100.0); p.push((q.y * 100.0).round() / 100.0); p.push((q.p * 1000.0).round() / 1000.0); }
        strokes.push(VFileStroke { b: bi, s: s.size, c: crate::util::hex(s.color), o: s.opacity, seed: s.seed, g: s.gamma, p });
    }
    serde_json::to_vec(&VFile { version: 1, brushes, strokes }).unwrap_or_default()
}

pub fn from_json(bytes: &[u8]) -> Option<Vec<VStroke>> {
    let f: VFile = serde_json::from_slice(bytes).ok()?;
    let brushes: Vec<Arc<Brush>> = f.brushes.into_iter().map(Arc::new).collect();
    let def = Arc::new(Brush::default());
    Some(f.strokes.into_iter().map(|s| VStroke {
        pts: s.p.chunks_exact(3).map(|c| Pt { x: c[0], y: c[1], p: c[2], tilt: 0.0 }).collect(),
        brush: brushes.get(s.b).cloned().unwrap_or_else(|| def.clone()),
        size: s.s,
        color: crate::util::parse_hex(&s.c).unwrap_or([0, 0, 0]),
        opacity: s.o,
        seed: s.seed,
        gamma: if s.g > 0.0 { s.g } else { 1.0 },
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> VStroke {
        let n = 40;
        let pts = (0..=n).map(|i| { let t = i as f32 / n as f32; Pt { x: x0 + (x1 - x0) * t, y: y0 + (y1 - y0) * t, p: 1.0, tilt: 0.0 } }).collect();
        VStroke { pts, brush: Arc::new(Brush { taper_start: 0.0, taper_end: 0.0, ..Brush::default() }), size: 4.0, color: [0, 0, 0], opacity: 1.0, seed: 1, gamma: 1.0 }
    }

    #[test]
    fn erase_to_intersection() {
        // горизонтальная линия и две вертикальные, пересекающие её в x=40 и x=60
        let mut list = vec![line(0.0, 50.0, 100.0, 50.0), line(40.0, 0.0, 40.0, 100.0), line(60.0, 0.0, 60.0, 100.0)];
        let d = erase(&mut list, (50.0, 50.0), (50.0, 50.0), 3.0, EraseMode::Intersect);
        assert!(d.is_some());
        assert_eq!(list.len(), 4, "горизонталь распалась на две части");
        let parts: Vec<&VStroke> = list.iter().filter(|s| (s.pts[0].y - 50.0).abs() < 0.01 && (s.pts.last().unwrap().y - 50.0).abs() < 0.01).collect();
        assert_eq!(parts.len(), 2);
        let ends: Vec<f32> = parts.iter().map(|s| s.pts.last().unwrap().x.max(s.pts[0].x)).collect();
        assert!(ends.iter().any(|x| (x - 40.0).abs() < 0.05), "левая часть заканчивается на пересечении: {ends:?}");
        let starts: Vec<f32> = parts.iter().map(|s| s.pts[0].x.min(s.pts.last().unwrap().x)).collect();
        assert!(starts.iter().any(|x| (x - 60.0).abs() < 0.05), "правая часть начинается на пересечении: {starts:?}");
    }

    #[test]
    fn erase_touch_and_whole() {
        let mut list = vec![line(0.0, 50.0, 100.0, 50.0)];
        erase(&mut list, (50.0, 45.0), (50.0, 55.0), 5.0, EraseMode::Touch);
        assert_eq!(list.len(), 2);
        let mut l2 = vec![line(0.0, 50.0, 100.0, 50.0), line(0.0, 10.0, 100.0, 10.0)];
        erase(&mut l2, (30.0, 50.0), (30.0, 50.0), 4.0, EraseMode::Whole);
        assert_eq!(l2.len(), 1);
    }

    #[test]
    fn json_roundtrip_and_render() {
        let list = vec![line(10.0, 10.0, 90.0, 90.0), line(10.0, 90.0, 90.0, 10.0)];
        let back = from_json(&to_json(&list)).unwrap();
        assert_eq!(back.len(), 2);
        let a = render_full(100, 100, &list);
        let b = render_full(100, 100, &back);
        let diff = a.px.iter().zip(b.px.iter()).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap();
        assert!(diff <= 2, "отрисовка после сохранения совпадает ({diff})");
        assert!(a.px.chunks_exact(4).any(|p| p[3] > 200));
    }
}
