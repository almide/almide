//! fs module + helpers — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    pub(super) fn emit_fs_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "read_text" => {
                // fs.read_text(path: String) -> Result[String, String]
                // 1. Evaluate path arg (Almide String ptr: [len:i32][data:u8...])
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Allocate fd_out (4 bytes) via bump allocator
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open(resolved_fd, dirflags=0, path_ptr, path_len, oflags=0,
                //           rights=fd_read|fd_seek (2|4=6), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    local_get(resolved_fd);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    i32_const(0);
                    i64_const(6);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                // If errno != 0, return err("file not found")
                wasm!(self.func, {
                    local_get(errno);
                    i32_const(0);
                    i32_ne;
                    if_i32;
                });
                // Build err result
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // fd_filestat_get(fd, stat_buf) — stat_buf needs 64 bytes (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd);
                    local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get);
                    drop;
                });

                // file_size = i32(stat_buf[32..40]) — file size is at offset 32 as i64, take lower 32 bits
                wasm!(self.func, {
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                });

                // Allocate buffer for file data
                wasm!(self.func, {
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                });

                // Build iov struct: [buf_ptr:i32, buf_len:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                });

                // nread_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                // fd_read(fd, iov_ptr, 1, nread_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(opened_fd);
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build Almide String: [len:i32][data:u8...]
                // Use nread as actual length (may be <= file_size)
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(file_size);
                    local_get(file_size); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(str_ptr);
                    local_get(str_ptr); local_get(file_size); i32_store(0);
                });

                // Copy data_buf[0..file_size] to str_ptr+4
                // Byte-by-byte copy loop
                let counter = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
                    local_get(str_ptr); i32_const(4); i32_add; local_get(counter); i32_add;
                    local_get(data_buf); local_get(counter); i32_add;
                    i32_load8_u(0);
                    i32_store8(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(counter);

                // Build ok result: [tag=0:i32][str_ptr:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(str_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "write" => {
                // fs.write(path: String, content: String) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let content_str = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                // Evaluate path
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Evaluate content
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(content_str); });

                // Allocate fd_out
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open(resolved_fd, dirflags=0, path_ptr, path_len,
                //           oflags=O_CREAT|O_TRUNC(=9),
                //           rights=fd_write(=64), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    local_get(resolved_fd);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    i32_const(9);
                    i64_const(64);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                // If errno != 0, return err
                wasm!(self.func, {
                    local_get(errno);
                    i32_const(0);
                    i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // Build iov: [content_ptr+4, content_len]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(4); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                });

                // nwritten_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                });

                // fd_write(fd, iov_ptr, 1, nwritten_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(opened_fd);
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build ok(unit) result: [tag=0:i32][0:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(content_str);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "exists" => {
                // fs.exists(path: String) -> Bool
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Allocate 64-byte stat buffer (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                });

                // path_filestat_get(resolved_fd, flags=0, path_ptr, path_len, stat_buf)
                wasm!(self.func, {
                    local_get(resolved_fd);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    // errno == 0 → true (1), else false (0)
                    i32_eqz;
                });

                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "read_bytes" => {
                // fs.read_bytes(path) -> Result[List[Int], String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open for reading
                wasm!(self.func, {
                    local_get(resolved_fd); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(0); i64_const(6); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // stat for file size
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                });

                // Read raw bytes
                wasm!(self.func, {
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
                    call(self.emitter.rt.fd_read); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                    local_get(nread_ptr); i32_load(0); local_set(file_size);
                });

                // Build List[Int]: [count:i32][i64 * count]
                wasm!(self.func, {
                    local_get(file_size); i32_const(8); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(file_size); i32_store(0);
                });

                // Copy each byte as i64
                wasm!(self.func, {
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
                    local_get(list_ptr); i32_const(4); i32_add;
                    local_get(counter); i32_const(8); i32_mul; i32_add;
                    local_get(data_buf); local_get(counter); i32_add; i32_load8_u(0);
                    i64_extend_i32_u;
                    i64_store(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(counter);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "write_bytes" => {
                // fs.write_bytes(path, bytes: List[Int]) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let byte_buf = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(list_ptr); });

                // Convert List[Int] (i64 elements) to byte buffer
                wasm!(self.func, {
                    local_get(list_ptr); i32_load(0); local_set(count);
                    local_get(count); call(self.emitter.rt.alloc); local_set(byte_buf);
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(count); i32_ge_u; br_if(1);
                    local_get(byte_buf); local_get(counter); i32_add;
                    local_get(list_ptr); i32_const(4); i32_add;
                    local_get(counter); i32_const(8); i32_mul; i32_add;
                    i64_load(0); i32_wrap_i64;
                    i32_store8(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open for writing (O_CREAT|O_TRUNC=9)
                wasm!(self.func, {
                    local_get(resolved_fd); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(9); i64_const(64); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); local_get(count); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(counter);
                self.scratch.free_i32(count);
                self.scratch.free_i32(byte_buf);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "append" => {
                // fs.append(path, content) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let content_str = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(content_str); });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open: oflags=O_CREAT(1), rights=fd_write(64), fdflags=APPEND(1)
                wasm!(self.func, {
                    local_get(resolved_fd); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(1);
                    i64_const(64); i64_const(0);
                    i32_const(1);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for appending");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(4); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(content_str);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "mkdir_p" => {
                // fs.mkdir_p(path) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Iterative mkdir_p: create each prefix segment
                let seg_end = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(seg_end);
                    block_empty; loop_empty;
                    local_get(seg_end); local_get(path_len); i32_ge_u; br_if(1);
                    // Advance seg_end past current char
                    local_get(seg_end); i32_const(1); i32_add; local_set(seg_end);
                    // Skip to next '/' or end of path
                    block_empty; loop_empty;
                    local_get(seg_end); local_get(path_len); i32_ge_u; br_if(1);
                    local_get(path_ptr); local_get(seg_end); i32_add; i32_load8_u(0);
                    i32_const(47); i32_eq; br_if(1);
                    local_get(seg_end); i32_const(1); i32_add; local_set(seg_end);
                    br(0);
                    end; end;
                    // Try creating directory for path[0..seg_end]
                    local_get(resolved_fd);
                    local_get(path_ptr);
                    local_get(seg_end);
                    call(self.emitter.rt.path_create_directory);
                    drop;
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(seg_end);

                // Final attempt: create the full path and check error
                wasm!(self.func, {
                    local_get(resolved_fd);
                    local_get(path_ptr);
                    local_get(path_len);
                    call(self.emitter.rt.path_create_directory);
                    local_set(errno);
                });

                // errno==0 or errno==20 (EEXIST) -> ok
                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    local_get(errno); i32_const(20); i32_eq;
                    i32_or;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to create directory");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "read_lines" => {
                // fs.read_lines(path) -> Result[List[String], String]
                // Call read_text internally, then split by '\n' using string.lines
                self.emit_fs_call_inner_read_text(args);
                let res = self.scratch.alloc_i32();
                let tag = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(res);
                    local_get(res); i32_load(0); local_set(tag);
                    local_get(tag); i32_eqz;
                    if_i32;
                });
                // ok path: split the string by '\n'
                let text_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(res); i32_const(4); i32_add; i32_load(0); local_set(text_ptr);
                    local_get(text_ptr);
                    call(self.emitter.rt.string.lines);
                });
                let result_ptr = self.scratch.alloc_i32();
                let list_val = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(list_val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_val); i32_store(4);
                    local_get(result_ptr);
                    else_;
                    local_get(res);
                    end;
                });

                self.scratch.free_i32(list_val);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(text_ptr);
                self.scratch.free_i32(tag);
                self.scratch.free_i32(res);
            }
            "list_dir" => {
                // fs.list_dir(path) -> Result[List[String], String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let dir_buf = self.scratch.alloc_i32();
                let bufused_ptr = self.scratch.alloc_i32();
                let bufused = self.scratch.alloc_i32();
                let offset = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let list_count = self.scratch.alloc_i32();
                let entry_name_len = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // path_open for directory: dirflags=1(symlink follow), oflags=O_DIRECTORY(2)
                // rights = fd_readdir(0x4000)
                wasm!(self.func, {
                    local_get(resolved_fd); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(2);
                    i64_const(0x4000);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open directory");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // Allocate readdir buffer (4KB) and bufused output
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(dir_buf);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(bufused_ptr);
                });

                // fd_readdir(fd, buf, buf_len, cookie=0, bufused_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(dir_buf);
                    i32_const(4096);
                    i64_const(0);
                    local_get(bufused_ptr);
                    call(self.emitter.rt.fd_readdir);
                    drop;
                    local_get(bufused_ptr); i32_load(0); local_set(bufused);
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // First pass: count entries (skipping "." and "..")
                // WASI dirent: d_next(8) + d_ino(8) + d_namlen(4) + d_type(4) = 24 bytes header
                let skip = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(offset);
                    i32_const(0); local_set(list_count);
                    block_empty; loop_empty;
                    local_get(offset); i32_const(24); i32_add;
                    local_get(bufused); i32_gt_u; br_if(1);
                    local_get(dir_buf); local_get(offset); i32_add; i32_const(16); i32_add;
                    i32_load(0); local_set(entry_name_len);

                    // skip = (namlen==1 && name[0]=='.') || (namlen==2 && name[0]=='.' && name[1]=='.')
                    i32_const(0); local_set(skip);
                    // Check "."
                    local_get(entry_name_len); i32_const(1); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      if_empty; i32_const(1); local_set(skip); end;
                    end;
                    // Check ".."
                    local_get(entry_name_len); i32_const(2); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(25); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      i32_and;
                      if_empty; i32_const(1); local_set(skip); end;
                    end;
                    // Count if not skipped
                    local_get(skip); i32_eqz;
                    if_empty;
                      local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                    end;

                    // Advance offset
                    local_get(offset); i32_const(24); i32_add; local_get(entry_name_len); i32_add;
                    local_set(offset);
                    br(0);
                    end; end;
                });

                // Allocate List[String]: [count:i32][ptr:i32 * count]
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(list_count); i32_store(0);
                });

                // Second pass: build string entries (same skip logic as counting pass)
                let copy_i = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(offset);
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(offset); i32_const(24); i32_add;
                    local_get(bufused); i32_gt_u; br_if(1);
                    local_get(dir_buf); local_get(offset); i32_add; i32_const(16); i32_add;
                    i32_load(0); local_set(entry_name_len);

                    // skip = (namlen==1 && name[0]=='.') || (namlen==2 && name[0]=='.' && name[1]=='.')
                    i32_const(0); local_set(skip);
                    local_get(entry_name_len); i32_const(1); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      if_empty; i32_const(1); local_set(skip); end;
                    end;
                    local_get(entry_name_len); i32_const(2); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(25); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      i32_and;
                      if_empty; i32_const(1); local_set(skip); end;
                    end;
                    // Build entry if not skipped
                    local_get(skip); i32_eqz;
                    if_empty;
                });
                self.emit_fs_list_dir_build_entry(copy_i, entry_name_len, str_ptr, dir_buf, offset, list_ptr, counter);
                wasm!(self.func, {
                    end;

                    // Advance offset
                    local_get(offset); i32_const(24); i32_add; local_get(entry_name_len); i32_add;
                    local_set(offset);
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(copy_i);

                // Update list count
                wasm!(self.func, {
                    local_get(list_ptr); local_get(counter); i32_store(0);
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(skip);
                self.scratch.free_i32(counter);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(entry_name_len);
                self.scratch.free_i32(list_count);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(offset);
                self.scratch.free_i32(bufused);
                self.scratch.free_i32(bufused_ptr);
                self.scratch.free_i32(dir_buf);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "is_dir" => {
                // fs.is_dir(path) -> Bool  (filetype 3 = directory)
                self.emit_fs_filetype_check(args, 3);
            }
            "is_file" => {
                // fs.is_file(path) -> Bool  (filetype 4 = regular file)
                self.emit_fs_filetype_check(args, 4);
            }
            "is_symlink" => {
                // fs.is_symlink(path) -> Bool  (filetype 7 = symbolic_link)
                // Use flags=0 (do NOT follow symlinks)
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    // flags=0: do NOT follow symlinks
                    local_get(resolved_fd); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                      i32_const(7); i32_eq;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "copy" => {
                // fs.copy(src, dst) -> Result[Unit, String]
                // Read source file bytes, write to destination
                let src_str = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let src_len = self.scratch.alloc_i32();
                let src_resolved_fd = self.scratch.alloc_i32();
                let dst_str = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let dst_len = self.scratch.alloc_i32();
                let dst_resolved_fd = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nrw_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src_str); });
                self.emit_fs_resolve_path(src_str, src_ptr, src_len, src_resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(dst_str); });
                self.emit_fs_resolve_path(dst_str, dst_ptr, dst_len, dst_resolved_fd);

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Open source for reading
                wasm!(self.func, {
                    local_get(src_resolved_fd); i32_const(0);
                    local_get(src_ptr); local_get(src_len);
                    i32_const(0); i64_const(6); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open source file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Read source content
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nrw_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nrw_ptr);
                    call(self.emitter.rt.fd_read); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                    local_get(nrw_ptr); i32_load(0); local_set(file_size);
                });

                // Open dst for writing
                wasm!(self.func, {
                    local_get(dst_resolved_fd); i32_const(0);
                    local_get(dst_ptr); local_get(dst_len);
                    i32_const(9); i64_const(64); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg2 = self.emitter.intern_string("failed to open destination file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg2 as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Write data to dst
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nrw_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit) -- close nested if blocks
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nrw_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(dst_resolved_fd);
                self.scratch.free_i32(dst_len);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(dst_str);
                self.scratch.free_i32(src_resolved_fd);
                self.scratch.free_i32(src_len);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(src_str);
            }
            "rename" => {
                // fs.rename(src, dst) -> Result[Unit, String]
                let src_str = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let src_len = self.scratch.alloc_i32();
                let src_resolved_fd = self.scratch.alloc_i32();
                let dst_str = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let dst_len = self.scratch.alloc_i32();
                let dst_resolved_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src_str); });
                self.emit_fs_resolve_path(src_str, src_ptr, src_len, src_resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(dst_str); });
                self.emit_fs_resolve_path(dst_str, dst_ptr, dst_len, dst_resolved_fd);

                // path_rename(old_fd, old_path, old_len, new_fd, new_path, new_len)
                wasm!(self.func, {
                    local_get(src_resolved_fd);
                    local_get(src_ptr); local_get(src_len);
                    local_get(dst_resolved_fd);
                    local_get(dst_ptr); local_get(dst_len);
                    call(self.emitter.rt.path_rename);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to rename");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(dst_resolved_fd);
                self.scratch.free_i32(dst_len);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(dst_str);
                self.scratch.free_i32(src_resolved_fd);
                self.scratch.free_i32(src_len);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(src_str);
            }
            "remove" => {
                // fs.remove(path) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    local_get(resolved_fd);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_unlink_file);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "remove_all" => {
                // fs.remove_all(path) -> Result[Unit, String]
                // Try unlink (file), then rmdir (empty dir)
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Try path_unlink_file first
                wasm!(self.func, {
                    local_get(resolved_fd);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_unlink_file);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Try path_remove_directory
                wasm!(self.func, {
                    local_get(resolved_fd);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_remove_directory);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove path");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "file_size" => {
                // fs.file_size(path) -> Result[Int, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(resolved_fd); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                // ok: file size at offset 32 as i64
                // Result[Int, String] = [tag:i32][padding:i32][i64] = 16 bytes
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(32); i32_add; i64_load(0);
                    i64_store(0);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    i32_const(err_msg as i32); i64_extend_i32_u; i64_store(0);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "modified_at" => {
                // fs.modified_at(path) -> Result[Int, String]
                // mtim at offset 40 (u64, nanoseconds) -> seconds
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(resolved_fd); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(40); i32_add; i64_load(0);
                    i64_const(1000000000); i64_div_u;
                    i64_store(0);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    i32_const(err_msg as i32); i64_extend_i32_u; i64_store(0);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "stat" => {
                // fs.stat(path) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let resolved_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let rec_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(path_str); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(resolved_fd); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });

                // Record: [size:i64(8)][is_dir:i32(4)][is_file:i32(4)][modified:i64(8)] = 24 bytes
                wasm!(self.func, {
                    i32_const(24); call(self.emitter.rt.alloc); local_set(rec_ptr);
                    // size at stat offset 32
                    local_get(rec_ptr);
                    local_get(stat_buf); i32_const(32); i32_add; i64_load(0);
                    i64_store(0);
                    // is_dir: filetype at offset 16 == 3
                    local_get(rec_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                    i32_const(3); i32_eq;
                    i32_store(0);
                    // is_file: filetype at offset 16 == 4
                    local_get(rec_ptr); i32_const(12); i32_add;
                    local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                    i32_const(4); i32_eq;
                    i32_store(0);
                    // modified: mtim at stat offset 40, nanoseconds -> seconds
                    local_get(rec_ptr); i32_const(16); i32_add;
                    local_get(stat_buf); i32_const(40); i32_add; i64_load(0);
                    i64_const(1000000000); i64_div_u;
                    i64_store(0);
                });

                // ok(rec_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(rec_ptr); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(rec_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(resolved_fd);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "walk" | "glob" | "create_temp_file" | "create_temp_dir" => {
                // These require recursive dir traversal (walk), glob pattern matching (glob),
                // or OS temp dir + random naming (create_temp_*) which are infeasible in pure WASI.
                for arg in args { self.emit_expr(arg); if super::values::ty_to_valtype(&arg.ty).is_some() { wasm!(self.func, { drop; }); } }
                let result_ptr = self.scratch.alloc_i32();
                let err_msg = self.emitter.intern_string("not supported in WASM");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                });
                self.scratch.free_i32(result_ptr);
            }
            "temp_dir" => {
                // fs.temp_dir() -> String: return "/tmp"
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(s as i32); });
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `fs.{}` — \
                 add an arm in emit_fs_call or resolve upstream",
                func
            ),
        }
    }

    /// Helper: resolve path using __resolve_path runtime function.
    /// After calling, the scratch locals contain:
    /// - resolved_fd: the preopened dir fd
    /// - path_ptr: pointer to the relative path bytes
    /// - path_len: length of the relative path
    /// This replaces the old pattern of hardcoded fd=3 + strip leading '/'.
    pub(super) fn emit_fs_resolve_path(&mut self, path_str: u32, path_ptr: u32, path_len: u32, resolved_fd: u32) {
        let resolve_result = self.scratch.alloc_i32();
        wasm!(self.func, {
            // Call __resolve_path(path_ptr, path_len) → result_ptr [fd, rel_ptr, rel_len]
            local_get(path_str); i32_const(4); i32_add; // raw path bytes (skip string length prefix)
            local_get(path_str); i32_load(0);            // path byte length
            call(self.emitter.rt.resolve_path);
            local_set(resolve_result);
            // Unpack result
            local_get(resolve_result); i32_load(0); local_set(resolved_fd);
            local_get(resolve_result); i32_load(4); local_set(path_ptr);
            local_get(resolve_result); i32_load(8); local_set(path_len);
        });
        self.scratch.free_i32(resolve_result);
    }

    /// Helper: check path filetype against expected value. Used by is_dir, is_file.
    pub(super) fn emit_fs_filetype_check(&mut self, args: &[IrExpr], expected_filetype: i32) {
        let path_str = self.scratch.alloc_i32();
        let path_ptr = self.scratch.alloc_i32();
        let path_len = self.scratch.alloc_i32();
        let resolved_fd = self.scratch.alloc_i32();
        let stat_buf = self.scratch.alloc_i32();
        let errno = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(path_str); });
        self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

        wasm!(self.func, {
            i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
            // flags=1 (follow symlinks) for is_dir/is_file
            local_get(resolved_fd); i32_const(1);
            local_get(path_ptr); local_get(path_len);
            local_get(stat_buf);
            call(self.emitter.rt.path_filestat_get);
            local_set(errno);
            local_get(errno); i32_const(0); i32_ne;
            if_i32;
              i32_const(0);
            else_;
              // filetype at stat offset 16
              local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
              i32_const(expected_filetype);
              i32_eq;
            end;
        });

        self.scratch.free_i32(errno);
        self.scratch.free_i32(stat_buf);
        self.scratch.free_i32(resolved_fd);
        self.scratch.free_i32(path_len);
        self.scratch.free_i32(path_ptr);
        self.scratch.free_i32(path_str);
    }

    /// Helper for list_dir: build a string entry from dirent name and store into list.
    pub(super) fn emit_fs_list_dir_build_entry(
        &mut self,
        copy_i: u32, entry_name_len: u32, str_ptr: u32,
        dir_buf: u32, offset: u32, list_ptr: u32, counter: u32,
    ) {
        wasm!(self.func, {
            local_get(entry_name_len); i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(entry_name_len); i32_store(0);
            // Copy name bytes
            i32_const(0); local_set(copy_i);
            block_empty; loop_empty;
            local_get(copy_i); local_get(entry_name_len); i32_ge_u; br_if(1);
            local_get(str_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
            local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
            local_get(copy_i); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
            br(0);
            end; end;
            // Store in list
            local_get(list_ptr); i32_const(4); i32_add;
            local_get(counter); i32_const(4); i32_mul; i32_add;
            local_get(str_ptr); i32_store(0);
            local_get(counter); i32_const(1); i32_add; local_set(counter);
        });
    }

    /// Helper: emit read_text logic, leaving Result[String, String] on stack.
    pub(super) fn emit_fs_call_inner_read_text(&mut self, args: &[IrExpr]) {
        let path_str = self.scratch.alloc_i32();
        let path_ptr = self.scratch.alloc_i32();
        let path_len = self.scratch.alloc_i32();
        let resolved_fd = self.scratch.alloc_i32();
        let fd_out_ptr = self.scratch.alloc_i32();
        let opened_fd = self.scratch.alloc_i32();
        let stat_buf = self.scratch.alloc_i32();
        let file_size = self.scratch.alloc_i32();
        let data_buf = self.scratch.alloc_i32();
        let iov_ptr = self.scratch.alloc_i32();
        let nread_ptr = self.scratch.alloc_i32();
        let result_ptr = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let errno = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(path_str); });
        self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

        wasm!(self.func, {
            i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
        });

        wasm!(self.func, {
            local_get(resolved_fd); i32_const(0);
            local_get(path_ptr); local_get(path_len);
            i32_const(0); i64_const(6); i64_const(0); i32_const(0);
            local_get(fd_out_ptr);
            call(self.emitter.rt.path_open);
            local_set(errno);
        });

        wasm!(self.func, {
            local_get(errno); i32_const(0); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("file not found");
        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
            local_get(result_ptr); i32_const(1); i32_store(0);
            local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
            local_get(result_ptr);
            else_;
        });

        wasm!(self.func, {
            local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
            i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
            local_get(opened_fd); local_get(stat_buf);
            call(self.emitter.rt.fd_filestat_get); drop;
            local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
            local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
            i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
            local_get(iov_ptr); local_get(data_buf); i32_store(0);
            local_get(iov_ptr); local_get(file_size); i32_store(4);
            i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
            local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
            call(self.emitter.rt.fd_read); drop;
            local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
            local_get(nread_ptr); i32_load(0); local_set(file_size);
        });

        wasm!(self.func, {
            local_get(file_size); i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(file_size); i32_store(0);
        });

        let counter = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(0); local_set(counter);
            block_empty; loop_empty;
            local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
            local_get(str_ptr); i32_const(4); i32_add; local_get(counter); i32_add;
            local_get(data_buf); local_get(counter); i32_add;
            i32_load8_u(0);
            i32_store8(0);
            local_get(counter); i32_const(1); i32_add; local_set(counter);
            br(0);
            end; end;
        });
        self.scratch.free_i32(counter);

        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
            local_get(result_ptr); i32_const(0); i32_store(0);
            local_get(result_ptr); local_get(str_ptr); i32_store(4);
            local_get(result_ptr);
            end;
        });

        self.scratch.free_i32(errno);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(result_ptr);
        self.scratch.free_i32(nread_ptr);
        self.scratch.free_i32(iov_ptr);
        self.scratch.free_i32(data_buf);
        self.scratch.free_i32(file_size);
        self.scratch.free_i32(stat_buf);
        self.scratch.free_i32(opened_fd);
        self.scratch.free_i32(fd_out_ptr);
        self.scratch.free_i32(resolved_fd);
        self.scratch.free_i32(path_len);
        self.scratch.free_i32(path_ptr);
        self.scratch.free_i32(path_str);
    }

}
