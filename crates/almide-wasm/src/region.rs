//! The `consume(produce(scalars))` REGION WINDOW (#1961), structural leg.
//!
//! The incumbent's `region_alloc.rs` recognises a call whose single heap
//! argument is itself a call fed only scalars, and whose result is a
//! scalar: everything `produce` allocates is dead the moment `consume`
//! returns, so the pair runs inside a bump window that is reset
//! wholesale instead of being freed block by block (binarytrees'
//! `check(make(depth))` — the incumbent ran depth 18 in 0.49 s, the
//! structural leg's per-block RC frees in 1.08 s).
//!
//! On the structural leg the window is the allocator state itself:
//! `RegionSave` files the bump pointer and the sixteen size-class heads
//! into one fresh block and zeroes the heads (a window block must never
//! be filed where the outer program could take it), the two calls run,
//! `RegionRestore` copies the heads back, rewinds the bump pointer and
//! frees the save block. Blocks freed INSIDE the window vanish with it
//! (they were window blocks), the outer free lists are exactly what they
//! were, and nothing has to be cloned — the structural leg never frees
//! variant payloads through a region, so the incumbent's `__rgn_` copies
//! have no counterpart here.
//!
//! Soundness rests on nothing crossing the window edge:
//! * `consume` returns Int/Float/Bool/Unit — no window block escapes by
//!   value;
//! * both callees (and every fn they reach) are REGION-PURE: no effect
//!   fns, no globals (read or written), no host, string, map, set or
//!   collection module ops, no lambdas / fn values / computed calls,
//!   only scalar arithmetic, control flow, tuples, records, variant
//!   constructors and calls to other region-pure fns — so nothing inside
//!   can free or retain a block that was allocated OUTSIDE the window;
//! * every argument of both calls is a scalar expression built from
//!   literals, locals, scalar operators and region-pure calls, so the
//!   arguments themselves touch no outer block.
//!
//! Windows nest: an inner save block is a window block of the outer one
//! and dies with it.
//!
//! Knobs: `ALMIDE_REGION_DEBUG=1` prints the region-pure set and every
//! window site; `ALMIDE_REGION_OFF=1` disables the window (A/B of the
//! allocation high-water or of a suspected window bug).

use std::collections::HashSet;

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt, IrStmtKind, VarId};
use wasm_encoder::{BlockType, MemArg};

use crate::emitter::Emitter;
use crate::*;

/// Stdlib modules whose every fn is scalar-in / scalar-out and touches
/// no heap block (the incumbent's `region_safe_op` whitelist).
const SCALAR_MODULES: &[&str] = &["int", "float", "math", "bool"];

/// Size of the save block payload: the bump pointer + one head per class.
const SAVE_BYTES: u32 = 4 + 4 * FREELIST_CLASSES;

fn abs(offset: u32) -> MemArg {
    MemArg { offset: offset as u64, align: 2, memory_index: 0 }
}

fn class_slot(k: u32) -> i32 {
    (FREELIST_BASE + 4 * k) as i32
}

/// Resolve a Named callee the way `lower_call_at` does: the current
/// module's qualified name first, then the entry program's globals.
fn resolve_named(table: &FnTable, cur_module: Option<&str>, name: &str) -> Option<usize> {
    cur_module
        .and_then(|m| table.by_name.get(&format!("{m}.{name}")))
        .or_else(|| table.by_name.get(name))
        .copied()
}

/// The region-pure subset of `program_fns`, as table indices — a greatest
/// fixpoint: start from every candidate and drop a fn whose body reaches
/// anything outside the pure vocabulary (including a dropped fn), until
/// nothing changes.
pub(crate) fn region_pure_fns(
    ir: &IrProgram,
    program_fns: &[(&IrFunction, Option<String>, u32)],
    table: &FnTable,
) -> HashSet<usize> {
    let globals: HashSet<VarId> = ir.top_lets.iter().map(|t| t.var).collect();
    let mut ctors: HashSet<String> = HashSet::new();
    for decl in ir.type_decls.iter().chain(ir.modules.iter().flat_map(|m| m.type_decls.iter())) {
        if let almide_ir::IrTypeDeclKind::Variant { cases, .. } = &decl.kind {
            ctors.extend(cases.iter().map(|c| c.name.as_str().to_string()));
        }
    }
    let mut pure: HashSet<usize> = program_fns
        .iter()
        .enumerate()
        .filter(|(i, (f, _, _))| {
            !f.is_effect
                && !f.is_test
                && f.name.as_str() != "main"
                && table.infos[*i].refuse.is_none()
                && f.params.iter().all(|p| !p.is_mut)
        })
        .map(|(i, _)| i)
        .collect();
    loop {
        let before = pure.len();
        let snapshot = pure.clone();
        pure.retain(|&i| {
            let (f, qual, _) = &program_fns[i];
            let cur_module = qual.as_deref().and_then(|q| q.split('.').next());
            let cx = PureCx { table, cur_module, pure: &snapshot, globals: &globals, ctors: &ctors };
            expr_pure(&f.body, &cx)
        });
        if pure.len() == before {
            if std::env::var_os("ALMIDE_REGION_DEBUG").is_some() {
                let mut names: Vec<&str> = pure.iter().map(|&i| program_fns[i].0.name.as_str()).collect();
                names.sort_unstable();
                eprintln!("[region] pure fns: {}", names.join(" "));
            }
            return pure;
        }
    }
}

struct PureCx<'a> {
    table: &'a FnTable,
    cur_module: Option<&'a str>,
    pure: &'a HashSet<usize>,
    globals: &'a HashSet<VarId>,
    ctors: &'a HashSet<String>,
}

fn all_pure(es: &[IrExpr], cx: &PureCx) -> bool {
    es.iter().all(|e| expr_pure(e, cx))
}

fn stmts_pure(ss: &[IrStmt], cx: &PureCx) -> bool {
    ss.iter().all(|s| match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            expr_pure(value, cx)
        }
        IrStmtKind::Assign { var, value } => !cx.globals.contains(var) && expr_pure(value, cx),
        IrStmtKind::Expr { expr } => expr_pure(expr, cx),
        IrStmtKind::Comment { .. } => true,
        _ => false,
    })
}

fn call_pure(target: &CallTarget, args: &[IrExpr], cx: &PureCx) -> bool {
    if !all_pure(args, cx) {
        return false;
    }
    match target {
        CallTarget::Named { name } => {
            let name = name.as_str();
            cx.ctors.contains(name)
                || resolve_named(cx.table, cx.cur_module, name).is_some_and(|i| cx.pure.contains(&i))
        }
        CallTarget::Module { module, .. } => SCALAR_MODULES.contains(&module.as_str()),
        _ => false,
    }
}

/// Is this expression inside the region-pure vocabulary?
fn expr_pure(e: &IrExpr, cx: &PureCx) -> bool {
    use IrExprKind as K;
    match &e.kind {
        K::LitInt { .. } | K::LitFloat { .. } | K::LitStr { .. } | K::LitBool { .. } | K::Unit => true,
        K::OptionNone | K::Break | K::Continue => true,
        K::Var { id } => !cx.globals.contains(id),
        K::BinOp { left, right, .. } => expr_pure(left, cx) && expr_pure(right, cx),
        K::UnOp { operand, .. } => expr_pure(operand, cx),
        K::If { cond, then, else_ } => {
            expr_pure(cond, cx) && expr_pure(then, cx) && expr_pure(else_, cx)
        }
        K::Match { subject, arms } => {
            expr_pure(subject, cx)
                && arms.iter().all(|a| {
                    a.guard.as_ref().is_none_or(|g| expr_pure(g, cx)) && expr_pure(&a.body, cx)
                })
        }
        K::Block { stmts, expr } => {
            stmts_pure(stmts, cx) && expr.as_deref().is_none_or(|t| expr_pure(t, cx))
        }
        K::While { cond, body } => expr_pure(cond, cx) && stmts_pure(body, cx),
        K::ForIn { iterable, body, .. } => {
            matches!(iterable.kind, K::Range { .. }) && expr_pure(iterable, cx) && stmts_pure(body, cx)
        }
        K::Range { start, end, .. } => expr_pure(start, cx) && expr_pure(end, cx),
        K::Call { target, args, .. } => call_pure(target, args, cx),
        K::Tuple { elements } => all_pure(elements, cx),
        K::Record { fields, .. } => fields.iter().all(|(_, fe)| expr_pure(fe, cx)),
        K::Member { object, .. } | K::TupleIndex { object, .. } => expr_pure(object, cx),
        K::OptionSome { expr }
        | K::ResultOk { expr }
        | K::ResultErr { expr }
        | K::Unwrap { expr } => expr_pure(expr, cx),
        K::UnwrapOr { expr, fallback } => expr_pure(expr, cx) && expr_pure(fallback, cx),
        _ => false,
    }
}

// ── the window at a call site ────────────────────────────────────────

/// A scalar argument the window may evaluate: literals, locals, scalar
/// operators and region-pure calls over the same — nothing that could
/// read or release a block allocated outside the window.
fn scalar_arg(e: &IrExpr, cx: &PureCx) -> bool {
    use IrExprKind as K;
    match &e.kind {
        K::LitInt { .. } | K::LitFloat { .. } | K::LitBool { .. } | K::Unit => true,
        K::Var { id } => !cx.globals.contains(id),
        K::BinOp { left, right, .. } => scalar_arg(left, cx) && scalar_arg(right, cx),
        K::UnOp { operand, .. } => scalar_arg(operand, cx),
        K::Call { target, args, .. } => {
            args.iter().all(|a| scalar_arg(a, cx))
                && match target {
                    CallTarget::Named { name } => resolve_named(cx.table, cx.cur_module, name.as_str())
                        .is_some_and(|i| cx.pure.contains(&i)),
                    CallTarget::Module { module, .. } => SCALAR_MODULES.contains(&module.as_str()),
                    _ => false,
                }
        }
        _ => false,
    }
}

fn scalar_slot(t: Option<SliceTy>) -> bool {
    matches!(t, None | Some(SliceTy::Unit) | Some(SliceTy::Scalar(Scalar::Int | Scalar::Float | Scalar::Bool)))
}

impl<'a> Emitter<'a> {
    /// Does the Named call `consume` (table index `g`, returning `ret`,
    /// with `args` against `params`) open a region window? Yes when
    /// exactly one argument is heap-typed, that argument is a call to a
    /// region-pure fn over scalar arguments, every other argument is a
    /// scalar expression, `consume` itself is region-pure and returns a
    /// scalar.
    pub(crate) fn region_window_opens(
        &self,
        g: usize,
        ret: Option<SliceTy>,
        args: &[IrExpr],
        params: &[SliceTy],
    ) -> bool {
        let pure = self.work.region_pure.borrow();
        if std::env::var_os("ALMIDE_REGION_OFF").is_some() || pure.is_empty() || !pure.contains(&g) || !scalar_slot(ret) {
            return false;
        }
        let cx = PureCx {
            table: self.table,
            cur_module: self.cur_module,
            pure: &pure,
            globals: &self.region_globals(),
            ctors: &HashSet::new(),
        };
        let mut heap_args = 0usize;
        for (a, &p) in args.iter().zip(params) {
            if scalar_slot(Some(p)) {
                if !scalar_arg(a, &cx) {
                    return false;
                }
                continue;
            }
            heap_args += 1;
            let IrExprKind::Call { target: CallTarget::Named { name }, args: fargs, .. } = &a.kind
            else {
                return false;
            };
            let Some(f) = resolve_named(self.table, self.cur_module, name.as_str()) else {
                return false;
            };
            if !pure.contains(&f) || !fargs.iter().all(|x| scalar_arg(x, &cx)) {
                return false;
            }
            let fparams = &self.table.infos[f].params;
            if !fparams.iter().all(|&p| scalar_slot(Some(p))) {
                return false;
            }
        }
        if heap_args == 1 && std::env::var_os("ALMIDE_REGION_DEBUG").is_some() {
            eprintln!("[region] window at call #{g}");
        }
        heap_args == 1
    }

    fn region_globals(&self) -> HashSet<VarId> {
        self.globals.keys().map(|g| g.1).collect()
    }

    /// `RegionSave`: allocate the save block, file the bump pointer and
    /// the class heads into it, zero the heads. Returns the local holding
    /// the block (released by `emit_region_restore`).
    pub(crate) fn emit_region_save(&mut self) -> Result<u32, EmitError> {
        self.work.region_used.set(true);
        let blk = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i32_const(SAVE_BYTES as i32).call(F_ALLOC).local_set(blk);
        i.local_get(blk).global_get(G_HEAP).i32_store(abs(almide_layout::PAYLOAD));
        for k in 0..FREELIST_CLASSES {
            i.local_get(blk);
            i.i32_const(class_slot(k)).i32_load(abs(0));
            i.i32_store(abs(almide_layout::PAYLOAD + 4 + 4 * k));
            i.i32_const(class_slot(k)).i32_const(0).i32_store(abs(0));
        }
        Ok(blk)
    }

    /// `RegionRestore`: heads back, bump pointer back, the save block
    /// filed into the restored lists.
    pub(crate) fn emit_region_restore(&mut self, blk: u32) {
        {
            let mut i = self.f.instructions();
            // The window's peak is the bump pointer right now (frees
            // inside the window file blocks, they never lower it): raise
            // the high-water mark before rewinding.
            i.global_get(G_HEAP).global_get(G_HEAP_HIGH).i32_gt_u().if_(BlockType::Empty);
            i.global_get(G_HEAP).global_set(G_HEAP_HIGH);
            i.end();
            for k in 0..FREELIST_CLASSES {
                i.i32_const(class_slot(k));
                i.local_get(blk).i32_load(abs(almide_layout::PAYLOAD + 4 + 4 * k));
                i.i32_store(abs(0));
            }
            i.local_get(blk).i32_load(abs(almide_layout::PAYLOAD)).global_set(G_HEAP);
            i.local_get(blk).call(F_FREE);
        }
        self.release_i32();
    }
}

/// Keyed by table index; empty when no fn qualifies (the common case for
/// programs without a pure producer/consumer pair).
pub(crate) type RegionPure = std::cell::RefCell<HashSet<usize>>;
