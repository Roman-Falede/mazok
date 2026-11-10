'use strict';
/* Мазок — панели: слои, кисти, студия кистей, действия, коррекция, помощник, окно-образец */

const Panels = {
  dragging: false, expanded: null, bcat: null, menuTab: 'add', samples: new Map(),
  /* ---------- слои ---------- */
  layersHead() {
    return U.el('button', { class: 'tb', title: 'Новый слой (Ctrl+Shift+N)', onclick: () => { Tools.commitPending(); Ops.add(); } }, U.icon('plus'));
  },
  blendSelect(L, cls) {
    const sel = U.el('select', { class: cls || 'input', title: 'Режим наложения слоя', onchange: () => Ops.setProps(L, { blend: sel.value }, 'Режим наложения') });
    BLEND_GROUPS.forEach(([a, b], gi) => {
      const g = U.el('optgroup', { label: ['Обычные', 'Затемнение', 'Осветление', 'Контраст', 'Сравнение', 'Компоненты'][gi] });
      for (let i = a; i < b; i++) g.append(U.el('option', { value: BLENDS[i][0], text: BLENDS[i][1], selected: BLENDS[i][0] === L.blend }));
      sel.append(g);
    });
    return sel;
  },
  layers() {
    const d = Engine.doc, wrap = U.el('div', { class: 'lpanel' });
    const n = d.layers.length, A = d.layer;
    if (A) {
      let before = null;
      const op = U.el('input', { type: 'range', min: 0, max: 100, value: Math.round(A.opacity * 100), class: 'mini-r' });
      const opn = U.el('span', { class: 'lp-n', text: Math.round(A.opacity * 100) + '%' });
      op.addEventListener('input', () => { if (!before) before = { opacity: A.opacity }; A.opacity = +op.value / 100; opn.textContent = op.value + '%'; Engine.renderAll(); });
      op.addEventListener('change', () => { if (before) Ops.propsCommit(A, before, 'Непрозрачность слоя'); before = null; UI.layersChanged(); });
      const lk = (icon, prop, title) => U.el('button', { class: 'lp-b' + (A[prop] ? ' on' : ''), title, onclick: () => Ops.toggle(A, prop, title) }, U.icon(icon, 'sm'));
      wrap.append(U.el('div', { class: 'lp-top' }, this.blendSelect(A, 'input lp-sel'), U.el('label', { class: 'lp-op', title: 'Непрозрачность слоя' }, 'Непрозр.', op, opn)),
        U.el('div', { class: 'lp-top' }, U.el('small', { text: 'Закрепить:' }), lk('alphalock', 'alphaLock', 'Защита прозрачности'), lk('lock', 'locked', 'Блокировка слоя'), lk('clip', 'clip', 'Обтравочная маска'),
          U.el('div', { class: 'grow' }), U.el('small', { text: `${n}/${d.maxLayers}` })));
    }
    for (let p = 0; p < n; p++) { const i = n - 1 - p; wrap.append(this.layerRow(d.layers[i], i)); if (this.expanded === d.layers[i].id) wrap.append(this.layerProps(d.layers[i])); }
    const bgIn = U.el('input', { type: 'color', value: d.bg, style: { position: 'absolute', opacity: 0, width: '1px', height: '1px' } });
    bgIn.addEventListener('change', () => this.setBg(bgIn.value, d.bgVisible));
    const bg = U.el('div', { class: 'layer bg', style: { '--bgc': d.bg } },
      U.el('div', { class: 'th', style: { background: d.bg }, onclick: () => bgIn.click() }), bgIn,
      U.el('div', { class: 'nm', onclick: () => bgIn.click() }, 'Цвет фона', U.el('small', { text: d.bg.toUpperCase() })),
      U.el('button', { class: 'vis' + (d.bgVisible ? '' : ' off'), title: 'Показать/скрыть фон', onclick: () => this.setBg(d.bg, !d.bgVisible) }, U.icon(d.bgVisible ? 'eye' : 'eyeoff', 'sm')));
    wrap.append(bg);
    const fb = (icon, title, fn) => U.el('button', { class: 'lp-b', title, onclick: () => { Tools.commitPending(); fn(); } }, U.icon(icon, 'sm'));
    wrap.append(U.el('div', { class: 'lp-foot' },
      fb('plus', 'Новый слой', () => Ops.add()), fb('copy', 'Дублировать слой', () => Ops.duplicate()), fb('merge', 'Объединить с нижним', () => Ops.mergeDown()),
      fb('select', 'Выделить по содержимому слоя', () => Sel.fromLayer(d.layer)), fb('more', 'Ещё действия со слоем', () => this.layerMenu(d.layer, d.active)), U.el('div', { class: 'grow' }),
      fb('trash', 'Удалить слой', () => Ops.remove())));
    return wrap;
  },
  setBg(color, vis) {
    const d = Engine.doc, before = { bg: d.bg, bgVisible: d.bgVisible }, after = { bg: color, bgVisible: vis };
    const apply = s => { d.bg = s.bg; d.bgVisible = s.bgVisible; Engine.renderAll(); UI.layersChanged(); };
    apply(after);
    History.push({ name: 'Цвет фона', undo: () => apply(before), redo: () => apply(after) });
  },
  layerRow(L, i) {
    const d = Engine.doc, on = i === d.active, bi = blendInfo(L.blend);
    const sub = [];
    if (L.blend !== 'source-over') sub.push(bi[1]);
    if (L.opacity < 1) sub.push(Math.round(L.opacity * 100) + '%');
    if (L.text) sub.push('текст');
    const flags = U.el('span', { class: 'flags' });
    if (L.locked) flags.append(U.icon('lock', 'sm')); if (L.alphaLock) flags.append(U.icon('alphalock', 'sm')); if (L.reference) flags.append(U.el('b', { text: 'Р', title: 'Референс', style: { fontSize: '11px' } }));
    const th = U.el('div', { class: 'th' }, L.thumb());
    const row = U.el('div', { class: 'layer' + (on ? ' on' : '') + (L.clip && i > 0 ? ' clip' : ''), 'data-i': i },
      th,
      U.el('div', { class: 'nm' }, L.name, sub.length ? U.el('small', { text: sub.join(' · ') }) : null),
      flags,
      U.el('button', { class: 'bm', title: 'Режим наложения и непрозрачность', onclick: e => { e.stopPropagation(); this.expanded = this.expanded === L.id ? null : L.id; UI.reopen('layers'); } }, bi[2]),
      U.el('button', { class: 'vis' + (L.visible ? '' : ' off'), title: 'Видимость', onclick: e => { e.stopPropagation(); Ops.toggle(L, 'visible', L.visible ? 'Скрыть слой' : 'Показать слой'); } }, U.icon(L.visible ? 'eye' : 'eyeoff', 'sm')));
    const hd = U.el('div', { class: 'hd', title: 'Перетащите для перемещения' }, U.icon('menu', 'sm'));
    row.prepend(hd);
    row.addEventListener('click', () => { if (i === d.active) this.layerMenu(L, i); else Ops.select(i); });
    this.dragRow(row, hd, i);
    return row;
  },
  dragRow(row, hd, i) {
    hd.addEventListener('pointerdown', e => {
      e.stopPropagation(); e.preventDefault(); hd.setPointerCapture(e.pointerId);
      const rows = U.$$('.layer:not(.bg)', row.parentNode), rects = rows.map(r => r.getBoundingClientRect()), y0 = e.clientY;
      this.dragging = true; row.classList.add('dragging');
      let target = rows.indexOf(row);
      const mv = ev => {
        const dy = ev.clientY - y0; row.style.transform = `translateY(${dy}px)`;
        const cy = rects[rows.indexOf(row)].top + rects[0].height / 2 + dy;
        target = rects.findIndex(r => cy < r.bottom); if (target < 0) target = rows.length - 1;
      };
      const up = () => {
        hd.removeEventListener('pointermove', mv); hd.removeEventListener('pointerup', up); hd.removeEventListener('pointercancel', up);
        this.dragging = false; row.style.transform = '';
        const n = Engine.doc.layers.length, to = n - 1 - target;
        if (to !== i) Ops.move(i, to); else UI.reopen('layers');
      };
      hd.addEventListener('pointermove', mv); hd.addEventListener('pointerup', up); hd.addEventListener('pointercancel', up);
    });
    row.addEventListener('click', e => { if (e.target.closest('.hd')) e.stopPropagation(); }, true);
  },
  layerProps(L) {
    const box = U.el('div', { class: 'lprops' });
    let before = null;
    box.append(UI.hslider('Непрозрачность', 0, 100, Math.round(L.opacity * 100), 1, v => { if (!before) before = { opacity: L.opacity }; L.opacity = v / 100; Engine.renderAll(); }, v => v + '%', () => { if (before) Ops.propsCommit(L, before, 'Непрозрачность слоя'); before = null; UI.layersChanged(); }));
    const grid = U.el('div', { style: { display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: '3px' } });
    BLENDS.forEach(([id, name], i) => {
      if (BLEND_GROUPS.some(g => g[0] === i && i > 0)) grid.append(U.el('div', { style: { gridColumn: '1/-1', height: '1px', background: 'var(--line)', margin: '2px 0' } }));
      grid.append(U.el('button', { class: 'btn sm' + (L.blend === id ? ' primary' : ''), text: name, style: { fontSize: '11px', padding: '0 4px', height: 'auto', minHeight: '28px', lineHeight: '1.1' }, onclick: () => Ops.setProps(L, { blend: id }, 'Режим наложения') }));
    });
    box.append(grid);
    return box;
  },
  layerMenu(L, i) {
    const d = Engine.doc, body = U.el('div');
    let close;
    const it = (icon, label, fn, sub) => body.append(UI.item(icon, label, () => { close(); fn(); }, sub));
    it('edit', 'Переименовать', async () => { const n = await U.prompt('Имя слоя', L.name); if (n) Ops.setProps(L, { name: n }, 'Переименовать слой'); });
    if (L.text) it('text', 'Изменить текст', () => TextTool.edit(L));
    it('select', 'Выделить по содержимому', () => Sel.fromLayer(L));
    it('copy', 'Копировать', () => Ops.copy());
    it('fill', 'Залить слой текущим цветом', () => Ops.fill(L));
    it('trash', 'Очистить', () => Ops.clear(L));
    body.append(UI.sw('Защита прозрачности', L.alphaLock, v => Ops.setProps(L, { alphaLock: v }, 'Защита прозрачности'), 'Рисовать только по уже закрашенным пикселям'));
    body.append(UI.sw('Обтравочная маска', L.clip, v => Ops.setProps(L, { clip: v }, 'Обтравочная маска'), 'Показывать только поверх слоя ниже'));
    body.append(UI.sw('Референс', L.reference, v => { for (const x of d.layers) if (x !== L) x.reference = false; Ops.setProps(L, { reference: v }, 'Референс'); }, 'Источник границ для заливки'));
    body.append(UI.sw('Заблокировать', L.locked, v => Ops.setProps(L, { locked: v }, v ? 'Заблокировать слой' : 'Разблокировать слой')));
    it('invert', 'Инвертировать цвета', () => Ops.invert(L));
    it('flipH', 'Отразить слой по горизонтали', () => Ops.flipLayer(true, L));
    it('flipV', 'Отразить слой по вертикали', () => Ops.flipLayer(false, L));
    it('merge', 'Объединить с нижним', () => Ops.mergeDown(i));
    it('layers', 'Свести все слои', () => Ops.flatten());
    it('copy', 'Дублировать', () => Ops.duplicate(i));
    it('download', 'Экспорт слоя в PNG', async () => U.save(await U.toBlob(L.canvas), U.safeName(L.name) + '.png'));
    it('trash', 'Удалить слой', () => Ops.remove(i));
    U.dialog({ title: L.name, body, buttons: [{ text: 'Закрыть', value: null }], onOpen: (b, c) => { close = c; } });
  },
  /* ---------- кисти ---------- */
  brushHead() {
    return U.el('div', { class: 'grp', style: { display: 'flex' } },
      U.el('button', { class: 'tb', title: 'Импорт кисти', onclick: async () => { const [f] = await U.pickFile('.mazokbrush,.json,application/json'); if (!f) return; try { const b = await Brushes.importFile(f); this.bcat = 'custom'; this.pickBrush(b); UI.reopen('brushes'); U.toast('Кисть импортирована'); } catch (e) { U.toast(e.message); } } }, U.icon('download', 'sm')),
      U.el('button', { class: 'tb', title: 'Новая кисть', onclick: () => { const b = Brushes.create(); this.bcat = 'custom'; this.pickBrush(b); UI.closePopovers(); this.studio(b.id); } }, U.icon('plus', 'sm')));
  },
  pickBrush(b) {
    const ts = Tools.ts();
    ts.sizes = ts.sizes || {}; ts.sizes[ts.brushId] = ts.size;
    ts.brushId = b.id; ts.size = ts.sizes[b.id] != null ? ts.sizes[b.id] : b.size;
    if (Tools.cur === 'brush' || Tools.cur === 'eraser') ts.opacity = Tools.cur === 'eraser' ? ts.opacity : b.opacity;
    UI.toolChanged(); App.saveTools();
  },
  brushLib() {
    const cur = Tools.brush();
    if (!this.bcat) this.bcat = cur.cat;
    const root = U.el('div', { class: 'blib', style: { height: 'min(62vh,520px)' } });
    const cats = U.el('div', { class: 'bcats' }), list = U.el('div', { class: 'blist' });
    for (const c of BRUSH_CATS) cats.append(U.el('button', { class: this.bcat === c.id ? 'on' : '', text: c.name, onclick: () => { this.bcat = c.id; UI.reopen('brushes'); } }));
    const col = U.hex2rgb(getComputedStyle(document.documentElement).getPropertyValue('--text').trim() || '#eeeeee') || { r: 230, g: 230, b: 230 };
    const items = Brushes.list(this.bcat);
    if (!items.length) list.append(U.el('div', { class: 'row', html: '<small>Здесь появятся ваши кисти. Нажмите «+», чтобы создать новую, или дублируйте любую кисть в студии.</small>' }));
    for (const b of items) {
      const key = b.id + '|' + Brushes.version + '|' + U.hex(col);
      let cv = this.samples.get(key);
      if (!cv) { cv = Brushes.sample(b, 230, 44, col); this.samples.set(key, cv); }
      const c2 = U.canvas(cv.width, cv.height); c2.getContext('2d').drawImage(cv, 0, 0);
      const it = U.el('div', { class: 'bitem' + (b.id === cur.id ? ' on' : ''), onclick: () => { this.pickBrush(b); UI.reopen('brushes'); } },
        U.el('div', { class: 'n' }, U.el('span', { text: b.name + (Brushes.isModified(b.id) ? ' •' : '') }),
          U.el('button', { class: 'tb', style: { height: '24px', minWidth: '24px', padding: 0 }, title: 'Студия кисти', onclick: e => { e.stopPropagation(); UI.closePopovers(); this.studio(b.id); } }, U.icon('settings', 'sm'))), c2);
      list.append(it);
    }
    root.append(cats, list);
    return root;
  },
  /* общие поля настроек кисти (студия и панель «Настройки кисти») */
  BRUSH_SECTIONS: [['shape', 'Форма отпечатка'], ['dyn', 'Динамика формы'], ['scatter', 'Рассеивание'], ['grain', 'Текстура'], ['transfer', 'Передача'], ['color', 'Динамика цвета'], ['taper', 'Сглаживание и сужение'], ['pressure', 'Перо и давление'], ['props', 'Свойства']],
  brushFields(sect, b, changed, rerender) {
    const out = [], A = (...e) => out.push(...e);
    const num = (label, key, min, max, step = 0.01, fmt) => UI.hslider(label, min, max, b[key], step, v => { b[key] = v; changed(); }, fmt || (v => (Math.round(v * 100) / 100).toString()));
    const pct = (label, key, max = 1) => UI.hslider(label, 0, max * 100, Math.round(b[key] * 100), 1, v => { b[key] = v / 100; changed(); }, v => v + '%');
    const tog = (label, key, sub, on = true, off = false) => UI.sw(label, !!b[key], v => { b[key] = v ? on : off; changed(); }, sub);
    if (sect === 'shape') {
      const grid = U.el('div', { class: 'tipgrid' });
      for (const s of Tips.shapes) {
        if (s.id === 'image' && !b.tipImage) continue;
        const c = U.canvas(40, 40), x = c.getContext('2d'), m = Tips.get(s.id, b.hardness, 40, Brushes.tipImage(b));
        x.drawImage(Tips.tint(m, getComputedStyle(document.body).getPropertyValue('--text').trim() || '#ddd'), 2, 2, 36, 36);
        grid.append(U.el('button', { class: b.shape === s.id ? 'on' : '', title: s.name, onclick: () => { b.shape = s.id; changed(); rerender && rerender(); } }, c));
      }
      A(grid, U.el('button', { class: 'btn sm', text: 'Форма из изображения…', style: { margin: '4px 8px' }, onclick: async () => { const [f] = await U.pickFile('image/*'); if (!f) return; const img = await U.loadImage(f); const c = U.canvas(256, 256); const k = Math.min(256 / img.width, 256 / img.height); c.getContext('2d').drawImage(img, (256 - img.width * k) / 2, (256 - img.height * k) / 2, img.width * k, img.height * k); b.tipImage = c.toDataURL('image/png'); b.shape = 'image'; Brushes.prepare(b); setTimeout(() => { changed(); rerender && rerender(); }, 80); } }),
        pct('Жёсткость', 'hardness'), pct('Интервал', 'spacing', 2.5), pct('Округлость', 'roundness'), num('Угол, °', 'angle', -180, 180, 1, v => v + '°'), tog('Поворачивать по направлению штриха', 'follow'));
    }
    if (sect === 'dyn') A(pct('Колебание размера', 'jSize'), pct('Нажим → размер', 'pSize'), pct('Минимальный диаметр', 'minSize'), num('Наклон пера → размер', 'tiltSize', 0, 4, 0.1), pct('Колебание угла', 'jAngle'));
    if (sect === 'scatter') A(pct('Рассеивание', 'scatter', 3), num('Количество отпечатков', 'count', 1, 8, 1, v => v));
    if (sect === 'grain') {
      const sel = U.el('select', { class: 'input', style: { margin: '4px 0' }, onchange: () => { b.grain = sel.value || null; b.grainImage = null; changed(); } });
      sel.append(U.el('option', { value: '', text: 'Без текстуры', selected: !b.grain }));
      for (const g of Grains.list) sel.append(U.el('option', { value: g.id, text: g.name, selected: g.id === b.grain && !b.grainImage }));
      if (b.grainImage) sel.append(U.el('option', { value: 'custom', text: 'Своя текстура', selected: true }));
      A(U.el('div', { class: 'field', style: { padding: '0 8px' } }, sel),
        U.el('button', { class: 'btn sm', text: 'Своя текстура из изображения…', style: { margin: '0 8px 4px' }, onclick: async () => { const [f] = await U.pickFile('image/*'); if (!f) return; const img = await U.loadImage(f); const c = U.canvas(256, 256); c.getContext('2d').drawImage(img, 0, 0, 256, 256); b.grainImage = c.toDataURL('image/png'); b.grain = 'custom'; Grains.addImage('img_' + b.id, c); changed(); rerender && rerender(); } }),
        num('Масштаб', 'grainScale', 0.2, 4, 0.05), pct('Глубина', 'grainStrength'));
    }
    if (sect === 'transfer') A(pct('Нажим (поток)', 'flow'), pct('Нажим пера → плотность', 'pFlow'), pct('Колебание непрозрачности', 'jOpacity'), pct('Непрозрачность по умолчанию', 'opacity'));
    if (sect === 'color') A(pct('Колебание цветового тона', 'jHue'), pct('Колебание насыщенности', 'jSat'), pct('Колебание яркости', 'jBri'));
    if (sect === 'taper') A(pct('Сглаживание линии', 'streamline'), num('Сужение в начале (в размерах кисти)', 'taperStart', 0, 30, 0.5), num('Сужение в конце', 'taperEnd', 0, 30, 0.5), pct('Толщина на концах', 'taperMin'));
    if (sect === 'pressure') A(pct('Нажим → размер', 'pSize'), pct('Нажим → плотность', 'pFlow'), pct('Минимальный размер', 'minSize'), num('Наклон пера → размер', 'tiltSize', 0, 4, 0.1),
      UI.hslider('Кривая нажима (общая): мягко ← → жёстко', 40, 250, Math.round(Settings.pressureGamma * 100), 1, v => { Settings.pressureGamma = v / 100; Settings.save(); changed(); }, v => (v / 100).toFixed(2)));
    if (sect === 'props') {
      const name = U.el('input', { class: 'input', value: b.name, oninput: () => { b.name = name.value; changed(); } });
      A(U.el('div', { class: 'field', style: { padding: '0 8px' } }, U.el('label', { text: 'Название' }), name),
        num('Минимальный размер, px', 'sizeMin', 0.5, 50, 0.5), num('Максимальный размер, px', 'sizeMax', 1, 1000, 1),
        pct('Сила размазывания (палец)', 'smudge'), tog('Режим «Добавление» (свечение)', 'blend', 'Цвета складываются, как свет', 'lighter', null), tog('Пиксельная кисть', 'pixel', 'Без сглаживания, квадратные пиксели'));
    }
    return out;
  },
  /* панель дока «Настройки кисти» — правит текущую кисть сразу */
  brushSettings() {
    const orig = Tools.brush(), id = orig.id, b = JSON.parse(JSON.stringify(Object.assign({}, orig, { _merged: undefined, _v: undefined })));
    const root = U.el('div', { class: 'bset' });
    const pv = U.el('div', { class: 'bset-pv' });
    const drawPv = () => { pv.innerHTML = ''; const c = Brushes.sample(b, 260, 54, Colors.rgb()); c.style.width = '100%'; c.style.height = '54px'; pv.append(c); };
    const save = U.debounce(() => { Brushes.update(id, strip(b)); Panels.samples.clear(); Desk.renderOptions(); }, 250);
    const strip = o => { const c = Object.assign({}, o); delete c.id; delete c.preset; delete c._merged; delete c._v; return c; };
    const pvT = U.rafThrottle(drawPv);
    const changed = () => { pvT(); save(); };
    root.append(U.el('div', { class: 'row', style: { padding: '2px 4px' } }, U.el('b', { class: 'grow', text: b.name + (Brushes.isModified(id) ? ' •' : '') }),
      b.preset && Brushes.isModified(id) ? U.el('button', { class: 'btn sm', text: 'Сбросить', onclick: () => { Brushes.reset(id); Panels.samples.clear(); UI.toolChanged(); } }) : null,
      U.el('button', { class: 'btn sm', text: 'Студия…', onclick: () => this.studio(id) })), pv);
    this.bsetOpen = this.bsetOpen || { shape: true };
    for (const [k, n] of this.BRUSH_SECTIONS) {
      const open = !!this.bsetOpen[k];
      const body = U.el('div', { class: 'bset-b' + (open ? '' : ' hidden') });
      const head = U.el('button', { class: 'bset-h' }, U.el('span', { text: open ? '▾' : '▸' }), n);
      head.onclick = () => { this.bsetOpen[k] = !this.bsetOpen[k]; body.classList.toggle('hidden'); head.firstChild.textContent = this.bsetOpen[k] ? '▾' : '▸'; if (this.bsetOpen[k] && !body.childNodes.length) body.append(...this.brushFields(k, b, changed, () => Desk.refreshPanel('brushset'))); };
      if (open) body.append(...this.brushFields(k, b, changed, () => { save(); setTimeout(() => Desk.refreshPanel('brushset'), 300); }));
      root.append(head, body);
    }
    drawPv();
    return root;
  },
  /* панель «История» */
  history() {
    const box = U.el('div', { class: 'hist' });
    const items = [{ name: 'Исходное состояние' }].concat(History.list);
    items.forEach((e, i) => box.append(U.el('div', { class: 'hist-i' + (i === History.pos ? ' on' : '') + (i > History.pos ? ' fut' : ''), text: e.name, onclick: () => History.goto(i) })));
    setTimeout(() => { const on = box.querySelector('.on'); on && on.scrollIntoView({ block: 'nearest' }); }, 0);
    return box;
  },
  studio(id) {
    const orig = Brushes.get(id), b = JSON.parse(JSON.stringify(Object.assign({}, orig, { _merged: undefined, _v: undefined })));
    let sect = 'shape';
    const nav = U.el('div', { class: 'snav' }), body = U.el('div', { class: 'sbody' }), pad = U.el('div', { class: 'spad' });
    const S = this.BRUSH_SECTIONS.concat([['about', 'О кисти']]);
    const pv = U.el('canvas'), padCtx = pv.getContext('2d');
    const render = () => {
      nav.innerHTML = ''; body.innerHTML = '';
      for (const [k, n] of S) nav.append(U.el('button', { class: sect === k ? 'on' : '', text: n, onclick: () => { sect = k; render(); } }));
      const A = (...e) => body.append(...e);
      if (sect !== 'about') A(...this.brushFields(sect, b, () => preview(), () => render()));
      if (sect === 'about') A(U.el('div', { class: 'row', html: `<div><b>${b.name}</b><br><small>${b.preset ? 'Встроенная кисть' + (Brushes.isModified(b.id) ? ' (изменена)' : '') : 'Пользовательская кисть'}</small></div>` }),
        U.el('div', { class: 'row', style: { flexWrap: 'wrap' } },
          U.el('button', { class: 'btn sm', text: 'Дублировать', onclick: () => { Brushes.update(id, strip(b)); const c = Brushes.duplicate(id); close(); this.pickBrush(c); this.studio(c.id); } }),
          U.el('button', { class: 'btn sm', text: 'Экспорт', onclick: () => { Brushes.update(id, strip(b)); U.save(Brushes.exportBlob(id), U.safeName(b.name) + '.mazokbrush'); } }),
          b.preset ? U.el('button', { class: 'btn sm', text: 'Сбросить к исходной', onclick: () => { Brushes.reset(id); close(); this.studio(id); } }) : U.el('button', { class: 'btn sm danger', text: 'Удалить кисть', onclick: async () => { if (await U.confirm('Удалить кисть?', b.name, 'Удалить', true)) { Brushes.remove(id); Tools.ts().brushId = 'pencil_hb'; close(); UI.toolChanged(); } } })));
    };
    const strip = o => { const c = Object.assign({}, o); delete c.id; delete c.preset; delete c._merged; delete c._v; return c; };
    /* блокнот для пробы кисти */
    let base = null, stroke = null, buf = null;
    const sizePad = () => { const r = pv.getBoundingClientRect(), dpr = Math.min(2, devicePixelRatio || 1); if (!r.width) return; pv.width = r.width * dpr; pv.height = r.height * dpr; base = U.canvas(pv.width, pv.height); buf = U.canvas(pv.width, pv.height); preview(); };
    const preview = () => {
      if (!base) return;
      padCtx.setTransform(1, 0, 0, 1, 0, 0); padCtx.clearRect(0, 0, pv.width, pv.height);
      padCtx.drawImage(base, 0, 0);
      if (!stroke) { const s = Brushes.sample(b, pv.width / Math.min(2, devicePixelRatio || 1), Math.min(120, pv.height / 2), Colors.rgb()); padCtx.globalAlpha = base._used ? 0 : 1; padCtx.drawImage(s, 0, 0); padCtx.globalAlpha = 1; }
    };
    pv.addEventListener('pointerdown', e => {
      pv.setPointerCapture(e.pointerId); const r = pv.getBoundingClientRect(), k = pv.width / r.width;
      if (!base._used) { base.getContext('2d').clearRect(0, 0, base.width, base.height); base._used = true; }
      buf.getContext('2d').clearRect(0, 0, buf.width, buf.height);
      stroke = new Stroke({ brush: b, size: Brushes.sizePx(b, Tools.ts().size) * k * 0.5 + 1, ctx: buf.getContext('2d'), color: Colors.rgb() });
      stroke.k = k; stroke.r = r;
      stroke.add((e.clientX - r.left) * k, (e.clientY - r.top) * k, e.pointerType === 'pen' ? e.pressure : 1, 0); drawStroke();
    });
    pv.addEventListener('pointermove', e => { if (!stroke) return; const evs = e.getCoalescedEvents ? e.getCoalescedEvents() : [e]; for (const ev of (evs.length ? evs : [e])) stroke.add((ev.clientX - stroke.r.left) * stroke.k, (ev.clientY - stroke.r.top) * stroke.k, ev.pointerType === 'pen' ? ev.pressure : 1, 0); drawStroke(); });
    pv.addEventListener('pointerup', () => { if (!stroke) return; if (b.taperEnd > 0) { buf.getContext('2d').clearRect(0, 0, buf.width, buf.height); stroke.catchUp(); stroke.rerender(); } else stroke.end(); drawStroke(); base.getContext('2d').drawImage(pv, 0, 0); stroke = null; });
    const drawStroke = () => {
      preview();
      const t = U.clone(buf), tx = t.getContext('2d');
      const g = Grains.pattern(tx, Brushes.grainId(b), b.grainScale, b.grainStrength);
      if (g) { tx.globalCompositeOperation = 'destination-in'; tx.fillStyle = g; tx.fillRect(0, 0, t.width, t.height); }
      padCtx.globalAlpha = Tools.ts().opacity * (b.opacity || 1); padCtx.globalCompositeOperation = b.blend || 'source-over'; padCtx.drawImage(t, 0, 0); padCtx.globalAlpha = 1; padCtx.globalCompositeOperation = 'source-over';
    };
    pad.append(U.el('div', { class: 'row', style: { padding: 0 } }, U.el('b', { class: 'grow', text: 'Блокнот' }), U.el('button', { class: 'btn sm', text: 'Очистить', onclick: () => { base.getContext('2d').clearRect(0, 0, base.width, base.height); base._used = false; preview(); } })), pv);
    const root = U.el('div', { class: 'studio' }, nav, body, pad);
    render();
    let close;
    U.dialog({ title: 'Студия кисти — ' + b.name, cls: 'full', body: root, onOpen: (box, c) => { close = c; box.querySelector('.modal-body').style.padding = '0'; setTimeout(sizePad, 30); },
      buttons: [{ text: 'Отмена', value: false }, { text: 'Готово', primary: true, value: true }] })
      .then(ok => { if (ok) { Brushes.update(id, strip(b)); this.samples.clear(); UI.toolChanged(); U.toast('Кисть сохранена'); } });
  },
  /* ---------- действия ---------- */
  menu() {
    const root = U.el('div'), tabs = U.el('div', { class: 'tabs' }), body = U.el('div');
    const T = [['add', 'Добавить'], ['canvas', 'Холст'], ['export', 'Экспорт'], ['video', 'Видео'], ['prefs', 'Настройки'], ['help', 'Справка']];
    for (const [id, n] of T) tabs.append(U.el('button', { class: this.menuTab === id ? 'on' : '', text: n, onclick: () => { this.menuTab = id; UI.reopen('menu'); } }));
    root.append(tabs, body);
    const it = (icon, label, fn, sub) => body.append(UI.item(icon, label, () => { UI.closePopovers(); fn(); }, sub));
    const d = Engine.doc;
    switch (this.menuTab) {
      case 'add':
        it('image', 'Вставить изображение…', async () => { const [f] = await U.pickFile('image/*'); if (f) Ops.insertImage(await U.loadImage(f), f.name.replace(/\.[^.]+$/, '')); });
        it('camera', 'Сделать фото', async () => { const [f] = await U.pickFile('image/*', false, 'environment'); if (f) Ops.insertImage(await U.loadImage(f), 'Фото'); });
        it('text', 'Добавить текст', () => { Tools.set('text'); });
        body.append(U.el('div', { class: 'sect', text: 'Буфер обмена' }));
        it('cut', 'Вырезать', () => Ops.cut(), 'Ctrl+X'); it('copy', 'Копировать', () => Ops.copy(), 'Ctrl+C');
        it('copy', 'Копировать всё (все слои)', () => Ops.copy(true), 'Ctrl+Shift+C'); it('paste', 'Вставить', () => App.paste(), 'Ctrl+V');
        it('copy', 'Скопировать и вставить на новый слой', () => Ops.copyPaste(false)); it('cut', 'Вырезать и вставить на новый слой', () => Ops.copyPaste(true));
        break;
      case 'canvas':
        it('crop', 'Обрезка и размер холста…', () => this.resizeDialog());
        it('rotate', 'Повернуть холст по часовой', () => Ops.rotateCanvas(true)); it('rotate', 'Повернуть холст против часовой', () => Ops.rotateCanvas(false));
        it('flipH', 'Отразить холст по горизонтали', () => Ops.flipCanvas(true)); it('flipV', 'Отразить холст по вертикали', () => Ops.flipCanvas(false));
        body.append(UI.sw('Ассистент анимации', d.anim.on, v => { UI.closePopovers(); Anim.toggle(v); }, 'Слои становятся кадрами, луковая кожура, воспроизведение'));
        body.append(UI.sw('Окно-образец', RefWin.open, v => { UI.closePopovers(); RefWin.toggle(v); }, 'Плавающее окно с холстом или изображением'));
        it('guide', 'Помощник рисования…', () => UI.action('guides', U.$('[data-act=guides]')));
        it('sym', 'Зеркальный вид (только экран)', () => View.toggleFlip(), 'H');
        it('fit', 'Вписать в экран', () => View.fit(), '0 · быстрое сведение пальцев'); it('search', 'Масштаб 100%', () => View.actual(), '1');
        it('info', 'Сведения о холсте', () => this.info());
        break;
      case 'export':
        body.append(U.el('div', { class: 'sect', text: 'Поделиться изображением' }));
        it('image', 'PNG', () => Exporter.image('png')); it('image', 'JPEG', () => Exporter.image('jpeg')); it('image', 'WebP', () => Exporter.image('webp'));
        it('layers', 'PSD (со слоями)', () => Exporter.psd(), 'Открывается в Photoshop, Krita, GIMP, Photopea');
        it('folder', 'Мазок (.mazok)', () => Exporter.mazok(), 'Полный проект со слоями и настройками');
        it('frames', 'Слои в PNG (ZIP)', () => Exporter.layersZip());
        body.append(U.el('div', { class: 'sect', text: 'Анимация (слои = кадры)' }));
        it('film', 'Анимированный GIF', () => Exporter.gif()); it('film', 'Видео анимации (MP4/WebM)', () => Exporter.animVideo());
        it('frames', 'Кадры в PNG (ZIP)', () => Exporter.framesZip());
        break;
      case 'video':
        body.append(UI.sw('Запись таймлапса', Settings.timelapse, v => { Settings.timelapse = v; Settings.save(); }, 'Каждый штрих сохраняется как кадр видео процесса'));
        it('play', 'Смотреть таймлапс', () => Timelapse.replay(), `${d.tlCount} кадров`);
        it('film', 'Экспорт таймлапса — полностью', () => Timelapse.export(false)); it('film', 'Экспорт таймлапса — 30 секунд', () => Timelapse.export(true));
        it('trash', 'Очистить запись таймлапса', async () => { if (await U.confirm('Очистить таймлапс?', 'Записанные кадры будут удалены.', 'Очистить', true)) Timelapse.clear(); });
        break;
      case 'prefs': body.append(this.prefsBody()); break;
      case 'help': body.append(this.helpBody()); break;
    }
    return root;
  },
  prefsDialog() { U.dialog({ title: 'Настройки', body: this.prefsBody(), cls: 'wide', buttons: [{ text: 'Готово', primary: true, value: true }] }); },
  prefsBody() {
    const body = U.el('div');
    body.append(U.el('div', { class: 'sect', text: 'Интерфейс' }), U.el('div', { style: { padding: '0 8px' } }, UI.seg([['auto', 'Автоматически'], ['desk', 'Настольный (меню и панели)'], ['touch', 'Планшетный']], Settings.uiMode || 'auto', v => { Settings.uiMode = v; Settings.save(); UI.applyLayout(); })));
    {
      {
        body.append(UI.sw('Светлая тема', Settings.theme === 'light', v => { Settings.theme = v ? 'light' : 'dark'; Settings.save(); App.applyTheme(); }));
        body.append(UI.sw('Интерфейс для левши', Settings.lefty, v => { Settings.lefty = v; Settings.save(); App.applyTheme(); }, 'Панели меняются местами'));
        body.append(UI.sw('Рисование пальцем', Settings.touchDraw, v => { Settings.touchDraw = v; Settings.save(); }, 'Выключите, если рисуете только стилусом — палец будет двигать холст'));
        body.append(UI.sw('Давление пальца', Settings.fingerPressure, v => { Settings.fingerPressure = v; Settings.save(); }, 'Использовать силу нажатия пальца, если экран её передаёт'));
        body.append(UI.sw('Пипетка при удержании', Settings.holdPick, v => { Settings.holdPick = v; Settings.save(); }));
        body.append(UI.hslider('Задержка пипетки', 200, 1200, Settings.holdDelay, 50, v => { Settings.holdDelay = v; }, v => v + ' мс', () => Settings.save()));
        body.append(UI.sw('Быстрая фигура', Settings.quickShape, v => { Settings.quickShape = v; Settings.save(); }, 'Задержите перо в конце линии, чтобы выровнять её в фигуру'));
        body.append(UI.hslider('Задержка быстрой фигуры', 300, 1500, Settings.quickShapeDelay, 50, v => { Settings.quickShapeDelay = v; }, v => v + ' мс', () => Settings.save()));
        body.append(UI.hslider('Сглаживание штриха (общее)', 0, 200, Math.round(Settings.streamlineMul * 100), 5, v => { Settings.streamlineMul = v / 100; }, v => v + '%', () => Settings.save()));
        body.append(UI.hslider('Кривая давления: мягко ← → жёстко', 40, 250, Math.round(Settings.pressureGamma * 100), 1, v => { Settings.pressureGamma = v / 100; }, v => (v / 100).toFixed(2), () => Settings.save()));
        body.append(UI.sw('Контур кисти', Settings.cursor, v => { Settings.cursor = v; Settings.save(); }, 'Показывать размер кисти под курсором'));
        body.append(UI.sw('Поворот жестом', Settings.rotateGesture, v => { Settings.rotateGesture = v; Settings.save(); }));
        body.append(UI.sw('Привязка поворота к 90°', Settings.rotateSnap, v => { Settings.rotateSnap = v; Settings.save(); }));
        body.append(UI.sw('Пипетка берёт цвет только с текущего слоя', Settings.pickSource === 'layer', v => { Settings.pickSource = v ? 'layer' : 'all'; Settings.save(); }));
        body.append(UI.hslider('Память для отмены', 128, 2048, Settings.historyMB, 64, v => { Settings.historyMB = v; }, v => v + ' МБ', () => Settings.save()));
      }
    }
    return body;
  },
  helpBody() {
    const rows = [
      ['Жесты', ''], ['2 пальца — касание', 'Отменить'], ['3 пальца — касание', 'Повторить'], ['4 пальца — касание', 'Скрыть/показать интерфейс'],
      ['2 пальца — движение', 'Сдвиг, масштаб и поворот холста'], ['Быстрое сведение', 'Вписать холст в экран'], ['Удержание пальца/пера', 'Пипетка'],
      ['Задержка в конце штриха', 'Быстрая фигура: линия, эллипс, многоугольник'], ['Перетащить цвет на холст', 'Заливка области'], ['Заливка + движение вбок', 'Порог заливки'],
      ['Клавиатура', ''], ['B / E / R / S', 'Кисть / Ластик / Палец / Штамп'], ['G / Shift+G', 'Заливка / Градиент'], ['I / M / L / W', 'Пипетка / Выделение / Лассо / Волшебная палочка'],
      ['V / T / U / H / Z', 'Трансформация / Текст / Фигуры / Рука / Масштаб'], ['X / D', 'Поменять цвета / Чёрный и белый'],
      ['[ и ]', 'Размер кисти'], ['Shift+1…0', 'Непрозрачность 10–100%'], ['Ctrl+Z / Ctrl+Shift+Z', 'Отменить / Повторить'], ['Ctrl+T', 'Свободное трансформирование'],
      ['Пробел + перетаскивание', 'Сдвиг холста'], ['Колесо мыши / Ctrl+колесо', 'Масштаб'], ['Alt+колесо', 'Поворот'], ['Alt или ПКМ', 'Пипетка'],
      ['Ctrl+0 / Ctrl+1 / F', 'Вписать / 100% / Зеркальный вид'], ['Ctrl+N / Ctrl+O', 'Новый холст / Открыть'], ['Ctrl+Shift+N', 'Новый слой'], ['Ctrl+J', 'Дублировать слой (или копию выделения)'], ['Ctrl+E', 'Объединить вниз'],
      ['Ctrl+A / Ctrl+D / Ctrl+Shift+I', 'Выделить всё / Снять / Инвертировать'], ['Ctrl+C / X / V', 'Копировать / Вырезать / Вставить'],
      ['Delete', 'Очистить слой или выделение'], ['Enter / Esc', 'Применить / Отменить трансформацию'], ['Tab', 'Скрыть интерфейс'], ['Ctrl+S', 'Сохранить']
    ];
    const t = U.el('table', { class: 'help' });
    for (const [a, b] of rows) t.append(b ? U.el('tr', null, U.el('td', { text: a }), U.el('td', { text: b })) : U.el('tr', null, U.el('td', { colSpan: 2, html: `<div class="sect" style="padding-left:0">${a}</div>` })));
    return U.el('div', null, t, U.el('div', { class: 'row', html: '<small>Мазок — работает без интернета. Рисунки хранятся на этом устройстве.</small>' }));
  },
  async number(title, value, min, max) {
    const inp = U.el('input', { class: 'input', type: 'number', min, max, value });
    const r = await U.dialog({ title, body: inp, buttons: [{ text: 'Отмена', value: null }, { text: 'OK', primary: true, value: () => +inp.value }] });
    return r == null ? null : U.clamp(r, min, max);
  },
  resizeDialog() {
    const d = Engine.doc;
    const W = U.el('input', { class: 'input', type: 'number', min: 1, max: 8192, value: d.w }), H = U.el('input', { class: 'input', type: 'number', min: 1, max: 8192, value: d.h });
    let keep = true, mode = 'canvas', ax = 1, ay = 1;
    W.oninput = () => { if (keep) H.value = Math.round(+W.value * d.h / d.w); };
    H.oninput = () => { if (keep) W.value = Math.round(+H.value * d.w / d.h); };
    const anchor = U.el('div', { style: { display: 'grid', gridTemplateColumns: 'repeat(3,34px)', gap: '4px' } });
    const drawA = () => { anchor.innerHTML = ''; for (let y = 0; y < 3; y++) for (let x = 0; x < 3; x++) anchor.append(U.el('button', { class: 'btn sm' + (x === ax && y === ay ? ' primary' : ''), style: { width: '34px', padding: 0 }, text: '•', onclick: () => { ax = x; ay = y; drawA(); } })); };
    drawA();
    const F = (l, e) => U.el('div', { class: 'field' }, U.el('label', { text: l }), e);
    const body = U.el('div', null,
      F('Что менять', UI.seg([['canvas', 'Размер холста (обрезка/поля)'], ['scale', 'Масштабировать изображение']], mode, v => { mode = v; })),
      U.el('div', { class: 'grid2' }, F('Ширина, px', W), F('Высота, px', H)),
      UI.sw('Сохранять пропорции', keep, v => { keep = v; }),
      F('Привязка (для размера холста)', anchor),
      U.el('div', { class: 'row', style: { flexWrap: 'wrap', gap: '4px' } }, ...[['Квадрат', 1], ['4:3', 4 / 3], ['16:9', 16 / 9], ['3:4', 3 / 4], ['9:16', 9 / 16]].map(([n, r]) => U.el('button', { class: 'btn sm', text: n, onclick: () => { const s = Math.max(+W.value, +H.value); if (r >= 1) { W.value = s; H.value = Math.round(s / r); } else { H.value = s; W.value = Math.round(s * r); } } }))));
    U.dialog({ title: 'Обрезка и размер', body, buttons: [{ text: 'Отмена', value: null }, { text: 'Применить', primary: true, value: () => true }] }).then(ok => {
      if (!ok) return;
      const nw = U.clamp(Math.round(+W.value), 1, 8192), nh = U.clamp(Math.round(+H.value), 1, 8192);
      if (nw * nh > 50e6) { U.toast('Слишком большой холст'); return; }
      if (nw === d.w && nh === d.h) return;
      Ops.resizeCanvas(nw, nh, Math.round((nw - d.w) * ax / 2), Math.round((nh - d.h) * ay / 2), mode === 'scale');
    });
  },
  info() {
    const d = Engine.doc, mb = (d.w * d.h * 4 * (d.layers.length + 6) / 1048576).toFixed(0);
    U.dialog({ title: 'Сведения о холсте', body: U.el('table', { class: 'help', html: `
      <tr><td>Название</td><td>${d.name.replace(/</g, '&lt;')}</td></tr><tr><td>Размер</td><td>${d.w} × ${d.h} px</td></tr>
      <tr><td>Слоёв</td><td>${d.layers.length} (максимум ${d.maxLayers})</td></tr><tr><td>Штрихов</td><td>${d.stats.strokes}</td></tr>
      <tr><td>Время работы</td><td>${U.fmtTime(d.stats.time)}</td></tr><tr><td>Кадров таймлапса</td><td>${d.tlCount}</td></tr>
      <tr><td>Создан</td><td>${U.fmtDate(d.created)}</td></tr><tr><td>Изменён</td><td>${U.fmtDate(d.modified)}</td></tr>
      <tr><td>Память</td><td>≈ ${mb} МБ</td></tr><tr><td>История</td><td>${History.list.length} шагов, ${(History.bytes / 1048576).toFixed(0)} МБ</td></tr>` }) });
  },
  /* ---------- коррекция ---------- */
  adjustList() {
    const body = U.el('div');
    const it = (icon, label, fn) => body.append(UI.item(icon, label, () => { UI.closePopovers(); fn(); }));
    const groups = [['Цвет', ['hsb', 'balance', 'curves', 'gmap', 'desat', 'invert', 'posterize', 'threshold']], ['Размытие', ['blur', 'motion', 'radial']], ['Эффекты', ['sharpen', 'noise', 'bloom', 'chroma', 'halftone', 'pixelate', 'vignette']]];
    for (const [g, ids] of groups) { body.append(U.el('div', { class: 'sect', text: g })); for (const id of ids) { const f = Adjust.list.find(x => x.id === id); it(null, f.name, () => Adjust.open(id)); } }
    body.append(U.el('div', { class: 'sect', text: 'Инструменты' }));
    it('wave', 'Пластика', () => Liquify.begin());
    it('clone', 'Клонирование', () => Tools.set('clone'));
    return body;
  },
  /* ---------- помощник рисования ---------- */
  guides() {
    const g = Engine.doc.guide, body = U.el('div');
    const re = () => { View.requestDraw(); UI.reopen('guides'); };
    body.append(UI.seg([['none', 'Нет'], ['grid', 'Сетка'], ['iso', 'Изометрия'], ['perspective', 'Перспектива'], ['symmetry', 'Симметрия']], g.type, v => { g.type = v; if (v === 'symmetry') g.assist = true; re(); }));
    if (g.type === 'grid' || g.type === 'iso') body.append(UI.hslider('Размер ячейки', 4, 1000, g.size, 1, v => { g.size = v; View.requestDraw(); }, v => v + ' px'));
    if (g.type === 'perspective') {
      body.append(U.el('div', { class: 'sect', text: 'Точки схода' }), UI.seg([[1, 'Одна'], [2, 'Две'], [3, 'Три']], g.vpCount, v => { g.vpCount = v; re(); }));
      body.append(U.el('div', { class: 'row', html: '<small>Перетаскивайте точки схода прямо на холсте, пока открыта эта панель</small>' }));
    }
    if (g.type === 'symmetry') {
      body.append(U.el('div', { class: 'sect', text: 'Вид симметрии' }), UI.seg([['vertical', 'Вертик.'], ['horizontal', 'Гориз.'], ['quadrant', 'Квадранты'], ['radial', 'Радиальная']], g.sym, v => { g.sym = v; re(); }));
      if (g.sym === 'radial') { body.append(UI.hslider('Сегменты', 2, 24, g.segments, 1, v => { g.segments = v; View.requestDraw(); })); body.append(UI.sw('Поворотная (без зеркала)', g.rotational, v => { g.rotational = v; View.requestDraw(); })); }
      body.append(U.el('div', { class: 'row', style: { gap: '6px' } }, U.el('small', { class: 'grow', text: 'Центр можно перетаскивать на холсте' }), U.el('button', { class: 'btn sm', text: 'По центру', onclick: () => { g.cx = Engine.doc.w / 2; g.cy = Engine.doc.h / 2; View.requestDraw(); } })));
    }
    if (g.type !== 'none') {
      body.append(UI.sw(g.type === 'symmetry' ? 'Зеркальное рисование' : 'Помощь при рисовании', g.assist, v => { g.assist = v; }, g.type === 'symmetry' ? 'Штрихи повторяются симметрично' : 'Линии притягиваются к направляющим'));
      body.append(UI.hslider('Непрозрачность', 5, 100, Math.round(g.opacity * 100), 1, v => { g.opacity = v / 100; View.requestDraw(); }, v => v + '%'));
      body.append(UI.hslider('Толщина', 1, 6, g.thick, 0.5, v => { g.thick = v; View.requestDraw(); }, v => v + ' px'));
      const c = U.el('input', { type: 'color', value: g.color, oninput: () => { g.color = c.value; View.requestDraw(); }, style: { width: '60px', height: '30px', border: 0, background: 'none' } });
      body.append(U.el('div', { class: 'row' }, U.el('span', { class: 'grow', text: 'Цвет направляющих' }), c));
    }
    return body;
  }
};

/* ---------- окно-образец ---------- */
const RefWin = {
  open: false, el: null, cv: null, img: null, t: 0,
  toggle(on) {
    if (!on) { this.el && this.el.remove(); this.el = null; this.open = false; return; }
    this.open = true; this.img = null;
    const head = U.el('div', { class: 'rh' }, U.el('span', { text: 'Образец' }),
      U.el('button', { class: 'tb', style: { height: '26px', minWidth: '26px', padding: 0 }, title: 'Показать холст', onclick: () => { this.img = null; this.render(); } }, U.icon('image', 'sm')),
      U.el('button', { class: 'tb', style: { height: '26px', minWidth: '26px', padding: 0 }, title: 'Загрузить изображение', onclick: async () => { const [f] = await U.pickFile('image/*'); if (f) { this.img = await U.loadImage(f); this.render(); } } }, U.icon('folder', 'sm')),
      U.el('button', { class: 'tb', style: { height: '26px', minWidth: '26px', padding: 0 }, title: 'Закрыть', onclick: () => this.toggle(false) }, U.icon('close', 'sm')));
    this.cv = U.el('canvas');
    const rz = U.el('div', { class: 'rz' });
    this.el = U.el('div', { class: 'refwin' }, head, U.el('div', { class: 'rb' }, this.cv), rz);
    U.$('#app').append(this.el);
    this.drag(head, (dx, dy, s) => { this.el.style.left = (s.l + dx) + 'px'; this.el.style.top = (s.t + dy) + 'px'; });
    this.drag(rz, (dx, dy, s) => { this.el.style.width = Math.max(140, s.w + dx) + 'px'; this.el.style.height = Math.max(110, s.h + dy) + 'px'; this.render(); });
    this.cv.addEventListener('pointerdown', e => {
      if (!this.img) return;
      const r = this.cv.getBoundingClientRect(), x = this.cv.getContext('2d', { willReadFrequently: true });
      const v = x.getImageData((e.clientX - r.left) * this.cv.width / r.width, (e.clientY - r.top) * this.cv.height / r.height, 1, 1).data;
      if (v[3]) Colors.set({ r: v[0], g: v[1], b: v[2] });
    });
    this.render();
  },
  drag(h, fn) {
    h.addEventListener('pointerdown', e => {
      if (e.target.closest('button')) return;
      h.setPointerCapture(e.pointerId);
      const s = { x: e.clientX, y: e.clientY, l: this.el.offsetLeft, t: this.el.offsetTop, w: this.el.offsetWidth, h: this.el.offsetHeight };
      const mv = ev => fn(ev.clientX - s.x, ev.clientY - s.y, s);
      const up = () => { h.removeEventListener('pointermove', mv); h.removeEventListener('pointerup', up); };
      h.addEventListener('pointermove', mv); h.addEventListener('pointerup', up);
    });
  },
  dirty() { if (!this.open || this.img) return; clearTimeout(this.t); this.t = setTimeout(() => this.render(), 150); },
  render() {
    if (!this.el) return;
    const src = this.img || Engine.comp; if (!src) return;
    const box = this.cv.parentNode.getBoundingClientRect(), k = Math.min(box.width / src.width, box.height / src.height), dpr = Math.min(2, devicePixelRatio || 1);
    this.cv.width = Math.max(1, src.width * k * dpr); this.cv.height = Math.max(1, src.height * k * dpr);
    this.cv.style.width = src.width * k + 'px'; this.cv.style.height = src.height * k + 'px';
    const x = this.cv.getContext('2d'); x.imageSmoothingQuality = 'high'; x.drawImage(src, 0, 0, this.cv.width, this.cv.height);
  }
};
