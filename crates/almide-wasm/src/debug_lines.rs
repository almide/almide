//! #1315: the `.almd` line each emitted instruction came from, and the DWARF
//! line table a debug build ships for it (`dwarf.rs`).
//!
//! Off by default. The switch is a thread-local scoped by
//! [`DebugLinesGuard`] (the `host_exports` discipline): without it nothing is
//! recorded and the emitted bytes do not change — the size ratchet and every
//! default artifact are untouched.
//!
//! Recording is a stack over the IR tree. `Emitter::lower` and
//! `Emitter::lower_stmt` bracket the bytes a node emits: entering a node with
//! a span makes its line current at the body's byte length so far, and
//! leaving it makes the ENCLOSING node's line current again, so the `call`
//! a parent emits after its arguments is the parent's line, not the last
//! argument's. A synthesized node (`span: None`) inherits the nearest
//! enclosing span; [`Loc::own`] says which of the two it was.
//!
//! The record outlives the post-passes by instruction ORDINAL, not byte
//! offset: the self-tail-call loop conversion (`tco.rs`) reports where each
//! of its instructions came from, the import declaration (`imports.rs`) and
//! the stock-WASI transform re-encode operator for operator (the latter
//! inserting a `proc_exit` call before each `unreachable`), and byte offsets
//! are recomputed only against the bytes that ship ([`attach_dwarf`]).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use almide_ir::{IrStmt, Span};
use wasm_encoder::Function;

use crate::emitter::Emitter;
use crate::EmitError;

#[path = "dwarf.rs"]
mod dwarf;
pub use dwarf::{attach_dwarf, DwarfIn, DwarfStats};

/// One instruction's source position (1-based line and column).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
    /// The innermost IR node emitting the instruction carries this span
    /// itself; `false` means it was inherited from an enclosing node.
    pub own: bool,
}

/// The per-instruction positions of one emitted function.
#[derive(Clone, Debug)]
pub struct FnLines {
    /// The function's absolute index in the emitted module.
    pub index: u32,
    pub name: String,
    /// Whose source: 0 is the entry file, `i + 1` is `LineTable::units[i + 1]`.
    pub space: u32,
    /// One entry per instruction ordinal (the closing `end` included).
    pub ops: Vec<Option<Loc>>,
}

/// What one emission recorded.
#[derive(Clone, Debug, Default)]
pub struct LineTable {
    /// Var space → module name (`""` for the entry program).
    pub units: Vec<String>,
    pub fns: Vec<FnLines>,
    /// Module name → source path, as the route noted them
    /// ([`note_unit_file`]); a module absent here is a bundled stdlib one.
    pub files: BTreeMap<String, String>,
}

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
    static TABLE: RefCell<Option<LineTable>> = const { RefCell::new(None) };
    static FILES: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

/// Whether this thread's emission records lines.
pub fn on() -> bool {
    ON.with(Cell::get)
}

/// Record lines for a scope; the previous state comes back on drop.
#[must_use = "the guard restores the previous state when dropped; binding it to `_` restores immediately"]
pub struct DebugLinesGuard(bool);

impl DebugLinesGuard {
    pub fn set() -> Self {
        let prev = on();
        ON.with(|c| c.set(true));
        TABLE.with(|t| t.borrow_mut().take());
        FILES.with(|f| f.borrow_mut().clear());
        Self(prev)
    }
}

impl Drop for DebugLinesGuard {
    fn drop(&mut self) {
        ON.with(|c| c.set(self.0));
    }
}

/// The source path of a linked module (the route knows it; the IR does not).
pub fn note_unit_file(module: &str, path: &str) {
    if on() {
        FILES.with(|f| f.borrow_mut().insert(module.to_string(), path.to_string()));
    }
}

/// The table of the last emission shipped under the guard.
pub fn take_table() -> Option<LineTable> {
    let mut t = TABLE.with(|t| t.borrow_mut().take())?;
    t.files = FILES.with(|f| f.borrow().clone());
    Some(t)
}

pub(crate) fn publish(fns: &[FnLines], ir: &almide_ir::IrProgram) {
    if !on() {
        return;
    }
    let mut units = vec![String::new()];
    units.extend(ir.modules.iter().map(|m| m.name.as_str().to_string()));
    TABLE.with(|t| *t.borrow_mut() = Some(LineTable { units, fns: fns.to_vec(), files: BTreeMap::new() }));
}

/// The byte-offset record of one function body being lowered.
#[derive(Default)]
pub(crate) struct Frame {
    stack: Vec<Option<(u32, u32)>>,
    /// (body byte length, the position current from there on)
    rows: Vec<(u32, Option<Loc>)>,
}

impl Frame {
    fn current(&self) -> Option<Loc> {
        let own = matches!(self.stack.last(), Some(Some(_)));
        self.stack.iter().rev().find_map(|s| *s).map(|(line, col)| Loc { line, col, own })
    }
}

/// Run one body's lowering with a frame open (nothing when off).
pub(crate) fn framed<T>(lower: impl FnOnce() -> T) -> (T, Option<Frame>) {
    if !on() {
        return (lower(), None);
    }
    let prev = FRAME.with(|c| c.replace(Some(Frame::default())));
    let out = lower();
    (out, FRAME.with(|c| c.replace(prev)))
}

fn with_frame(f: impl FnOnce(&mut Frame)) {
    if on() {
        FRAME.with(|c| {
            if let Some(fr) = c.borrow_mut().as_mut() {
                f(fr)
            }
        });
    }
}

pub(crate) fn enter(off: usize, span: Option<Span>) {
    with_frame(|fr| {
        fr.stack.push(span.filter(|s| s.line > 0).map(|s| (s.line as u32, s.col as u32)));
        let at = fr.current();
        fr.rows.push((off as u32, at));
    });
}

pub(crate) fn leave(off: usize) {
    with_frame(|fr| {
        fr.stack.pop();
        let at = fr.current();
        fr.rows.push((off as u32, at));
    });
}

impl Emitter<'_> {
    /// A statement's bytes are its own line (the `local.set` after a bind's
    /// value included).
    pub(crate) fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        enter(self.f.byte_len(), s.span);
        let r = self.lower_stmt_kind(s);
        leave(self.f.byte_len());
        r
    }
}

/// Per-instruction positions of a finished body, from its frame's byte rows.
fn per_op(f: &Function, frame: Frame) -> Option<Vec<Option<Loc>>> {
    let raw = f.clone().into_raw_body();
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&raw, 0));
    let mut ops = body.get_operators_reader().ok()?;
    let (mut out, mut k, mut cur) = (Vec::new(), 0usize, None);
    while !ops.eof() {
        let pos = ops.original_position() as usize;
        while k < frame.rows.len() && frame.rows[k].0 as usize <= pos {
            cur = frame.rows[k].1;
            k += 1;
        }
        ops.read().ok()?;
        out.push(cur);
    }
    Some(out)
}

/// Which lowered body a record belongs to, before indices are known.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Body {
    Program(usize),
    Main,
    Lambda(usize),
}

/// One emission pass's records (empty when off).
#[derive(Default)]
pub(crate) struct PassLines {
    bodies: HashMap<Body, (String, u32, Vec<Option<Loc>>)>,
}

/// Where each recorded body ships in the emitted module.
pub(crate) struct Placement<'a> {
    /// A program fn's pre-declaration index, `None` when it ships as a stub.
    pub(crate) program: &'a dyn Fn(usize) -> Option<u32>,
    pub(crate) main_index: u32,
    /// (lifted lambda, its table-entry function index)
    pub(crate) lambdas: Vec<(usize, u32)>,
    /// The module before `imports::declare`, and the stubs it declared.
    pub(crate) pre_declare: &'a [u8],
    pub(crate) stubs: Vec<u32>,
}

impl PassLines {
    pub(crate) fn record(&mut self, b: Body, name: String, space: u32, f: &Function, frame: Option<Frame>) {
        if let Some(ops) = frame.and_then(|fr| per_op(f, fr)) {
            self.bodies.insert(b, (name, space, ops));
        }
    }

    /// The tail-call loop conversion rewrote program fn `i`: `origin[j]` is
    /// the old ordinal new instruction `j` came from.
    pub(crate) fn remap(&mut self, b: Body, origin: &[u32]) {
        if let Some((_, _, ops)) = self.bodies.get_mut(&b) {
            *ops = origin.iter().map(|&o| ops.get(o as usize).copied().flatten()).collect();
        }
    }

    pub(crate) fn finish(self, at: Placement<'_>) -> Vec<FnLines> {
        if self.bodies.is_empty() {
            return Vec::new();
        }
        let base_imports = func_imports(at.pre_declare);
        let mut stubs = at.stubs;
        stubs.sort_unstable();
        let mut out: Vec<FnLines> = Vec::new();
        for (b, (name, space, ops)) in self.bodies {
            let pre: Vec<u32> = match b {
                Body::Program(i) => (at.program)(i).into_iter().collect(),
                Body::Main => vec![at.main_index],
                Body::Lambda(k) => at.lambdas.iter().filter(|(j, _)| *j == k).map(|(_, idx)| *idx).collect(),
            };
            for idx in pre {
                let index = if stubs.is_empty() { idx } else { crate::imports::remap_index(idx, &stubs, base_imports) };
                out.push(FnLines { index, name: name.clone(), space, ops: ops.clone() });
            }
        }
        out.sort_by_key(|f| f.index);
        out
    }
}

fn func_imports(bytes: &[u8]) -> u32 {
    let mut n = 0;
    for p in wasmparser::Parser::new(0).parse_all(bytes).flatten() {
        if let wasmparser::Payload::ImportSection(r) = p {
            n += r.into_imports().flatten().filter(|i| matches!(i.ty, wasmparser::TypeRef::Func(_))).count() as u32;
        }
    }
    n
}
