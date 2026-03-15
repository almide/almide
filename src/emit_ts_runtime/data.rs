/// Data modules: json, result, error, helpers, datetime, testing.

// ──────────────────────────────── json ────────────────────────────────

pub(super) const MOD_JSON_TS: &str = r#"const __almd_json = {
  parse(text: string): any { return JSON.parse(text); },
  stringify(j: any): string { return JSON.stringify(j); },
  get(j: any, key: string): any | null { return (j && typeof j === "object" && !Array.isArray(j) && key in j) ? j[key] : null; },
  get_string(j: any, key: string): string | null { const v = __almd_json.get(j, key); return typeof v === "string" ? v : null; },
  get_int(j: any, key: string): number | null { const v = __almd_json.get(j, key); return typeof v === "number" ? v : null; },
  get_bool(j: any, key: string): boolean | null { const v = __almd_json.get(j, key); return typeof v === "boolean" ? v : null; },
  get_array(j: any, key: string): any[] | null { const v = __almd_json.get(j, key); return Array.isArray(v) ? v : null; },
  keys(j: any): string[] { return (j && typeof j === "object" && !Array.isArray(j)) ? Object.keys(j).sort() : []; },
  to_string(j: any): string | null { return typeof j === "string" ? j : null; },
  to_int(j: any): number | null { return typeof j === "number" ? j : null; },
  from_string(s: string): any { return s; },
  from_int(n: number): any { return n; },
  from_bool(b: boolean): any { return b; },
  null_(): any { return null; },
  array(items: any[]): any { return items; },
  from_map(m: any): any { if (m instanceof Map) { const o: any = {}; m.forEach((v: any, k: string) => { o[k] = v; }); return o; } return m; },
  get_float(j: any, key: string): number | null { const v = __almd_json.get(j, key); return typeof v === "number" ? v : null; },
  from_float(n: number): any { return n; },
  stringify_pretty(j: any): string { return JSON.stringify(j, null, 2); },
  object(entries: [string, any][]): any { const o: any = {}; for (const [k, v] of entries) { o[k] = v; } return o; },
  as_float(j: any): number | null { return typeof j === "number" ? j : null; },
  as_bool(j: any): boolean | null { return typeof j === "boolean" ? j : null; },
  as_array(j: any): any[] | null { return Array.isArray(j) ? j : null; },
  path_root(): any { return { type: "root" }; },
  path_field(parent: any, name: string): any { return { type: "field", parent, name }; },
  path_index(parent: any, i: number): any { return { type: "index", parent, i }; },
  _path_segs(p: any): any[] { const s: any[] = []; let c = p; while (c.type !== "root") { s.push(c); c = c.parent; } s.reverse(); return s; },
  get_path(j: any, path: any): any | null { const segs = __almd_json._path_segs(path); let cur = j; for (const seg of segs) { if (seg.type === "field") { if (cur == null || typeof cur !== "object" || Array.isArray(cur)) return null; cur = cur[seg.name]; if (cur === undefined) return null; } else { if (!Array.isArray(cur) || seg.i < 0 || seg.i >= cur.length) return null; cur = cur[seg.i]; } } return cur; },
  set_path(j: any, path: any, value: any): any { const segs = __almd_json._path_segs(path); if (segs.length === 0) return value; return __almd_json._set_at(j, segs, 0, value, false); },
  upsert_path(j: any, path: any, value: any): any { const segs = __almd_json._path_segs(path); if (segs.length === 0) return value; return __almd_json._set_at(j, segs, 0, value, true); },
  remove_path(j: any, path: any): any { const segs = __almd_json._path_segs(path); if (segs.length === 0) return null; return __almd_json._remove_at(j, segs, 0); },
  _set_at(j: any, segs: any[], idx: number, value: any, upsert: boolean): any { if (idx >= segs.length) return value; const seg = segs[idx]; if (seg.type === "field") { let m = (j && typeof j === "object" && !Array.isArray(j)) ? { ...j } : (upsert ? {} : (() => { throw new Error(`path error: expected object at field "${seg.name}"`); })()); if (idx + 1 === segs.length) { if (!upsert && !(seg.name in m)) throw new Error(`path error: field "${seg.name}" does not exist`); m[seg.name] = value; } else { m[seg.name] = __almd_json._set_at(m[seg.name] ?? null, segs, idx + 1, value, upsert); } return m; } else { if (!Array.isArray(j)) throw new Error(`path error: expected array at index ${seg.i}`); if (seg.i < 0 || seg.i >= j.length) throw new Error(`path error: index ${seg.i} out of bounds`); const a = [...j]; if (idx + 1 === segs.length) { a[seg.i] = value; } else { a[seg.i] = __almd_json._set_at(a[seg.i], segs, idx + 1, value, upsert); } return a; } },
  _remove_at(j: any, segs: any[], idx: number): any { if (idx >= segs.length) return j; const seg = segs[idx]; if (seg.type === "field") { if (!j || typeof j !== "object" || Array.isArray(j)) return j; const m = { ...j }; if (idx + 1 === segs.length) { delete m[seg.name]; } else if (seg.name in m) { m[seg.name] = __almd_json._remove_at(m[seg.name], segs, idx + 1); } return m; } else { if (!Array.isArray(j) || seg.i < 0 || seg.i >= j.length) return j; const a = [...j]; if (idx + 1 === segs.length) { a.splice(seg.i, 1); } else { a[seg.i] = __almd_json._remove_at(a[seg.i], segs, idx + 1); } return a; } },
};
"#;

pub(super) const MOD_JSON_JS: &str = r#"const __almd_json = {
  parse(text) { return JSON.parse(text); },
  stringify(j) { return JSON.stringify(j); },
  get(j, key) { return (j && typeof j === "object" && !Array.isArray(j) && key in j) ? j[key] : null; },
  get_string(j, key) { const v = __almd_json.get(j, key); return typeof v === "string" ? v : null; },
  get_int(j, key) { const v = __almd_json.get(j, key); return typeof v === "number" ? v : null; },
  get_bool(j, key) { const v = __almd_json.get(j, key); return typeof v === "boolean" ? v : null; },
  get_array(j, key) { const v = __almd_json.get(j, key); return Array.isArray(v) ? v : null; },
  keys(j) { return (j && typeof j === "object" && !Array.isArray(j)) ? Object.keys(j).sort() : []; },
  to_string(j) { return typeof j === "string" ? j : null; },
  to_int(j) { return typeof j === "number" ? j : null; },
  from_string(s) { return s; },
  from_int(n) { return n; },
  from_bool(b) { return b; },
  null_() { return null; },
  array(items) { return items; },
  from_map(m) { if (m instanceof Map) { const o = {}; m.forEach((v, k) => { o[k] = v; }); return o; } return m; },
  get_float(j, key) { const v = __almd_json.get(j, key); return typeof v === "number" ? v : null; },
  from_float(n) { return n; },
  stringify_pretty(j) { return JSON.stringify(j, null, 2); },
  object(entries) { const o = {}; for (const [k, v] of entries) { o[k] = v; } return o; },
  as_float(j) { return typeof j === "number" ? j : null; },
  as_bool(j) { return typeof j === "boolean" ? j : null; },
  as_array(j) { return Array.isArray(j) ? j : null; },
  path_root() { return { type: "root" }; },
  path_field(parent, name) { return { type: "field", parent, name }; },
  path_index(parent, i) { return { type: "index", parent, i }; },
  _path_segs(p) { const s = []; let c = p; while (c.type !== "root") { s.push(c); c = c.parent; } s.reverse(); return s; },
  get_path(j, path) { const segs = __almd_json._path_segs(path); let cur = j; for (const seg of segs) { if (seg.type === "field") { if (cur == null || typeof cur !== "object" || Array.isArray(cur)) return null; cur = cur[seg.name]; if (cur === undefined) return null; } else { if (!Array.isArray(cur) || seg.i < 0 || seg.i >= cur.length) return null; cur = cur[seg.i]; } } return cur; },
  set_path(j, path, value) { const segs = __almd_json._path_segs(path); if (segs.length === 0) return value; return __almd_json._set_at(j, segs, 0, value, false); },
  upsert_path(j, path, value) { const segs = __almd_json._path_segs(path); if (segs.length === 0) return value; return __almd_json._set_at(j, segs, 0, value, true); },
  remove_path(j, path) { const segs = __almd_json._path_segs(path); if (segs.length === 0) return null; return __almd_json._remove_at(j, segs, 0); },
  _set_at(j, segs, idx, value, upsert) { if (idx >= segs.length) return value; const seg = segs[idx]; if (seg.type === "field") { let m = (j && typeof j === "object" && !Array.isArray(j)) ? { ...j } : (upsert ? {} : (() => { throw new Error(`path error: expected object at field "${seg.name}"`); })()); if (idx + 1 === segs.length) { if (!upsert && !(seg.name in m)) throw new Error(`path error: field "${seg.name}" does not exist`); m[seg.name] = value; } else { m[seg.name] = __almd_json._set_at(m[seg.name] ?? null, segs, idx + 1, value, upsert); } return m; } else { if (!Array.isArray(j)) throw new Error(`path error: expected array at index ${seg.i}`); if (seg.i < 0 || seg.i >= j.length) throw new Error(`path error: index ${seg.i} out of bounds`); const a = [...j]; if (idx + 1 === segs.length) { a[seg.i] = value; } else { a[seg.i] = __almd_json._set_at(a[seg.i], segs, idx + 1, value, upsert); } return a; } },
  _remove_at(j, segs, idx) { if (idx >= segs.length) return j; const seg = segs[idx]; if (seg.type === "field") { if (!j || typeof j !== "object" || Array.isArray(j)) return j; const m = { ...j }; if (idx + 1 === segs.length) { delete m[seg.name]; } else if (seg.name in m) { m[seg.name] = __almd_json._remove_at(m[seg.name], segs, idx + 1); } return m; } else { if (!Array.isArray(j) || seg.i < 0 || seg.i >= j.length) return j; const a = [...j]; if (idx + 1 === segs.length) { a.splice(seg.i, 1); } else { a[seg.i] = __almd_json._remove_at(a[seg.i], segs, idx + 1); } return a; } },
};
"#;

// ──────────────────────────────── result ────────────────────────────────

// Result erasure: ok(x) -> x, err(e) -> throw. These functions only see ok values
// at runtime. Functions like unwrap_or/is_ok? are identity/constant in TS because
// err values are already thrown before reaching these calls.
pub(super) const MOD_RESULT_TS: &str = r#"function __almd_result_unwrap_or<A>(v: A, _d: A): A { return v; }
function __almd_result_unwrap_or_else<A>(v: A, _f: (e: any) => A): A { return v; }
function __almd_result_is_ok(_v: any): boolean { return true; }
function __almd_result_is_err(_v: any): boolean { return false; }
function __almd_result_to_option<A>(v: A): A | null { return v; }
function __almd_result_to_err_option(_v: any): any { return null; }
"#;

pub(super) const MOD_RESULT_JS: &str = r#"function __almd_result_unwrap_or(v, _d) { return v; }
function __almd_result_unwrap_or_else(v, _f) { return v; }
function __almd_result_is_ok(_v) { return true; }
function __almd_result_is_err(_v) { return false; }
function __almd_result_to_option(v) { return v; }
function __almd_result_to_err_option(_v) { return null; }
"#;

// ──────────────────────────────── error ────────────────────────────────

// Error module: context/message are mostly no-ops in TS due to result erasure.
// Errors are thrown as exceptions, so only `chain` does real work.
pub(super) const MOD_ERROR_TS: &str = r#"const __almd_error = {
  chain(outer: string, cause: string): string { return outer + ": " + cause; },
  message(_r: any): string { return ""; },
};
"#;

pub(super) const MOD_ERROR_JS: &str = r#"const __almd_error = {
  chain(outer, cause) { return outer + ": " + cause; },
  message(_r) { return ""; },
};
"#;

// ──────────────────────────────── helpers ────────────────────────────────

pub(super) const HELPERS_TS: &str = r#"function __bigop(op: string, a: any, b: any): any {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    let r: bigint;
    switch(op) {
      case "^": r = ba ^ bb; break;
      case "*": r = ba * bb; break;
      case "%": r = ba % bb; break;
      case "+": r = ba + bb; break;
      case "-": r = ba - bb; break;
      default: r = ba;
    }
    return BigInt.asIntN(64, r);
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
    return BigInt.asIntN(64, ba / bb);
  }
  const r = a / b;
  return (Number.isInteger(a) && Number.isInteger(b)) ? Math.trunc(r) : r;
}
function println(s: string): void { console.log(s); }
function eprintln(s: string): void { console.error(s); }
class __Err { constructor(public message: string, public value?: any) {} }
function __deep_eq(a: any, b: any): boolean {
  if (a === b) return true;
  if (a instanceof __Err && b instanceof __Err) return __deep_eq(a.value, b.value);
  if (a instanceof __Err || b instanceof __Err) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a instanceof Map && b instanceof Map) {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) { if (!b.has(k) || !__deep_eq(v, b.get(k))) return false; }
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
function __throw(msg: string): never { throw new Error(msg); }
type Result<T, E> = { ok: true, value: T } | { ok: false, error: E };
function __unwrap<T, E>(r: Result<T, E>): T { if (r.ok) return r.value; throw new Error(String(r.error)); }
function __assert_throws(fn: () => any, expectedMsg: string): void {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
"#;

pub(super) const HELPERS_JS: &str = r#"function __bigop(op, a, b) {
  if (typeof a === "bigint" || typeof b === "bigint") {
    const ba = typeof a === "bigint" ? a : BigInt(a);
    const bb = typeof b === "bigint" ? b : BigInt(b);
    var r;
    switch(op) {
      case "^": r = ba ^ bb; break;
      case "*": r = ba * bb; break;
      case "%": r = ba % bb; break;
      case "+": r = ba + bb; break;
      case "-": r = ba - bb; break;
      default: r = ba;
    }
    return BigInt.asIntN(64, r);
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
    return BigInt.asIntN(64, ba / bb);
  }
  const r = a / b;
  return (Number.isInteger(a) && Number.isInteger(b)) ? Math.trunc(r) : r;
}
function println(s) { console.log(s); }
function eprintln(s) { console.error(s); }
class __Err { constructor(message, value) { this.message = message; this.value = value !== undefined ? value : message; } }
function __deep_eq(a, b) {
  if (a === b) return true;
  if (a instanceof __Err && b instanceof __Err) return __deep_eq(a.value, b.value);
  if (a instanceof __Err || b instanceof __Err) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) { if (!__deep_eq(a[i], b[i])) return false; }
    return true;
  }
  if (a instanceof Map && b instanceof Map) {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) { if (!b.has(k) || !__deep_eq(v, b.get(k))) return false; }
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
function __throw(msg) { throw new Error(msg); }
function __unwrap(r) { if (r.ok) return r.value; throw new Error(String(r.error)); }
function __assert_throws(fn, expectedMsg) {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
"#;

// ──────────────────────────────── datetime ────────────────────────────────

pub(super) const MOD_DATETIME_TS: &str = r#"const __almd_datetime = {
  now(): number { return Math.floor(Date.now() / 1000); },
  from_parts(y: number, m: number, d: number, h: number, min: number, s: number): number { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
  parse_iso(s: string): number { const d = new Date(s); if (isNaN(d.getTime())) throw new Error(`invalid ISO 8601 datetime: ${s}`); return Math.floor(d.getTime() / 1000); },
  format(ts: number, pattern: string): string { const d = new Date(ts * 1000); const pad = (n: number, w: number = 2) => String(n).padStart(w, "0"); const Y = pad(d.getUTCFullYear(), 4); const m = pad(d.getUTCMonth() + 1); const dd = pad(d.getUTCDate()); const H = pad(d.getUTCHours()); const M = pad(d.getUTCMinutes()); const S = pad(d.getUTCSeconds()); const days = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"]; const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]; const wd = d.getUTCDay(); const a = days[wd === 0 ? 6 : wd - 1]; const b = months[d.getUTCMonth()]; return pattern.replace("%F", `${Y}-${m}-${dd}`).replace("%T", `${H}:${M}:${S}`).replace("%Y", Y).replace("%m", m).replace("%d", dd).replace("%H", H).replace("%M", M).replace("%S", S).replace("%a", a).replace("%b", b); },
  to_iso(ts: number): string { const d = new Date(ts * 1000); const pad = (n: number, w: number = 2) => String(n).padStart(w, "0"); return `${pad(d.getUTCFullYear(), 4)}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}Z`; },
  year(ts: number): number { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts: number): number { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts: number): number { return new Date(ts * 1000).getUTCDate(); },
  hour(ts: number): number { return new Date(ts * 1000).getUTCHours(); },
  minute(ts: number): number { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts: number): number { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts: number): string { const days = ["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]; return days[new Date(ts * 1000).getUTCDay()]; },
};
"#;

pub(super) const MOD_DATETIME_JS: &str = r#"const __almd_datetime = {
  now() { return Math.floor(Date.now() / 1000); },
  from_parts(y, m, d, h, min, s) { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
  parse_iso(s) { const d = new Date(s); if (isNaN(d.getTime())) throw new Error(`invalid ISO 8601 datetime: ${s}`); return Math.floor(d.getTime() / 1000); },
  format(ts, pattern) { const d = new Date(ts * 1000); const pad = (n, w = 2) => String(n).padStart(w, "0"); const Y = pad(d.getUTCFullYear(), 4); const m = pad(d.getUTCMonth() + 1); const dd = pad(d.getUTCDate()); const H = pad(d.getUTCHours()); const M = pad(d.getUTCMinutes()); const S = pad(d.getUTCSeconds()); const days = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"]; const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]; const wd = d.getUTCDay(); const a = days[wd === 0 ? 6 : wd - 1]; const b = months[d.getUTCMonth()]; return pattern.replace("%F", `${Y}-${m}-${dd}`).replace("%T", `${H}:${M}:${S}`).replace("%Y", Y).replace("%m", m).replace("%d", dd).replace("%H", H).replace("%M", M).replace("%S", S).replace("%a", a).replace("%b", b); },
  to_iso(ts) { const d = new Date(ts * 1000); const pad = (n, w = 2) => String(n).padStart(w, "0"); return `${pad(d.getUTCFullYear(), 4)}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}Z`; },
  year(ts) { return new Date(ts * 1000).getUTCFullYear(); },
  month(ts) { return new Date(ts * 1000).getUTCMonth() + 1; },
  day(ts) { return new Date(ts * 1000).getUTCDate(); },
  hour(ts) { return new Date(ts * 1000).getUTCHours(); },
  minute(ts) { return new Date(ts * 1000).getUTCMinutes(); },
  second(ts) { return new Date(ts * 1000).getUTCSeconds(); },
  weekday(ts) { const days = ["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"]; return days[new Date(ts * 1000).getUTCDay()]; },
};
"#;

// ──────────────────────────────── testing ────────────────────────────────

pub(super) const MOD_TESTING_TS: &str = r#"const __almd_testing = {
  assert_throws(f: () => void, expected: string): void {
    try { f(); throw new Error("__no_throw__"); }
    catch (e: any) {
      if (e.message === "__no_throw__") throw new Error(`assert_throws: expected error '${expected}' but function succeeded`);
      if (!e.message.includes(expected)) throw new Error(`assert_throws: expected error containing '${expected}' but got '${e.message}'`);
    }
  },
  assert_contains(haystack: string, needle: string): void {
    if (!haystack.includes(needle)) throw new Error(`assert_contains failed\n  expected to contain: "${needle}"\n  in: "${haystack}"`);
  },
  assert_approx(a: number, b: number, tolerance: number): void {
    if (Math.abs(a - b) > tolerance) throw new Error(`assert_approx failed\n  left:  ${a}\n  right: ${b}\n  diff:  ${Math.abs(a - b)} > tolerance ${tolerance}`);
  },
  assert_gt(a: number, b: number): void {
    if (a <= b) throw new Error(`assert_gt failed: ${a} is not greater than ${b}`);
  },
  assert_lt(a: number, b: number): void {
    if (a >= b) throw new Error(`assert_lt failed: ${a} is not less than ${b}`);
  },
  assert_some(opt: any): void {
    if (opt === null || opt === undefined) throw new Error("assert_some failed: got none");
  },
  assert_ok(result: any): void {
    if (result instanceof __Err) throw new Error(`assert_ok failed: got err(${result.message})`);
  },
};
"#;

pub(super) const MOD_TESTING_JS: &str = r#"const __almd_testing = {
  assert_throws(f, expected) {
    try { f(); throw new Error("__no_throw__"); }
    catch (e) {
      if (e.message === "__no_throw__") throw new Error("assert_throws: expected error '" + expected + "' but function succeeded");
      if (!e.message.includes(expected)) throw new Error("assert_throws: expected error containing '" + expected + "' but got '" + e.message + "'");
    }
  },
  assert_contains(haystack, needle) {
    if (!haystack.includes(needle)) throw new Error("assert_contains failed\n  expected to contain: \"" + needle + "\"\n  in: \"" + haystack + "\"");
  },
  assert_approx(a, b, tolerance) {
    if (Math.abs(a - b) > tolerance) throw new Error("assert_approx failed\n  left:  " + a + "\n  right: " + b + "\n  diff:  " + Math.abs(a - b) + " > tolerance " + tolerance);
  },
  assert_gt(a, b) {
    if (a <= b) throw new Error("assert_gt failed: " + a + " is not greater than " + b);
  },
  assert_lt(a, b) {
    if (a >= b) throw new Error("assert_lt failed: " + a + " is not less than " + b);
  },
  assert_some(opt) {
    if (opt === null || opt === undefined) throw new Error("assert_some failed: got none");
  },
  assert_ok(result) {
    if (result instanceof __Err) throw new Error("assert_ok failed: got err(" + result.message + ")");
  },
};
"#;
