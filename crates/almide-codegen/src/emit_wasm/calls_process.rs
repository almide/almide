//! process module: exit, stdin_lines — WASM codegen dispatch.

use crate::emit_wasm::engine::{Imm32, Local};
use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
// Canonical heap layout offsets ([len@0][cap@4][data@8]) — process.args /
// stdin_lines build List[String] + the inner Strings and MUST frame them with
// the same 8-byte header the consumers (list.get / string.len / emit_member)
// read with, or every element/byte is read at the wrong offset (#645).
use super::rt_string::{string_hdr, string_data_off, string_cap_off, list_hdr, list_data_off, list_cap_off};
use wasm_encoder::Instruction;

/// Named WASM immediate constants for process-call codegen.
mod imm {
    // ── byte widths ────────────────────────────────────────────────────────
    /// Byte size of an i32 / pointer (pointer stride in List[String] element
    /// arrays and size of a single i32 out-parameter allocation).
    pub const I32_BYTES: i32 = 4;
    /// Byte size of a WASI iovec_t struct ([buf_ptr: i32, buf_len: i32] = 2×4).
    pub const IOV_BYTES: i32 = 8;

    // ── initial capacities ─────────────────────────────────────────────────
    /// Initial byte capacity of the stdin read buffer before any growth.
    pub const STDIN_BUF_INIT_CAP: i32 = 4096;
    /// Growth threshold: when fewer than this many bytes remain free in the
    /// read buffer, double the capacity before the next fd_read call.
    pub const STDIN_BUF_GROW_THRESHOLD: i32 = 4096;
    /// Initial element capacity of the line-pointer array while scanning
    /// stdin for newlines.
    pub const INIT_LINE_LIST_CAP: i32 = 64;
    /// Multiplicative growth factor applied to buffer / list capacities.
    pub const CAPACITY_DOUBLE: i32 = 2;

    // ── character codes ────────────────────────────────────────────────────
    /// ASCII code for the newline character '\n' (0x0A).
    pub const ASCII_NEWLINE: i32 = 10;
}
use imm::*;

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
                    i32_const(Imm32(STDIN_BUF_INIT_CAP)); call(self.emitter.rt.alloc); local_set(Local(buf));
                    i32_const(Imm32(STDIN_BUF_INIT_CAP)); local_set(Local(capacity));
                    i32_const(Imm32(0)); local_set(Local(len));
                    i32_const(Imm32(IOV_BYTES)); call(self.emitter.rt.alloc); local_set(Local(iov_ptr));
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(nread_ptr));
                });

                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow if needed
                wasm!(self.func, {
                    local_get(Local(capacity)); local_get(Local(len)); i32_sub;
                    i32_const(Imm32(STDIN_BUF_GROW_THRESHOLD)); i32_lt_u;
                    if_empty;
                      local_get(Local(capacity)); i32_const(Imm32(CAPACITY_DOUBLE)); i32_mul; local_set(Local(capacity));
                      local_get(Local(capacity)); call(self.emitter.rt.alloc); local_set(Local(new_buf));
                      i32_const(Imm32(0)); local_set(Local(copy_i));
                      block_empty; loop_empty;
                        local_get(Local(copy_i)); local_get(Local(len)); i32_ge_u; br_if(1);
                        local_get(Local(new_buf)); local_get(Local(copy_i)); i32_add;
                        local_get(Local(buf)); local_get(Local(copy_i)); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                        br(0);
                      end; end;
                      local_get(Local(new_buf)); local_set(Local(buf));
                    end;
                });

                // Read chunk
                wasm!(self.func, {
                    local_get(Local(iov_ptr)); local_get(Local(buf)); local_get(Local(len)); i32_add; i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(capacity)); local_get(Local(len)); i32_sub; i32_store(4);
                    i32_const(Imm32(0));
                    local_get(Local(iov_ptr));
                    i32_const(Imm32(1));
                    local_get(Local(nread_ptr));
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                wasm!(self.func, {
                    local_get(Local(nread_ptr)); i32_load(0); local_set(Local(nread_val));
                    local_get(Local(nread_val)); i32_eqz;
                    br_if(1);
                    local_get(Local(len)); local_get(Local(nread_val)); i32_add; local_set(Local(len));
                    br(0);
                    end; end;
                });

                // --- Phase 2: split buf[0..len] by '\n' into List[String] ---
                // List layout: [count:i32][elem0:i32][elem1:i32]...
                // Each elem is a ptr to Almide String [len:i32][data:u8...]
                // We'll build with a growable array of i32 pointers.
                wasm!(self.func, {
                    // Initial list capacity: INIT_LINE_LIST_CAP elements (i32 ptrs)
                    i32_const(Imm32(INIT_LINE_LIST_CAP)); local_set(Local(list_cap));
                    local_get(Local(list_cap)); i32_const(Imm32(I32_BYTES)); i32_mul;
                    call(self.emitter.rt.alloc); local_set(Local(list_ptr));
                    i32_const(Imm32(0)); local_set(Local(list_count));
                    i32_const(Imm32(0)); local_set(Local(scan_i));
                    i32_const(Imm32(0)); local_set(Local(line_start));
                });

                // Scan loop: iterate through buf looking for '\n'
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(Local(scan_i)); local_get(Local(len)); i32_ge_u;
                      br_if(1);
                });

                // Check if buf[scan_i] == '\n'
                wasm!(self.func, {
                      local_get(Local(buf)); local_get(Local(scan_i)); i32_add; i32_load8_u(0);
                      i32_const(Imm32(ASCII_NEWLINE)); i32_eq;
                      if_empty;
                });

                // Found '\n': build string from line_start..scan_i
                wasm!(self.func, {
                        local_get(Local(scan_i)); local_get(Local(line_start)); i32_sub; local_set(Local(line_len));
                        // Allocate Almide string [len][cap][bytes...]
                        local_get(Local(line_len)); i32_const(Imm32(string_hdr())); i32_add;
                        call(self.emitter.rt.alloc); local_set(Local(line_ptr));
                        local_get(Local(line_ptr)); local_get(Local(line_len)); i32_store(0);
                        local_get(Local(line_ptr)); i32_const(Imm32(string_cap_off())); i32_add; local_get(Local(line_len)); i32_store(0); // cap = len
                        // Copy line data
                        i32_const(Imm32(0)); local_set(Local(copy_i));
                        block_empty; loop_empty;
                          local_get(Local(copy_i)); local_get(Local(line_len)); i32_ge_u; br_if(1);
                          local_get(Local(line_ptr)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(copy_i)); i32_add;
                          local_get(Local(buf)); local_get(Local(line_start)); i32_add; local_get(Local(copy_i)); i32_add;
                          i32_load8_u(0);
                          i32_store8(0);
                          local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                          br(0);
                        end; end;
                });

                // Grow list if needed
                wasm!(self.func, {
                        local_get(Local(list_count)); local_get(Local(list_cap)); i32_ge_u;
                        if_empty;
                          local_get(Local(list_cap)); i32_const(Imm32(CAPACITY_DOUBLE)); i32_mul; local_set(Local(list_cap));
                          local_get(Local(list_cap)); i32_const(Imm32(I32_BYTES)); i32_mul;
                          call(self.emitter.rt.alloc); local_set(Local(new_list));
                          // Copy old list ptrs
                          i32_const(Imm32(0)); local_set(Local(copy_i));
                          block_empty; loop_empty;
                            local_get(Local(copy_i)); local_get(Local(list_count)); i32_ge_u; br_if(1);
                            local_get(Local(new_list)); local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                            local_get(Local(list_ptr)); local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                            i32_load(0);
                            i32_store(0);
                            local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                            br(0);
                          end; end;
                          local_get(Local(new_list)); local_set(Local(list_ptr));
                        end;
                });

                // Append line_ptr to list
                wasm!(self.func, {
                        local_get(Local(list_ptr)); local_get(Local(list_count)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                        local_get(Local(line_ptr)); i32_store(0);
                        local_get(Local(list_count)); i32_const(Imm32(1)); i32_add; local_set(Local(list_count));
                        // line_start = scan_i + 1
                        local_get(Local(scan_i)); i32_const(Imm32(1)); i32_add; local_set(Local(line_start));
                      end; // end if '\n'
                });

                // Advance scan_i
                wasm!(self.func, {
                      local_get(Local(scan_i)); i32_const(Imm32(1)); i32_add; local_set(Local(scan_i));
                      br(0);
                    end; end; // end loop, end block
                });

                // Handle last line (if no trailing '\n')
                wasm!(self.func, {
                    local_get(Local(line_start)); local_get(Local(len)); i32_lt_u;
                    if_empty;
                      local_get(Local(len)); local_get(Local(line_start)); i32_sub; local_set(Local(line_len));
                      local_get(Local(line_len)); i32_const(Imm32(string_hdr())); i32_add;
                      call(self.emitter.rt.alloc); local_set(Local(line_ptr));
                      local_get(Local(line_ptr)); local_get(Local(line_len)); i32_store(0);
                      local_get(Local(line_ptr)); i32_const(Imm32(string_cap_off())); i32_add; local_get(Local(line_len)); i32_store(0); // cap = len
                      i32_const(Imm32(0)); local_set(Local(copy_i));
                      block_empty; loop_empty;
                        local_get(Local(copy_i)); local_get(Local(line_len)); i32_ge_u; br_if(1);
                        local_get(Local(line_ptr)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(copy_i)); i32_add;
                        local_get(Local(buf)); local_get(Local(line_start)); i32_add; local_get(Local(copy_i)); i32_add;
                        i32_load8_u(0);
                        i32_store8(0);
                        local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                        br(0);
                      end; end;
                });

                // Grow list if needed for last line
                wasm!(self.func, {
                      local_get(Local(list_count)); local_get(Local(list_cap)); i32_ge_u;
                      if_empty;
                        local_get(Local(list_cap)); i32_const(Imm32(CAPACITY_DOUBLE)); i32_mul; local_set(Local(list_cap));
                        local_get(Local(list_cap)); i32_const(Imm32(I32_BYTES)); i32_mul;
                        call(self.emitter.rt.alloc); local_set(Local(new_list));
                        i32_const(Imm32(0)); local_set(Local(copy_i));
                        block_empty; loop_empty;
                          local_get(Local(copy_i)); local_get(Local(list_count)); i32_ge_u; br_if(1);
                          local_get(Local(new_list)); local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                          local_get(Local(list_ptr)); local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                          i32_load(0);
                          i32_store(0);
                          local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                          br(0);
                        end; end;
                        local_get(Local(new_list)); local_set(Local(list_ptr));
                      end;
                      // Append last line
                      local_get(Local(list_ptr)); local_get(Local(list_count)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(line_ptr)); i32_store(0);
                      local_get(Local(list_count)); i32_const(Imm32(1)); i32_add; local_set(Local(list_count));
                    end; // end if line_start < len
                });

                // Build final Almide List: [len:i32][cap:i32][elem0:i32][elem1:i32]...
                // elem_size = I32_BYTES (i32 pointer)
                wasm!(self.func, {
                    local_get(Local(list_count)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_const(Imm32(list_hdr())); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(result));
                    local_get(Local(result)); local_get(Local(list_count)); i32_store(0);
                    local_get(Local(result)); i32_const(Imm32(list_cap_off())); i32_add; local_get(Local(list_count)); i32_store(0); // cap = len
                    // Copy list_ptr[0..list_count] to result+data_off
                    i32_const(Imm32(0)); local_set(Local(copy_i));
                    block_empty; loop_empty;
                      local_get(Local(copy_i)); local_get(Local(list_count)); i32_ge_u; br_if(1);
                      local_get(Local(result)); i32_const(Imm32(list_data_off())); i32_add;
                      local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(list_ptr)); local_get(Local(copy_i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      i32_load(0);
                      i32_store(0);
                      local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                      br(0);
                    end; end;
                    local_get(Local(result));
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
                // (argv[0] = program path, then any program args). Uses WASI
                // args_sizes_get / args_get; builds the same List[String] layout
                // as stdin_lines above: [count:i32][strptr0:i32][strptr1:i32]...
                let argc_ptr = self.scratch.alloc_i32();
                let bufsize_ptr = self.scratch.alloc_i32();
                let argc = self.scratch.alloc_i32();
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
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(argc_ptr));
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(bufsize_ptr));
                    local_get(Local(argc_ptr));
                    local_get(Local(bufsize_ptr));
                    call(self.emitter.rt.args_sizes_get);
                    drop; // discard errno
                    local_get(Local(argc_ptr)); i32_load(0); local_set(Local(argc));
                    local_get(Local(bufsize_ptr)); i32_load(0); local_set(Local(buf_size));
                });

                // --- Phase 2: alloc the pointer array + the string buffer, fill them ---
                // argv_ptr: argc i32 pointers. argv_buf: buf_size NUL-terminated bytes.
                // Guard zero-size allocs with a minimum of 4 bytes so alloc never
                // returns a degenerate pointer (argc is always >= 1 in practice, but
                // stay defensive).
                wasm!(self.func, {
                    // argv_ptr: argc i32 pointers (+I32_BYTES guard so a zero argc never
                    // yields a degenerate alloc).
                    local_get(Local(argc)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_const(Imm32(I32_BYTES)); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(argv_ptr));
                    local_get(Local(buf_size)); i32_const(Imm32(I32_BYTES)); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(argv_buf));
                    local_get(Local(argv_ptr));
                    local_get(Local(argv_buf));
                    call(self.emitter.rt.args_get);
                    drop; // discard errno
                });

                // --- Phase 3: build List[String] = [len][cap][strptr0][strptr1]... ---
                wasm!(self.func, {
                    local_get(Local(argc)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_const(Imm32(list_hdr())); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(result));
                    local_get(Local(result)); local_get(Local(argc)); i32_store(0);
                    local_get(Local(result)); i32_const(Imm32(list_cap_off())); i32_add; local_get(Local(argc)); i32_store(0); // cap = len
                    i32_const(Imm32(0)); local_set(Local(i));
                    block_empty; loop_empty;
                      local_get(Local(i)); local_get(Local(argc)); i32_ge_u; br_if(1);
                      // cstr_ptr = argv_ptr[i]
                      local_get(Local(argv_ptr)); local_get(Local(i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      i32_load(0); local_set(Local(cstr_ptr));
                      // str_len = strlen(cstr_ptr): scan to NUL
                      i32_const(Imm32(0)); local_set(Local(str_len));
                      block_empty; loop_empty;
                        local_get(Local(cstr_ptr)); local_get(Local(str_len)); i32_add; i32_load8_u(0);
                        i32_eqz; br_if(1);
                        local_get(Local(str_len)); i32_const(Imm32(1)); i32_add; local_set(Local(str_len));
                        br(0);
                      end; end;
                      // alloc Almide string [len][cap][bytes...]
                      local_get(Local(str_len)); i32_const(Imm32(string_hdr())); i32_add;
                      call(self.emitter.rt.alloc); local_set(Local(str_ptr));
                      local_get(Local(str_ptr)); local_get(Local(str_len)); i32_store(0);
                      local_get(Local(str_ptr)); i32_const(Imm32(string_cap_off())); i32_add; local_get(Local(str_len)); i32_store(0); // cap = len
                      // copy str_len bytes from cstr_ptr into str_ptr+data_off
                      i32_const(Imm32(0)); local_set(Local(copy_i));
                      block_empty; loop_empty;
                        local_get(Local(copy_i)); local_get(Local(str_len)); i32_ge_u; br_if(1);
                        local_get(Local(str_ptr)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(copy_i)); i32_add;
                        local_get(Local(cstr_ptr)); local_get(Local(copy_i)); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
                        br(0);
                      end; end;
                      // result[data_off + i*I32_BYTES] = str_ptr
                      local_get(Local(result)); i32_const(Imm32(list_data_off())); i32_add;
                      local_get(Local(i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(str_ptr)); i32_store(0);
                      local_get(Local(i)); i32_const(Imm32(1)); i32_add; local_set(Local(i));
                      br(0);
                    end; end;
                    local_get(Local(result));
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
                self.scratch.free_i32(argc);
                self.scratch.free_i32(bufsize_ptr);
                self.scratch.free_i32(argc_ptr);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `process.{}` — \
                 add an arm in emit_process_call or resolve upstream",
                func
            ),
        }
}
}
