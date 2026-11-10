//! Общие утилиты: прямоугольники, генератор случайных чисел, цвет, шум.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn from_f(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        let (ax, bx) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        let (ay, by) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        let x = ax.floor() as i32;
        let y = ay.floor() as i32;
        Rect { x, y, w: bx.ceil() as i32 - x, h: by.ceil() as i32 - y }
    }
    pub fn around(cx: f32, cy: f32, r: f32) -> Rect {
        Rect::from_f(cx - r, cy - r, cx + r, cy + r)
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }
    pub fn clip(&self, w: usize, h: usize) -> Option<Rect> {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = self.right().min(w as i32);
        let y1 = self.bottom().min(h as i32);
        if x1 > x0 && y1 > y0 {
            Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
        } else {
            None
        }
    }
    pub fn inflate(&self, n: i32) -> Rect {
        Rect { x: self.x - n, y: self.y - n, w: self.w + 2 * n, h: self.h + 2 * n }
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32 && y >= self.y as f32 && x < self.right() as f32 && y < self.bottom() as f32
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        if x1 > x0 && y1 > y0 { Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }) } else { None }
    }
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.intersect(o).is_some()
    }
}

pub fn union_opt(a: Option<Rect>, b: Rect) -> Option<Rect> {
    Some(match a {
        Some(a) => a.union(&b),
        None => b,
    })
}

/// Генератор случайных чисел mulberry32 (как в веб-версии).
#[derive(Clone)]
pub struct Rng(pub u32);
impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed)
    }
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(1 | t);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }
}

pub fn random_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    (n as u32) ^ ((n >> 32) as u32).wrapping_mul(2654435761)
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn uid() -> String {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let t = now_ms() as u64;
    let r = random_seed() as u64;
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{:x}{:06x}{:x}", t, r & 0xffffff, n & 0xfff)
}

pub fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

pub fn smooth(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/* ---------- цвет ---------- */
pub fn hsv2rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = ((h % 360.0) + 360.0) % 360.0 / 60.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 1.0 {
        (c, x, 0.0)
    } else if h < 2.0 {
        (x, c, 0.0)
    } else if h < 3.0 {
        (0.0, c, x)
    } else if h < 4.0 {
        (0.0, x, c)
    } else if h < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [((r + m) * 255.0).round() as u8, ((g + m) * 255.0).round() as u8, ((b + m) * 255.0).round() as u8]
}

pub fn rgb2hsv(c: [u8; 3]) -> (f32, f32, f32) {
    let r = c[0] as f32 / 255.0;
    let g = c[1] as f32 / 255.0;
    let b = c[2] as f32 / 255.0;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let mut h = 0.0;
    if d > 0.0 {
        h = if mx == r {
            ((g - b) / d) % 6.0
        } else if mx == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        if h < 0.0 {
            h += 360.0;
        }
    }
    (h, if mx > 0.0 { d / mx } else { 0.0 }, mx)
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    let s: String = if s.len() == 3 { s.chars().flat_map(|c| [c, c]).collect() } else { s.to_string() };
    if s.len() < 6 {
        return None;
    }
    let n = u32::from_str_radix(&s[..6], 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

pub fn luma(c: [u8; 3]) -> f32 {
    (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0
}

/* ---------- шум ---------- */
pub fn hash2(ix: i32, iy: i32, seed: i32) -> f32 {
    let mut h = ix.wrapping_mul(374761393).wrapping_add(iy.wrapping_mul(668265263)).wrapping_add(seed.wrapping_mul(2147483647)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / 4294967296.0
}

pub fn value_noise(u: f32, v: f32, seed: i32) -> f32 {
    let ix = u.floor() as i32;
    let iy = v.floor() as i32;
    let fx = smooth(u - ix as f32);
    let fy = smooth(v - iy as f32);
    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1, iy, seed);
    let c = hash2(ix, iy + 1, seed);
    let d = hash2(ix + 1, iy + 1, seed);
    a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy
}

/// Бесшовный шум size×size.
pub fn tile_noise(size: usize, cx: usize, cy: usize, rng: &mut Rng) -> Vec<f32> {
    let g: Vec<f32> = (0..cx * cy).map(|_| rng.next()).collect();
    let mut out = vec![0.0; size * size];
    for y in 0..size {
        let fy = y as f32 / size as f32 * cy as f32;
        let iy = fy.floor() as usize;
        let ty = smooth(fy - iy as f32);
        let y0 = iy % cy;
        let y1 = (iy + 1) % cy;
        for x in 0..size {
            let fx = x as f32 / size as f32 * cx as f32;
            let ix = fx.floor() as usize;
            let tx = smooth(fx - ix as f32);
            let x0 = ix % cx;
            let x1 = (ix + 1) % cx;
            let a = g[y0 * cx + x0];
            let b = g[y0 * cx + x1];
            let c = g[y1 * cx + x0];
            let d = g[y1 * cx + x1];
            let top = a + (b - a) * tx;
            let bot = c + (d - c) * tx;
            out[y * size + x] = top + (bot - top) * ty;
        }
    }
    out
}

pub fn fbm(size: usize, octs: &[f32], rng: &mut Rng, sx: f32, sy: f32) -> Vec<f32> {
    let mut out = vec![0.0; size * size];
    let mut amp = 1.0;
    let mut tot = 0.0;
    for &c in octs {
        let n = tile_noise(size, ((c * sx).round() as usize).max(1), ((c * sy).round() as usize).max(1), rng);
        for i in 0..out.len() {
            out[i] += n[i] * amp;
        }
        tot += amp;
        amp *= 0.55;
    }
    for v in out.iter_mut() {
        *v /= tot;
    }
    out
}
