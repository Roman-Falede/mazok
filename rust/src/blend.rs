//! 27 режимов наложения слоёв (формулы W3C Compositing + режимы из графических редакторов).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Blend {
    Normal, Dissolve,
    Darken, Multiply, ColorBurn, LinearBurn, DarkerColor,
    Lighten, Screen, ColorDodge, Add, LighterColor,
    Overlay, SoftLight, HardLight, VividLight, LinearLight, PinLight, HardMix,
    Difference, Exclusion, Subtract, Divide,
    Hue, Saturation, Color, Luminosity,
}

/// (режим, идентификатор как в веб-версии, название, ключ PSD)
pub const BLENDS: [(Blend, &str, &str, &str); 27] = [
    (Blend::Normal, "source-over", "Обычный", "norm"),
    (Blend::Dissolve, "dissolve", "Растворение", "diss"),
    (Blend::Darken, "darken", "Замена тёмным", "dark"),
    (Blend::Multiply, "multiply", "Умножение", "mul "),
    (Blend::ColorBurn, "color-burn", "Затемнение основы", "idiv"),
    (Blend::LinearBurn, "linear-burn", "Линейный затемнитель", "lbrn"),
    (Blend::DarkerColor, "darker-color", "Темнее", "dkCl"),
    (Blend::Lighten, "lighten", "Замена светлым", "lite"),
    (Blend::Screen, "screen", "Экран", "scrn"),
    (Blend::ColorDodge, "color-dodge", "Осветление основы", "div "),
    (Blend::Add, "lighter", "Линейный осветлитель (добавить)", "lddg"),
    (Blend::LighterColor, "lighter-color", "Светлее", "lgCl"),
    (Blend::Overlay, "overlay", "Перекрытие", "over"),
    (Blend::SoftLight, "soft-light", "Мягкий свет", "sLit"),
    (Blend::HardLight, "hard-light", "Жёсткий свет", "hLit"),
    (Blend::VividLight, "vivid-light", "Яркий свет", "vLit"),
    (Blend::LinearLight, "linear-light", "Линейный свет", "lLit"),
    (Blend::PinLight, "pin-light", "Точечный свет", "pLit"),
    (Blend::HardMix, "hard-mix", "Жёсткое смешение", "hMix"),
    (Blend::Difference, "difference", "Разница", "diff"),
    (Blend::Exclusion, "exclusion", "Исключение", "smud"),
    (Blend::Subtract, "subtract", "Вычитание", "fsub"),
    (Blend::Divide, "divide", "Разделить", "fdiv"),
    (Blend::Hue, "hue", "Цветовой тон", "hue "),
    (Blend::Saturation, "saturation", "Насыщенность", "sat "),
    (Blend::Color, "color", "Цветность", "colr"),
    (Blend::Luminosity, "luminosity", "Яркость", "lum "),
];

/// Начала групп в списке (для разделителей в меню).
pub const GROUP_STARTS: [usize; 5] = [2, 7, 12, 19, 23];

impl Blend {
    fn row(self) -> &'static (Blend, &'static str, &'static str, &'static str) {
        BLENDS.iter().find(|b| b.0 == self).unwrap_or(&BLENDS[0])
    }
    pub fn id(self) -> &'static str {
        self.row().1
    }
    pub fn name(self) -> &'static str {
        self.row().2
    }
    pub fn psd_key(self) -> &'static str {
        self.row().3
    }
    pub fn from_id(s: &str) -> Blend {
        BLENDS.iter().find(|b| b.1 == s).map(|b| b.0).unwrap_or(Blend::Normal)
    }
    pub fn from_psd(k: &str) -> Blend {
        BLENDS.iter().find(|b| b.3 == k).map(|b| b.0).unwrap_or(Blend::Normal)
    }
}

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut o = c;
    if n < 0.0 {
        for v in o.iter_mut() {
            *v = l + (*v - l) * l / (l - n).max(1e-6);
        }
    }
    if x > 1.0 {
        for v in o.iter_mut() {
            *v = l + (*v - l) * (1.0 - l) / (x - l).max(1e-6);
        }
    }
    o
}
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap_or(std::cmp::Ordering::Equal));
    let (mn, md, mx) = (idx[0], idx[1], idx[2]);
    let mut o = [0.0; 3];
    if c[mx] > c[mn] {
        o[md] = (c[md] - c[mn]) * s / (c[mx] - c[mn]);
        o[mx] = s;
    }
    o
}

#[inline]
fn burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 { 1.0 } else if s <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / s).min(1.0) }
}
#[inline]
fn dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 { 0.0 } else if s >= 1.0 { 1.0 } else { (b / (1.0 - s)).min(1.0) }
}
#[inline]
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 { b * 2.0 * s } else { let t = 2.0 * s - 1.0; b + t - b * t }
}
#[inline]
fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

#[inline]
fn sep(mode: Blend, b: f32, s: f32) -> f32 {
    match mode {
        Blend::Darken => b.min(s),
        Blend::Multiply => b * s,
        Blend::ColorBurn => burn(b, s),
        Blend::LinearBurn => (b + s - 1.0).max(0.0),
        Blend::Lighten => b.max(s),
        Blend::Screen => b + s - b * s,
        Blend::ColorDodge => dodge(b, s),
        Blend::Overlay => hard_light(s, b),
        Blend::SoftLight => soft_light(b, s),
        Blend::HardLight => hard_light(b, s),
        Blend::VividLight => if s <= 0.5 { burn(b, 2.0 * s) } else { dodge(b, 2.0 * s - 1.0) },
        Blend::LinearLight => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        Blend::PinLight => if s <= 0.5 { b.min(2.0 * s) } else { b.max(2.0 * s - 1.0) },
        Blend::HardMix => if b + s >= 1.0 { 1.0 } else { 0.0 },
        Blend::Difference => (b - s).abs(),
        Blend::Exclusion => b + s - 2.0 * b * s,
        Blend::Subtract => (b - s).max(0.0),
        Blend::Divide => if s <= 0.0 { 1.0 } else { (b / s).min(1.0) },
        _ => s,
    }
}

fn blend_rgb(mode: Blend, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
    match mode {
        Blend::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Blend::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Blend::Color => set_lum(s, lum(b)),
        Blend::Luminosity => set_lum(b, lum(s)),
        Blend::DarkerColor => if lum(s) < lum(b) { s } else { b },
        Blend::LighterColor => if lum(s) > lum(b) { s } else { b },
        _ => [sep(mode, b[0], s[0]), sep(mode, b[1], s[1]), sep(mode, b[2], s[2])],
    }
}

/// Детерминированный шум для «Растворения».
#[inline]
pub fn dissolve_hash(x: i32, y: i32) -> f32 {
    let mut h = (x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263))) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / 4294967296.0
}

/// Наложить пиксель src (RGBA8, предумноженный) на dst (f32 0..1, предумноженный).
#[inline]
pub fn composite(dst: &mut [f32; 4], s: &[u8], mode: Blend, opacity: f32, x: i32, y: i32) {
    let s_a = s[3] as f32 / 255.0;
    let sa = s_a * opacity;
    if sa <= 0.0 {
        return;
    }
    let k = opacity / 255.0;
    match mode {
        Blend::Normal => {
            let inv = 1.0 - sa;
            dst[0] = s[0] as f32 * k + dst[0] * inv;
            dst[1] = s[1] as f32 * k + dst[1] * inv;
            dst[2] = s[2] as f32 * k + dst[2] * inv;
            dst[3] = sa + dst[3] * inv;
        }
        Blend::Add => {
            dst[0] = (dst[0] + s[0] as f32 * k).min(1.0);
            dst[1] = (dst[1] + s[1] as f32 * k).min(1.0);
            dst[2] = (dst[2] + s[2] as f32 * k).min(1.0);
            dst[3] = (dst[3] + sa).min(1.0);
        }
        Blend::Dissolve => {
            if dissolve_hash(x, y) < sa {
                let u = 1.0 / (s_a * 255.0);
                *dst = [s[0] as f32 * u, s[1] as f32 * u, s[2] as f32 * u, 1.0];
            }
        }
        _ => {
            let u = 1.0 / (s_a * 255.0);
            let cs = [(s[0] as f32 * u).min(1.0), (s[1] as f32 * u).min(1.0), (s[2] as f32 * u).min(1.0)];
            let ab = dst[3];
            let cb = if ab > 0.0 { [(dst[0] / ab).min(1.0), (dst[1] / ab).min(1.0), (dst[2] / ab).min(1.0)] } else { [0.0; 3] };
            let bl = blend_rgb(mode, cb, cs);
            let k1 = sa * (1.0 - ab);
            let k2 = sa * ab;
            let k3 = (1.0 - sa) * ab;
            for c in 0..3 {
                dst[c] = k1 * cs[c] + k2 * bl[c] + k3 * cb[c];
            }
            dst[3] = sa + ab * (1.0 - sa);
        }
    }
}

/// Наложение для кисти в рабочий слой: dst и src — RGBA8 предумноженные.
#[inline]
pub fn composite_u8(dst: &mut [u8], s: [f32; 4], mode: Blend) {
    // s — предумноженный цвет 0..1 с альфой s[3]
    if s[3] <= 0.0 {
        return;
    }
    let mut d = [dst[0] as f32 / 255.0, dst[1] as f32 / 255.0, dst[2] as f32 / 255.0, dst[3] as f32 / 255.0];
    let sp = [(s[0] / s[3] * 255.0).clamp(0.0, 255.0) as u8, (s[1] / s[3] * 255.0).clamp(0.0, 255.0) as u8, (s[2] / s[3] * 255.0).clamp(0.0, 255.0) as u8, 255];
    // используем общую формулу: s[3] как непрозрачность «слоя» из непрозрачного пикселя
    composite(&mut d, &sp, mode, s[3], 0, 0);
    for k in 0..4 {
        dst[k] = (d[k] * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}
