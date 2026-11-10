'use strict';
/* Мазок — помощник рисования: сетки, изометрия, перспектива, симметрия */

const Guides = {
  editing: false, drag: null,
  defaults(w, h) {
    return {
      type: 'none', size: Math.max(8, Math.round(Math.min(w, h) / 12)), opacity: 0.55, color: '#3aa0ff', thick: 1, assist: false,
      sym: 'vertical', segments: 6, rotational: true, cx: w / 2, cy: h / 2,
      vpCount: 1, vps: [{ x: w * 0.5, y: h * 0.42 }, { x: -w * 0.35, y: h * 0.42 }, { x: w * 1.35, y: h * 0.42 }, { x: w * 0.5, y: h * 2.2 }]
    };
  },
  get g() { return Engine.doc && Engine.doc.guide; },
  activeVps() { const g = this.g; return g.vpCount === 1 ? [g.vps[0]] : g.vpCount === 2 ? [g.vps[1], g.vps[2]] : [g.vps[1], g.vps[2], g.vps[3]]; },
  /* отражения для симметрии */
  mirrors() {
    const g = this.g; if (!g || g.type !== 'symmetry' || !g.assist) return null;
    const cx = g.cx, cy = g.cy, M = (a, b, c, d) => ({ a, b, c, d, cx, cy });
    const I = M(1, 0, 0, 1);
    switch (g.sym) {
      case 'vertical': return [I, M(-1, 0, 0, 1)];
      case 'horizontal': return [I, M(1, 0, 0, -1)];
      case 'quadrant': return [I, M(-1, 0, 0, 1), M(1, 0, 0, -1), M(-1, 0, 0, -1)];
      case 'radial': {
        const n = U.clamp(g.segments | 0, 2, 32), out = [];
        for (let i = 0; i < n; i++) {
          const t = i * 2 * Math.PI / n, c = Math.cos(t), s = Math.sin(t);
          out.push(M(c, -s, s, c));
          if (!g.rotational) out.push(M(c, s, s, -c));
        }
        return out;
      }
    }
    return null;
  },
  /* ограничение направления штриха (помощь рисованию) */
  constraint(p0) {
    const g = this.g; if (!g || !g.assist || !['grid', 'iso', 'perspective'].includes(g.type)) return null;
    let dir = null; const sx = p0.x, sy = p0.y;
    return (x, y) => {
      if (!dir) {
        const dx = x - sx, dy = y - sy, L = Math.hypot(dx, dy);
        if (L * View.s < 10) return [sx, sy];
        const cands = [];
        if (g.type === 'grid') cands.push(0, Math.PI / 2);
        else if (g.type === 'iso') cands.push(Math.PI / 6, Math.PI / 2, Math.PI * 5 / 6);
        else { for (const v of this.activeVps()) cands.push(Math.atan2(v.y - sy, v.x - sx)); if (g.vpCount < 3) cands.push(Math.PI / 2); if (g.vpCount === 1) cands.push(0); }
        const a = Math.atan2(dy, dx);
        let best = cands[0], bd = Infinity;
        for (const c of cands) { let d = Math.abs(((a - c) % Math.PI + Math.PI * 1.5) % Math.PI - Math.PI / 2); if (d < bd) { bd = d; best = c; } }
        dir = [Math.cos(best), Math.sin(best)];
      }
      const t = (x - sx) * dir[0] + (y - sy) * dir[1];
      return [sx + dir[0] * t, sy + dir[1] * t];
    };
  },
  handles() {
    const g = this.g; if (!g) return [];
    if (g.type === 'symmetry') return [{ k: 'c', x: g.cx, y: g.cy }];
    if (g.type === 'perspective') return this.activeVps().map(v => ({ k: 'vp', ref: v, x: v.x, y: v.y }));
    return [];
  },
  hit(sx, sy) {
    if (!this.editing) return null;
    for (const h of this.handles()) { const [X, Y] = View.toScreen(h.x, h.y); if (Math.hypot(X - sx, Y - sy) < 24) return h; }
    return null;
  },
  dragMove(h, x, y) {
    const g = this.g;
    if (h.k === 'c') { g.cx = Math.round(x); g.cy = Math.round(y); }
    else { h.ref.x = x; h.ref.y = y; if (g.vpCount === 2 || g.vpCount === 3) { if (h.ref === g.vps[1]) g.vps[2].y = y; if (h.ref === g.vps[2]) g.vps[1].y = y; } }
    View.requestDraw();
  },
  draw(ctx) {
    const g = this.g; if (!g || g.type === 'none') return;
    const d = Engine.doc, s = View.s, dpr = View.dpr, m = View.m();
    ctx.save();
    ctx.setTransform(m[0] * dpr, m[1] * dpr, m[2] * dpr, m[3] * dpr, m[4] * dpr, m[5] * dpr);
    ctx.beginPath(); ctx.rect(0, 0, d.w, d.h); ctx.clip();
    ctx.globalAlpha = g.opacity; ctx.strokeStyle = g.color; ctx.lineWidth = g.thick / s;
    ctx.beginPath();
    const D = Math.hypot(d.w, d.h);
    if (g.type === 'grid' && g.size * s >= 4) {
      for (let x = 0; x <= d.w; x += g.size) { ctx.moveTo(x, 0); ctx.lineTo(x, d.h); }
      for (let y = 0; y <= d.h; y += g.size) { ctx.moveTo(0, y); ctx.lineTo(d.w, y); }
    } else if (g.type === 'iso' && g.size * s >= 4) {
      const step = g.size, t = Math.tan(Math.PI / 6);
      for (let x = 0; x <= d.w; x += step * Math.cos(Math.PI / 6) * 2) { ctx.moveTo(x, 0); ctx.lineTo(x, d.h); }
      const span = d.h + d.w * t, dy = step;
      for (let k = -Math.ceil(d.w * t / dy); k * dy <= span; k++) {
        const y0 = k * dy; ctx.moveTo(0, y0); ctx.lineTo(d.w, y0 + d.w * t); ctx.moveTo(0, y0); ctx.lineTo(d.w, y0 - d.w * t);
      }
    } else if (g.type === 'perspective') {
      for (const v of this.activeVps()) {
        const n = 36;
        for (let i = 0; i < n; i++) { const a = i / n * Math.PI * 2; ctx.moveTo(v.x, v.y); ctx.lineTo(v.x + Math.cos(a) * D * 3, v.y + Math.sin(a) * D * 3); }
      }
      if (g.vpCount < 3) { const v = g.vpCount === 1 ? g.vps[0] : g.vps[1]; ctx.moveTo(-D, v.y); ctx.lineTo(d.w + D, v.y); }
    } else if (g.type === 'symmetry') {
      const ms = this.mirrors() || [];
      if (g.sym === 'vertical' || g.sym === 'quadrant') { ctx.moveTo(g.cx, 0); ctx.lineTo(g.cx, d.h); }
      if (g.sym === 'horizontal' || g.sym === 'quadrant') { ctx.moveTo(0, g.cy); ctx.lineTo(d.w, g.cy); }
      if (g.sym === 'radial') { const n = U.clamp(g.segments | 0, 2, 32); for (let i = 0; i < n; i++) { const a = -Math.PI / 2 + i * 2 * Math.PI / n; ctx.moveTo(g.cx, g.cy); ctx.lineTo(g.cx + Math.cos(a) * D, g.cy + Math.sin(a) * D); } }
      void ms;
    }
    ctx.stroke();
    ctx.restore();
    if (this.editing) {
      ctx.save(); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      for (const h of this.handles()) {
        const [X, Y] = View.toScreen(h.x, h.y);
        ctx.fillStyle = '#fff'; ctx.strokeStyle = g.color; ctx.lineWidth = 3;
        ctx.beginPath(); ctx.arc(X, Y, 10, 0, 7); ctx.fill(); ctx.stroke();
      }
      ctx.restore();
    }
  }
};
