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
    Tools.cur = Tools.isPaint() || ['fill', 'select', 'text'].includes(Tools.cur) ? Tools.cur : 'brush';
    if (Tools.cur === 'clone') Tools.cur = 'brush';
    Clone.src = null; Clone.off = null;
    Engine.setDoc(doc); this.dirty = false;
    Gallery.hide(); UI.setTitle(doc.name);
    document.body.classList.toggle('anim', doc.anim.on);
    View.fit(); Anim.refreshBar(); UI.toolChanged(); UI.historyChanged(); UI.layersChanged();
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
  async saveNow() {
    const d = Engine.doc; if (!d || !this.dirty || Tools.busy()) return;
    this.dirty = false; await DocIO.save(d);
  },
  blockHistory() {
    if (Paint.st || Fill.st || Xform.drag) return true;
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
      const d = Engine.doc; if (!d) return;
      const k = e.key.toLowerCase(), c = e.ctrlKey || e.metaKey;
      if (e.code === 'Space') { Input.space = true; if (!Input.mode) View.cv.style.cursor = 'grab'; e.preventDefault(); return; }
      if (c) {
        const map = {
          z: () => e.shiftKey ? History.redo() : History.undo(), y: () => History.redo(),
          s: () => { this.dirty = true; this.saveNow().then(() => U.toast('Сохранено')); },
          c: () => Ops.copy(e.shiftKey), x: () => Ops.cut(), v: () => { if (!Ops.clipboard) return 'pass'; Ops.paste(); },
          a: () => Sel.all(), d: () => Sel.clear(), i: () => e.shiftKey ? Sel.invert() : 'pass',
          n: () => e.shiftKey ? Ops.add() : 'pass', j: () => Ops.duplicate(), e: () => Ops.mergeDown(),
          0: () => View.fit(), '=': () => View.zoomAt(View.W / 2, View.H / 2, 1.25), '-': () => View.zoomAt(View.W / 2, View.H / 2, 0.8)
        };
        const fn = map[k];
        if (fn) { const r = fn(); if (r !== 'pass') e.preventDefault(); }
        return;
      }
      if (e.altKey) { if (Tools.isPaint()) e.preventDefault(); return; }
      if (e.shiftKey && /^Digit\d$/.test(e.code)) { const n = +e.code.slice(5); Tools.ts().opacity = n === 0 ? 1 : n / 10; UI.toolChanged(); this.saveTools(); UI.hint('Непрозрачность ' + Math.round(Tools.ts().opacity * 100) + '%'); return; }
      const tools = { b: 'brush', s: 'smudge', e: 'eraser', g: 'fill', i: 'picker', l: 'select', v: 'transform', t: 'text', m: 'select' };
      if (tools[k] && !e.repeat) { Tools.set(tools[k]); return; }
      switch (e.key) {
        case '[': case ']': {
          const ts = Tools.ts(); ts.size = U.clamp(ts.size + (e.key === ']' ? 0.03 : -0.03), 0, 1); UI.toolChanged(); UI.hint('Размер ' + (Math.round(Tools.cursorSize() * 10) / 10) + ' px'); this.saveTools(); break;
        }
        case '0': View.fit(); break;
        case '1': View.actual(); break;
        case 'h': case 'H': View.toggleFlip(); break;
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
