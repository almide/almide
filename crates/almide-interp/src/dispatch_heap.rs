// ── dispatch.rs, part 4: the block-heap prim floor ──
//
// include!-spliced into `dispatch.rs` at module level (#1856). The `prim.*`
// heap family (`heap_prim` and its alloc / load / store / slot-io tiers), the
// hinted materialisers, the block-string/list coercions, and the free helper
// fns the heap tier and the return sync share (`heap_addr`, the slot-shape
// predicates, `ty_short`).

impl<'a> Interpreter<'a> {
    /// The block-heap prim floor (#1226 slice 1). `None` = "not mine", which
    /// falls through to the argv/env/fs arms and ultimately to the honest
    /// abstain, so an unmodelled prim keeps its `Flow::Unsupported` rather than
    /// getting a guessed value.
    ///
    /// Split one fn per family, the way `bridge.rs` splits its scalar floor
    /// into `prim_bitwise_fn` / `prim_repr_fn` / …: a miss in one falls through
    /// to the next and ends as the same `None` an unmatched name would give, so
    /// the chain is equivalent to one flat table with the arms in this order.
    fn heap_prim(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let out = self
            .heap_prim_handle(func, args)
            .or_else(|| self.heap_prim_alloc(func, args))
            .or_else(|| self.heap_prim_load(func, args))
            .or_else(|| self.heap_prim_store(func, args))
            .or_else(|| self.heap_prim_slot_io(func, args));
        if std::env::var("ALMIDE_HEAP_TRACE").is_ok_and(|v| v == "1") {
            if let Some(f) = &out {
                let shown = match f {
                    Flow::Value(v) => format!("{v:?}"),
                    Flow::Unsupported(m) => format!("UNSUPPORTED: {m}"),
                    _ => "<flow>".to_string(),
                };
                eprintln!("[heap] prim.{func}({args:?}) -> {shown}");
            }
        }
        out
    }

    /// `prim.handle(v)` — the base address of v's block. A value that has not
    /// been in the arena is materialized ONCE and bound, so the `+ 4` (len) and
    /// `+ 12` (payload) reads in one body agree on the same block.
    fn heap_prim_handle(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        if func != "handle" {
            return None;
        }
        let hint = self.handle_arg_ty.take();
        let Some(v) = args.first() else {
            return Some(Flow::Unsupported("prim.handle with no argument".into()));
        };
        Some(match self.heap_materialize_hinted(v, hint.as_ref()) {
            Ok(a) => Flow::val(Value::Int(a)),
            Err(why) => Flow::Unsupported(why),
        })
    }

    /// A value as its arena address — the read direction, generalized in
    /// slice 2 to the SCALAR container family. `Err` carries the abstain
    /// reason.
    ///
    /// Scalar-only is a correctness line, not laziness: the pool tier resolves
    /// a PUBLIC container fn (`map.get_or`, `list.…`) by NAME to its scalar
    /// core impl — the type-directed rewrite that picks `_skv`/`_str`/`_hval`
    /// variants is a MIR-lowering pass the interp never runs. A heap-keyed Map
    /// materialized in ANY layout would therefore be walked by a body that
    /// compares key slots as raw i64s, and same-content strings in different
    /// blocks would miss — a wrong vote (`tm_map_int_lit_print` printed -8 for
    /// 12 in exactly this shape). Scalar containers are the ones the core
    /// impls are CORRECT for; everything else abstains with its shape named.
    /// [`Self::heap_materialize`] with the call site's STATIC argument type.
    ///
    /// The hint does two jobs. It settles representation questions the value
    /// alone cannot — `Bytes` spells 1 payload byte per element where
    /// `List[Int]` spells an 8-byte slot, and a CHILD list's byte-vs-slot
    /// choice needs the element type, not inspection. And it is the
    /// IMPL-CORRECTNESS guard for heap elements: the hint IS the resolved
    /// body's own declared parameter type, so when the untyped pool resolves
    /// a public name to its scalar core impl and a heap-element container
    /// arrives, the declaration says `List[Int]`, the value says Strings,
    /// and the mismatch abstains — while a body genuinely declared over
    /// `List[(String, Value)]` (value_object) materializes exactly what it
    /// claims. Without a concrete hint (a generic `handle[A]`), inspection
    /// decides and stays scalar-only, as before.
    fn heap_materialize_hinted(&mut self, v: &Value, hint: Option<&Ty>) -> Result<i64, String> {
        use crate::heap::rc_key;
        use almide_lang::types::constructor::TypeConstructorId as C;
        match (v, hint) {
            // A declared Bytes is STRICT: a non-byte element under it has no
            // faithful byte, and falling back to slots would hand a
            // byte-reading body 8x-strided memory.
            (Value::List(rc), Some(Ty::Bytes)) => {
                let bytes: Result<Vec<u8>, String> = rc
                    .iter()
                    .map(|v| match v {
                        Value::Int(i) if (0..=255).contains(i) => Ok(*i as u8),
                        other => Err(format!(
                            "prim.handle of a Bytes-typed list holding a non-byte {}",
                            other.type_name()
                        )),
                    })
                    .collect();
                let a = self.heap.bind(rc_key(rc), &bytes?, crate::heap::BlockKind::Bytes);
                self.heap.keep(Rc::clone(rc));
                Ok(a as i64)
            }
            (Value::List(rc), Some(Ty::Applied(C::List | C::Set, ts))) if ts.len() == 1 => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> =
                    rc.iter().map(|e| self.heap_slot_hinted(e, &ts[0])).collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            (Value::Set(rc), Some(Ty::Applied(C::Set | C::List, ts))) if ts.len() == 1 => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> =
                    rc.iter().map(|e| self.heap_slot_hinted(e, &ts[0])).collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // The paired scalar-map layout under its declared key/value types
            // — same strictness as the sequences. SCALAR declarations only:
            // a heap-keyed map spells the skv/interleaved layouts, not this
            // one, and those stay out of this slice.
            (Value::Map(rc), Some(Ty::Applied(C::Map, ts)))
                if ts.len() == 2
                    && !heap_slot_is_child(&ts[0])
                    && !heap_slot_is_child(&ts[1]) =>
            {
                let rc = Rc::clone(rc);
                let entries = rc.len() as u32;
                let mut slots = Vec::with_capacity(2 * rc.len());
                for (k, v) in rc.iter() {
                    slots.push(self.heap_slot_hinted(k, &ts[0])?);
                    slots.push(self.heap_slot_hinted(v, &ts[1])?);
                }
                let a = self.heap.bind_slots(rc_key(&rc), &slots, entries);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // A tuple is one more slot block: one slot per element, `len` =
            // the element count (value_object reads `(String, Value)` pairs
            // as `load64(tup+12)` / `load64(tup+20)`). Unlocked by the
            // `Value::Dyn` carrier (increment 4): the decode chains this
            // opens now hand fixture-level `==` and repr typed carriers,
            // not bare addresses.
            (Value::Tuple(rc), Some(Ty::Tuple(ts))) if ts.len() == rc.len() => {
                let rc = Rc::clone(rc);
                let slots: Result<Vec<i64>, String> = rc
                    .iter()
                    .zip(ts)
                    .map(|(e, t)| self.heap_slot_hinted(e, t))
                    .collect();
                let a = self.heap.bind_slots(rc_key(&rc), &slots?, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            _ => self.heap_materialize(v),
        }
    }

    /// One container element as its slot i64, driven by the DECLARED element
    /// type: scalars inline (NaN still abstains — #1403), heap elements as
    /// recursively-materialized children under their own hint.
    ///
    /// Scalars are STRICT against the declaration, and that strictness is the
    /// wrong-impl detector: a Float VALUE under a declared-Int element means
    /// the untyped pool resolved the scalar core impl where the backends'
    /// type-directed rewrite picks the `_f64` twin — the body would run, but
    /// its declared return type then mislabels the result and the f64 BITS
    /// leak out as integers (nightly fuzz 2026-08-19, seed 515402596033/74:
    /// `list.dedup([2.718…])` printed 4613303445314885481). A mismatch
    /// abstains; matching declarations pass. An Int under an OPAQUE declared
    /// type (`Value`, a type variable) is the address-identity and stays.
    fn heap_slot_hinted(&mut self, e: &Value, ty: &Ty) -> Result<i64, String> {
        let int_decl = matches!(
            ty,
            Ty::Int
                | Ty::Int8
                | Ty::Int16
                | Ty::Int32
                | Ty::Int64
                | Ty::UInt8
                | Ty::UInt16
                | Ty::UInt32
                | Ty::UInt64
        );
        let opaque_decl = matches!(ty, Ty::TypeVar(_) | Ty::Unknown)
            || matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value");
        match e {
            Value::Int(i) if int_decl || opaque_decl => Ok(*i),
            Value::Dyn { addr, .. } if opaque_decl => Ok(*addr),
            // An ADDRESS from the i64-uniform tier flowing back under a HEAP
            // declaration (a `load_handle`/`load64` borrow riding through
            // fixture-tier plumbing): accept iff it is a live block whose
            // KIND can spell the declared type — identity, no copy, aliasing
            // kept. A dead address or a kind mismatch stays the abstain.
            Value::Int(i) if heap_slot_is_child(ty) => {
                use crate::heap::BlockKind as K;
                let kind = u32::try_from(*i).ok().and_then(|a| self.heap.kind(a));
                let spellable = match (kind, ty) {
                    (Some(K::Str), Ty::String) => true,
                    (Some(K::Bytes), Ty::Bytes) => true,
                    (Some(K::Slots), Ty::Applied(..) | Ty::Tuple(_)) => true,
                    (Some(K::Slots), Ty::Named(n, args))
                        if args.is_empty() && n.as_str() == "Value" =>
                    {
                        true
                    }
                    _ => false,
                };
                if spellable {
                    Ok(*i)
                } else {
                    Err(format!(
                        "prim.handle of a container holding a non-block Int \
                         under the declared element type {}",
                        ty_short(ty)
                    ))
                }
            }
            Value::Float(_) if matches!(ty, Ty::Float | Ty::Float64) || opaque_decl => {
                heap_scalar_slot(e, "container")
            }
            Value::Bool(b) if matches!(ty, Ty::Bool) || opaque_decl => Ok(*b as i64),
            Value::Str(_) | Value::List(_) | Value::Set(_) | Value::Map(_) | Value::Tuple(_)
                if heap_slot_is_child(ty) && !opaque_decl =>
            {
                self.heap_materialize_hinted(e, Some(ty))
            }
            other => Err(format!(
                "prim.handle of a container holding a {} element under the \
                 declared element type {} (no faithful slot repr)",
                other.type_name(),
                ty_short(ty)
            )),
        }
    }

    fn heap_materialize(&mut self, v: &Value) -> Result<i64, String> {
        use crate::heap::{rc_key, BlockKind};
        match v {
            // The MIR is i64-uniform and the backends' `handle` is a bitwise
            // reinterpret, so on a value that is ALREADY an address — an
            // opaque `Value` flowing back into the pool tier, a `load_handle`
            // result — identity is the faithful model. It is identity for a
            // genuine scalar Int too, exactly as it is there.
            Value::Int(i) => Ok(*i),
            // The dynamic-Value carrier re-enters as ITS OWN block.
            Value::Dyn { addr, .. } => Ok(*addr),
            Value::Str(rc) => {
                let a = self.heap.bind(rc_key(rc), rc.as_bytes(), BlockKind::Str);
                self.heap.keep(Rc::clone(rc));
                Ok(a as i64)
            }
            // The interp models Bytes as List[Int]; an all-byte list stays the
            // byte block it was in slice 1 (the bytes.* domain). Any other
            // scalar list is a slot block.
            Value::List(rc) => {
                let bytes: Option<Vec<u8>> = rc
                    .iter()
                    .map(|v| match v {
                        Value::Int(i) if (0..=255).contains(i) => Some(*i as u8),
                        _ => None,
                    })
                    .collect();
                if let Some(b) = bytes {
                    let a = self.heap.bind(rc_key(rc), &b, BlockKind::Bytes);
                    self.heap.keep(Rc::clone(rc));
                    return Ok(a as i64);
                }
                let rc = Rc::clone(rc);
                let slots = heap_scalar_slots(&rc, "List")?;
                let a = self.heap.bind_slots(rc_key(&rc), &slots, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            Value::Set(rc) => {
                let rc = Rc::clone(rc);
                let slots = heap_scalar_slots(&rc, "Set")?;
                let a = self.heap.bind_slots(rc_key(&rc), &slots, rc.len() as u32);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            // The `alloc_map` paired layout `[k0,v0,…]`, `len` = entry count.
            // Un-hinted, so Int/Bool only — see `heap_scalar_slots` for why a
            // Float's bits must not enter without a declared type to leave by.
            Value::Map(rc) => {
                let rc = Rc::clone(rc);
                let entries = rc.len() as u32;
                let mut slots = Vec::with_capacity(2 * rc.len());
                for (k, v) in rc.iter() {
                    for x in [k, v] {
                        slots.push(match x {
                            Value::Int(i) => *i,
                            Value::Bool(b) => *b as i64,
                            other => {
                                return Err(format!(
                                    "prim.handle of a Map holding a {} with no \
                                     declared type (its slot could not be typed back)",
                                    other.type_name()
                                ))
                            }
                        });
                    }
                }
                let a = self.heap.bind_slots(rc_key(&rc), &slots, entries);
                self.heap.keep(rc);
                Ok(a as i64)
            }
            other => Err(format!(
                "prim.handle of a {} (outside the slice-2 heap family)",
                other.type_name()
            )),
        }
    }

    /// `prim.alloc_str(n)` / `alloc_bytes(n)` — a zeroed byte block — and the
    /// slice-2 slot family (`alloc_list*` / `alloc_set*` / `alloc_map*` /
    /// `alloc_value`) — a zeroed block of n i64 slots. Returned as the
    /// ADDRESS; the body writes through it and returns the value;
    /// `sync_block_return` is the read-back.
    ///
    /// The size ceiling protects the INTERP process, not the program: the
    /// backends fail a huge allocation inside their own memory model, and this
    /// arena must abstain there rather than take the whole oracle down with a
    /// host OOM — an abstain is recorded, a dead process votes nothing.
    fn heap_prim_alloc(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        use crate::heap::BlockKind;
        let byte_kind = match func {
            "alloc_str" => Some(BlockKind::Str),
            "alloc_bytes" => Some(BlockKind::Bytes),
            _ => None,
        };
        let slot_family = matches!(
            func,
            "alloc_list"
                | "alloc_list_f64"
                | "alloc_set"
                | "alloc_map"
                | "alloc_list_str"
                | "alloc_set_str"
                | "alloc_map_str"
                | "alloc_map_skv"
                | "alloc_value"
        );
        if byte_kind.is_none() && !slot_family {
            return None;
        }
        let Some(Value::Int(n)) = args.first().filter(|v| matches!(v, Value::Int(i) if *i >= 0))
        else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-Int size")));
        };
        let bytes_wanted = if slot_family { n.checked_mul(8) } else { Some(*n) };
        if bytes_wanted.is_none_or(|b| b > 1 << 30) {
            return Some(Flow::Unsupported(format!(
                "prim.{func}({n}) beyond the interp arena ceiling"
            )));
        }
        Some(Flow::val(Value::Int(match byte_kind {
            Some(kind) => self.heap.alloc(*n as u32, kind) as i64,
            None => self.heap.alloc_slots(*n as u32) as i64,
        })))
    }

    /// `prim.load8` / `load32` / `load64`. An out-of-range address ABSTAINS:
    /// the two backends read real memory there, so a guessed 0 would be a wrong
    /// third vote on a program whose whole point is the byte it reads.
    fn heap_prim_load(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let w = match func {
            "load8" => 1,
            "load32" => 4,
            "load64" => 8,
            _ => return None,
        };
        let Some(a) = heap_addr(args.first()) else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
        };
        Some(match self.heap.load(a, w) {
            Some(v) => Flow::val(Value::Int(v)),
            None => Flow::Unsupported(format!(
                "prim.{func} outside this heap's arena — the backends read real \
                 memory here, so a guessed value would be a wrong vote"
            )),
        })
    }

    /// `prim.store8` / `store32` / `store64`, little-endian like both backends.
    fn heap_prim_store(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let w = match func {
            "store8" => 1,
            "store32" => 4,
            "store64" => 8,
            _ => return None,
        };
        let (Some(a), Some(Value::Int(v))) = (heap_addr(args.first()), args.get(1)) else {
            return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
        };
        Some(match self.heap.store(a, w, *v) {
            Some(()) => Flow::val(Value::Unit),
            None => Flow::Unsupported(format!("prim.{func} outside this heap's arena")),
        })
    }

    /// The slot-block element prims (#1226 slice 2): `store_str` (move a heap
    /// piece's handle into a slot), `load_str` / `load_handle` (borrow a slot's
    /// child back out), and the raw refcount pair `rc_inc` / `rc_dec`.
    fn heap_prim_slot_io(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "store_str" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported("prim.store_str with a non-address".into()));
                };
                let Some(piece) = args.get(1) else {
                    return Some(Flow::Unsupported("prim.store_str with no piece".into()));
                };
                Some(match self.heap_materialize(piece) {
                    Ok(h) => match self.heap.store(a, 8, h) {
                        Some(()) => Flow::val(Value::Unit),
                        None => {
                            Flow::Unsupported("prim.store_str outside this heap's arena".into())
                        }
                    },
                    Err(why) => Flow::Unsupported(why),
                })
            }
            "load_str" | "load_handle" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
                };
                let Some(h) = self.heap.load(a, 8) else {
                    return Some(Flow::Unsupported(format!(
                        "prim.{func} outside this heap's arena"
                    )));
                };
                Some(self.child_block_value(h, func))
            }
            // The WASI entropy exit, served with REAL per-process randomness:
            // the backends read OS entropy, and every fixture that passes the
            // 2-way byte-compare can only be asserting SHAPE (uniqueness
            // suffixes, draws-differ properties) — so any honest entropy
            // source is a faithful third vote, and a deterministic stand-in
            // would be the lie. Bytes land in the arena like any store.
            "random_get" => {
                let (Some(a), Some(Value::Int(len))) = (heap_addr(args.first()), args.get(1))
                else {
                    return Some(Flow::Unsupported("prim.random_get with a non-address".into()));
                };
                if *len < 0 {
                    return Some(Flow::Unsupported("prim.random_get with a negative len".into()));
                }
                use std::hash::{BuildHasher, Hasher};
                let mut word = 0u64;
                for i in 0..*len as u32 {
                    if i % 8 == 0 {
                        let mut h =
                            std::collections::hash_map::RandomState::new().build_hasher();
                        h.write_u32(i);
                        word = h.finish();
                    }
                    if self.heap.store(a + i, 1, (word & 0xff) as i64).is_none() {
                        return Some(Flow::Unsupported(
                            "prim.random_get outside this heap's arena".into(),
                        ));
                    }
                    word >>= 8;
                }
                Some(Flow::val(Value::Int(0)))
            }
            // Raw refcount adjust on a block base. The arena never FREES —
            // `keepalive` is its whole liveness model and an address must stay
            // valid for the run — so `rc_dec` to zero leaks by design: a leak
            // is invisible to the vote, a recycled address is not.
            "rc_inc" | "rc_dec" => {
                let Some(a) = heap_addr(args.first()) else {
                    return Some(Flow::Unsupported(format!("prim.{func} with a non-address")));
                };
                if self.heap.kind(a).is_none() {
                    return Some(Flow::Unsupported(format!(
                        "prim.{func} on an address that is not a block base"
                    )));
                }
                let rc = self.heap.load(a, 4).unwrap_or(0);
                let next = if func == "rc_inc" { rc + 1 } else { rc - 1 };
                self.heap.store(a, 4, next);
                Some(Flow::val(Value::Unit))
            }
            _ => None,
        }
    }

    /// A `Str`-block address as its `Value::Str` (adopted, so `prim.handle`
    /// answers the same block) — anything else unchanged. The `ConcatStr`
    /// coercion (see `apply_binop_concat`).
    pub(crate) fn coerce_block_str(&mut self, v: Value) -> Value {
        let Value::Int(i) = v else { return v };
        let Some(addr) = u32::try_from(i).ok() else { return v };
        if self.heap.kind(addr) == Some(crate::heap::BlockKind::Str) {
            if let Flow::Value(s) = self.child_block_value(i, "coerce") {
                return s;
            }
        }
        v
    }

    /// A list-block address as a native list (adopted) — `Bytes` as byte
    /// Ints, `Slots` as raw slot i64s (child addresses stay addresses; the
    /// exit sync types them). The `ConcatList` coercion.
    pub(crate) fn coerce_block_list(&mut self, v: Value) -> Value {
        use crate::heap::{rc_key, BlockKind};
        let Value::Int(i) = v else { return v };
        let Some(addr) = u32::try_from(i).ok() else { return v };
        match self.heap.kind(addr) {
            Some(BlockKind::Bytes) => match self.child_block_value(i, "coerce") {
                Flow::Value(l) => l,
                _ => v,
            },
            Some(BlockKind::Slots) => {
                let Some(n) = self.heap.block_len(addr) else { return v };
                let Some(slots) = (0..n)
                    .map(|k| self.heap.slot(addr, k).map(Value::Int))
                    .collect::<Option<Vec<_>>>()
                else {
                    return v;
                };
                let rc = Rc::new(slots);
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Value::List(rc)
            }
            _ => v,
        }
    }

    /// A slot's child, rebuilt as the `Value` its block kind spells — and
    /// RE-BOUND to that same address, so `prim.handle` on the borrow answers
    /// the child's own block rather than materializing a copy (aliasing).
    /// A `Slots` child stays an ADDRESS: the pool tier is i64-uniform and only
    /// a typed return boundary may rebuild a container.
    fn child_block_value(&mut self, h: i64, func: &str) -> Flow {
        use crate::heap::{rc_key, BlockKind};
        let Ok(addr) = u32::try_from(h) else {
            return Flow::Unsupported(format!("prim.{func} of a slot holding a negative value"));
        };
        match self.heap.kind(addr) {
            Some(BlockKind::Str) => {
                let Some((bytes, _)) = self.heap.block_bytes(addr) else {
                    return Flow::Unsupported(format!("prim.{func} of an unreadable Str block"));
                };
                let Ok(s) = String::from_utf8(bytes) else {
                    return Flow::Unsupported(format!(
                        "prim.{func} of a Str block holding non-UTF-8 bytes"
                    ));
                };
                let rc = Rc::new(s);
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Flow::val(Value::Str(rc))
            }
            Some(BlockKind::Bytes) => {
                let Some((bytes, _)) = self.heap.block_bytes(addr) else {
                    return Flow::Unsupported(format!("prim.{func} of an unreadable Bytes block"));
                };
                let rc: Rc<Vec<Value>> =
                    Rc::new(bytes.into_iter().map(|b| Value::Int(b as i64)).collect());
                self.heap.adopt(rc_key(&rc), addr);
                self.heap.keep(Rc::clone(&rc));
                Flow::val(Value::List(rc))
            }
            Some(BlockKind::Slots) => Flow::val(Value::Int(h)),
            None => Flow::Unsupported(format!(
                "prim.{func} of a slot that does not hold a block address"
            )),
        }
    }
}

/// A heap prim's address argument: a non-negative Int, else `None` so the
/// caller abstains instead of reading somewhere arbitrary.
fn heap_addr(v: Option<&Value>) -> Option<u32> {
    match v {
        Some(Value::Int(i)) if *i >= 0 => u32::try_from(*i).ok(),
        _ => None,
    }
}

/// Container elements as raw slot i64s with NO declared element type in
/// sight: Int and Bool only. A Float would be stored as BITS and the resolved
/// body's (possibly mislabeling) declared return type is the only thing that
/// could type it back — exactly the leak the hinted path's strictness closes,
/// so the un-hinted path must not open it from the other side.
fn heap_scalar_slots(items: &[Value], shape: &str) -> Result<Vec<i64>, String> {
    items
        .iter()
        .map(|e| match e {
            Value::Int(i) => Ok(*i),
            Value::Bool(b) => Ok(*b as i64),
            other => Err(format!(
                "prim.handle of a {shape} holding a {} element with no \
                 declared element type (its slot could not be typed back)",
                other.type_name()
            )),
        })
        .collect()
}

fn heap_scalar_slot(e: &Value, shape: &str) -> Result<i64, String> {
    match e {
        Value::Int(i) => Ok(*i),
        // A NaN's BIT PATTERN is arch- and backend-conditional (#1403: x86
        // sign-set vs aarch64 canonical), and the slot impls compare raw
        // bits — the interp cannot know which pattern the backends hold.
        Value::Float(f) if f.is_nan() => Err(format!(
            "prim.handle of a {shape} holding a NaN float (NaN bits are \
             arch-conditional — #1403; a bit-compare vote would be a guess)"
        )),
        Value::Float(f) => Ok(f.to_bits() as i64),
        Value::Bool(b) => Ok(*b as i64),
        other => Err(format!(
            "prim.handle of a {shape} holding a {} element (the untyped pool \
             tier runs the scalar core impls, so only scalar slots are \
             faithful — #1226 slice 2)",
            other.type_name()
        )),
    }
}

/// Whether the DECLARED type of a returned block is one `rebuild_addr` can
/// spell IN FULL. A type outside this family that still received a block
/// address must abstain at the sync point: passing the raw address onward
/// hands native ops an Int where a container is expected (a wrong vote), and
/// rebuilding by guesswork is the same thing with extra steps.
fn heap_modeled_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as C;
    match ty {
        Ty::String | Ty::Bytes => true,
        Ty::Applied(C::List | C::Set, ts) if ts.len() == 1 => heap_slot_ty(&ts[0]),
        Ty::Applied(C::Map, ts) if ts.len() == 2 => heap_slot_ty(&ts[0]) && heap_slot_ty(&ts[1]),
        Ty::Tuple(ts) => ts.iter().all(heap_slot_ty),
        _ => false,
    }
}

/// Whether a slot holding this DECLARED element/key/value type can be read
/// back faithfully: an inline scalar, a child block of a modeled type, or the
/// opaque dynamic `Value` (whose slots deliberately STAY addresses — the
/// i64-uniform tier). A type variable is none of these: the instantiation is
/// erased by the time the pool body returns, and Int-vs-String cannot be told
/// apart from the raw slot.
fn heap_slot_ty(ty: &Ty) -> bool {
    !heap_slot_is_child(ty) || heap_modeled_ty(ty) || matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value")
}

/// Whether a slot for this DECLARED element/key/value type holds a child
/// block address (heap) rather than an inline scalar — the same split the
/// `alloc_map` / `alloc_map_str` / `alloc_map_skv` builders make.
fn heap_slot_is_child(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
    )
}

/// A compact spelling of a type for an abstain reason — the ledger keys on
/// these strings, so they must be stable and short, not `Debug`-shaped.
fn ty_short(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId as C;
    match ty {
        Ty::String => "String".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Applied(C::List, ts) if ts.len() == 1 => format!("List[{}]", ty_short(&ts[0])),
        Ty::Applied(C::Set, ts) if ts.len() == 1 => format!("Set[{}]", ty_short(&ts[0])),
        Ty::Applied(C::Map, ts) if ts.len() == 2 => {
            format!("Map[{}, {}]", ty_short(&ts[0]), ty_short(&ts[1]))
        }
        Ty::Applied(C::Option, ts) if ts.len() == 1 => format!("{}?", ty_short(&ts[0])),
        Ty::Tuple(ts) => format!(
            "({})",
            ts.iter().map(ty_short).collect::<Vec<_>>().join(", ")
        ),
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::Bool => "Bool".into(),
        other => format!("{other:?}").split('(').next().unwrap_or("?").to_string(),
    }
}
