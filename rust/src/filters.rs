//! Коррекция и фильтры. Каждый фильтр строит новое изображение из исходного слоя.

use crate::raster::{blur, Img};
use crate::util::{clamp01, hsv2rgb, rgb2hsv, Rng};
use rayon::prelude::*;

pub struct Param {
    pub key: &'static str,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub def: f32,
    pub choices: &'static [&'static str],
}

const fn p(key: &'static str, name: &'static str, min: f32, max: f32, def: f32) -> Param {
    Param { key, name, min, max, def, choices: &[] }
}
const fn c(key: &'static str, name: &'static str, choices: &'static [&'static str], def: f32) -> Param {
    Param { key, name, min: 0.0, max: (choices.len() - 1) as f32, def, choices }
}

pub struct Filter {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub params: &'static [Param],
}

pub const GRADIENTS: [(&str, &[(f32, [u8; 3])]); 7] = [
    ("Закат", &[(0.0, [0x1b, 0x0b, 0x3a]), (0.35, [0x7a, 0x1f, 0x5c]), (0.65, [0xf0, 0x5d, 0x3b]), (1.0, [0xff, 0xe2, 0x9a])]),
    ("Сепия", &[(0.0, [0x1e, 0x13, 0x0a]), (0.5, [0x8a, 0x64, 0x40]), (1.0, [0xf4, 0xe6, 0xcc])]),
    ("Ночь", &[(0.0, [0x02, 0x03, 0x0f]), (0.5, [0x1f, 0x3a, 0x7a]), (1.0, [0xbf, 0xe8, 0xff])]),
    ("Неон", &[(0.0, [0x0d, 0x02, 0x21]), (0.4, [0xa3, 0x00, 0x8c]), (0.75, [0x00, 0xd9, 0xff]), (1.0, [0xf6, 0xff, 0xad])]),
    ("Ч/Б", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])]),
    ("Тепло–холод", &[(0.0, [0x1c, 0x3c, 0x6e]), (0.5, [0x9c, 0x8f, 0x86]), (1.0, [0xff, 0xb0, 0x66])]),
    ("Лес", &[(0.0, [0x0c, 0x1a, 0x0f]), (0.5, [0x3f, 0x7a, 0x3a]), (1.0, [0xe6, 0xf2, 0xb4])]),
];
const GRAD_NAMES: [&str; 7] = ["Закат", "Сепия", "Ночь", "Неон", "Ч/Б", "Тепло–холод", "Лес"];

pub const FILTERS: &[Filter] = &[
    Filter { id: "hsb", name: "Тон, насыщенность, яркость", group: "Цвет", params: &[p("h", "Тон", -180.0, 180.0, 0.0), p("s", "Насыщенность", -100.0, 100.0, 0.0), p("b", "Яркость", -100.0, 100.0, 0.0)] },
    Filter { id: "bc", name: "Яркость и контраст", group: "Цвет", params: &[p("b", "Яркость", -100.0, 100.0, 0.0), p("c", "Контраст", -100.0, 100.0, 0.0)] },
    Filter { id: "balance", name: "Цветовой баланс", group: "Цвет", params: &[c("tone", "Диапазон", &["Тени", "Средние", "Света"], 1.0), p("cr", "Голубой ↔ Красный", -100.0, 100.0, 0.0), p("mg", "Пурпурный ↔ Зелёный", -100.0, 100.0, 0.0), p("yb", "Жёлтый ↔ Синий", -100.0, 100.0, 0.0)] },
    Filter { id: "gmap", name: "Карта градиента", group: "Цвет", params: &[c("preset", "Градиент", &GRAD_NAMES, 0.0), p("mix", "Сила", 0.0, 100.0, 100.0)] },
    Filter { id: "desat", name: "Обесцветить", group: "Цвет", params: &[p("amt", "Сила", 0.0, 100.0, 100.0)] },
    Filter { id: "invert", name: "Инверсия", group: "Цвет", params: &[] },
    Filter { id: "posterize", name: "Постеризация", group: "Цвет", params: &[p("n", "Уровни", 2.0, 24.0, 5.0)] },
    Filter { id: "threshold", name: "Порог", group: "Цвет", params: &[p("t", "Уровень", 0.0, 255.0, 128.0)] },
    Filter { id: "blur", name: "Размытие по Гауссу", group: "Размытие", params: &[p("r", "Радиус", 0.0, 120.0, 8.0)] },
    Filter { id: "motion", name: "Размытие в движении", group: "Размытие", params: &[p("dist", "Расстояние", 0.0, 300.0, 40.0), p("ang", "Угол", 0.0, 360.0, 0.0)] },
    Filter { id: "radial", name: "Радиальное размытие", group: "Размытие", params: &[p("amt", "Сила", 0.0, 100.0, 20.0), p("cx", "Центр X, %", 0.0, 100.0, 50.0), p("cy", "Центр Y, %", 0.0, 100.0, 50.0)] },
    Filter { id: "sharpen", name: "Резкость", group: "Эффекты", params: &[p("amt", "Сила", 0.0, 300.0, 80.0), p("r", "Радиус", 1.0, 10.0, 2.0)] },
    Filter { id: "noise", name: "Шум", group: "Эффекты", params: &[p("amt", "Количество", 0.0, 100.0, 25.0), c("mono", "Тип", &["Цветной", "Монохромный"], 1.0)] },
    Filter { id: "bloom", name: "Свечение", group: "Эффекты", params: &[p("th", "Порог", 0.0, 100.0, 60.0), p("size", "Размер", 1.0, 150.0, 30.0), p("amt", "Сила", 0.0, 200.0, 100.0)] },
    Filter { id: "chroma", name: "Хроматическая аберрация", group: "Эффекты", params: &[p("amt", "Сдвиг", 0.0, 60.0, 8.0), p("ang", "Угол", 0.0, 360.0, 0.0)] },
    Filter { id: "halftone", name: "Полутон", group: "Эффекты", params: &[p("size", "Размер ячейки", 4.0, 60.0, 12.0), c("mode", "Режим", &["Цветной", "Чёрно-белый"], 0.0)] },
    Filter { id: "pixelate", name: "Пикселизация", group: "Эффекты", params: &[p("size", "Размер пикселя", 2.0, 120.0, 12.0)] },
    Filter { id: "vignette", name: "Виньетка", group: "Эффекты", params: &[p("amt", "Сила", 0.0, 100.0, 50.0), p("size", "Размер", 10.0, 100.0, 60.0)] },
];

pub fn find(id: &str) -> &'static Filter {
    FILTERS.iter().find(|f| f.id == id).unwrap_or(&FILTERS[0])
}

pub fn defaults(f: &Filter) -> Vec<f32> {
    f.params.iter().map(|p| p.def).collect()
}

/// Попиксельная обработка цвета (без предумножения).
fn map_rgb(src: &Img, f: impl Fn([f32; 3], usize, usize) -> [f32; 3] + Sync) -> Img {
    let mut out = src.clone();
    let w = src.w;
    out.px.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let p = &mut row[x * 4..x * 4 + 4];
            let a = p[3] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let c = [p[0] as f32 / 255.0 / a, p[1] as f32 / 255.0 / a, p[2] as f32 / 255.0 / a];
            let r = f(c, x, y);
            for k in 0..3 {
                p[k] = (clamp01(r[k]) * a * 255.0).round() as u8;
            }
        }
    });
    out
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Применить фильтр с параметрами v.
pub fn apply(id: &str, src: &Img, v: &[f32]) -> Img {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    match id {
        "hsb" => {
            let (dh, ds, db) = (g(0), g(1) / 100.0, g(2) / 100.0);
            map_rgb(src, |c, _, _| {
                let rgb = [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8];
                let (h, s, val) = rgb2hsv(rgb);
                let s2 = clamp01(if ds >= 0.0 { s + (1.0 - s) * ds * s.max(0.15) } else { s * (1.0 + ds) });
                let o = hsv2rgb(h + dh, s2, val);
                let mut r = [o[0] as f32 / 255.0, o[1] as f32 / 255.0, o[2] as f32 / 255.0];
                for k in r.iter_mut() {
                    *k = if db >= 0.0 { *k + (1.0 - *k) * db } else { *k * (1.0 + db) };
                }
                r
            })
        }
        "bc" => {
            let (b, ct) = (g(0) / 100.0, g(1) / 100.0);
            let k = if ct >= 0.0 { 1.0 + ct * 3.0 } else { 1.0 + ct };
            map_rgb(src, |c, _, _| c.map(|x| ((x + b * 0.5) - 0.5) * k + 0.5))
        }
        "balance" => {
            let tone = g(0).round() as i32;
            let (cr, mg, yb) = (g(1) / 100.0 * 0.5, g(2) / 100.0 * 0.5, g(3) / 100.0 * 0.5);
            map_rgb(src, |c, _, _| {
                let l = luma(c);
                let w = match tone { 0 => clamp01(1.0 - l * 2.2), 2 => clamp01(l * 2.2 - 1.2), _ => 1.0 - (l - 0.5).abs() * 2.0 };
                [c[0] + cr * w, c[1] + mg * w, c[2] + yb * w]
            })
        }
        "gmap" => {
            let stops = GRADIENTS[(g(0).round() as usize).min(GRADIENTS.len() - 1)].1;
            let mix = g(1) / 100.0;
            let lut: Vec<[f32; 3]> = (0..256).map(|i| {
                let t = i as f32 / 255.0;
                let mut k = 0;
                while k + 2 < stops.len() && t > stops[k + 1].0 {
                    k += 1;
                }
                let (t0, c0) = stops[k];
                let (t1, c1) = stops[(k + 1).min(stops.len() - 1)];
                let u = if t1 > t0 { clamp01((t - t0) / (t1 - t0)) } else { 0.0 };
                [(c0[0] as f32 + (c1[0] as f32 - c0[0] as f32) * u) / 255.0, (c0[1] as f32 + (c1[1] as f32 - c0[1] as f32) * u) / 255.0, (c0[2] as f32 + (c1[2] as f32 - c0[2] as f32) * u) / 255.0]
            }).collect();
            map_rgb(src, |c, _, _| {
                let m = lut[(clamp01(luma(c)) * 255.0) as usize];
                [c[0] + (m[0] - c[0]) * mix, c[1] + (m[1] - c[1]) * mix, c[2] + (m[2] - c[2]) * mix]
            })
        }
        "desat" => {
            let k = g(0) / 100.0;
            map_rgb(src, |c, _, _| { let l = luma(c); c.map(|x| x + (l - x) * k) })
        }
        "invert" => map_rgb(src, |c, _, _| c.map(|x| 1.0 - x)),
        "posterize" => {
            let n = (g(0).round() - 1.0).max(1.0);
            map_rgb(src, |c, _, _| c.map(|x| (x * n).round() / n))
        }
        "threshold" => {
            let t = g(0) / 255.0;
            map_rgb(src, |c, _, _| { let v = if luma(c) >= t { 1.0 } else { 0.0 }; [v, v, v] })
        }
        "blur" => blur(src, g(0)),
        "motion" => {
            let (dist, a) = (g(0), g(1).to_radians());
            let n = ((dist / 2.0).ceil() as usize).clamp(1, 64);
            sample_avg(src, n, |x, y, i| { let t = if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 - 0.5 }; (x - a.cos() * dist * t, y - a.sin() * dist * t) })
        }
        "radial" => {
            let (amt, cx, cy) = (g(0) / 100.0, src.w as f32 * g(1) / 100.0, src.h as f32 * g(2) / 100.0);
            let n = 32;
            sample_avg(src, n, |x, y, i| { let k = 1.0 + amt * 0.25 * i as f32 / (n - 1) as f32; (cx + (x - cx) / k, cy + (y - cy) / k) })
        }
        "sharpen" => {
            let (amt, r) = (g(0) / 100.0, g(1));
            let b = blur(src, r);
            let mut out = src.clone();
            out.px.par_chunks_mut(4).zip(b.px.par_chunks(4)).for_each(|(p, q)| {
                for k in 0..3 {
                    p[k] = (p[k] as f32 + (p[k] as f32 - q[k] as f32) * amt).round().clamp(0.0, p[3] as f32) as u8;
                }
            });
            out
        }
        "noise" => {
            let (amt, mono) = (g(0) / 100.0, g(1) >= 0.5);
            map_rgb(src, |c, x, y| {
                let mut r = Rng::new((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663));
                if mono { let n = (r.next() - 0.5) * amt; c.map(|v| v + n) } else { [c[0] + (r.next() - 0.5) * amt, c[1] + (r.next() - 0.5) * amt, c[2] + (r.next() - 0.5) * amt] }
            })
        }
        "bloom" => {
            let (th, size, amt) = (g(0) / 100.0, g(1), g(2) / 100.0);
            let mut bright = src.clone();
            for p in bright.px.chunks_exact_mut(4) {
                let a = p[3] as f32 / 255.0;
                if a <= 0.0 { continue; }
                let l = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.0 / a;
                let k = if l < th { 0.0 } else { clamp01((l - th) / (1.0 - th).max(0.01) * 2.0) };
                for v in p.iter_mut() { *v = (*v as f32 * k) as u8; }
            }
            let gl = blur(&bright, size);
            let mut out = src.clone();
            out.px.par_chunks_mut(4).zip(gl.px.par_chunks(4)).for_each(|(p, q)| {
                for k in 0..4 {
                    let a = p[k] as f32 / 255.0;
                    let b = (q[k] as f32 / 255.0 * amt).min(1.0);
                    p[k] = ((a + b - a * b) * 255.0).round() as u8;
                }
            });
            out
        }
        "chroma" => {
            let (amt, a) = (g(0), g(1).to_radians());
            let (ox, oy) = ((a.cos() * amt).round() as i32, (a.sin() * amt).round() as i32);
            let (w, h) = (src.w as i32, src.h as i32);
            let mut out = src.clone();
            out.px.par_chunks_mut(src.w * 4).enumerate().for_each(|(y, row)| {
                for x in 0..src.w {
                    let at = |dx: i32, dy: i32| src.idx((x as i32 + dx).clamp(0, w - 1) as usize, (y as i32 + dy).clamp(0, h - 1) as usize);
                    let (ir, ib, i0) = (at(ox, oy), at(-ox, -oy), src.idx(x, y));
                    let a = src.px[i0 + 3].max(src.px[ir + 3]).max(src.px[ib + 3]);
                    row[x * 4] = src.px[ir];
                    row[x * 4 + 2] = src.px[ib + 2];
                    row[x * 4 + 1] = src.px[i0 + 1];
                    row[x * 4 + 3] = a;
                    for k in 0..3 { row[x * 4 + k] = row[x * 4 + k].min(a); }
                }
            });
            out
        }
        "halftone" => {
            let (s, bw) = (g(0).max(2.0), g(1) >= 0.5);
            let cells = src.resize(((src.w as f32 / s).ceil() as usize).max(1), ((src.h as f32 / s).ceil() as usize).max(1));
            let mut out = Img::new(src.w, src.h);
            let w = src.w;
            out.px.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                for x in 0..w {
                    let (cx, cy) = ((x as f32 / s) as usize, (y as f32 / s) as usize);
                    let ci = cells.idx(cx.min(cells.w - 1), cy.min(cells.h - 1));
                    let a = cells.px[ci + 3] as f32 / 255.0;
                    if a < 0.02 { continue; }
                    let col = [cells.px[ci] as f32 / 255.0 / a, cells.px[ci + 1] as f32 / 255.0 / a, cells.px[ci + 2] as f32 / 255.0 / a];
                    let l = luma(col);
                    let r = if bw { s * 0.72 * (1.0 - l).sqrt() } else { s * 0.55 * (1.0 - l * 0.6).max(0.35) };
                    let dx = x as f32 + 0.5 - (cx as f32 + 0.5) * s;
                    let dy = y as f32 + 0.5 - (cy as f32 + 0.5) * s;
                    let cov = clamp01(r - (dx * dx + dy * dy).sqrt() + 0.5) * a;
                    let c = if bw { [0.0; 3] } else { col };
                    for k in 0..3 { row[x * 4 + k] = (c[k] * cov * 255.0) as u8; }
                    row[x * 4 + 3] = (cov * 255.0) as u8;
                }
            });
            out
        }
        "pixelate" => {
            let s = g(0).max(2.0);
            let small = src.resize(((src.w as f32 / s).ceil() as usize).max(1), ((src.h as f32 / s).ceil() as usize).max(1));
            let mut out = Img::new(src.w, src.h);
            for y in 0..src.h {
                for x in 0..src.w {
                    let si = small.idx(((x as f32 / s) as usize).min(small.w - 1), ((y as f32 / s) as usize).min(small.h - 1));
                    let di = out.idx(x, y);
                    out.px[di..di + 4].copy_from_slice(&small.px[si..si + 4]);
                }
            }
            out
        }
        "vignette" => {
            let (amt, size) = (g(0) / 100.0, g(1) / 100.0);
            let (cx, cy) = (src.w as f32 / 2.0, src.h as f32 / 2.0);
            let r0 = src.w.min(src.h) as f32 * size / 2.0;
            let r1 = (cx * cx + cy * cy).sqrt();
            map_rgb(src, |c, x, y| {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                let t = clamp01((d - r0) / (r1 - r0).max(1.0)) * amt;
                c.map(|v| v * (1.0 - t))
            })
        }
        _ => src.clone(),
    }
}

fn sample_avg(src: &Img, n: usize, at: impl Fn(f32, f32, usize) -> (f32, f32) + Sync) -> Img {
    let mut out = Img::new(src.w, src.h);
    let w = src.w;
    out.px.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for i in 0..n {
                let (sx, sy) = at(x as f32 + 0.5, y as f32 + 0.5, i);
                let v = src.sample(sx - 0.5, sy - 0.5);
                for k in 0..4 { acc[k] += v[k]; }
            }
            for k in 0..4 { row[x * 4 + k] = (acc[k] / n as f32).round().clamp(0.0, 255.0) as u8; }
        }
    });
    out
}
