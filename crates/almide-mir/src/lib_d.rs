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
