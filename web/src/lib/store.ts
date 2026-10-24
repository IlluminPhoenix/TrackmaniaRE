//! Minimal observable store: one object of state, explicit patches, no magic.

export type Unsubscribe = () => void;

export interface Store<T extends object> {
  get(): T;
  /** Merges a patch and notifies subscribers. */
  set(patch: Partial<T>): void;
  subscribe(fn: (state: T) => void): Unsubscribe;
}

export function createStore<T extends object>(initial: T): Store<T> {
  let state = initial;
  const subscribers = new Set<(state: T) => void>();
  return {
    get: () => state,
    set(patch) {
      state = { ...state, ...patch };
      for (const fn of [...subscribers]) fn(state);
    },
    subscribe(fn) {
      subscribers.add(fn);
      return () => {
        subscribers.delete(fn);
      };
    },
  };
}
