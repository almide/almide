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
use wasm_encoder::{CodeSection, EntityType, FunctionSection, ImportSection, Module};

/// One import to declare: the stub's original function index and the
/// `(module, name)` the host serves it under.
pub(crate) struct Declared {
    pub(crate) index: u32,
    pub(crate) module: String,
    pub(crate) name: String,
}

/// Turn every stub in `declared` into a declared import of `bytes`.
pub(crate) fn declare(bytes: &[u8], declared: &[Declared]) -> Result<Vec<u8>, String> {
    if declared.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut base_imports = 0u32;
    let mut func_types: Vec<u32> = Vec::new();
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
    let mut re = Declare { base_imports, stubs };
    let mut module = Module::new();
    re.parse_core_module(&mut module, wasmparser::Parser::new(0), bytes).map_err(|e| e.to_string())?;
    Ok(module.finish())
}

struct Stub<'a> {
    import: u32,
    ty: u32,
    decl: &'a Declared,
}

struct Declare<'a> {
    base_imports: u32,
    stubs: BTreeMap<u32, Stub<'a>>,
}

impl Declare<'_> {
    fn is_stub_ordinal(&self, j: usize) -> bool {
        self.stubs.contains_key(&(self.base_imports + j as u32))
    }
}

impl Reencode for Declare<'_> {
    type Error = std::convert::Infallible;

    fn function_index(&mut self, func: u32) -> Result<u32, Error<Self::Error>> {
        if let Some(s) = self.stubs.get(&func) {
            return Ok(s.import);
        }
        if func < self.base_imports {
            return Ok(func);
        }
        let stubs_before = self.stubs.range(..func).count() as u32;
        Ok(func + self.stubs.len() as u32 - stubs_before)
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
        Ok(())
    }
}
