//! The structural leg's NAME-TOTALITY and CAPABILITY witnesses (#2759,
//! #1696 step 4), projected from finished module bytes.
//!
//! The ownership witness is recorded while instructions are emitted
//! (witness.rs). These two are properties of the whole module, so they are
//! read off the bytes after emission, with a wasm decoder
//! (`wasmparser`, the validator the route already runs):
//!
//! - NAMES (`proofs/NameTotality.v`, `check_names_cert`): every index the
//!   code references is defined. One MODULE witness per index space shared
//!   by all functions — functions, globals, types, tables, data and element
//!   segments (`defined = 0 .. size`); its used side collects every
//!   reference in every body, and for functions also the element segments,
//!   the exports and the start function. One witness PER FUNCTION
//!   covers its locals (`0 .. params + declared locals` vs every
//!   `local.get/set/tee`). Memory indices are NOT witnessed (the module has
//!   one memory; the validator, not the kernel, checks a memarg names it).
//!
//! - CAPABILITIES (`proofs/CapabilityBound.v` flat, `CapabilityReach.v`
//!   graph): a function's DIRECT capabilities are the host operations its
//!   body calls — `almide.println` / `eprintln` are console output, each
//!   `almide.fs_call` is the capability of its op number (the first
//!   argument, read as an `i32.const` from the operand stack at the call; an
//!   op the stack does not show as a constant, or an op the table below does
//!   not know, is [`SENTINEL`], which no function declares), and a program
//!   `@extern(wasm, ..)` import is [`cap::FOREIGN`]. `exit` and `host_read`
//!   (the transfer of a result an op already produced) carry none. The
//!   CALL GRAPH is every `call` / `return_call` / `ref.func` to a defined
//!   function plus, for `call_indirect`, every element-segment function of
//!   the site's type. A function's DECLARED bound comes from its source
//!   (witness_decls.rs): a plain `fn` declares the console (output, and the
//!   stdin byte readers io.almd declares plain), an `effect fn` every
//!   modeled capability; a function the source does not
//!   declare (a runtime routine, a helper, a lifted lambda) gets the least
//!   bound its own reach needs, and the graph checker holds every caller to
//!   it. `check_prog_cert` therefore accepts only if no source-declared
//!   function's transitive reach leaves its declaration.
//!
//! The projector is the UNTRUSTED producer, like the recorder: its op table,
//! its stack reading and the declaration table are what the checker's
//! verdict is relative to (proofs/TOR.md names them).

use std::collections::{BTreeMap, BTreeSet};

use wasmparser::{
    BlockType, ElementItems, FuncType, FuncValidator, FunctionBody, Operator, Parser, Payload, TypeRef,
    ValidPayload, Validator, ValidatorResources,
};

use crate::witness::decls::{Declared, PassDecls};

/// The capability ids the witness speaks (the first seven are the
/// registry `almide_mir::Capability::id` fixes; NET and FOREIGN extend it
/// for the host surface only the wasm leg reaches).
pub mod cap {
    pub const STDOUT: u32 = 0;
    pub const ENTROPY: u32 = 1;
    /// Command-line arguments and the process environment.
    pub const ENV: u32 = 2;
    pub const FS_READ: u32 = 3;
    pub const FS_WRITE: u32 = 4;
    pub const CLOCK: u32 = 5;
    pub const STDIN: u32 = 6;
    pub const NET: u32 = 7;
    /// A program-declared `@extern(wasm, ..)` host function.
    pub const FOREIGN: u32 = 8;
}

/// A host operation the projector cannot name: no function declares it, so
/// a declared function that reaches one is rejected. Small on purpose — the
/// extracted checker's `nat` is Peano (Extract.v), so an id costs as many
/// cells as its value, and a sentinel of 10^6 in every tainted node made
/// one call-graph witness run for minutes.
pub const SENTINEL: u32 = 9;

/// What an `effect fn` declares: every modeled capability.
const EFFECT_BOUND: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8];
/// What a plain `fn` declares: the console. `println` is admitted in any
/// function, and io.almd declares its byte-level stdin readers
/// (`io.read_byte`, `io.read_n_bytes`) plain `fn` on purpose — the
/// "impure" category it documents. Files, the clock, entropy, the
/// environment, the network and foreign imports need `effect fn`.
const PURE_BOUND: &[u32] = &[cap::STDOUT, cap::STDIN];

/// The capabilities one `almide.fs_call` op reaches (op numbers:
/// fs_meta.rs and its siblings). An op outside the table is [`SENTINEL`].
pub fn op_caps(op: i32) -> &'static [u32] {
    use cap::*;
    match op {
        1 | 4..=6 | 11..=14 | 17 | 18 | 22..=25 | 38..=42 | 51 | 52 | 61..=64 => &[FS_READ],
        2 | 3 | 7..=10 | 15 | 16 | 20 | 21 => &[FS_WRITE],
        19 => &[FS_READ, FS_WRITE],
        26..=29 | 33 | 37 => &[ENV],
        30 | 73 => &[STDOUT],
        32 => &[ENTROPY],
        34 | 36 | 60 => &[CLOCK],
        35 => &[STDIN],
        43..=50 | 53..=59 | 70..=72 => &[NET],
        _ => &[SENTINEL],
    }
}

/// What one function body references and reaches.
#[derive(Default, Debug)]
struct Body {
    /// Params + declared locals.
    locals: u32,
    locals_used: BTreeSet<u32>,
    funcs: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    types: BTreeSet<u32>,
    tables: BTreeSet<u32>,
    data: BTreeSet<u32>,
    elems: BTreeSet<u32>,
    /// Type indices of its `call_indirect` sites.
    indirect: BTreeSet<u32>,
    /// Each `almide.fs_call` site's op, `None` when not a visible constant.
    fs_ops: Vec<Option<i32>>,
}

/// A decoded module: the index spaces and every body.
#[derive(Default, Debug)]
struct Module {
    /// Function imports, `(module, name)`, in index order.
    imports: Vec<(String, String)>,
    /// Type index of each defined function.
    func_types: Vec<u32>,
    types: Vec<Option<FuncType>>,
    globals: u32,
    tables: u32,
    data: u32,
    elems: u32,
    /// Functions named by an element segment.
    elem_funcs: BTreeSet<u32>,
    /// Functions named by an export or the start section.
    roots: BTreeSet<u32>,
    bodies: Vec<Body>,
}

impl Module {
    fn n_funcs(&self) -> u32 {
        (self.imports.len() + self.func_types.len()) as u32
    }

    fn import_named(&self, module: &str, name: &str) -> Option<u32> {
        self.imports.iter().position(|(m, n)| m == module && n == name).map(|i| i as u32)
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Decode `bytes`, scanning every body with the validator's stack view.
fn decode(bytes: &[u8]) -> Result<Module, String> {
    let mut m = Module::default();
    let mut validator = Validator::new();
    let mut funcs: Vec<(wasmparser::FuncToValidate<ValidatorResources>, FunctionBody<'_>)> = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(err)?;
        if let ValidPayload::Func(f, body) = validator.payload(&payload).map_err(err)? {
            funcs.push((f, body));
        }
        section(&mut m, &payload)?;
    }
    let fs_call = m.import_named("almide", "fs_call");
    for (f, body) in funcs {
        let fv = f.into_validator(Default::default());
        let b = scan_body(fv, &body, fs_call)?;
        m.bodies.push(b);
    }
    Ok(m)
}

/// The module-level facts of one section.
fn section(m: &mut Module, payload: &Payload<'_>) -> Result<(), String> {
    match payload {
        Payload::TypeSection(r) => type_section(m, r)?,
        Payload::ImportSection(r) => import_section(m, r)?,
        Payload::FunctionSection(r) => {
            for t in r.clone() {
                m.func_types.push(t.map_err(err)?);
            }
        }
        Payload::GlobalSection(r) => m.globals += r.count(),
        Payload::TableSection(r) => m.tables += r.count(),
        Payload::DataSection(r) => m.data = r.count(),
        Payload::StartSection { func, .. } => {
            m.roots.insert(*func);
        }
        Payload::ExportSection(r) => {
            for e in r.clone() {
                let e = e.map_err(err)?;
                if e.kind == wasmparser::ExternalKind::Func {
                    m.roots.insert(e.index);
                }
            }
        }
        Payload::ElementSection(r) => element_section(m, r)?,
        _ => {}
    }
    Ok(())
}

fn type_section(m: &mut Module, r: &wasmparser::TypeSectionReader<'_>) -> Result<(), String> {
    for group in r.clone() {
        for st in group.map_err(err)?.into_types() {
            m.types.push(match st.composite_type.inner {
                wasmparser::CompositeInnerType::Func(f) => Some(f),
                _ => None,
            });
        }
    }
    Ok(())
}

fn import_section(m: &mut Module, r: &wasmparser::ImportSectionReader<'_>) -> Result<(), String> {
    for i in r.clone().into_imports() {
        let i = i.map_err(err)?;
        match i.ty {
            TypeRef::Func(_) => m.imports.push((i.module.to_string(), i.name.to_string())),
            TypeRef::Global(_) => m.globals += 1,
            TypeRef::Table(_) => m.tables += 1,
            _ => {}
        }
    }
    Ok(())
}

fn element_section(m: &mut Module, r: &wasmparser::ElementSectionReader<'_>) -> Result<(), String> {
    for el in r.clone() {
        m.elems += 1;
        if let ElementItems::Functions(fs) = el.map_err(err)?.items {
            for f in fs {
                m.elem_funcs.insert(f.map_err(err)?);
            }
        }
    }
    Ok(())
}

/// One body: its references, and each `fs_call` site's op constant.
///
/// The operand stack is modeled one slot per value: a slot holds the
/// constant an `i32.const` pushed, and every other instruction marks the
/// slots it writes (its results, per the validator's arity) unknown. The
/// validator's own stack height keeps the model aligned through control
/// flow and unreachable code.
fn scan_body(
    mut fv: FuncValidator<ValidatorResources>,
    body: &FunctionBody<'_>,
    fs_call: Option<u32>,
) -> Result<Body, String> {
    let mut b = Body::default();
    let locals = body.get_locals_reader().map_err(err)?;
    for l in locals {
        let (count, ty) = l.map_err(err)?;
        fv.define_locals(0, count, ty).map_err(err)?;
    }
    b.locals = fv.len_locals();
    let mut ops = body.get_operators_reader().map_err(err)?;
    let mut slots: Vec<Option<i32>> = Vec::new();
    while !ops.eof() {
        let offset = ops.original_position();
        let op = ops.read().map_err(err)?;
        let arity = op.operator_arity(&fv);
        let before = fv.operand_stack_height() as usize;
        slots.resize(before, None);
        if let (Operator::Call { function_index }, Some(fc)) = (&op, fs_call)
            && *function_index == fc
        {
            b.fs_ops.push(before.checked_sub(5).and_then(|s| slots[s]));
        }
        note_refs(&mut b, &op);
        fv.op(offset, &op).map_err(err)?;
        let after = fv.operand_stack_height() as usize;
        if let Operator::I32Const { value } = op {
            slots.push(Some(value));
            continue;
        }
        let keep = arity.map_or(0, |(p, _)| before.saturating_sub(p as usize)).min(after);
        slots.truncate(keep);
        slots.resize(after, None);
    }
    Ok(b)
}

/// Record every index `op` names.
fn note_refs(b: &mut Body, op: &Operator<'_>) {
    use Operator::*;
    match op {
        LocalGet { local_index } | LocalSet { local_index } | LocalTee { local_index } => {
            b.locals_used.insert(*local_index);
        }
        GlobalGet { global_index } | GlobalSet { global_index } => {
            b.globals.insert(*global_index);
        }
        Call { function_index } | ReturnCall { function_index } | RefFunc { function_index } => {
            b.funcs.insert(*function_index);
        }
        CallIndirect { type_index, table_index } | ReturnCallIndirect { type_index, table_index } => {
            b.types.insert(*type_index);
            b.tables.insert(*table_index);
            b.indirect.insert(*type_index);
        }
        Block { blockty } | Loop { blockty } | If { blockty } => {
            if let BlockType::FuncType(t) = blockty {
                b.types.insert(*t);
            }
        }
        TableGet { table } | TableSet { table } | TableSize { table } | TableGrow { table } | TableFill { table } => {
            b.tables.insert(*table);
        }
        TableCopy { dst_table, src_table } => {
            b.tables.extend([*dst_table, *src_table]);
        }
        TableInit { elem_index, table } => {
            b.tables.insert(*table);
            b.elems.insert(*elem_index);
        }
        ElemDrop { elem_index } => {
            b.elems.insert(*elem_index);
        }
        MemoryInit { data_index, .. } | DataDrop { data_index } => {
            b.data.insert(*data_index);
        }
        _ => {}
    }
}

fn ids(xs: impl IntoIterator<Item = u32>) -> String {
    let set: BTreeSet<u32> = xs.into_iter().collect();
    set.iter().map(u32::to_string).collect::<Vec<_>>().join(" ")
}

/// The index spaces the module witnesses cover, in output order.
const SPACES: [&str; 6] = ["funcs", "globals", "types", "tables", "data", "elems"];

/// The name-totality witnesses of a module: one `(module:<space>)`
/// witness per index space (each space its own id range, so a dangling
/// index of one space can never read as a defined index of another), then
/// one `(label, w)` per defined function's locals (`label(global index)`
/// names it).
pub fn names(bytes: &[u8], label: &dyn Fn(u32) -> String) -> Result<Vec<(String, String)>, String> {
    let m = decode(bytes)?;
    let sizes = [m.n_funcs(), m.globals, m.types.len() as u32, m.tables, m.data, m.elems];
    let mut used: [BTreeSet<u32>; 6] = Default::default();
    used[0].extend(m.elem_funcs.iter().chain(&m.roots).copied());
    for b in &m.bodies {
        let spaces = [&b.funcs, &b.globals, &b.types, &b.tables, &b.data, &b.elems];
        for (k, s) in spaces.iter().enumerate() {
            used[k].extend(s.iter().copied());
        }
    }
    let mut out: Vec<(String, String)> = (0..SPACES.len())
        .map(|k| (format!("(module:{})", SPACES[k]), format!("{}|{}", ids(0..sizes[k]), ids(used[k].iter().copied()))))
        .collect();
    let n_imports = m.imports.len() as u32;
    for (j, b) in m.bodies.iter().enumerate() {
        let w = format!("{}|{}", ids(0..b.locals), ids(b.locals_used.iter().copied()));
        out.push((label(n_imports + j as u32), w));
    }
    Ok(out)
}

/// The number of function imports of a module (the offset of its first
/// defined function).
pub fn function_imports(bytes: &[u8]) -> Result<u32, String> {
    let mut n = 0u32;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(r) = payload.map_err(err)? {
            for i in r.into_imports() {
                n += u32::from(matches!(i.map_err(err)?.ty, TypeRef::Func(_)));
            }
        }
    }
    Ok(n)
}

/// Each declared function's name at its index in `decls.bytes` (the
/// declare post-pass renumbering applied).
pub fn decl_names(decls: &PassDecls) -> Result<BTreeMap<u32, String>, String> {
    let n_imports = function_imports(&decls.bytes)?;
    let base_imports = n_imports - decls.stubs.len() as u32;
    Ok(decls
        .fns
        .iter()
        .map(|f| (crate::imports::remap_index(f.index, &decls.stubs, base_imports), f.name.clone()))
        .collect())
}

/// The capability witnesses of the structural module `decls.bytes`:
/// `(flat, graph)` — one `declared|reach` witness per source-declared
/// function, and the `declared|direct|callees` call graph of every defined
/// function (`check_prog_cert`).
pub fn caps(decls: &PassDecls) -> Result<(Vec<(String, String)>, String), String> {
    let m = decode(&decls.bytes)?;
    let n_imports = m.imports.len() as u32;
    let callees: Vec<BTreeSet<usize>> = m.bodies.iter().map(|b| graph_edges(&m, b)).collect();
    // The source declaration of each defined function, renumbered into the
    // bytes' index space (the declare post-pass map).
    let base_imports = n_imports - decls.stubs.len() as u32;
    let mut declared: BTreeMap<usize, (String, Declared)> = BTreeMap::new();
    let mut meter: BTreeMap<usize, u32> = BTreeMap::new();
    for f in &decls.fns {
        let g = crate::imports::remap_index(f.index, &decls.stubs, base_imports);
        let Some(j) = g.checked_sub(n_imports).map(|j| j as usize).filter(|&j| j < m.bodies.len()) else { continue };
        if let Some(d) = f.declared {
            declared.insert(j, (f.name.clone(), d));
        }
        meter.insert(j, f.meter_clock_reads);
    }
    let direct: Vec<BTreeSet<u32>> = m
        .bodies
        .iter()
        .enumerate()
        .map(|(j, b)| direct_caps(&m, b, meter.get(&j).copied().unwrap_or(0)))
        .collect();
    let bound = least_bounds(&direct, &callees, &declared);
    let flat = declared
        .iter()
        .map(|(&j, (name, _))| (name.clone(), format!("{}|{}", ids(bound[j].iter().copied()), ids(reach(&direct, &callees, j)))))
        .collect();
    Ok((flat, condensed_graph(&direct, &callees, &bound)))
}

/// One condensed node: the members' bound intersection, direct
/// capabilities and callee components.
type Node = (Option<BTreeSet<u32>>, BTreeSet<u32>, BTreeSet<usize>);

/// The call graph for `check_prog_cert`, one node per STRONGLY CONNECTED
/// COMPONENT. The proof computes a node's reach by re-expanding its callees
/// to a depth of the node count, with no memo, so a cycle with a branch in
/// it multiplies the work at every level: the raw graph of one 79-function
/// fixture expands to 5.6 x 10^10 terms, its condensation to 118. Members
/// of a component reach exactly what the component reaches, so a node
/// carries the union of its members' direct capabilities, the components
/// its members call, and the INTERSECTION of their bounds — accepted iff
/// the reach is within every member's bound, the per-function claim.
fn condensed_graph(direct: &[BTreeSet<u32>], callees: &[BTreeSet<usize>], bound: &[BTreeSet<u32>]) -> String {
    let comp = components(callees);
    let n = comp.iter().copied().max().map_or(0, |c| c + 1);
    let mut nodes: Vec<Node> = vec![(None, BTreeSet::new(), BTreeSet::new()); n];
    for (j, &c) in comp.iter().enumerate() {
        let node = &mut nodes[c];
        node.0 = Some(match node.0.take() {
            None => bound[j].clone(),
            Some(b) => b.intersection(&bound[j]).copied().collect(),
        });
        node.1.extend(direct[j].iter().copied());
        node.2.extend(callees[j].iter().map(|&k| comp[k]).filter(|&k| k != c));
    }
    nodes
        .iter()
        .map(|(b, d, cs)| {
            let b = b.as_ref().map(|b| ids(b.iter().copied())).unwrap_or_default();
            format!("{b}|{}|{}", ids(d.iter().copied()), ids(cs.iter().map(|&c| c as u32)))
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Each function's strongly connected component (Tarjan, iterative — a
/// module's call chains are deeper than a comfortable recursion).
fn components(callees: &[BTreeSet<usize>]) -> Vec<usize> {
    let n = callees.len();
    let (mut index, mut low, mut comp) = (vec![usize::MAX; n], vec![0usize; n], vec![usize::MAX; n]);
    let (mut on_stack, mut stack, mut next, mut count) = (vec![false; n], Vec::new(), 0usize, 0usize);
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, Vec<usize>)> = vec![(root, callees[root].iter().copied().collect())];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some((v, pending)) = work.last_mut() {
            let v = *v;
            if let Some(w) = pending.pop() {
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, callees[w].iter().copied().collect()));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some((parent, _)) = work.last() {
                low[*parent] = low[*parent].min(low[v]);
            }
            if low[v] == index[v] {
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp[w] = count;
                    if w == v {
                        break;
                    }
                }
                count += 1;
            }
        }
    }
    comp
}

/// The host capabilities one body calls directly. The first `meter_reads`
/// wall-clock reads (op [`OP_WALL_NOW`]) are the fuel meter's deadline test,
/// which the declaration table charges to the region opener, not this frame
/// (witness_decls.rs, `DeclFn::meter_clock_reads`, #3041): they name no
/// capability here. Any read beyond that count is the source's.
fn direct_caps(m: &Module, b: &Body, meter_reads: u32) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for &f in b.funcs.iter().filter(|&&f| (f as usize) < m.imports.len()) {
        match m.imports[f as usize].0.as_str() {
            "almide" => match m.imports[f as usize].1.as_str() {
                "println" | "eprintln" => {
                    out.insert(cap::STDOUT);
                }
                "exit" | "host_read" | "fs_call" => {}
                _ => {
                    out.insert(SENTINEL);
                }
            },
            _ => {
                out.insert(cap::FOREIGN);
            }
        }
    }
    let mut meter_left = meter_reads;
    for op in &b.fs_ops {
        if *op == Some(OP_WALL_NOW) && meter_left > 0 {
            meter_left -= 1;
            continue;
        }
        out.extend(op.map_or(&[SENTINEL][..], op_caps).iter().copied());
    }
    out
}

/// The wall-clock read (`fs_meta.rs` `OP_WALL_NOW`).
const OP_WALL_NOW: i32 = crate::fs_meta::OP_WALL_NOW;

/// The defined functions one body can transfer control to (positions in
/// the defined-function list).
fn graph_edges(m: &Module, b: &Body) -> BTreeSet<usize> {
    let n_imports = m.imports.len() as u32;
    let mut out: BTreeSet<usize> = b.funcs.iter().filter_map(|&f| f.checked_sub(n_imports)).map(|j| j as usize).collect();
    let ty_of = |t: u32| m.types.get(t as usize).cloned().flatten();
    for &t in &b.indirect {
        let want = ty_of(t);
        for &f in &m.elem_funcs {
            let Some(j) = f.checked_sub(n_imports) else { continue };
            let ft = m.func_types.get(j as usize).and_then(|&ti| ty_of(ti));
            if ft.is_some() && ft == want {
                out.insert(j as usize);
            }
        }
    }
    out
}

/// Each function's declared bound: its source declaration when it has one,
/// else the least set covering its direct capabilities and its callees'
/// bounds (a fixpoint — the call graph has cycles).
fn least_bounds(
    direct: &[BTreeSet<u32>],
    callees: &[BTreeSet<usize>],
    declared: &BTreeMap<usize, (String, Declared)>,
) -> Vec<BTreeSet<u32>> {
    let fixed = |j: usize| {
        declared.get(&j).map(|(_, d)| match d {
            Declared::Pure => PURE_BOUND.iter().copied().collect::<BTreeSet<u32>>(),
            Declared::Effect => EFFECT_BOUND.iter().copied().collect(),
        })
    };
    let mut bound: Vec<BTreeSet<u32>> = (0..direct.len()).map(|j| fixed(j).unwrap_or_else(|| direct[j].clone())).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for j in (0..direct.len()).filter(|j| !declared.contains_key(j)) {
            let add: Vec<u32> = callees[j].iter().flat_map(|&c| bound[c].iter().copied()).filter(|c| !bound[j].contains(c)).collect();
            if !add.is_empty() {
                bound[j].extend(add);
                changed = true;
            }
        }
    }
    bound
}

/// The direct capabilities of every function reachable from `start`.
fn reach(direct: &[BTreeSet<u32>], callees: &[BTreeSet<usize>], start: usize) -> Vec<u32> {
    let mut seen = vec![false; direct.len()];
    let mut stack = vec![start];
    seen[start] = true;
    let mut out = BTreeSet::new();
    while let Some(j) = stack.pop() {
        out.extend(direct[j].iter().copied());
        for &c in &callees[j] {
            if !seen[c] {
                seen[c] = true;
                stack.push(c);
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_op_names_a_capability_and_an_unknown_one_the_sentinel() {
        assert_eq!(op_caps(1), &[cap::FS_READ]);
        assert_eq!(op_caps(2), &[cap::FS_WRITE]);
        assert_eq!(op_caps(32), &[cap::ENTROPY]);
        assert_eq!(op_caps(34), &[cap::CLOCK]);
        assert_eq!(op_caps(35), &[cap::STDIN]);
        assert_eq!(op_caps(31), &[SENTINEL]);
        assert_eq!(op_caps(-1), &[SENTINEL]);
    }

    #[test]
    fn a_cycle_condenses_to_one_node_bounded_by_every_member() {
        // 0 -> 1 -> 2 -> 1 (a cycle), 2 reaches FS_READ; 1 is declared pure.
        let direct = vec![BTreeSet::new(), BTreeSet::new(), [cap::FS_READ].into()];
        let callees: Vec<BTreeSet<usize>> = vec![[1].into(), [2].into(), [1].into()];
        let comp = components(&callees);
        assert_eq!(comp[1], comp[2]);
        assert_ne!(comp[0], comp[1]);
        let bound: Vec<BTreeSet<u32>> = vec![EFFECT_BOUND.iter().copied().collect(), PURE_BOUND.iter().copied().collect(), [cap::FS_READ, cap::STDOUT].into()];
        let g = condensed_graph(&direct, &callees, &bound);
        // The cycle's node: bound = pure ∩ {0,3} = {0}, direct {3} — rejected.
        assert!(g.split(';').any(|n| n == "0|3|"), "{g}");
    }

    #[test]
    fn an_undeclared_function_takes_the_least_bound_its_callees_need() {
        // 0 declared pure, calls 1 (helper) which calls 2 (reaches FS_READ).
        let direct = vec![BTreeSet::new(), BTreeSet::new(), [cap::FS_READ].into()];
        let callees = vec![[1].into(), [2].into(), BTreeSet::new()];
        let declared: BTreeMap<usize, (String, Declared)> = [(0, ("f".to_string(), Declared::Pure))].into();
        let b = least_bounds(&direct, &callees, &declared);
        assert_eq!(b[1], [cap::FS_READ].into());
        assert_eq!(reach(&direct, &callees, 0), vec![cap::FS_READ]);
    }

    #[test]
    fn only_the_meters_own_clock_reads_leave_the_frame() {
        // #3041: two wall-clock reads and a file read; one read is the meter's.
        let m = Module::default();
        let b = Body { fs_ops: vec![Some(OP_WALL_NOW), Some(1), Some(OP_WALL_NOW)], ..Body::default() };
        assert_eq!(direct_caps(&m, &b, 0), [cap::FS_READ, cap::CLOCK].into());
        assert_eq!(direct_caps(&m, &b, 1), [cap::FS_READ, cap::CLOCK].into(), "the second read is the frame's own");
        assert_eq!(direct_caps(&m, &b, 2), [cap::FS_READ].into());
        // A read the stack does not show as the constant is never discounted.
        let hidden = Body { fs_ops: vec![None], ..Body::default() };
        assert_eq!(direct_caps(&m, &hidden, 1), [SENTINEL].into());
    }
}
