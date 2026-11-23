//! A loop whose only use of a list is appending to it keeps the list's length
//! in a local (#4906; ADR-0037 implementation notes: appends in loops).
//!
//! A native `List` is one shared block whose length and owner count live in the
//! block header (ADR-0037 D1), so each `push` reloads the length and capacity,
//! rechecks the count and stores the length back: an element store through the
//! data pointer may alias the header as far as the backend knows. The certified
//! render spells such a loop with an appender (`render_native_appends`); this is
//! the same lowering for the standard render. A loop whose every mention of the
//! list `xs` is one append statement of one element is rendered as
//!
//! ```text
//! {
//! let mut xs__push = xs.pusher();   // data, length, capacity in locals
//! while c {
//!     xs__push.push(x);             // was `almide_rt_list_push(&mut xs, x);` / `xs.push(x);`
//! }
//! }                                 // the length is written back here
//! ```
//!
//! The first push makes the block unique with room, exactly where `push` would
//! copy a shared block. Nothing else in the loop names `xs` — no read, clone,
//! move, call, rebinding or other write; the rendered text is checked, so a
//! spelling this module does not know keeps the plain loop — so nobody observes
//! the stale header while the appender lives, and leaving the loop by any path
//! (`break`, `return`, `?`, a panic) drops the appender, which writes the length
//! back. A loop that holds a closure keeps the plain loop: a push inside a
//! closure is not a push of this loop's pass.

use super::RenderContext;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::annotations::VarStorage;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};

/// `rendered` (the loop rendered from `heads` and `body`), spelled with an
/// appender when one list is only appended to in it; otherwise unchanged.
pub(super) fn with_appender(ctx: &RenderContext, heads: &[&IrExpr], body: &[IrStmt], rendered: String) -> String {
    let mut scan = Scan { pushes: Vec::new(), closure: false };
    for h in heads {
        scan.visit_expr(h);
    }
    for s in body {
        scan.visit_stmt(s);
    }
    if scan.closure || rendered.contains("move ") || rendered.contains("spawn") {
        return rendered;
    }
    let mut seen: Vec<VarId> = Vec::new();
    for &id in &scan.pushes {
        // A global or a copy-on-write cell is not a plain place the appender can borrow.
        if seen.contains(&id) || ctx.ann.global(id).is_some() || ctx.ann.get_var_storage(&id) == VarStorage::RcCow {
            continue;
        }
        seen.push(id);
        let count = scan.pushes.iter().filter(|&&x| x == id).count();
        if let Some(out) = rewrite(&ctx.var_name(id), count, &rendered) {
            return out;
        }
    }
    rendered
}

/// The append statements of the loop (one entry per statement, by target) and
/// whether it holds a closure.
struct Scan {
    pushes: Vec<VarId>,
    closure: bool,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::Fan { .. }) {
            self.closure = true;
            return;
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Expr { expr } = &s.kind
            && let Some(id) = push_target(expr)
        {
            self.pushes.push(id);
        }
        walk_stmt(self, s);
    }
}

/// The list var a `push` statement appends one element to: the runtime call
/// `almide_rt_list_push(xs, x)` or the method `xs.push(x)` of the `xs = xs + [x]`
/// rewrite, on a `List` var.
fn push_target(e: &IrExpr) -> Option<VarId> {
    let target = match &e.kind {
        IrExprKind::RuntimeCall { symbol, args } if symbol.as_str() == "almide_rt_list_push" && args.len() == 2 => &args[0],
        IrExprKind::Call { target: CallTarget::Method { object, method }, args, .. } if method.as_str() == "push" && args.len() == 1 => object,
        _ => return None,
    };
    let inner = match &target.kind {
        IrExprKind::Borrow { expr, mutable: true, .. } => expr,
        _ => target,
    };
    let IrExprKind::Var { id } = &inner.kind else { return None };
    matches!(&inner.ty, Ty::Applied(TypeConstructorId::List, _)).then_some(*id)
}

/// The loop text with the `count` appends to `name` going through an appender,
/// when those appends are every mention of `name` in it and the appender's name
/// is free; `None` otherwise.
fn rewrite(name: &str, count: usize, rendered: &str) -> Option<String> {
    let p = format!("{name}__push");
    // A raw identifier (`r#type`) has no plain suffixed spelling.
    if name.starts_with("r#") || mentions(rendered, &p) > 0 {
        return None;
    }
    let spellings = [format!("almide_rt_list_push(&mut {name}, "), format!("{name}.push(")];
    let mut done = 0;
    let mut lines: Vec<String> = Vec::new();
    for l in rendered.lines() {
        let n = mentions(l, name);
        let t = l.trim_start();
        let ind = &l[..l.len() - t.len()];
        match spellings.iter().find_map(|s| t.strip_prefix(s.as_str())) {
            Some(rest) if n == 1 => {
                lines.push(format!("{ind}{p}.push({rest}"));
                done += 1;
            }
            _ if n == 0 => lines.push(l.to_string()),
            _ => return None,
        }
    }
    (done == count && done > 0).then(|| format!("{{\nlet mut {p} = {name}.pusher();\n{}\n}}", lines.join("\n")))
}

/// Whole-word occurrences of the identifier `v` in `t`.
fn mentions(t: &str, v: &str) -> usize {
    let b = t.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    t.match_indices(v)
        .filter(|(k, _)| (*k == 0 || !word(b[k - 1])) && b.get(k + v.len()).is_none_or(|c| !word(*c)))
        .count()
}

#[cfg(test)]
mod tests {
    use super::rewrite;

    #[test]
    fn a_loop_that_only_pushes_a_list_takes_an_appender() {
        let l = "while (j < n) {\n    almide_rt_list_push(&mut merged, j);\n    j = (j + 1i64);\n}";
        let got = rewrite("merged", 1, l).unwrap();
        assert_eq!(
            got,
            "{\nlet mut merged__push = merged.pusher();\nwhile (j < n) {\n    merged__push.push(j);\n    j = (j + 1i64);\n}\n}"
        );
        let m = "for x in xs.iter().cloned() {\n    out.push(x);\n}";
        assert!(rewrite("out", 1, m).unwrap().contains("    out__push.push(x);"));
    }

    #[test]
    fn any_other_mention_of_the_list_keeps_the_plain_loop() {
        // read in the condition, read in the pushed value, a second statement, a rebinding inside the body
        for l in [
            "while (almide_rt_list_len(&merged) < 9i64) {\n    almide_rt_list_push(&mut merged, 1i64);\n}",
            "while c {\n    almide_rt_list_push(&mut merged, almide_rt_list_len(&merged));\n}",
            "while c {\n    almide_rt_list_push(&mut merged, 1i64);\n    g(&merged);\n}",
            "while c {\n    let mut merged = f();\n    almide_rt_list_push(&mut merged, 1i64);\n}",
            "while c {\n    almide_rt_list_push(merged.make_mut(), 1i64);\n}",
        ] {
            assert_eq!(rewrite("merged", 1, l), None, "{l}");
        }
        // the IR counted two appends, the text spells one
        assert_eq!(rewrite("merged", 2, "while c {\n    merged.push(1i64);\n}"), None);
        // the appender's name is taken
        assert_eq!(rewrite("merged", 1, "while c {\n    merged.push(merged__push);\n}"), None);
        // a longer identifier is not a mention
        assert!(rewrite("xs", 1, "while c {\n    xs.push(1i64);\n    xs2 = 3i64;\n}").is_some());
    }
}
