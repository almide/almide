//! Drop what a shipped module can prove it never uses (#3114, #3136):
//! defined functions no root reaches, function imports only those (or
//! nothing) call, globals nothing live reads, writes or exports, and the
//! types nothing live names.
//!
//! The emitter keeps its runtime helpers at fixed function slots and the
//! transforms append a fixed host surface and a fixed global block, so the
//! code that names them is written once. Most programs reach few of them:
//! hello, world calls two of the five base WASI imports, touches none of the
//! emitter's sixteen fixed globals, and reaches a handful of its 37 helper
//! slots — the rest ship as `unreachable` stubs that only keep the indices
//! stable. Rather than teach every shim and the emitter a sliding index,
//! this pass runs LAST, over the finished core module, and renumbers. The
//! fixed slots stay fixed in the module the emitter hands over (the embedded
//! host runs that one, and the gates that address helpers by slot read it);
//! only the shipped form is compacted, and a host reaches it by export name.
//!
//! The reference set is not hand-collected. The module is re-encoded twice
//! through one [`Reencode`]: the first pass is the identity and records every
//! function, global and type index the re-encoder is asked to translate (a
//! call, `ref.func`, an element, an export, a `global.get`, a const-expr, a
//! block type, a `call_indirect`, a function's own type) — inside a body
//! against that body, outside every body as a root. Liveness is then the
//! closure of the roots over the bodies' references, and the second pass
//! translates through the compacted maps and skips what is dead. Because
//! both passes go through the same hooks, an index form the first pass did
//! not see is one the second pass does not rewrite either, so the two cannot
//! disagree. Two indices are recorded per entry rather than as uses: an
//! import's own type and a defined function's own type count only when that
//! function stays.
//!
//! A module with any non-function import, a non-function type or a custom
//! section (a name section would carry dropped indices) is returned
//! unchanged — the transforms produce none of them before this pass.

use std::collections::BTreeSet;
use std::convert::Infallible;

use wasm_encoder::reencode::{Error, Reencode};

/// The indices one scope names: a function body, or everything outside the
/// bodies (exports, elements, the start function, const-exprs).
#[derive(Default)]
struct Refs {
    funcs: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    types: BTreeSet<u32>,
}

#[derive(Default)]
struct Prune {
    record: bool,
    /// Record pass: the defined function whose body is being read.
    cur: Option<usize>,
    /// Record pass: the references made outside every body.
    roots: Refs,
    /// Record pass: each defined function's body references.
    bodies: Vec<Refs>,
    /// Record pass: the type of each function import, by import index.
    import_types: Vec<u32>,
    /// Record pass: the type of each defined function, by definition index.
    func_types: Vec<u32>,
    imports: u32,
    /// Record pass: whether anything names a table (a `call_indirect`, an
    /// active element segment, an export, a table instruction).
    tables_used: bool,
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

impl Prune {
    fn scope(&mut self) -> &mut Refs {
        match self.cur {
            Some(i) => &mut self.bodies[i],
            None => &mut self.roots,
        }
    }

    fn kept(&self, func: u32) -> bool {
        self.fmap[func as usize].is_some()
    }
}

impl Reencode for Prune {
    type Error = Infallible;

    fn function_index(&mut self, func: u32) -> R<u32> {
        if self.record {
            self.scope().funcs.insert(func);
            return Ok(func);
        }
        mapped(&self.fmap, func)
    }

    fn global_index(&mut self, global: u32) -> R<u32> {
        if self.record {
            self.scope().globals.insert(global);
            return Ok(global);
        }
        mapped(&self.gmap, global)
    }

    fn table_index(&mut self, table: u32) -> R<u32> {
        self.tables_used |= self.record;
        Ok(table)
    }

    fn type_index(&mut self, ty: u32) -> R<u32> {
        if self.record {
            self.scope().types.insert(ty);
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
            } else if self.kept(idx as u32) {
                imports.import(import.module, import.name, self.entity_type(import.ty)?);
            }
        }
        Ok(())
    }

    fn parse_function_section(
        &mut self,
        functions: &mut wasm_encoder::FunctionSection,
        section: wasmparser::FunctionSectionReader<'_>,
    ) -> R<()> {
        for (idx, ty) in section.into_iter().enumerate() {
            let ty = ty?;
            if self.record {
                self.func_types.push(ty);
            } else if self.kept(self.imports + idx as u32) {
                functions.function(self.type_index(ty)?);
            }
        }
        Ok(())
    }

    fn parse_code_section(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        section: wasmparser::CodeSectionReader<'_>,
    ) -> R<()> {
        for (idx, body) in section.into_iter().enumerate() {
            let body = body?;
            if self.record {
                self.bodies.push(Refs::default());
                self.cur = Some(idx);
                self.parse_function_body(code, body)?;
                self.cur = None;
            } else if self.kept(self.imports + idx as u32) {
                self.parse_function_body(code, body)?;
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
/// does not renumber (a non-function import, an explicit rec group, a
/// non-function type or a custom section).
struct Shape {
    imports: u32,
    globals: u32,
    types: u32,
}

fn prunable(bytes: &[u8]) -> anyhow::Result<Option<Shape>> {
    use wasmparser::{CompositeInnerType, Payload, TypeRef};
    let mut s = Shape { imports: 0, globals: 0, types: 0 };
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
            Payload::CustomSection(_) => return Ok(None),
            _ => {}
        }
    }
    Ok(Some(s))
}

/// The functions the roots reach through the bodies' references (imports
/// are leaves).
fn live_functions(seen: &Prune) -> BTreeSet<u32> {
    let mut live: BTreeSet<u32> = BTreeSet::new();
    let mut work: Vec<u32> = seen.roots.funcs.iter().copied().collect();
    while let Some(f) = work.pop() {
        if !live.insert(f) {
            continue;
        }
        if let Some(body) = f.checked_sub(seen.imports).and_then(|d| seen.bodies.get(d as usize)) {
            work.extend(body.funcs.iter().copied().filter(|g| !live.contains(g)));
        }
    }
    live
}

/// `bytes` without the defined functions no root reaches, the function
/// imports and globals nothing live names, and the types only dead entries
/// used. The caller validates the result.
pub fn prune(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    prune_mapped(bytes).map(|(out, _)| out)
}

/// Old function index → new index (`None`: dropped).
pub type FuncIndexMap = Vec<Option<u32>>;

/// [`prune`] plus where each old function index went (`None`: dropped);
/// the map is `None` when the module was returned unchanged (#1315: a debug
/// build's line table follows its functions through this pass).
pub fn prune_mapped(bytes: &[u8]) -> anyhow::Result<(Vec<u8>, Option<FuncIndexMap>)> {
    let Some(shape) = prunable(bytes)? else { return Ok((bytes.to_vec(), None)) };
    let fail = |e: Error<Infallible>| anyhow::anyhow!("prune reencode: {e}");
    let mut seen = Prune { record: true, imports: shape.imports, ..Prune::default() };
    seen.parse_core_module(&mut wasm_encoder::Module::new(), wasmparser::Parser::new(0), bytes).map_err(fail)?;

    let live = live_functions(&seen);
    let n_funcs = shape.imports + seen.func_types.len() as u32;
    let own_type = |f: u32| match f.checked_sub(shape.imports) {
        None => seen.import_types[f as usize],
        Some(d) => seen.func_types[d as usize],
    };
    let live_bodies = live.iter().filter_map(|&f| f.checked_sub(shape.imports)).map(|d| &seen.bodies[d as usize]);
    let mut globals = seen.roots.globals.clone();
    let mut types = seen.roots.types.clone();
    for body in live_bodies {
        globals.extend(body.globals.iter().copied());
        types.extend(body.types.iter().copied());
    }
    types.extend(live.iter().map(|&f| own_type(f)));

    let fmap = compact(n_funcs, |f| live.contains(&f));
    let gmap = compact(shape.globals, |g| globals.contains(&g));
    let tmap = compact(shape.types, |t| types.contains(&t));

    let mut out = wasm_encoder::Module::new();
    let mut pass = Prune { imports: shape.imports, fmap, gmap, tmap, ..Prune::default() };
    pass.parse_core_module(&mut out, wasmparser::Parser::new(0), bytes).map_err(fail)?;
    Ok((drop_empty_sections(&out.finish(), !seen.tables_used)?, Some(pass.fmap)))
}

/// `bytes` without its empty element section and — when `unused_table`, so
/// nothing names one — its table section (#3136: 10 B of every module with
/// no function values). Every other section is copied as it is.
fn drop_empty_sections(bytes: &[u8], unused_table: bool) -> anyhow::Result<Vec<u8>> {
    use wasmparser::Payload;
    let mut out = wasm_encoder::Module::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload?;
        let skip = match &payload {
            Payload::TableSection(_) => unused_table,
            Payload::ElementSection(r) => r.count() == 0,
            _ => false,
        };
        if let (false, Some((id, range))) = (skip, payload.as_section()) {
            out.section(&wasm_encoder::RawSection { id, data: &bytes[range.start as usize..range.end as usize] });
        }
    }
    Ok(out.finish())
}
