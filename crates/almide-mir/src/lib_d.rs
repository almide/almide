// ── values defined on the path (#3267) ──
// A handle an arm of an `IfThen` defines is defined on that arm's path only:
// not in the other arm, and not after the join (a value reaches the join
// through the `IfThen` dst). The guard err arm of a `mut`-param effect fn read
// the then arm's copy-on-write clone, a value the err path never computes; the
// per-handle checks saw a dead or unknown handle there, but an op that checks
// nothing (`prim.handle`, a per-object call arg, an address) let it through.
// Every handle or address an op reads is checked against the handles the
// closed arms defined, the certificate's `cross_path_probes` rule mirrored.

impl OwnershipScan {
    /// Every key the scan tracks a handle or an address under.
    fn defined_keys(&self) -> BTreeSet<ValueId> {
        self.object_of.keys().chain(self.addr_of.keys()).copied().collect()
    }

    /// The arm that just closed defined everything tracked now that was not
    /// tracked at its `IfThen`: none of it is defined past the arm.
    fn retire_arm_keys(&mut self, entry: &BTreeSet<ValueId>) {
        let arm: Vec<ValueId> = self.defined_keys().into_iter().filter(|k| !entry.contains(k)).collect();
        self.out_of_path.extend(arm);
    }

    /// A read of a handle no path through here defines.
    fn check_defined_uses(&mut self, i: usize, op: &Op) {
        for v in crate::certificate::handle_uses(op) {
            if self.out_of_path.contains(&v) {
                self.violations.push(violation(i, v, ViolationKind::UseAfterFree));
            }
        }
    }
}

// ── children of a rebound slot (#3269) ──
// A `SetLocal` rebinds a slot to a new block (`xs = list.set(xs, i, v)`: new
// block, `Drop` of the old, `SetLocal`). Under a branch the join renames the
// arm's new object onto the slot's entry object (`unify_rebound_slots`), so a
// child loaded from the old block saw a live parent after the join although
// that block was freed on one path. The certificate's `rebind_ends_children`
// rule, mirrored: a rebind ends every view of the slot's object (a raw child
// rooted at it, an address into it, a `prim.handle` carrier of it); a child
// stays live only through a reference the frame took on it.

impl OwnershipScan {
    fn end_rebound_children(&mut self, local: ValueId) {
        let Some(&slot) = self.object_of.get(&local) else { return };
        let root = |mut o: ValueId| {
            while let Some(&p) = self.child_parent.get(&o) {
                o = p;
            }
            o
        };
        let mut ended: Vec<ValueId> = self.child_parent.keys().copied().filter(|&c| root(c) == slot).collect();
        ended.extend(self.addr_of.iter().filter(|(_, &o)| o == slot).map(|(&a, _)| a));
        ended.extend(self.carriers.iter().copied().filter(|&c| c != local && self.object_of.get(&c) == Some(&slot)));
        self.rebound.extend(ended);
    }

    /// An address or carrier of a rebound slot's old block. A raw child is
    /// judged by [`Self::object_alive`], where its own `Dup` can keep it.
    fn ended_view(&self, v: ValueId) -> bool {
        self.rebound.contains(&v) && !self.child_parent.contains_key(&v)
    }

    fn enter_rebound_frame(&mut self) {
        self.rebound_frames.push((self.rebound.clone(), BTreeSet::new()));
    }

    /// Each arm starts from the set at the `IfThen`; after the `EndIf` a child
    /// rebound on either arm stays ended.
    fn leave_rebound_arm(&mut self, is_end: bool) {
        if is_end {
            let Some((_, then_left)) = self.rebound_frames.pop() else { return };
            self.rebound.extend(then_left);
        } else if let Some((entry, then_left)) = self.rebound_frames.last_mut() {
            *then_left = std::mem::replace(&mut self.rebound, entry.clone());
        }
    }
}

// ── module-global slot roots (#3279) ──
// A mutable module-level `var` lives in a slot at a constant address; the slot
// holds the block's reference. `LoadHandle` of that address yields a handle the
// frame owns no reference to, held by the slot: the lowering `Dup`s it to read
// the global, or `MakeUnique`s it and stores it back for an in-place write.
// The certificate counts it as a line with no `i` (a root the caller keeps
// alive, like a borrowed param). Here it is a borrowed root for as long as the
// slot keeps it: a call may reassign the global (`__mg_take` + drop), and a
// store of any other value replaces it, so either ends the root.

impl OwnershipScan {
    fn record_const(&mut self, dst: ValueId, value: i64) {
        self.consts.insert(dst, value);
    }

    /// `LoadHandle` of a constant address in the mutable-global slot region
    /// ([`mg_slot_addr`]): a handle the slot at that address holds. Any other
    /// untracked address stays off the model.
    fn load_slot_root(&mut self, dst: ValueId, addr: ValueId) {
        let Some(&slot) = self.consts.get(&addr) else { return };
        let base = i64::from(MG_SLOT_BASE);
        if slot < base || (slot - base) % 8 != 0 {
            return;
        }
        self.object_of.insert(dst, dst);
        self.dead.insert(dst, false);
        self.borrowed.insert(dst);
        self.slot_roots.insert(dst, slot);
    }

    /// After an op: a call, or a store into a slot of anything but a carrier
    /// of the root it holds, ends the slot roots it may have replaced.
    fn end_slot_roots(&mut self, op: &Op) {
        if self.slot_roots.is_empty() {
            return;
        }
        let ended: Vec<ValueId> = match op {
            Op::Call { .. } | Op::CallFn { .. } | Op::CallImport { .. } | Op::CallIndirect { .. } => {
                self.slot_roots.keys().copied().collect()
            }
            Op::Prim { kind: PrimKind::Store { .. }, args, .. } => {
                let (Some(slot), Some(val)) = (args.first().and_then(|a| self.consts.get(a)), args.get(1)) else {
                    return;
                };
                let kept = self.carried.get(val).copied();
                self.slot_roots.iter().filter(|(r, s)| *s == slot && Some(**r) != kept).map(|(r, _)| *r).collect()
            }
            _ => return,
        };
        for r in ended {
            self.slot_roots.remove(&r);
            self.borrowed.remove(&r);
            self.dead.insert(r, true);
        }
    }

    /// `prim.handle(src)` of a slot root: the integer the COW write-back stores.
    fn record_carrier(&mut self, dst: Option<ValueId>, args: &[ValueId]) {
        if let (Some(d), Some(&src)) = (dst, args.first()) {
            if self.slot_roots.contains_key(&src) {
                self.carried.insert(d, src);
            }
        }
    }
}

// ── merge dsts that hold no reference (#3279) ──
// A heap `IfThen` dst whose value is stored into a container slot and never
// released (`Store(addr, prim.handle(dst))`, no `Consume`) was a fresh owned
// object here and so a leak at the end, while the certificate opens no line
// for it: the arms moved their values into the merge, and the merge's value
// moved on into the container, which releases it. The certificate's rule
// decides which merges own a reference.

impl OwnershipScan {
    fn own_merge(&mut self, dst: ValueId) {
        if self.owning_merges.contains(&dst) {
            self.own_fresh_object(dst);
        }
    }
}

// ── a slot rebound onto two pre-existing objects (#3279) ──
// A `SetLocal` rebinds a slot inside one arm (`DropListStr xs; SetLocal xs =
// dup(child)`, the C-132 write-back of a `mut` var): on that path the slot
// holds a reference to an object that already existed at the `IfThen` (a
// payload loaded from a carrier), on the other path its old object. Each path
// keeps exactly one reference in the slot, but per object the arms disagree.
// When both objects existed at the `IfThen` neither can be renamed onto the
// other (#3031 renames only an arm-fresh one), so the slot's reference moves
// off both onto a slot object of its own, the certificate's slot line.

impl OwnershipScan {
    fn move_slot_reference(
        &mut self,
        slot: ValueId,
        (then_object, else_object): (ValueId, ValueId),
        then_rc: &mut BTreeMap<ValueId, i64>,
        then_obj: &mut BTreeMap<ValueId, ValueId>,
    ) {
        let held_then = then_rc.get(&then_object).copied().unwrap_or(0);
        let held_else = self.rc.get(&else_object).copied().unwrap_or(0);
        if held_then < 1 || held_else < 1 {
            return;
        }
        self.slot_objects += 1;
        let own = ValueId(u32::MAX - self.slot_objects);
        then_rc.insert(then_object, held_then - 1);
        then_rc.insert(own, 1);
        then_obj.insert(slot, own);
        self.rc.insert(else_object, held_else - 1);
        self.rc.insert(own, 1);
        self.object_of.insert(slot, own);
    }
}

// ── copy-on-write copies (#3321) ──
// A `Dup` of a block someone else keeps alive (a borrowed param, a raw loaded
// child), which the function later `MakeUnique`s, is a COPY-ON-WRITE copy:
// `MakeUnique` always copies there (the other holder plus the `Dup` make the
// count at least 2), and from then on the handle owns a block of its own. The
// shared object's liveness says nothing about it, so the handle is its own
// object from the `Dup`: a read of it after its release is a use after free
// wherever it is checked (a call arg, an address, a `Dup`). The certificate's
// `cow_copy`, mirrored.

impl OwnershipScan {
    fn cow_copy(&mut self, dst: ValueId, o: ValueId) -> bool {
        let shared = self.borrowed.contains(&o) || self.child_parent.contains_key(&o);
        if !shared || !self.cow_dups.contains(&dst) {
            return false;
        }
        self.own_fresh_object(dst);
        true
    }
}
