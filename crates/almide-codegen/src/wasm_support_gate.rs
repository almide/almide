//! WASM target support gate.
//!
//! Some runtime intrinsics exist only for the native cdylib bridge and have no
//! WASM lowering. Reaching `emit_wasm` with one of them used to ICE in the
//! RuntimeCall dispatch (`[ICE] emit_wasm: RuntimeCall … no WASM runtime fn`),
//! which reads as a compiler bug even though the real cause is a user program
//! using a native-only feature on `--target wasm`.
//!
//! This gate runs before WASM emit and turns exactly that case into a clean,
//! source-located build error. It is deliberately a CLOSED list of the
//! intentionally-native-only symbols, not a catch-all: an *unregistered* symbol
//! is still a genuine compiler bug and must keep ICE-ing in `emit_wasm`, so the
//! two stay distinguishable (#440).

use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};

/// The RawPtr / linear-memory bridge runtime symbols. They are implemented only
/// in `runtime/rs/src/bytes.rs` (all `unsafe` raw-pointer ops) for the native
/// cdylib path; the wasm linear-memory bridge is not implemented yet (#440).
/// Listing them here, rather than treating every unknown symbol as native-only,
/// keeps a real "forgot to register a wasm runtime fn" bug ICE-ing as before.
const NATIVE_ONLY_PTR_SYMBOLS: &[&str] = &[
    "almide_rt_bytes_as_ptr",
    "almide_rt_bytes_as_mut_ptr",
    "almide_rt_bytes_from_raw_ptr",
    "almide_rt_bytes_copy_to_ptr",
];

fn is_native_only_ptr_symbol(symbol: &str) -> bool {
    NATIVE_ONLY_PTR_SYMBOLS.contains(&symbol)
}

/// `bytes.as_mut_ptr` from `almide_rt_bytes_as_mut_ptr`.
fn friendly_name(symbol: &str) -> String {
    match symbol.strip_prefix("almide_rt_bytes_") {
        Some(op) => format!("bytes.{}", op),
        None => symbol.to_string(),
    }
}

/// Collect each native-only RawPtr call site, pre-formatted as
/// `<location>:<line>:<col> <friendly-name>`.
fn collect_native_only_ptr_sites(program: &IrProgram) -> Vec<String> {
    struct Finder {
        location: String,
        sites: Vec<String>,
    }
    impl IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::RuntimeCall { symbol, .. } = &expr.kind {
                if is_native_only_ptr_symbol(symbol.as_str()) {
                    let loc = match expr.span {
                        Some(sp) => format!("{}:{}:{}", self.location, sp.line, sp.col),
                        None => self.location.clone(),
                    };
                    self.sites.push(format!("[{}] {}", loc, friendly_name(symbol.as_str())));
                }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            walk_stmt(self, stmt);
        }
    }

    let mut f = Finder { location: String::new(), sites: Vec::new() };
    for func in &program.functions {
        f.location = format!("fn {}", func.name);
        f.visit_expr(&func.body);
    }
    for m in &program.modules {
        let mname = m.name.to_string();
        for func in &m.functions {
            f.location = format!("{}::{}", mname, func.name);
            f.visit_expr(&func.body);
        }
    }
    f.sites
}

/// Refuse to emit WASM for a program that calls a native-only RawPtr intrinsic,
/// with a clean span-tagged diagnostic instead of the downstream `emit_wasm`
/// ICE. A controlled `exit(1)`, mirroring `assert_types_concretized` — NOT a
/// `panic!` (which would dump a Rust backtrace and read as a compiler bug).
/// Call only on the WASM path; on native these intrinsics are valid.
pub fn assert_no_native_only_ptr(program: &IrProgram) {
    let sites = collect_native_only_ptr_sites(program);
    if sites.is_empty() {
        return;
    }

    let mut msg = String::new();
    msg.push_str("error: RawPtr / linear-memory bridge is not supported on the wasm target\n");
    msg.push_str(&format!(
        "  {} call(s) to a native-only raw-pointer intrinsic reach WASM emit. These exist for\n",
        sites.len()
    ));
    msg.push_str("  the native cdylib bridge only (`bytes.as_ptr` / `as_mut_ptr` / `from_raw_ptr` /\n");
    msg.push_str("  `copy_to_ptr`); the wasm linear-memory bridge is not implemented yet.\n");
    const MAX_LISTED: usize = 20;
    for s in sites.iter().take(MAX_LISTED) {
        msg.push_str(&format!("    {}\n", s));
    }
    if sites.len() > MAX_LISTED {
        msg.push_str(&format!("    ... and {} more\n", sites.len() - MAX_LISTED));
    }
    msg.push_str("  hint: build this for a native target (drop `--target wasm`), or keep the RawPtr\n");
    msg.push_str("        code behind a target split. The wasm bridge is tracked at\n");
    msg.push_str("        https://github.com/almide/almide/issues/440\n");

    eprint!("{}", msg);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_lang::types::Ty;

    fn expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        }
    }

    fn program_with_body(body: IrExpr) -> IrProgram {
        IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table: VarTable::new(),
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        }
    }

    #[test]
    fn flags_native_only_ptr_call() {
        let p = program_with_body(expr(
            IrExprKind::RuntimeCall { symbol: "almide_rt_bytes_as_mut_ptr".into(), args: vec![] },
            Ty::Unit,
        ));
        let sites = collect_native_only_ptr_sites(&p);
        assert_eq!(sites.len(), 1);
        assert!(sites[0].contains("bytes.as_mut_ptr"), "got: {}", sites[0]);
    }

    #[test]
    fn ignores_a_normal_runtime_call() {
        // An ordinary wasm-supported intrinsic must NOT be flagged — only the
        // closed native-only set is. An unregistered symbol stays an emit-time
        // ICE elsewhere, by design.
        let p = program_with_body(expr(
            IrExprKind::RuntimeCall { symbol: "almide_rt_bytes_len".into(), args: vec![] },
            Ty::Unit,
        ));
        assert!(collect_native_only_ptr_sites(&p).is_empty());
    }

    #[test]
    fn friendly_name_strips_prefix() {
        assert_eq!(friendly_name("almide_rt_bytes_as_mut_ptr"), "bytes.as_mut_ptr");
        assert_eq!(friendly_name("almide_rt_other"), "almide_rt_other");
    }
}
