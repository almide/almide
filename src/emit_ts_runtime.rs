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
  exec(cmd: string, args: string[]): string { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { throw new Error(new TextDecoder().decode(out.stderr)); } },
  exit(code: number): void { Deno.exit(code); },
  stdin_lines(): string[] { const buf = new Uint8Array(1024 * 1024); const n = Deno.stdin.readSync(buf); return n ? new TextDecoder().decode(buf.subarray(0, n)).split("\n").filter(l => l.length > 0) : []; },
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
function assert_eq<T>(a: T, b: T): void { if (!__deep_eq(a, b)) throw new Error(`assert_eq: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`); }
function assert_ne<T>(a: T, b: T): void { if (a === b) throw new Error(`assert_ne: ${a} === ${b}`); }
function assert(c: boolean): void { if (!c) throw new Error("assertion failed"); }
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
  exec(cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8" }); } catch (e) { throw new Error(e.stderr || e.message); } },
  exit(code) { process.exit(code); },
  stdin_lines() { return require("fs").readFileSync(0, "utf-8").split("\n").filter(l => l.length > 0); },
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
function assert_eq(a, b) { if (!__deep_eq(a, b)) throw new Error("assert_eq: " + JSON.stringify(a) + " !== " + JSON.stringify(b)); }
function assert_ne(a, b) { if (a === b) throw new Error("assert_ne: " + a + " === " + b); }
function assert(c) { if (!c) throw new Error("assertion failed"); }
function unwrap_or(x, d) { return x !== null ? x : d; }
function __concat(a, b) { return typeof a === "string" ? a + b : [...a, ...b]; }
function __assert_throws(fn, expectedMsg) {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
// ---- End Runtime ----
"#;
