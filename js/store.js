'use strict';
/* Мазок — локальное хранилище (IndexedDB, с запасным вариантом в памяти) */

const Store = (() => {
  let db = null;
  const STORES = ['docs', 'layers', 'frames', 'kv'];
  const mem = Object.fromEntries(STORES.map(s => [s, new Map()]));

  function open() {
    return new Promise(res => {
      if (!('indexedDB' in window)) return res(null);
      let req;
      try { req = indexedDB.open('mazok', 1); } catch (e) { return res(null); }
      req.onupgradeneeded = () => { const d = req.result; for (const s of STORES) if (!d.objectStoreNames.contains(s)) d.createObjectStore(s); };
      req.onsuccess = () => { db = req.result; res(db); };
      req.onerror = () => res(null);
      req.onblocked = () => res(null);
    }).then(r => { if (navigator.storage && navigator.storage.persist) navigator.storage.persist().catch(() => { }); return r; });
  }
  function run(store, mode, fn) {
    return new Promise((res, rej) => {
      const t = db.transaction(store, mode), s = t.objectStore(store);
      let out;
      const r = fn(s);
      if (r) r.onsuccess = () => { out = r.result; };
      t.oncomplete = () => res(out);
      t.onerror = () => rej(t.error);
      t.onabort = () => rej(t.error);
    });
  }
  const api = {
    open,
    get(store, key) { return db ? run(store, 'readonly', s => s.get(key)) : Promise.resolve(mem[store].get(key)); },
    put(store, key, val) { if (!db) { mem[store].set(key, val); return Promise.resolve(); } return run(store, 'readwrite', s => s.put(val, key)); },
    del(store, key) { if (!db) { mem[store].delete(key); return Promise.resolve(); } return run(store, 'readwrite', s => s.delete(key)); },
    all(store) {
      if (!db) return Promise.resolve(Array.from(mem[store].values()));
      return run(store, 'readonly', s => s.getAll());
    },
    keys(store, prefix) {
      if (!db) return Promise.resolve(Array.from(mem[store].keys()).filter(k => !prefix || String(k).startsWith(prefix)));
      return run(store, 'readonly', s => prefix ? s.getAllKeys(IDBKeyRange.bound(prefix, prefix + '￿')) : s.getAllKeys());
    },
    delPrefix(store, prefix) {
      if (!db) { for (const k of Array.from(mem[store].keys())) if (String(k).startsWith(prefix)) mem[store].delete(k); return Promise.resolve(); }
      return run(store, 'readwrite', s => s.delete(IDBKeyRange.bound(prefix, prefix + '￿')));
    },
    kvGet(key) { return api.get('kv', key); },
    kvSet(key, val) { return api.put('kv', key, val); },
    get persistent() { return !!db; }
  };
  return api;
})();
