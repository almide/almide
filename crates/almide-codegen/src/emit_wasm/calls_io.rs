//! io module — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    pub(super) fn emit_io_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "print" => {
                // io.print(s: String) -> Unit
                // Same as println but WITHOUT the trailing newline.
                let s = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    // iov[0].buf = s + 4 (skip length prefix)
                    i32_const(0);
                    local_get(s); i32_const(4); i32_add;
                    i32_store(0);
                    // iov[0].len = *s (load length)
                    i32_const(4);
                    local_get(s); i32_load(0);
                    i32_store(0);
                    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
                    i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                    call(self.emitter.rt.fd_write);
                    drop;
                });
                self.scratch.free_i32(s);
            }
            "read_line" => {
                // io.read_line() -> String
                // Read one byte at a time from stdin (fd=0) until '\n' or EOF.
                // Accumulate into a heap buffer, then build an Almide string.
                let buf = self.scratch.alloc_i32();       // growing buffer ptr
                let capacity = self.scratch.alloc_i32();  // current capacity
                let len = self.scratch.alloc_i32();       // bytes read so far
                let iov_ptr = self.scratch.alloc_i32();   // iov struct for fd_read
                let nread_ptr = self.scratch.alloc_i32(); // nread output
                let byte_buf = self.scratch.alloc_i32();  // 1-byte read target
                let nread_val = self.scratch.alloc_i32(); // loaded nread value
                let byte_val = self.scratch.alloc_i32();  // loaded byte value
                let new_buf = self.scratch.alloc_i32();   // for realloc copy
                let copy_i = self.scratch.alloc_i32();    // copy loop counter
                let result = self.scratch.alloc_i32();    // final string ptr

                // Initial capacity = 256
                wasm!(self.func, {
                    i32_const(256); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(256); local_set(capacity);
                    i32_const(0); local_set(len);
                    // Allocate iov (8 bytes) and nread (4 bytes) and byte_buf (1 byte)
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    i32_const(1); call(self.emitter.rt.alloc); local_set(byte_buf);
                });

                // Main read loop
                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow buffer if full: len >= capacity
                wasm!(self.func, {
                    local_get(len); local_get(capacity); i32_ge_u;
                    if_empty;
                      // Double capacity
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      // Copy old data
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

                // Set up iov to read 1 byte into byte_buf
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); i32_const(1); i32_store(4);
                    // fd_read(stdin=0, iov_ptr, 1, nread_ptr)
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // Check nread: if 0, EOF → break
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1); // break outer block
                });

                // Load byte, check for '\n'
                wasm!(self.func, {
                    local_get(byte_buf); i32_load8_u(0); local_set(byte_val);
                    local_get(byte_val); i32_const(10); i32_eq; // '\n'
                    br_if(1); // break outer block (don't include '\n' in result)
                });

                // Append byte to buffer
                wasm!(self.func, {
                    local_get(buf); local_get(len); i32_add;
                    local_get(byte_val);
                    i32_store8(0);
                    local_get(len); i32_const(1); i32_add; local_set(len);
                    br(0); // continue loop
                    end; end; // end loop, end block
                });

                // Build Almide string [len:i32][data:u8...]
                wasm!(self.func, {
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    // Copy buf[0..len] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add; local_get(copy_i); i32_add;
                      local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                      i32_store8(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(byte_val);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(byte_buf);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            "read_all" => {
                // io.read_all() -> String
                // Read all bytes from stdin (fd=0) until EOF.
                // Strategy: read in chunks of 4096 bytes, grow buffer as needed.
                let buf = self.scratch.alloc_i32();
                let capacity = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                let chunk_buf = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                // Initial capacity = 4096
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(4096); local_set(capacity);
                    i32_const(0); local_set(len);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                // Read loop
                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Ensure we have room for at least 4096 bytes
                wasm!(self.func, {
                    local_get(capacity); local_get(len); i32_sub;
                    i32_const(4096); i32_lt_u;
                    if_empty;
                      // Double capacity
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      // Copy old data
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

                // Read chunk into buf+len, up to (capacity - len) bytes
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(buf); local_get(len); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(capacity); local_get(len); i32_sub; i32_store(4);
                    // fd_read(stdin=0, iov_ptr, 1, nread_ptr)
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // Check nread: if 0, EOF → break
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1);
                    // Advance len
                    local_get(len); local_get(nread_val); i32_add; local_set(len);
                    br(0);
                    end; end; // end loop, end block
                });

                // Build Almide string [len:i32][data:u8...]
                wasm!(self.func, {
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    // Copy buf[0..len] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add; local_get(copy_i); i32_add;
                      local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                      i32_store8(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(chunk_buf);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            "read_byte" => {
                // io.read_byte() -> Int
                // Read 1 byte from stdin via fd_read. Return byte as i64, or -1i64 on EOF.
                let byte_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();

                wasm!(self.func, {
                    i32_const(1); call(self.emitter.rt.alloc); local_set(byte_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    // iov: buf = byte_buf, len = 1
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); i32_const(1); i32_store(4);
                    // fd_read(stdin=0, iov, 1, nread_ptr)
                    i32_const(0); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                    // if nread == 0 → -1i64, else byte as i64
                    local_get(nread_ptr); i32_load(0); i32_eqz;
                    if_i64;
                      i64_const(-1);
                    else_;
                      local_get(byte_buf); i32_load8_u(0); i64_extend_i32_u;
                    end;
                });

                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(byte_buf);
            }
            "read_n_bytes" => {
                // io.read_n_bytes(n: Int) -> List[Int]
                // Read up to n bytes from stdin, return as List[Int].
                // List[Int] layout: [count:i32][elem0:i64][elem1:i64]...
                let n = self.scratch.alloc_i32();
                let raw_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // Allocate raw read buffer of n bytes
                    local_get(n); call(self.emitter.rt.alloc); local_set(raw_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    i32_const(0); local_set(total);
                });

                // Read loop: keep reading until total == n or EOF
                wasm!(self.func, {
                    block_empty; loop_empty;
                      // Check if done
                      local_get(total); local_get(n); i32_ge_u; br_if(1);
                      // iov: buf = raw_buf + total, len = n - total
                      local_get(iov_ptr); local_get(raw_buf); local_get(total); i32_add; i32_store(0);
                      local_get(iov_ptr); local_get(n); local_get(total); i32_sub; i32_store(4);
                      // fd_read(stdin=0, iov, 1, nread_ptr)
                      i32_const(0); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
                      call(self.emitter.rt.fd_read);
                      drop;
                      // Check nread
                      local_get(nread_ptr); i32_load(0); local_set(nread_val);
                      local_get(nread_val); i32_eqz; br_if(1); // EOF → break
                      local_get(total); local_get(nread_val); i32_add; local_set(total);
                      br(0);
                    end; end;
                });

                // Build List[Int]: [total:i32][i64 * total]
                wasm!(self.func, {
                    // Allocate: 4 + total * 8
                    local_get(total); i32_const(8); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(total); i32_store(0);
                    // Copy bytes → i64 elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(list_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(raw_buf); local_get(i); i32_add; i32_load8_u(0); i64_extend_i32_u;
                      i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(list_ptr);
                });

                self.scratch.free_i32(i);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(total);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(raw_buf);
                self.scratch.free_i32(n);
            }
            "write" | "write_bytes" => {
                // io.write(data: Bytes) — layout [len:i32][u8 data...], same as print
                // io.write_bytes(data: List[Int]) — layout [len:i32][i64 elements...]
                if func == "write" {
                    let s = self.scratch.alloc_i32();
                    self.emit_expr(&args[0]);
                    wasm!(self.func, {
                        local_set(s);
                        i32_const(0);
                        local_get(s); i32_const(4); i32_add;
                        i32_store(0);
                        i32_const(4);
                        local_get(s); i32_load(0);
                        i32_store(0);
                        i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                        call(self.emitter.rt.fd_write);
                        drop;
                    });
                    self.scratch.free_i32(s);
                } else {
                    // write_bytes: List[Int] → convert i64 to u8 then write
                    let list_ptr = self.scratch.alloc_i32();
                    let len = self.scratch.alloc_i32();
                    let tmp_buf = self.scratch.alloc_i32();
                    let i = self.scratch.alloc_i32();
                    self.emit_expr(&args[0]);
                    wasm!(self.func, {
                        local_set(list_ptr);
                        local_get(list_ptr); i32_load(0); local_set(len);
                        local_get(len); call(self.emitter.rt.alloc); local_set(tmp_buf);
                        i32_const(0); local_set(i);
                        block_empty; loop_empty;
                          local_get(i); local_get(len); i32_ge_u; br_if(1);
                          local_get(tmp_buf); local_get(i); i32_add;
                          local_get(list_ptr); i32_const(4); i32_add;
                          local_get(i); i32_const(8); i32_mul; i32_add;
                          i64_load(0); i32_wrap_i64;
                          i32_store8(0);
                          local_get(i); i32_const(1); i32_add; local_set(i);
                          br(0);
                        end; end;
                        i32_const(0); local_get(tmp_buf); i32_store(0);
                        i32_const(4); local_get(len); i32_store(0);
                        i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                        call(self.emitter.rt.fd_write);
                        drop;
                    });
                    self.scratch.free_i32(i);
                    self.scratch.free_i32(tmp_buf);
                    self.scratch.free_i32(len);
                    self.scratch.free_i32(list_ptr);
                }
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }
}
