//! Документ: слои, композитинг, история отмены.

use crate::blend::{composite, Blend};
use crate::raster::Img;
use crate::util::{self, Rect};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextSpec {
    pub str: String,
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub align: String,
    pub color: String,
    pub lh: f32,
    pub spacing: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for TextSpec {
    fn default() -> Self {
        TextSpec { str: String::new(), font: "default".into(), size: 64.0, bold: false, italic: false, align: "left".into(), color: "#000000".into(), lh: 1.2, spacing: 0.0, x: 0.0, y: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Props {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: Blend,
    pub alpha_lock: bool,
    pub clip: bool,
    pub locked: bool,
    pub reference: bool,
    pub draft: bool,
    pub tint: Option<[u8; 3]>,
    pub pass: bool,
    pub mask_on: bool,
}

#[derive(Clone)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub img: Arc<Img>,
    pub visible: bool,
    pub opacity: f32,
    pub blend: Blend,
    pub alpha_lock: bool,
    pub clip: bool,
    pub locked: bool,
    pub reference: bool,
    pub text: Option<TextSpec>,
    /// Векторные штрихи (векторный слой); пиксели в img — их отрисовка.
    pub vector: Option<crate::vector::VList>,
    /// Папка слоёв: её содержимое — слои ниже с parent == id (идут подряд сразу под папкой).
    pub group: bool,
    /// Папка, в которой лежит слой.
    pub parent: Option<String>,
    /// Папка свёрнута в панели слоёв.
    pub collapsed: bool,
    /// Режим папки «Сквозной»: содержимое накладывается прямо на нижние слои.
    pub pass: bool,
    /// Маска слоя: значима только прозрачность (255 — видно, 0 — скрыто).
    pub mask: Option<Arc<Img>>,
    pub mask_on: bool,
    /// Черновой слой: виден при работе, но не попадает в экспорт.
    pub draft: bool,
    /// Цвет слоя: всё содержимое показывается этим цветом (для набросков).
    pub tint: Option<[u8; 3]>,
    pub ver: u64,
}

impl Layer {
    pub fn new(w: usize, h: usize, name: &str) -> Layer {
        Layer {
            id: util::uid(),
            name: name.into(),
            img: Arc::new(Img::new(w, h)),
            visible: true,
            opacity: 1.0,
            blend: Blend::Normal,
            alpha_lock: false,
            clip: false,
            locked: false,
            reference: false,
            text: None,
            vector: None,
            group: false,
            parent: None,
            collapsed: false,
            pass: true,
            mask: None,
            mask_on: true,
            draft: false,
            tint: None,
            ver: 1,
        }
    }
    /// Новая папка (без собственных пикселей).
    pub fn new_group(name: &str) -> Layer {
        let mut l = Layer::new(0, 0, name);
        l.group = true;
        l
    }
    pub fn is_vector(&self) -> bool {
        self.vector.is_some()
    }
    pub fn has_mask(&self) -> bool {
        self.mask.is_some() && self.mask_on
    }
    pub fn img_mut(&mut self) -> &mut Img {
        self.ver += 1;
        Arc::make_mut(&mut self.img)
    }
    pub fn props(&self) -> Props {
        Props {
            name: self.name.clone(), visible: self.visible, opacity: self.opacity, blend: self.blend, alpha_lock: self.alpha_lock, clip: self.clip,
            locked: self.locked, reference: self.reference, draft: self.draft, tint: self.tint, pass: self.pass, mask_on: self.mask_on,
        }
    }
    pub fn set_props(&mut self, p: &Props) {
        self.name = p.name.clone();
        self.visible = p.visible;
        self.opacity = p.opacity;
        self.blend = p.blend;
        self.alpha_lock = p.alpha_lock;
        self.clip = p.clip;
        self.locked = p.locked;
        self.reference = p.reference;
        self.draft = p.draft;
        self.tint = p.tint;
        self.pass = p.pass;
        self.mask_on = p.mask_on;
        self.ver += 1;
    }
    /// Пиксели слоя с учётом маски (для экспорта и объединения).
    pub fn baked(&self) -> Img {
        let mut img = (*self.img).clone();
        if let (Some(m), true) = (&self.mask, self.mask_on) {
            if m.w == img.w && m.h == img.h {
                for (p, q) in img.px.chunks_exact_mut(4).zip(m.px.chunks_exact(4)) {
                    let k = q[3] as f32 / 255.0;
                    for c in p.iter_mut() { *c = (*c as f32 * k).round() as u8; }
                }
            }
        }
        if let Some(t) = self.tint {
            for p in img.px.chunks_exact_mut(4) {
                let a = p[3] as f32 / 255.0;
                for c in 0..3 { p[c] = (t[c] as f32 * a).round() as u8; }
            }
        }
        img
    }
}

/* ---------- дерево слоёв ---------- */

/// Лежит ли слой i (на любой глубине) в папке с id gid.
pub fn is_inside(layers: &[Layer], i: usize, gid: &str) -> bool {
    let mut p = layers[i].parent.clone();
    let mut guard = 0;
    while let Some(id) = p {
        if id == gid { return true; }
        guard += 1;
        if guard > 64 { return false; }
        p = layers.iter().find(|l| l.id == id).and_then(|l| l.parent.clone());
    }
    false
}

/// Начало содержимого папки gi: содержимое — индексы start..gi.
pub fn group_start(layers: &[Layer], gi: usize) -> usize {
    let id = &layers[gi].id;
    let mut s = gi;
    while s > 0 && is_inside(layers, s - 1, id) { s -= 1; }
    s
}

/// Диапазон «блока» слоя: для папки — вместе с содержимым.
pub fn block(layers: &[Layer], i: usize) -> std::ops::Range<usize> {
    if layers[i].group { group_start(layers, i)..i + 1 } else { i..i + 1 }
}

/// Глубина вложенности.
pub fn depth(layers: &[Layer], i: usize) -> usize {
    let mut d = 0;
    let mut p = layers[i].parent.clone();
    while let Some(id) = p {
        d += 1;
        if d > 64 { break; }
        p = layers.iter().find(|l| l.id == id).and_then(|l| l.parent.clone());
    }
    d
}

/// Видимость с учётом папок.
pub fn shown(layers: &[Layer], i: usize) -> bool {
    if !layers[i].visible { return false; }
    let mut p = layers[i].parent.clone();
    let mut guard = 0;
    while let Some(id) = p {
        guard += 1;
        if guard > 64 { break; }
        match layers.iter().find(|l| l.id == id) {
            Some(g) => { if !g.visible { return false; } p = g.parent.clone(); }
            None => break,
        }
    }
    true
}

/// Закреплён ли слой (сам или папкой).
pub fn locked(layers: &[Layer], i: usize) -> bool {
    if layers[i].locked { return true; }
    let mut p = layers[i].parent.clone();
    let mut guard = 0;
    while let Some(id) = p {
        guard += 1;
        if guard > 64 { break; }
        match layers.iter().find(|l| l.id == id) {
            Some(g) => { if g.locked { return true; } p = g.parent.clone(); }
            None => break,
        }
    }
    false
}

/// Исправить связи после загрузки или правки: родитель — существующая папка, содержимое — подряд под ней.
pub fn normalize(layers: &mut [Layer]) {
    for i in 0..layers.len() {
        if let Some(pid) = layers[i].parent.clone() {
            let ok = match layers.iter().position(|l| l.id == pid) {
                Some(gi) => layers[gi].group && gi > i && (i + 1..gi).all(|k| layers[k].parent.as_deref() == Some(pid.as_str()) || is_inside(layers, k, &pid)),
                None => false,
            };
            if !ok { layers[i].parent = None; }
        }
    }
}

struct Node {
    i: usize,
    kids: Vec<Node>,
}

/// Дерево уровня [lo, hi): снизу вверх.
fn build(layers: &[Layer], lo: usize, hi: usize) -> Vec<Node> {
    let mut out = vec![];
    let mut k = hi;
    while k > lo {
        let i = k - 1;
        if layers[i].group {
            let s = group_start(layers, i).max(lo);
            out.push(Node { i, kids: build(layers, s, i) });
            k = s;
        } else {
            out.push(Node { i, kids: vec![] });
            k = i;
        }
    }
    out.reverse();
    out
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Guide {
    pub sym: String,
    pub segments: usize,
    pub rotational: bool,
    pub cx: f32,
    pub cy: f32,
    pub grid: bool,
    pub grid_size: f32,
}

impl Default for Guide {
    fn default() -> Self {
        Guide { sym: "none".into(), segments: 6, rotational: true, cx: 0.0, cy: 0.0, grid: false, grid_size: 100.0 }
    }
}

pub struct Document {
    pub id: String,
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub bg: [u8; 3],
    pub bg_visible: bool,
    pub layers: Vec<Layer>,
    pub active: usize,
    pub layer_seq: u32,
    pub created: i64,
    pub modified: i64,
    pub strokes: u64,
    pub time: f64,
    pub guide: Guide,
}

impl Document {
    pub fn new(name: &str, w: usize, h: usize, bg: [u8; 3], bg_visible: bool) -> Document {
        let mut d = Document {
            id: util::uid(),
            name: name.into(),
            w,
            h,
            bg,
            bg_visible,
            layers: vec![],
            active: 0,
            layer_seq: 1,
            created: util::now_ms(),
            modified: util::now_ms(),
            strokes: 0,
            time: 0.0,
            guide: Guide { cx: w as f32 / 2.0, cy: h as f32 / 2.0, grid_size: (w.min(h) as f32 / 12.0).round().max(8.0), ..Default::default() },
        };
        let n = d.next_name();
        d.layers.push(Layer::new(w, h, &n));
        d
    }
    pub fn next_name(&mut self) -> String {
        let n = format!("Слой {}", self.layer_seq);
        self.layer_seq += 1;
        n
    }
    pub fn layer(&self) -> &Layer {
        &self.layers[self.active.min(self.layers.len() - 1)]
    }
    pub fn layer_mut(&mut self) -> &mut Layer {
        let i = self.active.min(self.layers.len() - 1);
        &mut self.layers[i]
    }
    pub fn find(&self, id: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }
    pub fn full(&self) -> Rect {
        Rect::new(0, 0, self.w as i32, self.h as i32)
    }
    /// Предел числа слоёв по памяти.
    pub fn max_layers(&self) -> usize {
        let budget: f64 = if cfg!(target_os = "android") { 700e6 } else { 2.5e9 };
        ((budget / (self.w * self.h * 4) as f64) as usize).saturating_sub(4).clamp(3, 200)
    }
}

/* ---------- композитинг ---------- */

/// Подмена содержимого слоя при рисовании: пиксели слоя или его маска.
#[derive(Clone, Copy)]
pub struct Ov<'a> {
    pub layer: usize,
    pub img: &'a Img,
    pub mask: bool,
}

/// Пересчитать композицию (с фоном) в прямоугольнике r.
pub fn render(doc: &Document, comp: &mut Img, r: Rect, ov: Option<Ov>) {
    render_opts(doc, comp, r, ov, doc.bg_visible, true);
}

pub fn render_with(doc: &Document, comp: &mut Img, r: Rect, ov: Option<Ov>, with_bg: bool) {
    render_opts(doc, comp, r, ov, with_bg, true);
}

struct Ctx<'a> {
    layers: &'a [Layer],
    ov: Option<Ov<'a>>,
    drafts: bool,
    w: usize,
}

#[inline]
fn to_u8(g: [f32; 4]) -> [u8; 4] {
    [(g[0] * 255.0).round().clamp(0.0, 255.0) as u8, (g[1] * 255.0).round().clamp(0.0, 255.0) as u8, (g[2] * 255.0).round().clamp(0.0, 255.0) as u8, (g[3] * 255.0).round().clamp(0.0, 255.0) as u8]
}

impl<'a> Ctx<'a> {
    fn img(&self, i: usize) -> &'a Img {
        match self.ov {
            Some(o) if o.layer == i && !o.mask => o.img,
            _ => &self.layers[i].img,
        }
    }
    fn mask(&self, i: usize) -> Option<&'a Img> {
        let l = &self.layers[i];
        if !l.mask_on { return None; }
        let m: Option<&'a Img> = match self.ov {
            Some(o) if o.layer == i && o.mask => Some(o.img),
            _ => l.mask.as_deref(),
        };
        m.filter(|m| m.w == self.w && m.px.len() >= self.w * 4)
    }
    fn live(&self, i: usize) -> bool {
        let l = &self.layers[i];
        l.visible && l.opacity > 0.0 && (self.drafts || !l.draft)
    }
    /// Пиксель обычного слоя с маской и цветом слоя (premultiplied).
    #[inline]
    fn pix(srow: &[u8], mrow: Option<&[u8]>, tint: Option<[u8; 3]>, x: usize) -> [u8; 4] {
        let mut p = [srow[x * 4], srow[x * 4 + 1], srow[x * 4 + 2], srow[x * 4 + 3]];
        if let Some(m) = mrow {
            let k = m[x * 4 + 3];
            if k < 255 {
                let k = k as f32 / 255.0;
                for c in p.iter_mut() { *c = (*c as f32 * k).round() as u8; }
            }
        }
        if let Some(t) = tint {
            let a = p[3] as u32;
            for c in 0..3 { p[c] = ((t[c] as u32 * a + 127) / 255) as u8; }
        }
        p
    }
    fn rows(&self, i: usize, y: usize) -> (&'a [u8], Option<&'a [u8]>) {
        let w = self.w;
        let img = self.img(i);
        let srow = &img.px[y * w * 4..(y + 1) * w * 4];
        let mrow = self.mask(i).filter(|m| m.h > y).map(|m| &m.px[y * w * 4..(y + 1) * w * 4]);
        (srow, mrow)
    }
    /// Изолированная строка узла (слой или папка) — для обтравки и непрозрачных папок.
    fn isolated(&self, node: &Node, y: usize, x0: usize, x1: usize) -> Vec<[f32; 4]> {
        let l = &self.layers[node.i];
        if l.group {
            let mut g = vec![[0f32; 4]; x1 - x0];
            self.nodes(&node.kids, y, x0, x1, &mut g);
            if let Some(m) = self.mask(node.i).filter(|m| m.h > y) {
                let mrow = &m.px[y * self.w * 4..(y + 1) * self.w * 4];
                for x in x0..x1 {
                    let k = mrow[x * 4 + 3] as f32 / 255.0;
                    for c in 0..4 { g[x - x0][c] *= k; }
                }
            }
            g
        } else {
            let (srow, mrow) = self.rows(node.i, y);
            (x0..x1).map(|x| { let p = Self::pix(srow, mrow, l.tint, x); [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0] }).collect()
        }
    }
    fn nodes(&self, nodes: &[Node], y: usize, x0: usize, x1: usize, buf: &mut [[f32; 4]]) {
        let n = nodes.len();
        let mut i = 0;
        while i < n {
            let node = &nodes[i];
            let mut j = i + 1;
            while j < n && self.layers[nodes[j].i].clip { j += 1; }
            let l = &self.layers[node.i];
            if self.live(node.i) {
                let clips: Vec<&Node> = nodes[i + 1..j].iter().filter(|c| self.live(c.i)).collect();
                if l.group && l.pass && clips.is_empty() && self.mask(node.i).is_none() {
                    // «Сквозной» режим: содержимое ложится прямо на нижние слои
                    if l.opacity >= 0.999 {
                        self.nodes(&node.kids, y, x0, x1, buf);
                    } else {
                        let mut t = buf.to_vec();
                        self.nodes(&node.kids, y, x0, x1, &mut t);
                        for (b, tt) in buf.iter_mut().zip(t) { for c in 0..4 { b[c] += (tt[c] - b[c]) * l.opacity; } }
                    }
                } else if !l.group && clips.is_empty() {
                    let (srow, mrow) = self.rows(node.i, y);
                    if mrow.is_none() && l.tint.is_none() {
                        for x in x0..x1 { composite(&mut buf[x - x0], &srow[x * 4..x * 4 + 4], l.blend, l.opacity, x as i32, y as i32); }
                    } else {
                        for x in x0..x1 { let p = Self::pix(srow, mrow, l.tint, x); composite(&mut buf[x - x0], &p, l.blend, l.opacity, x as i32, y as i32); }
                    }
                } else {
                    let mut g = self.isolated(node, y, x0, x1);
                    let base_a: Vec<f32> = g.iter().map(|p| p[3]).collect();
                    for c in clips {
                        let cl = &self.layers[c.i];
                        if cl.group {
                            let row = self.isolated(c, y, x0, x1);
                            for x in x0..x1 {
                                let ba = base_a[x - x0];
                                if ba <= 0.0 { continue; }
                                let s = to_u8(row[x - x0].map(|v| v * ba));
                                composite(&mut g[x - x0], &s, if cl.pass { Blend::Normal } else { cl.blend }, cl.opacity, x as i32, y as i32);
                            }
                        } else {
                            let (srow, mrow) = self.rows(c.i, y);
                            for x in x0..x1 {
                                let ba = base_a[x - x0];
                                if ba <= 0.0 { continue; }
                                let p = Self::pix(srow, mrow, cl.tint, x);
                                let s = [(p[0] as f32 * ba) as u8, (p[1] as f32 * ba) as u8, (p[2] as f32 * ba) as u8, (p[3] as f32 * ba) as u8];
                                composite(&mut g[x - x0], &s, cl.blend, cl.opacity, x as i32, y as i32);
                            }
                        }
                    }
                    let blend = if l.group && l.pass { Blend::Normal } else { l.blend };
                    for x in x0..x1 { let s = to_u8(g[x - x0]); composite(&mut buf[x - x0], &s, blend, l.opacity, x as i32, y as i32); }
                }
            }
            i = j;
        }
    }
}

/// Композиция: with_bg — с цветом фона, drafts — показывать черновые слои.
pub fn render_opts(doc: &Document, comp: &mut Img, r: Rect, ov: Option<Ov>, with_bg: bool, drafts: bool) {
    let Some(r) = r.clip(doc.w, doc.h) else { return };
    let w = doc.w;
    let bg = if with_bg { [doc.bg[0] as f32 / 255.0, doc.bg[1] as f32 / 255.0, doc.bg[2] as f32 / 255.0, 1.0] } else { [0.0; 4] };
    let tree = build(&doc.layers, 0, doc.layers.len());
    let ctx = Ctx { layers: &doc.layers, ov, drafts, w };
    comp.px.par_chunks_mut(w * 4).enumerate().skip(r.y as usize).take(r.h as usize).for_each(|(y, row)| {
        let x0 = r.x as usize;
        let x1 = r.right() as usize;
        let mut buf: Vec<[f32; 4]> = vec![bg; x1 - x0];
        ctx.nodes(&tree, y, x0, x1, &mut buf);
        for x in x0..x1 {
            let b = to_u8(buf[x - x0]);
            row[x * 4..x * 4 + 4].copy_from_slice(&b);
        }
    });
}

/// Плоское изображение всего документа (опционально без фона).
pub fn flatten(doc: &Document, with_bg: bool) -> Img {
    let mut out = Img::new(doc.w, doc.h);
    render_with(doc, &mut out, doc.full(), None, with_bg && doc.bg_visible);
    out
}

/// Для экспорта: без черновых слоёв.
pub fn flatten_export(doc: &Document, with_bg: bool) -> Img {
    let mut out = Img::new(doc.w, doc.h);
    render_opts(doc, &mut out, doc.full(), None, with_bg && doc.bg_visible, false);
    out
}

/// Сведение части слоёв (индексы в порядке снизу вверх) в одно изображение.
pub fn flatten_some(doc: &Document, idx: &[usize]) -> Img {
    let mut d = Document { id: String::new(), name: String::new(), w: doc.w, h: doc.h, bg: doc.bg, bg_visible: false, layers: vec![], active: 0, layer_seq: 1, created: 0, modified: 0, strokes: 0, time: 0.0, guide: doc.guide.clone() };
    let keep: std::collections::HashSet<usize> = idx.iter().copied().collect();
    // папки, в которых лежат выбранные слои, сохраняем — от них зависят режимы и видимость
    for (i, l) in doc.layers.iter().enumerate() {
        let needed = keep.contains(&i) || (l.group && idx.iter().any(|&k| is_inside(&doc.layers, k, &l.id)));
        if needed { d.layers.push(l.clone()); }
    }
    normalize(&mut d.layers);
    flatten(&d, false)
}
/* ---------- история ---------- */

pub enum Op {
    Pixels { layer: String, x: i32, y: i32, before: Img, after: Img },
    /// Пиксели маски слоя.
    Mask { layer: String, x: i32, y: i32, before: Img, after: Img },
    /// Маска добавлена или удалена.
    MaskSet { layer: String, before: Option<Arc<Img>>, after: Option<Arc<Img>> },
    /// Перестройка дерева слоёв (папки, перемещения): весь список слоёв.
    Tree { other: Vec<Layer>, active: usize },
    Props { layer: String, before: Props, after: Props },
    Text { layer: String, before: Option<TextSpec>, after: Option<TextSpec> },
    /// Смена векторных данных слоя (пиксели меняются отдельной операцией).
    Vector { layer: String, before: Option<crate::vector::VList>, after: Option<crate::vector::VList> },
    Insert { index: usize, layer: Option<Layer>, prev_active: usize },
    Remove { index: usize, layer: Option<Layer>, prev_active: usize },
    Move { from: usize, to: usize },
    Layers { other: Vec<Layer>, w: usize, h: usize, active: usize },
    Sel { other: Option<Vec<u8>> },
    Bg { other: ([u8; 3], bool) },
    Group(Vec<Op>),
}

pub struct Entry {
    pub name: String,
    pub op: Op,
    pub bytes: usize,
}

pub struct History {
    pub list: Vec<Entry>,
    pub pos: usize,
    pub bytes: usize,
    pub max_bytes: usize,
    group: Vec<Vec<Op>>,
}

impl Default for History {
    fn default() -> Self {
        History { list: vec![], pos: 0, bytes: 0, max_bytes: if cfg!(target_os = "android") { 256 << 20 } else { 1024 << 20 }, group: vec![] }
    }
}

fn op_bytes(op: &Op) -> usize {
    match op {
        Op::Pixels { before, after, .. } | Op::Mask { before, after, .. } => before.px.len() + after.px.len(),
        Op::MaskSet { before, after, .. } => before.as_ref().map_or(0, |m| m.px.len()) + after.as_ref().map_or(0, |m| m.px.len()),
        // в истории остаются только удалённые слои (общие с документом картинки не считаем)
        Op::Tree { other, .. } => other.iter().map(|l| 512 + if Arc::strong_count(&l.img) == 1 { l.img.px.len() } else { 0 } + l.mask.as_ref().map_or(0, |m| if Arc::strong_count(m) == 1 { m.px.len() } else { 0 })).sum(),
        Op::Layers { other, .. } => other.iter().map(|l| l.img.px.len()).sum(),
        Op::Sel { other } => other.as_ref().map_or(0, |v| v.len()),
        Op::Insert { layer, .. } | Op::Remove { layer, .. } => layer.as_ref().map_or(0, |l| l.img.px.len()),
        Op::Group(v) => v.iter().map(op_bytes).sum(),
        Op::Vector { before, after, .. } => [before, after].iter().map(|v| v.as_ref().map_or(0, |v| v.iter().map(|s| s.pts.len() * 16 + 64).sum::<usize>())).sum(),
        _ => 64,
    }
}

impl History {
    pub fn push(&mut self, name: &str, op: Op) {
        if let Some(g) = self.group.last_mut() {
            g.push(op);
            return;
        }
        while self.list.len() > self.pos {
            let e = self.list.pop().unwrap();
            self.bytes -= e.bytes;
        }
        let bytes = op_bytes(&op) + name.len();
        self.list.push(Entry { name: name.into(), op, bytes });
        self.pos += 1;
        self.bytes += bytes;
        while (self.bytes > self.max_bytes || self.list.len() > 300) && self.list.len() > 1 {
            let e = self.list.remove(0);
            self.pos -= 1;
            self.bytes -= e.bytes;
        }
    }
    pub fn begin_group(&mut self) {
        self.group.push(vec![]);
    }
    pub fn end_group(&mut self, name: &str) {
        if let Some(g) = self.group.pop() {
            if !g.is_empty() {
                self.push(name, Op::Group(g));
            }
        }
    }
    pub fn clear(&mut self) {
        self.list.clear();
        self.pos = 0;
        self.bytes = 0;
    }
    pub fn can_undo(&self) -> bool {
        self.pos > 0
    }
    pub fn can_redo(&self) -> bool {
        self.pos < self.list.len()
    }
    /// Отменить: возвращает имя действия.
    pub fn undo(&mut self, doc: &mut Document, sel: &mut Option<Vec<u8>>) -> Option<String> {
        if self.pos == 0 {
            return None;
        }
        self.pos -= 1;
        let e = &mut self.list[self.pos];
        apply(&mut e.op, doc, sel, true);
        Some(e.name.clone())
    }
    pub fn redo(&mut self, doc: &mut Document, sel: &mut Option<Vec<u8>>) -> Option<String> {
        if self.pos >= self.list.len() {
            return None;
        }
        let e = &mut self.list[self.pos];
        apply(&mut e.op, doc, sel, false);
        self.pos += 1;
        Some(e.name.clone())
    }
}

fn apply(op: &mut Op, doc: &mut Document, sel: &mut Option<Vec<u8>>, undo: bool) {
    match op {
        Op::Pixels { layer, x, y, before, after } => {
            if let Some(i) = doc.find(layer) {
                let src = if undo { &*before } else { &*after };
                doc.layers[i].img_mut().put(*x, *y, src);
            }
        }
        Op::Props { layer, before, after } => {
            if let Some(i) = doc.find(layer) {
                doc.layers[i].set_props(if undo { before } else { after });
            }
        }
        Op::Mask { layer, x, y, before, after } => {
            if let Some(i) = doc.find(layer) {
                let l = &mut doc.layers[i];
                if let Some(m) = &mut l.mask { Arc::make_mut(m).put(*x, *y, if undo { &*before } else { &*after }); }
                l.ver += 1;
            }
        }
        Op::MaskSet { layer, before, after } => {
            if let Some(i) = doc.find(layer) {
                doc.layers[i].mask = if undo { before.clone() } else { after.clone() };
                doc.layers[i].ver += 1;
            }
        }
        Op::Tree { other, active } => {
            std::mem::swap(&mut doc.layers, other);
            std::mem::swap(&mut doc.active, active);
            doc.active = doc.active.min(doc.layers.len().saturating_sub(1));
        }
        Op::Text { layer, before, after } => {
            if let Some(i) = doc.find(layer) {
                doc.layers[i].text = if undo { before.clone() } else { after.clone() };
            }
        }
        Op::Vector { layer, before, after } => {
            if let Some(i) = doc.find(layer) {
                doc.layers[i].vector = if undo { before.clone() } else { after.clone() };
                doc.layers[i].ver += 1;
            }
        }
        Op::Insert { index, layer, prev_active } => {
            if undo {
                if *index < doc.layers.len() {
                    *layer = Some(doc.layers.remove(*index));
                }
                doc.active = (*prev_active).min(doc.layers.len().saturating_sub(1));
            } else if let Some(l) = layer.take() {
                let i = (*index).min(doc.layers.len());
                doc.layers.insert(i, l);
                doc.active = i;
            }
        }
        Op::Remove { index, layer, prev_active } => {
            if undo {
                if let Some(l) = layer.take() {
                    let i = (*index).min(doc.layers.len());
                    doc.layers.insert(i, l);
                    doc.active = *prev_active;
                }
            } else if *index < doc.layers.len() {
                *layer = Some(doc.layers.remove(*index));
                doc.active = index.saturating_sub(1).min(doc.layers.len().saturating_sub(1));
            }
        }
        Op::Move { from, to } => {
            let (a, b) = if undo { (*to, *from) } else { (*from, *to) };
            if a < doc.layers.len() && b < doc.layers.len() {
                let l = doc.layers.remove(a);
                doc.layers.insert(b, l);
                doc.active = b;
            }
        }
        Op::Layers { other, w, h, active } => {
            std::mem::swap(&mut doc.layers, other);
            std::mem::swap(&mut doc.w, w);
            std::mem::swap(&mut doc.h, h);
            std::mem::swap(&mut doc.active, active);
            doc.active = doc.active.min(doc.layers.len().saturating_sub(1));
            if sel.as_ref().map_or(false, |m| m.len() != doc.w * doc.h) {
                *sel = None;
            }
        }
        Op::Sel { other } => std::mem::swap(sel, other),
        Op::Bg { other } => {
            let cur = (doc.bg, doc.bg_visible);
            doc.bg = other.0;
            doc.bg_visible = other.1;
            *other = cur;
        }
        Op::Group(v) => {
            if undo {
                for o in v.iter_mut().rev() {
                    apply(o, doc, sel, true);
                }
            } else {
                for o in v.iter_mut() {
                    apply(o, doc, sel, false);
                }
            }
        }
    }
}
