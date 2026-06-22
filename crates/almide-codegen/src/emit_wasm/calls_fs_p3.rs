// fs module dispatch — split part of calls_fs.rs (Technique B).
// include!d into calls_fs.rs; relies on its `use` imports + `wasm!` macro.
// Each sub-method matches a DISJOINT subset of `func`; chain order is irrelevant.
impl FuncCompiler<'_> {
    // Group 3 fs ops: is_dir / is_file / is_symlink / copy / rename / remove /
    // remove_all / file_size / modified_at / stat / walk|glob|create_temp_* /
    // temp_dir / read_bytes_raw.
    pub(super) fn emit_fs_call_g3(&mut self, func: &str, args: &[IrExpr]) -> bool {
        match func {
            "is_dir" => {
                // fs.is_dir(path) -> Bool  (filetype 3 = directory)
                self.emit_fs_filetype_check(args, 3);
                true
            }
            "is_file" => {
                // fs.is_file(path) -> Bool  (filetype 4 = regular file)
                self.emit_fs_filetype_check(args, 4);
                true
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                true
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                    local_get(file_size); call(self.emitter.rt.alloc_pinned); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nrw_ptr);
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
                true
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
                true
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
                true
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
                true
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                true
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                true
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
                    i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
                true
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
                true
            }
            "temp_dir" => {
                // fs.temp_dir() -> String: return "/tmp"
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(s as i32); });
                true
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
                    i32_const(msg as i32); local_set(msg_str);
                    // Result[Bytes, String] layout: [tag:i32=1 for err, payload:i32]
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(1); i32_store(0);
                    local_get(result); local_get(msg_str); i32_store(4);
                    local_get(result);
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(msg_str);
                true
            }
            _ => false,
        }
    }
}
