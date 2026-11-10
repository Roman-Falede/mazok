//! Форматы файлов: ZIP, проект .mazok (совместим с веб-версией), PSD, PNG/JPEG, GIF.

use crate::blend::Blend;
use crate::doc::{self, Document, Guide, Layer, TextSpec};
use crate::raster::Img;
use crate::util::{self, hex, parse_hex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

/* ---------- ZIP ---------- */

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        t[n as usize] = c;
    }
    t
}

pub fn crc32(d: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xFFFFFFFFu32;
    for &b in d {
        c = t[((c ^ b as u32) & 255) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFFFFFF
}

pub fn zip_write(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let off = out.len() as u32;
        let crc = crc32(data);
        let nb = name.as_bytes();
        let hdr = |v: &mut Vec<u8>, central: bool| {
            v.extend_from_slice(&(if central { 0x02014b50u32 } else { 0x04034b50 }).to_le_bytes());
            if central {
                v.extend_from_slice(&20u16.to_le_bytes());
            }
            v.extend_from_slice(&20u16.to_le_bytes());
            v.extend_from_slice(&0x0800u16.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&0x5821u16.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&(data.len() as u32).to_le_bytes());
            v.extend_from_slice(&(data.len() as u32).to_le_bytes());
            v.extend_from_slice(&(nb.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            if central {
                v.extend_from_slice(&[0u8; 8]);
                v.extend_from_slice(&0u32.to_le_bytes()[..2]);
                v.extend_from_slice(&off.to_le_bytes());
            }
            v.extend_from_slice(nb);
        };
        hdr(&mut out, false);
        out.extend_from_slice(data);
        hdr(&mut central, true);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

pub fn zip_read(d: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let u16_at = |i: usize| -> usize { u16::from_le_bytes([d[i], d[i + 1]]) as usize };
    let u32_at = |i: usize| -> usize { u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize };
    if d.len() < 22 {
        return Err("Файл повреждён".into());
    }
    let mut e = None;
    let lo = d.len().saturating_sub(70000);
    let mut i = d.len() - 22;
    loop {
        if u32_at(i) == 0x06054b50 {
            e = Some(i);
            break;
        }
        if i == lo || i == 0 {
            break;
        }
        i -= 1;
    }
    let e = e.ok_or("Это не ZIP-архив")?;
    let n = u16_at(e + 10);
    let mut p = u32_at(e + 16);
    let mut out = HashMap::new();
    for _ in 0..n {
        if p + 46 > d.len() || u32_at(p) != 0x02014b50 {
            break;
        }
        let method = u16_at(p + 10);
        let csize = u32_at(p + 20);
        let nl = u16_at(p + 28);
        let el = u16_at(p + 30);
        let cl = u16_at(p + 32);
        let lo = u32_at(p + 42);
        let name = String::from_utf8_lossy(&d[p + 46..p + 46 + nl]).replace('\\', "/");
        let ds = lo + 30 + u16_at(lo + 26) + u16_at(lo + 28);
        let raw = &d[ds..(ds + csize).min(d.len())];
        let data = if method == 8 { miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| "Ошибка распаковки")? } else { raw.to_vec() };
        out.insert(name, data);
        p += 46 + nl + el + cl;
    }
    Ok(out)
}

/* ---------- PNG / JPEG ---------- */

pub fn png(img: &Img) -> Vec<u8> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut out = Vec::new();
    let enc = PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive);
    let _ = enc.write_image(&img.to_straight(), img.w as u32, img.h as u32, image::ExtendedColorType::Rgba8);
    out
}

pub fn jpeg(img: &Img, bg: [u8; 3]) -> Vec<u8> {
    use image::codecs::jpeg::JpegEncoder;
    let mut rgb = Vec::with_capacity(img.w * img.h * 3);
    for p in img.px.chunks_exact(4) {
        let a = p[3] as f32 / 255.0;
        for k in 0..3 {
            rgb.push((p[k] as f32 + bg[k] as f32 * (1.0 - a)).round().min(255.0) as u8);
        }
    }
    let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, 92);
    let _ = enc.encode(&rgb, img.w as u32, img.h as u32, image::ExtendedColorType::Rgb8);
    out
}

pub fn decode_image(bytes: &[u8]) -> Result<Img, String> {
    let im = image::load_from_memory(bytes).map_err(|e| format!("Не удалось открыть изображение: {e}"))?.to_rgba8();
    let (w, h) = im.dimensions();
    Ok(Img::from_straight(w as usize, h as usize, im.as_raw()))
}

/* ---------- проект .mazok ---------- */

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LayerMeta {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: String,
    pub alpha_lock: bool,
    pub clip: bool,
    pub locked: bool,
    pub reference: bool,
    pub text: Option<TextSpec>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Stats {
    pub strokes: u64,
    pub time: f64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DocMeta {
    pub id: String,
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub bg: String,
    pub bg_visible: bool,
    pub active: usize,
    pub layer_seq: u32,
    pub created: i64,
    pub modified: i64,
    pub stats: Stats,
    pub layers: Vec<LayerMeta>,
    pub format: String,
    pub version: u32,
    pub guide_rs: Option<Guide>,
}

pub fn meta(doc: &Document) -> DocMeta {
    DocMeta {
        id: doc.id.clone(),
        name: doc.name.clone(),
        w: doc.w,
        h: doc.h,
        bg: hex(doc.bg),
        bg_visible: doc.bg_visible,
        active: doc.active,
        layer_seq: doc.layer_seq,
        created: doc.created,
        modified: doc.modified,
        stats: Stats { strokes: doc.strokes, time: doc.time },
        layers: doc.layers.iter().map(|l| LayerMeta {
            id: l.id.clone(),
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.opacity,
            blend: l.blend.id().into(),
            alpha_lock: l.alpha_lock,
            clip: l.clip,
            locked: l.locked,
            reference: l.reference,
            text: l.text.clone(),
        }).collect(),
        format: "mazok".into(),
        version: 1,
        guide_rs: Some(doc.guide.clone()),
    }
}

/// Сохранить документ в формат .mazok (ZIP: doc.json + layers/*.png + preview.png).
pub fn save_mazok(doc: &Document) -> Vec<u8> {
    let mut files = vec![("doc.json".to_string(), serde_json::to_vec(&meta(doc)).unwrap_or_default())];
    let pngs: Vec<(String, Vec<u8>)> = {
        use rayon::prelude::*;
        doc.layers.par_iter().map(|l| (format!("layers/{}.png", l.id), png(&l.img))).collect()
    };
    files.extend(pngs);
    files.push(("preview.png".into(), png(&doc::flatten(doc, true).thumbnail(512))));
    zip_write(&files)
}

pub fn load_mazok(bytes: &[u8]) -> Result<Document, String> {
    let z = zip_read(bytes)?;
    let j = z.get("doc.json").ok_or("В архиве нет doc.json")?;
    let m: DocMeta = serde_json::from_slice(j).map_err(|e| format!("Неверный doc.json: {e}"))?;
    if m.w == 0 || m.h == 0 || m.w > 16384 || m.h > 16384 {
        return Err("Неверный размер холста".into());
    }
    let mut d = Document::new(&m.name, m.w, m.h, parse_hex(&m.bg).unwrap_or([255, 255, 255]), m.bg_visible);
    d.layers.clear();
    if !m.id.is_empty() {
        d.id = m.id.clone();
    }
    d.created = m.created;
    d.modified = m.modified;
    d.strokes = m.stats.strokes;
    d.time = m.stats.time;
    if let Some(g) = m.guide_rs {
        d.guide = g;
    }
    for lm in &m.layers {
        let mut l = Layer::new(m.w, m.h, &lm.name);
        if !lm.id.is_empty() {
            l.id = lm.id.clone();
        }
        l.visible = lm.visible;
        l.opacity = lm.opacity;
        l.blend = Blend::from_id(&lm.blend);
        l.alpha_lock = lm.alpha_lock;
        l.clip = lm.clip;
        l.locked = lm.locked;
        l.reference = lm.reference;
        l.text = lm.text.clone();
        if let Some(p) = z.get(&format!("layers/{}.png", lm.id)) {
            if let Ok(img) = decode_image(p) {
                let mut full = Img::new(m.w, m.h);
                full.put(0, 0, &img);
                l.img = Arc::new(full);
            }
        }
        d.layers.push(l);
    }
    if d.layers.is_empty() {
        let n = d.next_name();
        d.layers.push(Layer::new(m.w, m.h, &n));
    }
    d.layer_seq = m.layer_seq.max(d.layers.len() as u32 + 1);
    d.active = m.active.min(d.layers.len() - 1);
    Ok(d)
}

/* ---------- PSD ---------- */

struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) { self.0.push(v); }
    fn u16(&mut self, v: u16) { self.0.extend_from_slice(&v.to_be_bytes()); }
    fn i16(&mut self, v: i16) { self.0.extend_from_slice(&v.to_be_bytes()); }
    fn u32(&mut self, v: u32) { self.0.extend_from_slice(&v.to_be_bytes()); }
    fn i32(&mut self, v: i32) { self.0.extend_from_slice(&v.to_be_bytes()); }
    fn s(&mut self, v: &str) { self.0.extend_from_slice(v.as_bytes()); }
    fn b(&mut self, v: &[u8]) { self.0.extend_from_slice(v); }
    fn z(&mut self, n: usize) { self.0.extend(std::iter::repeat(0).take(n)); }
}

pub fn psd_write(doc: &Document) -> Vec<u8> {
    let (w, h) = (doc.w, doc.h);
    let mut o = W(Vec::new());
    o.s("8BPS"); o.u16(1); o.z(6); o.u16(3); o.u32(h as u32); o.u32(w as u32); o.u16(8); o.u16(3);
    o.u32(0); o.u32(0);
    let mut li = W(Vec::new());
    let mut layers: Vec<Layer> = doc.layers.clone();
    if doc.bg_visible {
        let mut bg = Layer::new(w, h, "Фон");
        bg.img = Arc::new(Img::filled(w, h, [doc.bg[0], doc.bg[1], doc.bg[2], 255]));
        layers.insert(0, bg);
    }
    li.i16(layers.len() as i16);
    let mut planes_all = vec![];
    for l in &layers {
        let b = l.img.content_bounds().unwrap_or(crate::util::Rect::new(0, 0, 0, 0));
        li.i32(b.y); li.i32(b.x); li.i32(b.bottom()); li.i32(b.right()); li.u16(4);
        let mut planes: Option<[Vec<u8>; 4]> = None;
        if b.w > 0 && b.h > 0 {
            let c = l.img.crop(b).to_straight();
            let n = (b.w * b.h) as usize;
            let mut p = [vec![0u8; n], vec![0u8; n], vec![0u8; n], vec![0u8; n]];
            for i in 0..n {
                p[0][i] = c[i * 4 + 3];
                p[1][i] = c[i * 4];
                p[2][i] = c[i * 4 + 1];
                p[3][i] = c[i * 4 + 2];
            }
            let bw = b.w as usize;
            planes = Some(p.map(|ch| rle_channel(&ch, bw)));
        }
        for (k, id) in [-1i16, 0, 1, 2].iter().enumerate() {
            li.i16(*id);
            li.u32(planes.as_ref().map_or(2, |p| p[k].len()) as u32);
        }
        li.s("8BIM"); li.s(l.blend.psd_key()); li.u8((l.opacity * 255.0).round() as u8); li.u8(l.clip as u8);
        li.u8((l.alpha_lock as u8) | if l.visible { 0 } else { 2 }); li.u8(0);
        let mut ex = W(Vec::new());
        ex.u32(0); ex.u32(0);
        let nm: String = l.name.chars().map(|c| if c.is_ascii() { c } else { '_' }).take(255).collect();
        ex.u8(nm.len() as u8); ex.s(&nm); ex.z((4 - ((1 + nm.len()) % 4)) % 4);
        let u: Vec<u16> = l.name.encode_utf16().collect();
        let dl = 4 + u.len() * 2;
        let pl = dl + if dl % 4 != 0 { 4 - dl % 4 } else { 0 };
        ex.s("8BIM"); ex.s("luni"); ex.u32(pl as u32); ex.u32(u.len() as u32);
        for c in &u { ex.u16(*c); }
        ex.z(pl - dl);
        li.u32(ex.0.len() as u32); li.b(&ex.0);
        planes_all.push(planes);
    }
    for p in &planes_all {
        for k in 0..4 {
            match p {
                Some(p) => li.b(&p[k]),
                None => li.u16(0),
            }
        }
    }
    if li.0.len() % 2 == 1 { li.u8(0); }
    o.u32((4 + li.0.len() + 4) as u32); o.u32(li.0.len() as u32); o.b(&li.0); o.u32(0);
    let flat = doc::flatten(doc, true).to_straight();
    o.u16(1);
    let mut rows: Vec<Vec<u8>> = Vec::with_capacity(h * 3);
    for c in 0..3 {
        let p: Vec<u8> = flat.chunks_exact(4).map(|px| px[c]).collect();
        rows.extend(p.chunks(w.max(1)).map(packbits));
    }
    for r in &rows { o.u16(r.len() as u16); }
    for r in &rows { o.b(r); }
    o.0
}

/// Канал слоя PSD со сжатием RLE: заголовок, длины строк, строки.
fn rle_channel(p: &[u8], w: usize) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = p.chunks(w.max(1)).map(packbits).collect();
    let mut out = Vec::with_capacity(2 + rows.len() * 2 + rows.iter().map(|r| r.len()).sum::<usize>());
    out.extend_from_slice(&1u16.to_be_bytes());
    for r in &rows { out.extend_from_slice(&(r.len() as u16).to_be_bytes()); }
    for r in rows { out.extend(r); }
    out
}

/// Сжатие PackBits (как в PSD/TIFF).
fn packbits(src: &[u8]) -> Vec<u8> {
    let n = src.len();
    let mut out = Vec::with_capacity(n / 4 + 2);
    let mut i = 0;
    while i < n {
        let mut run = 1;
        while i + run < n && run < 128 && src[i + run] == src[i] { run += 1; }
        if run >= 2 {
            out.push((1 - run as i32) as i8 as u8);
            out.push(src[i]);
            i += run;
        } else {
            let start = i;
            let mut len = 0;
            while i < n && len < 128 {
                if i + 2 < n && src[i] == src[i + 1] && src[i] == src[i + 2] { break; }
                i += 1;
                len += 1;
            }
            out.push((len - 1) as u8);
            out.extend_from_slice(&src[start..start + len]);
        }
    }
    out
}

pub fn psd_read(d: &[u8], name: &str) -> Result<Document, String> {
    let mut o = 0usize;
    let err = || "Повреждённый PSD".to_string();
    macro_rules! need { ($n:expr) => { if o + $n > d.len() { return Err(err()); } } }
    let rd_u8 = |o: &mut usize| { let v = d[*o]; *o += 1; v };
    let rd_u16 = |o: &mut usize| { let v = u16::from_be_bytes([d[*o], d[*o + 1]]); *o += 2; v };
    let rd_i16 = |o: &mut usize| { let v = i16::from_be_bytes([d[*o], d[*o + 1]]); *o += 2; v };
    let rd_u32 = |o: &mut usize| { let v = u32::from_be_bytes([d[*o], d[*o + 1], d[*o + 2], d[*o + 3]]); *o += 4; v as usize };
    let rd_i32 = |o: &mut usize| { let v = i32::from_be_bytes([d[*o], d[*o + 1], d[*o + 2], d[*o + 3]]); *o += 4; v };
    need!(26);
    if &d[0..4] != b"8BPS" { return Err("Это не PSD-файл".into()); }
    o = 4;
    if rd_u16(&mut o) != 1 { return Err("Формат PSB не поддерживается".into()); }
    o += 6;
    let nch = rd_u16(&mut o) as usize;
    let h = rd_u32(&mut o);
    let w = rd_u32(&mut o);
    let depth = rd_u16(&mut o);
    let mode = rd_u16(&mut o);
    if depth != 8 || mode != 3 { return Err("Поддерживаются только 8-битные RGB-файлы PSD".into()); }
    need!(4); let n = rd_u32(&mut o); o += n;
    need!(4); let n = rd_u32(&mut o); o += n;
    let unpack = |start: usize, end: usize, len: usize| -> Vec<u8> {
        let mut out = vec![0u8; len];
        let (mut p, mut q) = (start, 0);
        while q < len && p < end.min(d.len()) {
            let n = d[p] as i8; p += 1;
            if n >= 0 {
                for _ in 0..=n as usize { if q < len && p < d.len() { out[q] = d[p]; q += 1; p += 1; } }
            } else if n != -128 {
                let b = if p < d.len() { d[p] } else { 0 }; p += 1;
                for _ in 0..(1 - n as i32) as usize { if q < len { out[q] = b; q += 1; } }
            }
        }
        out
    };
    let chan = |start: usize, end: usize, cw: usize, ch: usize| -> Option<Vec<u8>> {
        if start + 2 > d.len() { return None; }
        let comp = u16::from_be_bytes([d[start], d[start + 1]]);
        let n = cw * ch;
        if comp == 0 { return d.get(start + 2..start + 2 + n).map(|s| s.to_vec()); }
        if comp == 1 {
            let mut out = vec![0u8; n];
            let mut p = start + 2 + ch * 2;
            for y in 0..ch {
                let bc = u16::from_be_bytes([d[start + 2 + y * 2], d[start + 3 + y * 2]]) as usize;
                out[y * cw..(y + 1) * cw].copy_from_slice(&unpack(p, p + bc, cw));
                p += bc;
            }
            let _ = end;
            return Some(out);
        }
        None
    };
    let mut doc = Document::new(name, w, h, [255, 255, 255], false);
    doc.layers.clear();
    need!(4);
    let lm_len = rd_u32(&mut o);
    let lm_end = o + lm_len;
    if lm_len > 0 {
        need!(4);
        let li_len = rd_u32(&mut o);
        if li_len > 0 {
            need!(2);
            let count = rd_i16(&mut o).unsigned_abs() as usize;
            struct Rec { top: i32, left: i32, bottom: i32, right: i32, chans: Vec<(i16, usize)>, key: String, opacity: u8, clip: u8, flags: u8, name: String, divider: bool }
            let mut recs = vec![];
            for _ in 0..count {
                need!(18);
                let (top, left, bottom, right) = (rd_i32(&mut o), rd_i32(&mut o), rd_i32(&mut o), rd_i32(&mut o));
                let nc = rd_u16(&mut o) as usize;
                let mut chans = vec![];
                for _ in 0..nc { need!(6); chans.push((rd_i16(&mut o), rd_u32(&mut o))); }
                need!(16);
                o += 4;
                let key = String::from_utf8_lossy(&d[o..o + 4]).to_string(); o += 4;
                let opacity = rd_u8(&mut o); let clip = rd_u8(&mut o); let flags = rd_u8(&mut o); o += 1;
                let ex_len = rd_u32(&mut o);
                let ex_end = o + ex_len;
                need!(8);
                let n = rd_u32(&mut o); o += n;
                let n = rd_u32(&mut o); o += n;
                need!(1);
                let nl = rd_u8(&mut o) as usize;
                need!(nl);
                let mut nm = String::from_utf8_lossy(&d[o..o + nl]).to_string();
                o += nl + (4 - ((1 + nl) % 4)) % 4;
                let mut divider = false;
                while o + 12 <= ex_end {
                    let sig = &d[o..o + 4];
                    if sig != b"8BIM" && sig != b"8B64" { break; }
                    let k = String::from_utf8_lossy(&d[o + 4..o + 8]).to_string();
                    o += 8;
                    let l = rd_u32(&mut o);
                    let ds = o;
                    if k == "luni" && o + 4 <= d.len() {
                        let n = rd_u32(&mut o);
                        let mut u = vec![];
                        for _ in 0..n { if o + 2 <= d.len() { u.push(rd_u16(&mut o)); } }
                        nm = String::from_utf16_lossy(&u).trim_end_matches('\0').to_string();
                    }
                    if (k == "lsct" || k == "lsdk") && o + 4 <= d.len() {
                        let tp = rd_u32(&mut o);
                        if (1..=3).contains(&tp) { divider = true; }
                    }
                    o = ds + l;
                }
                o = ex_end;
                recs.push(Rec { top, left, bottom, right, chans, key, opacity, clip, flags, name: nm, divider });
            }
            for r in recs {
                let cw = (r.right - r.left).max(0) as usize;
                let chh = (r.bottom - r.top).max(0) as usize;
                let mut data: HashMap<i16, Vec<u8>> = HashMap::new();
                for (id, len) in &r.chans {
                    let s = o;
                    if cw > 0 && chh > 0 && (-1..=2).contains(id) && *len >= 2 {
                        if let Some(v) = chan(s, s + len, cw, chh) { data.insert(*id, v); }
                    }
                    o = s + len;
                }
                if r.divider { continue; }
                let mut l = Layer::new(w, h, if r.name.is_empty() { "Слой" } else { &r.name });
                l.opacity = r.opacity as f32 / 255.0;
                l.blend = Blend::from_psd(&r.key);
                l.clip = r.clip == 1;
                l.visible = r.flags & 2 == 0;
                l.alpha_lock = r.flags & 1 != 0;
                if cw > 0 && chh > 0 {
                    let mut rgba = vec![0u8; cw * chh * 4];
                    for i in 0..cw * chh {
                        rgba[i * 4] = data.get(&0).map_or(0, |v| v[i]);
                        rgba[i * 4 + 1] = data.get(&1).map_or(0, |v| v[i]);
                        rgba[i * 4 + 2] = data.get(&2).map_or(0, |v| v[i]);
                        rgba[i * 4 + 3] = data.get(&-1).map_or(255, |v| v[i]);
                    }
                    let part = Img::from_straight(cw, chh, &rgba);
                    l.img_mut().put(r.left, r.top, &part);
                }
                doc.layers.push(l);
            }
        }
        o = lm_end;
    }
    // Нижний сплошной слой «Фон» (так Мазок сохраняет цвет фона) превращаем обратно в фон холста.
    if doc.layers.len() > 1 {
        let l = &doc.layers[0];
        let px = &l.img.px;
        let solid = l.name == "Фон" && l.visible && l.opacity >= 0.999 && l.blend == Blend::Normal && px.len() >= 4 && px[3] == 255
            && px.chunks_exact(4).all(|p| p == &px[0..4]);
        if solid {
            doc.bg = [px[0], px[1], px[2]];
            doc.bg_visible = true;
            doc.layers.remove(0);
        }
    }
    if doc.layers.is_empty() {
        need!(2);
        let comp = rd_u16(&mut o);
        let n = w * h;
        let mut planes: Vec<Vec<u8>> = vec![];
        if comp == 0 {
            for _ in 0..nch { need!(n); planes.push(d[o..o + n].to_vec()); o += n; }
        } else {
            let mut counts = vec![];
            for _ in 0..nch * h { need!(2); counts.push(rd_u16(&mut o) as usize); }
            let mut k = 0;
            for _ in 0..nch {
                let mut pl = vec![0u8; n];
                for y in 0..h { pl[y * w..(y + 1) * w].copy_from_slice(&unpack(o, o + counts[k], w)); o += counts[k]; k += 1; }
                planes.push(pl);
            }
        }
        if planes.len() < 3 { return Err(err()); }
        let mut rgba = vec![0u8; n * 4];
        for i in 0..n {
            rgba[i * 4] = planes[0][i];
            rgba[i * 4 + 1] = planes[1][i];
            rgba[i * 4 + 2] = planes[2][i];
            rgba[i * 4 + 3] = if planes.len() > 3 { planes[3][i] } else { 255 };
        }
        let mut l = Layer::new(w, h, "Фон");
        l.img = Arc::new(Img::from_straight(w, h, &rgba));
        doc.layers.push(l);
    }
    doc.layer_seq = doc.layers.len() as u32 + 1;
    doc.active = doc.layers.len() - 1;
    Ok(doc)
}

/* ---------- GIF ---------- */

pub fn gif(frames: &[Img], delay_cs: u16) -> Result<Vec<u8>, String> {
    let (w, h) = (frames[0].w, frames[0].h);
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &[]).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        for f in frames {
            let mut rgba = f.to_straight();
            let mut fr = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
            fr.delay = delay_cs;
            enc.write_frame(&fr).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// Открыть файл любого поддерживаемого формата как новый документ.
pub fn import(bytes: &[u8], file_name: &str) -> Result<Document, String> {
    let lower = file_name.to_lowercase();
    let stem = std::path::Path::new(file_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Импорт".into());
    if lower.ends_with(".mazok") || lower.ends_with(".zip") {
        let mut d = load_mazok(bytes)?;
        d.id = util::uid();
        return Ok(d);
    }
    if lower.ends_with(".psd") {
        return psd_read(bytes, &stem);
    }
    let img = decode_image(bytes)?;
    let k = (8192.0 / img.w.max(img.h) as f32).min((36e6 / (img.w * img.h) as f32).sqrt()).min(1.0);
    let img = if k < 1.0 { img.resize((img.w as f32 * k) as usize, (img.h as f32 * k) as usize) } else { img };
    let mut d = Document::new(&stem, img.w, img.h, [255, 255, 255], true);
    d.layers[0].name = "Изображение".into();
    d.layers[0].img = Arc::new(img);
    Ok(d)
}

pub fn _unused(_: &Cursor<Vec<u8>>) {}
