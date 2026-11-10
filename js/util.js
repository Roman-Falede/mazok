'use strict';
/* Мазок — утилиты, настройки, иконки */

const U = {};
U.$ = (s, r = document) => r.querySelector(s);
U.$$ = (s, r = document) => Array.from(r.querySelectorAll(s));
U.clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
U.lerp = (a, b, t) => a + (b - a) * t;
U.smooth = t => { t = U.clamp(t, 0, 1); return t * t * (3 - 2 * t); };
U.uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
U.now = () => performance.now();
U.sleep = ms => new Promise(r => setTimeout(r, ms));

/* ---------- холсты ---------- */
U.canvas = (w, h) => { const c = document.createElement('canvas'); c.width = Math.max(1, Math.round(w)); c.height = Math.max(1, Math.round(h)); return c; };
U.crop = (src, r) => { const c = U.canvas(r.w, r.h); c.getContext('2d').drawImage(src, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h); return c; };
U.clone = src => { const c = U.canvas(src.width, src.height); c.getContext('2d').drawImage(src, 0, 0); return c; };
U.readPixels = (src, r) => {
  r = r || { x: 0, y: 0, w: src.width, h: src.height };
  const c = U.canvas(r.w, r.h), x = c.getContext('2d', { willReadFrequently: true });
  x.drawImage(src, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h);
  return x.getImageData(0, 0, r.w, r.h);
};
U.fromImageData = id => { const c = U.canvas(id.width, id.height); c.getContext('2d').putImageData(id, 0, 0); return c; };
U.contentBounds = (src) => {
  const w = src.width, h = src.height, id = U.readPixels(src), d = new Uint32Array(id.data.buffer);
  let top = -1, bottom = -1, left = w, right = -1;
  for (let y = 0; y < h && top < 0; y++) for (let x = 0, i = y * w; x < w; x++, i++) if (d[i] >>> 24) { top = y; break; }
  if (top < 0) return null;
  for (let y = h - 1; y >= top && bottom < 0; y--) for (let x = 0, i = y * w; x < w; x++, i++) if (d[i] >>> 24) { bottom = y; break; }
  for (let y = top; y <= bottom; y++) {
    const row = y * w;
    for (let x = 0; x < left; x++) if (d[row + x] >>> 24) { left = x; break; }
    for (let x = w - 1; x > right; x--) if (d[row + x] >>> 24) { right = x; break; }
  }
  return { x: left, y: top, w: right - left + 1, h: bottom - top + 1 };
};
U.clipCtx = (ctx, r) => { ctx.beginPath(); ctx.rect(r.x, r.y, r.w, r.h); ctx.clip(); };
U.canFilter = (() => { try { const c = U.canvas(2, 2).getContext('2d'); c.filter = 'blur(1px)'; return c.filter === 'blur(1px)'; } catch (e) { return false; } })();

/* ---------- прямоугольники ---------- */
U.union = (a, b) => {
  if (!a) return b ? { x: b.x, y: b.y, w: b.w, h: b.h } : null;
  if (!b) return { x: a.x, y: a.y, w: a.w, h: a.h };
  const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
};
U.clipRect = (r, W, H) => {
  if (!r) return null;
  const x0 = Math.max(0, Math.floor(r.x)), y0 = Math.max(0, Math.floor(r.y));
  const x1 = Math.min(W, Math.ceil(r.x + r.w)), y1 = Math.min(H, Math.ceil(r.y + r.h));
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
};
U.inflate = (r, n) => r && { x: r.x - n, y: r.y - n, w: r.w + 2 * n, h: r.h + 2 * n };
U.ptsBounds = pts => {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of pts) { const x = p[0] ?? p.x, y = p[1] ?? p.y; if (x < x0) x0 = x; if (y < y0) y0 = y; if (x > x1) x1 = x; if (y > y1) y1 = y; }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
};
U.inPoly = (x, y, poly) => {
  let ins = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) ins = !ins;
  }
  return ins;
};

/* ---------- цвет ---------- */
U.hsv2rgb = (h, s, v) => {
  h = ((h % 360) + 360) % 360 / 60;
  const c = v * s, x = c * (1 - Math.abs(h % 2 - 1)), m = v - c;
  let r = 0, g = 0, b = 0;
  if (h < 1) { r = c; g = x; } else if (h < 2) { r = x; g = c; } else if (h < 3) { g = c; b = x; }
  else if (h < 4) { g = x; b = c; } else if (h < 5) { r = x; b = c; } else { r = c; b = x; }
  return { r: Math.round((r + m) * 255), g: Math.round((g + m) * 255), b: Math.round((b + m) * 255) };
};
U.rgb2hsv = (r, g, b) => {
  r /= 255; g /= 255; b /= 255;
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn;
  let h = 0;
  if (d) { if (mx === r) h = ((g - b) / d) % 6; else if (mx === g) h = (b - r) / d + 2; else h = (r - g) / d + 4; h *= 60; if (h < 0) h += 360; }
  return { h, s: mx ? d / mx : 0, v: mx };
};
U.hex = c => '#' + [c.r, c.g, c.b].map(x => U.clamp(Math.round(x), 0, 255).toString(16).padStart(2, '0')).join('');
U.hex2rgb = hex => {
  hex = String(hex).replace('#', '').trim();
  if (hex.length === 3) hex = hex.split('').map(c => c + c).join('');
  const n = parseInt(hex.slice(0, 6), 16);
  if (isNaN(n)) return null;
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
};
U.rgbStr = (c, a = 1) => a >= 1 ? `rgb(${c.r},${c.g},${c.b})` : `rgba(${c.r},${c.g},${c.b},${a})`;
U.luma = c => (0.299 * c.r + 0.587 * c.g + 0.114 * c.b) / 255;

U.rng = seed => () => {
  seed |= 0; seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

/* ---------- DOM ---------- */
U.el = (tag, props, ...kids) => {
  const e = document.createElement(tag);
  if (props) for (const k in props) {
    const v = props[k];
    if (v == null || v === false) continue;
    if (k === 'class') e.className = v;
    else if (k === 'text') e.textContent = v;
    else if (k === 'html') e.innerHTML = v;
    else if (k === 'style' && typeof v === 'object') Object.assign(e.style, v);
    else if (k.startsWith('on') && typeof v === 'function') e.addEventListener(k.slice(2), v);
    else if (k in e && k !== 'list') { try { e[k] = v; } catch (_) { e.setAttribute(k, v); } }
    else e.setAttribute(k, v === true ? '' : v);
  }
  for (const c of kids.flat()) if (c != null && c !== false) e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  return e;
};
U.icon = (name, cls) => {
  const p = Icons[name] || Icons.dot;
  const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  s.setAttribute('viewBox', '0 0 24 24'); s.setAttribute('class', 'i' + (cls ? ' ' + cls : ''));
  s.innerHTML = p; return s;
};

let toastTimer = 0;
U.toast = (msg, ms = 1700) => {
  const t = U.$('#toast'); if (!t) return;
  t.textContent = msg; t.classList.add('show');
  clearTimeout(toastTimer); toastTimer = setTimeout(() => t.classList.remove('show'), ms);
};

U.dialog = ({ title, body, buttons = [{ text: 'OK', value: true, primary: true }], cls = '', onOpen, dismiss = null }) => new Promise(resolve => {
  const back = U.el('div', { class: 'modal-back' });
  const box = U.el('div', { class: 'modal ' + cls });
  if (title) box.append(U.el('h3', { text: title }));
  const content = U.el('div', { class: 'modal-body' });
  if (typeof body === 'string') content.innerHTML = body; else if (body) content.append(body);
  box.append(content);
  const foot = U.el('div', { class: 'modal-foot' });
  let done = false;
  const close = v => {
    if (done) return; done = true; back.remove();
    document.removeEventListener('keydown', onKey, true); resolve(v);
  };
  const press = b => { if (b.validate && !b.validate()) return; close(typeof b.value === 'function' ? b.value() : b.value); };
  for (const b of buttons) foot.append(U.el('button', { class: 'btn' + (b.primary ? ' primary' : '') + (b.danger ? ' danger' : ''), text: b.text, onclick: () => press(b) }));
  if (buttons.length) box.append(foot);
  back.append(box); U.$('#modals').append(back);
  back.addEventListener('pointerdown', e => { if (e.target === back) close(dismiss); });
  const onKey = e => {
    e.stopPropagation();
    if (e.key === 'Escape') close(dismiss);
    else if (e.key === 'Enter' && e.target.tagName !== 'TEXTAREA') { const p = buttons.find(b => b.primary); if (p) { e.preventDefault(); press(p); } }
  };
  document.addEventListener('keydown', onKey, true);
  if (onOpen) onOpen(box, close);
  const f = box.querySelector('input[type=text],input[type=number],textarea');
  if (f) setTimeout(() => { f.focus(); if (f.select) f.select(); }, 40);
});
U.prompt = async (title, value = '') => {
  const inp = U.el('input', { type: 'text', class: 'input', value });
  return U.dialog({ title, body: inp, buttons: [{ text: 'Отмена', value: null }, { text: 'Готово', primary: true, value: () => inp.value }] });
};
U.confirm = (title, text, ok = 'OK', danger = false) => U.dialog({
  title, body: U.el('div', { text }), buttons: [{ text: 'Отмена', value: false }, { text: ok, value: true, primary: !danger, danger }], dismiss: false
});

/* ---------- файлы ---------- */
U.download = (blob, name) => {
  const a = U.el('a', { href: URL.createObjectURL(blob), download: name });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 30000);
};
U.save = async (blob, name) => {
  if (window.MazokAndroid) {
    try {
      const b64 = await new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(String(r.result).split(',')[1] || ''); r.onerror = rej; r.readAsDataURL(blob); });
      const msg = window.MazokAndroid.saveFile(name, blob.type || 'application/octet-stream', b64);
      if (msg) U.toast(msg, 2600);
      return;
    } catch (e) { U.toast('Не удалось сохранить: ' + e.message, 3000); return; }
  }
  const touch = matchMedia('(pointer:coarse)').matches;
  if (touch && navigator.canShare) {
    try {
      const f = new File([blob], name, { type: blob.type || 'application/octet-stream' });
      if (navigator.canShare({ files: [f] })) { await navigator.share({ files: [f], title: name }); return; }
    } catch (e) { if (e && e.name === 'AbortError') return; }
  }
  U.download(blob, name);
};
U.pickFile = (accept, multiple = false, capture) => new Promise(resolve => {
  const inp = U.el('input', { type: 'file', accept, multiple, style: { display: 'none' } });
  if (capture) inp.setAttribute('capture', capture);
  inp.onchange = () => { resolve(Array.from(inp.files || [])); inp.remove(); };
  document.body.append(inp); inp.click();
});
U.loadImage = src => new Promise((res, rej) => {
  const img = new Image();
  const url = src instanceof Blob ? URL.createObjectURL(src) : src;
  img.onload = () => { res(img); if (src instanceof Blob) setTimeout(() => URL.revokeObjectURL(url), 1000); };
  img.onerror = () => rej(new Error('Не удалось открыть изображение'));
  img.src = url;
});
U.toBlob = (canvas, type = 'image/png', q) => new Promise(res => canvas.toBlob(b => res(b), type, q));
U.safeName = s => (s || 'Без названия').replace(/[\\/:*?"<>|]+/g, '_').trim() || 'Без названия';
U.fmtDate = t => new Date(t).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
U.fmtTime = s => { s = Math.round(s); const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60; return h ? `${h} ч ${m} мин` : `${m} мин ${s % 60} с`; };
U.debounce = (fn, ms) => { let t = 0; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };
U.rafThrottle = fn => { let id = 0, args; return (...a) => { args = a; if (!id) id = requestAnimationFrame(() => { id = 0; fn(...args); }); }; };

/* ---------- настройки ---------- */
const Settings = {
  theme: 'dark', lefty: false, touchDraw: true, fingerPressure: false, pressureGamma: 1, holdPick: true, holdDelay: 450,
  quickShape: true, quickShapeDelay: 650, cursor: true, rotateGesture: true, rotateSnap: true, historyMB: 600,
  timelapse: true, fillTol: 22, fillExpand: 1, fillSource: 'layer', pickSource: 'all', streamlineMul: 1,
  cloneAligned: true, uiMode: 'auto',
  load() {
    try { Object.assign(this, JSON.parse(localStorage.getItem('mazok.settings') || '{}')); } catch (e) { }
  },
  save() {
    const o = {};
    for (const k in this) if (typeof this[k] !== 'function') o[k] = this[k];
    try { localStorage.setItem('mazok.settings', JSON.stringify(o)); } catch (e) { }
  },
  pressure(p) { return Math.pow(U.clamp(p, 0, 1), Settings.pressureGamma); }
};

/* ---------- иконки (24×24, линии) ---------- */
const Icons = {
  dot: '<circle cx="12" cy="12" r="2"/>',
  gallery: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.6"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.6"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.6"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.6"/>',
  menu: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  adjust: '<path d="M4.5 19.5l10-10"/><path d="M13 8l3 3"/><path d="M17 3.5v3M15.5 5h3M20 9.5v2M19 10.5h2M9.5 3.5v2M8.5 4.5h2"/>',
  guide: '<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><path d="M12 3.5v17M3.5 12h17" stroke-dasharray="2 2.2"/>',
  select: '<path d="M5 9.5C5 6.5 8.1 4 12 4s7 2.5 7 5.5S15.9 15 12 15c-1.4 0-2.7-.3-3.8-.8" stroke-dasharray="2.4 2.4"/><path d="M7.5 13.5c-1.6 1.1-1.2 3.2.6 3.4 1.6.2 2.2 1.5 1.4 3.1"/>',
  move: '<path d="M12 3v18M3 12h18M12 3l-2.6 2.6M12 3l2.6 2.6M12 21l-2.6-2.6M12 21l2.6-2.6M3 12l2.6-2.6M3 12l2.6 2.6M21 12l-2.6-2.6M21 12l-2.6 2.6"/>',
  brush: '<path d="M19.5 3.5c.6.6.6 1.5 0 2.1l-7.8 7.8-2.1-2.1 7.8-7.8c.6-.6 1.5-.6 2.1 0z"/><path d="M9.6 11.3l-.7.1c-1.6.3-2.6 1.6-2.7 3.2-.1 1.6-1.2 3-2.7 3.4 1.3 1.2 3.3 1.7 5.2 1.1 2-.6 3.2-2.4 3.1-4.4l.1-1.2"/>',
  smudge: '<path d="M12 3.5c3.3 4.3 6 7.2 6 10.4a6 6 0 0 1-12 0c0-3.2 2.7-6.1 6-10.4z"/><path d="M9.3 14.5c.3 1.4 1.3 2.4 2.7 2.7"/>',
  eraser: '<path d="M14.8 4.6l4.6 4.6c.6.6.6 1.5 0 2.1L12 18.7H7.5l-3-3c-.6-.6-.6-1.5 0-2.1l8.2-9c.6-.6 1.5-.6 2.1 0z"/><path d="M9.5 8.8l5.7 5.7M12 18.7h8"/>',
  fill: '<path d="M4.5 11.5L11 5l7 7-6.5 6.5c-.6.6-1.5.6-2.1 0l-4.9-4.9c-.6-.6-.6-1.5 0-2.1z"/><path d="M4.5 12.5h13"/><path d="M8.5 3l2.5 2"/><path d="M20 15.5s1.6 2 1.6 3.1a1.6 1.6 0 0 1-3.2 0c0-1.1 1.6-3.1 1.6-3.1z"/>',
  picker: '<path d="M13.5 6.5l4 4"/><path d="M15.5 4.5l1.3-1.3a2.1 2.1 0 0 1 3 3L18.5 7.5"/><path d="M14.7 7.7L6 16.4l-.8 2.4 2.4-.8 8.7-8.7"/><path d="M14 4l6 6"/>',
  text: '<path d="M5 7V4.5h14V7M12 4.5v15M9 19.5h6"/>',
  layers: '<path d="M12 3.5l8.5 4.5-8.5 4.5L3.5 8z"/><path d="M3.5 12l8.5 4.5 8.5-4.5"/><path d="M3.5 16l8.5 4.5 8.5-4.5"/>',
  undo: '<path d="M9 14L4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  redo: '<path d="M15 14l5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  trash: '<path d="M4.5 7h15M9.5 7V4.8c0-.7.6-1.3 1.3-1.3h2.4c.7 0 1.3.6 1.3 1.3V7M6.5 7l.8 12c.1.8.7 1.5 1.5 1.5h6.4c.8 0 1.4-.7 1.5-1.5l.8-12"/>',
  eye: '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeoff: '<path d="M4 4l16 16"/><path d="M10 6c.6-.2 1.3-.3 2-.3 6 0 9.5 6.3 9.5 6.3s-1 1.8-2.8 3.5M6.5 7.6C4 9.3 2.5 12 2.5 12S6 18.3 12 18.3c1.6 0 3-.4 4.2-1"/>',
  lock: '<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
  alphalock: '<rect x="4" y="4" width="16" height="16" rx="2" stroke-dasharray="2.5 2"/><path d="M9 12.5h6M12 9.5v6"/>',
  clip: '<path d="M7 4v9a3 3 0 0 0 3 3h9"/><path d="M16 13l3 3-3 3"/>',
  more: '<circle cx="5.5" cy="12" r="1.3"/><circle cx="12" cy="12" r="1.3"/><circle cx="18.5" cy="12" r="1.3"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
  cut: '<circle cx="6.5" cy="17.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/><path d="M8.3 15.7L18 4M15.7 15.7L6 4"/>',
  paste: '<rect x="5" y="4.5" width="14" height="16" rx="2"/><path d="M9 4.5V3.5h6v1M9 11h6M9 15h4"/>',
  image: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="9.5" r="1.8"/><path d="M4 17l5-5 4 4 3-3 4 4"/>',
  camera: '<path d="M4 8.5h3l1.8-2.5h6.4L17 8.5h3v10H4z"/><circle cx="12" cy="13.2" r="3.3"/>',
  share: '<path d="M12 15V3.5M7.5 8L12 3.5 16.5 8"/><path d="M5 12v6.5c0 1.1.9 2 2 2h10c1.1 0 2-.9 2-2V12"/>',
  download: '<path d="M12 4v11M7.5 10.5L12 15l4.5-4.5"/><path d="M5 19.5h14"/>',
  play: '<path d="M8 5.5v13l10.5-6.5z"/>',
  pause: '<path d="M8.5 5.5v13M15.5 5.5v13"/>',
  grid: '<rect x="3.5" y="3.5" width="17" height="17" rx="1.5"/><path d="M9.2 3.5v17M14.8 3.5v17M3.5 9.2h17M3.5 14.8h17"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7"/>',
  help: '<circle cx="12" cy="12" r="8.5"/><path d="M9.6 9.4a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.1-2.4 3.7M12 17h.01"/>',
  flipH: '<path d="M12 3v18" stroke-dasharray="2 2"/><path d="M9 6.5L3.5 17H9zM15 6.5L20.5 17H15z"/>',
  flipV: '<path d="M3 12h18" stroke-dasharray="2 2"/><path d="M6.5 9L17 3.5V9zM6.5 15L17 20.5V15z"/>',
  rotate: '<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.5 4.5v4h-4"/>',
  crop: '<path d="M6.5 3v13.5H21M3 6.5h13.5V21"/>',
  film: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M7.5 4.5v15M16.5 4.5v15M3.5 9h4M3.5 15h4M16.5 9h4M16.5 15h4"/>',
  wave: '<path d="M3 9c3-3 6 3 9 0s6 3 9 0M3 15c3-3 6 3 9 0s6 3 9 0"/>',
  clone: '<path d="M9.5 3.5h5v5.2l2.5 2.3H7l2.5-2.3z"/><path d="M5.5 11h13v4h-13zM8 15v5.5h8V15"/>',
  star: '<path d="M12 3.5l2.6 5.5 6 .8-4.4 4.1 1.1 6-5.3-2.9-5.3 2.9 1.1-6L3.4 9.8l6-.8z"/>',
  folder: '<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  record: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="3.5"/>',
  palette: '<path d="M12 3.5a8.5 8.5 0 0 0 0 17c1.3 0 1.9-1 1.5-2-.5-1.2.3-2.5 1.6-2.5H17a3.5 3.5 0 0 0 3.5-3.5c0-4.9-3.8-9-8.5-9z"/><circle cx="7.8" cy="11" r="1.1"/><circle cx="10.5" cy="7.5" r="1.1"/><circle cx="15" cy="7.8" r="1.1"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5M12 7.8h.01"/>',
  window: '<rect x="3.5" y="5" width="17" height="14" rx="2"/><path d="M3.5 9h17"/>',
  shapes: '<circle cx="8" cy="8" r="4.5"/><rect x="11.5" y="11.5" width="9" height="9" rx="1"/>',
  sym: '<path d="M12 3v18" stroke-dasharray="2 2"/><path d="M9 7c-3 0-5 2.5-5 5s2 5 5 5M15 7c3 0 5 2.5 5 5s-2 5-5 5"/>',
  frames: '<rect x="3" y="6" width="5" height="12" rx="1"/><rect x="9.5" y="6" width="5" height="12" rx="1"/><rect x="16" y="6" width="5" height="12" rx="1"/>',
  edit: '<path d="M4.5 19.5l1-4L16 5a2.1 2.1 0 0 1 3 3L8.5 18.5z"/><path d="M14 7l3 3"/>',
  merge: '<path d="M7 4v5c0 2 1.5 3.5 3.5 3.5h3C15.5 12.5 17 14 17 16v4"/><path d="M17 4v5c0 2-1.5 3.5-3.5 3.5"/><path d="M14 17l3 3 3-3"/>',
  invert: '<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor"/>',
  hand: '<path d="M8 13V5.5a1.5 1.5 0 0 1 3 0V11M11 10V4.5a1.5 1.5 0 0 1 3 0V11M14 10.5V6a1.5 1.5 0 0 1 3 0v8c0 4-2.5 6.5-6 6.5-2.7 0-4-1.3-5.5-3.5L4 14.2a1.5 1.5 0 0 1 2.5-1.6L8 14.5"/>',
  wand: '<path d="M4.5 19.5l10-10"/><path d="M13 8l3 3"/><path d="M17 3.5v3M15.5 5h3M20 9.5v2M19 10.5h2"/>',
  rect: '<rect x="4" y="6" width="16" height="12" rx="1" stroke-dasharray="2.5 2"/>',
  ellipse: '<ellipse cx="12" cy="12" rx="8.5" ry="6.5" stroke-dasharray="2.5 2"/>',
  lasso: '<path d="M6 15c-2-1.5-2.5-4-1-6.5C7 5 13 3.5 17 5.5s3.5 7-1 9c-2.5 1-5.5 1-7.5.5" stroke-dasharray="2.5 2"/><path d="M8 14.5c-1.5 1.3-.7 3.2 1 3.3"/>',
  feather: '<path d="M20 4c-8 0-13 5-13 12v4M7 16c4 0 8-2 10-6M9.5 12.5H15"/>',
  fit: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>',
  record2: '<circle cx="12" cy="12" r="6" fill="currentColor"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5l5 5M8 10.5h5M10.5 8v5"/>',
  gradient: '<rect x="3.5" y="5" width="17" height="14" rx="2"/><path d="M8 5v14" stroke-opacity=".9"/><path d="M12 5v14" stroke-opacity=".6"/><path d="M16 5v14" stroke-opacity=".3"/>',
};
