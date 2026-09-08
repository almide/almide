//! DecodeSlotHintPass (#1679, slot-indexed field lookup): hand every field
//! read in a DERIVED `T.decode` the declaration index of its field.
//!
//! An object is an insertion-ordered association list, so
//! `almide_rt_value_field` is a linear scan and decoding an n-field record is
//! O(n²) key compares. The derive knows where each field sits in the
//! declaration — and `T.encode` writes keys in that order, so a round-tripped
//! (or declaration-ordered hand-written) document carries them there too. The
//! derive stamps the key order on the fn as `@codec_slots("k0", "k1", …)`;
//! this pass turns each borrowed `&(almide_rt_value_field(_v, "k_i"))?` in
//! that body into `&(almide_rt_value_field_at(_v, "k_i", i))?`, and the
//! walker's borrow fold renders that as `almide_rt_value_field_ref_at(_v,
//! "k_i", i)?` exactly as it renders the plain shape as `_ref` — the runtime
//! tries `pairs[i]` first and falls back to the scan, so the value and the
//! two error strings (C-084) are the same on every document.
//!
//! Scope: only a fn carrying `@codec_slots` (the derive is its sole author),
//! only the borrowed-lookup shape, and only a lookup on that fn's `Value`
//! parameter with a literal key that is one of the declared slots. A
//! `value.field` a user writes stays on the plain path. Native only: the
//! wasm leg neither reads the attribute nor carries the `_at` symbol, so its
//! output is untouched.

use almide_base::intern::sym;
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut, walk_stmt_mut};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

const SLOTS_ATTR: &str = "codec_slots";
const FIELD_SYM: &str = "almide_rt_value_field";
const FIELD_AT_SYM: &str = "almide_rt_value_field_at";

#[derive(Debug)]
pub struct DecodeSlotHintPass;

impl NanoPass for DecodeSlotHintPass {
    fn name(&self) -> &str { "DecodeSlotHint" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["BuiltinLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            changed |= hint_function(func);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                changed |= hint_function(func);
            }
        }
        PassResult { program, changed }
    }
}

/// The declared key order of a derived decode, or `None` for any other fn.
fn codec_slots(func: &IrFunction) -> Option<Vec<String>> {
    let attr = func.attrs.iter().find(|a| a.name.as_str() == SLOTS_ATTR)?;
    attr.args.iter().map(|a| match &a.value {
        almide_lang::ast::AttrValue::String { value } => Some(value.clone()),
        _ => None,
    }).collect()
}

fn hint_function(func: &mut IrFunction) -> bool {
    let Some(slots) = codec_slots(func) else { return false };
    let Some(doc) = func.params.first().map(|p| p.var) else { return false };
    let mut rw = Rewriter { doc, slots, changed: false };
    rw.visit_expr_mut(&mut func.body);
    rw.changed
}

struct Rewriter {
    doc: VarId,
    slots: Vec<String>,
    changed: bool,
}

impl Rewriter {
    /// `_v` or `&_v`, the shapes the document parameter takes as a call
    /// argument on either side of BorrowInsertion.
    fn is_doc(&self, e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Var { id } => *id == self.doc,
            IrExprKind::Borrow { expr, mutable: false, .. } => matches!(&expr.kind, IrExprKind::Var { id } if *id == self.doc),
            _ => false,
        }
    }

    /// The key literal, bare or as the `&str` borrow BorrowInsertion wraps it in.
    fn slot_of(&self, args: &[IrExpr]) -> Option<usize> {
        if args.len() != 2 || !self.is_doc(&args[0]) { return None; }
        let key = match &args[1].kind {
            IrExprKind::LitStr { value } => value,
            IrExprKind::Borrow { expr, as_str: true, .. } => match &expr.kind {
                IrExprKind::LitStr { value } => value,
                _ => return None,
            },
            _ => return None,
        };
        self.slots.iter().position(|k| k == key)
    }
}

impl IrMutVisitor for Rewriter {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        // Only the borrowed-lookup shape, `&(almide_rt_value_field(_v, "k"))?`
        // — the one the walker folds into `_ref` today and into `_ref_at`
        // with the hint. A defaulted field reads its lookup as a `match`
        // subject instead and keeps the plain scan.
        let IrExprKind::Borrow { expr: borrowed, as_str: false, mutable: false } = &mut expr.kind else { return };
        let IrExprKind::Try { expr: tried } = &mut borrowed.kind else { return };
        let (is_field, args) = match &mut tried.kind {
            IrExprKind::RuntimeCall { symbol, args } => (symbol.as_str() == FIELD_SYM, args),
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => (name.as_str() == FIELD_SYM, args),
            _ => return,
        };
        if !is_field { return; }
        let Some(slot) = self.slot_of(args) else { return };
        let mut args = std::mem::take(args);
        args.push(IrExpr { kind: IrExprKind::LitInt { value: slot as i64 }, ty: Ty::Int, span: None, def_id: None });
        tried.kind = IrExprKind::RuntimeCall { symbol: sym(FIELD_AT_SYM), args };
        self.changed = true;
    }
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
    }
}
