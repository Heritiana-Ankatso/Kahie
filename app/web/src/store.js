// Local persistence in IndexedDB (falls back to memory when unavailable,
// e.g. private windows that block storage).

const DB_NAME = 'kahie';
const DB_VERSION = 1;
let db = null;
const mem = { notebooks: new Map(), pages: new Map() };

function req(r) {
  return new Promise((resolve, reject) => { r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
}

export async function open() {
  try {
    const r = indexedDB.open(DB_NAME, DB_VERSION);
    r.onupgradeneeded = () => {
      const d = r.result;
      if (!d.objectStoreNames.contains('notebooks')) d.createObjectStore('notebooks', { keyPath: 'id' });
      if (!d.objectStoreNames.contains('pages')) {
        const s = d.createObjectStore('pages', { keyPath: 'id' });
        s.createIndex('notebookId', 'notebookId');
      }
    };
    db = await req(r);
    navigator.storage?.persist?.().catch(() => {});
  } catch (e) {
    console.warn('IndexedDB unavailable, using memory only', e);
    db = null;
  }
  return !!db;
}

export const persistent = () => !!db;

function tx(store, mode = 'readonly') {
  return db.transaction(store, mode).objectStore(store);
}

function done(t) {
  return new Promise((resolve, reject) => { t.oncomplete = resolve; t.onerror = () => reject(t.error); t.onabort = () => reject(t.error); });
}

export async function listNotebooks() {
  const all = db ? await req(tx('notebooks').getAll()) : [...mem.notebooks.values()];
  return all.sort((a, b) => b.updated - a.updated);
}

export async function putNotebook(nb) {
  if (!db) { mem.notebooks.set(nb.id, structuredClone(nb)); return; }
  await req(tx('notebooks', 'readwrite').put(nb));
}

export async function listPages(notebookId) {
  const all = db
    ? await req(tx('pages').index('notebookId').getAll(notebookId))
    : [...mem.pages.values()].filter((p) => p.notebookId === notebookId);
  return all.sort((a, b) => a.order - b.order);
}

export async function allPages() {
  return db ? req(tx('pages').getAll()) : [...mem.pages.values()];
}

export async function getPage(id) {
  return db ? req(tx('pages').get(id)) : structuredClone(mem.pages.get(id));
}

export async function putPage(page) {
  if (!db) { mem.pages.set(page.id, structuredClone(page)); return; }
  await req(tx('pages', 'readwrite').put(page));
}

export async function putMany(notebooks, pages) {
  if (!db) {
    notebooks.forEach((n) => mem.notebooks.set(n.id, structuredClone(n)));
    pages.forEach((p) => mem.pages.set(p.id, structuredClone(p)));
    return;
  }
  const t = db.transaction(['notebooks', 'pages'], 'readwrite');
  notebooks.forEach((n) => t.objectStore('notebooks').put(n));
  pages.forEach((p) => t.objectStore('pages').put(p));
  await done(t);
}

export async function deletePage(id) {
  if (!db) { mem.pages.delete(id); return; }
  await req(tx('pages', 'readwrite').delete(id));
}

export async function deleteNotebook(id) {
  const pages = await listPages(id);
  if (!db) {
    mem.notebooks.delete(id);
    pages.forEach((p) => mem.pages.delete(p.id));
    return;
  }
  const t = db.transaction(['notebooks', 'pages'], 'readwrite');
  t.objectStore('notebooks').delete(id);
  pages.forEach((p) => t.objectStore('pages').delete(p.id));
  await done(t);
}
