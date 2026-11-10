//! Импорт кистей: Photoshop (.abr) и Clip Studio Paint (.sut).
//! Формы отпечатков становятся своими формами (brush::add_tip_image), параметры — настройками кисти.

use crate::brush::{self, Brush};
use crate::raster::Img;
use crate::sqlite::{Db, Val};
use std::collections::HashMap;

/// Отпечаток из файла: маска w×h (0…255 — краска).
pub struct RawTip {
    pub key: String,
    pub w: usize,
    pub h: usize,
    pub mask: Vec<u8>,
}

/// Кисть до регистрации форм (для тестов и предпросмотра).
pub struct Imported {
    pub brush: Brush,
    pub tip: Option<RawTip>,
}

pub fn import(bytes: &[u8], file: &str) -> Result<Vec<Brush>, String> {
    let ext = std::path::Path::new(file).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let stem = std::path::Path::new(file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let list = match ext.as_str() {
        "abr" => abr(bytes, &stem)?,
        "sut" => sut(bytes, &stem)?,
        _ => return Err("Поддерживаются кисти Photoshop (.abr) и Clip Studio Paint (.sut)".into()),
    };
    if list.is_empty() { return Err("В файле не найдено кистей".into()); }
    Ok(list.into_iter().map(register).collect())
}

/// Зарегистрировать форму отпечатка и вернуть готовую кисть.
fn register(mut it: Imported) -> Brush {
    if let Some(t) = it.tip.take() {
        let mut img = Img::new(t.w, t.h);
        for (i, v) in t.mask.iter().enumerate() { img.px[i * 4..i * 4 + 4].copy_from_slice(&[*v, *v, *v, *v]); }
        it.brush.shape = brush::add_tip_image(brush::tip_mask_from_image(&img));
    }
    it.brush
}

fn base_brush(name: &str) -> Brush {
    Brush {
        name: name.chars().take(48).collect(),
        cat: "imported".into(),
        shape: "round".into(),
        hardness: 1.0,
        spacing: 0.25,
        streamline: 0.2,
        size_min: 1.0,
        size_max: 200.0,
        size: 0.45,
        p_size: 0.6,
        p_flow: 0.0,
        min_size: 0.15,
        ..Brush::default()
    }
}

/// Значение «0…100 или 0…1» → 0…1.
fn unit(v: f64) -> f32 {
    let v = v as f32;
    (if v > 1.0 { v / 100.0 } else { v }).clamp(0.0, 1.0)
}

/// Размер в пикселях → настройки размера кисти (диапазон и положение ползунка).
fn set_size(b: &mut Brush, px: f32) {
    let px = px.clamp(1.0, 2500.0);
    b.size_min = (px * 0.02).clamp(0.5, 4.0);
    b.size_max = (px * 2.0).max(8.0);
    // size_px = min + (max - min) * t²
    b.size = ((px - b.size_min) / (b.size_max - b.size_min)).max(0.0).sqrt().clamp(0.0, 1.0);
}

/* ---------- Photoshop ABR ---------- */

struct R<'a> {
    d: &'a [u8],
    o: usize,
}

impl<'a> R<'a> {
    fn new(d: &'a [u8]) -> R<'a> { R { d, o: 0 } }
    fn left(&self) -> usize { self.d.len().saturating_sub(self.o) }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.d.get(self.o..self.o + n).ok_or("Файл кистей обрезан")?;
        self.o += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> { Ok(self.bytes(1)?[0]) }
    fn u16(&mut self) -> Result<u16, String> { let b = self.bytes(2)?; Ok(u16::from_be_bytes([b[0], b[1]])) }
    fn u32(&mut self) -> Result<u32, String> { let b = self.bytes(4)?; Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }
    fn i32(&mut self) -> Result<i32, String> { Ok(self.u32()? as i32) }
    fn f64(&mut self) -> Result<f64, String> { let b = self.bytes(8)?; Ok(f64::from_be_bytes(b.try_into().unwrap())) }
    fn skip(&mut self, n: usize) -> Result<(), String> { self.bytes(n).map(|_| ()) }
    fn ustr(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        if n > 1 << 20 { return Err("Неверная строка".into()); }
        let b = self.bytes(n * 2)?;
        let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
        Ok(String::from_utf16_lossy(&u))
    }
    fn key(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        let n = if n == 0 { 4 } else { n };
        if n > 4096 { return Err("Неверный ключ".into()); }
        Ok(String::from_utf8_lossy(self.bytes(n)?).to_string())
    }
}

/// Распаковка PackBits по строкам (как в PSD).
fn rle_rows(r: &mut R, w: usize, h: usize, bpp: usize) -> Result<Vec<u8>, String> {
    let mut lens = Vec::with_capacity(h);
    for _ in 0..h { lens.push(r.u16()? as usize); }
    let mut out = Vec::with_capacity(w * h * bpp);
    for l in lens {
        let src = r.bytes(l)?;
        let mut row = Vec::with_capacity(w * bpp);
        let mut i = 0;
        while i < src.len() && row.len() < w * bpp {
            let n = src[i] as i8;
            i += 1;
            if n >= 0 {
                let c = n as usize + 1;
                row.extend_from_slice(src.get(i..i + c).unwrap_or(&[]));
                i += c;
            } else if n != -128 {
                let c = (-(n as i32)) as usize + 1;
                let v = *src.get(i).unwrap_or(&0);
                row.extend(std::iter::repeat(v).take(c));
                i += 1;
            }
        }
        row.resize(w * bpp, 0);
        out.extend_from_slice(&row);
    }
    Ok(out)
}

fn read_bitmap(r: &mut R, w: usize, h: usize, depth: u16, compressed: bool) -> Result<Vec<u8>, String> {
    if w == 0 || h == 0 || w > 10000 || h > 10000 { return Err("Неверный размер отпечатка".into()); }
    let bpp = (depth as usize / 8).max(1);
    let data = if compressed { rle_rows(r, w, h, bpp)? } else { r.bytes(w * h * bpp)?.to_vec() };
    Ok(if bpp == 1 { data } else { data.chunks_exact(bpp).map(|c| c[0]).collect() })
}

fn abr(d: &[u8], stem: &str) -> Result<Vec<Imported>, String> {
    let mut r = R::new(d);
    let ver = r.u16()?;
    match ver {
        1 | 2 => abr_v12(&mut r, ver, stem),
        6 | 7 | 10 => abr_v6(d, stem),
        _ => Err(format!("Версия файла кистей {ver} не поддерживается")),
    }
}

fn abr_v12(r: &mut R, ver: u16, stem: &str) -> Result<Vec<Imported>, String> {
    let count = r.u16()? as usize;
    let mut out = vec![];
    for i in 0..count {
        if r.left() < 6 { break; }
        let kind = r.u16()?;
        let size = r.u32()? as usize;
        let next = r.o + size;
        match kind {
            1 => {
                // вычисляемая кисть: круглая с жёсткостью
                let _misc = r.u32()?;
                let spacing = r.u16()?;
                let diameter = r.u16()?;
                let roundness = r.u16()?;
                let angle = r.u16()? as i16;
                let hardness = r.u16()?;
                let mut b = base_brush(&format!("{stem} {}", i + 1));
                set_size(&mut b, diameter as f32);
                b.spacing = (spacing as f32 / 100.0).clamp(0.01, 2.5);
                b.roundness = (roundness as f32 / 100.0).clamp(0.02, 1.0);
                b.angle = angle as f32;
                b.hardness = (hardness as f32 / 100.0).clamp(0.0, 1.0);
                out.push(Imported { brush: b, tip: None });
            }
            2 => {
                let _misc = r.u32()?;
                let spacing = r.u16()?;
                let name = if ver == 2 { r.ustr().unwrap_or_default() } else { String::new() };
                let _aa = r.u8()?;
                r.skip(8)?;
                let (top, left, bottom, right) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
                let depth = r.u16()?;
                let comp = r.u8()?;
                let (w, h) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
                let mask = read_bitmap(r, w, h, depth, comp != 0)?;
                let mut b = base_brush(if name.trim().is_empty() { stem } else { name.trim() });
                if name.trim().is_empty() { b.name = format!("{stem} {}", i + 1); }
                set_size(&mut b, w.max(h) as f32);
                b.spacing = (spacing as f32 / 100.0).clamp(0.01, 2.5);
                out.push(Imported { brush: b, tip: Some(RawTip { key: String::new(), w, h, mask }) });
            }
            _ => {}
        }
        r.o = next.min(r.d.len());
    }
    Ok(out)
}

/// Найти раздел 8BIM с ключом (samp, desc, patt…).
fn section<'a>(d: &'a [u8], key: &[u8; 4]) -> Option<&'a [u8]> {
    let mut o = 4;
    while o + 12 <= d.len() {
        if &d[o..o + 4] != b"8BIM" { return None; }
        let k = &d[o + 4..o + 8];
        let len = u32::from_be_bytes([d[o + 8], d[o + 9], d[o + 10], d[o + 11]]) as usize;
        let body = o + 12;
        let end = (body + len).min(d.len());
        if k == key { return Some(&d[body..end]); }
        o = body + len;
    }
    None
}

fn abr_v6(d: &[u8], stem: &str) -> Result<Vec<Imported>, String> {
    let sub = u16::from_be_bytes([d[2], d[3]]);
    let samp = section(d, b"samp").ok_or("В файле нет отпечатков кистей")?;
    let mut tips: Vec<RawTip> = vec![];
    let mut r = R::new(samp);
    while r.left() > 4 {
        let size = r.u32()? as usize;
        let start = r.o;
        let padded = (size + 3) / 4 * 4;
        let next = start + padded;
        let res: Result<RawTip, String> = (|| {
            let klen = r.u8()? as usize;
            let key = String::from_utf8_lossy(r.bytes(klen)?).to_string();
            // после ключа: служебные данные (47 байт всего для подверсии 1, 301 — для 2)
            let skip = if sub == 1 { 47 } else { 301 };
            r.o = start + skip;
            let (top, left, bottom, right) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
            let depth = r.u16()?;
            let comp = r.u8()?;
            let (w, h) = ((right - left).max(0) as usize, (bottom - top).max(0) as usize);
            let mask = read_bitmap(&mut r, w, h, depth, comp != 0)?;
            Ok(RawTip { key, w, h, mask })
        })();
        if let Ok(t) = res { tips.push(t); }
        if next <= start || next > samp.len() { break; }
        r.o = next;
    }
    if tips.is_empty() { return Err("Не удалось прочитать отпечатки кистей".into()); }
    // описания пресетов (имена, размер, интервал, угол)
    let presets = section(d, b"desc").and_then(|s| desc_presets(s).ok()).unwrap_or_default();
    let mut out = vec![];
    let tip_by_key: HashMap<String, usize> = tips.iter().enumerate().map(|(i, t)| (t.key.clone(), i)).collect();
    let mut taken = vec![false; tips.len()];
    for p in &presets {
        let mut b = base_brush(if p.name.is_empty() { stem } else { &p.name });
        let tip_idx = p.sample.as_ref().and_then(|k| tip_by_key.get(k).copied());
        if let Some(px) = p.diameter { set_size(&mut b, px as f32); }
        if let Some(s) = p.spacing { b.spacing = (s as f32 / 100.0).clamp(0.01, 2.5); }
        if let Some(a) = p.angle { b.angle = a as f32; }
        if let Some(rn) = p.roundness { b.roundness = (rn as f32 / 100.0).clamp(0.02, 1.0); }
        if let Some(h) = p.hardness { b.hardness = (h as f32 / 100.0).clamp(0.0, 1.0); }
        if p.size_jitter > 0.0 { b.j_size = unit(p.size_jitter); }
        if p.angle_jitter > 0.0 { b.j_angle = unit(p.angle_jitter); }
        if p.scatter > 0.0 { b.scatter = (p.scatter as f32 / 100.0).clamp(0.0, 3.0); }
        if p.opacity_jitter > 0.0 { b.j_opacity = unit(p.opacity_jitter); }
        let tip = match tip_idx {
            Some(i) => {
                taken[i] = true;
                let t = &tips[i];
                if p.diameter.is_none() { set_size(&mut b, t.w.max(t.h) as f32); }
                Some(RawTip { key: t.key.clone(), w: t.w, h: t.h, mask: t.mask.clone() })
            }
            None if p.sample.is_some() => continue,
            None => None,
        };
        out.push(Imported { brush: b, tip });
    }
    // отпечатки без описаний — отдельными кистями
    let mut k = 0;
    for (i, t) in tips.into_iter().enumerate() {
        if taken[i] { continue; }
        k += 1;
        let mut b = base_brush(&format!("{stem} {k}"));
        set_size(&mut b, t.w.max(t.h) as f32);
        out.push(Imported { brush: b, tip: Some(t) });
    }
    Ok(out)
}

#[derive(Default, Debug)]
struct Preset {
    name: String,
    sample: Option<String>,
    diameter: Option<f64>,
    spacing: Option<f64>,
    angle: Option<f64>,
    roundness: Option<f64>,
    hardness: Option<f64>,
    size_jitter: f64,
    angle_jitter: f64,
    scatter: f64,
    opacity_jitter: f64,
}

#[derive(Debug, Clone)]
enum DV {
    Num(f64),
    Text(String),
    Bool(bool),
    Obj(Vec<(String, DV)>),
    List(Vec<DV>),
    Other,
}

fn desc_value(r: &mut R, ty: &[u8], depth: usize) -> Result<DV, String> {
    if depth > 32 { return Err("Слишком глубокое описание".into()); }
    Ok(match ty {
        b"Objc" | b"GlbO" => DV::Obj(descriptor(r, depth + 1)?),
        b"VlLs" => {
            let n = r.u32()? as usize;
            let mut v = vec![];
            for _ in 0..n.min(100_000) {
                let t = r.bytes(4)?.to_vec();
                v.push(desc_value(r, &t, depth + 1)?);
            }
            DV::List(v)
        }
        b"doub" => DV::Num(r.f64()?),
        b"UntF" => { r.skip(4)?; DV::Num(r.f64()?) }
        b"UnFl" => { r.skip(4)?; let n = r.u32()? as usize; let mut v = 0.0; for _ in 0..n { v = r.f64()?; } DV::Num(v) }
        b"TEXT" => DV::Text(r.ustr()?),
        b"enum" => { r.key()?; DV::Text(r.key()?) }
        b"long" => DV::Num(r.i32()? as f64),
        b"comp" => { let hi = r.u32()? as u64; let lo = r.u32()? as u64; DV::Num(((hi << 32) | lo) as i64 as f64) }
        b"bool" => DV::Bool(r.u8()? != 0),
        b"type" | b"GlbC" => { r.ustr()?; r.key()?; DV::Other }
        b"alis" | b"tdta" | b"Pth " => { let n = r.u32()? as usize; r.skip(n)?; DV::Other }
        b"obj " => {
            let n = r.u32()? as usize;
            for _ in 0..n {
                let t = r.bytes(4)?.to_vec();
                match &t[..] {
                    b"prop" => { r.ustr()?; r.key()?; r.key()?; }
                    b"Clss" => { r.ustr()?; r.key()?; }
                    b"Enmr" => { r.ustr()?; r.key()?; r.key()?; r.key()?; }
                    b"rele" => { r.ustr()?; r.key()?; r.u32()?; }
                    b"Idnt" | b"indx" => { r.u32()?; }
                    b"name" => { r.ustr()?; r.key()?; r.ustr()?; }
                    _ => return Err("Неизвестная ссылка".into()),
                }
            }
            DV::Other
        }
        _ => return Err(format!("Неизвестный тип {:?}", String::from_utf8_lossy(ty))),
    })
}

fn descriptor(r: &mut R, depth: usize) -> Result<Vec<(String, DV)>, String> {
    r.ustr()?;
    r.key()?;
    let n = r.u32()? as usize;
    let mut items = vec![];
    for _ in 0..n.min(100_000) {
        let k = r.key()?;
        let t = r.bytes(4)?.to_vec();
        items.push((k, desc_value(r, &t, depth)?));
    }
    Ok(items)
}

fn find<'a>(o: &'a [(String, DV)], key: &str) -> Option<&'a DV> {
    o.iter().find(|(k, _)| k.trim() == key).map(|(_, v)| v)
}

fn num(o: &[(String, DV)], key: &str) -> Option<f64> {
    match find(o, key)? { DV::Num(n) => Some(*n), _ => None }
}

fn jitter(o: &[(String, DV)], key: &str) -> f64 {
    match find(o, key) { Some(DV::Obj(v)) => num(v, "jitter").unwrap_or(0.0), _ => 0.0 }
}

fn desc_presets(s: &[u8]) -> Result<Vec<Preset>, String> {
    let mut r = R::new(s);
    r.u32()?; // версия описания (16)
    let root = descriptor(&mut r, 0)?;
    let Some(DV::List(list)) = find(&root, "Brsh") else { return Ok(vec![]) };
    let mut out = vec![];
    for it in list {
        let DV::Obj(p) = it else { continue };
        let mut pr = Preset { name: match find(p, "Nm") { Some(DV::Text(t)) => t.clone(), _ => String::new() }, ..Default::default() };
        if let Some(DV::Obj(b)) = find(p, "Brsh") {
            pr.diameter = num(b, "Dmtr");
            pr.spacing = num(b, "Spcn");
            pr.angle = num(b, "Angl");
            pr.roundness = num(b, "Rndn");
            pr.hardness = num(b, "Hrdn");
            if let Some(DV::Text(t)) = find(b, "sampledData") { pr.sample = Some(t.clone()); }
        }
        pr.size_jitter = jitter(p, "szVr");
        pr.angle_jitter = jitter(p, "angleDynamics");
        pr.opacity_jitter = jitter(p, "opVr");
        if let Some(DV::Bool(true)) = find(p, "useScatter") { pr.scatter = num(p, "Sctr").unwrap_or(0.0); }
        out.push(pr);
    }
    Ok(out)
}

/* ---------- Clip Studio Paint SUT ---------- */

/// Найти PNG-картинки внутри двоичных данных материала.
fn embedded_pngs(b: &[u8]) -> Vec<&[u8]> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let mut out = vec![];
    let mut o = 0;
    while let Some(p) = b[o..].windows(8).position(|w| w == SIG) {
        let s = o + p;
        match b[s..].windows(4).position(|w| w == b"IEND") {
            Some(e) => {
                let end = (s + e + 8).min(b.len());
                out.push(&b[s..end]);
                o = end;
            }
            None => break,
        }
        if o >= b.len() { break; }
    }
    out
}

fn sut(d: &[u8], stem: &str) -> Result<Vec<Imported>, String> {
    let db = Db::open(d)?;
    // формы отпечатков из материалов
    let mut tips: Vec<RawTip> = vec![];
    if db.has("MaterialFile") {
        let t = db.table("MaterialFile")?;
        for row in &t.rows {
            let Some(blob) = t.get(row, "FileData").and_then(|v| v.blob()) else { continue };
            // берём самую большую из встроенных картинок (вторая обычно — миниатюра)
            let best = embedded_pngs(blob).into_iter().filter_map(|p| crate::formats::decode_image(p).ok()).max_by_key(|i| i.w * i.h);
            if let Some(img) = best {
                let m = brush::tip_mask_from_image(&img);
                // в сыром виде маска 512×512
                tips.push(RawTip { key: String::new(), w: 512, h: 512, mask: m.iter().map(|v| (v * 255.0).round() as u8).collect() });
            }
        }
    }
    // имя и параметры кисти
    let mut name = stem.to_string();
    let mut var_id: Option<i64> = None;
    if db.has("Node") {
        let t = db.table("Node")?;
        for row in &t.rows {
            let vid = t.get(row, "NodeVariantID").and_then(|v| v.f()).map(|v| v as i64).filter(|v| *v > 0);
            if let Some(n) = t.get(row, "NodeName").and_then(|v| v.text()).filter(|s| !s.trim().is_empty()) {
                if vid.is_some() || var_id.is_none() { name = n; }
            }
            if vid.is_some() { var_id = vid; break; }
        }
    }
    let mut b = base_brush(&name);
    if db.has("Variant") {
        let t = db.table("Variant")?;
        let row = t.rows.iter().find(|r| var_id.map_or(true, |id| t.get(r, "VariantID").and_then(|v| v.f()).map(|v| v as i64) == Some(id))).or(t.rows.first());
        if let Some(row) = row {
            let g = |k: &str| t.get(row, k).and_then(|v: &Val| v.f());
            if let Some(s) = g("BrushSize") { set_size(&mut b, s as f32); }
            if let Some(o) = g("Opacity") { b.opacity = unit(o).max(0.05); }
            if let Some(h) = g("BrushHardness") { b.hardness = unit(h); }
            if let Some(f) = g("BrushFlow") { b.flow = unit(f).max(0.02); }
            if let Some(i) = g("BrushInterval") {
                // интервал хранится в процентах от размера (или долях)
                let v = if i > 3.0 { i / 100.0 } else { i };
                b.spacing = (v as f32).clamp(0.01, 2.5);
            }
            if let Some(rt) = g("BrushRotation") { b.angle = rt as f32; }
            if let Some(th) = g("BrushThickness") { let th = unit(th); if th > 0.0 { b.roundness = th.clamp(0.02, 1.0); } }
            if let Some(hj) = g("BrushHueJitter") { b.j_hue = unit(hj) * 0.5; }
            if let Some(sj) = g("BrushSaturationJitter") { b.j_sat = unit(sj) * 0.5; }
            if let Some(vj) = g("BrushValueJitter") { b.j_bri = unit(vj) * 0.5; }
            if g("BrushUseSpray").unwrap_or(0.0) > 0.0 {
                b.scatter = 1.0;
                if let Some(dn) = g("BrushSprayDensity") { b.count = (dn as f32 / 10.0).clamp(1.0, 8.0); }
            }
        }
    }
    let mut out = vec![];
    if tips.is_empty() {
        out.push(Imported { brush: b, tip: None });
    } else {
        let n = tips.len();
        for (i, t) in tips.into_iter().enumerate() {
            let mut bb = b.clone();
            if n > 1 { bb.name = format!("{} {}", b.name, i + 1); }
            out.push(Imported { brush: bb, tip: Some(t) });
        }
    }
    Ok(out)
}

/* ---------- экспорт формы в PNG (для обмена) ---------- */

/// Картинка формы отпечатка: чёрная форма на прозрачном фоне.
pub fn tip_png(shape: &str) -> Vec<u8> {
    let m = brush::tip_mask(shape);
    let mut img = Img::new(512, 512);
    for (i, v) in m.iter().enumerate() {
        let a = (v * 255.0).round() as u8;
        img.px[i * 4 + 3] = a;
    }
    crate::formats::png(&img)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Собрать ABR v6 (подверсия 2) с одним отпечатком и описанием пресета.
    fn abr6() -> Vec<u8> {
        let (w, h) = (20usize, 10usize);
        let mut samp = vec![];
        let key = b"12345678-1234-1234-1234-123456789abc";
        let mut body = vec![key.len() as u8];
        body.extend_from_slice(key);
        body.resize(301, 0);
        for v in [0i32, 0, h as i32, w as i32] { body.extend_from_slice(&v.to_be_bytes()); }
        body.extend_from_slice(&8u16.to_be_bytes());
        body.push(1); // RLE
        let row: Vec<u8> = vec![(-(w as i32 - 1)) as i8 as u8, 200];
        for _ in 0..h { body.extend_from_slice(&(row.len() as u16).to_be_bytes()); }
        for _ in 0..h { body.extend_from_slice(&row); }
        samp.extend_from_slice(&(body.len() as u32).to_be_bytes());
        samp.extend_from_slice(&body);
        while samp.len() % 4 != 0 { samp.push(0); }
        // описание: { Brsh: [ { Nm: "Тест", Brsh: { Dmtr: 30, Spcn: 40, sampledData: key } } ] }
        let ustr = |s: &str| { let u: Vec<u16> = s.encode_utf16().chain([0]).collect(); let mut v = (u.len() as u32).to_be_bytes().to_vec(); for c in u { v.extend_from_slice(&c.to_be_bytes()); } v };
        let k4 = |s: &str| { let mut v = 0u32.to_be_bytes().to_vec(); v.extend_from_slice(s.as_bytes()); v };
        let kl = |s: &str| { let mut v = (s.len() as u32).to_be_bytes().to_vec(); v.extend_from_slice(s.as_bytes()); v };
        let untf = |v: f64| { let mut b = b"UntF#Pxl".to_vec(); b.extend_from_slice(&v.to_be_bytes()); b };
        let mut inner = ustr(""); inner.extend(k4("smpl")); inner.extend(3u32.to_be_bytes());
        inner.extend(k4("Dmtr")); inner.extend(untf(30.0));
        inner.extend(k4("Spcn")); inner.extend(b"UntF#Prc"); inner.extend(40f64.to_be_bytes());
        inner.extend(kl("sampledData")); inner.extend(b"TEXT"); inner.extend(ustr(std::str::from_utf8(key).unwrap()));
        let mut preset = ustr(""); preset.extend(k4("null")); preset.extend(2u32.to_be_bytes());
        preset.extend(k4("Nm  ")); preset.extend(b"TEXT"); preset.extend(ustr("Тест"));
        preset.extend(k4("Brsh")); preset.extend(b"Objc"); preset.extend(inner);
        let mut desc = 16u32.to_be_bytes().to_vec();
        desc.extend(ustr("")); desc.extend(k4("null")); desc.extend(1u32.to_be_bytes());
        desc.extend(k4("Brsh")); desc.extend(b"VlLs"); desc.extend(1u32.to_be_bytes()); desc.extend(b"Objc"); desc.extend(preset);
        let mut d = vec![0, 10, 0, 2];
        for (k, b) in [(b"samp", &samp), (b"desc", &desc)] {
            d.extend_from_slice(b"8BIM"); d.extend_from_slice(k); d.extend_from_slice(&(b.len() as u32).to_be_bytes()); d.extend_from_slice(b);
        }
        d
    }

    #[test]
    fn abr_v6_with_desc() {
        let list = abr(&abr6(), "set").unwrap();
        assert_eq!(list.len(), 1);
        let b = &list[0];
        assert_eq!(b.brush.name, "Тест");
        assert!((b.brush.spacing - 0.4).abs() < 1e-4);
        let t = b.tip.as_ref().unwrap();
        assert_eq!((t.w, t.h), (20, 10));
        assert!(t.mask.iter().all(|v| *v == 200));
        let px = brush::size_px(&b.brush, b.brush.size);
        assert!((px - 30.0).abs() < 1.0, "размер {px}");
    }

    #[test]
    fn abr_v2_sampled() {
        let (w, h) = (4i32, 3i32);
        let mut body = vec![];
        body.extend(0u32.to_be_bytes()); body.extend(25u16.to_be_bytes());
        let name: Vec<u16> = "Кисть".encode_utf16().chain([0]).collect();
        body.extend((name.len() as u32).to_be_bytes()); for c in name { body.extend(c.to_be_bytes()); }
        body.push(1); body.extend([0u8; 8]);
        for v in [0, 0, h, w] { body.extend(v.to_be_bytes()); }
        body.extend(8u16.to_be_bytes()); body.push(0);
        body.extend(vec![255u8; (w * h) as usize]);
        let mut d = vec![0, 2, 0, 1, 0, 2];
        d.extend((body.len() as u32).to_be_bytes()); d.extend(body);
        let list = abr(&d, "x").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].brush.name, "Кисть");
        assert_eq!(list[0].tip.as_ref().unwrap().mask.len(), 12);
    }

    #[test]
    fn png_carving() {
        let png = crate::formats::png(&Img::filled(3, 2, [0, 0, 0, 255]));
        let mut blob = b"CSFCHUNK....junk".to_vec();
        blob.extend_from_slice(&png);
        blob.extend_from_slice(b"tail");
        let found = embedded_pngs(&blob);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], &png[..]);
    }

    #[test]
    fn sut_from_real_sqlite() {
        // настоящая база SQLite с такой же схемой, как у Clip Studio Paint
        let dir = std::env::temp_dir().join(format!("mazok-sut-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.sut");
        let _ = std::fs::remove_file(&path);
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE Manager(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, ToolType INTEGER, Version INTEGER);
                CREATE TABLE Node(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, NodeUuid BLOB, NodeName TEXT, NodeVariantID INTEGER);
                CREATE TABLE Variant(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, VariantID INTEGER, Opacity INTEGER, BrushSize REAL, BrushHardness INTEGER, BrushInterval REAL, BrushFlow INTEGER);
                CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, InstallFolder INTEGER, OriginalPath TEXT, FileData BLOB);").unwrap();
            c.execute("INSERT INTO Node(NodeName, NodeVariantID) VALUES ('Сухая тушь', 7)", []).unwrap();
            for i in 0..300 { c.execute("INSERT INTO Variant(VariantID, Opacity, BrushSize, BrushHardness, BrushInterval, BrushFlow) VALUES (?1, 80, 42.5, 60, 15.0, 90)", [i + 100]).unwrap(); }
            c.execute("INSERT INTO Variant(VariantID, Opacity, BrushSize, BrushHardness, BrushInterval, BrushFlow) VALUES (7, 70, 64.0, 50, 20.0, 100)", []).unwrap();
            // материал: служебные байты + PNG формы (чёрный круг на прозрачном) + миниатюра
            let mut img = Img::new(300, 300);
            for y in 0..300 { for x in 0..300 { if ((x as f32 - 150.0).powi(2) + (y as f32 - 150.0).powi(2)).sqrt() < 120.0 { let i = (y * 300 + x) * 4; img.px[i + 3] = 255; } } }
            let mut blob = b"CSFCHUNK".to_vec();
            blob.extend(vec![7u8; 70_000]);
            blob.extend(crate::formats::png(&img));
            blob.extend(vec![1u8; 5000]);
            blob.extend(crate::formats::png(&Img::filled(16, 16, [0, 0, 0, 255])));
            c.execute("INSERT INTO MaterialFile(InstallFolder, OriginalPath, FileData) VALUES (0, 'tip', ?1)", [blob]).unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&bytes).unwrap();
        assert_eq!(db.table("Variant").unwrap().rows.len(), 301);
        let list = sut(&bytes, "test").unwrap();
        assert_eq!(list.len(), 1);
        let b = &list[0].brush;
        assert_eq!(b.name, "Сухая тушь");
        assert!((b.opacity - 0.7).abs() < 1e-4 && (b.hardness - 0.5).abs() < 1e-4 && (b.spacing - 0.2).abs() < 1e-4);
        let px = brush::size_px(b, b.size);
        assert!((px - 64.0).abs() < 1.0, "размер {px}");
        let t = list[0].tip.as_ref().unwrap();
        // круг занимает почти весь квадрат формы, центр закрашен, угол пуст
        assert!(t.mask[256 * 512 + 256] > 200);
        assert!(t.mask[5 * 512 + 5] < 10);
    }
}