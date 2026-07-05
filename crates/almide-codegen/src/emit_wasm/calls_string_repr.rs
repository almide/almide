//! Recursive Almide-literal repr for compound string interpolation (WASM).
//!
//! `"${[1, 2]}"`, `"${["a": 1]}"`, `"${(1, "x")}"`, … must render a value back
//! to its Almide-literal form, byte-identically with the native target. The walk
//! is driven by the STATIC `Ty` at the interpolation site (WASM carries no
//! runtime type tags), so each contained shape is specialized at emit time and
//! recursion bottoms out at primitives.
//!
//! Contract (mirrors the native `AlmideRepr` impls):
//!   List   → `[a, b, c]`            empty `[]`
//!   Map    → `[k: v, k: v]`         empty `[:]`   (brackets, Swift-style)
//!   Set    → `set.from_list([...])` (no set literal in the language)
//!   Tuple  → `(a, b)`
//!   Option → `some(v)` / `none`
//!   Result → `ok(v)` / `err(e)`
//!   String inside a container → double-quoted + escaped (`__repr_str`)
//!   Int/Bool/Float → same text as bare interpolation (shared rt drivers)
//!
//! Each `emit_repr_value` expects the value on the stack and leaves a string
//! pointer on the stack. Strings are joined with `__concat_str` and interned
//! literal separators (`[`, `, `, `]`, `: `, `some(`, …).

use super::FuncCompiler;
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId;
use super::values;

/// A `Result` cell is `[tag:i32][payload]` — the payload sits one i32 (the tag)
/// in. Mirrors the `ResultOk`/`ResultErr` emit in `expressions.rs`.
const RESULT_PAYLOAD_OFFSET: u32 = 4;

impl FuncCompiler<'_> {
    /// Repr a value of type `ty` already on the stack → string pointer on stack.
    /// Used by `emit_string_part` for compound interpolation parts and by the
    /// recursive container walks below.
    pub(super) fn emit_repr_value(&mut self, ty: &Ty) {
        match ty {
            // ── Primitives: route through the SAME drivers as bare interpolation ──
            Ty::String => {
                wasm!(self.func, { call(self.emitter.rt.repr_str); });
            }
            Ty::Int | Ty::Int64 | Ty::UInt64 => {
                wasm!(self.func, { call(self.emitter.rt.int_to_string); });
            }
            // Sized ints ride in the i32 bucket → widen to i64 for int_to_string.
            Ty::Int8 | Ty::Int16 | Ty::Int32 => {
                wasm!(self.func, { i64_extend_i32_s; call(self.emitter.rt.int_to_string); });
            }
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => {
                wasm!(self.func, { i64_extend_i32_u; call(self.emitter.rt.int_to_string); });
            }
            // A float inside a container uses the SAME Display form as a bare
            // interpolated float — native `AlmideRepr for f64` is `format!("{}",
            // self)`, so an integer-valued float drops its `.0` here too.
            Ty::Float | Ty::Float64 => {
                wasm!(self.func, { call(self.emitter.rt.float_display); });
            }
            Ty::Float32 => {
                wasm!(self.func, { f64_promote_f32; call(self.emitter.rt.float_display); });
            }
            Ty::Bool => {
                let t = self.emitter.intern_string("true");
                let f = self.emitter.intern_string("false");
                wasm!(self.func, {
                    if_i32; i32_const(t as i32); else_; i32_const(f as i32); end;
                });
            }
            // ── Containers ──
            Ty::Applied(TypeConstructorId::List, _) => {
                let elem_ty = self.list_elem_ty(ty);
                self.emit_repr_list(&elem_ty, "[", "]", "[]");
            }
            Ty::Applied(TypeConstructorId::Set, _) => {
                let elem_ty = self.list_elem_ty(ty);
                // No set literal → constructor form `set.from_list([...])`.
                self.emit_repr_list(&elem_ty, "set.from_list([", "])", "set.from_list([])");
            }
            Ty::Applied(TypeConstructorId::Map, _) => {
                self.emit_repr_map(ty);
            }
            Ty::Applied(TypeConstructorId::Option, args) => {
                let inner = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_repr_option(&inner);
            }
            Ty::Applied(TypeConstructorId::Result, args) => {
                let ok_ty = args.first().cloned().unwrap_or(Ty::Int);
                let err_ty = args.get(1).cloned().unwrap_or(Ty::String);
                self.emit_repr_result(&ok_ty, &err_ty);
            }
            Ty::Tuple(elems) => {
                let elems = elems.clone();
                self.emit_repr_tuple(&elems);
            }
            // ── `Value` (dynamic JSON-like) → its JSON text ──
            // Byte-identical to native `AlmideRepr for Value` / `Display`, which
            // both call `almide_rt_value_stringify`; `__value_stringify` mirrors
            // that serializer. Must precede the named record/variant arms below,
            // which would otherwise treat `Value` as an empty record and emit
            // `Value {  }`. The field ptr is on the stack; `__value_stringify`
            // consumes it and leaves the string ptr.
            Ty::Named(n, _) if n.as_str() == "Value" => {
                wasm!(self.func, { call(self.emitter.rt.value_stringify); });
            }
            // ── Recursive named record/variant → per-type repr fn ──
            // A self/mutually-recursive named type must NOT inline-expand its type
            // graph (infinite at compile time); it routes through its reserved
            // `__repr_<Type>(ptr) -> str` function, so recursion is a runtime CALL
            // that bottoms out on the finite value — exactly like native trait
            // dispatch. Only recursive types get a reserved fn (see
            // `register_repr_funcs`); the value pointer is already on the stack.
            Ty::Named(..) if self.emitter.repr_funcs.contains_key(super::mangle_repr_ty(ty).as_str())
                || matches!(ty, Ty::Named(n, _) if self.emitter.repr_funcs.contains_key(n.as_str())) => {
                // Route to the per-INSTANTIATION repr fn (`Tree[Int]` → __repr_Tree_Int)
                // so the payload reprs with the concrete `T`. A non-generic recursive
                // type mangles to its bare name (the `Tree[Int]` key falls back to the
                // bare `Tree` key only if an instantiation fn was never registered —
                // which can't happen for a site that reached here with concrete args).
                let mangled = super::mangle_repr_ty(ty);
                let f = self.emitter.repr_funcs.get(mangled.as_str())
                    .or_else(|| match ty {
                        Ty::Named(n, _) => self.emitter.repr_funcs.get(n.as_str()),
                        _ => None,
                    })
                    .copied()
                    .expect("guard guarantees one of the keys exists");
                wasm!(self.func, { call(f); });
            }
            // ── Non-recursive named variant → inline walk ──
            // Inlining is finite (acyclic) AND keeps the concrete `type_args` at
            // this site, so a generic variant (`Wrapper[Int]`) reprs its payload
            // with the resolved type — a monomorphic per-type fn could not.
            Ty::Named(name, _) if self.emitter.variant_info.contains_key(name.as_str()) => {
                self.emit_repr_variant(ty);
            }
            // ── Non-recursive named record (or opaque newtype) → inline walk ──
            // Same rationale: acyclic, and the site's `type_args` resolve a generic
            // record's field types (`Box[Int]` reprs its `T` value correctly).
            Ty::Named(..) => {
                let fields = self.extract_record_fields(ty);
                let type_name = match ty {
                    Ty::Named(n, _) => Some(n.to_string()),
                    _ => None,
                };
                self.emit_repr_record(type_name.as_deref(), &fields);
            }
            // ── Structural records → inline literal walk ──
            // A record literal inferred WITHOUT an annotation keeps its structural
            // `Ty::Record`, but if its field set matches a DECLARED record type the
            // native checker promotes it to that nominal type — so the repr must
            // adopt the type name + its DECLARATION field order (#627). We recover
            // that name by the sorted field-name set (mirrors the native walker's
            // `named_records` lookup). No match → a truly anonymous record, which
            // native renders prefix-less in SORTED name order (`AlmdRec_*`).
            Ty::Record { .. } | Ty::OpenRecord { .. } => {
                // `fields` is the value's ACTUAL in-memory LAYOUT (the literal's
                // structural field order); field VALUES load from its offsets. If
                // the field-set matches a declared record type, native promotes the
                // repr to that nominal type — `TypeName { … }` rendered in the
                // type's DECLARATION order (#627). The render order is decoupled
                // from the layout, so a literal written out of declaration order
                // still loads correct values. No match → truly anonymous: no
                // prefix, rendered in SORTED name order.
                let fields = self.extract_record_fields(ty);
                let mut sorted_names: Vec<String> =
                    fields.iter().map(|(n, _)| n.to_string()).collect();
                sorted_names.sort();
                if let Some(type_name) = self.emitter.named_records.get(&sorted_names).cloned() {
                    let decl_order: Vec<String> = self.emitter.record_fields
                        .get(&type_name)
                        .map(|fl| fl.iter().map(|(n, _)| n.to_string()).collect())
                        .unwrap_or_default();
                    self.emit_repr_record_ordered(Some(&type_name), &fields, Some(&decl_order));
                } else {
                    self.emit_repr_record(None, &fields);
                }
            }
            // Anything else has no repr (the walker only routes backed shapes
            // here); leave the value as-is.
            _ => {}
        }
    }

    /// List / Set walk: `open` + elem reprs joined by `, ` + `close`.
    /// `empty` is the exact literal for a zero-length collection.
    /// A List and a Set share the `[len:i32][cap:i32][data...]` layout, so one
    /// helper covers both — only the wrapper text differs.
    fn emit_repr_list(&mut self, elem_ty: &Ty, open: &str, close: &str, empty: &str) {
        use super::engine::layout::{LIST, list as ll};
        let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let elem_size = values::byte_size(elem_ty) as i32;
        let open_s = self.emitter.intern_string(open) as i32;
        let close_s = self.emitter.intern_string(close) as i32;
        let empty_s = self.emitter.intern_string(empty) as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;

        let lst = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(lst);
            local_get(lst); i32_load(0); local_set(len);
            // Empty → exact literal (`[]`, `[:]` is map-only, `set.from_list([])`).
            local_get(len); i32_eqz;
            if_i32;
              i32_const(empty_s);
            else_;
              i32_const(open_s); local_set(acc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // separator before every element except the first
                local_get(i); i32_eqz;
                if_empty; else_;
                  local_get(acc); i32_const(sep_s); call(concat); local_set(acc);
                end;
                // acc = acc ++ repr(elem[i])
                local_get(acc);
                // load elem[i] onto stack: addr = lst + data_off + i*elem_size
                local_get(lst); i32_const(data_off); i32_add;
                local_get(i); i32_const(elem_size); i32_mul; i32_add;
            });
            self.emit_load_at(elem_ty, 0);
            self.emit_repr_value(elem_ty);
            wasm!(self.func, {
                call(concat); local_set(acc);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(acc); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(acc);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(lst);
    }

    /// Map walk over the dense, insertion-ordered entries (compact-ordered-dict):
    /// `[k: v, k: v]`, empty `[:]`. Entry stride `es = ks + vs`; key @ +0, val @ +ks.
    fn emit_repr_map(&mut self, map_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let (ks, vs) = self.map_kv_sizes(map_ty);
        let es = (ks + vs) as i32;
        let (key_ty, val_ty) = match map_ty {
            Ty::Applied(_, args) => (
                args.first().cloned().unwrap_or(Ty::Int),
                args.get(1).cloned().unwrap_or(Ty::Int),
            ),
            _ => (Ty::Int, Ty::Int),
        };
        let cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let open_s = self.emitter.intern_string("[") as i32;
        let close_s = self.emitter.intern_string("]") as i32;
        let empty_s = self.emitter.intern_string("[:]") as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;
        let kv_s = self.emitter.intern_string(": ") as i32;

        let m = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let entry = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(m);
            local_get(m); i32_load(0); local_set(len);
            local_get(m); i32_load(cap_off); local_set(cap);
        });
        self.emit_dict_entries_base(m, cap);
        wasm!(self.func, {
            local_set(eb);
            local_get(len); i32_eqz;
            if_i32;
              i32_const(empty_s);
            else_;
              i32_const(open_s); local_set(acc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(i); i32_eqz;
                if_empty; else_;
                  local_get(acc); i32_const(sep_s); call(concat); local_set(acc);
                end;
                // entry = eb + i*es
                local_get(eb); local_get(i); i32_const(es); i32_mul; i32_add;
                local_set(entry);
                // acc ++ repr(key)
                local_get(acc);
                local_get(entry);
        });
        self.emit_load_at(&key_ty, 0);
        self.emit_repr_value(&key_ty);
        wasm!(self.func, {
                call(concat);
                i32_const(kv_s); call(concat); local_set(acc);
                // acc ++ repr(value)   (value at entry + ks)
                local_get(acc);
                local_get(entry); i32_const(ks as i32); i32_add;
        });
        self.emit_load_at(&val_ty, 0);
        self.emit_repr_value(&val_ty);
        wasm!(self.func, {
                call(concat); local_set(acc);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(acc); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(entry);
        self.scratch.free_i32(acc);
        self.scratch.free_i32(i);
        self.scratch.free_i32(eb);
        self.scratch.free_i32(cap);
        self.scratch.free_i32(len);
        self.scratch.free_i32(m);
    }

    /// Tuple walk: `(a, b, …)`. Fields laid out sequentially; offset = sum of
    /// preceding field byte sizes (matches `emit_tuple` / `emit_tuple_index`).
    fn emit_repr_tuple(&mut self, elems: &[Ty]) {
        let open_s = self.emitter.intern_string("(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;

        let tp = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(tp);
            i32_const(open_s); local_set(acc);
        });
        let mut offset = 0u32;
        for (idx, elem_ty) in elems.iter().enumerate() {
            if idx > 0 {
                wasm!(self.func, { local_get(acc); i32_const(sep_s); call(concat); local_set(acc); });
            }
            wasm!(self.func, {
                local_get(acc);
                local_get(tp); i32_const(offset as i32); i32_add;
            });
            self.emit_load_at(elem_ty, 0);
            self.emit_repr_value(elem_ty);
            wasm!(self.func, { call(concat); local_set(acc); });
            offset += values::byte_size(elem_ty);
        }
        wasm!(self.func, { local_get(acc); i32_const(close_s); call(concat); });

        self.scratch.free_i32(acc);
        self.scratch.free_i32(tp);
    }

    /// Option walk: `some(v)` / `none`. WASM repr: null pointer = None, else the
    /// payload is stored at offset 0 of the allocated cell.
    fn emit_repr_option(&mut self, inner_ty: &Ty) {
        let some_s = self.emitter.intern_string("some(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;
        let none_s = self.emitter.intern_string("none") as i32;

        let opt = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(opt);
            local_get(opt); i32_eqz;
            if_i32;
              i32_const(none_s);
            else_;
              i32_const(some_s);
              local_get(opt);
        });
        self.emit_load_at(inner_ty, 0);
        self.emit_repr_value(inner_ty);
        wasm!(self.func, {
              call(concat);
              i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(opt);
    }

    /// Result walk: `ok(v)` / `err(e)`. WASM repr: `[tag:i32][payload]`, tag 0 =
    /// ok, 1 = err; payload at offset 4.
    fn emit_repr_result(&mut self, ok_ty: &Ty, err_ty: &Ty) {
        let ok_s = self.emitter.intern_string("ok(") as i32;
        let err_s = self.emitter.intern_string("err(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;

        let res = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(res);
            // tag == 0 → ok branch
            local_get(res); i32_load(0); i32_eqz;
            if_i32;
              i32_const(ok_s);
              local_get(res);
        });
        self.emit_load_at(ok_ty, RESULT_PAYLOAD_OFFSET);
        self.emit_repr_value(ok_ty);
        wasm!(self.func, {
              call(concat); i32_const(close_s); call(concat);
            else_;
              i32_const(err_s);
              local_get(res);
        });
        self.emit_load_at(err_ty, RESULT_PAYLOAD_OFFSET);
        self.emit_repr_value(err_ty);
        wasm!(self.func, {
              call(concat); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(res);
    }

    /// Record walk: `TypeName { f0: <repr>, f1: <repr> }`, fields at sequential
    /// offsets (record layout `[field0][field1]...`).
    ///
    /// A NAMED record renders its fields in declaration order (the layout order),
    /// `TypeName { f0: …, f1: … }`. An ANONYMOUS record has no `type_name` and
    /// renders just `{ f0: …, … }`; its fields render in SORTED name order so the
    /// output matches the native synthesized `AlmdRec_*` struct, whose field list
    /// is the sorted field-name set (`collect_anon_records` sorts). The VALUE of
    /// each field is still loaded from its real layout offset (source order), so
    /// reordering the render does not misalign reads.
    pub(super) fn emit_repr_record(&mut self, type_name: Option<&str>, fields: &[(String, Ty)]) {
        self.emit_repr_record_ordered(type_name, fields, None);
    }

    /// Like [`emit_repr_record`], but the RENDER order is decoupled from the
    /// in-memory LAYOUT. `fields` is always the value's real layout (offsets are
    /// read from it, source order). `render_order`, when given, lists field names
    /// in the order to print them (a declared type's declaration order, #627) —
    /// independent of how the literal was written. `None` keeps the default:
    /// layout order for a named record, sorted name order for an anonymous one
    /// (matching native's `AlmdRec_*` struct).
    pub(super) fn emit_repr_record_ordered(
        &mut self,
        type_name: Option<&str>,
        fields: &[(String, Ty)],
        render_order: Option<&[String]>,
    ) {
        let open = match type_name {
            Some(n) => format!("{} {{ ", n),
            None => "{ ".to_string(),
        };
        let open_s = self.emitter.intern_string(&open) as i32;
        let close_s = self.emitter.intern_string(" }") as i32;

        // Pair each field with its absolute LAYOUT offset (source order), then
        // choose the render order. Values always load from the true offset, so
        // reordering the render never misaligns reads.
        let mut placed: Vec<(usize, u32)> = Vec::with_capacity(fields.len());
        let mut offset = 0u32;
        for (idx, (_, fty)) in fields.iter().enumerate() {
            placed.push((idx, offset));
            offset += values::byte_size(fty);
        }
        if let Some(order) = render_order {
            placed.sort_by_key(|&(idx, _)| {
                order.iter().position(|n| n == &fields[idx].0).unwrap_or(usize::MAX)
            });
        } else if type_name.is_none() {
            placed.sort_by(|&(a, _), &(b, _)| fields[a].0.cmp(&fields[b].0));
        }

        let rec = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(rec);
            i32_const(open_s); local_set(acc);
        });
        for (render_idx, &(field_idx, field_off)) in placed.iter().enumerate() {
            let (fname, fty) = &fields[field_idx];
            // `, f: ` (the `, ` separator before every field but the first is
            // folded into the label so the whole prefix is one interned string).
            let label = format!("{}{}: ", if render_idx > 0 { ", " } else { "" }, fname);
            let label_s = self.emitter.intern_string(&label) as i32;
            wasm!(self.func, {
                local_get(acc); i32_const(label_s); call(concat); local_set(acc);
                local_get(acc);
                local_get(rec); i32_const(field_off as i32); i32_add;
            });
            self.emit_load_at(fty, 0);
            self.emit_repr_value(fty);
            wasm!(self.func, { call(concat); local_set(acc); });
        }
        wasm!(self.func, { local_get(acc); i32_const(close_s); call(concat); });

        self.scratch.free_i32(acc);
        self.scratch.free_i32(rec);
    }

    /// Variant walk. Layout `[tag:i32][payload...]`; the tag selects the case.
    /// Each case renders in its constructor form, matching the native impl:
    ///   tuple variant  → `Click(<repr>, <repr>)`
    ///   record variant → `Scroll { dy: <repr> }`
    ///   nullary        → `Quit`
    /// Cases are dispatched by a chain of `if tag == k` tests (the final case is
    /// the trailing `else`), so exactly one constructor renders.
    pub(super) fn emit_repr_variant(&mut self, ty: &Ty) {
        let (type_name, type_args) = match ty {
            Ty::Named(n, args) => (n.to_string(), args.clone()),
            _ => return,
        };
        let mut cases = match self.emitter.variant_info.get(type_name.as_str()) {
            Some(c) => c.clone(),
            None => return,
        };
        // Substitute the site's concrete type args into each case's payload types
        // so a generic variant (`Wrapper[Int]`) reprs its payload with the resolved
        // type, not the raw `T`. Generic names are collected from ALL constructors
        // (so `Either[A,B]` indexes A,B consistently), mirroring
        // `extract_record_fields`. With no type args, the cases are unchanged.
        if !type_args.is_empty() {
            // Collect generic-param names as OWNED strings first (they borrow from
            // `cases`, which is mutated below).
            let generic_names: Vec<String> = {
                let mut names: Vec<&str> = Vec::new();
                for case in &cases {
                    for (_, fty) in &case.fields {
                        super::expressions::collect_type_param_names(fty, &mut names);
                    }
                }
                names.into_iter().map(|s| s.to_string()).collect()
            };
            if !generic_names.is_empty() {
                let name_refs: Vec<&str> = generic_names.iter().map(|s| s.as_str()).collect();
                for case in &mut cases {
                    for (_, fty) in &mut case.fields {
                        *fty = super::expressions::substitute_type_params(fty, &name_refs, &type_args);
                    }
                }
            }
        }
        // Payload starts after the tag word — the same offset the constructor
        // and match-destructure use (`variant_tag_offset`, the single source of
        // truth). The tag itself is the leading i32 of the cell.
        let payload_off = self.variant_tag_offset(ty);

        let val = self.scratch.alloc_i32();
        let tag = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(val);
            local_get(val); i32_load(0); local_set(tag); // tag @ cell start
        });

        let n = cases.len();
        for (idx, case) in cases.iter().enumerate() {
            let is_last = idx + 1 == n;
            if !is_last {
                // if tag == case.tag { <case> } else { …next… }
                wasm!(self.func, {
                    local_get(tag); i32_const(case.tag as i32); i32_eq;
                    if_i32;
                });
            }
            self.emit_repr_variant_case(val, payload_off, &case.name, &case.fields);
            if !is_last {
                wasm!(self.func, { else_; });
            }
        }
        // Close one `end` per non-final case (the if/else chain nests N-1 deep).
        for _ in 0..n.saturating_sub(1) {
            wasm!(self.func, { end; });
        }

        self.scratch.free_i32(tag);
        self.scratch.free_i32(val);
    }

    /// Render one variant case onto the stack from the value pointer in `val`.
    /// Tuple-variant fields carry synthetic `_0, _1, …` names (see
    /// `register_type_info`); they render positionally, `Click(a, b)`. Real
    /// field names render as a record-variant, `Scroll { dy: v }`. No fields →
    /// the bare constructor name.
    fn emit_repr_variant_case(&mut self, val: u32, payload_off: u32, ctor: &str, fields: &[(String, Ty)]) {
        if fields.is_empty() {
            let name_s = self.emitter.intern_string(ctor) as i32;
            wasm!(self.func, { i32_const(name_s); });
            return;
        }

        let is_tuple = fields.iter().all(|(n, _)| is_synthetic_index_name(n));
        let (open, close): (String, String) = if is_tuple {
            (format!("{}(", ctor), ")".to_string())
        } else {
            (format!("{} {{ ", ctor), " }".to_string())
        };
        let open_s = self.emitter.intern_string(&open) as i32;
        let close_s = self.emitter.intern_string(&close) as i32;

        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;
        wasm!(self.func, { i32_const(open_s); local_set(acc); });

        let mut offset = payload_off;
        for (idx, (fname, fty)) in fields.iter().enumerate() {
            // Tuple payload: `, ` separators only. Record payload: `, f: `.
            let label = if is_tuple {
                if idx > 0 { ", ".to_string() } else { String::new() }
            } else {
                format!("{}{}: ", if idx > 0 { ", " } else { "" }, fname)
            };
            if !label.is_empty() {
                let label_s = self.emitter.intern_string(&label) as i32;
                wasm!(self.func, { local_get(acc); i32_const(label_s); call(concat); local_set(acc); });
            }
            wasm!(self.func, {
                local_get(acc);
                local_get(val); i32_const(offset as i32); i32_add;
            });
            self.emit_load_at(fty, 0);
            self.emit_repr_value(fty);
            wasm!(self.func, { call(concat); local_set(acc); });
            offset += values::byte_size(fty);
        }
        wasm!(self.func, { local_get(acc); i32_const(close_s); call(concat); });
        self.scratch.free_i32(acc);
    }
}

/// A synthetic tuple-variant field name `_0`, `_1`, … (assigned in
/// `register_type_info`). Such names mark a tuple-payload variant, rendered
/// positionally; any other name marks a record-payload variant.
fn is_synthetic_index_name(name: &str) -> bool {
    name.strip_prefix('_').map_or(false, |rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}
