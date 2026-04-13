//! Declarative stdlib dispatch for WASM emit.
//!
//! Many stdlib functions follow simple emit patterns:
//!   - **RuntimeCall1**: `emit arg0 → call(runtime_fn)` — single arg runtime call.
//!   - **RuntimeCall2**: `emit arg0 → emit arg1 → call(runtime_fn)` — two args.
//!   - **RuntimeCall3**: three args.
//!   - **FloatUnary**: emit arg0, f64_convert if Int, then one WASM instruction or runtime call.
//!   - **Const**: push a literal f64/i64 constant.
//!
//! Callers declare the pattern + runtime function index (if any) and the
//! dispatcher emits the right WASM bytes. This eliminates the per-function
//! match boilerplate in `calls_*.rs`.
//!
//! NOTE: operations with custom control flow (loops, conditional unwraps,
//! allocator calls) stay in the caller — this registry only covers the
//! "emit args, call a runtime function, done" pattern.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;

/// Shape of a stdlib dispatch entry.
#[derive(Debug, Clone, Copy)]
pub(super) enum StdlibOp {
    /// Call runtime fn with N args (1, 2, or 3). Args are pushed left-to-right.
    Call1(u32),
    Call2(u32),
    Call3(u32),
    /// Call runtime fn after converting arg0 from i64→f64 if it's Int.
    /// Used for `math.sin(Int)` which must become `math.sin(Float)`.
    FloatUnaryCall(u32),
}

impl<'a> FuncCompiler<'a> {
    /// Dispatch a declarative stdlib op. Caller must have looked up the entry.
    pub(super) fn emit_stdlib_op(&mut self, op: StdlibOp, args: &[IrExpr]) {
        match op {
            StdlibOp::Call1(fn_idx) => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(fn_idx); });
            }
            StdlibOp::Call2(fn_idx) => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(fn_idx); });
            }
            StdlibOp::Call3(fn_idx) => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(fn_idx); });
            }
            StdlibOp::FloatUnaryCall(fn_idx) => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Float) {
                    // already f64
                } else {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(fn_idx); });
            }
        }
    }
}
