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

/// Per-value definition index, plus the values with MORE than one definition
/// (a second `defined_value` or any `SetLocal` — neither is SSA, so nothing may
/// be inferred from a single defining op).
fn single_def_index(ops: &[Op]) -> (BTreeMap<ValueId, usize>, BTreeSet<ValueId>) {
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
    (def_idx, multi)
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
        let (def_idx, multi) = single_def_index(ops);
        let mut evens: BTreeSet<ValueId> = BTreeSet::new();
        for op in ops {
            let Op::IntBinOp { dst, op: iop, a, b } = op else { continue };
            if multi.contains(dst) {
                continue;
            }
            if self.binop_is_even(ops, &def_idx, &multi, *iop, *a, *b) {
                evens.insert(*dst);
            }
        }
        self.evens = evens;
    }

    /// Is this integer binop's result provably EVEN?
    ///
    /// A product is even when one factor is an even constant, or when the two
    /// factors are consecutive integers (one of any two consecutive values is
    /// even). A left shift by 1..64 is even by construction.
    fn binop_is_even(
        &self,
        ops: &[Op],
        def_idx: &BTreeMap<ValueId, usize>,
        multi: &BTreeSet<ValueId>,
        iop: IntOp,
        a: ValueId,
        b: ValueId,
    ) -> bool {
        match iop {
            IntOp::Mul => {
                consecutive_values(ops, def_idx, multi, &self.consts, a, b)
                    || consecutive_values(ops, def_idx, multi, &self.consts, b, a)
                    || self.consts.get(&a).is_some_and(|c| c % 2 == 0)
                    || self.consts.get(&b).is_some_and(|c| c % 2 == 0)
            }
            IntOp::Shl => self.consts.get(&b).is_some_and(|c| (1..64).contains(c)),
            _ => false,
        }
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
    consecutive_values_opt(ops, def_idx, multi, consts, x, y).unwrap_or(false)
}

/// The `Option`-returning body of [`consecutive_values`]: every `None` is a
/// "cannot prove consecutive", which the wrapper reads as `false`.
#[allow(clippy::too_many_arguments)]
fn consecutive_values_opt(
    ops: &[Op],
    def_idx: &BTreeMap<ValueId, usize>,
    multi: &BTreeSet<ValueId>,
    consts: &BTreeMap<ValueId, i64>,
    x: ValueId,
    y: ValueId,
) -> Option<bool> {
    if multi.contains(&y) {
        return None;
    }
    let &dy = def_idx.get(&y)?;
    let w = neighbour_of(&ops[dy], consts)?;
    if multi.contains(&w) || multi.contains(&x) {
        // A reassignable name can change between y's def and the multiply
        // that consumes the pair — no stable "same number" witness.
        return None;
    }
    if w == x {
        return Some(true);
    }
    let (&dw, &dx) = (def_idx.get(&w)?, def_idx.get(&x)?);
    let (sa, sb) = same_binop_operands(&ops[dw], &ops[dx])?;
    let (lo, hi) = if dw < dx { (dw, dx) } else { (dx, dw) };
    Some(ops[lo + 1..hi].iter().all(|o| !disturbs(o, sa, sb)))
}

/// `y = w ± 1` — the neighbour `w` whose successor/predecessor `y` is, or `None`
/// when the defining op is not that shape.
fn neighbour_of(def: &Op, consts: &BTreeMap<ValueId, i64>) -> Option<ValueId> {
    let Op::IntBinOp { op, a, b, .. } = def else { return None };
    match op {
        IntOp::Add if consts.get(b) == Some(&1) => Some(*a),
        IntOp::Add if consts.get(a) == Some(&1) => Some(*b),
        IntOp::Sub if consts.get(b) == Some(&1) => Some(*a),
        _ => None,
    }
}

/// Two ops that compute the SAME integer expression: same operator, same
/// operands. Returns those operands, which is what must stay untouched between
/// the two definitions.
fn same_binop_operands(dw: &Op, dx: &Op) -> Option<(ValueId, ValueId)> {
    match (dw, dx) {
        (
            Op::IntBinOp { op: o1, a: a1, b: b1, .. },
            Op::IntBinOp { op: o2, a: a2, b: b2, .. },
        ) if o1 == o2 && a1 == a2 && b1 == b2 => Some((*a1, *b1)),
        _ => None,
    }
}

/// Does this op break the "both definitions compute the same number" witness —
/// by redefining or writing an operand, or by being control flow (which could
/// skip one of them)?
fn disturbs(op: &Op, sa: ValueId, sb: ValueId) -> bool {
    defined_value(op).is_some_and(|d| d == sa || d == sb)
        || matches!(op, Op::SetLocal { local, .. } if *local == sa || *local == sb)
        || matches!(
            op,
            Op::LoopStart
                | Op::LoopEnd
                | Op::LoopBreakUnless { .. }
                | Op::IfThen { .. }
                | Op::Else { .. }
                | Op::EndIf { .. }
        )
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
        Op::IntBinOp { op, .. } => {
            !matches!(op, IntOp::Div | IntOp::Mod | IntOp::DivU | IntOp::ModU)
        }
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
