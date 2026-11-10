//! One generated type per declared record on the native MIR path.
//!
//! A structural record type IS the declared record type with its field names
//! and field types (`almide_ir::record_shape`, #3189): the checker unifies the
//! two. It spells the declared type on a literal that meets it directly
//! (`let r: R = { .. }`, `some({ .. })` against `Option[R]`), and leaves the
//! structural spelling where the literal sits inside another value first
//! (`let t = (s, { .. })`, later returned as `(String, R)`). The native render
//! generates one Rust type per type spelling, so the second form built a
//! struct of its own beside the declared one and rustc refused the function
//! that returns it under the declared signature.
//!
//! This pass rewrites every structural record type a declared record matches
//! into that declared type (a generic one with the arguments its fields
//! bind), in every type position of the program, before any generated type is
//! derived. A record no declared type matches, or a generic one whose
//! parameters the fields do not all bind, keeps its structural type.
use almide_ir::{record_shape::RecordShapeIndex, visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor}, *};
use almide_lang::types::Ty;

pub(crate) fn expand(ir: &mut IrProgram) {
    let shapes = RecordShapeIndex::build(ir, |d| d.name.to_string());
    let mut pass = Nominal { shapes: &shapes };
    for info in &mut ir.var_table.entries { pass.set(&mut info.ty); }
    for d in &mut ir.type_decls { pass.decl(d); }
    for f in &mut ir.functions {
        for p in &mut f.params { pass.set(&mut p.ty); }
        pass.set(&mut f.ret_ty);
        pass.visit_expr_mut(&mut f.body);
    }
    for t in &mut ir.top_lets {
        pass.set(&mut t.ty);
        pass.visit_expr_mut(&mut t.value);
    }
}

struct Nominal<'a> {
    shapes: &'a RecordShapeIndex,
}

/// `t` with each structural record a declared record matches spelled as that
/// record, innermost first.
pub(crate) fn declared(t: &Ty, shapes: &RecordShapeIndex) -> Ty {
    let t = t.map_children(&|c| declared(c, shapes));
    let Ty::Record { fields } = &t else { return t };
    let named = shapes.lookup(fields).and_then(|(shape, bound)| {
        let args = bound.into_iter().collect::<Option<Vec<Ty>>>()?;
        Some(Ty::Named(shape.name, args.into()))
    });
    named.unwrap_or(t)
}

impl Nominal<'_> {
    fn set(&self, t: &mut Ty) {
        *t = declared(t, self.shapes);
    }

    fn decl(&self, d: &mut IrTypeDecl) {
        match &mut d.kind {
            IrTypeDeclKind::Record { fields } => fields.iter_mut().for_each(|f| self.set(&mut f.ty)),
            IrTypeDeclKind::Variant { cases, .. } => cases.iter_mut().for_each(|c| self.case(c)),
            IrTypeDeclKind::Alias { .. } => {}
        }
    }

    fn case(&self, c: &mut IrVariantDecl) {
        match &mut c.kind {
            IrVariantKind::Unit => {}
            IrVariantKind::Tuple { fields } => fields.iter_mut().for_each(|t| self.set(t)),
            IrVariantKind::Record { fields } => fields.iter_mut().for_each(|f| self.set(&mut f.ty)),
        }
    }
}

impl IrMutVisitor for Nominal<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        self.set(&mut e.ty);
        match &mut e.kind {
            IrExprKind::Lambda { params, .. } => params.iter_mut().for_each(|(_, t)| self.set(t)),
            IrExprKind::Call { type_args, .. } => type_args.iter_mut().for_each(|t| self.set(t)),
            _ => {}
        }
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { ty, .. } = &mut s.kind { self.set(ty); }
    }

    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        walk_pattern_mut(self, p);
        if let IrPattern::Bind { ty, .. } = p { self.set(ty); }
    }
}
