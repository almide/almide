use crate::ast::*;

const RUNTIME: &str = r#"// ---- Almide Runtime ----
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
const __env = {
  unix_timestamp(): number { return Math.floor(Date.now() / 1000); },
  args(): string[] { return Deno.args; },
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

const RUNTIME_JS: &str = r#"// ---- Almide Runtime (JS) ----
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
const __map = {
  new_() { return new Map(); },
  get(m, k) { return m.has(k) ? m.get(k) : null; },
  set(m, k, v) { const r = new Map(m); r.set(k, v); return r; },
  contains(m, k) { return m.has(k); },
  remove(m, k) { const r = new Map(m); r.delete(k); return r; },
  keys(m) { return [...m.keys()].sort(); },
  values(m) { return [...m.values()]; },
  len(m) { return m.size; },
  entries(m) { return [...m.entries()].map(([k, v]) => [k, v]); },
  from_list(xs, f) { const r = new Map(); for (const x of xs) { const [k, v] = f(x); r.set(k, v); } return r; },
};
const __env = {
  unix_timestamp() { return Math.floor(Date.now() / 1000); },
  args() { return process.argv.slice(2); },
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

struct TsEmitter {
    out: String,
    js_mode: bool,
}

impl TsEmitter {
    fn new() -> Self {
        Self { out: String::new(), js_mode: false }
    }

    fn emit_program(&mut self, prog: &Program, modules: &[(String, Program)]) {
        if self.js_mode {
            self.out.push_str(RUNTIME_JS);
        } else {
            self.out.push_str(RUNTIME);
        }
        self.out.push('\n');

        // Emit imported modules as namespace objects
        for (mod_name, mod_prog) in modules {
            self.emit_user_module(mod_name, mod_prog);
        }

        if let Some(module) = &prog.module {
            if let Decl::Module { path } = module {
                self.out.push_str(&format!("// module: {}\n", path.join(".")));
            }
        }

        let mut has_main = false;
        for decl in &prog.decls {
            if let Decl::Fn { name, .. } = decl {
                if name == "main" {
                    has_main = true;
                }
            }
            self.out.push_str(&self.gen_decl(decl));
            self.out.push_str("\n\n");
        }

        if has_main {
            self.out.push_str("// ---- Entry Point ----\n");
            if self.js_mode {
                self.out.push_str("try { main([\"app\", ...process.argv.slice(2)]); } catch (e) { if (e instanceof Error) { console.error(e.message); process.exit(1); } throw e; }\n");
            } else {
                self.out.push_str("try { main([\"minigit\", ...Deno.args]); } catch (e) { if (e instanceof Error) { eprintln(e.message); Deno.exit(1); } throw e; }\n");
            }
        }
    }

    fn emit_user_module(&mut self, name: &str, prog: &Program) {
        self.out.push_str(&format!("// module: {}\n", name));
        self.out.push_str(&format!("const {} = (() => {{\n", name));

        for decl in &prog.decls {
            match decl {
                Decl::Fn { .. } => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push('\n');
                }
                Decl::Type { .. } => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push('\n');
                }
                _ => {}
            }
        }

        // Export all functions
        let fn_names: Vec<&str> = prog.decls.iter().filter_map(|d| {
            if let Decl::Fn { name, .. } = d { Some(name.as_str()) } else { None }
        }).collect();
        self.out.push_str(&format!("  return {{ {} }};\n", fn_names.join(", ")));
        self.out.push_str("})();\n\n");
    }

    fn gen_decl(&self, decl: &Decl) -> String {
        match decl {
            Decl::Module { path } => format!("// module: {}", path.join(".")),
            Decl::Import { path, .. } => format!("// import: {}", path.join(".")),
            Decl::Type { name, ty, .. } => {
                if self.js_mode {
                    // In JS mode, skip pure type decls but still generate variant constructors
                    if matches!(ty, TypeExpr::Variant { .. }) {
                        self.gen_type_decl(name, ty)
                    } else {
                        format!("// type: {}", name)
                    }
                } else {
                    self.gen_type_decl(name, ty)
                }
            }
            Decl::Fn { name, params, return_type, body, r#async, .. } => {
                self.gen_fn_decl(name, params, return_type, body, r#async.unwrap_or(false))
            }
            Decl::Trait { name, .. } => format!("// trait {}", name),
            Decl::Impl { trait_, for_, methods } => {
                let mut lines = vec![format!("// impl {} for {}", trait_, for_)];
                for m in methods {
                    lines.push(self.gen_decl(m));
                }
                lines.join("\n")
            }
            Decl::Test { name, body } => {
                let body_str = self.gen_expr(body);
                if self.js_mode {
                    format!("// test: {}\n(() => {})();", name, body_str)
                } else {
                    format!("Deno.test({}, () => {});", Self::json_string(name), body_str)
                }
            }
            Decl::Strict { mode } => format!("// strict {}", mode),
        }
    }

    fn gen_type_decl(&self, name: &str, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("  {}: {};", f.name, self.gen_type_expr(&f.ty)))
                    .collect();
                format!("interface {} {{\n{}\n}}", name, fs.join("\n"))
            }
            TypeExpr::Variant { cases } => {
                let mut lines = vec![format!("// variant type {}", name)];
                for case in cases {
                    match case {
                        VariantCase::Unit { name: cname } => {
                            lines.push(format!("function {}() {{ return {{ tag: {} }}; }}", cname, Self::json_string(cname)));
                        }
                        VariantCase::Tuple { name: cname, fields } => {
                            let params: Vec<String> = fields.iter().enumerate()
                                .map(|(i, _)| format!("_{}", i))
                                .collect();
                            let obj_fields: Vec<String> = fields.iter().enumerate()
                                .map(|(i, _)| format!("_{}: _{}", i, i))
                                .collect();
                            lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                                cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                        }
                        VariantCase::Record { name: cname, fields } => {
                            let params: Vec<String> = fields.iter()
                                .map(|f| f.name.clone())
                                .collect();
                            let obj_fields: Vec<String> = fields.iter()
                                .map(|f| format!("{}: {}", f.name, f.name))
                                .collect();
                            lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                                cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                        }
                    }
                }
                lines.join("\n")
            }
            TypeExpr::Newtype { inner } => {
                format!("type {} = {} & {{ readonly __brand: \"{}\" }};", name, self.gen_type_expr(inner), name)
            }
            _ => format!("type {} = {};", name, self.gen_type_expr(ty)),
        }
    }

    fn gen_type_expr(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Simple { name } => Self::map_type_name(name).to_string(),
            TypeExpr::Generic { name, args } => {
                match name.as_str() {
                    "List" => format!("{}[]", self.gen_type_expr(&args[0])),
                    "Map" => format!("Map<{}>", args.iter().map(|a| self.gen_type_expr(a)).collect::<Vec<_>>().join(", ")),
                    "Set" => format!("Set<{}>", self.gen_type_expr(&args[0])),
                    "Result" => self.gen_type_expr(&args[0]),
                    "Option" => format!("{} | null", self.gen_type_expr(&args[0])),
                    _ => format!("{}<{}>", name, args.iter().map(|a| self.gen_type_expr(a)).collect::<Vec<_>>().join(", ")),
                }
            }
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_type_expr(&f.ty)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            TypeExpr::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().enumerate()
                    .map(|(i, p)| format!("_{}: {}", i, self.gen_type_expr(p)))
                    .collect();
                format!("({}) => {}", ps.join(", "), self.gen_type_expr(ret))
            }
            TypeExpr::Newtype { inner } => self.gen_type_expr(inner),
            TypeExpr::Variant { .. } => "any".to_string(),
        }
    }

    fn map_type_name(name: &str) -> &str {
        match name {
            "Int" => "number",
            "Float" => "number",
            "String" => "string",
            "Bool" => "boolean",
            "Unit" => "void",
            "Path" => "string",
            other => other,
        }
    }

    fn gen_fn_decl(&self, name: &str, params: &[Param], ret_type: &TypeExpr, body: &Expr, is_async: bool) -> String {
        let async_ = if is_async { "async " } else { "" };
        let sname = Self::sanitize(name);
        let params_str: Vec<String> = params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                if self.js_mode {
                    Self::sanitize(&p.name)
                } else {
                    format!("{}: {}", Self::sanitize(&p.name), self.gen_type_expr(&p.ty))
                }
            })
            .collect();
        let ret_str = if self.js_mode { String::new() } else { format!(": {}", self.gen_type_expr(ret_type)) };
        let body_str = self.gen_expr(body);

        match body {
            Expr::Block { .. } => {
                format!("{}function {}({}){} {}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
            Expr::DoBlock { .. } => {
                format!("{}function {}({}){} {{\n{}\n}}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
            _ => {
                format!("{}function {}({}){} {{\n  return {};\n}}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
        }
    }

    fn gen_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<i128>() {
                    if n > 9007199254740991 || n < -9007199254740991 {
                        return format!("{}n", raw);
                    }
                }
                raw.clone()
            }
            Expr::Float { value } => format!("{}", value),
            Expr::String { value } => Self::json_string(value),
            Expr::InterpolatedString { value } => format!("`{}`", value),
            Expr::Bool { value } => format!("{}", value),
            Expr::Ident { name } => Self::sanitize(name),
            Expr::TypeName { name } => name.clone(),
            Expr::Unit => "undefined".to_string(),
            Expr::None => "null".to_string(),
            Expr::Some { expr } => self.gen_expr(expr),
            Expr::Ok { expr } => self.gen_expr(expr),
            Expr::Err { expr } => self.gen_err(expr),
            Expr::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            Expr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Expr::SpreadRecord { base, fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ ...{}, {} }}", self.gen_expr(base), fs.join(", "))
            }
            Expr::Call { callee, args } => self.gen_call(callee, args),
            Expr::Member { object, field } => {
                let obj = self.gen_expr(object);
                format!("{}.{}", Self::map_module(&obj), Self::sanitize(field))
            }
            Expr::Pipe { left, right } => self.gen_pipe(left, right),
            Expr::If { cond, then, else_ } => {
                let t = if Self::needs_iife(then) {
                    format!("(() => {})()", self.gen_expr(then))
                } else {
                    self.gen_expr(then)
                };
                let e = if Self::needs_iife(else_) {
                    format!("(() => {})()", self.gen_expr(else_))
                } else {
                    self.gen_expr(else_)
                };
                format!("({} ? {} : {})", self.gen_expr(cond), t, e)
            }
            Expr::Match { subject, arms } => self.gen_match(subject, arms),
            Expr::Block { stmts, expr: final_expr } => {
                self.gen_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::DoBlock { stmts, expr: final_expr } => {
                self.gen_do_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::ForIn { var, iterable, body } => {
                let iter_str = self.gen_expr(iterable);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                format!("for (const {} of {}) {{\n{}\n}}", Self::sanitize(var), iter_str, stmts_str.join("\n"))
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                format!("(({}) => {})", ps.join(", "), self.gen_expr(body))
            }
            Expr::Binary { op, left, right } => self.gen_binary(op, left, right),
            Expr::Unary { op, operand } => {
                if op == "not" {
                    format!("!({})", self.gen_expr(operand))
                } else {
                    format!("{}{}", op, self.gen_expr(operand))
                }
            }
            Expr::Paren { expr } => format!("({})", self.gen_expr(expr)),
            Expr::Try { expr } => self.gen_expr(expr),
            Expr::Await { expr } => format!("await {}", self.gen_expr(expr)),
            Expr::Hole => if self.js_mode { "null /* hole */".to_string() } else { "null as any /* hole */".to_string() },
            Expr::Todo { message } => format!("(() => {{ throw new Error({}); }})()", Self::json_string(message)),
            Expr::Placeholder => "__placeholder__".to_string(),
        }
    }

    fn gen_err(&self, expr: &Expr) -> String {
        match expr {
            Expr::Call { callee, args } => {
                let callee_str = if let Expr::TypeName { name } = callee.as_ref() {
                    Self::pascal_to_message(name)
                } else {
                    self.gen_expr(callee)
                };
                let arg = if !args.is_empty() { self.gen_expr(&args[0]) } else { "\"\"".to_string() };
                format!("(() => {{ throw new Error({} + \": \" + {}); }})()", Self::json_string(&callee_str), arg)
            }
            Expr::TypeName { name } => {
                let msg = Self::pascal_to_message(name);
                format!("(() => {{ throw new Error({}); }})()", Self::json_string(&msg))
            }
            Expr::String { value } => {
                format!("(() => {{ throw new Error({}); }})()", Self::json_string(value))
            }
            _ => {
                format!("(() => {{ throw new Error(String({})); }})()", self.gen_expr(expr))
            }
        }
    }

    fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
        match method {
            // string methods
            "trim" | "split" | "join" | "pad_left" | "starts_with" | "starts_with_qm_"
            | "ends_with_qm_" | "slice" | "to_bytes" | "contains" | "to_upper" | "to_lower"
            | "to_int" | "replace" | "char_at" | "lines" => Some("__string"),
            // list methods
            "get" | "get_or" | "sort" | "reverse" | "each" | "map" | "filter" | "find" | "fold" | "any" | "all" => Some("__list"),
            // int methods
            "to_string" | "to_hex" => Some("__int"),
            // map methods
            "keys" | "values" | "entries" => Some("__map"),
            // len / contains are ambiguous — prioritize based on context
            // "len" and "contains" exist in both string and list; handled separately
            _ => None,
        }
    }

    fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // UFCS: expr.method(args) => __module.method(expr, args)
        if let Expr::Member { object, field } = callee {
            if let Expr::Ident { name } = object.as_ref() {
                let is_module = matches!(name.as_str(), "string" | "list" | "int" | "float" | "fs" | "env" | "map");
                if !is_module {
                    // UFCS: non-module receiver
                    if let Some(module) = Self::resolve_ufcs_module(field) {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("{}.{}({})", module, Self::sanitize(field), all_args.join(", "));
                    }
                    // len/contains: try both, default to list for identifiers
                    if field == "len" || field == "contains" {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        // Use list by default for ident receivers; string.len works the same way
                        return format!("__list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            } else {
                // Non-ident object (e.g. call result, member chain)
                let module_name = if let Expr::Member { object: inner_obj, .. } = object.as_ref() {
                    if let Expr::Ident { name } = inner_obj.as_ref() {
                        matches!(name.as_str(), "string" | "list" | "int" | "float" | "fs" | "env")
                    } else { false }
                } else { false };

                if !module_name {
                    if let Some(module) = Self::resolve_ufcs_module(field) {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("{}.{}({})", module, Self::sanitize(field), all_args.join(", "));
                    }
                    if field == "len" || field == "contains" {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("__list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            }
        }

        let callee_str = self.gen_expr(callee);
        // Special case: assert_eq(x, err(e))
        if callee_str == "assert_eq" && args.len() == 2 {
            if let Expr::Err { expr: err_expr } = &args[1] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[0]), self.gen_err_message(err_expr));
            }
            if let Expr::Err { expr: err_expr } = &args[0] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[1]), self.gen_err_message(err_expr));
            }
        }
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        format!("{}({})", callee_str, args_str.join(", "))
    }

    fn gen_err_message(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value } => Self::json_string(value),
            Expr::Call { callee, args } if matches!(callee.as_ref(), Expr::TypeName { .. }) => {
                if let Expr::TypeName { name } = callee.as_ref() {
                    let msg = Self::pascal_to_message(name);
                    format!("{} + \": \" + {}", Self::json_string(&msg), self.gen_expr(&args[0]))
                } else {
                    format!("String({})", self.gen_expr(expr))
                }
            }
            Expr::TypeName { name } => Self::json_string(&Self::pascal_to_message(name)),
            _ => format!("String({})", self.gen_expr(expr)),
        }
    }

    fn gen_binary(&self, op: &str, left: &Expr, right: &Expr) -> String {
        let l = self.gen_expr(left);
        let r = self.gen_expr(right);
        let has_float = Self::expr_has_float(left) || Self::expr_has_float(right);
        match op {
            "and" => format!("({} && {})", l, r),
            "or" => format!("({} || {})", l, r),
            "==" => format!("__deep_eq({}, {})", l, r),
            "!=" => format!("!__deep_eq({}, {})", l, r),
            "++" => format!("__concat({}, {})", l, r),
            "^" if !has_float => format!("__bigop(\"^\", {}, {})", l, r),
            "*" if !has_float => format!("__bigop(\"*\", {}, {})", l, r),
            "%" if !has_float => format!("__bigop(\"%\", {}, {})", l, r),
            "/" if !has_float => format!("__div({}, {})", l, r),
            _ => format!("({} {} {})", l, op, r),
        }
    }

    /// Check if an expression involves Float values (heuristic for JS codegen).
    fn expr_has_float(expr: &Expr) -> bool {
        match expr {
            Expr::Float { .. } => true,
            Expr::Binary { left, right, .. } => {
                Self::expr_has_float(left) || Self::expr_has_float(right)
            }
            Expr::Paren { expr: inner } => Self::expr_has_float(inner),
            Expr::Call { callee, .. } => {
                // float.xxx() calls return Float
                if let Expr::Member { object, .. } = callee.as_ref() {
                    if let Expr::Ident { name } = object.as_ref() {
                        return name == "float";
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn gen_pipe(&self, left: &Expr, right: &Expr) -> String {
        let l = self.gen_expr(left);
        match right {
            Expr::Call { callee, args } => {
                let has_placeholder = args.iter().any(|a| matches!(a, Expr::Placeholder));
                if has_placeholder {
                    let mapped_args: Vec<String> = args.iter().map(|a| {
                        if matches!(a, Expr::Placeholder) { l.clone() } else { self.gen_expr(a) }
                    }).collect();
                    let callee_str = self.gen_expr(callee);
                    format!("{}({})", callee_str, mapped_args.join(", "))
                } else {
                    let callee_str = self.gen_expr(callee);
                    let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                    if args_str.is_empty() {
                        format!("{}({})", callee_str, l)
                    } else {
                        format!("{}({}, {})", callee_str, l, args_str.join(", "))
                    }
                }
            }
            _ => format!("{}({})", self.gen_expr(right), l),
        }
    }

    fn gen_match(&self, subject: &Expr, arms: &[MatchArm]) -> String {
        let subj = self.gen_expr(subject);
        let tmp = "__m";

        let err_arm = arms.iter().find(|a| matches!(&a.pattern, Pattern::Err { .. }));

        if let Some(err_arm) = err_arm {
            let ok_arms: Vec<&MatchArm> = arms.iter().filter(|a| !matches!(&a.pattern, Pattern::Err { .. })).collect();
            let err_body = if Self::needs_iife(&err_arm.body) {
                format!("(() => {})()", self.gen_expr(&err_arm.body))
            } else {
                self.gen_expr(&err_arm.body)
            };
            let err_binding = if let Pattern::Err { inner } = &err_arm.pattern {
                if let Pattern::Ident { name } = inner.as_ref() {
                    Some(name.clone())
                } else { None }
            } else { None };

            let catch_return = if let Some(ref binding) = err_binding {
                format!("const {} = __e instanceof Error ? __e.message : String(__e); return {};", binding, err_body)
            } else {
                format!("return {};", err_body)
            };

            let mut lines = vec![format!("(() => {{ let {}; try {{ {} = {}; }} catch (__e) {{ {} }}", tmp, tmp, subj, catch_return)];
            for arm in &ok_arms {
                self.emit_match_arm(&mut lines, tmp, arm);
            }
            lines.push("  throw new Error(\"match exhausted\");".to_string());
            lines.push("})()".to_string());
            return lines.join("\n");
        }

        let mut lines = vec![format!("(({}) => {{", tmp)];
        for arm in arms {
            self.emit_match_arm(&mut lines, tmp, arm);
        }
        lines.push("  throw new Error(\"match exhausted\");".to_string());
        lines.push(format!("}})({})", subj));
        lines.join("\n")
    }

    fn emit_match_arm(&self, lines: &mut Vec<String>, tmp: &str, arm: &MatchArm) {
        let (cond, bindings) = self.gen_pattern_cond(tmp, &arm.pattern);
        let bind_str: String = bindings.iter()
            .map(|b| format!("    const {} = {};", b.0, b.1))
            .collect::<Vec<_>>()
            .join("\n");
        let body_str = if Self::needs_iife(&arm.body) {
            format!("(() => {})()", self.gen_expr(&arm.body))
        } else {
            self.gen_expr(&arm.body)
        };

        if let Some(guard) = &arm.guard {
            let guard_str = self.gen_expr(guard);
            if !bind_str.is_empty() {
                lines.push(format!("  {{ {}\n    if ({} && {}) return {}; }}", bind_str, cond, guard_str, body_str));
            } else {
                lines.push(format!("  if ({} && {}) return {};", cond, guard_str, body_str));
            }
        } else if !bind_str.is_empty() {
            lines.push(format!("  if ({}) {{ {}\n    return {}; }}", cond, bind_str, body_str));
        } else {
            lines.push(format!("  if ({}) return {};", cond, body_str));
        }
    }

    fn gen_pattern_cond(&self, expr: &str, pattern: &Pattern) -> (String, Vec<(String, String)>) {
        match pattern {
            Pattern::Wildcard => ("true".to_string(), vec![]),
            Pattern::Ident { name } => ("true".to_string(), vec![(name.clone(), expr.to_string())]),
            Pattern::Literal { value } => {
                (format!("{} === {}", expr, self.gen_expr(value)), vec![])
            }
            Pattern::None => (format!("{} === null", expr), vec![]),
            Pattern::Some { inner } => {
                let (inner_cond, bindings) = self.gen_pattern_cond(expr, inner);
                let cond = if inner_cond == "true" {
                    format!("{} !== null", expr)
                } else {
                    format!("{} !== null && {}", expr, inner_cond)
                };
                (cond, bindings)
            }
            Pattern::Ok { inner } => self.gen_pattern_cond(expr, inner),
            Pattern::Err { .. } => ("false".to_string(), vec![]),
            Pattern::Constructor { name, args } => {
                if args.is_empty() {
                    (format!("{}?.tag === {}", expr, Self::json_string(name)), vec![])
                } else {
                    let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                    let mut bindings = vec![];
                    for (i, arg) in args.iter().enumerate() {
                        let sub_expr = format!("{}._{}", expr, i);
                        let (sub_cond, sub_bindings) = self.gen_pattern_cond(&sub_expr, arg);
                        if sub_cond != "true" {
                            conds.push(sub_cond);
                        }
                        bindings.extend(sub_bindings);
                    }
                    (conds.join(" && "), bindings)
                }
            }
            Pattern::RecordPattern { name, fields } => {
                let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                let mut bindings = vec![];
                for f in fields {
                    let field_expr = format!("{}.{}", expr, f.name);
                    if let Some(p) = &f.pattern {
                        let (sub_cond, sub_bindings) = self.gen_pattern_cond(&field_expr, p);
                        if sub_cond != "true" {
                            conds.push(sub_cond);
                        }
                        bindings.extend(sub_bindings);
                    } else {
                        bindings.push((f.name.clone(), field_expr));
                    }
                }
                (conds.join(" && "), bindings)
            }
        }
    }

    fn gen_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>, indent: usize) -> String {
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();

        // Detect let-match inlining pattern for Result erasure
        if let Some(fe) = final_expr {
            if let Expr::Match { subject, arms } = fe {
                if let Expr::Ident { name: subj_name } = subject.as_ref() {
                    if !stmts.is_empty() {
                        if let Stmt::Let { name: last_name, value, .. } = &stmts[stmts.len() - 1] {
                            if last_name == subj_name && arms.iter().any(|a| matches!(&a.pattern, Pattern::Err { .. })) {
                                for i in 0..stmts.len() - 1 {
                                    lines.push(format!("{}{}", ind, self.gen_stmt(&stmts[i])));
                                }
                                // Inline value into match subject
                                let inlined_match = self.gen_match(value, arms);
                                lines.push(format!("{}return {};", ind, inlined_match));
                                return format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent));
                            }
                        }
                    }
                }
            }
        }

        for stmt in stmts {
            lines.push(format!("{}{}", ind, self.gen_stmt(stmt)));
        }
        if let Some(fe) = final_expr {
            match fe {
                Expr::DoBlock { stmts: ds, expr: de } => {
                    lines.push(format!("{}{}", ind, self.gen_do_block(ds, de.as_deref(), indent + 1)));
                }
                _ => {
                    lines.push(format!("{}return {};", ind, self.gen_expr(fe)));
                }
            }
        }
        format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
    }

    fn gen_do_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>, indent: usize) -> String {
        let has_guard = stmts.iter().any(|s| matches!(s, Stmt::Guard { .. }));
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();

        for stmt in stmts {
            if has_guard {
                if let Stmt::Guard { cond, else_ } = stmt {
                    let c = self.gen_expr(cond);
                    if Self::is_unit(else_) {
                        lines.push(format!("{}if (!({})) {{ break; }}", ind, c));
                    } else {
                        lines.push(format!("{}if (!({})) {{ return {}; }}", ind, c, self.gen_expr(else_)));
                    }
                    continue;
                }
            }
            lines.push(format!("{}{}", ind, self.gen_stmt(stmt)));
        }

        if has_guard {
            if let Some(fe) = final_expr {
                lines.push(format!("{}{};", ind, self.gen_expr(fe)));
            }
            format!("while (true) {{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        } else {
            if let Some(fe) = final_expr {
                lines.push(format!("{}return {};", ind, self.gen_expr(fe)));
            }
            format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        }
    }

    fn gen_stmt(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let { name, value, .. } => {
                format!("const {} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::LetDestructure { fields, value } => {
                format!("const {{ {} }} = {};", fields.join(", "), self.gen_expr(value))
            }
            Stmt::Var { name, value, .. } => {
                format!("let {} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::Assign { name, value } => {
                format!("{} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::Guard { cond, else_ } => {
                let c = self.gen_expr(cond);
                self.gen_guard_stmt(&c, else_)
            }
            Stmt::Expr { expr } => {
                format!("{};", self.gen_expr(expr))
            }
        }
    }

    fn gen_guard_stmt(&self, cond: &str, else_: &Expr) -> String {
        match else_ {
            Expr::Block { stmts, expr } | Expr::DoBlock { stmts, expr } => {
                let body_stmts: Vec<String> = stmts.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                let final_part = expr.as_ref()
                    .map(|e| format!("  return {};", self.gen_expr(e)))
                    .unwrap_or_default();
                let body = [body_stmts.join("\n"), final_part]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("if (!({})) {{\n{}\n}}", cond, body)
            }
            _ => format!("if (!({})) {{ return {}; }}", cond, self.gen_expr(else_)),
        }
    }

    // Helpers

    fn needs_iife(expr: &Expr) -> bool {
        matches!(expr, Expr::Block { .. } | Expr::DoBlock { .. })
    }

    fn is_unit(expr: &Expr) -> bool {
        match expr {
            Expr::Unit => true,
            Expr::Ok { expr } | Expr::Some { expr } => matches!(expr.as_ref(), Expr::Unit),
            _ => false,
        }
    }

    fn sanitize(name: &str) -> String {
        name.replace('?', "_qm_")
    }

    fn map_module(name: &str) -> String {
        match name {
            "fs" => "__fs".to_string(),
            "string" => "__string".to_string(),
            "list" => "__list".to_string(),
            "int" => "__int".to_string(),
            "float" => "__float".to_string(),
            "map" => "__map".to_string(),
            "env" => "__env".to_string(),
            other => other.to_string(),
        }
    }

    fn json_string(s: &str) -> String {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s))
    }

    fn pascal_to_message(name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
                result.push(c.to_lowercase().next().unwrap());
            } else if i == 0 {
                result.push(c.to_uppercase().next().unwrap());
            } else {
                result.push(c);
            }
        }
        result
    }
}

pub fn emit(program: &Program) -> String {
    emit_with_modules(program, &[])
}

pub fn emit_with_modules(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = TsEmitter::new();
    emitter.emit_program(program, modules);
    emitter.out
}

pub fn emit_js(program: &Program) -> String {
    emit_js_with_modules(program, &[])
}

pub fn emit_js_with_modules(program: &Program, modules: &[(String, Program)]) -> String {
    let mut emitter = TsEmitter::new();
    emitter.js_mode = true;
    emitter.emit_program(program, modules);
    emitter.out
}
