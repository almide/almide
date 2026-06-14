//! fs module + helpers — WASM codegen dispatch.

use crate::emit_wasm::engine::{Imm32, Imm64, Local};
use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

/// Named WASM immediate constants for fs-call codegen.
mod imm {
    // ── byte widths ────────────────────────────────────────────────────
    /// Byte size of an i32 / pointer (used to alloc i32-sized scratch slots,
    /// and as the pointer stride in List[String] and Result-pair payload offset).
    pub const I32_BYTES: i32 = 4;
    /// Byte size of an i64 element (stride in List[Int] element array).
    pub const I64_BYTES: i32 = 8;
    /// Byte size of a two-field i32 pair: Result/IOV struct [tag:i32, payload:i32].
    pub const RESULT_PAIR_BYTES: i32 = I64_BYTES; // 2 × I32_BYTES
    /// Byte size of a Result[Int, String] slot [tag:i32, pad:i32, value:i64].
    pub const RESULT_INT_BYTES: i32 = 16;

    // ── WASI filestat (64-byte struct) ─────────────────────────────────
    /// Total byte size of the WASI filestat_t buffer to allocate.
    pub const WASI_FILESTAT_BUF_BYTES: i32 = 64;
    /// Byte offset of the filetype field within filestat_t (dev:8 + ino:8 = 16).
    pub const WASI_FILESTAT_OFFSET_FILETYPE: i32 = 16;
    /// Byte offset of the st_size field within filestat_t.
    pub const WASI_FILESTAT_OFFSET_SIZE: i32 = 32;
    /// Byte offset of the st_mtim field within filestat_t (nanoseconds).
    pub const WASI_FILESTAT_OFFSET_MTIM: i32 = 40;

    // ── WASI dirent ────────────────────────────────────────────────────
    /// Byte size of a WASI dirent header (d_next:8 + d_ino:8 + d_namlen:4 + d_type:4).
    pub const WASI_DIRENT_HEADER_BYTES: i32 = 24;
    /// Byte offset of d_namlen within a WASI dirent (d_next:8 + d_ino:8).
    pub const WASI_DIRENT_OFFSET_NAMLEN: i32 = 16;
    /// Byte offset of the first name character within a WASI dirent (= header size).
    pub const WASI_DIRENT_OFFSET_NAME: i32 = 24;
    /// Byte offset of the second name character within a WASI dirent (for ".." check).
    pub const WASI_DIRENT_OFFSET_NAME1: i32 = 25;
    /// Name length of the ".." directory entry (used to identify it by length).
    pub const DOTDOT_NAME_LEN: i32 = 2;

    // ── WASI path_open flags / rights ─────────────────────────────────
    /// oflags: O_CREAT | O_TRUNC — create and truncate on open for write.
    pub const WASI_OFLAGS_CREAT_TRUNC: i32 = 9;
    /// oflags: O_DIRECTORY — open target as a directory.
    pub const WASI_OFLAGS_DIRECTORY: i32 = 2;
    /// rights: fd_read (2) | fd_seek (4) — minimum rights to read a file.
    pub const WASI_RIGHTS_FD_READ_SEEK: i64 = 6;
    /// rights: fd_write (64) — right to write to a file.
    pub const WASI_RIGHTS_FD_WRITE: i64 = 64;
    /// rights: fd_readdir (0x4000) — right to read directory entries.
    pub const WASI_RIGHTS_FD_READDIR: i64 = 0x4000;

    // ── WASI filetypes ─────────────────────────────────────────────────
    /// WASI filetype value: directory.
    pub const WASI_FILETYPE_DIRECTORY: i32 = 3;
    /// WASI filetype value: regular file.
    pub const WASI_FILETYPE_REGULAR_FILE: i32 = 4;
    /// WASI filetype value: symbolic link.
    pub const WASI_FILETYPE_SYMLINK: i32 = 7;

    // ── WASI errno ─────────────────────────────────────────────────────
    /// WASI errno EEXIST: file or directory already exists (mkdir_p success case).
    pub const WASI_ERRNO_EEXIST: i32 = 20;

    // ── readdir ────────────────────────────────────────────────────────
    /// Byte size of the readdir buffer (4 KiB).
    pub const WASI_READDIR_BUF_BYTES: i32 = 4096;

    // ── Almide stat record layout ──────────────────────────────────────
    // Record: [size:i64(8)][is_dir:i32(4)][is_file:i32(4)][modified:i64(8)] = 24 bytes.
    /// Byte offset of the is_dir field in the Almide stat record.
    pub const STAT_REC_OFFSET_IS_DIR: i32 = I64_BYTES; // after size:i64
    /// Byte offset of the is_file field in the Almide stat record.
    pub const STAT_REC_OFFSET_IS_FILE: i32 = 12;
    /// Byte offset of the modified field in the Almide stat record.
    pub const STAT_REC_OFFSET_MODIFIED: i32 = 16;
    /// Total byte size of the Almide stat record.
    pub const STAT_REC_BYTES: i32 = 24;

    // ── ASCII character codes ──────────────────────────────────────────
    /// ASCII code for '.' (used to identify "." and ".." directory entries).
    pub const ASCII_DOT: i32 = 46;
    /// ASCII code for '/' (path separator, used in mkdir_p segment scan).
    pub const ASCII_SLASH: i32 = 47;

    // ── time conversion ────────────────────────────────────────────────
    /// Nanoseconds per second (converts WASI nanosecond timestamps to seconds).
    pub const NANOS_PER_SEC: i64 = 1_000_000_000;
}
use imm::*;

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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Allocate fd_out (4 bytes) via bump allocator
                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open(resolved_fd, dirflags=0, path_ptr, path_len, oflags=0,
                //           rights=fd_read|fd_seek (2|4=6), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    i32_const(Imm32(0));
                    local_get(Local(path_ptr));
                    local_get(Local(path_len));
                    i32_const(Imm32(0));
                    i64_const(Imm64(WASI_RIGHTS_FD_READ_SEEK));
                    i64_const(Imm64(0));
                    i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                // If errno != 0, return err("file not found")
                wasm!(self.func, {
                    local_get(Local(errno));
                    i32_const(Imm32(0));
                    i32_ne;
                    if_i32;
                });
                // Build err result
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                });

                // fd_filestat_get(fd, stat_buf) — stat_buf needs 64 bytes (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(opened_fd));
                    local_get(Local(stat_buf));
                    call(self.emitter.rt.fd_filestat_get);
                    drop;
                });

                // file_size = i32(stat_buf[32..40]) — file size is at offset 32 as i64, take lower 32 bits
                wasm!(self.func, {
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i32_load(0); local_set(Local(file_size));
                });

                // Allocate buffer for file data
                wasm!(self.func, {
                    local_get(Local(file_size)); call(self.emitter.rt.alloc_pinned); local_set(Local(data_buf));
                });

                // Build iov struct: [buf_ptr:i32, buf_len:i32]
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(data_buf)); i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(file_size)); i32_store(4);
                });

                // nread_ptr
                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nread_ptr));
                });

                // fd_read(fd, iov_ptr, 1, nread_ptr)
                wasm!(self.func, {
                    local_get(Local(opened_fd));
                    local_get(Local(iov_ptr));
                    i32_const(Imm32(1));
                    local_get(Local(nread_ptr));
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(Local(opened_fd));
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build Almide String: [len:i32][data:u8...]
                // Use nread as actual length (may be <= file_size)
                wasm!(self.func, {
                    local_get(Local(nread_ptr)); i32_load(0); local_set(Local(file_size));
                    local_get(Local(file_size)); i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32)); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(str_ptr));
                    local_get(Local(str_ptr)); local_get(Local(file_size)); i32_store(0);
                });

                // Copy data_buf[0..file_size] to str_ptr+4
                // Byte-by-byte copy loop
                let counter = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(0)); local_set(Local(counter));
                    block_empty; loop_empty;
                    local_get(Local(counter)); local_get(Local(file_size)); i32_ge_u; br_if(1);
                    local_get(Local(str_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; local_get(Local(counter)); i32_add;
                    local_get(Local(data_buf)); local_get(Local(counter)); i32_add;
                    i32_load8_u(0);
                    i32_store8(0);
                    local_get(Local(counter)); i32_const(Imm32(1)); i32_add; local_set(Local(counter));
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(counter);

                // Build ok result: [tag=0:i32][str_ptr:i32]
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); local_get(Local(str_ptr)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Evaluate content
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(content_str)); });

                // Allocate fd_out
                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open(resolved_fd, dirflags=0, path_ptr, path_len,
                //           oflags=O_CREAT|O_TRUNC(=9),
                //           rights=fd_write(=64), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    i32_const(Imm32(0));
                    local_get(Local(path_ptr));
                    local_get(Local(path_len));
                    i32_const(Imm32(WASI_OFLAGS_CREAT_TRUNC));
                    i64_const(Imm64(WASI_RIGHTS_FD_WRITE));
                    i64_const(Imm64(0));
                    i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                // If errno != 0, return err
                wasm!(self.func, {
                    local_get(Local(errno));
                    i32_const(Imm32(0));
                    i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                });

                // Build iov: [content_ptr+4, content_len]
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(content_str)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(content_str)); i32_load(0); i32_store(4);
                });

                // nwritten_ptr
                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nwritten_ptr));
                });

                // fd_write(fd, iov_ptr, 1, nwritten_ptr)
                wasm!(self.func, {
                    local_get(Local(opened_fd));
                    local_get(Local(iov_ptr));
                    i32_const(Imm32(1));
                    local_get(Local(nwritten_ptr));
                    call(self.emitter.rt.fd_write);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(Local(opened_fd));
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build ok(unit) result: [tag=0:i32][0:i32]
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Allocate 64-byte stat buffer (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                });

                // path_filestat_get(resolved_fd, flags=0, path_ptr, path_len, stat_buf)
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    i32_const(Imm32(0));
                    local_get(Local(path_ptr));
                    local_get(Local(path_len));
                    local_get(Local(stat_buf));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open for reading
                wasm!(self.func, {
                    local_get(Local(resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    i32_const(Imm32(0)); i64_const(Imm64(WASI_RIGHTS_FD_READ_SEEK)); i64_const(Imm64(0)); i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // stat for file size
                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(opened_fd)); local_get(Local(stat_buf));
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i32_load(0); local_set(Local(file_size));
                });

                // Read raw bytes
                wasm!(self.func, {
                    local_get(Local(file_size)); call(self.emitter.rt.alloc_pinned); local_set(Local(data_buf));
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(data_buf)); i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(file_size)); i32_store(4);
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nread_ptr));
                    local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nread_ptr));
                    call(self.emitter.rt.fd_read); drop;
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                    local_get(Local(nread_ptr)); i32_load(0); local_set(Local(file_size));
                });

                // Build List[Int]: [len:i32][cap:i32][i64 * count]
                wasm!(self.func, {
                    local_get(Local(file_size)); i32_const(Imm32(I64_BYTES)); i32_mul; i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32)); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(list_ptr));
                    local_get(Local(list_ptr)); local_get(Local(file_size)); i32_store(0);
                });

                // Copy each byte as i64
                wasm!(self.func, {
                    i32_const(Imm32(0)); local_set(Local(counter));
                    block_empty; loop_empty;
                    local_get(Local(counter)); local_get(Local(file_size)); i32_ge_u; br_if(1);
                    local_get(Local(list_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                    local_get(Local(counter)); i32_const(Imm32(I64_BYTES)); i32_mul; i32_add;
                    local_get(Local(data_buf)); local_get(Local(counter)); i32_add; i32_load8_u(0);
                    i64_extend_i32_u;
                    i64_store(0);
                    local_get(Local(counter)); i32_const(Imm32(1)); i32_add; local_set(Local(counter));
                    br(0);
                    end; end;
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); local_get(Local(list_ptr)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(list_ptr)); });

                // Convert List[Int] (i64 elements) to byte buffer
                wasm!(self.func, {
                    local_get(Local(list_ptr)); i32_load(0); local_set(Local(count));
                    local_get(Local(count)); call(self.emitter.rt.alloc_pinned); local_set(Local(byte_buf));
                    i32_const(Imm32(0)); local_set(Local(counter));
                    block_empty; loop_empty;
                    local_get(Local(counter)); local_get(Local(count)); i32_ge_u; br_if(1);
                    local_get(Local(byte_buf)); local_get(Local(counter)); i32_add;
                    local_get(Local(list_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                    local_get(Local(counter)); i32_const(Imm32(I64_BYTES)); i32_mul; i32_add;
                    i64_load(0); i32_wrap_i64;
                    i32_store8(0);
                    local_get(Local(counter)); i32_const(Imm32(1)); i32_add; local_set(Local(counter));
                    br(0);
                    end; end;
                });

                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open for writing (O_CREAT|O_TRUNC=9)
                wasm!(self.func, {
                    local_get(Local(resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    i32_const(Imm32(WASI_OFLAGS_CREAT_TRUNC)); i64_const(Imm64(WASI_RIGHTS_FD_WRITE)); i64_const(Imm64(0)); i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(byte_buf)); i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(count)); i32_store(4);
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nwritten_ptr));
                    local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nwritten_ptr));
                    call(self.emitter.rt.fd_write); drop;
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(content_str)); });

                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open: oflags=O_CREAT(1), rights=fd_write(64), fdflags=APPEND(1)
                wasm!(self.func, {
                    local_get(Local(resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    i32_const(Imm32(1));
                    i64_const(Imm64(WASI_RIGHTS_FD_WRITE)); i64_const(Imm64(0));
                    i32_const(Imm32(1));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for appending");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(content_str)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(content_str)); i32_load(0); i32_store(4);
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nwritten_ptr));
                    local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nwritten_ptr));
                    call(self.emitter.rt.fd_write); drop;
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Iterative mkdir_p: create each prefix segment
                let seg_end = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(0)); local_set(Local(seg_end));
                    block_empty; loop_empty;
                    local_get(Local(seg_end)); local_get(Local(path_len)); i32_ge_u; br_if(1);
                    // Advance seg_end past current char
                    local_get(Local(seg_end)); i32_const(Imm32(1)); i32_add; local_set(Local(seg_end));
                    // Skip to next '/' or end of path
                    block_empty; loop_empty;
                    local_get(Local(seg_end)); local_get(Local(path_len)); i32_ge_u; br_if(1);
                    local_get(Local(path_ptr)); local_get(Local(seg_end)); i32_add; i32_load8_u(0);
                    i32_const(Imm32(ASCII_SLASH)); i32_eq; br_if(1);
                    local_get(Local(seg_end)); i32_const(Imm32(1)); i32_add; local_set(Local(seg_end));
                    br(0);
                    end; end;
                    // Try creating directory for path[0..seg_end]
                    local_get(Local(resolved_fd));
                    local_get(Local(path_ptr));
                    local_get(Local(seg_end));
                    call(self.emitter.rt.path_create_directory);
                    drop;
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(seg_end);

                // Final attempt: create the full path and check error
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    local_get(Local(path_ptr));
                    local_get(Local(path_len));
                    call(self.emitter.rt.path_create_directory);
                    local_set(Local(errno));
                });

                // errno==0 or errno==20 (EEXIST) -> ok
                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    local_get(Local(errno)); i32_const(Imm32(WASI_ERRNO_EEXIST)); i32_eq;
                    i32_or;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to create directory");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
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
                    local_set(Local(res));
                    local_get(Local(res)); i32_load(0); local_set(Local(tag));
                    local_get(Local(tag)); i32_eqz;
                    if_i32;
                });
                // ok path: split the string by '\n'
                let text_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(Local(res)); i32_const(Imm32(I32_BYTES)); i32_add; i32_load(0); local_set(Local(text_ptr));
                    local_get(Local(text_ptr));
                    call(self.emitter.rt.string.lines);
                });
                let result_ptr = self.scratch.alloc_i32();
                let list_val = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(Local(list_val));
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); local_get(Local(list_val)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                    local_get(Local(res));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // path_open for directory: dirflags=1(symlink follow), oflags=O_DIRECTORY(2)
                // rights = fd_readdir(0x4000)
                wasm!(self.func, {
                    local_get(Local(resolved_fd)); i32_const(Imm32(1));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    i32_const(Imm32(WASI_OFLAGS_DIRECTORY));
                    i64_const(Imm64(WASI_RIGHTS_FD_READDIR));
                    i64_const(Imm64(0));
                    i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open directory");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                });

                // Allocate readdir buffer (4KB) and bufused output
                wasm!(self.func, {
                    i32_const(Imm32(WASI_READDIR_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(dir_buf));
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(bufused_ptr));
                });

                // fd_readdir(fd, buf, buf_len, cookie=0, bufused_ptr)
                wasm!(self.func, {
                    local_get(Local(opened_fd));
                    local_get(Local(dir_buf));
                    i32_const(Imm32(WASI_READDIR_BUF_BYTES));
                    i64_const(Imm64(0));
                    local_get(Local(bufused_ptr));
                    call(self.emitter.rt.fd_readdir);
                    drop;
                    local_get(Local(bufused_ptr)); i32_load(0); local_set(Local(bufused));
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                });

                // First pass: count entries (skipping "." and "..")
                // WASI dirent: d_next(8) + d_ino(8) + d_namlen(4) + d_type(4) = 24 bytes header
                let skip = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(0)); local_set(Local(offset));
                    i32_const(Imm32(0)); local_set(Local(list_count));
                    block_empty; loop_empty;
                    local_get(Local(offset)); i32_const(Imm32(WASI_DIRENT_HEADER_BYTES)); i32_add;
                    local_get(Local(bufused)); i32_gt_u; br_if(1);
                    local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAMLEN)); i32_add;
                    i32_load(0); local_set(Local(entry_name_len));

                    // skip = (namlen==1 && name[0]=='.') || (namlen==2 && name[0]=='.' && name[1]=='.')
                    i32_const(Imm32(0)); local_set(Local(skip));
                    // Check "."
                    local_get(Local(entry_name_len)); i32_const(Imm32(1)); i32_eq;
                    if_empty;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      if_empty; i32_const(Imm32(1)); local_set(Local(skip)); end;
                    end;
                    // Check ".."
                    local_get(Local(entry_name_len)); i32_const(Imm32(DOTDOT_NAME_LEN)); i32_eq;
                    if_empty;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME1)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      i32_and;
                      if_empty; i32_const(Imm32(1)); local_set(Local(skip)); end;
                    end;
                    // Count if not skipped
                    local_get(Local(skip)); i32_eqz;
                    if_empty;
                      local_get(Local(list_count)); i32_const(Imm32(1)); i32_add; local_set(Local(list_count));
                    end;

                    // Advance offset
                    local_get(Local(offset)); i32_const(Imm32(WASI_DIRENT_HEADER_BYTES)); i32_add; local_get(Local(entry_name_len)); i32_add;
                    local_set(Local(offset));
                    br(0);
                    end; end;
                });

                // Allocate List[String]: [len:i32][cap:i32][ptr:i32 * count]
                wasm!(self.func, {
                    local_get(Local(list_count)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32)); i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(list_ptr));
                    local_get(Local(list_ptr)); local_get(Local(list_count)); i32_store(0);
                });

                // Second pass: build string entries (same skip logic as counting pass)
                let copy_i = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(0)); local_set(Local(offset));
                    i32_const(Imm32(0)); local_set(Local(counter));
                    block_empty; loop_empty;
                    local_get(Local(offset)); i32_const(Imm32(WASI_DIRENT_HEADER_BYTES)); i32_add;
                    local_get(Local(bufused)); i32_gt_u; br_if(1);
                    local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAMLEN)); i32_add;
                    i32_load(0); local_set(Local(entry_name_len));

                    // skip = (namlen==1 && name[0]=='.') || (namlen==2 && name[0]=='.' && name[1]=='.')
                    i32_const(Imm32(0)); local_set(Local(skip));
                    local_get(Local(entry_name_len)); i32_const(Imm32(1)); i32_eq;
                    if_empty;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      if_empty; i32_const(Imm32(1)); local_set(Local(skip)); end;
                    end;
                    local_get(Local(entry_name_len)); i32_const(Imm32(DOTDOT_NAME_LEN)); i32_eq;
                    if_empty;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME1)); i32_add;
                      i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
                      i32_and;
                      if_empty; i32_const(Imm32(1)); local_set(Local(skip)); end;
                    end;
                    // Build entry if not skipped
                    local_get(Local(skip)); i32_eqz;
                    if_empty;
                });
                self.emit_fs_list_dir_build_entry(copy_i, entry_name_len, str_ptr, dir_buf, offset, list_ptr, counter);
                wasm!(self.func, {
                    end;

                    // Advance offset
                    local_get(Local(offset)); i32_const(Imm32(WASI_DIRENT_HEADER_BYTES)); i32_add; local_get(Local(entry_name_len)); i32_add;
                    local_set(Local(offset));
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(copy_i);

                // Update list count
                wasm!(self.func, {
                    local_get(Local(list_ptr)); local_get(Local(counter)); i32_store(0);
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); local_get(Local(list_ptr)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    // flags=0: do NOT follow symlinks
                    local_get(Local(resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    local_get(Local(stat_buf));
                    call(self.emitter.rt.path_filestat_get);
                    local_set(Local(errno));
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                      i32_const(Imm32(0));
                    else_;
                      local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_FILETYPE)); i32_add; i32_load8_u(0);
                      i32_const(Imm32(WASI_FILETYPE_SYMLINK)); i32_eq;
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
                wasm!(self.func, { local_set(Local(src_str)); });
                self.emit_fs_resolve_path(src_str, src_ptr, src_len, src_resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(dst_str)); });
                self.emit_fs_resolve_path(dst_str, dst_ptr, dst_len, dst_resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
                });

                // Open source for reading
                wasm!(self.func, {
                    local_get(Local(src_resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(src_ptr)); local_get(Local(src_len));
                    i32_const(Imm32(0)); i64_const(Imm64(WASI_RIGHTS_FD_READ_SEEK)); i64_const(Imm64(0)); i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open source file");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // Read source content
                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(opened_fd)); local_get(Local(stat_buf));
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i32_load(0); local_set(Local(file_size));
                    local_get(Local(file_size)); call(self.emitter.rt.alloc_pinned); local_set(Local(data_buf));
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr));
                    local_get(Local(iov_ptr)); local_get(Local(data_buf)); i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(file_size)); i32_store(4);
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nrw_ptr));
                    local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nrw_ptr));
                    call(self.emitter.rt.fd_read); drop;
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                    local_get(Local(nrw_ptr)); i32_load(0); local_set(Local(file_size));
                });

                // Open dst for writing
                wasm!(self.func, {
                    local_get(Local(dst_resolved_fd)); i32_const(Imm32(0));
                    local_get(Local(dst_ptr)); local_get(Local(dst_len));
                    i32_const(Imm32(WASI_OFLAGS_CREAT_TRUNC)); i64_const(Imm64(WASI_RIGHTS_FD_WRITE)); i64_const(Imm64(0)); i32_const(Imm32(0));
                    local_get(Local(fd_out_ptr));
                    call(self.emitter.rt.path_open);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
                    if_i32;
                });
                let err_msg2 = self.emitter.intern_string("failed to open destination file");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg2 as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // Write data to dst
                wasm!(self.func, {
                    local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
                    local_get(Local(iov_ptr)); local_get(Local(data_buf)); i32_store(0);
                    local_get(Local(iov_ptr)); local_get(Local(file_size)); i32_store(4);
                    local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nrw_ptr));
                    call(self.emitter.rt.fd_write); drop;
                    local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit) -- close nested if blocks
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(src_str)); });
                self.emit_fs_resolve_path(src_str, src_ptr, src_len, src_resolved_fd);

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(dst_str)); });
                self.emit_fs_resolve_path(dst_str, dst_ptr, dst_len, dst_resolved_fd);

                // path_rename(old_fd, old_path, old_len, new_fd, new_path, new_len)
                wasm!(self.func, {
                    local_get(Local(src_resolved_fd));
                    local_get(Local(src_ptr)); local_get(Local(src_len));
                    local_get(Local(dst_resolved_fd));
                    local_get(Local(dst_ptr)); local_get(Local(dst_len));
                    call(self.emitter.rt.path_rename);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to rename");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    call(self.emitter.rt.path_unlink_file);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove file");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                // Try path_unlink_file first
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    call(self.emitter.rt.path_unlink_file);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });

                // Try path_remove_directory
                wasm!(self.func, {
                    local_get(Local(resolved_fd));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    call(self.emitter.rt.path_remove_directory);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove path");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(resolved_fd)); i32_const(Imm32(1));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    local_get(Local(stat_buf));
                    call(self.emitter.rt.path_filestat_get);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                // ok: file size at offset 32 as i64
                // Result[Int, String] = [tag:i32][padding:i32][i64] = 16 bytes
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_INT_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(RESULT_PAIR_BYTES)); i32_add;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i64_load(0);
                    i64_store(0);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_INT_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(RESULT_PAIR_BYTES)); i32_add;
                    i32_const(Imm32(err_msg as i32)); i64_extend_i32_u; i64_store(0);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(resolved_fd)); i32_const(Imm32(1));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    local_get(Local(stat_buf));
                    call(self.emitter.rt.path_filestat_get);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_INT_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(RESULT_PAIR_BYTES)); i32_add;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_MTIM)); i32_add; i64_load(0);
                    i64_const(Imm64(NANOS_PER_SEC)); i64_div_u;
                    i64_store(0);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_INT_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(RESULT_PAIR_BYTES)); i32_add;
                    i32_const(Imm32(err_msg as i32)); i64_extend_i32_u; i64_store(0);
                    local_get(Local(result_ptr));
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
                wasm!(self.func, { local_set(Local(path_str)); });
                self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

                wasm!(self.func, {
                    i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
                    local_get(Local(resolved_fd)); i32_const(Imm32(1));
                    local_get(Local(path_ptr)); local_get(Local(path_len));
                    local_get(Local(stat_buf));
                    call(self.emitter.rt.path_filestat_get);
                    local_set(Local(errno));
                });

                wasm!(self.func, {
                    local_get(Local(errno)); i32_eqz;
                    if_i32;
                });

                // Record: [size:i64(8)][is_dir:i32(4)][is_file:i32(4)][modified:i64(8)] = 24 bytes
                wasm!(self.func, {
                    i32_const(Imm32(STAT_REC_BYTES)); call(self.emitter.rt.alloc); local_set(Local(rec_ptr));
                    // size at stat offset 32
                    local_get(Local(rec_ptr));
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i64_load(0);
                    i64_store(0);
                    // is_dir: filetype at offset 16 == 3
                    local_get(Local(rec_ptr)); i32_const(Imm32(STAT_REC_OFFSET_IS_DIR)); i32_add;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_FILETYPE)); i32_add; i32_load8_u(0);
                    i32_const(Imm32(WASI_FILETYPE_DIRECTORY)); i32_eq;
                    i32_store(0);
                    // is_file: filetype at offset 16 == 4
                    local_get(Local(rec_ptr)); i32_const(Imm32(STAT_REC_OFFSET_IS_FILE)); i32_add;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_FILETYPE)); i32_add; i32_load8_u(0);
                    i32_const(Imm32(WASI_FILETYPE_REGULAR_FILE)); i32_eq;
                    i32_store(0);
                    // modified: mtim at stat offset 40, nanoseconds -> seconds
                    local_get(Local(rec_ptr)); i32_const(Imm32(STAT_REC_OFFSET_MODIFIED)); i32_add;
                    local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_MTIM)); i32_add; i64_load(0);
                    i64_const(Imm64(NANOS_PER_SEC)); i64_div_u;
                    i64_store(0);
                });

                // ok(rec_ptr)
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(result_ptr)); local_get(Local(rec_ptr)); i32_store(4);
                    local_get(Local(result_ptr));
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
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
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
                    local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
                    local_get(Local(result_ptr));
                });
                self.scratch.free_i32(result_ptr);
            }
            "temp_dir" => {
                // fs.temp_dir() -> String: return "/tmp"
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(Imm32(s as i32)); });
            }
            "read_bytes_raw" => {
                // fs.read_bytes_raw(path: String) -> Result[Bytes, String].
                // WASI implementation would mirror `read_bytes` but emit a
                // `Bytes` (i.e. `[len:i32][u8...]`) instead of a List[Int].
                // Until that lands, emit a runtime `err("fs.read_bytes_raw
                // not supported on WASM yet")` so compilation succeeds and
                // any caller that exercises the path at runtime surfaces
                // the limitation clearly rather than compile-time panic.
                // Drop the path arg.
                self.emit_expr(&args[0]);
                wasm!(self.func, { drop; });
                let msg = self.emitter.intern_string("fs.read_bytes_raw not supported on WASM yet");
                let msg_str = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                wasm!(self.func, {
                    // Err payload: String pointer (already interned).
                    i32_const(Imm32(msg as i32)); local_set(Local(msg_str));
                    // Result[Bytes, String] layout: [tag:i32=1 for err, payload:i32]
                    i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result));
                    local_get(Local(result)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(result)); local_get(Local(msg_str)); i32_store(4);
                    local_get(Local(result));
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(msg_str);
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
            local_get(Local(path_str)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; // raw path bytes (skip string length prefix)
            local_get(Local(path_str)); i32_load(0);            // path byte length
            call(self.emitter.rt.resolve_path);
            local_set(Local(resolve_result));
            // Unpack result
            local_get(Local(resolve_result)); i32_load(0); local_set(Local(resolved_fd));
            local_get(Local(resolve_result)); i32_load(4); local_set(Local(path_ptr));
            local_get(Local(resolve_result)); i32_load(8); local_set(Local(path_len));
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
        wasm!(self.func, { local_set(Local(path_str)); });
        self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

        wasm!(self.func, {
            i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
            // flags=1 (follow symlinks) for is_dir/is_file
            local_get(Local(resolved_fd)); i32_const(Imm32(1));
            local_get(Local(path_ptr)); local_get(Local(path_len));
            local_get(Local(stat_buf));
            call(self.emitter.rt.path_filestat_get);
            local_set(Local(errno));
            local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
            if_i32;
              i32_const(Imm32(0));
            else_;
              // filetype at stat offset 16
              local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_FILETYPE)); i32_add; i32_load8_u(0);
              i32_const(Imm32(expected_filetype));
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
            local_get(Local(entry_name_len)); i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32)); i32_add;
            call(self.emitter.rt.alloc); local_set(Local(str_ptr));
            local_get(Local(str_ptr)); local_get(Local(entry_name_len)); i32_store(0);
            // Copy name bytes
            i32_const(Imm32(0)); local_set(Local(copy_i));
            block_empty; loop_empty;
            local_get(Local(copy_i)); local_get(Local(entry_name_len)); i32_ge_u; br_if(1);
            local_get(Local(str_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; local_get(Local(copy_i)); i32_add;
            local_get(Local(dir_buf)); local_get(Local(offset)); i32_add; i32_const(Imm32(WASI_DIRENT_OFFSET_NAME)); i32_add;
            local_get(Local(copy_i)); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(Local(copy_i)); i32_const(Imm32(1)); i32_add; local_set(Local(copy_i));
            br(0);
            end; end;
            // Store in list
            local_get(Local(list_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
            local_get(Local(counter)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
            local_get(Local(str_ptr)); i32_store(0);
            local_get(Local(counter)); i32_const(Imm32(1)); i32_add; local_set(Local(counter));
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
        wasm!(self.func, { local_set(Local(path_str)); });
        self.emit_fs_resolve_path(path_str, path_ptr, path_len, resolved_fd);

        wasm!(self.func, {
            i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(fd_out_ptr));
        });

        wasm!(self.func, {
            local_get(Local(resolved_fd)); i32_const(Imm32(0));
            local_get(Local(path_ptr)); local_get(Local(path_len));
            i32_const(Imm32(0)); i64_const(Imm64(WASI_RIGHTS_FD_READ_SEEK)); i64_const(Imm64(0)); i32_const(Imm32(0));
            local_get(Local(fd_out_ptr));
            call(self.emitter.rt.path_open);
            local_set(Local(errno));
        });

        wasm!(self.func, {
            local_get(Local(errno)); i32_const(Imm32(0)); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("file not found");
        wasm!(self.func, {
            i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
            local_get(Local(result_ptr)); i32_const(Imm32(1)); i32_store(0);
            local_get(Local(result_ptr)); i32_const(Imm32(err_msg as i32)); i32_store(4);
            local_get(Local(result_ptr));
            else_;
        });

        wasm!(self.func, {
            local_get(Local(fd_out_ptr)); i32_load(0); local_set(Local(opened_fd));
            i32_const(Imm32(WASI_FILESTAT_BUF_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(stat_buf));
            local_get(Local(opened_fd)); local_get(Local(stat_buf));
            call(self.emitter.rt.fd_filestat_get); drop;
            local_get(Local(stat_buf)); i32_const(Imm32(WASI_FILESTAT_OFFSET_SIZE)); i32_add; i32_load(0); local_set(Local(file_size));
            local_get(Local(file_size)); call(self.emitter.rt.alloc_pinned); local_set(Local(data_buf));
            i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(iov_ptr)); // iov struct [buf_ptr:i32, buf_len:i32]
            local_get(Local(iov_ptr)); local_get(Local(data_buf)); i32_store(0);
            local_get(Local(iov_ptr)); local_get(Local(file_size)); i32_store(4);
            i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc_pinned); local_set(Local(nread_ptr));
            local_get(Local(opened_fd)); local_get(Local(iov_ptr)); i32_const(Imm32(1)); local_get(Local(nread_ptr));
            call(self.emitter.rt.fd_read); drop;
            local_get(Local(opened_fd)); call(self.emitter.rt.fd_close); drop;
            local_get(Local(nread_ptr)); i32_load(0); local_set(Local(file_size));
        });

        wasm!(self.func, {
            local_get(Local(file_size)); i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32)); i32_add;
            call(self.emitter.rt.alloc); local_set(Local(str_ptr));
            local_get(Local(str_ptr)); local_get(Local(file_size)); i32_store(0);
        });

        let counter = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(Imm32(0)); local_set(Local(counter));
            block_empty; loop_empty;
            local_get(Local(counter)); local_get(Local(file_size)); i32_ge_u; br_if(1);
            local_get(Local(str_ptr)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32)); i32_add; local_get(Local(counter)); i32_add;
            local_get(Local(data_buf)); local_get(Local(counter)); i32_add;
            i32_load8_u(0);
            i32_store8(0);
            local_get(Local(counter)); i32_const(Imm32(1)); i32_add; local_set(Local(counter));
            br(0);
            end; end;
        });
        self.scratch.free_i32(counter);

        wasm!(self.func, {
            i32_const(Imm32(RESULT_PAIR_BYTES)); call(self.emitter.rt.alloc); local_set(Local(result_ptr));
            local_get(Local(result_ptr)); i32_const(Imm32(0)); i32_store(0);
            local_get(Local(result_ptr)); local_get(Local(str_ptr)); i32_store(4);
            local_get(Local(result_ptr));
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
