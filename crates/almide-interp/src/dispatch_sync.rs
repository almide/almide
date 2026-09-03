// ── dispatch.rs, part 3: the #1226 return sync ──
//
// include!-spliced into `dispatch.rs` at module level (#1856). The typed
// read-back of a block address into the value its declared return type names
// (`sync_value`), and the arena walkers it is built on (`dyn_node_of`,
// `rebuild_addr` / `rebuild_seq` / `rebuild_map`).

impl<'a> Interpreter<'a> {
    /// The typed read-back behind `sync_block_return`, recursive through the
    /// carrier shells a body may have already built (`Option` / `Result` /
    /// tuples). `Ok(None)` = leave the value untouched.
    fn sync_value(&self, v: &Value, ty: &Ty) -> Result<Option<Value>, String> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        match (v, ty) {
            (Value::Option(Some(x)), Ty::Applied(C::Option, ts)) if ts.len() == 1 => Ok(self
                .sync_value(x, &ts[0])?
                .map(|r| Value::Option(Some(Box::new(r))))),
            (Value::Result(Ok(x)), Ty::Applied(C::Result, ts)) if ts.len() == 2 => {
                Ok(self.sync_value(x, &ts[0])?.map(|r| Value::Result(Ok(Box::new(r)))))
            }
            (Value::Result(Err(x)), Ty::Applied(C::Result, ts)) if ts.len() == 2 => {
                Ok(self.sync_value(x, &ts[1])?.map(|r| Value::Result(Err(Box::new(r)))))
            }
            (Value::Tuple(xs), Ty::Tuple(ts)) if xs.len() == ts.len() => {
                let mut rebuilt = Vec::with_capacity(xs.len());
                let mut any = false;
                for (x, t) in xs.iter().zip(ts) {
                    match self.sync_value(x, t)? {
                        Some(r) => {
                            any = true;
                            rebuilt.push(r);
                        }
                        None => rebuilt.push(x.clone()),
                    }
                }
                Ok(any.then(|| Value::tuple(rebuilt)))
            }
            // A NATIVE container built inside the pool tier can hold
            // address elements (`__rx_split_go` accumulates `__rx_sub`
            // pieces with the native list concat): recurse per element by
            // the declared element type.
            (Value::List(xs), Ty::Applied(C::List, ts)) if ts.len() == 1 => {
                Ok(self.sync_elems(xs, &ts[0])?.map(Value::list))
            }
            (Value::Set(xs), Ty::Applied(C::Set, ts)) if ts.len() == 1 => {
                Ok(self.sync_elems(xs, &ts[0])?.map(|v| Value::Set(Rc::new(v))))
            }
            (Value::Map(es), Ty::Applied(C::Map, ts)) if ts.len() == 2 => {
                let mut rebuilt = Vec::with_capacity(es.len());
                let mut any = false;
                for (k, v) in es.iter() {
                    let nk = self.sync_value(k, &ts[0])?;
                    let nv = self.sync_value(v, &ts[1])?;
                    any |= nk.is_some() || nv.is_some();
                    rebuilt.push((nk.unwrap_or_else(|| k.clone()), nv.unwrap_or_else(|| v.clone())));
                }
                Ok(any.then(|| Value::Map(Rc::new(rebuilt))))
            }
            (Value::Int(i), _) => {
                let Some(addr) = u32::try_from(*i).ok().filter(|a| self.heap.kind(*a).is_some())
                else {
                    // An ordinary integer (or a non-base address): not ours.
                    return Ok(None);
                };
                if heap_modeled_ty(ty) {
                    return match self.rebuild_addr(addr, ty) {
                        Some(rebuilt) => Ok(Some(rebuilt)),
                        None => Err(format!(
                            "return sync: a block has no faithful read-back \
                             under the declared return type ({})",
                            ty_short(ty)
                        )),
                    };
                }
                use almide_lang::types::constructor::TypeConstructorId as C;
                if matches!(ty, Ty::Applied(C::List | C::Set | C::Map | C::Option | C::Result, _))
                {
                    // A container-typed return we cannot spell (a generic
                    // element erased to a type variable, an unmodeled Option
                    // block): the fixture expects a container, so the raw
                    // address must not leak into native ops — abstain.
                    return Err(format!(
                        "return sync: a block under an unspellable container \
                         return type ({})",
                        ty_short(ty)
                    ));
                }
                if matches!(ty, Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value") {
                    // The dynamic `Value` leaves the pool tier as a CARRIER:
                    // the block address (so `prim.handle` re-enters the SAME
                    // block) plus the structural snapshot display and `==`
                    // read. A bare Int here used to leak to native ops as an
                    // integer (value_repr printed the address, value_eq
                    // pointer-compared to false).
                    return match self.dyn_value_of(addr) {
                        Some(d) => Ok(Some(d)),
                        None => Err(format!(
                            "return sync: a Value-typed block at {addr} cannot \
                             be walked (unknown tag or unreadable child)"
                        )),
                    };
                }
                // An opaque return type (a bare type variable, Int): the
                // address IS the value in the i64-uniform tier.
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Per-element sync for a native list/set — `Ok(None)` when nothing
    /// needed rebuilding.
    fn sync_elems(&self, xs: &[Value], elem: &Ty) -> Result<Option<Vec<Value>>, String> {
        let mut rebuilt = Vec::with_capacity(xs.len());
        let mut any = false;
        for x in xs {
            match self.sync_value(x, elem)? {
                Some(r) => {
                    any = true;
                    rebuilt.push(r);
                }
                None => rebuilt.push(x.clone()),
            }
        }
        Ok(any.then_some(rebuilt))
    }

    /// The structural snapshot of a dynamic-`Value` block — value_core's tag
    /// walk (0 null, 1 bool, 2 int, 3 float, 4 str payload = child String
    /// handle, 5 array of n Value handles with n at @8, 6 object of n
    /// (String, Value) pairs with the SLOT count 2n at @8). `None` (an
    /// abstain upstream) for a non-block address, an unknown tag, an
    /// unreadable child, or a depth past the cap — never a guess.
    fn dyn_node_of(&self, addr: u32, depth: u32) -> Option<crate::value::DynNode> {
        use crate::value::DynNode;
        if depth > 512 || self.heap.kind(addr)? != crate::heap::BlockKind::Slots {
            return None;
        }
        let tag = self.heap.block_len(addr)?;
        Some(match tag {
            0 => DynNode::Null,
            1 => DynNode::Bool(self.heap.slot(addr, 0)? != 0),
            2 => DynNode::Int(self.heap.slot(addr, 0)?),
            3 => DynNode::Float(f64::from_bits(self.heap.slot(addr, 0)? as u64)),
            4 => {
                let child = u32::try_from(self.heap.slot(addr, 0)?).ok()?;
                let (bytes, kind) = self.heap.block_bytes(child)?;
                if kind != crate::heap::BlockKind::Str {
                    return None;
                }
                DynNode::Str(String::from_utf8(bytes).ok()?)
            }
            5 => {
                let n = self.heap.cap_field(addr)?;
                let mut xs = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let child = u32::try_from(self.heap.slot(addr, i)?).ok()?;
                    xs.push(self.dyn_node_of(child, depth + 1)?);
                }
                DynNode::Arr(xs)
            }
            6 => {
                let pairs = self.heap.cap_field(addr)? / 2;
                let mut out = Vec::with_capacity(pairs as usize);
                for i in 0..pairs {
                    let kaddr = u32::try_from(self.heap.slot(addr, 2 * i)?).ok()?;
                    let (kb, kk) = self.heap.block_bytes(kaddr)?;
                    if kk != crate::heap::BlockKind::Str {
                        return None;
                    }
                    let vaddr = u32::try_from(self.heap.slot(addr, 2 * i + 1)?).ok()?;
                    out.push((
                        String::from_utf8(kb).ok()?,
                        self.dyn_node_of(vaddr, depth + 1)?,
                    ));
                }
                DynNode::Obj(out)
            }
            _ => return None,
        })
    }

    /// A dynamic-`Value` carrier for a block address, or `None` when the
    /// block cannot honestly be walked.
    fn dyn_value_of(&self, addr: u32) -> Option<Value> {
        Some(Value::Dyn {
            addr: addr as i64,
            node: Rc::new(self.dyn_node_of(addr, 0)?),
        })
    }

    /// A block address as the `Value` the declared type `ty` spells — `None`
    /// when the block's kind or shape cannot honestly spell it.
    fn rebuild_addr(&self, addr: u32, ty: &Ty) -> Option<Value> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        use crate::heap::BlockKind as K;
        match ty {
            // The slice-1 read-back: the kind decides Str vs byte list.
            Ty::String | Ty::Bytes => {
                let (bytes, kind) = self.heap.block_bytes(addr)?;
                Some(match kind {
                    K::Str => Value::str(String::from_utf8(bytes).ok()?),
                    K::Bytes => {
                        Value::list(bytes.into_iter().map(|b| Value::Int(b as i64)).collect())
                    }
                    K::Slots => return None,
                })
            }
            Ty::Applied(C::List, ts) if ts.len() == 1 => {
                Some(Value::list(self.rebuild_seq(addr, &ts[0])?))
            }
            Ty::Applied(C::Set, ts) if ts.len() == 1 => {
                Some(Value::Set(Rc::new(self.rebuild_seq(addr, &ts[0])?)))
            }
            Ty::Applied(C::Map, ts) if ts.len() == 2 => self.rebuild_map(addr, &ts[0], &ts[1]),
            // A tuple block: one slot per element, `len` = the element count.
            Ty::Tuple(ts) => {
                if self.heap.kind(addr)? != crate::heap::BlockKind::Slots
                    || self.heap.block_len(addr)? as usize != ts.len()
                {
                    return None;
                }
                let elems: Option<Vec<Value>> = ts
                    .iter()
                    .enumerate()
                    .map(|(i, t)| self.rebuild_slot(self.heap.slot(addr, i as u32)?, t))
                    .collect();
                Some(Value::tuple(elems?))
            }
            _ => None,
        }
    }

    /// A List/Set block's elements. A `Bytes` block under a byte-element list
    /// type is the slice-1 family; otherwise the block must be a slot block
    /// with `len` = element count.
    fn rebuild_seq(&self, addr: u32, elem: &Ty) -> Option<Vec<Value>> {
        match self.heap.kind(addr)? {
            crate::heap::BlockKind::Bytes if matches!(elem, Ty::Int | Ty::Int64) => {
                let (bytes, _) = self.heap.block_bytes(addr)?;
                Some(bytes.into_iter().map(|b| Value::Int(b as i64)).collect())
            }
            crate::heap::BlockKind::Slots => {
                let n = self.heap.block_len(addr)?;
                (0..n)
                    .map(|i| self.rebuild_slot(self.heap.slot(addr, i)?, elem))
                    .collect()
            }
            _ => None,
        }
    }

    /// One raw slot as the `Value` its element type spells: scalars by value,
    /// heap elements by recursive block read, opaque types (`Value`, type
    /// variables) as the ADDRESS itself.
    fn rebuild_slot(&self, s: i64, elem: &Ty) -> Option<Value> {
        use almide_lang::types::constructor::TypeConstructorId as C;
        match elem {
            Ty::Int | Ty::Int64 | Ty::Int8 | Ty::Int16 | Ty::Int32 => Some(Value::Int(s)),
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => Some(Value::Int(s)),
            Ty::Float | Ty::Float64 => Some(Value::Float(f64::from_bits(s as u64))),
            Ty::Bool => Some(Value::Bool(s != 0)),
            Ty::String
            | Ty::Bytes
            | Ty::Tuple(_)
            | Ty::Applied(C::List | C::Set | C::Map | C::Option | C::Result, _) => {
                self.rebuild_addr(u32::try_from(s).ok()?, elem)
            }
            Ty::Named(n, args) if args.is_empty() && n.as_str() == "Value" => {
                self.dyn_value_of(u32::try_from(s).ok()?)
            }
            _ => Some(Value::Int(s)),
        }
    }

    /// A Map block's entries, by the three physical layouts the alloc family
    /// documents (see `heap_materialize`). Which layout applies is decided by
    /// the heapness of the DECLARED key/value types, same as the builders.
    fn rebuild_map(&self, addr: u32, kty: &Ty, vty: &Ty) -> Option<Value> {
        if self.heap.kind(addr)? != crate::heap::BlockKind::Slots {
            return None;
        }
        let len = self.heap.block_len(addr)?;
        let k_heap = heap_slot_is_child(kty);
        let v_heap = heap_slot_is_child(vty);
        let entries = if k_heap && v_heap { len / 2 } else { len };
        let mut pairs = Vec::with_capacity(entries as usize);
        for i in 0..entries {
            let (kslot, vslot) = if k_heap && v_heap {
                (self.heap.slot(addr, 2 * i)?, self.heap.slot(addr, 2 * i + 1)?)
            } else if k_heap {
                // alloc_map_skv: `entries` key slots then `entries` value
                // slots — the value region starts at slot `len`, NOT `cap/2`
                // (`map_set` may leave geometric slack above 2*entries).
                (self.heap.slot(addr, i)?, self.heap.slot(addr, len + i)?)
            } else {
                (self.heap.slot(addr, 2 * i)?, self.heap.slot(addr, 2 * i + 1)?)
            };
            pairs.push((self.rebuild_slot(kslot, kty)?, self.rebuild_slot(vslot, vty)?));
        }
        Some(Value::Map(Rc::new(pairs)))
    }
}
