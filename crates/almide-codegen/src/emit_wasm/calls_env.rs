//! env module: environment access — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;

impl FuncCompiler<'_> {
    pub(super) fn emit_env_call(&mut self, func: &str, _args: &[IrExpr]) {
        match func {
            "args" => {
                // env.args() → List[String]. Mirrors native almide_rt_env_args =
                // std::env::args() with argv[0] (the binary name) skipped — only the
                // real program args. Shares the WASI argv→List[String] builder with
                // process.args (calls_process.rs); skip=1 drops argv[0]. (process.args
                // uses skip=0 to keep the full argv.)
                self.emit_wasi_argv_list(1);
            }
            "unix_timestamp" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to seconds.
                // alloc returns (8n+4), need 8-byte aligned for i64 store.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc);
                    i32_const(7); i32_add; i32_const(-8); i32_and;
                    local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "millis" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to milliseconds.
                // alloc returns (8n+4), need 8-byte aligned for i64 store.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc);
                    i32_const(7); i32_add; i32_const(-8); i32_and;
                    local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "os" => {
                let s = self.emitter.intern_string("wasi");
                wasm!(self.func, { i32_const(s as i32); });
            }
            "temp_dir" => {
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(s as i32); });
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `env.{}` — \
                 add an arm in emit_env_call or resolve upstream",
                func
            ),
        }
    }

}
