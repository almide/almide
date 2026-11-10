//! Native test entry identities, kept separate from their libtest names.
//! The checked bodies still pass through the ordinary native MIR pipeline.

use almide_ir::{IrFunction, IrProgram};
use almide_lang::intern::sym;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ENTRIES: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
    static RAW_PANIC_BODY: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn is_entry(name: &str) -> bool {
    ENTRIES.with(|s| s.borrow().contains(name))
}

pub(crate) fn active() -> bool { ACTIVE.with(Cell::get) }

pub(crate) struct EntryScope(BTreeSet<String>, Option<String>, bool);
impl Drop for EntryScope {
    fn drop(&mut self) {
        ENTRIES.with(|s| s.replace(std::mem::take(&mut self.0)));
        RAW_PANIC_BODY.with(|s| s.replace(self.1.take()));
        ACTIVE.with(|s| s.set(self.2));
    }
}

pub(crate) fn register_raw_panic(name: String) {
    RAW_PANIC_BODY.with(|s| s.replace(Some(name)));
}

pub(crate) fn raw_panic(name: &str) -> bool {
    RAW_PANIC_BODY.with(|s| s.borrow().as_deref() == Some(name))
}

struct Entry {
    origin: Option<String>,
    name: String,
    body: String,
    abort: bool,
    span: Option<almide_ir::Span>,
}

pub(crate) struct Plan {
    entries: Vec<Entry>,
}

impl Plan {
    /// Rename test bodies before linking, so punctuation in display names is
    /// never mistaken for a callable MIR symbol. Preserve the established
    /// libtest names independently, including non-ASCII name hashes.
    pub(crate) fn prepare(ir: &mut IrProgram) -> Self {
        let mut plan = Self { entries: Vec::new() };
        let mut ordinal = 0;
        plan.functions(&mut ir.functions, None, &mut ordinal);
        for module in &mut ir.modules {
            let identity = module.versioned_name.unwrap_or(module.name);
            let origin = almide_base::names::module_ident(identity.as_str());
            plan.functions(&mut module.functions, Some(origin), &mut ordinal);
        }
        plan
    }

    fn functions(&mut self, functions: &mut [IrFunction], origin: Option<String>, ordinal: &mut usize) {
        let has_tests = functions.iter().any(|f| f.is_test);
        let mut names: BTreeSet<_> = functions.iter().map(|f| f.name.to_string()).collect();
        for function in functions.iter_mut().filter(|f| f.is_test) {
            let display = almide_base::names::rust_safe_fn_name(function.name.as_str());
            let internal = loop {
                let candidate = format!("__almd_native_test_body_{}", *ordinal);
                *ordinal += 1;
                if names.insert(candidate.clone()) { break candidate; }
            };
            let qualify = |name: &str| match &origin {
                Some(module) => format!("almide_rt_{module}_{name}"),
                None => name.to_owned(),
            };
            self.entries.push(Entry {
                origin: origin.clone(), name: qualify(&display), body: qualify(&internal), abort: false,
                span: function.body.span,
            });
            function.name = sym(&internal);
        }
        if has_tests && functions.iter().any(|f| f.name.as_str() == almide_ir::ABORT_DISPATCH_FN) {
            let prefix = origin.as_ref().map(|m| format!("almide_rt_{m}_")).unwrap_or_default();
            self.entries.push(Entry {
                origin, name: format!("{prefix}__almd_abort_site_entry"),
                body: format!("{prefix}{}", almide_ir::ABORT_DISPATCH_FN), abort: true,
                span: functions.iter().find(|f| f.name.as_str() == almide_ir::ABORT_DISPATCH_FN)
                    .and_then(|f| f.body.span),
            });
        }
    }

    pub(crate) fn enter(&self) -> EntryScope {
        EntryScope(
            ENTRIES.with(|s| s.replace(self.entries.iter().map(|e| e.body.clone()).collect())),
            RAW_PANIC_BODY.with(|s| s.replace(None)),
            ACTIVE.with(|s| s.replace(true)),
        )
    }

    pub(crate) fn promote(&self, ir: &mut IrProgram) -> Result<(), crate::lower::LowerError> {
        for entry in &self.entries {
            let f = ir.functions.iter_mut().find(|f| f.name.as_str() == entry.body)
                .ok_or_else(|| crate::lower::LowerError::at(entry.span, format!("native test entry was lost during linking: {}", entry.name)))?;
            f.is_test = false;
        }
        Ok(())
    }

    pub(crate) fn render(&self, functions: &[crate::MirFunction]) -> Result<String, crate::lower::LowerError> {
        let templates = almide_codegen::template::rust_templates();
        let init = if functions.iter().any(|f| f.name == "__mg_init") { "almd___mg_init();\n" } else { "" };
        let mut items = Vec::new();
        for entry in &self.entries {
            if !functions.iter().any(|f| f.name == entry.body) {
                return Err(crate::lower::LowerError::at(entry.span, format!("native test entry was lost during MIR linking: {}", entry.name)));
            }
            let body = crate::render_native::mangle(&entry.body);
            let rendered = if entry.abort {
                almide_codegen::test_harness::abort_entry_with(&templates, &body, &entry.name,
                    init, "__almd_native_test_abort", "__almd_native_test_flush")
            } else {
                let code = format!("    {init}    __AlmdNativeTestOutcome::finish({body}());");
                templates.render_with("test_block", None, &[], &[("name", entry.name.as_str()), ("body", code.as_str())])
            }.ok_or_else(|| crate::lower::LowerError::at(entry.span, "native test harness template is missing"))?;
            items.push((entry.origin.clone(), rendered));
        }
        let fan = crate::render_native::fan_rendered();
        let mut out = OUTCOME
            .replace("__ALMD_TEST_TRAP_WAIT__", if fan { "almide_fan_trap_wait();" } else { "" })
            .replace("__ALMD_TEST_ABORT_PROBE__", ABORT_PROBE);
        out.push_str(&almide_codegen::test_harness::render_module(&templates, items));
        Ok(if fan {
            crate::render_native::route_exits(&out)
        } else { out })
    }
}

/// The switch the EMITTED harness reads in the test binary (the parent of an
/// `assert_aborts` child sets it); the compiler itself never reads it, so the
/// name is spliced into the emitted text and not spelled as a read here.
const ABORT_PROBE: &str = "ALMIDE_TEST_ABORT_PROBE";

const OUTCOME: &str = r#"
fn __almd_native_test_flush() {
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
fn __almd_native_test_abort(e: impl std::fmt::Display) -> ! {
    __ALMD_TEST_TRAP_WAIT__
    eprintln!("Error: {}", e);
    __almd_native_test_flush();
    std::process::exit(1);
}
fn __almd_native_test_panic(e: impl std::fmt::Display) -> ! {
    if std::env::var_os("__ALMD_TEST_ABORT_PROBE__").is_none() { panic!("{}", e); }
    __ALMD_TEST_TRAP_WAIT__
    __almd_native_test_flush();
    let _ = std::io::Write::write_fmt(&mut std::io::stderr().lock(), format_args!("PANIC: {}", e));
    std::process::exit(1);
}
trait __AlmdNativeTestOutcome { fn finish(self); }
impl __AlmdNativeTestOutcome for () { fn finish(self) {} }
// MIR's lifted Unit ABI can carry a scalar placeholder. Only Err determines
// the test verdict; the checked test declaration discards every Ok payload.
impl<T, E: std::fmt::Display> __AlmdNativeTestOutcome for Result<T, E> {
    fn finish(self) { if let Err(e) = self { panic!("{}", e); } }
}
"#;
