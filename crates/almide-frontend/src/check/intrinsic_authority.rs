//! E085: the runtime boundary used outside the stdlib — an `@intrinsic` /
//! `@wasm_intrinsic` declaration (#2152 item 3), or a reference to the `prim`
//! module (#3025).
//!
//! An `@intrinsic("almide_rt_fs_write")` declaration binds a fn name to a
//! runtime symbol directly. The capability story (`--profile critical`,
//! `--allow IO`) is told in terms of stdlib MODULES — a user file that binds
//! the runtime symbol itself never imports `fs`, so nothing in the profile
//! sees the write. `attr_vocab.rs` listed the attribute as an ordinary word
//! and `scripts/check-intrinsic-boundary.sh` proved that intrinsics LINK, not
//! that they are ALLOWED. This is the authority gate: the attribute is legal
//! only in the stdlib's own sources.
//!
//! `prim` is the same boundary one level down: the primitive floor (raw loads
//! and stores, allocators, `fd_write`, the host file calls) the self-hosted
//! stdlib is written over. It sat in the always-accessible set, so
//! `prim.read_text_file(p)` type-checked in a user file, reached the host
//! without an `import fs`, and was lowered by MIR arms nothing else drove.
//! The ruling on #3025: `prim` is not user surface. The hint names the public
//! function that wraps the floor op (`prim_wrappers` derives it from the
//! stdlib sources).
//!
//! The judgement is by ORIGIN, never by module name: a user module named
//! `list` is still a user module. A source is the stdlib when the module
//! driver says it is a bundled module (compiled into the binary, no path), or
//! when it is a file in a directory named `stdlib` whose text is exactly the
//! embedded stdlib source of its name — so `stdlib/fs_read_text.almd`
//! checked on its own is the stdlib, `spec/stdlib/fs_test.almd` is not, and
//! neither is a package that copies the layout with its own text (#4100). When the checker has no
//! origin at all (no source path and not inside a bundled module — the LSP's
//! in-memory analysis) it abstains, as E084 does, rather than guess.
use super::{Checker, err};
use crate::ast::{Attribute, Decl, Span};
use almide_base::intern::Sym;

impl Checker {
    /// `Some(true)` when the source under inference is the stdlib's own,
    /// `Some(false)` when it is not, `None` when its origin is unknown.
    fn stdlib_authority(&self) -> Option<bool> {
        if self.in_bundled_module {
            return Some(true);
        }
        let file = self.source_file.as_deref()?;
        let raw = std::path::Path::new(file);
        let path = std::fs::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf());
        let in_stdlib_dir = path
            .parent()
            .and_then(|d| d.file_name())
            .is_some_and(|d| d == "stdlib");
        // #4100: identity, not layout. A file is the stdlib's only when its
        // text IS the embedded source of that name (the compiler's own
        // copy); a package laid out as `stdlib/string.almd` with its own
        // `@intrinsic`s is not. The text compared is the one under check.
        let embedded_identity = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(almide_lang::embedded::source_of)
            .is_some_and(|embedded| match self.source_text.as_deref() {
                Some(text) => text == embedded,
                None => std::fs::read_to_string(&path).is_ok_and(|t| t == embedded),
            });
        Some(in_stdlib_dir && embedded_identity)
    }

    pub(super) fn reject_user_intrinsic(&mut self, fn_name: &str, attrs: &[Attribute], decl_span: Option<Span>) {
        let Some(attr) = attrs
            .iter()
            .find(|a| matches!(a.name.as_str(), "intrinsic" | "wasm_intrinsic"))
        else {
            return;
        };
        if self.stdlib_authority() != Some(false) {
            return;
        }
        let mut diagnostic = err(
            format!("@{} on fn '{}' outside the stdlib", attr.name, fn_name),
            "intrinsics live in the stdlib; wrap in an effect fn: call the stdlib module that owns the operation (fs, http, process, ...) from an `effect fn` of your own, so `--profile critical` sees the capability",
            format!("fn {fn_name}"),
        )
        .with_code("E085");
        if let Some(span) = attr.span.or(decl_span) {
            diagnostic.file = self.source_file.clone();
            diagnostic.line = Some(span.line);
            diagnostic.col = Some(span.col);
            diagnostic.end_col = Some(span.end_col);
        }
        self.emit(diagnostic);
    }

    /// E085 for `prim.<field>` — a call, a pipe target or a fn value — written
    /// outside the stdlib. Every site that resolves a `module.field` spelling
    /// calls this with the module identifier, so a binding named `prim` (which
    /// shadows the module) and a user module imported as `prim` are left alone.
    pub(crate) fn reject_user_prim(&mut self, module: &Sym, field: &Sym, span: Option<Span>) {
        if module.as_str() != "prim"
            || self.env.lookup_var("prim").is_some()
            || self.env.import_table.resolve("prim").map(|c| c.as_str() == "prim") != Some(true)
            || self.stdlib_authority() != Some(false)
        {
            return;
        }
        let at = span.map(|s| (s.line, s.col));
        if at.is_some()
            && self.diagnostics.iter().any(|d| {
                d.code == Some("E085") && d.line.zip(d.col) == at
            })
        {
            return;
        }
        let field = field.as_str();
        let wrappers = super::prim_wrappers::public_wrappers(field);
        let hint = match wrappers {
            [] => format!(
                "`prim` is the standard library's primitive floor, not user surface; `prim.{field}` is internal to the standard library and has no public counterpart"
            ),
            [one] => format!(
                "`prim` is the standard library's primitive floor, not user surface; call `{one}` instead"
            ),
            many => format!(
                "`prim` is the standard library's primitive floor, not user surface; call the public function that wraps it instead: {}",
                many.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", ")
            ),
        };
        let mut diagnostic = err(
            format!("`prim.{field}` outside the stdlib"),
            hint,
            format!("call to prim.{field}"),
        )
        .with_code("E085");
        if let Some(sp) = span {
            diagnostic.file = self.source_file.clone();
            diagnostic.line = Some(sp.line);
            diagnostic.col = Some(sp.col);
            if sp.end_col > sp.col {
                diagnostic.end_col = Some(sp.end_col);
            }
        }
        self.emit(diagnostic);
    }

    /// #4008: record where this user file leaves the memory-safety guarantee
    /// (`outside_guarantee`). An `@extern` fn's body is foreign code. A raw
    /// pointer is the one value the `ffi` functions and the deprecated
    /// `bytes` raw-pointer spellings take or give, so judging by TYPE finds
    /// every spelling of them (call, method call, alias, fn value, a helper's
    /// parameter). The stdlib's own sources, and a source whose origin is
    /// unknown (the language server), are not recorded.
    pub(super) fn record_outside_guarantee(&mut self, program: &crate::ast::Program) {
        if self.stdlib_authority() != Some(false) {
            return;
        }
        let file = self.source_file.clone().unwrap_or_default();
        let at = |span: Option<Span>| span.map(|s| format!("{file}:{}", s.line)).unwrap_or_else(|| file.clone());
        let mut found = Vec::new();
        for decl in &program.decls {
            if let Decl::Fn { name, extern_attrs, span, .. } = decl
                && !extern_attrs.is_empty()
            {
                found.push(format!("@extern fn `{name}` ({})", at(*span)));
            }
            let (label, body) = match decl {
                Decl::Fn { name, body: Some(b), .. } => (format!("fn `{name}`"), b),
                Decl::TopLet { name, value, .. } => (format!("`{name}`"), value),
                Decl::Test { name, body, .. } => (format!("test \"{name}\""), body),
                _ => continue,
            };
            let mut first: Option<Option<Span>> = None;
            crate::ast::visit_expr(body, &mut |e| {
                if first.is_none()
                    && self.type_map.get(&e.id).is_some_and(|t| {
                        let t = super::resolve_ty(t, &self.uf);
                        t.any_child_recursive(&|c| matches!(c, almide_lang::types::Ty::RawPtr))
                            || matches!(t, almide_lang::types::Ty::RawPtr)
                    })
                {
                    first = Some(e.span);
                }
            });
            if let Some(span) = first {
                found.push(format!("a raw pointer in {label} ({})", at(span)));
            }
        }
        self.outside_guarantee.extend(found);
    }

    /// E085 for `import prim` outside the stdlib: the module is not user
    /// surface, so naming it in an import is refused like naming one of its fns.
    pub(super) fn reject_user_prim_import(&mut self, imports: &[Decl]) {
        for imp in imports {
            let Decl::Import { path, span, .. } = imp else { continue };
            if !(path.len() == 1 && path[0].as_str() == "prim") || self.stdlib_authority() != Some(false) {
                continue;
            }
            let mut diagnostic = err(
                "`import prim` outside the stdlib".to_string(),
                "`prim` is the standard library's primitive floor, not user surface; import the stdlib module that owns the operation (fs, io, bytes, ...) instead",
                "import prim".to_string(),
            )
            .with_code("E085");
            if let Some(sp) = span {
                diagnostic.file = self.source_file.clone();
                diagnostic.line = Some(sp.line);
                diagnostic.col = Some(sp.col);
            }
            self.emit(diagnostic);
        }
    }
}
