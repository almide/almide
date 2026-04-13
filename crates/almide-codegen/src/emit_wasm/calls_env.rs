//! env module: environment access — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    pub(super) fn emit_env_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "args" => {
                // env.args() → List[String]: return empty list (WASI args not implemented yet)
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(s);
            }
            "unix_timestamp" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to seconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
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
                // Returns nanoseconds as i64, convert to milliseconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
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
            _ => {
                self.emit_stub_call_named("env", func, args);
            }
        }
    }

}
