// ── dispatch.rs, part 6: the sandboxed vfs prim floors ──
//
// include!-spliced into `dispatch.rs` at module level (the 800-line file
// discipline, #1856). The fs floors a lowered self-hosted body reaches
// (`prim.read_text_file*`, `write_text_file*`, `read_dir*`, `make_dir*`,
// `remove_all*`, the path stats) over the per-interpreter overlay (vfs.rs):
// the argument coercions, the message composition every leg agrees on
// (#2090's `fs.<call>("<operand>"): <errno>` head, #2206's call-head twins)
// and the typed-twin routing of the fold_lines family.

impl<'a> Interpreter<'a> {
    /// The sandboxed fs floor (#1218, vfs.rs): writes land in the
    /// per-interpreter overlay, reads fall back to the real fs
    /// read-only. Same tier as the argv/env floors — these prims
    /// read INTERPRETER state, which the stateless bridge cannot.
    /// A path argument: a `Str`, or a Str-block ADDRESS a body built with
    /// `alloc_str` (coerced through the arena). Anything else is an honest
    /// abstain named by shape — a body reaching a vfs prim with a value the
    /// interp cannot spell must not vote.
    fn vfs_path_arg(&mut self, func: &str, arg: Option<&Value>) -> Result<String, Flow> {
        let Some(v) = arg else {
            return Err(Flow::Unsupported(format!("prim.{func} with no path argument")));
        };
        match self.coerce_block_str(v.clone()) {
            Value::Str(s) => Ok(s.to_string()),
            other => Err(Flow::Unsupported(format!(
                "prim.{func} with a {} path (no faithful String)",
                other.type_name()
            ))),
        }
    }

    /// A content argument as raw bytes: a `Str`'s UTF-8, or the payload of a
    /// Str / Bytes block at the given ADDRESS (the byte-filled `alloc_str`
    /// form need not be UTF-8, so it is read as bytes, never as a String).
    fn vfs_bytes_arg(&mut self, func: &str, arg: Option<&Value>) -> Result<Vec<u8>, Flow> {
        use crate::heap::BlockKind;
        match arg {
            Some(Value::Str(s)) => Ok(s.as_bytes().to_vec()),
            Some(Value::Int(i)) => {
                let block = u32::try_from(*i).ok().and_then(|a| self.heap.block_bytes(a));
                match block {
                    Some((bytes, BlockKind::Str | BlockKind::Bytes)) => Ok(bytes),
                    _ => Err(Flow::Unsupported(format!(
                        "prim.{func} with a non-block Int content (no faithful bytes)"
                    ))),
                }
            }
            Some(other) => Err(Flow::Unsupported(format!(
                "prim.{func} with a {} content (no faithful bytes)",
                other.type_name()
            ))),
            None => Err(Flow::Unsupported(format!("prim.{func} with no content argument"))),
        }
    }

    /// #2090 — the third oracle names the failing call too, or the ledger's
    /// "all three legs agree" claim stops being true the moment the other two
    /// name theirs.
    ///
    /// The prefix goes on HERE, at the dispatch layer, and not in `vfs.rs`: that
    /// file holds ONE spelling of each platform string (its header says so), and
    /// keeping it that way is what made the errno-as-suffix design work. The
    /// dispatcher is the layer that knows which call it is serving.
    fn fs_prim_err(func: &str, operands: &str, e: String) -> String {
        // The PLAIN prims name their own call here; the #2206 `_as` twins carry the
        // call they serve as an argument and go through `fs_floor_err`.
        let call = match func {
            "read_text_file" => "fs.read_text",
            "read_bytes_file" => "fs.read_bytes",
            "write_text_file" => "fs.write",
            "read_dir" => "fs.list_dir",
            "rename" => "fs.rename",
            "make_dir" => "fs.mkdir_p",
            "remove_all" => "fs.remove_all",
            other => other,
        };
        format!("{call}({operands}): {e}")
    }

    /// The typed twin a TOTAL fold_lines-family call runs: `fs.fold_lines` /
    /// `fold_lines_chunked` / `fold_lines_range` are registered only as their
    /// accumulator-typed twins (`fs_fold_lines_i`, …), which the wasm leg picks
    /// from the init argument's STATIC type (`fold_lines_call_name`); here the
    /// init VALUE's shape decides — the same four cells, at the same argument
    /// position. Fixture-tier values only (the caller guards `pool_depth == 0`):
    /// inside the pool tier a String is a raw address and would read as Int.
    fn fold_lines_family_twin(func: Sym, args: &[Value]) -> Option<Sym> {
        let init_idx = match func.as_str() {
            "fold_lines" => 1,
            "fold_lines_chunked" => 2,
            "fold_lines_range" => 3,
            _ => return None,
        };
        let suffix = match args.get(init_idx)? {
            Value::Int(_) => "_i",
            Value::Str(_) => "_s",
            Value::List(_) => "_ls",
            Value::Map(_) => "_msi",
            _ => return None,
        };
        Some(almide_base::intern::sym(&format!("{}{}", func.as_str(), suffix)))
    }

    /// The message of a read/write floor failure (#2206): the plain prim names its
    /// own call over the quoted path; a `_as` twin names the call its `call`
    /// argument spells (`args[head]`); a `_as_pair` twin also names BOTH operands
    /// (`args[head + 1..]`) in message order — `fs.copy(src, dst)` from a floor that
    /// sees one path. The quoting is native's `fs_q`: the path verbatim between
    /// two quotes, no escaping.
    fn fs_floor_err(&mut self, func: &str, head: usize, args: &[Value], path: &str, e: String) -> Result<String, Flow> {
        let q = |s: &str| format!("\"{s}\"");
        if !func.ends_with("_as") && !func.ends_with("_as_pair") {
            return Ok(Self::fs_prim_err(func, &q(path), e));
        }
        let call = self.vfs_path_arg(func, args.get(head))?;
        let operands = if func.ends_with("_as_pair") {
            let first = self.vfs_path_arg(func, args.get(head + 1))?;
            let second = self.vfs_path_arg(func, args.get(head + 2))?;
            format!("{}, {}", q(&first), q(&second))
        } else {
            q(path)
        };
        Ok(format!("{call}({operands}): {e}"))
    }

    fn vfs_prim(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            // The read floor and its #2206 call-head twins: same read, one message
            // composer (`fs_floor_err`), the head at args[1].
            "read_text_file" | "read_text_file_as" | "read_text_file_as_pair" => {
                let path = match self.vfs_path_arg(func, args.first()) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                Some(Flow::val(match crate::vfs::read_text(&self.vfs, &path) {
                    Ok(s) => Value::Result(Ok(Box::new(Value::str(s)))),
                    Err(e) => match self.fs_floor_err(func, 1, args, &path, e) {
                        Ok(m) => Value::Result(Err(Box::new(Value::str(m)))),
                        Err(f) => return Some(f),
                    },
                }))
            }
            // The write floor and its twins: the head at args[2], after the content.
            "write_text_file" | "write_text_file_as" | "write_text_file_as_pair" => {
                let path = match self.vfs_path_arg(func, args.first()) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                // The content is BYTES: a stdlib body that writes bytes fills
                // an `alloc_str` block byte by byte and hands its ADDRESS here
                // (`fs.write_bytes`), and those bytes need not be UTF-8.
                let content = match self.vfs_bytes_arg(func, args.get(1)) {
                    Ok(b) => b,
                    Err(f) => return Some(f),
                };
                Some(Flow::val(match crate::vfs::write_bytes(&mut self.vfs, &path, &content) {
                    Ok(()) => Value::Result(Ok(Box::new(Value::Unit))),
                    Err(e) => match self.fs_floor_err(func, 2, args, &path, e) {
                        Ok(m) => Value::Result(Err(Box::new(Value::str(m)))),
                        Err(f) => return Some(f),
                    },
                }))
            }
            "read_bytes_file" | "read_bytes_file_as" => {
                let path = match self.vfs_path_arg(func, args.first()) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                Some(Flow::val(match crate::vfs::read_bytes(&self.vfs, &path) {
                    Ok(b) => Value::Result(Ok(Box::new(Value::List(std::rc::Rc::new(
                        b.into_iter().map(|x| Value::Int(x as i64)).collect(),
                    ))))),
                    Err(e) => match self.fs_floor_err(func, 1, args, &path, e) {
                        Ok(m) => Value::Result(Err(Box::new(Value::str(m)))),
                        Err(f) => return Some(f),
                    },
                }))
            }
            // `prim.path_filestat(buf, path)`: the WASI filestat lands in the
            // caller's 64-byte scratch — filetype@16, size@32, mtim@48 are the
            // fields the stdlib bodies read — and the errno is the return
            // (0 = ok; the bodies only test it against 0, so a missing path
            // answers WASI's ENOENT 44).
            "path_filestat" | "path_filestat_nofollow" => {
                let Some(Value::Int(buf)) = args.first() else {
                    return Some(Flow::Unsupported(
                        "prim.path_filestat with a non-address buffer".into(),
                    ));
                };
                let path = match self.vfs_path_arg(func, args.get(1)) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                let Ok(base) = u32::try_from(*buf) else {
                    return Some(Flow::Unsupported("prim.path_filestat with a negative buffer".into()));
                };
                let statted = if func == "path_filestat" {
                    crate::vfs::stat(&self.vfs, &path)
                } else {
                    crate::vfs::stat_nofollow(&self.vfs, &path)
                };
                let Some((ftype, size, mtime_ns)) = statted else {
                    return Some(Flow::val(Value::Int(44)));
                };
                let stores = [
                    (16u32, 1u32, ftype as i64),
                    (32, 8, size as i64),
                    (48, 8, mtime_ns),
                ];
                for (off, w, val) in stores {
                    if self.heap.store(base + off, w, val).is_none() {
                        return Some(Flow::Unsupported(
                            "prim.path_filestat outside this heap's arena".into(),
                        ));
                    }
                }
                Some(Flow::val(Value::Int(0)))
            }
            // The directory floors and their #2206 call-head twins (`fs.walk` / `fs.glob` /
            // `fs.remove` reach read_dir, `fs.remove` remove_all, `fs.create_temp_dir` make_dir).
            "read_dir" | "read_dir_as" => {
                let path = match self.vfs_path_arg(func, args.first()) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                Some(Flow::val(match crate::vfs::read_dir(&self.vfs, &path) {
                    Ok(names) => Value::Result(Ok(Box::new(Value::List(std::rc::Rc::new(
                        names.into_iter().map(Value::str).collect(),
                    ))))),
                    Err(e) => match self.fs_floor_err(func, 1, args, &path, e) {
                        Ok(m) => Value::Result(Err(Box::new(Value::str(m)))),
                        Err(f) => return Some(f),
                    },
                }))
            }
            "rename" => {
                let src = match self.vfs_path_arg(func, args.first()) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                let dst = match self.vfs_path_arg(func, args.get(1)) {
                    Ok(p) => p,
                    Err(f) => return Some(f),
                };
                Some(match crate::vfs::rename(&mut self.vfs, &src, &dst) {
                    crate::vfs::RenameOutcome::Renamed => {
                        Flow::val(Value::Result(Ok(Box::new(Value::Unit))))
                    }
                    crate::vfs::RenameOutcome::Failed(e) => {
                        Flow::val(Value::Result(Err(Box::new(Value::str(Self::fs_prim_err(
                            func,
                            &format!("\"{src}\", \"{dst}\""),
                            e,
                        ))))))
                    }
                    crate::vfs::RenameOutcome::HostOnly => Flow::Unsupported(
                        "prim.rename of a host-only path (the overlay is read-only toward the host)"
                            .into(),
                    ),
                })
            }
            "make_dir" | "make_dir_as" => {
                let Some(Value::Str(path)) = args.first() else {
                    return Some(Flow::Abort("internal: prim.make_dir expects a String".into()));
                };
                let path = path.to_string();
                Some(Flow::val(match crate::vfs::make_dir(&mut self.vfs, &path) {
                    Ok(()) => Value::Result(Ok(Box::new(Value::Unit))),
                    Err(e) => match self.fs_floor_err(func, 1, args, &path, e) {
                        Ok(m) => Value::Result(Err(Box::new(Value::str(m)))),
                        Err(f) => return Some(f),
                    },
                }))
            }
            "path_exists" => {
                let Some(Value::Str(path)) = args.first() else {
                    return Some(Flow::Abort("internal: prim.path_exists expects a String".into()));
                };
                Some(Flow::val(Value::Bool(crate::vfs::exists(&self.vfs, path))))
            }
            "remove_all" | "remove_all_as" => {
                let Some(Value::Str(path)) = args.first() else {
                    return Some(Flow::Abort("internal: prim.remove_all expects a String".into()));
                };
                let path = path.to_string();
                Some(match crate::vfs::remove_all(&mut self.vfs, &path) {
                    crate::vfs::RemoveOutcome::Removed => {
                        Flow::val(Value::Result(Ok(Box::new(Value::Unit))))
                    }
                // A host path the overlay never wrote: refusing to
                // delete real files is the sandbox's point, and
                // pretending to would be a wrong vote — abstain.
                    crate::vfs::RemoveOutcome::HostOnly => Flow::Unsupported(
                        "prim.remove_all on a host path (the overlay is read-only toward the real fs)".into(),
                    ),
                    crate::vfs::RemoveOutcome::Missing => {
                        let e = almide_base::fs_errno::ENOENT.text.to_string();
                        match self.fs_floor_err(func, 1, args, &path, e) {
                            Ok(m) => Flow::val(Value::Result(Err(Box::new(Value::str(m))))),
                            Err(f) => f,
                        }
                    }
                })
            }
            _ => None,
        }
    }
}
