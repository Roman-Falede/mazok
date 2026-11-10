//! Палитры: импорт и экспорт .aco (Photoshop), .ase (Adobe Swatch Exchange), .gpl (GIMP, Krita, Inkscape),
//! .pal (JASC / Paint Shop Pro), .swatches (Procreate), .hex / .txt (список цветов).

use crate::state::Palette;
use crate::util;

pub const EXTS: [&str; 7] = ["aco", "ase", "gpl", "pal", "swatches", "hex", "txt"];

fn stem(file: &str) -> String {
    std::path::Path::new(file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Палитра".into())
}

fn ext(file: &str) -> String {
    std::path::Path::new(file).extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Прочитать палитры из файла (в .ase и .swatches их может быть несколько).
pub fn import(bytes: &[u8], file: &str) -> Result<Vec<Palette>, String> {
    let name = stem(file);
    let mut out = match ext(file).as_str() {
        "aco" => vec![aco_read(bytes, &name)?],
        "ase" => ase_read(bytes, &name)?,
        "swatches" => swatches_read(bytes, &name)?,
        "gpl" => vec![gpl_read(&String::from_utf8_lossy(bytes), &name)?],
        "pal" => {
            if bytes.starts_with(b"RIFF") { vec![riff_pal_read(bytes, &name)?] } else { vec![jasc_read(&String::from_utf8_lossy(bytes), &name)?] }
        }
        _ => vec![hex_read(&String::from_utf8_lossy(bytes), &name)?],
    };
    out.retain(|p| !p.colors.is_empty());
    if out.is_empty() { return Err("В файле нет цветов".into()); }
    for p in out.iter_mut() {
        if p.name.trim().is_empty() { p.name = name.clone(); }
        if p.names.len() != p.colors.len() { p.names.resize(p.colors.len(), String::new()); }
    }
    Ok(out)
}

/* ---------- Photoshop .aco ---------- */

fn be16(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn aco_color(space: u16, w: u16, x: u16, y: u16, z: u16) -> Option<[u8; 3]> {
    let f = |v: u16| v as f32 / 65535.0;
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match space {
        0 => Some([(w >> 8) as u8, (x >> 8) as u8, (y >> 8) as u8]),
        1 => Some(util::hsv2rgb(w as f32 / 65535.0 * 360.0, f(x), f(y))),
        2 => {
            // CMYK: 0 = 100 % краски
            let (c, m, yy, k) = (1.0 - f(w), 1.0 - f(x), 1.0 - f(y), 1.0 - f(z));
            Some([to8((1.0 - c) * (1.0 - k)), to8((1.0 - m) * (1.0 - k)), to8((1.0 - yy) * (1.0 - k))])
        }
        7 => {
            let l = w as f32 / 100.0;
            let a = x as i16 as f32 / 100.0;
            let b = y as i16 as f32 / 100.0;
            Some(lab_to_rgb(l, a, b))
        }
        8 => { let g = to8(1.0 - (w as f32 / 10000.0)); Some([g, g, g]) }
        _ => None,
    }
}

fn lab_to_rgb(l: f32, a: f32, b: f32) -> [u8; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inv = |t: f32| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.95047 * inv(fx), inv(fy), 1.08883 * inv(fz));
    let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
    let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
    let bb = 0.0557 * x - 0.2040 * y + 1.0570 * z;
    let gam = |c: f32| { let c = c.clamp(0.0, 1.0); if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 } };
    [(gam(r) * 255.0).round() as u8, (gam(g) * 255.0).round() as u8, (gam(bb) * 255.0).round() as u8]
}

fn aco_read(d: &[u8], name: &str) -> Result<Palette, String> {
    let bad = || "Неверный файл .aco".to_string();
    let mut o = 0;
    let mut best: Option<Palette> = None;
    while o + 4 <= d.len() {
        let ver = be16(d, o).ok_or_else(bad)?;
        let n = be16(d, o + 2).ok_or_else(bad)? as usize;
        if ver != 1 && ver != 2 { break; }
        o += 4;
        let mut p = Palette { name: name.into(), ..Default::default() };
        for _ in 0..n {
            let v: Vec<u16> = (0..5).map(|k| be16(d, o + k * 2).unwrap_or(0)).collect();
            if o + 10 > d.len() { return Err(bad()); }
            o += 10;
            let mut cname = String::new();
            if ver == 2 {
                // 0x0000, длина (в символах UTF-16, с завершающим нулём), имя
                let len = be16(d, o + 2).ok_or_else(bad)? as usize;
                o += 4;
                let units: Vec<u16> = (0..len).filter_map(|k| be16(d, o + k * 2)).take_while(|u| *u != 0).collect();
                cname = String::from_utf16_lossy(&units);
                o += len * 2;
            }
            if let Some(c) = aco_color(v[0], v[1], v[2], v[3], v[4]) {
                p.colors.push(c);
                p.names.push(cname);
            }
        }
        // раздел версии 2 (с именами) предпочтительнее
        best = Some(p);
        if ver == 2 { break; }
    }
    best.ok_or_else(bad)
}

pub fn aco_write(p: &Palette) -> Vec<u8> {
    let mut d = vec![];
    for ver in [1u16, 2] {
        d.extend_from_slice(&ver.to_be_bytes());
        d.extend_from_slice(&(p.colors.len() as u16).to_be_bytes());
        for (i, c) in p.colors.iter().enumerate() {
            d.extend_from_slice(&0u16.to_be_bytes());
            for k in 0..3 { d.extend_from_slice(&(c[k] as u16 * 257).to_be_bytes()); }
            d.extend_from_slice(&0u16.to_be_bytes());
            if ver == 2 {
                let nm = p.names.get(i).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| util::hex(*c));
                let units: Vec<u16> = nm.encode_utf16().collect();
                d.extend_from_slice(&0u16.to_be_bytes());
                d.extend_from_slice(&((units.len() + 1) as u16).to_be_bytes());
                for u in units { d.extend_from_slice(&u.to_be_bytes()); }
                d.extend_from_slice(&0u16.to_be_bytes());
            }
        }
    }
    d
}

/* ---------- Adobe Swatch Exchange .ase ---------- */

fn be32(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn ase_read(d: &[u8], name: &str) -> Result<Vec<Palette>, String> {
    if !d.starts_with(b"ASEF") { return Err("Неверный файл .ase".into()); }
    let blocks = be32(d, 8).unwrap_or(0) as usize;
    let mut o = 12;
    let mut out: Vec<Palette> = vec![Palette { name: name.into(), ..Default::default() }];
    let mut in_group = false;
    for _ in 0..blocks {
        let (Some(kind), Some(len)) = (be16(d, o), be32(d, o + 2)) else { break };
        let body = o + 6;
        let end = body + len as usize;
        if end > d.len() { break; }
        let read_name = |at: usize| -> (String, usize) {
            let n = be16(d, at).unwrap_or(0) as usize;
            let units: Vec<u16> = (0..n).filter_map(|k| be16(d, at + 2 + k * 2)).take_while(|u| *u != 0).collect();
            (String::from_utf16_lossy(&units), at + 2 + n * 2)
        };
        match kind {
            0xc001 => {
                let (g, _) = read_name(body);
                out.push(Palette { name: g, ..Default::default() });
                in_group = true;
            }
            0xc002 => { in_group = false; }
            0x0001 => {
                let (cname, at) = read_name(body);
                let model = d.get(at..at + 4).unwrap_or(b"    ");
                let fl = |k: usize| d.get(at + 4 + k * 4..at + 8 + k * 4).map(|b| f32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0.0);
                let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                let c = match model {
                    b"RGB " => Some([to8(fl(0)), to8(fl(1)), to8(fl(2))]),
                    b"CMYK" => { let k = fl(3); Some([to8((1.0 - fl(0)) * (1.0 - k)), to8((1.0 - fl(1)) * (1.0 - k)), to8((1.0 - fl(2)) * (1.0 - k))]) }
                    b"LAB " => Some(lab_to_rgb(fl(0) * 100.0, fl(1), fl(2))),
                    b"Gray" => { let g = to8(fl(0)); Some([g, g, g]) }
                    _ => None,
                };
                if let Some(c) = c {
                    let target = if in_group { out.len() - 1 } else { 0 };
                    out[target].colors.push(c);
                    out[target].names.push(cname);
                }
            }
            _ => {}
        }
        o = end;
    }
    Ok(out)
}

pub fn ase_write(p: &Palette) -> Vec<u8> {
    let mut d = b"ASEF".to_vec();
    d.extend_from_slice(&1u16.to_be_bytes());
    d.extend_from_slice(&0u16.to_be_bytes());
    d.extend_from_slice(&((p.colors.len() + 2) as u32).to_be_bytes());
    let name_bytes = |s: &str| -> Vec<u8> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut v = ((units.len() + 1) as u16).to_be_bytes().to_vec();
        for u in units { v.extend_from_slice(&u.to_be_bytes()); }
        v.extend_from_slice(&0u16.to_be_bytes());
        v
    };
    let g = name_bytes(&p.name);
    d.extend_from_slice(&0xc001u16.to_be_bytes());
    d.extend_from_slice(&(g.len() as u32).to_be_bytes());
    d.extend_from_slice(&g);
    for (i, c) in p.colors.iter().enumerate() {
        let nm = p.names.get(i).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| util::hex(*c));
        let mut b = name_bytes(&nm);
        b.extend_from_slice(b"RGB ");
        for k in 0..3 { b.extend_from_slice(&(c[k] as f32 / 255.0).to_be_bytes()); }
        b.extend_from_slice(&2u16.to_be_bytes()); // обычный цвет
        d.extend_from_slice(&0x0001u16.to_be_bytes());
        d.extend_from_slice(&(b.len() as u32).to_be_bytes());
        d.extend_from_slice(&b);
    }
    d.extend_from_slice(&0xc002u16.to_be_bytes());
    d.extend_from_slice(&0u32.to_be_bytes());
    d
}

/* ---------- текстовые форматы ---------- */

fn gpl_read(s: &str, name: &str) -> Result<Palette, String> {
    let mut lines = s.lines();
    if !lines.next().unwrap_or("").trim_start_matches('\u{feff}').trim().starts_with("GIMP Palette") {
        return Err("Неверный файл .gpl".into());
    }
    let mut p = Palette { name: name.into(), ..Default::default() };
    for l in lines {
        let t = l.trim();
        if let Some(n) = t.strip_prefix("Name:") { p.name = n.trim().to_string(); continue; }
        if t.is_empty() || t.starts_with('#') || t.starts_with("Columns:") { continue; }
        let mut it = t.split_whitespace();
        let nums: Vec<u8> = it.by_ref().take(3).filter_map(|v| v.parse::<u16>().ok().map(|v| v.min(255) as u8)).collect();
        if nums.len() == 3 {
            p.colors.push([nums[0], nums[1], nums[2]]);
            p.names.push(it.collect::<Vec<_>>().join(" "));
        }
    }
    Ok(p)
}

pub fn gpl_write(p: &Palette) -> Vec<u8> {
    let mut s = format!("GIMP Palette\nName: {}\nColumns: 8\n#\n", p.name);
    for (i, c) in p.colors.iter().enumerate() {
        let nm = p.names.get(i).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| util::hex(*c));
        s.push_str(&format!("{:3} {:3} {:3}\t{}\n", c[0], c[1], c[2], nm));
    }
    s.into_bytes()
}

fn jasc_read(s: &str, name: &str) -> Result<Palette, String> {
    let mut lines = s.lines().map(|l| l.trim());
    if lines.next() != Some("JASC-PAL") { return hex_read(s, name); }
    let mut p = Palette { name: name.into(), ..Default::default() };
    for l in lines.skip(2) {
        let n: Vec<u8> = l.split_whitespace().filter_map(|v| v.parse::<u16>().ok().map(|v| v.min(255) as u8)).collect();
        if n.len() >= 3 { p.colors.push([n[0], n[1], n[2]]); p.names.push(String::new()); }
    }
    Ok(p)
}

fn riff_pal_read(d: &[u8], name: &str) -> Result<Palette, String> {
    let pos = d.windows(4).position(|w| w == b"data").ok_or("Неверный файл .pal")?;
    let n = d.get(pos + 10..pos + 12).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0) as usize;
    let mut p = Palette { name: name.into(), ..Default::default() };
    for i in 0..n {
        let o = pos + 12 + i * 4;
        if let Some(c) = d.get(o..o + 3) { p.colors.push([c[0], c[1], c[2]]); p.names.push(String::new()); }
    }
    Ok(p)
}

fn hex_read(s: &str, name: &str) -> Result<Palette, String> {
    let mut p = Palette { name: name.into(), ..Default::default() };
    for l in s.lines() {
        let t = l.trim();
        if t.starts_with(';') || t.starts_with("//") { continue; }
        for tok in t.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            let tok = tok.trim_matches(|c: char| c == '"' || c == '\'');
            let h = tok.trim_start_matches('#').trim_start_matches("0x");
            // Paint.NET: AARRGGBB
            let h = if h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()) { &h[2..] } else { h };
            if (h.len() == 6 || (h.len() == 3 && tok.starts_with('#'))) && h.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Some(c) = util::parse_hex(h) { p.colors.push(c); p.names.push(String::new()); }
            }
        }
    }
    if p.colors.is_empty() { return Err("Не удалось найти цвета в файле".into()); }
    Ok(p)
}

pub fn hex_write(p: &Palette) -> Vec<u8> {
    p.colors.iter().map(|c| util::hex(*c)[1..].to_uppercase()).collect::<Vec<_>>().join("\n").into_bytes()
}

/* ---------- Procreate .swatches ---------- */

fn swatches_read(d: &[u8], name: &str) -> Result<Vec<Palette>, String> {
    let z = crate::formats::zip_read(d)?;
    let j = z.iter().find(|(k, _)| k.ends_with(".json")).map(|(_, v)| v).ok_or("В архиве нет Swatches.json")?;
    let v: serde_json::Value = serde_json::from_slice(j).map_err(|e| e.to_string())?;
    let arr = if v.is_array() { v.as_array().cloned().unwrap_or_default() } else { vec![v] };
    let mut out = vec![];
    for pal in arr {
        let mut p = Palette { name: pal.get("name").and_then(|n| n.as_str()).unwrap_or(name).to_string(), ..Default::default() };
        for s in pal.get("swatches").and_then(|s| s.as_array()).cloned().unwrap_or_default() {
            if s.is_null() { continue; }
            let g = |k: &str| s.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            p.colors.push(util::hsv2rgb(g("hue") * 360.0, g("saturation"), g("brightness")));
            p.names.push(String::new());
        }
        out.push(p);
    }
    Ok(out)
}

pub fn swatches_write(p: &Palette) -> Vec<u8> {
    let sw: Vec<serde_json::Value> = p.colors.iter().map(|c| {
        let (h, s, v) = util::rgb2hsv(*c);
        serde_json::json!({ "hue": h / 360.0, "saturation": s, "brightness": v, "alpha": 1, "colorSpace": 0 })
    }).collect();
    let j = serde_json::json!([{ "name": p.name, "swatches": sw }]);
    crate::formats::zip_write(&[("Swatches.json".into(), serde_json::to_vec(&j).unwrap_or_default())])
}

/// Экспорт палитры в формат по расширению.
pub fn export(p: &Palette, kind: &str) -> (String, &'static str, Vec<u8>) {
    let base = crate::app::safe_name(&p.name);
    match kind {
        "aco" => (format!("{base}.aco"), "application/octet-stream", aco_write(p)),
        "ase" => (format!("{base}.ase"), "application/octet-stream", ase_write(p)),
        "swatches" => (format!("{base}.swatches"), "application/zip", swatches_write(p)),
        "hex" => (format!("{base}.hex"), "text/plain", hex_write(p)),
        _ => (format!("{base}.gpl"), "text/plain", gpl_write(p)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Palette {
        Palette { name: "Тест".into(), colors: vec![[255, 0, 0], [0, 128, 255], [12, 34, 56]], names: vec!["Красный".into(), "Голубой".into(), String::new()] }
    }

    #[test]
    fn roundtrips() {
        let p = sample();
        for kind in ["aco", "ase", "gpl", "swatches", "hex"] {
            let (file, _, bytes) = export(&p, kind);
            let back = import(&bytes, &file).unwrap_or_else(|e| panic!("{kind}: {e}"));
            let q = back.iter().find(|q| !q.colors.is_empty()).unwrap();
            assert_eq!(q.colors.len(), 3, "{kind}");
            for (a, b) in q.colors.iter().zip(p.colors.iter()) {
                for k in 0..3 { assert!((a[k] as i32 - b[k] as i32).abs() <= 2, "{kind}: {a:?} vs {b:?}"); }
            }
            if kind == "aco" || kind == "ase" || kind == "gpl" { assert_eq!(q.names[0], "Красный", "{kind}"); }
        }
    }

    #[test]
    fn text_lists() {
        let p = import(b"#ff0000\n00ff00, 0000FF\nFF112233", "x.txt").unwrap();
        assert_eq!(p[0].colors, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [0x11, 0x22, 0x33]]);
        let j = import(b"JASC-PAL\n0100\n2\n255 0 0\n0 0 0\n", "a.pal").unwrap();
        assert_eq!(j[0].colors.len(), 2);
    }
}
