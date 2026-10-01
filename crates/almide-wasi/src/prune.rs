//! Drop what the shipped p1 module can prove it never uses (#3114): function
//! imports nothing calls, globals nothing reads, writes or exports, and the
//! types only those dead imports named.
//!
//! The transform appends a fixed WASI surface and the emitter a fixed global
//! block, and both keep their slots at constant indices so the code that
//! names them is written once. Most programs reach a few of them: hello,
//! world calls two of the five base imports and touches none of the
//! emitter's sixteen fixed globals. Rather than teach every shim a sliding
//! index, this pass runs LAST, over the finished module, and renumbers.
//!
//! The reference set is not hand-collected. The module is re-encoded twice
//! through one [`Reencode`]: the first pass is the identity and records every
//! function, global and type index the re-encoder is asked to translate (a
//! call, `ref.func`, an element, an export, a `global.get`, a const-expr, a
//! block type, a `call_indirect`, a function's own type); the second pass
//! translates through the compacted maps. Because both passes go through the
//! same hooks, an index form the first pass did not see is one the second
//! pass does not rewrite either, so the two cannot disagree. The one
//! exception is the import section: an import's own type is not a use of
//! the import, so it is recorded per import and counts only for the imports
//! that stay.
//!
//! Defined functions are never removed (the emitter's slot discipline keeps
//! unreached ones as `unreachable` stubs, and gates name those slots), and a
//! module with any non-function import or a non-function type is returned
//! unchanged — the structural emitter produces neither.

use std::collections::BTreeSet;
use std::convert::Infallible;

use wasm_encoder::reencode::{Error, Reencode};

#[derive(Default)]
struct Prune {
    record: bool,
    funcs: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    types: BTreeSet<u32>,
    /// The type of each function import, by import index (record pass).
    import_types: Vec<u32>,
    fmap: Vec<Option<u32>>,
    gmap: Vec<Option<u32>>,
    tmap: Vec<Option<u32>>,
}

type R<T> = Result<T, Error<Infallible>>;

fn mapped(map: &[Option<u32>], i: u32) -> R<u32> {
    map.get(i as usize).copied().flatten().ok_or(Error::InvalidConstExpr)
}

/// Old index -> new index over `0..n`, keeping the indices `keep` names.
fn compact(n: u32, keep: impl Fn(u32) -> bool) -> Vec<Option<u32>> {
    let mut next = 0u32;
    (0..n)
        .map(|i| {
            keep(i).then(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

impl Reencode for Prune {
    type Error = Infallible;

    fn function_index(&mut self, func: u32) -> R<u32> {
        if self.record {
            self.funcs.insert(func);
            return Ok(func);
        }
        mapped(&self.fmap, func)
    }

    fn global_index(&mut self, global: u32) -> R<u32> {
        if self.record {
            self.globals.insert(global);
            return Ok(global);
        }
        mapped(&self.gmap, global)
    }

    fn type_index(&mut self, ty: u32) -> R<u32> {
        if self.record {
            self.types.insert(ty);
            return Ok(ty);
        }
        mapped(&self.tmap, ty)
    }

    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> R<()> {
        for (idx, import) in section.into_imports().enumerate() {
            let import = import?;
            let wasmparser::TypeRef::Func(ty) = import.ty else { unreachable!("screened by prunable") };
            if self.record {
                self.import_types.push(ty);
            } else if self.fmap[idx].is_some() {
                imports.import(import.module, import.name, self.entity_type(import.ty)?);
            }
        }
        Ok(())
    }

    fn parse_global_section(
        &mut self,
        globals: &mut wasm_encoder::GlobalSection,
        section: wasmparser::GlobalSectionReader<'_>,
    ) -> R<()> {
        for (idx, global) in section.into_iter().enumerate() {
            let global = global?;
            if self.record || self.gmap[idx].is_some() {
                self.parse_global(globals, global)?;
            }
        }
        Ok(())
    }

    fn parse_type_section(
        &mut self,
        types: &mut wasm_encoder::TypeSection,
        section: wasmparser::TypeSectionReader<'_>,
    ) -> R<()> {
        for (idx, group) in section.into_iter().enumerate() {
            let group = group?;
            if self.record || self.tmap[idx].is_some() {
                self.parse_recursive_type_group(types.ty(), group)?;
            }
        }
        Ok(())
    }
}

/// The module's index spaces, or `None` when it holds something this pass
/// does not renumber (a non-function import, an explicit rec group or a
/// non-function type).
struct Shape {
    imports: u32,
    defined: u32,
    globals: u32,
    types: u32,
}

fn prunable(bytes: &[u8]) -> anyhow::Result<Option<Shape>> {
    use wasmparser::{CompositeInnerType, Payload, TypeRef};
    let mut s = Shape { imports: 0, defined: 0, globals: 0, types: 0 };
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(r) => {
                for i in r.into_imports() {
                    if !matches!(i?.ty, TypeRef::Func(_)) {
                        return Ok(None);
                    }
                    s.imports += 1;
                }
            }
            Payload::FunctionSection(r) => s.defined = r.count(),
            Payload::GlobalSection(r) => s.globals = r.count(),
            Payload::TypeSection(r) => {
                for g in r {
                    let g = g?;
                    let funcs_only = g.types().all(|t| matches!(t.composite_type.inner, CompositeInnerType::Func(_)));
                    if g.is_explicit_rec_group() || !funcs_only {
                        return Ok(None);
                    }
                    s.types += 1;
                }
            }
            _ => {}
        }
    }
    Ok(Some(s))
}

/// `bytes` without its uncalled function imports, unreferenced globals and
/// the types only those imports used. The caller validates the result.
pub fn prune(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let Some(shape) = prunable(bytes)? else { return Ok(bytes.to_vec()) };
    let fail = |e: Error<Infallible>| anyhow::anyhow!("prune reencode: {e}");
    let mut seen = Prune { record: true, ..Prune::default() };
    seen.parse_core_module(&mut wasm_encoder::Module::new(), wasmparser::Parser::new(0), bytes).map_err(fail)?;

    let kept_import = |i: u32| seen.funcs.contains(&i);
    let dropped = (0..shape.imports).filter(|&i| !kept_import(i)).count() as u32;
    let mut fmap = compact(shape.imports, kept_import);
    fmap.extend((shape.imports..shape.imports + shape.defined).map(|i| Some(i - dropped)));
    let import_types = seen.import_types.iter().enumerate().filter(|(i, _)| kept_import(*i as u32)).map(|(_, t)| *t);
    let types: BTreeSet<u32> = seen.types.iter().copied().chain(import_types).collect();
    let gmap = compact(shape.globals, |g| seen.globals.contains(&g));
    let tmap = compact(shape.types, |t| types.contains(&t));

    let mut out = wasm_encoder::Module::new();
    Prune { fmap, gmap, tmap, ..Prune::default() }
        .parse_core_module(&mut out, wasmparser::Parser::new(0), bytes)
        .map_err(fail)?;
    Ok(out.finish())
}
