//! env module: environment access — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

// ── Immediate constants used by env WASM emit ─────────────────────────────
/// Byte size of a single i32 value; used to alloc the len field of an empty list.
const I32_BYTES: i32 = 4;
/// Scratch buffer size for a WASI clock_time_get i64 result (over-alloc for alignment).
const CLOCK_BUF_ALLOC_BYTES: i32 = 16;
/// Low bits that must be zero for 8-byte alignment (round-up mask: add this, then AND with ALIGN8_NEG_MASK).
const ALIGN8_MASK: i32 = 7;
/// AND mask that clears the low 3 bits, aligning a pointer down to an 8-byte boundary.
const ALIGN8_NEG_MASK: i32 = -8;
/// Nanoseconds in one second; divides a WASI nanosecond timestamp to produce Unix seconds.
const NANOS_PER_SEC: i64 = 1_000_000_000;
/// Nanoseconds in one millisecond; divides a WASI nanosecond timestamp to produce milliseconds.
const NANOS_PER_MILLI: i64 = 1_000_000;

impl FuncCompiler<'_> {
    pub(super) fn emit_env_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "args" => {
                // env.args() → List[String]: return empty list (WASI args not implemented yet)
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(I32_BYTES); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(s);
            }
            "unix_timestamp" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to seconds.
                // alloc returns (8n+4), need 8-byte aligned for i64 store.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(CLOCK_BUF_ALLOC_BYTES); call(self.emitter.rt.alloc);
                    i32_const(ALIGN8_MASK); i32_add; i32_const(ALIGN8_NEG_MASK); i32_and;
                    local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(NANOS_PER_SEC); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "millis" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to milliseconds.
                // alloc returns (8n+4), need 8-byte aligned for i64 store.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(CLOCK_BUF_ALLOC_BYTES); call(self.emitter.rt.alloc);
                    i32_const(ALIGN8_MASK); i32_add; i32_const(ALIGN8_NEG_MASK); i32_and;
                    local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(NANOS_PER_MILLI); i64_div_u;
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
