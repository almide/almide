// Option module — TS runtime
// Option[T] = T | null (some(x) = x, none = null)

const __almd_option = {
  map<T, U>(o: T | null, f: (x: T) => U): U | null {
    return o !== null ? f(o) : null;
  },
  flat_map<T, U>(o: T | null, f: (x: T) => U | null): U | null {
    return o !== null ? f(o) : null;
  },
  flatten<T>(o: (T | null) | null): T | null {
    return o !== null ? o : null;
  },
  unwrap_or<T>(o: T | null, def: T): T {
    return o !== null ? o : def;
  },
  unwrap_or_else<T>(o: T | null, f: () => T): T {
    return o !== null ? o : f();
  },
  is_some<T>(o: T | null): boolean {
    return o !== null;
  },
  is_none<T>(o: T | null): boolean {
    return o === null;
  },
  to_result<T>(o: T | null, err: string): T {
    if (o !== null) return o;
    throw new Error(err);
  },
  filter<T>(o: T | null, f: (x: T) => boolean): T | null {
    return o !== null && f(o) ? o : null;
  },
  zip<T, U>(a: T | null, b: U | null): [T, U] | null {
    return a !== null && b !== null ? [a, b] : null;
  },
  or_else<T>(o: T | null, f: () => T | null): T | null {
    return o !== null ? o : f();
  },
  to_list<T>(o: T | null): T[] {
    return o !== null ? [o] : [];
  },
};
