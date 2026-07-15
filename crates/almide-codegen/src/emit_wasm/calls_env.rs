//! env module: environment access — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
// Canonical heap layout offsets ([len@0][cap@4][data@8]) — env.get builds the
// value String with the same 8-byte header every consumer reads with (#645).
use super::rt_string::{string_data_off, string_hdr};

impl FuncCompiler<'_> {
    pub(super) fn emit_env_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "get" => {
                // env.get(name) → Option[String]. Oracle: almide_rt_env_get =
                // std::env::var(name).ok(). Scans the WASI environ (`KEY=VALUE\0`
                // entries — environ_sizes_get/environ_get, the SAME signatures as
                // the args pair emit_wasi_argv_list uses) for `name` followed by
                // '='. some(String) = the value's String pointer, none = 0 (the
                // v0-wasm Option convention — see calls_option.rs is_some).
                // NOTE the byte-level compare: entry bytes vs the key's UTF-8
                // bytes (i32_load(0) = BYTE length, not char count).
                let key = self.scratch.alloc_i32();
                let key_len = self.scratch.alloc_i32();
                let count_ptr = self.scratch.alloc_i32();
                let bufsize_ptr = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let buf_size = self.scratch.alloc_i32();
                let env_ptr = self.scratch.alloc_i32();
                let env_buf = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let entry = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let val_ptr = self.scratch.alloc_i32();
                let val_len = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(key);
                    local_get(key); i32_load(0); local_set(key_len);
                    // Phase 1: environ count + buffer size.
                    i32_const(4); call(self.emitter.rt.alloc); local_set(count_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(bufsize_ptr);
                    local_get(count_ptr);
                    local_get(bufsize_ptr);
                    call(self.emitter.rt.environ_sizes_get);
                    drop; // discard errno
                    local_get(count_ptr); i32_load(0); local_set(count);
                    local_get(bufsize_ptr); i32_load(0); local_set(buf_size);
                    // Phase 2: pointer array + entry buffer (+4 guards so a zero
                    // count/size never yields a degenerate alloc).
                    local_get(count); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(env_ptr);
                    local_get(buf_size); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(env_buf);
                    local_get(env_ptr);
                    local_get(env_buf);
                    call(self.emitter.rt.environ_get);
                    drop; // discard errno
                    // Phase 3: scan entries for `key + '='`; first hit wins.
                    i32_const(0); local_set(result);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(count); i32_ge_u; br_if(1);
                      local_get(env_ptr); local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(entry);
                      // Compare entry[0..key_len] to the key's bytes; j == key_len
                      // afterwards ⟺ the prefix matched (mismatch breaks early).
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(key_len); i32_ge_u; br_if(1);
                        local_get(entry); local_get(j); i32_add; i32_load8_u(0);
                        local_get(key); i32_const(string_data_off()); i32_add; local_get(j); i32_add; i32_load8_u(0);
                        i32_ne; br_if(1);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // Full prefix + '=' right after ⇒ a hit.
                      local_get(j); local_get(key_len); i32_eq;
                      if_empty;
                        local_get(entry); local_get(key_len); i32_add; i32_load8_u(0);
                        i32_const(61); i32_eq; // '='
                        if_empty;
                          // val = entry + key_len + 1, NUL-terminated.
                          local_get(entry); local_get(key_len); i32_add; i32_const(1); i32_add;
                          local_set(val_ptr);
                          i32_const(0); local_set(val_len);
                          block_empty; loop_empty;
                            local_get(val_ptr); local_get(val_len); i32_add; i32_load8_u(0);
                            i32_eqz; br_if(1);
                            local_get(val_len); i32_const(1); i32_add; local_set(val_len);
                            br(0);
                          end; end;
                          // Build the Almide String [len][cap][bytes].
                          local_get(val_len); i32_const(string_hdr()); i32_add;
                          call(self.emitter.rt.alloc); local_set(str_ptr);
                          local_get(str_ptr); local_get(val_len); i32_store(0);
                          local_get(str_ptr); i32_const(4); i32_add; local_get(val_len); i32_store(0);
                          i32_const(0); local_set(j);
                          block_empty; loop_empty;
                            local_get(j); local_get(val_len); i32_ge_u; br_if(1);
                            local_get(str_ptr); i32_const(string_data_off()); i32_add; local_get(j); i32_add;
                            local_get(val_ptr); local_get(j); i32_add; i32_load8_u(0);
                            i32_store8(0);
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                          end; end;
                          // BOX the some: the v0-wasm Option is a CELL holding the
                          // payload (`unwrap_or`/match load *opt) — none = 0. Mirrors
                          // list.find's `alloc(size) + store` some-construction.
                          i32_const(4); call(self.emitter.rt.alloc); local_set(val_ptr);
                          local_get(val_ptr); local_get(str_ptr); i32_store(0);
                          local_get(val_ptr); local_set(result);
                        end;
                      end;
                      // A hit ends the scan.
                      local_get(result); i32_const(0); i32_ne; br_if(1);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(key);
                self.scratch.free_i32(key_len);
                self.scratch.free_i32(count_ptr);
                self.scratch.free_i32(bufsize_ptr);
                self.scratch.free_i32(count);
                self.scratch.free_i32(buf_size);
                self.scratch.free_i32(env_ptr);
                self.scratch.free_i32(env_buf);
                self.scratch.free_i32(i);
                self.scratch.free_i32(entry);
                self.scratch.free_i32(j);
                self.scratch.free_i32(val_ptr);
                self.scratch.free_i32(val_len);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(result);
            }
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
