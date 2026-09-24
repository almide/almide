//! Function-VALUE work shared across one program's lowering: funcref-
//! table entries, per-program emitted helpers, call_indirect type
//! interning, lifted lambdas. Split from lib.rs for the complexity
//! budget.

use std::collections::HashMap;

use wasm_encoder::ValType;

use crate::*;

/// Which type-directed body a [`Helper::NamedOp`] carries. The two walk
/// the same fields in the same order and differ only in what the i32 they
/// leave MEANS — a 0/1 verdict for `Eq`, a signed three-way verdict for
/// `Cmp` — which is why one builder emits both (#2172).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum NamedOp {
    Eq,
    Cmp,
}

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
    /// `$vjson_pretty(cursor, v, depth) -> cursor` — the two-space
    /// indented form (json.stringify_pretty), same leaves as JsonValue.
    JsonValuePretty { float_to_string: u32, frags: JsonFrags, pfrags: PrettyFrags },
    /// `$vfield(value, key) -> i32`: 0 = not an Object, 1 = missing key,
    /// else the found Value's address (real addresses never collide with
    /// the sentinels — the heap starts past the null guard).
    ValueField,
    /// `$vkeys(value) -> i32`: the object's keys as a List[String]
    /// (addresses shared — strings are immutable).
    ValueKeys,
    /// `$value_eq(a, b) -> i32` — deep structural Value equality
    /// (recursive): tags must match, Float is IEEE ==, Array/Object
    /// compare element/pair-wise IN ORDER (the oracle's value_eq).
    ValueEq { key_off: u32, val_off: u32 },
    /// `$value_merge(a, b) -> i32` — object merge: A's keys in order
    /// (B's value wins on a shared key), then B's new keys in B order;
    /// any non-Object operand yields b (the oracle's value_merge).
    ValueMerge { key_off: u32, val_off: u32 },
    /// `$utf8_lossy(bytes) -> str` — String::from_utf8_lossy verbatim:
    /// Table 3-7 well-formed ranges, one U+FFFD per MAXIMAL invalid
    /// subpart (the WHATWG replacement walk).
    Utf8Lossy,
    /// `$fast_exp(f64) -> f64` — the canonical unfused fast-exp (#1197).
    FastExp,
    /// `$named_<op>_<ti>(a, b) -> i32` — the runtime-recursive body of a
    /// type-directed walk over a RECURSIVE Named type (the DisplayNamed
    /// doctrine for `==` and for the total order). Both ops have the same
    /// `(a, b) -> i32` signature and the same reason to exist: the emitter
    /// inlines the type's shape, so a type that contains itself has to
    /// become a CALL somewhere or the emitter recurses forever.
    NamedOp { op: NamedOp, ti: u32 },
    /// `$jp_set(j, path, k, nv) -> Value` — json.set_path's recursive
    /// core over THIS backend's Value layout.
    JsonPathSet { vdec: u32 },
    /// `$jp_remove(j, path, k) -> Value` — json.remove_path's core.
    JsonPathRemove,
    /// `$scan_deep_<key>(block, stride, off, needle) -> i32` — the scan
    /// family's DEEP lane: compound keys (tuples, records) compare by
    /// the type-directed `==` instead of a word/byte class.
    ScanDeep { key: crate::ETy },
    /// `$q10_val(data, off, k) -> f64` — one Q1_0 weight (global-k).
    Q10Val,
    /// `$gelu(f64) -> f64` — tanh-approximation gelu over `$fast_exp`.
    GeluScalar { fast_exp: u32 },
    /// `$bytes_to_string(bytes) -> i32` — std::str::from_utf8 verbatim:
    /// ok(shared block) or err(the Utf8Error Display line, "invalid
    /// UTF-8: " prefixed — the native wrapper's format).
    BytesToString { inv_pre: u32, inv_mid: u32, inc_pre: u32 },
    /// `$map_reserve(block, esz) -> block` — the map twin of
    /// `$list_push`'s growth discipline (#1219 stage 1): room for one
    /// more `esz`-byte entry in place when the size class has slack,
    /// else the doubled block with the entries copied and the outgrown
    /// block freed iff rc == 1. `len` is left for the caller to bump
    /// after it stores the pair at the old end.
    MapReserve,
    /// `$scan_f64(block, stride, off, needle) -> i32` — the float lane of
    /// the scan family (native PartialEq: -0.0 == 0.0, NaN never matches;
    /// a Helper, not a fixed function, because its f64 param breaks the
    /// fixed-index signature set).
    ScanF64,
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
    /// The keyed-lookup index family (#1219 stage 2, map_index.rs): the
    /// address-keyed side table (`get` / `raw` / `set`), the per-class
    /// key hash, the index builder, the `$scan_*`-shaped `find` and the
    /// in-place window's `append` maintenance hook.
    MapIdxSideGet,
    MapIdxSideRaw,
    MapIdxSideSet { raw: u32 },
    MapIdxHash { key: crate::map_index::IdxKey },
    MapIdxBuild { key: crate::map_index::IdxKey, hash: u32 },
    MapIdxFind { key: crate::map_index::IdxKey, fns: crate::map_index::IdxFns, build: u32 },
    MapIdxAppend { key: crate::map_index::IdxKey, fns: crate::map_index::IdxFns },
    /// `$drop_list(block)` — the typed drop of a List whose elements are
    /// heap HANDLES (#2010 stage 2b): the spine's credit down; at zero
    /// every element released through `elem_dec` (`$dec_flat` for a
    /// Str / Bytes element, the inner list's own drop glue for a nested
    /// one), then the spine freed. One helper per element drop fn.
    DropList { elem_dec: u32 },
    /// `$inc_elems(block)`: +1 on every element handle of a spine whose
    /// slots were COPIED from another spine — the copy holds its own
    /// credits, so its typed drop releases exactly what it acquired.
    IncElems,
    /// `$copy_elems(block) -> block`: `$block_copy` plus the element
    /// credits of the copy.
    CopyElems { inc_elems: u32 },
    /// `$cow_elems(block) -> block`: `$cow` plus the element credits of
    /// the copy it made (none when the block was uniquely held).
    CowElems { inc_elems: u32 },
    /// `$drop_<shape>(block)` — the typed drop of an Option / Result /
    /// tuple block with handle payloads (#2010 stage 2c): the block's
    /// credit down; at zero each handle slot released through its own
    /// dec fn (a Result by its tag), then the block freed. The body is
    /// built by the emitter at registration (`drop_bodies`) — the slot
    /// table needs the type table.
    DropShape { ty: SliceTy },
    /// `$inc_<shape>(block)`: +1 on every handle slot of an Option /
    /// Result / tuple / record / variant block — the credits a whole-block
    /// COPY of it must hold (`CopyElems { inc_elems }` calls it).
    IncShape { ty: SliceTy },
    /// `$drop_map(block)` — the drop of a Map / Set whose entries hold NO
    /// heap handle (flat keys and values): the block's credit down; at
    /// zero its index side-table entry is cleared (`side_clear` =
    /// `$mapidx_side_set`, so a reused address inherits no stale index)
    /// and the entries array freed. Handle entries take `DropEntries`.
    DropMapSpine { side_clear: u32 },
    /// `$drop_entries(block)` — the typed drop of a Map / Set whose
    /// entries hold heap HANDLES (#2010, Map stage b): the block's credit
    /// down; at zero every entry's handle slots (`slots` = up to two
    /// `(offset, dec fn)` pairs — a Map's key and value, a Set's member)
    /// released, the index side-table entry cleared, the entries array
    /// freed. One helper per entry layout.
    DropEntries { stride: u32, slots: [Option<(u32, u32)>; 2], side_clear: u32 },
    /// `$inc_entries(block, nbytes)`: +1 on every handle slot of the
    /// entries in the first `nbytes` payload bytes — the credits a copied
    /// entries array must hold (`nbytes` is explicit so an append copy
    /// can walk the copied prefix and leave its fresh tail entry alone).
    IncEntries { stride: u32, slots: [Option<u32>; 2] },
    /// `$copy_entries(block) -> block`: `$block_copy` plus the entry
    /// credits of the whole copy.
    CopyEntries { inc_entries: u32 },
    /// `$drop_env_<layout>(block)` — the drop of ONE closure env layout
    /// (#2010 closures, ruling B): the block's credit down; at zero every
    /// handle capture released through its own dec fn (`slots` =
    /// `(payload offset, dec fn)`), then the block freed. Its funcref-table
    /// slot is what the env stores at `ENV_DROP_OFF`; one helper per slot
    /// table, so lambdas with the same capture layout share it.
    DropEnv { slots: Vec<(u32, u32)> },
    /// `$drop_fn(block)` — the release of a Fn VALUE: a pool-static block
    /// (a named fn, a capture-free lambda) is immortal; any other env
    /// `call_indirect`s the drop fn its own payload names (type `ti` =
    /// `(i32) -> ()`, the one fixed drop signature).
    DropFn { ti: u32 },
    /// `$drop_cell(block)` — the release of a C-319 shared cell (#2010):
    /// the cell's credit down; at zero its occupant released through
    /// `elem_dec` (none for a flat occupant), then the cell freed.
    DropCell { elem_dec: Option<u32> },
    /// #2312 shape 1 — the ROOM-FREE appends of a bounded build
    /// (`runtime_line::BoundedBuild`): the same writes as `$append_copy` /
    /// `$append_i64` / `$append_bool`, without the room check and so
    /// without the `$line_grow` edge. Only a build whose written extent is
    /// statically inside the fixed room may call them.
    /// `$append_raw(cur, src, len) -> cur`.
    AppendRaw,
    /// `$append_i64_raw(cur, v: i64) -> cur`.
    AppendI64Raw,
    /// `$append_bool_raw(cur, b) -> cur` over the interned `"true"` /
    /// `"false"` pool blocks.
    AppendBoolRaw { true_base: u32, false_base: u32 },
    /// `$print_i64(v: i64)`: a line that is ONE Int's display — itoa into
    /// the scratch, hand `[ITOA_END - len, ITOA_END)` to the stream import
    /// (`import` = println / eprintln). No block, no build.
    PrintI64 { import: u32 },
}

/// The pretty printer's extra pooled fragments.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PrettyFrags {
    pub(crate) nl: u32,
    pub(crate) colon_sp: u32,
    pub(crate) comma_nl: u32,
    pub(crate) indent2: u32,
    pub(crate) empty_arr: u32,
    pub(crate) empty_obj: u32,
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
    /// Every host op number an `almide.fs_call` site in this emission
    /// spelled (#1710/#1423): build.rs audits the set against the stock
    /// p1 shim's served ops BEFORE shipping — an unserved op refuses at
    /// build time instead of the runtime refusal (the env.set lesson:
    /// a reverse-handover artifact refused on stock where native ran).
    pub(crate) host_ops: std::cell::RefCell<std::collections::BTreeSet<i32>>,
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
    /// #2312 shape 1: this pass may emit the bounded-line rewrites
    /// (line_bounded.rs — room-free outermost builds, the one-Int line,
    /// the `${int.to_string(e)}` fold). Off = the checked emission.
    pub(crate) bounded_lines: std::cell::Cell<bool>,
    /// Set when a bounded-line rewrite was actually emitted: only then is
    /// there a second emission to compare against.
    pub(crate) bounded_fired: std::cell::Cell<bool>,
    /// DisplayNamed helper bodies, built in the display-helper phase
    /// right after the fn that first registered them (per-fn refusal
    /// granularity survives: a failing body refuses THAT fn, later
    /// callers see Failed and refuse themselves, and assembly stubs the
    /// promised index with `unreachable`).
    pub(crate) display_bodies: std::cell::RefCell<HashMap<u32, DisplayBuild>>,
    /// `Helper::NamedOp` bodies, keyed by `(op, ti)` — ONE map for both
    /// ops, so neither the build loop nor assembly can learn about one and
    /// forget the other (#2172).
    pub(crate) named_bodies: std::cell::RefCell<HashMap<(NamedOp, u32), DisplayBuild>>,
    pub(crate) scan_bodies: std::cell::RefCell<HashMap<crate::ETy, DisplayBuild>>,
    /// `Helper::DropShape` bodies, built by `dec_fn_of` when the helper
    /// is first registered (assembly takes them by type).
    pub(crate) drop_bodies: std::cell::RefCell<Vec<(Helper, Option<wasm_encoder::Function>)>>,
    /// Region-pure fns by table index (#1961) — the vocabulary the
    /// `consume(produce(scalars))` window recogniser consults.
    pub(crate) region_pure: crate::region::RegionPure,
    /// Set once any region window was emitted (exports `__heap_high`).
    pub(crate) region_used: std::cell::Cell<bool>,
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

    /// A REAL closure lift: no module of its own, hop-charged. The #1627
    /// synthetic module initializers are the one other construction site
    /// (crate::func::emit_modinit_call) and spell their fields directly.
    pub(crate) fn register_closure_lambda(
        &self,
        params: Vec<(almide_ir::VarId, crate::SliceTy)>,
        ret: Option<crate::SliceTy>,
        effect_raw: Option<crate::SliceTy>,
        body: almide_ir::IrExpr,
        captures: Vec<(almide_ir::VarId, crate::SliceTy, u32, bool)>,
        var_space: u32,
    ) -> u32 {
        self.register_lambda(crate::LiftedLambda {
            params,
            ret,
            effect_raw,
            body,
            captures,
            var_space,
            cur_module: None,
            charge_hop: true,
        })
    }
}

