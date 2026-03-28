//! Real-world comparison: feed actual IR from `almide emit --emit-ir` through
//! the codegen v3 walker and compare with existing codegen output.

use almide::codegen::{self, template};
use almide::codegen::pass::Target;
use almide::codegen::walker::{self, RenderContext};
#[allow(unused_imports)]
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


/// End-to-end: codegen::codegen() — full pipeline in one call
#[test]
fn test_emit_end_to_end_rust() {
    let mut program: IrProgram = serde_json::from_str(IR_JSON)
        .expect("failed to parse IR JSON");

    let output = match codegen::codegen(&mut program, Target::Rust) {
        codegen::CodegenOutput::Source(s) => s,
        codegen::CodegenOutput::Binary(_) => unreachable!(),
    };
    eprintln!("=== codegen::codegen Rust ===\n{}", output);

    assert!(output.contains("pub fn find_price"), "should have pub fn");
    assert!(output.contains("almide_rt_list_find"), "should have stdlib call");
    assert!(output.contains("Vec<Product>"), "should have Vec<Product>");
    assert!(output.contains("format!"), "should have format!");
}

