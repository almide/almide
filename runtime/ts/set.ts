const __almd_set = {
  insert<T>(s: Set<T>, v: T): Set<T> { const r = new Set(s); r.add(v); return r; },
  remove<T>(s: Set<T>, v: T): Set<T> { const r = new Set(s); r.delete(v); return r; },
  union<T>(a: Set<T>, b: Set<T>): Set<T> { const r = new Set(a); for (const v of b) r.add(v); return r; },
  intersection<T>(a: Set<T>, b: Set<T>): Set<T> { const r = new Set<T>(); for (const v of a) { if (b.has(v)) r.add(v); } return r; },
  difference<T>(a: Set<T>, b: Set<T>): Set<T> { const r = new Set<T>(); for (const v of a) { if (!b.has(v)) r.add(v); } return r; },
};
