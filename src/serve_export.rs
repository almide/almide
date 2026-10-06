//! The stock serve export's shape rule (#2659, ADR-0020 §5.4, C-375).
//!
//! `almide build --target wasm` of a program that calls `http.serve` emits a
//! `wasi:http/handler@0.3.0` component. On that host `main` is not an entry
//! point and the host owns the address, so the build accepts only a `main`
//! that is SERVE-SHAPED: its body is exactly one call `http.serve(port, app)`
//! (a trailing `!` allowed), optionally preceded by `let`s. The app is
//! instance-closed (E095, #2698), so those `let`s can be read only by `port`
//! or by each other; neither `port` nor the `let`s is evaluated on the export
//! host.
//!
//! [`rewrite_main_for_export`] is the build's one source rewrite: it drops the
//! `let`s and replaces `port` with `0`, so the main the handler export calls
//! per request evaluates the app and serves exactly the request in hand
//! (`stdlib/http_serve.almd` over the guest-side ops the p3 service shim
//! answers, `crates/almide-wasm-run/src/wasi_p3_serve.rs`).

use crate::ast::{Decl, Expr, ExprKind, Program, Stmt};

/// Why a program that calls `http.serve` does not build as the export, in the
/// words the E081 reason prints.
pub const SHAPE_RULE: &str = "the stock wasm artifact of a server is a wasi:http/handler@0.3.0 export, where `main` is not run \
     and the host owns the address: `main`'s body must be exactly one call `http.serve(port, app)`, optionally preceded by \
     `let`s that only `port` reads — move other setup into a top-level `let` or into the handler, and drop readiness lines \
     (the host prints its own)";

fn main_body(program: &Program) -> Option<&Expr> {
    program.decls.iter().find_map(|d| match d {
        Decl::Fn { name, body: Some(b), .. } if name.as_str() == "main" => Some(b),
        _ => None,
    })
}

fn is_serve_call(e: &Expr) -> bool {
    let call = match &e.kind {
        ExprKind::Unwrap { expr } | ExprKind::Paren { expr } => expr.as_ref(),
        _ => e,
    };
    matches!(&call.kind, ExprKind::Call { callee, args, named_args, .. }
        if args.len() == 2 && named_args.is_empty() && matches!(&callee.kind,
            ExprKind::Member { object, field } if field.as_str() == "serve"
                && matches!(&object.kind, ExprKind::Ident { name } if name.as_str() == "http")))
}

/// The statements of a serve-shaped body before its call, and the call;
/// `None` when the body is not serve-shaped.
fn shape(body: &Expr) -> Option<(&[Stmt], &Expr)> {
    let (stmts, last): (&[Stmt], &Expr) = match &body.kind {
        ExprKind::Block { stmts, expr: Some(e) } => (stmts.as_slice(), e.as_ref()),
        ExprKind::Block { stmts, expr: None } => match stmts.split_last() {
            Some((Stmt::Expr { expr, .. }, rest)) => (rest, expr),
            _ => return None,
        },
        _ => (&[], body),
    };
    let lets_only = stmts.iter().all(|s| matches!(s, Stmt::Let { .. } | Stmt::LetDestructure { mutable: false, .. } | Stmt::Comment { .. }));
    (lets_only && is_serve_call(last)).then_some((stmts, last))
}

/// `Ok` when `main` is serve-shaped, else the shape rule as the reason.
pub fn check_serve_shape(program: &Program) -> Result<(), String> {
    match main_body(program).and_then(shape) {
        Some(_) => Ok(()),
        None => Err(SHAPE_RULE.to_string()),
    }
}

/// Rewrite a serve-shaped `main` into the per-request body the export runs:
/// the `let`s dropped, `port` replaced by `0` (the host owns the address).
pub fn rewrite_main_for_export(program: &mut Program) -> Result<(), String> {
    check_serve_shape(program)?;
    for d in program.decls.iter_mut() {
        let Decl::Fn { name, body: Some(body), .. } = d else { continue };
        if name.as_str() != "main" {
            continue;
        }
        let mut call = match &mut body.kind {
            ExprKind::Block { stmts, expr } => match expr.take() {
                Some(e) => *e,
                None => match stmts.pop() {
                    Some(Stmt::Expr { expr, .. }) => expr,
                    _ => return Err(SHAPE_RULE.to_string()),
                },
            },
            _ => body.clone(),
        };
        let target = match &mut call.kind {
            ExprKind::Unwrap { expr } | ExprKind::Paren { expr } => expr.as_mut(),
            _ => &mut call,
        };
        if let ExprKind::Call { args, .. } = &mut target.kind {
            let port = &mut args[0];
            port.kind = ExprKind::Int { value: serde_json::Value::from(0), raw: "0".to_string() };
        }
        *body = call;
        return Ok(());
    }
    Err(SHAPE_RULE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Program {
        crate::wasm_leg::parse_entry("t.almd", src).expect("parses")
    }

    const APP: &str = "import http\neffect fn app(req: HttpRequest) -> HttpResponse = http.response(200, \"x\")\n";

    #[test]
    fn a_main_of_one_serve_call_after_lets_is_serve_shaped() {
        for main in [
            "effect fn main() -> Unit = http.serve(8080, app)!",
            "effect fn main() -> Unit = {\n  let p = 8080\n  http.serve(p, app)!\n}",
            "effect fn main() -> Unit = {\n  let a = 80\n  let p = a + 1\n  http.serve(p, app)\n}",
        ] {
            assert!(check_serve_shape(&parse(&format!("{APP}{main}\n"))).is_ok(), "{main}");
        }
    }

    #[test]
    fn any_other_statement_in_main_breaks_the_shape() {
        for main in [
            "effect fn main() -> Unit = {\n  println(\"ready\")\n  http.serve(8080, app)!\n}",
            "effect fn main() -> Unit = {\n  http.serve(8080, app)!\n  println(\"bye\")\n}",
            "effect fn main() -> Unit = {\n  var p = 8080\n  http.serve(p, app)!\n}",
            "effect fn main() -> Unit = println(\"no server\")",
        ] {
            assert_eq!(check_serve_shape(&parse(&format!("{APP}{main}\n"))), Err(SHAPE_RULE.to_string()), "{main}");
        }
    }

    #[test]
    fn the_rewrite_drops_the_lets_and_serves_on_port_zero() {
        let mut p = parse(&format!("{APP}effect fn main() -> Unit = {{\n  let p = 8080\n  http.serve(p, app)!\n}}\n"));
        rewrite_main_for_export(&mut p).expect("serve-shaped");
        let body = main_body(&p).expect("main");
        let ExprKind::Unwrap { expr } = &body.kind else { panic!("the call keeps its `!`: {body:?}") };
        let ExprKind::Call { args, .. } = &expr.kind else { panic!("a call: {expr:?}") };
        assert!(matches!(&args[0].kind, ExprKind::Int { raw, .. } if raw == "0"), "{:?}", args[0]);
    }
}
