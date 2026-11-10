'use strict';
/* Мазок — цвет: текущий/предыдущий, круг, квадрат, гармония, значения, палитры */

const Colors = {
  cur: { h: 0, s: 0, v: 0.08 }, prev: { h: 0, s: 0, v: 1 }, recent: [],
  rgb() { return U.hsv2rgb(this.cur.h, this.cur.s, this.cur.v); },
  prevRgb() { return U.hsv2rgb(this.prev.h, this.prev.s, this.prev.v); },
  hex() { return U.hex(this.rgb()); },
  set(c, live) {
    const hsv = c.h !== undefined ? { h: c.h, s: c.s, v: c.v } : U.rgb2hsv(c.r, c.g, c.b);
    if (hsv.s === 0 && c.h === undefined) hsv.h = this.cur.h;
    if (!live) this.prev = Object.assign({}, this.cur);
    this.cur = hsv; this.changed();
  },
  live(hsv) { this.cur = hsv; this.changed(); },
  beginLive() { this._before = Object.assign({}, this.cur); },
  endLive() { if (this._before) { const b = this._before; if (b.h !== this.cur.h || b.s !== this.cur.s || b.v !== this.cur.v) this.prev = b; this._before = null; this.changed(); } },
  swap() { const t = this.cur; this.cur = this.prev; this.prev = t; this.changed(); },
  used() { const h = this.hex(); this.recent = [h].concat(this.recent.filter(x => x !== h)).slice(0, 12); this.persist(); },
  changed() { UI.colorChanged(); this.persist(); },
  persist: U.debounce(() => { try { localStorage.setItem('mazok.color', JSON.stringify({ cur: Colors.cur, prev: Colors.prev, recent: Colors.recent })); } catch (e) { } }, 400),
  load() { try { const o = JSON.parse(localStorage.getItem('mazok.color') || 'null'); if (o) { this.cur = o.cur || this.cur; this.prev = o.prev || this.prev; this.recent = o.recent || []; } } catch (e) { } }
};

const Palettes = {
  list: [], def: 0,
  builtins() {
    return [
      { name: 'Базовая', colors: ['#000000', '#3d3d3d', '#7a7a7a', '#bdbdbd', '#ffffff', '#e53935', '#fb8c00', '#fdd835', '#43a047', '#1e88e5', '#3949ab', '#8e24aa', '#6d4c41', '#f48fb1', '#80deea'] },
      { name: 'Кожа', colors: ['#ffe0c7', '#f9c9a4', '#eab08c', '#d89a73', '#c18060', '#a26849', '#83503a', '#653c2c', '#4a2b20', '#2e1a14', '#f2b8a0', '#e3967f'] },
      { name: 'Пастель', colors: ['#ffd1dc', '#ffe4c4', '#fff5ba', '#d4f0c0', '#c1e7e3', '#bde0fe', '#cdb4db', '#ffc8dd', '#a2d2ff', '#e2ece9', '#fde2e4', '#dfe7fd'] },
      { name: 'Природа', colors: ['#1b2f1c', '#2f4f2f', '#4f772d', '#90a955', '#ecf39e', '#7f5539', '#b08968', '#ddb892', '#3a5a40', '#588157', '#a3b18a', '#dad7cd', '#386fa4', '#59a5d8', '#84d2f6'] },
      { name: 'Ночной город', colors: ['#0b0c1e', '#1b1f3b', '#2d3561', '#7c3aed', '#c026d3', '#f43f5e', '#fb923c', '#facc15', '#22d3ee', '#a3e635', '#f5f5f5', '#64748b'] }
    ];
  },
  async load() {
    try { const d = await Store.kvGet('palettes'); if (d && d.list && d.list.length) { this.list = d.list; this.def = d.def || 0; } } catch (e) { }
    if (!this.list.length) this.list = this.builtins();
  },
  save() { Store.kvSet('palettes', { list: this.list, def: this.def }).catch(() => { }); },
  async fromImage(img) {
    const S = 72, c = U.canvas(S, S), x = c.getContext('2d', { willReadFrequently: true });
    x.drawImage(img, 0, 0, S, S);
    const d = x.getImageData(0, 0, S, S).data, px = [];
    for (let i = 0; i < d.length; i += 4) if (d[i + 3] > 128) px.push([d[i], d[i + 1], d[i + 2]]);
    if (!px.length) return [];
    const k = Math.min(15, px.length), dist = (a, b) => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;
    let cen = [px[(px.length / 2) | 0].slice()];
    const md = px.map(p => dist(p, cen[0]));
    while (cen.length < k) {
      let bi = 0; for (let i = 1; i < px.length; i++) if (md[i] > md[bi]) bi = i;
      if (md[bi] < 200) break;
      const c = px[bi].slice(); cen.push(c);
      for (let i = 0; i < px.length; i++) md[i] = Math.min(md[i], dist(px[i], c));
    }
    for (let it = 0; it < 10; it++) {
      const sum = cen.map(() => [0, 0, 0, 0]);
      for (const p of px) { let b = 0, bd = Infinity; cen.forEach((c2, j) => { const dd = (p[0] - c2[0]) ** 2 + (p[1] - c2[1]) ** 2 + (p[2] - c2[2]) ** 2; if (dd < bd) { bd = dd; b = j; } }); const s = sum[b]; s[0] += p[0]; s[1] += p[1]; s[2] += p[2]; s[3]++; }
      cen = cen.map((c2, j) => sum[j][3] ? [sum[j][0] / sum[j][3], sum[j][1] / sum[j][3], sum[j][2] / sum[j][3]] : c2);
    }
    return cen.map(([r2, g, b]) => U.hex({ r: r2, g, b })).filter((h, i, a) => a.indexOf(h) === i).sort((a, b) => U.luma(U.hex2rgb(a)) - U.luma(U.hex2rgb(b)));
  }
};

const ColorUI = {
  tab: 'wheel', harm: 'comp',
  build() {
    const root = U.el('div', { class: 'cpanel' });
    const top = U.el('div', { class: 'ctop' });
    const curSw = U.el('div', { class: 'cswatch', title: 'Текущий цвет' }), prevSw = U.el('button', { class: 'cswatch', title: 'Предыдущий цвет (коснитесь, чтобы поменять местами)', onclick: () => Colors.swap() });
    const hex = U.el('input', { class: 'input', style: { width: '96px', height: '30px' }, onchange: () => { const c = U.hex2rgb(hex.value); if (c) Colors.set(c); } });
    top.append(curSw, prevSw, U.el('div', { class: 'grow', style: { flex: 1 } }), hex);
    const tabs = U.el('div', { class: 'tabs' });
    const body = U.el('div');
    const T = [['wheel', 'Круг'], ['square', 'Квадрат'], ['harmony', 'Гармония'], ['values', 'Значения'], ['palettes', 'Палитры']];
    for (const [id, name] of T) tabs.append(U.el('button', { text: name, class: this.tab === id ? 'on' : '', onclick: () => { this.tab = id; UI.reopen('color'); } }));
    const recent = U.el('div', { class: 'recent' });
    root.append(top, tabs, body, recent);
    let upd = () => { };
    if (this.tab === 'wheel') upd = this.wheel(body);
    else if (this.tab === 'square') upd = this.square(body);
    else if (this.tab === 'harmony') upd = this.harmony(body);
    else if (this.tab === 'values') upd = this.values(body);
    else this.palettes(body);
    if (this.tab !== 'palettes') { const p = Palettes.list[Palettes.def]; if (p) body.append(U.el('div', { class: 'sect', text: 'Палитра: ' + p.name }), this.palGrid(p, Palettes.def, true)); }
    const refresh = () => {
      curSw.style.background = Colors.hex(); prevSw.style.background = U.hex(Colors.prevRgb());
      if (document.activeElement !== hex) hex.value = Colors.hex();
      recent.innerHTML = '';
      for (const h of Colors.recent) recent.append(U.el('button', { style: { background: h }, title: h, onclick: () => Colors.set(U.hex2rgb(h)) }));
      upd();
    };
    root._refresh = refresh; refresh();
    return root;
  },
  drag(el, fn) {
    el.addEventListener('pointerdown', e => { el.setPointerCapture(e.pointerId); Colors.beginLive(); this._dragging = true; fn(e, true); });
    el.addEventListener('pointermove', e => { if (this._dragging && e.buttons) fn(e, false); });
    el.addEventListener('pointerup', () => { if (this._dragging) { this._dragging = false; Colors.endLive(); } });
  },
  local(e, el) { const r = el.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top, r.width, r.height]; },
  wheel(body) {
    const S = 260, dpr = Math.min(2, window.devicePixelRatio || 1);
    const c = U.el('canvas', { class: 'cwheel', width: S * dpr, height: S * dpr, style: { width: S + 'px', height: S + 'px' } }), x = c.getContext('2d');
    body.append(c);
    const R = S / 2, ring = 22, half = (R - ring - 8) / Math.SQRT2;
    let mode = null;
    const draw = () => {
      const h = Colors.cur; x.setTransform(dpr, 0, 0, dpr, 0, 0); x.clearRect(0, 0, S, S);
      if (x.createConicGradient) { const g = x.createConicGradient(-Math.PI / 2, R, R); for (let i = 0; i <= 12; i++) g.addColorStop(i / 12, `hsl(${i * 30},100%,50%)`); x.fillStyle = g; }
      else x.fillStyle = '#888';
      x.beginPath(); x.arc(R, R, R - 1, 0, Math.PI * 2); x.moveTo(R + R - ring, R); x.arc(R, R, R - ring, Math.PI * 2, 0, true); x.fill();
      const sx = R - half, sy = R - half, sw = half * 2;
      x.fillStyle = `hsl(${h.h},100%,50%)`; x.fillRect(sx, sy, sw, sw);
      let g = x.createLinearGradient(sx, 0, sx + sw, 0); g.addColorStop(0, '#fff'); g.addColorStop(1, 'rgba(255,255,255,0)'); x.fillStyle = g; x.fillRect(sx, sy, sw, sw);
      g = x.createLinearGradient(0, sy, 0, sy + sw); g.addColorStop(0, 'rgba(0,0,0,0)'); g.addColorStop(1, '#000'); x.fillStyle = g; x.fillRect(sx, sy, sw, sw);
      const a = (h.h - 90) * Math.PI / 180, rr = R - ring / 2;
      this.marker(x, R + Math.cos(a) * rr, R + Math.sin(a) * rr, `hsl(${h.h},100%,50%)`);
      this.marker(x, sx + h.s * sw, sy + (1 - h.v) * sw, Colors.hex());
    };
    this.drag(c, (e, first) => {
      const [px, py] = this.local(e, c), dx = px - R, dy = py - R, d = Math.hypot(dx, dy);
      if (first) mode = d > R - ring - 4 ? 'ring' : 'sq';
      const h = Object.assign({}, Colors.cur);
      if (mode === 'ring') h.h = (Math.atan2(dy, dx) * 180 / Math.PI + 90 + 360) % 360;
      else { h.s = U.clamp((px - (R - half)) / (half * 2), 0, 1); h.v = U.clamp(1 - (py - (R - half)) / (half * 2), 0, 1); }
      Colors.live(h);
    });
    return draw;
  },
  marker(x, X, Y, fill) { x.lineWidth = 3; x.strokeStyle = '#fff'; x.fillStyle = fill; x.beginPath(); x.arc(X, Y, 8, 0, 7); x.fill(); x.stroke(); x.lineWidth = 1; x.strokeStyle = '#0008'; x.beginPath(); x.arc(X, Y, 9.5, 0, 7); x.stroke(); },
  square(body) {
    const W = 276, H = 200, dpr = Math.min(2, window.devicePixelRatio || 1);
    const c = U.el('canvas', { class: 'cwheel', width: W * dpr, height: H * dpr, style: { width: W + 'px', height: H + 'px', borderRadius: '10px' } }), x = c.getContext('2d');
    const hue = U.el('div', { class: 'vrow' }), track = U.el('div', { class: 'track', style: { background: 'linear-gradient(90deg,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)' } }), knob = U.el('i');
    track.append(knob); hue.append(U.el('span', { text: 'Т' }), track, U.el('span'));
    body.append(c, hue);
    const draw = () => {
      const h = Colors.cur; x.setTransform(dpr, 0, 0, dpr, 0, 0);
      x.fillStyle = `hsl(${h.h},100%,50%)`; x.fillRect(0, 0, W, H);
      let g = x.createLinearGradient(0, 0, W, 0); g.addColorStop(0, '#fff'); g.addColorStop(1, 'rgba(255,255,255,0)'); x.fillStyle = g; x.fillRect(0, 0, W, H);
      g = x.createLinearGradient(0, 0, 0, H); g.addColorStop(0, 'rgba(0,0,0,0)'); g.addColorStop(1, '#000'); x.fillStyle = g; x.fillRect(0, 0, W, H);
      this.marker(x, h.s * W, (1 - h.v) * H, Colors.hex());
      knob.style.left = (h.h / 360 * 100) + '%';
    };
    this.drag(c, e => { const [px, py] = this.local(e, c); Colors.live(Object.assign({}, Colors.cur, { s: U.clamp(px / W, 0, 1), v: U.clamp(1 - py / H, 0, 1) })); });
    this.drag(track, e => { const [px, , w] = this.local(e, track); Colors.live(Object.assign({}, Colors.cur, { h: U.clamp(px / w, 0, 0.9999) * 360 })); });
    return draw;
  },
  harmony(body) {
    const S = 240, dpr = Math.min(2, window.devicePixelRatio || 1), R = S / 2;
    const modes = [['comp', 'Дополнительный'], ['split', 'Расщеплённый'], ['analog', 'Аналоговый'], ['triad', 'Триада'], ['tetrad', 'Тетрада']];
    const sel = U.el('select', { class: 'input', onchange: () => { this.harm = sel.value; draw(); } });
    for (const [id, n] of modes) sel.append(U.el('option', { value: id, text: n, selected: id === this.harm }));
    const c = U.el('canvas', { class: 'cwheel', width: S * dpr, height: S * dpr, style: { width: S + 'px', height: S + 'px', margin: '8px auto' } }), x = c.getContext('2d');
    const vrow = U.el('div', { class: 'vrow' }), track = U.el('div', { class: 'track' }), knob = U.el('i'); track.append(knob);
    vrow.append(U.el('span', { text: 'Я' }), track, U.el('span'));
    const sw = U.el('div', { class: 'harm' });
    body.append(sel, c, vrow, sw);
    let disc = null, discV = -1;
    const offs = () => ({ comp: [180], split: [150, 210], analog: [-30, 30], triad: [120, 240], tetrad: [90, 180, 270] })[this.harm];
    const draw = () => {
      const h = Colors.cur;
      if (!disc || discV !== h.v) {
        disc = U.canvas(S * dpr, S * dpr); const dx = disc.getContext('2d'), im = dx.createImageData(disc.width, disc.height), dd = im.data, RR = disc.width / 2;
        for (let y = 0; y < disc.height; y++) for (let X = 0; X < disc.width; X++) {
          const ux = X - RR, uy = y - RR, r = Math.hypot(ux, uy) / RR; if (r > 1) continue;
          const c2 = U.hsv2rgb((Math.atan2(uy, ux) * 180 / Math.PI + 90 + 360) % 360, r, h.v), i = (y * disc.width + X) * 4;
          dd[i] = c2.r; dd[i + 1] = c2.g; dd[i + 2] = c2.b; dd[i + 3] = 255;
        }
        dx.putImageData(im, 0, 0); discV = h.v;
      }
      x.setTransform(1, 0, 0, 1, 0, 0); x.clearRect(0, 0, c.width, c.height); x.drawImage(disc, 0, 0);
      x.setTransform(dpr, 0, 0, dpr, 0, 0);
      const pos = (hh, s) => { const a = (hh - 90) * Math.PI / 180; return [R + Math.cos(a) * s * (R - 2), R + Math.sin(a) * s * (R - 2)]; };
      sw.innerHTML = '';
      const all = [h.h].concat(offs().map(o => (h.h + o) % 360));
      all.forEach((hh, i) => {
        const [X, Y] = pos(hh, h.s), col = U.hex(U.hsv2rgb(hh, h.s, h.v));
        this.marker(x, X, Y, col); if (i) { x.strokeStyle = '#fff8'; x.beginPath(); x.moveTo(R, R); x.lineTo(X, Y); x.stroke(); }
        sw.append(U.el('button', { style: { background: col }, onclick: () => Colors.set({ h: hh, s: h.s, v: h.v }) }));
      });
      track.style.background = `linear-gradient(90deg,#000,${U.hex(U.hsv2rgb(h.h, h.s, 1))})`; knob.style.left = (h.v * 100) + '%';
    };
    this.drag(c, e => { const [px, py] = this.local(e, c), dx = px - R, dy = py - R; Colors.live(Object.assign({}, Colors.cur, { h: (Math.atan2(dy, dx) * 180 / Math.PI + 90 + 360) % 360, s: U.clamp(Math.hypot(dx, dy) / R, 0, 1) })); });
    this.drag(track, e => { const [px, , w] = this.local(e, track); Colors.live(Object.assign({}, Colors.cur, { v: U.clamp(px / w, 0, 1) })); });
    return draw;
  },
  values(body) {
    const rows = [
      ['Т', 'h', 360, h => `linear-gradient(90deg,${[0, 60, 120, 180, 240, 300, 360].map(a => U.hex(U.hsv2rgb(a, Math.max(0.5, h.s), Math.max(0.5, h.v)))).join(',')})`],
      ['Н', 's', 100, h => `linear-gradient(90deg,${U.hex(U.hsv2rgb(h.h, 0, h.v))},${U.hex(U.hsv2rgb(h.h, 1, h.v))})`],
      ['Я', 'v', 100, h => `linear-gradient(90deg,#000,${U.hex(U.hsv2rgb(h.h, h.s, 1))})`],
      ['R', 'r', 255, (h, c) => `linear-gradient(90deg,rgb(0,${c.g},${c.b}),rgb(255,${c.g},${c.b}))`],
      ['G', 'g', 255, (h, c) => `linear-gradient(90deg,rgb(${c.r},0,${c.b}),rgb(${c.r},255,${c.b}))`],
      ['B', 'b', 255, (h, c) => `linear-gradient(90deg,rgb(${c.r},${c.g},0),rgb(${c.r},${c.g},255))`]
    ];
    const items = [];
    const setV = (k, v) => {
      const h = Object.assign({}, Colors.cur);
      if (k === 'h') h.h = U.clamp(v, 0, 359.99); else if (k === 's') h.s = U.clamp(v / 100, 0, 1); else if (k === 'v') h.v = U.clamp(v / 100, 0, 1);
      else { const c = Colors.rgb(); c[k] = U.clamp(Math.round(v), 0, 255); const n = U.rgb2hsv(c.r, c.g, c.b); if (n.s === 0) n.h = h.h; Colors.live(n); return; }
      Colors.live(h);
    };
    for (const [lbl, k, max, bg] of rows) {
      const track = U.el('div', { class: 'track' }), knob = U.el('i'), inp = U.el('input', { type: 'number', min: 0, max, onchange: () => { Colors.beginLive(); setV(k, +inp.value); Colors.endLive(); } });
      track.append(knob); body.append(U.el('div', { class: 'vrow' }, U.el('span', { text: lbl }), track, inp));
      this.drag(track, e => { const [px, , w] = this.local(e, track); setV(k, U.clamp(px / w, 0, 1) * max); });
      items.push({ k, max, bg, track, knob, inp });
    }
    return () => {
      const h = Colors.cur, c = Colors.rgb();
      const val = { h: h.h, s: h.s * 100, v: h.v * 100, r: c.r, g: c.g, b: c.b };
      for (const it of items) { it.track.style.background = it.bg(h, c); it.knob.style.left = (val[it.k] / it.max * 100) + '%'; if (document.activeElement !== it.inp) it.inp.value = Math.round(val[it.k]); }
    };
  },
  palGrid(p, idx, compact) {
    const g = U.el('div', { class: 'pal' });
    const n = compact ? p.colors.length : Math.max(30, Math.ceil((p.colors.length + 1) / 10) * 10);
    for (let i = 0; i < n; i++) {
      const h = p.colors[i];
      if (h) {
        const b = U.el('button', { style: { background: h }, title: h });
        let t = 0;
        b.addEventListener('pointerdown', () => { t = setTimeout(() => { t = -1; if (!compact) { p.colors.splice(i, 1); Palettes.save(); UI.reopen('color'); U.toast('Цвет удалён из палитры'); } }, 600); });
        b.addEventListener('pointerup', () => { if (t !== -1) { clearTimeout(t); Colors.set(U.hex2rgb(h)); } });
        b.addEventListener('pointerleave', () => { if (t > 0) clearTimeout(t); });
        g.append(b);
      } else if (!compact && i === p.colors.length) g.append(U.el('button', { class: 'empty', text: '+', title: 'Добавить текущий цвет', onclick: () => { p.colors.push(Colors.hex()); Palettes.save(); UI.reopen('color'); } }));
      else if (!compact) g.append(U.el('button', { class: 'empty' }));
    }
    return g;
  },
  palettes(body) {
    Palettes.list.forEach((p, i) => {
      const head = U.el('div', { class: 'row' },
        U.el('b', { class: 'grow', text: p.name, style: { cursor: 'pointer' }, onclick: async () => { const n = await U.prompt('Название палитры', p.name); if (n) { p.name = n; Palettes.save(); UI.reopen('color'); } } }),
        U.el('button', { class: 'btn sm' + (Palettes.def === i ? ' primary' : ''), text: Palettes.def === i ? 'По умолчанию' : 'Сделать основной', onclick: () => { Palettes.def = i; Palettes.save(); UI.reopen('color'); } }),
        U.el('button', { class: 'tb', title: 'Удалить палитру', onclick: async () => { if (await U.confirm('Удалить палитру?', p.name, 'Удалить', true)) { Palettes.list.splice(i, 1); Palettes.def = 0; if (!Palettes.list.length) Palettes.list = Palettes.builtins(); Palettes.save(); UI.reopen('color'); } } }, U.icon('trash', 'sm')));
      body.append(head, this.palGrid(p, i, false));
    });
    body.append(U.el('div', { class: 'row', style: { flexWrap: 'wrap' } },
      U.el('button', { class: 'btn sm', text: '+ Новая палитра', onclick: () => { Palettes.list.push({ name: 'Палитра ' + (Palettes.list.length + 1), colors: [] }); Palettes.save(); UI.reopen('color'); } }),
      U.el('button', { class: 'btn sm', text: 'Из изображения…', onclick: async () => {
        const [f] = await U.pickFile('image/*'); if (!f) return;
        const img = await U.loadImage(f), cols = await Palettes.fromImage(img);
        Palettes.list.push({ name: f.name.replace(/\.[^.]+$/, '').slice(0, 24), colors: cols }); Palettes.save(); UI.reopen('color');
      } }),
      U.el('button', { class: 'btn sm', text: 'Из холста', onclick: async () => { const cols = await Palettes.fromImage(Engine.flatten()); Palettes.list.push({ name: 'Из «' + Engine.doc.name + '»', colors: cols }); Palettes.save(); UI.reopen('color'); } })));
  }
};
