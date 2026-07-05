// fs module dispatch — split part of calls_fs.rs (Technique B).
// include!d into calls_fs.rs; relies on its `use` imports + `wasm!` macro.
// Each sub-method matches a DISJOINT subset of `func`; chain order is irrelevant.
impl FuncCompiler<'_> {
    // Group 2 fs ops: write_bytes / append / mkdir_p / read_lines / list_dir.
    pub(super) fn emit_fs_call_g2(&mut self, func: &str, args: &[IrExpr]) -> bool {
        match func {
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
                    local_get(count); call(self.emitter.rt.alloc_pinned); local_set(byte_buf);
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(count); i32_ge_u; br_if(1);
                    local_get(byte_buf); local_get(counter); i32_add;
                    local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(counter); i32_const(8); i32_mul; i32_add;
                    i64_load(0); i32_wrap_i64;
                    i32_store8(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); local_get(count); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nwritten_ptr);
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
                true
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nwritten_ptr);
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
                true
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
                true
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
                true
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
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
                    i32_const(4096); call(self.emitter.rt.alloc_pinned); local_set(dir_buf);
                    i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(bufused_ptr);
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

                // Allocate List[String]: [len:i32][cap:i32][ptr:i32 * count]
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
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
                true
            }
            _ => false,
        }
    }
}
