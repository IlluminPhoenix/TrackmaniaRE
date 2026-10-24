//! Recent runs in IndexedDB: the raw file bytes, capped so the browser quota
//! is never in danger. Everything stays on the device.

export interface RecentRun {
  id: string;
  fileName: string;
  fileSize: number;
  savedAt: number;
  runCount: number;
}

const DB_NAME = "trackmaniare";
const DB_VERSION = 1;
const STORE = "replays";

export const LIMITS = {
  maxRuns: 10,
  maxBytes: 100 * 1024 * 1024,
};

let dbPromise: Promise<IDBDatabase | null> | null = null;

function openDb(): Promise<IDBDatabase | null> {
  dbPromise ??= new Promise((resolve) => {
    try {
      if (typeof indexedDB === "undefined") {
        resolve(null);
        return;
      }
      const request = indexedDB.open(DB_NAME, DB_VERSION);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE, { keyPath: "id" });
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
      request.onblocked = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
  return dbPromise;
}

function tx<T>(db: IDBDatabase, mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>): Promise<T | null> {
  return new Promise((resolve) => {
    try {
      const transaction = db.transaction(STORE, mode);
      const request = run(transaction.objectStore(STORE));
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
      transaction.onabort = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

export async function saveRecent(
  id: string,
  fileName: string,
  fileSize: number,
  bytes: Uint8Array,
  runCount: number,
): Promise<boolean> {
  const db = await openDb();
  if (!db) return false;
  const copy = bytes.slice();
  const record = {
    id,
    fileName,
    fileSize,
    savedAt: Date.now(),
    runCount,
    bytes: copy.buffer,
  };
  const ok = await tx(db, "readwrite", (store) => store.put(record) as IDBRequest<IDBValidKey>);
  await enforceLimits(db, id);
  return ok !== null;
}

async function enforceLimits(db: IDBDatabase, keepId: string): Promise<void> {
  const all = await listRecentRecords(db);
  if (!all) return;
  const sorted = all.sort((a, b) => b.savedAt - a.savedAt);
  let totalBytes = 0;
  const doomed: string[] = [];
  sorted.forEach((entry, index) => {
    totalBytes += entry.fileSize;
    if (index >= LIMITS.maxRuns || (totalBytes > LIMITS.maxBytes && entry.id !== keepId)) doomed.push(entry.id);
  });
  for (const id of doomed) await tx(db, "readwrite", (store) => store.delete(id) as IDBRequest<undefined>);
}

async function listRecentRecords(db: IDBDatabase): Promise<RecentRun[] | null> {
  return tx(db, "readonly", (store) => store.getAll() as IDBRequest<RecentRun[]>);
}

export async function listRecents(): Promise<RecentRun[]> {
  const db = await openDb();
  if (!db) return [];
  const all = await listRecentRecords(db);
  if (!all) return [];
  return all
    .map(({ id, fileName, fileSize, savedAt, runCount }) => ({ id, fileName, fileSize, savedAt, runCount }))
    .sort((a, b) => b.savedAt - a.savedAt);
}

export async function loadRecent(id: string): Promise<Uint8Array | null> {
  const db = await openDb();
  if (!db) return null;
  const record = await tx<{ bytes?: ArrayBuffer }>(db, "readonly", (store) => store.get(id));
  if (!record?.bytes) return null;
  return new Uint8Array(record.bytes);
}

export async function deleteRecent(id: string): Promise<void> {
  const db = await openDb();
  if (!db) return;
  await tx(db, "readwrite", (store) => store.delete(id) as IDBRequest<undefined>);
}

export async function clearRecents(): Promise<void> {
  const db = await openDb();
  if (!db) return;
  await tx(db, "readwrite", (store) => store.clear() as IDBRequest<undefined>);
}
