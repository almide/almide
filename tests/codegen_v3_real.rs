//! Real-world comparison: feed actual IR from `almide emit --emit-ir` through
//! the codegen v3 walker and compare with existing codegen output.

use almide::codegen::{self, template};
use almide::codegen::pass::Target;
use almide::codegen::walker::{self, RenderContext};
use almide::ir::IrProgram;

const IR_JSON: &str = include_str!("fixtures/codegen_v3_ir.json");

#[test]
fn test_real_ir_rust_walker() {
    let program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    let templates = template::rust_templates();
    let ctx = RenderContext::new(&templates, &program.var_table);
    let output = walker::render_program(&ctx, &program);

    eprintln!("=== v3 Rust Walker Output ===\n{}", output);

    // Structural checks — does the walker produce the right shape?
    assert!(output.contains("fn find_price"), "should have find_price function");
    assert!(output.contains("fn total"), "should have total function");
    assert!(output.contains("fn greet"), "should have greet function");

    // Type checks — are types rendered correctly?
    assert!(output.contains("Vec<"), "should use Vec for List");
    assert!(output.contains("i64"), "should use i64 for Int");
    assert!(output.contains("String"), "should use String type");

    // Option handling
    assert!(output.contains("Option<") || output.contains("option"), "should have Option type");

    // String interpolation
    assert!(output.contains("Hello") || output.contains("greet"), "should have greet function");
}

#[test]
fn test_real_ir_ts_walker() {
    let program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    let templates = template::typescript_templates();
    let ctx = RenderContext::new(&templates, &program.var_table);
    let output = walker::render_program(&ctx, &program);

    eprintln!("=== v3 TS Walker Output ===\n{}", output);

    // Structural checks
    assert!(output.contains("function find_price"), "should have find_price");
    assert!(output.contains("function total"), "should have total");
    assert!(output.contains("function greet"), "should have greet");

    // TS-specific type checks
    assert!(output.contains("number"), "should use number for Int");
    assert!(output.contains("string"), "should use string type");

    // Should NOT have Rust patterns
    assert!(!output.contains("Vec<"), "should NOT use Vec in TS");
    assert!(!output.contains("i64"), "should NOT use i64 in TS");
    assert!(!output.contains("Some("), "should NOT use Some( in TS");
}

#[test]
fn test_real_ir_diff_summary() {
    let program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    // Render both targets
    let rust_templates = template::rust_templates();
    let rust_ctx = RenderContext::new(&rust_templates, &program.var_table);
    let rust_output = walker::render_program(&rust_ctx, &program);

    let ts_templates = template::typescript_templates();
    let ts_ctx = RenderContext::new(&ts_templates, &program.var_table);
    let ts_output = walker::render_program(&ts_ctx, &program);

    // Summary comparison
    eprintln!("=== Diff Summary ===");
    eprintln!("Rust output: {} lines, {} chars", rust_output.lines().count(), rust_output.len());
    eprintln!("TS output:   {} lines, {} chars", ts_output.lines().count(), ts_output.len());

    // Both should produce non-empty output
    assert!(rust_output.len() > 50, "Rust output should be substantial");
    assert!(ts_output.len() > 50, "TS output should be substantial");

    // Key divergence checks
    let rust_has_some = rust_output.contains("Some(");
    let ts_has_some = ts_output.contains("Some(");
    eprintln!("Some() present — Rust: {}, TS: {} (expected: true/false)", rust_has_some, ts_has_some);

    let rust_has_format = rust_output.contains("format!");
    let ts_has_backtick = ts_output.contains("`");
    eprintln!("String interp — Rust format!: {}, TS backtick: {}", rust_has_format, ts_has_backtick);

    let rust_has_vec = rust_output.contains("Vec<");
    let ts_has_array = ts_output.contains("[]");
    eprintln!("List type — Rust Vec: {}, TS []: {}", rust_has_vec, ts_has_array);
}

/// End-to-end: codegen::emit() — full pipeline in one call
#[test]
fn test_emit_end_to_end_rust() {
    let mut program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    let output = codegen::emit(&mut program, Target::Rust);
    eprintln!("=== codegen::emit Rust ===\n{}", output);

    assert!(output.contains("pub fn find_price"), "should have pub fn");
    assert!(output.contains("almide_rt_list_find"), "should have stdlib call");
    assert!(output.contains("Vec<Product>"), "should have Vec<Product>");
    assert!(output.contains("format!"), "should have format!");
}

#[test]
fn test_emit_end_to_end_ts() {
    let mut program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    let output = codegen::emit(&mut program, Target::TypeScript);
    eprintln!("=== codegen::emit TS ===\n{}", output);

    assert!(output.contains("function find_price"), "should have function");
    assert!(output.contains("__almd_list.fold"), "should have stdlib call (fold)");
    assert!(output.contains("__almd_list.find"), "should have stdlib call (find) — MatchLowering exposes it");
    assert!(output.contains("__deep_eq"), "should have __deep_eq — MatchLowering exposes equality");
    assert!(output.contains("interface Product"), "should have interface");
    assert!(output.contains("Hello,") || output.contains("greet"), "should have greeting string");
    // Note: runtime code may contain (() =>) — only check user code section
    // MatchLowering converts match→if/else in user code, but runtime is pre-rendered
}
