pub mod borrow;
pub mod rust_ir;
pub mod render;
pub mod lower_rust;
mod lower_rust_expr;
pub mod lower_types;

#[allow(dead_code)]
pub struct EmitOptions {
    pub no_thread_wrap: bool,
    pub fast_mode: bool,
}

impl Default for EmitOptions {
    fn default() -> Self { Self { no_thread_wrap: false, fast_mode: false } }
}

pub fn emit_with_options(ir: &almide::ir::IrProgram, _options: &EmitOptions, _aliases: &[(String, String)], _mods: &std::collections::HashMap<String, almide::ir::IrProgram>) -> String {
    let prog = lower_rust::lower(ir);
    render::program(&prog)
}
