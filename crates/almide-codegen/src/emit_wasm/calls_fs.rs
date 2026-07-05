//! fs module + helpers — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;

impl FuncCompiler<'_> {
    pub(super) fn emit_fs_call(&mut self, func: &str, args: &[IrExpr]) {
        // Dispatch is split across DISJOINT sub-match groups (each `func`
        // string matches exactly one). Chain order is irrelevant. Group 1
        // (read_text / write / exists / read_bytes) + the real default live
        // in this method; groups 2-3 live in calls_fs_p2.rs / calls_fs_p3.rs.
        if self.emit_fs_call_g2(func, args) { return; }
        if self.emit_fs_call_g3(func, args) { return; }
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                    local_get(file_size); call(self.emitter.rt.alloc_pinned); local_set(data_buf);
                });

                // Build iov struct: [buf_ptr:i32, buf_len:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                });

                // nread_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nread_ptr);
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
                    local_get(file_size); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
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
                    local_get(str_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(counter); i32_add;
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                });

                // nwritten_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nwritten_ptr);
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                });

                // Read raw bytes
                wasm!(self.func, {
                    local_get(file_size); call(self.emitter.rt.alloc_pinned); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nread_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
                    call(self.emitter.rt.fd_read); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                    local_get(nread_ptr); i32_load(0); local_set(file_size);
                });

                // Build List[Int]: [len:i32][cap:i32][i64 * count]
                wasm!(self.func, {
                    local_get(file_size); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(file_size); i32_store(0);
                });

                // Copy each byte as i64
                wasm!(self.func, {
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
                    local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
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
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `fs.{}` — \
                 add an arm in emit_fs_call or resolve upstream",
                func
            ),
        }
    }
}

// Dispatch groups + helpers moved out to keep every file < 1000 lines.
// These are module-level includes (each part re-opens `impl FuncCompiler<'_>`),
// mirroring the established mod_p*.rs convention in this crate.
include!("calls_fs_p2.rs");
include!("calls_fs_p3.rs");
include!("calls_fs_p4.rs");
