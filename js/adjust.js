'use strict';
/* Мазок — коррекция и фильтры (на слой или кистью) */

const GRADIENT_MAPS = [
  { name: 'Закат', stops: [[0, '#1b0b3a'], [0.35, '#7a1f5c'], [0.65, '#f05d3b'], [1, '#ffe29a']] },
  { name: 'Сепия', stops: [[0, '#1e130a'], [0.5, '#8a6440'], [1, '#f4e6cc']] },
  { name: 'Ночь', stops: [[0, '#02030f'], [0.5, '#1f3a7a'], [1, '#bfe8ff']] },
  { name: 'Неон', stops: [[0, '#0d0221'], [0.4, '#a3008c'], [0.75, '#00d9ff'], [1, '#f6ffad']] },
  { name: 'Ч/Б', stops: [[0, '#000000'], [1, '#ffffff']] },
  { name: 'Тепло–холод', stops: [[0, '#1c3c6e'], [0.5, '#9c8f86'], [1, '#ffb066']] },
  { name: 'Лес', stops: [[0, '#0c1a0f'], [0.5, '#3f7a3a'], [1, '#e6f2b4']] },
  { name: 'Тёмный/светлый цвет', stops: null }
];

const lut = fn => { const t = new Uint8ClampedArray(256); for (let i = 0; i < 256; i++) t[i] = fn(i); return t; };
const monotone = pts => {
  const p = pts.slice().sort((a, b) => a[0] - b[0]), n = p.length;
  if (n < 2) return x => x;
  const dx = [], dy = [], m = [];
  for (let i = 0; i < n - 1; i++) { dx.push(p[i + 1][0] - p[i][0] || 1e-6); dy.push(p[i + 1][1] - p[i][1]); m.push(dy[i] / dx[i]); }
  const t = [m[0]];
  for (let i = 1; i < n - 1; i++) t.push(m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2);
  t.push(m[n - 2]);
  for (let i = 0; i < n - 1; i++) { if (m[i] === 0) { t[i] = t[i + 1] = 0; continue; } const a = t[i] / m[i], b = t[i + 1] / m[i], s = a * a + b * b; if (s > 9) { const k = 3 / Math.sqrt(s); t[i] = k * a * m[i]; t[i + 1] = k * b * m[i]; } }
  return x => {
    if (x <= p[0][0]) return p[0][1]; if (x >= p[n - 1][0]) return p[n - 1][1];
    let i = 0; while (x > p[i + 1][0]) i++;
    const h = dx[i], u = (x - p[i][0]) / h, u2 = u * u, u3 = u2 * u;
    return (2 * u3 - 3 * u2 + 1) * p[i][1] + (u3 - 2 * u2 + u) * h * t[i] + (-2 * u3 + 3 * u2) * p[i + 1][1] + (u3 - u2) * h * t[i + 1];
  };
};

const Adjust = {
  s: null, F: null, brushMode: false,
  list: [
    { id: 'hsb', name: 'Тон, насыщенность, яркость', params: [['h', 'Тон', -180, 180, 0], ['s', 'Насыщенность', -100, 100, 0], ['b', 'Яркость', -100, 100, 0]],
      px(d, p) {
        const a = p.h * Math.PI / 180, c = Math.cos(a), s = Math.sin(a), sat = 1 + p.s / 100, br = p.b / 100;
        const m = [0.299 + 0.701 * c + 0.168 * s, 0.587 - 0.587 * c + 0.330 * s, 0.114 - 0.114 * c - 0.497 * s,
          0.299 - 0.299 * c - 0.328 * s, 0.587 + 0.413 * c + 0.035 * s, 0.114 - 0.114 * c + 0.292 * s,
          0.299 - 0.3 * c + 1.25 * s, 0.587 - 0.588 * c - 1.05 * s, 0.114 + 0.886 * c - 0.203 * s];
        for (let i = 0; i < d.length; i += 4) {
          let r = d[i], g = d[i + 1], b = d[i + 2];
          if (p.h) { const R = m[0] * r + m[1] * g + m[2] * b, G = m[3] * r + m[4] * g + m[5] * b, B = m[6] * r + m[7] * g + m[8] * b; r = R; g = G; b = B; }
          if (p.s) { const l = 0.299 * r + 0.587 * g + 0.114 * b; r = l + (r - l) * sat; g = l + (g - l) * sat; b = l + (b - l) * sat; }
          if (br > 0) { r += (255 - r) * br; g += (255 - g) * br; b += (255 - b) * br; } else if (br < 0) { r *= 1 + br; g *= 1 + br; b *= 1 + br; }
          d[i] = r; d[i + 1] = g; d[i + 2] = b;
        }
      } },
    { id: 'balance', name: 'Цветовой баланс', params: [['tone', 'Диапазон', ['Тени', 'Средние', 'Света'], 1], ['cr', 'Голубой ↔ Красный', -100, 100, 0], ['mg', 'Пурпурный ↔ Зелёный', -100, 100, 0], ['yb', 'Жёлтый ↔ Синий', -100, 100, 0]],
      px(d, p) {
        const wt = lut(i => { const l = i / 255; return 255 * (p.tone === 0 ? U.clamp(1 - l * 2.2, 0, 1) : p.tone === 2 ? U.clamp(l * 2.2 - 1.2, 0, 1) : 1 - Math.abs(l - 0.5) * 2); });
        for (let i = 0; i < d.length; i += 4) { const w = wt[(d[i] * 77 + d[i + 1] * 150 + d[i + 2] * 29) >> 8] / 255 * 1.27; d[i] += p.cr * w; d[i + 1] += p.mg * w; d[i + 2] += p.yb * w; }
      } },
    { id: 'curves', name: 'Кривые', params: [['curves', 'curves']],
      px(d, p) {
        const c = p.curves, f = ch => lut(i => monotone(c[ch])(i / 255) * 255);
        const A = f('rgb'), R = f('r'), G = f('g'), B = f('b');
        for (let i = 0; i < d.length; i += 4) { d[i] = R[A[d[i]]]; d[i + 1] = G[A[d[i + 1]]]; d[i + 2] = B[A[d[i + 2]]]; }
      } },
    { id: 'gmap', name: 'Карта градиента', params: [['preset', 'Градиент', GRADIENT_MAPS.map(g => g.name), 0], ['mix', 'Сила', 0, 100, 100]],
      px(d, p) {
        let stops = GRADIENT_MAPS[p.preset].stops;
        if (!stops) { const a = Colors.prevRgb(), b = Colors.rgb(); stops = U.luma(a) < U.luma(b) ? [[0, U.hex(a)], [1, U.hex(b)]] : [[0, U.hex(b)], [1, U.hex(a)]]; }
        const st = stops.map(([t, h]) => [t, U.hex2rgb(h)]), tab = new Uint8ClampedArray(768);
        for (let i = 0; i < 256; i++) {
          const t = i / 255; let k = 0; while (k < st.length - 2 && t > st[k + 1][0]) k++;
          const [t0, c0] = st[k], [t1, c1] = st[Math.min(k + 1, st.length - 1)], u = t1 > t0 ? U.clamp((t - t0) / (t1 - t0), 0, 1) : 0;
          tab[i * 3] = c0.r + (c1.r - c0.r) * u; tab[i * 3 + 1] = c0.g + (c1.g - c0.g) * u; tab[i * 3 + 2] = c0.b + (c1.b - c0.b) * u;
        }
        const m = p.mix / 100;
        for (let i = 0; i < d.length; i += 4) { const l = (d[i] * 77 + d[i + 1] * 150 + d[i + 2] * 29) >> 8; d[i] += (tab[l * 3] - d[i]) * m; d[i + 1] += (tab[l * 3 + 1] - d[i + 1]) * m; d[i + 2] += (tab[l * 3 + 2] - d[i + 2]) * m; }
      } },
    { id: 'blur', name: 'Размытие по Гауссу', params: [['r', 'Радиус', 0, 120, 8]],
      canvas(src, x, p) { x.drawImage(U.blur(src, p.r), 0, 0); } },
    { id: 'motion', name: 'Размытие в движении', params: [['dist', 'Расстояние', 0, 300, 40], ['ang', 'Угол', 0, 360, 0]],
      canvas(src, x, p) {
        const n = Math.max(1, Math.min(48, Math.ceil(p.dist / 2))), a = p.ang * Math.PI / 180;
        for (let i = 0; i < n; i++) { const t = n === 1 ? 0 : i / (n - 1) - 0.5; x.globalAlpha = 1 / (i + 1); x.drawImage(src, Math.cos(a) * p.dist * t, Math.sin(a) * p.dist * t); }
        x.globalAlpha = 1;
      } },
    { id: 'radial', name: 'Радиальное размытие', params: [['amt', 'Сила', 0, 100, 20], ['cx', 'Центр X, %', 0, 100, 50], ['cy', 'Центр Y, %', 0, 100, 50]],
      canvas(src, x, p) {
        const n = 32, cx = src.width * p.cx / 100, cy = src.height * p.cy / 100;
        for (let i = 0; i < n; i++) { const k = 1 + p.amt / 100 * 0.25 * i / (n - 1); x.globalAlpha = 1 / (i + 1); x.setTransform(k, 0, 0, k, cx - cx * k, cy - cy * k); x.drawImage(src, 0, 0); }
        x.setTransform(1, 0, 0, 1, 0, 0); x.globalAlpha = 1;
      } },
    { id: 'sharpen', name: 'Резкость', params: [['amt', 'Сила', 0, 300, 80], ['r', 'Радиус', 1, 10, 2]],
      pre(src, p) { return U.readPixels(U.blur(src, p.r)).data; },
      px(d, p, w, h, pre) { const k = p.amt / 100; for (let i = 0; i < d.length; i += 4) { d[i] += (d[i] - pre[i]) * k; d[i + 1] += (d[i + 1] - pre[i + 1]) * k; d[i + 2] += (d[i + 2] - pre[i + 2]) * k; } } },
    { id: 'noise', name: 'Шум', params: [['amt', 'Количество', 0, 100, 25], ['mono', 'Тип', ['Цветной', 'Монохромный'], 1]],
      px(d, p) { const a = p.amt * 2.55, r = U.rng(7); for (let i = 0; i < d.length; i += 4) { if (!d[i + 3]) continue; if (p.mono) { const n = (r() - 0.5) * a; d[i] += n; d[i + 1] += n; d[i + 2] += n; } else { d[i] += (r() - 0.5) * a; d[i + 1] += (r() - 0.5) * a; d[i + 2] += (r() - 0.5) * a; } } } },
    { id: 'bloom', name: 'Свечение', params: [['th', 'Порог', 0, 100, 60], ['size', 'Размер', 1, 150, 30], ['amt', 'Сила', 0, 200, 100]],
      canvas(src, x, p) {
        const id = U.readPixels(src), d = id.data, t = p.th * 2.55;
        for (let i = 0; i < d.length; i += 4) { const l = d[i] * 0.299 + d[i + 1] * 0.587 + d[i + 2] * 0.114; if (l < t) d[i + 3] = 0; else d[i + 3] *= U.clamp((l - t) / Math.max(1, 255 - t) * 2, 0, 1); }
        const g = U.blur(U.fromImageData(id), p.size);
        x.drawImage(src, 0, 0); x.globalCompositeOperation = 'screen';
        let a = p.amt / 100; while (a > 0) { x.globalAlpha = Math.min(1, a); x.drawImage(g, 0, 0); a -= 1; }
        x.globalAlpha = 1; x.globalCompositeOperation = 'source-over';
      } },
    { id: 'chroma', name: 'Хроматическая аберрация', params: [['amt', 'Сдвиг', 0, 60, 8], ['ang', 'Угол', 0, 360, 0]],
      pre(src) { return U.readPixels(src).data.slice(); },
      px(d, p, w, h, s) {
        const a = p.ang * Math.PI / 180, ox = Math.round(Math.cos(a) * p.amt), oy = Math.round(Math.sin(a) * p.amt);
        for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
          const i = (y * w + x) * 4;
          const xr = U.clamp(x + ox, 0, w - 1), yr = U.clamp(y + oy, 0, h - 1), xb = U.clamp(x - ox, 0, w - 1), yb = U.clamp(y - oy, 0, h - 1);
          d[i] = s[(yr * w + xr) * 4]; d[i + 2] = s[(yb * w + xb) * 4 + 2];
          d[i + 3] = Math.max(s[i + 3], s[(yr * w + xr) * 4 + 3], s[(yb * w + xb) * 4 + 3]);
        }
      } },
    { id: 'halftone', name: 'Полутон', params: [['size', 'Размер ячейки', 4, 60, 12], ['mode', 'Режим', ['Цветной', 'Чёрно-белый'], 0]],
      canvas(src, x, p) {
        const S = p.size, w = src.width, h = src.height, sm = U.canvas(Math.ceil(w / S), Math.ceil(h / S)), sx = sm.getContext('2d', { willReadFrequently: true });
        sx.imageSmoothingQuality = 'high'; sx.drawImage(src, 0, 0, sm.width, sm.height);
        const d = sx.getImageData(0, 0, sm.width, sm.height).data;
        for (let j = 0; j < sm.height; j++) for (let i = 0; i < sm.width; i++) {
          const k = (j * sm.width + i) * 4, a = d[k + 3] / 255; if (a < 0.02) continue;
          const l = (d[k] * 0.299 + d[k + 1] * 0.587 + d[k + 2] * 0.114) / 255, r = S * 0.72 * Math.sqrt(1 - l);
          x.fillStyle = p.mode ? '#000' : `rgb(${d[k]},${d[k + 1]},${d[k + 2]})`; x.globalAlpha = a;
          x.beginPath(); x.arc(i * S + S / 2, j * S + S / 2, p.mode ? r : S * 0.55 * Math.max(0.35, 1 - l * 0.6), 0, 7); x.fill();
        }
        x.globalAlpha = 1;
      } },
    { id: 'pixelate', name: 'Пикселизация', params: [['size', 'Размер пикселя', 2, 120, 12]],
      canvas(src, x, p) { const sm = U.canvas(Math.ceil(src.width / p.size), Math.ceil(src.height / p.size)); sm.getContext('2d').drawImage(src, 0, 0, sm.width, sm.height); x.imageSmoothingEnabled = false; x.drawImage(sm, 0, 0, sm.width * p.size, sm.height * p.size); } },
    { id: 'vignette', name: 'Виньетка', params: [['amt', 'Сила', 0, 100, 50], ['size', 'Размер', 10, 100, 60]],
      canvas(src, x, p) {
        const w = src.width, h = src.height; x.drawImage(src, 0, 0);
        const g = x.createRadialGradient(w / 2, h / 2, Math.min(w, h) * p.size / 200, w / 2, h / 2, Math.hypot(w, h) / 2);
        g.addColorStop(0, 'rgba(0,0,0,0)'); g.addColorStop(1, `rgba(0,0,0,${p.amt / 100})`);
        x.globalCompositeOperation = 'source-atop'; x.fillStyle = g; x.fillRect(0, 0, w, h); x.globalCompositeOperation = 'source-over';
      } },
    { id: 'desat', name: 'Обесцветить', params: [['amt', 'Сила', 0, 100, 100]],
      px(d, p) { const k = p.amt / 100; for (let i = 0; i < d.length; i += 4) { const l = d[i] * 0.299 + d[i + 1] * 0.587 + d[i + 2] * 0.114; d[i] += (l - d[i]) * k; d[i + 1] += (l - d[i + 1]) * k; d[i + 2] += (l - d[i + 2]) * k; } } },
    { id: 'invert', name: 'Инверсия', params: [],
      px(d) { for (let i = 0; i < d.length; i += 4) { d[i] = 255 - d[i]; d[i + 1] = 255 - d[i + 1]; d[i + 2] = 255 - d[i + 2]; } } },
    { id: 'posterize', name: 'Постеризация', params: [['n', 'Уровни', 2, 24, 5]],
      px(d, p) { const n = p.n - 1, t = lut(i => Math.round(i / 255 * n) / n * 255); for (let i = 0; i < d.length; i += 4) { d[i] = t[d[i]]; d[i + 1] = t[d[i + 1]]; d[i + 2] = t[d[i + 2]]; } } },
    { id: 'threshold', name: 'Порог', params: [['t', 'Уровень', 0, 255, 128]],
      px(d, p) { for (let i = 0; i < d.length; i += 4) { const v = (d[i] * 0.299 + d[i + 1] * 0.587 + d[i + 2] * 0.114) >= p.t ? 255 : 0; d[i] = d[i + 1] = d[i + 2] = v; } } }
  ],
  defaults(f) {
    const p = {};
    for (const q of f.params) {
      if (q[1] === 'curves') p.curves = { rgb: [[0, 0], [1, 1]], r: [[0, 0], [1, 1]], g: [[0, 0], [1, 1]], b: [[0, 0], [1, 1]] };
      else p[q[0]] = Array.isArray(q[2]) ? q[3] : q[4];
    }
    return p;
  },
  open(id) {
    const d = Engine.doc, L = d.layer;
    if (!Ops.editable(L)) return;
    if (!L.visible) { U.toast('Слой скрыт'); return; }
    Tools.commitPending();
    if (L.text) Ops.rasterize(L);
    const f = this.list.find(x => x.id === id);
    this.s = { f, L, p: this.defaults(f), src: U.clone(L.canvas), data: null, pre: null };
    this.F = Engine.scratch('adjF').canvas;
    this.brushMode = false;
    Engine.beginOverride(L);
    UI.showAdjustPanel(f);
    this.update();
  },
  update() {
    if (!this.s || this.pend) return;
    this.pend = true;
    requestAnimationFrame(() => { this.pend = false; if (this.s) this.compute(); });
  },
  compute() {
    const s = this.s, f = s.f, Fx = Engine._s.adjF.ctx, w = s.src.width, h = s.src.height;
    Fx.save(); Fx.setTransform(1, 0, 0, 1, 0, 0); Fx.globalAlpha = 1; Fx.globalCompositeOperation = 'source-over'; Fx.clearRect(0, 0, w, h);
    if (f.canvas) f.canvas(s.src, Fx, s.p);
    else {
      if (!s.data) s.data = U.readPixels(s.src);
      const id = new ImageData(new Uint8ClampedArray(s.data.data), w, h);
      const pre = f.pre ? f.pre(s.src, s.p) : null;
      f.px(id.data, s.p, w, h, pre);
      Fx.putImageData(id, 0, 0);
    }
    Fx.restore();
    if (!this.brushMode) {
      const A = Engine._s.active.ctx;
      if (Sel.active) Engine.maskedMix(A, s.src, this.F, Sel.mask);
      else { A.save(); A.globalCompositeOperation = 'copy'; A.drawImage(this.F, 0, 0); A.restore(); }
      Engine.renderAll();
    }
  },
  setParam(k, v) { if (!this.s) return; this.s.p[k] = v; this.update(); },
  setBrushMode(on) {
    const s = this.s; if (!s || on === this.brushMode) return;
    this.brushMode = on;
    if (on) { Engine.endOverride(); Engine.renderAll(); if (Tools.cur !== 'brush') Tools.set('brush'); UI.hint('Рисуйте, чтобы применить эффект кистью'); }
    else { s.src = U.clone(s.L.canvas); s.data = null; Engine.beginOverride(s.L); this.compute(); }
  },
  reset() { if (!this.s) return; this.s.p = this.defaults(this.s.f); UI.showAdjustPanel(this.s.f); this.update(); },
  apply() {
    const s = this.s; if (!s) return;
    if (!this.brushMode) { this.compute(); Engine.commit(Engine.full(), s.f.name); }
    this.end();
  },
  cancel() {
    const s = this.s; if (!s) return;
    if (!this.brushMode) { Engine.endOverride(); Engine.renderAll(); }
    this.end();
  },
  end() { this.s = null; this.brushMode = false; this.F = null; Engine.free('adjF'); UI.hideOptbar(); },
  /* редактор кривых */
  curveEditor(p, onChange) {
    const S = 240, c = U.el('canvas', { class: 'curve', width: S * 2, height: S * 2, style: { width: S + 'px', height: S + 'px' } }), x = c.getContext('2d');
    let ch = 'rgb', drag = -1;
    const cols = { rgb: '#ddd', r: '#ff5a5a', g: '#4cd964', b: '#4aa3ff' };
    const draw = () => {
      x.setTransform(2, 0, 0, 2, 0, 0); x.clearRect(0, 0, S, S);
      x.strokeStyle = 'rgba(128,128,128,.3)'; x.lineWidth = 1; x.beginPath();
      for (let i = 1; i < 4; i++) { x.moveTo(i * S / 4, 0); x.lineTo(i * S / 4, S); x.moveTo(0, i * S / 4); x.lineTo(S, i * S / 4); }
      x.stroke();
      for (const k of ['r', 'g', 'b', 'rgb']) {
        const fn = monotone(p.curves[k]); x.strokeStyle = cols[k]; x.globalAlpha = k === ch ? 1 : 0.3; x.lineWidth = k === ch ? 2 : 1;
        x.beginPath(); for (let i = 0; i <= S; i++) { const y = S - U.clamp(fn(i / S), 0, 1) * S; i ? x.lineTo(i, y) : x.moveTo(i, y); } x.stroke();
      }
      x.globalAlpha = 1;
      for (const [u, v] of p.curves[ch]) { x.fillStyle = '#fff'; x.strokeStyle = cols[ch]; x.beginPath(); x.arc(u * S, S - v * S, 5, 0, 7); x.fill(); x.stroke(); }
    };
    const pos = e => { const r = c.getBoundingClientRect(); return [U.clamp((e.clientX - r.left) / S, 0, 1), U.clamp(1 - (e.clientY - r.top) / S, 0, 1)]; };
    c.addEventListener('pointerdown', e => {
      c.setPointerCapture(e.pointerId); const [u, v] = pos(e), pts = p.curves[ch];
      drag = pts.findIndex(([a, b]) => Math.hypot((a - u) * S, (b - v) * S) < 12);
      if (drag < 0) { pts.push([u, v]); pts.sort((a, b) => a[0] - b[0]); drag = pts.findIndex(q => q[0] === u && q[1] === v); }
      draw(); onChange();
    });
    c.addEventListener('pointermove', e => {
      if (drag < 0) return; const [u, v] = pos(e), pts = p.curves[ch];
      const lo = drag > 0 ? pts[drag - 1][0] + 0.01 : 0, hi = drag < pts.length - 1 ? pts[drag + 1][0] - 0.01 : 1;
      const out = (e.clientY - c.getBoundingClientRect().bottom > 30 || e.clientY < c.getBoundingClientRect().top - 30) && pts.length > 2 && drag > 0 && drag < pts.length - 1;
      if (out) { pts.splice(drag, 1); drag = -1; } else pts[drag] = [U.clamp(u, lo, hi), v];
      draw(); onChange();
    });
    c.addEventListener('pointerup', () => { drag = -1; });
    const tabs = UI.seg([['rgb', 'RGB'], ['r', 'R'], ['g', 'G'], ['b', 'B']], ch, v => { ch = v; draw(); });
    draw();
    return U.el('div', null, tabs, c, U.el('div', { class: 'row', html: '<small>Коснитесь, чтобы добавить точку; утащите точку за край, чтобы удалить</small>' }));
  }
};
