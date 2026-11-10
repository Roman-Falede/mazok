'use strict';
/* Мазок — документ, слои, композитинг, история, выделение, операции со слоями */

const BLENDS = [
  ['source-over', 'Обычный', 'N'], ['multiply', 'Умножение', 'M'], ['screen', 'Экран', 'S'], ['overlay', 'Перекрытие', 'O'],
  ['darken', 'Затемнение', 'D'], ['lighten', 'Осветление', 'L'], ['color-dodge', 'Осветление основы', 'CD'], ['color-burn', 'Затемнение основы', 'CB'],
  ['lighter', 'Добавление', 'A'], ['hard-light', 'Жёсткий свет', 'HL'], ['soft-light', 'Мягкий свет', 'SL'], ['difference', 'Разница', 'Df'],
  ['exclusion', 'Исключение', 'E'], ['hue', 'Цветовой тон', 'H'], ['saturation', 'Насыщенность', 'Sa'], ['color', 'Цвет', 'C'], ['luminosity', 'Яркость', 'Lu']
];
const blendInfo = id => BLENDS.find(b => b[0] === id) || BLENDS[0];

class Layer {
  constructor(w, h, name) {
    this.id = U.uid(); this.name = name || 'Слой';
    this.canvas = U.canvas(w, h); this.ctx = this.canvas.getContext('2d');
    this.visible = true; this.opacity = 1; this.blend = 'source-over';
    this.alphaLock = false; this.clip = false; this.locked = false; this.reference = false; this.text = null;
    this.dirty = true; this.thumbV = 1; this._tv = 0; this._thumb = null;
  }
  props() { return { id: this.id, name: this.name, visible: this.visible, opacity: this.opacity, blend: this.blend, alphaLock: this.alphaLock, clip: this.clip, locked: this.locked, reference: this.reference, text: this.text }; }
  touch() { this.dirty = true; this.thumbV++; }
  thumb(mw = 52, mh = 40) {
    if (this._thumb && this._tv === this.thumbV) return this._thumb;
    const k = Math.min(mw / this.canvas.width, mh / this.canvas.height) * Math.min(2, window.devicePixelRatio || 1);
    const c = this._thumb && this._thumb.width === Math.round(this.canvas.width * k) ? this._thumb : U.canvas(this.canvas.width * k, this.canvas.height * k);
    const x = c.getContext('2d'); x.clearRect(0, 0, c.width, c.height); x.imageSmoothingQuality = 'high';
    x.drawImage(this.canvas, 0, 0, c.width, c.height);
    c.style.width = (this.canvas.width * k / Math.min(2, window.devicePixelRatio || 1)) + 'px';
    this._thumb = c; this._tv = this.thumbV; return c;
  }
}

class Doc {
  constructor(o) {
    this.id = o.id || U.uid(); this.name = o.name || 'Без названия';
    this.w = o.w; this.h = o.h; this.bg = o.bg || '#ffffff'; this.bgVisible = o.bgVisible !== false;
    this.layers = []; this.active = 0; this.layerSeq = 1;
    this.anim = Object.assign({ on: false, fps: 8, onion: 2, onionOpacity: 0.35, onionColor: true, mode: 'loop', holdFirst: false }, o.anim || {});
    this.guide = Object.assign(Guides.defaults(this.w, this.h), o.guide || {});
    this.stats = Object.assign({ strokes: 0, time: 0 }, o.stats || {});
    this.created = o.created || Date.now(); this.modified = o.modified || Date.now();
    this.tlCount = o.tlCount || 0; this.dpi = o.dpi || 72;
    const mem = (navigator.deviceMemory || 4) * 1073741824 * 0.2;
    this.maxLayers = U.clamp(Math.floor(mem / (this.w * this.h * 4)) - 6, 3, 250);
  }
  get layer() { return this.layers[this.active]; }
  nextName() { return 'Слой ' + (this.layerSeq++); }
  find(id) { return this.layers.find(l => l.id === id); }
}

/* ---------- композитинг ---------- */
const Engine = {
  doc: null, comp: null, cctx: null, _s: {}, override: null, overrideLayer: null,
  setDoc(doc) {
    this.doc = doc; this._s = {}; this.override = null; this.overrideLayer = null;
    this.comp = U.canvas(doc.w, doc.h); this.cctx = this.comp.getContext('2d');
    Sel.reset(); History.clear(); this.renderAll();
  },
  resized() {
    const d = this.doc; this._s = {};
    this.comp = U.canvas(d.w, d.h); this.cctx = this.comp.getContext('2d');
    Sel.reset(); this.renderAll(); View.fit();
  },
  scratch(name) {
    const d = this.doc; let s = this._s[name];
    if (!s || s.canvas.width !== d.w || s.canvas.height !== d.h) { const c = U.canvas(d.w, d.h); s = this._s[name] = { canvas: c, ctx: c.getContext('2d') }; }
    return s;
  },
  free(name) { delete this._s[name]; },
  src(L) { return L === this.overrideLayer && this.override ? this.override : L.canvas; },
  full() { return { x: 0, y: 0, w: this.doc.w, h: this.doc.h }; },
  renderAll() { if (this.doc) this.renderRect(this.full()); },
  renderRect(r) {
    const d = this.doc; if (!d) return;
    r = U.clipRect(r, d.w, d.h); if (!r) return;
    const c = this.cctx;
    c.save(); U.clipCtx(c, r); c.clearRect(r.x, r.y, r.w, r.h);
    this.compose(c, r, { bg: true, anim: d.anim.on });
    c.restore();
    View.requestDraw(); RefWin.dirty();
  },
  compose(c, r, o = {}) {
    const d = this.doc, layers = o.layers || d.layers;
    c.globalAlpha = 1; c.globalCompositeOperation = 'source-over';
    if (o.bg && d.bgVisible) { c.fillStyle = d.bg; c.fillRect(r.x, r.y, r.w, r.h); }
    if (o.anim) { this.composeAnim(c, r); return; }
    for (let i = 0; i < layers.length;) {
      const L = layers[i]; let j = i + 1;
      while (j < layers.length && layers[j].clip) j++;
      if (L.visible && L.opacity > 0) {
        let src = this.src(L);
        const clips = [];
        for (let k = i + 1; k < j; k++) if (layers[k].visible && layers[k].opacity > 0) clips.push(layers[k]);
        if (clips.length) {
          const g = this.scratch('group').ctx, t = this.scratch('clipt').ctx;
          g.save(); U.clipCtx(g, r); g.clearRect(r.x, r.y, r.w, r.h); g.globalCompositeOperation = 'source-over'; g.globalAlpha = 1;
          g.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
          for (const C of clips) {
            t.save(); U.clipCtx(t, r); t.clearRect(r.x, r.y, r.w, r.h);
            t.globalAlpha = C.opacity; t.globalCompositeOperation = 'source-over'; t.drawImage(this.src(C), r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
            t.globalAlpha = 1; t.globalCompositeOperation = 'destination-in'; t.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
            t.restore();
            g.globalCompositeOperation = C.blend; g.drawImage(this._s.clipt.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
          }
          g.restore(); src = this._s.group.canvas;
        }
        c.globalAlpha = L.opacity; c.globalCompositeOperation = L.blend;
        c.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
      }
      i = j;
    }
    c.globalAlpha = 1; c.globalCompositeOperation = 'source-over';
  },
  composeAnim(c, r) {
    const d = this.doc, L = d.layers, n = L.length;
    const f = Anim.playing ? Anim.frame : d.active;
    const draw = (layer, a) => { c.globalAlpha = a; c.drawImage(this.src(layer), r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h); };
    if (d.anim.holdFirst && f !== 0 && L[0] && L[0].visible) draw(L[0], L[0].opacity);
    if (!Anim.playing && d.anim.onion > 0) {
      for (let k = d.anim.onion; k >= 1; k--) for (const dir of [-1, 1]) {
        const idx = f + dir * k; if (idx < 0 || idx >= n || (d.anim.holdFirst && idx === 0)) continue;
        const F = L[idx]; if (!F.visible) continue;
        const a = d.anim.onionOpacity * (1 - (k - 1) / d.anim.onion * 0.6);
        if (d.anim.onionColor) {
          const t = this.scratch('onion').ctx;
          t.save(); U.clipCtx(t, r); t.clearRect(r.x, r.y, r.w, r.h); t.globalCompositeOperation = 'source-over'; t.globalAlpha = 1;
          t.drawImage(this.src(F), r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
          t.globalCompositeOperation = 'source-atop'; t.globalAlpha = 0.65; t.fillStyle = dir < 0 ? '#e5484d' : '#30a46c'; t.fillRect(r.x, r.y, r.w, r.h);
          t.restore();
          c.globalAlpha = a; c.drawImage(this._s.onion.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
        } else draw(F, a);
      }
    }
    if (L[f] && L[f].visible) draw(L[f], L[f].opacity);
    c.globalAlpha = 1;
  },
  /* плоское изображение (без анимационного режима) */
  flatten(o = {}) {
    const d = this.doc, c = U.canvas(d.w, d.h), x = c.getContext('2d');
    this.compose(x, this.full(), { bg: o.bg !== false, layers: o.layers });
    return c;
  },
  frame(i, bg = true) {
    const d = this.doc, c = U.canvas(d.w, d.h), x = c.getContext('2d');
    if (bg && d.bgVisible) { x.fillStyle = d.bg; x.fillRect(0, 0, d.w, d.h); }
    if (d.anim.holdFirst && i !== 0 && d.layers[0].visible) { x.globalAlpha = d.layers[0].opacity; x.drawImage(d.layers[0].canvas, 0, 0); }
    const L = d.layers[i]; if (L && L.visible) { x.globalAlpha = L.opacity; x.drawImage(L.canvas, 0, 0); }
    return c;
  },
  /* временная подмена активного слоя (рисование, трансформация, коррекция) */
  beginOverride(L) {
    const a = this.scratch('active');
    a.ctx.save(); a.ctx.setTransform(1, 0, 0, 1, 0, 0); a.ctx.globalAlpha = 1; a.ctx.globalCompositeOperation = 'copy';
    a.ctx.drawImage(L.canvas, 0, 0); a.ctx.restore();
    this.override = a.canvas; this.overrideLayer = L;
    return a;
  },
  endOverride() { const L = this.overrideLayer; this.override = null; this.overrideLayer = null; return L; },
  commit(rect, name) {
    const L = this.overrideLayer, d = this.doc;
    const r = U.clipRect(rect, d.w, d.h);
    if (!L || !r) { this.endOverride(); this.renderAll(); return; }
    const before = U.crop(L.canvas, r);
    L.ctx.clearRect(r.x, r.y, r.w, r.h);
    L.ctx.drawImage(this.override, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    const after = U.crop(L.canvas, r);
    this.endOverride(); L.touch();
    History.pixels(L, r, before, after, name);
    this.renderRect(r); UI.layersChanged();
  },
  applyBuffer(r, o) {
    const a = this._s.active.ctx, L = this.overrideLayer, buf = this._s.buffer.canvas;
    a.save(); U.clipCtx(a, r); a.clearRect(r.x, r.y, r.w, r.h);
    a.globalCompositeOperation = 'source-over'; a.globalAlpha = 1;
    a.drawImage(L.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    let src = buf;
    if (o.grain || Sel.active) {
      const t = this.scratch('tmp').ctx;
      t.save(); U.clipCtx(t, r); t.clearRect(r.x, r.y, r.w, r.h); t.globalCompositeOperation = 'source-over'; t.globalAlpha = 1;
      t.drawImage(buf, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
      if (o.grain) { t.globalCompositeOperation = 'destination-in'; t.fillStyle = o.grain; t.fillRect(r.x, r.y, r.w, r.h); }
      if (Sel.active) { t.globalCompositeOperation = 'destination-in'; t.drawImage(Sel.mask, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h); }
      t.restore(); src = this._s.tmp.canvas;
    }
    a.globalAlpha = o.opacity;
    if (o.mode === 'erase') { a.globalCompositeOperation = 'destination-out'; a.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h); }
    else if (o.mode === 'reveal' && Adjust.F) {
      a.globalCompositeOperation = 'destination-out'; a.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
      const t2 = this.scratch('tmp2').ctx;
      t2.save(); U.clipCtx(t2, r); t2.clearRect(r.x, r.y, r.w, r.h); t2.globalCompositeOperation = 'source-over'; t2.globalAlpha = 1;
      t2.drawImage(Adjust.F, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
      t2.globalCompositeOperation = 'destination-in'; t2.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h); t2.restore();
      a.globalCompositeOperation = 'lighter'; a.drawImage(this._s.tmp2.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    } else {
      a.globalCompositeOperation = L.alphaLock ? 'source-atop' : (o.blend || 'source-over');
      a.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    }
    a.restore();
  },
  /* dst = base·(1−mask) + F·mask */
  maskedMix(dst, base, F, mask) {
    dst.save(); dst.setTransform(1, 0, 0, 1, 0, 0); dst.globalAlpha = 1;
    dst.globalCompositeOperation = 'copy'; dst.drawImage(base, 0, 0);
    dst.globalCompositeOperation = 'destination-out'; dst.drawImage(mask, 0, 0);
    const t = this.scratch('tmp2').ctx;
    t.save(); t.globalCompositeOperation = 'copy'; t.drawImage(F, 0, 0); t.globalCompositeOperation = 'destination-in'; t.drawImage(mask, 0, 0); t.restore();
    dst.globalCompositeOperation = 'lighter'; dst.drawImage(this._s.tmp2.canvas, 0, 0);
    dst.restore();
  },
  layerChanged(L, r) { L.touch(); this.renderRect(r || this.full()); UI.layersChanged(); }
};

/* ---------- история ---------- */
const History = {
  list: [], pos: 0, bytes: 0, _grp: null,
  clear() { this.list = []; this.pos = 0; this.bytes = 0; UI.historyChanged(); },
  push(e) {
    if (this._grp) { this._grp.push(e); return; }
    while (this.list.length > this.pos) { const x = this.list.pop(); this.bytes -= x.bytes || 0; }
    this.list.push(e); this.pos++; this.bytes += e.bytes || 0;
    const max = Settings.historyMB * 1048576;
    while ((this.bytes > max || this.list.length > 250) && this.list.length > 1) { const x = this.list.shift(); this.pos--; this.bytes -= x.bytes || 0; }
    UI.historyChanged(); App.changed();
  },
  group(name, fn) {
    const outer = this._grp; this._grp = [];
    let r;
    try { r = fn(); } finally {
      const g = this._grp; this._grp = outer;
      if (g.length) this.push({ name, bytes: g.reduce((s, e) => s + (e.bytes || 0), 0), undo: () => { for (let i = g.length - 1; i >= 0; i--) g[i].undo(); }, redo: () => { for (const e of g) e.redo(); } });
    }
    return r;
  },
  undo() {
    if (App.blockHistory()) return;
    if (!this.pos) { U.toast('Нечего отменять'); return; }
    const e = this.list[--this.pos]; e.undo(); this.after(); U.toast('Отменено: ' + e.name, 1100);
  },
  redo() {
    if (App.blockHistory()) return;
    if (this.pos >= this.list.length) { U.toast('Нечего повторять'); return; }
    const e = this.list[this.pos++]; e.redo(); this.after(); U.toast('Повторено: ' + e.name, 1100);
  },
  after() { for (const L of Engine.doc.layers) L.touch(); Engine.renderAll(); Sel.refresh(); UI.layersChanged(); UI.historyChanged(); App.changed(); },
  pixels(L, r, before, after, name) {
    const put = c => { L.ctx.save(); L.ctx.globalCompositeOperation = 'source-over'; L.ctx.globalAlpha = 1; L.ctx.clearRect(r.x, r.y, r.w, r.h); L.ctx.drawImage(c, r.x, r.y); L.ctx.restore(); L.touch(); };
    this.push({ name, bytes: r.w * r.h * 8, undo: () => put(before), redo: () => put(after) });
  },
  /* записать изменение всего слоя, выполненное fn */
  layerOp(L, name, fn) {
    const before = U.clone(L.canvas);
    fn();
    const after = U.clone(L.canvas);
    L.touch();
    this.pixels(L, Engine.full(), before, after, name);
  }
};

/* ---------- выделение ---------- */
const Sel = {
  active: false, mask: null, mctx: null, overlay: null, path: null,
  reset() { this.active = false; this.mask = null; this.mctx = null; this.overlay = null; this.path = null; },
  ensure() {
    const d = Engine.doc;
    if (!this.mask || this.mask.width !== d.w || this.mask.height !== d.h) { this.mask = U.canvas(d.w, d.h); this.mctx = this.mask.getContext('2d'); }
  },
  snap() { return { active: this.active, mask: this.active ? U.clone(this.mask) : null, path: this.path }; },
  restore(s) {
    this.active = s.active; this.path = s.path;
    if (s.active) { this.ensure(); this.mctx.save(); this.mctx.globalCompositeOperation = 'copy'; this.mctx.drawImage(s.mask, 0, 0); this.mctx.restore(); }
    this.refresh();
  },
  change(name, fn) {
    const before = this.snap(); fn(); const after = this.snap();
    const d = Engine.doc;
    History.push({ name, bytes: d.w * d.h * 8, undo: () => this.restore(before), redo: () => this.restore(after) });
    this.refresh(); UI.selChanged();
  },
  prep(op) {
    this.ensure(); const m = this.mctx;
    m.setTransform(1, 0, 0, 1, 0, 0); m.globalAlpha = 1;
    if (op === 'replace' || !this.active) { m.globalCompositeOperation = 'source-over'; m.clearRect(0, 0, this.mask.width, this.mask.height); }
    m.globalCompositeOperation = op === 'subtract' && this.active ? 'destination-out' : (op === 'intersect' && this.active ? 'destination-in' : 'source-over');
    m.fillStyle = '#fff';
    return m;
  },
  applyPath(path, op) {
    const wasActive = this.active;
    const m = this.prep(op); m.fill(path); m.globalCompositeOperation = 'source-over';
    if (op === 'replace' || !wasActive) this.path = new Path2D(path);
    else { const p = new Path2D(this.path || undefined); p.addPath(path); this.path = p; }
    this.active = true;
  },
  applyMask(canvas, op, x = 0, y = 0) {
    const wasActive = this.active;
    const m = this.prep(op); m.drawImage(canvas, x, y); m.globalCompositeOperation = 'source-over';
    if (op === 'replace' || !wasActive) this.path = null;
    this.active = true;
  },
  all() { this.change('Выделить всё', () => { const d = Engine.doc, p = new Path2D(); p.rect(0, 0, d.w, d.h); this.applyPath(p, 'replace'); }); },
  clear() { if (!this.active) return; this.change('Снять выделение', () => { this.active = false; this.path = null; }); },
  invert() {
    if (!this.active) { this.all(); return; }
    this.change('Инвертировать выделение', () => {
      const d = Engine.doc, t = U.canvas(d.w, d.h), x = t.getContext('2d');
      x.fillStyle = '#fff'; x.fillRect(0, 0, d.w, d.h); x.globalCompositeOperation = 'destination-out'; x.drawImage(this.mask, 0, 0);
      this.mctx.globalCompositeOperation = 'copy'; this.mctx.drawImage(t, 0, 0); this.mctx.globalCompositeOperation = 'source-over';
    });
  },
  feather(px) {
    if (!this.active || px <= 0) return;
    this.change('Растушёвка', () => {
      const t = U.blur(this.mask, px);
      this.mctx.globalCompositeOperation = 'copy'; this.mctx.drawImage(t, 0, 0); this.mctx.globalCompositeOperation = 'source-over';
      this.path = null;
    });
  },
  fromLayer(L) {
    this.change('Выделить по слою', () => { this.prep('replace'); this.mctx.drawImage(L.canvas, 0, 0); this.path = null; this.active = true; });
  },
  bounds() { return this.active ? U.contentBounds(this.mask) : null; },
  refresh() {
    if (!this.active) { this.overlay = null; Engine.free('selov'); View.requestDraw(); return; }
    const o = Engine.scratch('selov'), x = o.ctx, d = Engine.doc;
    x.save(); x.globalCompositeOperation = 'copy'; x.fillStyle = 'rgba(12,12,22,0.45)'; x.fillRect(0, 0, d.w, d.h);
    x.globalCompositeOperation = 'destination-out'; x.drawImage(this.mask, 0, 0); x.restore();
    this.overlay = o.canvas; View.requestDraw();
  }
};

/* размытие холста (GPU-фильтр или запасной вариант) */
U.blur = (src, radius) => {
  const w = src.width, h = src.height, c = U.canvas(w, h), x = c.getContext('2d');
  if (radius <= 0) { x.drawImage(src, 0, 0); return c; }
  if (U.canFilter) { x.filter = `blur(${radius}px)`; x.drawImage(src, 0, 0); x.filter = 'none'; return c; }
  const id = U.readPixels(src); U.boxBlur(id.data, w, h, radius); x.putImageData(id, 0, 0); return c;
};
U.boxBlur = (d, w, h, radius) => {
  const r = Math.max(1, Math.round(radius / 2)), tmp = new Float32Array(d.length);
  for (let pass = 0; pass < 3; pass++) {
    for (let y = 0; y < h; y++) for (let ch = 0; ch < 4; ch++) {
      let acc = 0; const row = y * w;
      for (let x = -r; x <= r; x++) acc += d[(row + U.clamp(x, 0, w - 1)) * 4 + ch];
      for (let x = 0; x < w; x++) { tmp[(row + x) * 4 + ch] = acc / (2 * r + 1); acc += d[(row + Math.min(w - 1, x + r + 1)) * 4 + ch] - d[(row + Math.max(0, x - r)) * 4 + ch]; }
    }
    for (let x = 0; x < w; x++) for (let ch = 0; ch < 4; ch++) {
      let acc = 0;
      for (let y = -r; y <= r; y++) acc += tmp[(U.clamp(y, 0, h - 1) * w + x) * 4 + ch];
      for (let y = 0; y < h; y++) { d[(y * w + x) * 4 + ch] = acc / (2 * r + 1); acc += tmp[(Math.min(h - 1, y + r + 1) * w + x) * 4 + ch] - tmp[(Math.max(0, y - r) * w + x) * 4 + ch]; }
    }
  }
};

/* ---------- операции со слоями ---------- */
const Ops = {
  clipboard: null,
  get d() { return Engine.doc; },
  after() { Engine.renderAll(); UI.layersChanged(); },
  insert(L, at, name) {
    const d = this.d;
    if (d.layers.length >= d.maxLayers) { U.toast('Достигнут предел слоёв для холста этого размера'); return null; }
    const prev = d.active;
    const doIt = () => { d.layers.splice(at, 0, L); d.active = at; L.touch(); };
    doIt();
    History.push({ name: name || 'Новый слой', bytes: d.w * d.h * 4, undo: () => { d.layers.splice(d.layers.indexOf(L), 1); d.active = U.clamp(prev, 0, d.layers.length - 1); this.after(); }, redo: () => { doIt(); this.after(); } });
    this.after(); return L;
  },
  add(name, canvas, x = 0, y = 0) {
    const d = this.d, L = new Layer(d.w, d.h, name || d.nextName());
    if (canvas) L.ctx.drawImage(canvas, x, y);
    return this.insert(L, d.active + 1, name ? 'Добавлен слой' : 'Новый слой');
  },
  duplicate(i = this.d.active) {
    const d = this.d, S = d.layers[i]; if (!S) return;
    const L = new Layer(d.w, d.h, S.name + ' (копия)');
    Object.assign(L, { visible: S.visible, opacity: S.opacity, blend: S.blend, alphaLock: S.alphaLock, clip: S.clip, text: S.text ? Object.assign({}, S.text) : null });
    L.ctx.drawImage(S.canvas, 0, 0);
    return this.insert(L, i + 1, 'Дублировать слой');
  },
  remove(i = this.d.active) {
    const d = this.d, L = d.layers[i]; if (!L) return;
    if (d.layers.length === 1) { this.clear(L); return; }
    const prev = d.active;
    const doIt = () => { d.layers.splice(i, 1); d.active = U.clamp(i - 1 < 0 ? 0 : i - 1, 0, d.layers.length - 1); };
    doIt();
    History.push({ name: 'Удалить слой', bytes: 0, undo: () => { d.layers.splice(i, 0, L); d.active = prev; L.touch(); this.after(); }, redo: () => { doIt(); this.after(); } });
    this.after();
  },
  move(from, to) {
    const d = this.d; if (from === to || to < 0 || to >= d.layers.length) return;
    const doIt = (a, b) => { const [L] = d.layers.splice(a, 1); d.layers.splice(b, 0, L); d.active = b; };
    doIt(from, to);
    History.push({ name: 'Переместить слой', undo: () => { doIt(to, from); this.after(); }, redo: () => { doIt(from, to); this.after(); } });
    this.after();
  },
  select(i) { const d = this.d; if (i < 0 || i >= d.layers.length || i === d.active) return; Tools.commitPending(); d.active = i; UI.layersChanged(); if (d.anim.on) Engine.renderAll(); },
  setProps(L, props, name) {
    const before = {}; for (const k in props) before[k] = L[k];
    Object.assign(L, props); L.touch();
    History.push({ name: name || 'Свойства слоя', undo: () => { Object.assign(L, before); this.after(); }, redo: () => { Object.assign(L, props); this.after(); } });
    this.after();
  },
  propsCommit(L, before, name) {
    const after = {}; for (const k in before) after[k] = L[k];
    if (Object.keys(before).every(k => before[k] === after[k])) return;
    History.push({ name, undo: () => { Object.assign(L, before); this.after(); }, redo: () => { Object.assign(L, after); this.after(); } });
  },
  toggle(L, prop, name) { this.setProps(L, { [prop]: !L[prop] }, name); },
  mergeDown(i = this.d.active) {
    const d = this.d; if (i <= 0) { U.toast('Ниже нет слоя'); return; }
    const Up = d.layers[i], B = d.layers[i - 1];
    History.group('Объединить вниз', () => {
      History.layerOp(B, 'Объединить', () => {
        if (!Up.visible) return;
        let src = Up.canvas;
        if (Up.clip) { const t = U.clone(Up.canvas), x = t.getContext('2d'); x.globalCompositeOperation = 'destination-in'; x.drawImage(B.canvas, 0, 0); src = t; }
        B.ctx.save(); B.ctx.globalAlpha = Up.opacity; B.ctx.globalCompositeOperation = Up.blend; B.ctx.drawImage(src, 0, 0); B.ctx.restore();
      });
      if (B.text) this.setProps(B, { text: null });
      this.remove(i);
    });
    d.active = i - 1; this.after();
  },
  flatten() {
    const d = this.d; if (d.layers.length < 2) return;
    const old = d.layers.slice(), prevActive = d.active;
    const L = new Layer(d.w, d.h, 'Слой 1');
    Engine.compose(L.ctx, Engine.full(), { bg: false });
    const doIt = () => { d.layers = [L]; d.active = 0; L.touch(); };
    doIt();
    History.push({ name: 'Свести изображение', bytes: d.w * d.h * 4, undo: () => { d.layers = old.slice(); d.active = prevActive; this.after(); }, redo: () => { doIt(); this.after(); } });
    this.after();
  },
  clear(L = this.d.layer) {
    if (!this.editable(L)) return;
    History.layerOp(L, 'Очистить слой', () => {
      if (Sel.active) { L.ctx.save(); L.ctx.globalCompositeOperation = 'destination-out'; L.ctx.drawImage(Sel.mask, 0, 0); L.ctx.restore(); }
      else L.ctx.clearRect(0, 0, L.canvas.width, L.canvas.height);
    });
    if (L.text) L.text = null;
    this.after();
  },
  fill(L = this.d.layer, rgb = Colors.rgb()) {
    if (!this.editable(L)) return;
    const col = U.rgbStr(rgb);
    History.layerOp(L, 'Залить слой', () => {
      const x = L.ctx; x.save();
      x.globalCompositeOperation = L.alphaLock ? 'source-atop' : 'source-over';
      if (Sel.active) { const t = U.clone(Sel.mask), tx = t.getContext('2d'); tx.globalCompositeOperation = 'source-in'; tx.fillStyle = col; tx.fillRect(0, 0, t.width, t.height); x.drawImage(t, 0, 0); }
      else { x.fillStyle = col; x.fillRect(0, 0, L.canvas.width, L.canvas.height); }
      x.restore();
    });
    this.after();
  },
  invert(L = this.d.layer) {
    if (!this.editable(L)) return;
    History.layerOp(L, 'Инвертировать', () => {
      const id = U.readPixels(L.canvas), p = id.data;
      for (let i = 0; i < p.length; i += 4) { p[i] = 255 - p[i]; p[i + 1] = 255 - p[i + 1]; p[i + 2] = 255 - p[i + 2]; }
      const inv = U.fromImageData(id);
      if (Sel.active) Engine.maskedMix(L.ctx, U.clone(L.canvas), inv, Sel.mask);
      else { L.ctx.clearRect(0, 0, inv.width, inv.height); L.ctx.drawImage(inv, 0, 0); }
    });
    this.after();
  },
  editable(L) {
    if (!L) return false;
    if (L.locked) { U.toast('Слой заблокирован'); return false; }
    return true;
  },
  selectionCanvas(L, merged) {
    const src = merged ? Engine.flatten({ bg: false }) : L.canvas;
    const r = Sel.active ? Sel.bounds() : U.contentBounds(src);
    if (!r) return null;
    const c = U.crop(src, r);
    if (Sel.active) { const x = c.getContext('2d'); x.globalCompositeOperation = 'destination-in'; x.drawImage(Sel.mask, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h); }
    return { canvas: c, x: r.x, y: r.y };
  },
  copy(merged = false) {
    const L = this.d.layer, s = this.selectionCanvas(L, merged);
    if (!s) { U.toast('Нечего копировать'); return; }
    this.clipboard = s; U.toast(merged ? 'Скопировано (все слои)' : 'Скопировано');
    if (navigator.clipboard && window.ClipboardItem) U.toBlob(s.canvas).then(b => navigator.clipboard.write([new ClipboardItem({ 'image/png': b })])).catch(() => { });
  },
  cut() {
    const L = this.d.layer; if (!this.editable(L)) return;
    const s = this.selectionCanvas(L, false); if (!s) { U.toast('Нечего вырезать'); return; }
    this.clipboard = s; this.clear(L); U.toast('Вырезано');
  },
  paste() {
    const s = this.clipboard; if (!s) { U.toast('Буфер обмена пуст'); return; }
    this.add('Вставка', s.canvas, s.x, s.y);
  },
  copyPaste(cut) {
    const L = this.d.layer, s = this.selectionCanvas(L, false);
    if (!s) { U.toast('Нечего копировать'); return; }
    History.group(cut ? 'Вырезать и вставить' : 'Копировать и вставить', () => {
      if (cut && this.editable(L)) this.clear(L);
      this.add(cut ? 'Вырезанный фрагмент' : 'Копия фрагмента', s.canvas, s.x, s.y);
    });
    if (Sel.active) Sel.clear();
  },
  async insertImage(src, name = 'Изображение') {
    const d = this.d;
    const k = Math.min(1, d.w / src.width, d.h / src.height);
    const w = Math.max(1, Math.round(src.width * k)), h = Math.max(1, Math.round(src.height * k));
    const c = U.canvas(w, h), x = c.getContext('2d'); x.imageSmoothingQuality = 'high'; x.drawImage(src, 0, 0, w, h);
    const L = this.add(name, c, Math.round((d.w - w) / 2), Math.round((d.h - h) / 2));
    if (L) { Sel.active && Sel.clear(); Tools.set('transform'); }
  },
  /* операции со всем холстом */
  docOp(name, nw, nh, draw) {
    const d = this.d;
    Tools.commitPending();
    const old = { w: d.w, h: d.h, cans: d.layers.map(L => [L, L.canvas, L.ctx]) };
    const neu = { w: nw, h: nh, cans: d.layers.map(L => { const c = U.canvas(nw, nh), x = c.getContext('2d'); x.imageSmoothingQuality = 'high'; draw(x, L.canvas); return [L, c, x]; }) };
    const apply = s => { d.w = s.w; d.h = s.h; for (const [L, c, x] of s.cans) { L.canvas = c; L.ctx = x; L.touch(); } d.maxLayers = Math.max(d.layers.length, d.maxLayers); Engine.resized(); UI.layersChanged(); };
    apply(neu);
    History.push({ name, bytes: nw * nh * 4 * d.layers.length, undo: () => apply(old), redo: () => apply(neu) });
  },
  resizeCanvas(nw, nh, ox, oy, scale) {
    this.docOp(scale ? 'Размер изображения' : 'Размер холста', nw, nh, (x, src) => scale ? x.drawImage(src, 0, 0, nw, nh) : x.drawImage(src, ox, oy));
  },
  rotateCanvas(cw) {
    const d = this.d;
    this.docOp('Повернуть холст', d.h, d.w, (x, src) => { x.translate(d.h / 2, d.w / 2); x.rotate(cw ? Math.PI / 2 : -Math.PI / 2); x.drawImage(src, -src.width / 2, -src.height / 2); });
  },
  flipCanvas(horiz) {
    const d = this.d;
    this.docOp(horiz ? 'Отразить холст' : 'Отразить холст по вертикали', d.w, d.h, (x, src) => { if (horiz) { x.translate(d.w, 0); x.scale(-1, 1); } else { x.translate(0, d.h); x.scale(1, -1); } x.drawImage(src, 0, 0); });
  },
  flipLayer(horiz, L = this.d.layer) {
    if (!this.editable(L)) return;
    History.layerOp(L, 'Отразить слой', () => {
      const t = U.clone(L.canvas), x = L.ctx; x.save(); x.clearRect(0, 0, t.width, t.height);
      if (horiz) { x.translate(t.width, 0); x.scale(-1, 1); } else { x.translate(0, t.height); x.scale(1, -1); }
      x.drawImage(t, 0, 0); x.restore();
    });
    this.after();
  },
  /* текстовые слои */
  renderText(L) {
    const t = L.text, x = L.ctx;
    x.clearRect(0, 0, L.canvas.width, L.canvas.height);
    x.save();
    x.font = `${t.italic ? 'italic ' : ''}${t.bold ? '700 ' : '400 '}${t.size}px ${t.font}`;
    x.fillStyle = t.color; x.textBaseline = 'top'; x.textAlign = t.align || 'left';
    if ('letterSpacing' in x) x.letterSpacing = (t.spacing || 0) + 'px';
    const lines = String(t.str).split('\n'), lh = t.size * (t.lh || 1.2);
    let w = 0;
    lines.forEach((ln, i) => { x.fillText(ln, t.x, t.y + i * lh); w = Math.max(w, x.measureText(ln).width); });
    if (t.stroke > 0) { x.lineWidth = t.stroke; x.strokeStyle = t.strokeColor || '#000'; x.lineJoin = 'round'; lines.forEach((ln, i) => x.strokeText(ln, t.x, t.y + i * lh)); }
    x.restore();
    const ax = t.align === 'center' ? t.x - w / 2 : t.align === 'right' ? t.x - w : t.x;
    t.bounds = { x: ax - 4, y: t.y - 4, w: w + 8, h: lines.length * lh + 8 };
  },
  addText(t) {
    const d = this.d, L = new Layer(d.w, d.h, (t.str.split('\n')[0] || 'Текст').slice(0, 24));
    L.text = t; this.renderText(L);
    return this.insert(L, d.active + 1, 'Текст');
  },
  setText(L, t) {
    const before = U.clone(L.canvas), oldT = L.text, oldName = L.name;
    L.text = t; L.name = (t.str.split('\n')[0] || 'Текст').slice(0, 24); this.renderText(L);
    const after = U.clone(L.canvas), r = Engine.full();
    const put = (c, tx, nm) => { L.ctx.clearRect(0, 0, c.width, c.height); L.ctx.drawImage(c, 0, 0); L.text = tx; L.name = nm; L.touch(); };
    History.push({ name: 'Изменить текст', bytes: r.w * r.h * 8, undo: () => put(before, oldT, oldName), redo: () => put(after, t, L.name) });
    L.touch(); this.after();
  },
  rasterize(L) { if (L.text) { const t = L.text; L.text = null; History.push({ name: 'Растеризовать текст', undo: () => { L.text = t; UI.layersChanged(); }, redo: () => { L.text = null; UI.layersChanged(); } }); UI.layersChanged(); } }
};
