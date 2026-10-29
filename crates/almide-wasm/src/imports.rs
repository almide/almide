//! Declared imports (#2275): the `@extern(wasm, module, name)` fns of a
//! program become `(import module name (func ...))` entries of the structural
//! module, so a program that talks to JS builds on the default leg.
//!
//! The lowering keeps every fixed function index where it is: an extern fn
//! is emitted as an ordinary program-fn slot whose body is a loud stub
//! (`unreachable`), so call sites, funcref table entries and the ownership
//! table treat it like any other named fn (every param borrowed — the host
//! reads its arguments and releases nothing; a `String` result is the fresh
//! `+1` a named call always returns). This post-pass then rewrites the
//! finished bytes once: the stubs leave the function and code sections,
//! one import per stub is appended behind the five `almide.*` imports, and
//! every function index — calls, exports, element segments — is renumbered
//! through one map (`Reencode::function_index`), the same discipline the
//! p1 WASI transform uses to shift indices (crates/almide-wasm-run/wasi.rs).

use std::collections::BTreeMap;

use wasm_encoder::reencode::{Error, Reencode};
use wasm_encoder::{CodeSection, EntityType, Function, FunctionSection, ImportSection, Module, TypeSection, ValType};

use almide_ir::{IrExprKind, IrFunction};

use crate::types_table::TypeTable;
use crate::SliceTy;

/// One import to declare: the stub's original function index and the
/// `(module, name)` the host serves it under.
pub(crate) struct Declared {
    pub(crate) index: u32,
    pub(crate) module: String,
    pub(crate) name: String,
    /// The parameters that carry an address or a length (a Region's
    /// `(base, len)`, ADR-0031 D3): they cross in the target's address
    /// width. On a 32-bit memory the import takes them as `i32` and a shim
    /// in the stub's place wraps the `i64` values the emitter computes; on
    /// memory64 they stay `i64` and no shim is made.
    pub(crate) narrow: Vec<u32>,
}

/// One narrowed import: its stub, the import's own type, and the shim that
/// takes the stub's place in every call.
struct Narrow {
    stub: u32,
    /// The stub's own (`i64`) type, which the shim keeps.
    ty: u32,
    shim: u32,
    arity: u32,
}

/// Turn every stub in `declared` into a declared import of `bytes`.
pub(crate) fn declare(bytes: &[u8], declared: &[Declared]) -> Result<Vec<u8>, String> {
    if declared.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut base_imports = 0u32;
    let mut func_types: Vec<u32> = Vec::new();
    let mut sigs: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    let mut memory64 = false;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            wasmparser::Payload::ImportSection(r) => {
                for i in r.into_imports() {
                    if matches!(i.map_err(|e| e.to_string())?.ty, wasmparser::TypeRef::Func(_)) {
                        base_imports += 1;
                    }
                }
            }
            wasmparser::Payload::FunctionSection(r) => {
                for t in r {
                    func_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            wasmparser::Payload::TypeSection(r) => {
                for t in r.into_iter_err_on_gc_types() {
                    let t = t.map_err(|e| e.to_string())?;
                    let conv = |v: &wasmparser::ValType| match v {
                        wasmparser::ValType::I32 => ValType::I32,
                        wasmparser::ValType::I64 => ValType::I64,
                        wasmparser::ValType::F32 => ValType::F32,
                        wasmparser::ValType::F64 => ValType::F64,
                        _ => ValType::I32,
                    };
                    sigs.push((t.params().iter().map(conv).collect(), t.results().iter().map(conv).collect()));
                }
            }
            wasmparser::Payload::MemorySection(r) => {
                for m in r {
                    memory64 |= m.map_err(|e| e.to_string())?.memory64;
                }
            }
            _ => {}
        }
    }
    // Imports are numbered in stub order (ascending original index).
    let mut stubs: BTreeMap<u32, Stub<'_>> = BTreeMap::new();
    for d in declared {
        if d.index < base_imports || (d.index - base_imports) as usize >= func_types.len() {
            return Err(format!("import `{}.{}`: stub index {} is not a defined function", d.module, d.name, d.index));
        }
        stubs.insert(d.index, Stub { import: 0, ty: func_types[(d.index - base_imports) as usize], decl: d });
    }
    for (k, s) in stubs.values_mut().enumerate() {
        s.import = base_imports + k as u32;
    }
    let keys: Vec<u32> = stubs.keys().copied().collect();
    // The address width is the target memory's: narrow only on wasm32.
    let mut narrows: Vec<Narrow> = Vec::new();
    let mut new_types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    for s in stubs.values_mut() {
        if memory64 || s.decl.narrow.is_empty() {
            continue;
        }
        let (mut params, results) = sigs.get(s.ty as usize).cloned().ok_or("stub type out of range")?;
        for &p in &s.decl.narrow {
            if let Some(v) = params.get_mut(p as usize) {
                *v = ValType::I32;
            }
        }
        let shim = base_imports + func_types.len() as u32 + narrows.len() as u32;
        let ty = sigs.len() as u32 + new_types.len() as u32;
        narrows.push(Narrow { stub: s.decl.index, ty: s.ty, shim, arity: params.len() as u32 });
        new_types.push((params, results));
        s.ty = ty;
    }
    let mut re = Declare { base_imports, stubs, keys, narrows, new_types };
    let mut module = Module::new();
    re.parse_core_module(&mut module, wasmparser::Parser::new(0), bytes).map_err(|e| e.to_string())?;
    Ok(module.finish())
}

/// Where function `func` lands once the ascending `stubs` became imports
/// numbered from `base_imports` — the ONE renumbering both this post-pass
/// and the witness projector (cert_project.rs, #2759) apply.
pub(crate) fn remap_index(func: u32, stubs: &[u32], base_imports: u32) -> u32 {
    if let Some(k) = stubs.iter().position(|&s| s == func) {
        return base_imports + k as u32;
    }
    if func < base_imports {
        return func;
    }
    let stubs_before = stubs.iter().filter(|&&s| s < func).count() as u32;
    func + stubs.len() as u32 - stubs_before
}

struct Stub<'a> {
    import: u32,
    ty: u32,
    decl: &'a Declared,
}

struct Declare<'a> {
    base_imports: u32,
    stubs: BTreeMap<u32, Stub<'a>>,
    /// The stub indices, ascending (`remap_index`'s input).
    keys: Vec<u32>,
    narrows: Vec<Narrow>,
    new_types: Vec<(Vec<ValType>, Vec<ValType>)>,
}

impl Declare<'_> {
    fn is_stub_ordinal(&self, j: usize) -> bool {
        self.stubs.contains_key(&(self.base_imports + j as u32))
    }
}

impl Reencode for Declare<'_> {
    type Error = std::convert::Infallible;

    fn function_index(&mut self, func: u32) -> Result<u32, Error<Self::Error>> {
        // A narrowed stub's callers reach its shim, which calls the import.
        if let Some(n) = self.narrows.iter().find(|n| n.stub == func) {
            return Ok(n.shim);
        }
        Ok(remap_index(func, &self.keys, self.base_imports))
    }

    fn parse_type_section(
        &mut self,
        types: &mut TypeSection,
        section: wasmparser::TypeSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        wasm_encoder::reencode::utils::parse_type_section(self, types, section)?;
        for (params, results) in &self.new_types {
            types.ty().function(params.clone(), results.clone());
        }
        Ok(())
    }

    fn parse_import_section(
        &mut self,
        imports: &mut ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        wasm_encoder::reencode::utils::parse_import_section(self, imports, section)?;
        for s in self.stubs.values() {
            imports.import(&s.decl.module, &s.decl.name, EntityType::Function(s.ty));
        }
        Ok(())
    }

    fn parse_function_section(
        &mut self,
        functions: &mut FunctionSection,
        section: wasmparser::FunctionSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        for (j, t) in section.into_iter().enumerate() {
            let t = t?;
            if !self.is_stub_ordinal(j) {
                functions.function(self.type_index(t)?);
            }
        }
        for n in &self.narrows {
            functions.function(n.ty);
        }
        Ok(())
    }

    fn parse_code_section(
        &mut self,
        code: &mut CodeSection,
        section: wasmparser::CodeSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        for (j, body) in section.into_iter().enumerate() {
            let body = body?;
            if !self.is_stub_ordinal(j) {
                self.parse_function_body(code, body)?;
            }
        }
        // Each shim: the stub's own `i64` signature, wrapping the address and
        // length parameters to the import's `i32`.
        for n in &self.narrows {
            let s = &self.stubs[&n.stub];
            let mut f = Function::new([]);
            let mut i = f.instructions();
            for p in 0..n.arity {
                i.local_get(p);
                if s.decl.narrow.contains(&p) {
                    i.i32_wrap_i64();
                }
            }
            i.call(s.import);
            i.end();
            code.function(&f);
        }
        Ok(())
    }
}

/// The `@extern(wasm, module, name)` import a body-less fn declares (#2275):
/// `Ok(Some((module, name)))` when its signature has the scalar host ABI
/// (`Int`/sized ints → i64, `Float` → f64, `Bool` → i32, `String` → i32
/// block, `Unit` → no result), or — a fallible import (#3356: an `effect fn`
/// extern, or one declaring `Result[T, String]`) — an i32 `Result` block the
/// host builds, ok a scalar or Unit, err a String; `Ok(None)` for a fn with
/// a body; `Err` for a native (`rs`/`rust`) extern — there is no wasm host
/// for it, so an import would be a hollow lie — and for a param or return
/// outside the ABI.
pub(crate) fn extern_import(f: &IrFunction, params: &[SliceTy], ret: Option<SliceTy>, types: &TypeTable) -> Result<Option<(String, String)>, String> {
    if f.extern_attrs.is_empty() {
        return Ok(None);
    }
    let Some(a) = f.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") else {
        return Err(format!("extern-native:{}", f.name));
    };
    if !matches!(f.body.kind, IrExprKind::Hole) {
        return Ok(None);
    }
    for (p, t) in f.params.iter().zip(params) {
        if !matches!(t, SliceTy::Scalar(_)) {
            return Err(format!("extern-ty:{}:{}", f.name, p.name));
        }
    }
    let fallible = |ok: crate::ETy, err: crate::ETy| {
        matches!(types.el(ok), SliceTy::Scalar(_) | SliceTy::Unit) && types.el(err) == crate::STR
    };
    let host_ret = match ret {
        None | Some(SliceTy::Scalar(_)) => true,
        Some(SliceTy::Result(ok, err)) => fallible(ok, err),
        _ => false,
    };
    if !host_ret {
        return Err(format!("extern-ret:{}", f.name));
    }
    Ok(Some((a.module.as_str().to_string(), a.function.as_str().to_string())))
}
