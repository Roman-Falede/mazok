'use strict';
/* Мазок — движок кистей: формы, зерно, библиотека, штрих, быстрые фигуры */

/* ---------- шум ---------- */
const Noise = {
  hash(ix, iy, seed) { let h = (ix * 374761393 + iy * 668265263 + seed * 2147483647) | 0; h = Math.imul(h ^ (h >>> 13), 1274126177); return ((h ^ (h >>> 16)) >>> 0) / 4294967296; },
  value(u, v, seed = 0) {
    const ix = Math.floor(u), iy = Math.floor(v), fx = U.smooth(u - ix), fy = U.smooth(v - iy);
    const a = this.hash(ix, iy, seed), b = this.hash(ix + 1, iy, seed), c = this.hash(ix, iy + 1, seed), d = this.hash(ix + 1, iy + 1, seed);
    return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy;
  },
  /* бесшовная сетка значений size×size */
  tile(size, cx, cy, rng) {
    const g = new Float32Array(cx * cy); for (let i = 0; i < g.length; i++) g[i] = rng();
    const out = new Float32Array(size * size);
    for (let y = 0; y < size; y++) {
      const fy = y / size * cy, iy = Math.floor(fy), ty = U.smooth(fy - iy), y0 = iy % cy, y1 = (iy + 1) % cy;
      for (let x = 0; x < size; x++) {
        const fx = x / size * cx, ix = Math.floor(fx), tx = U.smooth(fx - ix), x0 = ix % cx, x1 = (ix + 1) % cx;
        const a = g[y0 * cx + x0], b = g[y0 * cx + x1], c = g[y1 * cx + x0], d = g[y1 * cx + x1];
        const top = a + (b - a) * tx, bot = c + (d - c) * tx;
        out[y * size + x] = top + (bot - top) * ty;
      }
    }
    return out;
  },
  fbm(size, octs, rng, sx = 1, sy = 1) {
    const out = new Float32Array(size * size); let amp = 1, tot = 0;
    for (const c of octs) {
      const n = this.tile(size, Math.max(1, Math.round(c * sx)), Math.max(1, Math.round(c * sy)), rng);
      for (let i = 0; i < out.length; i++) out[i] += n[i] * amp;
      tot += amp; amp *= 0.55;
    }
    for (let i = 0; i < out.length; i++) out[i] /= tot;
    return out;
  }
};

/* ---------- зерно (текстура штриха) ---------- */
const Grains = (() => {
  const S = 256, cache = new Map(), adj = new Map(), custom = new Map();
  const list = [
    { id: 'paper', name: 'Бумага' }, { id: 'canvas', name: 'Холст' }, { id: 'noise', name: 'Шум' },
    { id: 'charcoal', name: 'Уголь' }, { id: 'sponge', name: 'Губка' }, { id: 'grit', name: 'Крупка' }, { id: 'hatch', name: 'Штриховка' }
  ];
  const C = v => U.clamp(v, 0, 1);
  const gens = {
    paper: r => Noise.fbm(S, [16, 32, 64, 128], r).map(v => C((v - 0.5) * 2.6 + 0.6)),
    canvas: r => {
      const n = Noise.fbm(S, [64, 128], r), o = new Float32Array(S * S), k = 2 * Math.PI * 32 / S;
      for (let y = 0; y < S; y++) for (let x = 0; x < S; x++) {
        const w = 0.5 + 0.25 * Math.sin(x * k) * (y % 16 < 8 ? 1 : -1) + 0.25 * Math.sin(y * k);
        o[y * S + x] = C(w * 0.7 + (n[y * S + x] - 0.5) * 0.8 + 0.25);
      }
      return o;
    },
    noise: r => { const o = new Float32Array(S * S); for (let i = 0; i < o.length; i++) o[i] = Math.pow(r(), 0.6); return o; },
    charcoal: r => {
      const a = Noise.fbm(S, [4, 8, 16, 32], r, 0.25, 1), o = new Float32Array(S * S);
      for (let i = 0; i < o.length; i++) o[i] = C((a[i] - 0.45) * 3 + (r() - 0.5) * 0.5 + 0.5);
      return o;
    },
    sponge: r => Noise.fbm(S, [6, 12, 24], r).map(v => U.smooth((v - 0.38) * 3.2)),
    grit: r => { const n = Noise.fbm(S, [32, 64], r); return n.map(v => (v + (r() - 0.5) * 0.35) > 0.52 ? 1 : 0.15); },
    hatch: r => { const o = new Float32Array(S * S); for (let y = 0; y < S; y++) for (let x = 0; x < S; x++) { const t = ((x + y) % 16) / 16; o[y * S + x] = t < 0.45 ? 1 : 0.1 + r() * 0.1; } return o; }
  };
  function get(id) {
    if (custom.has(id)) return custom.get(id);
    let c = cache.get(id);
    if (c) return c;
    const vals = (gens[id] || gens.paper)(U.rng(777 + id.length * 13));
    c = U.canvas(S, S);
    const x = c.getContext('2d'), im = x.createImageData(S, S);
    for (let i = 0; i < vals.length; i++) { im.data[i * 4] = im.data[i * 4 + 1] = im.data[i * 4 + 2] = 255; im.data[i * 4 + 3] = vals[i] * 255; }
    x.putImageData(im, 0, 0); cache.set(id, c);
    return c;
  }
  function strengthened(id, strength) {
    const key = id + '|' + strength.toFixed(2);
    let c = adj.get(key); if (c) return c;
    const g = get(id), w = g.width, h = g.height;
    c = U.canvas(w, h);
    const x = c.getContext('2d');
    x.fillStyle = `rgba(255,255,255,${1 - strength})`; x.fillRect(0, 0, w, h);
    x.globalAlpha = strength; x.drawImage(g, 0, 0);
    adj.set(key, c); return c;
  }
  return {
    list, get,
    pattern(ctx, id, scale = 1, strength = 0.5) {
      if (!id || strength <= 0) return null;
      const p = ctx.createPattern(strengthened(id, U.clamp(strength, 0, 1)), 'repeat');
      if (p && p.setTransform) p.setTransform(new DOMMatrix([scale, 0, 0, scale, 0, 0]));
      return p;
    },
    addImage(id, img) {
      const S2 = 256, c = U.canvas(S2, S2), x = c.getContext('2d', { willReadFrequently: true });
      x.drawImage(img, 0, 0, S2, S2);
      const im = x.getImageData(0, 0, S2, S2), d = im.data;
      for (let i = 0; i < d.length; i += 4) { const l = (d[i] * 0.3 + d[i + 1] * 0.59 + d[i + 2] * 0.11) / 255; d[i] = d[i + 1] = d[i + 2] = 255; d[i + 3] = l * d[i + 3]; }
      x.putImageData(im, 0, 0); custom.set(id, c);
      for (const k of Array.from(adj.keys())) if (k.startsWith(id + '|')) adj.delete(k);
    }
  };
})();

/* ---------- формы отпечатка ---------- */
const Tips = (() => {
  const LV = [8, 16, 32, 64, 128, 256, 512];
  const cache = new Map(), scratch = new Map();
  const shapes = [
    { id: 'round', name: 'Круглая' }, { id: 'square', name: 'Квадратная' }, { id: 'chalk', name: 'Меловая' }, { id: 'bristle', name: 'Ворс' },
    { id: 'rake', name: 'Гребёнка' }, { id: 'spray', name: 'Распыление' }, { id: 'splatter', name: 'Клякса' }, { id: 'sponge', name: 'Губка' },
    { id: 'star', name: 'Звезда' }, { id: 'leaf', name: 'Лист' }, { id: 'hatch', name: 'Штрихи' }, { id: 'image', name: 'Из изображения' }
  ];
  const level = s => { for (const l of LV) if (l >= s) return l; return 512; };
  function softDot(x, cx, cy, r, a) {
    const g = x.createRadialGradient(cx, cy, 0, cx, cy, r);
    g.addColorStop(0, `rgba(255,255,255,${a})`); g.addColorStop(0.55, `rgba(255,255,255,${a * 0.8})`); g.addColorStop(1, 'rgba(255,255,255,0)');
    x.fillStyle = g; x.beginPath(); x.arc(cx, cy, r, 0, 7); x.fill();
  }
  function round(x, S, h) {
    const R = S / 2;
    if (h >= 0.99) { x.fillStyle = '#fff'; x.beginPath(); x.arc(R, R, R - 0.5, 0, 7); x.fill(); return; }
    const g = x.createRadialGradient(R, R, 0, R, R, R);
    g.addColorStop(0, 'rgba(255,255,255,1)');
    const n = 12;
    for (let i = 0; i <= n; i++) { const t = h + (1 - h) * i / n; g.addColorStop(Math.min(1, t), `rgba(255,255,255,${(1 - U.smooth(i / n)).toFixed(3)})`); }
    x.fillStyle = g; x.fillRect(0, 0, S, S);
  }
  function perPixel(x, S, fn) {
    const im = x.getImageData(0, 0, S, S), d = im.data;
    for (let y = 0; y < S; y++) for (let X = 0; X < S; X++) {
      const i = (y * S + X) * 4, u = (X + 0.5) / S, v = (y + 0.5) / S;
      d[i] = d[i + 1] = d[i + 2] = 255; d[i + 3] = U.clamp(fn(u, v, d[i + 3] / 255), 0, 1) * 255;
    }
    x.putImageData(im, 0, 0);
  }
  function gen(shape, hard, S, img) {
    const c = U.canvas(S, S), x = c.getContext('2d', { willReadFrequently: true }), R = S / 2, r = U.rng(4242);
    x.fillStyle = '#fff'; x.strokeStyle = '#fff';
    switch (shape) {
      case 'square': {
        const m = S * 0.05; x.fillRect(m, m, S - 2 * m, S - 2 * m);
        if (hard < 0.95 && U.canFilter) { const t = U.clone(c); x.clearRect(0, 0, S, S); x.filter = `blur(${(1 - hard) * S * 0.08}px)`; x.drawImage(t, 0, 0); x.filter = 'none'; }
        break;
      }
      case 'chalk': {
        round(x, S, Math.max(0.6, hard));
        perPixel(x, S, (u, v, a) => {
          const d = Math.hypot(u - 0.5, v - 0.5) * 2;
          const n = Noise.value(u * 9, v * 9, 3) * 0.55 + Noise.value(u * 31, v * 31, 5) * 0.45;
          return a * U.clamp((1.35 - d * d) - (1 - n) * 1.1, 0, 1);
        });
        break;
      }
      case 'bristle':
        for (let k = 0; k < 48; k++) { const t = r() * Math.PI * 2, rr = Math.sqrt(r()) * R * 0.82; softDot(x, R + Math.cos(t) * rr, R + Math.sin(t) * rr, R * (0.06 + 0.07 * r()), 0.45 + 0.55 * r()); }
        break;
      case 'rake': {
        const n = 11;
        for (let i = 0; i < n; i++) { const y = S * (0.06 + 0.88 * i / (n - 1)) + (r() - 0.5) * S * 0.03; softDot(x, R + (r() - 0.5) * S * 0.12, y, R * (0.07 + 0.05 * r()), 0.5 + 0.5 * r()); }
        break;
      }
      case 'spray':
        for (let k = 0; k < 320; k++) {
          const g = Math.sqrt(-2 * Math.log(r() + 1e-6)) * Math.cos(2 * Math.PI * r()), t = r() * Math.PI * 2, rr = Math.min(0.94, Math.abs(g) * 0.36) * R;
          x.globalAlpha = 0.55 + 0.45 * r(); x.beginPath(); x.arc(R + Math.cos(t) * rr, R + Math.sin(t) * rr, Math.max(0.5, R * (0.012 + 0.022 * r())), 0, 7); x.fill();
        }
        break;
      case 'splatter':
        for (let k = 0; k < 7; k++) { x.beginPath(); x.arc(R + (r() - 0.5) * R * 0.35, R + (r() - 0.5) * R * 0.35, R * (0.2 + 0.14 * r()), 0, 7); x.fill(); }
        for (let k = 0; k < 22; k++) { const t = r() * Math.PI * 2, rr = R * (0.45 + 0.48 * r()); x.beginPath(); x.arc(R + Math.cos(t) * rr, R + Math.sin(t) * rr, R * (0.015 + 0.07 * r() * r()), 0, 7); x.fill(); }
        break;
      case 'sponge':
        perPixel(x, S, (u, v) => {
          const d = Math.hypot(u - 0.5, v - 0.5) * 2;
          const n = Noise.value(u * 6, v * 6, 11) * 0.6 + Noise.value(u * 15, v * 15, 13) * 0.4;
          return U.smooth((n - 0.42) * 4) * (1 - U.smooth((d - 0.7) / 0.3));
        });
        break;
      case 'star': {
        x.beginPath();
        for (let i = 0; i < 10; i++) { const a = -Math.PI / 2 + i * Math.PI / 5, rr = i % 2 ? R * 0.42 : R * 0.96; x.lineTo(R + Math.cos(a) * rr, R + Math.sin(a) * rr); }
        x.closePath(); x.fill(); break;
      }
      case 'leaf':
        x.beginPath(); x.moveTo(R, S * 0.03);
        x.bezierCurveTo(S * 0.96, S * 0.3, S * 0.8, S * 0.8, R, S * 0.98);
        x.bezierCurveTo(S * 0.2, S * 0.8, S * 0.04, S * 0.3, R, S * 0.03); x.fill();
        x.globalCompositeOperation = 'destination-out'; x.lineWidth = Math.max(1, S * 0.025);
        x.beginPath(); x.moveTo(R, S * 0.12); x.lineTo(R, S * 0.9); x.stroke(); x.globalCompositeOperation = 'source-over';
        break;
      case 'hatch':
        x.lineWidth = Math.max(1, S * 0.06); x.lineCap = 'round';
        for (let i = 0; i < 5; i++) { const o = S * (0.12 + i * 0.19); x.beginPath(); x.moveTo(o - S * 0.1, S * 0.9); x.lineTo(o + S * 0.1, S * 0.1); x.stroke(); }
        break;
      case 'image':
        if (img && img.width) {
          const k = Math.min(S / img.width, S / img.height), w = img.width * k, h = img.height * k;
          x.drawImage(img, (S - w) / 2, (S - h) / 2, w, h);
          const im = x.getImageData(0, 0, S, S), d = im.data;
          let aSum = 0; for (let i = 3; i < d.length; i += 4) aSum += d[i];
          const hasAlpha = aSum / (S * S * 255) < 0.93;
          for (let i = 0; i < d.length; i += 4) {
            const l = (d[i] * 0.3 + d[i + 1] * 0.59 + d[i + 2] * 0.11) / 255;
            d[i + 3] = hasAlpha ? d[i + 3] : (1 - l) * d[i + 3]; d[i] = d[i + 1] = d[i + 2] = 255;
          }
          x.putImageData(im, 0, 0);
        } else round(x, S, hard);
        break;
      default: round(x, S, hard);
    }
    x.globalAlpha = 1;
    return c;
  }
  return {
    shapes, level,
    get(shape, hardness, size, img) {
      const L = level(size), key = shape + '|' + (+hardness).toFixed(2) + '|' + L + (shape === 'image' && img ? '|' + (img._mzid || (img._mzid = U.uid())) : '');
      let c = cache.get(key);
      if (!c) { c = gen(shape, hardness, L, img); cache.set(key, c); }
      return c;
    },
    tint(mask, col) {
      const c = U.canvas(mask.width, mask.height), x = c.getContext('2d');
      x.drawImage(mask, 0, 0); x.globalCompositeOperation = 'source-in'; x.fillStyle = col; x.fillRect(0, 0, c.width, c.height);
      return c;
    },
    tintScratch(mask, col) {
      const L = mask.width; let c = scratch.get(L);
      if (!c) { c = U.canvas(L, L); scratch.set(L, c); }
      const x = c.getContext('2d');
      x.globalCompositeOperation = 'copy'; x.drawImage(mask, 0, 0); x.globalCompositeOperation = 'source-in'; x.fillStyle = col; x.fillRect(0, 0, L, L);
      x.globalCompositeOperation = 'source-over';
      return c;
    },
    /* отпечаток маски (белой) в контекст */
    stampMask(ctx, b, x, y, s, ang, img) {
      const m = Tips.get(b.shape, b.hardness, s, img), rd = b.roundness ?? 1;
      if (!ang && rd === 1) ctx.drawImage(m, x - s / 2, y - s / 2, s, s);
      else { ctx.save(); ctx.translate(x, y); ctx.rotate(ang || 0); ctx.drawImage(m, -s / 2, -s * rd / 2, s, s * rd); ctx.restore(); }
    }
  };
})();

/* ---------- библиотека кистей ---------- */
const BRUSH_BASE = {
  shape: 'round', hardness: 0.85, roundness: 1, angle: 0, follow: false, spacing: 0.08, streamline: 0.25,
  sizeMin: 1, sizeMax: 60, size: 0.3, opacity: 1, flow: 1, pSize: 0.7, pFlow: 0.2, minSize: 0.08, tiltSize: 0,
  jSize: 0, jOpacity: 0, jAngle: 0, scatter: 0, count: 1, jHue: 0, jSat: 0, jBri: 0,
  grain: null, grainScale: 1, grainStrength: 0.5, taperStart: 0, taperEnd: 0, taperMin: 0.1,
  smudge: 0.6, pixel: false, blend: null, tipImage: null, grainImage: null
};
const BRUSH_CATS = [
  { id: 'sketch', name: 'Эскиз' }, { id: 'ink', name: 'Тушь' }, { id: 'paint', name: 'Живопись' }, { id: 'air', name: 'Аэрограф' },
  { id: 'texture', name: 'Текстура' }, { id: 'fx', name: 'Декор' }, { id: 'pixel', name: 'Пиксель' }, { id: 'custom', name: 'Мои кисти' }
];
const BRUSH_PRESETS = [
  ['pencil_hb', 'Карандаш HB', 'sketch', { shape: 'chalk', hardness: 0.9, spacing: 0.1, sizeMax: 24, size: 0.22, flow: 0.55, pFlow: 0.8, pSize: 0.35, grain: 'paper', grainStrength: 0.75, streamline: 0.12 }],
  ['pencil_6b', 'Карандаш 6B', 'sketch', { shape: 'chalk', hardness: 0.85, spacing: 0.09, sizeMax: 48, size: 0.28, flow: 0.5, pFlow: 0.7, pSize: 0.4, grain: 'paper', grainStrength: 0.65, grainScale: 1.3, streamline: 0.12 }],
  ['graphite', 'Графит', 'sketch', { shape: 'chalk', roundness: 0.55, follow: true, spacing: 0.07, sizeMax: 64, size: 0.3, flow: 0.35, pFlow: 0.85, pSize: 0.2, tiltSize: 1.5, grain: 'charcoal', grainStrength: 0.5, streamline: 0.1 }],
  ['tech_pen', 'Чертёжный', 'sketch', { hardness: 0.96, spacing: 0.06, sizeMax: 10, size: 0.3, flow: 0.9, pFlow: 0.4, pSize: 0.2, streamline: 0.35 }],
  ['liner', 'Линер', 'ink', { hardness: 1, spacing: 0.04, sizeMax: 40, size: 0.18, flow: 1, pFlow: 0, pSize: 0.15, streamline: 0.45 }],
  ['pen', 'Перо', 'ink', { hardness: 1, spacing: 0.04, sizeMax: 50, size: 0.22, pSize: 0.95, minSize: 0.03, pFlow: 0, taperStart: 6, taperEnd: 8, taperMin: 0.05, streamline: 0.5 }],
  ['dry_ink', 'Сухая тушь', 'ink', { shape: 'chalk', hardness: 1, spacing: 0.06, sizeMax: 60, size: 0.25, pSize: 0.75, pFlow: 0.1, grain: 'grit', grainStrength: 0.4, taperStart: 3, taperEnd: 4, streamline: 0.35 }],
  ['calligraphy', 'Каллиграфия', 'ink', { hardness: 0.98, roundness: 0.22, angle: 40, spacing: 0.03, sizeMax: 80, size: 0.3, pSize: 0.35, pFlow: 0, streamline: 0.4 }],
  ['brushpen', 'Брашпен', 'ink', { hardness: 0.95, spacing: 0.04, sizeMax: 70, size: 0.3, pSize: 1, minSize: 0.02, pFlow: 0, taperStart: 10, taperEnd: 12, taperMin: 0.02, streamline: 0.55 }],
  ['oil', 'Масло', 'paint', { shape: 'bristle', follow: true, spacing: 0.04, sizeMax: 150, size: 0.3, flow: 0.9, pFlow: 0.5, pSize: 0.45, grain: 'canvas', grainStrength: 0.35, jAngle: 0.02, streamline: 0.2, smudge: 0.75 }],
  ['flat', 'Плоская', 'paint', { shape: 'rake', follow: true, spacing: 0.03, sizeMax: 160, size: 0.3, flow: 0.8, pFlow: 0.4, pSize: 0.3, grain: 'canvas', grainStrength: 0.3, smudge: 0.7 }],
  ['gouache', 'Гуашь', 'paint', { hardness: 0.7, spacing: 0.06, sizeMax: 120, size: 0.3, flow: 0.85, pFlow: 0.5, pSize: 0.5, grain: 'paper', grainStrength: 0.3 }],
  ['bristle', 'Щетина', 'paint', { shape: 'rake', follow: true, spacing: 0.02, sizeMax: 100, size: 0.3, flow: 0.45, pFlow: 0.6, pSize: 0.4, jOpacity: 0.2, smudge: 0.8 }],
  ['knife', 'Мастихин', 'paint', { shape: 'square', hardness: 1, roundness: 0.25, follow: true, angle: 90, spacing: 0.03, sizeMax: 150, size: 0.3, pSize: 0.3, pFlow: 0, grain: 'sponge', grainStrength: 0.25, smudge: 0.9 }],
  ['air_soft', 'Мягкий аэрограф', 'air', { hardness: 0, spacing: 0.07, sizeMin: 5, sizeMax: 700, size: 0.35, flow: 0.12, pFlow: 0.9, pSize: 0, streamline: 0.15, smudge: 0.5 }],
  ['air_mid', 'Средний аэрограф', 'air', { hardness: 0.5, spacing: 0.06, sizeMin: 3, sizeMax: 500, size: 0.3, flow: 0.25, pFlow: 0.8, pSize: 0.1 }],
  ['air_hard', 'Жёсткий аэрограф', 'air', { hardness: 0.92, spacing: 0.05, sizeMin: 2, sizeMax: 400, size: 0.3, flow: 0.55, pFlow: 0.7, pSize: 0.1 }],
  ['watercolor', 'Акварель', 'air', { hardness: 0.25, spacing: 0.05, sizeMax: 250, size: 0.35, flow: 0.1, pFlow: 0.7, pSize: 0.5, jSize: 0.25, scatter: 0.06, grain: 'paper', grainStrength: 0.55, grainScale: 1.5, opacity: 0.85 }],
  ['glow', 'Свечение', 'air', { hardness: 0.3, spacing: 0.05, sizeMax: 200, size: 0.3, flow: 0.3, pFlow: 0.6, pSize: 0.5, blend: 'lighter' }],
  ['charcoal', 'Уголь', 'texture', { shape: 'chalk', hardness: 0.9, roundness: 0.7, follow: true, spacing: 0.07, sizeMax: 90, size: 0.3, flow: 0.6, pFlow: 0.8, pSize: 0.3, grain: 'charcoal', grainStrength: 0.8, tiltSize: 1 }],
  ['pastel', 'Пастель', 'texture', { shape: 'chalk', spacing: 0.06, sizeMax: 100, size: 0.3, flow: 0.7, pFlow: 0.6, pSize: 0.35, grain: 'paper', grainStrength: 0.85, grainScale: 1.6 }],
  ['chalk', 'Мел', 'texture', { shape: 'chalk', spacing: 0.08, sizeMax: 80, size: 0.3, flow: 0.8, pFlow: 0.4, pSize: 0.3, grain: 'grit', grainStrength: 0.7 }],
  ['spray', 'Баллончик', 'texture', { shape: 'spray', spacing: 0.18, jAngle: 1, sizeMin: 6, sizeMax: 500, size: 0.4, flow: 0.7, pFlow: 0.6, pSize: 0.2 }],
  ['sponge', 'Губка', 'texture', { shape: 'sponge', spacing: 0.3, jAngle: 1, jSize: 0.35, sizeMin: 6, sizeMax: 300, size: 0.35, flow: 0.7, pSize: 0.2 }],
  ['noise', 'Шумовая', 'texture', { hardness: 0.6, spacing: 0.08, sizeMax: 300, size: 0.3, flow: 0.8, pSize: 0.3, grain: 'noise', grainStrength: 0.9 }],
  ['stars', 'Звёзды', 'fx', { shape: 'star', hardness: 1, spacing: 1.6, jSize: 0.7, jAngle: 1, scatter: 1.6, jBri: 0.25, jHue: 0.06, sizeMax: 120, size: 0.3, pSize: 0.3, pFlow: 0, streamline: 0 }],
  ['splatter', 'Брызги', 'fx', { shape: 'splatter', hardness: 1, spacing: 1.2, jSize: 0.7, jAngle: 1, scatter: 1.2, sizeMax: 200, size: 0.3, pSize: 0.4, pFlow: 0, streamline: 0 }],
  ['leaves', 'Листва', 'fx', { shape: 'leaf', hardness: 1, follow: true, spacing: 0.9, jAngle: 0.15, scatter: 1.1, jSize: 0.5, jHue: 0.05, jBri: 0.3, jSat: 0.2, sizeMax: 120, size: 0.3, pFlow: 0, streamline: 0 }],
  ['dots', 'Пунктир', 'fx', { hardness: 1, spacing: 2.2, sizeMax: 60, size: 0.2, pSize: 0.3, pFlow: 0 }],
  ['hatching', 'Штриховка', 'fx', { shape: 'hatch', hardness: 1, spacing: 0.45, sizeMax: 120, size: 0.3, pSize: 0.3, pFlow: 0.3, streamline: 0.3 }],
  ['pixel', 'Пиксель', 'pixel', { pixel: true, shape: 'square', sizeMin: 1, sizeMax: 32, size: 0, spacing: 0.5, pSize: 0, pFlow: 0, flow: 1, streamline: 0 }],
  ['pixel_p', 'Пиксель с нажимом', 'pixel', { pixel: true, shape: 'square', sizeMin: 1, sizeMax: 64, size: 0.2, spacing: 0.5, pSize: 0.8, pFlow: 0, flow: 1, streamline: 0 }]
].map(([id, name, cat, o]) => Object.assign({}, BRUSH_BASE, { id, name, cat, preset: true }, o));

const Brushes = {
  custom: [], overrides: {}, version: 1, imgs: new Map(),
  async load() {
    try {
      const d = await Store.kvGet('brushes');
      if (d) { this.custom = d.custom || []; this.overrides = d.overrides || {}; }
    } catch (e) { }
    for (const b of this.all()) this.prepare(b);
  },
  persist() { this.version++; Store.kvSet('brushes', { custom: this.custom, overrides: this.overrides }).catch(() => { }); },
  all() { return BRUSH_PRESETS.map(p => this.get(p.id)).concat(this.custom); },
  list(cat) { return cat === 'custom' ? this.custom.slice() : BRUSH_PRESETS.filter(p => p.cat === cat).map(p => this.get(p.id)).concat(this.custom.filter(b => b.cat === cat)); },
  get(id) {
    const c = this.custom.find(b => b.id === id);
    if (c) return c;
    const p = BRUSH_PRESETS.find(b => b.id === id) || BRUSH_PRESETS[0];
    const o = this.overrides[p.id];
    if (!o) return p;
    if (!o._merged || o._v !== this.version) { o._merged = Object.assign({}, p, o, { _merged: undefined }); o._v = this.version; }
    return o._merged;
  },
  isModified(id) { return !!this.overrides[id]; },
  update(id, props) {
    const c = this.custom.find(b => b.id === id);
    if (c) Object.assign(c, props);
    else { const o = this.overrides[id] || (this.overrides[id] = {}); Object.assign(o, props); delete o._merged; }
    this.persist(); this.prepare(this.get(id));
  },
  reset(id) { delete this.overrides[id]; this.persist(); },
  duplicate(id) {
    const src = this.get(id), b = JSON.parse(JSON.stringify(Object.assign({}, src, { _merged: undefined, _v: undefined })));
    b.id = 'c_' + U.uid(); b.name = src.name + ' (копия)'; b.preset = false; b.cat = 'custom';
    this.custom.push(b); this.persist(); this.prepare(b); return b;
  },
  create() {
    const b = Object.assign({}, BRUSH_BASE, { id: 'c_' + U.uid(), name: 'Новая кисть', cat: 'custom', preset: false });
    this.custom.push(b); this.persist(); return b;
  },
  remove(id) { this.custom = this.custom.filter(b => b.id !== id); this.persist(); },
  exportBlob(id) {
    const b = Object.assign({}, this.get(id)); delete b._merged; delete b._v;
    return new Blob([JSON.stringify({ mazokBrush: 1, brush: b })], { type: 'application/json' });
  },
  async importFile(file) {
    const j = JSON.parse(await file.text());
    const src = j.brush || j;
    if (!src || typeof src !== 'object') throw new Error('Неверный файл кисти');
    const b = Object.assign({}, BRUSH_BASE, src, { id: 'c_' + U.uid(), cat: 'custom', preset: false });
    this.custom.push(b); this.persist(); this.prepare(b); return b;
  },
  prepare(b) {
    if (b.tipImage && !this.imgs.has(b.tipImage)) { const im = new Image(); im.src = b.tipImage; this.imgs.set(b.tipImage, im); }
    if (b.grainImage) {
      const gid = 'img_' + b.id;
      if (!this.imgs.has(gid + b.grainImage.length)) {
        const im = new Image(); im.onload = () => Grains.addImage(gid, im); im.src = b.grainImage; this.imgs.set(gid + b.grainImage.length, im);
      }
    }
  },
  tipImage(b) { if (!b.tipImage) return null; const im = this.imgs.get(b.tipImage); return im && im.complete && im.naturalWidth ? im : null; },
  grainId(b) { return b.grainImage ? 'img_' + b.id : b.grain; },
  sizePx(b, t) { const s = b.sizeMin + (b.sizeMax - b.sizeMin) * t * t; return b.pixel ? Math.max(1, Math.round(s)) : s; },
  /* образец штриха для библиотеки и студии */
  sample(b, w, h, col, opts = {}) {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const W = Math.round(w * dpr), H = Math.round(h * dpr);
    const buf = U.canvas(W, H), bx = buf.getContext('2d');
    const size = opts.size || Math.min(H * 0.55, Math.max(b.pixel ? 2 : 3, Brushes.sizePx(b, Math.max(b.size, 0.25)) * dpr * 0.6));
    const st = new Stroke({ brush: Object.assign({}, b, { streamline: 0 }), size: Math.min(size, H * 0.6), ctx: bx, color: col, seed: 99 });
    const n = 60;
    for (let i = 0; i <= n; i++) {
      const t = i / n, x = W * 0.08 + W * 0.84 * t, y = H / 2 + Math.sin(t * Math.PI * 2) * H * 0.18;
      st.add(x, y, Math.sin(t * Math.PI) * 0.9 + 0.1, 0);
    }
    if (b.taperEnd > 0) st.rerender(); else st.end();
    const out = U.canvas(W, H), ox = out.getContext('2d');
    ox.globalAlpha = b.opacity;
    const g = Grains.pattern(bx, Brushes.grainId(b), b.grainScale * dpr * 0.6, b.grainStrength);
    if (g) { bx.globalAlpha = 1; bx.globalCompositeOperation = 'destination-in'; bx.fillStyle = g; bx.fillRect(0, 0, W, H); }
    ox.drawImage(buf, 0, 0);
    return out;
  }
};

/* ---------- штрих ---------- */
class Stroke {
  constructor(o) {
    this.b = o.brush; this.base = o.size; this.ctx = o.ctx; this.color = o.color;
    this.mirrors = o.mirrors || null; this.onStamp = o.onStamp || null;
    this.seed = o.seed != null ? o.seed : (Math.random() * 2147483647) | 0;
    this.tipImg = Brushes.tipImage(this.b);
    this.colStr = U.rgbStr(this.color);
    this.jitterCol = this.b.jHue > 0 || this.b.jSat > 0 || this.b.jBri > 0;
    this.baseHsv = U.rgb2hsv(this.color.r, this.color.g, this.color.b);
    this.pts = []; this.sm = null; this.lastRaw = null; this.dirty = null; this.total = null;
    this.reset();
  }
  reset() { this.rng = U.rng(this.seed); this.dist = 0; this.next = 0; this.mid = null; this.dir = 0; this.lastSize = this.base; this.tints = new Map(); }
  add(x, y, p, tilt) {
    const raw = { x, y, p, tilt: tilt || 0 };
    this.lastRaw = raw;
    if (!this.sm) this.sm = Object.assign({}, raw);
    else {
      const s = U.clamp((this.b.streamline || 0) * Settings.streamlineMul, 0, 0.97);
      const k = 1 - s * 0.93, kp = Math.max(k, 0.4), sm = this.sm;
      this.sm = { x: sm.x + (x - sm.x) * k, y: sm.y + (y - sm.y) * k, p: sm.p + (p - sm.p) * kp, tilt: raw.tilt };
    }
    this.feed(Object.assign({}, this.sm));
  }
  feed(pt) {
    const P = this.pts;
    if (P.length) { const l = P[P.length - 1]; if (Math.abs(pt.x - l.x) + Math.abs(pt.y - l.y) < 0.5) { l.p = Math.max(l.p, pt.p); return; } }
    P.push(pt); this.renderAt(P.length - 1);
  }
  renderAt(i) {
    const P = this.pts;
    if (i === 0) { this.mid = Object.assign({}, P[0]); this.stampAt(P[0], 0); this.next = this.spacing(); return; }
    const c = P[i - 1], n = P[i];
    const b = { x: (c.x + n.x) / 2, y: (c.y + n.y) / 2, p: (c.p + n.p) / 2, tilt: n.tilt };
    this.walkQuad(this.mid, c, b); this.mid = b;
  }
  static quadSteps(a, c, b) { return Math.max(1, Math.ceil((Math.hypot(c.x - a.x, c.y - a.y) + Math.hypot(b.x - c.x, b.y - c.y)) / 2)); }
  walkQuad(a, c, b) {
    const n = Stroke.quadSteps(a, c, b);
    let prev = a;
    for (let k = 1; k <= n; k++) {
      const t = k / n, u = 1 - t;
      const q = { x: u * u * a.x + 2 * u * t * c.x + t * t * b.x, y: u * u * a.y + 2 * u * t * c.y + t * t * b.y, p: a.p + (b.p - a.p) * t, tilt: b.tilt };
      this.walkLine(prev, q); prev = q;
    }
  }
  walkLine(a, b) {
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    if (len < 1e-6) return;
    const d0 = this.dist, d1 = d0 + len;
    this.dir = Math.atan2(b.y - a.y, b.x - a.x);
    let guard = 0;
    while (this.next <= d1 && guard++ < 100000) {
      const t = (this.next - d0) / len;
      this.stampAt({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, p: a.p + (b.p - a.p) * t, tilt: b.tilt }, this.next);
      this.next += this.spacing();
    }
    this.dist = d1;
  }
  spacing() { return this.b.pixel ? Math.max(0.5, Math.min(1, this.lastSize * 0.5)) : Math.max(0.35, this.b.spacing * this.lastSize); }
  measure() {
    const P = this.pts; if (P.length < 2) return 0;
    let L = 0, mid = P[0];
    for (let i = 1; i < P.length; i++) {
      const c = P[i - 1], n = P[i], b = { x: (c.x + n.x) / 2, y: (c.y + n.y) / 2 };
      const k = Stroke.quadSteps(mid, c, b); let prev = mid;
      for (let j = 1; j <= k; j++) { const t = j / k, u = 1 - t; const q = { x: u * u * mid.x + 2 * u * t * c.x + t * t * b.x, y: u * u * mid.y + 2 * u * t * c.y + t * t * b.y }; L += Math.hypot(q.x - prev.x, q.y - prev.y); prev = q; }
      mid = b;
    }
    const l = P[P.length - 1];
    return L + Math.hypot(l.x - mid.x, l.y - mid.y);
  }
  catchUp() {
    const a = this.sm, r = this.lastRaw;
    if (!a || !r || !(this.b.streamline > 0)) return;
    const d = Math.hypot(r.x - a.x, r.y - a.y);
    if (d > 0.5) {
      const n = Math.min(16, Math.ceil(d / 2));
      for (let i = 1; i <= n; i++) { const t = i / n; this.feed({ x: a.x + (r.x - a.x) * t, y: a.y + (r.y - a.y) * t, p: a.p + (r.p - a.p) * t, tilt: r.tilt }); }
    }
    this.sm = Object.assign({}, r);
  }
  finish() { const P = this.pts; if (P.length > 1 && this.mid) { const l = P[P.length - 1]; this.walkLine(this.mid, l); this.mid = Object.assign({}, l); } }
  end() { this.catchUp(); this.finish(); }
  rerender() { this.total = this.measure(); this.reset(); for (let i = 0; i < this.pts.length; i++) this.renderAt(i); this.finish(); }
  replace(pts) { this.pts = pts; this.sm = pts.length ? Object.assign({}, pts[pts.length - 1]) : null; this.lastRaw = this.sm; }
  stampAt(pt, d) {
    const b = this.b, r = this.rng, pr = Settings.pressure(pt.p);
    let size = this.base * Math.max(b.minSize, 1 - b.pSize + b.pSize * pr);
    if (b.tiltSize && pt.tilt) size *= 1 + b.tiltSize * pt.tilt;
    const ts = b.taperStart * this.base, te = b.taperEnd * this.base;
    if (ts > 0 && d < ts) size *= U.lerp(b.taperMin, 1, U.smooth(d / ts));
    if (te > 0 && this.total != null && this.total - d < te) size *= U.lerp(b.taperMin, 1, U.smooth(Math.max(0, this.total - d) / te));
    const alpha = b.flow * (1 - b.pFlow + b.pFlow * pr);
    this.lastSize = size;
    const n = Math.max(1, b.count | 0);
    for (let k = 0; k < n; k++) {
      const s = size * (1 - b.jSize * r());
      const a = alpha * (1 - b.jOpacity * r());
      let ang = (b.angle || 0) * Math.PI / 180;
      if (b.follow) ang += this.dir;
      if (b.jAngle) ang += (r() - 0.5) * 2 * Math.PI * b.jAngle;
      let x = pt.x, y = pt.y;
      if (b.scatter) { const rr = Math.sqrt(r()) * b.scatter * size, th = r() * Math.PI * 2; x += Math.cos(th) * rr; y += Math.sin(th) * rr; }
      let col = null;
      if (this.jitterCol) {
        const h = this.baseHsv;
        col = U.rgbStr(U.hsv2rgb(h.h + (r() - 0.5) * 360 * b.jHue, U.clamp(h.s + (r() - 0.5) * 2 * b.jSat, 0, 1), U.clamp(h.v + (r() - 0.5) * 2 * b.jBri, 0, 1)));
      }
      this.emit(x, y, s, a, ang, col);
    }
  }
  emit(x, y, s, a, ang, col) {
    if (!this.b.pixel && s < 1) { a *= Math.max(0.15, s); s = 1; }
    const ms = this.mirrors;
    if (!ms) return this.draw(x, y, s, a, ang, col, 0);
    for (let i = 0; i < ms.length; i++) {
      const m = ms[i], dx = x - m.cx, dy = y - m.cy;
      const X = m.cx + m.a * dx + m.b * dy, Y = m.cy + m.c * dx + m.d * dy;
      const ca = Math.cos(ang), sa = Math.sin(ang);
      this.draw(X, Y, s, a, Math.atan2(m.c * ca + m.d * sa, m.a * ca + m.b * sa), col, i);
    }
  }
  draw(x, y, s, a, ang, col, mi) {
    const R = s * 0.75 + 2;
    const r = { x: x - R, y: y - R, w: 2 * R, h: 2 * R };
    this.dirty = this.dirty ? U.union(this.dirty, r) : r;
    if (this.onStamp) return this.onStamp(x, y, s, a, ang, mi);
    const ctx = this.ctx, b = this.b;
    ctx.globalAlpha = Math.min(1, a);
    if (b.pixel) {
      ctx.fillStyle = col || this.colStr;
      const q = Math.max(1, Math.round(s));
      ctx.fillRect(Math.round(x - q / 2), Math.round(y - q / 2), q, q);
      return;
    }
    let tip;
    if (col) tip = Tips.tintScratch(Tips.get(b.shape, b.hardness, s, this.tipImg), col);
    else {
      const L = Tips.level(s);
      tip = this.tints.get(L);
      if (!tip) { tip = Tips.tint(Tips.get(b.shape, b.hardness, s, this.tipImg), this.colStr); this.tints.set(L, tip); }
    }
    const rd = b.roundness ?? 1;
    if (!ang && rd === 1) ctx.drawImage(tip, x - s / 2, y - s / 2, s, s);
    else {
      const c = Math.cos(ang), sn = Math.sin(ang);
      ctx.setTransform(c, sn, -sn, c, x, y);
      ctx.drawImage(tip, -s / 2, -s * rd / 2, s, s * rd);
      ctx.setTransform(1, 0, 0, 1, 0, 0);
    }
  }
}

/* ---------- быстрые фигуры (удержание в конце штриха) ---------- */
const QuickShape = {
  detect(P) {
    if (P.length < 5) return null;
    const pts = P.map(p => [p.x, p.y]);
    let L = 0; for (let i = 1; i < pts.length; i++) L += Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
    if (L < 20) return null;
    const pr = P.map(p => p.p).sort((a, b) => a - b)[P.length >> 1];
    const a = pts[0], z = pts[pts.length - 1], dz = Math.hypot(z[0] - a[0], z[1] - a[1]);
    const bb = U.ptsBounds(pts), diag = Math.hypot(bb.w, bb.h);
    if (dz / L > 0.9) return { type: 'line', label: 'Линия', pts: this.line(a, z, pr) };
    const closed = dz < Math.max(18, diag * 0.22);
    if (closed) {
      const e = this.fitEllipse(pts);
      let poly = this.rdp(pts, diag * 0.07);
      if (poly.length > 3 && Math.hypot(poly[0][0] - poly[poly.length - 1][0], poly[0][1] - poly[poly.length - 1][1]) < diag * 0.25) poly.pop();
      poly = this.mergeCorner(poly, diag);
      if (e.err < 0.07 || poly.length > 6 || poly.length < 3) {
        if (e.err < 0.16) return { type: 'ellipse', label: e.a === e.b ? 'Круг' : 'Эллипс', pts: this.ellipse(e, pr) };
        return null;
      }
      if (poly.length === 4) poly = this.squareUp(poly);
      return { type: 'poly', label: ['', '', '', 'Треугольник', 'Прямоугольник', 'Пятиугольник', 'Шестиугольник'][poly.length], pts: this.poly(poly, true, pr) };
    }
    const poly = this.rdp(pts, diag * 0.06);
    if (poly.length <= 4) return { type: 'polyline', label: 'Ломаная', pts: this.poly(poly, false, pr) };
    return null;
  },
  mergeCorner(poly, diag) {
    if (poly.length < 4) return poly;
    const out = [];
    for (const p of poly) { const l = out[out.length - 1]; if (!l || Math.hypot(p[0] - l[0], p[1] - l[1]) > diag * 0.12) out.push(p); }
    if (out.length > 3 && Math.hypot(out[0][0] - out[out.length - 1][0], out[0][1] - out[out.length - 1][1]) < diag * 0.12) out.pop();
    return out;
  },
  squareUp(q) {
    const ang = i => { const a = q[i], b = q[(i + 1) % 4]; return Math.atan2(b[1] - a[1], b[0] - a[0]); };
    const axis = [0, 1, 2, 3].every(i => { const t = Math.abs(((ang(i) % (Math.PI / 2)) + Math.PI / 2) % (Math.PI / 2)); return t < 0.2 || t > Math.PI / 2 - 0.2; });
    if (!axis) return q;
    const b = U.ptsBounds(q);
    return [[b.x, b.y], [b.x + b.w, b.y], [b.x + b.w, b.y + b.h], [b.x, b.y + b.h]];
  },
  rdp(pts, eps) {
    if (pts.length < 3) return pts.slice();
    const keep = new Uint8Array(pts.length); keep[0] = keep[pts.length - 1] = 1;
    const st = [[0, pts.length - 1]];
    while (st.length) {
      const [i0, i1] = st.pop(); const [ax, ay] = pts[i0], [bx, by] = pts[i1];
      const dx = bx - ax, dy = by - ay, L = Math.hypot(dx, dy) || 1;
      let md = 0, mi = -1;
      for (let i = i0 + 1; i < i1; i++) { const d = Math.abs((pts[i][0] - ax) * dy - (pts[i][1] - ay) * dx) / L; if (d > md) { md = d; mi = i; } }
      if (md > eps && mi > 0) { keep[mi] = 1; st.push([i0, mi], [mi, i1]); }
    }
    return pts.filter((_, i) => keep[i]);
  },
  fitEllipse(pts) {
    let cx = 0, cy = 0; for (const p of pts) { cx += p[0]; cy += p[1]; } cx /= pts.length; cy /= pts.length;
    let sxx = 0, syy = 0, sxy = 0; for (const p of pts) { const dx = p[0] - cx, dy = p[1] - cy; sxx += dx * dx; syy += dy * dy; sxy += dx * dy; }
    const th = 0.5 * Math.atan2(2 * sxy, sxx - syy), c = Math.cos(th), s = Math.sin(th);
    let u0 = Infinity, u1 = -Infinity, v0 = Infinity, v1 = -Infinity;
    const uv = pts.map(p => { const dx = p[0] - cx, dy = p[1] - cy, u = dx * c + dy * s, v = -dx * s + dy * c; if (u < u0) u0 = u; if (u > u1) u1 = u; if (v < v0) v0 = v; if (v > v1) v1 = v; return [u, v]; });
    const cu = (u0 + u1) / 2, cv = (v0 + v1) / 2;
    let A = (u1 - u0) / 2, B = (v1 - v0) / 2;
    if (A < 2 || B < 2) return { err: 1 };
    let err = 0; for (const [u, v] of uv) err += Math.abs(Math.hypot((u - cu) / A, (v - cv) / B) - 1); err /= uv.length;
    if (Math.min(A, B) / Math.max(A, B) > 0.88) A = B = (A + B) / 2;
    return { cx: cx + cu * c - cv * s, cy: cy + cu * s + cv * c, a: A, b: B, th, err };
  },
  line(a, z, p) { const n = Math.max(2, Math.ceil(Math.hypot(z[0] - a[0], z[1] - a[1]) / 2)); const out = []; for (let i = 0; i <= n; i++) { const t = i / n; out.push({ x: a[0] + (z[0] - a[0]) * t, y: a[1] + (z[1] - a[1]) * t, p, tilt: 0 }); } return out; },
  ellipse(e, p) {
    const n = Math.max(24, Math.ceil(Math.PI * (e.a + e.b) / 2)), c = Math.cos(e.th), s = Math.sin(e.th), out = [];
    for (let i = 0; i <= n + 1; i++) { const t = i / n * Math.PI * 2, u = Math.cos(t) * e.a, v = Math.sin(t) * e.b; out.push({ x: e.cx + u * c - v * s, y: e.cy + u * s + v * c, p, tilt: 0 }); }
    return out;
  },
  poly(P, closed, p) {
    const out = [], pts = closed ? P.concat([P[0]]) : P;
    for (let i = 1; i < pts.length; i++) { const seg = this.line(pts[i - 1], pts[i], p); if (out.length) seg.shift(); out.push(...seg); }
    if (closed && out.length > 2) out.push(out[1]);
    return out;
  }
};
