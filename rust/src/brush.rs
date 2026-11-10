//! Движок кистей: параметры, библиотека, формы отпечатка, зерно, штрих и быстрые фигуры.

use crate::raster::{fill_polys, flatten_cubic, flatten_quad, seg_dist};
use crate::util::{self, clamp01, smooth, Rect, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Brush {
    pub id: String,
    pub name: String,
    pub cat: String,
    pub shape: String,
    pub hardness: f32,
    pub roundness: f32,
    pub angle: f32,
    pub follow: bool,
    pub spacing: f32,
    pub streamline: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub size: f32,
    pub opacity: f32,
    pub flow: f32,
    pub p_size: f32,
    pub p_flow: f32,
    pub min_size: f32,
    pub tilt_size: f32,
    pub j_size: f32,
    pub j_opacity: f32,
    pub j_angle: f32,
    pub scatter: f32,
    pub count: f32,
    pub j_hue: f32,
    pub j_sat: f32,
    pub j_bri: f32,
    pub grain: Option<String>,
    pub grain_scale: f32,
    pub grain_strength: f32,
    pub taper_start: f32,
    pub taper_end: f32,
    pub taper_min: f32,
    pub smudge: f32,
    pub pixel: bool,
    pub blend: Option<String>,
    #[serde(skip)]
    pub preset: bool,
}

impl Default for Brush {
    fn default() -> Self {
        Brush {
            id: String::new(), name: "Кисть".into(), cat: "custom".into(), shape: "round".into(), hardness: 0.85, roundness: 1.0, angle: 0.0,
            follow: false, spacing: 0.08, streamline: 0.25, size_min: 1.0, size_max: 60.0, size: 0.3, opacity: 1.0, flow: 1.0, p_size: 0.7,
            p_flow: 0.2, min_size: 0.08, tilt_size: 0.0, j_size: 0.0, j_opacity: 0.0, j_angle: 0.0, scatter: 0.0, count: 1.0, j_hue: 0.0,
            j_sat: 0.0, j_bri: 0.0, grain: None, grain_scale: 1.0, grain_strength: 0.5, taper_start: 0.0, taper_end: 0.0, taper_min: 0.1,
            smudge: 0.6, pixel: false, blend: None, preset: false,
        }
    }
}

pub const CATS: [(&str, &str); 10] = [
    ("sketch", "Эскиз"), ("ink", "Тушь"), ("paint", "Живопись"), ("air", "Аэрограф"), ("texture", "Текстура"),
    ("nature", "Природа"), ("fx", "Декор"), ("pixel", "Пиксель"), ("custom", "Мои кисти"), ("imported", "Импорт"),
];

/* ---------- свои формы отпечатка и текстуры из картинок ---------- */

static ASSETS_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Папка, где хранятся формы отпечатка (tips/) и текстуры (grains/) из картинок.
pub fn set_assets_dir(p: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(p.join("tips"));
    let _ = std::fs::create_dir_all(p.join("grains"));
    let _ = ASSETS_DIR.set(p);
}

fn image_store() -> &'static Mutex<HashMap<String, Arc<Vec<f32>>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<Vec<f32>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Загрузить маску (size×size) из реестра или с диска.
fn stored_mask(kind: &str, id: &str, size: usize) -> Option<Arc<Vec<f32>>> {
    let key = format!("{kind}/{id}");
    if let Some(m) = image_store().lock().unwrap().get(&key) {
        return Some(m.clone());
    }
    let path = ASSETS_DIR.get()?.join(kind).join(format!("{id}.png"));
    let img = crate::formats::decode_image(&std::fs::read(path).ok()?).ok()?;
    let img = if img.w != size || img.h != size { img.resize(size, size) } else { img };
    let m: Vec<f32> = img.px.chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
    let m = Arc::new(m);
    image_store().lock().unwrap().insert(key, m.clone());
    Some(m)
}

/// Сохранить маску как PNG (белый цвет, маска в альфа-канале) и зарегистрировать.
fn store_mask(kind: &str, id: &str, size: usize, m: Vec<f32>) {
    let mut img = crate::raster::Img::new(size, size);
    for (i, v) in m.iter().enumerate() {
        let a = (clamp01(*v) * 255.0).round() as u8;
        img.px[i * 4..i * 4 + 4].copy_from_slice(&[a, a, a, a]);
    }
    if let Some(dir) = ASSETS_DIR.get() {
        let _ = std::fs::write(dir.join(kind).join(format!("{id}.png")), crate::formats::png(&img));
    }
    image_store().lock().unwrap().insert(format!("{kind}/{id}"), Arc::new(m));
}

/// Маска 512×512 для формы отпечатка из произвольной картинки (сохраняет пропорции, по центру).
pub fn tip_mask_from_image(img: &crate::raster::Img) -> Vec<f32> {
    let (w, h) = (img.w, img.h);
    let n = w * h;
    let has_alpha = img.px.chunks_exact(4).any(|p| p[3] < 250);
    let lum = |p: &[u8]| -> f32 {
        let a = p[3] as f32 / 255.0;
        if a <= 0.0 { return 1.0; }
        (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0 / a
    };
    let mut v = vec![0f32; n];
    if has_alpha {
        for (i, p) in img.px.chunks_exact(4).enumerate() { v[i] = p[3] as f32 / 255.0; }
    } else {
        let mut border = 0.0f32;
        let mut cnt = 0.0f32;
        for x in 0..w { for y in [0, h - 1] { border += lum(&img.px[(y * w + x) * 4..]); cnt += 1.0; } }
        for y in 0..h { for x in [0, w - 1] { border += lum(&img.px[(y * w + x) * 4..]); cnt += 1.0; } }
        let light_bg = border / cnt.max(1.0) > 0.5;
        for (i, p) in img.px.chunks_exact(4).enumerate() { let l = lum(p).clamp(0.0, 1.0); v[i] = if light_bg { 1.0 - l } else { l }; }
    }
    // обрезка по содержимому
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h { for x in 0..w { if v[y * w + x] > 0.02 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } }
    if x1 <= x0 || y1 <= y0 { return vec![0.0; 512 * 512]; }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let side = cw.max(ch);
    let mut sq = crate::raster::Img::new(side, side);
    let (ox, oy) = ((side - cw) / 2, (side - ch) / 2);
    for y in 0..ch { for x in 0..cw {
        let a = (v[(y0 + y) * w + x0 + x] * 255.0).round() as u8;
        let o = ((oy + y) * side + ox + x) * 4;
        sq.px[o..o + 4].copy_from_slice(&[a, a, a, a]);
    } }
    // небольшой отступ, чтобы край отпечатка не обрезался
    let inner = 492;
    let small = sq.resize(inner, inner);
    let mut out = vec![0f32; 512 * 512];
    let o = (512 - inner) / 2;
    for y in 0..inner { for x in 0..inner { out[(o + y) * 512 + o + x] = small.px[(y * inner + x) * 4 + 3] as f32 / 255.0; } }
    out
}

/// Текстура зерна 256×256 из картинки: светлое — краска проходит, тёмное — нет.
pub fn grain_from_image(img: &crate::raster::Img) -> Vec<f32> {
    let s = img.resize(GRAIN_SIZE, GRAIN_SIZE);
    s.px.chunks_exact(4).map(|p| {
        let a = p[3] as f32 / 255.0;
        let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0;
        clamp01(l + (1.0 - a))
    }).collect()
}

/// Зарегистрировать форму отпечатка; возвращает имя формы для Brush.shape.
pub fn add_tip_image(mask512: Vec<f32>) -> String {
    let id = util::uid();
    store_mask("tips", &id, 512, mask512);
    format!("img:{id}")
}

/// Зарегистрировать текстуру; возвращает id для Brush.grain.
pub fn add_grain_image(v: Vec<f32>) -> String {
    let id = util::uid();
    store_mask("grains", &id, GRAIN_SIZE, v);
    format!("img:{id}")
}

/// Маска формы для превью и экспорта (512×512).
pub fn tip_mask(shape: &str) -> Vec<f32> {
    tip(shape, 1.0, 512).a.clone()
}

/// Сгладить точки штриха после рисования (концы на месте).
pub fn smooth_points(pts: &mut [Pt], strength: f32) {
    let n = pts.len();
    if n < 4 || strength <= 0.0 { return; }
    let iters = (strength * 10.0).round().max(1.0) as usize;
    for _ in 0..iters {
        let prev: Vec<Pt> = pts.to_vec();
        for i in 1..n - 1 {
            pts[i].x = prev[i].x * 0.5 + (prev[i - 1].x + prev[i + 1].x) * 0.25;
            pts[i].y = prev[i].y * 0.5 + (prev[i - 1].y + prev[i + 1].y) * 0.25;
        }
    }
}

pub const SHAPES: [(&str, &str); 26] = [
    ("round", "Круглая"), ("square", "Квадратная"), ("chalk", "Меловая"), ("bristle", "Ворс"), ("rake", "Гребёнка"), ("spray", "Распыление"),
    ("splatter", "Клякса"), ("sponge", "Губка"), ("star", "Звезда"), ("leaf", "Лист"), ("hatch", "Штрихи"), ("grass", "Трава"),
    ("hair", "Волоски"), ("fur", "Мех"), ("cloud", "Облако"), ("rock", "Камень"), ("blot", "Клякса туши"), ("drybrush", "Сухая кисть"),
    ("triangle", "Треугольник"), ("heart", "Сердце"), ("snowflake", "Снежинка"), ("ring", "Кольцо"), ("petal", "Лепесток"), ("cross", "Крест"),
    ("dots", "Точки"), ("diamond", "Ромб"),
];

pub const GRAINS: [(&str, &str); 7] = [("paper", "Бумага"), ("canvas", "Холст"), ("noise", "Шум"), ("charcoal", "Уголь"), ("sponge", "Губка"), ("grit", "Крупка"), ("hatch", "Штриховка")];

/* ---------- библиотека ---------- */

#[derive(Default, Serialize, Deserialize)]
struct LibFile {
    custom: Vec<Brush>,
    overrides: HashMap<String, Brush>,
}

pub struct BrushLib {
    pub presets: Vec<Brush>,
    pub custom: Vec<Brush>,
    pub overrides: HashMap<String, Brush>,
    pub version: u64,
    path: Option<std::path::PathBuf>,
}

impl BrushLib {
    pub fn load(path: Option<std::path::PathBuf>) -> BrushLib {
        let mut presets: Vec<Brush> = serde_json::from_str(include_str!("presets.json")).unwrap_or_default();
        for b in presets.iter_mut() {
            b.preset = true;
        }
        let mut lib = BrushLib { presets, custom: vec![], overrides: HashMap::new(), version: 1, path };
        if let Some(p) = &lib.path {
            if let Ok(s) = std::fs::read_to_string(p) {
                if let Ok(f) = serde_json::from_str::<LibFile>(&s) {
                    lib.custom = f.custom;
                    lib.overrides = f.overrides;
                }
            }
        }
        for b in lib.overrides.values_mut() {
            b.preset = true;
        }
        lib
    }
    pub fn save(&self) {
        if let Some(p) = &self.path {
            let f = LibFile { custom: self.custom.clone(), overrides: self.overrides.clone() };
            if let Ok(s) = serde_json::to_string(&f) {
                let _ = std::fs::write(p, s);
            }
        }
    }
    pub fn get(&self, id: &str) -> &Brush {
        if let Some(b) = self.custom.iter().find(|b| b.id == id) {
            return b;
        }
        if let Some(b) = self.overrides.get(id) {
            return b;
        }
        self.presets.iter().find(|b| b.id == id).unwrap_or(&self.presets[0])
    }
    pub fn is_modified(&self, id: &str) -> bool {
        self.overrides.contains_key(id)
    }
    pub fn list(&self, cat: &str) -> Vec<&Brush> {
        if cat == "custom" {
            return self.custom.iter().filter(|b| !CATS.iter().any(|c| c.0 == b.cat) || b.cat == "custom").collect();
        }
        let mut v: Vec<&Brush> = self.presets.iter().filter(|b| b.cat == cat).map(|b| self.get(&b.id)).collect();
        v.extend(self.custom.iter().filter(|b| b.cat == cat));
        v
    }
    pub fn update(&mut self, b: Brush) {
        if let Some(c) = self.custom.iter_mut().find(|c| c.id == b.id) {
            *c = b;
        } else {
            let mut b = b;
            b.preset = true;
            self.overrides.insert(b.id.clone(), b);
        }
        self.version += 1;
    }
    pub fn reset(&mut self, id: &str) {
        self.overrides.remove(id);
        self.version += 1;
    }
    pub fn duplicate(&mut self, id: &str) -> String {
        let mut b = self.get(id).clone();
        b.id = format!("c_{}", util::uid());
        b.name = format!("{} (копия)", b.name);
        b.cat = "custom".into();
        b.preset = false;
        let id = b.id.clone();
        self.custom.push(b);
        self.version += 1;
        id
    }
    pub fn create(&mut self) -> String {
        let b = Brush { id: format!("c_{}", util::uid()), name: "Новая кисть".into(), ..Default::default() };
        let id = b.id.clone();
        self.custom.push(b);
        self.version += 1;
        id
    }
    pub fn remove(&mut self, id: &str) {
        self.custom.retain(|b| b.id != id);
        self.version += 1;
    }
    /// Добавить импортированные кисти; возвращает id первой.
    pub fn add_imported(&mut self, list: Vec<Brush>) -> Option<String> {
        let mut first = None;
        for mut b in list {
            b.id = format!("c_{}", util::uid());
            if b.cat.is_empty() { b.cat = "imported".into(); }
            b.preset = false;
            first.get_or_insert_with(|| b.id.clone());
            self.custom.push(b);
            // uid() зависит от времени — разводим идентификаторы
            std::thread::sleep(std::time::Duration::from_micros(50));
        }
        self.version += 1;
        first
    }
}

pub fn size_px(b: &Brush, t: f32) -> f32 {
    let s = b.size_min + (b.size_max - b.size_min) * t * t;
    if b.pixel { s.round().max(1.0) } else { s }
}

/* ---------- маска для генерации форм ---------- */

struct M {
    s: usize,
    a: Vec<f32>,
}

impl M {
    fn new(s: usize) -> M {
        M { s, a: vec![0.0; s * s] }
    }
    #[inline]
    fn over(&mut self, i: usize, v: f32) {
        let d = self.a[i];
        self.a[i] = d + v * (1.0 - d);
    }
    fn soft_dot(&mut self, cx: f32, cy: f32, r: f32, alpha: f32) {
        let s = self.s as i32;
        let (x0, x1) = (((cx - r).floor() as i32).max(0), ((cx + r).ceil() as i32).min(s));
        let (y0, y1) = (((cy - r).floor() as i32).max(0), ((cy + r).ceil() as i32).min(s));
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() / r.max(0.01);
                if d >= 1.0 {
                    continue;
                }
                let v = if d < 0.55 { alpha * (1.0 - 0.2 * d / 0.55) } else { alpha * 0.8 * (1.0 - (d - 0.55) / 0.45) };
                self.over(y as usize * self.s + x as usize, v);
            }
        }
    }
    fn circle(&mut self, cx: f32, cy: f32, r: f32, alpha: f32) {
        let s = self.s as i32;
        let (x0, x1) = (((cx - r - 1.0).floor() as i32).max(0), ((cx + r + 1.0).ceil() as i32).min(s));
        let (y0, y1) = (((cy - r - 1.0).floor() as i32).max(0), ((cy + r + 1.0).ceil() as i32).min(s));
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let c = (r - d + 0.5).clamp(0.0, 1.0);
                if c > 0.0 {
                    self.over(y as usize * self.s + x as usize, c * alpha);
                }
            }
        }
    }
    fn poly(&mut self, pts: &[(f32, f32)], alpha: f32) {
        let s = self.s;
        let mut cov = vec![];
        fill_polys(s, s, &[pts.to_vec()], false, |x, y, c| cov.push((y * s + x, c)));
        for (i, c) in cov {
            self.over(i, c * alpha);
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, alpha: f32, erase: bool) {
        let s = self.s as i32;
        let hw = width / 2.0;
        let (x0, x1) = (((a.0.min(b.0) - hw - 1.0).floor() as i32).max(0), ((a.0.max(b.0) + hw + 1.0).ceil() as i32).min(s));
        let (y0, y1) = (((a.1.min(b.1) - hw - 1.0).floor() as i32).max(0), ((a.1.max(b.1) + hw + 1.0).ceil() as i32).min(s));
        for y in y0..y1 {
            for x in x0..x1 {
                let d = seg_dist(x as f32 + 0.5, y as f32 + 0.5, a.0, a.1, b.0, b.1);
                let c = (hw - d + 0.5).clamp(0.0, 1.0) * alpha;
                if c <= 0.0 {
                    continue;
                }
                let i = y as usize * self.s + x as usize;
                if erase {
                    self.a[i] *= 1.0 - c;
                } else {
                    self.over(i, c);
                }
            }
        }
    }
    fn round(&mut self, h: f32) {
        let s = self.s as f32;
        let r = s / 2.0;
        for y in 0..self.s {
            for x in 0..self.s {
                let d = ((x as f32 + 0.5 - r).powi(2) + (y as f32 + 0.5 - r).powi(2)).sqrt();
                let v = if h >= 0.99 {
                    (r - 0.5 - d + 0.5).clamp(0.0, 1.0)
                } else {
                    let t = d / r;
                    if t >= 1.0 { 0.0 } else if t <= h { 1.0 } else { 1.0 - smooth((t - h) / (1.0 - h)) }
                };
                self.a[y * self.s + x] = v;
            }
        }
    }
    fn per_pixel(&mut self, f: impl Fn(f32, f32, f32) -> f32) {
        for y in 0..self.s {
            for x in 0..self.s {
                let i = y * self.s + x;
                let u = (x as f32 + 0.5) / self.s as f32;
                let v = (y as f32 + 0.5) / self.s as f32;
                self.a[i] = clamp01(f(u, v, self.a[i]));
            }
        }
    }
}

fn gen_tip(shape: &str, hard: f32, sz: usize) -> Vec<f32> {
    if let Some(id) = shape.strip_prefix("img:") {
        if let Some(m) = stored_mask("tips", id, 512) {
            return if sz == 512 { (*m).clone() } else { downsample(&m, 512) };
        }
        return gen_tip("round", hard, sz);
    }
    let mut m = M::new(sz);
    let s = sz as f32;
    let r = s / 2.0;
    let mut rng = Rng::new(4242);
    match shape {
        "square" => {
            let mg = s * 0.05;
            m.poly(&[(mg, mg), (s - mg, mg), (s - mg, s - mg), (mg, s - mg)], 1.0);
        }
        "chalk" => {
            m.round(hard.max(0.6));
            m.per_pixel(|u, v, a| {
                let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
                let n = util::value_noise(u * 9.0, v * 9.0, 3) * 0.55 + util::value_noise(u * 31.0, v * 31.0, 5) * 0.45;
                a * clamp01((1.35 - d * d) - (1.0 - n) * 1.1)
            });
        }
        "bristle" => {
            for _ in 0..48 {
                let t = rng.next() * TAU;
                let rr = rng.next().sqrt() * r * 0.82;
                let rad = r * (0.06 + 0.07 * rng.next());
                let a = 0.45 + 0.55 * rng.next();
                m.soft_dot(r + t.cos() * rr, r + t.sin() * rr, rad, a);
            }
        }
        "rake" => {
            for i in 0..11 {
                let y = s * (0.06 + 0.88 * i as f32 / 10.0) + (rng.next() - 0.5) * s * 0.03;
                let x = r + (rng.next() - 0.5) * s * 0.12;
                let rad = r * (0.07 + 0.05 * rng.next());
                let a = 0.5 + 0.5 * rng.next();
                m.soft_dot(x, y, rad, a);
            }
        }
        "spray" => {
            for _ in 0..320 {
                let g = (-2.0 * (rng.next() + 1e-6).ln()).sqrt() * (TAU * rng.next()).cos();
                let t = rng.next() * TAU;
                let rr = (g.abs() * 0.36).min(0.94) * r;
                let a = 0.55 + 0.45 * rng.next();
                let rad = (r * (0.012 + 0.022 * rng.next())).max(0.5);
                m.circle(r + t.cos() * rr, r + t.sin() * rr, rad, a);
            }
        }
        "splatter" => {
            for _ in 0..7 {
                let (x, y, rad) = (r + (rng.next() - 0.5) * r * 0.35, r + (rng.next() - 0.5) * r * 0.35, r * (0.2 + 0.14 * rng.next()));
                m.circle(x, y, rad, 1.0);
            }
            for _ in 0..22 {
                let t = rng.next() * TAU;
                let rr = r * (0.45 + 0.48 * rng.next());
                let k = rng.next();
                m.circle(r + t.cos() * rr, r + t.sin() * rr, r * (0.015 + 0.07 * k * rng.next()), 1.0);
            }
        }
        "sponge" => m.per_pixel(|u, v, _| {
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
            let n = util::value_noise(u * 6.0, v * 6.0, 11) * 0.6 + util::value_noise(u * 15.0, v * 15.0, 13) * 0.4;
            smooth((n - 0.42) * 4.0) * (1.0 - smooth((d - 0.7) / 0.3))
        }),
        "star" => {
            let pts: Vec<(f32, f32)> = (0..10).map(|i| {
                let a = -PI / 2.0 + i as f32 * PI / 5.0;
                let rr = if i % 2 == 1 { r * 0.42 } else { r * 0.96 };
                (r + a.cos() * rr, r + a.sin() * rr)
            }).collect();
            m.poly(&pts, 1.0);
        }
        "leaf" | "petal" => {
            let leaf = shape == "leaf";
            let mut p = vec![(r, s * 0.03)];
            if leaf {
                flatten_cubic(&mut p, (r, s * 0.03), (s * 0.96, s * 0.3), (s * 0.8, s * 0.8), (r, s * 0.98), 24);
                flatten_cubic(&mut p, (r, s * 0.98), (s * 0.2, s * 0.8), (s * 0.04, s * 0.3), (r, s * 0.03), 24);
            } else {
                flatten_cubic(&mut p, (r, s * 0.04), (s * 0.98, s * 0.25), (s * 0.85, s * 0.9), (r, s * 0.97), 24);
                flatten_cubic(&mut p, (r, s * 0.97), (s * 0.15, s * 0.9), (s * 0.02, s * 0.25), (r, s * 0.04), 24);
            }
            m.poly(&p, 1.0);
            if leaf {
                m.line((r, s * 0.12), (r, s * 0.9), (s * 0.025).max(1.0), 1.0, true);
            }
        }
        "hatch" => {
            for i in 0..5 {
                let o = s * (0.12 + i as f32 * 0.19);
                m.line((o - s * 0.1, s * 0.9), (o + s * 0.1, s * 0.1), (s * 0.06).max(1.0), 1.0, false);
            }
        }
        "grass" => {
            for _ in 0..11 {
                let bx = r + (rng.next() - 0.5) * s * 0.55;
                let by = s * 0.99;
                let tx = bx + (rng.next() - 0.5) * s * 0.6;
                let ty = s * (0.03 + 0.45 * rng.next());
                let w = s * (0.018 + 0.02 * rng.next());
                let cx = (bx + tx) / 2.0 + (rng.next() - 0.5) * s * 0.15;
                let cy = (by + ty) / 2.0;
                let a = 0.6 + 0.4 * rng.next();
                let mut p = vec![(bx - w, by)];
                flatten_quad(&mut p, (bx - w, by), (cx - w * 0.5, cy), (tx, ty), 12);
                flatten_quad(&mut p, (tx, ty), (cx + w * 0.5, cy), (bx + w, by), 12);
                m.poly(&p, a);
            }
        }
        "hair" => {
            for i in 0..26 {
                let y = s * (0.04 + 0.92 * i as f32 / 25.0) + (rng.next() - 0.5) * s * 0.02;
                let a = 0.25 + 0.75 * rng.next();
                let x = r + (rng.next() - 0.5) * s * 0.05;
                m.circle(x, y, (s * (0.008 + 0.012 * rng.next())).max(0.5), a);
            }
        }
        "fur" => {
            for _ in 0..70 {
                let a = rng.next() * TAU;
                let r0 = r * 0.1 * rng.next();
                let r1 = r * (0.55 + 0.42 * rng.next());
                let al = 0.3 + 0.6 * rng.next();
                let a2 = a + (rng.next() - 0.5) * 0.3;
                m.line((r + a.cos() * r0, r + a.sin() * r0), (r + a2.cos() * r1, r + a2.sin() * r1), (s * 0.012).max(0.6), al, false);
            }
        }
        "cloud" => {
            for _ in 0..9 {
                let a = rng.next() * TAU;
                let d = r * 0.35 * rng.next().sqrt();
                let rad = r * (0.3 + 0.25 * rng.next());
                let al = 0.45 + 0.4 * rng.next();
                m.soft_dot(r + a.cos() * d, r + a.sin() * d * 0.7, rad, al);
            }
        }
        "rock" => {
            let pts: Vec<(f32, f32)> = (0..8).map(|i| {
                let a = i as f32 / 8.0 * TAU;
                let rr = r * (0.6 + 0.35 * rng.next());
                (r + a.cos() * rr, r + a.sin() * rr)
            }).collect();
            m.poly(&pts, 1.0);
            m.per_pixel(|u, v, a| a * (0.55 + 0.45 * util::value_noise(u * 12.0, v * 12.0, 17)));
        }
        "blot" => m.per_pixel(|u, v, _| {
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
            let n = util::value_noise(u * 5.0, v * 5.0, 23) * 0.6 + util::value_noise(u * 14.0, v * 14.0, 29) * 0.4;
            smooth((1.0 - d) * 2.2 + (n - 0.5) * 1.6)
        }),
        "drybrush" => {
            for i in 0..30 {
                if rng.next() < 0.3 {
                    continue;
                }
                let y = s * (0.03 + 0.94 * i as f32 / 29.0);
                let x = r + (rng.next() - 0.5) * s * 0.1;
                let rad = r * (0.03 + 0.05 * rng.next());
                let a = 0.25 + 0.75 * rng.next();
                m.soft_dot(x, y, rad, a);
            }
        }
        "triangle" => m.poly(&[(r, s * 0.06), (s * 0.95, s * 0.9), (s * 0.05, s * 0.9)], 1.0),
        "diamond" => m.poly(&[(r, s * 0.04), (s * 0.8, r), (r, s * 0.96), (s * 0.2, r)], 1.0),
        "heart" => {
            let mut p = vec![(r, s * 0.92)];
            flatten_cubic(&mut p, (r, s * 0.92), (s * 0.1, s * 0.62), (0.0, s * 0.3), (s * 0.27, s * 0.16), 16);
            flatten_cubic(&mut p, (s * 0.27, s * 0.16), (s * 0.42, s * 0.08), (r, s * 0.22), (r, s * 0.3), 12);
            flatten_cubic(&mut p, (r, s * 0.3), (r, s * 0.22), (s * 0.58, s * 0.08), (s * 0.73, s * 0.16), 12);
            flatten_cubic(&mut p, (s * 0.73, s * 0.16), (s, s * 0.3), (s * 0.9, s * 0.62), (r, s * 0.92), 16);
            m.poly(&p, 1.0);
        }
        "snowflake" => {
            let lw = (s * 0.06).max(1.0);
            for i in 0..6 {
                let a = i as f32 * PI / 3.0;
                let (c, sn) = (a.cos(), a.sin());
                let l1 = r * 0.92;
                m.line((r, r), (r + c * l1, r + sn * l1), lw, 1.0, false);
                for t in [0.45, 0.7] {
                    let (px, py) = (r + c * l1 * t, r + sn * l1 * t);
                    let l = r * 0.22;
                    for sg in [-1.0, 1.0] {
                        let b = a + sg * 0.7;
                        m.line((px, py), (px + b.cos() * l, py + b.sin() * l), lw, 1.0, false);
                    }
                }
            }
        }
        "ring" => m.per_pixel(|u, v, _| {
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * s;
            ((s * 0.05) - (d - r * 0.85).abs() + 0.5).clamp(0.0, 1.0)
        }),
        "cross" => {
            let lw = (s * 0.1).max(1.0);
            m.line((s * 0.15, s * 0.15), (s * 0.85, s * 0.85), lw, 1.0, false);
            m.line((s * 0.85, s * 0.15), (s * 0.15, s * 0.85), lw, 1.0, false);
        }
        "dots" => {
            for (x, y) in [(0.3, 0.35), (0.68, 0.3), (0.5, 0.7)] {
                m.circle(s * x, s * y, s * 0.16, 1.0);
            }
        }
        _ => m.round(hard),
    }
    m.a
}

pub struct Tip {
    pub size: usize,
    pub a: Vec<f32>,
}

impl Tip {
    #[inline]
    pub fn sample(&self, fx: f32, fy: f32) -> f32 {
        let s = self.size as i32;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as i32, y0 as i32);
        let g = |x: i32, y: i32| -> f32 { if x < 0 || y < 0 || x >= s || y >= s { 0.0 } else { self.a[(y * s + x) as usize] } };
        let a = g(x0, y0);
        let b = g(x0 + 1, y0);
        let c = g(x0, y0 + 1);
        let d = g(x0 + 1, y0 + 1);
        (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + d * tx) * ty
    }
}

fn downsample(a: &[f32], s: usize) -> Vec<f32> {
    let h = s / 2;
    let mut o = vec![0.0; h * h];
    for y in 0..h {
        for x in 0..h {
            o[y * h + x] = (a[(2 * y) * s + 2 * x] + a[(2 * y) * s + 2 * x + 1] + a[(2 * y + 1) * s + 2 * x] + a[(2 * y + 1) * s + 2 * x + 1]) * 0.25;
        }
    }
    o
}

type TipKey = (String, u8, usize);
fn tip_cache() -> &'static Mutex<HashMap<TipKey, Arc<Tip>>> {
    static C: OnceLock<Mutex<HashMap<TipKey, Arc<Tip>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn level_for(size: f32) -> usize {
    for l in [8usize, 16, 32, 64, 128, 256, 512] {
        if l as f32 >= size {
            return l;
        }
    }
    512
}

/// Форма отпечатка нужного уровня детализации (с кэшем).
pub fn tip(shape: &str, hardness: f32, level: usize) -> Arc<Tip> {
    let hq = (hardness.clamp(0.0, 1.0) * 100.0).round() as u8;
    let key = (shape.to_string(), hq, level);
    if let Some(t) = tip_cache().lock().unwrap().get(&key) {
        return t.clone();
    }
    let t = if level >= 512 {
        Arc::new(Tip { size: 512, a: gen_tip(shape, hardness, 512) })
    } else if level == 256 {
        Arc::new(Tip { size: 256, a: gen_tip(shape, hardness, 256) })
    } else {
        let up = tip(shape, hardness, level * 2);
        Arc::new(Tip { size: level, a: downsample(&up.a, level * 2) })
    };
    tip_cache().lock().unwrap().insert(key, t.clone());
    t
}

/* ---------- зерно ---------- */

pub struct Grain {
    pub v: Vec<f32>,
}
pub const GRAIN_SIZE: usize = 256;

fn gen_grain(id: &str) -> Vec<f32> {
    let s = GRAIN_SIZE;
    let mut r = Rng::new(777 + id.len() as u32 * 13);
    let c = |v: f32| clamp01(v);
    match id {
        "canvas" => {
            let n = util::fbm(s, &[64.0, 128.0], &mut r, 1.0, 1.0);
            let k = TAU * 32.0 / s as f32;
            let mut o = vec![0.0; s * s];
            for y in 0..s {
                for x in 0..s {
                    let w = 0.5 + 0.25 * (x as f32 * k).sin() * if y % 16 < 8 { 1.0 } else { -1.0 } + 0.25 * (y as f32 * k).sin();
                    o[y * s + x] = c(w * 0.7 + (n[y * s + x] - 0.5) * 0.8 + 0.25);
                }
            }
            o
        }
        "noise" => (0..s * s).map(|_| r.next().powf(0.6)).collect(),
        "charcoal" => {
            let a = util::fbm(s, &[4.0, 8.0, 16.0, 32.0], &mut r, 0.25, 1.0);
            a.iter().map(|&v| c((v - 0.45) * 3.0 + (r.next() - 0.5) * 0.5 + 0.5)).collect()
        }
        "sponge" => util::fbm(s, &[6.0, 12.0, 24.0], &mut r, 1.0, 1.0).iter().map(|&v| smooth((v - 0.38) * 3.2)).collect(),
        "grit" => {
            let n = util::fbm(s, &[32.0, 64.0], &mut r, 1.0, 1.0);
            n.iter().map(|&v| if v + (r.next() - 0.5) * 0.35 > 0.52 { 1.0 } else { 0.15 }).collect()
        }
        "hatch" => {
            let mut o = vec![0.0; s * s];
            for y in 0..s {
                for x in 0..s {
                    let t = ((x + y) % 16) as f32 / 16.0;
                    o[y * s + x] = if t < 0.45 { 1.0 } else { 0.1 + r.next() * 0.1 };
                }
            }
            o
        }
        _ => util::fbm(s, &[16.0, 32.0, 64.0, 128.0], &mut r, 1.0, 1.0).iter().map(|&v| c((v - 0.5) * 2.6 + 0.6)).collect(),
    }
}

pub fn grain(id: &str) -> Arc<Grain> {
    static C: OnceLock<Mutex<HashMap<String, Arc<Grain>>>> = OnceLock::new();
    let c = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(g) = c.lock().unwrap().get(id) {
        return g.clone();
    }
    let v = match id.strip_prefix("img:") {
        Some(iid) => stored_mask("grains", iid, GRAIN_SIZE).map(|m| (*m).clone()).unwrap_or_else(|| gen_grain("paper")),
        None => gen_grain(id),
    };
    let g = Arc::new(Grain { v });
    c.lock().unwrap().insert(id.to_string(), g.clone());
    g
}

impl Grain {
    /// Множитель зерна в точке холста.
    #[inline]
    pub fn at(&self, x: i32, y: i32, scale: f32, strength: f32) -> f32 {
        let gx = ((x as f32 / scale) as i32).rem_euclid(GRAIN_SIZE as i32) as usize;
        let gy = ((y as f32 / scale) as i32).rem_euclid(GRAIN_SIZE as i32) as usize;
        1.0 - strength * (1.0 - self.v[gy * GRAIN_SIZE + gx])
    }
}

/* ---------- штрих ---------- */

#[derive(Clone, Copy, Debug, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
    pub p: f32,
    pub tilt: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Mirror {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub cx: f32,
    pub cy: f32,
}

/// Один отпечаток кисти.
#[derive(Clone, Copy, Debug)]
pub struct Stamp {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub alpha: f32,
    pub angle: f32,
    pub color: [f32; 3],
    pub mirror: usize,
}

pub trait Sink {
    fn stamp(&mut self, s: &Stamp);
}

pub struct Stroke {
    pub b: Brush,
    pub base: f32,
    color: [f32; 3],
    hsv: (f32, f32, f32),
    jitter_col: bool,
    pub mirrors: Vec<Mirror>,
    seed: u32,
    rng: Rng,
    pub pts: Vec<Pt>,
    sm: Option<Pt>,
    last_raw: Option<Pt>,
    dist: f32,
    next: f32,
    mid: Option<Pt>,
    dir: f32,
    last_size: f32,
    total: Option<f32>,
    pub dirty: Option<Rect>,
    pub gamma: f32,
    pub streamline_mul: f32,
    /// Радиус «ленивой кисти» (стабилизатор с нитью) в пикселях холста.
    pub lazy_r: f32,
    lazy: Option<Pt>,
}

impl Stroke {
    pub fn seed(&self) -> u32 {
        self.seed
    }
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.rng = Rng::new(seed);
    }
    /// Положение кисти при включённом стабилизаторе.
    pub fn lazy_pos(&self) -> Option<Pt> {
        if self.lazy_r > 0.0 { self.lazy } else { None }
    }
    pub fn last_input(&self) -> Option<Pt> {
        self.last_raw
    }
    pub fn new(b: Brush, size: f32, rgb: [u8; 3], mirrors: Vec<Mirror>, gamma: f32, streamline_mul: f32) -> Stroke {
        let jitter_col = b.j_hue > 0.0 || b.j_sat > 0.0 || b.j_bri > 0.0;
        let seed = util::random_seed();
        Stroke {
            base: size,
            color: [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0],
            hsv: util::rgb2hsv(rgb),
            jitter_col,
            mirrors,
            seed,
            rng: Rng::new(seed),
            pts: vec![],
            sm: None,
            last_raw: None,
            dist: 0.0,
            next: 0.0,
            mid: None,
            dir: 0.0,
            last_size: size,
            total: None,
            dirty: None,
            gamma,
            streamline_mul,
            lazy_r: 0.0,
            lazy: None,
            b,
        }
    }
    fn reset(&mut self) {
        self.rng = Rng::new(self.seed);
        self.dist = 0.0;
        self.next = 0.0;
        self.mid = None;
        self.dir = 0.0;
        self.last_size = self.base;
    }
    pub fn add(&mut self, x: f32, y: f32, p: f32, tilt: f32, sink: &mut dyn Sink) {
        let mut raw = Pt { x, y, p, tilt };
        if self.lazy_r > 0.0 {
            // Стабилизатор «нить»: кисть догоняет перо, только когда нить натянута.
            let l = match self.lazy {
                None => raw,
                Some(l) => {
                    let (dx, dy) = (x - l.x, y - l.y);
                    let d = dx.hypot(dy);
                    if d <= self.lazy_r {
                        self.lazy = Some(Pt { p: l.p.max(p * 0.5 + l.p * 0.5), ..l });
                        if let Some(last) = self.pts.last_mut() { last.p = last.p.max(self.lazy.unwrap().p); }
                        self.last_raw = self.lazy;
                        return;
                    }
                    let k = (d - self.lazy_r) / d;
                    Pt { x: l.x + dx * k, y: l.y + dy * k, p: l.p + (p - l.p) * 0.5, tilt }
                }
            };
            self.lazy = Some(l);
            raw = l;
        }
        self.last_raw = Some(raw);
        let next = match self.sm {
            None => raw,
            Some(sm) => {
                let s = (self.b.streamline * self.streamline_mul).clamp(0.0, 0.97);
                let k = 1.0 - s * 0.93;
                let kp = k.max(0.4);
                Pt { x: sm.x + (x - sm.x) * k, y: sm.y + (y - sm.y) * k, p: sm.p + (p - sm.p) * kp, tilt }
            }
        };
        self.sm = Some(next);
        self.feed(next, sink);
    }
    fn feed(&mut self, pt: Pt, sink: &mut dyn Sink) {
        if let Some(l) = self.pts.last_mut() {
            if (pt.x - l.x).abs() + (pt.y - l.y).abs() < 0.5 {
                l.p = l.p.max(pt.p);
                return;
            }
        }
        self.pts.push(pt);
        let i = self.pts.len() - 1;
        self.render_at(i, sink);
    }
    fn render_at(&mut self, i: usize, sink: &mut dyn Sink) {
        if i == 0 {
            let p0 = self.pts[0];
            self.mid = Some(p0);
            self.stamp_at(p0, 0.0, sink);
            self.next = self.spacing();
            return;
        }
        let c = self.pts[i - 1];
        let n = self.pts[i];
        let b = Pt { x: (c.x + n.x) / 2.0, y: (c.y + n.y) / 2.0, p: (c.p + n.p) / 2.0, tilt: n.tilt };
        let a = self.mid.unwrap_or(c);
        self.walk_quad(a, c, b, sink);
        self.mid = Some(b);
    }
    fn quad_steps(a: Pt, c: Pt, b: Pt) -> usize {
        ((((c.x - a.x).hypot(c.y - a.y)) + ((b.x - c.x).hypot(b.y - c.y))) / 2.0).ceil().max(1.0) as usize
    }
    fn walk_quad(&mut self, a: Pt, c: Pt, b: Pt, sink: &mut dyn Sink) {
        let n = Self::quad_steps(a, c, b);
        let mut prev = a;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            let u = 1.0 - t;
            let q = Pt { x: u * u * a.x + 2.0 * u * t * c.x + t * t * b.x, y: u * u * a.y + 2.0 * u * t * c.y + t * t * b.y, p: a.p + (b.p - a.p) * t, tilt: b.tilt };
            self.walk_line(prev, q, sink);
            prev = q;
        }
    }
    fn walk_line(&mut self, a: Pt, b: Pt, sink: &mut dyn Sink) {
        let len = (b.x - a.x).hypot(b.y - a.y);
        if len < 1e-6 {
            return;
        }
        let d0 = self.dist;
        let d1 = d0 + len;
        self.dir = (b.y - a.y).atan2(b.x - a.x);
        let mut guard = 0;
        while self.next <= d1 && guard < 100_000 {
            guard += 1;
            let t = (self.next - d0) / len;
            let pt = Pt { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, p: a.p + (b.p - a.p) * t, tilt: b.tilt };
            let at = self.next;
            self.stamp_at(pt, at, sink);
            self.next += self.spacing();
        }
        self.dist = d1;
    }
    fn spacing(&self) -> f32 {
        if self.b.pixel { (self.last_size * 0.5).clamp(0.5, 1.0) } else { (self.b.spacing * self.last_size).max(0.35) }
    }
    pub fn measure(&self) -> f32 {
        let p = &self.pts;
        if p.len() < 2 {
            return 0.0;
        }
        let mut l = 0.0;
        let mut mid = p[0];
        for i in 1..p.len() {
            let c = p[i - 1];
            let n = p[i];
            let b = Pt { x: (c.x + n.x) / 2.0, y: (c.y + n.y) / 2.0, ..n };
            let k = Self::quad_steps(mid, c, b);
            let mut prev = mid;
            for j in 1..=k {
                let t = j as f32 / k as f32;
                let u = 1.0 - t;
                let q = (u * u * mid.x + 2.0 * u * t * c.x + t * t * b.x, u * u * mid.y + 2.0 * u * t * c.y + t * t * b.y);
                l += (q.0 - prev.x).hypot(q.1 - prev.y);
                prev = Pt { x: q.0, y: q.1, ..prev };
            }
            mid = b;
        }
        let last = p[p.len() - 1];
        l + (last.x - mid.x).hypot(last.y - mid.y)
    }
    pub fn catch_up(&mut self, sink: &mut dyn Sink) {
        let (Some(a), Some(r)) = (self.sm, self.last_raw) else { return };
        if self.b.streamline <= 0.0 {
            return;
        }
        let d = (r.x - a.x).hypot(r.y - a.y);
        if d > 0.5 {
            let n = ((d / 2.0).ceil() as usize).min(16);
            for i in 1..=n {
                let t = i as f32 / n as f32;
                self.feed(Pt { x: a.x + (r.x - a.x) * t, y: a.y + (r.y - a.y) * t, p: a.p + (r.p - a.p) * t, tilt: r.tilt }, sink);
            }
        }
        self.sm = Some(r);
    }
    pub fn finish(&mut self, sink: &mut dyn Sink) {
        if self.pts.len() > 1 {
            if let Some(m) = self.mid {
                let l = *self.pts.last().unwrap();
                self.walk_line(m, l, sink);
                self.mid = Some(l);
            }
        }
    }
    pub fn end(&mut self, sink: &mut dyn Sink) {
        self.catch_up(sink);
        self.finish(sink);
    }
    pub fn rerender(&mut self, sink: &mut dyn Sink) {
        self.total = Some(self.measure());
        self.reset();
        for i in 0..self.pts.len() {
            self.render_at(i, sink);
        }
        self.finish(sink);
    }
    pub fn replace(&mut self, pts: Vec<Pt>) {
        self.sm = pts.last().copied();
        self.last_raw = self.sm;
        if self.lazy.is_some() { self.lazy = self.sm; }
        self.pts = pts;
    }
    fn stamp_at(&mut self, pt: Pt, d: f32, sink: &mut dyn Sink) {
        let pr = pt.p.clamp(0.0, 1.0).powf(self.gamma);
        let b = &self.b;
        let mut size = self.base * b.min_size.max(1.0 - b.p_size + b.p_size * pr);
        if b.tilt_size > 0.0 && pt.tilt > 0.0 {
            size *= 1.0 + b.tilt_size * pt.tilt;
        }
        let ts = b.taper_start * self.base;
        let te = b.taper_end * self.base;
        if ts > 0.0 && d < ts {
            size *= util::lerp(b.taper_min, 1.0, smooth(d / ts));
        }
        if let Some(total) = self.total {
            if te > 0.0 && total - d < te {
                size *= util::lerp(b.taper_min, 1.0, smooth((total - d).max(0.0) / te));
            }
        }
        let alpha = b.flow * (1.0 - b.p_flow + b.p_flow * pr);
        self.last_size = size;
        let n = (b.count.round() as usize).max(1);
        let (angle0, follow, j_angle, scatter, j_size, j_op) = (b.angle.to_radians(), b.follow, b.j_angle, b.scatter, b.j_size, b.j_opacity);
        let (jh, js, jb) = (b.j_hue, b.j_sat, b.j_bri);
        for _ in 0..n {
            let s = size * (1.0 - j_size * self.rng.next());
            let a = alpha * (1.0 - j_op * self.rng.next());
            let mut ang = angle0;
            if follow {
                ang += self.dir;
            }
            if j_angle > 0.0 {
                ang += (self.rng.next() - 0.5) * TAU * j_angle;
            }
            let (mut x, mut y) = (pt.x, pt.y);
            if scatter > 0.0 {
                let rr = self.rng.next().sqrt() * scatter * size;
                let th = self.rng.next() * TAU;
                x += th.cos() * rr;
                y += th.sin() * rr;
            }
            let mut col = self.color;
            if self.jitter_col {
                let (h, sat, v) = self.hsv;
                let c = util::hsv2rgb(h + (self.rng.next() - 0.5) * 360.0 * jh, clamp01(sat + (self.rng.next() - 0.5) * 2.0 * js), clamp01(v + (self.rng.next() - 0.5) * 2.0 * jb));
                col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
            }
            self.emit(x, y, s, a, ang, col, sink);
        }
    }
    fn emit(&mut self, x: f32, y: f32, mut s: f32, mut a: f32, ang: f32, col: [f32; 3], sink: &mut dyn Sink) {
        if !self.b.pixel && s < 1.0 {
            a *= s.max(0.15);
            s = 1.0;
        }
        let rad = s * 0.75 + 2.0;
        if self.mirrors.is_empty() {
            self.dirty = util::union_opt(self.dirty, Rect::around(x, y, rad));
            sink.stamp(&Stamp { x, y, size: s, alpha: a, angle: ang, color: col, mirror: 0 });
            return;
        }
        for (i, m) in self.mirrors.clone().iter().enumerate() {
            let dx = x - m.cx;
            let dy = y - m.cy;
            let xx = m.cx + m.a * dx + m.b * dy;
            let yy = m.cy + m.c * dx + m.d * dy;
            let (ca, sa) = (ang.cos(), ang.sin());
            let a2 = (m.c * ca + m.d * sa).atan2(m.a * ca + m.b * sa);
            self.dirty = util::union_opt(self.dirty, Rect::around(xx, yy, rad));
            sink.stamp(&Stamp { x: xx, y: yy, size: s, alpha: a, angle: a2, color: col, mirror: i });
        }
    }
}

/* ---------- быстрые фигуры ---------- */

pub struct ShapeHit {
    pub kind: &'static str,
    pub label: &'static str,
    pub pts: Vec<Pt>,
}

pub fn detect_shape(p: &[Pt]) -> Option<ShapeHit> {
    if p.len() < 5 {
        return None;
    }
    let pts: Vec<(f32, f32)> = p.iter().map(|q| (q.x, q.y)).collect();
    let mut len = 0.0;
    for i in 1..pts.len() {
        len += (pts[i].0 - pts[i - 1].0).hypot(pts[i].1 - pts[i - 1].1);
    }
    if len < 20.0 {
        return None;
    }
    let mut pr: Vec<f32> = p.iter().map(|q| q.p).collect();
    pr.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pm = pr[pr.len() / 2];
    let a = pts[0];
    let z = *pts.last().unwrap();
    let dz = (z.0 - a.0).hypot(z.1 - a.1);
    let (minx, maxx) = pts.iter().fold((f32::MAX, f32::MIN), |(m, x), q| (m.min(q.0), x.max(q.0)));
    let (miny, maxy) = pts.iter().fold((f32::MAX, f32::MIN), |(m, x), q| (m.min(q.1), x.max(q.1)));
    let diag = (maxx - minx).hypot(maxy - miny);
    if dz / len > 0.9 {
        return Some(ShapeHit { kind: "line", label: "Линия", pts: line_pts(a, z, pm) });
    }
    let closed = dz < (diag * 0.22).max(18.0);
    if closed {
        let e = fit_ellipse(&pts);
        let mut poly = rdp(&pts, diag * 0.07);
        if poly.len() > 3 {
            let f = poly[0];
            let l = *poly.last().unwrap();
            if (f.0 - l.0).hypot(f.1 - l.1) < diag * 0.25 {
                poly.pop();
            }
        }
        let poly = merge_corners(poly, diag);
        if let Some(e) = &e {
            if e.err < 0.07 || poly.len() > 6 || poly.len() < 3 {
                if e.err < 0.16 {
                    return Some(ShapeHit { kind: "ellipse", label: if e.a == e.b { "Круг" } else { "Эллипс" }, pts: ellipse_pts(e, pm) });
                }
                return None;
            }
        }
        if poly.len() < 3 || poly.len() > 6 {
            return None;
        }
        let poly = if poly.len() == 4 { square_up(poly) } else { poly };
        let label = ["", "", "", "Треугольник", "Прямоугольник", "Пятиугольник", "Шестиугольник"][poly.len()];
        return Some(ShapeHit { kind: "poly", label, pts: poly_pts(&poly, true, pm) });
    }
    let poly = rdp(&pts, diag * 0.06);
    if poly.len() <= 4 {
        return Some(ShapeHit { kind: "polyline", label: "Ломаная", pts: poly_pts(&poly, false, pm) });
    }
    None
}

pub fn line_pts(a: (f32, f32), z: (f32, f32), p: f32) -> Vec<Pt> {
    let n = (((z.0 - a.0).hypot(z.1 - a.1) / 2.0).ceil() as usize).max(2);
    (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        Pt { x: a.0 + (z.0 - a.0) * t, y: a.1 + (z.1 - a.1) * t, p, tilt: 0.0 }
    }).collect()
}

fn poly_pts(p: &[(f32, f32)], closed: bool, pr: f32) -> Vec<Pt> {
    let mut v: Vec<(f32, f32)> = p.to_vec();
    if closed {
        v.push(p[0]);
    }
    let mut out: Vec<Pt> = vec![];
    for i in 1..v.len() {
        let mut seg = line_pts(v[i - 1], v[i], pr);
        if !out.is_empty() {
            seg.remove(0);
        }
        out.extend(seg);
    }
    if closed && out.len() > 2 {
        out.push(out[1]);
    }
    out
}

struct Ell {
    cx: f32,
    cy: f32,
    a: f32,
    b: f32,
    th: f32,
    err: f32,
}

fn fit_ellipse(pts: &[(f32, f32)]) -> Option<Ell> {
    let n = pts.len() as f32;
    let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in pts {
        let (dx, dy) = (p.0 - cx, p.1 - cy);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let th = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (c, s) = (th.cos(), th.sin());
    let uv: Vec<(f32, f32)> = pts.iter().map(|p| {
        let (dx, dy) = (p.0 - cx, p.1 - cy);
        (dx * c + dy * s, -dx * s + dy * c)
    }).collect();
    let (u0, u1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
    let (v0, v1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
    let (cu, cv) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    let (mut a, mut b) = ((u1 - u0) / 2.0, (v1 - v0) / 2.0);
    if a < 2.0 || b < 2.0 {
        return None;
    }
    let err = uv.iter().map(|q| (((q.0 - cu) / a).hypot((q.1 - cv) / b) - 1.0).abs()).sum::<f32>() / uv.len() as f32;
    if a.min(b) / a.max(b) > 0.88 {
        let m = (a + b) / 2.0;
        a = m;
        b = m;
    }
    Some(Ell { cx: cx + cu * c - cv * s, cy: cy + cu * s + cv * c, a, b, th, err })
}

fn ellipse_pts(e: &Ell, p: f32) -> Vec<Pt> {
    let n = ((PI * (e.a + e.b) / 2.0).ceil() as usize).max(24);
    let (c, s) = (e.th.cos(), e.th.sin());
    (0..=n + 1).map(|i| {
        let t = i as f32 / n as f32 * TAU;
        let (u, v) = (t.cos() * e.a, t.sin() * e.b);
        Pt { x: e.cx + u * c - v * s, y: e.cy + u * s + v * c, p, tilt: 0.0 }
    }).collect()
}

fn rdp(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut st = vec![(0usize, pts.len() - 1)];
    while let Some((i0, i1)) = st.pop() {
        let (ax, ay) = pts[i0];
        let (bx, by) = pts[i1];
        let (dx, dy) = (bx - ax, by - ay);
        let l = dx.hypot(dy).max(1e-6);
        let mut md = 0.0;
        let mut mi = 0;
        for i in i0 + 1..i1 {
            let d = ((pts[i].0 - ax) * dy - (pts[i].1 - ay) * dx).abs() / l;
            if d > md {
                md = d;
                mi = i;
            }
        }
        if md > eps && mi > 0 {
            keep[mi] = true;
            st.push((i0, mi));
            st.push((mi, i1));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

fn merge_corners(poly: Vec<(f32, f32)>, diag: f32) -> Vec<(f32, f32)> {
    if poly.len() < 4 {
        return poly;
    }
    let mut out: Vec<(f32, f32)> = vec![];
    for p in poly {
        if out.last().map_or(true, |l: &(f32, f32)| (p.0 - l.0).hypot(p.1 - l.1) > diag * 0.12) {
            out.push(p);
        }
    }
    if out.len() > 3 {
        let f = out[0];
        let l = *out.last().unwrap();
        if (f.0 - l.0).hypot(f.1 - l.1) < diag * 0.12 {
            out.pop();
        }
    }
    out
}

fn square_up(q: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let axis = (0..4).all(|i| {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let ang = (b.1 - a.1).atan2(b.0 - a.0);
        let t = ((ang % (PI / 2.0)) + PI / 2.0) % (PI / 2.0);
        t < 0.2 || t > PI / 2.0 - 0.2
    });
    if !axis {
        return q;
    }
    let (x0, x1) = q.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = q.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// Зеркала для симметрии.
pub fn mirrors(kind: &str, segments: usize, rotational: bool, cx: f32, cy: f32) -> Vec<Mirror> {
    let m = |a, b, c, d| Mirror { a, b, c, d, cx, cy };
    match kind {
        "vertical" => vec![m(1.0, 0.0, 0.0, 1.0), m(-1.0, 0.0, 0.0, 1.0)],
        "horizontal" => vec![m(1.0, 0.0, 0.0, 1.0), m(1.0, 0.0, 0.0, -1.0)],
        "quadrant" => vec![m(1.0, 0.0, 0.0, 1.0), m(-1.0, 0.0, 0.0, 1.0), m(1.0, 0.0, 0.0, -1.0), m(-1.0, 0.0, 0.0, -1.0)],
        "radial" => {
            let n = segments.clamp(2, 32);
            let mut out = vec![];
            for i in 0..n {
                let t = i as f32 * TAU / n as f32;
                let (c, s) = (t.cos(), t.sin());
                out.push(m(c, -s, s, c));
                if !rotational {
                    out.push(m(c, s, s, -c));
                }
            }
            out
        }
        _ => vec![],
    }
}
