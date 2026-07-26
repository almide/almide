// ── Operand fusion for the wasm renderer ─────────────────────────
//
// `Fuser` folds a value's defining op into the operand position of its single
// consumer, so the emitted body pushes a constant or a computed value inline
// instead of round-tripping it through a local. Split out of `render_wasm_b.rs`
// (which renders ops) so neither half is oversized; `include!`d from there.

/// #806 step 3c: the expression-tree fuser. A single-use PURE scalar def
/// (const / non-trapping int op / f64 op) is DEFERRED instead of emitted as a
/// `local.set`, and spliced as a nested expression at its one consumer —
/// collapsing the per-op `local.set`/`local.get` churn of hot arithmetic
/// chains into wasm expression trees. Safety is enforced by flushing, never
/// by reordering effects: a pending expr reads ONLY locals (no memory), so it
/// is flushed (materialized as the original `local.set`) before (a) any
/// control marker (block boundary), (b) any op that REDEFINES a local it
/// reads (unless that op is its own consumer — operand evaluation precedes
/// the write), and (c) any op that would read it through a non-splicing
/// position. Render-level only: the MIR and its certificate are untouched.
pub(crate) struct Fuser {
    /// dst → (rendered expr, the locals the expr reads). The expr is typed
    /// exactly as the local would have been (f64 for float-classified dsts).
    pending: BTreeMap<ValueId, (String, BTreeSet<ValueId>)>,
    /// def order, for deterministic flushing.
    order: Vec<ValueId>,
    /// SSA-const values: `ConstInt` dsts never reassigned by a `SetLocal`.
    /// Lets the Div/Mod render elide the (statically decided) zero / MIN÷-1
    /// checks for a constant divisor and strength-reduce `÷ 2^k` to the exact
    /// correction-shift sequence — wasmtime's Cranelift does neither, and the
    /// serialized hardware sdiv alone cost ~25% of spectralnorm's inner loop.
    consts: BTreeMap<ValueId, i64>,
    /// Values PROVABLY EVEN (mod 2^64 — wrap preserves parity): a product of
    /// consecutive integers `x*(x±1)`, a product with an even constant, or a
    /// left shift. For an even dividend, `÷ 2` needs no negative-rounding
    /// correction: truncating division of an EXACT quotient equals `shr_s 1`
    /// for every sign — so the Div render drops the 4-op correction (the
    /// spectralnorm triangular index `ij*(ij+1)/2` sits in the innermost loop).
    evens: BTreeSet<ValueId>,
}

impl Fuser {
    pub(crate) fn new() -> Self {
        Fuser {
            pending: BTreeMap::new(),
            order: Vec::new(),
            consts: BTreeMap::new(),
            evens: BTreeSet::new(),
        }
    }
    /// Pre-scan the function for SSA-const locals (a `ConstInt` def with no
    /// `SetLocal` reassignment — reassigned loop seeds are removed).
    pub(crate) fn scan_consts(&mut self, ops: &[Op]) {
        for op in ops {
            if let Op::ConstInt { dst, value } = op {
                self.consts.insert(*dst, *value);
            }
        }
        for op in ops {
            if let Op::SetLocal { local, .. } = op {
                self.consts.remove(local);
            }
        }
    }
    pub(crate) fn const_of(&self, v: ValueId) -> Option<i64> {
        self.consts.get(&v).copied()
    }
    /// Pre-scan for provably-even values (see the `evens` field doc). Uses
    /// the same single-def discipline as `scan_consts`: a dst defined more
    /// than once or reassigned by any `SetLocal` is never classified.
    /// Parity is preserved by two's-complement wrapping (2^64 is even), so
    /// `x*(x±1)` stays even even when the add/mul wrap.
    pub(crate) fn scan_evens(&mut self, ops: &[Op]) {
        let mut def_idx: BTreeMap<ValueId, usize> = BTreeMap::new();
        let mut multi: BTreeSet<ValueId> = BTreeSet::new();
        for (i, op) in ops.iter().enumerate() {
            if let Some(d) = defined_value(op) {
                if def_idx.insert(d, i).is_some() {
                    multi.insert(d);
                }
            }
            if let Op::SetLocal { local, .. } = op {
                multi.insert(*local);
            }
        }
        let mut evens: BTreeSet<ValueId> = BTreeSet::new();
        for op in ops {
            let Op::IntBinOp { dst, op: iop, a, b } = op else { continue };
            if multi.contains(dst) {
                continue;
            }
            let even = match iop {
                IntOp::Mul => {
                    consecutive_values(ops, &def_idx, &multi, &self.consts, *a, *b)
                        || consecutive_values(ops, &def_idx, &multi, &self.consts, *b, *a)
                        || self.consts.get(a).is_some_and(|c| c % 2 == 0)
                        || self.consts.get(b).is_some_and(|c| c % 2 == 0)
                }
                IntOp::Shl => self.consts.get(b).is_some_and(|c| (1..64).contains(c)),
                _ => false,
            };
            if even {
                evens.insert(*dst);
            }
        }
        self.evens = evens;
    }
    pub(crate) fn is_even(&self, v: ValueId) -> bool {
        self.evens.contains(&v)
    }
    /// Read operand `v`: consume its pending expr if one exists, else a plain
    /// `local.get`. Accumulates the transitive read-set into `reads`.
    fn take(&mut self, v: ValueId, reads: &mut BTreeSet<ValueId>) -> String {
        if let Some((e, rs)) = self.pending.remove(&v) {
            self.order.retain(|x| *x != v);
            reads.extend(rs);
            e
        } else {
            reads.insert(v);
            format!("(local.get {})", local(v))
        }
    }
    /// Operand read for render_op arms that do not need read-set tracking.
    pub(crate) fn operand(&mut self, v: ValueId) -> String {
        let mut reads = BTreeSet::new();
        self.take(v, &mut reads)
    }
    fn emit(&mut self, v: ValueId, body: &mut String) {
        if let Some((e, _)) = self.pending.remove(&v) {
            self.order.retain(|x| *x != v);
            body.push_str(&format!("    (local.set {} {e})\n", local(v)));
        }
    }
    fn flush_all(&mut self, body: &mut String) {
        for v in std::mem::take(&mut self.order) {
            if let Some((e, _)) = self.pending.remove(&v) {
                body.push_str(&format!("    (local.set {} {e})\n", local(v)));
            }
        }
    }
    /// Flush pendings that READ any of `written`, except those in `consumed`
    /// (about to be spliced into the writing op itself, whose operand
    /// evaluation precedes the write).
    fn flush_reading(&mut self, written: &[ValueId], consumed: &[ValueId], body: &mut String) {
        let victims: Vec<ValueId> = self
            .order
            .iter()
            .filter(|v| {
                !consumed.contains(v)
                    && self.pending.get(v).is_some_and(|(_, rs)| {
                        written.iter().any(|w| rs.contains(w))
                    })
            })
            .copied()
            .collect();
        for v in victims {
            self.emit(v, body);
        }
    }
    /// Flush pendings whose dst appears in `vals` (an op will read them
    /// through a position that cannot splice).
    fn flush_values(&mut self, vals: &[ValueId], body: &mut String) {
        let victims: Vec<ValueId> =
            self.order.iter().filter(|v| vals.contains(v)).copied().collect();
        for v in victims {
            self.emit(v, body);
        }
    }
}

/// `y == x ± 1` at the point both are consumed — the [`Fuser::scan_evens`]
/// consecutive-integers witness for `x*(x±1)` parity. `y`'s single def must
/// be `Add(w, 1)`/`Add(1, w)`/`Sub(w, 1)` where `w` NAMES THE SAME NUMBER as
/// `x`: either the same ValueId, or two single-def `IntBinOp`s of identical
/// shape (op + operand ids) sitting in one STRAIGHT-LINE stretch — no control
/// marker between them and nothing redefining their operands (a `SetLocal`
/// or a re-executed operand def would let the two evaluations diverge). This
/// is the `ij*(ij+1)` shape, where lowering materializes `i + j` twice.
fn consecutive_values(
    ops: &[Op],
    def_idx: &BTreeMap<ValueId, usize>,
    multi: &BTreeSet<ValueId>,
    consts: &BTreeMap<ValueId, i64>,
    x: ValueId,
    y: ValueId,
) -> bool {
    if multi.contains(&y) {
        return false;
    }
    let Some(&dy) = def_idx.get(&y) else { return false };
    let (w, one) = match &ops[dy] {
        Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
            if consts.get(b) == Some(&1) {
                (*a, true)
            } else if consts.get(a) == Some(&1) {
                (*b, true)
            } else {
                return false;
            }
        }
        Op::IntBinOp { op: IntOp::Sub, a, b, .. } => (*a, consts.get(b) == Some(&1)),
        _ => return false,
    };
    if !one {
        return false;
    }
    if multi.contains(&w) || multi.contains(&x) {
        // A reassignable name can change between y's def and the multiply
        // that consumes the pair — no stable "same number" witness.
        return false;
    }
    if w == x {
        return true;
    }
    let (Some(&dw), Some(&dx)) = (def_idx.get(&w), def_idx.get(&x)) else { return false };
    let (sa, sb) = match (&ops[dw], &ops[dx]) {
        (
            Op::IntBinOp { op: o1, a: a1, b: b1, .. },
            Op::IntBinOp { op: o2, a: a2, b: b2, .. },
        ) if o1 == o2 && a1 == a2 && b1 == b2 => (*a1, *b1),
        _ => return false,
    };
    let (lo, hi) = if dw < dx { (dw, dx) } else { (dx, dw) };
    ops[lo + 1..hi].iter().all(|o| {
        let redefines = defined_value(o).is_some_and(|d| d == sa || d == sb);
        let writes = matches!(o, Op::SetLocal { local, .. } if *local == sa || *local == sb);
        let control = matches!(
            o,
            Op::LoopStart
                | Op::LoopEnd
                | Op::LoopBreakUnless { .. }
                | Op::IfThen { .. }
                | Op::Else { .. }
                | Op::EndIf { .. }
        );
        !redefines && !writes && !control
    })
}

/// Read a FLOAT-op operand: splice a pending expr / plain `local.get`, in the
/// f64 form when the value is float-classified, else reinterpreted from the
/// i64-uniform slot.
fn float_operand(fuser: &mut Fuser, floats: &BTreeSet<ValueId>, v: ValueId) -> String {
    let raw = fuser.operand(v);
    if floats.contains(&v) {
        raw
    } else {
        format!("(f64.reinterpret_i64 {raw})")
    }
}

/// The splice-capable op kinds: every read position of these renders through
/// [`Fuser::operand`], so pendings among their operands are consumed, never
/// stale-read. `Div`/`Mod` are excluded — their checked render reads each
/// operand several times.
fn splice_capable(op: &Op) -> bool {
    match op {
        Op::IntBinOp { op, .. } => !matches!(op, IntOp::Div | IntOp::Mod),
        // No read positions at all — trivially splice-clean, and its dst is a
        // prime defer candidate (a single-use const in a hot loop).
        Op::ConstInt { .. } => true,
        Op::SetLocal { .. } => true,
        Op::Prim { kind, .. } => matches!(
            kind,
            PrimKind::FloatUn(_)
                | PrimKind::FloatBin(_)
                | PrimKind::FloatCmp(_)
                | PrimKind::F64FromInt
                | PrimKind::FloatToInt
                | PrimKind::IntToFloat
        ),
        _ => false,
    }
}
