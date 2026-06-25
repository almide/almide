//! process module: exit, stdin_lines — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
// Canonical heap layout offsets ([len@0][cap@4][data@8]) — process.args /
// stdin_lines build List[String] + the inner Strings and MUST frame them with
// the same 8-byte header the consumers (list.get / string.len / emit_member)
// read with, or every element/byte is read at the wrong offset (#645).
use super::rt_string::{string_hdr, string_data_off, string_cap_off, list_hdr, list_data_off, list_cap_off};

/// Stride, in bytes, of one `List[String]` element slot: each element is an i32
/// pointer to an Almide String, so the data region is `count` 4-byte words.
const LIST_ELEM_STRIDE: i32 = 4;

impl FuncCompiler<'_> {
    /// process module: exit, stdin_lines
    pub(super) fn emit_process_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "exit" => {
                // process.exit(code: Int) -> Unit
                // Emit code arg (i64), wrap to i32, call proc_exit
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    call(self.emitter.rt.proc_exit);
                });
            }
            "stdin_lines" => {
                // process.stdin_lines() -> List[String]
                // Strategy: read all stdin, then split by '\n'.
                // 1. Read all stdin into a raw buffer (same logic as io.read_all)
                // 2. Split by '\n', building a list of Almide strings
                let buf = self.scratch.alloc_i32();
                let capacity = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                let scan_i = self.scratch.alloc_i32();
                let line_start = self.scratch.alloc_i32();
                let line_len = self.scratch.alloc_i32();
                let line_ptr = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let list_cap = self.scratch.alloc_i32();
                let list_count = self.scratch.alloc_i32();
                let new_list = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                // --- Phase 1: read all stdin ---
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(4096); local_set(capacity);
                    i32_const(0); local_set(len);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow if needed
                wasm!(self.func, {
                    local_get(capacity); local_get(len); i32_sub;
                    i32_const(4096); i32_lt_u;
                    if_empty;
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                        local_get(new_buf); local_get(copy_i); i32_add;
                        local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      local_get(new_buf); local_set(buf);
                    end;
                });

                // Read chunk
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(buf); local_get(len); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(capacity); local_get(len); i32_sub; i32_store(4);
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1);
                    local_get(len); local_get(nread_val); i32_add; local_set(len);
                    br(0);
                    end; end;
                });

                // --- Phase 2: split buf[0..len] by '\n' into List[String] ---
                // List layout: [count:i32][elem0:i32][elem1:i32]...
                // Each elem is a ptr to Almide String [len:i32][data:u8...]
                // We'll build with a growable array of i32 pointers.
                wasm!(self.func, {
                    // Initial list capacity: 64 elements (i32 ptrs)
                    i32_const(64); local_set(list_cap);
                    local_get(list_cap); i32_const(4); i32_mul;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    i32_const(0); local_set(list_count);
                    i32_const(0); local_set(scan_i);
                    i32_const(0); local_set(line_start);
                });

                // Scan loop: iterate through buf looking for '\n'
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(scan_i); local_get(len); i32_ge_u;
                      br_if(1);
                });

                // Check if buf[scan_i] == '\n'
                wasm!(self.func, {
                      local_get(buf); local_get(scan_i); i32_add; i32_load8_u(0);
                      i32_const(10); i32_eq;
                      if_empty;
                });

                // Found '\n': build string from line_start..scan_i
                wasm!(self.func, {
                        local_get(scan_i); local_get(line_start); i32_sub; local_set(line_len);
                        // Allocate Almide string [len][cap][bytes...]
                        local_get(line_len); i32_const(string_hdr()); i32_add;
                        call(self.emitter.rt.alloc); local_set(line_ptr);
                        local_get(line_ptr); local_get(line_len); i32_store(0);
                        local_get(line_ptr); i32_const(string_cap_off()); i32_add; local_get(line_len); i32_store(0); // cap = len
                        // Copy line data
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                          local_get(line_ptr); i32_const(string_data_off()); i32_add; local_get(copy_i); i32_add;
                          local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                          i32_load8_u(0);
                          i32_store8(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                });

                // Grow list if needed
                wasm!(self.func, {
                        local_get(list_count); local_get(list_cap); i32_ge_u;
                        if_empty;
                          local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                          local_get(list_cap); i32_const(4); i32_mul;
                          call(self.emitter.rt.alloc); local_set(new_list);
                          // Copy old list ptrs
                          i32_const(0); local_set(copy_i);
                          block_empty; loop_empty;
                            local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                            local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            i32_load(0);
                            i32_store(0);
                            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                            br(0);
                          end; end;
                          local_get(new_list); local_set(list_ptr);
                        end;
                });

                // Append line_ptr to list
                wasm!(self.func, {
                        local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                        local_get(line_ptr); i32_store(0);
                        local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                        // line_start = scan_i + 1
                        local_get(scan_i); i32_const(1); i32_add; local_set(line_start);
                      end; // end if '\n'
                });

                // Advance scan_i
                wasm!(self.func, {
                      local_get(scan_i); i32_const(1); i32_add; local_set(scan_i);
                      br(0);
                    end; end; // end loop, end block
                });

                // Handle last line (if no trailing '\n')
                wasm!(self.func, {
                    local_get(line_start); local_get(len); i32_lt_u;
                    if_empty;
                      local_get(len); local_get(line_start); i32_sub; local_set(line_len);
                      local_get(line_len); i32_const(string_hdr()); i32_add;
                      call(self.emitter.rt.alloc); local_set(line_ptr);
                      local_get(line_ptr); local_get(line_len); i32_store(0);
                      local_get(line_ptr); i32_const(string_cap_off()); i32_add; local_get(line_len); i32_store(0); // cap = len
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                        local_get(line_ptr); i32_const(string_data_off()); i32_add; local_get(copy_i); i32_add;
                        local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                        i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                });

                // Grow list if needed for last line
                wasm!(self.func, {
                      local_get(list_count); local_get(list_cap); i32_ge_u;
                      if_empty;
                        local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                        local_get(list_cap); i32_const(4); i32_mul;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                          local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          i32_load(0);
                          i32_store(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                        local_get(new_list); local_set(list_ptr);
                      end;
                      // Append last line
                      local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                      local_get(line_ptr); i32_store(0);
                      local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                    end; // end if line_start < len
                });

                // Build final Almide List: [len:i32][cap:i32][elem0:i32][elem1:i32]...
                // elem_size = 4 (i32 pointer)
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(list_hdr()); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(list_count); i32_store(0);
                    local_get(result); i32_const(list_cap_off()); i32_add; local_get(list_count); i32_store(0); // cap = len
                    // Copy list_ptr[0..list_count] to result+data_off
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                      local_get(result); i32_const(list_data_off()); i32_add;
                      local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      i32_load(0);
                      i32_store(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(new_list);
                self.scratch.free_i32(list_count);
                self.scratch.free_i32(list_cap);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(line_ptr);
                self.scratch.free_i32(line_len);
                self.scratch.free_i32(line_start);
                self.scratch.free_i32(scan_i);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            "args" => {
                // process.args() -> List[String]
                // Mirror native almide_rt_process_args = std::env::args().collect()
                // (argv[0] = program path, then any program args). skip=0 keeps the
                // full argv, including argv[0]. env.args (calls_env.rs) shares the
                // same builder with skip=1 to drop argv[0].
                self.emit_wasi_argv_list(0);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `process.{}` — \
                 add an arm in emit_process_call or resolve upstream",
                func
            ),
        }
}

    /// Build a `List[String]` from the WASI program arguments, skipping the
    /// first `skip` leading argv entries.
    ///
    /// This is the shared body behind BOTH `process.args` (skip=0, the full argv
    /// including argv[0] = program path — mirrors native `almide_rt_process_args`
    /// = `std::env::args().collect()`) and `env.args` (skip=1, dropping argv[0] —
    /// mirrors native `almide_rt_env_args` = `std::env::args()` with the binary
    /// name removed). Leaves the result list pointer on the wasm stack.
    ///
    /// Mechanism: WASI `args_sizes_get` / `args_get` give us `argc` and a flat
    /// NUL-terminated argv buffer. We allocate the canonical 8-byte-header
    /// `List[String]` of length `argc - skip`, then for each result slot `j` we
    /// take `argv[j + skip]`, `strlen`-scan it, allocate a canonical Almide
    /// String, and copy the bytes in. The `skip` leading C-strings are simply
    /// never visited. All scratch locals are freed before returning (#645: the
    /// header layout MUST be [len@0][cap@4][data@8] so list.get / string.len
    /// read at the right offsets).
    pub(super) fn emit_wasi_argv_list(&mut self, skip: i32) {
                let argc_ptr = self.scratch.alloc_i32();
                let bufsize_ptr = self.scratch.alloc_i32();
                let argc = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let buf_size = self.scratch.alloc_i32();
                let argv_ptr = self.scratch.alloc_i32();
                let argv_buf = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cstr_ptr = self.scratch.alloc_i32();
                let str_len = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();

                // --- Phase 1: discover argc + total argv buffer size ---
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(argc_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(bufsize_ptr);
                    local_get(argc_ptr);
                    local_get(bufsize_ptr);
                    call(self.emitter.rt.args_sizes_get);
                    drop; // discard errno
                    local_get(argc_ptr); i32_load(0); local_set(argc);
                    local_get(bufsize_ptr); i32_load(0); local_set(buf_size);
                    // count = result-list length = max(argc - skip, 0). skip leading
                    // argv entries are dropped (env.args skips argv[0]; process.args
                    // skips nothing). Clamp so a degenerate argc < skip can never
                    // underflow the unsigned loop bound below. `select` is
                    // `val1 val2 cond -> (cond ? val1 : val2)`, so the condition MUST
                    // be pushed LAST: val1=(argc-skip), val2=0, cond=(argc>=skip).
                    local_get(argc); i32_const(skip); i32_sub;          // val1 = argc - skip
                    i32_const(0);                                        // val2 = 0
                    local_get(argc); i32_const(skip); i32_ge_u;         // cond = argc >= skip
                    select;                                             // cond ? (argc-skip) : 0
                    local_set(count);
                });

                // --- Phase 2: alloc the pointer array + the string buffer, fill them ---
                // argv_ptr: argc i32 pointers. argv_buf: buf_size NUL-terminated bytes.
                // Guard zero-size allocs with a minimum of 4 bytes so alloc never
                // returns a degenerate pointer (argc is always >= 1 in practice, but
                // stay defensive).
                wasm!(self.func, {
                    // argv_ptr: argc i32 pointers (+4 guard so a zero argc never
                    // yields a degenerate alloc).
                    local_get(argc); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(argv_ptr);
                    local_get(buf_size); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(argv_buf);
                    local_get(argv_ptr);
                    local_get(argv_buf);
                    call(self.emitter.rt.args_get);
                    drop; // discard errno
                });

                // --- Phase 3: build List[String] = [len][cap][strptr0][strptr1]... ---
                // Result length is `count` (= argc - skip). The per-slot argv index
                // is `i + skip`, so the `skip` leading C-strings are never visited.
                wasm!(self.func, {
                    local_get(count); i32_const(LIST_ELEM_STRIDE); i32_mul; i32_const(list_hdr()); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(count); i32_store(0);
                    local_get(result); i32_const(list_cap_off()); i32_add; local_get(count); i32_store(0); // cap = len
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(count); i32_ge_u; br_if(1);
                      // cstr_ptr = argv_ptr[i + skip]
                      local_get(argv_ptr); local_get(i); i32_const(skip); i32_add; i32_const(LIST_ELEM_STRIDE); i32_mul; i32_add;
                      i32_load(0); local_set(cstr_ptr);
                      // str_len = strlen(cstr_ptr): scan to NUL
                      i32_const(0); local_set(str_len);
                      block_empty; loop_empty;
                        local_get(cstr_ptr); local_get(str_len); i32_add; i32_load8_u(0);
                        i32_eqz; br_if(1);
                        local_get(str_len); i32_const(1); i32_add; local_set(str_len);
                        br(0);
                      end; end;
                      // alloc Almide string [len][cap][bytes...]
                      local_get(str_len); i32_const(string_hdr()); i32_add;
                      call(self.emitter.rt.alloc); local_set(str_ptr);
                      local_get(str_ptr); local_get(str_len); i32_store(0);
                      local_get(str_ptr); i32_const(string_cap_off()); i32_add; local_get(str_len); i32_store(0); // cap = len
                      // copy str_len bytes from cstr_ptr into str_ptr+data_off
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(str_len); i32_ge_u; br_if(1);
                        local_get(str_ptr); i32_const(string_data_off()); i32_add; local_get(copy_i); i32_add;
                        local_get(cstr_ptr); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      // result[data_off + i*4] = str_ptr
                      local_get(result); i32_const(list_data_off()); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(str_ptr); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(str_len);
                self.scratch.free_i32(cstr_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(argv_buf);
                self.scratch.free_i32(argv_ptr);
                self.scratch.free_i32(buf_size);
                self.scratch.free_i32(count);
                self.scratch.free_i32(argc);
                self.scratch.free_i32(bufsize_ptr);
                self.scratch.free_i32(argc_ptr);
    }
}
