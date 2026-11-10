'use strict';
/* Мазок — каркас интерфейса: панели, ползунки, всплывающие окна, контекстные панели */

const UI = {
  openId: null, popEl: null, popDef: null, optKind: null, sliders: {}, desk: false,
  build() {
    const tb = U.$('#topbar');
    const B = (act, icon, label, title) => U.el('button', { class: 'tb', 'data-act': act, title: title || label, onclick: e => this.action(act, e.currentTarget) }, U.icon(icon), label ? U.el('span', { text: label }) : null);
    this.undoBtn = B('undo', 'undo', '', 'Отменить (Ctrl+Z, касание двумя пальцами)');
    this.redoBtn = B('redo', 'redo', '', 'Повторить (Ctrl+Shift+Z, касание тремя пальцами)');
    this.title = U.el('div', { id: 'doctitle', title: 'Переименовать', onclick: () => App.rename() });
    this.swatch = U.el('div', { class: 'swatch', title: 'Цвет (перетащите на холст для заливки)' });
    tb.append(
      U.el('div', { class: 'grp' }, B('gallery', 'gallery', 'Галерея'), B('menu', 'menu', 'Действия'), B('adjust', 'adjust', 'Коррекция'), B('guides', 'guide', 'Помощник')),
      this.title, U.el('div', { class: 'grow' }),
      U.el('div', { class: 'grp' }, this.undoBtn, this.redoBtn),
      U.el('div', { class: 'grow' }),
      U.el('div', { class: 'grp' }, B('layers', 'layers', 'Слои'), U.el('button', { class: 'tb', 'data-act': 'color', title: 'Цвет' }, this.swatch))
    );
    this.colorDrop(tb.querySelector('[data-act=color]'));
    const rail = U.$('#rail');
    const tools = [['brush', 'brush', 'Кисть (B)'], ['smudge', 'smudge', 'Палец — растушёвка (R)'], ['eraser', 'eraser', 'Ластик (E)'], null, ['fill', 'fill', 'Заливка (G)'], ['gradient', 'gradient', 'Градиент'], ['picker', 'picker', 'Пипетка (I, удержание пальца, Alt)'], null, ['select', 'select', 'Выделение (M)'], ['transform', 'move', 'Трансформация (V)'], ['text', 'text', 'Текст (T)'], ['shapes', 'shapes', 'Фигуры (U)']];
    for (const t of tools) {
      if (!t) { rail.append(U.el('div', { class: 'rail-sep' })); continue; }
      rail.append(U.el('button', { class: 'tool', 'data-tool': t[0], title: t[2], onclick: e => { if (['brush', 'smudge', 'eraser'].includes(t[0]) && Tools.cur === t[0]) this.toggleBrushLib(e.currentTarget); else Tools.set(t[0]); } }, U.icon(t[1])));
    }
    const sl = U.$('#sliders');
    this.sliders.size = this.vslider('Размер', v => { Tools.ts().size = v; this.sizePreview(true); }, () => Tools.ts().size, v => { const b = Tools.brush(); return Math.round(Brushes.sizePx(b, v) * 10) / 10 + ' px'; }, () => { this.sizePreview(false); App.saveTools(); Desk.renderOptions(); });
    this.sliders.opacity = this.vslider('Непрозр.', v => { Tools.ts().opacity = Math.max(0.01, v); }, () => Tools.ts().opacity, v => Math.round(v * 100) + '%', () => { App.saveTools(); Desk.renderOptions(); });
    this.pickBtn = U.el('button', { class: 'sbtn', title: 'Пипетка', onclick: () => Tools.set('picker') }, U.icon('picker', 'sm'));
    const setBtn = U.el('button', { class: 'sbtn', title: 'Настройки кисти', onclick: () => Panels.studio(Tools.brush().id) }, U.icon('settings', 'sm'));
    sl.append(this.sliders.size.el, this.pickBtn, this.sliders.opacity.el, setBtn);
    Desk.build();
    window.addEventListener('resize', () => { this.applyLayout(true); this.position(); });
    this.applyLayout();
  },
  /* выбор раскладки: настольная (меню и панели) или планшетная */
  applyLayout(fromResize) {
    const m = Settings.uiMode || 'auto';
    const auto = window.innerWidth >= 1000 && window.innerHeight >= 600 && !matchMedia('(pointer:coarse)').matches;
    const desk = m === 'desk' ? window.innerWidth >= 700 : m === 'touch' ? false : auto;
    if (desk === this.desk && fromResize) return;
    const changed = desk !== this.desk;
    this.desk = desk; document.body.classList.toggle('desk', desk);
    this.closePopovers(); Desk.closeMenus();
    if (desk) { Desk.renderDock(); Desk.renderOptions(); Desk.sync(); Desk.status(); }
    if (this.optKind) this.optbarRefresh();
    if (changed && View.ctx) { View.theme(); View.fit(); }
  },
  action(act, btn) {
    switch (act) {
      case 'gallery': App.toGallery(); break;
      case 'undo': History.undo(); break;
      case 'redo': History.redo(); break;
      case 'menu': this.pop('menu', btn, () => Panels.menu(), { title: 'Действия', width: 360 }); break;
      case 'adjust': this.pop('adjust', btn, () => Panels.adjustList(), { title: 'Коррекция', width: 300 }); break;
      case 'guides': this.pop('guides', btn, () => Panels.guides(), { title: 'Помощник рисования', width: 330, onOpen: () => { Guides.editing = true; View.requestDraw(); }, onClose: () => { Guides.editing = false; View.requestDraw(); App.changed(); } }); break;
      case 'layers': this.pop('layers', btn, () => Panels.layers(), { title: 'Слои', width: 340, head: Panels.layersHead() }); break;
      case 'color': this.pop('color', btn, () => ColorUI.build(), { title: 'Цвет', width: 324 }); break;
    }
  },
  /* ---------- всплывающие панели ---------- */
  pop(id, anchor, build, o = {}) {
    if (this.openId === id) { this.closePopovers(); return; }
    this.closePopovers();
    const el = U.el('div', { class: 'pop', style: { width: (o.width || 320) + 'px' } });
    const head = U.el('div', { class: 'pop-h' }, U.el('span', { class: 't', text: o.title || '' }));
    if (o.head) head.append(o.head);
    head.append(U.el('button', { class: 'tb', title: 'Закрыть', onclick: () => this.closePopovers() }, U.icon('close', 'sm')));
    const body = U.el('div', { class: 'pop-b' });
    body.append(build());
    el.append(head, body);
    U.$('#popovers').append(el);
    this.openId = id; this.popEl = el; this.popDef = { anchor, build, o, body };
    if (anchor) anchor.classList.add('on');
    this.placePop();
    if (o.onOpen) o.onOpen();
    return el;
  },
  placePop() {
    const el = this.popEl; if (!el) return;
    const a = this.popDef.anchor, w = el.offsetWidth;
    if (window.innerWidth <= 560) { el.style.left = ''; el.style.top = ''; return; }
    let left = this.desk ? 60 : 70, top = this.desk ? 106 : 56;
    if (a && a.isConnected) { const r = a.getBoundingClientRect(); left = r.left + r.width / 2 > window.innerWidth / 2 ? r.right - w : r.left; top = r.bottom + 6; if (this.desk && r.left < 50) { left = r.right + 6; top = r.top; } }
    el.style.left = U.clamp(left, 6, window.innerWidth - w - 6) + 'px'; el.style.top = Math.max(6, Math.min(top, window.innerHeight - el.offsetHeight - 6)) + 'px';
  },
  reopen(id) {
    Desk.refreshPanel(id);
    if (this.openId !== id) return;
    const { build, body } = this.popDef, st = body.scrollTop;
    body.innerHTML = ''; body.append(build()); body.scrollTop = st;
  },
  closePopovers() {
    const menus = Desk.built ? Desk.closeMenus() : false;
    if (!this.popEl) return menus;
    const def = this.popDef;
    this.popEl.remove(); if (def.anchor) def.anchor.classList.remove('on');
    this.popEl = null; this.openId = null; this.popDef = null;
    if (def.o.onClose) def.o.onClose();
    return true;
  },
  toggleBrushLib(anchor) {
    anchor = anchor || U.$(`#rail [data-tool=${Tools.cur}]`);
    this.pop('brushes', anchor, () => Panels.brushLib(), { title: { brush: 'Кисти', smudge: 'Кисть для пальца', eraser: 'Кисть ластика', clone: 'Кисть штампа' }[Tools.cur] || 'Кисти', width: 380, head: Panels.brushHead() });
    if (this.popEl && !this.desk) { const r = U.$('#rail').getBoundingClientRect(); if (window.innerWidth > 560) { this.popEl.style.left = (Settings.lefty ? r.left - 386 : r.right + 6) + 'px'; this.popEl.style.top = r.top + 'px'; } }
  },
  /* ---------- элементы ---------- */
  vslider(label, set, get, fmt, end) {
    const el = U.el('div', { class: 'vs', title: label }), fill = U.el('div', { class: 'fill' }), knob = U.el('div', { class: 'knob' }), tip = U.el('div', { class: 'tip' });
    el.append(fill, knob, U.el('div', { class: 'lbl', text: label.slice(0, 6) }), tip);
    const show = () => { const v = get(); fill.style.height = (v * 100) + '%'; knob.style.bottom = (v * 100) + '%'; tip.textContent = fmt(v); tip.style.bottom = `calc(${v * 100}% - 12px)`; };
    let y0 = 0, v0 = 0;
    el.addEventListener('pointerdown', e => {
      el.setPointerCapture(e.pointerId); el.classList.add('drag');
      const r = el.getBoundingClientRect(); y0 = e.clientY; v0 = get();
      if (e.pointerType === 'mouse' || Math.abs((1 - (e.clientY - r.top) / r.height) - v0) > 0.12) { v0 = U.clamp(1 - (e.clientY - r.top) / r.height, 0, 1); set(v0); }
      show();
    });
    el.addEventListener('pointermove', e => { if (!el.classList.contains('drag')) return; const r = el.getBoundingClientRect(); set(U.clamp(v0 - (e.clientY - y0) / r.height, 0, 1)); show(); });
    const stop = () => { if (el.classList.contains('drag')) { el.classList.remove('drag'); end && end(); } };
    el.addEventListener('pointerup', stop); el.addEventListener('pointercancel', stop);
    return { el, show };
  },
  sizePreview(on) {
    const p = U.$('#sizepreview');
    if (!on) { p.style.display = 'none'; return; }
    const d = Math.max(2, Math.min(600, Tools.cursorSize() * View.s));
    p.style.display = 'block'; p.style.width = p.style.height = d + 'px';
  },
  hslider(label, min, max, value, step, onInput, fmt, onEnd) {
    const out = U.el('output', { text: fmt ? fmt(value) : value });
    const inp = U.el('input', { type: 'range', min, max, step: step || 1, value });
    inp.addEventListener('input', () => { const v = +inp.value; out.textContent = fmt ? fmt(v) : v; onInput(v); });
    if (onEnd) inp.addEventListener('change', () => onEnd(+inp.value));
    return U.el('div', { class: 'hs' }, U.el('label', { text: label }), out, inp);
  },
  seg(options, value, onChange) {
    const el = U.el('div', { class: 'seg' });
    for (const [v, label, icon] of options) {
      const b = U.el('button', { class: v === value ? 'on' : '', title: label, onclick: () => { U.$$('button', el).forEach(x => x.classList.remove('on')); b.classList.add('on'); onChange(v); } }, icon ? U.icon(icon, 'sm') : label);
      el.append(b);
    }
    return el;
  },
  sw(label, checked, onChange, sub) {
    const inp = U.el('input', { type: 'checkbox', checked, onchange: () => onChange(inp.checked) });
    return U.el('label', { class: 'row btn' }, U.el('div', { class: 'grow' }, label, sub ? U.el('small', { html: '<br>' + sub }) : null), U.el('span', { class: 'sw' }, inp, U.el('span')));
  },
  item(icon, label, fn, sub) {
    return U.el('div', { class: 'row btn', onclick: fn }, icon ? U.icon(icon) : null, U.el('div', { class: 'grow' }, label, sub ? U.el('small', { html: '<br>' + sub }) : null));
  },
  hint(text, ms = 1300) { const h = U.$('#hint'); h.textContent = text; h.classList.add('show'); clearTimeout(this._ht); this._ht = setTimeout(() => h.classList.remove('show'), ms); },
  /* ---------- перетаскивание цвета на холст ---------- */
  colorDrop(btn) {
    let st = null, ghost = null;
    btn.addEventListener('pointerdown', e => { btn.setPointerCapture(e.pointerId); st = { x: e.clientX, y: e.clientY, drag: false }; });
    btn.addEventListener('pointermove', e => {
      if (!st) return;
      if (!st.drag && Math.hypot(e.clientX - st.x, e.clientY - st.y) > 12) { st.drag = true; ghost = U.el('div', { style: { position: 'fixed', width: '34px', height: '34px', borderRadius: '50%', background: Colors.hex(), border: '3px solid #fff', boxShadow: '0 4px 16px #0008', pointerEvents: 'none', zIndex: 99, transform: 'translate(-50%,-50%)' } }); document.body.append(ghost); }
      if (ghost) { ghost.style.left = e.clientX + 'px'; ghost.style.top = e.clientY + 'px'; }
    });
    btn.addEventListener('pointerup', e => {
      if (!st) return;
      if (st.drag) { ghost && ghost.remove(); ghost = null; const el = document.elementFromPoint(e.clientX, e.clientY); if (el && el.id === 'stage') { Tools.commitPending(); Fill.dropAt(e.clientX, e.clientY, Colors.rgb()); } }
      else this.action('color', btn);
      st = null;
    });
  },
  /* ---------- уведомления о смене состояния ---------- */
  toolChanged() {
    U.$$('#rail .tool').forEach(b => b.classList.toggle('on', b.dataset.tool === Tools.cur || (Tools.cur === 'clone' && b.dataset.tool === 'brush')));
    this.pickBtn && this.pickBtn.classList.toggle('on', Tools.cur === 'picker');
    this.sliders.size && this.sliders.size.show(); this.sliders.opacity && this.sliders.opacity.show();
    if (this.openId === 'brushes' && !this.desk) this.closePopovers();
    if (this.openId === 'brushes' && this.desk) this.reopen('brushes');
    Desk.sync(); Desk.renderOptions(); Desk.refreshPanel('brushset'); Desk.refreshPanel('brushes');
    const hints = { brush: 'Кисть: рисуйте; удержание — пипетка, задержка в конце — фигура', eraser: 'Ластик', smudge: 'Палец: растушёвка цвета', clone: 'Штамп: Alt+клик — источник', fill: 'Заливка: щелчок — залить, тяните вбок — порог', gradient: 'Градиент: протяните от начала к концу (Shift — 45°)', select: 'Выделение: Shift — добавить, Alt — вычесть', transform: 'Трансформация: тяните маркеры, Enter — применить, Esc — отмена', text: 'Текст: щелчок по холсту', shapes: 'Фигуры: протяните (Shift — пропорционально, Alt — от центра)', picker: 'Пипетка: щелчок — взять цвет', hand: 'Рука: перемещение холста', zoom: 'Масштаб: щелчок — увеличить, Alt — уменьшить, тяните — плавно' };
    Desk.hint(hints[Tools.cur] || '');
  },
  colorChanged() {
    if (this.swatch) this.swatch.style.background = Colors.hex();
    if (this.openId === 'color' && this.popDef.body.firstChild && this.popDef.body.firstChild._refresh) this.popDef.body.firstChild._refresh();
    const p = Desk.panelEl('color'); if (p && p._refresh) p._refresh();
    Desk.sync();
  },
  layersChanged() {
    if (this._lc) return;
    this._lc = requestAnimationFrame(() => {
      this._lc = 0;
      if (this.openId === 'layers' && !Panels.dragging) this.reopen('layers');
      else if (!Panels.dragging) Desk.refreshPanel('layers');
      Anim.refreshBar(); Desk.status();
    });
  },
  historyChanged() {
    if (!this.undoBtn) return;
    this.undoBtn.disabled = !History.pos; this.redoBtn.disabled = History.pos >= History.list.length;
    Desk.refreshPanel('history');
  },
  selChanged() { if (this.optKind === 'select' || (this.desk && Tools.cur === 'select')) this.optbarRefresh(); },
  viewChanged() {
    if (!Engine.doc) return;
    const deg = Math.round(((View.r * 180 / Math.PI) % 360 + 360) % 360);
    if (Input.mode === 'gesture') this.hint(`${Math.round(View.s * 100)}%` + (deg ? ` · ${deg}°` : ''), 700);
    Desk.status();
  },
  setTitle(t) { if (this.title) this.title.textContent = t; document.title = t + ' — Мазок'; Desk.status(); },
  toggleHidden() { document.body.classList.toggle('ui-hidden'); this.closePopovers(); if (document.body.classList.contains('ui-hidden')) U.toast('Интерфейс скрыт — Tab или касание 4 пальцами, чтобы вернуть'); },
  position() { this.placePop(); },
  /* ---------- контекстная панель ---------- */
  showOptbar(kind) { this.optKind = kind; this.optbarRefresh(); },
  hideOptbar() { this.optKind = null; const o = U.$('#optbar'); o.className = 'hidden'; o.innerHTML = ''; Desk.renderOptions(); },
  optbarRefresh() {
    const o = U.$('#optbar'), k = this.optKind;
    if (this.desk) {
      Desk.renderOptions();
      if (k === 'adjust') { o.innerHTML = ''; o.className = ''; this.adjustPanel(o); } else { o.className = 'hidden'; o.innerHTML = ''; }
      return;
    }
    if (!k) return;
    o.innerHTML = ''; o.className = '';
    if (k === 'adjust') this.adjustPanel(o); else this.optContent(k, o);
  },
  optContent(k, o) {
    const btn = (label, fn, cls, icon, title) => U.el('button', { class: 'ob' + (cls ? ' ' + cls : ''), title: title || label, onclick: fn }, icon ? U.icon(icon, 'sm') : null, label ? U.el('span', { text: label }) : null);
    const sep = () => U.el('div', { class: 'obsep' });
    const wide = this.desk || window.innerWidth > 900;
    if (k === 'paint') Desk.paintOptions(o);
    else if (k === 'select') {
      const T = SelectTool;
      if (!this.desk) for (const [id, n, ic] of [['rect', 'Прямоугольник', 'rect'], ['ellipse', 'Эллипс', 'ellipse'], ['lasso', 'Лассо', 'lasso'], ['wand', 'Авто', 'wand']])
        o.append(btn(wide ? n : '', () => { T.shape = id; T.poly = []; this.optbarRefresh(); }, T.shape === id ? 'on' : '', ic, n));
      if (!this.desk) o.append(sep());
      for (const [id, n] of [['replace', 'Новое'], ['add', '+ Добавить'], ['subtract', '− Вычесть'], ['intersect', '∩ Пересечь']]) o.append(btn(n, () => { T.op = id; this.optbarRefresh(); }, T.op === id ? 'on' : ''));
      if (T.shape === 'wand') o.append(sep(), this.desk ? Desk.mini('Допуск', 0, 100, T.tol, 1, v => v + '', v => { T.tol = v; }) : U.el('span', { class: 'ob', text: 'Порог ' + T.tol + '% — тяните вбок' }));
      o.append(sep(), btn('', () => Sel.invert(), '', 'invert', 'Инвертировать'), btn('', async () => { const v = await Panels.number('Растушёвка, px', 8, 0, 200); if (v) Sel.feather(v); }, '', 'feather', 'Растушевать'),
        btn('', () => Ops.copyPaste(false), '', 'copy', 'Скопировать на новый слой'), btn('', () => Ops.fill(), '', 'fill', 'Залить выделение цветом'), btn('', () => Ops.clear(), '', 'trash', 'Очистить выделенное'),
        btn('Снять', () => Sel.clear(), Sel.active ? '' : 'off'));
      if (T.shape === 'lasso') o.append(U.el('span', { class: 'ob', style: { opacity: 0.7 }, text: 'Обведите или ставьте точки' }));
    } else if (k === 'transform') {
      const X = Xform;
      for (const [id, n] of [['free', 'Свободно'], ['uniform', 'Пропорц.'], ['distort', 'Искажение'], ['warp', 'Деформация']]) o.append(btn(n, () => X.setMode(id), X.mode === id ? 'on' : ''));
      o.append(sep(), btn('', () => X.act('flipH'), '', 'flipH', 'Отразить по горизонтали'), btn('', () => X.act('flipV'), '', 'flipV', 'Отразить по вертикали'), btn('', () => X.act('rot45'), '', 'rotate', 'Повернуть на 45°'),
        btn('', () => X.act('fit'), '', 'fit', 'По размеру холста'), btn('Сглаж.', () => X.act('smooth'), X.smooth ? 'on' : '', null, 'Бикубическая интерполяция'), btn('Сброс', () => X.act('reset')),
        sep(), btn('Отмена', () => { X.cancelAll(); Tools.set(Tools.prev && Tools.prev !== 'transform' ? Tools.prev : 'brush'); }), btn('Готово', () => Tools.set(Tools.prev && Tools.prev !== 'transform' ? Tools.prev : 'brush'), 'primary'));
    } else if (k === 'fill') {
      o.append(U.el('span', { class: 'ob', text: 'Источник:' }));
      for (const [id, n] of [['layer', 'Слой'], ['all', 'Все слои'], ['reference', 'Референс']]) o.append(btn(n, () => { Settings.fillSource = id; Settings.save(); this.optbarRefresh(); }, Settings.fillSource === id ? 'on' : ''));
      o.append(sep());
      if (this.desk) o.append(Desk.mini('Допуск', 0, 100, Settings.fillTol, 1, v => v + '%', v => { Settings.fillTol = v; }, () => Settings.save()), Desk.mini('Расширить', 0, 4, Settings.fillExpand, 1, v => v + ' px', v => { Settings.fillExpand = v; }, () => Settings.save()));
      else {
        const tol = this.hslider('Порог', 0, 100, Settings.fillTol, 1, v => { Settings.fillTol = v; }, v => v + '%', () => Settings.save()); tol.style.width = '170px';
        const ex = this.hslider('Расширить', 0, 4, Settings.fillExpand, 1, v => { Settings.fillExpand = v; }, v => v + ' px', () => Settings.save()); ex.style.width = '130px';
        o.append(tol, ex);
      }
    } else if (k === 'clone') {
      o.append(btn('Задать источник', () => { Clone.src = null; this.hint('Коснитесь холста, чтобы задать источник'); }, '', 'clone'),
        btn('Выровненный', () => { Settings.cloneAligned = !Settings.cloneAligned; Clone.off = null; Settings.save(); this.optbarRefresh(); }, Settings.cloneAligned ? 'on' : ''),
        U.el('span', { class: 'ob', style: { opacity: 0.7 }, text: Clone.src ? 'Рисуйте — копируется из источника' : 'Коснитесь холста или Alt+клик — источник' }));
      if (!this.desk) o.append(btn('Готово', () => Tools.set('brush'), 'primary'));
    } else if (k === 'liquify') {
      const Lq = Liquify;
      const sel = U.el('select', { class: 'input op-sel', style: { width: '140px', height: '30px' }, onchange: () => { Lq.mode = sel.value; } });
      for (const [id, n] of Lq.modes) sel.append(U.el('option', { value: id, text: n, selected: id === Lq.mode }));
      const pc = v => v + '%';
      const s1 = this.desk ? Desk.mini('Размер', 1, 100, Math.round(Lq.size * 100), 1, pc, v => { Lq.size = v / 100; View.requestDraw(); }) : this.hslider('Размер', 1, 100, Math.round(Lq.size * 100), 1, v => { Lq.size = v / 100; View.requestDraw(); }, pc);
      const s2 = this.desk ? Desk.mini('Сила', 1, 100, Math.round(Lq.strength * 100), 1, pc, v => { Lq.strength = v / 100; }) : this.hslider('Сила', 1, 100, Math.round(Lq.strength * 100), 1, v => { Lq.strength = v / 100; }, pc);
      const s3 = this.desk ? Desk.mini('Искажение', 0, 100, Math.round(Lq.distortion * 100), 1, pc, v => { Lq.distortion = v / 100; }) : this.hslider('Искажение', 0, 100, Math.round(Lq.distortion * 100), 1, v => { Lq.distortion = v / 100; }, pc);
      if (!this.desk) { s1.style.width = '140px'; s2.style.width = '130px'; s3.style.width = '130px'; }
      o.append(U.el('b', { class: 'ob', text: 'Пластика' }), sel, s1, s2, s3, btn('Сброс', () => Lq.reset()), btn('Отмена', () => Lq.discard()), btn('Применить', () => Lq.apply(), 'primary'));
    } else if (k === 'gradient') {
      const G = GradientTool;
      for (const [id, n] of G.types) o.append(btn(n, () => { G.type = id; this.optbarRefresh(); }, G.type === id ? 'on' : ''));
      const sel = U.el('select', { class: 'input op-sel', style: { width: '190px', height: '30px' }, onchange: () => { G.colors = sel.value; } });
      for (const [id, n] of G.presets()) sel.append(U.el('option', { value: id, text: n, selected: id === G.colors }));
      o.append(sep(), sel, btn('Инверсия', () => { G.reverse = !G.reverse; this.optbarRefresh(); }, G.reverse ? 'on' : ''));
      if (this.desk) o.append(Desk.mini('Непрозр.', 0, 100, Math.round(G.opacity * 100), 1, v => v + '%', v => { G.opacity = v / 100; }));
    } else if (k === 'shapes') {
      const S = ShapeTool;
      for (const [id, n] of S.kinds) o.append(btn(n, () => { S.kind = id; this.optbarRefresh(); }, S.kind === id ? 'on' : ''));
      o.append(sep());
      if (S.kind !== 'line') o.append(btn('Заливка', () => { S.fill = !S.fill; this.optbarRefresh(); }, S.fill ? 'on' : ''), btn('Обводка', () => { S.stroke = !S.stroke; this.optbarRefresh(); }, S.stroke || !S.fill ? 'on' : ''));
      const px = v => v + ' px';
      o.append(this.desk ? Desk.mini('Толщина', 1, 200, S.width, 1, px, v => { S.width = v; }) : this.hslider('Толщина', 1, 200, S.width, 1, v => { S.width = v; }, px));
      if (S.kind === 'rect') o.append(this.desk ? Desk.mini('Скругление', 0, 300, S.radius, 1, px, v => { S.radius = v; }) : this.hslider('Скругление', 0, 300, S.radius, 1, v => { S.radius = v; }, px));
      if (S.kind === 'polygon' || S.kind === 'star') o.append(this.desk ? Desk.mini('Углов', 3, 16, S.sides, 1, v => v + '', v => { S.sides = v; }) : this.hslider('Углов', 3, 16, S.sides, 1, v => { S.sides = v; }));
      o.append(U.el('span', { class: 'ob', style: { opacity: 0.7 }, text: S.fill && S.stroke ? 'Заливка — основной цвет, обводка — фоновый' : 'Цвет — основной' }));
    } else if (k === 'picker') {
      o.append(U.el('span', { class: 'ob', text: 'Образец:' }));
      for (const [id, n] of [['all', 'Все слои'], ['layer', 'Текущий слой']]) o.append(btn(n, () => { Settings.pickSource = id; Settings.save(); this.optbarRefresh(); }, Settings.pickSource === id ? 'on' : ''));
    } else if (k === 'text') {
      o.append(U.el('span', { class: 'ob', style: { opacity: 0.8 }, text: 'Щёлкните по холсту, чтобы добавить текст; по существующему тексту — чтобы изменить' }));
    } else if (k === 'nav') {
      o.append(btn('Вписать', () => View.fit(), '', 'fit'), btn('100%', () => View.actual()), btn('Сбросить поворот', () => View.rotateAt(View.W / 2, View.H / 2, -View.r), '', 'rotate'), btn('Зеркальный вид', () => View.toggleFlip(), View.flip < 0 ? 'on' : '', 'flipH'));
    }
  },
  showAdjustPanel(f) { this.adjF = f; this.showOptbar('adjust'); },
  adjustPanel(o) {
    const f = this.adjF, A = Adjust; if (!A.s) return;
    o.className = 'tall';
    o.append(U.el('div', { class: 'row', style: { padding: '0 0 6px' } }, U.el('b', { class: 'grow', text: f.name }),
      this.seg([['layer', 'Слой'], ['brush', 'Кисть']], A.brushMode ? 'brush' : 'layer', v => A.setBrushMode(v === 'brush'))));
    for (const q of f.params) {
      if (q[1] === 'curves') { o.append(A.curveEditor(A.s.p, () => A.update())); continue; }
      if (Array.isArray(q[2])) { o.append(U.el('div', { class: 'hs' }, U.el('label', { text: q[1] }), U.el('span'), U.el('div', { style: { gridColumn: '1/3' } }, this.seg(q[2].map((n, i) => [i, n]), A.s.p[q[0]], v => A.setParam(q[0], v))))); continue; }
      o.append(this.hslider(q[1], q[2], q[3], A.s.p[q[0]], 1, v => A.setParam(q[0], v)));
    }
    if (!f.params.length) o.append(U.el('div', { class: 'row', html: '<small>Без параметров — нажмите «Применить»</small>' }));
    o.append(U.el('div', { class: 'row', style: { justifyContent: 'flex-end', padding: '8px 0 0' } },
      U.el('button', { class: 'btn', text: 'Сброс', onclick: () => A.reset() }), U.el('button', { class: 'btn', text: 'Отмена', onclick: () => A.cancel() }),
      U.el('button', { class: 'btn primary', text: A.brushMode ? 'Готово' : 'Применить', onclick: () => A.apply() })));
  },
  /* ---------- диалог текста ---------- */
  textDialog(t) {
    const ta = U.el('textarea', { class: 'input', value: t.str, placeholder: 'Введите текст…', rows: 3 });
    const fonts = [['system-ui, sans-serif', 'Системный'], ['Arial, Helvetica, sans-serif', 'Arial'], ['"Segoe UI", Roboto, sans-serif', 'Segoe/Roboto'], ['Georgia, serif', 'Georgia'], ['"Times New Roman", serif', 'Times'], ['"Courier New", monospace', 'Courier'], ['Impact, "Arial Black", sans-serif', 'Impact'], ['"Comic Sans MS", cursive', 'Comic'], ['"Segoe Script", "Brush Script MT", cursive', 'Рукописный'], ['Verdana, sans-serif', 'Verdana'], ['"Trebuchet MS", sans-serif', 'Trebuchet']];
    const font = U.el('select', { class: 'input' }); for (const [v, n] of fonts) font.append(U.el('option', { value: v, text: n, selected: v === t.font }));
    const size = U.el('input', { class: 'input', type: 'number', min: 4, max: 2000, value: t.size });
    const color = U.el('input', { type: 'color', value: t.color, style: { width: '100%', height: '36px', border: 0, background: 'none' } });
    const lh = U.el('input', { class: 'input', type: 'number', step: 0.1, min: 0.5, max: 4, value: t.lh || 1.2 });
    const sp = U.el('input', { class: 'input', type: 'number', min: -50, max: 200, value: t.spacing || 0 });
    const stroke = U.el('input', { class: 'input', type: 'number', min: 0, max: 100, value: t.stroke || 0 });
    const scol = U.el('input', { type: 'color', value: t.strokeColor || '#000000', style: { width: '100%', height: '36px', border: 0, background: 'none' } });
    let bold = t.bold, italic = t.italic, align = t.align || 'left';
    const F = (l, e) => U.el('div', { class: 'field' }, U.el('label', { text: l }), e);
    const body = U.el('div', null, ta,
      U.el('div', { class: 'grid2', style: { marginTop: '10px' } }, F('Шрифт', font), F('Размер, px', size), F('Цвет', color), F('Межстрочный', lh), F('Трекинг, px', sp), F('Обводка, px', stroke), F('Цвет обводки', scol),
        F('Начертание', this.seg([['b', 'Жирный'], ['i', 'Курсив'], ['n', 'Обычный']], bold ? 'b' : italic ? 'i' : 'n', v => { bold = v === 'b'; italic = v === 'i'; }))),
      F('Выравнивание', this.seg([['left', 'Влево'], ['center', 'По центру'], ['right', 'Вправо']], align, v => { align = v; })));
    return U.dialog({ title: t.str ? 'Изменить текст' : 'Новый текст', body, buttons: [{ text: 'Отмена', value: null }, { text: 'Готово', primary: true, value: () => Object.assign({}, t, { str: ta.value, font: font.value, size: U.clamp(+size.value || 24, 4, 2000), color: color.value, lh: +lh.value || 1.2, spacing: +sp.value || 0, stroke: +stroke.value || 0, strokeColor: scol.value, bold, italic, align }) }] });
  }
};
