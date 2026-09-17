//! `almide build app.almd --target wasm --host js` (#2265): the JS host the
//! module needs to run in a page or under node, written by the compiler
//! itself next to the `.wasm` — `<mod>.js` (a dependency-free ES module) and
//! `<mod>.d.ts` — so the artifact is page-runnable without a hand-written
//! WASI stub, import object or String marshalling.
//!
//! What the host does:
//! - the `wasi_snapshot_preview1` imports the module actually names get a
//!   shim (`fd_write` → stdout/stderr, `proc_exit` → an [`AlmideExit`]
//!   throw, the clock/random/read floor); nothing else is linked, so nothing
//!   else is stubbed;
//! - every `@extern(wasm, "js", "name")` import is wired to
//!   `init(source, { js: { name } })`, with `String` args decoded from the
//!   block header before the user function runs and its return encoded;
//! - every `pub fn` becomes a wrapper marshalling `Int` ↔ `number` (range
//!   checked at ±2^53), `Float`, `Bool`, `String` and `Unit`; any other type
//!   is a compile-time refusal naming the function and the type;
//! - `main` is exposed as `run()`; `_start` is not called by `init`.
//!
//! The marshalling reads the module's OWN signatures (the wasm type section)
//! for the valtype of each slot — `Bool` is `i32` on the structural leg and
//! `i64` on the incumbent — and the block layout both legs share
//! (`almide-layout`: rc @0, len @4, cap @8, payload @12). `String` blocks the
//! host builds go through the module's exported allocator, and blocks the
//! host takes out are released through its exported release, so the
//! guest's free lists see every block they own.

use std::collections::BTreeMap;
use almide_ir::IrProgram;
use almide_lang::types::Ty;

/// One function on the host boundary: an exported `pub fn` or an extern.
#[derive(Debug, Clone)]
pub(crate) struct HostFn {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
}

/// One `@extern(wasm, module, name)` import.
#[derive(Debug, Clone)]
pub(crate) struct HostExtern {
    pub module: String,
    pub import: String,
    pub sig: HostFn,
}

/// The host-visible surface of a program, read from the IR before routing.
#[derive(Debug, Clone, Default)]
pub(crate) struct HostSurface {
    pub exports: Vec<HostFn>,
    pub externs: Vec<HostExtern>,
    pub has_main: bool,
}

impl HostSurface {
    pub(crate) fn of(program: &IrProgram) -> Self {
        let mut s = HostSurface::default();
        for f in &program.functions {
            let name = f.name.as_str();
            let sig = || HostFn {
                name: name.to_string(),
                params: f.params.iter().map(|p| (p.name.as_str().to_string(), p.ty.clone())).collect(),
                ret: f.ret_ty.clone(),
            };
            if let Some(a) = f.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
                s.externs.push(HostExtern {
                    module: a.module.as_str().to_string(),
                    import: a.function.as_str().to_string(),
                    sig: sig(),
                });
                continue;
            }
            if name == "main" {
                s.has_main = true;
                continue;
            }
            // The same set the structural emitter exports (#457): entry-program
            // pub fns, monomorphic, not tests, not compiler-internal.
            if name.starts_with("__")
                || f.is_test
                || f.generics.as_ref().is_some_and(|g| !g.is_empty())
                || !matches!(f.visibility, almide_ir::IrVisibility::Public)
            {
                continue;
            }
            s.exports.push(sig());
        }
        s
    }
}

/// The marshalled kinds — the five the host converts today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marshal {
    Int,
    Float,
    Bool,
    Str,
    Unit,
}

fn marshal_of(ty: &Ty) -> Option<Marshal> {
    match ty {
        Ty::Int => Some(Marshal::Int),
        Ty::Float => Some(Marshal::Float),
        Ty::Bool => Some(Marshal::Bool),
        Ty::String => Some(Marshal::Str),
        Ty::Unit => Some(Marshal::Unit),
        _ => None,
    }
}

fn ts_of(m: Marshal) -> &'static str {
    match m {
        Marshal::Int | Marshal::Float => "number",
        Marshal::Bool => "boolean",
        Marshal::Str => "string",
        Marshal::Unit => "void",
    }
}

/// A wasm valtype the boundary can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Val {
    I32,
    I64,
    F64,
}

struct WasmSig {
    params: Vec<Val>,
    results: Vec<Val>,
}

/// Function signatures of the emitted module: its function imports (by
/// module and name) and its function exports (by name).
struct WasmSigs {
    imports: Vec<(String, String, WasmSig)>,
    exports: BTreeMap<String, WasmSig>,
}

fn val_of(v: wasmparser::ValType) -> Result<Val, String> {
    match v {
        wasmparser::ValType::I32 => Ok(Val::I32),
        wasmparser::ValType::I64 => Ok(Val::I64),
        wasmparser::ValType::F64 => Ok(Val::F64),
        other => Err(format!("wasm valtype {other:?} has no host marshalling")),
    }
}

/// The function type a type-section entry declares (a non-function entry
/// keeps the index space aligned with an empty signature).
fn func_sig_of(sub: &wasmparser::SubType) -> Result<WasmSig, String> {
    match &sub.composite_type.inner {
        wasmparser::CompositeInnerType::Func(ft) => Ok(WasmSig {
            params: ft.params().iter().map(|v| val_of(*v)).collect::<Result<_, _>>()?,
            results: ft.results().iter().map(|v| val_of(*v)).collect::<Result<_, _>>()?,
        }),
        _ => Ok(WasmSig { params: Vec::new(), results: Vec::new() }),
    }
}

/// The raw sections a signature table is built from.
#[derive(Default)]
struct Sections {
    types: Vec<WasmSig>,
    /// The type index of every function in index order (imports first).
    func_types: Vec<u32>,
    imports: Vec<(String, String, u32)>,
    exports: Vec<(String, u32)>,
}

fn collect_sections(bytes: &[u8]) -> Result<Sections, String> {
    use wasmparser::{ExternalKind, Parser, Payload, TypeRef};
    let mut s = Sections::default();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    for sub in group.map_err(|e| e.to_string())?.into_types() {
                        s.types.push(func_sig_of(&sub)?);
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for (_, imp) in reader.into_iter().flatten().flat_map(|g| g.into_iter().flatten()) {
                    if let TypeRef::Func(t) = imp.ty {
                        s.func_types.push(t);
                        s.imports.push((imp.module.to_string(), imp.name.to_string(), t));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for t in reader {
                    s.func_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            Payload::ExportSection(reader) => {
                for e in reader {
                    let e = e.map_err(|e| e.to_string())?;
                    if e.kind == ExternalKind::Func {
                        s.exports.push((e.name.to_string(), e.index));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

fn wasm_sigs(bytes: &[u8]) -> Result<WasmSigs, String> {
    let s = collect_sections(bytes)?;
    let sig_at = |t: u32| -> Result<WasmSig, String> {
        let sig = s.types.get(t as usize).ok_or_else(|| format!("type index {t} out of range"))?;
        Ok(WasmSig { params: sig.params.clone(), results: sig.results.clone() })
    };
    let imports = s.imports.iter().map(|(m, n, t)| sig_at(*t).map(|sig| (m.clone(), n.clone(), sig))).collect::<Result<Vec<_>, _>>()?;
    let mut exports = BTreeMap::new();
    for (name, idx) in &s.exports {
        let t = *s.func_types.get(*idx as usize).ok_or_else(|| format!("export `{name}` names function {idx} outside the index space"))?;
        exports.insert(name.clone(), sig_at(t)?);
    }
    Ok(WasmSigs { imports, exports })
}

/// The refusal for a type the host cannot marshal yet.
fn refuse(fn_name: &str, what: &str, ty: &Ty) -> String {
    format!(
        "error: --host js cannot marshal {what} of `{fn_name}`: `{ty:?}` — the JS host marshals Int, Float, Bool, String and Unit (#2265); List, records and variants are a later step\n  hint: keep `{fn_name}` private (drop `pub`) or wrap it in a pub fn over the marshalled types"
    )
}

/// The JS expression converting `expr` (a JS value) into the wasm slot `v`
/// for marshal kind `m`.
fn to_wasm(m: Marshal, v: Val, expr: &str, what: &str) -> String {
    match (m, v) {
        (Marshal::Int, Val::I64) => format!("toI64({expr}, \"{what}\")"),
        (Marshal::Float, Val::F64) => format!("Number({expr})"),
        (Marshal::Bool, Val::I32) => format!("({expr} ? 1 : 0)"),
        (Marshal::Bool, Val::I64) => format!("({expr} ? 1n : 0n)"),
        (Marshal::Str, Val::I32) => format!("allocString({expr})"),
        _ => format!("/* unmarshallable {m:?} as {v:?} */ {expr}"),
    }
}

/// The JS expression converting the wasm slot value `expr` of valtype `v`
/// into the JS value for marshal kind `m`. `take` releases a String block.
fn from_wasm(m: Marshal, v: Val, expr: &str, what: &str, take: bool) -> String {
    match (m, v) {
        (Marshal::Int, Val::I64) => format!("fromI64({expr}, \"{what}\")"),
        (Marshal::Float, Val::F64) => expr.to_string(),
        (Marshal::Bool, Val::I32) => format!("({expr} !== 0)"),
        (Marshal::Bool, Val::I64) => format!("({expr} !== 0n)"),
        (Marshal::Str, Val::I32) if take => format!("takeString({expr})"),
        (Marshal::Str, Val::I32) => format!("readString({expr})"),
        _ => format!("/* unmarshallable {m:?} from {v:?} */ {expr}"),
    }
}

/// Which WASI imports the shim serves.
const WASI_SHIMS: &[&str] = &[
    "fd_write", "proc_exit", "fd_read", "random_get", "clock_time_get", "args_sizes_get", "args_get",
    "environ_sizes_get", "environ_get", "fd_close", "fd_fdstat_get", "fd_seek", "fd_prestat_get",
    "fd_prestat_dir_name", "path_open", "fd_filestat_get", "path_filestat_get", "path_create_directory",
    "path_remove_directory", "path_unlink_file", "fd_readdir", "sched_yield", "poll_oneoff",
];

/// Every type on the boundary is marshallable, or the build refuses.
fn check_marshallable(surface: &HostSurface) -> Result<(), String> {
    let sigs = surface.exports.iter().chain(surface.externs.iter().map(|e| &e.sig));
    for f in sigs {
        for (p, ty) in &f.params {
            if marshal_of(ty).is_none() {
                return Err(refuse(&f.name, &format!("parameter `{p}`"), ty));
            }
        }
        if marshal_of(&f.ret).is_none() {
            return Err(refuse(&f.name, "the return type", &f.ret));
        }
    }
    Ok(())
}

/// Every import the module names has a shim or a declared extern.
fn check_imports_served(sigs: &WasmSigs, surface: &HostSurface) -> Result<(), String> {
    for (module, name, _) in &sigs.imports {
        let known = (module == "wasi_snapshot_preview1" && WASI_SHIMS.contains(&name.as_str()))
            || surface.externs.iter().any(|e| &e.module == module && &e.import == name);
        if !known {
            return Err(format!("error: --host js has no shim for the import `{module}.{name}` the module names — declare it with @extern(wasm, \"{module}\", \"{name}\") or file an issue naming the program shape"));
        }
    }
    Ok(())
}

/// The `imports()` function: the WASI shims the module names and one
/// marshalling closure per extern import.
fn import_object_js(sigs: &WasmSigs, surface: &HostSurface) -> Result<String, String> {
    let mut js = String::from("\nfunction imports() {\n  const wasiImports = {};\n  const jsImports = {};\n");
    for (module, name, sig) in &sigs.imports {
        if module == "wasi_snapshot_preview1" {
            js.push_str(&format!("  wasiImports.{name} = wasi.{name};\n"));
            continue;
        }
        let e = surface.externs.iter().find(|e| &e.module == module && &e.import == name).expect("checked by check_imports_served");
        let args: Vec<String> = (0..sig.params.len()).map(|i| format!("a{i}")).collect();
        let mut conv = Vec::new();
        for (i, (_, ty)) in e.sig.params.iter().enumerate() {
            let m = marshal_of(ty).expect("checked by check_marshallable");
            let v = *sig.params.get(i).ok_or_else(|| format!("import `{name}` has fewer wasm params than `{}` declares", e.sig.name))?;
            conv.push(from_wasm(m, v, &args[i], name, false));
        }
        let call = format!("hook(\"{module}\", \"{name}\")({})", conv.join(", "));
        let body = match (marshal_of(&e.sig.ret).expect("checked"), sig.results.first()) {
            (Marshal::Unit, _) | (_, None) => format!("{call};"),
            (m, Some(v)) => format!("return {};", to_wasm(m, *v, &call, name)),
        };
        js.push_str(&format!("  jsImports.{name} = ({}) => {{ {body} }};\n", args.join(", ")));
    }
    js.push_str("  const obj = { wasi_snapshot_preview1: wasiImports };\n");
    let modules: std::collections::BTreeSet<&str> = surface.externs.iter().map(|e| e.module.as_str()).collect();
    for m in &modules {
        js.push_str(&format!("  obj[\"{m}\"] = jsImports;\n"));
    }
    js.push_str("  return obj;\n}\n");
    Ok(js)
}

/// The wrapper for one exported function. A `String` argument goes in
/// through the module's allocator; the callee owns the block iff its param
/// is owned (the structural ownership table) — the incumbent's callees
/// borrow every param — and a borrowed block is the host's to release.
fn wrapper_js(f: &HostFn, sig: &WasmSig, structural: bool, owned: &[bool]) -> Result<String, String> {
    let mut pre = Vec::new();
    let mut args = Vec::new();
    let mut post = Vec::new();
    for (i, (p, ty)) in f.params.iter().enumerate() {
        let m = marshal_of(ty).expect("checked by check_marshallable");
        let v = *sig.params.get(i).ok_or_else(|| format!("export `{}` has fewer wasm params than declared", f.name))?;
        if m == Marshal::Str {
            let callee_owns = structural && owned.get(i).copied().unwrap_or(false);
            pre.push(format!("  const h{i} = allocString({p});\n"));
            args.push(format!("h{i}"));
            if !callee_owns {
                post.push(format!("    instance.exports.__release(h{i});\n"));
            }
        } else {
            args.push(to_wasm(m, v, p, &f.name));
        }
    }
    let call = format!("instance.exports.{}({})", f.name, args.join(", "));
    let body = match (marshal_of(&f.ret).expect("checked"), sig.results.first()) {
        (Marshal::Unit, _) | (_, None) => format!("    {call};\n"),
        (m, Some(v)) => format!("    return {};\n", from_wasm(m, *v, &call, &f.name, true)),
    };
    let names: Vec<&str> = f.params.iter().map(|(p, _)| p.as_str()).collect();
    let mut js = format!("\nexport function {}({}) {{\n  ready();\n{}", f.name, names.join(", "), pre.concat());
    if post.is_empty() {
        js.push_str(&body);
    } else {
        js.push_str(&format!("  try {{\n{body}  }} finally {{\n{}  }}\n", post.concat()));
    }
    js.push_str("}\n");
    Ok(js)
}

fn signature_dts(f: &HostFn) -> String {
    let params: Vec<String> = f.params.iter().map(|(p, ty)| format!("{p}: {}", ts_of(marshal_of(ty).expect("checked")))).collect();
    format!("({}) => {}", params.join(", "), ts_of(marshal_of(&f.ret).expect("checked")))
}

/// The `Hooks` interface's `js` member: one entry per extern import.
fn hooks_dts(surface: &HostSurface) -> String {
    if surface.externs.is_empty() {
        return "  /** The program declares no `@extern(wasm, ...)` import. */\n  js?: Record<string, never>;\n}\n\n".to_string();
    }
    let rows: Vec<String> = surface.externs.iter().map(|e| format!("    {}: {};", e.import, signature_dts(&e.sig))).collect();
    format!("  /** The `@extern(wasm, \"js\", ...)` imports the program declares. */\n  js: {{\n{}\n  }};\n}}\n\n", rows.join("\n"))
}

const RUN_JS: &str = "\n/** Run `main` (the module's `_start`); a non-zero exit throws AlmideExit. */\nexport function run() {\n  ready();\n  try {\n    instance.exports._start();\n  } catch (e) {\n    if (e instanceof AlmideExit && e.code === 0) return;\n    throw e;\n  } finally {\n    flush();\n  }\n}\n";

/// The generated host: `(js, d_ts)`.
pub(crate) fn generate(
    wasm_name: &str,
    source_file: &str,
    bytes: &[u8],
    surface: &HostSurface,
    structural: bool,
    export_param_owned: &BTreeMap<String, Vec<bool>>,
) -> Result<(String, String), String> {
    let sigs = wasm_sigs(bytes)?;
    check_marshallable(surface)?;
    check_imports_served(&sigs, surface)?;

    let version = env!("CARGO_PKG_VERSION");
    let leg = if structural { "structural" } else { "incumbent" };
    let alloc_body = if structural {
        "  const h = instance.exports.__alloc(b.length); // the module sets rc = 1, len, cap"
    } else {
        "  const cap = Math.max(1, Math.ceil(b.length / 8)); // the incumbent counts cap in 8-byte slots\n  const h = instance.exports.__alloc(PAYLOAD + cap * 8);\n  const v = view();\n  v.setUint32(h, 1, true);\n  v.setUint32(h + 4, b.length, true);\n  v.setUint32(h + 8, cap, true);"
    };
    let banner = format!("// Generated by `almide build {source_file} --target wasm --host js` (almide {version}, {leg} leg).\n// Do not edit: change the .almd and rebuild.\n");

    let mut js = banner.clone();
    js.push_str(&format!("const WASM_URL = new URL(\"./{wasm_name}\", import.meta.url);\n"));
    js.push_str(&JS_RUNTIME.replace("{ALLOC_BODY}", alloc_body));
    js.push_str(&import_object_js(&sigs, surface)?);

    let mut dts = format!("// Generated by `almide build {source_file} --target wasm --host js` (almide {version}).\n\n");
    dts.push_str(DTS_RUNTIME);
    dts.push_str(&hooks_dts(surface));
    dts.push_str("export function init(source?: WasmSource, hooks?: Hooks): Promise<void>;\n");
    if surface.has_main && sigs.exports.contains_key("_start") {
        js.push_str(RUN_JS);
        dts.push_str("/** Run `main`; a non-zero exit code throws `AlmideExit`. */\nexport function run(): void;\n");
    }
    for f in &surface.exports {
        // The structural leg exports a pub fn only when its whole call
        // closure lowers; a fn the module does not export gets no wrapper.
        let Some(sig) = sigs.exports.get(&f.name) else { continue };
        let owned = export_param_owned.get(&f.name).map(Vec::as_slice).unwrap_or(&[]);
        js.push_str(&wrapper_js(f, sig, structural, owned)?);
        dts.push_str(&format!("export function {}{};\n", f.name, signature_dts(f).replacen(" => ", ": ", 1)));
    }
    Ok((js, dts))
}

const JS_RUNTIME: &str = r#"const PAYLOAD = 12;
const INT_LIMIT = 9007199254740992n;

/** Thrown by `proc_exit`: the program ended with `code`. */
export class AlmideExit extends Error {
  constructor(code) {
    super(`almide: exit ${code}`);
    this.name = "AlmideExit";
    this.code = code;
  }
}

let instance = null;
let memory = null;
let hooks = {};
const lineBuf = { 1: "", 2: "" };
const decoder = new TextDecoder("utf-8");
const encoder = new TextEncoder();

function bytes() { return new Uint8Array(memory.buffer); }
function view() { return new DataView(memory.buffer); }
function ready() { if (instance === null) throw new Error("almide: call init() before using the module"); }

function readString(h) {
  const len = view().getUint32(h + 4, true);
  return decoder.decode(bytes().subarray(h + PAYLOAD, h + PAYLOAD + len));
}
function takeString(h) {
  const s = readString(h);
  instance.exports.__release(h);
  return s;
}
function allocString(s) {
  const b = encoder.encode(String(s));
{ALLOC_BODY}
  bytes().set(b, h + PAYLOAD);
  return h;
}
function toI64(x, what) {
  if (typeof x === "bigint") return x;
  if (!Number.isSafeInteger(x)) throw new RangeError(`almide: ${what} takes an Int (an integer within ±2^53), got ${x}`);
  return BigInt(x);
}
function fromI64(b, what) {
  if (b > INT_LIMIT || b < -INT_LIMIT) throw new RangeError(`almide: ${what} produced an Int outside ±2^53: ${b}n — pass a BigInt-aware hook to keep it exact`);
  return Number(b);
}
function hook(module, name) {
  const table = hooks[module] || {};
  const f = table[name];
  if (typeof f !== "function") throw new Error(`almide: init() needs hooks.${module}.${name} for @extern(wasm, "${module}", "${name}")`);
  return f;
}
function emit(fd, chunk) {
  const custom = fd === 2 ? hooks.stderr : hooks.stdout;
  if (custom) { custom(chunk); return; }
  if (typeof process !== "undefined" && process.stdout && typeof process.stdout.write === "function") {
    (fd === 2 ? process.stderr : process.stdout).write(chunk);
    return;
  }
  lineBuf[fd] += decoder.decode(chunk, { stream: true });
  let nl;
  while ((nl = lineBuf[fd].indexOf("\n")) >= 0) {
    (fd === 2 ? console.error : console.log)(lineBuf[fd].slice(0, nl));
    lineBuf[fd] = lineBuf[fd].slice(nl + 1);
  }
}
function flush() {
  for (const fd of [1, 2]) {
    if (lineBuf[fd] !== "") { (fd === 2 ? console.error : console.log)(lineBuf[fd]); lineBuf[fd] = ""; }
  }
}
const wasi = {
  fd_write(fd, iovs, iovsLen, nwritten) {
    const v = view();
    let total = 0;
    const parts = [];
    for (let i = 0; i < iovsLen; i++) {
      const ptr = v.getUint32(iovs + 8 * i, true);
      const len = v.getUint32(iovs + 8 * i + 4, true);
      parts.push(bytes().slice(ptr, ptr + len));
      total += len;
    }
    if (fd !== 1 && fd !== 2) return 8; // EBADF
    const chunk = new Uint8Array(total);
    let off = 0;
    for (const p of parts) { chunk.set(p, off); off += p.length; }
    emit(fd, chunk);
    view().setUint32(nwritten, total, true);
    return 0;
  },
  proc_exit(code) { flush(); throw new AlmideExit(code); },
  fd_read(fd, iovs, iovsLen, nread) { view().setUint32(nread, 0, true); return 0; },
  random_get(ptr, len) { globalThis.crypto.getRandomValues(bytes().subarray(ptr, ptr + len)); return 0; },
  clock_time_get(id, precision, out) { view().setBigUint64(out, BigInt(Date.now()) * 1000000n, true); return 0; },
  args_sizes_get(argc, bufSize) { const v = view(); v.setUint32(argc, 0, true); v.setUint32(bufSize, 0, true); return 0; },
  args_get() { return 0; },
  environ_sizes_get(count, bufSize) { const v = view(); v.setUint32(count, 0, true); v.setUint32(bufSize, 0, true); return 0; },
  environ_get() { return 0; },
  fd_close() { return 0; },
  fd_fdstat_get() { return 8; },
  fd_seek() { return 8; },
  fd_prestat_get() { return 8; },
  fd_prestat_dir_name() { return 8; },
  fd_filestat_get() { return 8; },
  fd_readdir() { return 8; },
  path_open() { return 44; }, // ENOENT: the host has no filesystem
  path_filestat_get() { return 44; },
  path_create_directory() { return 44; },
  path_remove_directory() { return 44; },
  path_unlink_file() { return 44; },
  sched_yield() { return 0; },
  poll_oneoff() { return 52; }, // ENOSYS
};

/**
 * Compile and instantiate the module. `source` may be omitted (the .wasm
 * next to this file is loaded: `fs.readFile` under node, `fetch` in a page),
 * or be bytes, a `Response`, a promise of either, or a compiled
 * `WebAssembly.Module`. `hooks.js` carries the `@extern(wasm, "js", ...)`
 * functions; `hooks.stdout` / `hooks.stderr` receive raw `Uint8Array`
 * chunks (default: `process.stdout` under node, `console.log` per line in a page).
 */
export async function init(source, h = {}) {
  hooks = h;
  let module;
  if (source instanceof WebAssembly.Module) {
    module = source;
  } else {
    let data = await source;
    if (data == null) {
      if (typeof process !== "undefined" && process.versions && process.versions.node) {
        const fs = await import("node:fs/promises");
        data = await fs.readFile(WASM_URL);
      } else {
        data = await fetch(WASM_URL);
      }
    }
    if (typeof Response !== "undefined" && data instanceof Response) data = await data.arrayBuffer();
    module = await WebAssembly.compile(data);
  }
  instance = await WebAssembly.instantiate(module, imports());
  memory = instance.exports.memory;
}
"#;

const DTS_RUNTIME: &str = r#"export type WasmSource =
  | BufferSource
  | Response
  | WebAssembly.Module
  | Promise<BufferSource | Response>;

/** Thrown by the program's `proc_exit`: `code` is the exit code. */
export class AlmideExit extends Error {
  code: number;
}

export interface Hooks {
  /** Raw stdout chunks; default: `process.stdout` under node, `console.log` per line in a page. */
  stdout?: (chunk: Uint8Array) => void;
  /** Raw stderr chunks; default: `process.stderr` / `console.error`. */
  stderr?: (chunk: Uint8Array) => void;
"#;
