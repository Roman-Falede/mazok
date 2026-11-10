//! Редактор: вид холста, ввод, инструменты, операции со слоями и выделением.

use crate::blend::{self, Blend};
use crate::brush::{self, Brush, Grain, Mirror, Sink, Stamp, Stroke, Tip};
use crate::doc::{self, Document, History, Layer, Op, TextSpec};
use crate::filters;
use crate::raster::{self, Affine, Img};
use crate::state::{Shared, ToolState};
use crate::util::{self, Rect};
use crate::vector::{self, EraseMode, VStroke};
use egui::{Color32, Pos2, Vec2};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tool { Brush, Eraser, Smudge, Clone, Fill, Gradient, Picker, Select, Transform, Text, Shapes, Hand, Zoom }

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Brush => "Кисть", Tool::Eraser => "Ластик", Tool::Smudge => "Палец", Tool::Clone => "Штамп", Tool::Fill => "Заливка",
            Tool::Gradient => "Градиент", Tool::Picker => "Пипетка", Tool::Select => "Выделение", Tool::Transform => "Трансформация",
            Tool::Text => "Текст", Tool::Shapes => "Фигуры", Tool::Hand => "Рука", Tool::Zoom => "Масштаб",
        }
    }
    pub fn is_paint(self) -> bool {
        matches!(self, Tool::Brush | Tool::Eraser | Tool::Smudge | Tool::Clone)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelShape { Rect, Ellipse, Lasso, Wand }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelOp { Replace, Add, Subtract, Intersect }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GradKind { Linear, Radial, Reflected, Angle }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind { Rect, Ellipse, Line, Polygon, Star }
/// Куда перетащить слой.
#[derive(Clone, Debug, PartialEq)]
pub enum Drop { Above(String), Below(String), Into(String) }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PaintMode { Paint, Erase, Smudge, Clone }

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub s: f32,
    pub r: f32,
    pub flip: f32,
    pub ox: f32,
    pub oy: f32,
}

impl Default for View {
    fn default() -> Self {
        View { s: 1.0, r: 0.0, flip: 1.0, ox: 0.0, oy: 0.0 }
    }
}

impl View {
    fn m(&self) -> [f32; 6] {
        let (c, sn) = (self.r.cos() * self.s, self.r.sin() * self.s);
        [c * self.flip, sn * self.flip, -sn, c, self.ox, self.oy]
    }
    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        let m = self.m();
        Pos2::new(m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
    pub fn to_doc(&self, p: Pos2) -> (f32, f32) {
        let m = self.m();
        let det = m[0] * m[3] - m[1] * m[2];
        let (dx, dy) = (p.x - m[4], p.y - m[5]);
        ((m[3] * dx - m[2] * dy) / det, (-m[1] * dx + m[0] * dy) / det)
    }
    pub fn zoom_at(&mut self, p: Pos2, k: f32) {
        let (x, y) = self.to_doc(p);
        self.s = (self.s * k).clamp(0.02, 64.0);
        let q = self.to_screen(x, y);
        self.ox += p.x - q.x;
        self.oy += p.y - q.y;
    }
    pub fn rotate_at(&mut self, p: Pos2, da: f32) {
        let (x, y) = self.to_doc(p);
        self.r += da;
        let q = self.to_screen(x, y);
        self.ox += p.x - q.x;
        self.oy += p.y - q.y;
    }
    pub fn fit(&mut self, w: usize, h: usize, area: egui::Rect) {
        self.s = (area.width() / w as f32).min(area.height() / h as f32) * 0.92;
        self.r = 0.0;
        self.flip = 1.0;
        self.ox = area.center().x - self.s * w as f32 / 2.0;
        self.oy = area.center().y - self.s * h as f32 / 2.0;
    }
}

pub struct Work {
    pub layer: usize,
    pub img: Img,
    /// Рабочая копия — маска слоя, а не его пиксели.
    pub mask: bool,
}

struct Pick {
    d: usize,
    a: Vec<f32>,
}

enum Active {
    Paint {
        mode: PaintMode,
        stroke: Stroke,
        total: Option<Rect>,
        opacity: f32,
        grain: Option<(Arc<Grain>, f32, f32)>,
        blend: Blend,
        start: Pos2,
        start_t: f64,
        started: bool,
        shape: Option<&'static str>,
        qs_anchor: Pos2,
        qs_t: f64,
        picks: Vec<Option<Pick>>,
        clone_src: Option<Arc<Img>>,
        clone_off: (f32, f32),
    },
    Pick,
    Fill { src: Img, x: usize, y: usize, sx0: f32, base: f32, tol: f32, color: [u8; 3], prev: Option<Rect>, total: Option<Rect>, pending: bool },
    Gradient { x0: f32, y0: f32, x1: f32, y1: f32, moved: bool, start: Pos2, pending: bool },
    Shape { x0: f32, y0: f32, x1: f32, y1: f32, moved: bool, start: Pos2, center: bool, pending: bool },
    Select { x0: f32, y0: f32, x1: f32, y1: f32, op: SelOp, pts: Vec<(f32, f32)>, start: Pos2, square: bool },
    Wand { src: Img, x: usize, y: usize, sx0: f32, base: f32, tol: f32, op: SelOp, preview: Option<(Vec<u8>, Rect)> },
    XDrag { h: Handle, p0: (f32, f32), snap: XState, anchor: (f32, f32), a0: f32 },
    VErase { list: Vec<VStroke>, before: Option<vector::VList>, last: (f32, f32), r: f32, total: Option<Rect>, mode: EraseMode, changed: bool },
    Pan { last: Pos2 },
    Zoom { start: Pos2, last_x: f32, moved: bool, out: bool },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Handle { Move, Rotate, Corner(usize), Edge(usize) }

#[derive(Clone, Copy, Debug)]
pub struct XState {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub rot: f32,
}

pub struct Xform {
    pub layer: usize,
    on_mask: bool,
    float: Img,
    hole: Img,
    orig: Arc<Img>,
    mask: Option<Vec<u8>>,
    pub st: XState,
    init: XState,
    pub changed: bool,
    pub smooth: bool,
    pub uniform: bool,
}

pub struct FilterSession {
    pub id: &'static str,
    pub mask: bool,
    pub params: Vec<f32>,
    pub layer: usize,
    src: Arc<Img>,
    pub dirty: bool,
}

/// Запросы к интерфейсу (диалоги), которые редактор не может показать сам.
pub enum Request {
    Text { spec: TextSpec, layer: Option<usize> },
}

pub struct Editor {
    pub doc: Document,
    pub comp: Img,
    comp_dirty: Option<Rect>,
    tex: Option<egui::TextureHandle>,
    tex_dirty: Option<Rect>,
    tex_nearest: bool,
    checker: Option<egui::TextureHandle>,
    pub hist: History,
    pub sel: Option<Vec<u8>>,
    sel_tex: Option<egui::TextureHandle>,
    sel_dirty: bool,
    pub sel_outline: Option<Vec<(f32, f32)>>,
    pub view: View,
    fitted: bool,
    pub canvas_rect: egui::Rect,
    pub tool: Tool,
    pub prev_tool: Tool,
    pub sel_shape: SelShape,
    pub sel_op: SelOp,
    pub wand_tol: f32,
    pub grad_kind: GradKind,
    pub grad_colors: usize,
    pub grad_reverse: bool,
    pub grad_opacity: f32,
    pub shape_kind: ShapeKind,
    pub shape_fill: bool,
    pub shape_stroke: bool,
    pub shape_width: f32,
    pub shape_sides: usize,
    pub work: Option<Work>,
    buffer: Vec<u16>,
    active: Option<Active>,
    pub xform: Option<Xform>,
    pub filter: Option<FilterSession>,
    pub clone_src: Option<(f32, f32)>,
    clone_off: Option<(f32, f32)>,
    pub changed: bool,
    pub change_time: f64,
    pub toast: Option<(String, f64)>,
    pub request: Option<Request>,
    pub clipboard: Option<(Img, i32, i32)>,
    pub hover: Option<Pos2>,
    pub loupe: Option<(Pos2, [u8; 3])>,
    touches: HashMap<u64, Pos2>,
    draw_touch: Option<u64>,
    gesture: Option<(f64, f32, bool, usize)>,
    mouse_down: bool,
    tool_t0: f64,
    pub now: f64,
    pub space: bool,
    pub ui_hidden: bool,
    fill_expand: usize,
    /// Рисуем по маске активного слоя.
    pub mask_edit: bool,
    /// Дополнительно выбранные слои (id), кроме активного.
    pub multi: Vec<String>,
}

/* ---------- приёмники отпечатков ---------- */

const LEVELS: [usize; 7] = [8, 16, 32, 64, 128, 256, 512];

pub(crate) fn tip_for(cache: &mut [Option<Arc<Tip>>; 7], shape: &str, hard: f32, size: f32) -> Arc<Tip> {
    let lvl = brush::level_for(size);
    let li = LEVELS.iter().position(|&l| l == lvl).unwrap_or(6);
    cache[li].get_or_insert_with(|| brush::tip(shape, hard, lvl)).clone()
}

/// Обход пикселей отпечатка: f(x, y, покрытие).
pub(crate) fn cover(tip: &Tip, s: &Stamp, round: f32, w: usize, h: usize, f: impl FnMut(usize, usize, f32)) {
    cover_clip(tip, s, round, Rect::new(0, 0, w as i32, h as i32), f)
}

/// То же, но с отсечением по прямоугольнику clip (координаты холста).
pub(crate) fn cover_clip(tip: &Tip, s: &Stamp, round: f32, clip: Rect, mut f: impl FnMut(usize, usize, f32)) {
    let rad = s.size * 0.75 + 1.5;
    let Some(r) = Rect::around(s.x, s.y, rad).intersect(&clip) else { return };
    if r.x < 0 || r.y < 0 { return; }
    let (cs, sn) = (s.angle.cos(), s.angle.sin());
    let scale = tip.size as f32 / s.size.max(0.01);
    let half = tip.size as f32 / 2.0;
    let rd = round.max(0.02);
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let dx = x as f32 + 0.5 - s.x;
            let dy = y as f32 + 0.5 - s.y;
            let u = dx * cs + dy * sn;
            let v = (-dx * sn + dy * cs) / rd;
            let tu = u * scale + half - 0.5;
            let tv = v * scale + half - 0.5;
            if tu < -1.0 || tv < -1.0 || tu > tip.size as f32 || tv > tip.size as f32 {
                continue;
            }
            let c = tip.sample(tu, tv);
            if c > 0.002 {
                f(x as usize, y as usize, c);
            }
        }
    }
}

#[inline]
pub(crate) fn over16(buf: &mut [u16], i: usize, col: [f32; 3], a: f32) {
    let inv = 1.0 - a;
    let k = a * 65535.0;
    buf[i] = (buf[i] as f32 * inv + col[0] * k) as u16;
    buf[i + 1] = (buf[i + 1] as f32 * inv + col[1] * k) as u16;
    buf[i + 2] = (buf[i + 2] as f32 * inv + col[2] * k) as u16;
    buf[i + 3] = (buf[i + 3] as f32 * inv + k) as u16;
}

struct PaintSink<'a> {
    buf: &'a mut [u16],
    w: usize,
    h: usize,
    shape: String,
    hard: f32,
    round: f32,
    pixel: bool,
    tips: [Option<Arc<Tip>>; 7],
}

impl<'a> Sink for PaintSink<'a> {
    fn stamp(&mut self, s: &Stamp) {
        let (w, h) = (self.w, self.h);
        if self.pixel {
            let q = s.size.round().max(1.0) as i32;
            let x0 = (s.x - q as f32 / 2.0).round() as i32;
            let y0 = (s.y - q as f32 / 2.0).round() as i32;
            let a = s.alpha.min(1.0);
            for y in y0.max(0)..(y0 + q).min(h as i32) {
                for x in x0.max(0)..(x0 + q).min(w as i32) {
                    over16(self.buf, (y as usize * w + x as usize) * 4, s.color, a);
                }
            }
            return;
        }
        let tip = tip_for(&mut self.tips, &self.shape, self.hard, s.size);
        let a = s.alpha.min(1.0);
        let buf = &mut *self.buf;
        cover(&tip, s, self.round, w, h, |x, y, c| over16(buf, (y * w + x) * 4, s.color, c * a));
    }
}

struct CloneSink<'a> {
    buf: &'a mut [u16],
    w: usize,
    h: usize,
    src: &'a Img,
    off: (f32, f32),
    shape: String,
    hard: f32,
    round: f32,
    tips: [Option<Arc<Tip>>; 7],
}

impl<'a> Sink for CloneSink<'a> {
    fn stamp(&mut self, s: &Stamp) {
        let tip = tip_for(&mut self.tips, &self.shape, self.hard, s.size);
        let (w, h, src, off) = (self.w, self.h, self.src, self.off);
        let a = s.alpha.min(1.0);
        let buf = &mut *self.buf;
        cover(&tip, s, self.round, w, h, |x, y, c| {
            let sx = x as i32 + off.0 as i32;
            let sy = y as i32 + off.1 as i32;
            if sx < 0 || sy < 0 || sx >= src.w as i32 || sy >= src.h as i32 {
                return;
            }
            let si = src.idx(sx as usize, sy as usize);
            let sa = src.px[si + 3] as f32 / 255.0;
            if sa <= 0.0 {
                return;
            }
            let col = [src.px[si] as f32 / 255.0 / sa, src.px[si + 1] as f32 / 255.0 / sa, src.px[si + 2] as f32 / 255.0 / sa];
            over16(buf, (y * w + x) * 4, col, c * a * sa);
        });
    }
}

struct SmudgeSink<'a> {
    work: &'a mut Img,
    sel: Option<&'a [u8]>,
    alpha_lock: bool,
    picks: &'a mut Vec<Option<Pick>>,
    strength: f32,
    flow: f32,
    rate: f32,
    shape: String,
    hard: f32,
    round: f32,
    tips: [Option<Arc<Tip>>; 7],
}

impl<'a> Sink for SmudgeSink<'a> {
    fn stamp(&mut self, s: &Stamp) {
        let d = (s.size.ceil() as usize).max(2);
        let x0 = (s.x - d as f32 / 2.0).round() as i32;
        let y0 = (s.y - d as f32 / 2.0).round() as i32;
        let (w, h) = (self.work.w as i32, self.work.h as i32);
        while self.picks.len() <= s.mirror {
            self.picks.push(None);
        }
        let read = |img: &Img, i: i32, j: i32| -> [f32; 4] {
            let (x, y) = (x0 + i, y0 + j);
            if x < 0 || y < 0 || x >= w || y >= h {
                return [0.0; 4];
            }
            let k = img.idx(x as usize, y as usize);
            [img.px[k] as f32, img.px[k + 1] as f32, img.px[k + 2] as f32, img.px[k + 3] as f32]
        };
        let pk = &mut self.picks[s.mirror];
        if pk.is_none() {
            let mut a = vec![0.0; d * d * 4];
            for j in 0..d {
                for i in 0..d {
                    let v = read(self.work, i as i32, j as i32);
                    a[(j * d + i) * 4..(j * d + i) * 4 + 4].copy_from_slice(&v);
                }
            }
            *pk = Some(Pick { d, a });
            return;
        }
        let p = pk.as_mut().unwrap();
        if p.d != d {
            let mut a = vec![0.0; d * d * 4];
            for j in 0..d {
                for i in 0..d {
                    let si = ((j * p.d / d) * p.d + i * p.d / d) * 4;
                    a[(j * d + i) * 4..(j * d + i) * 4 + 4].copy_from_slice(&p.a[si..si + 4]);
                }
            }
            p.d = d;
            p.a = a;
        }
        let k = (self.strength * (s.alpha / self.flow.max(0.01))).clamp(0.0, 1.0);
        let tip = tip_for(&mut self.tips, &self.shape, self.hard, s.size);
        let local = Stamp { x: d as f32 / 2.0, y: d as f32 / 2.0, ..*s };
        let mut mask = vec![0f32; d * d];
        cover(&tip, &local, self.round, d, d, |i, j, c| mask[j * d + i] = c);
        for j in 0..d {
            for i in 0..d {
                let (x, y) = (x0 + i as i32, y0 + j as i32);
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let mut m = mask[j * d + i] * k;
                if let Some(sel) = self.sel {
                    m *= sel[y as usize * w as usize + x as usize] as f32 / 255.0;
                }
                if m <= 0.001 {
                    continue;
                }
                let wi = self.work.idx(x as usize, y as usize);
                let pi = (j * d + i) * 4;
                let wa = self.work.px[wi + 3] as f32;
                for c in 0..4 {
                    let wv = self.work.px[wi + c] as f32;
                    let nv = wv + (p.a[pi + c] - wv) * m;
                    self.work.px[wi + c] = nv.round().clamp(0.0, 255.0) as u8;
                }
                if self.alpha_lock {
                    let na = self.work.px[wi + 3] as f32;
                    if na > 0.0 {
                        for c in 0..3 {
                            self.work.px[wi + c] = (self.work.px[wi + c] as f32 * wa / na).min(wa) as u8;
                        }
                    }
                    self.work.px[wi + 3] = wa as u8;
                }
            }
        }
        let rate = self.rate;
        for j in 0..d {
            for i in 0..d {
                let v = read(self.work, i as i32, j as i32);
                let pi = (j * d + i) * 4;
                for c in 0..4 {
                    p.a[pi + c] += (v[c] - p.a[pi + c]) * rate;
                }
            }
        }
    }
}

/* ---------- редактор ---------- */

impl Editor {
    pub fn new(doc: Document) -> Editor {
        let comp = Img::new(doc.w, doc.h);
        let mut e = Editor {
            comp,
            comp_dirty: None,
            tex: None,
            tex_dirty: None,
            tex_nearest: false,
            checker: None,
            hist: History::default(),
            sel: None,
            sel_tex: None,
            sel_dirty: false,
            sel_outline: None,
            view: View::default(),
            fitted: false,
            canvas_rect: egui::Rect::NOTHING,
            tool: Tool::Brush,
            prev_tool: Tool::Brush,
            sel_shape: SelShape::Rect,
            sel_op: SelOp::Replace,
            wand_tol: 20.0,
            grad_kind: GradKind::Linear,
            grad_colors: 0,
            grad_reverse: false,
            grad_opacity: 1.0,
            shape_kind: ShapeKind::Rect,
            shape_fill: true,
            shape_stroke: false,
            shape_width: 8.0,
            shape_sides: 6,
            work: None,
            buffer: vec![],
            active: None,
            xform: None,
            filter: None,
            clone_src: None,
            clone_off: None,
            changed: false,
            change_time: 0.0,
            toast: None,
            request: None,
            clipboard: None,
            hover: None,
            loupe: None,
            touches: HashMap::new(),
            draw_touch: None,
            gesture: None,
            mouse_down: false,
            tool_t0: 0.0,
            now: 0.0,
            space: false,
            ui_hidden: false,
            fill_expand: 1,
            mask_edit: false,
            multi: vec![],
            doc,
        };
        e.invalidate_all();
        e
    }

    pub fn toast(&mut self, s: &str) {
        self.toast = Some((s.to_string(), self.now));
    }
    pub fn invalidate(&mut self, r: Rect) {
        self.comp_dirty = util::union_opt(self.comp_dirty, r);
    }
    pub fn invalidate_all(&mut self) {
        self.comp_dirty = Some(self.doc.full());
    }
    pub fn mark_changed(&mut self) {
        self.changed = true;
        self.change_time = self.now;
        self.doc.modified = util::now_ms();
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn fit(&mut self) {
        if self.canvas_rect.is_positive() {
            self.view.fit(self.doc.w, self.doc.h, self.canvas_rect.shrink(12.0));
        } else {
            self.fitted = false;
        }
    }

    /* ---------- композиция и текстуры ---------- */

    fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.comp_dirty.take() {
            let ov = self.work.as_ref().map(|w| doc::Ov { layer: w.layer, img: &w.img, mask: w.mask });
            doc::render(&self.doc, &mut self.comp, r, ov);
            self.tex_dirty = util::union_opt(self.tex_dirty, r);
        }
        let nearest = self.view.s >= 2.0;
        let opts = if nearest { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)) };
        let size = [self.doc.w, self.doc.h];
        let need_full = self.tex.as_ref().map_or(true, |t| t.size() != size) || nearest != self.tex_nearest;
        if need_full {
            let img = egui::ColorImage::from_rgba_premultiplied(size, &self.comp.px);
            match &mut self.tex {
                Some(t) if t.size() == size => t.set(img, opts),
                _ => self.tex = Some(ctx.load_texture("mazok-canvas", img, opts)),
            }
            self.tex_nearest = nearest;
            self.tex_dirty = None;
        } else if let Some(r) = self.tex_dirty.take() {
            if let Some(r) = r.clip(self.doc.w, self.doc.h) {
                let part = self.comp.crop(r);
                let img = egui::ColorImage::from_rgba_premultiplied([r.w as usize, r.h as usize], &part.px);
                if let Some(t) = &mut self.tex {
                    t.set_partial([r.x as usize, r.y as usize], img, opts);
                }
            }
        }
        if self.sel_dirty {
            self.sel_dirty = false;
            if let Some(m) = &self.sel {
                let mut px = vec![0u8; m.len() * 4];
                for (i, &v) in m.iter().enumerate() {
                    let a = ((255 - v) as f32 * 0.45) as u8;
                    px[i * 4] = (10.0 * a as f32 / 255.0) as u8;
                    px[i * 4 + 1] = (10.0 * a as f32 / 255.0) as u8;
                    px[i * 4 + 2] = (22.0 * a as f32 / 255.0) as u8;
                    px[i * 4 + 3] = a;
                }
                let img = egui::ColorImage::from_rgba_premultiplied([self.doc.w, self.doc.h], &px);
                self.sel_tex = Some(ctx.load_texture("mazok-sel", img, egui::TextureOptions::LINEAR));
            } else {
                self.sel_tex = None;
            }
        }
    }

    /* ---------- рисование холста ---------- */

    pub fn canvas_ui(&mut self, ui: &mut egui::Ui, sh: &mut Shared, stage: Color32) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);
        let (rect, _resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        self.canvas_rect = rect;
        if !self.fitted && rect.is_positive() {
            self.view.fit(self.doc.w, self.doc.h, rect.shrink(12.0));
            self.fitted = true;
        }
        self.handle_input(ui, rect, sh);
        self.tick(sh);
        self.flush_active(sh);
        self.refresh(&ctx);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, stage);
        let d = &self.doc;
        let corners = [self.view.to_screen(0.0, 0.0), self.view.to_screen(d.w as f32, 0.0), self.view.to_screen(d.w as f32, d.h as f32), self.view.to_screen(0.0, d.h as f32)];
        // тень
        let shadow: Vec<Pos2> = corners.iter().map(|p| *p + Vec2::new(3.0, 4.0)).collect();
        painter.add(egui::Shape::convex_polygon(shadow, Color32::from_black_alpha(90), egui::Stroke::NONE));
        if !d.bg_visible {
            let checker = self.checker.get_or_insert_with(|| {
                let mut px = vec![0u8; 16 * 16 * 4];
                for y in 0..16 {
                    for x in 0..16 {
                        let v = if (x < 8) ^ (y < 8) { 214 } else { 255 };
                        px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4].copy_from_slice(&[v, v, v, 255]);
                    }
                }
                ctx.load_texture("mazok-checker", egui::ColorImage::from_rgba_premultiplied([16, 16], &px), egui::TextureOptions::NEAREST_REPEAT)
            });
            let uvw = d.w as f32 * self.view.s / 16.0;
            let uvh = d.h as f32 * self.view.s / 16.0;
            quad(&painter, checker.id(), corners, [Pos2::ZERO, Pos2::new(uvw, 0.0), Pos2::new(uvw, uvh), Pos2::new(0.0, uvh)]);
        }
        if let Some(t) = &self.tex {
            quad(&painter, t.id(), corners, [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)]);
        }
        if let Some(t) = &self.sel_tex {
            if self.xform.is_none() {
                quad(&painter, t.id(), corners, [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)]);
            }
        }
        self.draw_guides(&painter);
        self.draw_overlay(&painter, sh);
        if self.active.is_some() || self.xform.is_some() || self.filter.as_ref().map_or(false, |f| f.dirty) {
            ctx.request_repaint();
        }
    }

    fn draw_guides(&self, p: &egui::Painter) {
        let g = &self.doc.guide;
        let col = Color32::from_rgba_unmultiplied(58, 160, 255, 150);
        let st = egui::Stroke::new(1.0, col);
        let (w, h) = (self.doc.w as f32, self.doc.h as f32);
        if g.grid && g.grid_size * self.view.s >= 4.0 {
            let mut x = 0.0;
            while x <= w {
                p.line_segment([self.view.to_screen(x, 0.0), self.view.to_screen(x, h)], st);
                x += g.grid_size;
            }
            let mut y = 0.0;
            while y <= h {
                p.line_segment([self.view.to_screen(0.0, y), self.view.to_screen(w, y)], st);
                y += g.grid_size;
            }
        }
        match g.sym.as_str() {
            "vertical" => { p.line_segment([self.view.to_screen(g.cx, 0.0), self.view.to_screen(g.cx, h)], st); }
            "horizontal" => { p.line_segment([self.view.to_screen(0.0, g.cy), self.view.to_screen(w, g.cy)], st); }
            "quadrant" => {
                p.line_segment([self.view.to_screen(g.cx, 0.0), self.view.to_screen(g.cx, h)], st);
                p.line_segment([self.view.to_screen(0.0, g.cy), self.view.to_screen(w, g.cy)], st);
            }
            "radial" => {
                let n = g.segments.clamp(2, 32);
                let dd = (w * w + h * h).sqrt();
                for i in 0..n {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n as f32;
                    p.line_segment([self.view.to_screen(g.cx, g.cy), self.view.to_screen(g.cx + a.cos() * dd, g.cy + a.sin() * dd)], st);
                }
            }
            _ => {}
        }
    }

    fn draw_overlay(&self, p: &egui::Painter, sh: &Shared) {
        let v = &self.view;
        let white = egui::Stroke::new(1.5, Color32::WHITE);
        let black = egui::Stroke::new(3.0, Color32::from_black_alpha(160));
        let accent = Color32::from_rgb(61, 143, 230);
        if let Some(o) = &self.sel_outline {
            if self.xform.is_none() && self.sel.is_some() {
                let pts: Vec<Pos2> = o.iter().chain(o.first()).map(|&(x, y)| v.to_screen(x, y)).collect();
                p.add(egui::Shape::line(pts.clone(), black));
                let ph = (self.now * 8.0) as f32 % 10.0;
                let _ = ph;
                p.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.2, Color32::WHITE), 5.0, 5.0));
            }
        }
        match &self.active {
            Some(Active::Select { x0, y0, x1, y1, pts, .. }) => {
                let poly: Vec<Pos2> = match self.sel_shape {
                    SelShape::Lasso => pts.iter().map(|&(x, y)| v.to_screen(x, y)).collect(),
                    SelShape::Ellipse => raster::ellipse_poly((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0).iter().map(|&(x, y)| v.to_screen(x, y)).collect(),
                    _ => vec![v.to_screen(*x0, *y0), v.to_screen(*x1, *y0), v.to_screen(*x1, *y1), v.to_screen(*x0, *y1)],
                };
                if poly.len() > 1 {
                    let mut closed = poly.clone();
                    closed.push(poly[0]);
                    p.add(egui::Shape::line(closed.clone(), black));
                    p.extend(egui::Shape::dashed_line(&closed, egui::Stroke::new(1.2, Color32::WHITE), 5.0, 5.0));
                }
            }
            Some(Active::Wand { preview: Some((_, b)), .. }) => {
                let c = [v.to_screen(b.x as f32, b.y as f32), v.to_screen(b.right() as f32, b.y as f32), v.to_screen(b.right() as f32, b.bottom() as f32), v.to_screen(b.x as f32, b.bottom() as f32)];
                p.add(egui::Shape::closed_line(c.to_vec(), egui::Stroke::new(1.5, accent)));
            }
            Some(Active::Gradient { x0, y0, x1, y1, .. }) => {
                let (a, b) = (v.to_screen(*x0, *y0), v.to_screen(*x1, *y1));
                p.line_segment([a, b], black);
                p.line_segment([a, b], white);
                p.circle_filled(a, 5.0, Color32::WHITE);
                p.circle_filled(b, 5.0, Color32::WHITE);
            }
            _ => {}
        }
        if let Some(x) = &self.xform {
            let c = xcorners(&x.st);
            let pts: Vec<Pos2> = c.iter().map(|&(a, b)| v.to_screen(a, b)).collect();
            p.add(egui::Shape::closed_line(pts.clone(), egui::Stroke::new(1.5, accent)));
            for (i, q) in pts.iter().enumerate() {
                p.circle_filled(*q, 7.0, Color32::WHITE);
                p.circle_stroke(*q, 7.0, egui::Stroke::new(1.5, accent));
                let n = pts[(i + 1) % 4];
                let mid = Pos2::new((q.x + n.x) / 2.0, (q.y + n.y) / 2.0);
                if !x.uniform {
                    p.circle_filled(mid, 5.0, Color32::WHITE);
                    p.circle_stroke(mid, 5.0, egui::Stroke::new(1.5, accent));
                }
            }
            let rh = rot_handle(&x.st, v);
            let tm = Pos2::new((pts[0].x + pts[1].x) / 2.0, (pts[0].y + pts[1].y) / 2.0);
            p.line_segment([tm, rh], egui::Stroke::new(1.5, accent));
            p.circle_filled(rh, 7.0, accent);
        }
        if self.tool == Tool::Clone {
            if let Some((sx, sy)) = self.clone_src {
                let q = match (self.clone_off, self.hover) {
                    (Some(off), Some(hv)) => { let (x, y) = v.to_doc(hv); v.to_screen(x + off.0, y + off.1) }
                    _ => v.to_screen(sx, sy),
                };
                p.circle_stroke(q, 12.0, black);
                p.circle_stroke(q, 12.0, white);
                p.line_segment([q - Vec2::new(18.0, 0.0), q + Vec2::new(18.0, 0.0)], white);
                p.line_segment([q - Vec2::new(0.0, 18.0), q + Vec2::new(0.0, 18.0)], white);
            }
        }
        if let Some((pos, c)) = self.loupe {
            let ctr = pos - Vec2::new(0.0, 70.0);
            p.circle_filled(ctr, 46.0, Color32::from_gray(30));
            let half_new = egui::Rect::from_center_size(ctr - Vec2::new(0.0, 20.0), Vec2::new(76.0, 38.0));
            p.circle_filled(ctr, 42.0, Color32::from_rgb(sh.colors.fg[0], sh.colors.fg[1], sh.colors.fg[2]));
            p.rect_filled(half_new, 0.0, Color32::TRANSPARENT);
            let pts: Vec<Pos2> = (0..=24).map(|i| { let a = std::f32::consts::PI + i as f32 / 24.0 * std::f32::consts::PI; ctr + Vec2::new(a.cos() * 42.0, a.sin() * 42.0) }).collect();
            p.add(egui::Shape::convex_polygon(pts, Color32::from_rgb(c[0], c[1], c[2]), egui::Stroke::NONE));
            p.circle_stroke(ctr, 44.0, egui::Stroke::new(4.0, Color32::from_gray(40)));
        }
        if let Some(Active::Paint { stroke, .. }) = &self.active {
            if let (Some(l), Some(h)) = (stroke.lazy_pos(), self.hover) {
                // нить стабилизатора: от пера к кисти
                let lp = v.to_screen(l.x, l.y);
                p.line_segment([lp, h], egui::Stroke::new(1.0, Color32::from_white_alpha(170)));
                p.circle_stroke(h, stroke.lazy_r * v.s, egui::Stroke::new(1.0, Color32::from_white_alpha(60)));
                p.circle_filled(lp, 2.5, accent);
            }
        }
        if let Some(h) = self.hover {
            if sh.prefs.cursor && self.tool.is_paint() && self.active.is_none() && self.canvas_rect.contains(h) {
                let ts = tool_state(sh, self.tool);
                let b = sh.brushes.get(&ts.brush_id);
                let r = (brush::size_px(b, ts.size) * self.view.s / 2.0).max(1.5);
                p.circle_stroke(h, r + 0.75, egui::Stroke::new(1.0, Color32::from_black_alpha(170)));
                p.circle_stroke(h, r - 0.25, egui::Stroke::new(1.0, Color32::from_white_alpha(220)));
            }
        }
    }

    /* ---------- ввод ---------- */

    fn handle_input(&mut self, ui: &egui::Ui, rect: egui::Rect, sh: &mut Shared) {
        let ctx = ui.ctx().clone();
        let (events, mods, multi, scroll, zoom, hover) = ctx.input(|i| (i.events.clone(), i.modifiers, i.multi_touch(), i.raw_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        let over = |p: Pos2| rect.contains(p) && ctx.layer_id_at(p).map_or(true, |l| l.order == egui::Order::Background);
        self.hover = hover.filter(|p| over(*p));
        if let Some(hp) = hover {
            if over(hp) {
                if zoom != 1.0 {
                    self.view.zoom_at(hp, zoom);
                } else if scroll.y != 0.0 && self.touches.is_empty() {
                    if mods.alt && self.tool.is_paint() {
                        // Alt + колесо — размер кисти
                        let ts = self.tool_state_mut(sh);
                        ts.size = (ts.size + scroll.y.signum() * 0.02).clamp(0.0, 1.0);
                        let (id, sz) = (ts.brush_id.clone(), ts.size);
                        let px = brush::size_px(sh.brushes.get(&id), sz);
                        self.toast(&format!("Размер: {px:.1} px"));
                    } else {
                        self.view.zoom_at(hp, (scroll.y * 0.0022).exp());
                    }
                }
            }
        }
        for ev in events {
            match ev {
                egui::Event::Touch { id, phase, pos, force, .. } => {
                    let id = id.0;
                    match phase {
                        egui::TouchPhase::Start => {
                            if !over(pos) && self.touches.is_empty() {
                                continue;
                            }
                            self.touches.insert(id, pos);
                            let n = self.touches.len();
                            if n == 1 && self.active.is_none() && self.gesture.is_none() {
                                if sh.prefs.touch_draw {
                                    self.draw_touch = Some(id);
                                    self.tool_t0 = self.now;
                                    let pr = force.filter(|f| *f > 0.0 && *f <= 1.0).unwrap_or(1.0);
                                    self.down(pos, pr, false, mods, sh);
                                } else {
                                    self.gesture = Some((self.now, 0.0, false, 1));
                                }
                            } else if n >= 2 {
                                if self.draw_touch.is_some() {
                                    if self.now - self.tool_t0 < 0.35 { self.cancel(); } else { let p = pos; self.up(p, sh); }
                                    self.draw_touch = None;
                                }
                                let g = self.gesture.get_or_insert((self.now, 0.0, false, n));
                                g.3 = g.3.max(n);
                            }
                        }
                        egui::TouchPhase::Move => {
                            if let Some(t) = self.touches.get_mut(&id) {
                                let prev = *t;
                                *t = pos;
                                if self.draw_touch == Some(id) {
                                    let pr = force.filter(|f| *f > 0.0 && *f <= 1.0).unwrap_or(1.0);
                                    self.moved(pos, pr, mods, sh);
                                } else if self.touches.len() == 1 && self.gesture.is_some() {
                                    let d = pos - prev;
                                    self.view.ox += d.x;
                                    self.view.oy += d.y;
                                    if let Some(g) = &mut self.gesture { g.1 += d.length(); if g.1 > 10.0 { g.2 = true; } }
                                }
                            }
                        }
                        egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                            if self.touches.remove(&id).is_none() {
                                continue;
                            }
                            if self.draw_touch == Some(id) {
                                if phase == egui::TouchPhase::Cancel { self.cancel(); } else { self.up(pos, sh); }
                                self.draw_touch = None;
                            }
                            if self.touches.is_empty() {
                                if let Some((t0, _, moved, maxn)) = self.gesture.take() {
                                    if !moved && self.now - t0 < 0.32 {
                                        match maxn { 2 => self.undo(), 3 => self.redo(), n if n >= 4 => self.ui_hidden = !self.ui_hidden, _ => {} }
                                    }
                                }
                            }
                        }
                    }
                }
                egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                    if !self.touches.is_empty() || self.draw_touch.is_some() {
                        continue;
                    }
                    if pressed {
                        if !over(pos) || self.active.is_some() {
                            continue;
                        }
                        match button {
                            egui::PointerButton::Middle => self.active = Some(Active::Pan { last: pos }),
                            egui::PointerButton::Secondary => { self.active = Some(Active::Pick); self.pick_preview(pos, sh); }
                            egui::PointerButton::Primary => {
                                self.mouse_down = true;
                                if self.space { self.active = Some(Active::Pan { last: pos }); }
                                else if modifiers.alt && self.tool.is_paint() && self.tool != Tool::Clone { self.active = Some(Active::Pick); self.pick_preview(pos, sh); }
                                else { self.down(pos, 1.0, true, modifiers, sh); }
                            }
                            _ => {}
                        }
                    } else if self.active.is_some() && (self.mouse_down || button != egui::PointerButton::Primary) {
                        self.mouse_down = false;
                        self.up(pos, sh);
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    if !self.touches.is_empty() || self.draw_touch.is_some() {
                        continue;
                    }
                    if self.active.is_some() {
                        self.moved(pos, 1.0, mods, sh);
                    }
                }
                _ => {}
            }
        }
        if let Some(mt) = multi {
            if mt.num_touches >= 2 && self.touches.len() >= 2 {
                let n = self.touches.len() as f32;
                let c = self.touches.values().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n;
                let cp = c.to_pos2();
                self.view.zoom_at(cp, mt.zoom_delta);
                if mt.rotation_delta.abs() > 0.0 {
                    self.view.rotate_at(cp, mt.rotation_delta);
                    let q = std::f32::consts::FRAC_PI_2;
                    let snap = (self.view.r / q).round() * q;
                    if (self.view.r - snap).abs() < 0.03 && mt.rotation_delta.abs() < 0.02 {
                        self.view.rotate_at(cp, snap - self.view.r);
                    }
                }
                self.view.ox += mt.translation_delta.x;
                self.view.oy += mt.translation_delta.y;
                if let Some(g) = &mut self.gesture {
                    g.1 += mt.translation_delta.length() + (mt.zoom_delta - 1.0).abs() * 200.0 + mt.rotation_delta.abs() * 200.0;
                    if g.1 > 12.0 { g.2 = true; }
                }
            }
        }
    }

    fn down(&mut self, pos: Pos2, pressure: f32, mouse: bool, mods: egui::Modifiers, sh: &mut Shared) {
        let (x, y) = self.view.to_doc(pos);
        let _ = mouse;
        self.tool_t0 = self.now;
        match self.tool {
            Tool::Brush => self.paint_begin(PaintMode::Paint, x, y, pressure, pos, sh),
            Tool::Eraser => self.paint_begin(PaintMode::Erase, x, y, pressure, pos, sh),
            Tool::Smudge => self.paint_begin(PaintMode::Smudge, x, y, pressure, pos, sh),
            Tool::Clone => {
                if self.clone_src.is_none() || mods.alt {
                    self.clone_src = Some((x, y));
                    self.clone_off = None;
                    self.toast("Источник задан — теперь рисуйте");
                } else {
                    self.paint_begin(PaintMode::Clone, x, y, pressure, pos, sh);
                }
            }
            Tool::Fill => self.fill_begin(x, y, pos.x, sh),
            Tool::Picker => { self.active = Some(Active::Pick); self.pick_preview(pos, sh); }
            Tool::Gradient | Tool::Shapes => {
                if !self.check_layer() || !self.raster_only() { return; }
                self.begin_work();
                self.active = Some(if self.tool == Tool::Gradient {
                    Active::Gradient { x0: x, y0: y, x1: x, y1: y, moved: false, start: pos, pending: false }
                } else {
                    Active::Shape { x0: x, y0: y, x1: x, y1: y, moved: false, start: pos, center: mods.alt, pending: false }
                });
            }
            Tool::Select => {
                let op = if mods.shift { SelOp::Add } else if mods.alt { SelOp::Subtract } else { self.sel_op };
                if self.sel_shape == SelShape::Wand {
                    self.wand_begin(x, y, pos.x, op, sh);
                } else {
                    self.active = Some(Active::Select { x0: x, y0: y, x1: x, y1: y, op, pts: vec![(x, y)], start: pos, square: false });
                }
            }
            Tool::Transform => self.xform_down(pos),
            Tool::Text => {
                let l = self.doc.layer();
                let edit = l.text.as_ref().map(|t| (t.clone(), self.doc.active)).filter(|(t, _)| {
                    let bw = t.size * t.str.lines().map(|s| s.chars().count()).max().unwrap_or(1) as f32 * 0.6;
                    let bh = t.size * t.lh * t.str.lines().count().max(1) as f32;
                    x >= t.x - 10.0 && y >= t.y - 10.0 && x <= t.x + bw + 10.0 && y <= t.y + bh + 10.0
                });
                self.request = Some(match edit {
                    Some((spec, li)) => Request::Text { spec, layer: Some(li) },
                    None => Request::Text { spec: TextSpec { x, y, size: (self.doc.h as f32 / 14.0).round().max(24.0), color: util::hex(sh.colors.fg), ..Default::default() }, layer: None },
                });
            }
            Tool::Hand => self.active = Some(Active::Pan { last: pos }),
            Tool::Zoom => self.active = Some(Active::Zoom { start: pos, last_x: pos.x, moved: false, out: mods.alt }),
        }
    }

    fn moved(&mut self, pos: Pos2, pressure: f32, mods: egui::Modifiers, sh: &mut Shared) {
        let (x, y) = self.view.to_doc(pos);
        let now = self.now;
        match &mut self.active {
            Some(Active::Paint { .. }) => self.paint_move(x, y, pressure, pos, sh),
            Some(Active::Pick) => self.pick_preview(pos, sh),
            Some(Active::Fill { sx0, base, tol, pending, .. }) => {
                let t = (*base + (pos.x - *sx0) / 4.0).clamp(0.0, 100.0).round();
                if t != *tol { *tol = t; *pending = true; }
            }
            Some(Active::Gradient { x0, y0, x1, y1, moved, start, pending }) => {
                let (mut nx, mut ny) = (x, y);
                if mods.shift {
                    let a = ((ny - *y0).atan2(nx - *x0) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                    let d = (nx - *x0).hypot(ny - *y0);
                    nx = *x0 + a.cos() * d;
                    ny = *y0 + a.sin() * d;
                }
                *x1 = nx; *y1 = ny; *pending = true;
                if (pos - *start).length() > 4.0 { *moved = true; }
            }
            Some(Active::Shape { x0, y0, x1, y1, moved, start, center, pending }) => {
                let (mut nx, mut ny) = (x, y);
                if mods.shift {
                    if self.shape_kind == ShapeKind::Line {
                        let a = ((ny - *y0).atan2(nx - *x0) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                        let d = (nx - *x0).hypot(ny - *y0);
                        nx = *x0 + a.cos() * d;
                        ny = *y0 + a.sin() * d;
                    } else {
                        let m = (nx - *x0).abs().max((ny - *y0).abs());
                        nx = *x0 + (nx - *x0).signum() * m;
                        ny = *y0 + (ny - *y0).signum() * m;
                    }
                }
                *x1 = nx; *y1 = ny; *center = mods.alt; *pending = true;
                if (pos - *start).length() > 3.0 { *moved = true; }
            }
            Some(Active::Select { x0, y0, x1, y1, pts, square, .. }) => {
                *x1 = x; *y1 = y; *square = mods.shift;
                if mods.shift {
                    let m = (x - *x0).abs().max((y - *y0).abs());
                    *x1 = *x0 + (x - *x0).signum() * m;
                    *y1 = *y0 + (y - *y0).signum() * m;
                }
                if let Some(l) = pts.last() {
                    if (x - l.0).hypot(y - l.1) * self.view.s > 2.0 { pts.push((x, y)); }
                }
            }
            Some(Active::Wand { sx0, base, tol, .. }) => {
                let t = (*base + (pos.x - *sx0) / 4.0).clamp(0.0, 100.0).round();
                if t != *tol { *tol = t; self.wand_compute(); }
            }
            Some(Active::XDrag { .. }) => self.xform_move(x, y, mods.shift),
            Some(Active::VErase { .. }) => self.verase_to(x, y),
            Some(Active::Pan { last }) => {
                let d = pos - *last;
                *last = pos;
                self.view.ox += d.x;
                self.view.oy += d.y;
            }
            Some(Active::Zoom { start, last_x, moved, .. }) => {
                let dx = pos.x - *last_x;
                if (pos.x - start.x).abs() > 4.0 { *moved = true; }
                if *moved {
                    let s = *start;
                    *last_x = pos.x;
                    self.view.zoom_at(s, (dx * 0.01).exp());
                }
            }
            None => {}
        }
        let _ = now;
    }

    fn up(&mut self, pos: Pos2, sh: &mut Shared) {
        let Some(a) = self.active.take() else { return };
        match a {
            Active::Paint { .. } => { self.active = Some(a); self.paint_end(sh); }
            Active::Pick => {
                if let Some(c) = self.sample(pos, sh.prefs.pick_layer) { sh.colors.set(c); }
                self.loupe = None;
                if self.tool == Tool::Picker { let p = self.prev_tool; self.set_tool(p); }
            }
            Active::Fill { .. } => { self.active = Some(a); self.flush_active(sh); self.active = None; self.commit_work("Заливка"); }
            Active::Gradient { moved, .. } | Active::Shape { moved, .. } => {
                let name = if matches!(a, Active::Gradient { .. }) { "Градиент" } else { "Фигура" };
                if moved {
                    self.active = Some(a);
                    self.render_tool_preview(sh);
                    self.active = None;
                    self.commit_work(name);
                } else {
                    self.work = None;
                    self.invalidate_all();
                }
            }
            Active::Select { x0, y0, x1, y1, op, pts, start, .. } => {
                if (pos - start).length() < 4.0 && self.sel_shape != SelShape::Lasso {
                    if op == SelOp::Replace && self.sel.is_some() { self.select_none(); }
                    return;
                }
                let poly: Vec<(f32, f32)> = match self.sel_shape {
                    SelShape::Lasso => { if pts.len() < 3 { return; } pts }
                    SelShape::Ellipse => raster::ellipse_poly((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0),
                    _ => vec![(x0.min(x1), y0.min(y1)), (x0.max(x1), y0.min(y1)), (x0.max(x1), y0.max(y1)), (x0.min(x1), y0.max(y1))],
                };
                let mut m = vec![0u8; self.doc.w * self.doc.h];
                let w = self.doc.w;
                raster::fill_polys(self.doc.w, self.doc.h, &[poly.clone()], false, |x, y, c| m[y * w + x] = (c * 255.0) as u8);
                self.apply_selection(m, op, Some(poly), "Выделение");
            }
            Active::Wand { preview, op, tol, .. } => {
                self.wand_tol = tol;
                if let Some((m, _)) = preview { self.apply_selection(m, op, None, "Волшебная палочка"); }
            }
            Active::XDrag { .. } => {}
            Active::VErase { before, list, total, changed, .. } => self.verase_commit(before, list, total, changed),
            Active::Pan { .. } => {}
            Active::Zoom { start, moved, out, .. } => { if !moved { self.view.zoom_at(start, if out { 0.5 } else { 2.0 }); } }
        }
    }

    pub fn cancel(&mut self) {
        if let Some(a) = self.active.take() {
            match a {
                Active::Paint { total, .. } => { self.work = None; if let Some(t) = total { self.invalidate(t); } }
                Active::Fill { .. } | Active::Gradient { .. } | Active::Shape { .. } => { self.work = None; self.invalidate_all(); }
                Active::VErase { total, .. } => { self.work = None; if let Some(t) = total { self.invalidate(t); } }
                _ => {}
            }
        }
        self.loupe = None;
    }

    /// Таймеры: пипетка при удержании и быстрые фигуры.
    fn tick(&mut self, sh: &mut Shared) {
        let now = self.now;
        let mut do_pick = false;
        let mut do_shape = false;
        if let Some(Active::Paint { mode, started, start_t, shape, qs_t, .. }) = &self.active {
            if !*started && sh.prefs.hold_pick && *mode != PaintMode::Clone && now - *start_t > sh.prefs.hold_delay as f64 {
                do_pick = true;
            }
            if *started && shape.is_none() && sh.prefs.quick_shape && matches!(mode, PaintMode::Paint | PaintMode::Erase) && now - *qs_t > sh.prefs.quick_shape_delay as f64 {
                do_shape = true;
            }
        }
        if do_pick {
            let pos = match &self.active { Some(Active::Paint { start, .. }) => *start, _ => Pos2::ZERO };
            self.cancel();
            self.active = Some(Active::Pick);
            self.pick_preview(pos, sh);
        }
        if do_shape {
            let hit = match &self.active { Some(Active::Paint { stroke, .. }) => brush::detect_shape(&stroke.pts), _ => None };
            if let Some(Active::Paint { qs_t, .. }) = &mut self.active { *qs_t = f64::MAX; }
            if let Some(h) = hit {
                if let Some(Active::Paint { stroke, shape, .. }) = &mut self.active {
                    stroke.replace(h.pts);
                    *shape = Some(h.kind);
                }
                self.paint_rerender();
                self.toast(&format!("{}{}", h.label, if h.kind == "line" { " — двигайте, чтобы изменить" } else { "" }));
            }
        }
    }

    /* ---------- кисть ---------- */

    fn check_layer(&mut self) -> bool {
        let li = self.doc.active;
        if self.doc.layers[li].group { self.toast("Выбрана папка — выберите слой внутри неё"); return false; }
        if !doc::shown(&self.doc.layers, li) { self.toast(if self.doc.layers[li].visible { "Слой в скрытой папке" } else { "Слой скрыт" }); return false; }
        if doc::locked(&self.doc.layers, li) { self.toast("Слой заблокирован"); return false; }
        if self.doc.layers[li].text.is_some() && !self.editing_mask() {
            let li = self.doc.active;
            let id = self.doc.layers[li].id.clone();
            let before = self.doc.layers[li].text.take();
            self.hist.push("Растеризовать текст", Op::Text { layer: id, before, after: None });
        }
        true
    }

    fn begin_work(&mut self) {
        let li = self.doc.active;
        let mask = self.editing_mask();
        self.work = Some(Work { layer: li, img: (**self.target(li)).clone(), mask });
    }

    /// Рисуем ли сейчас по маске активного слоя.
    pub fn editing_mask(&self) -> bool {
        self.mask_edit && self.doc.layer().mask.is_some()
    }

    /// Что редактируется на слое li: маска (если выбрана) или пиксели.
    fn target(&self, li: usize) -> &Arc<Img> {
        let l = &self.doc.layers[li];
        if self.mask_edit && li == self.doc.active { if let Some(m) = &l.mask { return m; } }
        &l.img
    }

    fn mirrors(&self) -> Vec<Mirror> {
        let g = &self.doc.guide;
        brush::mirrors(&g.sym, g.segments, g.rotational, g.cx, g.cy)
    }

    fn paint_begin(&mut self, mode: PaintMode, x: f32, y: f32, pressure: f32, pos: Pos2, sh: &mut Shared) {
        if !self.check_layer() { return; }
        let is_vec = self.doc.layer().is_vector() && !self.editing_mask();
        if is_vec && matches!(mode, PaintMode::Smudge | PaintMode::Clone) {
            self.toast("На векторном слое работают кисть и ластик. Для этого инструмента растеризуйте слой");
            return;
        }
        let tool = self.tool;
        let ts = tool_state(sh, tool).clone();
        let b = sh.brushes.get(&ts.brush_id).clone();
        let size = brush::size_px(&b, ts.size);
        self.begin_work();
        if is_vec && mode == PaintMode::Erase {
            let before = self.doc.layer().vector.clone();
            let list = before.as_ref().map(|v| (**v).clone()).unwrap_or_default();
            let r = (size / 2.0).max(1.0);
            let emode = EraseMode::from_id(&sh.prefs.vector_erase);
            self.active = Some(Active::VErase { list, before, last: (x, y), r, total: None, mode: emode, changed: false });
            self.verase_to(x, y);
            return;
        }
        let n = self.doc.w * self.doc.h * 4;
        if self.buffer.len() != n { self.buffer = vec![0; n]; } else { self.buffer.fill(0); }
        let grain = b.grain.as_ref().filter(|_| b.grain_strength > 0.0).map(|g| (brush::grain(g), b.grain_scale, b.grain_strength));
        let blend = if is_vec { Blend::Normal } else if tool == Tool::Brush && ts.mode != "source-over" { Blend::from_id(&ts.mode) } else { b.blend.as_deref().map(Blend::from_id).unwrap_or(Blend::Normal) };
        let mut clone_off = (0.0, 0.0);
        let mut clone_src = None;
        if mode == PaintMode::Clone {
            let (sx, sy) = self.clone_src.unwrap_or((x, y));
            if self.clone_off.is_none() || !sh.prefs.clone_aligned { self.clone_off = Some((sx - x, sy - y)); }
            clone_off = self.clone_off.unwrap();
            clone_src = Some(self.doc.layer().img.clone());
        }
        let mut stroke = Stroke::new(b, size, sh.colors.fg, self.mirrors(), sh.prefs.pressure_gamma, sh.prefs.streamline_mul);
        if mode != PaintMode::Smudge {
            stroke.lazy_r = sh.prefs.stabilizer.clamp(0.0, 1.0) * 70.0 / self.view.s.max(0.01);
        }
        self.active = Some(Active::Paint {
            mode, stroke, total: None, opacity: ts.opacity, grain, blend, start: pos, start_t: self.now, started: false, shape: None,
            qs_anchor: pos, qs_t: self.now, picks: vec![], clone_src, clone_off,
        });
        self.paint_feed(|s, sink| s.add(x, y, pressure, 0.0, sink));
    }

    /// Выполнить действие со штрихом с подходящим приёмником отпечатков.
    fn paint_feed(&mut self, f: impl FnOnce(&mut Stroke, &mut dyn Sink)) {
        let (w, h) = (self.doc.w, self.doc.h);
        let alpha_lock = self.doc.layer().alpha_lock;
        let Editor { active, buffer, work, sel, .. } = self;
        let Some(Active::Paint { mode, stroke, picks, clone_src, clone_off, opacity, .. }) = active else { return };
        let b = &stroke.b;
        let (shape, hard, round, pixel) = (b.shape.clone(), b.hardness, b.roundness, b.pixel);
        match mode {
            PaintMode::Smudge => {
                let Some(work) = work else { return };
                let strength = *opacity * b.smudge;
                let (flow, rate) = (b.flow, 0.12 + 0.5 * (1.0 - b.smudge));
                let mut sink = SmudgeSink { work: &mut work.img, sel: sel.as_deref(), alpha_lock, picks, strength, flow, rate, shape, hard, round, tips: Default::default() };
                f(stroke, &mut sink);
            }
            PaintMode::Clone => {
                let Some(src) = clone_src.clone() else { return };
                let mut sink = CloneSink { buf: buffer, w, h, src: &src, off: *clone_off, shape, hard, round, tips: Default::default() };
                f(stroke, &mut sink);
            }
            _ => {
                let mut sink = PaintSink { buf: buffer, w, h, shape, hard, round, pixel, tips: Default::default() };
                f(stroke, &mut sink);
            }
        }
    }

    fn paint_move(&mut self, x: f32, y: f32, pressure: f32, pos: Pos2, _sh: &mut Shared) {
        let now = self.now;
        let mut line_from = None;
        if let Some(Active::Paint { start, started, shape, qs_anchor, qs_t, stroke, .. }) = &mut self.active {
            if !*started && (pos - *start).length() > 7.0 { *started = true; }
            if let Some(k) = shape {
                if *k == "line" { line_from = stroke.pts.first().copied(); } else { return; }
            } else if (pos - *qs_anchor).length() > 4.0 {
                *qs_anchor = pos;
                *qs_t = now;
            }
        }
        if let Some(a) = line_from {
            if let Some(Active::Paint { stroke, .. }) = &mut self.active { stroke.replace(brush::line_pts((a.x, a.y), (x, y), a.p)); }
            self.paint_rerender();
            return;
        }
        self.paint_feed(|s, sink| s.add(x, y, pressure, 0.0, sink));
    }

    fn paint_rerender(&mut self) {
        let total = match &self.active { Some(Active::Paint { total, .. }) => *total, _ => None };
        if let Some(t) = total.and_then(|t| t.clip(self.doc.w, self.doc.h)) {
            let w = self.doc.w;
            for y in t.y as usize..t.bottom() as usize {
                self.buffer[(y * w + t.x as usize) * 4..(y * w + t.right() as usize) * 4].fill(0);
            }
            if let (Some(Active::Paint { mode: PaintMode::Smudge, .. }), Some(work)) = (&self.active, &mut self.work) {
                let l = &self.doc.layers[work.layer];
                let s: &Img = if work.mask { l.mask.as_deref().unwrap_or(&l.img) } else { &l.img };
                work.img.copy_rect_from(s, t);
            }
        }
        self.paint_feed(|s, sink| s.rerender(sink));
        if let (Some(t), Some(Active::Paint { stroke, .. })) = (total, &mut self.active) {
            stroke.dirty = util::union_opt(stroke.dirty, t);
        }
    }

    fn paint_end(&mut self, sh: &mut Shared) {
        let (needs_taper, has_shape, mode) = match &self.active {
            Some(Active::Paint { stroke, shape, mode, .. }) => (stroke.b.taper_end > 0.0 && stroke.pts.len() > 1, shape.is_some(), *mode),
            _ => return,
        };
        let post = sh.prefs.post_smooth;
        if !has_shape {
            let smooth = post > 0.0 && matches!(mode, PaintMode::Paint | PaintMode::Erase);
            if (needs_taper || smooth) && matches!(mode, PaintMode::Paint | PaintMode::Erase) {
                self.paint_feed(|s, sink| s.catch_up(sink));
                if smooth {
                    if let Some(Active::Paint { stroke, .. }) = &mut self.active { brush::smooth_points(&mut stroke.pts, post); }
                }
                self.paint_rerender();
            } else {
                self.paint_feed(|s, sink| s.end(sink));
            }
        }
        self.flush_active(sh);
        let name = match mode { PaintMode::Paint => "Мазок кистью", PaintMode::Erase => "Стирание", PaintMode::Smudge => "Растушёвка пальцем", PaintMode::Clone => "Клонирование" };
        let (total, stroke, opacity) = match self.active.take() { Some(Active::Paint { total, stroke, opacity, .. }) => (total, Some(stroke), opacity), _ => (None, None, 1.0) };
        let wl = self.work.as_ref().map(|w| w.layer);
        if let (Some(li), Some(st), PaintMode::Paint) = (wl, stroke, mode) {
            if self.doc.layers.get(li).map_or(false, |l| l.is_vector()) {
                self.vector_add_stroke(li, st, opacity, total, sh.colors.fg);
                self.doc.strokes += 1;
                sh.colors.used();
                return;
            }
        }
        if let Some(t) = total {
            self.commit_rect(t, name);
            self.doc.strokes += 1;
            if mode == PaintMode::Paint { sh.colors.used(); }
        } else {
            self.work = None;
            self.invalidate_all();
        }
    }

    /* ---------- векторные слои ---------- */

    /// Продолжить движение векторного ластика до точки (x, y).
    fn verase_to(&mut self, x: f32, y: f32) {
        let (w, h) = (self.doc.w, self.doc.h);
        let Editor { active, work, .. } = self;
        let Some(Active::VErase { list, last, r, total, mode, changed, .. }) = active else { return };
        let dirty = vector::erase(list, *last, (x, y), *r, *mode);
        *last = (x, y);
        let Some(d) = dirty.and_then(|d| d.clip(w, h)) else { return };
        *changed = true;
        *total = util::union_opt(*total, d);
        if let Some(work) = work { vector::render(&mut work.img, list, d); }
        self.invalidate(d);
    }

    fn verase_commit(&mut self, before: Option<vector::VList>, list: Vec<VStroke>, total: Option<Rect>, changed: bool) {
        let li = self.work.as_ref().map_or(self.doc.active, |w| w.layer);
        if !changed || li >= self.doc.layers.len() { self.work = None; return; }
        let id = self.doc.layers[li].id.clone();
        let after = Some(Arc::new(list));
        self.hist.begin_group();
        match total { Some(t) => self.commit_rect(t, "Векторный ластик"), None => self.work = None }
        self.doc.layers[li].vector = after.clone();
        self.hist.push("Векторный ластик", Op::Vector { layer: id, before, after });
        self.hist.end_group("Векторный ластик");
        self.mark_changed();
    }

    /// Записать законченный штрих в векторный слой (с копиями по симметрии).
    fn vector_add_stroke(&mut self, li: usize, st: Stroke, opacity: f32, total: Option<Rect>, color: [u8; 3]) {
        if st.pts.is_empty() { self.work = None; self.invalidate_all(); return; }
        let base = VStroke { pts: st.pts.clone(), brush: Arc::new(Brush { streamline: 0.0, ..st.b.clone() }), size: st.base, color, opacity, seed: st.seed(), gamma: st.gamma };
        let mut new = vec![];
        if st.mirrors.is_empty() {
            new.push(base);
        } else {
            for m in &st.mirrors {
                let aff = Affine { a: m.a, c: m.b, b: m.c, d: m.d, e: m.cx - m.a * m.cx - m.b * m.cy, f: m.cy - m.c * m.cx - m.d * m.cy };
                new.extend(vector::transform(std::slice::from_ref(&base), aff));
            }
        }
        let before = self.doc.layers[li].vector.clone();
        let mut list = before.as_ref().map(|v| (**v).clone()).unwrap_or_default();
        let mut rect = total;
        for v in &new { rect = util::union_opt(rect, v.bbox()); }
        list.extend(new);
        let Some(r) = rect.and_then(|r| r.clip(self.doc.w, self.doc.h)) else { self.work = None; return };
        if let Some(work) = &mut self.work { vector::render(&mut work.img, &list, r); }
        let id = self.doc.layers[li].id.clone();
        self.hist.begin_group();
        self.commit_rect(r, "Векторная линия");
        let after = Some(Arc::new(list));
        self.doc.layers[li].vector = after.clone();
        self.hist.push("Векторная линия", Op::Vector { layer: id, before, after });
        self.hist.end_group("Векторная линия");
    }

    /// Заменить штрихи активного векторного слоя и перерисовать его.
    fn vector_replace(&mut self, name: &str, f: impl FnOnce(&[VStroke]) -> Vec<VStroke>) {
        let li = self.doc.active;
        let Some(before) = self.doc.layers[li].vector.clone() else { return };
        let list = f(&before);
        let img = vector::render_full(self.doc.w, self.doc.h, &list);
        self.work = Some(Work { layer: li, img, mask: false });
        let id = self.doc.layers[li].id.clone();
        self.hist.begin_group();
        self.commit_work(name);
        let after = Some(Arc::new(list));
        self.doc.layers[li].vector = after.clone();
        self.hist.push(name, Op::Vector { layer: id, before: Some(before), after });
        self.hist.end_group(name);
    }

    pub fn layer_add_vector(&mut self) {
        self.commit_pending();
        let n = self.doc.next_name().replace("Слой", "Вектор");
        let mut l = Layer::new(self.doc.w, self.doc.h, &n);
        l.vector = Some(Arc::new(vec![]));
        self.add_layer(l, "Новый векторный слой");
        self.toast("Векторный слой: линии можно стирать до пересечения и масштабировать без потерь");
    }

    pub fn layer_rasterize(&mut self) {
        self.commit_pending();
        let li = self.doc.active;
        let l = &mut self.doc.layers[li];
        let Some(before) = l.vector.take() else { return };
        l.ver += 1;
        let id = l.id.clone();
        self.hist.push("Растеризовать слой", Op::Vector { layer: id, before: Some(before), after: None });
        self.mark_changed();
    }

    /// Проверка для инструментов, которые работают только с пикселями.
    fn raster_only(&mut self) -> bool {
        if self.doc.layer().is_vector() && !self.editing_mask() {
            self.toast("Это векторный слой. Растеризуйте его (Слои → Растеризовать) или выберите обычный слой");
            return false;
        }
        true
    }

    /// Перенос буфера штриха в рабочую копию слоя.
    fn apply_buffer(&mut self, r: Rect, mode: PaintMode, opacity: f32, grain: Option<&(Arc<Grain>, f32, f32)>, blend: Blend) {
        let Some(work) = &mut self.work else { return };
        let layer = &self.doc.layers[work.layer];
        let src: &Img = if work.mask { layer.mask.as_deref().unwrap_or(&layer.img) } else { &layer.img };
        let is_vec = layer.is_vector() && !work.mask;
        let alpha_lock = layer.alpha_lock && !is_vec && !work.mask;
        let w = self.doc.w;
        let buf = &self.buffer;
        let sel = if is_vec { None } else { self.sel.as_deref() };
        work.img.px.par_chunks_mut(w * 4).enumerate().skip(r.y as usize).take(r.h as usize).for_each(|(y, row)| {
            for x in r.x as usize..r.right() as usize {
                let i = (y * w + x) * 4;
                let o = x * 4;
                row[o..o + 4].copy_from_slice(&src.px[i..i + 4]);
                let ba = buf[i + 3] as f32 / 65535.0;
                if ba <= 0.0 {
                    continue;
                }
                let mut k = opacity;
                if let Some((g, sc, st)) = grain { k *= g.at(x as i32, y as i32, *sc, *st); }
                if let Some(m) = sel { k *= m[y * w + x] as f32 / 255.0; }
                if k <= 0.0 {
                    continue;
                }
                let sa = ba * k;
                match mode {
                    PaintMode::Erase => {
                        for c in 0..4 { row[o + c] = (row[o + c] as f32 * (1.0 - sa)).round() as u8; }
                    }
                    _ => {
                        let s = [buf[i] as f32 / 65535.0 * k, buf[i + 1] as f32 / 65535.0 * k, buf[i + 2] as f32 / 65535.0 * k, sa];
                        if alpha_lock {
                            let da = row[o + 3] as f32 / 255.0;
                            if da <= 0.0 { continue; }
                            let mut px = [row[o], row[o + 1], row[o + 2], row[o + 3]];
                            blend::composite_u8(&mut px, s, blend);
                            let na = px[3] as f32 / 255.0;
                            for c in 0..3 { row[o + c] = ((px[c] as f32 / na.max(1e-6)) * da).round().min(row[o + 3] as f32) as u8; }
                        } else if blend == Blend::Normal {
                            for c in 0..4 { row[o + c] = (s[c] * 255.0 + row[o + c] as f32 * (1.0 - sa)).round().clamp(0.0, 255.0) as u8; }
                        } else {
                            blend::composite_u8(&mut row[o..o + 4], s, blend);
                        }
                    }
                }
            }
        });
    }

    /// Обработка активного действия раз в кадр.
    fn flush_active(&mut self, sh: &mut Shared) {
        let mut paint: Option<(Rect, PaintMode, f32, Option<(Arc<Grain>, f32, f32)>, Blend)> = None;
        if let Some(Active::Paint { stroke, total, mode, opacity, grain, blend, .. }) = &mut self.active {
            if let Some(r) = stroke.dirty.take().and_then(|r| r.inflate(1).clip(self.doc.w, self.doc.h)) {
                *total = util::union_opt(*total, r);
                paint = Some((r, *mode, *opacity, grain.clone(), *blend));
            }
        }
        if let Some((r, mode, opacity, grain, blend)) = paint {
            if mode != PaintMode::Smudge { self.apply_buffer(r, mode, opacity, grain.as_ref(), blend); }
            self.invalidate(r);
        }
        let fill_pending = matches!(&self.active, Some(Active::Fill { pending: true, .. }));
        if fill_pending { self.fill_compute(); }
        let tool_pending = matches!(&self.active, Some(Active::Gradient { pending: true, .. }) | Some(Active::Shape { pending: true, .. }));
        if tool_pending { self.render_tool_preview(sh); }
        if let Some(f) = &mut self.filter {
            if f.dirty {
                f.dirty = false;
                let img = filters::apply(f.id, &f.src, &f.params);
                let out = if let Some(m) = &self.sel { mix_mask(&f.src, &img, m) } else { img };
                self.work = Some(Work { layer: f.layer, img: out, mask: f.mask });
                self.invalidate_all();
            }
        }
    }

    /// Записать рабочую копию в слой (прямоугольник r) с историей.
    fn commit_rect(&mut self, r: Rect, name: &str) {
        let Some(work) = self.work.take() else { return };
        let Some(r) = r.clip(self.doc.w, self.doc.h) else { self.invalidate_all(); return };
        let li = work.layer.min(self.doc.layers.len() - 1);
        let id = self.doc.layers[li].id.clone();
        let after = work.img.crop(r);
        if work.mask && self.doc.layers[li].mask.is_some() {
            let l = &mut self.doc.layers[li];
            let m = Arc::make_mut(l.mask.as_mut().unwrap());
            let before = m.crop(r);
            m.put(r.x, r.y, &after);
            l.ver += 1;
            self.hist.push(name, Op::Mask { layer: id, x: r.x, y: r.y, before, after });
        } else {
            let before = self.doc.layers[li].img.crop(r);
            self.doc.layers[li].img_mut().put(r.x, r.y, &after);
            self.hist.push(name, Op::Pixels { layer: id, x: r.x, y: r.y, before, after });
        }
        self.invalidate(r);
        self.mark_changed();
    }

    fn commit_work(&mut self, name: &str) {
        let f = self.doc.full();
        self.commit_rect(f, name);
    }

    /* ---------- пипетка ---------- */

    fn sample(&self, pos: Pos2, layer_only: bool) -> Option<[u8; 3]> {
        let (x, y) = self.view.to_doc(pos);
        if x < 0.0 || y < 0.0 || x >= self.doc.w as f32 || y >= self.doc.h as f32 { return None; }
        let img: &Img = if layer_only { &self.doc.layer().img } else { &self.comp };
        let i = img.idx(x as usize, y as usize);
        let a = img.px[i + 3] as f32 / 255.0;
        if a < 0.03 { return None; }
        let bg = self.doc.bg;
        Some([0, 1, 2].map(|k| (img.px[i + k] as f32 + bg[k] as f32 * (1.0 - a)).round().min(255.0) as u8))
    }

    fn pick_preview(&mut self, pos: Pos2, sh: &Shared) {
        let c = self.sample(pos, sh.prefs.pick_layer).unwrap_or(sh.colors.fg);
        self.loupe = Some((pos, c));
    }

    /* ---------- заливка ---------- */

    fn fill_begin(&mut self, x: f32, y: f32, sx: f32, sh: &Shared) {
        if x < 0.0 || y < 0.0 || x >= self.doc.w as f32 || y >= self.doc.h as f32 { return; }
        if !self.check_layer() || !self.raster_only() { return; }
        let src = if sh.prefs.fill_all_layers && !self.editing_mask() { doc::flatten(&self.doc, false) } else { (**self.target(self.doc.active)).clone() };
        self.begin_work();
        self.active = Some(Active::Fill { src, x: x as usize, y: y as usize, sx0: sx, base: sh.prefs.fill_tol, tol: sh.prefs.fill_tol, color: sh.colors.fg, prev: None, total: None, pending: true });
        self.fill_expand = sh.prefs.fill_expand;
    }

    pub fn fill_at(&mut self, pos: Pos2, sh: &mut Shared) {
        let (x, y) = self.view.to_doc(pos);
        self.fill_begin(x, y, pos.x, sh);
        if self.active.is_some() {
            self.fill_compute();
            self.active = None;
            self.commit_work("Заливка");
        }
    }

    fn fill_compute(&mut self) {
        let expand = self.fill_expand;
        let Some(Active::Fill { src, x, y, tol, color, prev, total, pending, .. }) = &mut self.active else { return };
        *pending = false;
        let (mask, b) = raster::flood(src, *x, *y, *tol, expand);
        let r = match prev { Some(p) => p.union(&b), None => b };
        *prev = Some(b);
        *total = util::union_opt(*total, b);
        let col = *color;
        let Some(work) = &mut self.work else { return };
        let layer = &self.doc.layers[work.layer];
        work.img.copy_rect_from(&layer.img, r);
        let alpha_lock = layer.alpha_lock;
        let w = self.doc.w;
        for yy in b.y as usize..b.bottom() as usize {
            for xx in b.x as usize..b.right() as usize {
                let mut m = mask[yy * w + xx] as f32 / 255.0;
                if let Some(s) = &self.sel { m *= s[yy * w + xx] as f32 / 255.0; }
                if m <= 0.0 { continue; }
                let i = (yy * w + xx) * 4;
                let px = &mut work.img.px[i..i + 4];
                let da = px[3] as f32 / 255.0;
                if alpha_lock {
                    for c in 0..3 { px[c] = (px[c] as f32 * (1.0 - m) + col[c] as f32 * da * m).round() as u8; }
                } else {
                    for c in 0..3 { px[c] = (col[c] as f32 * m + px[c] as f32 * (1.0 - m)).round() as u8; }
                    px[3] = (255.0 * m + px[3] as f32 * (1.0 - m)).round() as u8;
                }
            }
        }
        self.invalidate(r);
    }

    /* ---------- градиент и фигуры ---------- */

    pub fn grad_presets() -> Vec<&'static str> {
        let mut v = vec!["Основной в фоновый", "Основной в прозрачный", "Радуга"];
        v.extend(filters::GRADIENTS.iter().map(|g| g.0));
        v
    }

    fn grad_stops(&self, sh: &Shared) -> Vec<(f32, [f32; 4])> {
        let f = |c: [u8; 3], a: f32| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, a];
        let mut s: Vec<(f32, [f32; 4])> = match self.grad_colors {
            0 => vec![(0.0, f(sh.colors.fg, 1.0)), (1.0, f(sh.colors.bg, 1.0))],
            1 => vec![(0.0, f(sh.colors.fg, 1.0)), (1.0, f(sh.colors.fg, 0.0))],
            2 => (0..=6).map(|i| (i as f32 / 6.0, f(util::hsv2rgb(i as f32 * 60.0, 1.0, 1.0), 1.0))).collect(),
            n => filters::GRADIENTS[(n - 3).min(filters::GRADIENTS.len() - 1)].1.iter().map(|(t, c)| (*t, f(*c, 1.0))).collect(),
        };
        if self.grad_reverse { s = s.into_iter().rev().map(|(t, c)| (1.0 - t, c)).collect(); }
        s
    }

    fn render_tool_preview(&mut self, sh: &Shared) {
        let (w, h) = (self.doc.w, self.doc.h);
        let mut cov: Option<Vec<f32>> = None;
        let mut colors: Option<Vec<[f32; 4]>> = None;
        let fg = sh.colors.fg.map(|v| v as f32 / 255.0);
        let bgc = sh.colors.bg.map(|v| v as f32 / 255.0);
        let mut fill_col = [fg[0], fg[1], fg[2], 1.0];
        let mut opacity = 1.0;
        let grad_stops = self.grad_stops(sh);
        match &mut self.active {
            Some(Active::Gradient { x0, y0, x1, y1, pending, .. }) => {
                *pending = false;
                let stops = grad_stops;
                let (ax, ay, bx, by) = (*x0, *y0, *x1, *y1);
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = (dx * dx + dy * dy).max(1.0);
                let len = len2.sqrt();
                let kind = self.grad_kind;
                let a0 = dy.atan2(dx);
                let mut out = vec![[0f32; 4]; w * h];
                out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                    for x in 0..w {
                        let (px, py) = (x as f32 + 0.5 - ax, y as f32 + 0.5 - ay);
                        let t = match kind {
                            GradKind::Linear => (px * dx + py * dy) / len2,
                            GradKind::Reflected => ((px * dx + py * dy) / len2).abs(),
                            GradKind::Radial => (px * px + py * py).sqrt() / len,
                            GradKind::Angle => ((py.atan2(px) - a0).rem_euclid(std::f32::consts::TAU)) / std::f32::consts::TAU,
                        }.clamp(0.0, 1.0);
                        let mut k = 0;
                        while k + 2 < stops.len() && t > stops[k + 1].0 { k += 1; }
                        let (t0, c0) = stops[k];
                        let (t1, c1) = stops[(k + 1).min(stops.len() - 1)];
                        let u = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
                        row[x] = [0, 1, 2, 3].map(|c| c0[c] + (c1[c] - c0[c]) * u);
                    }
                });
                colors = Some(out);
                opacity = self.grad_opacity;
            }
            Some(Active::Shape { x0, y0, x1, y1, center, pending, .. }) => {
                *pending = false;
                let (mut ax, mut ay, bx, by) = (*x0, *y0, *x1, *y1);
                if *center && self.shape_kind != ShapeKind::Line { ax = *x0 - (bx - *x0); ay = *y0 - (by - *y0); }
                let (l, t, r, b) = (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by));
                let (cx, cy, rx, ry) = ((l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0);
                let sw = self.shape_width;
                let poly_for = |grow: f32| -> Vec<(f32, f32)> {
                    match self.shape_kind {
                        ShapeKind::Ellipse => raster::ellipse_poly(cx, cy, (rx + grow).max(0.0), (ry + grow).max(0.0)),
                        ShapeKind::Rect => vec![(l - grow, t - grow), (r + grow, t - grow), (r + grow, b + grow), (l - grow, b + grow)],
                        ShapeKind::Polygon | ShapeKind::Star => {
                            let star = self.shape_kind == ShapeKind::Star;
                            let n = if star { self.shape_sides * 2 } else { self.shape_sides };
                            (0..n).map(|i| {
                                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n as f32;
                                let k = if star && i % 2 == 1 { 0.45 } else { 1.0 };
                                (cx + a.cos() * ((rx + grow) * k).max(0.0), cy + a.sin() * ((ry + grow) * k).max(0.0))
                            }).collect()
                        }
                        ShapeKind::Line => vec![],
                    }
                };
                let mut c = vec![0f32; w * h];
                if self.shape_kind == ShapeKind::Line {
                    let r = Rect::from_f(ax.min(bx) - sw, ay.min(by) - sw, ax.max(bx) + sw, ay.max(by) + sw);
                    if let Some(r) = r.clip(w, h) {
                        for y in r.y as usize..r.bottom() as usize {
                            for x in r.x as usize..r.right() as usize {
                                let d = raster::seg_dist(x as f32 + 0.5, y as f32 + 0.5, ax, ay, bx, by);
                                c[y * w + x] = (sw / 2.0 - d + 0.5).clamp(0.0, 1.0);
                            }
                        }
                    }
                } else {
                    let stroke_on = self.shape_stroke || !self.shape_fill;
                    if self.shape_fill {
                        raster::fill_polys(w, h, &[poly_for(0.0)], false, |x, y, v| c[y * w + x] = v);
                    }
                    if stroke_on {
                        let outer = poly_for(sw / 2.0);
                        let mut inner = poly_for(-sw / 2.0);
                        inner.reverse();
                        let mut ring = vec![0f32; w * h];
                        raster::fill_polys(w, h, &[outer, inner], true, |x, y, v| ring[y * w + x] = v);
                        if self.shape_fill && self.shape_stroke {
                            let mut cols = vec![[0f32; 4]; w * h];
                            for i in 0..w * h {
                                let f = c[i];
                                let s = ring[i];
                                let a = s + f * (1.0 - s);
                                if a > 0.0 {
                                    for k in 0..3 { cols[i][k] = (bgc[k] * s + fg[k] * f * (1.0 - s)) / a; }
                                    cols[i][3] = a;
                                }
                            }
                            colors = Some(cols);
                        } else {
                            c = ring;
                        }
                    }
                }
                if colors.is_none() { cov = Some(c); }
                fill_col = [fg[0], fg[1], fg[2], 1.0];
            }
            _ => return,
        }
        let Some(work) = &mut self.work else { return };
        let layer = &self.doc.layers[work.layer];
        work.img.px.copy_from_slice(&layer.img.px);
        let alpha_lock = layer.alpha_lock;
        let sel = self.sel.as_deref();
        work.img.px.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                let (col, mut a) = match (&colors, &cov) {
                    (Some(cs), _) => { let c = cs[i]; ([c[0], c[1], c[2]], c[3]) }
                    (None, Some(cv)) => ([fill_col[0], fill_col[1], fill_col[2]], cv[i]),
                    _ => continue,
                };
                a *= opacity;
                if let Some(m) = sel { a *= m[i] as f32 / 255.0; }
                if a <= 0.0 { continue; }
                let o = x * 4;
                let da = row[o + 3] as f32 / 255.0;
                if alpha_lock {
                    if da <= 0.0 { continue; }
                    for c in 0..3 { row[o + c] = (row[o + c] as f32 * (1.0 - a) + col[c] * 255.0 * da * a).round() as u8; }
                } else {
                    for c in 0..3 { row[o + c] = (col[c] * 255.0 * a + row[o + c] as f32 * (1.0 - a)).round() as u8; }
                    row[o + 3] = (255.0 * a + row[o + 3] as f32 * (1.0 - a)).round() as u8;
                }
            }
        });
        self.invalidate_all();
    }

    /* ---------- выделение ---------- */

    fn wand_begin(&mut self, x: f32, y: f32, sx: f32, op: SelOp, sh: &Shared) {
        if x < 0.0 || y < 0.0 || x >= self.doc.w as f32 || y >= self.doc.h as f32 { return; }
        let src = if sh.prefs.fill_all_layers { doc::flatten(&self.doc, true) } else { (*self.doc.layer().img).clone() };
        self.active = Some(Active::Wand { src, x: x as usize, y: y as usize, sx0: sx, base: self.wand_tol, tol: self.wand_tol, op, preview: None });
        self.wand_compute();
    }

    fn wand_compute(&mut self) {
        if let Some(Active::Wand { src, x, y, tol, preview, .. }) = &mut self.active {
            let (m, b) = raster::flood(src, *x, *y, *tol, 0);
            *preview = Some((m, b));
        }
    }

    pub fn apply_selection(&mut self, new: Vec<u8>, op: SelOp, outline: Option<Vec<(f32, f32)>>, name: &str) {
        let old = self.sel.clone();
        let combined = match (&old, op) {
            (None, SelOp::Subtract) | (None, SelOp::Intersect) => None,
            (None, _) | (_, SelOp::Replace) => Some(new),
            (Some(o), SelOp::Add) => Some(o.iter().zip(&new).map(|(a, b)| (*a).max(*b)).collect()),
            (Some(o), SelOp::Subtract) => Some(o.iter().zip(&new).map(|(a, b)| ((*a as u32 * (255 - *b as u32)) / 255) as u8).collect()),
            (Some(o), SelOp::Intersect) => Some(o.iter().zip(&new).map(|(a, b)| (*a).min(*b)).collect()),
        };
        self.sel_outline = if op == SelOp::Replace || old.is_none() { outline } else { None };
        self.sel = combined;
        self.hist.push(name, Op::Sel { other: old });
        self.sel_dirty = true;
    }

    pub fn select_all(&mut self) {
        let (w, h) = (self.doc.w as f32, self.doc.h as f32);
        self.apply_selection(vec![255; self.doc.w * self.doc.h], SelOp::Replace, Some(vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]), "Выделить всё");
    }
    pub fn select_none(&mut self) {
        if self.sel.is_none() { return; }
        let old = self.sel.take();
        self.sel_outline = None;
        self.hist.push("Снять выделение", Op::Sel { other: old });
        self.sel_dirty = true;
    }
    pub fn select_invert(&mut self) {
        let Some(m) = &self.sel else { self.select_all(); return };
        let inv: Vec<u8> = m.iter().map(|v| 255 - v).collect();
        let outline = self.sel_outline.clone();
        self.apply_selection(inv, SelOp::Replace, outline, "Инвертировать выделение");
    }
    pub fn select_feather(&mut self, px: f32) {
        let Some(m) = &self.sel else { return };
        let (w, h) = (self.doc.w, self.doc.h);
        let mut img = Img::new(w, h);
        for (i, v) in m.iter().enumerate() { img.px[i * 4 + 3] = *v; }
        let b = raster::blur(&img, px);
        let nm: Vec<u8> = (0..w * h).map(|i| b.px[i * 4 + 3]).collect();
        self.apply_selection(nm, SelOp::Replace, None, "Растушёвка");
    }
    pub fn select_from_layer(&mut self) {
        self.select_layer_content(self.doc.active);
    }
    pub fn sel_bounds(&self) -> Option<Rect> {
        let m = self.sel.as_ref()?;
        let mut img = Img::new(1, 1);
        img.w = self.doc.w;
        img.h = self.doc.h;
        img.px = m.iter().flat_map(|v| [0, 0, 0, *v]).collect();
        img.content_bounds()
    }

    /* ---------- трансформация ---------- */

    pub fn xform_begin(&mut self) -> bool {
        if !self.check_layer() { return false; }
        let li = self.doc.active;
        let xmask = self.editing_mask();
        let orig = self.target(li).clone();
        let use_sel = self.sel.is_some() && (!self.doc.layers[li].is_vector() || xmask);
        let bounds = if use_sel { self.sel_bounds() } else { orig.content_bounds() };
        let Some(b) = bounds else {
            self.toast(if use_sel { "Выделение пустое" } else { "На слое нечего трансформировать" });
            return false;
        };
        let mut float = orig.crop(b);
        let mut hole = (*orig).clone();
        let mut mask_crop = None;
        if let Some(m) = self.sel.as_ref().filter(|_| use_sel) {
            let w = self.doc.w;
            for y in 0..b.h as usize {
                for x in 0..b.w as usize {
                    let k = m[(b.y as usize + y) * w + b.x as usize + x] as f32 / 255.0;
                    let i = (y * b.w as usize + x) * 4;
                    for c in 0..4 { float.px[i + c] = (float.px[i + c] as f32 * k).round() as u8; }
                }
            }
            for (i, v) in m.iter().enumerate() {
                let k = 1.0 - *v as f32 / 255.0;
                for c in 0..4 { hole.px[i * 4 + c] = (hole.px[i * 4 + c] as f32 * k).round() as u8; }
            }
            let mut mc = vec![0u8; (b.w * b.h) as usize];
            for y in 0..b.h as usize { for x in 0..b.w as usize { mc[y * b.w as usize + x] = m[(b.y as usize + y) * w + b.x as usize + x]; } }
            mask_crop = Some(mc);
        } else {
            hole.clear();
        }
        let st = XState { cx: b.x as f32 + b.w as f32 / 2.0, cy: b.y as f32 + b.h as f32 / 2.0, w: b.w as f32, h: b.h as f32, rot: 0.0 };
        self.xform = Some(Xform { layer: li, on_mask: xmask, float, hole, orig, mask: mask_crop, st, init: st, changed: false, smooth: true, uniform: false });
        self.xform_render();
        true
    }

    fn xform_matrix(x: &Xform) -> Affine {
        let (fw, fh) = (x.float.w as f32, x.float.h as f32);
        let (c, s) = (x.st.rot.cos(), x.st.rot.sin());
        let (sx, sy) = (x.st.w / fw, x.st.h / fh);
        let (a, b, cc, d) = (c * sx, s * sx, -s * sy, c * sy);
        Affine { a, b, c: cc, d, e: x.st.cx - (a * fw / 2.0 + cc * fh / 2.0), f: x.st.cy - (b * fw / 2.0 + d * fh / 2.0) }
    }

    pub fn xform_render(&mut self) {
        let Some(x) = &self.xform else { return };
        let mut img = x.hole.clone();
        raster::draw_affine(&mut img, &x.float, Self::xform_matrix(x), x.smooth);
        self.work = Some(Work { layer: x.layer, img, mask: x.on_mask });
        self.invalidate_all();
    }

    fn xform_down(&mut self, pos: Pos2) {
        if self.xform.is_none() && !self.xform_begin() { return; }
        let v = self.view;
        let Some(x) = &self.xform else { return };
        let corners = xcorners(&x.st);
        let scr: Vec<Pos2> = corners.iter().map(|&(a, b)| v.to_screen(a, b)).collect();
        let mut hit = None;
        let mut best = 26.0;
        for (i, q) in scr.iter().enumerate() {
            let d = (*q - pos).length();
            if d < best { best = d; hit = Some(Handle::Corner(i)); }
            if !x.uniform {
                let n = scr[(i + 1) % 4];
                let mid = Pos2::new((q.x + n.x) / 2.0, (q.y + n.y) / 2.0);
                let d = (mid - pos).length();
                if d < best { best = d; hit = Some(Handle::Edge(i)); }
            }
        }
        if (rot_handle(&x.st, &v) - pos).length() < best { hit = Some(Handle::Rotate); }
        let inside = point_in_poly(pos, &scr);
        let h = hit.unwrap_or(if inside { Handle::Move } else { Handle::Rotate });
        let (px, py) = v.to_doc(pos);
        let anchor = match h {
            Handle::Corner(i) => corners[(i + 2) % 4],
            Handle::Edge(i) => { let o = (i + 2) % 4; let (a, b) = (corners[o], corners[(o + 1) % 4]); ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0) }
            _ => (0.0, 0.0),
        };
        let a0 = (py - x.st.cy).atan2(px - x.st.cx);
        self.active = Some(Active::XDrag { h, p0: (px, py), snap: x.st, anchor, a0 });
    }

    fn xform_move(&mut self, px: f32, py: f32, shift: bool) {
        let Some(Active::XDrag { h, p0, snap, anchor, a0 }) = &self.active else { return };
        let (h, p0, s, an, a0) = (*h, *p0, *snap, *anchor, *a0);
        let Some(x) = &mut self.xform else { return };
        let (c, sn) = (s.rot.cos(), s.rot.sin());
        let (ux, uy) = ((c, sn), (-sn, c));
        match h {
            Handle::Move => { x.st.cx = s.cx + px - p0.0; x.st.cy = s.cy + py - p0.1; }
            Handle::Rotate => {
                let mut r = s.rot + (py - s.cy).atan2(px - s.cx) - a0;
                let q = (r / (std::f32::consts::PI / 12.0)).round() * std::f32::consts::PI / 12.0;
                if shift || (r - q).abs() < 0.035 { r = q; }
                x.st.rot = r;
            }
            Handle::Corner(i) => {
                let sg = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i];
                let (vx, vy) = (px - an.0, py - an.1);
                let (lx, ly) = (vx * ux.0 + vy * ux.1, vx * uy.0 + vy * uy.1);
                let (mut w, mut hh);
                if x.uniform || shift {
                    let (ox, oy) = (sg.0 * s.w, sg.1 * s.h);
                    let k = (lx * ox + ly * oy) / (ox * ox + oy * oy).max(1e-6);
                    w = s.w * k; hh = s.h * k;
                } else { w = lx / sg.0; hh = ly / sg.1; }
                if w.abs() < 1.0 { w = w.signum().max(0.0) * 2.0 - 1.0; }
                if hh.abs() < 1.0 { hh = hh.signum().max(0.0) * 2.0 - 1.0; }
                x.st.w = w; x.st.h = hh;
                let (hx, hy) = (sg.0 * w / 2.0, sg.1 * hh / 2.0);
                x.st.cx = an.0 + hx * c - hy * sn;
                x.st.cy = an.1 + hx * sn + hy * c;
            }
            Handle::Edge(i) => {
                let horiz = i == 1 || i == 3;
                let sgn = if i == 1 || i == 2 { 1.0 } else { -1.0 };
                let ax = if horiz { ux } else { uy };
                let mut v = ((px - an.0) * ax.0 + (py - an.1) * ax.1) * sgn;
                if v.abs() < 1.0 { v = 1.0; }
                if horiz { x.st.w = v; } else { x.st.h = v; }
                x.st.cx = an.0 + ax.0 * sgn * v / 2.0;
                x.st.cy = an.1 + ax.1 * sgn * v / 2.0;
            }
        }
        x.changed = true;
        self.xform_render();
    }

    pub fn xform_act(&mut self, kind: &str) {
        let (dw, dh) = (self.doc.w as f32, self.doc.h as f32);
        let Some(x) = &mut self.xform else { return };
        match kind {
            "flipH" => x.st.w = -x.st.w,
            "flipV" => x.st.h = -x.st.h,
            "rot45" => x.st.rot += std::f32::consts::FRAC_PI_4,
            "fit" => {
                let k = (dw / x.st.w.abs()).min(dh / x.st.h.abs());
                x.st.w *= k; x.st.h *= k; x.st.rot = 0.0; x.st.cx = dw / 2.0; x.st.cy = dh / 2.0;
            }
            "reset" => x.st = x.init,
            "smooth" => x.smooth = !x.smooth,
            "uniform" => x.uniform = !x.uniform,
            _ => {}
        }
        x.changed = true;
        self.xform_render();
    }

    pub fn xform_commit(&mut self) {
        let Some(x) = self.xform.take() else { return };
        self.active = None;
        if !x.changed { self.work = None; self.invalidate_all(); return; }
        let m = Self::xform_matrix(&x);
        if let Some(list) = self.doc.layers.get(x.layer).and_then(|l| l.vector.clone()).filter(|_| !x.on_mask) {
            // Векторный слой: преобразуем сами линии — без потери качества.
            let (bx, by) = (x.init.cx - x.init.w / 2.0, x.init.cy - x.init.h / 2.0);
            let md = Affine { e: m.e - (m.a * bx + m.c * by), f: m.f - (m.b * bx + m.d * by), ..m };
            let nl = vector::transform(&list, md);
            self.work = Some(Work { layer: x.layer, img: vector::render_full(self.doc.w, self.doc.h, &nl), mask: false });
            let id = self.doc.layers[x.layer].id.clone();
            self.hist.begin_group();
            self.commit_work("Трансформация");
            let after = Some(Arc::new(nl));
            self.doc.layers[x.layer].vector = after.clone();
            self.hist.push("Трансформация", Op::Vector { layer: id, before: Some(list), after });
            self.hist.end_group("Трансформация");
            return;
        }
        self.hist.begin_group();
        self.commit_work("Трансформация");
        if let Some(mc) = &x.mask {
            let (bw, bh) = (x.float.w, x.float.h);
            let mut mimg = Img::new(bw, bh);
            for (i, v) in mc.iter().enumerate() { mimg.px[i * 4 + 3] = *v; mimg.px[i * 4] = *v; mimg.px[i * 4 + 1] = *v; mimg.px[i * 4 + 2] = *v; }
            let mut dst = Img::new(self.doc.w, self.doc.h);
            raster::draw_affine(&mut dst, &mimg, m, true);
            let nm: Vec<u8> = dst.px.chunks_exact(4).map(|p| p[3]).collect();
            self.apply_selection(nm, SelOp::Replace, None, "Трансформация выделения");
        }
        self.hist.end_group("Трансформация");
        let _ = x.orig;
    }

    pub fn xform_cancel(&mut self) {
        if self.xform.take().is_some() {
            self.active = None;
            self.work = None;
            self.invalidate_all();
        }
    }

    /* ---------- фильтры ---------- */

    pub fn filter_open(&mut self, id: &'static str) {
        self.commit_pending();
        if !self.check_layer() || !self.raster_only() { return; }
        let f = filters::find(id);
        let li = self.doc.active;
        let mask = self.editing_mask();
        self.filter = Some(FilterSession { id: f.id, mask, params: filters::defaults(f), layer: li, src: self.target(li).clone(), dirty: true });
    }
    pub fn filter_apply(&mut self) {
        if let Some(f) = self.filter.take() {
            if f.dirty {
                let img = filters::apply(f.id, &f.src, &f.params);
                let out = if let Some(m) = &self.sel { mix_mask(&f.src, &img, m) } else { img };
                self.work = Some(Work { layer: f.layer, img: out, mask: f.mask });
            }
            self.commit_work(filters::find(f.id).name);
        }
    }
    pub fn filter_cancel(&mut self) {
        if self.filter.take().is_some() { self.work = None; self.invalidate_all(); }
    }

    /* ---------- общие действия ---------- */

    pub fn commit_pending(&mut self) {
        if self.xform.is_some() { self.xform_commit(); }
        if self.filter.is_some() { self.filter_apply(); }
    }

    pub fn set_tool(&mut self, t: Tool) {
        if t == self.tool { return; }
        self.cancel();
        if self.xform.is_some() { self.xform_commit(); }
        if self.tool != Tool::Picker { self.prev_tool = self.tool; }
        self.tool = t;
        if t == Tool::Transform && !self.xform_begin() { self.tool = self.prev_tool; }
    }

    pub fn undo(&mut self) {
        if self.active.is_some() { return; }
        if self.xform.is_some() { self.xform_cancel(); self.toast("Трансформация отменена"); return; }
        if self.filter.is_some() { self.filter_cancel(); return; }
        match self.hist.undo(&mut self.doc, &mut self.sel) {
            Some(n) => { self.toast(&format!("Отменено: {n}")); self.after_history(); }
            None => self.toast("Нечего отменять"),
        }
    }
    pub fn redo(&mut self) {
        if self.active.is_some() || self.xform.is_some() { return; }
        match self.hist.redo(&mut self.doc, &mut self.sel) {
            Some(n) => { self.toast(&format!("Повторено: {n}")); self.after_history(); }
            None => self.toast("Нечего повторять"),
        }
    }
    pub fn history_goto(&mut self, i: usize) {
        if self.active.is_some() { return; }
        self.commit_pending();
        while self.hist.pos > i { if self.hist.undo(&mut self.doc, &mut self.sel).is_none() { break; } }
        while self.hist.pos < i { if self.hist.redo(&mut self.doc, &mut self.sel).is_none() { break; } }
        self.after_history();
    }
    fn after_history(&mut self) {
        if self.comp.w != self.doc.w || self.comp.h != self.doc.h {
            self.comp = Img::new(self.doc.w, self.doc.h);
            self.buffer.clear();
            self.fit();
        }
        self.sel_outline = None;
        self.sel_dirty = true;
        for l in self.doc.layers.iter_mut() { l.ver += 1; }
        self.after_tree();
        self.invalidate_all();
        self.mark_changed();
    }

    /* ---------- слои ---------- */

    pub fn layer_insert(&mut self, l: Layer, at: usize, name: &str) {
        if !l.group && self.doc.layers.iter().filter(|x| !x.group).count() >= self.doc.max_layers() { self.toast("Достигнут предел слоёв для этого размера холста"); return; }
        let prev = self.doc.active;
        let at = at.min(self.doc.layers.len());
        self.doc.layers.insert(at, l);
        self.doc.active = at;
        self.hist.push(name, Op::Insert { index: at, layer: None, prev_active: prev });
        self.invalidate_all();
        self.mark_changed();
    }

    /// Куда вставлять новый слой: над активным (в той же папке) или в начало раскрытой выбранной папки.
    fn insert_pos(&self) -> (usize, Option<String>) {
        let a = self.doc.active.min(self.doc.layers.len().saturating_sub(1));
        match self.doc.layers.get(a) {
            Some(l) if l.group && !l.collapsed => (a, Some(l.id.clone())),
            Some(l) => (a + 1, l.parent.clone()),
            None => (0, None),
        }
    }

    /// Добавить новый слой над активным.
    pub fn add_layer(&mut self, mut l: Layer, name: &str) {
        let (at, parent) = self.insert_pos();
        l.parent = parent;
        self.multi.clear();
        self.mask_edit = false;
        self.layer_insert(l, at, name);
    }

    pub fn layer_add(&mut self) {
        self.commit_pending();
        let n = self.doc.next_name();
        let l = Layer::new(self.doc.w, self.doc.h, &n);
        self.add_layer(l, "Новый слой");
    }

    pub fn layer_add_folder(&mut self) {
        self.commit_pending();
        let n = self.doc.next_name().replace("Слой", "Папка");
        self.add_layer(Layer::new_group(&n), "Новая папка");
    }

    pub fn layer_add_image(&mut self, img: Img, name: &str) {
        self.commit_pending();
        let mut l = Layer::new(self.doc.w, self.doc.h, name);
        let k = (self.doc.w as f32 / img.w as f32).min(self.doc.h as f32 / img.h as f32).min(1.0);
        let im = if k < 1.0 { img.resize(((img.w as f32 * k) as usize).max(1), ((img.h as f32 * k) as usize).max(1)) } else { img };
        let (x, y) = ((self.doc.w as i32 - im.w as i32) / 2, (self.doc.h as i32 - im.h as i32) / 2);
        l.img_mut().put(x, y, &im);
        self.add_layer(l, "Вставить изображение");
    }

    /// Строки панели слоёв сверху вниз (без содержимого свёрнутых папок).
    pub fn visible_rows(&self) -> Vec<usize> {
        let ls = &self.doc.layers;
        (0..ls.len()).rev().filter(|&i| {
            let mut p = ls[i].parent.clone();
            let mut g = 0;
            while let Some(id) = p {
                g += 1;
                if g > 64 { break; }
                match ls.iter().find(|l| l.id == id) { Some(x) => { if x.collapsed { return false; } p = x.parent.clone(); } None => break }
            }
            true
        }).collect()
    }

    /// Выбрать соседнюю строку панели слоёв.
    pub fn layer_select_step(&mut self, up: bool) {
        let rows = self.visible_rows();
        let Some(k) = rows.iter().position(|&i| i == self.doc.active) else { return };
        let n = if up { k.checked_sub(1) } else { (k + 1 < rows.len()).then_some(k + 1) };
        if let Some(n) = n { self.layer_select(rows[n]); }
    }

    /// Выбранные слои (активный и отмеченные), снизу вверх; содержимое выбранных папок не дублируется.
    pub fn selected(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.doc.layers.iter().enumerate().filter(|(i, l)| *i == self.doc.active || self.multi.contains(&l.id)).map(|(i, _)| i).collect();
        let ids: Vec<String> = v.iter().map(|&i| self.doc.layers[i].id.clone()).collect();
        v.retain(|&i| !ids.iter().any(|g| doc::is_inside(&self.doc.layers, i, g)));
        v
    }

    /// Изменение структуры слоёв одним шагом истории.
    fn tree_edit(&mut self, name: &str, f: impl FnOnce(&mut Vec<Layer>, &mut usize) -> Result<(), String>) {
        self.commit_pending();
        let old = self.doc.layers.clone();
        let old_active = self.doc.active;
        let mut layers = self.doc.layers.clone();
        let mut active = self.doc.active;
        if let Err(e) = f(&mut layers, &mut active) {
            if !e.is_empty() { self.toast(&e); }
            return;
        }
        if layers.is_empty() {
            let n = self.doc.next_name();
            layers.push(Layer::new(self.doc.w, self.doc.h, &n));
        }
        doc::normalize(&mut layers);
        self.doc.layers = layers;
        self.doc.active = active.min(self.doc.layers.len() - 1);
        self.hist.push(name, Op::Tree { other: old, active: old_active });
        self.after_tree();
        self.mark_changed();
    }

    fn after_tree(&mut self) {
        if self.doc.layer().mask.is_none() { self.mask_edit = false; }
        let ids: Vec<String> = self.doc.layers.iter().map(|l| l.id.clone()).collect();
        let act = self.doc.layer().id.clone();
        self.multi.retain(|m| ids.contains(m) && *m != act);
        self.invalidate_all();
    }

    pub fn layer_duplicate(&mut self) {
        let sel = self.selected();
        let extra: usize = sel.iter().map(|&i| doc::block(&self.doc.layers, i).filter(|&k| !self.doc.layers[k].group).count()).sum();
        if self.doc.layers.iter().filter(|x| !x.group).count() + extra > self.doc.max_layers() { self.toast("Достигнут предел слоёв для этого размера холста"); return; }
        self.tree_edit(if sel.len() > 1 { "Дублировать слои" } else { "Дублировать слой" }, |layers, active| {
            let mut last = 0;
            // сверху вниз: вставка выше блока не сдвигает индексы ниже
            for &i0 in sel.iter().rev() {
                let r = doc::block(layers, i0);
                let mut copy: Vec<Layer> = layers[r.clone()].to_vec();
                let mut map: HashMap<String, String> = HashMap::new();
                for l in copy.iter_mut() { let nid = util::uid(); map.insert(l.id.clone(), nid.clone()); l.id = nid; l.ver += 1; }
                for l in copy.iter_mut() { if let Some(p) = l.parent.clone() { if let Some(np) = map.get(&p) { l.parent = Some(np.clone()); } } }
                if let Some(root) = copy.last_mut() { root.name = format!("{} (копия)", root.name); }
                let at = r.end;
                let n = copy.len();
                for (k, l) in copy.into_iter().enumerate() { layers.insert(at + k, l); }
                last = last.max(at + n - 1);
            }
            *active = last;
            Ok(())
        });
        self.multi.clear();
    }

    pub fn layer_delete(&mut self) {
        let sel = self.selected();
        self.tree_edit(if sel.len() > 1 { "Удалить слои" } else { "Удалить слой" }, |layers, active| {
            let ranges: Vec<std::ops::Range<usize>> = sel.iter().map(|&i| doc::block(layers, i)).collect();
            let low = ranges.iter().map(|r| r.start).min().unwrap_or(0);
            for r in ranges.iter().rev() { layers.drain(r.clone()); }
            *active = low.saturating_sub(1).min(layers.len().saturating_sub(1));
            Ok(())
        });
        self.multi.clear();
    }

    /// Перетащить слой (или папку целиком) к другому слою.
    pub fn layer_move_to(&mut self, id: &str, target: Drop) {
        let Some(from) = self.doc.find(id) else { return };
        let moving = id.to_string();
        self.tree_edit("Переместить слой", |layers, active| {
            let r = doc::block(layers, from);
            let tid = match &target { Drop::Above(t) | Drop::Below(t) | Drop::Into(t) => t.clone() };
            if tid == moving || r.clone().any(|k| layers[k].id == tid) { return Err(String::new()); }
            let moved: Vec<Layer> = layers.drain(r).collect();
            let Some(t) = layers.iter().position(|l| l.id == tid) else { return Err(String::new()) };
            let (at, parent) = match &target {
                Drop::Above(_) => (t + 1, layers[t].parent.clone()),
                Drop::Below(_) => (if layers[t].group { doc::group_start(layers, t) } else { t }, layers[t].parent.clone()),
                Drop::Into(_) => {
                    if !layers[t].group { return Err(String::new()); }
                    layers[t].collapsed = false;
                    (t, Some(tid.clone()))
                }
            };
            let n = moved.len();
            for (k, mut l) in moved.into_iter().enumerate() {
                if k == n - 1 { l.parent = parent.clone(); }
                layers.insert(at + k, l);
            }
            *active = at + n - 1;
            Ok(())
        });
    }

    /// Сдвинуть активный слой на одну позицию вверх или вниз (папки перепрыгиваются целиком, из папки — выход).
    pub fn layer_step(&mut self, up: bool) {
        let layers = &self.doc.layers;
        let i = self.doc.active;
        let r = doc::block(layers, i);
        let my_parent = layers[i].parent.clone();
        let id = layers[i].id.clone();
        let target = if up {
            if r.end >= layers.len() { return; }
            let mut cand = r.end;
            loop {
                if Some(&layers[cand].id) == my_parent.as_ref() || layers[cand].parent == my_parent { break Some(Drop::Above(layers[cand].id.clone())); }
                match layers[cand].parent.as_ref().and_then(|p| layers.iter().position(|l| &l.id == p)) {
                    Some(pi) => cand = pi,
                    None => break None,
                }
            }
        } else {
            if r.start == 0 { return; }
            let k = r.start - 1;
            if layers[k].parent == my_parent { Some(Drop::Below(layers[k].id.clone())) } else { my_parent.clone().map(Drop::Below) }
        };
        if let Some(t) = target { self.layer_move_to(&id, t); }
    }

    /// Совместимость: переместить слой с позиции from на позицию to (соседние слои).
    pub fn layer_move(&mut self, from: usize, to: usize) {
        if from >= self.doc.layers.len() || to >= self.doc.layers.len() || from == to { return; }
        self.doc.active = from;
        self.layer_step(to > from);
    }

    /// Положить выбранные слои в новую папку (Ctrl+G).
    pub fn group_selected(&mut self) {
        let sel = self.selected();
        let name = self.doc.next_name().replace("Слой", "Папка");
        self.tree_edit("Объединить в папку", |layers, active| {
            if sel.is_empty() { return Err(String::new()); }
            let ranges: Vec<std::ops::Range<usize>> = sel.iter().map(|&i| doc::block(layers, i)).collect();
            let top = *sel.last().unwrap();
            let mut g = Layer::new_group(&name);
            g.parent = layers[top].parent.clone();
            let gid = g.id.clone();
            let removed_below: usize = ranges[..ranges.len() - 1].iter().map(|r| r.len()).sum();
            let at = ranges.last().unwrap().start - removed_below;
            let mut blocks: Vec<Vec<Layer>> = vec![];
            for r in ranges.iter().rev() {
                let mut b: Vec<Layer> = layers.drain(r.clone()).collect();
                if let Some(root) = b.last_mut() { root.parent = Some(gid.clone()); }
                blocks.push(b);
            }
            blocks.reverse();
            let mut k = at;
            for b in blocks { for l in b { layers.insert(k, l); k += 1; } }
            layers.insert(k, g);
            *active = k;
            Ok(())
        });
        self.multi.clear();
    }

    /// Расформировать папку: содержимое остаётся на месте.
    pub fn ungroup(&mut self) {
        let i = self.doc.active;
        if !self.doc.layers[i].group { self.toast("Выберите папку"); return; }
        self.tree_edit("Расформировать папку", |layers, active| {
            let g = layers.remove(i);
            for l in layers.iter_mut() { if l.parent.as_deref() == Some(g.id.as_str()) { l.parent = g.parent.clone(); } }
            *active = i.saturating_sub(1).min(layers.len().saturating_sub(1));
            Ok(())
        });
    }

    pub fn layer_select(&mut self, i: usize) {
        if i < self.doc.layers.len() {
            if i != self.doc.active { self.commit_pending(); }
            self.doc.active = i;
            self.multi.clear();
            self.mask_edit = false;
        }
    }

    /// Ctrl+щелчок: добавить слой к выбранным или убрать из них.
    pub fn layer_select_toggle(&mut self, i: usize) {
        if i >= self.doc.layers.len() { return; }
        self.commit_pending();
        let id = self.doc.layers[i].id.clone();
        if i == self.doc.active {
            if let Some(next) = self.multi.pop() { if let Some(k) = self.doc.find(&next) { self.doc.active = k; } }
        } else if let Some(p) = self.multi.iter().position(|m| *m == id) {
            self.multi.remove(p);
        } else {
            let a = self.doc.layer().id.clone();
            self.multi.push(a);
            self.doc.active = i;
        }
        self.mask_edit = false;
    }

    /// Shift+щелчок: выбрать диапазон.
    pub fn layer_select_range(&mut self, i: usize) {
        if i >= self.doc.layers.len() { return; }
        self.commit_pending();
        let (a, b) = (self.doc.active.min(i), self.doc.active.max(i));
        self.multi = (a..=b).filter(|&k| k != i).map(|k| self.doc.layers[k].id.clone()).collect();
        self.doc.active = i;
        self.mask_edit = false;
    }

    /// Выбрать маску слоя для рисования.
    pub fn layer_select_mask(&mut self, i: usize) {
        self.layer_select(i);
        self.mask_edit = self.doc.layer().mask.is_some();
    }

    /// Alt+щелчок по глазу: показать только этот слой; повторно — показать все.
    pub fn layer_solo(&mut self, i: usize) {
        let n = self.doc.layers.len();
        if i >= n { return; }
        let id = self.doc.layers[i].id.clone();
        let keep: Vec<bool> = (0..n).map(|k| k == i || doc::is_inside(&self.doc.layers, k, &id) || doc::is_inside(&self.doc.layers, i, &self.doc.layers[k].id)).collect();
        let others_hidden = (0..n).filter(|&k| !keep[k] && !self.doc.layers[k].group).all(|k| !self.doc.layers[k].visible);
        self.hist.begin_group();
        for k in 0..n {
            if keep[k] { self.layer_props(k, |l| l.visible = true, "Видимость"); }
            else if !self.doc.layers[k].group { self.layer_props(k, |l| l.visible = others_hidden, "Видимость"); }
        }
        self.hist.end_group(if others_hidden { "Показать все слои" } else { "Показать только этот слой" });
    }

    /// Выделить по содержимому слоя или папки (Ctrl+щелчок по миниатюре).
    pub fn select_layer_content(&mut self, i: usize) {
        if i >= self.doc.layers.len() { return; }
        let m: Vec<u8> = if self.doc.layers[i].group {
            let idx: Vec<usize> = doc::block(&self.doc.layers, i).collect();
            doc::flatten_some(&self.doc, &idx).px.chunks_exact(4).map(|p| p[3]).collect()
        } else {
            let img: &Img = if i == self.doc.active { self.target(i) } else { &self.doc.layers[i].img };
            img.px.chunks_exact(4).map(|p| p[3]).collect()
        };
        self.apply_selection(m, SelOp::Replace, None, "Выделить по слою");
    }

    /* ---------- маски слоя ---------- */

    pub fn mask_add(&mut self) {
        self.commit_pending();
        let li = self.doc.active;
        if self.doc.layers[li].mask.is_some() { self.mask_edit = true; self.toast("У слоя уже есть маска — она выбрана для рисования"); return; }
        let (w, h) = (self.doc.w, self.doc.h);
        let mut m = Img::new(w, h);
        match &self.sel {
            Some(s) => for (i, v) in s.iter().enumerate() { m.px[i * 4..i * 4 + 4].copy_from_slice(&[*v, *v, *v, *v]); },
            None => m.px.fill(255),
        }
        let id = self.doc.layers[li].id.clone();
        let after = Some(Arc::new(m));
        self.doc.layers[li].mask = after.clone();
        self.doc.layers[li].mask_on = true;
        self.doc.layers[li].ver += 1;
        self.hist.push("Маска слоя", Op::MaskSet { layer: id, before: None, after });
        self.mask_edit = true;
        self.invalidate_all();
        self.mark_changed();
        self.toast("Маска слоя: ластик скрывает, кисть возвращает");
    }

    pub fn mask_delete(&mut self) {
        self.commit_pending();
        let li = self.doc.active;
        let Some(before) = self.doc.layers[li].mask.take() else { return };
        self.doc.layers[li].ver += 1;
        let id = self.doc.layers[li].id.clone();
        self.hist.push("Удалить маску", Op::MaskSet { layer: id, before: Some(before), after: None });
        self.mask_edit = false;
        self.invalidate_all();
        self.mark_changed();
    }

    /// Применить маску: скрытое удаляется из пикселей слоя.
    pub fn mask_apply(&mut self) {
        self.commit_pending();
        let li = self.doc.active;
        if self.doc.layers[li].group { self.toast("Маску папки можно применить, объединив папку"); return; }
        let Some(m) = self.doc.layers[li].mask.clone() else { return };
        let before = (*self.doc.layers[li].img).clone();
        let mut after = before.clone();
        if self.doc.layers[li].mask_on {
            for (p, q) in after.px.chunks_exact_mut(4).zip(m.px.chunks_exact(4)) {
                let k = q[3] as f32 / 255.0;
                for c in p.iter_mut() { *c = (*c as f32 * k).round() as u8; }
            }
        }
        let id = self.doc.layers[li].id.clone();
        self.hist.begin_group();
        self.doc.layers[li].img = Arc::new(after.clone());
        self.hist.push("Применить маску", Op::Pixels { layer: id.clone(), x: 0, y: 0, before, after });
        self.doc.layers[li].mask = None;
        self.hist.push("Применить маску", Op::MaskSet { layer: id.clone(), before: Some(m), after: None });
        if let Some(v) = self.doc.layers[li].vector.take() { self.hist.push("Применить маску", Op::Vector { layer: id, before: Some(v), after: None }); }
        self.hist.end_group("Применить маску");
        self.doc.layers[li].ver += 1;
        self.mask_edit = false;
        self.invalidate_all();
        self.mark_changed();
    }

    pub fn mask_invert(&mut self) {
        self.commit_pending();
        let li = self.doc.active;
        let Some(m) = self.doc.layers[li].mask.clone() else { return };
        let before = (*m).clone();
        let mut after = before.clone();
        for p in after.px.chunks_exact_mut(4) { let v = 255 - p[3]; p.copy_from_slice(&[v, v, v, v]); }
        let id = self.doc.layers[li].id.clone();
        self.doc.layers[li].mask = Some(Arc::new(after.clone()));
        self.doc.layers[li].ver += 1;
        self.hist.push("Инвертировать маску", Op::Mask { layer: id, x: 0, y: 0, before, after });
        self.invalidate_all();
        self.mark_changed();
    }
    pub fn layer_props(&mut self, i: usize, f: impl FnOnce(&mut Layer), name: &str) {
        let Some(l) = self.doc.layers.get_mut(i) else { return };
        let before = l.props();
        f(l);
        let after = l.props();
        if before == after { return; }
        l.ver += 1;
        let id = l.id.clone();
        self.hist.push(name, Op::Props { layer: id, before, after });
        self.invalidate_all();
        self.mark_changed();
    }
    /// Изменение свойства «на лету» (ползунок) без записи в историю; потом — layer_props_commit.
    pub fn layer_props_live(&mut self, i: usize, f: impl FnOnce(&mut Layer)) {
        if let Some(l) = self.doc.layers.get_mut(i) { f(l); }
        self.invalidate_all();
    }
    pub fn layer_props_commit(&mut self, i: usize, before: doc::Props, name: &str) {
        let Some(l) = self.doc.layers.get(i) else { return };
        let after = l.props();
        if before != after {
            let id = l.id.clone();
            self.hist.push(name, Op::Props { layer: id, before, after });
            self.mark_changed();
        }
    }
    fn layer_pixels(&mut self, name: &str, f: impl FnOnce(&mut Img, Option<&[u8]>)) {
        if !self.check_layer() { return; }
        let li = self.doc.active;
        let sel = self.sel.clone();
        let id = self.doc.layers[li].id.clone();
        if self.editing_mask() {
            let l = &mut self.doc.layers[li];
            let m = Arc::make_mut(l.mask.as_mut().unwrap());
            let before = m.clone();
            f(m, sel.as_deref());
            let after = m.clone();
            l.ver += 1;
            self.hist.push(name, Op::Mask { layer: id, x: 0, y: 0, before, after });
        } else {
            let before = (*self.doc.layers[li].img).clone();
            f(self.doc.layers[li].img_mut(), sel.as_deref());
            let after = (*self.doc.layers[li].img).clone();
            self.hist.push(name, Op::Pixels { layer: id, x: 0, y: 0, before, after });
        }
        self.invalidate_all();
        self.mark_changed();
    }
    pub fn layer_clear(&mut self) {
        self.commit_pending();
        if self.doc.layer().is_vector() && !self.editing_mask() {
            if self.doc.layer().locked { self.toast("Слой заблокирован"); return; }
            let (w, h) = (self.doc.w, self.doc.h);
            let sel = self.sel.clone();
            self.vector_replace("Очистить", |l| match &sel {
                Some(m) => { let hit = vector::touched_by_mask(l, m, w, h); l.iter().zip(hit).filter(|(_, t)| !*t).map(|(s, _)| s.clone()).collect() }
                None => vec![],
            });
            return;
        }
        self.layer_pixels("Очистить", |img, sel| match sel {
            Some(m) => for (i, v) in m.iter().enumerate() { let k = 1.0 - *v as f32 / 255.0; for c in 0..4 { img.px[i * 4 + c] = (img.px[i * 4 + c] as f32 * k).round() as u8; } },
            None => img.clear(),
        });
    }
    pub fn layer_fill(&mut self, col: [u8; 3]) {
        self.commit_pending();
        if !self.raster_only() { return; }
        let alpha_lock = self.doc.layer().alpha_lock;
        self.layer_pixels("Залить", |img, sel| {
            for i in 0..img.w * img.h {
                let m = sel.map_or(1.0, |m| m[i] as f32 / 255.0);
                if m <= 0.0 { continue; }
                let p = &mut img.px[i * 4..i * 4 + 4];
                let da = p[3] as f32 / 255.0;
                if alpha_lock {
                    for c in 0..3 { p[c] = (p[c] as f32 * (1.0 - m) + col[c] as f32 * da * m).round() as u8; }
                } else {
                    for c in 0..3 { p[c] = (col[c] as f32 * m + p[c] as f32 * (1.0 - m)).round() as u8; }
                    p[3] = (255.0 * m + p[3] as f32 * (1.0 - m)).round() as u8;
                }
            }
        });
    }
    pub fn layer_flip(&mut self, horiz: bool) {
        self.commit_pending();
        if self.doc.layer().is_vector() && !self.editing_mask() {
            let (w, h) = (self.doc.w as f32, self.doc.h as f32);
            let m = if horiz { Affine { a: -1.0, b: 0.0, c: 0.0, d: 1.0, e: w, f: 0.0 } } else { Affine { a: 1.0, b: 0.0, c: 0.0, d: -1.0, e: 0.0, f: h } };
            self.vector_replace("Отразить слой", |l| vector::transform(l, m));
            return;
        }
        self.layer_pixels("Отразить слой", |img, _| img.flip(horiz));
    }
    /// Нижний соседний слой (в той же папке), с которым можно объединить активный.
    fn lower_sibling(&mut self) -> Option<usize> {
        let i = self.doc.active;
        if i == 0 { self.toast("Ниже нет слоя"); return None; }
        let li = i - 1;
        if self.doc.layers[li].parent != self.doc.layers[i].parent || self.doc.layers[li].group {
            self.toast(if self.doc.layers[li].group { "Ниже папка — объединять можно только с обычным слоем" } else { "Ниже нет слоя в этой папке" });
            return None;
        }
        Some(li)
    }

    /// Свести пару «нижний + верхний» так, как они видны (нижний — как основа без своих эффектов).
    fn merge_pair(&self, low: &Layer, up: &Layer) -> Img {
        let mut base = low.clone();
        base.opacity = 1.0;
        base.blend = Blend::Normal;
        base.clip = false;
        base.visible = true;
        base.parent = None;
        base.draft = false;
        let mut top = up.clone();
        top.parent = None;
        top.draft = false;
        let mut tmp = Document::new("", self.doc.w, self.doc.h, [0, 0, 0], false);
        tmp.layers = vec![base, top];
        doc::flatten(&tmp, false)
    }

    pub fn merge_down(&mut self) {
        self.commit_pending();
        let sel = self.selected();
        if sel.len() > 1 { self.merge_selected(); return; }
        let i = self.doc.active;
        if self.doc.layers[i].group { self.merge_folder(); return; }
        let Some(li) = self.lower_sibling() else { return };
        let (low, up) = (self.doc.layers[li].clone(), self.doc.layers[i].clone());
        let img = if up.visible { self.merge_pair(&low, &up) } else { low.baked() };
        let simple = up.blend == Blend::Normal && !up.clip && (up.opacity - 1.0).abs() < 1e-3 && up.mask.is_none() && up.tint.is_none() && low.mask.is_none() && low.tint.is_none();
        let vec = match (&low.vector, &up.vector, up.visible) {
            (Some(lv), _, false) => Some(lv.clone()),
            (Some(lv), Some(uv), true) if simple => { let mut v = (**lv).clone(); v.extend(uv.iter().cloned()); Some(Arc::new(v)) }
            _ => None,
        };
        let mut nl = low.clone();
        nl.img = Arc::new(img);
        nl.mask = None;
        nl.tint = None;
        nl.vector = vec;
        nl.text = None;
        nl.ver += 1;
        self.tree_edit("Объединить с нижним", |layers, active| {
            layers[li] = nl;
            layers.remove(i);
            *active = li;
            Ok(())
        });
    }

    /// Перенести изображение на нижний слой (верхний остаётся пустым).
    pub fn transfer_down(&mut self) {
        self.commit_pending();
        let i = self.doc.active;
        if self.doc.layers[i].group { self.toast("Выберите слой, а не папку"); return; }
        let Some(li) = self.lower_sibling() else { return };
        let (low, up) = (self.doc.layers[li].clone(), self.doc.layers[i].clone());
        let mut nl = low.clone();
        nl.img = Arc::new(self.merge_pair(&low, &up));
        nl.mask = None;
        nl.tint = None;
        nl.vector = None;
        nl.text = None;
        nl.ver += 1;
        let mut nu = up.clone();
        nu.img = Arc::new(Img::new(self.doc.w, self.doc.h));
        nu.text = None;
        if nu.vector.is_some() { nu.vector = Some(Arc::new(vec![])); }
        nu.ver += 1;
        self.tree_edit("Перенести на нижний слой", |layers, active| {
            layers[li] = nl;
            layers[i] = nu;
            *active = i;
            Ok(())
        });
    }

    /// Объединить папку в один слой.
    pub fn merge_folder(&mut self) {
        self.commit_pending();
        let i = self.doc.active;
        if !self.doc.layers[i].group { return; }
        let r = doc::block(&self.doc.layers, i);
        let mut blk: Vec<Layer> = self.doc.layers[r.clone()].to_vec();
        let g = self.doc.layers[i].clone();
        if let Some(root) = blk.last_mut() {
            root.parent = None;
            root.visible = true;
            root.opacity = 1.0;
            root.blend = Blend::Normal;
            root.pass = false;
            root.clip = false;
        }
        let mut tmp = Document::new("", self.doc.w, self.doc.h, [0, 0, 0], false);
        tmp.layers = blk;
        doc::normalize(&mut tmp.layers);
        let mut nl = Layer::new(self.doc.w, self.doc.h, &g.name);
        nl.img = Arc::new(doc::flatten(&tmp, false));
        nl.opacity = g.opacity;
        nl.blend = if g.pass { Blend::Normal } else { g.blend };
        nl.visible = g.visible;
        nl.parent = g.parent.clone();
        nl.clip = g.clip;
        self.tree_edit("Объединить папку", |layers, active| {
            let start = r.start;
            layers.drain(r);
            layers.insert(start, nl);
            *active = start;
            Ok(())
        });
    }

    /// Объединить все выбранные слои в один.
    pub fn merge_selected(&mut self) {
        self.commit_pending();
        let sel = self.selected();
        if sel.len() < 2 { self.merge_down(); return; }
        let idx: Vec<usize> = sel.iter().flat_map(|&i| doc::block(&self.doc.layers, i)).collect();
        let img = doc::flatten_some(&self.doc, &idx);
        let top = *sel.last().unwrap();
        let mut nl = Layer::new(self.doc.w, self.doc.h, &self.doc.layers[top].name);
        nl.img = Arc::new(img);
        nl.parent = self.doc.layers[top].parent.clone();
        self.tree_edit("Объединить выбранные слои", |layers, active| {
            let ranges: Vec<std::ops::Range<usize>> = sel.iter().map(|&i| doc::block(layers, i)).collect();
            let removed_below: usize = ranges[..ranges.len() - 1].iter().map(|r| r.len()).sum();
            let at = ranges.last().unwrap().start - removed_below;
            for r in ranges.iter().rev() { layers.drain(r.clone()); }
            layers.insert(at, nl);
            *active = at;
            Ok(())
        });
        self.multi.clear();
    }

    /// Новый слой из всего видимого (без изменения остальных).
    pub fn merge_visible_new(&mut self) {
        self.commit_pending();
        let mut l = Layer::new(self.doc.w, self.doc.h, "Видимое");
        l.img = Arc::new(doc::flatten(&self.doc, false));
        self.tree_edit("Совместить видимые на новом слое", |layers, active| {
            let at = layers.len();
            layers.insert(at, l);
            *active = at;
            Ok(())
        });
    }
    fn replace_layers(&mut self, layers: Vec<Layer>, w: usize, h: usize, name: &str) {
        let old = std::mem::replace(&mut self.doc.layers, layers);
        let (ow, oh, oa) = (self.doc.w, self.doc.h, self.doc.active);
        self.doc.w = w;
        self.doc.h = h;
        self.doc.active = self.doc.active.min(self.doc.layers.len() - 1);
        self.hist.push(name, Op::Layers { other: old, w: ow, h: oh, active: oa });
        if self.sel.as_ref().map_or(false, |m| m.len() != w * h) { self.sel = None; self.sel_dirty = true; }
        self.after_history();
    }
    pub fn flatten_all(&mut self) {
        self.commit_pending();
        if self.doc.layers.len() < 2 { return; }
        let mut l = Layer::new(self.doc.w, self.doc.h, "Слой 1");
        l.img = Arc::new(doc::flatten(&self.doc, false));
        let (w, h) = (self.doc.w, self.doc.h);
        self.doc.active = 0;
        self.replace_layers(vec![l], w, h, "Свести изображение");
    }
    pub fn canvas_op(&mut self, name: &str, nw: usize, nh: usize, aff: Affine, f: impl Fn(&Img) -> Img) {
        self.commit_pending();
        let layers: Vec<Layer> = self.doc.layers.iter().map(|l| {
            let mut n = l.clone();
            match &l.vector {
                _ if l.group => {}
                Some(v) => { let nv = vector::transform(v, aff); n.img = Arc::new(vector::render_full(nw, nh, &nv)); n.vector = Some(Arc::new(nv)); }
                None => n.img = Arc::new(f(&l.img)),
            }
            if let Some(m) = &l.mask { n.mask = Some(Arc::new(f(m))); }
            n.ver += 1;
            n
        }).collect();
        self.replace_layers(layers, nw, nh, name);
    }
    pub fn resize_canvas(&mut self, nw: usize, nh: usize, ox: i32, oy: i32, scale: bool) {
        let (ow, oh) = (self.doc.w as f32, self.doc.h as f32);
        let aff = if scale { Affine { a: nw as f32 / ow, b: 0.0, c: 0.0, d: nh as f32 / oh, e: 0.0, f: 0.0 } } else { Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: ox as f32, f: oy as f32 } };
        self.canvas_op(if scale { "Размер изображения" } else { "Размер холста" }, nw, nh, aff, |img| {
            if scale { img.resize(nw, nh) } else { let mut o = Img::new(nw, nh); o.put(ox, oy, img); o }
        });
    }
    pub fn rotate_canvas(&mut self, cw: bool) {
        let (w, h) = (self.doc.h, self.doc.w);
        let (ow, oh) = (self.doc.w as f32, self.doc.h as f32);
        let aff = if cw { Affine { a: 0.0, b: 1.0, c: -1.0, d: 0.0, e: oh, f: 0.0 } } else { Affine { a: 0.0, b: -1.0, c: 1.0, d: 0.0, e: 0.0, f: ow } };
        self.canvas_op("Повернуть холст", w, h, aff, |img| img.rotate90(cw));
    }
    pub fn flip_canvas(&mut self, horiz: bool) {
        let (w, h) = (self.doc.w, self.doc.h);
        let aff = if horiz { Affine { a: -1.0, b: 0.0, c: 0.0, d: 1.0, e: w as f32, f: 0.0 } } else { Affine { a: 1.0, b: 0.0, c: 0.0, d: -1.0, e: 0.0, f: h as f32 } };
        self.canvas_op("Отразить холст", w, h, aff, |img| { let mut o = img.clone(); o.flip(horiz); o });
    }
    pub fn set_bg(&mut self, c: [u8; 3], vis: bool) {
        let old = (self.doc.bg, self.doc.bg_visible);
        if old == (c, vis) { return; }
        self.doc.bg = c;
        self.doc.bg_visible = vis;
        self.hist.push("Цвет фона", Op::Bg { other: old });
        self.invalidate_all();
        self.mark_changed();
    }

    /* ---------- буфер обмена ---------- */

    fn selection_content(&self, merged: bool) -> Option<(Img, i32, i32)> {
        let src = if merged { doc::flatten(&self.doc, false) } else { (*self.doc.layer().img).clone() };
        let b = if self.sel.is_some() { self.sel_bounds()? } else { src.content_bounds()? };
        let mut c = src.crop(b);
        if let Some(m) = &self.sel {
            let w = self.doc.w;
            for y in 0..b.h as usize { for x in 0..b.w as usize {
                let k = m[(b.y as usize + y) * w + b.x as usize + x] as f32 / 255.0;
                let i = (y * b.w as usize + x) * 4;
                for ch in 0..4 { c.px[i + ch] = (c.px[i + ch] as f32 * k).round() as u8; }
            } }
        }
        Some((c, b.x, b.y))
    }
    pub fn copy(&mut self, merged: bool) {
        match self.selection_content(merged) { Some(c) => { self.clipboard = Some(c); self.toast("Скопировано"); } None => self.toast("Нечего копировать") }
    }
    pub fn cut(&mut self) {
        if let Some(c) = self.selection_content(false) { self.clipboard = Some(c); self.layer_clear(); self.toast("Вырезано"); }
    }
    pub fn paste(&mut self) {
        let Some((img, x, y)) = self.clipboard.clone() else { self.toast("Буфер обмена пуст"); return };
        let mut l = Layer::new(self.doc.w, self.doc.h, "Вставка");
        l.img_mut().put(x, y, &img);
        self.add_layer(l, "Вставить");
    }
    pub fn copy_to_layer(&mut self, cut: bool) {
        let Some((img, x, y)) = self.selection_content(false) else { self.toast("Нечего копировать"); return };
        self.hist.begin_group();
        if cut { self.layer_clear(); }
        let mut l = Layer::new(self.doc.w, self.doc.h, if cut { "Вырезанный фрагмент" } else { "Копия фрагмента" });
        l.img_mut().put(x, y, &img);
        self.add_layer(l, "Новый слой");
        self.hist.end_group(if cut { "Вырезать на новый слой" } else { "Скопировать на новый слой" });
    }

    /* ---------- текст ---------- */

    pub fn apply_text(&mut self, spec: TextSpec, layer: Option<usize>, font: Option<&[u8]>) {
        let img = render_text(&spec, self.doc.w, self.doc.h, font);
        let name: String = spec.str.lines().next().unwrap_or("Текст").chars().take(24).collect();
        match layer {
            Some(li) if li < self.doc.layers.len() => {
                let id = self.doc.layers[li].id.clone();
                let before = (*self.doc.layers[li].img).clone();
                let old_text = self.doc.layers[li].text.clone();
                self.hist.begin_group();
                self.doc.layers[li].img = Arc::new(img.clone());
                self.doc.layers[li].ver += 1;
                self.doc.layers[li].text = Some(spec.clone());
                self.hist.push("Текст", Op::Pixels { layer: id.clone(), x: 0, y: 0, before, after: img });
                self.hist.push("Текст", Op::Text { layer: id, before: old_text, after: Some(spec) });
                self.hist.end_group("Изменить текст");
                self.invalidate_all();
                self.mark_changed();
            }
            _ => {
                let mut l = Layer::new(self.doc.w, self.doc.h, if name.is_empty() { "Текст" } else { &name });
                l.img = Arc::new(img);
                l.text = Some(spec);
                self.add_layer(l, "Текст");
            }
        }
    }

    /* ---------- клавиатура ---------- */

    pub fn keys(&mut self, ctx: &egui::Context, sh: &mut Shared) -> Option<&'static str> {
        if ctx.wants_keyboard_input() { return None; }
        let mut action = None;
        ctx.input(|i| {
            self.space = i.key_down(egui::Key::Space);
            let cmd = i.modifiers.command;
            let shift = i.modifiers.shift;
            let k = |key| i.key_pressed(key);
            use egui::Key;
            if cmd {
                if k(Key::Z) { action = Some(if shift { "redo" } else { "undo" }); }
                if k(Key::Y) { action = Some("redo"); }
                if k(Key::S) { action = Some(if shift { "save_as" } else { "save" }); }
                if k(Key::N) { action = Some(if shift { "layer_new" } else { "new" }); }
                if k(Key::O) { action = Some("open"); }
                if k(Key::A) { action = Some("sel_all"); }
                if k(Key::D) { action = Some("sel_none"); }
                if k(Key::I) && shift { action = Some("sel_invert"); }
                if k(Key::J) { action = Some(if shift { "cut_layer" } else { "dup_or_copy" }); }
                if k(Key::E) { action = Some(if shift { "flatten" } else { "merge" }); }
                if k(Key::T) { action = Some("transform"); }
                if k(Key::G) { action = Some(if shift { "ungroup" } else { "group" }); }
                if k(Key::OpenBracket) { action = Some("layer_down"); }
                if k(Key::CloseBracket) { action = Some("layer_up"); }
                if k(Key::Num0) { action = Some("fit"); }
                if k(Key::Num1) { action = Some("actual"); }
                if k(Key::Plus) || k(Key::Equals) { action = Some("zoom_in"); }
                if k(Key::Minus) { action = Some("zoom_out"); }
                return;
            }
            for (key, t) in [(Key::B, Tool::Brush), (Key::E, Tool::Eraser), (Key::R, Tool::Smudge), (Key::S, Tool::Clone), (Key::I, Tool::Picker), (Key::V, Tool::Transform), (Key::T, Tool::Text), (Key::U, Tool::Shapes), (Key::H, Tool::Hand), (Key::Z, Tool::Zoom)] {
                if k(key) { action = Some(match t { Tool::Brush => "t_brush", Tool::Eraser => "t_eraser", Tool::Smudge => "t_smudge", Tool::Clone => "t_clone", Tool::Picker => "t_picker", Tool::Transform => "t_transform", Tool::Text => "t_text", Tool::Shapes => "t_shapes", Tool::Hand => "t_hand", _ => "t_zoom" }); }
            }
            if k(Key::G) { action = Some(if shift { "t_gradient" } else { "t_fill" }); }
            if k(Key::M) { action = Some("t_select_rect"); }
            if k(Key::L) { action = Some("t_select_lasso"); }
            if k(Key::W) { action = Some("t_select_wand"); }
            if k(Key::X) { action = Some("swap"); }
            if k(Key::D) { action = Some("default_colors"); }
            if k(Key::F) { action = Some("flip_view"); }
            if k(Key::OpenBracket) { action = Some(if i.modifiers.alt { "layer_sel_down" } else { "size_down" }); }
            if k(Key::CloseBracket) { action = Some(if i.modifiers.alt { "layer_sel_up" } else { "size_up" }); }
            if k(Key::Delete) || k(Key::Backspace) { action = Some("clear"); }
            if k(Key::Enter) { action = Some("enter"); }
            if k(Key::Escape) { action = Some("escape"); }
            if k(Key::Tab) { action = Some("toggle_ui"); }
            // 1…9, 0 — непрозрачность 10…100 %
            const OPS: [(egui::Key, &str); 10] = [(Key::Num1, "opacity_1"), (Key::Num2, "opacity_2"), (Key::Num3, "opacity_3"), (Key::Num4, "opacity_4"), (Key::Num5, "opacity_5"), (Key::Num6, "opacity_6"), (Key::Num7, "opacity_7"), (Key::Num8, "opacity_8"), (Key::Num9, "opacity_9"), (Key::Num0, "opacity_10")];
            for (key, a) in OPS { if k(key) { action = Some(a); } }
        });
        let _ = sh;
        action
    }

    pub fn tool_state_mut<'a>(&self, sh: &'a mut Shared) -> &'a mut ToolState {
        match self.tool { Tool::Eraser => &mut sh.tools.eraser, Tool::Smudge => &mut sh.tools.smudge, Tool::Clone => &mut sh.tools.clone, _ => &mut sh.tools.brush }
    }
}

pub fn tool_state(sh: &Shared, t: Tool) -> &ToolState {
    match t { Tool::Eraser => &sh.tools.eraser, Tool::Smudge => &sh.tools.smudge, Tool::Clone => &sh.tools.clone, _ => &sh.tools.brush }
}

fn quad(p: &egui::Painter, tex: egui::TextureId, c: [Pos2; 4], uv: [Pos2; 4]) {
    let mut m = egui::Mesh::with_texture(tex);
    for i in 0..4 {
        m.vertices.push(egui::epaint::Vertex { pos: c[i], uv: uv[i], color: Color32::WHITE });
    }
    m.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(egui::Shape::mesh(m));
}

fn xcorners(s: &XState) -> [(f32, f32); 4] {
    let (c, sn) = (s.rot.cos(), s.rot.sin());
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(a, b)| {
        let (x, y) = (a * s.w / 2.0, b * s.h / 2.0);
        (s.cx + x * c - y * sn, s.cy + x * sn + y * c)
    })
}

fn rot_handle(s: &XState, v: &View) -> Pos2 {
    let c = xcorners(s);
    let tm = v.to_screen((c[0].0 + c[1].0) / 2.0, (c[0].1 + c[1].1) / 2.0);
    let cc = v.to_screen(s.cx, s.cy);
    let d = tm - cc;
    let l = d.length().max(1.0);
    tm + d / l * 34.0
}

fn point_in_poly(p: Pos2, poly: &[Pos2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x { inside = !inside; }
        j = i;
    }
    inside
}

/// Смешать исходник и результат фильтра по маске выделения.
fn mix_mask(src: &Img, res: &Img, m: &[u8]) -> Img {
    let mut out = src.clone();
    out.px.par_chunks_mut(4).enumerate().for_each(|(i, p)| {
        let k = m[i] as f32 / 255.0;
        if k <= 0.0 { return; }
        for c in 0..4 { p[c] = (p[c] as f32 * (1.0 - k) + res.px[i * 4 + c] as f32 * k).round() as u8; }
    });
    out
}

/* ---------- текст ---------- */

pub fn default_font() -> Vec<u8> {
    let defs = egui::FontDefinitions::default();
    defs.font_data.get("Ubuntu-Light").map(|f| f.font.to_vec()).unwrap_or_default()
}

pub fn render_text(spec: &TextSpec, w: usize, h: usize, font: Option<&[u8]>) -> Img {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
    let mut img = Img::new(w, h);
    let def;
    let bytes: &[u8] = match font { Some(b) if !b.is_empty() => b, _ => { def = default_font(); &def } };
    let Ok(f) = FontRef::try_from_slice(bytes) else { return img };
    let scale = PxScale::from(spec.size.max(4.0));
    let sf = f.as_scaled(scale);
    let col = util::parse_hex(&spec.color).unwrap_or([0, 0, 0]);
    let lh = spec.size * spec.lh.max(0.5);
    for (li, line) in spec.str.lines().enumerate() {
        let width: f32 = line.chars().map(|c| sf.h_advance(sf.glyph_id(c)) + spec.spacing).sum();
        let x0 = match spec.align.as_str() { "center" => spec.x - width / 2.0, "right" => spec.x - width, _ => spec.x };
        let mut caret = ab_glyph::point(x0, spec.y + li as f32 * lh + sf.ascent());
        let mut prev = None;
        for ch in line.chars() {
            let gid = sf.glyph_id(ch);
            if let Some(p) = prev { caret.x += sf.kern(p, gid); }
            let g = gid.with_scale_and_position(scale, caret);
            caret.x += sf.h_advance(gid) + spec.spacing;
            prev = Some(gid);
            if let Some(og) = f.outline_glyph(g) {
                let b = og.px_bounds();
                let mut put = |gx: u32, gy: u32, c: f32| {
                    let x = b.min.x as i32 + gx as i32;
                    let y = b.min.y as i32 + gy as i32;
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || c <= 0.0 { return; }
                    let i = (y as usize * w + x as usize) * 4;
                    let p = &mut img.px[i..i + 4];
                    let a = c.min(1.0);
                    for k in 0..3 { p[k] = (col[k] as f32 * a + p[k] as f32 * (1.0 - a)).round() as u8; }
                    p[3] = (255.0 * a + p[3] as f32 * (1.0 - a)).round() as u8;
                };
                og.draw(&mut put);
                if spec.bold {
                    let g2 = gid.with_scale_and_position(scale, ab_glyph::point(caret.x - sf.h_advance(gid) - spec.spacing + spec.size * 0.03, caret.y));
                    if let Some(og2) = f.outline_glyph(g2) {
                        let b2 = og2.px_bounds();
                        og2.draw(|gx, gy, c| {
                            let x = b2.min.x as i32 + gx as i32;
                            let y = b2.min.y as i32 + gy as i32;
                            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 || c <= 0.0 { return; }
                            let i = (y as usize * w + x as usize) * 4;
                            let p = &mut img.px[i..i + 4];
                            let a = c.min(1.0);
                            for k in 0..3 { p[k] = (col[k] as f32 * a + p[k] as f32 * (1.0 - a)).round() as u8; }
                            p[3] = (255.0 * a + p[3] as f32 * (1.0 - a)).round() as u8;
                        });
                    }
                }
            }
        }
    }
    img
}
