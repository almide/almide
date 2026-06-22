// fs module helpers — split part of calls_fs.rs (Technique A, moved verbatim).
// include!d into calls_fs.rs; relies on its `use` imports + `wasm!` macro.
impl FuncCompiler<'_> {
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
            local_get(path_str); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; // raw path bytes (skip string length prefix)
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
            i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
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
            local_get(entry_name_len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(entry_name_len); i32_store(0);
            // Copy name bytes
            i32_const(0); local_set(copy_i);
            block_empty; loop_empty;
            local_get(copy_i); local_get(entry_name_len); i32_ge_u; br_if(1);
            local_get(str_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(copy_i); i32_add;
            local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
            local_get(copy_i); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
            br(0);
            end; end;
            // Store in list
            local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
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
            i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(fd_out_ptr);
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
            i32_const(64); call(self.emitter.rt.alloc_pinned); local_set(stat_buf);
            local_get(opened_fd); local_get(stat_buf);
            call(self.emitter.rt.fd_filestat_get); drop;
            local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
            local_get(file_size); call(self.emitter.rt.alloc_pinned); local_set(data_buf);
            i32_const(8); call(self.emitter.rt.alloc_pinned); local_set(iov_ptr); // iov struct [buf_ptr:i32, buf_len:i32]
            local_get(iov_ptr); local_get(data_buf); i32_store(0);
            local_get(iov_ptr); local_get(file_size); i32_store(4);
            i32_const(4); call(self.emitter.rt.alloc_pinned); local_set(nread_ptr);
            local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
            call(self.emitter.rt.fd_read); drop;
            local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
            local_get(nread_ptr); i32_load(0); local_set(file_size);
        });

        wasm!(self.func, {
            local_get(file_size); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(file_size); i32_store(0);
        });

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
