//! Function-VALUE work shared across one program's lowering: funcref-
//! table entries, per-program emitted helpers, call_indirect type
//! interning, lifted lambdas. Split from lib.rs for the complexity
//! budget.

use std::collections::HashMap;

use wasm_encoder::ValType;

use crate::*;

/// A per-program emitted helper (assembled right after `main`, BEFORE the
/// table-entry extras — call sites need these indices DURING lowering).
#[derive(Clone, PartialEq)]
pub(crate) enum Helper {
    /// `$vjson(cursor, value) -> cursor` — the JSON serializer core
    /// (recursive; floats through the LINKED float.to_string minus any
    /// trailing ".0" — the incumbent's `{}` form).
    JsonValue { float_to_string: u32, frags: JsonFrags },
    /// `$vjson_quote(cursor, str) -> cursor` — the 5-escape quoted form.
    JsonQuote { frags: JsonFrags },
    /// `$vfield(value, key) -> i32`: 0 = not an Object, 1 = missing key,
    /// else the found Value's address (real addresses never collide with
    /// the sentinels — the heap starts past the null guard).
    ValueField,
    /// `$vkeys(value) -> i32`: the object's keys as a List[String]
    /// (addresses shared — strings are immutable).
    ValueKeys,
    /// `$split(str, sep) -> i32` — Rust split semantics: byte-level
    /// full-separator match, non-overlapping left-to-right, empty pieces
    /// kept, count = separators + 1. Empty separator traps (Rust's
    /// empty-pattern oddity is out of contract).
    StringSplit,
    /// `$display_<ti>(block, cursor) -> cursor` — the runtime-recursive
    /// display of a RECURSIVE Named type (emit-time inlining follows the
    /// type shape and cycles are cut here; the body is Emitter-built in
    /// the display-helper phase and stored in `display_bodies`).
    DisplayNamed { ti: u32 },
}

/// Pooled fragment addresses the JSON helpers append from.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct JsonFrags {
    pub(crate) null_: u32,
    pub(crate) true_: u32,
    pub(crate) false_: u32,
    pub(crate) esc_backslash: u32,
    pub(crate) esc_quote: u32,
    pub(crate) esc_n: u32,
    pub(crate) esc_r: u32,
    pub(crate) esc_t: u32,
    pub(crate) quote: u32,
    pub(crate) comma: u32,
    pub(crate) colon: u32,
    pub(crate) lbrack: u32,
    pub(crate) rbrack: u32,
    pub(crate) lbrace: u32,
    pub(crate) rbrace: u32,
}

#[derive(Default)]
pub(crate) struct FnWork {
    pub(crate) entries: std::cell::RefCell<Vec<TableEntry>>,
    pub(crate) entry_ids: std::cell::RefCell<HashMap<TableEntry, u32>>,
    pub(crate) itypes: std::cell::RefCell<Vec<WasmSig>>,
    pub(crate) itype_ids: std::cell::RefCell<HashMap<WasmSig, u32>>,
    /// First extra type index (15 fixed + one per table fn).
    pub(crate) itype_base: std::cell::Cell<u32>,
    pub(crate) lifted: std::cell::RefCell<Vec<LiftedLambda>>,
    /// Emitted helpers; function index = helper_base + position.
    pub(crate) helpers: std::cell::RefCell<Vec<Helper>>,
    /// F_FN_BASE + infos.len() + 1 (right after main) — known before
    /// lowering starts, so call sites take helper indices eagerly.
    pub(crate) helper_base: std::cell::Cell<u32>,
    /// DisplayNamed helper bodies, built in the display-helper phase
    /// right after the fn that first registered them (per-fn refusal
    /// granularity survives: a failing body refuses THAT fn, later
    /// callers see Failed and refuse themselves, and assembly stubs the
    /// promised index with `unreachable`).
    pub(crate) display_bodies: std::cell::RefCell<HashMap<u32, DisplayBuild>>,
}

pub(crate) enum DisplayBuild {
    /// The calls set already merged into the BFS roots at build time —
    /// kept out of the variant so the body is the only payload.
    Built(wasm_encoder::Function),
    Failed,
}

impl FnWork {
    /// The +1-biased funcref-table slot for an entry.
    pub(crate) fn slot(&self, e: TableEntry) -> u32 {
        if let Some(&i) = self.entry_ids.borrow().get(&e) {
            return i + 1;
        }
        let mut v = self.entries.borrow_mut();
        let i = v.len() as u32;
        v.push(e.clone());
        self.entry_ids.borrow_mut().insert(e, i);
        i + 1
    }

    /// The wasm type index for a call_indirect signature.
    pub(crate) fn itype(&self, params: Vec<ValType>, ret: Option<ValType>) -> u32 {
        let key = (params, ret);
        if let Some(&i) = self.itype_ids.borrow().get(&key) {
            return self.itype_base.get() + i;
        }
        let mut v = self.itypes.borrow_mut();
        let i = v.len() as u32;
        v.push(key.clone());
        self.itype_ids.borrow_mut().insert(key, i);
        self.itype_base.get() + i
    }

    /// The function index of a helper, registering it on first use.
    pub(crate) fn helper(&self, h: Helper) -> u32 {
        let mut v = self.helpers.borrow_mut();
        if let Some(pos) = v.iter().position(|x| *x == h) {
            return self.helper_base.get() + pos as u32;
        }
        let pos = v.len() as u32;
        v.push(h);
        self.helper_base.get() + pos
    }

    pub(crate) fn register_lambda(&self, ll: LiftedLambda) -> u32 {
        let mut v = self.lifted.borrow_mut();
        let i = v.len() as u32;
        v.push(ll);
        i
    }
}

