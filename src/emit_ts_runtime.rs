/// Runtime modules for Almide TypeScript/JavaScript code generation.
///
/// Each stdlib module (`__almd_*`) is defined as a separate constant pair
/// (TS for Deno, JS for Node.js). The `full_runtime()` function composes them
/// into the monolithic runtime string used by `--target ts` and `--target js`.
/// Individual modules can be retrieved via `get_module_source()` for the
/// `--target npm` output.

// ──────────────────────────────── Preambles ────────────────────────────────

const PREAMBLE_TS: &str = "// ---- Almide Runtime ----\n";
const PREAMBLE_JS: &str = "\
// ---- Almide Runtime (JS) ----
const __node_process = globalThis.process || {};
";

const EPILOGUE: &str = "// ---- End Runtime ----\n";

// ──────────────────────────────── fs ────────────────────────────────

const MOD_FS_TS: &str = r#"const __almd_fs = {
  exists(p: string): boolean { try { Deno.statSync(p); return true; } catch { return false; } },
  read_text(p: string): string { return Deno.readTextFileSync(p); },
  read_bytes(p: string): Uint8Array { return Deno.readFileSync(p); },
  write(p: string, s: string): void { Deno.writeTextFileSync(p, s); },
  write_bytes(p: string, b: Uint8Array | number[]): void { Deno.writeFileSync(p, b instanceof Uint8Array ? b : new Uint8Array(b)); },
  append(p: string, s: string): void { Deno.writeTextFileSync(p, Deno.readTextFileSync(p) + s); },
  mkdir_p(p: string): void { Deno.mkdirSync(p, { recursive: true }); },
  read_lines(p: string): string[] { return Deno.readTextFileSync(p).split("\n").filter(l => l.length > 0).map(l => l.replace(/\r$/, "")); },
  remove(p: string): void { Deno.removeSync(p, { recursive: true }); },
  list_dir(p: string): string[] { return [...Deno.readDirSync(p)].map(e => e.name).sort(); },
  walk(dir: string): string[] {
    const results: string[] = [];
    function inner(d: string) {
      for (const entry of Deno.readDirSync(d)) {
        const p = d + "/" + entry.name;
        results.push(p);
        if (entry.isDirectory) inner(p);
      }
    }
    inner(dir);
    return results.sort();
  },
  stat(path: string): {size: number, is_dir: boolean, is_file: boolean, modified: number} {
    const s = Deno.statSync(path);
    return { size: s.size, is_dir: s.isDirectory, is_file: s.isFile, modified: Math.floor((s.mtime?.getTime() ?? 0) / 1000) };
  },
  is_dir(p: string): boolean { try { return Deno.statSync(p).isDirectory; } catch { return false; } },
  is_file(p: string): boolean { try { return Deno.statSync(p).isFile; } catch { return false; } },
  copy(src: string, dst: string): void { Deno.copyFileSync(src, dst); },
  rename(src: string, dst: string): void { Deno.renameSync(src, dst); },
  remove_all(p: string): void { try { Deno.removeSync(p, { recursive: true }); } catch {} },
  file_size(p: string): number { return Deno.statSync(p).size; },
  temp_dir(): string { return Deno.env.get("TMPDIR") || "/tmp"; },
  glob(pattern: string): string[] {
    const results: string[] = [];
    const parts = pattern.split("/");
    let base = "";
    let gi = 0;
    for (let i = 0; i < parts.length; i++) {
      if (parts[i].includes("*") || parts[i].includes("?")) { gi = i; break; }
      base += (i > 0 ? "/" : "") + parts[i];
      gi = i + 1;
    }
    if (!base) base = ".";
    const globParts = parts.slice(gi);
    function match1(pat: string, name: string): boolean {
      if (pat === "*") return true;
      let pi = 0, ni = 0, sp = -1, sn = 0;
      while (ni < name.length) {
        if (pi < pat.length && (pat[pi] === "?" || pat[pi] === name[ni])) { pi++; ni++; }
        else if (pi < pat.length && pat[pi] === "*") { sp = pi; sn = ni; pi++; }
        else if (sp >= 0) { pi = sp + 1; sn++; ni = sn; }
        else return false;
      }
      while (pi < pat.length && pat[pi] === "*") pi++;
      return pi === pat.length;
    }
    function inner(dir: string, gp: string[]) {
      if (gp.length === 0) return;
      const part = gp[0], rest = gp.slice(1);
      if (part === "**") {
        if (rest.length) inner(dir, rest);
        try { for (const e of Deno.readDirSync(dir)) { if (e.isDirectory) inner(dir + "/" + e.name, gp); } } catch {}
      } else {
        try {
          for (const e of Deno.readDirSync(dir)) {
            if (match1(part, e.name)) {
              const p = dir + "/" + e.name;
              if (rest.length === 0) results.push(p);
              else if (e.isDirectory) inner(p, rest);
            }
          }
        } catch {}
      }
    }
    if (globParts.length === 0) { try { Deno.statSync(base); results.push(base); } catch {} }
    else inner(base, globParts);
    return results.sort();
  },
  create_temp_file(prefix: string): string {
    const p = Deno.makeTempFileSync({ prefix });
    return p.replace(/\\/g, "/");
  },
  create_temp_dir(prefix: string): string {
    const p = Deno.makeTempDirSync({ prefix });
    return p.replace(/\\/g, "/");
  },
  is_symlink(p: string): boolean { try { return Deno.lstatSync(p).isSymlink; } catch { return false; } },
  modified_at(p: string): number { const s = Deno.statSync(p); return Math.floor((s.mtime?.getTime() ?? 0) / 1000); },
};
"#;

const MOD_FS_JS: &str = r#"const __almd_fs = {
  exists(p) { const fs = require("fs"); try { fs.statSync(p); return true; } catch { return false; } },
  read_text(p) { return require("fs").readFileSync(p, "utf-8"); },
  read_bytes(p) { return Array.from(require("fs").readFileSync(p)); },
  write(p, s) { require("fs").writeFileSync(p, s); },
  write_bytes(p, b) { require("fs").writeFileSync(p, Buffer.from(b)); },
  append(p, s) { require("fs").appendFileSync(p, s); },
  mkdir_p(p) { require("fs").mkdirSync(p, { recursive: true }); },
  read_lines(p) { return require("fs").readFileSync(p, "utf-8").split("\n").filter(l => l.length > 0).map(l => l.replace(/\r$/, "")); },
  remove(p) { const fs = require("fs"); try { const s = fs.statSync(p); if (s.isDirectory()) fs.rmSync(p, { recursive: true }); else fs.unlinkSync(p); } catch(e) { throw e; } },
  list_dir(p) { return require("fs").readdirSync(p).sort(); },
  walk(dir) {
    const fs = require("fs");
    const results = [];
    function inner(d) {
      const entries = fs.readdirSync(d, { withFileTypes: true });
      for (const entry of entries) {
        const p = d + "/" + entry.name;
        results.push(p);
        if (entry.isDirectory()) inner(p);
      }
    }
    inner(dir);
    return results.sort();
  },
  stat(path) {
    const fs = require("fs");
    const s = fs.statSync(path);
    return { size: s.size, is_dir: s.isDirectory(), is_file: s.isFile(), modified: Math.floor(s.mtimeMs / 1000) };
  },
  is_dir(p) { try { return require("fs").statSync(p).isDirectory(); } catch { return false; } },
  is_file(p) { try { return require("fs").statSync(p).isFile(); } catch { return false; } },
  copy(src, dst) { require("fs").copyFileSync(src, dst); },
  rename(src, dst) { require("fs").renameSync(src, dst); },
  remove_all(p) { require("fs").rmSync(p, { recursive: true, force: true }); },
  file_size(p) { return require("fs").statSync(p).size; },
  temp_dir() { return require("os").tmpdir().replace(/\\/g, "/"); },
  glob(pattern) {
    const fs = require("fs"), path = require("path");
    const results = [];
    const parts = pattern.split("/");
    var base = "", gi = 0;
    for (var i = 0; i < parts.length; i++) {
      if (parts[i].includes("*") || parts[i].includes("?")) { gi = i; break; }
      base += (i > 0 ? "/" : "") + parts[i];
      gi = i + 1;
    }
    if (!base) base = ".";
    const globParts = parts.slice(gi);
    function match1(pat, name) {
      if (pat === "*") return true;
      var pi = 0, ni = 0, sp = -1, sn = 0;
      while (ni < name.length) {
        if (pi < pat.length && (pat[pi] === "?" || pat[pi] === name[ni])) { pi++; ni++; }
        else if (pi < pat.length && pat[pi] === "*") { sp = pi; sn = ni; pi++; }
        else if (sp >= 0) { pi = sp + 1; sn++; ni = sn; }
        else return false;
      }
      while (pi < pat.length && pat[pi] === "*") pi++;
      return pi === pat.length;
    }
    function inner(dir, gp) {
      if (gp.length === 0) return;
      var part = gp[0], rest = gp.slice(1);
      if (part === "**") {
        if (rest.length) inner(dir, rest);
        try { for (const e of fs.readdirSync(dir, { withFileTypes: true })) { if (e.isDirectory()) inner(dir + "/" + e.name, gp); } } catch {}
      } else {
        try {
          for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
            if (match1(part, e.name)) {
              var p = dir + "/" + e.name;
              if (rest.length === 0) results.push(p);
              else if (e.isDirectory()) inner(p, rest);
            }
          }
        } catch {}
      }
    }
    if (globParts.length === 0) { try { fs.statSync(base); results.push(base); } catch {} }
    else inner(base, globParts);
    return results.sort();
  },
  create_temp_file(prefix) {
    const os = require("os"), fs = require("fs"), path = require("path");
    const p = path.join(os.tmpdir(), prefix + Date.now() + Math.random().toString(36).slice(2));
    fs.writeFileSync(p, "");
    return p.replace(/\\/g, "/");
  },
  create_temp_dir(prefix) {
    const os = require("os"), fs = require("fs"), path = require("path");
    const p = path.join(os.tmpdir(), prefix + Date.now() + Math.random().toString(36).slice(2));
    fs.mkdirSync(p, { recursive: true });
    return p.replace(/\\/g, "/");
  },
  is_symlink(p) { try { return require("fs").lstatSync(p).isSymbolicLink(); } catch { return false; } },
  modified_at(p) { return Math.floor(require("fs").statSync(p).mtimeMs / 1000); },
};
"#;

// ──────────────────────────────── string ────────────────────────────────

const MOD_STRING_TS: &str = r#"const __almd_string = {
  trim(s: string): string { return s.trim(); },
  split(s: string, sep: string): string[] { return s.split(sep); },
  join(arr: string[], sep: string): string { return arr.join(sep); },
  len(s: string): number { return s.length; },
  pad_left(s: string, n: number, ch: string): string { return s.padStart(n, ch); },
  starts_with(s: string, prefix: string): boolean { return s.startsWith(prefix); },
  slice(s: string, start: number, end?: number): string { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s: string): number[] { return Array.from(new TextEncoder().encode(s)); },
  contains(s: string, sub: string): boolean { return s.includes(sub); },
  capitalize(s: string): string { return s.length === 0 ? "" : s[0].toUpperCase() + s.slice(1); },
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
  is_digit(s: string): boolean { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha(s: string): boolean { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric(s: string): boolean { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace(s: string): boolean { return s.length > 0 && /^\s+$/.test(s); },
  replace_first(s: string, from: string, to: string): string { const i = s.indexOf(from); return i < 0 ? s : s.slice(0, i) + to + s.slice(i + from.length); },
  last_index_of(s: string, needle: string): number | null { const i = s.lastIndexOf(needle); return i >= 0 ? i : null; },
  to_float(s: string): number { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float number: " + s); return n; },
  pad_right(s: string, n: number, ch: string): string { return s.padEnd(n, ch); },
  trim_start(s: string): string { return s.trimStart(); },
  trim_end(s: string): string { return s.trimEnd(); },
  count(s: string, sub: string): number { if (!sub) return 0; let c = 0, i = 0; while ((i = s.indexOf(sub, i)) >= 0) { c++; i += sub.length; } return c; },
  is_empty(s: string): boolean { return s.length === 0; },
  reverse(s: string): string { return [...s].reverse().join(""); },
  strip_prefix(s: string, prefix: string): string | null { return s.startsWith(prefix) ? s.slice(prefix.length) : null; },
  strip_suffix(s: string, suffix: string): string | null { return s.endsWith(suffix) ? s.slice(0, -suffix.length) : null; },
  ends_with(s: string, suffix: string): boolean { return s.endsWith(suffix); },
  is_upper(s: string): boolean { return s.length > 0 && s === s.toUpperCase() && s !== s.toLowerCase(); },
  is_lower(s: string): boolean { return s.length > 0 && s === s.toLowerCase() && s !== s.toUpperCase(); },
};
"#;

const MOD_STRING_JS: &str = r#"const __almd_string = {
  trim(s) { return s.trim(); },
  split(s, sep) { return s.split(sep); },
  join(arr, sep) { return arr.join(sep); },
  len(s) { return s.length; },
  pad_left(s, n, ch) { return s.padStart(n, ch); },
  starts_with(s, prefix) { return s.startsWith(prefix); },
  slice(s, start, end) { return end !== undefined ? s.slice(start, end) : s.slice(start); },
  to_bytes(s) { return Array.from(new TextEncoder().encode(s)); },
  contains(s, sub) { return s.includes(sub); },
  capitalize(s) { return s.length === 0 ? "" : s[0].toUpperCase() + s.slice(1); },
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
  is_digit(s) { return s.length > 0 && /^[0-9]+$/.test(s); },
  is_alpha(s) { return s.length > 0 && /^[a-zA-Z]+$/.test(s); },
  is_alphanumeric(s) { return s.length > 0 && /^[a-zA-Z0-9]+$/.test(s); },
  is_whitespace(s) { return s.length > 0 && /^\s+$/.test(s); },
  replace_first(s, from, to) { const i = s.indexOf(from); return i < 0 ? s : s.slice(0, i) + to + s.slice(i + from.length); },
  last_index_of(s, needle) { const i = s.lastIndexOf(needle); return i >= 0 ? i : null; },
  to_float(s) { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float number: " + s); return n; },
  pad_right(s, n, ch) { return s.padEnd(n, ch); },
  trim_start(s) { return s.trimStart(); },
  trim_end(s) { return s.trimEnd(); },
  count(s, sub) { if (!sub) return 0; let c = 0, i = 0; while ((i = s.indexOf(sub, i)) >= 0) { c++; i += sub.length; } return c; },
  is_empty(s) { return s.length === 0; },
  reverse(s) { return [...s].reverse().join(""); },
  strip_prefix(s, prefix) { return s.startsWith(prefix) ? s.slice(prefix.length) : null; },
  strip_suffix(s, suffix) { return s.endsWith(suffix) ? s.slice(0, -suffix.length) : null; },
  ends_with(s, suffix) { return s.endsWith(suffix); },
  is_upper(s) { return s.length > 0 && s === s.toUpperCase() && s !== s.toLowerCase(); },
  is_lower(s) { return s.length > 0 && s === s.toLowerCase() && s !== s.toUpperCase(); },
};
"#;

// ──────────────────────────────── list ────────────────────────────────

const MOD_LIST_TS: &str = r#"const __almd_list = {
  len<T>(xs: T[]): number { return xs.length; },
  get<T>(xs: T[], i: number): T | null { return (i >= 0 && i < xs.length) ? xs[i] : null; },
  get_or<T>(xs: T[], i: number, d: T): T { return (i >= 0 && i < xs.length) ? xs[i] : d; },
  set<T>(xs: T[], i: number, v: T): T[] { const r = [...xs]; if (i >= 0 && i < r.length) r[i] = v; return r; },
  swap<T>(xs: T[], i: number, j: number): T[] { const r = [...xs]; if (i >= 0 && i < r.length && j >= 0 && j < r.length) { const t = r[i]; r[i] = r[j]; r[j] = t; } return r; },
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
  index_of<T>(xs: T[], x: T): number | null { const i = xs.indexOf(x); return i >= 0 ? i : null; },
  chunk<T>(xs: T[], n: number): T[][] { const r: T[][] = []; for (let i = 0; i < xs.length; i += n) r.push(xs.slice(i, i + n)); return r; },
  filter_map<T, U>(xs: T[], f: (x: T) => U | null): U[] { const r: U[] = []; for (const x of xs) { const v = f(x); if (v !== null) r.push(v); } return r; },
  take_while<T>(xs: T[], f: (x: T) => boolean): T[] { const r: T[] = []; for (const x of xs) { if (!f(x)) break; r.push(x); } return r; },
  drop_while<T>(xs: T[], f: (x: T) => boolean): T[] { let dropping = true; const r: T[] = []; for (const x of xs) { if (dropping && f(x)) continue; dropping = false; r.push(x); } return r; },
  count<T>(xs: T[], f: (x: T) => boolean): number { return xs.filter(f).length; },
  partition<T>(xs: T[], f: (x: T) => boolean): [T[], T[]] { const a: T[] = [], b: T[] = []; for (const x of xs) { if (f(x)) a.push(x); else b.push(x); } return [a, b]; },
  reduce<T>(xs: T[], f: (a: T, b: T) => T): T | null { if (xs.length === 0) return null; return xs.reduce(f); },
  group_by<T, K>(xs: T[], f: (x: T) => K): Map<K, T[]> { const m = new Map<K, T[]>(); for (const x of xs) { const k = f(x); if (!m.has(k)) m.set(k, []); m.get(k)!.push(x); } return m; },
  last<T>(xs: T[]): T | null { return xs.length > 0 ? xs[xs.length - 1] : null; },
  first<T>(xs: T[]): T | null { return xs.length > 0 ? xs[0] : null; },
  sum(xs: number[]): number { return xs.reduce((a, b) => a + b, 0); },
  product(xs: number[]): number { return xs.reduce((a, b) => a * b, 1); },
  is_empty<T>(xs: T[]): boolean { return xs.length === 0; },
  flat_map<T, U>(xs: T[], f: (x: T) => U[]): U[] { return xs.flatMap(f); },
  min<T>(xs: T[]): T | null { return xs.length === 0 ? null : xs.reduce((a, b) => a < b ? a : b); },
  max<T>(xs: T[]): T | null { return xs.length === 0 ? null : xs.reduce((a, b) => a > b ? a : b); },
  join(xs: string[], sep: string): string { return xs.join(sep); },
  range(start: number, end: number): number[] { const r: number[] = []; for (let i = start; i < end; i++) r.push(i); return r; },
  slice<T>(xs: T[], start: number, end: number): T[] { return xs.slice(start, end); },
  insert<T>(xs: T[], i: number, v: T): T[] { const r = [...xs]; r.splice(i, 0, v); return r; },
  remove_at<T>(xs: T[], i: number): T[] { const r = [...xs]; if (i >= 0 && i < r.length) r.splice(i, 1); return r; },
  find_index<T>(xs: T[], f: (x: T) => boolean): number | null { const i = xs.findIndex(f); return i >= 0 ? i : null; },
  update<T>(xs: T[], i: number, f: (x: T) => T): T[] { const r = [...xs]; if (i >= 0 && i < r.length) r[i] = f(r[i]); return r; },
  repeat<T>(v: T, n: number): T[] { return Array(Math.max(0, n)).fill(v); },
  scan<T, U>(xs: T[], init: U, f: (acc: U, x: T) => U): U[] { let acc = init; return xs.map(x => { acc = f(acc, x); return acc; }); },
  intersperse<T>(xs: T[], sep: T): T[] { const r: T[] = []; xs.forEach((x, i) => { if (i > 0) r.push(sep); r.push(x); }); return r; },
  windows<T>(xs: T[], n: number): T[][] { if (n <= 0 || n > xs.length) return []; const r: T[][] = []; for (let i = 0; i <= xs.length - n; i++) r.push(xs.slice(i, i + n)); return r; },
  dedup<T>(xs: T[]): T[] { const r: T[] = []; for (const x of xs) { if (r.length === 0 || r[r.length - 1] !== x) r.push(x); } return r; },
  zip_with<A, B, C>(a: A[], b: B[], f: (x: A, y: B) => C): C[] { return a.slice(0, Math.min(a.length, b.length)).map((x, i) => f(x, b[i])); },
  sum_float(xs: number[]): number { return xs.reduce((a, b) => a + b, 0); },
  product_float(xs: number[]): number { return xs.reduce((a, b) => a * b, 1); },
};
"#;

const MOD_LIST_JS: &str = r#"const __almd_list = {
  len(xs) { return xs.length; },
  get(xs, i) { return (i >= 0 && i < xs.length) ? xs[i] : null; },
  get_or(xs, i, d) { return (i >= 0 && i < xs.length) ? xs[i] : d; },
  set(xs, i, v) { const r = [...xs]; if (i >= 0 && i < r.length) r[i] = v; return r; },
  swap(xs, i, j) { const r = [...xs]; if (i >= 0 && i < r.length && j >= 0 && j < r.length) { const t = r[i]; r[i] = r[j]; r[j] = t; } return r; },
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
  index_of(xs, x) { const i = xs.indexOf(x); return i >= 0 ? i : null; },
  chunk(xs, n) { const r = []; for (let i = 0; i < xs.length; i += n) r.push(xs.slice(i, i + n)); return r; },
  filter_map(xs, f) { const r = []; for (const x of xs) { const v = f(x); if (v !== null) r.push(v); } return r; },
  take_while(xs, f) { const r = []; for (const x of xs) { if (!f(x)) break; r.push(x); } return r; },
  drop_while(xs, f) { let dropping = true; const r = []; for (const x of xs) { if (dropping && f(x)) continue; dropping = false; r.push(x); } return r; },
  count(xs, f) { return xs.filter(f).length; },
  partition(xs, f) { const a = [], b = []; for (const x of xs) { if (f(x)) a.push(x); else b.push(x); } return [a, b]; },
  reduce(xs, f) { if (xs.length === 0) return null; return xs.reduce(f); },
  group_by(xs, f) { const m = new Map(); for (const x of xs) { const k = f(x); if (!m.has(k)) m.set(k, []); m.get(k).push(x); } return m; },
  last(xs) { return xs.length > 0 ? xs[xs.length - 1] : null; },
  first(xs) { return xs.length > 0 ? xs[0] : null; },
  sum(xs) { return xs.reduce((a, b) => a + b, 0); },
  product(xs) { return xs.reduce((a, b) => a * b, 1); },
  is_empty(xs) { return xs.length === 0; },
  flat_map(xs, f) { return xs.flatMap(f); },
  min(xs) { return xs.length === 0 ? null : xs.reduce((a, b) => a < b ? a : b); },
  max(xs) { return xs.length === 0 ? null : xs.reduce((a, b) => a > b ? a : b); },
  join(xs, sep) { return xs.join(sep); },
  range(start, end) { const r = []; for (let i = start; i < end; i++) r.push(i); return r; },
  slice(xs, start, end) { return xs.slice(start, end); },
  insert(xs, i, v) { const r = [...xs]; r.splice(i, 0, v); return r; },
  remove_at(xs, i) { const r = [...xs]; if (i >= 0 && i < r.length) r.splice(i, 1); return r; },
  find_index(xs, f) { const i = xs.findIndex(f); return i >= 0 ? i : null; },
  update(xs, i, f) { const r = [...xs]; if (i >= 0 && i < r.length) r[i] = f(r[i]); return r; },
  repeat(v, n) { return Array(Math.max(0, n)).fill(v); },
  scan(xs, init, f) { let acc = init; return xs.map(x => { acc = f(acc, x); return acc; }); },
  intersperse(xs, sep) { const r = []; xs.forEach((x, i) => { if (i > 0) r.push(sep); r.push(x); }); return r; },
  windows(xs, n) { if (n <= 0 || n > xs.length) return []; const r = []; for (let i = 0; i <= xs.length - n; i++) r.push(xs.slice(i, i + n)); return r; },
  dedup(xs) { const r = []; for (const x of xs) { if (r.length === 0 || r[r.length - 1] !== x) r.push(x); } return r; },
  zip_with(a, b, f) { return a.slice(0, Math.min(a.length, b.length)).map((x, i) => f(x, b[i])); },
  sum_float(xs) { return xs.reduce((a, b) => a + b, 0); },
  product_float(xs) { return xs.reduce((a, b) => a * b, 1); },
};
"#;

// ──────────────────────────────── map ────────────────────────────────

const MOD_MAP_TS: &str = r#"const __almd_map = {
  new<K, V>(): Map<K, V> { return new Map(); },
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
  map_values<K, V, V2>(m: Map<K, V>, f: (v: V) => V2): Map<K, V2> { const r = new Map<K, V2>(); m.forEach((v, k) => r.set(k, f(v))); return r; },
  filter<K, V>(m: Map<K, V>, f: (k: K, v: V) => boolean): Map<K, V> { const r = new Map<K, V>(); m.forEach((v, k) => { if (f(k, v)) r.set(k, v); }); return r; },
  from_entries<K, V>(entries: [K, V][]): Map<K, V> { const r = new Map<K, V>(); for (const [k, v] of entries) r.set(k, v); return r; },
  merge<K, V>(a: Map<K, V>, b: Map<K, V>): Map<K, V> { const r = new Map(a); b.forEach((v, k) => r.set(k, v)); return r; },
  is_empty<K, V>(m: Map<K, V>): boolean { return m.size === 0; },
};
"#;

const MOD_MAP_JS: &str = r#"const __almd_map = {
  new() { return new Map(); },
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
  map_values(m, f) { const r = new Map(); m.forEach((v, k) => r.set(k, f(v))); return r; },
  filter(m, f) { const r = new Map(); m.forEach((v, k) => { if (f(k, v)) r.set(k, v); }); return r; },
  from_entries(entries) { const r = new Map(); for (const [k, v] of entries) r.set(k, v); return r; },
  merge(a, b) { const r = new Map(a); b.forEach((v, k) => r.set(k, v)); return r; },
  is_empty(m) { return m.size === 0; },
};
"#;

// ──────────────────────────────── int ────────────────────────────────

const MOD_INT_TS: &str = r#"const __almd_int = {
  to_hex(n: bigint): string { return (n >= 0n ? n : n + (1n << 64n)).toString(16); },
  to_string(n: number): string { return String(n); },
  band(a: number, b: number): number { return (a & b) >>> 0; },
  bor(a: number, b: number): number { return (a | b) >>> 0; },
  bxor(a: number, b: number): number { return (a ^ b) >>> 0; },
  bshl(a: number, n: number): number { return (a << n) >>> 0; },
  bshr(a: number, n: number): number { return a >>> n; },
  bnot(a: number): number { return ~a; },
  wrap_add(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a: number, b: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v >>> n) | (v << (bits - n))) & mask) >>> 0; },
  rotate_left(a: number, n: number, bits: number): number { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v << n) | (v >>> (bits - n))) & mask) >>> 0; },
  to_u32(a: number): number { return a >>> 0; },
  to_u8(a: number): number { return a & 0xFF; },
  clamp(n: number, lo: number, hi: number): number { return Math.max(lo, Math.min(hi, n)); },
  parse(s: string): number { const n = parseInt(s, 10); if (isNaN(n) || !/^-?\d+$/.test(s.trim())) throw new Error("invalid digit found in string"); return n; },
  parse_hex(s: string): number { const h = s.startsWith("0x") || s.startsWith("0X") ? s.slice(2) : s; const n = parseInt(h, 16); if (isNaN(n)) throw new Error("invalid digit found in string"); return n; },
  abs(n: number): number { return Math.abs(n); },
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  to_float(n: number): number { return n; },
};
"#;

const MOD_INT_JS: &str = r#"const __almd_int = {
  to_hex(n) { return (typeof n === "bigint" ? (n >= 0n ? n : n + (1n << 64n)).toString(16) : n.toString(16)); },
  to_string(n) { return String(n); },
  band(a, b) { return (a & b) >>> 0; },
  bor(a, b) { return (a | b) >>> 0; },
  bxor(a, b) { return (a ^ b) >>> 0; },
  bshl(a, n) { return (a << n) >>> 0; },
  bshr(a, n) { return a >>> n; },
  bnot(a) { return ~a; },
  wrap_add(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return ((a + b) & mask) >>> 0; },
  wrap_mul(a, b, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; return (Math.imul(a, b) & mask) >>> 0; },
  rotate_right(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v >>> n) | (v << (bits - n))) & mask) >>> 0; },
  rotate_left(a, n, bits) { const mask = bits === 32 ? 0xFFFFFFFF : (1 << bits) - 1; const v = a & mask; n = n % bits; return (((v << n) | (v >>> (bits - n))) & mask) >>> 0; },
  to_u32(a) { return a >>> 0; },
  to_u8(a) { return a & 0xFF; },
  clamp(n, lo, hi) { return Math.max(lo, Math.min(hi, n)); },
  parse(s) { var n = parseInt(s, 10); if (isNaN(n) || !/^-?\d+$/.test(s.trim())) throw new Error("invalid digit found in string"); return n; },
  parse_hex(s) { var h = s.startsWith("0x") || s.startsWith("0X") ? s.slice(2) : s; var n = parseInt(h, 16); if (isNaN(n)) throw new Error("invalid digit found in string"); return n; },
  abs(n) { return Math.abs(n); },
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  to_float(n) { return n; },
};
"#;

// ──────────────────────────────── float ────────────────────────────────

const MOD_FLOAT_TS: &str = r#"const __almd_float = {
  to_string(n: number): string { const s = String(n); return s.includes('.') || s.includes('e') ? s : s + '.0'; },
  to_int(n: number): number { return Math.trunc(n); },
  round(n: number): number { return Math.round(n); },
  floor(n: number): number { return Math.floor(n); },
  ceil(n: number): number { return Math.ceil(n); },
  abs(n: number): number { return Math.abs(n); },
  sqrt(n: number): number { return Math.sqrt(n); },
  parse(s: string): number { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n: number): number { return n; },
  min(a: number, b: number): number { return Math.min(a, b); },
  max(a: number, b: number): number { return Math.max(a, b); },
  clamp(n: number, lo: number, hi: number): number { return Math.max(lo, Math.min(hi, n)); },
  to_fixed(n: number, decimals: number): string { return n.toFixed(decimals); },
};
"#;

const MOD_FLOAT_JS: &str = r#"const __almd_float = {
  to_string(n) { const s = String(n); return s.includes('.') || s.includes('e') ? s : s + '.0'; },
  to_int(n) { return Math.trunc(n); },
  round(n) { return Math.round(n); },
  floor(n) { return Math.floor(n); },
  ceil(n) { return Math.ceil(n); },
  abs(n) { return Math.abs(n); },
  sqrt(n) { return Math.sqrt(n); },
  parse(s) { const n = parseFloat(s); if (isNaN(n)) throw new Error("invalid float: " + s); return n; },
  from_int(n) { return n; },
  min(a, b) { return Math.min(a, b); },
  max(a, b) { return Math.max(a, b); },
  clamp(n, lo, hi) { return Math.max(lo, Math.min(hi, n)); },
  to_fixed(n, decimals) { return n.toFixed(decimals); },
};
"#;

// ──────────────────────────────── path ────────────────────────────────

const MOD_PATH_TS: &str = r#"const __almd_path = {
  join(base: string, child: string): string { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p: string): string { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p: string): string | null { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p: string): boolean { return p.startsWith("/"); },
};
"#;

const MOD_PATH_JS: &str = r#"const __almd_path = {
  join(base, child) { return base.replace(/\/+$/, "") + "/" + child; },
  dirname(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(0, i) : "."; },
  basename(p) { const i = p.lastIndexOf("/"); return i >= 0 ? p.substring(i + 1) : p; },
  extension(p) { const b = __almd_path.basename(p); const i = b.lastIndexOf("."); return i > 0 ? b.substring(i + 1) : null; },
  is_absolute(p) { return p.startsWith("/"); },
};
"#;

// ──────────────────────────────── json ────────────────────────────────

const MOD_JSON_TS: &str = r#"const __almd_json = {
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

const MOD_JSON_JS: &str = r#"const __almd_json = {
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

// ──────────────────────────────── env ────────────────────────────────

const MOD_ENV_TS: &str = r#"const __almd_env = {
  unix_timestamp(): number { return Math.floor(Date.now() / 1000); },
  args(): string[] { return Deno.args; },
  get(name: string): string | null { const v = Deno.env.get(name); return v !== undefined ? v : null; },
  set(name: string, value: string): void { Deno.env.set(name, value); },
  cwd(): string { return Deno.cwd(); },
  millis(): number { return Date.now(); },
  sleep_ms(ms: number): void { const end = Date.now() + ms; while (Date.now() < end) {} },
  temp_dir(): string { return Deno.env.get("TMPDIR") || Deno.env.get("TEMP") || Deno.env.get("TMP") || "/tmp"; },
  os(): string { return Deno.build.os === "darwin" ? "macos" : Deno.build.os; },
};
"#;

const MOD_ENV_JS: &str = r#"const __almd_env = {
  unix_timestamp() { return Math.floor(Date.now() / 1000); },
  args() { return __node_process.argv.slice(2); },
  get(name) { const v = __node_process.env[name]; return v !== undefined ? v : null; },
  set(name, value) { __node_process.env[name] = value; },
  cwd() { return __node_process.cwd(); },
  millis() { return Date.now(); },
  sleep_ms(ms) { const end = Date.now() + ms; while (Date.now() < end) {} },
  temp_dir() { return require("os").tmpdir(); },
  os() { const p = require("os").platform(); return p === "darwin" ? "macos" : p === "win32" ? "windows" : p; },
};
"#;

// ──────────────────────────────── process ────────────────────────────────

const MOD_PROCESS_TS: &str = r#"const __almd_process = {
  exec(cmd: string, args: string[]): string { try { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { const msg = new TextDecoder().decode(out.stderr); throw new Error(msg || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
  exec_status(cmd: string, args: string[]): {code: number, stdout: string, stderr: string} { try { const p = new Deno.Command(cmd, { args, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); return { code: out.code, stdout: new TextDecoder().decode(out.stdout), stderr: new TextDecoder().decode(out.stderr) }; } catch (e) { throw e instanceof Error ? e : new Error(String(e)); } },
  exit(code: number): void { Deno.exit(code); },
  stdin_lines(): string[] { const buf = new Uint8Array(1024 * 1024); const n = Deno.stdin.readSync(buf); return n ? new TextDecoder().decode(buf.subarray(0, n)).split("\n").filter(l => l.length > 0) : []; },
  exec_in(dir: string, cmd: string, args: string[]): string { try { const p = new Deno.Command(cmd, { args, cwd: dir, stdout: "piped", stderr: "piped" }); const out = p.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { const msg = new TextDecoder().decode(out.stderr); throw new Error(msg || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
  exec_with_stdin(cmd: string, args: string[], input: string): string { try { const p = new Deno.Command(cmd, { args, stdin: "piped", stdout: "piped", stderr: "piped" }); const child = p.spawn(); const writer = child.stdin.getWriter(); writer.write(new TextEncoder().encode(input)); writer.close(); const out = child.outputSync(); if (out.success) { return new TextDecoder().decode(out.stdout); } else { throw new Error(new TextDecoder().decode(out.stderr) || "command failed"); } } catch (e) { if (e instanceof Error) throw e; throw new Error(String(e)); } },
};
"#;

const MOD_PROCESS_JS: &str = r#"const __almd_process = {
  exec(cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8" }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_status(cmd, args) { const { spawnSync } = require("child_process"); const r = spawnSync(cmd, args, { encoding: "utf-8" }); if (r.error) throw r.error; return { code: r.status ?? 1, stdout: r.stdout || "", stderr: r.stderr || "" }; },
  exit(code) { __node_process.exit(code); },
  stdin_lines() { return require("fs").readFileSync(0, "utf-8").split("\n").filter(l => l.length > 0); },
  exec_in(dir, cmd, args) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", cwd: dir }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
  exec_with_stdin(cmd, args, input) { const { execFileSync } = require("child_process"); try { return execFileSync(cmd, args, { encoding: "utf-8", input }); } catch (e) { const msg = e.stderr ? String(e.stderr) : e.message; throw new Error(msg || "command failed"); } },
};
"#;

// ──────────────────────────────── math ────────────────────────────────

const MOD_MATH_TS: &str = r#"const __almd_math = {
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
  factorial(n: number): number { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r; },
  choose(n: number, k: number): number { if (k < 0 || k > n) return 0; if (k === 0 || k === n) return 1; k = Math.min(k, n - k); let r = 1; for (let i = 0; i < k; i++) { r = r * (n - i) / (i + 1); } return Math.round(r); },
  log_gamma(x: number): number { if (x <= 0) return Infinity; if (x < 0.5) return Math.log(Math.PI / Math.sin(Math.PI * x)) - __almd_math.log_gamma(1 - x); x -= 1; const c = [76.18009172947146, -86.50532032941677, 24.01409824083091, -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5]; let sum = 1.000000000190015; let y = x; for (const ci of c) { y += 1; sum += ci / y; } const t = x + 5.5; return -t + (x + 0.5) * Math.log(t) + Math.log(2.5066282746310005 * sum / (x + 1)); },
};
"#;

const MOD_MATH_JS: &str = r#"const __almd_math = {
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
  factorial(n) { let r = 1; for (let i = 2; i <= n; i++) r *= i; return r; },
  choose(n, k) { if (k < 0 || k > n) return 0; if (k === 0 || k === n) return 1; k = Math.min(k, n - k); let r = 1; for (let i = 0; i < k; i++) { r = r * (n - i) / (i + 1); } return Math.round(r); },
  log_gamma(x) { if (x <= 0) return Infinity; if (x < 0.5) return Math.log(Math.PI / Math.sin(Math.PI * x)) - __almd_math.log_gamma(1 - x); x -= 1; const c = [76.18009172947146, -86.50532032941677, 24.01409824083091, -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5]; let sum = 1.000000000190015; let y = x; for (const ci of c) { y += 1; sum += ci / y; } const t = x + 5.5; return -t + (x + 0.5) * Math.log(t) + Math.log(2.5066282746310005 * sum / (x + 1)); },
};
"#;

// ──────────────────────────────── random ────────────────────────────────

const MOD_RANDOM_TS: &str = r#"const __almd_random = {
  int(min: number, max: number): number { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float(): number { return Math.random(); },
  choice<T>(xs: T[]): T | null { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle<T>(xs: T[]): T[] { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
"#;

const MOD_RANDOM_JS: &str = r#"const __almd_random = {
  int(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; },
  float() { return Math.random(); },
  choice(xs) { return xs.length > 0 ? xs[Math.floor(Math.random() * xs.length)] : null; },
  shuffle(xs) { const a = [...xs]; for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; },
};
"#;

// ──────────────────────────────── regex ────────────────────────────────

const MOD_REGEX_TS: &str = r#"const __almd_regex = {
  is_match(pat: string, s: string): boolean { return new RegExp(pat).test(s); },
  full_match(pat: string, s: string): boolean { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat: string, s: string): string | null { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat: string, s: string): string[] { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat: string, s: string, rep: string): string { return s.replace(new RegExp(pat), rep); },
  split(pat: string, s: string): string[] { return s.split(new RegExp(pat)); },
  captures(pat: string, s: string): string[] | null { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
"#;

const MOD_REGEX_JS: &str = r#"const __almd_regex = {
  is_match(pat, s) { return new RegExp(pat).test(s); },
  full_match(pat, s) { return new RegExp(`^(?:${pat})$`).test(s); },
  find(pat, s) { const m = s.match(new RegExp(pat)); return m ? m[0] : null; },
  find_all(pat, s) { const m = s.match(new RegExp(pat, 'g')); return m ? [...m] : []; },
  replace(pat, s, rep) { return s.replace(new RegExp(pat, 'g'), rep); },
  replace_first(pat, s, rep) { return s.replace(new RegExp(pat), rep); },
  split(pat, s) { return s.split(new RegExp(pat)); },
  captures(pat, s) { const m = s.match(new RegExp(pat)); return m && m.length > 1 ? m.slice(1) : null; },
};
"#;

// ──────────────────────────────── io ────────────────────────────────

const MOD_IO_TS: &str = r#"const __almd_io = {
  read_line(): string { return prompt("") ?? ""; },
  print(s: string): void { const buf = new TextEncoder().encode(s); Deno.stdout.writeSync(buf); },
  read_all(): string { const d = new TextDecoder(); let r = ""; const buf = new Uint8Array(4096); let n: number | null; while ((n = Deno.stdin.readSync(buf)) !== null && n > 0) { r += d.decode(buf.subarray(0, n)); } return r; },
};
"#;

const MOD_IO_JS: &str = r#"const __almd_io = {
  read_line() { const buf = Buffer.alloc(1024); let s = ""; while (true) { const n = require("fs").readSync(0, buf, 0, 1, null); if (n === 0) break; const ch = buf.toString("utf-8", 0, n); s += ch; if (ch === "\n") break; } return s.replace(/\r?\n$/, ""); },
  print(s) { __node_process.stdout.write(s); },
  read_all() { return require("fs").readFileSync(0, "utf-8"); },
};
"#;

// ──────────────────────────────── time ────────────────────────────────

const MOD_TIME_TS: &str = r#"const __almd_time = {
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
  to_iso(ts: number): string { const [y, m, d, h, mi, s] = __almd_time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y: number, m: number, d: number, h: number, min: number, s: number): number { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
"#;

const MOD_TIME_JS: &str = r#"const __almd_time = {
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
  to_iso(ts) { const [y, m, d, h, mi, s] = __almd_time._parts(ts); return `${String(y).padStart(4,"0")}-${String(m).padStart(2,"0")}-${String(d).padStart(2,"0")}T${String(h).padStart(2,"0")}:${String(mi).padStart(2,"0")}:${String(s).padStart(2,"0")}Z`; },
  from_parts(y, m, d, h, min, s) { return Math.floor(Date.UTC(y, m - 1, d, h, min, s) / 1000); },
};
"#;

// ──────────────────────────────── http ────────────────────────────────

const MOD_HTTP_TS: &str = r#"const __almd_http = {
  async serve(port: number, handler: (req: any) => any): Promise<void> { const server = Deno.serve({ port }, async (request: Request) => { const url = new URL(request.url); const method = request.method; const path = url.pathname; const body = method === "POST" || method === "PUT" ? await request.text() : ""; const headers: Record<string, string> = {}; request.headers.forEach((v: string, k: string) => { headers[k] = v; }); const req = { method, path, body, headers }; const res = handler(req); return new Response(res.body, { status: res.status, headers: res.headers || {} }); }); },
  response(status: number, body: string): any { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status: number, body: string): any { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status: number, body: string, headers: any): any { const h: Record<string, string> = {}; if (headers instanceof Map) { headers.forEach((v: string, k: string) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url: string): Promise<string> { const r = await fetch(url); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async post(url: string, body: string): Promise<string> { const r = await fetch(url, { method: "POST", body, headers: { "content-type": "application/json" } }); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async get_with_headers(url: string, headers: Map<string, string>): Promise<string> { const h: Record<string, string> = {}; headers.forEach((v, k) => { h[k] = v; }); const r = await fetch(url, { headers: h }); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
  async request(method: string, url: string, body: string, headers: Map<string, string>): Promise<string> { const h: Record<string, string> = {}; headers.forEach((v, k) => { h[k] = v; }); const opts: any = { method, headers: h }; if (body) opts.body = body; const r = await fetch(url, opts); if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.text(); },
};
"#;

const MOD_HTTP_JS: &str = r#"const __almd_http = {
  async serve(port, handler) { const http = require("http"); const server = http.createServer(async (req, res) => { let body = ""; req.on("data", (c) => { body += c; }); req.on("end", () => { const r = handler({ method: req.method, path: req.url, body, headers: req.headers || {} }); const headers = r.headers || {}; res.writeHead(r.status, headers); res.end(r.body); }); }); server.listen(port); },
  response(status, body) { return { status, body, headers: { "content-type": "text/plain" } }; },
  json(status, body) { return { status, body, headers: { "content-type": "application/json" } }; },
  with_headers(status, body, headers) { const h = {}; if (headers instanceof Map) { headers.forEach((v, k) => { h[k] = v; }); } else { Object.assign(h, headers); } return { status, body, headers: h }; },
  async get(url) { return new Promise((resolve, reject) => { const m = url.startsWith("https") ? require("https") : require("http"); m.get(url, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }).on("error", reject); }); },
  async post(url, body) { return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); const req = m.request({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, method: "POST", headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) } }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }); req.on("error", reject); req.write(body); req.end(); }); },
  async get_with_headers(url, headers) { const h = {}; headers.forEach((v, k) => { h[k] = v; }); return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); m.get({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, headers: h }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }).on("error", reject); }); },
  async request(method, url, body, headers) { const h = {}; headers.forEach((v, k) => { h[k] = v; }); return new Promise((resolve, reject) => { const u = new URL(url); const m = u.protocol === "https:" ? require("https") : require("http"); const req = m.request({ hostname: u.hostname, port: u.port, path: u.pathname + u.search, method, headers: h }, (r) => { let d = ""; r.on("data", (c) => d += c); r.on("end", () => r.statusCode >= 400 ? reject(new Error("HTTP " + r.statusCode)) : resolve(d)); }); req.on("error", reject); if (body) req.write(body); req.end(); }); },
};
"#;

// ──────────────────────────────── helpers ────────────────────────────────

const HELPERS_TS: &str = r#"function __bigop(op: string, a: any, b: any): any {
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
function __assert_throws(fn: () => any, expectedMsg: string): void {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
"#;

const HELPERS_JS: &str = r#"function __bigop(op, a, b) {
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
function __assert_throws(fn, expectedMsg) {
  try { fn(); throw new Error("Expected error but succeeded with: " + fn); }
  catch (e) { if (e instanceof Error && e.message === expectedMsg) return; throw e; }
}
"#;

// ──────────────────────────────── result ────────────────────────────────

// Result erasure: ok(x) → x, err(e) → throw. These functions only see ok values
// at runtime. Functions like unwrap_or/is_ok? are identity/constant in TS because
// err values are already thrown before reaching these calls.
const MOD_RESULT_TS: &str = r#"function __almd_result_unwrap_or<A>(v: A, _d: A): A { return v; }
function __almd_result_unwrap_or_else<A>(v: A, _f: (e: any) => A): A { return v; }
function __almd_result_is_ok(_v: any): boolean { return true; }
function __almd_result_is_err(_v: any): boolean { return false; }
function __almd_result_to_option<A>(v: A): A | null { return v; }
function __almd_result_to_err_option(_v: any): any { return null; }
"#;

const MOD_RESULT_JS: &str = r#"function __almd_result_unwrap_or(v, _d) { return v; }
function __almd_result_unwrap_or_else(v, _f) { return v; }
function __almd_result_is_ok(_v) { return true; }
function __almd_result_is_err(_v) { return false; }
function __almd_result_to_option(v) { return v; }
function __almd_result_to_err_option(_v) { return null; }
"#;

// ──────────────────────────────── error ────────────────────────────────

// Error module: context/message are mostly no-ops in TS due to result erasure.
// Errors are thrown as exceptions, so only `chain` does real work.
const MOD_ERROR_TS: &str = r#"const __almd_error = {
  chain(outer: string, cause: string): string { return outer + ": " + cause; },
  message(_r: any): string { return ""; },
};
"#;

const MOD_ERROR_JS: &str = r#"const __almd_error = {
  chain(outer, cause) { return outer + ": " + cause; },
  message(_r) { return ""; },
};
"#;

// ──────────────────────────────── datetime ────────────────────────────────

const MOD_DATETIME_TS: &str = r#"const __almd_datetime = {
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

const MOD_DATETIME_JS: &str = r#"const __almd_datetime = {
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

const MOD_TESTING_TS: &str = r#"const __almd_testing = {
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

const MOD_TESTING_JS: &str = r#"const __almd_testing = {
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

const MOD_CRYPTO_TS: &str = r#"const __almd_crypto = {
  random_bytes(n: number): number[] {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf);
  },
  random_hex(n: number): string {
    const buf = new Uint8Array(n);
    crypto.getRandomValues(buf);
    return Array.from(buf).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_sha256(key: string, data: string): Promise<string> {
    const enc = new TextEncoder();
    const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const sig = await crypto.subtle.sign("HMAC", k, enc.encode(data));
    return Array.from(new Uint8Array(sig)).map(b => b.toString(16).padStart(2, "0")).join("");
  },
  async hmac_verify(key: string, data: string, signature: string): Promise<boolean> {
    const computed = await __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    let diff = 0;
    for (let i = 0; i < computed.length; i++) diff |= computed.charCodeAt(i) ^ signature.charCodeAt(i);
    return diff === 0;
  },
};
"#;

const MOD_CRYPTO_JS: &str = r#"const __almd_crypto = {
  random_bytes(n) {
    return Array.from(require("crypto").randomBytes(n));
  },
  random_hex(n) {
    return require("crypto").randomBytes(n).toString("hex");
  },
  hmac_sha256(key, data) {
    const h = require("crypto").createHmac("sha256", key);
    h.update(data);
    return h.digest("hex");
  },
  hmac_verify(key, data, signature) {
    const computed = __almd_crypto.hmac_sha256(key, data);
    if (computed.length !== signature.length) return false;
    return require("crypto").timingSafeEqual(Buffer.from(computed), Buffer.from(signature));
  },
};
"#;

const MOD_UUID_TS: &str = r#"const __almd_uuid = {
  v4(): string {
    return crypto.randomUUID();
  },
  v5(namespace: string, name: string): string {
    // v5 requires SHA-1 — simplified implementation
    const data = namespace.replace(/-/g, "") + name;
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      hash = ((hash << 5) - hash + data.charCodeAt(i)) | 0;
    }
    const hex = Math.abs(hash).toString(16).padStart(32, "0").slice(0, 32);
    return `${hex.slice(0,8)}-${hex.slice(8,12)}-5${hex.slice(13,16)}-${(parseInt(hex.slice(16,18),16) & 0x3F | 0x80).toString(16)}${hex.slice(18,20)}-${hex.slice(20,32)}`;
  },
  parse(s: string): string {
    if (!__almd_uuid.is_valid(s)) throw new Error(`invalid UUID: ${s}`);
    return s.toLowerCase();
  },
  is_valid(s: string): boolean {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());
  },
  nil(): string { return "00000000-0000-0000-0000-000000000000"; },
  version(s: string): number {
    if (!__almd_uuid.is_valid(s)) throw new Error(`invalid UUID: ${s}`);
    return parseInt(s.charAt(14), 16);
  },
};
"#;

const MOD_UUID_JS: &str = r#"const __almd_uuid = {
  v4() {
    return require("crypto").randomUUID();
  },
  v5(namespace, name) {
    const data = namespace.replace(/-/g, "") + name;
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      hash = ((hash << 5) - hash + data.charCodeAt(i)) | 0;
    }
    const hex = Math.abs(hash).toString(16).padStart(32, "0").slice(0, 32);
    return `${hex.slice(0,8)}-${hex.slice(8,12)}-5${hex.slice(13,16)}-${(parseInt(hex.slice(16,18),16) & 0x3F | 0x80).toString(16)}${hex.slice(18,20)}-${hex.slice(20,32)}`;
  },
  parse(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return s.toLowerCase();
  },
  is_valid(s) {
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s.trim());
  },
  nil() { return "00000000-0000-0000-0000-000000000000"; },
  version(s) {
    if (!__almd_uuid.is_valid(s)) throw new Error("invalid UUID: " + s);
    return parseInt(s.charAt(14), 16);
  },
};
"#;

const MOD_LOG_TS: &str = r#"const __almd_log = {
  debug(msg: string): void { console.error(`[DEBUG] ${msg}`); },
  info(msg: string): void { console.error(`[INFO] ${msg}`); },
  warn(msg: string): void { console.error(`[WARN] ${msg}`); },
  error(msg: string): void { console.error(`[ERROR] ${msg}`); },
  debug_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[DEBUG] ${msg} ${kv}`); },
  info_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[INFO] ${msg} ${kv}`); },
  warn_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[WARN] ${msg} ${kv}`); },
  error_with(msg: string, fields: [string, string][]): void { const kv = fields.map(([k,v]) => `${k}=${v}`).join(" "); console.error(`[ERROR] ${msg} ${kv}`); },
};
"#;

const MOD_LOG_JS: &str = r#"const __almd_log = {
  debug(msg) { console.error("[DEBUG] " + msg); },
  info(msg) { console.error("[INFO] " + msg); },
  warn(msg) { console.error("[WARN] " + msg); },
  error(msg) { console.error("[ERROR] " + msg); },
  debug_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[DEBUG] " + msg + " " + kv); },
  info_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[INFO] " + msg + " " + kv); },
  warn_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[WARN] " + msg + " " + kv); },
  error_with(msg, fields) { var kv = fields.map(function(f) { return f[0] + "=" + f[1]; }).join(" "); console.error("[ERROR] " + msg + " " + kv); },
};
"#;

// ──────────────────────────────── Registry ────────────────────────────────

/// A runtime module with TS (Deno) and JS (Node.js) source variants.
pub struct RuntimeModule {
    pub name: &'static str,
    pub ts_source: &'static str,
    pub js_source: &'static str,
}

/// All stdlib runtime modules in emit order.
pub static ALL_MODULES: &[RuntimeModule] = &[
    RuntimeModule { name: "fs",      ts_source: MOD_FS_TS,      js_source: MOD_FS_JS },
    RuntimeModule { name: "string",  ts_source: MOD_STRING_TS,  js_source: MOD_STRING_JS },
    RuntimeModule { name: "list",    ts_source: MOD_LIST_TS,    js_source: MOD_LIST_JS },
    RuntimeModule { name: "map",     ts_source: MOD_MAP_TS,     js_source: MOD_MAP_JS },
    RuntimeModule { name: "int",     ts_source: MOD_INT_TS,     js_source: MOD_INT_JS },
    RuntimeModule { name: "float",   ts_source: MOD_FLOAT_TS,   js_source: MOD_FLOAT_JS },
    RuntimeModule { name: "path",    ts_source: MOD_PATH_TS,    js_source: MOD_PATH_JS },
    RuntimeModule { name: "json",    ts_source: MOD_JSON_TS,    js_source: MOD_JSON_JS },
    RuntimeModule { name: "env",     ts_source: MOD_ENV_TS,     js_source: MOD_ENV_JS },
    RuntimeModule { name: "process", ts_source: MOD_PROCESS_TS, js_source: MOD_PROCESS_JS },
    RuntimeModule { name: "math",    ts_source: MOD_MATH_TS,    js_source: MOD_MATH_JS },
    RuntimeModule { name: "random",  ts_source: MOD_RANDOM_TS,  js_source: MOD_RANDOM_JS },
    RuntimeModule { name: "regex",   ts_source: MOD_REGEX_TS,   js_source: MOD_REGEX_JS },
    RuntimeModule { name: "io",      ts_source: MOD_IO_TS,      js_source: MOD_IO_JS },
    RuntimeModule { name: "time",    ts_source: MOD_TIME_TS,    js_source: MOD_TIME_JS },
    RuntimeModule { name: "http",    ts_source: MOD_HTTP_TS,    js_source: MOD_HTTP_JS },
    RuntimeModule { name: "result",  ts_source: MOD_RESULT_TS,  js_source: MOD_RESULT_JS },
    RuntimeModule { name: "error",    ts_source: MOD_ERROR_TS,    js_source: MOD_ERROR_JS },
    RuntimeModule { name: "datetime", ts_source: MOD_DATETIME_TS, js_source: MOD_DATETIME_JS },
    RuntimeModule { name: "testing",  ts_source: MOD_TESTING_TS,  js_source: MOD_TESTING_JS },
    RuntimeModule { name: "crypto",   ts_source: MOD_CRYPTO_TS,   js_source: MOD_CRYPTO_JS },
    RuntimeModule { name: "uuid",     ts_source: MOD_UUID_TS,     js_source: MOD_UUID_JS },
    RuntimeModule { name: "log",      ts_source: MOD_LOG_TS,      js_source: MOD_LOG_JS },
];

/// Compose the full runtime string (backwards compatible with --target ts/js).
pub fn full_runtime(js_mode: bool) -> String {
    let mut out = String::with_capacity(if js_mode { 16384 } else { 20480 });
    out.push_str(if js_mode { PREAMBLE_JS } else { PREAMBLE_TS });
    for m in ALL_MODULES {
        out.push_str(if js_mode { m.js_source } else { m.ts_source });
    }
    out.push_str(if js_mode { HELPERS_JS } else { HELPERS_TS });
    out.push_str(EPILOGUE);
    out
}

/// Get the source for a single stdlib module.
pub fn get_module_source(name: &str, js_mode: bool) -> Option<&'static str> {
    ALL_MODULES.iter().find(|m| m.name == name).map(|m| {
        if js_mode { m.js_source } else { m.ts_source }
    })
}

/// Get the helpers source (always needed).
pub fn get_helpers_source(js_mode: bool) -> &'static str {
    if js_mode { HELPERS_JS } else { HELPERS_TS }
}

/// Get the preamble (platform-specific setup code).
pub fn get_preamble(js_mode: bool) -> &'static str {
    if js_mode { PREAMBLE_JS } else { PREAMBLE_TS }
}
