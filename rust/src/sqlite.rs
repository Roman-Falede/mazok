//! Минимальное чтение базы SQLite (только чтение таблиц) — для кистей Clip Studio Paint (.sut).

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Val {
    pub fn f(&self) -> Option<f64> {
        match self {
            Val::Int(i) => Some(*i as f64),
            Val::Real(r) => Some(*r),
            Val::Text(t) => t.trim().parse().ok(),
            _ => None,
        }
    }
    pub fn text(&self) -> Option<String> {
        match self {
            Val::Text(t) => Some(t.clone()),
            Val::Int(i) => Some(i.to_string()),
            Val::Real(r) => Some(r.to_string()),
            Val::Blob(b) => Some(String::from_utf8_lossy(b).to_string()),
            Val::Null => None,
        }
    }
    pub fn blob(&self) -> Option<&[u8]> {
        match self {
            Val::Blob(b) => Some(b),
            _ => None,
        }
    }
}

pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Val>>,
}

impl Table {
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.eq_ignore_ascii_case(name))
    }
    pub fn get<'a>(&self, row: &'a [Val], name: &str) -> Option<&'a Val> {
        self.col(name).and_then(|i| row.get(i)).filter(|v| **v != Val::Null)
    }
}

pub struct Db<'a> {
    d: &'a [u8],
    page: usize,
    usable: usize,
    utf16: u32,
    pub schema: HashMap<String, (u32, String)>,
}

fn varint(d: &[u8], mut o: usize) -> Option<(u64, usize)> {
    let mut v: u64 = 0;
    for i in 0..9 {
        let b = *d.get(o)?;
        o += 1;
        if i == 8 {
            v = (v << 8) | b as u64;
            return Some((v, o));
        }
        v = (v << 7) | (b & 0x7f) as u64;
        if b & 0x80 == 0 {
            return Some((v, o));
        }
    }
    Some((v, o))
}

fn be(d: &[u8], o: usize, n: usize) -> Option<u64> {
    let s = d.get(o..o + n)?;
    Some(s.iter().fold(0u64, |a, b| (a << 8) | *b as u64))
}

impl<'a> Db<'a> {
    pub fn open(d: &'a [u8]) -> Result<Db<'a>, String> {
        if d.len() < 100 || &d[..16] != b"SQLite format 3\0" {
            return Err("Файл не является базой SQLite".into());
        }
        let ps = be(d, 16, 2).unwrap_or(4096) as usize;
        let page = if ps == 1 { 65536 } else { ps };
        if page < 512 { return Err("Неверный размер страницы SQLite".into()); }
        let reserved = d[20] as usize;
        let utf16 = be(d, 56, 4).unwrap_or(1) as u32;
        let mut db = Db { d, page, usable: page - reserved, utf16, schema: HashMap::new() };
        let mut rows = vec![];
        db.walk(1, &mut rows, 0)?;
        for (_, rec) in rows {
            let v = db.record(&rec);
            if v.len() >= 5 && v[0].text().as_deref() == Some("table") {
                if let (Some(name), Some(root), Some(sql)) = (v[1].text(), v[3].f(), v[4].text()) {
                    db.schema.insert(name.to_lowercase(), (root as u32, sql));
                }
            }
        }
        Ok(db)
    }

    fn page_slice(&self, n: u32) -> Option<&'a [u8]> {
        let s = (n as usize).checked_sub(1)? * self.page;
        self.d.get(s..s + self.page)
    }

    /// Обход B-дерева таблицы: (rowid, полезная нагрузка записи).
    fn walk(&self, pgno: u32, out: &mut Vec<(i64, Vec<u8>)>, depth: usize) -> Result<(), String> {
        if depth > 40 { return Err("Слишком глубокое дерево SQLite".into()); }
        let pg = self.page_slice(pgno).ok_or("Страница SQLite за пределами файла")?;
        let h = if pgno == 1 { 100 } else { 0 };
        let kind = pg[h];
        let ncell = be(pg, h + 3, 2).unwrap_or(0) as usize;
        match kind {
            0x0d => {
                for i in 0..ncell {
                    let cp = be(pg, h + 8 + i * 2, 2).ok_or("Ошибка ячейки")? as usize;
                    let (plen, o) = varint(pg, cp).ok_or("Ошибка ячейки")?;
                    let (rowid, o) = varint(pg, o).ok_or("Ошибка ячейки")?;
                    let payload = self.payload(pg, o, plen as usize)?;
                    out.push((rowid as i64, payload));
                }
            }
            0x05 => {
                for i in 0..ncell {
                    let cp = be(pg, h + 12 + i * 2, 2).ok_or("Ошибка ячейки")? as usize;
                    let child = be(pg, cp, 4).ok_or("Ошибка ячейки")? as u32;
                    self.walk(child, out, depth + 1)?;
                }
                let right = be(pg, h + 8, 4).ok_or("Ошибка страницы")? as u32;
                self.walk(right, out, depth + 1)?;
            }
            _ => return Err(format!("Неожиданный тип страницы SQLite: {kind:#x}")),
        }
        Ok(())
    }

    fn payload(&self, pg: &[u8], o: usize, p: usize) -> Result<Vec<u8>, String> {
        let u = self.usable;
        let x = u - 35;
        let local = if p <= x {
            p
        } else {
            let m = ((u - 12) * 32 / 255) - 23;
            let k = m + (p - m) % (u - 4);
            if k <= x { k } else { m }
        };
        let mut v = pg.get(o..o + local).ok_or("Ошибка записи SQLite")?.to_vec();
        if local < p {
            let mut next = be(pg, o + local, 4).ok_or("Ошибка переполнения")? as u32;
            let mut guard = 0;
            while next != 0 && v.len() < p && guard < 1_000_000 {
                guard += 1;
                let op = self.page_slice(next).ok_or("Ошибка страницы переполнения")?;
                next = be(op, 0, 4).unwrap_or(0) as u32;
                let take = (p - v.len()).min(u - 4);
                v.extend_from_slice(&op[4..4 + take]);
            }
        }
        Ok(v)
    }

    fn record(&self, r: &[u8]) -> Vec<Val> {
        let Some((hlen, mut o)) = varint(r, 0) else { return vec![] };
        let mut types = vec![];
        while o < hlen as usize {
            let Some((t, no)) = varint(r, o) else { break };
            types.push(t);
            o = no;
        }
        let mut p = hlen as usize;
        let mut out = vec![];
        for t in types {
            let (val, n) = match t {
                0 => (Val::Null, 0),
                1..=6 => {
                    let n = [0, 1, 2, 3, 4, 6, 8][t as usize];
                    let raw = be(r, p, n).unwrap_or(0);
                    let shift = 64 - n * 8;
                    (Val::Int(((raw << shift) as i64) >> shift), n)
                }
                7 => (Val::Real(f64::from_bits(be(r, p, 8).unwrap_or(0))), 8),
                8 => (Val::Int(0), 0),
                9 => (Val::Int(1), 0),
                t if t >= 12 && t % 2 == 0 => {
                    let n = ((t - 12) / 2) as usize;
                    (Val::Blob(r.get(p..p + n).unwrap_or(&[]).to_vec()), n)
                }
                t if t >= 13 => {
                    let n = ((t - 13) / 2) as usize;
                    let b = r.get(p..p + n).unwrap_or(&[]);
                    let s = if self.utf16 == 1 {
                        String::from_utf8_lossy(b).to_string()
                    } else {
                        let le = self.utf16 == 2;
                        let units: Vec<u16> = b.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
                        String::from_utf16_lossy(&units)
                    };
                    (Val::Text(s), n)
                }
                _ => (Val::Null, 0),
            };
            out.push(val);
            p += n;
        }
        out
    }

    pub fn has(&self, table: &str) -> bool {
        self.schema.contains_key(&table.to_lowercase())
    }

    /// Прочитать всю таблицу.
    pub fn table(&self, name: &str) -> Result<Table, String> {
        let (root, sql) = self.schema.get(&name.to_lowercase()).ok_or_else(|| format!("Нет таблицы {name}"))?;
        let (columns, ipk) = parse_columns(sql);
        let mut raw = vec![];
        self.walk(*root, &mut raw, 0)?;
        let rows = raw.into_iter().map(|(rowid, rec)| {
            let mut v = self.record(&rec);
            v.resize(columns.len().max(v.len()), Val::Null);
            if let Some(i) = ipk {
                if v[i] == Val::Null { v[i] = Val::Int(rowid); }
            }
            v
        }).collect();
        Ok(Table { columns, rows })
    }
}

/// Имена столбцов из CREATE TABLE и индекс столбца INTEGER PRIMARY KEY (псевдоним rowid).
fn parse_columns(sql: &str) -> (Vec<String>, Option<usize>) {
    let Some(a) = sql.find('(') else { return (vec![], None) };
    let b = sql.rfind(')').unwrap_or(sql.len());
    let body = &sql[a + 1..b.max(a + 1)];
    let mut parts = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    for ch in body.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 => { parts.push(std::mem::take(&mut cur)); }
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    let mut cols = vec![];
    let mut ipk = None;
    for p in parts {
        let t = p.trim();
        let up = t.to_uppercase();
        if t.is_empty() || ["PRIMARY", "UNIQUE", "CHECK", "FOREIGN", "CONSTRAINT"].iter().any(|k| up.starts_with(k)) { continue; }
        let name: String = if let Some(q) = t.strip_prefix('"') { q.split('"').next().unwrap_or("").to_string() }
            else if let Some(q) = t.strip_prefix('`') { q.split('`').next().unwrap_or("").to_string() }
            else if let Some(q) = t.strip_prefix('[') { q.split(']').next().unwrap_or("").to_string() }
            else { t.split_whitespace().next().unwrap_or("").to_string() };
        if up.contains("INTEGER") && up.contains("PRIMARY KEY") { ipk = Some(cols.len()); }
        cols.push(name);
    }
    (cols, ipk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns() {
        let (c, ipk) = parse_columns("CREATE TABLE Node(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, NodeName TEXT DEFAULT '', NodeVariantID INTEGER, PRIMARY KEY(x))");
        assert_eq!(c, vec!["_PW_ID", "NodeName", "NodeVariantID"]);
        assert_eq!(ipk, Some(0));
    }

    #[test]
    fn varints() {
        assert_eq!(varint(&[0x7f], 0), Some((127, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
    }
}
