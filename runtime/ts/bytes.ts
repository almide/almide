// bytes extern — TypeScript implementations
const __almd_bytes = {
  len(b: Uint8Array): number { return b.length; },
  get(b: Uint8Array, i: number): number | null { return (i >= 0 && i < b.length) ? b[i] : null; },
  get_or(b: Uint8Array, i: number, d: number): number { return (i >= 0 && i < b.length) ? b[i] : d; },
  slice(b: Uint8Array, start: number, end: number): Uint8Array { return b.slice(Math.max(0, start), Math.min(b.length, end)); },
  from_list(xs: number[]): Uint8Array { return new Uint8Array(xs.map(x => x & 0xFF)); },
  to_list(b: Uint8Array): number[] { return Array.from(b); },
  is_empty(b: Uint8Array): boolean { return b.length === 0; },
  concat(a: Uint8Array, b: Uint8Array): Uint8Array { const r = new Uint8Array(a.length + b.length); r.set(a); r.set(b, a.length); return r; },
  repeat(b: Uint8Array, n: number): Uint8Array { const r = new Uint8Array(b.length * Math.max(0, n)); for (let i = 0; i < n; i++) r.set(b, i * b.length); return r; },
  new_bytes(len: number): Uint8Array { return new Uint8Array(Math.max(0, len)); },
};
