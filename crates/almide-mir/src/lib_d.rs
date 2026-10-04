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
