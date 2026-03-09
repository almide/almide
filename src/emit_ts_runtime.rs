/// TypeScript runtime (Deno) for emitted code.
pub const RUNTIME: &str = r#"// ---- Almide Runtime ----
const __fs = {
  exists(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
  read_text(p: string): string { return Deno.readTextFileSync(p); },
  read_bytes(p: string): Uint8Array { return Deno.readFileSync(p); },
  write(p: string, s: string): void { Deno.writeTextFileSync(p, s); },
  write_bytes(p: string, b: Uint8Array | number[]): void { Deno.writeFileSync(p, b instanceof Uint8Array ? b : new Uint8Array(b)); },
  append(p: string, s: string): void { Deno.writeTextFileSync(p, Deno.readTextFileSync(p) + s); },
  mkdir_p(p: string): void { Deno.mkdirSync(p, { recursive: true }); },
  exists_qm_(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
  read_lines(p: string): string[] { return Deno.readTextFileSync(p).split("\n").filter(l => l.length > 0); },
  remove(p: string): void { Deno.removeSync(p); },
  list_dir(p: string): string[] { return [...Deno.readDirSync(p)].map(e => e.name).sort(); },
};
const __string = {
  trim(s: string): string { return s.trim(); },
  split(s: string, sep: string): string[] { return s.split(sep); },
  join(arr: string[], sep: string): string { return arr.join(sep); },
  len(s: string): number { return s.length; },
  pad_left(s: string, n: number, ch: string): string { return s.padStart(n, ch); },
  starts_with(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  slice(s: string, start: number, end?: number): string { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s: string): number[] { return Array.from(new TextEncoder().encode(s)); },
  contains(s: string, sub: string): boolean { return s.includes(sub); },
  starts_with_qm_(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  ends_with_qm_(s: string, suffix: string): boolean { return s.endsWith(suffix); },
  to_upper(s: string): string { return s.toUpperCase(); },
  to_lower(s: string): string { return s.toLowerCase(); },
  to_int(s: string): number { const n = parseInt(s, 10); if (isNaN(n)) throw new Error("invalid integer: " + s); return n; },
  replace(s: string, from: string, to: string): string { return s.split(from).join(to); },
  char_at(s: string, i: number): string | null { return i < s.length ? s[i] : null; },
  lines(s: string): string[] { return s.split("\n").filter(l => l.length > 0); },
  chars(s: string): string[] { return [...s]; },
  index_of(s: string, needle: string): number | null { const i = s.indexOf(needle); return i >= 0 ? i : null; },
  repeat(s: string, n: number): string { return s.repeat(n); },
  from_bytes(bytes: number[]): string { return new TextDecoder().decode(new Uint8Array(bytes)); },
  is_digit_qm_(s: string): boolean { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha_qm_(s: string): boolean { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric_qm_(s: string): boolean { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace_qm_(s: string): boolean { return s.length > 0 && /^\s+$/.test(s); },
};
const __list = {
  len<T>(xs: T[]): number { return xs.length; },
  get<T>(xs: T[], i: number): T | null { return i < xs.length ? xs[i] : null; },
  get_or<T>(xs: T[], i: number, d: T): T { return i < xs.length ? xs[i] : d; },
  sort<T>(xs: T[]): T[] { return [...xs].sort(); },
  reverse<T>(xs: T[]): T[] { return [...xs].reverse(); },
  any<T>(xs: T[], f: (x: T) => boolean): boolean { return xs.some(f); },
  all<T>(xs: T[], f: (x: T) => boolean): boolean { return xs.every(f); },
  contains<T>(xs: T[], x: T): boolean { return xs.includes(x); },
  each<T>(xs: T[], f: (x: T) => void): void { xs.forEach(f); },
  map<T, U>(xs: T[], f: (x: T) => U): U[] { return xs.map(f); },
  filter<T>(xs: T[], f: (x: T) => boolean): T[] { return xs.filter(f); },
  find<T>(xs: T[], f: (x: T) => boolean): T | null { return xs.find(f) ?? null; },
  fold<T, U>(xs: T[], init: U, f: (acc: U, x: T) => U): U { return xs.reduce(f, init); },
  enumerate<T>(xs: T[]): [number, T][] { return xs.map((x, i) => [i, x]); },
  zip<T, U>(a: T[], b: U[]): [T, U][] { return a.slice(0, Math.min(a.length, b.length)).map((x, i) => [x, b[i]]); },
  flatten<T>(xss: T[][]): T[] { return xss.flat(); },
  take<T>(xs: T[], n: number): T[] { return xs.slice(0, n); },
  drop<T>(xs: T[], n: number): T[] { return xs.slice(n); },
  sort_by<T>(xs: T[], f: (x: T) => any): T[] { return [...xs].sort((a, b) => { const ka = f(a), kb = f(b); return ka < kb ? -1 : ka > kb ? 1 : 0; }); },
  unique<T>(xs: T[]): T[] { const seen: T[] = []; return xs.filter(x => { if (seen.includes(x)) return false; seen.push(x); return true; }); },
};
const __map = {
  new_<K, V>(): Map<K, V> { return new Map(); },
  get<K, V>(m: Map<K, V>, k: K): V | null { return m.has(k) ? m.get(k)! : null; },
  get_or<K, V>(m: Map<K, V>, k: K, d: V): V { return m.has(k) ? m.get(k)! : d; },
  set<K, V>(m: Map<K, V>, k: K, v: V): Map<K, V> { const r = new Map(m); r.set(k, v); return r; },
  contains<K, V>(m: Map<K, V>, k: K): boolean { return m.has(k); },
  remove<K, V>(m: Map<K, V>, k: K): Map<K, V> { const r = new Map(m); r.delete(k); return r; },
  keys<K, V>(m: Map<K, V>): K[] { return [...m.keys()].sort() as any; },
  values<K, V>(m: Map<K, V>): V[] { return [...m.values()]; },
  len<K, V>(m: Map<K, V>): number { return m.size; },
  entries<K, V>(m: Map<K, V>): [K, V][] { return [...m.entries()]; },
  from_list<T, K, V>(xs: T[], f: (x: T) => [K, V]): Map<K, V> { const r = new Map<K, V>(); for (const x of xs) { const [k, v] = f(x); r.set(k, v); } return r; },
};
const __int = {
  to_hex(n: bigint): string { return (n >= 0n ? n : n + (1n << 64n)).toString(16); },
  to_string(n: number): string { return String(n); },
  band(a: number, b: number): number { return a & b; },
  bor(a: number, b: number): number { return a | b; },
  bxor(a: number, b: number): number { return a ^ b; },
  bshl(a: number, n: number): number { return a << n; },
  bshr(a: number, n: number): number { return a >>> n; },
  bnot(a: number): number { return ~a; },
  wrap_add(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return ((v >>> n) | (v << (bits - n))) & mask; },
  rotate_left(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return ((v << n) | (v >>> (bits - n))) & mask; },
  to_u32(a: number): number { return a >>> 0; },
  to_u8(a: number): number { return a & 0xFF; },
};
const __float = {
  to_string(n: number): string { return String(n); },
  to_int(n: number): number { return Math.trunc(n); },
  round(n: number): number { return Math.round(n); },
  floor(n: number): number { return Math.floor(n); },
  ceil(n: number): number { return Math.ceil(n); },
  abs(n: number): number { return Math.abs(n); },
  sqrt(n: number): number { return Math.sqrt(n); },
  parse(s: string): number { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n: number): number { return n; },
};
const __path = {
  join(base: string, child: string): string { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p: string): string | null { const b = __path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute_qm_(p: string): boolean { return p.startsWith("/"); },
};
const __json = {
  parse(text: string): any { return JSON.parse(text); },
  stringify(j: any): string { return JSON.stringify(j); },
  get(j: any, key: string): any | null { return (j && typeof j === "object" && !Array.isArray(j) && key in j) ? j[key] : null; },
  get_string(j: any, key: string): string | null { const v = __json.get(j, key); return typeof v === "string" ? v : null; },
  get_int(j: any, key: string): number | null { const v = __json.get(j, key); return typeof v === "number" ? v : null; },
  get_bool(j: any, key: string): boolean | null { const v = __json.get(j, key); return typeof v === "boolean" ? v : null; },
  get_array(j: any, key: string): any[] | null { const v = __json.get(j, key); return Array.isArray(v) ? v : null; },
  keys(j: any): string[] { return (j && typeof j === "object" && !Array.isArray(j)) ? Object.keys(j).sort() : []; },
  to_string(j: any): string | null { return typeof j === "string" ? j : null; },
  to_int(j: any): number | null { return typeof j === "number" ? j : null; },
  from_string(s: string): any { return s; },
  from_int(n: number): any { return n; },
  from_bool(b: boolean): any { return b; },
  null_(): any { return null; },
  array(items: any[]): any { return items; },
  from_map(m: any): any { if (m instanceof Map) { const o: any = {}; m.forEach((v: any, k: string) => { o[k] = v; }); return o; } return m; },
};
const __env = {
  unix_timestamp(): number { return Math.floor(Date.now() / 1000); },
  args(): string[] { return Deno.args; },
  get(name: string): string | null { const v = Deno.env.get(name); return v !== undefined ? v : null; },
  set(name: string, value: string): void { Deno.env.set(name, value); },
  cwd(): string { return Deno.cwd(); },
};
const __process = {
  exec(cmd: string, args: string[]): string { try { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { const msg = new TextDecoder().decode(out.stderr); throw new Error(msg || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
  exit(code: number): void { Deno.exit(code); },
  stdin_lines(): string[] { const buf = new Uint8Array(1024 * 1024); const n = Deno.stdin.readSync(buf); return n ? new TextDecoder().decode(buf.subarray(0, n)).split("\n").filter(l => l.length > 0) : []; },
};
const __math = {
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  abs(n: number): number { return Math.abs(n); },
  pow(base: number, exp: number): number { return Math.pow(base, exp); },
  pi(): number { return Math.PI; },
  e(): number { return Math.E; },
  sin(x: number): number { return Math.sin(x); },
  cos(x: number): number { return Math.cos(x); },
  tan(x: number): number { return Math.tan(x); },
  log(x: number): number { return Math.log(x); },
  exp(x: number): number { return Math.exp(x); },
  sqrt(x: number): number { return Math.sqrt(x); },
};
const __random = {
  int(min: number, max: number): number { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float(): number { return Math.random(); },
  choice<T>(xs: T[]): T | null { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle<T>(xs: T[]): T[] { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
const __regex = {
  match_qm_(pat: string, s: string): boolean { return new RegExp(pat).test(s); },
  full_match_qm_(pat: string, s: string): boolean { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat: string, s: string): string | null { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat: string, s: string): string[] { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat), rep); },
  split(pat: string, s: string): string[] { return s.split(new RegExp(pat)); },
  captures(pat: string, s: string): string[] | null { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
const __io = {
  read_line(): string { return prompt("") ?? ""; },
  print(s: string): void { const buf = new TextEncoder().encode(s); Deno.stdout.writeSync(buf); },
  read_all(): string { const d = new TextDecoder(); let r = ""; const buf = new Uint8Array(4096); let n: number | null; while ((n = Deno.stdin.readSync(buf)) !== null && n > 0) { r += d.decode(buf.subarray(0, n)); } return r; },
};
const __time = {
  now(): number { return Math.floor(Date.now() / 1000); },
  millis(): number { return Date.now(); },
  sleep(ms: number): void { /* Deno */ if (typeof Deno !== "undefined") { const end = Date.now() + ms; while (Date.now() < end) {} } },
  _parts(ts: number): [number, number, number, number, number, number] { const d = new Date(ts * 1000); return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()]; },
  year(ts: number): number { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts: number): number { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts: number): number { return new Date(ts * 1000).getUTCDate(); },
  hour(ts: number): number { return new Date(ts * 1000).getUTCHours(); },
  minute(ts: number): number { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts: number): number { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts: number): number { const d = new Date(ts * 1000).getUTCDay(); return d === 0 ? 6 : d - 1; },
  to_iso(ts: number): string { const [y, m, d, h, mi, s] = __time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y: number, m: number, d: number, h: number, min: number, s: number): number { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
const __http = {
  async serve(port: number, handler: (req: any) => any): Promise<void> { const server = Deno.serve({ port }, async (request: Request) => { const url = new URL(request.url); const method = request.method; const path = url.pathname; const body = method === "POST" || method === "PUT" ? await request.text() : ""; const headers: Record<string, string> = {}; request.headers.forEach((v: string, k: string) => { headers[k] = v; }); const req = { method, path, body, headers }; const res = handler(req); return new Response(res.body, { status: res.status, headers: res.headers || {} }); }); },
  response(status: number, body: string): any { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status: number, body: string): any { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status: number, body: string, headers: any): any { const h: Record<string, string> = {}; if (headers instanceof Map) { headers.forEach((v: string, k: string) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url: string): Promise<string> { const r = await fetch(url); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async post(url: string, body: string): Promise<string> { const r = await fetch(url, { method: "POST", body, headers: { "content-type": "application/json" } }); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
};
function __bigop(op: string, a: any, b: any): any {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    switch(op) {
      case "^": return ba ^ bb;
      case "*": return ba * bb;
      case "%": return ba % bb;
      case "+": return ba + bb;
      case "-": return ba - bb;
      default: return ba;
    }
  }
  switch(op) {
    case "^": return a ^ b; case "*": return a * b; case "%": return a % b;
    case "+": return a + b; case "-": return a - b; default: return a;
  }
}
function __div(a: any, b: any): any {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    return ba / bb;
  }
  const r = a / b;
  return (Number.isInteger(a) && Number.isInteger(b)) ? Math.trunc(r) : r;
}
function println(s: string): void { console.log(s); }
function eprintln(s: string): void { console.error(s); }
function __deep_eq(a: any, b: any): boolean {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) { if (!__deep_eq(a[k], b[k])) return false; }
    return true;
  }
  return false;
}
function assert_eq<T>(a: T, b: T, msg?: string): void { if (!__deep_eq(a, b)) { const m = msg ? msg + ": " : ""; throw new Error(`${m}assert_eq failed\n  expected: ${JSON.stringify(b)}\n       got: ${JSON.stringify(a)}`); } }
function assert_ne<T>(a: T, b: T, msg?: string): void { if (__deep_eq(a, b)) { const m = msg ? msg + ": " : ""; throw new Error(`${m}assert_ne failed\n  both are: ${JSON.stringify(a)}`); } }
function assert(c: boolean, msg?: string): void { if (!c) throw new Error(msg ? msg : "assertion failed"); }
function unwrap_or<T>(x: T | null, d: T): T { return x !== null ? x : d; }
function __concat(a: any, b: any): any { return typeof a === "string" ? a + b : [...a, ...b]; }
function __assert_throws(fn: () => any, expectedMsg: string): void {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
// ---- End Runtime ----
"#;

/// JavaScript runtime (Node.js) for emitted code.
pub const RUNTIME_JS: &str = r#"// ---- Almide Runtime (JS) ----
const __fs = {
  exists(p) { const fs = require("fs"); try { fs.statSync(p); return true; } catch { return false; } },
  read_text(p) { return require("fs").readFileSync(p, "utf-8"); },
  read_bytes(p) { return Array.from(require("fs").readFileSync(p)); },
  write(p, s) { require("fs").writeFileSync(p, s); },
  write_bytes(p, b) { require("fs").writeFileSync(p, Buffer.from(b)); },
  append(p, s) { require("fs").appendFileSync(p, s); },
  mkdir_p(p) { require("fs").mkdirSync(p, { recursive: true }); },
  exists_qm_(p) { const fs = require("fs"); try { fs.statSync(p); return true; } catch { return false; } },
  read_lines(p) { return require("fs").readFileSync(p, "utf-8").split("\n").filter(l => l.length > 0); },
  remove(p) { require("fs").unlinkSync(p); },
  list_dir(p) { return require("fs").readdirSync(p).sort(); },
};
const __string = {
  trim(s) { return s.trim(); },
  split(s, sep) { return s.split(sep); },
  join(arr, sep) { return arr.join(sep); },
  len(s) { return s.length; },
  pad_left(s, n, ch) { return s.padStart(n, ch); },
  starts_with(s, prefix) { return s.startsWith(prefix); },
  slice(s, start, end) { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s) { return Array.from(new TextEncoder().encode(s)); },
  contains(s, sub) { return s.includes(sub); },
  starts_with_qm_(s, prefix) { return s.startsWith(prefix); },
  ends_with_qm_(s, suffix) { return s.endsWith(suffix); },
  to_upper(s) { return s.toUpperCase(); },
  to_lower(s) { return s.toLowerCase(); },
  to_int(s) { const n = parseInt(s, 10); if (isNaN(n)) throw new Error("invalid integer: " + s); return n; },
  replace(s, from, to) { return s.split(from).join(to); },
  char_at(s, i) { return i < s.length ? s[i] : null; },
  lines(s) { return s.split("\n").filter(l => l.length > 0); },
  chars(s) { return [...s]; },
  index_of(s, needle) { const i = s.indexOf(needle); return i >= 0 ? i : null; },
  repeat(s, n) { return s.repeat(n); },
  from_bytes(bytes) { return new TextDecoder().decode(new Uint8Array(bytes)); },
  is_digit_qm_(s) { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha_qm_(s) { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric_qm_(s) { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace_qm_(s) { return s.length > 0 && /^\s+$/.test(s); },
};
const __list = {
  len(xs) { return xs.length; },
  get(xs, i) { return i < xs.length ? xs[i] : null; },
  get_or(xs, i, d) { return i < xs.length ? xs[i] : d; },
  sort(xs) { return [...xs].sort(); },
  reverse(xs) { return [...xs].reverse(); },
  any(xs, f) { return xs.some(f); },
  all(xs, f) { return xs.every(f); },
  contains(xs, x) { return xs.includes(x); },
  each(xs, f) { xs.forEach(f); },
  map(xs, f) { return xs.map(f); },
  filter(xs, f) { return xs.filter(f); },
  find(xs, f) { return xs.find(f) ?? null; },
  fold(xs, init, f) { return xs.reduce(f, init); },
  enumerate(xs) { return xs.map((x, i) => [i, x]); },
  zip(a, b) { return a.slice(0, Math.min(a.length, b.length)).map((x, i) => [x, b[i]]); },
  flatten(xss) { return xss.flat(); },
  take(xs, n) { return xs.slice(0, n); },
  drop(xs, n) { return xs.slice(n); },
  sort_by(xs, f) { return [...xs].sort((a, b) => { const ka = f(a), kb = f(b); return ka < kb ? -1 : ka > kb ? 1 : 0; }); },
  unique(xs) { const seen = []; return xs.filter(x => { if (seen.includes(x)) return false; seen.push(x); return true; }); },
};
const __map = {
  new_() { return new Map(); },
  get(m, k) { return m.has(k) ? m.get(k) : null; },
  get_or(m, k, d) { return m.has(k) ? m.get(k) : d; },
  set(m, k, v) { const r = new Map(m); r.set(k, v); return r; },
  contains(m, k) { return m.has(k); },
  remove(m, k) { const r = new Map(m); r.delete(k); return r; },
  keys(m) { return [...m.keys()].sort(); },
  values(m) { return [...m.values()]; },
  len(m) { return m.size; },
  entries(m) { return [...m.entries()]; },
  from_list(xs, f) { const r = new Map(); for (const x of xs) { const [k, v] = f(x); r.set(k, v); } return r; },
};
const __int = {
  to_hex(n) { return (typeof n === "bigint" ? (n >= 0n ? n : n + (1n << 64n)).toString(16) : n.toString(16)); },
  to_string(n) { return String(n); },
  band(a, b) { return a & b; },
  bor(a, b) { return a | b; },
  bxor(a, b) { return a ^ b; },
  bshl(a, n) { return a << n; },
  bshr(a, n) { return a >>> n; },
  bnot(a) { return ~a; },
  wrap_add(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return ((v >>> n) | (v << (bits - n))) & mask; },
  rotate_left(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return ((v << n) | (v >>> (bits - n))) & mask; },
  to_u32(a) { return a >>> 0; },
  to_u8(a) { return a & 0xFF; },
};
const __float = {
  to_string(n) { return String(n); },
  to_int(n) { return Math.trunc(n); },
  round(n) { return Math.round(n); },
  floor(n) { return Math.floor(n); },
  ceil(n) { return Math.ceil(n); },
  abs(n) { return Math.abs(n); },
  sqrt(n) { return Math.sqrt(n); },
  parse(s) { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n) { return n; },
};
const __path = {
  join(base, child) { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p) { const b = __path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute_qm_(p) { return p.startsWith("/"); },
};
const __json = {
  parse(text) { return JSON.parse(text); },
  stringify(j) { return JSON.stringify(j); },
  get(j, key) { return (j && typeof j === "object" && !Array.isArray(j) && key in j) ? j[key] : null; },
  get_string(j, key) { const v = __json.get(j, key); return typeof v === "string" ? v : null; },
  get_int(j, key) { const v = __json.get(j, key); return typeof v === "number" ? v : null; },
  get_bool(j, key) { const v = __json.get(j, key); return typeof v === "boolean" ? v : null; },
  get_array(j, key) { const v = __json.get(j, key); return Array.isArray(v) ? v : null; },
  keys(j) { return (j && typeof j === "object" && !Array.isArray(j)) ? Object.keys(j).sort() : []; },
  to_string(j) { return typeof j === "string" ? j : null; },
  to_int(j) { return typeof j === "number" ? j : null; },
  from_string(s) { return s; },
  from_int(n) { return n; },
  from_bool(b) { return b; },
  null_() { return null; },
  array(items) { return items; },
  from_map(m) { if (m instanceof Map) { const o = {}; m.forEach((v, k) => { o[k] = v; }); return o; } return m; },
};
const __env = {
  unix_timestamp() { return Math.floor(Date.now() / 1000); },
  args() { return process.argv.slice(2); },
  get(name) { const v = process.env[name]; return v !== undefined ? v : null; },
  set(name, value) { process.env[name] = value; },
  cwd() { return process.cwd(); },
};
const __process = {
  exec(cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8" }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exit(code) { process.exit(code); },
  stdin_lines() { return require("fs").readFileSync(0, "utf-8").split("\n").filter(l => l.length > 0); },
};
const __math = {
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  abs(n) { return Math.abs(n); },
  pow(base, exp) { return Math.pow(base, exp); },
  pi() { return Math.PI; },
  e() { return Math.E; },
  sin(x) { return Math.sin(x); },
  cos(x) { return Math.cos(x); },
  tan(x) { return Math.tan(x); },
  log(x) { return Math.log(x); },
  exp(x) { return Math.exp(x); },
  sqrt(x) { return Math.sqrt(x); },
};
const __random = {
  int(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float() { return Math.random(); },
  choice(xs) { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle(xs) { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
const __regex = {
  match_qm_(pat, s) { return new RegExp(pat).test(s); },
  full_match_qm_(pat, s) { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat, s) { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat, s) { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat, s, rep) { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat, s, rep) { return s.replace(new RegExp(pat), rep); },
  split(pat, s) { return s.split(new RegExp(pat)); },
  captures(pat, s) { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
const __io = {
  read_line() { const buf = Buffer.alloc(1024); let s = ""; while (true) { const n = require("fs").readSync(0, buf, 0, 1, null); if (n === 0) break; const ch = buf.toString("utf-8", 0, n); s += ch; if (ch === "\n") break; } return s.replace(/\r?\n$/, ""); },
  print(s) { process.stdout.write(s); },
  read_all() { return require("fs").readFileSync(0, "utf-8"); },
};
const __time = {
  now() { return Math.floor(Date.now() / 1000); },
  millis() { return Date.now(); },
  sleep(ms) { const end = Date.now() + ms; while (Date.now() < end) {} },
  _parts(ts) { const d = new Date(ts * 1000); return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()]; },
  year(ts) { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts) { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts) { return new Date(ts * 1000).getUTCDate(); },
  hour(ts) { return new Date(ts * 1000).getUTCHours(); },
  minute(ts) { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts) { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts) { const d = new Date(ts * 1000).getUTCDay(); return d === 0 ? 6 : d - 1; },
  to_iso(ts) { const [y, m, d, h, mi, s] = __time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y, m, d, h, min, s) { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
const __http = {
  async serve(port, handler) { const http = require("http"); const server = http.createServer(async (req, res) => { let body = ""; req.on("data", (c) => { body += c; }); req.on("end", () => { const r = handler({ method: req.method, path: req.url, body, headers: req.headers || {} }); const headers = r.headers || {}; res.writeHead(r.status, headers); res.end(r.body); }); }); server.listen(port); },
  response(status, body) { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status, body) { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status, body, headers) { const h = {}; if (headers instanceof Map) { headers.forEach((v, k) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url) { return new Promise((resolve, reject) => { const m = url.startsWith("https") ? require("https") : require("http"); m.get(url, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }).on("error", reject); }); },
  async post(url, body) { return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); const req = m.request({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, method: "POST", headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) } }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }); req.on("error", reject); req.write(body); req.end(); }); },
};
function __bigop(op, a, b) {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    switch(op) {
      case "^": return ba ^ bb;
      case "*": return ba * bb;
      case "%": return ba % bb;
      case "+": return ba + bb;
      case "-": return ba - bb;
      default: return ba;
    }
  }
  switch(op) {
    case "^": return a ^ b; case "*": return a * b; case "%": return a % b;
    case "+": return a + b; case "-": return a - b; default: return a;
  }
}
function __div(a, b) {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    return ba / bb;
  }
  const r = a / b;
  return (Number.isInteger(a) && Number.isInteger(b)) ? Math.trunc(r) : r;
}
function println(s) { console.log(s); }
function eprintln(s) { console.error(s); }
function __deep_eq(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) { if (!__deep_eq(a[k], b[k])) return false; }
    return true;
  }
  return false;
}
function assert_eq(a, b, msg) { if (!__deep_eq(a, b)) { var m = msg ? msg + ": " : ""; throw new Error(m + "assert_eq failed\n  expected: " + JSON.stringify(b) + "\n       got: " + JSON.stringify(a)); } }
function assert_ne(a, b, msg) { if (__deep_eq(a, b)) { var m = msg ? msg + ": " : ""; throw new Error(m + "assert_ne failed\n  both are: " + JSON.stringify(a)); } }
function assert(c, msg) { if (!c) throw new Error(msg ? msg : "assertion failed"); }
function unwrap_or(x, d) { return x !== null ? x : d; }
function __concat(a, b) { return typeof a === "string" ? a + b : [...a, ...b]; }
function __assert_throws(fn, expectedMsg) {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
// ---- End Runtime ----
"#;
