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
            ver: 1,
        }
    }
    pub fn img_mut(&mut self) -> &mut Img {
        self.ver += 1;
        Arc::make_mut(&mut self.img)
    }
    pub fn props(&self) -> Props {
        Props { name: self.name.clone(), visible: self.visible, opacity: self.opacity, blend: self.blend, alpha_lock: self.alpha_lock, clip: self.clip, locked: self.locked, reference: self.reference }
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
        self.ver += 1;
    }
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

/// Пересчитать композицию (с фоном) в прямоугольнике r. ov — подмена содержимого активного слоя.
pub fn render(doc: &Document, comp: &mut Img, r: Rect, ov: Option<(usize, &Img)>) {
    render_with(doc, comp, r, ov, doc.bg_visible);
}

pub fn render_with(doc: &Document, comp: &mut Img, r: Rect, ov: Option<(usize, &Img)>, with_bg: bool) {
    let Some(r) = r.clip(doc.w, doc.h) else { return };
    let w = doc.w;
    let bg = if with_bg { [doc.bg[0] as f32 / 255.0, doc.bg[1] as f32 / 255.0, doc.bg[2] as f32 / 255.0, 1.0] } else { [0.0; 4] };
    let layers = &doc.layers;
    let n = layers.len();
    let src_of = |i: usize| -> &Img {
        match ov {
            Some((k, img)) if k == i => img,
            _ => &layers[i].img,
        }
    };
    comp.px.par_chunks_mut(w * 4).enumerate().skip(r.y as usize).take(r.h as usize).for_each(|(y, row)| {
        let x0 = r.x as usize;
        let x1 = r.right() as usize;
        let mut buf: Vec<[f32; 4]> = vec![bg; x1 - x0];
        let mut grp: Vec<[f32; 4]> = Vec::new();
        let mut i = 0;
        while i < n {
            let l = &layers[i];
            let mut j = i + 1;
            while j < n && layers[j].clip {
                j += 1;
            }
            if l.visible && l.opacity > 0.0 {
                let src = src_of(i);
                let srow = &src.px[y * w * 4..(y + 1) * w * 4];
                let has_clips = (i + 1..j).any(|k| layers[k].visible && layers[k].opacity > 0.0);
                if !has_clips {
                    for x in x0..x1 {
                        composite(&mut buf[x - x0], &srow[x * 4..x * 4 + 4], l.blend, l.opacity, x as i32, y as i32);
                    }
                } else {
                    grp.clear();
                    grp.extend((x0..x1).map(|x| {
                        let p = &srow[x * 4..x * 4 + 4];
                        [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]
                    }));
                    for k in i + 1..j {
                        let c = &layers[k];
                        if !c.visible || c.opacity <= 0.0 {
                            continue;
                        }
                        let crow = &src_of(k).px[y * w * 4..(y + 1) * w * 4];
                        for x in x0..x1 {
                            let ba = srow[x * 4 + 3] as f32 / 255.0;
                            if ba <= 0.0 {
                                continue;
                            }
                            let p = &crow[x * 4..x * 4 + 4];
                            let s = [(p[0] as f32 * ba) as u8, (p[1] as f32 * ba) as u8, (p[2] as f32 * ba) as u8, (p[3] as f32 * ba) as u8];
                            composite(&mut grp[x - x0], &s, c.blend, c.opacity, x as i32, y as i32);
                        }
                    }
                    for x in x0..x1 {
                        let g = grp[x - x0];
                        let s = [(g[0] * 255.0).round() as u8, (g[1] * 255.0).round() as u8, (g[2] * 255.0).round() as u8, (g[3] * 255.0).round() as u8];
                        composite(&mut buf[x - x0], &s, l.blend, l.opacity, x as i32, y as i32);
                    }
                }
            }
            i = j;
        }
        for x in x0..x1 {
            let b = buf[x - x0];
            let o = x * 4;
            row[o] = (b[0] * 255.0).round().clamp(0.0, 255.0) as u8;
            row[o + 1] = (b[1] * 255.0).round().clamp(0.0, 255.0) as u8;
            row[o + 2] = (b[2] * 255.0).round().clamp(0.0, 255.0) as u8;
            row[o + 3] = (b[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    });
}

/// Плоское изображение всего документа (опционально без фона).
pub fn flatten(doc: &Document, with_bg: bool) -> Img {
    let mut out = Img::new(doc.w, doc.h);
    render_with(doc, &mut out, doc.full(), None, with_bg && doc.bg_visible);
    out
}

/* ---------- история ---------- */

pub enum Op {
    Pixels { layer: String, x: i32, y: i32, before: Img, after: Img },
    Props { layer: String, before: Props, after: Props },
    Text { layer: String, before: Option<TextSpec>, after: Option<TextSpec> },
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
        Op::Pixels { before, after, .. } => before.px.len() + after.px.len(),
        Op::Layers { other, .. } => other.iter().map(|l| l.img.px.len()).sum(),
        Op::Sel { other } => other.as_ref().map_or(0, |v| v.len()),
        Op::Insert { layer, .. } | Op::Remove { layer, .. } => layer.as_ref().map_or(0, |l| l.img.px.len()),
        Op::Group(v) => v.iter().map(op_bytes).sum(),
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
        Op::Text { layer, before, after } => {
            if let Some(i) = doc.find(layer) {
                doc.layers[i].text = if undo { before.clone() } else { after.clone() };
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
