const __almd_set = {
  insert(s, v) { const r = new Set(s); r.add(v); return r; },
  remove(s, v) { const r = new Set(s); r.delete(v); return r; },
  union(a, b) { const r = new Set(a); for (const v of b) r.add(v); return r; },
  intersection(a, b) { const r = new Set(); for (const v of a) { if (b.has(v)) r.add(v); } return r; },
  difference(a, b) { const r = new Set(); for (const v of a) { if (!b.has(v)) r.add(v); } return r; },
};
