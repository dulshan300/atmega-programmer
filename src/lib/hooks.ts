import { useState, type Dispatch, type SetStateAction } from "react";

/**
 * Local, editable state that resets whenever `source` changes (compared with
 * Object.is). Uses React's "adjust state while rendering" pattern instead of an
 * effect, so there is no extra render with stale values.
 */
export function useSyncedState<T>(source: T): [T, Dispatch<SetStateAction<T>>] {
  const [state, setState] = useState(source);
  const [prev, setPrev] = useState(source);
  if (!Object.is(prev, source)) {
    setPrev(source);
    setState(source);
  }
  return [state, setState];
}

/**
 * Stable empty array for store selectors. A selector must not return a new
 * `[]` on each call (e.g. `s.x ?? []`): Zustand would see a changed value on
 * every read and re-render forever.
 */
export const EMPTY: readonly never[] = Object.freeze([]);
