'use strict';
/* Мазок — настольный интерфейс: меню, панель параметров, инструменты, док панелей, строка состояния */

const Desk = {
  built: false, menuEl: null, menuIdx: -1, dock: null, state: null, statusEls: {},
  groups: [
    { id: 'g1', tabs: [['color', 'Цвет'], ['swatches', 'Образцы']] },
    { id: 'g2', tabs: [['brushes', 'Кисти'], ['brushset', 'Настройки кисти']] },
    { id: 'g3', tabs: [['layers', 'Слои'], ['history', 'История']] }
  ],
  tools: [
    ['transform', 'move', 'Перемещение и трансформация (V)'], ['select:rect', 'rect', 'Прямоугольное выделение (M)'], ['select:ellipse', 'ellipse', 'Овальное выделение (M)'],
    ['select:lasso', 'lasso', 'Лассо (L)'], ['select:wand', 'wand', 'Волшебная палочка (W)'], null,
    ['brush', 'brush', 'Кисть (B)'], ['eraser', 'eraser', 'Ластик (E)'], ['smudge', 'smudge', 'Палец (R)'], ['clone', 'clone', 'Штамп (S)'],
    ['fill', 'fill', 'Заливка (G)'], ['gradient', 'gradient', 'Градиент (Shift+G)'], null,
    ['text', 'text', 'Текст (T)'], ['shapes', 'shapes', 'Фигуры (U)'], ['picker', 'picker', 'Пипетка (I)'], null,
    ['hand', 'hand', 'Рука (H, пробел)'], ['zoom', 'search', 'Масштаб (Z)']
  ],
  load() {
    try { this.state = JSON.parse(localStorage.getItem('mazok.dock') || 'null'); } catch (e) { }
    if (!this.state) this.state = { g1: { tab: 'color', open: true }, g2: { tab: 'brushes', open: true }, g3: { tab: 'layers', open: true } };
  },
  saveState() { try { localStorage.setItem('mazok.dock', JSON.stringify(this.state)); } catch (e) { } },
  build() {
    if (this.built) return; this.built = true; this.load();
    /* меню */
    const mb = U.$('#menubar');
    mb.append(U.el('img', { src: 'icons/icon.svg', class: 'mb-logo', alt: '' }));
    this.menus().forEach((m, i) => mb.append(U.el('button', { class: 'mb-item', text: m.label, onpointerdown: e => { e.preventDefault(); this.menuIdx === i ? this.closeMenus() : this.openMenu(i); }, onpointerenter: () => { if (this.menuEl && this.menuIdx !== i) this.openMenu(i); } })));
    mb.append(U.el('div', { class: 'grow' }), this.titleEl = U.el('div', { class: 'mb-title', onclick: () => App.rename() }));
    /* инструменты */
    const tb = U.$('#toolbar');
    for (const t of this.tools) {
      if (!t) { tb.append(U.el('div', { class: 'tb-sep' })); continue; }
      tb.append(U.el('button', { class: 'tbtn', 'data-t': t[0], title: t[2], onclick: () => this.pickTool(t[0]) }, U.icon(t[1], 'sm')));
    }
    this.fgbg = U.el('div', { class: 'fgbg' },
      U.el('button', { class: 'bgc', title: 'Фоновый цвет (X — поменять местами)', onclick: () => Colors.swap() }),
      U.el('button', { class: 'fgc', title: 'Основной цвет', onclick: e => UI.pop('color', e.currentTarget, () => ColorUI.build(), { title: 'Основной цвет', width: 324 }) }),
      U.el('button', { class: 'sw-x', title: 'Поменять местами (X)', text: '⇄', onclick: () => Colors.swap() }),
      U.el('button', { class: 'sw-d', title: 'Чёрный и белый (D)', onclick: () => Desk.defaultColors() }));
    tb.append(U.el('div', { class: 'grow' }), this.fgbg);
    /* док */
    this.dock = U.$('#dock');
    for (const g of this.groups) {
      const st = this.state[g.id] || (this.state[g.id] = { tab: g.tabs[0][0], open: true });
      const head = U.el('div', { class: 'dk-h' }), body = U.el('div', { class: 'dk-b' }), el = U.el('section', { class: 'dk-g', 'data-g': g.id }, head, body);
      g.el = el; g.head = head; g.body = body;
      this.dock.append(el);
    }
    /* строка состояния */
    const sb = U.$('#statusbar');
    this.statusEls.zoom = U.el('input', { class: 'sb-zoom', title: 'Масштаб, %', onchange: e => { const v = parseFloat(e.target.value); if (v > 0) { View.zoomAt(View.W / 2, View.H / 2, v / 100 / View.s); } } });
    this.statusEls.size = U.el('span'); this.statusEls.layer = U.el('span'); this.statusEls.hint = U.el('span', { class: 'grow' }); this.statusEls.pos = U.el('span', { class: 'sb-pos' });
    sb.append(this.statusEls.zoom, this.statusEls.size, this.statusEls.layer, this.statusEls.hint, this.statusEls.pos);
    /* вкладка документа */
    this.tabEl = U.el('div', { class: 'dt-name' });
    U.$('#doctab').append(this.tabEl, U.el('button', { class: 'dt-x', title: 'Закрыть и вернуться в галерею', onclick: () => App.toGallery() }, U.icon('close', 'sm')));
    document.addEventListener('pointerdown', e => { if (this.menuEl && !e.target.closest('.mb-drop') && !e.target.closest('#menubar')) this.closeMenus(); }, true);
    this.renderDock();
  },
  defaultColors() { Colors.prev = { h: 0, s: 0, v: 1 }; Colors.set({ h: 0, s: 0, v: 0 }, true); },
  pickTool(id) {
    const [tool, shape] = id.split(':');
    if (tool === 'select') { SelectTool.shape = shape; SelectTool.poly = []; if (Tools.cur === 'select') { this.renderOptions(); this.sync(); return; } }
    if (Tools.cur === tool && ['brush', 'smudge', 'eraser', 'clone'].includes(tool)) { this.showPanel('brushes'); return; }
    Tools.set(tool);
  },
  sync() {
    if (!this.built) return;
    U.$$('#toolbar .tbtn').forEach(b => { const [t, sh] = b.dataset.t.split(':'); b.classList.toggle('on', t === Tools.cur && (!sh || sh === SelectTool.shape)); });
    this.fgbg.querySelector('.fgc').style.background = Colors.hex();
    this.fgbg.querySelector('.bgc').style.background = U.hex(Colors.prevRgb());
  },
  /* ---------- док ---------- */
  renderDock() { for (const g of this.groups) this.renderGroup(g); },
  renderGroup(g) {
    const st = this.state[g.id];
    g.head.innerHTML = '';
    for (const [id, name] of g.tabs) g.head.append(U.el('button', { class: 'dk-t' + (st.tab === id ? ' on' : ''), text: name, onclick: () => { st.tab = id; st.open = true; this.saveState(); this.renderGroup(g); } }));
    g.head.append(U.el('div', { class: 'grow' }));
    if (st.tab === 'layers') g.head.append(U.el('button', { class: 'dk-a', title: 'Новый слой', onclick: () => { Tools.commitPending(); Ops.add(); } }, U.icon('plus', 'sm')));
    if (st.tab === 'brushes') g.head.append(Panels.brushHead());
    g.head.append(U.el('button', { class: 'dk-a', title: st.open ? 'Свернуть' : 'Развернуть', onclick: () => { st.open = !st.open; this.saveState(); this.renderGroup(g); } }, U.el('span', { text: st.open ? '▾' : '▸' })));
    g.el.classList.toggle('closed', !st.open);
    g.body.innerHTML = '';
    if (!st.open) return;
    g.body.append(this.panel(st.tab));
  },
  panel(id) {
    if (!Engine.doc && (id === 'layers' || id === 'history')) return U.el('div', { class: 'row', html: '<small>Откройте рисунок</small>' });
    switch (id) {
      case 'color': { ColorUI.compact = true; const p = ColorUI.build(); ColorUI.compact = false; return p; }
      case 'swatches': { const d = U.el('div'); ColorUI.palettes(d); return d; }
      case 'brushes': { const p = Panels.brushLib(); p.style.height = '100%'; return p; }
      case 'brushset': return Panels.brushSettings();
      case 'layers': return Panels.layers(true);
      case 'history': return Panels.history();
    }
    return U.el('div');
  },
  refreshPanel(id) {
    if (!this.built || !UI.desk) return;
    for (const g of this.groups) {
      const st = this.state[g.id];
      if (st.tab === id && st.open) { const sc = g.body.scrollTop, inner = g.body.querySelector('.blist'), isc = inner ? inner.scrollTop : 0; g.body.innerHTML = ''; g.body.append(this.panel(id)); g.body.scrollTop = sc; const ni = g.body.querySelector('.blist'); if (ni) ni.scrollTop = isc; }
    }
  },
  panelEl(id) { for (const g of this.groups) { const st = this.state[g.id]; if (st.tab === id && st.open) return g.body.firstChild; } return null; },
  showPanel(id) {
    if (!UI.desk) { if (id === 'brushset') Panels.studio(Tools.brush().id); else if (id === 'brushes') UI.toggleBrushLib(); return; }
    for (const g of this.groups) if (g.tabs.some(t => t[0] === id)) { const st = this.state[g.id]; st.tab = id; st.open = true; this.saveState(); this.renderGroup(g); g.el.scrollIntoView({ block: 'nearest' }); }
  },
  /* ---------- панель параметров ---------- */
  renderOptions() {
    if (!this.built || !UI.desk) return;
    const el = U.$('#options'); el.innerHTML = '';
    const map = { brush: 'paint', smudge: 'paint', eraser: 'paint', clone: 'paint', fill: 'fill', select: 'select', transform: 'transform', gradient: 'gradient', shapes: 'shapes', picker: 'picker', text: 'text', hand: 'nav', zoom: 'nav' };
    const k = Liquify.s ? 'liquify' : (map[Tools.cur] || 'nav');
    const t = this.tools.find(x => x && x[0].split(':')[0] === Tools.cur);
    el.append(U.el('div', { class: 'op-tool', title: t ? t[2] : '' }, U.icon(t ? t[1] : (Tools.cur === 'liquify' ? 'wave' : 'brush'), 'sm')), U.el('div', { class: 'obsep' }));
    UI.optContent(k, el);
    if (Tools.cur === 'clone' && !Liquify.s) { el.append(U.el('div', { class: 'obsep' })); UI.optContent('clone', el); }
  },
  mini(label, min, max, value, step, fmt, onInput, onChange, title) {
    const out = U.el('input', { class: 'mini-n', value: fmt(value) });
    const r = U.el('input', { type: 'range', class: 'mini-r', min, max, step, value });
    r.addEventListener('input', () => { out.value = fmt(+r.value); onInput(+r.value); });
    r.addEventListener('change', () => onChange && onChange(+r.value));
    out.addEventListener('change', () => { const v = parseFloat(out.value); if (!isNaN(v)) { const val = U.clamp(fmt.parse ? fmt.parse(v) : v, +min, +max); r.value = val; out.value = fmt(val); onInput(val); onChange && onChange(val); } });
    return U.el('label', { class: 'mini', title: title || label }, U.el('span', { text: label }), r, out);
  },
  paintOptions(el) {
    const ts = Tools.ts(), b = Tools.brush(), id = b.id;
    const cv = Brushes.sample(b, 70, 24, U.hex2rgb(getComputedStyle(document.body).getPropertyValue('--text').trim()) || { r: 220, g: 220, b: 220 });
    cv.style.width = '70px'; cv.style.height = '24px';
    const bbtn = U.el('button', { class: 'op-brush', title: 'Выбрать кисть', onclick: e => UI.toggleBrushLib(e.currentTarget) }, cv, U.el('span', { text: b.name }), U.el('span', { text: '▾', style: { opacity: 0.6 } }));
    el.append(bbtn);
    const pxFmt = v => (Math.round(Brushes.sizePx(b, v) * 10) / 10) + '';
    pxFmt.parse = px => Math.sqrt(U.clamp((px - b.sizeMin) / Math.max(1e-6, b.sizeMax - b.sizeMin), 0, 1));
    el.append(this.mini('Размер', 0, 1, ts.size, 0.001, pxFmt, v => { ts.size = v; UI.sliders.size && UI.sliders.size.show(); }, () => App.saveTools(), 'Размер кисти, px ([ и ])'));
    const upd = (k, v) => Brushes.update(id, { [k]: v });
    const pc = v => Math.round(v * 100) + '%'; pc.parse = v => v / 100;
    if (!b.pixel) el.append(this.mini('Жёсткость', 0, 1, b.hardness, 0.01, pc, v => upd('hardness', v), () => Desk.refreshPanel('brushset')));
    if (Tools.cur === 'brush') {
      const sel = U.el('select', { class: 'op-sel', title: 'Режим наложения кисти', onchange: () => { ts.mode = sel.value; App.saveTools(); } });
      for (const [bid, name] of BLENDS) if (!CustomBlend.ids.has(bid)) sel.append(U.el('option', { value: bid, text: name, selected: (ts.mode || 'source-over') === bid }));
      el.append(U.el('label', { class: 'mini' }, U.el('span', { text: 'Режим' }), sel));
    }
    el.append(this.mini(Tools.cur === 'smudge' ? 'Сила' : 'Непрозр.', 0, 1, ts.opacity, 0.01, pc, v => { ts.opacity = Math.max(0.01, v); UI.sliders.opacity && UI.sliders.opacity.show(); }, () => App.saveTools(), 'Непрозрачность (Shift+1…0)'));
    if (Tools.cur !== 'smudge') el.append(this.mini('Нажим', 0, 1, b.flow, 0.01, pc, v => upd('flow', v), () => Desk.refreshPanel('brushset'), 'Плотность отпечатка (поток)'));
    else el.append(this.mini('Размазывание', 0, 1, b.smudge, 0.01, pc, v => upd('smudge', v), () => Desk.refreshPanel('brushset')));
    el.append(this.mini('Сглаживание', 0, 1, b.streamline, 0.01, pc, v => upd('streamline', v), () => Desk.refreshPanel('brushset'), 'Стабилизация линии'));
    const tg = (label, key, def, icon) => U.el('button', { class: 'ob' + (b[key] > 0 ? ' on' : ''), title: label, onclick: () => { Brushes.update(id, { [key]: b[key] > 0 ? 0 : def }); this.renderOptions(); Desk.refreshPanel('brushset'); } }, U.el('span', { text: icon }));
    el.append(tg('Нажим пера управляет размером', 'pSize', 0.7, '◐ размер'), tg('Нажим пера управляет плотностью', 'pFlow', 0.6, '◐ плотность'));
    el.append(U.el('button', { class: 'ob', title: 'Все настройки кисти', onclick: () => this.showPanel('brushset') }, U.icon('settings', 'sm'), U.el('span', { text: 'Настройки кисти' })));
  },
  /* ---------- строка состояния ---------- */
  status() {
    if (!this.built || !UI.desk || !Engine.doc) return;
    const d = Engine.doc, z = Math.round(View.s * 1000) / 10;
    if (document.activeElement !== this.statusEls.zoom) this.statusEls.zoom.value = z + '%';
    this.statusEls.size.textContent = `${d.w} × ${d.h} px`;
    this.statusEls.layer.textContent = d.layer ? 'Слой: ' + d.layer.name : '';
    this.tabEl.textContent = `${d.name} @ ${z}% (${d.layer ? d.layer.name : ''}, RGB/8)`;
    this.titleEl.textContent = d.name;
  },
  coords(x, y) {
    if (!this.built || !UI.desk) return;
    const d = Engine.doc; if (!d) return;
    this.statusEls.pos.textContent = x >= 0 && y >= 0 && x < d.w && y < d.h ? `X: ${Math.floor(x)}  Y: ${Math.floor(y)}` : '';
  },
  hint(t) { if (this.statusEls.hint) this.statusEls.hint.textContent = t || ''; },
  /* ---------- меню ---------- */
  menus() {
    const d = () => Engine.doc, ifDoc = () => !Engine.doc, lay = () => Engine.doc && Engine.doc.layer;
    const adj = ids => ids.map(id => { const f = Adjust.list.find(x => x.id === id); return { label: f.name + '…', act: () => Adjust.open(id), off: ifDoc }; });
    return [
      { label: 'Файл', items: [
        { label: 'Создать…', key: 'Ctrl+N', act: () => App.newCanvas() },
        { label: 'Открыть / импорт…', key: 'Ctrl+O', act: () => App.openFile() },
        { label: 'Галерея', act: () => App.toGallery(), off: ifDoc },
        '-',
        { label: 'Сохранить', key: 'Ctrl+S', act: () => App.saveNow(true).then(() => U.toast('Сохранено')), off: ifDoc },
        { label: 'Экспорт', off: ifDoc, sub: [
          { label: 'PNG', act: () => Exporter.image('png') }, { label: 'JPEG', act: () => Exporter.image('jpeg') }, { label: 'WebP', act: () => Exporter.image('webp') },
          '-', { label: 'PSD со слоями', act: () => Exporter.psd() }, { label: 'Проект Мазок (.mazok)', act: () => Exporter.mazok() }, { label: 'Слои в PNG (ZIP)', act: () => Exporter.layersZip() }] },
        { label: 'Анимация', off: ifDoc, sub: [{ label: 'Анимированный GIF…', act: () => Exporter.gif() }, { label: 'Видео MP4/WebM…', act: () => Exporter.animVideo() }, { label: 'Кадры в PNG (ZIP)', act: () => Exporter.framesZip() }] },
        { label: 'Таймлапс', off: ifDoc, sub: [{ label: 'Смотреть', act: () => Timelapse.replay() }, { label: 'Экспорт — полностью', act: () => Timelapse.export(false) }, { label: 'Экспорт — 30 секунд', act: () => Timelapse.export(true) }, '-', { label: 'Записывать таймлапс', check: () => Settings.timelapse, act: () => { Settings.timelapse = !Settings.timelapse; Settings.save(); } }, { label: 'Очистить запись', act: () => Timelapse.clear() }] },
        '-', { label: 'Закрыть', key: 'Ctrl+W', act: () => App.toGallery(), off: ifDoc }
      ] },
      { label: 'Редактирование', items: [
        { label: 'Отменить: ' + (History.pos ? History.list[History.pos - 1].name : '—'), key: 'Ctrl+Z', act: () => History.undo(), off: () => !History.pos },
        { label: 'Повторить', key: 'Ctrl+Shift+Z', act: () => History.redo(), off: () => History.pos >= History.list.length },
        '-',
        { label: 'Вырезать', key: 'Ctrl+X', act: () => Ops.cut(), off: ifDoc }, { label: 'Копировать', key: 'Ctrl+C', act: () => Ops.copy(), off: ifDoc },
        { label: 'Скопировать совмещённые данные', key: 'Ctrl+Shift+C', act: () => Ops.copy(true), off: ifDoc }, { label: 'Вставить', key: 'Ctrl+V', act: () => App.paste(), off: ifDoc },
        { label: 'Очистить', key: 'Delete', act: () => Ops.clear(), off: ifDoc }, { label: 'Выполнить заливку основным цветом', key: 'Alt+Backspace', act: () => Ops.fill(), off: ifDoc },
        '-',
        { label: 'Свободное трансформирование', key: 'Ctrl+T', act: () => Tools.set('transform'), off: ifDoc },
        { label: 'Отразить слой по горизонтали', act: () => Ops.flipLayer(true), off: ifDoc }, { label: 'Отразить слой по вертикали', act: () => Ops.flipLayer(false), off: ifDoc },
        '-', { label: 'Настройки…', key: 'Ctrl+K', act: () => Panels.prefsDialog() }
      ] },
      { label: 'Изображение', items: [
        { label: 'Коррекция', off: ifDoc, sub: adj(['hsb', 'balance', 'curves', 'gmap', 'desat', 'invert', 'posterize', 'threshold']) },
        '-',
        { label: 'Размер изображения и холста…', key: 'Ctrl+Alt+I', act: () => Panels.resizeDialog(), off: ifDoc },
        { label: 'Вращение изображения', off: ifDoc, sub: [{ label: '90° по часовой', act: () => Ops.rotateCanvas(true) }, { label: '90° против часовой', act: () => Ops.rotateCanvas(false) }, { label: '180°', act: () => { History.group('Повернуть холст на 180°', () => { Ops.rotateCanvas(true); Ops.rotateCanvas(true); }); } }, '-', { label: 'Отразить холст по горизонтали', act: () => Ops.flipCanvas(true) }, { label: 'Отразить холст по вертикали', act: () => Ops.flipCanvas(false) }] },
        { label: 'Цвет фона…', act: () => Desk.bgColor(), off: ifDoc },
        { label: 'Сведения о холсте', act: () => Panels.info(), off: ifDoc }
      ] },
      { label: 'Слои', items: [
        { label: 'Новый слой', key: 'Ctrl+Shift+N', act: () => Ops.add(), off: ifDoc }, { label: 'Дублировать слой', key: 'Ctrl+J', act: () => Ops.duplicate(), off: ifDoc },
        { label: 'Удалить слой', act: () => Ops.remove(), off: ifDoc }, { label: 'Переименовать…', act: async () => { const L = lay(); const n = await U.prompt('Имя слоя', L.name); if (n) Ops.setProps(L, { name: n }, 'Переименовать слой'); }, off: ifDoc },
        { label: 'Новый текстовый слой', act: () => Tools.set('text'), off: ifDoc },
        '-',
        { label: 'Режим наложения', off: ifDoc, sub: BLENDS.map(([id, name], i) => ({ label: name, sepBefore: BLEND_GROUPS.some(g => g[0] === i && i > 0), check: () => lay() && lay().blend === id, act: () => Ops.setProps(lay(), { blend: id }, 'Режим наложения') })) },
        { label: 'Создать обтравочную маску', key: 'Ctrl+Alt+G', check: () => lay() && lay().clip, act: () => Ops.toggle(lay(), 'clip', 'Обтравочная маска'), off: ifDoc },
        { label: 'Защитить прозрачность', key: '/', check: () => lay() && lay().alphaLock, act: () => Ops.toggle(lay(), 'alphaLock', 'Защита прозрачности'), off: ifDoc },
        { label: 'Закрепить слой', check: () => lay() && lay().locked, act: () => Ops.toggle(lay(), 'locked', 'Блокировка слоя'), off: ifDoc },
        { label: 'Показать/скрыть слой', act: () => Ops.toggle(lay(), 'visible', 'Видимость слоя'), off: ifDoc },
        '-',
        { label: 'Объединить с нижним', key: 'Ctrl+E', act: () => Ops.mergeDown(), off: ifDoc }, { label: 'Выполнить сведение', key: 'Ctrl+Shift+E', act: () => Ops.flatten(), off: ifDoc },
        '-', { label: 'Экспорт слоя в PNG', act: async () => U.save(await U.toBlob(lay().canvas), U.safeName(lay().name) + '.png'), off: ifDoc }
      ] },
      { label: 'Выделение', items: [
        { label: 'Всё', key: 'Ctrl+A', act: () => Sel.all(), off: ifDoc }, { label: 'Отменить выделение', key: 'Ctrl+D', act: () => Sel.clear(), off: () => !Sel.active },
        { label: 'Инверсия', key: 'Ctrl+Shift+I', act: () => Sel.invert(), off: ifDoc },
        { label: 'Растушёвка…', key: 'Shift+F6', act: async () => { const v = await Panels.number('Растушёвка, px', 8, 0, 200); if (v) Sel.feather(v); }, off: () => !Sel.active },
        { label: 'Загрузить из слоя', act: () => Sel.fromLayer(lay()), off: ifDoc },
        '-', { label: 'Скопировать на новый слой', key: 'Ctrl+J', act: () => Ops.copyPaste(false), off: () => !Sel.active }, { label: 'Вырезать на новый слой', key: 'Ctrl+Shift+J', act: () => Ops.copyPaste(true), off: () => !Sel.active }
      ] },
      { label: 'Фильтр', items: [
        { label: 'Размытие', off: ifDoc, sub: adj(['blur', 'motion', 'radial']) },
        { label: 'Усиление резкости…', act: () => Adjust.open('sharpen'), off: ifDoc },
        { label: 'Шум…', act: () => Adjust.open('noise'), off: ifDoc },
        { label: 'Стилизация', off: ifDoc, sub: adj(['bloom', 'chroma', 'halftone', 'pixelate', 'vignette']) },
        '-', { label: 'Пластика…', key: 'Shift+Ctrl+X', act: () => Liquify.begin(), off: ifDoc }
      ] },
      { label: 'Просмотр', items: [
        { label: 'Увеличить', key: 'Ctrl++', act: () => View.zoomAt(View.W / 2, View.H / 2, 1.25), off: ifDoc }, { label: 'Уменьшить', key: 'Ctrl+−', act: () => View.zoomAt(View.W / 2, View.H / 2, 0.8), off: ifDoc },
        { label: 'Показать во весь экран', key: 'Ctrl+0', act: () => View.fit(), off: ifDoc }, { label: '100%', key: 'Ctrl+1', act: () => View.actual(), off: ifDoc },
        { label: 'Сбросить поворот вида', act: () => { View.rotateAt(View.W / 2, View.H / 2, -View.r); }, off: ifDoc },
        { label: 'Зеркальный вид', key: 'F', check: () => View.flip < 0, act: () => View.toggleFlip(), off: ifDoc },
        '-', { label: 'Помощник рисования…', act: () => UI.action('guides', null), off: ifDoc },
        { label: 'Окно-образец', check: () => RefWin.open, act: () => RefWin.toggle(!RefWin.open), off: ifDoc },
        { label: 'Скрыть панели', key: 'Tab', act: () => UI.toggleHidden() }
      ] },
      { label: 'Окно', items: [
        ...[['color', 'Цвет'], ['swatches', 'Образцы'], ['brushes', 'Кисти'], ['brushset', 'Настройки кисти'], ['layers', 'Слои'], ['history', 'История']].map(([id, n]) => ({ label: n, act: () => this.showPanel(id) })),
        '-', { label: 'Шкала анимации', check: () => Engine.doc && Engine.doc.anim.on, act: () => Anim.toggle(!Engine.doc.anim.on), off: ifDoc },
        '-', { label: 'Интерфейс: настольный', check: () => UI.desk, act: () => { Settings.uiMode = 'desk'; Settings.save(); UI.applyLayout(); } },
        { label: 'Интерфейс: планшетный', check: () => !UI.desk, act: () => { Settings.uiMode = 'touch'; Settings.save(); UI.applyLayout(); } }
      ] },
      { label: 'Справка', items: [{ label: 'Горячие клавиши и жесты', key: 'F1', act: () => U.dialog({ title: 'Справка', body: Panels.helpBody(), cls: 'wide' }) }, { label: 'О программе', act: () => U.dialog({ title: 'Мазок', body: U.el('div', { html: '<p>Приложение для цифрового рисования.<br>Работает без интернета, рисунки хранятся на этом устройстве.</p>' }) }) }] }
    ];
  },
  openMenu(i) {
    this.closeMenus(false);
    const m = this.menus()[i], btn = U.$$('#menubar .mb-item')[i]; if (!btn) return;
    btn.classList.add('on');
    const r = btn.getBoundingClientRect();
    this.menuEl = this.dropdown(m.items, r.left, r.bottom);
    this.menuIdx = i;
  },
  dropdown(items, x, y) {
    const el = U.el('div', { class: 'mb-drop' });
    let subEl = null;
    for (const it of items) {
      if (it === '-') { el.append(U.el('div', { class: 'md-sep' })); continue; }
      if (it.sepBefore) el.append(U.el('div', { class: 'md-sep' }));
      const off = it.off && it.off(), chk = it.check && it.check();
      const row = U.el('div', { class: 'md-it' + (off ? ' off' : '') + (it.sub ? ' has' : '') },
        U.el('span', { class: 'md-c', text: chk ? '✓' : '' }), U.el('span', { class: 'md-l', text: it.label }), U.el('span', { class: 'md-k', text: it.sub ? '▸' : (it.key || '') }));
      row.addEventListener('pointerenter', () => {
        if (subEl) { subEl.remove(); subEl = null; }
        if (it.sub && !off) { const rr = row.getBoundingClientRect(); subEl = this.dropdown(it.sub, rr.right - 2, rr.top - 4); }
      });
      row.addEventListener('click', () => { if (off || it.sub) return; this.closeMenus(); it.act && it.act(); });
      el.append(row);
    }
    U.$('#menus').append(el);
    const w = el.offsetWidth, h = el.offsetHeight;
    el.style.left = Math.max(2, Math.min(x, window.innerWidth - w - 2)) + 'px';
    el.style.top = Math.max(2, Math.min(y, window.innerHeight - h - 2)) + 'px';
    return el;
  },
  closeMenus() {
    const had = !!this.menuEl;
    U.$('#menus').innerHTML = ''; this.menuEl = null; this.menuIdx = -1;
    U.$$('#menubar .mb-item.on').forEach(b => b.classList.remove('on'));
    return had;
  },
  bgColor() {
    const d = Engine.doc, inp = U.el('input', { type: 'color', value: d.bg, style: { width: '100%', height: '44px', border: 0, background: 'none' } });
    U.dialog({ title: 'Цвет фона', body: U.el('div', null, inp, UI.sw('Показывать фон', d.bgVisible, v => { d._bgv = v; })), buttons: [{ text: 'Отмена', value: null }, { text: 'Готово', primary: true, value: () => true }] })
      .then(ok => { if (ok) Panels.setBg(inp.value, d._bgv != null ? d._bgv : d.bgVisible); delete d._bgv; });
  }
};
