/**
 * Tiny IndexedDB wrapper used to persist FileSystemDirectoryHandle across reloads.
 * Handles are structured-cloneable, so the browser can store them.
 */

const DB_NAME = 'loom'
const STORE = 'handles'
const VERSION = 1

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, VERSION)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function withStore<T>(
  mode: IDBTransactionMode,
  fn: (store: IDBObjectStore) => IDBRequest<T> | IDBRequest<undefined>,
): Promise<T | undefined> {
  const db = await openDb()
  try {
    return await new Promise<T | undefined>((resolve, reject) => {
      const tx = db.transaction(STORE, mode)
      const req = fn(tx.objectStore(STORE))
      req.onsuccess = () => resolve(req.result as T | undefined)
      req.onerror = () => reject(req.error)
    })
  } finally {
    db.close()
  }
}

export const idbGet = <T>(key: string) =>
  withStore<T>('readonly', (s) => s.get(key) as IDBRequest<T>)

export const idbSet = (key: string, value: unknown) =>
  withStore<undefined>('readwrite', (s) => s.put(value, key) as unknown as IDBRequest<undefined>)

export const idbDel = (key: string) =>
  withStore<undefined>('readwrite', (s) => s.delete(key))
