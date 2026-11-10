'use strict';
/* Мазок — запуск приложения, автосохранение, горячие клавиши */

const App = {
  dirty: false,
  get doc() { return Engine.doc; },
  async boot() {
    Settings.load(); this.applyTheme(); Colors.load(); this.loadTools();
    await Store.open();
    await Promise.all([Brushes.load(), Palettes.load()]);
    UI.build(); View.init(); View.theme(); this.keys(); this.dnd();
    UI.colorChanged(); UI.toolChanged(); UI.historyChanged();
    window.addEventListener('paste', e => this.onPaste(e));
    document.addEventListener('visibilitychange', () => { if (document.hidden) this.saveNow(); });
    window.addEventListener('pagehide', () => this.saveNow());
    window.addEventListener('popstate', () => {
      if (UI.closePopovers()) { history.pushState({ c: 1 }, ''); return; }
      if (U.$('.modal-back')) { history.pushState({ c: 1 }, ''); return; }
      if (Engine.doc) this.toGallery(true);
    });
    setInterval(() => { if (Engine.doc && !document.hidden) Engine.doc.stats.time++; }, 1000);
    if ('serviceWorker' in navigator && /^https?:/.test(location.protocol)) navigator.serviceWorker.register('sw.js').catch(() => { });
    await Gallery.show();
    if (!Store.persistent) U.toast('Хранилище браузера недоступно — рисунки не сохранятся после закрытия', 5000);
  },
  async open(id) {
    try { U.toast('Открываю…', 800); this.openDoc(await DocIO.load(id)); }
    catch (e) { console.error(e); U.toast('Не удалось открыть: ' + e.message, 3000); }
  },
  openDoc(doc) {
    Tools.cur = Tools.isPaint() || ['fill', 'select', 'text', 'gradient', 'shapes', 'hand', 'zoom'].includes(Tools.cur) ? Tools.cur : 'brush';
    if (Tools.cur === 'clone') Tools.cur = 'brush';
    Clone.src = null; Clone.off = null;
    Engine.setDoc(doc); this.dirty = false;
    Gallery.hide(); UI.setTitle(doc.name);
    document.body.classList.toggle('anim', doc.anim.on);
    View.fit(); Anim.refreshBar(); if (UI.desk) Desk.renderDock(); UI.toolChanged(); UI.historyChanged(); UI.layersChanged();
    const t = Tools.map[Tools.cur]; if (t && t.activate) t.activate();
    if (!history.state || !history.state.c) history.pushState({ c: 1 }, '');
  },
  async toGallery(fromPop) {
    Tools.commitPending(); Anim.stop(); UI.closePopovers(); UI.hideOptbar();
    if (RefWin.open) RefWin.toggle(false);
    this.dirty = true; await this.saveNow();
    Engine.doc = null; Engine.comp = null; Engine._s = {}; History.clear(); Sel.reset();
    View.requestDraw(); U.$('#animbar').classList.add('hidden');
    if (!fromPop && history.state && history.state.c) history.back();
    await Gallery.show();
  },
  changed() {
    if (!Engine.doc) return;
    this.dirty = true; this.autosave();
    if (Settings.timelapse) Timelapse.capture();
  },
  autosave: U.debounce(() => App.saveNow(), 1800),
  async saveNow(force) {
    const d = Engine.doc; if (!d || (!this.dirty && !force) || Tools.busy()) return;
    this.dirty = false; await DocIO.save(d);
  },
  async newCanvas() { if (Engine.doc) { Tools.commitPending(); await this.saveNow(true); } Gallery.newDialog(); },
  async openFile() {
    const files = await U.pickFile('image/*,.psd,.mazok,.zip', true); if (!files.length) return;
    if (Engine.doc) { Tools.commitPending(); await this.saveNow(true); }
    let last = null;
    for (const f of files) { try { U.toast('Открываю: ' + f.name); last = await DocIO.importFile(f); } catch (e) { U.toast(f.name + ': ' + e.message, 3000); } }
    if (last) this.openDoc(last);
  },
  blockHistory() {
    if (Paint.st || Fill.st || Xform.drag || GradientTool.st || ShapeTool.st) return true;
    if (Xform.s) { Xform.cancelAll(); Tools.set(Tools.prev && Tools.prev !== 'transform' ? Tools.prev : 'brush'); U.toast('Трансформация отменена'); return true; }
    if (Liquify.s) { Liquify.discard(); U.toast('Пластика отменена'); return true; }
    if (Adjust.s && !Adjust.brushMode) { Adjust.cancel(); U.toast('Коррекция отменена'); return true; }
    return false;
  },
  async rename() {
    const d = Engine.doc; if (!d) return;
    const n = await U.prompt('Название рисунка', d.name);
    if (n && n.trim()) { d.name = n.trim(); UI.setTitle(d.name); this.changed(); }
  },
  applyTheme() {
    document.documentElement.dataset.theme = Settings.theme;
    document.body.classList.toggle('lefty', !!Settings.lefty);
    const m = document.querySelector('meta[name=theme-color]'); if (m) m.content = Settings.theme === 'light' ? '#efeee9' : '#141518';
    if (View.ctx) { View.theme(); View.fit(); }
    Panels.samples.clear();
  },
  saveTools() { try { localStorage.setItem('mazok.tools', JSON.stringify(Tools.state)); } catch (e) { } },
  loadTools() { try { const o = JSON.parse(localStorage.getItem('mazok.tools') || 'null'); if (o) for (const k in Tools.state) if (o[k]) Object.assign(Tools.state[k], o[k]); } catch (e) { } },
  async paste() {
    if (Ops.clipboard) { Ops.paste(); return; }
    try {
      const items = await navigator.clipboard.read();
      for (const it of items) for (const t of it.types) if (t.startsWith('image/')) { Ops.insertImage(await U.loadImage(await it.getType(t)), 'Вставка'); return; }
    } catch (e) { }
    U.toast('Буфер обмена пуст');
  },
  async onPaste(e) {
    if (e.target.closest && e.target.closest('input,textarea')) return;
    const f = Array.from(e.clipboardData ? e.clipboardData.files : []).find(x => x.type.startsWith('image/'));
    if (!f) { if (Engine.doc && Ops.clipboard) { e.preventDefault(); Ops.paste(); } return; }
    e.preventDefault();
    if (Engine.doc) { Ops.clipboard = null; Ops.insertImage(await U.loadImage(f), 'Вставка'); }
    else Gallery.importFiles([f]);
  },
  dnd() {
    window.addEventListener('dragover', e => e.preventDefault());
    window.addEventListener('drop', async e => {
      e.preventDefault();
      const files = Array.from(e.dataTransfer.files || []); if (!files.length) return;
      if (Engine.doc) {
        for (const f of files) {
          if (f.type.startsWith('image/') && !/\.psd$/i.test(f.name)) Ops.insertImage(await U.loadImage(f), f.name.replace(/\.[^.]+$/, ''));
          else { await Gallery.importFiles([f]); }
        }
      } else Gallery.importFiles(files);
    });
  },
  keys() {
    document.addEventListener('keydown', e => {
      if (e.target.closest && e.target.closest('input,textarea,select')) return;
      if (U.$('.modal-back')) return;
      const k = e.key.toLowerCase(), c = e.ctrlKey || e.metaKey;
      if (c && (e.code === 'KeyN' && !e.shiftKey)) { e.preventDefault(); this.newCanvas(); return; }
      if (c && e.code === 'KeyO') { e.preventDefault(); this.openFile(); return; }
      if (e.key === 'F1') { e.preventDefault(); U.dialog({ title: 'Справка', body: Panels.helpBody(), cls: 'wide' }); return; }
      const d = Engine.doc; if (!d) return;
      if (e.code === 'Space') { Input.space = true; if (!Input.mode) View.cv.style.cursor = 'grab'; e.preventDefault(); return; }
      if (c) {
        const code = e.code.replace(/^Key|^Digit/, '').toLowerCase();
        const map = {
          z: () => e.shiftKey ? History.redo() : History.undo(), y: () => History.redo(),
          s: () => { this.saveNow(true).then(() => U.toast('Сохранено')); },
          c: () => Ops.copy(e.shiftKey), x: () => e.shiftKey ? Liquify.begin() : Ops.cut(), v: () => { if (!Ops.clipboard) return 'pass'; Ops.paste(); },
          a: () => Sel.all(), d: () => Sel.clear(), i: () => e.shiftKey ? Sel.invert() : (e.altKey ? Panels.resizeDialog() : 'pass'),
          n: () => e.shiftKey ? Ops.add() : 'pass', j: () => Sel.active ? Ops.copyPaste(!!e.shiftKey) : Ops.duplicate(), e: () => e.shiftKey ? Ops.flatten() : Ops.mergeDown(),
          t: () => Tools.set('transform'), w: () => this.toGallery(), k: () => Panels.prefsDialog(),
          g: () => e.altKey ? Ops.toggle(d.layer, 'clip', 'Обтравочная маска') : 'pass',
          0: () => View.fit(), 1: () => View.actual(), equal: () => View.zoomAt(View.W / 2, View.H / 2, 1.25), minus: () => View.zoomAt(View.W / 2, View.H / 2, 0.8)
        };
        const fn = map[code] || map[k];
        if (fn) { const r = fn(); if (r !== 'pass') e.preventDefault(); }
        return;
      }
      if (e.altKey) { if (e.key === 'Backspace') { e.preventDefault(); Ops.fill(); return; } if (Tools.isPaint()) e.preventDefault(); return; }
      if (e.shiftKey && /^Digit\d$/.test(e.code)) { const n = +e.code.slice(5); Tools.ts().opacity = n === 0 ? 1 : n / 10; UI.toolChanged(); this.saveTools(); UI.hint('Непрозрачность ' + Math.round(Tools.ts().opacity * 100) + '%'); return; }
      const code = e.code.replace(/^Key/, '').toLowerCase();
      const tools = { b: 'brush', r: 'smudge', e: 'eraser', s: 'clone', g: e.shiftKey ? 'gradient' : 'fill', i: 'picker', v: 'transform', t: 'text', u: 'shapes', h: 'hand', z: 'zoom' };
      const sel = { m: SelectTool.shape === 'rect' && Tools.cur === 'select' ? 'ellipse' : 'rect', l: 'lasso', w: 'wand' };
      if (!e.repeat && sel[code]) { Desk.pickTool('select:' + sel[code]); if (!UI.desk) UI.optbarRefresh(); return; }
      if (!e.repeat && tools[code]) { Tools.set(tools[code]); return; }
      if (code === 'x' && !e.repeat) { Colors.swap(); return; }
      if (code === 'd' && !e.repeat) { Desk.defaultColors(); return; }
      if (code === 'f' && !e.repeat) { View.toggleFlip(); return; }
      if (e.key === '/') { Ops.toggle(d.layer, 'alphaLock', 'Защита прозрачности'); return; }
      switch (e.key) {
        case '[': case ']': {
          const ts = Tools.ts(); ts.size = U.clamp(ts.size + (e.key === ']' ? 0.03 : -0.03), 0, 1); UI.toolChanged(); UI.hint('Размер ' + (Math.round(Tools.cursorSize() * 10) / 10) + ' px'); this.saveTools(); break;
        }
        case '0': View.fit(); break;
        case '1': View.actual(); break;
        case 'Tab': e.preventDefault(); UI.toggleHidden(); break;
        case 'Delete': case 'Backspace': if (Xform.s) break; Ops.clear(); break;
        case 'Enter':
          if (Xform.s) Tools.set(Tools.prev && Tools.prev !== 'transform' ? Tools.prev : 'brush');
          else if (Liquify.s) Liquify.apply(); else if (Adjust.s) Adjust.apply();
          else if (Tools.cur === 'select' && SelectTool.poly.length > 2) SelectTool.finishPoly();
          break;
        case 'Escape':
          if (UI.closePopovers()) break;
          if (Xform.s) { Xform.cancelAll(); Tools.set(Tools.prev && Tools.prev !== 'transform' ? Tools.prev : 'brush'); }
          else if (Liquify.s) Liquify.discard(); else if (Adjust.s) Adjust.cancel();
          else if (SelectTool.poly.length) { SelectTool.poly = []; View.requestDraw(); }
          else if (document.body.classList.contains('ui-hidden')) UI.toggleHidden();
          break;
      }
    });
    document.addEventListener('keyup', e => { if (e.code === 'Space') { Input.space = false; View.cv.style.cursor = ''; } });
    window.addEventListener('blur', () => { Input.space = false; });
  }
};

window.addEventListener('DOMContentLoaded', () => App.boot().catch(e => { console.error(e); U.toast('Ошибка запуска: ' + e.message, 6000); }));
