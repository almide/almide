//! Unit 6 stage 1: typed IR → structural wasm emission.
//!
//! Constitution (ARCHITECTURE.md §3/§6.6): binary emission via wasm-encoder
//! only — no WAT text, no string templates; block layout derives from
//! `almide-layout` (string literals are laid out as REAL blocks,
//! `[rc][len][cap][payload]`, from the first byte this backend ever emits);
//! every module is validated before acceptance (the wall, in the gate).
//!
//! Stage-1 coverage is DELIBERATELY tiny — `main` bodies made of
//! `println(<string literal>)` statements — because the point of the stage
//! is the skeleton: the emission path, the wasmtime harness, the
//! interp-parity gate, and the unsupported-histogram ratchet that will
//! drive every later slice (the same burn-up mechanic that took the interp
//! from 138 skips to 121).

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, IrStmtKind};
use wasm_encoder::{
    CodeSection, DataSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
};

#[derive(Debug)]
pub enum EmitError {
    /// This IR shape is outside the current slice. The reason string feeds
    /// the burn-up histogram — precise, greppable, shrink-only.
    Unsupported(String),
}

fn unsup<T>(what: &str) -> Result<T, EmitError> {
    Err(EmitError::Unsupported(what.to_string()))
}

/// A string literal placed in linear memory as a REAL layout block.
struct Pool {
    data: Vec<u8>,
}

impl Pool {
    fn new() -> Self {
        // Reserve the null address region: the layout's NULL_ADDR (0) must
        // never name a live block, so the pool starts one header past it.
        Pool { data: vec![0; almide_layout::PAYLOAD as usize] }
    }

    /// Intern `s` as a block; returns the PAYLOAD address (what print wants).
    fn intern(&mut self, s: &str) -> (u32, u32) {
        let base = self.data.len() as u32;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;
        let mut header = vec![0u8; almide_layout::PAYLOAD as usize];
        header[almide_layout::RC.offset as usize..][..4].copy_from_slice(&1u32.to_le_bytes());
        header[almide_layout::LEN.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        header[almide_layout::CAP.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(&header);
        self.data.extend_from_slice(bytes);
        (base + almide_layout::PAYLOAD, len)
    }
}

/// Emit a core wasm module for `ir`, or say precisely why not yet.
pub fn emit_program(ir: &IrProgram) -> Result<Vec<u8>, EmitError> {
    let Some(main) = ir.functions.iter().find(|f| f.name.as_str() == "main") else {
        return unsup("no main function");
    };
    // Top-level lets evaluate EAGERLY before main on both legs (the
    // top_let_div_eager fixture aborts in that phase) — refusing them is
    // honest until the slice lands. The burn-up gate caught the over-claim.
    if !ir.top_lets.is_empty() {
        return unsup("top-lets");
    }

    let mut pool = Pool::new();
    let mut body_ops: Vec<(u32, u32)> = Vec::new(); // (payload addr, len) per println
    lower_main_expr(&main.body, &mut pool, &mut body_ops)?;

    // ── assemble the module structurally ────────────────────────────────
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32, ValType::I32], []); // type 0: println(ptr, len)
    types.ty().function([], []); // type 1: main()

    let mut imports = ImportSection::new();
    imports.import("almide", "println", EntityType::Function(0));

    let mut functions = FunctionSection::new();
    functions.function(1); // main is func index 1 (0 is the import)

    let mut memories = MemorySection::new();
    let pages = (pool.data.len() as u64).div_ceil(65536).max(1);
    memories.memory(MemoryType {
        minimum: pages,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("main", ExportKind::Func, 1);

    let mut code = CodeSection::new();
    let mut f = Function::new([]);
    for (addr, len) in &body_ops {
        f.instructions()
            .i32_const(*addr as i32)
            .i32_const(*len as i32)
            .call(0);
    }
    f.instructions().end();
    code.function(&f);

    let mut data = DataSection::new();
    data.active(0, &wasm_encoder::ConstExpr::i32_const(0), pool.data.iter().copied());

    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&memories)
        .section(&exports)
        .section(&code)
        .section(&data);
    Ok(module.finish())
}

/// Stage-1 lowering of the main body: blocks of `println(<lit str>)`.
fn lower_main_expr(e: &IrExpr, pool: &mut Pool, out: &mut Vec<(u32, u32)>) -> Result<(), EmitError> {
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Expr { expr } => lower_main_expr(expr, pool, out)?,
                    IrStmtKind::Comment { .. } => {}
                    other => return unsup(&format!("stmt:{}", stmt_kind_name(other))),
                }
            }
            if let Some(tail) = expr {
                lower_main_expr(tail, pool, out)?;
            }
            Ok(())
        }
        IrExprKind::Call { target, args, .. } => match target {
            CallTarget::Named { name } if name.as_str() == "println" && args.len() == 1 => {
                match &args[0].kind {
                    IrExprKind::LitStr { value } => {
                        out.push(pool.intern(value));
                        Ok(())
                    }
                    other => unsup(&format!("println-arg:{}", expr_kind_name(other))),
                }
            }
            CallTarget::Named { name } => unsup(&format!("call:{}", name.as_str())),
            CallTarget::Module { module, func, .. } => {
                unsup(&format!("call:{}.{}", module.as_str(), func.as_str()))
            }
            _ => unsup("call:computed-or-method"),
        },
        IrExprKind::Unit => Ok(()),
        other => unsup(&format!("expr:{}", expr_kind_name(other))),
    }
}

fn expr_kind_name(k: &IrExprKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

fn stmt_kind_name(k: &IrStmtKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}
