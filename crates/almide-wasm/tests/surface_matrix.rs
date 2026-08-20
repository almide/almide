//! V-7 (RESEARCH-verification.md; the requirements-matrix flight gap):
//! the wasm leg's EXERCISED SURFACE as a committed, reviewed manifest.
//!
//! For every corpus fixture the backend can emit, walk its IR and record
//! each construct actually lowered (expression kinds, operators, call
//! targets, pattern shapes, statement kinds). The union is compared to
//! `tests/golden/wasm-exercised-surface.txt`:
//!
//!   - a construct DISAPPEARING from the measured set is a FAILURE — the
//!     silent-regression class the supported-count floor cannot see (a
//!     lowering reroute that keeps fixtures green while a feature stops
//!     being exercised);
//!   - a NEW construct requires deliberately regenerating the golden
//!     (`ALMIDE_UPDATE_SURFACE=1`), making surface growth a reviewed diff.
//!
//! The refused side of the matrix needs no second registry: the burn-up
//! gate's reason histogram already names every wall precisely.

use std::collections::BTreeSet;
use std::path::PathBuf;

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrPattern, IrProgram};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

fn head(dbg: String) -> String {
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

fn record_pattern(p: &IrPattern, out: &mut BTreeSet<String>) {
    out.insert(format!("pattern:{}", head(format!("{p:?}"))));
    match p {
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            record_pattern(inner, out)
        }
        IrPattern::Constructor { args, .. } | IrPattern::Tuple { elements: args } => {
            for a in args {
                record_pattern(a, out);
            }
        }
        _ => {}
    }
}

fn walk(e: &IrExpr, out: &mut BTreeSet<String>) {
    out.insert(format!("expr:{}", head(format!("{:?}", e.kind))));
    match &e.kind {
        IrExprKind::BinOp { op, .. } => {
            out.insert(format!("binop:{op:?}"));
        }
        IrExprKind::UnOp { op, .. } => {
            out.insert(format!("unop:{op:?}"));
        }
        IrExprKind::Call { target, .. } => {
            match target {
                // Fixture-local fn/ctor names are noise, not surface —
                // normalize; builtins and stdlib calls stay precise.
                CallTarget::Named { name }
                    if matches!(name.as_str(), "println" | "eprintln") =>
                {
                    out.insert(format!("call:{}", name.as_str()))
                }
                CallTarget::Named { .. } => out.insert("call:user-fn".into()),
                CallTarget::Module { module, func, .. } => {
                    out.insert(format!("call:{}.{}", module.as_str(), func.as_str()))
                }
                _ => out.insert("call:computed-or-method".into()),
            };
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                record_pattern(&arm.pattern, out);
            }
        }
        IrExprKind::Block { stmts, .. } => {
            for s in stmts {
                out.insert(format!("stmt:{}", head(format!("{:?}", s.kind))));
            }
        }
        IrExprKind::While { body, .. } | IrExprKind::ForIn { body, .. } => {
            for s in body {
                out.insert(format!("stmt:{}", head(format!("{:?}", s.kind))));
            }
        }
        _ => {}
    }
    // Child recursion via the IR's own traversal (covers statement-held
    // expressions too), identity-mapped.
    e.clone().map_children(&mut |c| {
        walk(&c, out);
        c
    });
}

fn measure(ir: &IrProgram, out: &mut BTreeSet<String>) {
    for f in &ir.functions {
        if !f.is_test {
            walk(&f.body, out);
        }
    }
    for tl in &ir.top_lets {
        walk(&tl.value, out);
    }
}

#[test]
fn exercised_surface_matches_golden() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");

    let mut surface: BTreeSet<String> = BTreeSet::new();
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let Ok(ir) = almide_spine::s5::lower_to_ir(rel, &text) else { continue };
        if almide_wasm::emit_program(&ir).is_ok() {
            measure(&ir, &mut surface);
        }
    }
    let measured: String =
        surface.iter().map(|s| format!("{s}\n")).collect::<String>();

    let golden_path = root.join("crates/almide-wasm/tests/golden/wasm-exercised-surface.txt");
    if std::env::var("ALMIDE_UPDATE_SURFACE").is_ok() {
        std::fs::create_dir_all(golden_path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&golden_path, &measured).expect("write golden");
        println!("surface golden regenerated: {} constructs", surface.len());
        return;
    }
    let golden = std::fs::read_to_string(&golden_path)
        .expect("golden missing — run with ALMIDE_UPDATE_SURFACE=1 once");
    let golden_set: BTreeSet<&str> = golden.lines().filter(|l| !l.is_empty()).collect();
    let measured_set: BTreeSet<&str> = measured.lines().filter(|l| !l.is_empty()).collect();

    let lost: Vec<&&str> = golden_set.difference(&measured_set).collect();
    let gained: Vec<&&str> = measured_set.difference(&golden_set).collect();
    assert!(
        lost.is_empty(),
        "constructs DISAPPEARED from the wasm leg's exercised surface (silent regression): {lost:?}"
    );
    assert!(
        gained.is_empty(),
        "new constructs entered the exercised surface — review and regenerate the golden \
         with ALMIDE_UPDATE_SURFACE=1: {gained:?}"
    );
}
