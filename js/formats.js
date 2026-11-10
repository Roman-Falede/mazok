'use strict';
/* Мазок — форматы файлов: ZIP, PSD (чтение/запись), GIF, видео */

class BinWriter {
  constructor() { this.parts = []; this.length = 0; this.buf = new DataView(new ArrayBuffer(8)); }
  push(u8) { this.parts.push(u8); this.length += u8.length; }
  num(fn, n, v) { const b = new DataView(new ArrayBuffer(n)); b[fn](0, v, false); this.push(new Uint8Array(b.buffer)); }
  u8(v) { this.push(new Uint8Array([v & 255])); }
  u16(v) { this.num('setUint16', 2, v); } i16(v) { this.num('setInt16', 2, v); }
  u32(v) { this.num('setUint32', 4, v >>> 0); } i32(v) { this.num('setInt32', 4, v | 0); }
  str(s) { this.push(new Uint8Array(Array.from(s, c => c.charCodeAt(0) & 255))); }
  bytes(u8) { this.push(u8); }
  zeros(n) { if (n > 0) this.push(new Uint8Array(n)); }
  append(w) { for (const p of w.parts) this.push(p); }
  blob(type) { return new Blob(this.parts, { type }); }
}

const Zip = {
  table: null,
  crc(d) {
    if (!this.table) { this.table = new Uint32Array(256); for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1; this.table[n] = c >>> 0; } }
    let c = 0xFFFFFFFF; for (let i = 0; i < d.length; i++) c = this.table[(c ^ d[i]) & 255] ^ (c >>> 8);
    return (c ^ 0xFFFFFFFF) >>> 0;
  },
  async make(files) {
    const enc = new TextEncoder(), parts = [], central = [], now = new Date();
    const time = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1);
    const date = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate();
    let off = 0, cdLen = 0;
    for (const f of files) {
      let data = f.data;
      if (typeof data === 'string') data = enc.encode(data); else if (data instanceof Blob) data = new Uint8Array(await data.arrayBuffer());
      const name = enc.encode(f.name), crc = this.crc(data);
      const lh = new DataView(new ArrayBuffer(30));
      lh.setUint32(0, 0x04034b50, true); lh.setUint16(4, 20, true); lh.setUint16(6, 0x0800, true); lh.setUint16(8, 0, true);
      lh.setUint16(10, time, true); lh.setUint16(12, date, true); lh.setUint32(14, crc, true); lh.setUint32(18, data.length, true); lh.setUint32(22, data.length, true);
      lh.setUint16(26, name.length, true); lh.setUint16(28, 0, true);
      parts.push(lh.buffer, name, data);
      const ch = new DataView(new ArrayBuffer(46));
      ch.setUint32(0, 0x02014b50, true); ch.setUint16(4, 20, true); ch.setUint16(6, 20, true); ch.setUint16(8, 0x0800, true); ch.setUint16(10, 0, true);
      ch.setUint16(12, time, true); ch.setUint16(14, date, true); ch.setUint32(16, crc, true); ch.setUint32(20, data.length, true); ch.setUint32(24, data.length, true);
      ch.setUint16(28, name.length, true); ch.setUint32(42, off, true);
      central.push(ch.buffer, name); cdLen += 46 + name.length;
      off += 30 + name.length + data.length;
    }
    const end = new DataView(new ArrayBuffer(22));
    end.setUint32(0, 0x06054b50, true); end.setUint16(8, files.length, true); end.setUint16(10, files.length, true); end.setUint32(12, cdLen, true); end.setUint32(16, off, true);
    return new Blob([...parts, ...central, end.buffer], { type: 'application/zip' });
  },
  async read(blob) {
    const buf = new Uint8Array(await blob.arrayBuffer()), v = new DataView(buf.buffer), dec = new TextDecoder();
    let e = -1; for (let i = buf.length - 22; i >= Math.max(0, buf.length - 70000); i--) if (v.getUint32(i, true) === 0x06054b50) { e = i; break; }
    if (e < 0) throw new Error('Это не ZIP-архив');
    const n = v.getUint16(e + 10, true); let p = v.getUint32(e + 16, true);
    const out = new Map();
    for (let k = 0; k < n; k++) {
      if (v.getUint32(p, true) !== 0x02014b50) break;
      const method = v.getUint16(p + 10, true), csize = v.getUint32(p + 20, true), nl = v.getUint16(p + 28, true), el = v.getUint16(p + 30, true), cl = v.getUint16(p + 32, true), lo = v.getUint32(p + 42, true);
      const name = dec.decode(buf.subarray(p + 46, p + 46 + nl));
      const ds = lo + 30 + v.getUint16(lo + 26, true) + v.getUint16(lo + 28, true);
      let data = buf.subarray(ds, ds + csize);
      if (method === 8) data = new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer());
      out.set(name, data);
      p += 46 + nl + el + cl;
    }
    return out;
  }
};

const PSD = {
  keys: {
    'source-over': 'norm', dissolve: 'diss', multiply: 'mul ', screen: 'scrn', overlay: 'over', darken: 'dark', lighten: 'lite', 'color-dodge': 'div ', 'color-burn': 'idiv',
    'linear-burn': 'lbrn', 'darker-color': 'dkCl', lighter: 'lddg', 'lighter-color': 'lgCl', 'hard-light': 'hLit', 'soft-light': 'sLit', 'vivid-light': 'vLit', 'linear-light': 'lLit',
    'pin-light': 'pLit', 'hard-mix': 'hMix', difference: 'diff', exclusion: 'smud', subtract: 'fsub', divide: 'fdiv', hue: 'hue ', saturation: 'sat ', color: 'colr', luminosity: 'lum '
  },
  write(doc) {
    const w = doc.w, h = doc.h, W = new BinWriter();
    W.str('8BPS'); W.u16(1); W.zeros(6); W.u16(3); W.u32(h); W.u32(w); W.u16(8); W.u16(3);
    W.u32(0); W.u32(0);
    const LI = new BinWriter(), planesAll = [];
    const layers = doc.layers.slice();
    LI.i16(layers.length);
    for (const L of layers) {
      const b = U.contentBounds(L.canvas) || { x: 0, y: 0, w: 0, h: 0 };
      LI.i32(b.y); LI.i32(b.x); LI.i32(b.y + b.h); LI.i32(b.x + b.w); LI.u16(4);
      let planes = null;
      if (b.w && b.h) {
        const d = U.readPixels(L.canvas, b).data, n = b.w * b.h;
        planes = [new Uint8Array(n), new Uint8Array(n), new Uint8Array(n), new Uint8Array(n)];
        for (let i = 0; i < n; i++) { planes[0][i] = d[i * 4 + 3]; planes[1][i] = d[i * 4]; planes[2][i] = d[i * 4 + 1]; planes[3][i] = d[i * 4 + 2]; }
      }
      [-1, 0, 1, 2].forEach((id, c) => { LI.i16(id); LI.u32(2 + (planes ? planes[c].length : 0)); });
      LI.str('8BIM'); LI.str(this.keys[L.blend] || 'norm'); LI.u8(Math.round(L.opacity * 255)); LI.u8(L.clip ? 1 : 0);
      LI.u8((L.alphaLock ? 1 : 0) | (L.visible ? 0 : 2)); LI.u8(0);
      const ex = new BinWriter();
      ex.u32(0); ex.u32(0);
      const nm = Array.from(L.name).map(c => c.charCodeAt(0) < 128 ? c : '_').join('').slice(0, 255);
      ex.u8(nm.length); ex.str(nm); ex.zeros((4 - ((1 + nm.length) % 4)) % 4);
      const dl = 4 + L.name.length * 2, pl = dl + (dl % 4 ? 4 - dl % 4 : 0);
      ex.str('8BIM'); ex.str('luni'); ex.u32(pl); ex.u32(L.name.length); for (const ch of L.name) ex.u16(ch.charCodeAt(0)); ex.zeros(pl - dl);
      LI.u32(ex.length); LI.append(ex);
      planesAll.push(planes);
    }
    for (const planes of planesAll) for (let c = 0; c < 4; c++) { LI.u16(0); if (planes) LI.bytes(planes[c]); }
    if (LI.length % 2) LI.u8(0);
    W.u32(4 + LI.length + 4); W.u32(LI.length); W.append(LI); W.u32(0);
    const d = U.readPixels(Engine.flatten()).data, n = w * h;
    W.u16(0);
    for (let c = 0; c < 3; c++) { const p = new Uint8Array(n); for (let i = 0; i < n; i++) p[i] = d[i * 4 + c]; W.bytes(p); }
    return W.blob('image/vnd.adobe.photoshop');
  },
  async read(blob) {
    const buf = await blob.arrayBuffer(), v = new DataView(buf), u = new Uint8Array(buf);
    let o = 0;
    const u8 = () => v.getUint8(o++), u16 = () => { const x = v.getUint16(o); o += 2; return x; }, i16 = () => { const x = v.getInt16(o); o += 2; return x; };
    const u32 = () => { const x = v.getUint32(o); o += 4; return x; }, i32 = () => { const x = v.getInt32(o); o += 4; return x; };
    const str = n => { let s = ''; for (let i = 0; i < n; i++) s += String.fromCharCode(u[o + i]); o += n; return s; };
    const skip = () => { const n = u32(); o += n; };
    if (str(4) !== '8BPS') throw new Error('Это не PSD-файл');
    if (u16() !== 1) throw new Error('Формат PSB не поддерживается');
    o += 6; const nch = u16(), h = u32(), w = u32(), depth = u16(), mode = u16();
    if (depth !== 8 || mode !== 3) throw new Error('Поддерживаются только 8-битные RGB-файлы PSD');
    skip(); skip();
    const unpack = (start, end, len) => {
      const out = new Uint8Array(len); let p = start, q = 0;
      while (q < len && p < end) { let n = v.getInt8(p++); if (n >= 0) { for (let i = 0; i <= n && q < len; i++) out[q++] = u[p++]; } else if (n !== -128) { const b = u[p++]; for (let i = 0; i < 1 - n && q < len; i++) out[q++] = b; } }
      return out;
    };
    const chan = (start, end, cw, ch) => {
      const comp = v.getUint16(start), n = cw * ch;
      if (comp === 0) return u.slice(start + 2, start + 2 + n);
      if (comp === 1) { let p = start + 2 + ch * 2, out = new Uint8Array(n); for (let y = 0; y < ch; y++) { const bc = v.getUint16(start + 2 + y * 2); out.set(unpack(p, p + bc, cw), y * cw); p += bc; } return out; }
      return null;
    };
    const rev = Object.fromEntries(Object.entries(this.keys).map(([a, b]) => [b, a]));
    const lmLen = u32(), lmEnd = o + lmLen, layers = [];
    if (lmLen > 0) {
      const liLen = u32();
      if (liLen > 0) {
        const count = Math.abs(i16()), recs = [];
        for (let i = 0; i < count; i++) {
          const top = i32(), left = i32(), bottom = i32(), right = i32(), nc = u16(), chans = [];
          for (let c = 0; c < nc; c++) chans.push({ id: i16(), len: u32() });
          str(4); const key = str(4), opacity = u8(), clip = u8(), flags = u8(); o++;
          const exLen = u32(), exEnd = o + exLen;
          skip(); skip();
          const nl = u8(); let name = str(nl); o += (4 - ((1 + nl) % 4)) % 4;
          let divider = false;
          while (o + 12 <= exEnd) {
            const sig = str(4); if (sig !== '8BIM' && sig !== '8B64') break;
            const k = str(4), l = u32(), ds = o;
            if (k === 'luni') { const n = u32(); let t = ''; for (let j = 0; j < n; j++) t += String.fromCharCode(u16()); name = t.replace(/\0+$/, ''); }
            if (k === 'lsct' || k === 'lsdk') { const tp = u32(); if (tp >= 1 && tp <= 3) divider = true; }
            o = ds + l;
          }
          o = exEnd;
          recs.push({ top, left, bottom, right, chans, key, opacity, clip, flags, name, divider });
        }
        for (const r of recs) {
          const cw = r.right - r.left, ch = r.bottom - r.top; r.data = {};
          for (const c of r.chans) { const s = o; if (cw > 0 && ch > 0 && c.id >= -1 && c.id <= 2 && c.len >= 2) r.data[c.id] = chan(s, s + c.len, cw, ch); o = s + c.len; }
          if (r.divider) continue;
          const L = { name: r.name || 'Слой', opacity: r.opacity / 255, blend: rev[r.key] || 'source-over', clip: r.clip === 1, visible: !(r.flags & 2), alphaLock: !!(r.flags & 1), canvas: U.canvas(w, h) };
          if (cw > 0 && ch > 0) {
            const im = new ImageData(cw, ch), d = im.data, R = r.data[0], G = r.data[1], B = r.data[2], A = r.data[-1];
            for (let i = 0; i < cw * ch; i++) { d[i * 4] = R ? R[i] : 0; d[i * 4 + 1] = G ? G[i] : 0; d[i * 4 + 2] = B ? B[i] : 0; d[i * 4 + 3] = A ? A[i] : 255; }
            const t = U.fromImageData(im); L.canvas.getContext('2d').drawImage(t, r.left, r.top);
          }
          layers.push(L);
        }
      }
      o = lmEnd;
    }
    if (!layers.length) {
      const comp = u16(), n = w * h, planes = [];
      if (comp === 0) for (let c = 0; c < nch; c++) { planes.push(u.subarray(o, o + n)); o += n; }
      else { const counts = []; for (let i = 0; i < nch * h; i++) counts.push(u16()); let k = 0; for (let c = 0; c < nch; c++) { const pl = new Uint8Array(n); for (let y = 0; y < h; y++) { pl.set(unpack(o, o + counts[k], w), y * w); o += counts[k++]; } planes.push(pl); } }
      const im = new ImageData(w, h), d = im.data;
      for (let i = 0; i < n; i++) { d[i * 4] = planes[0][i]; d[i * 4 + 1] = planes[1][i]; d[i * 4 + 2] = planes[2][i]; d[i * 4 + 3] = nch > 3 ? planes[3][i] : 255; }
      layers.push({ name: 'Фон', opacity: 1, blend: 'source-over', clip: false, visible: true, alphaLock: false, canvas: U.fromImageData(im) });
    }
    return { w, h, layers };
  }
};

const GIF = {
  encode(frames, w, h, delayCs, transparent) {
    const counts = new Map();
    for (const f of frames) {
      const d = f.data, step = Math.max(1, Math.floor(d.length / 4 / 60000));
      for (let i = 0; i < d.length; i += 4 * step) { if (transparent && d[i + 3] < 128) continue; const k = ((d[i] >> 3) << 10) | ((d[i + 1] >> 3) << 5) | (d[i + 2] >> 3); counts.set(k, (counts.get(k) || 0) + 1); }
    }
    const max = transparent ? 255 : 256;
    let pal = Array.from(counts.entries()).sort((a, b) => b[1] - a[1]).slice(0, max).map(([k]) => [((k >> 10) & 31) * 8 + 4, ((k >> 5) & 31) * 8 + 4, (k & 31) * 8 + 4]);
    if (!pal.length) pal = [[0, 0, 0]];
    const off = transparent ? 1 : 0, full = new Uint8Array(768);
    pal.forEach((c, i) => full.set(c, (i + off) * 3));
    const map = new Uint8Array(32768);
    for (let k = 0; k < 32768; k++) {
      const r = ((k >> 10) & 31) * 8 + 4, g = ((k >> 5) & 31) * 8 + 4, b = (k & 31) * 8 + 4;
      let bi = 0, bd = Infinity;
      for (let i = 0; i < pal.length; i++) { const p = pal[i], dd = (p[0] - r) ** 2 * 3 + (p[1] - g) ** 2 * 4 + (p[2] - b) ** 2 * 2; if (dd < bd) { bd = dd; bi = i; } }
      map[k] = bi + off;
    }
    const out = [];
    const B = (...a) => out.push(...a), W16 = v => out.push(v & 255, (v >> 8) & 255);
    'GIF89a'.split('').forEach(c => B(c.charCodeAt(0)));
    W16(w); W16(h); B(0xF7, 0, 0);
    for (let i = 0; i < 768; i++) out.push(full[i]);
    B(0x21, 0xFF, 11); 'NETSCAPE2.0'.split('').forEach(c => B(c.charCodeAt(0))); B(3, 1, 0, 0, 0);
    const chunks = [new Uint8Array(out)];
    for (const f of frames) {
      const d = f.data, n = w * h, idx = new Uint8Array(n);
      for (let i = 0; i < n; i++) { const j = i * 4; idx[i] = transparent && d[j + 3] < 128 ? 0 : map[((d[j] >> 3) << 10) | ((d[j + 1] >> 3) << 5) | (d[j + 2] >> 3)]; }
      const hdr = [0x21, 0xF9, 4, transparent ? 0x09 : 0x04, delayCs & 255, (delayCs >> 8) & 255, 0, 0, 0x2C, 0, 0, 0, 0, w & 255, w >> 8, h & 255, h >> 8, 0, 8];
      chunks.push(new Uint8Array(hdr), this.lzw(idx, 8));
    }
    chunks.push(new Uint8Array([0x3B]));
    return new Blob(chunks, { type: 'image/gif' });
  },
  lzw(idx, min) {
    const clear = 1 << min, eoi = clear + 1, bytes = [];
    let size = min + 1, next = eoi + 1, cur = 0, bits = 0, dict = new Map();
    const emit = c => { cur |= c << bits; bits += size; while (bits >= 8) { bytes.push(cur & 255); cur >>>= 8; bits -= 8; } };
    emit(clear);
    let prefix = idx[0];
    for (let i = 1; i < idx.length; i++) {
      const k = idx[i], key = prefix * 256 + k, f = dict.get(key);
      if (f !== undefined) { prefix = f; continue; }
      emit(prefix);
      if (next < 4096) { dict.set(key, next++); if (next > (1 << size) && size < 12) size++; }
      else { emit(clear); dict = new Map(); size = min + 1; next = eoi + 1; }
      prefix = k;
    }
    emit(prefix); emit(eoi);
    if (bits > 0) bytes.push(cur & 255);
    const out = new Uint8Array(bytes.length + Math.ceil(bytes.length / 255) + 1); let o = 0;
    for (let i = 0; i < bytes.length; i += 255) { const n = Math.min(255, bytes.length - i); out[o++] = n; for (let j = 0; j < n; j++) out[o++] = bytes[i + j]; }
    out[o++] = 0;
    return out.subarray(0, o);
  }
};

const VideoRec = {
  mime() {
    if (!window.MediaRecorder) return null;
    for (const m of ['video/mp4;codecs=avc1.42E01E', 'video/mp4', 'video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm']) if (MediaRecorder.isTypeSupported(m)) return m;
    return null;
  },
  async record(w, h, count, fps, drawFrame, onProgress) {
    const mime = this.mime(); if (!mime) throw new Error('Запись видео не поддерживается этим браузером');
    w = w & ~1; h = h & ~1;
    const c = U.canvas(w, h), x = c.getContext('2d');
    x.fillStyle = '#fff'; x.fillRect(0, 0, w, h);
    const stream = c.captureStream(0), track = stream.getVideoTracks()[0];
    const rec = new MediaRecorder(stream, { mimeType: mime, videoBitsPerSecond: 10e6 }), chunks = [];
    rec.ondataavailable = e => { if (e.data && e.data.size) chunks.push(e.data); };
    const done = new Promise(r => { rec.onstop = r; });
    rec.start(250);
    const dt = 1000 / fps; let t0 = performance.now();
    for (let i = 0; i < count; i++) {
      await drawFrame(x, i, w, h);
      if (track.requestFrame) track.requestFrame();
      onProgress && onProgress(i / count);
      const wait = t0 + (i + 1) * dt - performance.now(); if (wait > 0) await U.sleep(wait);
    }
    await U.sleep(dt * 2); rec.stop(); await done;
    return { blob: new Blob(chunks, { type: mime.split(';')[0] }), ext: mime.includes('mp4') ? 'mp4' : 'webm' };
  }
};
