'use strict';
/* Мазок — вид холста (масштаб, поворот, отражение) и обработка ввода (перо, мышь, жесты) */

const View = {
  cv: null, ctx: null, dpr: 1, W: 0, H: 0, s: 1, r: 0, flip: 1, ox: 0, oy: 0,
  hover: null, pending: false, stageColor: '#2a2b30', checker: null, antsT: 0,
  init() {
    this.cv = U.$('#stage'); this.ctx = this.cv.getContext('2d');
    window.addEventListener('resize', () => this.resize());
    this.resize(); Input.init(this.cv);
    setInterval(() => { if (Sel.active && Sel.path && Engine.doc) { this.antsT = (this.antsT + 1) % 16; this.requestDraw(); } }, 140);
  },
  theme() { this.stageColor = getComputedStyle(document.body).getPropertyValue('--stage').trim() || '#2a2b30'; this.requestDraw(); },
  resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 3);
    const W = window.innerWidth, H = window.innerHeight;
    const keep = this.W ? this.toDoc(this.W / 2, this.H / 2) : null;
    this.dpr = dpr; this.cv.width = Math.round(W * dpr); this.cv.height = Math.round(H * dpr);
    this.cv.style.width = W + 'px'; this.cv.style.height = H + 'px';
    const wasEmpty = !this.W || !this.H;
    this.W = W; this.H = H;
    if (Engine.doc && W && H) { if (wasEmpty || !keep) this.fit(); else this.centerOn(keep[0], keep[1]); }
    this.requestDraw();
  },
  m() { const c = Math.cos(this.r) * this.s, sn = Math.sin(this.r) * this.s; return [c * this.flip, sn * this.flip, -sn, c, this.ox, this.oy]; },
  toScreen(x, y) { const m = this.m(); return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]; },
  toDoc(X, Y) { const m = this.m(), det = m[0] * m[3] - m[1] * m[2], dx = X - m[4], dy = Y - m[5]; return [(m[3] * dx - m[2] * dy) / det, (-m[1] * dx + m[0] * dy) / det]; },
  area() {
    if (UI.desk) {
      const hid = document.body.classList.contains('ui-hidden');
      const l = hid ? 0 : 44, r = hid ? 0 : 300, t = hid ? 0 : 98, b = (hid ? 0 : 24) + (Engine.doc && Engine.doc.anim.on ? 70 : 0);
      return { x: l + 8, y: t + 8, w: Math.max(50, this.W - l - r - 16), h: Math.max(50, this.H - t - b - 16) };
    }
    const narrow = this.W < 600;
    const l = narrow ? 58 : 66, r = narrow ? 52 : 62, t = 58, b = Engine.doc && Engine.doc.anim.on ? 84 : 14;
    return { x: l, y: t, w: Math.max(50, this.W - l - r), h: Math.max(50, this.H - t - b) };
  },
  fit() {
    const d = Engine.doc; if (!d) return;
    const a = this.area();
    this.s = Math.min(a.w / d.w, a.h / d.h) * 0.94; this.r = 0; this.flip = 1;
    this.ox = a.x + a.w / 2 - this.s * d.w / 2; this.oy = a.y + a.h / 2 - this.s * d.h / 2;
    this.requestDraw(); UI.viewChanged();
  },
  actual() { const d = Engine.doc; const c = this.toDoc(this.W / 2, this.H / 2); this.s = 1; this.centerOn(c[0], c[1]); UI.viewChanged(); },
  centerOn(x, y) { const [X, Y] = this.toScreen(x, y); this.ox += this.W / 2 - X; this.oy += this.H / 2 - Y; this.requestDraw(); },
  zoomAt(X, Y, k) {
    const [x, y] = this.toDoc(X, Y);
    this.s = U.clamp(this.s * k, 0.02, 64);
    const [X2, Y2] = this.toScreen(x, y); this.ox += X - X2; this.oy += Y - Y2;
    this.requestDraw(); UI.viewChanged();
  },
  rotateAt(X, Y, da) {
    const [x, y] = this.toDoc(X, Y); this.r += da;
    const [X2, Y2] = this.toScreen(x, y); this.ox += X - X2; this.oy += Y - Y2; this.requestDraw();
  },
  toggleFlip() {
    const [x, y] = this.toDoc(this.W / 2, this.H / 2);
    this.flip *= -1; this.centerOn(x, y);
    U.toast(this.flip < 0 ? 'Вид отражён' : 'Обычный вид'); UI.viewChanged();
  },
  requestDraw() { if (!this.pending) { this.pending = true; requestAnimationFrame(() => { this.pending = false; this.draw(); }); } },
  checkerPat() {
    if (!this.checker) {
      const c = U.canvas(16, 16), x = c.getContext('2d');
      x.fillStyle = '#ffffff'; x.fillRect(0, 0, 16, 16); x.fillStyle = '#d6d6d6'; x.fillRect(0, 0, 8, 8); x.fillRect(8, 8, 8, 8);
      this.checker = this.ctx.createPattern(c, 'repeat');
    }
    return this.checker;
  },
  draw() {
    const ctx = this.ctx, dpr = this.dpr, d = Engine.doc;
    ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.globalAlpha = 1; ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = this.stageColor; ctx.fillRect(0, 0, this.cv.width, this.cv.height);
    if (!d || !Engine.comp) return;
    const m = this.m();
    ctx.setTransform(m[0] * dpr, m[1] * dpr, m[2] * dpr, m[3] * dpr, m[4] * dpr, m[5] * dpr);
    ctx.save(); ctx.shadowColor = 'rgba(0,0,0,0.35)'; ctx.shadowBlur = 18 * dpr; ctx.fillStyle = '#888'; ctx.fillRect(0, 0, d.w, d.h); ctx.restore();
    if (!d.bgVisible) {
      const p = this.checkerPat(); if (p.setTransform) p.setTransform(new DOMMatrix([1 / this.s, 0, 0, 1 / this.s, 0, 0]));
      ctx.fillStyle = p; ctx.fillRect(0, 0, d.w, d.h);
    }
    ctx.imageSmoothingEnabled = this.s < 2;
    ctx.imageSmoothingQuality = this.s < 0.6 ? 'medium' : 'low';
    ctx.drawImage(Engine.comp, 0, 0);
    ctx.imageSmoothingEnabled = true;
    if (Sel.active && Sel.overlay) ctx.drawImage(Sel.overlay, 0, 0);
    if (Sel.active && Sel.path && !Xform.s) {
      ctx.lineWidth = 1.2 / this.s; ctx.setLineDash([5 / this.s, 5 / this.s]);
      ctx.strokeStyle = '#000'; ctx.lineDashOffset = -this.antsT / this.s; ctx.stroke(Sel.path);
      ctx.strokeStyle = '#fff'; ctx.lineDashOffset = (5 - this.antsT) / this.s; ctx.stroke(Sel.path);
      ctx.setLineDash([]);
    }
    Guides.draw(ctx);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    Tools.overlay(ctx);
    const h = this.hover;
    if (h && Settings.cursor && Tools.isPaint() && !Input.mode && h.type !== 'touch') {
      const r = Math.max(1.5, Tools.cursorSize() * this.s / 2);
      ctx.lineWidth = 1; ctx.strokeStyle = 'rgba(0,0,0,0.7)'; ctx.beginPath(); ctx.arc(h.x, h.y, r + 0.75, 0, 7); ctx.stroke();
      ctx.strokeStyle = 'rgba(255,255,255,0.9)'; ctx.beginPath(); ctx.arc(h.x, h.y, r - 0.25, 0, 7); ctx.stroke();
    }
  }
};

const Input = {
  cv: null, ptrs: new Map(), mode: null, toolId: null, gest: null, space: false, lastPen: 0, ignore: new Set(), maxTouches: 0, gestT0: 0,
  init(cv) {
    this.cv = cv;
    cv.addEventListener('pointerdown', e => this.down(e));
    cv.addEventListener('pointermove', e => this.move(e));
    cv.addEventListener('pointerup', e => this.up(e, false));
    cv.addEventListener('pointercancel', e => this.up(e, true));
    cv.addEventListener('pointerleave', e => { if (!this.ptrs.size) { View.hover = null; View.requestDraw(); } });
    cv.addEventListener('wheel', e => this.wheel(e), { passive: false });
    cv.addEventListener('contextmenu', e => e.preventDefault());
    cv.addEventListener('touchstart', e => e.preventDefault(), { passive: false });
  },
  P(e) {
    const [x, y] = View.toDoc(e.clientX, e.clientY);
    const pen = e.pointerType === 'pen';
    let p = 1;
    if (pen) p = e.pressure > 0 ? e.pressure : 0.02;
    else if (e.pointerType === 'touch' && Settings.fingerPressure && e.pressure > 0 && e.pressure !== 0.5) p = e.pressure;
    const tilt = pen ? Math.min(1, Math.hypot(e.tiltX || 0, e.tiltY || 0) / 70) : 0;
    return { x, y, p, tilt, sx: e.clientX, sy: e.clientY, type: e.pointerType, t: e.timeStamp, shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey || e.metaKey, button: e.button };
  },
  touches() { return Array.from(this.ptrs.entries()).filter(([, p]) => p.type === 'touch'); },
  down(e) {
    if (UI.closePopovers()) { this.ignore.add(e.pointerId); return; }
    try { this.cv.setPointerCapture(e.pointerId); } catch (_) { }
    const t = e.pointerType;
    this.ptrs.set(e.pointerId, { x: e.clientX, y: e.clientY, sx: e.clientX, sy: e.clientY, type: t });
    if (t === 'pen') this.lastPen = Date.now();
    if (t === 'touch') {
      const n = this.touches().length;
      if (n === 1 && !this.mode) {
        this.maxTouches = 1; this.gestT0 = Date.now();
        const penActive = Date.now() - this.lastPen < 4000;
        if (Settings.touchDraw && !penActive) this.startTool(e);
        else this.startGesture();
        return;
      }
      this.maxTouches = Math.max(this.maxTouches, n);
      if (this.mode === 'tool') {
        if (Date.now() - this.toolT0 < 350) Tools.cancel(); else Tools.up(this.P(e));
        this.toolId = null;
      } else if (this.mode === 'pick') { Picker.hide(); }
      this.startGesture();
      return;
    }
    if (this.mode) return;
    if (t === 'mouse') {
      if (e.button === 1 || (e.button === 0 && this.space)) { this.mode = 'pan'; this.panId = e.pointerId; this.cv.style.cursor = 'grabbing'; return; }
      if (e.button === 2) { this.mode = 'pick'; this.toolId = e.pointerId; Picker.start(this.P(e)); return; }
      if (e.button !== 0) return;
    }
    if (e.altKey && Tools.isPaint()) { this.mode = 'pick'; this.toolId = e.pointerId; Picker.start(this.P(e)); return; }
    this.startTool(e);
  },
  startTool(e) {
    const p = this.P(e);
    const h = Guides.hit(p.sx, p.sy);
    if (h) { this.mode = 'guide'; this.toolId = e.pointerId; this.guideH = h; return; }
    this.mode = 'tool'; this.toolId = e.pointerId; this.toolT0 = Date.now();
    View.hover = null; Tools.down(p); View.requestDraw();
  },
  startGesture() {
    const ts = this.touches();
    this.mode = 'gesture';
    this.gest = { ids: ts.slice(0, 2).map(([id]) => id), start: ts.slice(0, 2).map(([, p]) => [p.x, p.y]), view: { s: View.s, r: View.r, ox: View.ox, oy: View.oy }, moved: false, t0: Date.now() };
    if (ts.length >= 2) {
      const [a, b] = this.gest.start; const mx = (a[0] + b[0]) / 2, my = (a[1] + b[1]) / 2;
      this.gest.mid = [mx, my]; this.gest.doc = View.toDoc(mx, my);
    } else if (ts.length === 1) { this.gest.doc = View.toDoc(ts[0][1].x, ts[0][1].y); this.gest.mid = [ts[0][1].x, ts[0][1].y]; }
  },
  move(e) {
    if (this.ignore.has(e.pointerId)) return;
    const pt = this.ptrs.get(e.pointerId);
    if (UI.desk) { const [x, y] = View.toDoc(e.clientX, e.clientY); Desk.coords(x, y); }
    if (!pt) {
      if (e.pointerType !== 'touch') { View.hover = { x: e.clientX, y: e.clientY, type: e.pointerType }; if (Tools.isPaint() || Tools.cur === 'liquify') View.requestDraw(); }
      return;
    }
    pt.x = e.clientX; pt.y = e.clientY;
    if (this.mode === 'tool' && e.pointerId === this.toolId) {
      const evs = e.getCoalescedEvents ? e.getCoalescedEvents() : null;
      if (evs && evs.length) for (const ce of evs) Tools.move(this.P(ce)); else Tools.move(this.P(e));
      if (e.pointerType !== 'touch') View.hover = { x: e.clientX, y: e.clientY, type: e.pointerType };
    } else if (this.mode === 'gesture') this.gesture();
    else if (this.mode === 'pan' && e.pointerId === this.panId) { View.ox += e.movementX || 0; View.oy += e.movementY || 0; View.requestDraw(); }
    else if (this.mode === 'pick' && e.pointerId === this.toolId) Picker.move(this.P(e));
    else if (this.mode === 'guide' && e.pointerId === this.toolId) { const p = this.P(e); Guides.dragMove(this.guideH, p.x, p.y); }
  },
  gesture() {
    const g = this.gest, ts = this.touches();
    if (ts.length >= 2) {
      const ids = ts.slice(0, 2).map(([id]) => id);
      if (ids[0] !== g.ids[0] || ids[1] !== g.ids[1] || g.start.length < 2) { this.startGesture(); this.gest.moved = g.moved; this.gest.t0 = g.t0; return; }
      const [a, b] = ts.slice(0, 2).map(([, p]) => [p.x, p.y]), [sa, sb] = g.start;
      const v0 = [sb[0] - sa[0], sb[1] - sa[1]], v1 = [b[0] - a[0], b[1] - a[1]];
      const k = Math.hypot(v1[0], v1[1]) / Math.max(1, Math.hypot(v0[0], v0[1]));
      let da = Math.atan2(v1[1], v1[0]) - Math.atan2(v0[1], v0[0]);
      const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
      if (Math.hypot(mid[0] - g.mid[0], mid[1] - g.mid[1]) > 10 || Math.abs(k - 1) > 0.06 || Math.abs(da) > 0.08) g.moved = true;
      if (!g.moved) return;
      View.s = U.clamp(g.view.s * k, 0.02, 64);
      if (Settings.rotateGesture) {
        let r = g.view.r + da;
        if (Settings.rotateSnap) { const q = Math.round(r / (Math.PI / 2)) * Math.PI / 2; if (Math.abs(r - q) < 0.07) r = q; }
        View.r = r;
      }
      g.k = k;
      View.ox = 0; View.oy = 0;
      const [X, Y] = View.toScreen(g.doc[0], g.doc[1]);
      View.ox = mid[0] - X; View.oy = mid[1] - Y;
      View.requestDraw(); UI.viewChanged();
    } else if (ts.length === 1 && !Settings.touchDraw || (ts.length === 1 && Date.now() - this.lastPen < 4000 && this.maxTouches === 1)) {
      const p = ts[0][1];
      if (Math.hypot(p.x - g.mid[0], p.y - g.mid[1]) > 8) g.moved = true;
      if (!g.moved) return;
      View.ox = 0; View.oy = 0; View.s = g.view.s; View.r = g.view.r;
      const [X, Y] = View.toScreen(g.doc[0], g.doc[1]);
      View.ox = p.x - X; View.oy = p.y - Y; View.requestDraw();
    }
  },
  up(e, cancelled) {
    if (this.ignore.has(e.pointerId)) { this.ignore.delete(e.pointerId); this.ptrs.delete(e.pointerId); return; }
    if (!this.ptrs.has(e.pointerId)) return;
    this.ptrs.delete(e.pointerId);
    if (this.mode === 'tool' && e.pointerId === this.toolId) {
      if (cancelled) Tools.cancel(); else Tools.up(this.P(e));
      this.mode = null; this.toolId = null; View.requestDraw();
    } else if (this.mode === 'gesture') {
      if (!this.touches().length) {
        const g = this.gest, dt = Date.now() - this.gestT0;
        if (!g.moved && dt < 320) {
          if (this.maxTouches === 2) History.undo();
          else if (this.maxTouches === 3) History.redo();
          else if (this.maxTouches >= 4) UI.toggleHidden();
        } else if (g.moved && g.k && g.k < 0.6 && Date.now() - g.t0 < 300) View.fit();
        this.mode = null; this.gest = null; this.maxTouches = 0;
      }
    } else if (this.mode === 'pan' && e.pointerId === this.panId) { this.mode = null; this.cv.style.cursor = ''; }
    else if (this.mode === 'pick' && e.pointerId === this.toolId) { Picker.end(this.P(e)); this.mode = null; this.toolId = null; }
    else if (this.mode === 'guide' && e.pointerId === this.toolId) { this.mode = null; this.toolId = null; App.changed(); }
    if (!this.ptrs.size && this.mode !== 'tool') this.mode = null;
  },
  wheel(e) {
    e.preventDefault();
    const mouseWheel = e.deltaMode === 1 || (Math.abs(e.deltaY) >= 50 && e.deltaX === 0 && Number.isInteger(e.deltaY));
    if (e.ctrlKey || e.metaKey) View.zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * 0.01));
    else if (e.altKey) View.rotateAt(e.clientX, e.clientY, (e.deltaY > 0 ? 1 : -1) * Math.PI / 36);
    else if (mouseWheel && !e.shiftKey) View.zoomAt(e.clientX, e.clientY, Math.exp(-Math.sign(e.deltaY) * Math.min(Math.abs(e.deltaY), 120) * 0.0018 * (e.deltaMode === 1 ? 40 : 1)));
    else { View.ox -= e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX; View.oy -= e.shiftKey && !e.deltaX ? 0 : e.deltaY; View.requestDraw(); }
  }
};
