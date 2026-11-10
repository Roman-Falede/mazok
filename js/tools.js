'use strict';
/* Мазок — инструменты: кисть, палец, ластик, клон, заливка, пипетка, выделение, трансформация, текст, пластика */

const Tools = {
  cur: 'brush', prev: 'brush', map: {},
  state: {
    brush: { brushId: 'pencil_hb', size: 0.22, opacity: 1 },
    smudge: { brushId: 'oil', size: 0.3, opacity: 0.7 },
    eraser: { brushId: 'air_soft', size: 0.3, opacity: 1 },
    clone: { brushId: 'air_mid', size: 0.3, opacity: 1 }
  },
  ts(id = this.cur) { return this.state[id] || this.state.brush; },
  set(id) {
    if (id === this.cur) { if (['brush', 'smudge', 'eraser', 'clone'].includes(id)) UI.toggleBrushLib(); return; }
    const old = this.map[this.cur];
    if (old && old.deactivate) old.deactivate(id);
    if (!['picker'].includes(this.cur)) this.prev = this.cur;
    this.cur = id;
    const t = this.map[id];
    if (t && t.activate && t.activate() === false) { this.cur = this.prev === id ? 'brush' : this.prev; }
    UI.toolChanged(); View.requestDraw();
  },
  get t() { return this.map[this.cur] || {}; },
  down(p) { this.t.down && this.t.down(p); },
  move(p) { this.t.move && this.t.move(p); },
  up(p) { this.t.up && this.t.up(p); },
  cancel() { this.t.cancel && this.t.cancel(); },
  overlay(ctx) { this.t.overlay && this.t.overlay(ctx); },
  isPaint() { return ['brush', 'smudge', 'eraser', 'clone'].includes(this.cur); },
  busy() { return !!(Paint.st || Xform.s || Liquify.s || Fill.st); },
  commitPending() { if (Xform.s) Xform.commit(); if (Liquify.s) Liquify.apply(); if (Adjust.s) Adjust.apply(); },
  cursorSize() { const ts = this.ts(); return Brushes.sizePx(Brushes.get(ts.brushId), ts.size); },
  brush() { return Brushes.get(this.ts().brushId); }
};

/* ---------- кисть / палец / ластик / клон ---------- */
const Paint = {
  st: null, lastP: null,
  begin(mode, p) {
    const d = Engine.doc, L = d && d.layer;
    if (!L) return;
    if (!L.visible) { U.toast('Слой скрыт'); return; }
    if (L.locked) { U.toast('Слой заблокирован'); return; }
    if (mode === 'clone' && (!Clone.src || p.alt)) { Clone.setSource(p); return; }
    if (Adjust.s && Adjust.brushMode && mode !== 'brush') return;
    if (L.text) Ops.rasterize(L);
    const ts = Tools.ts(mode), brush = Brushes.get(ts.brushId);
    const size = Brushes.sizePx(brush, ts.size);
    Engine.beginOverride(L);
    const buf = Engine.scratch('buffer');
    if (mode !== 'smudge') { buf.ctx.setTransform(1, 0, 0, 1, 0, 0); buf.ctx.globalAlpha = 1; buf.ctx.globalCompositeOperation = 'source-over'; buf.ctx.clearRect(0, 0, d.w, d.h); }
    const st = this.st = {
      mode, L, brush, size, opacity: ts.opacity, total: null, raf: 0, startP: p, started: false, shape: null,
      grain: brush.grain || brush.grainImage ? Grains.pattern(Engine.scratch('tmp').ctx, Brushes.grainId(brush), brush.grainScale, brush.grainStrength) : null,
      applyMode: mode === 'eraser' ? 'erase' : (Adjust.s && Adjust.brushMode ? 'reveal' : 'paint')
    };
    let onStamp = null;
    if (mode === 'smudge') onStamp = Smudge.stamper(st);
    else if (mode === 'clone') onStamp = Clone.stamper(st, buf.ctx);
    st.stroke = new Stroke({ brush, size, ctx: buf.ctx, color: Colors.rgb(), mirrors: Guides.mirrors(), onStamp });
    st.constraint = Guides.constraint(p);
    st.stroke.add(p.x, p.y, p.p, p.tilt);
    this.lastP = p;
    this.schedule();
    if (Settings.holdPick && mode !== 'clone' && !(Adjust.s && Adjust.brushMode)) st.holdTimer = setTimeout(() => this.holdPick(), Settings.holdDelay);
  },
  move(p) {
    const st = this.st; if (!st) return;
    this.lastP = p;
    if (st.picking) { Picker.move(p); return; }
    if (!st.started && Math.hypot(p.sx - st.startP.sx, p.sy - st.startP.sy) > 7) { st.started = true; clearTimeout(st.holdTimer); }
    if (st.shape) { if (st.shape === 'line') { const a = st.stroke.pts[0]; st.stroke.replace(QuickShape.line([a.x, a.y], [p.x, p.y], a.p)); this.rerender(); } return; }
    let x = p.x, y = p.y;
    if (st.constraint) [x, y] = st.constraint(x, y);
    st.stroke.add(x, y, p.p, p.tilt);
    if (Settings.quickShape && st.started && st.mode !== 'smudge' && st.mode !== 'clone') {
      if (!st.qs || Math.hypot(p.sx - st.qs[0], p.sy - st.qs[1]) > 4) {
        st.qs = [p.sx, p.sy]; clearTimeout(st.qsTimer);
        st.qsTimer = setTimeout(() => this.quickShape(), Settings.quickShapeDelay);
      }
    }
    this.schedule();
  },
  schedule() { const st = this.st; if (st && !st.raf) st.raf = requestAnimationFrame(() => { st.raf = 0; this.flush(); }); },
  flush() {
    const st = this.st; if (!st || st.picking) return;
    const r0 = st.stroke.dirty; st.stroke.dirty = null;
    if (!r0) return;
    const d = Engine.doc, r = U.clipRect(U.inflate(r0, 1), d.w, d.h);
    if (!r) return;
    st.total = U.union(st.total, r);
    if (st.mode !== 'smudge') Engine.applyBuffer(r, { mode: st.applyMode, opacity: st.opacity, grain: st.grain, blend: st.brush.blend });
    Engine.renderRect(r);
  },
  rerender() {
    const st = this.st, buf = Engine.scratch('buffer').ctx;
    if (st.total) buf.clearRect(st.total.x, st.total.y, st.total.w, st.total.h);
    st.stroke.rerender();
    st.stroke.dirty = U.union(st.stroke.dirty, st.total);
    this.flush();
  },
  end() {
    const st = this.st; if (!st) return;
    clearTimeout(st.holdTimer); clearTimeout(st.qsTimer);
    if (st.picking) { Picker.end(this.lastP); this.st = null; return; }
    cancelAnimationFrame(st.raf); st.raf = 0;
    if (!st.shape) {
      if (st.mode !== 'smudge' && st.mode !== 'clone' && st.brush.taperEnd > 0 && st.stroke.pts.length > 1) { st.stroke.catchUp(); this.rerender(); }
      else st.stroke.end();
    }
    this.flush();
    const names = { brush: 'Мазок кистью', smudge: 'Растушёвка пальцем', eraser: 'Стирание', clone: 'Клонирование' };
    if (st.total) { Engine.commit(st.total, Adjust.brushMode && Adjust.s ? Adjust.s.f.name : names[st.mode]); Engine.doc.stats.strokes++; if (st.mode === 'brush') Colors.used(); }
    else { Engine.endOverride(); Engine.renderAll(); }
    this.st = null;
  },
  cancel() {
    const st = this.st; if (!st) return;
    clearTimeout(st.holdTimer); clearTimeout(st.qsTimer); cancelAnimationFrame(st.raf);
    if (st.picking) Picker.hide();
    else { Engine.endOverride(); if (st.total) Engine.renderRect(st.total); }
    this.st = null;
  },
  holdPick() {
    const st = this.st; if (!st || st.started) return;
    Engine.endOverride(); if (st.total) Engine.renderRect(st.total);
    this.st = { picking: true };
    Picker.start(this.lastP);
  },
  quickShape() {
    const st = this.st; if (!st || st.shape || st.picking) return;
    const res = QuickShape.detect(st.stroke.pts);
    if (!res) return;
    st.shape = res.type; st.stroke.replace(res.pts); this.rerender();
    UI.hint(res.label + (res.type === 'line' ? ' — двигайте, чтобы изменить' : ''));
  }
};
const paintTool = mode => ({
  down: p => Paint.begin(mode, p), move: p => Paint.move(p), up: p => Paint.end(p), cancel: () => Paint.cancel(),
  activate() { if (mode === 'clone') UI.showOptbar('clone'); },
  deactivate() { if (mode === 'clone') UI.hideOptbar(); },
  overlay: ctx => { if (mode === 'clone') Clone.overlay(ctx); }
});

const Smudge = {
  stamper(st) {
    const A = Engine.scratch('active'), L = st.L, picks = [], tmp = U.canvas(4, 4), tc = tmp.getContext('2d'), mk = U.canvas(4, 4), mc = mk.getContext('2d');
    const img = Brushes.tipImage(st.brush), b = st.brush;
    return (x, y, s, a, ang, mi) => {
      const D = Math.max(2, Math.ceil(s));
      const k = U.clamp(st.opacity * b.smudge * (a / Math.max(0.01, b.flow)), 0, 1);
      const x0 = Math.round(x - D / 2), y0 = Math.round(y - D / 2);
      let pk = picks[mi];
      if (!pk) { const c = U.canvas(D, D), cx = c.getContext('2d'); cx.drawImage(A.canvas, x0, y0, D, D, 0, 0, D, D); pk = picks[mi] = { c, cx, D }; return; }
      if (pk.D !== D) { const c = U.canvas(D, D), cx = c.getContext('2d'); cx.drawImage(pk.c, 0, 0, D, D); pk.c = c; pk.cx = cx; pk.D = D; }
      if (tmp.width !== D) { tmp.width = tmp.height = D; mk.width = mk.height = D; }
      mc.clearRect(0, 0, D, D); mc.globalCompositeOperation = 'source-over'; Tips.stampMask(mc, b, D / 2, D / 2, s, ang, img);
      if (Sel.active) { mc.globalCompositeOperation = 'destination-in'; mc.drawImage(Sel.mask, x0, y0, D, D, 0, 0, D, D); }
      tc.clearRect(0, 0, D, D); tc.globalCompositeOperation = 'source-over'; tc.drawImage(pk.c, 0, 0);
      tc.globalCompositeOperation = 'destination-in'; tc.drawImage(mk, 0, 0);
      const ac = A.ctx; ac.save(); ac.globalAlpha = k;
      if (L.alphaLock) { ac.globalCompositeOperation = 'source-atop'; ac.drawImage(tmp, x0, y0); }
      else { ac.globalCompositeOperation = 'destination-out'; ac.drawImage(mk, x0, y0); ac.globalCompositeOperation = 'lighter'; ac.drawImage(tmp, x0, y0); }
      ac.restore();
      const rate = 0.12 + 0.5 * (1 - b.smudge);
      const px = pk.cx; px.globalAlpha = rate; px.globalCompositeOperation = 'destination-out'; px.fillRect(0, 0, D, D);
      px.globalCompositeOperation = 'lighter'; px.drawImage(A.canvas, x0, y0, D, D, 0, 0, D, D);
      px.globalAlpha = 1; px.globalCompositeOperation = 'source-over';
    };
  }
};

const Clone = {
  src: null, off: null,
  setSource(p) { this.src = { x: p.x, y: p.y }; this.off = null; UI.hint('Источник задан — теперь рисуйте'); View.requestDraw(); },
  stamper(st, bctx) {
    if (!this.off || !Settings.cloneAligned) this.off = { x: this.src.x - st.startP.x, y: this.src.y - st.startP.y };
    const L = st.L, tmp = U.canvas(4, 4), tc = tmp.getContext('2d'), img = Brushes.tipImage(st.brush), off = this.off;
    return (x, y, s, a, ang) => {
      const D = Math.max(2, Math.ceil(s)); if (tmp.width !== D) tmp.width = tmp.height = D;
      tc.clearRect(0, 0, D, D); tc.globalCompositeOperation = 'source-over';
      tc.drawImage(L.canvas, x + off.x - D / 2, y + off.y - D / 2, D, D, 0, 0, D, D);
      tc.globalCompositeOperation = 'destination-in'; Tips.stampMask(tc, st.brush, D / 2, D / 2, s, ang, img);
      bctx.globalAlpha = Math.min(1, a); bctx.drawImage(tmp, x - D / 2, y - D / 2);
    };
  },
  overlay(ctx) {
    if (!this.src) return;
    let p = this.src;
    if (Paint.st && Paint.lastP && this.off) p = { x: Paint.lastP.x + this.off.x, y: Paint.lastP.y + this.off.y };
    else if (View.hover && this.off) { const [x, y] = View.toDoc(View.hover.x, View.hover.y); p = { x: x + this.off.x, y: y + this.off.y }; }
    const [X, Y] = View.toScreen(p.x, p.y);
    ctx.lineWidth = 2; ctx.strokeStyle = '#000a'; ctx.beginPath(); ctx.arc(X, Y, 12, 0, 7); ctx.moveTo(X - 18, Y); ctx.lineTo(X + 18, Y); ctx.moveTo(X, Y - 18); ctx.lineTo(X, Y + 18); ctx.stroke();
    ctx.lineWidth = 1; ctx.strokeStyle = '#fff'; ctx.stroke();
  }
};

/* ---------- заливка ---------- */
function floodMask(id, sx, sy, tol, expand) {
  const w = id.width, h = id.height, d = id.data, mask = new Uint8Array(w * h);
  const i0 = (sy * w + sx) * 4, r0 = d[i0], g0 = d[i0 + 1], b0 = d[i0 + 2], a0 = d[i0 + 3], t = tol * 2.55;
  const close = a0 === 0 ? i => d[i * 4 + 3] <= t
    : i => { const j = i * 4; return Math.abs(d[j] - r0) <= t && Math.abs(d[j + 1] - g0) <= t && Math.abs(d[j + 2] - b0) <= t && Math.abs(d[j + 3] - a0) <= t; };
  let minx = sx, maxx = sx, miny = sy, maxy = sy;
  const stack = [sx, sy];
  while (stack.length) {
    const y = stack.pop(); let x = stack.pop(), i = y * w + x;
    if (mask[i] || !close(i)) continue;
    while (x > 0 && !mask[i - 1] && close(i - 1)) { x--; i--; }
    let up = false, dn = false;
    if (x < minx) minx = x; if (y < miny) miny = y; if (y > maxy) maxy = y;
    while (x < w && !mask[i] && close(i)) {
      mask[i] = 1;
      if (y > 0) { const u = i - w; if (!mask[u] && close(u)) { if (!up) { stack.push(x, y - 1); up = true; } } else up = false; }
      if (y < h - 1) { const v = i + w; if (!mask[v] && close(v)) { if (!dn) { stack.push(x, y + 1); dn = true; } } else dn = false; }
      x++; i++;
    }
    if (x - 1 > maxx) maxx = x - 1;
  }
  let b = { x: minx, y: miny, w: maxx - minx + 1, h: maxy - miny + 1 };
  if (expand > 0) {
    const e = Math.round(expand); b = U.clipRect(U.inflate(b, e), w, h);
    const tmp = new Uint8Array(w * h);
    for (let y = b.y; y < b.y + b.h; y++) for (let x = b.x; x < b.x + b.w; x++) {
      let v = 0; for (let k = -e; k <= e && !v; k++) { const xx = x + k; if (xx >= 0 && xx < w && mask[y * w + xx]) v = 1; } tmp[y * w + x] = v;
    }
    for (let y = b.y; y < b.y + b.h; y++) for (let x = b.x; x < b.x + b.w; x++) {
      let v = 0; for (let k = -e; k <= e && !v; k++) { const yy = y + k; if (yy >= 0 && yy < h && tmp[yy * w + x]) v = 1; } mask[y * w + x] = v;
    }
  }
  return { mask, bounds: b };
}
function maskCanvas(mask, w, b, rgb, alpha = 255) {
  const c = U.canvas(b.w, b.h), x = c.getContext('2d'), im = x.createImageData(b.w, b.h), p = im.data;
  for (let y = 0; y < b.h; y++) for (let X = 0; X < b.w; X++) {
    if (mask[(y + b.y) * w + X + b.x]) { const j = (y * b.w + X) * 4; p[j] = rgb.r; p[j + 1] = rgb.g; p[j + 2] = rgb.b; p[j + 3] = alpha; }
  }
  x.putImageData(im, 0, 0); return c;
}

const Fill = {
  st: null,
  source(L) {
    const d = Engine.doc;
    if (Settings.fillSource === 'all') return Engine.flatten({ bg: false });
    if (Settings.fillSource === 'reference') { const R = d.layers.find(l => l.reference); return R ? R.canvas : Engine.flatten({ bg: false }); }
    return L.canvas;
  },
  activate() { UI.showOptbar('fill'); }, deactivate() { UI.hideOptbar(); },
  down(p) {
    const d = Engine.doc, L = d.layer;
    if (!Ops.editable(L)) return;
    if (!L.visible) { U.toast('Слой скрыт'); return; }
    const x = Math.floor(p.x), y = Math.floor(p.y);
    if (x < 0 || y < 0 || x >= d.w || y >= d.h) return;
    if (L.text) Ops.rasterize(L);
    this.st = { L, data: U.readPixels(this.source(L)), x, y, sx: p.sx, base: Settings.fillTol, tol: Settings.fillTol, rgb: Colors.rgb(), prev: null };
    Engine.beginOverride(L); this.compute();
  },
  compute() {
    const st = this.st, d = Engine.doc;
    const { mask, bounds } = floodMask(st.data, st.x, st.y, st.tol, Settings.fillExpand);
    const c = maskCanvas(mask, d.w, bounds, st.rgb);
    if (Sel.active) { const x = c.getContext('2d'); x.globalCompositeOperation = 'destination-in'; x.drawImage(Sel.mask, bounds.x, bounds.y, bounds.w, bounds.h, 0, 0, bounds.w, bounds.h); }
    const r = U.union(st.prev, bounds), A = Engine._s.active.ctx;
    A.save(); U.clipCtx(A, r); A.clearRect(r.x, r.y, r.w, r.h); A.globalCompositeOperation = 'source-over'; A.drawImage(st.L.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    A.globalCompositeOperation = st.L.alphaLock ? 'source-atop' : 'source-over'; A.drawImage(c, bounds.x, bounds.y); A.restore();
    st.prev = bounds; st.total = U.union(st.total, bounds);
    Engine.renderRect(r);
  },
  move(p) {
    const st = this.st; if (!st) return;
    const tol = Math.round(U.clamp(st.base + (p.sx - st.sx) / 4, 0, 100));
    if (tol !== st.tol) { st.tol = tol; UI.hint('Порог заливки: ' + tol + '%'); if (!st.pend) { st.pend = true; requestAnimationFrame(() => { if (this.st) { this.st.pend = false; this.compute(); } }); } }
  },
  up() {
    const st = this.st; if (!st) return;
    if (st.tol !== st.base) { Settings.fillTol = st.tol; Settings.save(); UI.optbarRefresh(); }
    Engine.commit(st.total, 'Заливка'); this.st = null;
  },
  cancel() { if (this.st) { Engine.endOverride(); Engine.renderAll(); this.st = null; } },
  dropAt(sx, sy, rgb) {
    const [x, y] = View.toDoc(sx, sy), d = Engine.doc, L = d.layer;
    if (!Ops.editable(L)) return;
    if (x < 0 || y < 0 || x >= d.w || y >= d.h) return;
    this.st = { L, data: U.readPixels(this.source(L)), x: Math.floor(x), y: Math.floor(y), sx, base: Settings.fillTol, tol: Settings.fillTol, rgb, prev: null };
    Engine.beginOverride(L); this.compute(); this.up();
  }
};

/* ---------- пипетка ---------- */
const Picker = {
  el: null, rc: null,
  sample(p) {
    const d = Engine.doc, x = Math.floor(p.x), y = Math.floor(p.y);
    if (!d || x < 0 || y < 0 || x >= d.w || y >= d.h) return null;
    if (!this.rc) { const c = U.canvas(1, 1); this.rc = c.getContext('2d', { willReadFrequently: true }); }
    const src = Settings.pickSource === 'layer' ? d.layer.canvas : Engine.comp;
    this.rc.clearRect(0, 0, 1, 1); this.rc.drawImage(src, x, y, 1, 1, 0, 0, 1, 1);
    const v = this.rc.getImageData(0, 0, 1, 1).data;
    if (v[3] < 8) return null;
    const a = v[3] / 255, bg = U.hex2rgb(d.bg || '#ffffff');
    return { r: Math.round(v[0] * a + bg.r * (1 - a)), g: Math.round(v[1] * a + bg.g * (1 - a)), b: Math.round(v[2] * a + bg.b * (1 - a)) };
  },
  start(p) { this.el = U.$('#loupe'); this.el.style.display = 'block'; this.move(p); },
  move(p) {
    if (!this.el) return;
    const c = this.sample(p);
    this.el.style.left = p.sx + 'px'; this.el.style.top = p.sy + 'px';
    this.el.querySelector('.new').style.background = c ? U.rgbStr(c) : 'transparent';
    this.el.querySelector('.old').style.background = Colors.hex();
    this.cur = c;
  },
  end(p) { const c = this.sample(p) || this.cur; this.hide(); if (c) Colors.set(c); },
  hide() { if (this.el) this.el.style.display = 'none'; this.el = null; },
  tool: {
    down: p => Picker.start(p), move: p => Picker.move(p),
    up: p => { Picker.end(p); Tools.set(Tools.prev || 'brush'); }, cancel: () => Picker.hide()
  }
};

/* ---------- выделение ---------- */
const SelectTool = {
  shape: 'rect', op: 'replace', drag: null, poly: [], tol: 20, wand: null,
  activate() { UI.showOptbar('select'); }, deactivate() { this.poly = []; this.drag = null; this.wand = null; UI.hideOptbar(); },
  opFor(p) { return p.shift ? 'add' : p.alt ? 'subtract' : this.op; },
  down(p) {
    const op = this.opFor(p);
    if (this.shape === 'wand') { this.wandStart(p, op); return; }
    if (this.shape === 'lasso') {
      if (this.poly.length > 2) { const [X, Y] = View.toScreen(this.poly[0][0], this.poly[0][1]); if (Math.hypot(X - p.sx, Y - p.sy) < 18) { this.finishPoly(op); this.drag = { closed: true }; return; } }
      if (!this.poly.length) this.poly.push([p.x, p.y]);
      this.drag = { moved: false, sx: p.sx, sy: p.sy, op, start: this.poly.length };
      return;
    }
    this.drag = { x0: p.x, y0: p.y, x1: p.x, y1: p.y, op, sx: p.sx, sy: p.sy };
  },
  move(p) {
    const g = this.drag;
    if (this.wand) { this.wandMove(p); return; }
    if (!g || g.closed) return;
    if (this.shape === 'lasso') {
      if (Math.hypot(p.sx - g.sx, p.sy - g.sy) > 4) g.moved = true;
      if (g.moved) { const l = this.poly[this.poly.length - 1]; if (Math.hypot(p.x - l[0], p.y - l[1]) * View.s > 2) this.poly.push([p.x, p.y]); }
    } else {
      g.x1 = p.x; g.y1 = p.y;
      if (p.shift) { const w = g.x1 - g.x0, h = g.y1 - g.y0, m = Math.max(Math.abs(w), Math.abs(h)); g.x1 = g.x0 + Math.sign(w || 1) * m; g.y1 = g.y0 + Math.sign(h || 1) * m; }
    }
    View.requestDraw();
  },
  up(p) {
    if (this.wand) { this.wandEnd(); return; }
    const g = this.drag; this.drag = null;
    if (!g || g.closed) return;
    if (this.shape === 'lasso') {
      if (g.moved) { if (this.poly.length > 2) this.finishPoly(g.op); else this.poly = []; }
      else if (this.poly.length === 1 && g.start === 1) { /* первая точка многоугольника */ }
      else this.poly.push([p.x, p.y]);
      View.requestDraw(); return;
    }
    if (Math.hypot(p.sx - g.sx, p.sy - g.sy) < 4) { if (g.op === 'replace' && Sel.active) Sel.clear(); return; }
    const x = Math.min(g.x0, g.x1), y = Math.min(g.y0, g.y1), w = Math.abs(g.x1 - g.x0), h = Math.abs(g.y1 - g.y0);
    const path = new Path2D();
    if (this.shape === 'ellipse') path.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2); else path.rect(x, y, w, h);
    Sel.change('Выделение', () => Sel.applyPath(path, g.op));
    View.requestDraw();
  },
  finishPoly(op) {
    const path = new Path2D(); this.poly.forEach(([x, y], i) => i ? path.lineTo(x, y) : path.moveTo(x, y)); path.closePath();
    this.poly = [];
    Sel.change('Выделение лассо', () => Sel.applyPath(path, op || this.op));
  },
  wandStart(p, op) {
    const d = Engine.doc, x = Math.floor(p.x), y = Math.floor(p.y);
    if (x < 0 || y < 0 || x >= d.w || y >= d.h) return;
    const src = Settings.fillSource === 'layer' ? d.layer.canvas : Engine.flatten({ bg: true });
    this.wand = { data: U.readPixels(src), x, y, sx: p.sx, base: this.tol, tol: this.tol, op };
    this.wandCompute();
  },
  wandCompute() {
    const w = this.wand, d = Engine.doc, { mask, bounds } = floodMask(w.data, w.x, w.y, w.tol, 0);
    w.mask = maskCanvas(mask, d.w, bounds, { r: 255, g: 255, b: 255 }); w.bounds = bounds;
    w.view = maskCanvas(mask, d.w, bounds, { r: 255, g: 107, b: 87 }, 110);
    View.requestDraw();
  },
  wandMove(p) {
    const w = this.wand, tol = Math.round(U.clamp(w.base + (p.sx - w.sx) / 4, 0, 100));
    if (tol !== w.tol) { w.tol = tol; UI.hint('Порог: ' + tol + '%'); if (!w.pend) { w.pend = true; requestAnimationFrame(() => { if (this.wand) { this.wand.pend = false; this.wandCompute(); } }); } }
  },
  wandEnd() {
    const w = this.wand; this.wand = null; if (!w || !w.mask) return;
    this.tol = w.tol;
    Sel.change('Автовыделение', () => Sel.applyMask(w.mask, w.op, w.bounds.x, w.bounds.y));
  },
  cancel() { this.drag = null; this.wand = null; View.requestDraw(); },
  overlay(ctx) {
    const S = (x, y) => View.toScreen(x, y);
    ctx.save(); ctx.lineWidth = 1.5; ctx.setLineDash([6, 5]);
    const g = this.drag;
    if (this.wand && this.wand.view) {
      const m = View.m(), dpr = View.dpr; ctx.save();
      ctx.setTransform(m[0] * dpr, m[1] * dpr, m[2] * dpr, m[3] * dpr, m[4] * dpr, m[5] * dpr);
      ctx.drawImage(this.wand.view, this.wand.bounds.x, this.wand.bounds.y); ctx.restore();
    }
    if (g && this.shape !== 'lasso' && g.x1 !== undefined) {
      const corners = [[g.x0, g.y0], [g.x1, g.y0], [g.x1, g.y1], [g.x0, g.y1]].map(([x, y]) => S(x, y));
      ctx.beginPath();
      if (this.shape === 'ellipse') {
        const cx = (g.x0 + g.x1) / 2, cy = (g.y0 + g.y1) / 2, rx = Math.abs(g.x1 - g.x0) / 2, ry = Math.abs(g.y1 - g.y0) / 2;
        for (let i = 0; i <= 64; i++) { const t = i / 64 * Math.PI * 2, [X, Y] = S(cx + Math.cos(t) * rx, cy + Math.sin(t) * ry); i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }
      } else corners.forEach(([X, Y], i) => i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y));
      ctx.closePath(); ctx.strokeStyle = '#000'; ctx.stroke(); ctx.lineDashOffset = 5; ctx.strokeStyle = '#fff'; ctx.stroke();
    }
    if (this.poly.length) {
      ctx.beginPath(); this.poly.forEach(([x, y], i) => { const [X, Y] = S(x, y); i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); });
      if (View.hover && !(g && g.moved)) ctx.lineTo(View.hover.x, View.hover.y);
      ctx.strokeStyle = '#000'; ctx.stroke(); ctx.lineDashOffset = 5; ctx.strokeStyle = '#fff'; ctx.stroke();
      ctx.setLineDash([]); const [X, Y] = S(this.poly[0][0], this.poly[0][1]);
      ctx.fillStyle = '#fff'; ctx.strokeStyle = '#ff6b57'; ctx.lineWidth = 2; ctx.beginPath(); ctx.arc(X, Y, 7, 0, 7); ctx.fill(); ctx.stroke();
    }
    ctx.restore();
  }
};

/* ---------- сетка искажения ---------- */
const Mesh = {
  tri(ctx, img, s0, s1, s2, d0, d1, d2) {
    const cx = (d0[0] + d1[0] + d2[0]) / 3, cy = (d0[1] + d1[1] + d2[1]) / 3;
    const ex = p => { const dx = p[0] - cx, dy = p[1] - cy, l = Math.hypot(dx, dy) || 1; return [p[0] + dx / l * 0.8, p[1] + dy / l * 0.8]; };
    const a = ex(d0), b = ex(d1), c = ex(d2);
    const sx1 = s1[0] - s0[0], sy1 = s1[1] - s0[1], sx2 = s2[0] - s0[0], sy2 = s2[1] - s0[1];
    const det = sx1 * sy2 - sx2 * sy1; if (Math.abs(det) < 1e-9) return;
    const dx1 = d1[0] - d0[0], dy1 = d1[1] - d0[1], dx2 = d2[0] - d0[0], dy2 = d2[1] - d0[1];
    const A = (dx1 * sy2 - dx2 * sy1) / det, B = (dy1 * sy2 - dy2 * sy1) / det, C = (dx2 * sx1 - dx1 * sx2) / det, D = (dy2 * sx1 - dy1 * sx2) / det;
    const E = d0[0] - A * s0[0] - C * s0[1], F = d0[1] - B * s0[0] - D * s0[1];
    ctx.save(); ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.lineTo(c[0], c[1]); ctx.closePath(); ctx.clip();
    ctx.transform(A, B, C, D, E, F); ctx.drawImage(img, 0, 0); ctx.restore();
  },
  draw(ctx, img, map, n) {
    const W = img.width, H = img.height, G = [];
    for (let j = 0; j <= n; j++) for (let i = 0; i <= n; i++) G.push(map(i / n, j / n));
    for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) {
      const s00 = [i / n * W, j / n * H], s10 = [(i + 1) / n * W, j / n * H], s11 = [(i + 1) / n * W, (j + 1) / n * H], s01 = [i / n * W, (j + 1) / n * H];
      const d00 = G[j * (n + 1) + i], d10 = G[j * (n + 1) + i + 1], d11 = G[(j + 1) * (n + 1) + i + 1], d01 = G[(j + 1) * (n + 1) + i];
      this.tri(ctx, img, s00, s10, s11, d00, d10, d11); this.tri(ctx, img, s00, s11, s01, d00, d11, d01);
    }
  },
  homography(q) {
    const [x0, y0] = q[0], [x1, y1] = q[1], [x2, y2] = q[2], [x3, y3] = q[3];
    const dx1 = x1 - x2, dx2 = x3 - x2, dy1 = y1 - y2, dy2 = y3 - y2, sx = x0 - x1 + x2 - x3, sy = y0 - y1 + y2 - y3;
    let g = 0, h = 0;
    if (Math.abs(sx) > 1e-9 || Math.abs(sy) > 1e-9) { const den = dx1 * dy2 - dx2 * dy1 || 1e-9; g = (sx * dy2 - dx2 * sy) / den; h = (dx1 * sy - sx * dy1) / den; }
    const a = x1 - x0 + g * x1, b = x3 - x0 + h * x3, c = x0, d = y1 - y0 + g * y1, e = y3 - y0 + h * y3, f = y0;
    return (u, v) => { const w = g * u + h * v + 1; return [(a * u + b * v + c) / w, (d * u + e * v + f) / w]; };
  },
  bezier(P) {
    const B = t => { const u = 1 - t; return [u * u * u, 3 * t * u * u, 3 * t * t * u, t * t * t]; };
    return (u, v) => { const bu = B(u), bv = B(v); let x = 0, y = 0; for (let j = 0; j < 4; j++) for (let i = 0; i < 4; i++) { const w = bu[i] * bv[j], p = P[j * 4 + i]; x += p[0] * w; y += p[1] * w; } return [x, y]; };
  }
};

/* ---------- трансформация ---------- */
const Xform = {
  s: null, mode: 'free', smooth: true, drag: null,
  activate() { if (!this.begin()) return false; UI.showOptbar('transform'); },
  deactivate() { if (this.s) this.commit(); UI.hideOptbar(); },
  begin() {
    const d = Engine.doc, L = d.layer;
    if (!L || L.locked) { U.toast('Слой заблокирован'); return false; }
    if (L.text) Ops.rasterize(L);
    const sel = Sel.active;
    const bounds = sel ? Sel.bounds() : U.contentBounds(L.canvas);
    if (!bounds) { U.toast(sel ? 'Выделение пустое' : 'На слое нечего трансформировать'); return false; }
    const fc = U.crop(L.canvas, bounds);
    const hole = U.clone(L.canvas), hx = hole.getContext('2d');
    if (sel) {
      const fx = fc.getContext('2d'); fx.globalCompositeOperation = 'destination-in'; fx.drawImage(Sel.mask, bounds.x, bounds.y, bounds.w, bounds.h, 0, 0, bounds.w, bounds.h);
      hx.globalCompositeOperation = 'destination-out'; hx.drawImage(Sel.mask, 0, 0);
    } else hx.clearRect(0, 0, d.w, d.h);
    this.s = { L, fc, hole, b: bounds, mask: sel ? U.crop(Sel.mask, bounds) : null, cx: bounds.x + bounds.w / 2, cy: bounds.y + bounds.h / 2, w: bounds.w, h: bounds.h, rot: 0, mesh: null, q: null, wp: null, changed: false };
    this.s.init = { cx: this.s.cx, cy: this.s.cy, w: this.s.w, h: this.s.h };
    if (this.mode === 'distort' || this.mode === 'warp') this.toMesh(this.mode);
    Engine.beginOverride(L); this.render();
    return true;
  },
  corners() {
    const s = this.s;
    if (s.mesh === 'distort') return s.q;
    if (s.mesh === 'warp') return [s.wp[0], s.wp[3], s.wp[15], s.wp[12]];
    const c = Math.cos(s.rot), sn = Math.sin(s.rot);
    return [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, b]) => { const x = a * s.w / 2, y = b * s.h / 2; return [s.cx + x * c - y * sn, s.cy + x * sn + y * c]; });
  },
  mapFn() {
    const s = this.s;
    if (s.mesh === 'warp') return Mesh.bezier(s.wp);
    const q = this.corners(); return Mesh.homography(q);
  },
  toMesh(kind) {
    const s = this.s;
    if (kind === 'distort') {
      if (s.mesh === 'warp') this.bake();
      if (!s.mesh) s.q = this.corners();
      s.mesh = 'distort';
    } else {
      const f = this.mapFn(); s.wp = [];
      for (let j = 0; j < 4; j++) for (let i = 0; i < 4; i++) s.wp.push(f(i / 3, j / 3));
      s.mesh = 'warp';
    }
  },
  bake() {
    const s = this.s, d = Engine.doc, t = U.canvas(d.w, d.h), x = t.getContext('2d');
    x.imageSmoothingEnabled = this.smooth; this.drawFloat(x, s.fc);
    const b = U.clipRect(U.inflate(U.ptsBounds(this.meshPts()), 2), d.w, d.h) || { x: 0, y: 0, w: 1, h: 1 };
    let nm = null;
    if (s.mask) { const m = U.canvas(d.w, d.h), mx = m.getContext('2d'); this.drawFloat(mx, s.mask); nm = U.crop(m, b); }
    s.fc = U.crop(t, b); s.mask = nm; s.b = b;
    s.cx = b.x + b.w / 2; s.cy = b.y + b.h / 2; s.w = b.w; s.h = b.h; s.rot = 0; s.mesh = null; s.q = null; s.wp = null;
  },
  meshPts() {
    const s = this.s;
    if (s.mesh === 'warp') { const f = this.mapFn(), out = []; for (let j = 0; j <= 8; j++) for (let i = 0; i <= 8; i++) out.push(f(i / 8, j / 8)); return out; }
    return this.corners();
  },
  setMode(m) {
    this.mode = m;
    if (!this.s) return;
    if (m === 'distort' || m === 'warp') this.toMesh(m);
    else if (this.s.mesh) this.bake();
    this.render(); UI.optbarRefresh();
  },
  drawFloat(ctx, img) {
    const s = this.s;
    ctx.save(); ctx.imageSmoothingEnabled = this.smooth; ctx.imageSmoothingQuality = 'high';
    if (s.mesh) Mesh.draw(ctx, img, this.mapFn(), s.mesh === 'warp' ? 14 : 12);
    else { ctx.translate(s.cx, s.cy); ctx.rotate(s.rot); ctx.scale(s.w / img.width, s.h / img.height); ctx.drawImage(img, -img.width / 2, -img.height / 2); }
    ctx.restore();
  },
  render() {
    const s = this.s, A = Engine._s.active.ctx;
    A.save(); A.setTransform(1, 0, 0, 1, 0, 0); A.globalAlpha = 1; A.globalCompositeOperation = 'copy'; A.drawImage(s.hole, 0, 0); A.restore();
    this.drawFloat(A, s.fc);
    Engine.renderAll();
  },
  handles() {
    const s = this.s, c = this.corners(), out = [];
    if (s.mesh === 'warp') { s.wp.forEach((p, i) => out.push({ k: 'wp', i, p })); return out; }
    c.forEach((p, i) => out.push({ k: s.mesh ? 'pt' : 'corner', i, p }));
    if (!s.mesh) {
      if (this.mode === 'free') for (let i = 0; i < 4; i++) { const a = c[i], b = c[(i + 1) % 4]; out.push({ k: 'edge', i, p: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] }); }
      const tm = [(c[0][0] + c[1][0]) / 2, (c[0][1] + c[1][1]) / 2], [X, Y] = View.toScreen(tm[0], tm[1]), [CX, CY] = View.toScreen(s.cx, s.cy);
      const l = Math.hypot(X - CX, Y - CY) || 1; out.push({ k: 'rot', sp: [X + (X - CX) / l * 34, Y + (Y - CY) / l * 34] });
    }
    return out;
  },
  down(p) {
    const s = this.s; if (!s) return;
    let hit = null, best = 26;
    for (const h of this.handles()) { const [X, Y] = h.sp || View.toScreen(h.p[0], h.p[1]); const dd = Math.hypot(X - p.sx, Y - p.sy); if (dd < best) { best = dd; hit = h; } }
    const snap = { cx: s.cx, cy: s.cy, w: s.w, h: s.h, rot: s.rot, q: s.q && s.q.map(a => a.slice()), wp: s.wp && s.wp.map(a => a.slice()) };
    const scr = this.corners().map(([x, y]) => View.toScreen(x, y));
    if (!hit) hit = U.inPoly(p.sx, p.sy, scr) ? { k: s.mesh === 'warp' ? 'wdrag' : 'move' } : { k: s.mesh ? 'move' : 'rot' };
    this.drag = { h: hit, p0: p, snap, a0: Math.atan2(p.y - s.cy, p.x - s.cx) };
    if (hit.k === 'corner') this.drag.anchor = this.corners()[(hit.i + 2) % 4];
    if (hit.k === 'edge') { const c = this.corners(); const o = (hit.i + 2) % 4, a = c[o], b = c[(o + 1) % 4]; this.drag.anchor = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]; }
  },
  move(p) {
    const g = this.drag, s = this.s; if (!g || !s) return;
    const S = g.snap, dx = p.x - g.p0.x, dy = p.y - g.p0.y, h = g.h;
    const c = Math.cos(S.rot), sn = Math.sin(S.rot), ux = [c, sn], uy = [-sn, c];
    if (h.k === 'move') {
      if (s.mesh === 'distort') s.q = S.q.map(([x, y]) => [x + dx, y + dy]);
      else if (s.mesh === 'warp') s.wp = S.wp.map(([x, y]) => [x + dx, y + dy]);
      else { s.cx = S.cx + dx; s.cy = S.cy + dy; }
    } else if (h.k === 'rot') {
      let r = S.rot + Math.atan2(p.y - S.cy, p.x - S.cx) - g.a0;
      const q = Math.round(r / (Math.PI / 12)) * Math.PI / 12; if (p.shift || Math.abs(r - q) < 0.035) r = q;
      s.rot = r;
    } else if (h.k === 'corner') {
      const sg = [[-1, -1], [1, -1], [1, 1], [-1, 1]][h.i], A = g.anchor;
      const vx = p.x - A[0], vy = p.y - A[1], lx = vx * ux[0] + vy * ux[1], ly = vx * uy[0] + vy * uy[1];
      let w, hh;
      if (this.mode === 'uniform' || p.shift) {
        const ox = sg[0] * S.w, oy = sg[1] * S.h, k = (lx * ox + ly * oy) / Math.max(1e-6, ox * ox + oy * oy);
        w = S.w * k; hh = S.h * k;
      } else { w = lx / sg[0]; hh = ly / sg[1]; }
      if (Math.abs(w) < 1) w = Math.sign(w || 1); if (Math.abs(hh) < 1) hh = Math.sign(hh || 1);
      s.w = w; s.h = hh;
      const hx = sg[0] * w / 2, hy = sg[1] * hh / 2;
      s.cx = A[0] + hx * c - hy * sn; s.cy = A[1] + hx * sn + hy * c;
    } else if (h.k === 'edge') {
      const A = g.anchor, horiz = h.i === 1 || h.i === 3, sgn = (h.i === 1 || h.i === 2) ? 1 : -1, ax = horiz ? ux : uy;
      let v = ((p.x - A[0]) * ax[0] + (p.y - A[1]) * ax[1]) * sgn;
      if (Math.abs(v) < 1) v = Math.sign(v || 1);
      if (horiz) s.w = v; else s.h = v;
      s.cx = A[0] + ax[0] * sgn * v / 2; s.cy = A[1] + ax[1] * sgn * v / 2;
    } else if (h.k === 'pt') {
      s.q = S.q.map(a => a.slice()); s.q[h.i] = [S.q[h.i][0] + dx, S.q[h.i][1] + dy];
    } else if (h.k === 'wp') {
      s.wp = S.wp.map(a => a.slice()); s.wp[h.i] = [S.wp[h.i][0] + dx, S.wp[h.i][1] + dy];
    } else if (h.k === 'wdrag') {
      const b = U.ptsBounds(S.wp), sig = Math.max(b.w, b.h) * 0.3;
      s.wp = S.wp.map(([x, y]) => { const w = Math.exp(-((x - g.p0.x) ** 2 + (y - g.p0.y) ** 2) / (2 * sig * sig)); return [x + dx * w, y + dy * w]; });
    }
    s.changed = true;
    if (!this.pend) { this.pend = true; requestAnimationFrame(() => { this.pend = false; if (this.s) this.render(); }); }
  },
  up() { this.drag = null; },
  cancel() { this.drag = null; },
  act(kind) {
    const s = this.s; if (!s) return;
    const cen = () => { const b = U.ptsBounds(s.mesh === 'warp' ? s.wp : this.corners()); return [b.x + b.w / 2, b.y + b.h / 2]; };
    const mapPts = f => { if (s.mesh === 'distort') s.q = s.q.map(f); else if (s.mesh === 'warp') s.wp = s.wp.map(f); };
    if (kind === 'flipH') { if (s.mesh) { const [cx] = cen(); mapPts(([x, y]) => [2 * cx - x, y]); } else s.w = -s.w; }
    else if (kind === 'flipV') { if (s.mesh) { const [, cy] = cen(); mapPts(([x, y]) => [x, 2 * cy - y]); } else s.h = -s.h; }
    else if (kind === 'rot45') {
      if (s.mesh) { const [cx, cy] = cen(), a = Math.PI / 4, c = Math.cos(a), sn = Math.sin(a); mapPts(([x, y]) => [cx + (x - cx) * c - (y - cy) * sn, cy + (x - cx) * sn + (y - cy) * c]); }
      else s.rot += Math.PI / 4;
    } else if (kind === 'fit') {
      if (s.mesh) this.bake();
      const d = Engine.doc, k = Math.min(d.w / Math.abs(s.w), d.h / Math.abs(s.h));
      s.w *= k; s.h *= k; s.rot = 0; s.cx = d.w / 2; s.cy = d.h / 2;
    } else if (kind === 'reset') {
      s.mesh = null; s.q = null; s.wp = null; Object.assign(s, s.init); s.rot = 0;
      if (s.fc.width !== s.init.w || s.fc.height !== s.init.h) { this.cancelAll(); this.begin(); return; }
      if (this.mode === 'distort' || this.mode === 'warp') this.toMesh(this.mode);
    } else if (kind === 'smooth') { this.smooth = !this.smooth; UI.optbarRefresh(); }
    s.changed = true; this.render();
  },
  cancelAll() { if (!this.s) return; Engine.endOverride(); this.s = null; Engine.renderAll(); },
  commit() {
    const s = this.s; if (!s) return;
    this.s = null;
    if (!s.changed) { Engine.endOverride(); Engine.renderAll(); return; }
    const d = Engine.doc, L = s.L;
    History.group('Трансформация', () => {
      Engine.commit(Engine.full(), 'Трансформация');
      if (s.mask) {
        const keep = this.s; this.s = s;
        Sel.change('Трансформация выделения', () => {
          Sel.ensure(); Sel.mctx.clearRect(0, 0, d.w, d.h); this.drawFloat(Sel.mctx, s.mask);
          if (Sel.path && !s.mesh) {
            const m = new DOMMatrix().translate(s.cx, s.cy).rotate(s.rot * 180 / Math.PI).scale(s.w / s.b.w, s.h / s.b.h).translate(-(s.b.x + s.b.w / 2), -(s.b.y + s.b.h / 2));
            const p = new Path2D(); p.addPath(Sel.path, m); Sel.path = p;
          } else Sel.path = null;
        });
        this.s = keep;
      }
    });
    L.touch(); UI.layersChanged();
  },
  overlay(ctx) {
    const s = this.s; if (!s) return;
    const S = (x, y) => View.toScreen(x, y);
    ctx.save(); ctx.lineWidth = 1.5; ctx.strokeStyle = '#ff6b57';
    if (s.mesh === 'warp') {
      const f = this.mapFn(); ctx.beginPath();
      for (let j = 0; j <= 6; j++) for (let i = 0; i <= 24; i++) { const [X, Y] = S(...f(i / 24, j / 6)); i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }
      for (let i = 0; i <= 6; i++) for (let j = 0; j <= 24; j++) { const [X, Y] = S(...f(i / 6, j / 24)); j ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }
      ctx.globalAlpha = 0.6; ctx.stroke(); ctx.globalAlpha = 1;
    } else {
      ctx.beginPath(); this.corners().forEach(([x, y], i) => { const [X, Y] = S(x, y); i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }); ctx.closePath(); ctx.stroke();
    }
    for (const h of this.handles()) {
      const [X, Y] = h.sp || S(h.p[0], h.p[1]);
      if (h.k === 'rot') { const c = this.corners(), tm = S((c[0][0] + c[1][0]) / 2, (c[0][1] + c[1][1]) / 2); ctx.beginPath(); ctx.moveTo(tm[0], tm[1]); ctx.lineTo(X, Y); ctx.stroke(); }
      ctx.fillStyle = h.k === 'rot' ? '#ff6b57' : '#fff'; ctx.beginPath(); ctx.arc(X, Y, h.k === 'edge' ? 6 : h.k === 'wp' ? 6 : 8, 0, 7); ctx.fill(); ctx.stroke();
    }
    ctx.restore();
  }
};

/* ---------- текст ---------- */
const TextTool = {
  activate() { UI.hint('Коснитесь холста, чтобы добавить или изменить текст'); },
  down() { }, move() { },
  async up(p) {
    const L = Engine.doc.layer;
    if (L && L.text && L.text.bounds) { const b = L.text.bounds; if (p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h) { this.edit(L); return; } }
    const t = await UI.textDialog({ str: '', font: 'system-ui, sans-serif', size: Math.round(Math.max(24, Engine.doc.h / 14)), bold: false, italic: false, align: 'left', color: Colors.hex(), lh: 1.2, spacing: 0, stroke: 0, strokeColor: '#000000', x: Math.round(p.x), y: Math.round(p.y) });
    if (t && t.str.trim()) Ops.addText(t);
  },
  async edit(L) { const t = await UI.textDialog(Object.assign({}, L.text)); if (t) Ops.setText(L, t); }
};

/* ---------- пластика ---------- */
const Liquify = {
  s: null, mode: 'push', size: 0.25, strength: 0.5, distortion: 0.5,
  modes: [['push', 'Толкать'], ['twirlR', 'Вихрь ↻'], ['twirlL', 'Вихрь ↺'], ['pinch', 'Сжать'], ['expand', 'Расширить'], ['crystals', 'Кристаллы'], ['smooth', 'Сгладить'], ['reconstruct', 'Восстановить']],
  begin() {
    const d = Engine.doc, L = d.layer;
    if (!Ops.editable(L)) return false;
    if (L.text) Ops.rasterize(L);
    Tools.commitPending();
    const step = Math.max(1, Math.ceil(Math.sqrt(d.w * d.h / 3e6)));
    const gw = Math.ceil(d.w / step) + 2, gh = Math.ceil(d.h / step) + 2;
    this.s = { L, src: U.readPixels(L.canvas), step, gw, gh, dx: new Float32Array(gw * gh), dy: new Float32Array(gw * gh), changed: false };
    Engine.beginOverride(L);
    this.prevTool = Tools.cur; Tools.cur = 'liquify'; UI.toolChanged(); UI.showOptbar('liquify');
    return true;
  },
  radius() { const d = Engine.doc; return Math.max(4, this.size * this.size * Math.max(d.w, d.h) * 0.5); },
  down(p) { if (!this.s) return; this.last = p; this.dab(p.x, p.y, 0, 0, p.p); clearInterval(this.timer); if (this.mode !== 'push') this.timer = setInterval(() => this.last && this.dab(this.last.x, this.last.y, 0, 0, this.last.p), 33); },
  move(p) {
    if (!this.s || !this.last) return;
    const r = this.radius(), dx = p.x - this.last.x, dy = p.y - this.last.y, L = Math.hypot(dx, dy), step = Math.max(1, r * 0.15);
    if (L < step) { if (this.mode !== 'push') this.last = Object.assign({}, this.last, { x: p.x, y: p.y }); return; }
    const n = Math.ceil(L / step);
    for (let i = 1; i <= n; i++) this.dab(this.last.x + dx * i / n, this.last.y + dy * i / n, dx / n, dy / n, p.p);
    this.last = p;
  },
  up() { clearInterval(this.timer); this.last = null; },
  cancel() { this.up(); },
  dab(cx, cy, mx, my, pr) {
    const s = this.s, r = this.radius(), st = s.step, k = this.strength * (0.3 + 0.7 * Settings.pressure(pr));
    const gx0 = Math.max(0, Math.floor((cx - r) / st)), gx1 = Math.min(s.gw - 1, Math.ceil((cx + r) / st));
    const gy0 = Math.max(0, Math.floor((cy - r) / st)), gy1 = Math.min(s.gh - 1, Math.ceil((cy + r) / st));
    const rng = Math.random;
    for (let gy = gy0; gy <= gy1; gy++) for (let gx = gx0; gx <= gx1; gx++) {
      const x = gx * st, y = gy * st, ddx = x - cx, ddy = y - cy, dist = Math.hypot(ddx, ddy);
      if (dist >= r) continue;
      const f = U.smooth(1 - dist / r) * k, i = gy * s.gw + gx;
      switch (this.mode) {
        case 'push': s.dx[i] -= mx * f * 1.2; s.dy[i] -= my * f * 1.2; break;
        case 'twirlR': case 'twirlL': { const a = (this.mode === 'twirlR' ? -1 : 1) * 0.06 * f, c = Math.cos(a), sn = Math.sin(a); s.dx[i] += ddx * c - ddy * sn - ddx; s.dy[i] += ddx * sn + ddy * c - ddy; break; }
        case 'pinch': s.dx[i] += ddx * 0.04 * f; s.dy[i] += ddy * 0.04 * f; break;
        case 'expand': s.dx[i] -= ddx * 0.04 * f; s.dy[i] -= ddy * 0.04 * f; break;
        case 'crystals': s.dx[i] += (rng() - 0.5) * r * 0.08 * f * this.distortion * 2; s.dy[i] += (rng() - 0.5) * r * 0.08 * f * this.distortion * 2; break;
        case 'reconstruct': s.dx[i] *= 1 - 0.2 * f; s.dy[i] *= 1 - 0.2 * f; break;
        case 'smooth': {
          if (gx > 0 && gy > 0 && gx < s.gw - 1 && gy < s.gh - 1) {
            const ax = (s.dx[i - 1] + s.dx[i + 1] + s.dx[i - s.gw] + s.dx[i + s.gw]) / 4, ay = (s.dy[i - 1] + s.dy[i + 1] + s.dy[i - s.gw] + s.dy[i + s.gw]) / 4;
            s.dx[i] += (ax - s.dx[i]) * 0.5 * f; s.dy[i] += (ay - s.dy[i]) * 0.5 * f;
          }
          break;
        }
      }
    }
    s.changed = true;
    this.renderRegion({ x: cx - r - st * 2, y: cy - r - st * 2, w: 2 * r + st * 4, h: 2 * r + st * 4 });
  },
  renderRegion(r0) {
    const s = this.s, d = Engine.doc, r = U.clipRect(r0, d.w, d.h); if (!r) return;
    const W = d.w, H = d.h, src = s.src.data, st = s.step, gw = s.gw;
    const out = new ImageData(r.w, r.h), o = out.data;
    for (let y = 0; y < r.h; y++) {
      const Y = r.y + y, gyf = Y / st, gy = Math.floor(gyf), ty = gyf - gy;
      for (let x = 0; x < r.w; x++) {
        const X = r.x + x, gxf = X / st, gx = Math.floor(gxf), tx = gxf - gx, i = gy * gw + gx;
        const dx = (s.dx[i] * (1 - tx) + s.dx[i + 1] * tx) * (1 - ty) + (s.dx[i + gw] * (1 - tx) + s.dx[i + gw + 1] * tx) * ty;
        const dy = (s.dy[i] * (1 - tx) + s.dy[i + 1] * tx) * (1 - ty) + (s.dy[i + gw] * (1 - tx) + s.dy[i + gw + 1] * tx) * ty;
        const sx = U.clamp(X + dx, 0, W - 1.001), sy = U.clamp(Y + dy, 0, H - 1.001);
        const x0 = sx | 0, y0 = sy | 0, fx = sx - x0, fy = sy - y0, j = (y0 * W + x0) * 4, j2 = j + W * 4, oi = (y * r.w + x) * 4;
        for (let c = 0; c < 4; c++) o[oi + c] = (src[j + c] * (1 - fx) + src[j + 4 + c] * fx) * (1 - fy) + (src[j2 + c] * (1 - fx) + src[j2 + 4 + c] * fx) * fy;
      }
    }
    const A = Engine._s.active.ctx;
    if (Sel.active) {
      const t = U.fromImageData(out), tx = t.getContext('2d'); tx.globalCompositeOperation = 'destination-in'; tx.drawImage(Sel.mask, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h);
      A.save(); U.clipCtx(A, r); A.clearRect(r.x, r.y, r.w, r.h); A.drawImage(s.L.canvas, 0, 0);
      A.globalCompositeOperation = 'destination-out'; A.drawImage(Sel.mask, 0, 0); A.globalCompositeOperation = 'source-over'; A.drawImage(t, r.x, r.y); A.restore();
    } else A.putImageData(out, r.x, r.y);
    Engine.renderRect(r);
  },
  reset() { const s = this.s; if (!s) return; s.dx.fill(0); s.dy.fill(0); this.renderRegion(Engine.full()); },
  finish() { this.s = null; Tools.cur = this.prevTool || 'brush'; UI.toolChanged(); UI.hideOptbar(); clearInterval(this.timer); },
  apply() { const s = this.s; if (!s) return; if (s.changed) Engine.commit(Engine.full(), 'Пластика'); else { Engine.endOverride(); Engine.renderAll(); } this.finish(); },
  discard() { if (!this.s) return; Engine.endOverride(); Engine.renderAll(); this.finish(); },
  overlay(ctx) {
    const h = View.hover || (this.last && { x: this.last.sx, y: this.last.sy }); if (!h) return;
    const r = this.radius() * View.s;
    ctx.lineWidth = 1; ctx.strokeStyle = '#000a'; ctx.beginPath(); ctx.arc(h.x, h.y, r + 1, 0, 7); ctx.stroke(); ctx.strokeStyle = '#fff'; ctx.beginPath(); ctx.arc(h.x, h.y, r, 0, 7); ctx.stroke();
  }
};

Object.assign(Tools.map, {
  brush: paintTool('brush'), smudge: paintTool('smudge'), eraser: paintTool('eraser'), clone: paintTool('clone'),
  fill: Fill, picker: Picker.tool, select: SelectTool, transform: Xform, text: TextTool, liquify: Liquify
});
