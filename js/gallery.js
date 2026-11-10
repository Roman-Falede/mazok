'use strict';
/* Мазок — галерея, сохранение, импорт/экспорт, таймлапс, анимация */

const DocIO = {
  chain: Promise.resolve(),
  meta(doc) {
    return {
      id: doc.id, name: doc.name, w: doc.w, h: doc.h, bg: doc.bg, bgVisible: doc.bgVisible, active: doc.active, layerSeq: doc.layerSeq,
      anim: doc.anim, guide: doc.guide, stats: doc.stats, created: doc.created, modified: doc.modified, tlCount: doc.tlCount,
      layers: doc.layers.map(L => L.props())
    };
  },
  thumb(doc, max = 420) {
    const k = Math.min(1, max / Math.max(doc.w, doc.h)), c = U.canvas(doc.w * k, doc.h * k), x = c.getContext('2d');
    if (!doc.bgVisible) { x.fillStyle = '#fff'; x.fillRect(0, 0, c.width, c.height); }
    x.imageSmoothingQuality = 'high';
    x.drawImage(Engine.doc === doc && !doc.anim.on ? Engine.comp : this.flat(doc), 0, 0, c.width, c.height);
    return c;
  },
  flat(doc) {
    const c = U.canvas(doc.w, doc.h), x = c.getContext('2d');
    if (doc.bgVisible) { x.fillStyle = doc.bg; x.fillRect(0, 0, doc.w, doc.h); }
    for (const L of doc.layers) if (L.visible) { x.globalAlpha = L.opacity; x.globalCompositeOperation = L.blend; x.drawImage(L.canvas, 0, 0); }
    return c;
  },
  save(doc) {
    this.chain = this.chain.then(() => this._save(doc)).catch(e => { console.error(e); U.toast('Ошибка сохранения: ' + e.message, 3000); });
    return this.chain;
  },
  async _save(doc) {
    doc.modified = Date.now();
    for (const L of doc.layers) {
      if (!L.dirty) continue;
      const v = L.thumbV, b = await U.toBlob(L.canvas);
      await Store.put('layers', doc.id + '/' + L.id, b);
      if (L.thumbV === v) L.dirty = false;
    }
    const keys = await Store.keys('layers', doc.id + '/'), ids = new Set(doc.layers.map(L => doc.id + '/' + L.id));
    for (const k of keys) if (!ids.has(k)) await Store.del('layers', k);
    const m = this.meta(doc); m.thumb = await U.toBlob(this.thumb(doc), 'image/jpeg', 0.85);
    await Store.put('docs', doc.id, m);
  },
  async load(id) {
    const m = await Store.get('docs', id);
    if (!m) throw new Error('Рисунок не найден');
    const doc = new Doc(m);
    for (const lp of m.layers) {
      const L = new Layer(m.w, m.h, lp.name);
      Object.assign(L, lp); L.id = lp.id;
      const b = await Store.get('layers', id + '/' + lp.id);
      if (b) { const img = await this.decode(b); L.ctx.drawImage(img, 0, 0); if (img.close) img.close(); }
      L.dirty = false; doc.layers.push(L);
    }
    if (!doc.layers.length) doc.layers.push(new Layer(doc.w, doc.h, doc.nextName()));
    doc.active = U.clamp(m.active || 0, 0, doc.layers.length - 1); doc.layerSeq = m.layerSeq || doc.layers.length + 1;
    return doc;
  },
  decode(blob) { return window.createImageBitmap ? createImageBitmap(blob) : U.loadImage(blob); },
  async create(o) {
    const doc = new Doc(o);
    if (o.layers) for (const l of o.layers) { const L = new Layer(doc.w, doc.h, l.name || doc.nextName()); for (const k of ['opacity', 'blend', 'clip', 'visible', 'alphaLock']) if (l[k] !== undefined) L[k] = l[k]; if (l.canvas) L.ctx.drawImage(l.canvas, 0, 0); doc.layers.push(L); doc.layerSeq++; }
    else doc.layers.push(new Layer(doc.w, doc.h, doc.nextName()));
    doc.active = doc.layers.length - 1;
    await this._save(doc);
    return doc;
  },
  async serialize(doc) {
    const files = [{ name: 'doc.json', data: JSON.stringify(Object.assign(this.meta(doc), { format: 'mazok', version: 1 })) }];
    for (const L of doc.layers) files.push({ name: 'layers/' + L.id + '.png', data: await U.toBlob(L.canvas) });
    files.push({ name: 'preview.png', data: await U.toBlob(this.thumb(doc, 1024)) });
    return Zip.make(files);
  },
  async importFile(f) {
    const n = f.name.toLowerCase();
    if (n.endsWith('.mazok') || n.endsWith('.zip')) {
      const z = await Zip.read(f), j = z.get('doc.json'); if (!j) throw new Error('В архиве нет doc.json');
      const m = JSON.parse(new TextDecoder().decode(j)), layers = [];
      for (const lp of m.layers) { const d = z.get('layers/' + lp.id + '.png'); const img = d ? await this.decode(new Blob([d], { type: 'image/png' })) : null; const c = U.canvas(m.w, m.h); if (img) c.getContext('2d').drawImage(img, 0, 0); layers.push(Object.assign({}, lp, { canvas: c })); }
      const o = Object.assign({}, m, { id: U.uid(), layers, tlCount: 0, name: m.name });
      const doc = await this.create(o);
      doc.layers.forEach((L, i) => { L.text = m.layers[i].text || null; L.locked = !!m.layers[i].locked; L.reference = !!m.layers[i].reference; });
      return doc;
    }
    if (n.endsWith('.psd')) { const p = await PSD.read(f); return this.create({ name: f.name.replace(/\.[^.]+$/, ''), w: p.w, h: p.h, layers: p.layers, bgVisible: false }); }
    const img = await U.loadImage(f);
    let w = img.naturalWidth || img.width, h = img.naturalHeight || img.height;
    const k = Math.min(1, 8192 / Math.max(w, h), Math.sqrt(36e6 / (w * h))); w = Math.round(w * k); h = Math.round(h * k);
    const c = U.canvas(w, h); c.getContext('2d').drawImage(img, 0, 0, w, h);
    return this.create({ name: f.name.replace(/\.[^.]+$/, ''), w, h, layers: [{ name: 'Изображение', canvas: c }] });
  },
  async remove(id) { await Store.del('docs', id); await Store.delPrefix('layers', id + '/'); await Store.delPrefix('frames', id + '/'); },
  async duplicate(id) {
    const doc = await this.load(id);
    const o = Object.assign(this.meta(doc), { id: U.uid(), name: doc.name + ' (копия)', tlCount: 0, created: Date.now(), layers: doc.layers.map(L => Object.assign(L.props(), { canvas: L.canvas })) });
    await this.create(o);
  }
};

const Gallery = {
  el: null, urls: [], selMode: false, sel: new Set(),
  async show() {
    this.el = U.$('#gallery'); this.el.classList.remove('hidden');
    document.title = 'Мазок — галерея';
    await this.render();
  },
  hide() { U.$('#gallery').classList.add('hidden'); },
  async render() {
    const el = this.el; this.urls.forEach(u => URL.revokeObjectURL(u)); this.urls = [];
    const docs = (await Store.all('docs')).sort((a, b) => b.modified - a.modified);
    el.innerHTML = '';
    const head = U.el('div', { class: 'g-head' },
      U.el('h1', null, U.el('img', { src: 'icons/icon.svg', alt: '' }), 'Мазок'),
      this.selMode ? U.el('button', { class: 'btn', text: `Удалить (${this.sel.size})`, onclick: () => this.deleteSelected() }) : null,
      this.selMode ? U.el('button', { class: 'btn', text: `Дублировать`, onclick: async () => { for (const id of this.sel) await DocIO.duplicate(id); this.selMode = false; this.sel.clear(); this.render(); } }) : null,
      U.el('button', { class: 'btn', text: this.selMode ? 'Готово' : 'Выбрать', onclick: () => { this.selMode = !this.selMode; this.sel.clear(); this.render(); } }),
      U.el('button', { class: 'btn', title: 'Импорт изображений, PSD или .mazok', onclick: () => this.importDialog() }, 'Импорт'),
      U.el('button', { class: 'btn primary', onclick: () => this.newDialog() }, '+ Холст'));
    const grid = U.el('div', { class: 'g-grid' });
    if (!docs.length) grid.append(U.el('div', { class: 'g-empty', html: '<h2>Здесь будут ваши рисунки</h2><p>Нажмите «+ Холст», чтобы начать, или перетащите сюда изображение, PSD или файл .mazok.</p>' }));
    for (const m of docs) {
      const url = m.thumb ? URL.createObjectURL(m.thumb) : ''; if (url) this.urls.push(url);
      const card = U.el('div', { class: 'card' + (this.sel.has(m.id) ? ' sel' : '') },
        U.el('div', { class: 'im' }, url ? U.el('img', { src: url, alt: '' }) : null),
        U.el('div', { class: 'cn', text: m.name }), U.el('div', { class: 'cs', text: `${m.w}×${m.h} · ${U.fmtDate(m.modified)}` }));
      if (this.selMode) card.append(U.el('div', { class: 'chk' }));
      else card.append(U.el('button', { class: 'more', title: 'Ещё', onclick: e => { e.stopPropagation(); this.cardMenu(m); } }, U.icon('more', 'sm')));
      card.addEventListener('click', () => {
        if (this.selMode) { this.sel.has(m.id) ? this.sel.delete(m.id) : this.sel.add(m.id); this.render(); }
        else App.open(m.id);
      });
      grid.append(card);
    }
    el.append(head, grid);
  },
  async deleteSelected() {
    if (!this.sel.size) return;
    if (!(await U.confirm('Удалить рисунки?', `Будет удалено: ${this.sel.size}. Это нельзя отменить.`, 'Удалить', true))) return;
    for (const id of this.sel) await DocIO.remove(id);
    this.sel.clear(); this.selMode = false; this.render();
  },
  cardMenu(m) {
    const body = U.el('div'); let close;
    const it = (icon, label, fn) => body.append(UI.item(icon, label, async () => { close(); await fn(); }));
    it('edit', 'Переименовать', async () => { const n = await U.prompt('Название', m.name); if (n) { m.name = n; await Store.put('docs', m.id, m); this.render(); } });
    it('copy', 'Дублировать', async () => { await DocIO.duplicate(m.id); this.render(); });
    it('share', 'Поделиться PNG', async () => { const d = await DocIO.load(m.id); U.save(await U.toBlob(DocIO.flat(d)), U.safeName(m.name) + '.png'); });
    it('folder', 'Экспорт .mazok', async () => { const d = await DocIO.load(m.id); U.save(await DocIO.serialize(d), U.safeName(m.name) + '.mazok'); });
    it('trash', 'Удалить', async () => { if (await U.confirm('Удалить рисунок?', m.name + ' — это нельзя отменить.', 'Удалить', true)) { await DocIO.remove(m.id); this.render(); } });
    U.dialog({ title: m.name, body, buttons: [{ text: 'Закрыть', value: null }], onOpen: (b, c) => { close = c; } });
  },
  async importDialog() {
    const files = await U.pickFile('image/*,.psd,.mazok,.zip', true);
    await this.importFiles(files);
  },
  async importFiles(files) {
    let last = null;
    for (const f of files) { try { U.toast('Импорт: ' + f.name); last = await DocIO.importFile(f); } catch (e) { U.toast(f.name + ': ' + e.message, 3000); } }
    await this.render();
    if (last && files.length === 1) App.openDoc(last);
  },
  newDialog() {
    const dpr = window.devicePixelRatio || 1;
    const sw = Math.min(4096, Math.round(screen.width * dpr)), sh = Math.min(4096, Math.round(screen.height * dpr));
    const P = [['Экран', sw, sh], ['Квадрат', 2048, 2048], ['A4, 300 dpi', 2480, 3508], ['A5, 300 dpi', 1748, 2480], ['Full HD', 1920, 1080], ['4K', 3840, 2160], ['Сторис 9:16', 1080, 1920], ['Пост 4:5', 1080, 1350], ['Комикс', 2480, 3508], ['Пиксель-арт', 128, 128], ['Иконка', 512, 512], ['Своё', 0, 0]];
    let sel = 1;
    const W = U.el('input', { class: 'input', type: 'number', min: 16, max: 8192, value: 2048 }), H = U.el('input', { class: 'input', type: 'number', min: 16, max: 8192, value: 2048 });
    const name = U.el('input', { class: 'input', type: 'text', value: 'Без названия' });
    const bg = U.el('input', { type: 'color', value: '#ffffff', style: { width: '100%', height: '36px', border: 0, background: 'none' } });
    let transparent = false;
    const info = U.el('div', { class: 'row', style: { padding: '4px 0' } });
    const upd = () => { const w = +W.value, h = +H.value; const mem = (navigator.deviceMemory || 4) * 1073741824 * 0.2; info.innerHTML = `<small>${w} × ${h} px · до ${U.clamp(Math.floor(mem / (w * h * 4)) - 6, 3, 250)} слоёв на этом устройстве</small>`; };
    W.oninput = H.oninput = () => { sel = P.length - 1; drawP(); upd(); };
    const grid = U.el('div', { class: 'presets' });
    const drawP = () => {
      grid.innerHTML = '';
      P.forEach(([n, w, h], i) => grid.append(U.el('button', { class: i === sel ? 'on' : '', onclick: () => { sel = i; if (w) { W.value = w; H.value = h; } drawP(); upd(); } }, n, U.el('small', { text: w ? `${w} × ${h}` : 'укажите размер' }))));
    };
    drawP(); upd();
    const F = (l, e) => U.el('div', { class: 'field' }, U.el('label', { text: l }), e);
    const body = U.el('div', null, F('Название', name), grid, U.el('div', { class: 'grid2' }, F('Ширина, px', W), F('Высота, px', H)), info,
      U.el('div', { class: 'grid2' }, F('Цвет фона', bg), U.el('div', null, UI.sw('Прозрачный фон', false, v => { transparent = v; }))));
    U.dialog({ title: 'Новый холст', body, cls: 'wide', buttons: [{ text: 'Отмена', value: null }, { text: 'Создать', primary: true, value: () => true }] }).then(async ok => {
      if (!ok) return;
      const w = U.clamp(Math.round(+W.value), 16, 8192), h = U.clamp(Math.round(+H.value), 16, 8192);
      if (w * h > 50e6) { U.toast('Слишком большой холст'); return; }
      const doc = await DocIO.create({ name: name.value.trim() || 'Без названия', w, h, bg: bg.value, bgVisible: !transparent });
      App.openDoc(doc);
    });
  }
};

/* ---------- экспорт ---------- */
const Exporter = {
  name() { return U.safeName(Engine.doc.name); },
  async image(type) {
    Tools.commitPending();
    const d = Engine.doc; let c = Engine.flatten();
    if (type !== 'png' && !d.bgVisible) { const t = U.canvas(d.w, d.h), x = t.getContext('2d'); x.fillStyle = '#fff'; x.fillRect(0, 0, d.w, d.h); x.drawImage(c, 0, 0); c = t; }
    const blob = await U.toBlob(c, 'image/' + type, 0.93);
    if (!blob || (type !== 'png' && blob.type !== 'image/' + type)) { U.toast('Формат не поддерживается этим браузером'); return; }
    U.save(blob, this.name() + '.' + (type === 'jpeg' ? 'jpg' : type));
  },
  async psd() { Tools.commitPending(); U.toast('Создаю PSD…'); await U.sleep(30); U.save(PSD.write(Engine.doc), this.name() + '.psd'); },
  async mazok() { Tools.commitPending(); U.toast('Упаковываю проект…'); U.save(await DocIO.serialize(Engine.doc), this.name() + '.mazok'); },
  async layersZip() {
    const d = Engine.doc, files = [];
    for (let i = 0; i < d.layers.length; i++) files.push({ name: String(i + 1).padStart(2, '0') + ' ' + U.safeName(d.layers[i].name) + '.png', data: await U.toBlob(d.layers[i].canvas) });
    U.save(await Zip.make(files), this.name() + ' — слои.zip');
  },
  async framesZip() {
    const d = Engine.doc, files = [];
    for (let i = 0; i < d.layers.length; i++) files.push({ name: 'кадр ' + String(i + 1).padStart(3, '0') + '.png', data: await U.toBlob(Engine.frame(i)) });
    U.save(await Zip.make(files), this.name() + ' — кадры.zip');
  },
  async askScale(title) {
    const d = Engine.doc, max = Math.max(d.w, d.h);
    const opts = [[1, `Оригинал (${d.w}×${d.h})`], [Math.min(1, 1024 / max), 'До 1024 px'], [Math.min(1, 512 / max), 'До 512 px']];
    let k = max > 1024 ? opts[1][0] : 1;
    return U.dialog({ title, body: U.el('div', null, U.el('div', { class: 'field' }, U.el('label', { text: 'Размер' }), UI.seg(opts, k, v => { k = v; })), U.el('small', { text: `Кадров: ${d.layers.length} · ${d.anim.fps} кадр/с (настраивается в ассистенте анимации)` })),
      buttons: [{ text: 'Отмена', value: null }, { text: 'Экспорт', primary: true, value: () => k }] });
  },
  async gif() {
    const k = await this.askScale('Анимированный GIF'); if (!k) return;
    const d = Engine.doc, w = Math.max(1, Math.round(d.w * k)), h = Math.max(1, Math.round(d.h * k)), frames = [];
    U.toast('Создаю GIF…', 4000); await U.sleep(30);
    for (let i = 0; i < d.layers.length; i++) { const c = U.canvas(w, h), x = c.getContext('2d', { willReadFrequently: true }); x.imageSmoothingQuality = 'high'; x.drawImage(Engine.frame(i), 0, 0, w, h); frames.push(x.getImageData(0, 0, w, h)); }
    const order = d.anim.mode === 'pingpong' ? frames.concat(frames.slice(1, -1).reverse()) : frames;
    U.save(GIF.encode(order, w, h, Math.round(100 / d.anim.fps), !d.bgVisible), this.name() + '.gif');
  },
  async animVideo() {
    const k = await this.askScale('Видео анимации'); if (!k) return;
    const d = Engine.doc, n = d.layers.length, w = Math.round(d.w * k), h = Math.round(d.h * k);
    const seq = d.anim.mode === 'pingpong' ? [...Array(n).keys()].concat([...Array(n).keys()].slice(1, -1).reverse()) : [...Array(n).keys()];
    const loops = Math.max(1, Math.ceil(d.anim.fps * 4 / seq.length)), frames = seq.map(i => Engine.frame(i));
    await this.video(w, h, seq.length * loops, d.anim.fps, (x, i) => { x.fillStyle = '#fff'; x.fillRect(0, 0, w, h); x.drawImage(frames[i % seq.length], 0, 0, w, h); }, this.name() + ' — анимация');
  },
  async video(w, h, count, fps, draw, name) {
    const bar = U.el('div', { style: { height: '6px', background: 'var(--accent)', width: '0%', borderRadius: '3px', transition: 'width .2s' } });
    let closeFn; U.dialog({ title: 'Запись видео…', body: U.el('div', null, U.el('small', { text: 'Видео записывается в реальном времени, не закрывайте приложение.' }), U.el('div', { style: { background: 'var(--panel3)', borderRadius: '3px', marginTop: '12px' } }, bar)), buttons: [], onOpen: (b, c) => { closeFn = c; } });
    try {
      const r = await VideoRec.record(w, h, count, fps, draw, p => { bar.style.width = (p * 100).toFixed(1) + '%'; });
      closeFn(); U.save(r.blob, U.safeName(name) + '.' + r.ext);
    } catch (e) { closeFn(); U.toast(e.message, 3000); }
  }
};

/* ---------- таймлапс ---------- */
const Timelapse = {
  key(id, n) { return id + '/' + String(n).padStart(7, '0'); },
  capture: U.debounce(() => Timelapse.now(), 400),
  async now() {
    const d = Engine.doc; if (!d || !Engine.comp || this.busy) return;
    this.busy = true;
    try {
      const k = Math.min(1, 1080 / Math.max(d.w, d.h)), c = U.canvas(d.w * k, d.h * k), x = c.getContext('2d');
      x.fillStyle = '#fff'; x.fillRect(0, 0, c.width, c.height); x.imageSmoothingQuality = 'high'; x.drawImage(Engine.comp, 0, 0, c.width, c.height);
      const b = await U.toBlob(c, 'image/jpeg', 0.82);
      if (b && Engine.doc === d) { await Store.put('frames', this.key(d.id, d.tlCount), b); d.tlCount++; }
    } finally { this.busy = false; }
  },
  async keys() { const d = Engine.doc; return (await Store.keys('frames', d.id + '/')).sort(); },
  async clear() { const d = Engine.doc; await Store.delPrefix('frames', d.id + '/'); d.tlCount = 0; App.changed(); U.toast('Таймлапс очищен'); },
  async replay() {
    const keys = await this.keys();
    if (!keys.length) { U.toast('Таймлапс пуст — порисуйте, и он запишется автоматически'); return; }
    const d = Engine.doc, cv = U.el('canvas', { style: { width: '100%', maxHeight: '62vh', objectFit: 'contain', background: '#fff', borderRadius: '10px', display: 'block' } });
    const k = Math.min(1, 1080 / Math.max(d.w, d.h)); cv.width = Math.round(d.w * k); cv.height = Math.round(d.h * k);
    const x = cv.getContext('2d'), range = U.el('input', { type: 'range', min: 0, max: keys.length - 1, value: 0 });
    let i = 0, playing = true, speed = 1, timer = 0, alive = true;
    const show = async n => { const b = await Store.get('frames', keys[n]); if (!b || !alive) return; const im = await DocIO.decode(b); x.drawImage(im, 0, 0, cv.width, cv.height); if (im.close) im.close(); };
    const play = U.el('button', { class: 'btn', text: '⏸' });
    const tick = async () => { if (!alive || !playing) return; await show(i); range.value = i; i = i + 1 >= keys.length ? (playing = false, play.textContent = '▶', keys.length - 1) : i + 1; timer = setTimeout(tick, 1000 / (30 * speed)); };
    play.onclick = () => { playing = !playing; play.textContent = playing ? '⏸' : '▶'; if (playing) { if (i >= keys.length - 1) i = 0; tick(); } };
    range.oninput = () => { i = +range.value; show(i); };
    const body = U.el('div', null, cv, U.el('div', { class: 'row' }, play, range, UI.seg([[0.5, '½×'], [1, '1×'], [2, '2×'], [4, '4×']], 1, v => { speed = v; })));
    U.dialog({ title: `Таймлапс — ${keys.length} кадров`, cls: 'wide', body, buttons: [{ text: 'Экспорт видео', value: 'exp' }, { text: 'Закрыть', primary: true, value: null }] }).then(v => { alive = false; clearTimeout(timer); if (v === 'exp') this.export(false); });
    tick();
  },
  async export(short) {
    const keys = await this.keys();
    if (!keys.length) { U.toast('Таймлапс пуст'); return; }
    const d = Engine.doc, fps = 30, k = Math.min(1, 1080 / Math.max(d.w, d.h)), w = Math.round(d.w * k), h = Math.round(d.h * k);
    let seq = keys; if (short && keys.length > fps * 30) { const n = fps * 30; seq = Array.from({ length: n }, (_, i) => keys[Math.floor(i * keys.length / n)]); }
    const tail = fps * 2, total = seq.length + tail;
    await Exporter.video(w, h, total, fps, async (x, i) => {
      const key = seq[Math.min(i, seq.length - 1)];
      if (i < seq.length || i === seq.length) { const b = await Store.get('frames', key); if (b) { const im = await DocIO.decode(b); x.drawImage(im, 0, 0, w, h); if (im.close) im.close(); } }
    }, Exporter.name() + ' — таймлапс');
  }
};

/* ---------- ассистент анимации ---------- */
const Anim = {
  playing: false, frame: 0, timer: 0, dir: 1,
  toggle(on) {
    const d = Engine.doc; d.anim.on = on; this.stop();
    document.body.classList.toggle('anim', on); this.refreshBar(); Engine.renderAll(); App.changed();
    if (on) UI.hint('Каждый слой — кадр. Нижний слой — первый кадр.', 2200);
  },
  refreshBar() {
    const bar = U.$('#animbar'), d = Engine.doc;
    if (!d || !d.anim.on) { bar.classList.add('hidden'); document.body.classList.remove('anim'); return; }
    bar.classList.remove('hidden'); bar.innerHTML = '';
    const frames = U.el('div', { class: 'frames' });
    d.layers.forEach((L, i) => {
      const f = U.el('button', { class: 'fr' + (i === (this.playing ? this.frame : d.active) ? ' on' : ''), title: L.name, onclick: () => { if (this.playing) this.stop(); Ops.select(i); Engine.renderAll(); this.refreshBar(); } }, L.thumb(58, 44), U.el('b', { text: i + 1 }));
      frames.append(f);
    });
    bar.append(
      U.el('button', { class: 'tb', title: 'Настройки анимации', onclick: () => this.settings() }, U.icon('settings', 'sm')),
      U.el('button', { class: 'tb', title: this.playing ? 'Стоп' : 'Воспроизвести', onclick: () => this.playing ? this.stop() : this.play() }, U.icon(this.playing ? 'pause' : 'play', 'sm')),
      frames,
      U.el('button', { class: 'tb', title: 'Добавить кадр', onclick: () => { Ops.add(); setTimeout(() => { const fr = U.$('#animbar .fr.on'); fr && fr.scrollIntoView({ inline: 'nearest' }); }, 50); } }, U.icon('plus', 'sm')));
    const on = frames.querySelector('.on'); if (on) on.scrollIntoView({ inline: 'nearest', block: 'nearest' });
  },
  play() {
    const d = Engine.doc; if (d.layers.length < 2) { U.toast('Добавьте хотя бы два кадра'); return; }
    this.playing = true; this.frame = d.active; this.dir = 1;
    const step = () => {
      const n = d.layers.length, m = d.anim.mode;
      let f = this.frame + this.dir;
      if (m === 'pingpong') { if (f >= n || f < 0) { this.dir *= -1; f = this.frame + this.dir; } }
      else if (f >= n) { if (m === 'once') { this.stop(); return; } f = d.anim.holdFirst ? Math.min(1, n - 1) : 0; }
      this.frame = U.clamp(f, 0, n - 1); Engine.renderAll(); this.markFrame();
      this.timer = setTimeout(step, 1000 / d.anim.fps);
    };
    this.refreshBar(); this.timer = setTimeout(step, 1000 / d.anim.fps); Engine.renderAll();
  },
  markFrame() { U.$$('#animbar .fr').forEach((f, i) => f.classList.toggle('on', i === this.frame)); },
  stop() { clearTimeout(this.timer); if (this.playing) { this.playing = false; Engine.renderAll(); this.refreshBar(); } },
  settings() {
    const a = Engine.doc.anim, body = U.el('div');
    body.append(UI.hslider('Кадров в секунду', 1, 60, a.fps, 1, v => { a.fps = v; }));
    body.append(U.el('div', { class: 'field' }, U.el('label', { text: 'Воспроизведение' }), UI.seg([['loop', 'Цикл'], ['pingpong', 'Пинг-понг'], ['once', 'Один раз']], a.mode, v => { a.mode = v; })));
    body.append(UI.hslider('Кадры луковой кожуры', 0, 10, a.onion, 1, v => { a.onion = v; Engine.renderAll(); }));
    body.append(UI.hslider('Непрозрачность кожуры', 5, 100, Math.round(a.onionOpacity * 100), 1, v => { a.onionOpacity = v / 100; Engine.renderAll(); }, v => v + '%'));
    body.append(UI.sw('Цветная кожура', a.onionColor, v => { a.onionColor = v; Engine.renderAll(); }, 'Предыдущие кадры — красные, следующие — зелёные'));
    body.append(UI.sw('Первый кадр — фон', a.holdFirst, v => { a.holdFirst = v; Engine.renderAll(); }, 'Нижний слой виден во всех кадрах'));
    U.dialog({ title: 'Настройки анимации', body, buttons: [{ text: 'Готово', primary: true, value: true }] }).then(() => App.changed());
  }
};
